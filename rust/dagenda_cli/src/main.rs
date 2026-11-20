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
//! `path:line:column: severity: message`.
//!
//! `render -o` takes the format from the output's extension (`.svg` or
//! `.txt`, in any case), or from `--format`; any other extension is refused
//! rather than guessed at. It never writes over one of the plan's own
//! documents, however the path is spelled.
//!
//! Exit status: 0 on success, 1 if there are errors (or warnings, with
//! `--deny-warnings`) or a file can't be read or written, 2 for bad
//! arguments (including an output it won't write).

use clap::{Parser, Subcommand};
use dagenda_cli::render::Format;
use dagenda_core::check::{Problem, Severity, Source, check};
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

        /// The output format (default: from the output's extension, `.svg`
        /// or `.txt`; text on stdout)
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
            e.exit_code()
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
            // Settle where it goes before reading anything, so a bad output
            // is refused whatever state the plan is in. Writing over one of
            // the plan's own documents is the worse mistake, so it's named
            // first.
            if let Some(input) = output
                .as_deref()
                .and_then(|output| overwritten(output, &inputs))
            {
                return Err(CliError::Overwrite {
                    path: input.clone(),
                });
            }
            let format = match (format, output.as_deref()) {
                (Some(format), _) => format,
                (None, None) => Format::Text,
                (None, Some(path)) => {
                    Format::for_path(path).ok_or_else(|| CliError::UnknownFormat {
                        path: path.to_owned(),
                    })?
                }
            };

            let (plan, problems) = check(&read_all(&inputs)?);
            report(&problems);
            let Some(plan) = plan else {
                return Ok(ExitCode::FAILURE);
            };
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

/// The input `output` would write over, if any: the same file, however it's
/// spelled (`./plan.dagenda`, through a symlink). An output that doesn't
/// exist yet can't be one, since every input has to.
fn overwritten<'a>(output: &Path, inputs: &'a [PathBuf]) -> Option<&'a PathBuf> {
    let output = std::fs::canonicalize(output).ok()?;
    inputs
        .iter()
        .filter(|input| input.as_path() != Path::new("-"))
        .find(|input| std::fs::canonicalize(input).is_ok_and(|input| input == output))
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

    #[error(
        "can't tell what to write to {} from its name; end it in .svg or .txt, or pass --format svg or --format text",
        path.display()
    )]
    UnknownFormat { path: PathBuf },

    #[error(
        "won't write over {}, which is one of the plan's documents; pick another --output",
        path.display()
    )]
    Overwrite { path: PathBuf },
}

impl CliError {
    /// 2 for an argument it won't act on, like clap's own; 1 for a file
    /// that couldn't be read or written.
    fn exit_code(&self) -> ExitCode {
        match self {
            Self::Read { .. } | Self::Write { .. } => ExitCode::FAILURE,
            Self::UnknownFormat { .. } | Self::Overwrite { .. } => ExitCode::from(2),
        }
    }
}
