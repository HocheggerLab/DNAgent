//! `dnagent mcp` relays stdio to the app's agent socket unchanged, and fails clearly
//! when no app is running.
#![cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

fn socket(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(format!("/tmp/dnagent-relay-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("agent.sock")
}

#[test]
fn relays_lines_both_ways_until_stdin_closes() {
    let path = socket("echo");
    let listener = UnixListener::bind(&path).unwrap();
    // A stand-in app: answers each line with a marked copy, then closes after EOF.
    let app = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut received = Vec::new();
        for line in BufReader::new(stream).lines() {
            let line = line.unwrap();
            writeln!(writer, "{{\"echo\":{line}}}").unwrap();
            received.push(line);
        }
        received
    });
    let mut relay = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(["mcp", "--socket", path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = relay.stdin.take().unwrap();
    let mut stdout = BufReader::new(relay.stdout.take().unwrap());
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"ping"}}"#).unwrap();
    let mut answer = String::new();
    stdout.read_line(&mut answer).unwrap();
    assert_eq!(
        answer,
        "{\"echo\":{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}}\n"
    );
    drop(stdin);
    let status = relay.wait().unwrap();
    assert!(status.success());
    assert_eq!(app.join().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn explains_when_the_app_is_not_running() {
    let path = socket("absent");
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(["mcp", "--socket", path.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "stdout must stay protocol-only");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not running"), "{stderr}");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
