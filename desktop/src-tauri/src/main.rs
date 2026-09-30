#![forbid(unsafe_code)]
use dnagent_desktop_api::Diagnostic;
use dnagent_desktop_api::detection::DetectionResult;
use dnagent_desktop_api::restriction::{EnzymeCatalogueInfo, EnzymeCount, Fragment, Site};
use dnagent_desktop_api::session::{
    DocumentState, FeaturePreview, FeatureRequest, FileStamp, HandoffItem, HandoffResult, SaveResult, Session,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

/// One edit session per app window; commands run on a blocking worker thread.
#[derive(Default)]
struct AppSession(Arc<Mutex<Session>>);

async fn with_session<T: Send + 'static>(
    state: State<'_, AppSession>,
    work: impl FnOnce(&mut Session) -> Result<T, Diagnostic> + Send + 'static,
) -> Result<T, Diagnostic> {
    let session = Arc::clone(&state.0);
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = session.lock().map_err(|_| Diagnostic { code: "session_poisoned".into(), message: "desktop session lock poisoned".into() })?;
        work(&mut session)
    })
    .await
    .map_err(|error| Diagnostic { code: "task_failed".into(), message: error.to_string() })?
}

#[tauri::command]
async fn open_document(state: State<'_, AppSession>, path: String) -> Result<DocumentState, Diagnostic> {
    with_session(state, move |s| s.open(&PathBuf::from(path))).await
}

#[tauri::command]
async fn preview_feature(state: State<'_, AppSession>, document_id: u32, request: FeatureRequest) -> Result<FeaturePreview, Diagnostic> {
    with_session(state, move |s| s.preview_feature(document_id, &request)).await
}

#[tauri::command]
async fn add_feature(state: State<'_, AppSession>, document_id: u32, request: FeatureRequest) -> Result<DocumentState, Diagnostic> {
    with_session(state, move |s| s.add_feature(document_id, &request)).await
}

#[tauri::command]
async fn remove_feature(state: State<'_, AppSession>, document_id: u32, feature_id: String) -> Result<DocumentState, Diagnostic> {
    with_session(state, move |s| s.remove_feature(document_id, &feature_id)).await
}

#[tauri::command]
async fn undo(state: State<'_, AppSession>, document_id: u32) -> Result<DocumentState, Diagnostic> {
    with_session(state, move |s| s.undo(document_id)).await
}

#[tauri::command]
async fn redo(state: State<'_, AppSession>, document_id: u32) -> Result<DocumentState, Diagnostic> {
    with_session(state, move |s| s.redo(document_id)).await
}

#[tauri::command]
async fn save_genbank(state: State<'_, AppSession>, document_id: u32, path: String) -> Result<SaveResult, Diagnostic> {
    with_session(state, move |s| s.save_genbank(document_id, &PathBuf::from(path))).await
}

#[tauri::command]
async fn close_document(state: State<'_, AppSession>, document_id: u32) -> Result<(), Diagnostic> {
    with_session(state, move |s| {
        s.close(document_id);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn write_handoff(state: State<'_, AppSession>, workspace: String, items: Vec<HandoffItem>) -> Result<HandoffResult, Diagnostic> {
    with_session(state, move |s| s.write_handoff(&PathBuf::from(workspace), &items)).await
}

#[tauri::command]
async fn poll_files(workspace: String, open_paths: Vec<String>) -> Result<Vec<FileStamp>, Diagnostic> {
    tauri::async_runtime::spawn_blocking(move || dnagent_desktop_api::session::poll_files(&PathBuf::from(workspace), &open_paths))
        .await
        .map_err(|error| Diagnostic { code: "task_failed".into(), message: error.to_string() })
}

#[tauri::command]
async fn write_svg(path: String, svg: String) -> Result<(), Diagnostic> {
    tauri::async_runtime::spawn_blocking(move || dnagent_desktop_api::session::write_svg(&PathBuf::from(path), &svg))
        .await
        .map_err(|error| Diagnostic { code: "task_failed".into(), message: error.to_string() })?
}

#[tauri::command]
fn default_workspace() -> String {
    dnagent_desktop_api::session::default_workspace()
}

#[tauri::command]
async fn detect_features(state: State<'_, AppSession>, document_id: u32) -> Result<DetectionResult, Diagnostic> {
    with_session(state, move |s| s.detect_features(document_id)).await
}

#[tauri::command]
async fn add_features(state: State<'_, AppSession>, document_id: u32, requests: Vec<FeatureRequest>) -> Result<DocumentState, Diagnostic> {
    with_session(state, move |s| s.add_features(document_id, &requests)).await
}

#[tauri::command]
fn enzyme_catalogue() -> EnzymeCatalogueInfo {
    dnagent_desktop_api::restriction::catalogue_info()
}

#[tauri::command]
async fn enzyme_counts(state: State<'_, AppSession>, document_id: u32) -> Result<Vec<EnzymeCount>, Diagnostic> {
    with_session(state, move |s| s.enzyme_counts(document_id)).await
}

#[tauri::command]
async fn find_sites(state: State<'_, AppSession>, document_id: u32, enzymes: Vec<String>) -> Result<Vec<Site>, Diagnostic> {
    with_session(state, move |s| s.find_sites(document_id, &enzymes)).await
}

#[tauri::command]
async fn digest(state: State<'_, AppSession>, document_id: u32, enzymes: Vec<String>) -> Result<Vec<Fragment>, Diagnostic> {
    with_session(state, move |s| s.digest(document_id, &enzymes)).await
}

fn main() {
    for warning in dnagent_app::enzymes::activate() {
        eprintln!("warning [{}]: {}", warning.code, warning.message);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppSession::default())
        .invoke_handler(tauri::generate_handler![
            open_document, preview_feature, add_feature, remove_feature, undo, redo, save_genbank,
            close_document, write_handoff, poll_files, default_workspace, enzyme_catalogue, enzyme_counts, find_sites, digest, detect_features, add_features, write_svg
        ])
        .run(tauri::generate_context!())
        .expect("desktop runtime failed");
}
