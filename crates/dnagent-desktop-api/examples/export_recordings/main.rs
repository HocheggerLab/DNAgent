//! Write recorded `open_document` responses for the desktop e2e harness:
//! `cargo run -p dnagent-desktop-api --example export_recordings > desktop/e2e/fixtures/recordings.json`
//!
//! With file arguments, records only those files (for the gitignored review gallery).
mod recordings;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        print!("{}", recordings::recordings_json(&recordings::repo_root()));
    } else {
        print!("{}", recordings::local_recordings_json(&paths));
    }
}
