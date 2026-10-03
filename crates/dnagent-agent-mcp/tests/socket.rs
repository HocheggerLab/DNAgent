//! The agent server over a real unix socket: MCP handshake, the committed tool contract
//! and live view reads against a desktop session.
use dnagent_agent_mcp::{AgentServer, serve};
use dnagent_desktop_api::session::{RangeRequest, Session, ViewReport};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const CONTRACT: &str = "../../schemas/agent-tools.json";

struct Client {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl Client {
    async fn connect(path: &Path) -> Self {
        let mut stream = None;
        for _ in 0..100 {
            if let Ok(s) = UnixStream::connect(path).await {
                stream = Some(s);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let (read, writer) = stream.expect("server socket").into_split();
        let mut client = Self {
            reader: BufReader::new(read),
            writer,
            next_id: 0,
        };
        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "dnagent-test", "version": "0"}
                }),
            )
            .await;
        assert_eq!(init["result"]["serverInfo"]["name"], "dnagent", "{init}");
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        client
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        loop {
            let mut line = String::new();
            assert!(
                self.reader.read_line(&mut line).await.unwrap() > 0,
                "server closed"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }

    /// A tool's JSON result, or the error text when the tool failed.
    async fn call(&mut self, tool: &str, arguments: Value) -> Result<Value, String> {
        let response = self
            .request("tools/call", json!({"name": tool, "arguments": arguments}))
            .await;
        if let Some(error) = response.get("error") {
            return Err(error["message"].as_str().unwrap_or_default().to_owned());
        }
        let result = &response["result"];
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if result["isError"] == true {
            return Err(text);
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene/synthetic_linear.dna")
}

fn socket(name: &str) -> PathBuf {
    // Short path: unix socket paths are limited to ~100 bytes on macOS.
    PathBuf::from(format!("/tmp/dnagent-test-{}-{name}", std::process::id())).join("agent.sock")
}

async fn start(
    session: Arc<Mutex<Session>>,
    name: &str,
) -> (Client, tokio::sync::watch::Sender<bool>, PathBuf) {
    let path = socket(name);
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let server = AgentServer::for_session(session);
    let socket_path = path.clone();
    tokio::spawn(async move { serve(server, &socket_path, shutdown).await.expect("serve") });
    (Client::connect(&path).await, stop, path)
}

/// Keys in sorted order: schema generation does not fix the order of keys such as
/// `$schema` and `$defs`, so the contract compares canonical JSON.
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonical(v)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_list_matches_the_committed_contract() {
    let (mut client, stop, path) = start(Arc::default(), "contract").await;
    let mut tools = client.request("tools/list", json!({})).await["result"]["tools"].clone();
    tools
        .as_array_mut()
        .unwrap()
        .sort_by_key(|t| t["name"].as_str().unwrap().to_owned());
    let actual = serde_json::to_string_pretty(&canonical(tools)).unwrap() + "\n";
    let contract = Path::new(env!("CARGO_MANIFEST_DIR")).join(CONTRACT);
    if std::env::var_os("DNAGENT_UPDATE_CONTRACTS").is_some() {
        std::fs::write(&contract, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&contract).unwrap_or_default();
    let _ = stop.send(true);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    assert_eq!(
        actual, expected,
        "agent tools changed: review, then rerun with DNAGENT_UPDATE_CONTRACTS=1"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_reads_the_live_view_and_features() {
    let session: Arc<Mutex<Session>> = Arc::default();
    let workspace = std::env::temp_dir().join(format!("dnagent-mcp-ws-{}", std::process::id()));
    let id = {
        let mut s = session.lock().unwrap();
        let id = s.open(&fixture()).unwrap().edit.document_id;
        let feature = dnagent_app::feature_views(&s.report(id).unwrap().record)[0]
            .id
            .clone();
        s.report_view(ViewReport {
            document_ids: vec![id],
            active_document_id: Some(id),
            view_tab: "sequence".into(),
            selection: Some(RangeRequest { start: 0, end: 3 }),
            selected_feature_id: Some(feature),
            workspace: workspace.display().to_string(),
            agent_seen: 0,
        });
        id
    };
    let (mut client, stop, path) = start(Arc::clone(&session), "view").await;

    let documents = client.call("list_documents", json!({})).await.unwrap();
    assert_eq!(documents[0]["document_id"], id);
    assert_eq!(documents[0]["active"], true);

    let view = client.call("get_view", json!({})).await.unwrap();
    assert_eq!(view["view_tab"], "sequence");
    assert_eq!(
        view["selection"],
        json!({"start": 0, "end": 3, "length": 3, "wraps_origin": false})
    );
    let features = client
        .call("get_features", json!({"document_id": id}))
        .await
        .unwrap();
    assert_eq!(view["selected_feature"], features["features"][0]);

    // The user selects something else: the agent sees it on the next read.
    {
        let mut s = session.lock().unwrap();
        let mut v = s.view().clone();
        v.selected_feature_id = None;
        s.report_view(v);
    }
    assert_eq!(
        client.call("get_view", json!({})).await.unwrap()["selected_feature"],
        Value::Null
    );

    let snapshot = client
        .call("export_snapshot", json!({"document_id": id}))
        .await
        .unwrap();
    assert!(Path::new(snapshot["path"].as_str().unwrap()).exists());

    let missing = client
        .call("get_features", json!({"document_id": 999}))
        .await
        .unwrap_err();
    assert!(missing.contains("no_such_document"), "{missing}");

    let _ = stop.send(true);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_socket_is_not_taken_over() {
    use std::os::unix::fs::PermissionsExt;
    let (_client, stop, path) = start(Arc::default(), "twice").await;
    let (_, shutdown) = tokio::sync::watch::channel(false);
    let error = serve(AgentServer::for_session(Arc::default()), &path, shutdown)
        .await
        .unwrap_err();
    assert!(error.contains("already serves"), "{error}");
    let mode = std::fs::metadata(path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    let _ = stop.send(true);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// A stand-in frontend: collects agent requests and reports the view they produce,
/// as `main.ts` does every ~250 ms.
fn fake_frontend(session: &Mutex<Session>, stop: &std::sync::atomic::AtomicBool) {
    use dnagent_desktop_api::session::AgentRequest;
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        {
            let mut s = session.lock().unwrap();
            let mut view = s.view().clone();
            for request in s.agent_sync() {
                match request {
                    AgentRequest::Open { seq, state } => {
                        view.agent_seen = seq;
                        let id = state.edit.document_id;
                        if !view.document_ids.contains(&id) {
                            view.document_ids.push(id);
                        }
                        view.active_document_id = Some(id);
                        view.selection = None;
                        view.selected_feature_id = None;
                    }
                    AgentRequest::Select {
                        seq,
                        document_id,
                        range,
                        feature_id,
                    } => {
                        view.agent_seen = seq;
                        view.active_document_id = Some(document_id);
                        view.selection = range;
                        view.selected_feature_id = feature_id;
                    }
                    AgentRequest::Present {
                        seq,
                        document_id,
                        highlights,
                        ..
                    } => {
                        view.agent_seen = seq;
                        view.active_document_id = Some(document_id);
                        view.selection = highlights.first().map(|h| RangeRequest {
                            start: h.start,
                            end: h.end,
                        });
                        view.selected_feature_id = None;
                    }
                    AgentRequest::Notify { seq, .. } => view.agent_seen = seq,
                }
            }
            s.report_view(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_points_back_and_waits_until_the_gui_shows_it() {
    let session: Arc<Mutex<Session>> = Arc::default();
    let (mut client, stop, path) = start(Arc::clone(&session), "show").await;

    // Without a frontend the request is accepted but reported as not shown.
    let unseen = client
        .call("open_file", json!({"path": fixture().to_str().unwrap()}))
        .await
        .unwrap();
    assert_eq!(unseen["applied"], false);
    assert!(unseen["note"].as_str().unwrap().contains("did not show"));

    let halt = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let frontend = {
        let (session, halt) = (Arc::clone(&session), Arc::clone(&halt));
        std::thread::spawn(move || fake_frontend(&session, &halt))
    };
    let opened = client
        .call("open_file", json!({"path": fixture().to_str().unwrap()}))
        .await
        .unwrap();
    assert_eq!(opened["already_open"], true);
    assert_eq!(opened["applied"], true);
    let id = opened["document_id"].clone();

    let ranged = client
        .call(
            "select_range",
            json!({"document_id": id, "start": 2, "end": 7}),
        )
        .await
        .unwrap();
    assert_eq!(ranged["applied"], true);
    assert_eq!(ranged["view"]["selection"]["length"], 5);

    let features = client
        .call("get_features", json!({"document_id": id}))
        .await
        .unwrap();
    let feature = features["features"][0].clone();
    let picked = client
        .call(
            "select_feature",
            json!({"document_id": id, "feature_id": feature["id"]}),
        )
        .await
        .unwrap();
    assert_eq!(picked["view"]["selected_feature"], feature);

    let presented = client
        .call(
            "present",
            json!({
                "path": fixture().to_str().unwrap(),
                "summary": "Checked the construct.",
                "highlights": [{"label": "first", "start": 0, "end": 3}, {"label": "second", "start": 4, "end": 6}]
            }),
        )
        .await
        .unwrap();
    assert_eq!(presented["applied"], true);
    assert_eq!(presented["view"]["selection"]["end"], 3);

    let bad = client
        .call(
            "select_range",
            json!({"document_id": id, "start": 0, "end": 100_000}),
        )
        .await
        .unwrap_err();
    assert!(bad.contains("invalid_range"), "{bad}");
    assert_eq!(
        client
            .call("notify", json!({"message": "done"}))
            .await
            .unwrap()["applied"],
        true
    );

    halt.store(true, std::sync::atomic::Ordering::Relaxed);
    frontend.join().unwrap();
    let _ = stop.send(true);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
