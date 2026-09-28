#[path = "../examples/export_recordings/recordings.rs"]
mod recordings;

#[test]
fn e2e_recordings_are_current() {
    assert!(
        recordings::recordings_json(&recordings::repo_root())
            == include_str!("../../../desktop/e2e/fixtures/recordings.json"),
        "desktop/e2e/fixtures/recordings.json is stale; regenerate with \
         `cargo run -p dnagent-desktop-api --example export_recordings > desktop/e2e/fixtures/recordings.json`"
    );
}
