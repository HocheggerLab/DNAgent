//! `dnagent library …` and `dnagent detect-features`.
use crate::{OutputMode, load_input, print_json};
use clap::{Args, Subcommand};
use dnagent_app::library::{
    DEFAULT_MIN_LENGTH, Detection, FileOutcome, LibraryImport, detect_features,
    import_into_library, library_path,
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
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Library location and counts.
    Info {
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// List features, most often seen first.
    List {
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
            output,
        } => {
            let mut library = Library::open_or_create(&path)?;
            let report = import_into_library(&mut library, &inputs, min_length)?;
            match output {
                OutputMode::Json => print_json(name, &report, warnings)?,
                OutputMode::Text => print_import_text(&report)?,
            }
        }
        LibraryCommand::Info { output } => {
            let info = Library::open(&path)?.info()?;
            match output {
                OutputMode::Json => print_json(name, &info, warnings)?,
                OutputMode::Text => println!(
                    "{}\nschema {}\n{} features ({} curated, {} hidden) from {} files, {} occurrences",
                    info.path,
                    info.schema_version,
                    info.features,
                    info.curated,
                    info.hidden,
                    info.sources,
                    info.occurrences
                ),
            }
        }
        LibraryCommand::List {
            search,
            kind,
            include_hidden,
            limit,
            output,
        } => list(
            &path,
            name,
            search.as_deref(),
            kind.as_deref(),
            include_hidden,
            limit,
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
            Some(&text),
            None,
            false,
            limit,
            output,
            warnings,
        )?,
        LibraryCommand::Edit {
            id,
            name: new_name,
            kind,
            hide,
            unhide,
            output,
        } => {
            let hidden = (hide || unhide).then_some(hide);
            let detail = Library::open(&path)?.edit(
                id,
                &dnagent_library::Edit {
                    name: new_name,
                    kind,
                    hidden,
                },
            )?;
            match output {
                OutputMode::Json => print_json(name, &detail, warnings)?,
                OutputMode::Text => println!(
                    "{}\t{}\t{}\t{}",
                    detail.summary.id,
                    detail.summary.name,
                    detail.summary.kind,
                    detail.summary.status
                ),
            }
        }
        LibraryCommand::Show { id, output } => {
            let detail = Library::open(&path)?.show(id)?;
            match output {
                OutputMode::Json => print_json(name, &detail, warnings)?,
                OutputMode::Text => print_show_text(&detail),
            }
        }
    }
    Ok(())
}

fn print_show_text(detail: &dnagent_library::FeatureDetail) {
    let s = &detail.summary;
    println!("{}\t{}\t{} bp\t{}", s.id, s.name, s.length, s.kind);
    if !s.aliases.is_empty() {
        println!("also: {}", s.aliases.join(", "));
    }
    println!("{}", detail.sequence);
    for seen in &detail.seen_in {
        let direction = if seen.reversed { "reverse" } else { "forward" };
        println!("{}\t{}\t{direction}", seen.source_path, seen.label);
    }
}

#[allow(clippy::too_many_arguments)] // one call per list-style subcommand
fn list(
    path: &std::path::Path,
    name: &'static str,
    search: Option<&str>,
    kind: Option<&str>,
    include_hidden: bool,
    limit: Option<usize>,
    output: OutputMode,
    warnings: &[ImportWarning],
) -> Result<(), Box<dyn std::error::Error>> {
    let features = Library::open(path)?.list(search, kind, include_hidden, limit)?;
    match output {
        OutputMode::Json => print_json(name, &features, warnings)?,
        OutputMode::Text => {
            println!("id\tname\tkind\tlength\tseen\taliases");
            for f in features {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    f.id,
                    f.name,
                    f.kind,
                    f.length,
                    f.occurrences,
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
    let library = Library::open(&args.db.unwrap_or_else(library_path))?;
    let report = load_input(&args.input, strict, json, warnings)?;
    let mut matches = detect_features(&report.record, &library.entries()?, args.min_length)?;
    if args.new_only {
        matches.retain(|m| m.annotated_as.is_empty());
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
            println!("start\tlength\tstrand\tname\tkind\tannotated");
            for m in &detection.matches {
                println!(
                    "{}\t{}\t{:?}\t{}\t{}\t{}",
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
