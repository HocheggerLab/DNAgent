//! MCP server exposing a running DNAgent desktop session to agents (Claude Code, Pi via
//! MCPorter) over a user-only unix socket; `dnagent mcp` relays stdio to it. The tools
//! are thin adapters over [`dnagent_desktop_api::agent`], which holds the logic and tests.
//! Built on turbomcp (as TurboVault); schemas come from the tool signatures.
use dnagent_desktop_api::Diagnostic;
use dnagent_desktop_api::session::Session;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use turbomcp::prelude::*;

/// Finds the session an agent should see: the app's only session, or (in the e2e test
/// server) the browser session that last reported its view.
pub type SessionSource = Arc<dyn Fn() -> Option<Arc<Mutex<Session>>> + Send + Sync>;

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

    /// Run `work` on the session on a blocking thread; tool errors carry the
    /// diagnostic code so agents can react to e.g. `no_such_document`.
    async fn run(
        &self,
        tool: &'static str,
        work: impl FnOnce(&Session) -> Result<Value, Diagnostic> + Send + 'static,
    ) -> McpResult<String> {
        let source = Arc::clone(&self.source);
        let result = tokio::task::spawn_blocking(move || {
            let session = source().ok_or_else(|| Diagnostic {
                code: "no_session".into(),
                message: "no DNAgent window is connected".into(),
            })?;
            let session = session.lock().map_err(|_| Diagnostic {
                code: "session_poisoned".into(),
                message: "desktop session lock poisoned".into(),
            })?;
            work(&session)
        })
        .await
        .map_err(|e| McpError::internal(e.to_string()))?;
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
