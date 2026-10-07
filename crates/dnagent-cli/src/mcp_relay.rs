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
        let _ = pump(&mut std::io::stdin().lock(), &mut to_app);
        let _ = to_app.shutdown(std::net::Shutdown::Write);
    });
    let mut from_app = stream;
    let mut stdout = std::io::stdout().lock();
    let copied = pump(&mut from_app, &mut stdout);
    let _ = stdout.flush();
    match copied {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dnagent mcp: connection to the app failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Copy `from` to `to` until end of input, flushing whatever arrives before waiting for
/// more.
///
/// Deliberately not `std::io::copy`: on Linux that specialises a pipe-to-socket copy into
/// `splice(2)`, which held the agent's first request in the pipe and never delivered it —
/// both sides then waited on each other forever (issue #1). A relay carries one small
/// JSON-RPC message at a time and must forward each as it lands, which is what a plain
/// read-write loop does and what the specialisation did not.
#[cfg(unix)]
fn pump(from: &mut impl std::io::Read, to: &mut impl std::io::Write) -> std::io::Result<()> {
    // One request or response per read in practice; the loop handles any size.
    let mut buffer = [0u8; 8192];
    loop {
        match from.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                to.write_all(&buffer[..read])?;
                to.flush()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(not(unix))]
pub fn run(_socket: Option<PathBuf>) -> ExitCode {
    eprintln!("dnagent mcp: the agent socket needs a Unix platform (macOS or Linux)");
    ExitCode::FAILURE
}
