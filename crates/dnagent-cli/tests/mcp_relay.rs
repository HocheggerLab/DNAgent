//! `dnagent mcp` relays stdio to the app's agent socket unchanged, and fails clearly
//! when no app is running.
#![cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

/// How long the relay exchange may take before the test reports where it stalled.
const RELAY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn socket(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(format!("/tmp/dnagent-relay-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("agent.sock")
}

/// How far the relay exchange got, so a hang names the step instead of timing out blind.
const STAGES: [&str; 5] = [
    "connecting to the stand-in app",
    "writing the request to the relay's stdin",
    "reading the echoed line from the relay's stdout",
    "waiting for the relay to exit after stdin closed",
    "joining the stand-in app",
];

// Runs on every unix platform again: this is the test that caught the Linux stall of
// issue #1, where `io::copy`'s splice specialisation held the first request in the pipe.
#[test]
fn relays_lines_both_ways_until_stdin_closes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

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
    let mut stderr = relay.stderr.take().unwrap();

    // The exchange runs on its own thread: every step below can block forever if the
    // relay stops forwarding, and neither `cargo test` nor the harness has a timeout,
    // so without this a stuck relay hangs the whole suite until CI kills the job.
    let stage = std::sync::Arc::new(AtomicUsize::new(0));
    let (done, finished) = mpsc::channel();
    let exchange = {
        let stage = std::sync::Arc::clone(&stage);
        std::thread::spawn(move || {
            stage.store(1, Ordering::Relaxed);
            writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"ping"}}"#).unwrap();
            stage.store(2, Ordering::Relaxed);
            let mut answer = String::new();
            stdout.read_line(&mut answer).unwrap();
            assert_eq!(
                answer,
                "{\"echo\":{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}}\n"
            );
            stage.store(3, Ordering::Relaxed);
            drop(stdin);
            let status = relay.wait().unwrap();
            assert!(status.success(), "relay exited with {status}");
            stage.store(4, Ordering::Relaxed);
            let _ = done.send(());
        })
    };

    if finished.recv_timeout(RELAY_TIMEOUT).is_err() {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        panic!(
            "the relay stalled while {} (after {RELAY_TIMEOUT:?}); its stderr: {text:?}",
            STAGES[stage.load(Ordering::Relaxed)]
        );
    }
    exchange.join().unwrap();
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
