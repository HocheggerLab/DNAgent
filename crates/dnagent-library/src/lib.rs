//! The DNAgent feature library: a SQLite database of annotated features collected from
//! sequence files. A feature is identified by its sequence, independent of strand; every
//! file it was seen in is kept as an occurrence with the label, type and colour used there.
//! Names, types and colours are the most common ones among occurrences; other names are
//! aliases. This crate stores and queries; it computes no biology (sequences and their
//! strand-independent keys come from the caller).
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Bump with a migration when the schema changes.
pub const SCHEMA_VERSION: u32 = 2;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS features (
    id INTEGER PRIMARY KEY,
    canonical TEXT NOT NULL UNIQUE,  -- the smaller of the sequence and its reverse complement
    sequence TEXT NOT NULL,          -- 5'->3' in the direction of the representative occurrence
    length INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    color TEXT,
    qualifiers TEXT NOT NULL,        -- JSON [{key, value|null}, ...] of the representative occurrence
    stranded INTEGER NOT NULL DEFAULT 1, -- most occurrences have a strand (directional feature)
    family_id INTEGER,               -- the family head (itself for heads); set by the app layer
    grouping TEXT NOT NULL DEFAULT 'auto' CHECK (grouping IN ('auto', 'standalone')),
    status TEXT NOT NULL DEFAULT 'imported' CHECK (status IN ('imported', 'curated', 'hidden'))
);
CREATE TABLE IF NOT EXISTS sources (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    sha256 TEXT NOT NULL UNIQUE,
    record_name TEXT NOT NULL,
    imported INTEGER NOT NULL        -- Unix seconds
);
CREATE TABLE IF NOT EXISTS occurrences (
    id INTEGER PRIMARY KEY,
    feature_id INTEGER NOT NULL REFERENCES features(id) ON DELETE CASCADE,
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    kind TEXT NOT NULL,
    color TEXT,
    qualifiers TEXT NOT NULL,
    sequence TEXT NOT NULL,          -- 5'->3' in this occurrence's own direction
    stranded INTEGER NOT NULL,       -- the source gave it a strand
    start INTEGER NOT NULL,          -- first part start in the source (zero-based)
    parts INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS occurrences_feature ON occurrences(feature_id);
CREATE INDEX IF NOT EXISTS occurrences_source ON occurrences(source_id);
";

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("feature library {path}: {source}")]
    Sqlite {
        path: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("no feature library at {0}; build one with `dnagent library import <folder>`")]
    Missing(String),
    #[error(
        "feature library {path} has schema version {found}; this DNAgent reads version {SCHEMA_VERSION}"
    )]
    Version { path: String, found: String },
    #[error("failed to create {path}: {source}")]
    Create {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("no library feature with id {0}")]
    NoSuchFeature(i64),
    #[error("invalid qualifier JSON in the library: {0}")]
    Json(#[from] serde_json::Error),
}

/// A feature qualifier, as in `dnagent features` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Qualifier {
    pub key: String,
    pub value: Option<String>,
}

/// One annotated feature as found in a source file, ready to store.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub label: String,
    pub kind: String,
    pub color: Option<String>,
    pub qualifiers: Vec<Qualifier>,
    /// Bases 5'->3' in the feature's own direction.
    pub sequence: String,
    /// Strand-independent key (computed by the caller).
    pub canonical: String,
    /// The source gave the feature a strand (otherwise its direction is arbitrary).
    pub stranded: bool,
    pub start: usize,
    pub parts: usize,
}

/// What happened to one source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceOutcome {
    /// New or changed file: its occurrences were (re)recorded.
    Imported {
        occurrences: usize,
        new_features: usize,
        replaced: bool,
    },
    /// Same path and content as before: nothing to do.
    Unchanged,
    /// Same content already imported from another path.
    Duplicate { of: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryInfo {
    pub path: String,
    pub schema_version: u32,
    pub features: u64,
    /// Variant families among visible features (a feature without variants is its own family).
    pub families: u64,
    pub curated: u64,
    pub hidden: u64,
    pub sources: u64,
    pub occurrences: u64,
}

/// A library feature for listing (no sequence).
#[derive(Debug, Clone, Serialize)]
pub struct FeatureSummary {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub length: u64,
    pub color: Option<String>,
    pub status: String,
    /// Other labels this sequence carries, most common first.
    pub aliases: Vec<String>,
    pub occurrences: u64,
    pub sources: u64,
    /// The family head's id (the feature's own id for a head).
    pub family_id: i64,
    /// Other visible members of the family (0 for members).
    pub variants: u64,
    /// Occurrences of the whole family (heads; equals `occurrences` for members).
    pub family_occurrences: u64,
    /// `auto`, or `standalone` (never grouped into a family).
    pub grouping: String,
}

/// Where a feature was seen.
#[derive(Debug, Clone, Serialize)]
pub struct OccurrenceView {
    pub source_path: String,
    pub record_name: String,
    pub label: String,
    pub kind: String,
    pub start: u64,
    pub parts: u64,
    /// The occurrence reads as the reverse complement of the feature's `sequence`.
    pub reversed: bool,
}

/// A library feature with its sequence and provenance.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureDetail {
    #[serde(flatten)]
    pub summary: FeatureSummary,
    pub sequence: String,
    pub qualifiers: Vec<Qualifier>,
    pub seen_in: Vec<OccurrenceView>,
    /// The other members of its family (head first), shortest last.
    pub family: Vec<FeatureSummary>,
}

/// What detection needs: every feature that is not hidden.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub color: Option<String>,
    pub sequence: String,
    /// Directional: matches report a strand. Otherwise the strand is unknown.
    pub stranded: bool,
    /// Family head (the entry's own id for heads).
    pub family_id: i64,
    pub qualifiers: Vec<Qualifier>,
}

/// A manual change to one feature; any change marks it `curated` (or `hidden`), so later
/// imports no longer rename or retype it.
#[derive(Debug, Clone, Default)]
pub struct Edit {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub hidden: Option<bool>,
    /// Keep out of variant families (e.g. a homology arm inside an exon).
    pub standalone: Option<bool>,
}

/// A visible feature, for computing families.
#[derive(Debug, Clone)]
pub struct FamilyInput {
    pub id: i64,
    pub sequence: String,
    pub standalone: bool,
}

pub struct Library {
    connection: Connection,
    path: PathBuf,
}

impl Library {
    /// Open the library, creating the file and schema if needed.
    pub fn open_or_create(path: &Path) -> Result<Self, LibraryError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| LibraryError::Create {
                path: parent.display().to_string(),
                source,
            })?;
        }
        Self::connect(path, true)
    }

    /// Open an existing library; a missing file is an error, not an empty library.
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        if !path.exists() {
            return Err(LibraryError::Missing(path.display().to_string()));
        }
        Self::connect(path, false)
    }

    fn connect(path: &Path, create: bool) -> Result<Self, LibraryError> {
        let sql = |source| LibraryError::Sqlite {
            path: path.display().to_string(),
            source,
        };
        let connection = Connection::open(path).map_err(sql)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(sql)?;
        let library = Self {
            connection,
            path: path.to_owned(),
        };
        let has_meta: bool = library
            .connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map_err(sql)?
            > 0;
        if has_meta {
            let version: Option<String> = library
                .connection
                .query_row(
                    "SELECT value FROM meta WHERE key = 'schema_version'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            if version.as_deref() == Some("1") {
                library
                    .connection
                    .execute_batch(&format!("BEGIN; {MIGRATE_1_TO_2} COMMIT;"))
                    .map_err(sql)?;
            } else if version.as_deref() != Some(&SCHEMA_VERSION.to_string()) {
                return Err(LibraryError::Version {
                    path: path.display().to_string(),
                    found: version.unwrap_or_else(|| "none".into()),
                });
            }
        } else if create {
            library.connection.execute_batch(SCHEMA).map_err(sql)?;
            library
                .connection
                .execute("INSERT INTO meta (key, value) VALUES ('schema_version', ?1), ('format', 'dnagent-feature-library'), ('families', 'current')", params![SCHEMA_VERSION.to_string()])
                .map_err(sql)?;
        } else {
            return Err(LibraryError::Version {
                path: path.display().to_string(),
                found: "none (not a DNAgent feature library)".into(),
            });
        }
        Ok(library)
    }

    fn sql(&self, source: rusqlite::Error) -> LibraryError {
        LibraryError::Sqlite {
            path: self.path.display().to_string(),
            source,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Record one source file and its candidate features, in one transaction. A file seen
    /// before at the same path with other content replaces its earlier occurrences.
    pub fn import_source(
        &mut self,
        path: &str,
        sha256: &str,
        record_name: &str,
        imported: i64,
        candidates: &[Candidate],
        rescan: bool,
    ) -> Result<SourceOutcome, LibraryError> {
        let error_path = self.path.display().to_string();
        let sql = |source| LibraryError::Sqlite {
            path: error_path.clone(),
            source,
        };
        let tx = self.connection.transaction().map_err(sql)?;
        let by_hash: Option<String> = tx
            .query_row(
                "SELECT path FROM sources WHERE sha256 = ?1",
                [sha256],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        // `rescan` re-reads an unchanged file (e.g. after the collection rules changed).
        if let Some(existing) = by_hash.filter(|existing| !(rescan && existing == path)) {
            return Ok(if existing == path {
                SourceOutcome::Unchanged
            } else {
                SourceOutcome::Duplicate { of: existing }
            });
        }
        let replaced = tx
            .execute("DELETE FROM sources WHERE path = ?1", [path])
            .map_err(sql)?
            > 0;
        tx.execute(
            "INSERT INTO sources (path, sha256, record_name, imported) VALUES (?1, ?2, ?3, ?4)",
            params![path, sha256, record_name, imported],
        )
        .map_err(sql)?;
        let source_id = tx.last_insert_rowid();
        let mut new_features = 0;
        for candidate in candidates {
            let qualifiers = serde_json::to_string(&candidate.qualifiers)?;
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM features WHERE canonical = ?1",
                    [&candidate.canonical],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            let feature_id = if let Some(id) = existing {
                id
            } else {
                new_features += 1;
                tx.execute(
                    "INSERT INTO features (canonical, sequence, length, name, kind, color, qualifiers) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![candidate.canonical, candidate.sequence, int(candidate.sequence.len()), candidate.label, candidate.kind, candidate.color, qualifiers],
                )
                .map_err(sql)?;
                tx.last_insert_rowid()
            };
            tx.execute(
                "INSERT INTO occurrences (feature_id, source_id, label, kind, color, qualifiers, sequence, stranded, start, parts) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![feature_id, source_id, candidate.label, candidate.kind, candidate.color, qualifiers, candidate.sequence, candidate.stranded, int(candidate.start), int(candidate.parts)],
            )
            .map_err(sql)?;
        }
        refresh(&tx).map_err(sql)?;
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('families', 'stale')",
            [],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(SourceOutcome::Imported {
            occurrences: candidates.len(),
            new_features,
            replaced,
        })
    }

    pub fn info(&self) -> Result<LibraryInfo, LibraryError> {
        let count = |query: &str| {
            self.connection
                .query_row(query, [], |r| r.get::<_, i64>(0))
                .map(|n| u64::try_from(n).unwrap_or(0))
                .map_err(|e| self.sql(e))
        };
        Ok(LibraryInfo {
            path: self.path.display().to_string(),
            schema_version: SCHEMA_VERSION,
            features: count("SELECT count(*) FROM features WHERE status != 'hidden'")?,
            families: count(
                "SELECT count(*) FROM features WHERE status != 'hidden' AND coalesce(family_id, id) = id",
            )?,
            curated: count("SELECT count(*) FROM features WHERE status = 'curated'")?,
            hidden: count("SELECT count(*) FROM features WHERE status = 'hidden'")?,
            sources: count("SELECT count(*) FROM sources")?,
            occurrences: count("SELECT count(*) FROM occurrences")?,
        })
    }

    /// Features (hidden ones excluded unless asked), filtered by a case-insensitive
    /// substring of the name or any alias and by exact type; most-seen first.
    pub fn list(
        &self,
        search: Option<&str>,
        kind: Option<&str>,
        include_hidden: bool,
        limit: Option<usize>,
    ) -> Result<Vec<FeatureSummary>, LibraryError> {
        self.query(search, kind, include_hidden, false, limit)
    }

    /// Like [`Library::list`], one row per family (heads only), counting variants.
    pub fn families(
        &self,
        search: Option<&str>,
        kind: Option<&str>,
        include_hidden: bool,
        limit: Option<usize>,
    ) -> Result<Vec<FeatureSummary>, LibraryError> {
        self.query(search, kind, include_hidden, true, limit)
    }

    fn query(
        &self,
        search: Option<&str>,
        kind: Option<&str>,
        include_hidden: bool,
        heads_only: bool,
        limit: Option<usize>,
    ) -> Result<Vec<FeatureSummary>, LibraryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT f.id, f.name, f.kind, f.length, f.color, f.status,
                        count(o.id), count(DISTINCT o.source_id), coalesce(f.family_id, f.id), f.grouping,
                        (SELECT count(*) FROM features m WHERE m.family_id = f.id AND m.id != f.id AND m.status != 'hidden'),
                        count(o.id) + (SELECT count(*) FROM occurrences a JOIN features m ON m.id = a.feature_id
                                        WHERE m.family_id = f.id AND m.id != f.id AND m.status != 'hidden') AS total
                   FROM features f LEFT JOIN occurrences o ON o.feature_id = f.id
                  WHERE (?1 OR f.status != 'hidden')
                    AND (?2 IS NULL OR lower(f.kind) = lower(?2))
                    AND (NOT ?4 OR coalesce(f.family_id, f.id) = f.id)
                    AND (?3 IS NULL OR instr(lower(f.name), lower(?3)) > 0
                         OR EXISTS (SELECT 1 FROM occurrences a WHERE a.feature_id = f.id AND instr(lower(a.label), lower(?3)) > 0)
                         OR (?4 AND EXISTS (SELECT 1 FROM features m JOIN occurrences a ON a.feature_id = m.id
                                             WHERE m.family_id = f.id AND instr(lower(a.label), lower(?3)) > 0)))
                  GROUP BY f.id
                  ORDER BY (CASE WHEN ?4 THEN total ELSE count(o.id) END) DESC, f.name, f.id",
            )
            .map_err(|e| self.sql(e))?;
        let rows = statement
            .query_map(params![include_hidden, kind, search, heads_only], |r| {
                Ok(FeatureSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: r.get(2)?,
                    length: r.get::<_, i64>(3)?.unsigned_abs(),
                    color: r.get(4)?,
                    status: r.get(5)?,
                    aliases: Vec::new(),
                    occurrences: r.get::<_, i64>(6)?.unsigned_abs(),
                    sources: r.get::<_, i64>(7)?.unsigned_abs(),
                    family_id: r.get(8)?,
                    grouping: r.get(9)?,
                    variants: r.get::<_, i64>(10)?.unsigned_abs(),
                    family_occurrences: r.get::<_, i64>(11)?.unsigned_abs(),
                })
            })
            .map_err(|e| self.sql(e))?;
        let mut features = Vec::new();
        for row in rows {
            let mut feature = row.map_err(|e| self.sql(e))?;
            feature.aliases = self.aliases(feature.id, &feature.name)?;
            features.push(feature);
            if limit.is_some_and(|limit| features.len() >= limit) {
                break;
            }
        }
        Ok(features)
    }

    fn aliases(&self, id: i64, name: &str) -> Result<Vec<String>, LibraryError> {
        let mut statement = self
            .connection
            .prepare("SELECT label FROM occurrences WHERE feature_id = ?1 AND label != ?2 GROUP BY label ORDER BY count(*) DESC, min(id)")
            .map_err(|e| self.sql(e))?;
        let labels = statement
            .query_map(params![id, name], |r| r.get(0))
            .map_err(|e| self.sql(e))?;
        labels.collect::<Result<_, _>>().map_err(|e| self.sql(e))
    }

    pub fn show(&self, id: i64) -> Result<FeatureDetail, LibraryError> {
        let summary = self
            .list(None, None, true, None)?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or(LibraryError::NoSuchFeature(id))?;
        let (sequence, qualifiers): (String, String) = self
            .connection
            .query_row(
                "SELECT sequence, qualifiers FROM features WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| self.sql(e))?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT s.path, s.record_name, o.label, o.kind, o.start, o.parts, o.sequence != f.sequence
                   FROM occurrences o JOIN sources s ON s.id = o.source_id JOIN features f ON f.id = o.feature_id
                  WHERE o.feature_id = ?1 ORDER BY s.path, o.start",
            )
            .map_err(|e| self.sql(e))?;
        let seen_in = statement
            .query_map([id], |r| {
                Ok(OccurrenceView {
                    source_path: r.get(0)?,
                    record_name: r.get(1)?,
                    label: r.get(2)?,
                    kind: r.get(3)?,
                    start: r.get::<_, i64>(4)?.unsigned_abs(),
                    parts: r.get::<_, i64>(5)?.unsigned_abs(),
                    reversed: r.get(6)?,
                })
            })
            .map_err(|e| self.sql(e))?
            .collect::<Result<_, _>>()
            .map_err(|e| self.sql(e))?;
        let mut family: Vec<FeatureSummary> = self
            .list(None, None, false, None)?
            .into_iter()
            .filter(|f| f.family_id == summary.family_id && f.id != id)
            .collect();
        family.sort_by(|a, b| {
            (a.id != a.family_id)
                .cmp(&(b.id != b.family_id))
                .then(b.length.cmp(&a.length))
                .then(a.id.cmp(&b.id))
        });
        Ok(FeatureDetail {
            summary,
            sequence,
            qualifiers: serde_json::from_str(&qualifiers)?,
            seen_in,
            family,
        })
    }

    /// Every feature that is not hidden, for detection.
    pub fn entries(&self) -> Result<Vec<Entry>, LibraryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name, kind, color, sequence, stranded, qualifiers, coalesce(family_id, id) FROM features WHERE status != 'hidden' ORDER BY id")
            .map_err(|e| self.sql(e))?;
        let rows = statement
            .query_map([], |r| {
                Ok(Entry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: r.get(2)?,
                    color: r.get(3)?,
                    sequence: r.get(4)?,
                    stranded: r.get(5)?,
                    family_id: r.get(7)?,
                    qualifiers: serde_json::from_str(&r.get::<_, String>(6)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })
            .map_err(|e| self.sql(e))?;
        rows.collect::<Result<_, _>>().map_err(|e| self.sql(e))
    }

    /// Rename, retype, hide or unhide a feature. Returns the updated feature.
    pub fn edit(&mut self, id: i64, edit: &Edit) -> Result<FeatureDetail, LibraryError> {
        let status: Option<String> = self
            .connection
            .query_row("SELECT status FROM features WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| self.sql(e))?;
        let Some(status) = status else {
            return Err(LibraryError::NoSuchFeature(id));
        };
        let status = match edit.hidden {
            Some(true) => "hidden",
            None if status == "hidden" => "hidden",
            Some(false) | None => "curated",
        };
        let grouping = edit
            .standalone
            .map(|s| if s { "standalone" } else { "auto" });
        self.connection
            .execute(
                "UPDATE features SET name = coalesce(?2, name), kind = coalesce(?3, kind), status = ?4, grouping = coalesce(?5, grouping) WHERE id = ?1",
                params![id, edit.name, edit.kind, status, grouping],
            )
            .map_err(|e| self.sql(e))?;
        if edit.hidden.is_some() || edit.standalone.is_some() {
            self.connection
                .execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES ('families', 'stale')",
                    [],
                )
                .map_err(|e| self.sql(e))?;
        }
        self.show(id)
    }

    /// Whether families must be recomputed (after imports, hiding or grouping changes).
    pub fn families_stale(&self) -> Result<bool, LibraryError> {
        let value: Option<String> = self
            .connection
            .query_row("SELECT value FROM meta WHERE key = 'families'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| self.sql(e))?;
        Ok(value.as_deref() != Some("current"))
    }

    /// Visible features with their sequences, for computing families.
    pub fn family_inputs(&self) -> Result<Vec<FamilyInput>, LibraryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, sequence, grouping = 'standalone' FROM features WHERE status != 'hidden' ORDER BY id")
            .map_err(|e| self.sql(e))?;
        let rows = statement
            .query_map([], |r| {
                Ok(FamilyInput {
                    id: r.get(0)?,
                    sequence: r.get(1)?,
                    standalone: r.get(2)?,
                })
            })
            .map_err(|e| self.sql(e))?;
        rows.collect::<Result<_, _>>().map_err(|e| self.sql(e))
    }

    /// Store computed families as (feature, head) pairs; features not listed are their own head.
    pub fn set_families(&mut self, heads: &[(i64, i64)]) -> Result<(), LibraryError> {
        let error_path = self.path.display().to_string();
        let sql = |source| LibraryError::Sqlite {
            path: error_path.clone(),
            source,
        };
        let tx = self.connection.transaction().map_err(sql)?;
        tx.execute("UPDATE features SET family_id = id", [])
            .map_err(sql)?;
        for (id, head) in heads {
            tx.execute(
                "UPDATE features SET family_id = ?2 WHERE id = ?1",
                params![id, head],
            )
            .map_err(sql)?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('families', 'current')",
            [],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)
    }
}

/// v1 → v2: variant families. Families are recomputed by the app layer (`families_stale`).
const MIGRATE_1_TO_2: &str = "
ALTER TABLE features ADD COLUMN family_id INTEGER;
ALTER TABLE features ADD COLUMN grouping TEXT NOT NULL DEFAULT 'auto' CHECK (grouping IN ('auto', 'standalone'));
UPDATE meta SET value = '2' WHERE key = 'schema_version';
INSERT OR REPLACE INTO meta (key, value) VALUES ('families', 'stale');
";

/// SQLite integers are signed 64-bit.
fn int(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Re-derive each imported (not curated) feature's name, type, colour, qualifiers and
/// orientation from its occurrences, and drop imported features no file still carries.
fn refresh(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM features WHERE status = 'imported' AND NOT EXISTS (SELECT 1 FROM occurrences o WHERE o.feature_id = features.id)", [])?;
    // Most common label (ties: first seen); the representative occurrence is the first with it.
    tx.execute_batch(
        "UPDATE features SET
            name = (SELECT label FROM occurrences o WHERE o.feature_id = features.id GROUP BY label ORDER BY count(*) DESC, min(id) LIMIT 1),
            kind = (SELECT kind FROM occurrences o WHERE o.feature_id = features.id GROUP BY kind ORDER BY count(*) DESC, min(id) LIMIT 1),
            color = (SELECT color FROM occurrences o WHERE o.feature_id = features.id AND color IS NOT NULL GROUP BY color ORDER BY count(*) DESC, min(id) LIMIT 1)
          WHERE status = 'imported';
         UPDATE features SET
            stranded = (SELECT 2 * sum(stranded) >= count(*) FROM occurrences o WHERE o.feature_id = features.id)
          WHERE EXISTS (SELECT 1 FROM occurrences o WHERE o.feature_id = features.id);
         UPDATE features SET
            sequence = (SELECT sequence FROM occurrences o WHERE o.feature_id = features.id AND o.label = features.name ORDER BY id LIMIT 1),
            qualifiers = (SELECT qualifiers FROM occurrences o WHERE o.feature_id = features.id AND o.label = features.name ORDER BY id LIMIT 1)
          WHERE status = 'imported';",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(label: &str, sequence: &str, canonical: &str) -> Candidate {
        Candidate {
            label: label.into(),
            kind: "CDS".into(),
            color: Some("#ff0000".into()),
            qualifiers: vec![Qualifier {
                key: "note".into(),
                value: Some(label.into()),
            }],
            sequence: sequence.into(),
            canonical: canonical.into(),
            stranded: true,
            start: 0,
            parts: 1,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dnagent-library-{name}-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one import history, asserted step by step
    fn dedup_names_aliases_and_idempotent_reimport() {
        let path = temp_path("dedup");
        let mut library = Library::open_or_create(&path).unwrap();
        // Same feature on opposite strands in two files, under two names; then a third file.
        let a = library
            .import_source(
                "a.dna",
                "h1",
                "a",
                1,
                &[candidate("KanR", "AACCG", "AACCG")],
                false,
            )
            .unwrap();
        assert_eq!(
            a,
            SourceOutcome::Imported {
                occurrences: 1,
                new_features: 1,
                replaced: false
            }
        );
        library
            .import_source(
                "b.dna",
                "h2",
                "b",
                2,
                &[candidate("NeoR/KanR", "CGGTT", "AACCG")],
                false,
            )
            .unwrap();
        library
            .import_source(
                "c.dna",
                "h3",
                "c",
                3,
                &[candidate("NeoR/KanR", "CGGTT", "AACCG")],
                false,
            )
            .unwrap();
        let features = library.list(None, None, false, None).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].name, "NeoR/KanR", "most common label wins");
        assert_eq!(features[0].aliases, vec!["KanR"]);
        assert_eq!((features[0].occurrences, features[0].sources), (3, 3));
        let detail = library.show(features[0].id).unwrap();
        assert_eq!(
            detail.sequence, "CGGTT",
            "orientation of the representative occurrence"
        );
        assert_eq!(detail.seen_in.iter().filter(|o| o.reversed).count(), 1);
        // Re-import: unchanged; copy elsewhere: duplicate; changed content: replaced.
        assert_eq!(
            library
                .import_source("a.dna", "h1", "a", 4, &[], false)
                .unwrap(),
            SourceOutcome::Unchanged
        );
        assert_eq!(
            library
                .import_source("copy.dna", "h1", "a", 4, &[], false)
                .unwrap(),
            SourceOutcome::Duplicate { of: "a.dna".into() }
        );
        let replaced = library
            .import_source(
                "a.dna",
                "h4",
                "a",
                5,
                &[candidate("PuroR", "GGGGA", "GGGGA")],
                false,
            )
            .unwrap();
        assert_eq!(
            replaced,
            SourceOutcome::Imported {
                occurrences: 1,
                new_features: 1,
                replaced: true
            }
        );
        let names: Vec<_> = library
            .list(None, None, false, None)
            .unwrap()
            .into_iter()
            .map(|f| (f.name, f.occurrences))
            .collect();
        assert_eq!(names, vec![("NeoR/KanR".into(), 2), ("PuroR".into(), 1)]);
        assert_eq!(
            library.list(Some("kanr"), None, false, None).unwrap().len(),
            1
        );
        assert_eq!(
            library
                .list(Some("puro"), Some("cds"), false, None)
                .unwrap()
                .len(),
            1
        );
        let info = library.info().unwrap();
        assert_eq!((info.features, info.sources, info.occurrences), (2, 3, 3));
        // Curated names survive later imports; hidden features are not detected.
        let kan = library.list(Some("kanr"), None, false, None).unwrap()[0].id;
        library
            .edit(
                kan,
                &Edit {
                    name: Some("KanR (curated)".into()),
                    ..Edit::default()
                },
            )
            .unwrap();
        library
            .import_source(
                "d.dna",
                "h5",
                "d",
                6,
                &[candidate("NeoR/KanR", "AACCG", "AACCG")],
                false,
            )
            .unwrap();
        assert_eq!(library.show(kan).unwrap().summary.name, "KanR (curated)");
        assert_eq!(library.show(kan).unwrap().summary.status, "curated");
        library
            .edit(
                kan,
                &Edit {
                    hidden: Some(true),
                    ..Edit::default()
                },
            )
            .unwrap();
        assert!(library.entries().unwrap().iter().all(|e| e.id != kan));
        assert!(
            library
                .list(None, None, false, None)
                .unwrap()
                .iter()
                .all(|f| f.id != kan)
        );
        assert_eq!(library.list(None, None, true, None).unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_and_foreign_files_are_errors() {
        let path = temp_path("missing");
        assert!(matches!(
            Library::open(&path),
            Err(LibraryError::Missing(_))
        ));
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE other (x);")
            .unwrap();
        assert!(matches!(
            Library::open(&path),
            Err(LibraryError::Version { .. })
        ));
        let _ = std::fs::remove_file(&path);
    }
}
