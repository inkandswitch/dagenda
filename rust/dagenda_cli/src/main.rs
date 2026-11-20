//! `dagenda`: check and render roadmaps written in the dagenda notation.
//!
//! ```console
//! $ dagenda check ROADMAP.dagenda
//! $ dagenda check public.dagenda private.dagenda --deny-warnings
//! $ dagenda render ROADMAP.dagenda                 # text to stdout
//! $ dagenda render ROADMAP.dagenda -o roadmap.svg  # format from the extension
//! ```
//!
//! Files given together are one plan, so a private plan can `extend` a task
//! in a public one, or say it `needs` one. Problems are printed as
//! `path:line:column: severity: message`. Exit status: 0 on success, 1 if
//! there are errors (or warnings, with `--deny-warnings`) or a file can't be
//! read or written, 2 for bad arguments.

use clap::{Parser, Subcommand};
use dagenda_cli::{
    check::{Problem, Severity, Source, check},
    render::Format,
};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use thiserror::Error;

/// Check and render roadmaps written in the dagenda notation.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check documents as one plan, printing every error and warning
    Check {
        /// The documents (`-` for stdin)
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Fail on warnings too
        #[arg(long)]
        deny_warnings: bool,
    },

    /// Render documents as one plan: as text, or its tech tree as SVG
    Render {
        /// The documents (`-` for stdin)
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// The output format (default: from the output file's extension,
        /// else text)
        #[arg(short, long, value_enum)]
        format: Option<Format>,

        /// Where to write it (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("dagenda: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, CliError> {
    match cli.command {
        Command::Check {
            inputs,
            deny_warnings,
        } => {
            let (_, problems) = check(&read_all(&inputs)?);
            report(&problems);

            let failed = match problems.iter().map(|problem| problem.severity).max() {
                Some(Severity::Error) => true,
                Some(Severity::Warning) => deny_warnings,
                None => false,
            };
            Ok(if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }

        Command::Render {
            inputs,
            format,
            output,
        } => {
            let (plan, problems) = check(&read_all(&inputs)?);
            report(&problems);
            let Some(plan) = plan else {
                return Ok(ExitCode::FAILURE);
            };

            let format = format
                .or_else(|| output.as_deref().and_then(Format::for_path))
                .unwrap_or_default();
            let rendered = format.render(&plan);
            match output {
                Some(path) => std::fs::write(&path, rendered)
                    .map_err(|source| CliError::Write { path, source })?,
                None => io::stdout()
                    .write_all(rendered.as_bytes())
                    .map_err(|source| CliError::Write {
                        path: "<stdout>".into(),
                        source,
                    })?,
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Print each problem to stderr. If stderr goes away (piped into `head`, say),
/// stop quietly rather than panic, as `eprintln!` would.
fn report(problems: &[Problem]) {
    let mut stderr = io::stderr().lock();
    for problem in problems {
        if writeln!(stderr, "{problem}").is_err() {
            break;
        }
    }
}

fn read_all(inputs: &[PathBuf]) -> Result<Vec<Source>, CliError> {
    inputs.iter().map(|input| read(input)).collect()
}

fn read(input: &Path) -> Result<Source, CliError> {
    if input == Path::new("-") {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|source| CliError::Read {
                path: "<stdin>".into(),
                source,
            })?;
        return Ok(Source {
            name: "<stdin>".into(),
            text,
        });
    }

    let text = std::fs::read_to_string(input).map_err(|source| CliError::Read {
        path: input.to_owned(),
        source,
    })?;
    Ok(Source {
        name: input.display().to_string(),
        text,
    })
}

#[derive(Debug, Error)]
enum CliError {
    #[error("can't read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("can't write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
}
