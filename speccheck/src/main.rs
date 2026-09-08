use clap::{Parser, Subcommand};
use speccheck::{Requirements, Spec, check, render, spec};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    version,
    about = "Capture a machine spec and check it against a requirements matrix"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Detect this machine and print its spec as JSON.
    Spec {
        /// Filesystem to measure free space on (default: $HOME)
        #[arg(long)]
        disk: Option<PathBuf>,
        /// Tools to probe with --version (repeatable)
        #[arg(long = "tool")]
        tools: Vec<String>,
    },
    /// Check requirements (JSON file, or - for stdin) against this machine or a saved spec. Exit 0 can run, 2 pointless, 3 cannot run.
    Check {
        #[arg(long)]
        requirements: PathBuf,
        /// A spec JSON from `speccheck spec`; default is to detect now
        #[arg(long)]
        spec: Option<PathBuf>,
        #[arg(long)]
        disk: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn read_arg(path: &PathBuf) -> Result<String, String> {
    if path.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        Ok(s)
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Cmd::Spec { disk, tools } => {
            let s = spec::detect(&disk.unwrap_or_else(home), &tools);
            println!(
                "{}",
                serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Check {
            requirements,
            spec: spec_path,
            disk,
            json,
        } => {
            let req: Requirements = serde_json::from_str(&read_arg(&requirements)?)
                .map_err(|e| format!("requirements: {e}"))?;
            if req.version != speccheck::CONTRACT_VERSION {
                return Err(format!(
                    "requirements version {} unsupported (want {})",
                    req.version,
                    speccheck::CONTRACT_VERSION
                ));
            }
            let s: Spec = match spec_path {
                Some(p) => {
                    serde_json::from_str(&read_arg(&p)?).map_err(|e| format!("spec: {e}"))?
                }
                None => spec::detect(&disk.unwrap_or_else(home), &req.tool_names()),
            };
            let v = check(&s, &req);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?
                );
            } else {
                print!("{}", render(&v));
            }
            Ok(ExitCode::from(v.outcome.exit_code()))
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("speccheck: {e}");
            ExitCode::FAILURE
        }
    }
}
