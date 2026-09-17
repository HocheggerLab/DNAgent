#![forbid(unsafe_code)]
use dnagent_desktop_api::{Diagnostic, Document};

#[tauri::command]
async fn open_document(path: String) -> Result<Document, Diagnostic> {
    tauri::async_runtime::spawn_blocking(move || {
        dnagent_desktop_api::open_document(std::path::Path::new(&path))
    })
    .await
    .map_err(|error| Diagnostic {
        code: "import_task_failed".into(),
        message: error.to_string(),
    })?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![open_document])
        .run(tauri::generate_context!())
        .expect("desktop runtime failed");
}
