//! Write recorded `open_document` responses for the desktop e2e harness:
//! `cargo run -p dnagent-desktop-api --example export_recordings > desktop/e2e/fixtures/recordings.json`
mod recordings;

fn main() {
    print!("{}", recordings::recordings_json(&recordings::repo_root()));
}
