//! Test-only HTTP bridge for the desktop e2e harness: runs the real desktop session
//! behind `POST /invoke` so browser tests exercise Rust, not recordings. With
//! `--agent-sockets <dir>`, each browser session is also served to agents (MCP) at
//! `<dir>/<session>.sock` once it reports its view, as the desktop app does.
//!
//! `cargo run -p dnagent-agent-mcp --example e2e_server -- --port 1431 --agent-sockets /tmp/dnagent-e2e-1431`
//! Request: `{"session": "<id>", "command": "<tauri command>", "args": {...}}`.
//! Response: `{"ok": true, "value": ...}` or `{"ok": false, "error": {code, message}}`.
//! Binds 127.0.0.1 only. Relative paths resolve against the working directory.
use dnagent_agent_mcp::{AgentServer, serve};
use dnagent_desktop_api::session::{Session, dispatch};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Default)]
struct Served {
    sessions: HashMap<String, Arc<Mutex<Session>>>,
    /// Stop handles of running agent servers; dropping one would stop its server.
    agents: HashMap<String, watch::Sender<bool>>,
}

struct State {
    served: Mutex<Served>,
    agent_sockets: Option<PathBuf>,
    runtime: tokio::runtime::Runtime,
}

impl State {
    fn session(&self, id: &str) -> Arc<Mutex<Session>> {
        let mut served = self.served.lock().expect("served lock");
        Arc::clone(served.sessions.entry(id.to_owned()).or_default())
    }

    /// Serve a browser session to agents once it reports its view (like the app at start).
    fn serve_agents(&self, id: &str, session: &Arc<Mutex<Session>>) {
        let Some(folder) = &self.agent_sockets else {
            return;
        };
        let mut served = self.served.lock().expect("served lock");
        if served.agents.contains_key(id) {
            return;
        }
        let path = folder.join(format!("{id}.sock"));
        let (stop, shutdown) = watch::channel(false);
        let agent = AgentServer::for_session(Arc::clone(session));
        self.runtime.spawn(async move {
            if let Err(error) = serve(agent, &path, shutdown).await {
                eprintln!("agent server {}: {error}", path.display());
            }
        });
        served.agents.insert(id.to_owned(), stop);
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn handle(mut stream: TcpStream, state: &State) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let target = request_line.split_whitespace().nth(1).unwrap_or("");
    if target.ends_with("/health") {
        respond(&mut stream, "200 OK", "{\"ok\":true}");
        return;
    }
    if !target.ends_with("/invoke") {
        respond(&mut stream, "404 Not Found", "{\"ok\":false}");
        return;
    }
    let request: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            respond(&mut stream, "400 Bad Request", &serde_json::json!({"ok": false, "error": {"code": "invalid_request", "message": error.to_string()}}).to_string());
            return;
        }
    };
    let session_id = request["session"].as_str().unwrap_or("default").to_owned();
    let command = request["command"].as_str().unwrap_or("").to_owned();
    let session = state.session(&session_id);
    let result = {
        let mut locked = session.lock().expect("session lock");
        dispatch(&mut locked, &command, &request["args"])
    };
    if command == "report_view" && result.is_ok() {
        state.serve_agents(&session_id, &session);
    }
    let payload = match result {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    };
    respond(&mut stream, "200 OK", &payload.to_string());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .map_or("1431", String::as_str);
    let agent_sockets = args
        .iter()
        .position(|a| a == "--agent-sockets")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    if let Some(folder) = &agent_sockets {
        let _ = std::fs::remove_dir_all(folder);
    }
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind 127.0.0.1");
    eprintln!("dnagent e2e server on http://127.0.0.1:{port}");
    for warning in dnagent_app::enzymes::activate() {
        eprintln!("warning [{}]: {}", warning.code, warning.message);
    }
    let state = Arc::new(State {
        served: Mutex::default(),
        agent_sockets,
        runtime: tokio::runtime::Runtime::new().expect("tokio runtime"),
    });
    for stream in listener.incoming().flatten() {
        let state = Arc::clone(&state);
        std::thread::spawn(move || handle(stream, &state));
    }
}
