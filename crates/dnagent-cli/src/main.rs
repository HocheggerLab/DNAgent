use clap::{Args, Parser, Subcommand, ValueEnum};
use dnagent_app::{
    AppError, InspectView, end_compatibility, feature_views, open_path, primer_views,
    require_warning_free_import, restriction_sites, sequence_range, simulate_digest,
};
use dnagent_domain::restriction::ENZYMES;
use dnagent_formats::{ImportReport, ImportWarning};
use dnagent_render::MapScene;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SCHEMA_VERSION: &str = "0.5.0";

#[derive(Debug, Parser)]
#[command(
    name = "dnagent",
    version,
    about = "Agent-friendly DNA design and cloning workbench"
)]
struct Cli {
    /// Reject any import warning, including preserved uninterpreted metadata (CLI only).
    #[arg(long, global = true)]
    strict: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect sequence metadata and import fidelity.
    Inspect {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// List imported annotations.
    Features {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// List retained primers (not predicted binding sites).
    Primers {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Print a zero-based, half-open sequence range.
    Sequence {
        input: PathBuf,
        #[arg(long, value_parser = parse_range)]
        range: Option<(usize, usize)>,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// List the supported restriction enzymes and cleavage offsets.
    Enzymes {
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Find restriction recognition sites and nominal cut positions (not a digest).
    Sites {
        input: PathBuf,
        #[arg(long, value_delimiter = ',', required = true)]
        enzymes: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Simulate complete restriction cleavage with both strand sequences and fragment ends.
    Digest {
        input: PathBuf,
        #[arg(long, value_delimiter = ',', required = true)]
        enzymes: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Compare all distinct fragment ends from one or two complete digests.
    CompatibleEnds(CompatibilityArgs),
    /// Render a deterministic SVG map.
    Map {
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Open the desktop viewer (requires the gui build feature).
    #[cfg(feature = "gui")]
    Gui { input: Option<PathBuf> },
}

#[derive(Debug, Args)]
struct CompatibilityArgs {
    input: PathBuf,
    #[arg(long, value_delimiter = ',', required = true)]
    enzymes: Vec<String>,
    #[arg(long, requires = "other_enzymes")]
    other: Option<PathBuf>,
    #[arg(long, value_delimiter = ',', requires = "other")]
    other_enzymes: Vec<String>,
    #[arg(long, value_enum, default_value_t = OutputMode::Text)]
    output: OutputMode,
}

impl Command {
    const fn name(&self) -> &'static str {
        match self {
            Self::Inspect { .. } => "inspect",
            Self::Features { .. } => "features",
            Self::Primers { .. } => "primers",
            Self::Sequence { .. } => "sequence",
            Self::Map { .. } => "map",
            Self::Enzymes { .. } => "enzymes",
            Self::Sites { .. } => "sites",
            Self::Digest { .. } => "digest",
            Self::CompatibleEnds(_) => "compatible-ends",
            #[cfg(feature = "gui")]
            Self::Gui { .. } => "gui",
        }
    }

    const fn requests_json(&self) -> bool {
        match self {
            Self::Inspect { output, .. }
            | Self::Features { output, .. }
            | Self::Primers { output, .. }
            | Self::Enzymes { output, .. }
            | Self::Sites { output, .. }
            | Self::Digest { output, .. }
            | Self::Sequence { output, .. } => matches!(output, OutputMode::Json),
            Self::CompatibleEnds(args) => matches!(args.output, OutputMode::Json),
            Self::Map { .. } => true,
            #[cfg(feature = "gui")]
            Self::Gui { .. } => false,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputMode {
    Text,
    Json,
}

#[derive(Debug, Serialize)]
struct Envelope<T> {
    schema_version: &'static str,
    command: &'static str,
    ok: bool,
    result: T,
    warnings: Vec<ImportWarning>,
}

#[derive(Debug, Serialize)]
struct MapResult {
    output_path: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope<'a> {
    schema_version: &'static str,
    command: &'static str,
    ok: bool,
    error: ErrorBody<'a>,
    warnings: Vec<ImportWarning>,
}

#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    code: &'static str,
    message: &'a str,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command = cli.command.name();
    let requests_json = cli.command.requests_json();
    let mut warnings = Vec::new();
    match run(cli, &mut warnings) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if requests_json {
                let message = error.to_string();
                let envelope = ErrorEnvelope {
                    schema_version: SCHEMA_VERSION,
                    command,
                    ok: false,
                    error: ErrorBody {
                        code: if matches!(
                            error.downcast_ref::<AppError>(),
                            Some(AppError::ImportWarnings { .. })
                        ) {
                            "import_warnings"
                        } else if matches!(
                            error.downcast_ref::<AppError>(),
                            Some(AppError::Restriction(_))
                        ) {
                            "restriction_scan_failed"
                        } else if matches!(
                            error.downcast_ref::<AppError>(),
                            Some(AppError::Digest(_))
                        ) {
                            "digest_failed"
                        } else if matches!(
                            error.downcast_ref::<AppError>(),
                            Some(AppError::Compatibility(_))
                        ) {
                            "compatibility_failed"
                        } else {
                            "command_failed"
                        },
                        message: &message,
                    },
                    warnings,
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&envelope)
                        .expect("the static error envelope must serialize")
                );
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli, warnings: &mut Vec<ImportWarning>) -> Result<(), Box<dyn std::error::Error>> {
    let strict = cli.strict;
    #[cfg(feature = "gui")]
    if strict && matches!(cli.command, Command::Gui { .. }) {
        return Err("--strict is supported only for CLI import commands, not gui".into());
    }
    let requests_json = cli.command.requests_json();
    match cli.command {
        Command::Inspect { input, output } => {
            let report = load_input(&input, strict, requests_json, warnings)?;
            let view = InspectView::from_report(&report);
            match output {
                OutputMode::Json => print_json("inspect", &view, warnings)?,
                OutputMode::Text => {
                    println!("{}", view.name);
                    println!("length: {} bp", view.length);
                    println!("topology: {:?}", view.topology);
                    println!("features: {}", view.feature_count);
                    println!("primers: {}", view.primer_count);
                }
            }
        }
        Command::Features { input, output } => {
            let report = load_input(&input, strict, requests_json, warnings)?;
            let features = feature_views(&report.record);
            match output {
                OutputMode::Json => print_json("features", &features, warnings)?,
                OutputMode::Text => {
                    for feature in features {
                        println!(
                            "{}\t{}\t{}\t{:?}\t{:?}",
                            feature.id,
                            feature.label,
                            feature.kind,
                            feature.strand,
                            feature.location.parts()
                        );
                    }
                }
            }
        }
        Command::Primers { input, output } => {
            let report = load_input(&input, strict, requests_json, warnings)?;
            let primers = primer_views(&report.record);
            match output {
                OutputMode::Json => print_json("primers", &primers, warnings)?,
                OutputMode::Text => {
                    for primer in primers {
                        println!(
                            "{}\t{}\t{}",
                            primer.name,
                            primer.sequence,
                            primer.description.as_deref().unwrap_or("")
                        );
                    }
                }
            }
        }
        Command::Sequence {
            input,
            range,
            output,
        } => {
            let report = load_input(&input, strict, requests_json, warnings)?;
            let (start, end) = range.unwrap_or((0, report.record.sequence().len()));
            let view = sequence_range(&report.record, start, end)?;
            match output {
                OutputMode::Json => print_json("sequence", &view, warnings)?,
                OutputMode::Text => println!("{}", view.sequence),
            }
        }
        Command::Enzymes { output } => print_enzymes(output, warnings)?,
        Command::Sites {
            input,
            enzymes,
            output,
        } => run_sites(&input, &enzymes, output, strict, warnings)?,
        Command::Digest {
            input,
            enzymes,
            output,
        } => run_digest(&input, &enzymes, output, strict, warnings)?,
        Command::CompatibleEnds(args) => run_compatibility(&args, strict, warnings)?,
        Command::Map { input, out } => {
            let report = load_input(&input, strict, requests_json, warnings)?;
            let svg = MapScene::from_record(&report.record).to_svg();
            std::fs::write(&out, svg)?;
            print_json(
                "map",
                &MapResult {
                    output_path: out.display().to_string(),
                },
                warnings,
            )?;
        }
        #[cfg(feature = "gui")]
        Command::Gui { input } => dnagent_gui::run(input.as_deref())?,
    }
    Ok(())
}

fn run_compatibility(
    args: &CompatibilityArgs,
    strict: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = matches!(args.output, OutputMode::Json);
    let first = load_input(&args.input, strict, json, warnings)?;
    let second = args
        .other
        .as_ref()
        .map(|path| load_input(path, strict, json, warnings))
        .transpose()?;
    let mut inputs = vec![(&first.record, args.enzymes.as_slice())];
    if let Some(second) = &second {
        inputs.push((&second.record, args.other_enzymes.as_slice()));
    }
    let view = end_compatibility(&inputs)?;
    match args.output {
        OutputMode::Json => print_json("compatible-ends", &view, warnings)?,
        OutputMode::Text => {
            println!("Sequence-compatible ends only; not experimental ligation validation.");
            println!(
                "{} free ends; {} pair comparisons",
                view.analysis.endpoints.len(),
                view.analysis.pairs.len()
            );
            println!("first\tsecond\tcompatible\treason\tsecond_orientation\tsecond_placement");
            for pair in view.analysis.pairs {
                println!(
                    "{}\t{}\t{}\t{:?}\t{:?}\t{:?}",
                    pair.first,
                    pair.second,
                    pair.assessment.compatible,
                    pair.assessment.reason,
                    pair.assessment.second_fragment_orientation,
                    pair.assessment.second_placement
                );
            }
        }
    }
    Ok(())
}

fn run_digest(
    input: &Path,
    enzymes: &[String],
    output: OutputMode,
    strict: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = load_input(input, strict, matches!(output, OutputMode::Json), warnings)?;
    let digest = simulate_digest(&report.record, enzymes)?;
    match output {
        OutputMode::Json => print_json("digest", &digest, warnings)?,
        OutputMode::Text => {
            println!("Complete sequence-only digest; not experimental validation.");
            println!("fragment\ttop_bases\tbottom_bases\tpaired_bases\ttopology");
            for fragment in digest.fragments {
                println!(
                    "{}\t{}\t{}\t{}\t{:?}",
                    fragment.id,
                    fragment.top.length,
                    fragment.bottom.length,
                    fragment.paired_length,
                    fragment.topology
                );
            }
        }
    }
    Ok(())
}

fn print_enzymes(output: OutputMode, warnings: &[ImportWarning]) -> Result<(), serde_json::Error> {
    match output {
        OutputMode::Json => print_json("enzymes", &ENZYMES, warnings)?,
        OutputMode::Text => {
            println!("enzyme\trecognition\ttop_offset\tbottom_offset");
            for enzyme in ENZYMES {
                println!(
                    "{}\t{}\t{}\t{}",
                    enzyme.name,
                    enzyme.recognition_sequence,
                    enzyme.top_cut_offset,
                    enzyme.bottom_cut_offset
                );
            }
        }
    }
    Ok(())
}

fn run_sites(
    input: &Path,
    enzymes: &[String],
    output: OutputMode,
    strict: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), Box<dyn std::error::Error>> {
    let requests_json = matches!(output, OutputMode::Json);
    let report = load_input(input, strict, requests_json, warnings)?;
    let view = restriction_sites(&report.record, enzymes)?;
    if !requests_json {
        for warning in &view.warnings {
            eprintln!("warning [{}]: {}", warning.code, warning.message);
        }
    }
    warnings.extend(view.warnings);
    if strict && !warnings.is_empty() {
        return Err(AppError::ImportWarnings {
            count: warnings.len(),
        }
        .into());
    }
    match output {
        OutputMode::Json => print_json("sites", &view.result, warnings)?,
        OutputMode::Text => {
            println!("Nominal cuts only; not a digest or prediction of experimental cleavage.");
            println!("enzyme\tstart\tstrand\ttop_cut\tbottom_cut\toverhang\tlength");
            for site in view.result.sites {
                println!(
                    "{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{}",
                    site.enzyme,
                    site.recognition.start().get(),
                    site.strand,
                    site.top_cut,
                    site.bottom_cut,
                    site.overhang_polarity,
                    site.overhang_length
                );
            }
        }
    }
    Ok(())
}

fn load_input(
    path: &Path,
    strict: bool,
    requests_json: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<ImportReport, AppError> {
    let report = open_path(path)?;
    warnings.extend(report.warnings.iter().cloned());
    if !requests_json {
        for warning in &report.warnings {
            eprintln!("warning [{}]: {}", warning.code, warning.message);
        }
    }
    if strict {
        require_warning_free_import(&report)?;
    }
    Ok(report)
}

fn print_json<T: Serialize>(
    command: &'static str,
    result: &T,
    warnings: &[ImportWarning],
) -> Result<(), serde_json::Error> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        command,
        ok: true,
        result,
        warnings: warnings.to_vec(),
    };
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    Ok(())
}

fn parse_range(value: &str) -> Result<(usize, usize), String> {
    let (start, end) = value
        .split_once("..")
        .ok_or_else(|| "range must use START..END".to_owned())?;
    let start = start
        .parse::<usize>()
        .map_err(|_| "range start must be a non-negative integer".to_owned())?;
    let end = end
        .parse::<usize>()
        .map_err(|_| "range end must be a non-negative integer".to_owned())?;
    if start > end {
        return Err("range start must not exceed end".to_owned());
    }
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_half_open_range() {
        assert_eq!(parse_range("10..20").unwrap(), (10, 20));
        assert!(parse_range("20..10").is_err());
        assert!(parse_range("10-20").is_err());
    }
}
