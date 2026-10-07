//! Where DNAgent keeps things, per platform.
//!
//! Every path the app and CLI derive from the user's account goes through here, because
//! `$HOME` does not exist on Windows: reading it directly silently resolves to `.` and
//! scatters a feature library and an enzyme catalogue into whatever directory the app
//! happened to start in. The three conventions:
//!
//! | | home | application data |
//! |---|---|---|
//! | macOS | `$HOME` | `~/Library/Application Support/DNAgent` |
//! | Windows | `%USERPROFILE%` | `%APPDATA%\DNAgent` |
//! | Linux and other unix | `$HOME` | `$XDG_DATA_HOME/dnagent`, else `~/.local/share/dnagent` |
use std::ffi::OsString;
use std::path::PathBuf;

/// The user's home directory, or `.` when the environment does not say.
///
/// `.` is a deliberate last resort rather than an error: every caller is choosing where
/// to put a cache or a workspace, and a relative path is recoverable where a panic in a
/// headless CLI is not.
#[must_use]
pub fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// The per-user directory for DNAgent's own data (feature library, enzyme catalogue).
///
/// `folder` is the leaf under the platform's data directory, e.g. `enzymes`.
#[must_use]
pub fn data_dir(folder: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        return home()
            .join("Library/Application Support/DNAgent")
            .join(folder);
    }
    if cfg!(windows) {
        // %APPDATA% is the roaming profile, which is where a user's own data belongs;
        // it is set for every interactive session, so the fallback is rarely used.
        let roaming: Option<OsString> = std::env::var_os("APPDATA").filter(|v| !v.is_empty());
        return roaming
            .map_or_else(|| home().join("AppData/Roaming"), PathBuf::from)
            .join("DNAgent")
            .join(folder);
    }
    std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map_or_else(|| home().join(".local/share"), PathBuf::from)
        .join("dnagent")
        .join(folder)
}

/// Expand a leading `~/` against [`home`]; any other path is returned unchanged.
#[must_use]
pub fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_under_the_platform_convention() {
        let path = data_dir("enzymes");
        let text = path.to_string_lossy().replace('\\', "/");
        assert!(text.ends_with("enzymes"), "{text}");
        if cfg!(target_os = "macos") {
            assert!(
                text.contains("Library/Application Support/DNAgent"),
                "{text}"
            );
        } else if cfg!(windows) {
            assert!(text.contains("DNAgent"), "{text}");
        } else {
            assert!(text.contains("dnagent"), "{text}");
        }
    }

    #[test]
    fn expand_home_only_touches_a_leading_tilde() {
        assert_eq!(expand_home("/tmp/x"), PathBuf::from("/tmp/x"));
        assert_eq!(expand_home("relative/x"), PathBuf::from("relative/x"));
        assert_eq!(expand_home("~/x"), home().join("x"));
        // A bare `~` is a filename, not a home reference.
        assert_eq!(expand_home("~x"), PathBuf::from("~x"));
    }
}
