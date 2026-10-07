//! Agent tools over the desktop session: what the user has open and is looking at, read
//! live by an agent (served over MCP by `dnagent-agent-mcp`). Results are JSON values in
//! the CLI's shapes (feature rows as in `dnagent features --output json`); coordinates are
//! zero-based and half-open, and `end < start` wraps through the origin.
use crate::Diagnostic;
use crate::session::{
    DocumentState, Highlight, QueuedRequest, RangeRequest, Session, default_workspace, safe_stem,
    selection_json,
};
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

// ------------------------------------------------------------------ pointing back

/// What the GUI must show before a view request counts as applied.
#[derive(Debug, Clone)]
pub enum Expected {
    /// The document has a tab and is active.
    Document(u32),
    Range(u32, RangeRequest),
    Feature(u32, String),
    /// The GUI applied the request with this sequence number.
    Delivered(u32),
}

/// Whether the frontend's last view report shows `expected`.
#[must_use]
pub fn applied(session: &Session, expected: &Expected) -> bool {
    let view = session.view();
    let active = |id: u32| view.active_document_id == Some(id) && view.document_ids.contains(&id);
    match expected {
        Expected::Document(id) => active(*id),
        Expected::Range(id, range) => {
            active(*id)
                && view
                    .selection
                    .is_some_and(|s| s.start == range.start && s.end == range.end)
        }
        Expected::Feature(id, feature) => {
            active(*id) && view.selected_feature_id.as_deref() == Some(feature.as_str())
        }
        Expected::Delivered(seq) => view.agent_seen >= *seq,
    }
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(path),
    }
}

/// Open a file in the app (or switch to its tab if it is already open). The document
/// is open in the session at once; the GUI shows it on its next sync.
pub fn open_file(session: &mut Session, path: &str) -> Result<(Value, Expected), Diagnostic> {
    let path = expand_home(path);
    let (id, already_open) = match session.document_for_source(&path) {
        Some(id) => (id, true),
        None => (session.open(&path)?.edit.document_id, false),
    };
    session.queue_agent_request(QueuedRequest::Open(id));
    let state = session.document_state(id)?;
    let mut result = summary(session, &state);
    result["already_open"] = json!(already_open);
    Ok((result, Expected::Document(id)))
}

fn checked_range(
    session: &Session,
    id: u32,
    start: u32,
    end: u32,
    what: &str,
) -> Result<RangeRequest, Diagnostic> {
    let record = &session.report(id)?.record;
    let range = RangeRequest { start, end };
    selection_json(Some(range), record.sequence().len(), record.name())
        .map_err(|_| {
            diagnostic(
                "invalid_range",
                format!(
                    "{what} [{start}, {end}) is outside {} ({} bp; half-open, end < start wraps only on circular records)",
                    record.name(),
                    record.sequence().len()
                ),
            )
        })?;
    if end < start && record.topology() != dnagent_domain::Topology::Circular {
        return Err(diagnostic(
            "invalid_range",
            format!(
                "{what} [{start}, {end}) wraps, but {} is linear",
                record.name()
            ),
        ));
    }
    Ok(range)
}

/// Select a base range in an open document (making it the active tab).
pub fn select_range(
    session: &mut Session,
    id: u32,
    start: u32,
    end: u32,
) -> Result<Expected, Diagnostic> {
    let range = checked_range(session, id, start, end, "range")?;
    session.queue_agent_request(QueuedRequest::Select {
        document_id: id,
        range: Some(range),
        feature_id: None,
    });
    Ok(Expected::Range(id, range))
}

/// Select a feature in an open document (making it the active tab).
pub fn select_feature(
    session: &mut Session,
    id: u32,
    feature_id: &str,
) -> Result<Expected, Diagnostic> {
    let record = &session.report(id)?.record;
    if !dnagent_app::feature_views(record)
        .iter()
        .any(|f| f.id == feature_id)
    {
        return Err(diagnostic(
            "no_such_feature",
            format!(
                "{} has no feature {feature_id}; see get_features",
                record.name()
            ),
        ));
    }
    session.queue_agent_request(QueuedRequest::Select {
        document_id: id,
        range: None,
        feature_id: Some(feature_id.to_owned()),
    });
    Ok(Expected::Feature(id, feature_id.to_owned()))
}

/// Show a message to the user in the app's agent panel.
pub fn notify(session: &mut Session, message: &str) -> Result<(Value, Expected), Diagnostic> {
    let message = message.trim();
    if message.is_empty() {
        return Err(diagnostic("invalid_request", "the message is empty"));
    }
    let seq = session.queue_agent_request(QueuedRequest::Notify(message.to_owned()));
    Ok((json!({}), Expected::Delivered(seq)))
}

/// Present a result: open `path` (e.g. an assembled product), show `summary` in the
/// agent panel and list `highlights` (the first is selected; the user clicks the rest).
pub fn present(
    session: &mut Session,
    path: &str,
    summary_text: &str,
    highlights: Vec<Highlight>,
) -> Result<(Value, Expected), Diagnostic> {
    if summary_text.trim().is_empty() {
        return Err(diagnostic("invalid_request", "the summary is empty"));
    }
    // Validate against the file before changing anything in the GUI.
    let (mut result, _) = open_file(session, path)?;
    let id = u32::try_from(result["document_id"].as_u64().unwrap_or_default())
        .map_err(|e| diagnostic("internal", e))?;
    for highlight in &highlights {
        if let Err(error) = checked_range(
            session,
            id,
            highlight.start,
            highlight.end,
            &format!("highlight {:?}", highlight.label),
        ) {
            // Opened just for this presentation: do not leave a half-presented tab.
            if result["already_open"] == json!(false) {
                session.close(id);
            }
            return Err(error);
        }
    }
    let expected = match highlights.first() {
        Some(first) => Expected::Range(
            id,
            RangeRequest {
                start: first.start,
                end: first.end,
            },
        ),
        None => Expected::Document(id),
    };
    result["highlights"] = json!(highlights.len());
    session.queue_agent_request(QueuedRequest::Present {
        document_id: id,
        summary: summary_text.trim().to_owned(),
        highlights,
    });
    Ok((result, expected))
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
            agent_seen: 0,
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
    fn agent_requests_reach_the_gui_once_and_in_order() {
        let (mut session, id) = opened(Path::new("/nonexistent"));
        let feature = dnagent_app::feature_views(&session.report(id).unwrap().record)[0]
            .id
            .clone();
        select_feature(&mut session, id, &feature).unwrap();
        select_range(&mut session, id, 1, 4).unwrap();
        notify(&mut session, " done ").unwrap();
        let requests = serde_json::to_value(session.agent_sync()).unwrap();
        assert_eq!(requests[0]["kind"], "select");
        assert_eq!(requests[0]["feature_id"], feature.as_str());
        assert_eq!(requests[1]["range"], json!({"start": 1, "end": 4}));
        assert_eq!(
            requests[2],
            json!({"kind": "notify", "seq": 3, "message": "done"})
        );
        assert!(session.agent_sync().is_empty(), "delivered once");
    }

    #[test]
    fn invalid_targets_are_rejected_before_queueing() {
        let (mut session, id) = opened(Path::new("/nonexistent"));
        assert_eq!(
            select_range(&mut session, id, 0, 10_000).unwrap_err().code,
            "invalid_range"
        );
        assert_eq!(
            select_range(&mut session, id, 5, 2).unwrap_err().code,
            "invalid_range",
            "linear records do not wrap"
        );
        assert_eq!(
            select_feature(&mut session, id, "feature-9999")
                .unwrap_err()
                .code,
            "no_such_feature"
        );
        assert_eq!(
            select_range(&mut session, 42, 0, 1).unwrap_err().code,
            "no_such_document"
        );
        assert!(notify(&mut session, "  ").is_err());
        assert_eq!(session.agent_sync().len(), 0, "nothing queued for the GUI");
    }

    #[test]
    fn open_file_reuses_an_open_tab() {
        let (mut session, id) = opened(Path::new("/nonexistent"));
        let (result, _) = open_file(
            &mut session,
            fixture("synthetic_linear.dna").to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(result["document_id"], id);
        assert_eq!(result["already_open"], true);
        let (other, expected) = open_file(
            &mut session,
            fixture("synthetic_multipart_origin.dna").to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(other["already_open"], false);
        let requests = serde_json::to_value(session.agent_sync()).unwrap();
        assert_eq!(requests.as_array().unwrap().len(), 2);
        assert_eq!(
            requests[1]["state"]["edit"]["document_id"],
            other["document_id"]
        );
        // Applied once the frontend reports the new tab as active.
        assert!(!applied(&session, &expected));
        let new_id = u32::try_from(other["document_id"].as_u64().unwrap()).unwrap();
        let mut view = session.view().clone();
        view.document_ids.push(new_id);
        view.active_document_id = Some(new_id);
        session.report_view(view);
        assert!(applied(&session, &expected));
    }

    #[test]
    fn present_with_a_bad_highlight_leaves_nothing_behind() {
        let (mut session, _) = opened(Path::new("/nonexistent"));
        let path = fixture("synthetic_multipart_origin.dna");
        let before = session.document_states().len();
        let bad = vec![Highlight {
            label: "junction".into(),
            start: 0,
            end: 99_999,
        }];
        assert_eq!(
            present(&mut session, path.to_str().unwrap(), "product", bad)
                .unwrap_err()
                .code,
            "invalid_range"
        );
        assert_eq!(session.document_states().len(), before);
        assert_eq!(session.agent_sync().len(), 0, "nothing queued for the GUI");
        let good = vec![Highlight {
            label: "junction".into(),
            start: 1,
            end: 3,
        }];
        let (result, expected) =
            present(&mut session, path.to_str().unwrap(), "product", good).unwrap();
        assert_eq!(result["highlights"], 1);
        assert!(matches!(
            expected,
            Expected::Range(_, RangeRequest { start: 1, end: 3 })
        ));
        let requests = serde_json::to_value(session.agent_sync()).unwrap();
        assert_eq!(requests[0]["kind"], "open");
        assert_eq!(requests[1]["kind"], "present");
        assert_eq!(requests[1]["summary"], "product");
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
