//! MCP server exposing a running DNAgent desktop session to agents (Claude Code, Pi via
//! MCPorter) over a user-only unix socket; `dnagent mcp` relays stdio to it. The tools
//! are thin adapters over [`dnagent_desktop_api::agent`], which holds the logic and tests.
//! Built on turbomcp (as TurboVault); schemas come from the tool signatures.
use dnagent_desktop_api::Diagnostic;
use dnagent_desktop_api::agent::{self, Expected};
use dnagent_desktop_api::session::{Highlight, Session};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use turbomcp::prelude::*;

/// Finds the session an agent should see: the app's only session, or (in the e2e test
/// server) the browser session that last reported its view.
pub type SessionSource = Arc<dyn Fn() -> Option<Arc<Mutex<Session>>> + Send + Sync>;

/// How long a view tool waits for the GUI to show its change (it syncs every ~250 ms).
const APPLY_TIMEOUT: Duration = Duration::from_secs(3);

/// A region to highlight in `present` (half-open; end < start wraps on circular records).
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct HighlightArg {
    /// Short name shown to the user, e.g. "5' junction" or "F primer".
    pub label: String,
    /// Zero-based start.
    pub start: u32,
    /// Zero-based exclusive end.
    pub end: u32,
}

/// The DNAgent agent tools over a desktop session.
#[derive(Clone)]
pub struct AgentServer {
    source: SessionSource,
}

impl AgentServer {
    /// Serve one fixed session (the desktop app).
    #[must_use]
    pub fn for_session(session: Arc<Mutex<Session>>) -> Self {
        Self::with_source(Arc::new(move || Some(Arc::clone(&session))))
    }

    #[must_use]
    pub fn with_source(source: SessionSource) -> Self {
        Self { source }
    }

    /// Run `work` on the session on a blocking thread.
    async fn with_session<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Session) -> Result<T, Diagnostic> + Send + 'static,
    ) -> Result<T, Diagnostic> {
        let source = Arc::clone(&self.source);
        tokio::task::spawn_blocking(move || {
            let session = source().ok_or_else(|| Diagnostic {
                code: "no_session".into(),
                message: "no DNAgent window is connected".into(),
            })?;
            let mut session = session.lock().map_err(|_| Diagnostic {
                code: "session_poisoned".into(),
                message: "desktop session lock poisoned".into(),
            })?;
            work(&mut session)
        })
        .await
        .map_err(|e| Diagnostic {
            code: "internal".into(),
            message: e.to_string(),
        })?
    }

    /// Tool errors carry the diagnostic code so agents can react to e.g. `no_such_document`.
    fn reply(tool: &'static str, result: Result<Value, Diagnostic>) -> McpResult<String> {
        match result {
            Ok(value) => {
                serde_json::to_string_pretty(&value).map_err(|e| McpError::internal(e.to_string()))
            }
            Err(error) => Err(McpError::tool_execution_failed(
                tool,
                format!("[{}] {}", error.code, error.message),
            )),
        }
    }

    async fn run(
        &self,
        tool: &'static str,
        work: impl FnOnce(&Session) -> Result<Value, Diagnostic> + Send + 'static,
    ) -> McpResult<String> {
        Self::reply(tool, self.with_session(move |s| work(s)).await)
    }

    /// Queue a GUI request, then wait until the GUI reports showing it; the reply says
    /// whether it did (`applied`) and what the user now sees (`view`).
    async fn show(
        &self,
        tool: &'static str,
        work: impl FnOnce(&mut Session) -> Result<(Value, Expected), Diagnostic> + Send + 'static,
    ) -> McpResult<String> {
        let (mut result, expected) = match self.with_session(work).await {
            Ok(done) => done,
            Err(error) => return Self::reply(tool, Err(error)),
        };
        let deadline = tokio::time::Instant::now() + APPLY_TIMEOUT;
        let applied = loop {
            let check = expected.clone();
            let shown = self
                .with_session(move |s| Ok(agent::applied(s, &check)))
                .await
                .unwrap_or(false);
            if shown || tokio::time::Instant::now() >= deadline {
                break shown;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        result["applied"] = json!(applied);
        if !applied {
            result["note"] = json!("the app did not show this within 3 s; is its window open?");
        }
        result["view"] = self
            .with_session(|s| agent::get_view(s))
            .await
            .unwrap_or(Value::Null);
        Self::reply(tool, Ok(result))
    }
}

#[turbomcp::server(
    name = "dnagent",
    version = "0.1.0",
    description = "Live view of the DNAgent desktop app: open constructs, the user's selection, features and GenBank snapshots. Coordinates are zero-based and half-open; end < start wraps through the origin. Use the dnagent CLI on exported snapshots for analysis."
)]
impl AgentServer {
    /// The app version, workspace folder and how many constructs are open.
    #[tool]
    async fn status(&self) -> McpResult<String> {
        self.run("status", |s| Ok(dnagent_desktop_api::agent::status(s)))
            .await
    }

    /// Every open construct in tab order: id, name, length, topology, whether it is the active tab, unsaved changes and edit_counter.
    #[tool]
    async fn list_documents(&self) -> McpResult<String> {
        self.run("list_documents", |s| {
            Ok(dnagent_desktop_api::agent::list_documents(s))
        })
        .await
    }

    /// What the user is looking at: the active construct, view tab, selected base range and selected feature (as a `dnagent features` row). "This", "here" or "the selection" refer to these.
    #[tool]
    async fn get_view(&self) -> McpResult<String> {
        self.run("get_view", dnagent_desktop_api::agent::get_view)
            .await
    }

    /// All features of an open construct (current state, unsaved edits included), as `dnagent features --output json` rows.
    #[tool]
    async fn get_features(
        &self,
        #[description("document_id from list_documents or get_view")] document_id: u32,
    ) -> McpResult<String> {
        self.run("get_features", move |s| {
            dnagent_desktop_api::agent::get_features(s, document_id)
        })
        .await
    }

    /// Open a construct file (.dna, GenBank, FASTA, .locus.json) in the app as a new tab, or switch to it if it is already open. Use an absolute path (~/ is expanded). Returns its document_id once the app shows it.
    #[tool]
    async fn open_file(
        &self,
        #[description("absolute path of the file to open")] path: String,
    ) -> McpResult<String> {
        self.show("open_file", move |s| agent::open_file(s, &path))
            .await
    }

    /// Select a base range in an open construct and make it the active tab, to show the user a region. Zero-based, half-open [start, end); end < start wraps through the origin on circular constructs. Returns the resulting view.
    #[tool]
    async fn select_range(
        &self,
        #[description("document_id from list_documents or get_view")] document_id: u32,
        #[description("zero-based start")] start: u32,
        #[description("zero-based exclusive end")] end: u32,
    ) -> McpResult<String> {
        self.show("select_range", move |s| {
            agent::select_range(s, document_id, start, end).map(|e| (json!({}), e))
        })
        .await
    }

    /// Select a feature (by its id from get_features) and make its construct the active tab. Returns the resulting view.
    #[tool]
    async fn select_feature(
        &self,
        #[description("document_id from list_documents or get_view")] document_id: u32,
        #[description("feature id, e.g. feature-0003")] feature_id: String,
    ) -> McpResult<String> {
        self.show("select_feature", move |s| {
            agent::select_feature(s, document_id, &feature_id).map(|e| (json!({}), e))
        })
        .await
    }

    /// Show a short message to the user in the app's agent panel, e.g. progress or a question to answer in the terminal.
    #[tool]
    async fn notify(&self, #[description("the message")] message: String) -> McpResult<String> {
        self.show("notify", move |s| agent::notify(s, &message))
            .await
    }

    /// Present a finished result: open the file (e.g. an assembled product written as GenBank), show the summary in the app's agent panel and list the highlights (junctions, primer sites, new features) for the user to click; the first highlight is selected. Use at the end of autonomous work.
    #[tool]
    async fn present(
        &self,
        #[description("absolute path of the result file to open")] path: String,
        #[description("what was done and what to check, in one or two sentences")] summary: String,
        #[description("regions to point out, in order of importance")] highlights: Vec<
            HighlightArg,
        >,
    ) -> McpResult<String> {
        let highlights = highlights
            .into_iter()
            .map(|h| Highlight {
                label: h.label,
                start: h.start,
                end: h.end,
            })
            .collect();
        self.show("present", move |s| {
            agent::present(s, &path, &summary, highlights)
        })
        .await
    }

    /// Write the construct as it is open now (unsaved edits included) as GenBank into the workspace's .dnagent/snapshots folder and return the path, for analysis with the dnagent CLI.
    #[tool]
    async fn export_snapshot(
        &self,
        #[description("document_id from list_documents or get_view")] document_id: u32,
    ) -> McpResult<String> {
        self.run("export_snapshot", move |s| {
            dnagent_desktop_api::agent::export_snapshot(s, document_id)
        })
        .await
    }
}

/// Serve `server` on the unix socket at `path` until `shutdown` turns true. The parent
/// folder is created user-only (0700). A live socket of another instance is left alone
/// (error); a stale socket file is replaced.
#[cfg(unix)]
pub async fn serve(
    server: AgentServer,
    path: &Path,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        restrict_to_user(parent)?;
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(format!(
            "another DNAgent app already serves agents at {}",
            path.display()
        ));
    }
    let path = path
        .to_str()
        .ok_or_else(|| format!("socket path is not UTF-8: {}", path.display()))?;
    turbomcp_server::transport::unix::run_with_shutdown(
        &server,
        path,
        &turbomcp::ServerConfig::default(),
        shutdown,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Windows has no unix socket, and turbomcp's remaining transports are TCP, HTTP and
/// WebSocket — all reachable by every process on the machine, where the socket's
/// directory is 0700. Serving the user's constructs over one needs a port file and a
/// token first, so until then the app runs with no agent channel and says so. The file
/// handoff (`docs/agent-handoff.md`) works on every platform.
#[cfg(not(unix))]
// `async` with nothing to await, deliberately: the signature has to match the unix
// `serve` its callers already await.
#[allow(clippy::unused_async)]
pub async fn serve(
    _server: AgentServer,
    _path: &Path,
    _shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    Err(
        "the live agent channel needs a Unix platform (macOS or Linux); \
         on Windows use the file handoff in the workspace folder"
            .into(),
    )
}

#[cfg(unix)]
fn restrict_to_user(folder: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("cannot restrict {}: {e}", folder.display()))
}

/// Start the agent server for the desktop app on a background task of the current tokio
/// runtime at [`dnagent_app::agent_socket_path`]. Failures are reported, never fatal: the
/// app works without agents.
pub fn spawn_for_app(
    session: Arc<Mutex<Session>>,
    spawn: impl FnOnce(std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>),
) -> watch::Sender<bool> {
    let (stop, shutdown) = watch::channel(false);
    let path = dnagent_app::agent_socket_path();
    spawn(Box::pin(async move {
        if let Err(error) = serve(AgentServer::for_session(session), &path, shutdown).await {
            eprintln!("warning [agent_server]: {error}");
        }
    }));
    stop
}
