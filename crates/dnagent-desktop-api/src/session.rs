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
    /// Features added in this session (marked "added" in the list).
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
    /// Optional `/note` qualifier.
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
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
        note: request.note.clone(),
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

    /// Add several features as one edit (one undo step), in the given order.
    pub fn add_features(
        &mut self,
        id: u32,
        requests: &[FeatureRequest],
    ) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        let current = &open.history[open.index];
        let mut report = current.report.clone();
        let mut added = current.added.clone();
        for request in requests {
            let (next, feature_id, _) = editing::add_feature(&report, &spec_of(request))
                .map_err(|e| diagnostic("edit_failed", format!("{}: {e}", request.label)))?;
            report = next;
            added.insert(feature_id);
        }
        self.push(id, Entry { report, added })
    }

    /// Library features found in the document (see `detection`).
    pub fn detect_features(
        &self,
        id: u32,
    ) -> Result<crate::detection::DetectionResult, Diagnostic> {
        crate::detection::detect(self.record(id)?)
    }

    /// Delete a feature (imported or added); undo restores it.
    pub fn remove_feature(
        &mut self,
        id: u32,
        feature_id: &str,
    ) -> Result<DocumentState, Diagnostic> {
        let open = self.open_mut(id)?;
        let current = &open.history[open.index];
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

    fn record(&self, id: u32) -> Result<&dnagent_domain::SequenceRecord, Diagnostic> {
        let open = self
            .documents
            .get(&id)
            .ok_or_else(|| diagnostic("no_such_document", format!("document {id} is not open")))?;
        Ok(&open.history[open.index].report.record)
    }

    pub fn enzyme_counts(
        &self,
        id: u32,
    ) -> Result<Vec<crate::restriction::EnzymeCount>, Diagnostic> {
        crate::restriction::counts(self.record(id)?)
    }

    pub fn find_sites(
        &self,
        id: u32,
        enzymes: &[String],
    ) -> Result<Vec<crate::restriction::Site>, Diagnostic> {
        crate::restriction::sites(self.record(id)?, enzymes)
    }

    pub fn digest(
        &self,
        id: u32,
        enzymes: &[String],
    ) -> Result<Vec<crate::restriction::Fragment>, Diagnostic> {
        crate::restriction::digest(self.record(id)?, enzymes)
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
        "add_features" => {
            #[derive(Deserialize)]
            struct FeaturesArg {
                document_id: u32,
                requests: Vec<FeatureRequest>,
            }
            let a: FeaturesArg = arg(args)?;
            value(session.add_features(a.document_id, &a.requests))
        }
        "detect_features" => value(session.detect_features(arg::<DocumentArg>(args)?.document_id)),
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
        "close_document" => {
            session.close(arg::<DocumentArg>(args)?.document_id);
            Ok(serde_json::Value::Null)
        }
        "write_handoff" => {
            #[derive(Deserialize)]
            struct HandoffArg {
                workspace: String,
                items: Vec<HandoffItem>,
            }
            let a: HandoffArg = arg(args)?;
            value(session.write_handoff(Path::new(&a.workspace), &a.items))
        }
        "poll_files" => {
            #[derive(Deserialize)]
            struct PollArg {
                workspace: String,
                open_paths: Vec<String>,
            }
            let a: PollArg = arg(args)?;
            value(Ok(poll_files(Path::new(&a.workspace), &a.open_paths)))
        }
        "default_workspace" => value(Ok(default_workspace())),
        "enzyme_catalogue" => value(Ok(crate::restriction::catalogue_info())),
        "enzyme_counts" => value(session.enzyme_counts(arg::<DocumentArg>(args)?.document_id)),
        "find_sites" | "digest" => {
            #[derive(Deserialize)]
            struct EnzymesArg {
                document_id: u32,
                enzymes: Vec<String>,
            }
            let a: EnzymesArg = arg(args)?;
            if command == "digest" {
                value(session.digest(a.document_id, &a.enzymes))
            } else {
                value(session.find_sites(a.document_id, &a.enzymes))
            }
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
            note: None,
        }
    }

    #[test]
    fn detected_features_are_added_as_one_undo_step_with_a_note() {
        let dir =
            std::env::temp_dir().join(format!("dnagent-session-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = dir.join("features.sqlite");
        let mut library = dnagent_library::Library::open_or_create(&db).unwrap();
        let genbank = fixture("pUC19_M77789.dna")
            .parent()
            .unwrap()
            .join("../genbank/pUC19_M77789.gb");
        dnagent_app::library::import_into_library(&mut library, &[genbank], 12, false).unwrap();
        let mut session = Session::default();
        let fasta = fixture("pUC19_M77789.dna")
            .parent()
            .unwrap()
            .join("../fasta/pUC19_M77789.fasta");
        let opened = session.open(&fasta).unwrap();
        let id = opened.edit.document_id;
        let found = crate::detection::detect_with(session.record(id).unwrap(), &db).unwrap();
        assert!(found.available);
        assert_eq!(
            found.proposals.len(),
            7,
            "the seven pUC19 GenBank features, none annotated in FASTA"
        );
        assert!(found.proposals.iter().all(|p| p.annotated_as.is_empty()));
        let requests: Vec<FeatureRequest> = found
            .proposals
            .iter()
            .map(|p| FeatureRequest {
                start: p.start,
                end: p.start + p.length,
                strand: p.strand,
                kind: p.kind.clone(),
                label: p.name.clone(),
                color: p.color.clone(),
                translate: None,
                note: Some(format!("DNAgent feature library #{}", p.library_id)),
            })
            .collect();
        let state = session.add_features(id, &requests).unwrap();
        assert_eq!(state.document.features.len(), 7);
        assert_eq!(state.edit.added_feature_ids.len(), 7);
        let first = &session.record(id).unwrap().features()[0];
        assert!(first.qualifiers().iter().any(|q| {
            q.key == "note"
                && q.value
                    .as_deref()
                    .is_some_and(|v| v.starts_with("DNAgent feature library #"))
        }));
        let again = crate::detection::detect_with(session.record(id).unwrap(), &db).unwrap();
        assert!(
            again.proposals.iter().all(|p| !p.annotated_as.is_empty()),
            "now all annotated"
        );
        let undone = session.undo(id).unwrap();
        assert!(
            undone.document.features.is_empty(),
            "one undo removes them all"
        );
        let missing =
            crate::detection::detect_with(session.record(id).unwrap(), &dir.join("none.sqlite"))
                .unwrap();
        assert!(!missing.available && missing.message.contains("dnagent library import"));
        assert!(!dir.join("none.sqlite").exists());
        let _ = std::fs::remove_dir_all(&dir);
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
    fn any_feature_can_be_removed_and_undone_and_saving_clears_dirty() {
        let mut session = Session::default();
        let opened = session.open(&fixture("synthetic_translation.dna")).unwrap();
        let id = opened.edit.document_id;
        let imported = opened.document.features.len();
        let removed = session.remove_feature(id, "feature-0001").unwrap();
        assert_eq!(
            removed.document.features.len(),
            imported - 1,
            "imported features can be deleted"
        );
        assert!(
            removed
                .document
                .features
                .iter()
                .all(|f| f.id != "feature-0001")
        );
        let restored = session.undo(id).unwrap();
        assert_eq!(restored.document.features.len(), imported);
        assert_eq!(
            restored.document.features[0].id, "feature-0001",
            "undo restores it in place"
        );
        assert_eq!(
            session.remove_feature(id, "missing").unwrap_err().code,
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

// ------------------------------------------------------------------ agent handoff

/// A selected base range from the GUI (half-open; `end < start` wraps on circles).
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct RangeRequest {
    pub start: u32,
    pub end: u32,
}

/// One open tab in a handoff: its GUI selection is recorded for the agent.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct HandoffItem {
    pub document_id: u32,
    pub active: bool,
    pub selection: Option<RangeRequest>,
    pub selected_feature_id: Option<String>,
}

/// What a handoff wrote, and the prompt to give the agent.
#[derive(Debug, Serialize, TS)]
pub struct HandoffResult {
    pub workspace: String,
    pub context_path: String,
    pub snapshots: Vec<String>,
    pub prompt: String,
}

/// Modification stamp of a file the GUI watches.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FileStamp {
    pub path: String,
    pub modified_ms: f64,
    pub size: f64,
}

/// Folder name inside the workspace for handoff snapshots (not watched).
pub const HANDOFF_DIR: &str = "handoff";
const SEQUENCE_EXTENSIONS: [&str; 7] = ["dna", "gb", "gbk", "genbank", "fa", "fasta", "fna"];

/// `~/DNAgent`, the default workspace.
#[must_use]
pub fn default_workspace() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join("DNAgent").display().to_string()
}

fn safe_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("construct");
    let clean: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        "construct".into()
    } else {
        clean
    }
}

fn stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    #[allow(clippy::cast_precision_loss)]
    // millisecond stamps and file sizes fit f64 exactly in practice
    Some(FileStamp {
        path: path.display().to_string(),
        modified_ms: modified.as_millis() as f64,
        size: meta.len() as f64,
    })
}

/// Sequence files in the workspace (two levels, skipping hidden folders and the handoff
/// folder) plus the given open files, sorted by path.
#[must_use]
pub fn poll_files(workspace: &Path, open_paths: &[String]) -> Vec<FileStamp> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<FileStamp>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.ends_with(".dnagent-tmp") {
                continue;
            }
            if path.is_dir() {
                if depth > 0 && name != HANDOFF_DIR {
                    walk(&path, depth - 1, out);
                }
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| SEQUENCE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            {
                out.extend(stamp(&path));
            }
        }
    }
    let mut stamps = Vec::new();
    walk(workspace, 1, &mut stamps);
    for path in open_paths {
        if !stamps.iter().any(|s| &s.path == path) {
            stamps.extend(stamp(Path::new(path)));
        }
    }
    stamps.sort_by(|a, b| a.path.cmp(&b.path));
    stamps
}

/// Create `<workspace>/handoff/` and remove the previous handoff's files.
fn prepare_handoff_folder(workspace: &Path) -> Result<PathBuf, Diagnostic> {
    let folder = workspace.join(HANDOFF_DIR);
    std::fs::create_dir_all(&folder).map_err(|e| {
        diagnostic(
            "handoff_failed",
            format!("cannot create {}: {e}", folder.display()),
        )
    })?;
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "gb")
                || path.file_name().is_some_and(|n| n == "context.json")
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    Ok(folder)
}

/// Validated selection for context.json (null when nothing is selected).
fn selection_json(
    selection: Option<RangeRequest>,
    length: usize,
    name: &str,
) -> Result<serde_json::Value, Diagnostic> {
    Ok(match selection {
        Some(range) => {
            let (start, end) = (range.start as usize, range.end as usize);
            if start >= length || end > length || start == end {
                return Err(diagnostic(
                    "handoff_failed",
                    format!("selection {start}..{end} is outside {name}"),
                ));
            }
            let span = if end > start {
                end - start
            } else {
                length - start + end
            };
            serde_json::json!({"start": start, "end": end, "length": span, "wraps_origin": end < start})
        }
        None => serde_json::Value::Null,
    })
}

impl Session {
    /// Write GenBank snapshots of the given documents (current revisions, unsaved edits
    /// included) and `context.json` into `<workspace>/handoff/`, replacing the previous
    /// handoff. Source files and saved state are untouched.
    pub fn write_handoff(
        &self,
        workspace: &Path,
        items: &[HandoffItem],
    ) -> Result<HandoffResult, Diagnostic> {
        if items.is_empty() {
            return Err(diagnostic(
                "handoff_failed",
                "no open constructs to hand off",
            ));
        }
        let folder = prepare_handoff_folder(workspace)?;
        let date = dnagent_app::genbank_date_today();
        let mut names: Vec<String> = Vec::new();
        let mut constructs = Vec::new();
        let mut snapshots = Vec::new();
        for item in items {
            let open = self.documents.get(&item.document_id).ok_or_else(|| {
                diagnostic(
                    "no_such_document",
                    format!("document {} is not open", item.document_id),
                )
            })?;
            let report = &open.history[open.index].report;
            let record = &report.record;
            let length = record.sequence().len();
            let base = safe_stem(&open.source);
            let mut name = base.clone();
            let mut n = 2;
            while names.contains(&name) {
                name = format!("{base}-{n}");
                n += 1;
            }
            names.push(name.clone());
            let snapshot = folder.join(format!("{name}.gb"));
            let (text, _) = dnagent_app::genbank_text(report, &date);
            dnagent_app::write_atomic(&snapshot, &text)
                .map_err(|e| diagnostic("handoff_failed", e))?;
            let features = dnagent_app::feature_views(record);
            let selection = selection_json(item.selection, length, record.name())?;
            let selected_feature = item
                .selected_feature_id
                .as_ref()
                .and_then(|id| features.iter().find(|f| &f.id == id));
            constructs.push(serde_json::json!({
                "document_id": item.document_id,
                "name": record.name(),
                "active": item.active,
                "snapshot": snapshot.display().to_string(),
                "source": open.source.display().to_string(),
                "unsaved_changes": open.index != open.saved_index,
                "length": length,
                "topology": record.topology(),
                "selection": selection,
                "selected_feature": selected_feature,
                "added_feature_ids": open.history[open.index].added.iter().collect::<Vec<_>>(),
                "features": features,
            }));
            snapshots.push(snapshot.display().to_string());
        }
        let context_path = folder.join("context.json");
        let context = serde_json::json!({
            "format": "dnagent-handoff",
            "version": 1,
            "created": dnagent_app::utc_timestamp(),
            "workspace": workspace.display().to_string(),
            "coordinates": "zero-based, half-open; selections with end < start wrap through the origin",
            "instructions": [
                "Each construct's snapshot is DNAgent GenBank of what is open in DNAgent, including unsaved edits; read it with the dnagent CLI.",
                "Selections and selected features mark the regions the user is pointing at.",
                "Write results as GenBank (.gb) into the workspace folder (not into handoff/); DNAgent offers to open new files and reloads changed open files.",
            ],
            "constructs": constructs,
        });
        let text =
            serde_json::to_string_pretty(&context).map_err(|e| diagnostic("internal", e))? + "\n";
        dnagent_app::write_atomic(&context_path, &text)
            .map_err(|e| diagnostic("handoff_failed", e))?;
        let prompt = format!(
            "DNAgent handoff: {} open construct{} (snapshots include unsaved edits) — context: {}\n\
             Selections mark the regions of interest. Use the dnagent CLI and write results as GenBank into {} (they will open in DNAgent automatically).",
            items.len(),
            if items.len() == 1 { "" } else { "s" },
            context_path.display(),
            workspace.display(),
        );
        Ok(HandoffResult {
            workspace: workspace.display().to_string(),
            context_path: context_path.display().to_string(),
            snapshots,
            prompt,
        })
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/formats/snapgene")
            .join(name)
    }

    #[test]
    fn handoff_writes_snapshots_with_unsaved_edits_and_context() {
        let dir = std::env::temp_dir().join(format!("dnagent-handoff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut session = Session::default();
        let a = session
            .open(&fixture("synthetic_translation.dna"))
            .unwrap()
            .edit
            .document_id;
        let b = session
            .open(&fixture("synthetic_translation.dna"))
            .unwrap()
            .edit
            .document_id;
        let request = FeatureRequest {
            start: 0,
            end: 9,
            strand: Direction::Forward,
            kind: "misc_feature".into(),
            label: "unsaved".into(),
            color: None,
            translate: None,
            note: None,
        };
        session.add_feature(a, &request).unwrap();
        let result = session
            .write_handoff(
                &dir,
                &[
                    HandoffItem {
                        document_id: a,
                        active: true,
                        selection: Some(RangeRequest {
                            start: 550,
                            end: 10,
                        }),
                        selected_feature_id: None,
                    },
                    HandoffItem {
                        document_id: b,
                        active: false,
                        selection: None,
                        selected_feature_id: Some("feature-0001".into()),
                    },
                ],
            )
            .unwrap();
        assert_eq!(result.snapshots.len(), 2);
        assert!(
            result.snapshots[1].ends_with("synthetic_translation-2.gb"),
            "names are unique"
        );
        let snapshot = dnagent_app::open_path(Path::new(&result.snapshots[0])).unwrap();
        assert!(
            snapshot
                .record
                .features()
                .iter()
                .any(|f| f.label() == "unsaved"),
            "unsaved edits reach the agent"
        );
        let context: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&result.context_path).unwrap()).unwrap();
        assert_eq!(
            context["constructs"][0]["selection"],
            serde_json::json!({"start": 550, "end": 10, "length": 24, "wraps_origin": true})
        );
        assert_eq!(context["constructs"][0]["unsaved_changes"], true);
        assert_eq!(
            context["constructs"][1]["selected_feature"]["id"],
            "feature-0001"
        );
        assert!(result.prompt.contains("context.json"));
        // Handoff files are not reported as workspace files; a written product is.
        std::fs::write(dir.join("product.gb"), "x").unwrap();
        let stamps = poll_files(&dir, &[]);
        assert_eq!(
            stamps.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            vec![dir.join("product.gb").display().to_string()]
        );
        // Out-of-range selections are refused.
        assert!(
            session
                .write_handoff(
                    &dir,
                    &[HandoffItem {
                        document_id: a,
                        active: true,
                        selection: Some(RangeRequest { start: 5, end: 5 }),
                        selected_feature_id: None
                    }]
                )
                .is_err()
        );
        assert!(session.write_handoff(&dir, &[]).is_err());
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    /// Every Tauri command name must also be served by `dispatch` (the e2e server).
    #[test]
    fn every_desktop_command_dispatches() {
        let tauri_main = include_str!("../../../desktop/src-tauri/src/main.rs");
        let handler = tauri_main
            .split("generate_handler![")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("handler list");
        let mut session = Session::default();
        for command in handler.split(',').map(str::trim).filter(|c| !c.is_empty()) {
            let error = dispatch(&mut session, command, &serde_json::json!({})).err();
            assert_ne!(
                error.map(|e| e.code),
                Some("unknown_command".to_owned()),
                "{command} is not dispatched"
            );
        }
    }
}
