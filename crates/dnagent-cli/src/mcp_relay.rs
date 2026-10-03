//! `dnagent mcp`: a byte relay between the agent's stdio and the running desktop app's
//! agent socket. The MCP protocol itself is served by the app (`dnagent-agent-mcp`), so
//! this stays a few lines of std: no async runtime in the headless CLI.
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(unix)]
pub fn run(socket: Option<PathBuf>) -> ExitCode {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let path = socket.unwrap_or_else(dnagent_app::agent_socket_path);
    let stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!(
                "dnagent mcp: the DNAgent desktop app is not running ({}: {error}).\n\
                 Start the app, then reconnect the MCP server (e.g. /mcp in Claude Code); \
                 without the app, use the CLI and the file handoff.",
                path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let Ok(mut to_app) = stream.try_clone() else {
        eprintln!("dnagent mcp: cannot duplicate the socket");
        return ExitCode::FAILURE;
    };
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut to_app);
        let _ = to_app.shutdown(std::net::Shutdown::Write);
    });
    let mut from_app = stream;
    let mut stdout = std::io::stdout().lock();
    // Line-buffered stdout flushes each newline-delimited JSON-RPC message.
    let copied = std::io::copy(&mut from_app, &mut stdout);
    let _ = stdout.flush();
    match copied {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dnagent mcp: connection to the app failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(unix))]
pub fn run(_socket: Option<PathBuf>) -> ExitCode {
    eprintln!("dnagent mcp: the agent socket needs a Unix platform (macOS or Linux)");
    ExitCode::FAILURE
}
