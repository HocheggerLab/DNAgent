//! Test-only HTTP bridge for the desktop e2e harness: runs the real desktop session
//! behind `POST /invoke` so browser tests exercise Rust, not recordings.
//!
//! `cargo run -p dnagent-desktop-api --example e2e_server -- --port 1431`
//! Request: `{"session": "<id>", "command": "<tauri command>", "args": {...}}`.
//! Response: `{"ok": true, "value": ...}` or `{"ok": false, "error": {code, message}}`.
//! Binds 127.0.0.1 only. Relative paths resolve against the working directory.
use dnagent_desktop_api::session::{Session, dispatch};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

type Sessions = Arc<Mutex<HashMap<String, Session>>>;

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn handle(mut stream: TcpStream, sessions: &Sessions) {
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
    let result = {
        let mut sessions = sessions.lock().expect("session lock");
        let session = sessions.entry(session_id).or_default();
        dispatch(session, &command, &request["args"])
    };
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
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind 127.0.0.1");
    eprintln!("dnagent e2e server on http://127.0.0.1:{port}");
    for warning in dnagent_app::enzymes::activate() {
        eprintln!("warning [{}]: {}", warning.code, warning.message);
    }
    let sessions: Sessions = Arc::default();
    for stream in listener.incoming().flatten() {
        let sessions = Arc::clone(&sessions);
        std::thread::spawn(move || handle(stream, &sessions));
    }
}
