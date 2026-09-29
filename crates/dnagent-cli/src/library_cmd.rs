//! `dnagent library …` and `dnagent detect-features`.
use crate::{OutputMode, load_input, print_json};
use clap::{Args, Subcommand};
use dnagent_app::library::{
    DEFAULT_MIN_LENGTH, Detection, FileOutcome, LibraryImport, detect_features,
    import_into_library, library_path, open_library, refresh_families,
};
use dnagent_formats::ImportWarning;
use dnagent_library::Library;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct LibraryArgs {
    #[command(subcommand)]
    pub command: LibraryCommand,
    /// Library file (default: DNAGENT_FEATURE_DB, else ~/Library/Application Support/DNAgent/features.sqlite on macOS).
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum LibraryCommand {
    /// Collect annotated features from sequence files or folders (searched recursively).
    Import {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Skip features shorter than this many bases.
        #[arg(long, default_value_t = DEFAULT_MIN_LENGTH)]
        min_length: usize,
        /// Re-read unchanged files too (after DNAgent's collection rules change); curated edits are kept.
        #[arg(long)]
        rescan: bool,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Library location and counts.
    Info {
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// List variant families (one row each, with a variant count), most often seen first.
    List {
        /// Every feature, not only family heads.
        #[arg(long)]
        all: bool,
        /// Case-insensitive substring of the name or any alias.
        #[arg(long)]
        search: Option<String>,
        /// Exact feature type (case-insensitive), e.g. CDS or promoter.
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        include_hidden: bool,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Shorthand for `list --search TEXT`.
    Search {
        text: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Rename, retype, hide or unhide a feature. Edited features keep your changes on later imports.
    Edit {
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        /// Hide from listings and detection.
        #[arg(long, conflicts_with = "unhide")]
        hide: bool,
        #[arg(long)]
        unhide: bool,
        /// Keep out of variant families (e.g. a homology arm inside an exon).
        #[arg(long, conflicts_with = "grouped")]
        standalone: bool,
        /// Allow grouping into a variant family again (the default).
        #[arg(long)]
        grouped: bool,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// One feature with its sequence, qualifiers and every file it was seen in.
    Show {
        id: i64,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
}

#[derive(Debug, Args)]
pub struct DetectArgs {
    pub input: PathBuf,
    /// Library file (default as for `dnagent library`).
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Ignore library features shorter than this many bases.
    #[arg(long, default_value_t = DEFAULT_MIN_LENGTH)]
    pub min_length: usize,
    /// Only matches not already annotated with exactly that span and strand.
    #[arg(long)]
    pub new_only: bool,
    #[arg(long, value_enum, default_value_t = OutputMode::Text)]
    pub output: OutputMode,
}

impl LibraryArgs {
    pub const fn name(&self) -> &'static str {
        match self.command {
            LibraryCommand::Import { .. } => "library-import",
            LibraryCommand::Info { .. } => "library-info",
            LibraryCommand::List { .. } => "library-list",
            LibraryCommand::Search { .. } => "library-search",
            LibraryCommand::Show { .. } => "library-show",
            LibraryCommand::Edit { .. } => "library-edit",
        }
    }

    pub const fn requests_json(&self) -> bool {
        match self.command {
            LibraryCommand::Import { output, .. }
            | LibraryCommand::Info { output }
            | LibraryCommand::List { output, .. }
            | LibraryCommand::Search { output, .. }
            | LibraryCommand::Show { output, .. }
            | LibraryCommand::Edit { output, .. } => matches!(output, OutputMode::Json),
        }
    }
}

fn print_import_text(report: &LibraryImport) -> Result<(), serde_json::Error> {
    let t = &report.totals;
    println!(
        "{} files: {} imported, {} unchanged, {} duplicate, {} failed",
        t.files, t.imported, t.unchanged, t.duplicate, t.failed
    );
    println!(
        "{} feature occurrences, {} new library features",
        t.occurrences, t.new_features
    );
    for (reason, count) in &t.skipped_features {
        println!(
            "skipped {count} features: {}",
            serde_json::to_value(reason)?.as_str().unwrap_or_default()
        );
    }
    for file in &report.files {
        if let FileOutcome::Failed { reason, .. } = &file.outcome {
            let reason = reason
                .strip_prefix(&file.path)
                .map_or(reason.as_str(), str::trim_start);
            println!("failed\t{}\t{reason}", file.path);
        }
    }
    println!(
        "library: {} ({} features from {} files)",
        report.library.path, report.library.features, report.library.sources
    );
    Ok(())
}

pub fn run_library(
    args: LibraryArgs,
    warnings: &[ImportWarning],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.db.clone().unwrap_or_else(library_path);
    let name = args.name();
    match args.command {
        LibraryCommand::Import {
            inputs,
            min_length,
            rescan,
            output,
        } => {
            let mut library = Library::open_or_create(&path)?;
            let report = import_into_library(&mut library, &inputs, min_length, rescan)?;
            match output {
                OutputMode::Json => print_json(name, &report, warnings)?,
                OutputMode::Text => print_import_text(&report)?,
            }
        }
        LibraryCommand::Info { output } => {
            let info = open_library(&path)?.info()?;
            match output {
                OutputMode::Json => print_json(name, &info, warnings)?,
                OutputMode::Text => print_info_text(&info),
            }
        }
        LibraryCommand::List {
            all,
            search,
            kind,
            include_hidden,
            limit,
            output,
        } => list(
            &path,
            name,
            &Query {
                search: search.as_deref(),
                kind: kind.as_deref(),
                include_hidden,
                all,
                limit,
            },
            output,
            warnings,
        )?,
        LibraryCommand::Search {
            text,
            limit,
            output,
        } => list(
            &path,
            name,
            &Query {
                search: Some(&text),
                kind: None,
                include_hidden: false,
                all: false,
                limit,
            },
            output,
            warnings,
        )?,
        LibraryCommand::Edit {
            id,
            name: new_name,
            kind,
            hide,
            unhide,
            standalone,
            grouped,
            output,
        } => {
            let edit = dnagent_library::Edit {
                name: new_name,
                kind,
                hidden: (hide || unhide).then_some(hide),
                standalone: (standalone || grouped).then_some(standalone),
            };
            let detail = apply_edit(&path, id, &edit)?;
            match output {
                OutputMode::Json => print_json(name, &detail, warnings)?,
                OutputMode::Text => {
                    let s = &detail.summary;
                    println!(
                        "{}\t{}\t{}\t{}\t{}",
                        s.id, s.name, s.kind, s.status, s.grouping
                    );
                }
            }
        }
        LibraryCommand::Show { id, output } => {
            let detail = open_library(&path)?.show(id)?;
            match output {
                OutputMode::Json => print_json(name, &detail, warnings)?,
                OutputMode::Text => print_show_text(&detail),
            }
        }
    }
    Ok(())
}

fn print_info_text(info: &dnagent_library::LibraryInfo) {
    println!(
        "{}\nschema {}\n{} features in {} families ({} curated, {} hidden) from {} files, {} occurrences",
        info.path,
        info.schema_version,
        info.features,
        info.families,
        info.curated,
        info.hidden,
        info.sources,
        info.occurrences
    );
}

fn apply_edit(
    path: &std::path::Path,
    id: i64,
    edit: &dnagent_library::Edit,
) -> Result<dnagent_library::FeatureDetail, Box<dyn std::error::Error>> {
    let mut library = open_library(path)?;
    library.edit(id, edit)?;
    refresh_families(&mut library)?;
    Ok(library.show(id)?)
}

fn print_show_text(detail: &dnagent_library::FeatureDetail) {
    let s = &detail.summary;
    println!("{}\t{}\t{} bp\t{}", s.id, s.name, s.length, s.kind);
    if !s.aliases.is_empty() {
        println!("also: {}", s.aliases.join(", "));
    }
    println!("{}", detail.sequence);
    for member in &detail.family {
        let role = match (&member.relation, member.identity) {
            (Some(relation), Some(identity)) => {
                format!("variant ({relation}, {:.1} %)", identity * 100.0)
            }
            _ => "family head".to_owned(),
        };
        println!(
            "{role}\t{}\t{}\t{} bp",
            member.id, member.name, member.length
        );
    }
    for seen in &detail.seen_in {
        let direction = if seen.reversed { "reverse" } else { "forward" };
        println!("{}\t{}\t{direction}", seen.source_path, seen.label);
    }
}

struct Query<'a> {
    search: Option<&'a str>,
    kind: Option<&'a str>,
    include_hidden: bool,
    all: bool,
    limit: Option<usize>,
}

fn list(
    path: &std::path::Path,
    name: &'static str,
    query: &Query<'_>,
    output: OutputMode,
    warnings: &[ImportWarning],
) -> Result<(), Box<dyn std::error::Error>> {
    let library = open_library(path)?;
    let features = if query.all {
        library.list(query.search, query.kind, query.include_hidden, query.limit)?
    } else {
        library.families(query.search, query.kind, query.include_hidden, query.limit)?
    };
    match output {
        OutputMode::Json => print_json(name, &features, warnings)?,
        OutputMode::Text => {
            println!("id\tname\tkind\tlength\tseen\tvariants\taliases");
            for f in features {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    f.id,
                    f.name,
                    f.kind,
                    f.length,
                    if query.all {
                        f.occurrences
                    } else {
                        f.family_occurrences
                    },
                    f.variants,
                    f.aliases.join(", ")
                );
            }
        }
    }
    Ok(())
}

pub fn run_detect(
    args: DetectArgs,
    strict: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = matches!(args.output, OutputMode::Json);
    let library = open_library(&args.db.unwrap_or_else(library_path))?;
    let report = load_input(&args.input, strict, json, warnings)?;
    let mut matches = detect_features(&report.record, &library.entries()?, args.min_length)?;
    if args.new_only {
        // Keep `superseded_by` pointing at the same match after filtering.
        let kept: Vec<Option<usize>> = matches
            .iter()
            .scan(0, |next, m| {
                Some(m.annotated_as.is_empty().then(|| {
                    *next += 1;
                    *next - 1
                }))
            })
            .collect();
        matches.retain(|m| m.annotated_as.is_empty());
        for m in &mut matches {
            m.superseded_by = m.superseded_by.and_then(|i| kept[i]);
        }
    }
    let detection = Detection {
        library: library.info()?,
        length: report.record.sequence().len(),
        topology: report.record.topology(),
        min_length: args.min_length,
        matches,
    };
    match args.output {
        OutputMode::Json => print_json("detect-features", &detection, warnings)?,
        OutputMode::Text => {
            println!("start\tlength\tstrand\tname\tkind\tannotated\tvariant of");
            for m in &detection.matches {
                let variant_of = m
                    .superseded_by
                    .map(|i| {
                        format!(
                            "{} ({} bp)",
                            detection.matches[i].name, detection.matches[i].length
                        )
                    })
                    .unwrap_or_default();
                println!(
                    "{}\t{}\t{:?}\t{}\t{}\t{}\t{variant_of}",
                    m.location.parts()[0].start().get(),
                    m.length,
                    m.strand,
                    m.name,
                    m.kind,
                    m.annotated_as.join(",")
                );
            }
        }
    }
    Ok(())
}
