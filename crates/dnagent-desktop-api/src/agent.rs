//! Agent tools over the desktop session: what the user has open and is looking at, read
//! live by an agent (served over MCP by `dnagent-agent-mcp`). Results are JSON values in
//! the CLI's shapes (feature rows as in `dnagent features --output json`); coordinates are
//! zero-based and half-open, and `end < start` wraps through the origin.
use crate::Diagnostic;
use crate::session::{DocumentState, Session, default_workspace, safe_stem, selection_json};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Workspace subfolder for agent snapshots (hidden, so the workspace watcher ignores it).
pub const SNAPSHOT_DIR: &str = ".dnagent/snapshots";

fn diagnostic(code: &str, message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message.to_string(),
    }
}

fn workspace(session: &Session) -> PathBuf {
    let reported = &session.view().workspace;
    PathBuf::from(if reported.is_empty() {
        default_workspace()
    } else {
        reported.clone()
    })
}

fn summary(session: &Session, state: &DocumentState) -> Value {
    let edit = &state.edit;
    let record = session
        .report(edit.document_id)
        .map(|report| &report.record)
        .ok();
    json!({
        "document_id": edit.document_id,
        "name": state.document.name,
        "length": record.map(|r| r.sequence().len()),
        "topology": record.map(dnagent_domain::SequenceRecord::topology),
        "active": session.view().active_document_id == Some(edit.document_id),
        "unsaved_changes": edit.dirty,
        "edit_counter": edit.edit_counter,
        "source": edit.source_path,
        "saved_path": edit.saved_path,
    })
}

/// The app, the workspace and how many documents are open.
#[must_use]
pub fn status(session: &Session) -> Value {
    json!({
        "app": "DNAgent desktop",
        "version": env!("CARGO_PKG_VERSION"),
        "workspace": workspace(session).display().to_string(),
        "open_documents": session.document_states().len(),
        "active_document_id": session.view().active_document_id,
        "coordinates": "zero-based, half-open; selections with end < start wrap through the origin",
    })
}

/// Every open document in tab order.
#[must_use]
pub fn list_documents(session: &Session) -> Value {
    Value::Array(
        session
            .document_states()
            .iter()
            .map(|state| summary(session, state))
            .collect(),
    )
}

/// What the user is looking at: the active document, view tab, selected range and
/// selected feature (null fields when nothing is open or selected).
pub fn get_view(session: &Session) -> Result<Value, Diagnostic> {
    let view = session.view();
    let Some(id) = view.active_document_id else {
        return Ok(
            json!({"document": null, "view_tab": view.view_tab, "selection": null, "selected_feature": null}),
        );
    };
    let state = session.document_state(id)?;
    let record = &session.report(id)?.record;
    let selection = selection_json(view.selection, record.sequence().len(), record.name())
        .map_err(|e| diagnostic("invalid_view", e.message))?;
    let selected_feature = match &view.selected_feature_id {
        Some(feature_id) => dnagent_app::feature_views(record)
            .into_iter()
            .find(|f| &f.id == feature_id)
            .map(|f| serde_json::to_value(f).map_err(|e| diagnostic("internal", e)))
            .transpose()?,
        None => None,
    };
    Ok(json!({
        "document": summary(session, &state),
        "view_tab": view.view_tab,
        "selection": selection,
        "selected_feature": selected_feature,
    }))
}

/// All features of a document (current revision, unsaved edits included).
pub fn get_features(session: &Session, id: u32) -> Result<Value, Diagnostic> {
    let state = session.document_state(id)?;
    let features = dnagent_app::feature_views(&session.report(id)?.record);
    Ok(json!({
        "document_id": id,
        "edit_counter": state.edit.edit_counter,
        "features": features,
    }))
}

/// Write the current revision (unsaved edits included) as DNAgent GenBank into
/// `<workspace>/.dnagent/snapshots/` and return its path, for work with the CLI.
pub fn export_snapshot(session: &Session, id: u32) -> Result<Value, Diagnostic> {
    let state = session.document_state(id)?;
    let report = session.report(id)?;
    let folder = workspace(session).join(SNAPSHOT_DIR);
    std::fs::create_dir_all(&folder).map_err(|e| {
        diagnostic(
            "snapshot_failed",
            format!("cannot create {}: {e}", folder.display()),
        )
    })?;
    let path = folder.join(format!(
        "{}-{id}.gb",
        safe_stem(std::path::Path::new(&state.edit.source_path))
    ));
    let (text, _) = dnagent_app::genbank_text(report, &dnagent_app::genbank_date_today());
    dnagent_app::write_atomic(&path, &text).map_err(|e| diagnostic("snapshot_failed", e))?;
    Ok(json!({
        "document_id": id,
        "edit_counter": state.edit.edit_counter,
        "path": path.display().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{RangeRequest, ViewReport};
    use std::path::Path;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/formats/snapgene")
            .join(name)
    }

    fn opened(workspace: &Path) -> (Session, u32) {
        let mut session = Session::default();
        let id = session
            .open(&fixture("synthetic_linear.dna"))
            .expect("open")
            .edit
            .document_id;
        session.report_view(ViewReport {
            document_ids: vec![id],
            active_document_id: Some(id),
            view_tab: "map".into(),
            selection: None,
            selected_feature_id: None,
            workspace: workspace.display().to_string(),
        });
        (session, id)
    }

    #[test]
    fn view_reports_the_selected_feature_as_a_cli_row() {
        let (mut session, id) = opened(Path::new("/nonexistent"));
        let features = dnagent_app::feature_views(&session.report(id).unwrap().record);
        let feature = features.last().expect("fixture has features");
        let mut view = session.view().clone();
        view.selected_feature_id = Some(feature.id.clone());
        view.selection = Some(RangeRequest { start: 2, end: 5 });
        session.report_view(view);
        let result = get_view(&session).unwrap();
        assert_eq!(
            result["selected_feature"],
            serde_json::to_value(feature).unwrap()
        );
        assert_eq!(result["selection"]["length"], 3);
        assert_eq!(result["document"]["document_id"], id);
        assert_eq!(result["document"]["active"], true);
    }

    #[test]
    fn empty_view_has_null_document() {
        let session = Session::default();
        assert_eq!(get_view(&session).unwrap()["document"], Value::Null);
        assert_eq!(list_documents(&session), json!([]));
    }

    #[test]
    fn invalid_reported_selection_is_an_error() {
        let (mut session, _) = opened(Path::new("/nonexistent"));
        let mut view = session.view().clone();
        view.selection = Some(RangeRequest {
            start: 1,
            end: 100_000,
        });
        session.report_view(view);
        assert_eq!(get_view(&session).unwrap_err().code, "invalid_view");
    }

    #[test]
    fn edit_counter_never_repeats_across_undo() {
        let (mut session, id) = opened(Path::new("/nonexistent"));
        let request: crate::session::FeatureRequest = serde_json::from_value(json!({
            "start": 1, "end": 4, "strand": "forward", "kind": "misc_feature",
            "label": "probe", "color": null, "translate": null
        }))
        .unwrap();
        let after_add = session.add_feature(id, &request).unwrap().edit;
        let after_undo = session.undo(id).unwrap().edit;
        let after_other = session.add_feature(id, &request).unwrap().edit;
        // The history index repeats; the counter must not.
        assert_eq!(after_add.revision, after_other.revision);
        assert!(after_add.edit_counter < after_undo.edit_counter);
        assert!(after_undo.edit_counter < after_other.edit_counter);
        assert_eq!(
            get_features(&session, id).unwrap()["edit_counter"],
            after_other.edit_counter
        );
    }

    #[test]
    fn snapshot_is_written_into_the_hidden_workspace_folder() {
        let dir = std::env::temp_dir().join(format!("dnagent-agent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (session, id) = opened(&dir);
        let result = export_snapshot(&session, id).unwrap();
        let path = PathBuf::from(result["path"].as_str().unwrap());
        assert!(path.starts_with(dir.join(SNAPSHOT_DIR)));
        let reopened = dnagent_app::open_path(&path).expect("snapshot reopens");
        assert_eq!(
            reopened.record.sequence().len(),
            session.report(id).unwrap().record.sequence().len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
