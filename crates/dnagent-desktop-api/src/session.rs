//! Stateful desktop session: open documents, edit history (undo/redo) and saving.
//! Used by the Tauri shell and by the e2e test server (via [`dispatch`]); the frontend
//! only requests operations. All biology and validation stay in `dnagent-app`.
use crate::{Diagnostic, Direction, Document, Segment, document_from_report};
use dnagent_app::editing::{self, FeatureSpec, TranslateSpec};
use dnagent_domain::Strand;
use dnagent_formats::ImportReport;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Edit status of an open document.
#[derive(Debug, Serialize, TS)]
pub struct EditState {
    pub document_id: u32,
    /// Position in the edit history (0 = as opened).
    pub revision: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Changes since opening or the last save.
    pub dirty: bool,
    /// Features added in this session (the only ones that can be deleted).
    pub added_feature_ids: Vec<String>,
    pub source_path: String,
    /// Last GenBank save of this session, if any.
    pub saved_path: Option<String>,
}

/// A document plus its edit state.
#[derive(Debug, Serialize, TS)]
pub struct DocumentState {
    pub document: Document,
    pub edit: EditState,
}

/// Translation settings for a new CDS.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct TranslateRequest {
    pub table: u32,
    pub codon_start: u8,
}

/// A new single-part feature over `[start, end)` (`end < start` wraps on circular records).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct FeatureRequest {
    pub start: u32,
    pub end: u32,
    pub strand: Direction,
    pub kind: String,
    pub label: String,
    pub color: Option<String>,
    pub translate: Option<TranslateRequest>,
}

/// Engine preview of a feature before it is added.
#[derive(Debug, Serialize, TS)]
pub struct FeaturePreview {
    pub length: u32,
    pub parts: Vec<Segment>,
    /// One letter per codon (stops as `*`), when translated.
    pub protein: Option<String>,
    pub warnings: Vec<Diagnostic>,
}

/// Result of saving: the updated state and limitations of the write (never silent).
#[derive(Debug, Serialize, TS)]
pub struct SaveResult {
    pub state: DocumentState,
    pub warnings: Vec<Diagnostic>,
}

struct Entry {
    report: ImportReport,
    added: BTreeSet<String>,
}

struct Open {
    source: PathBuf,
    history: Vec<Entry>,
    index: usize,
    saved_index: usize,
    saved_path: Option<PathBuf>,
}

/// All documents opened in one window (desktop) or one browser session (e2e).
#[derive(Default)]
pub struct Session {
    documents: HashMap<u32, Open>,
    next_id: u32,
}

fn diagnostic(code: &str, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: error.to_string(),
    }
}

fn spec_of(request: &FeatureRequest) -> FeatureSpec {
    FeatureSpec {
        start: request.start as usize,
        end: request.end as usize,
        strand: match request.strand {
            Direction::Forward => Strand::Forward,
            Direction::Reverse => Strand::Reverse,
            Direction::Unknown => Strand::Unknown,
        },
        kind: request.kind.clone(),
        label: request.label.clone(),
        color: request.color.clone(),
        translate: request.translate.map(|t| TranslateSpec {
            table: t.table,
            codon_start: t.codon_start,
        }),
    }
}

impl Session {
    fn state(&self, id: u32) -> Result<DocumentState, Diagnostic> {
        let open = self
            .documents
            .get(&id)
            .ok_or_else(|| diagnostic("no_such_document", format!("document {id} is not open")))?;
        let entry = &open.history[open.index];
        Ok(DocumentState {
            document: document_from_report(&entry.report)?,
            edit: EditState {
                document_id: id,
                revision: u32::try_from(open.index).unwrap_or(u32::MAX),
                can_undo: open.index > 0,
                can_redo: open.index + 1 < open.history.len(),
                dirty: open.index != open.saved_index,
                added_feature_ids: entry.added.iter().cloned().collect(),
                source_path: open.source.display().to_string(),
                saved_path: open.saved_path.as_ref().map(|p| p.display().to_string()),
            },
        })
    }

    fn open_mut(&mut self, id: u32) -> Result<&mut Open, Diagnostic> {
        self.documents
            .get_mut(&id)
            .ok_or_else(|| diagnostic("no_such_document", format!("document {id} is not open")))
    }

    fn push(&mut self, id: u32, entry: Entry) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        open.history.truncate(open.index + 1);
        open.history.push(entry);
        open.index += 1;
        self.state(id)
    }

    /// Open a file as a new document.
    pub fn open(&mut self, path: &Path) -> Result<DocumentState, Diagnostic> {
        let report = dnagent_app::open_path(path).map_err(|e| diagnostic("import_failed", e))?;
        document_from_report(&report)?; // size limit etc. before accepting
        self.next_id += 1;
        let id = self.next_id;
        self.documents.insert(
            id,
            Open {
                source: path.to_path_buf(),
                history: vec![Entry {
                    report,
                    added: BTreeSet::new(),
                }],
                index: 0,
                saved_index: 0,
                saved_path: None,
            },
        );
        self.state(id)
    }

    /// Engine preview of a feature (translation and warnings) without changing anything.
    pub fn preview_feature(
        &self,
        id: u32,
        request: &FeatureRequest,
    ) -> Result<FeaturePreview, Diagnostic> {
        let open = self
            .documents
            .get(&id)
            .ok_or_else(|| diagnostic("no_such_document", format!("document {id} is not open")))?;
        let preview =
            editing::preview_feature(&open.history[open.index].report.record, &spec_of(request))
                .map_err(|e| diagnostic("edit_failed", e))?;
        Ok(FeaturePreview {
            length: u32::try_from(preview.length).unwrap_or(u32::MAX),
            parts: preview
                .location
                .parts()
                .iter()
                .map(|p| Segment {
                    start: u32::try_from(p.start().get()).unwrap_or(u32::MAX),
                    length: u32::try_from(p.length().get()).unwrap_or(u32::MAX),
                })
                .collect(),
            protein: preview.translation.map(|t| t.protein),
            warnings: preview
                .warnings
                .into_iter()
                .map(|w| Diagnostic {
                    code: w.code,
                    message: w.message,
                })
                .collect(),
        })
    }

    pub fn add_feature(
        &mut self,
        id: u32,
        request: &FeatureRequest,
    ) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        let current = &open.history[open.index];
        let (report, feature_id, _) = editing::add_feature(&current.report, &spec_of(request))
            .map_err(|e| diagnostic("edit_failed", e))?;
        let mut added = current.added.clone();
        added.insert(feature_id);
        self.push(id, Entry { report, added })
    }

    /// Delete a feature added in this session (imported features are read-only).
    pub fn remove_feature(
        &mut self,
        id: u32,
        feature_id: &str,
    ) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        let current = &open.history[open.index];
        if !current.added.contains(feature_id) {
            return Err(diagnostic(
                "edit_failed",
                format!(
                    "{feature_id} was not added in this session; imported features are read-only"
                ),
            ));
        }
        let report = editing::remove_feature(&current.report, feature_id)
            .map_err(|e| diagnostic("edit_failed", e))?;
        let mut added = current.added.clone();
        added.remove(feature_id);
        self.push(id, Entry { report, added })
    }

    pub fn undo(&mut self, id: u32) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        if open.index == 0 {
            return Err(diagnostic("edit_failed", "nothing to undo"));
        }
        open.index -= 1;
        self.state(id)
    }

    pub fn redo(&mut self, id: u32) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        if open.index + 1 >= open.history.len() {
            return Err(diagnostic("edit_failed", "nothing to redo"));
        }
        open.index += 1;
        self.state(id)
    }

    /// Save the current revision as GenBank at `path` (atomic write).
    pub fn save_genbank(&mut self, id: u32, path: &Path) -> Result<SaveResult, Diagnostic> {
        let open = self.open_mut(id)?;
        let warnings = dnagent_app::save_genbank(
            &open.history[open.index].report,
            path,
            &dnagent_app::genbank_date_today(),
        )
        .map_err(|e| diagnostic("save_failed", e))?;
        open.saved_index = open.index;
        open.saved_path = Some(path.to_path_buf());
        Ok(SaveResult {
            state: self.state(id)?,
            warnings: warnings
                .into_iter()
                .map(|w| Diagnostic {
                    code: w.code,
                    message: w.message,
                })
                .collect(),
        })
    }

    pub fn close(&mut self, id: u32) {
        self.documents.remove(&id);
    }
}

#[derive(Deserialize)]
struct DocumentArg {
    document_id: u32,
}

fn arg<T: serde::de::DeserializeOwned>(args: &serde_json::Value) -> Result<T, Diagnostic> {
    serde_json::from_value(args.clone()).map_err(|e| diagnostic("invalid_request", e))
}

fn value<T: Serialize>(result: Result<T, Diagnostic>) -> Result<serde_json::Value, Diagnostic> {
    result.and_then(|v| serde_json::to_value(v).map_err(|e| diagnostic("internal", e)))
}

/// JSON command dispatch with the same names and arguments as the Tauri commands
/// (used by the e2e test server). Unknown commands fail loudly.
pub fn dispatch(
    session: &mut Session,
    command: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, Diagnostic> {
    #[derive(Deserialize)]
    struct PathArg {
        path: String,
    }
    #[derive(Deserialize)]
    struct FeatureArg {
        document_id: u32,
        request: FeatureRequest,
    }
    #[derive(Deserialize)]
    struct RemoveArg {
        document_id: u32,
        feature_id: String,
    }
    #[derive(Deserialize)]
    struct SaveArg {
        document_id: u32,
        path: String,
    }
    match command {
        "open_document" => value(session.open(Path::new(&arg::<PathArg>(args)?.path))),
        "preview_feature" => {
            let a: FeatureArg = arg(args)?;
            value(session.preview_feature(a.document_id, &a.request))
        }
        "add_feature" => {
            let a: FeatureArg = arg(args)?;
            value(session.add_feature(a.document_id, &a.request))
        }
        "remove_feature" => {
            let a: RemoveArg = arg(args)?;
            value(session.remove_feature(a.document_id, &a.feature_id))
        }
        "undo" => value(session.undo(arg::<DocumentArg>(args)?.document_id)),
        "redo" => value(session.redo(arg::<DocumentArg>(args)?.document_id)),
        "save_genbank" => {
            let a: SaveArg = arg(args)?;
            value(session.save_genbank(a.document_id, Path::new(&a.path)))
        }
        other => Err(diagnostic(
            "unknown_command",
            format!("no desktop command {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/formats/snapgene")
            .join(name)
    }

    fn request(start: u32, end: u32, translate: bool) -> FeatureRequest {
        FeatureRequest {
            start,
            end,
            strand: Direction::Forward,
            kind: "misc_feature".into(),
            label: "new".into(),
            color: None,
            translate: translate.then_some(TranslateRequest {
                table: 1,
                codon_start: 1,
            }),
        }
    }

    #[test]
    fn edits_have_linear_history_with_undo_redo_and_dirty_state() {
        let mut session = Session::default();
        let opened = session.open(&fixture("synthetic_translation.dna")).unwrap();
        let id = opened.edit.document_id;
        let count = opened.document.features.len();
        assert!(!opened.edit.dirty && !opened.edit.can_undo);
        let added = session.add_feature(id, &request(32, 278, true)).unwrap();
        assert_eq!(added.document.features.len(), count + 1);
        assert!(added.edit.dirty && added.edit.can_undo);
        let new_id = added.edit.added_feature_ids[0].clone();
        assert!(
            added
                .document
                .translations
                .iter()
                .any(|t| t.feature_id == new_id),
            "new CDS is translated"
        );
        let undone = session.undo(id).unwrap();
        assert_eq!(
            (
                undone.document.features.len(),
                undone.edit.dirty,
                undone.edit.can_redo
            ),
            (count, false, true)
        );
        let redone = session.redo(id).unwrap();
        assert_eq!(redone.document.features.len(), count + 1);
        // A new edit after undo discards the redo branch.
        session.undo(id).unwrap();
        let other = session.add_feature(id, &request(0, 9, false)).unwrap();
        assert!(!other.edit.can_redo);
        assert!(session.redo(id).is_err());
    }

    #[test]
    fn only_added_features_can_be_removed_and_saving_clears_dirty() {
        let mut session = Session::default();
        let id = session
            .open(&fixture("synthetic_translation.dna"))
            .unwrap()
            .edit
            .document_id;
        assert_eq!(
            session.remove_feature(id, "feature-0001").unwrap_err().code,
            "edit_failed"
        );
        let added = session.add_feature(id, &request(0, 9, false)).unwrap();
        let new_id = added.edit.added_feature_ids[0].clone();
        let dir = std::env::temp_dir().join(format!("dnagent-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("saved.gb");
        let saved = session.save_genbank(id, &out).unwrap();
        assert!(!saved.state.edit.dirty);
        assert_eq!(
            saved.state.edit.saved_path.as_deref(),
            Some(out.to_str().unwrap())
        );
        let removed = session.remove_feature(id, &new_id).unwrap();
        assert!(removed.edit.dirty);
        // The saved file reopens with the added feature.
        let reopened = Session::default().open(&out).unwrap();
        assert_eq!(
            reopened.document.features.len(),
            added.document.features.len()
        );
        assert_eq!(
            session
                .save_genbank(id, &dir.join("bad.dna"))
                .unwrap_err()
                .code,
            "save_failed"
        );
    }

    #[test]
    fn previews_and_dispatch_errors_are_structured() {
        let mut session = Session::default();
        let id = session
            .open(&fixture("synthetic_translation.dna"))
            .unwrap()
            .edit
            .document_id;
        let preview = session
            .preview_feature(id, &request(32, 278, true))
            .unwrap();
        assert!(preview.protein.unwrap().ends_with('*'));
        assert_eq!(preview.length, 246);
        let bad = dispatch(
            &mut session,
            "add_feature",
            &serde_json::json!({"document_id": id, "request": {"start": 5}}),
        );
        assert_eq!(bad.unwrap_err().code, "invalid_request");
        assert_eq!(
            dispatch(&mut session, "nope", &serde_json::json!({}))
                .unwrap_err()
                .code,
            "unknown_command"
        );
        assert_eq!(
            dispatch(
                &mut session,
                "undo",
                &serde_json::json!({"document_id": 99})
            )
            .unwrap_err()
            .code,
            "no_such_document"
        );
        let opened = dispatch(
            &mut session,
            "open_document",
            &serde_json::json!({"path": fixture("synthetic_linear.dna")}),
        )
        .unwrap();
        assert_eq!(opened["edit"]["revision"], 0);
    }
}
