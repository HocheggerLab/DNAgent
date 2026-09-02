use clap::{Parser, Subcommand, ValueEnum};
use dnagent_app::{InspectView, feature_views, open_path, sequence_range};
use dnagent_render::MapScene;
use serde::Serialize;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "dnagent",
    version,
    about = "Agent-friendly DNA design and cloning workbench"
)]
struct Cli {
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
    /// Print a zero-based, half-open sequence range.
    Sequence {
        input: PathBuf,
        #[arg(long, value_parser = parse_range)]
        range: Option<(usize, usize)>,
        #[arg(long, value_enum, default_value_t = OutputMode::Text)]
        output: OutputMode,
    },
    /// Render a deterministic SVG map.
    Map {
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Open the desktop viewer.
    Gui { input: Option<PathBuf> },
}

impl Command {
    const fn name(&self) -> &'static str {
        match self {
            Self::Inspect { .. } => "inspect",
            Self::Features { .. } => "features",
            Self::Sequence { .. } => "sequence",
            Self::Map { .. } => "map",
            Self::Gui { .. } => "gui",
        }
    }

    const fn requests_json(&self) -> bool {
        match self {
            Self::Inspect { output, .. }
            | Self::Features { output, .. }
            | Self::Sequence { output, .. } => matches!(output, OutputMode::Json),
            Self::Map { .. } => true,
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
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if requests_json {
                let message = error.to_string();
                let envelope = ErrorEnvelope {
                    schema_version: "0.1.0",
                    command,
                    ok: false,
                    error: ErrorBody {
                        code: "command_failed",
                        message: &message,
                    },
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

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Inspect { input, output } => {
            let report = open_path(&input)?;
            let view = InspectView::from_report(&report);
            match output {
                OutputMode::Json => print_json("inspect", &view)?,
                OutputMode::Text => {
                    println!("{}", view.name);
                    println!("length: {} bp", view.length);
                    println!("topology: {:?}", view.topology);
                    println!("features: {}", view.feature_count);
                    println!("primers: {}", view.primer_count);
                    for warning in view.warnings {
                        println!("warning [{}]: {}", warning.code, warning.message);
                    }
                }
            }
        }
        Command::Features { input, output } => {
            let report = open_path(&input)?;
            let features = feature_views(&report.record);
            match output {
                OutputMode::Json => print_json("features", &features)?,
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
        Command::Sequence {
            input,
            range,
            output,
        } => {
            let report = open_path(&input)?;
            let (start, end) = range.unwrap_or((0, report.record.sequence().len()));
            let view = sequence_range(&report.record, start, end)?;
            match output {
                OutputMode::Json => print_json("sequence", &view)?,
                OutputMode::Text => println!("{}", view.sequence),
            }
        }
        Command::Map { input, out } => {
            let report = open_path(&input)?;
            let svg = MapScene::from_record(&report.record).to_svg();
            std::fs::write(&out, svg)?;
            print_json(
                "map",
                &MapResult {
                    output_path: out.display().to_string(),
                },
            )?;
        }
        Command::Gui { input } => dnagent_gui::run(input.as_deref())?,
    }
    Ok(())
}

fn print_json<T: Serialize>(command: &'static str, result: &T) -> Result<(), serde_json::Error> {
    let envelope = Envelope {
        schema_version: "0.1.0",
        command,
        ok: true,
        result,
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
