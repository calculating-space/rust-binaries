use grindstone::{check, scan};

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Turns Claude Code session transcripts into a Parquet metrics dataset.
#[derive(Parser)]
#[command(name = "grindstone", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn default_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".claude")
        .join("projects")
}

#[derive(Subcommand)]
enum Command {
    /// Parse transcripts and write facts/*.parquet + manifest.json
    Scan {
        /// Transcript root directory
        #[arg(long, default_value_os_t = default_root())]
        root: PathBuf,
        /// Output dataset directory
        #[arg(long, default_value = "./out")]
        out: PathBuf,
        /// Only scan files modified since (e.g. 30d, 12h, 2026-08-01)
        #[arg(long)]
        since: Option<String>,
        /// Only scan these project slugs (repeatable)
        #[arg(long = "project")]
        projects: Vec<String>,
        /// Also write a texts table (message bodies; off by default)
        #[arg(long)]
        include_text: bool,
        /// Ignore checkpoints and rebuild the dataset from scratch
        #[arg(long)]
        force: bool,
    },
    /// Validate manifest / hashes / parquet integrity of an output dataset
    Check {
        #[arg(long, default_value_os_t = default_root())]
        root: PathBuf,
        #[arg(long, default_value = "./out")]
        out: PathBuf,
    },
    /// LLM classification pass (not yet implemented)
    Enrich,
    /// Cost/aggregate reporting (not yet implemented)
    Stats,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Scan {
            root,
            out,
            since,
            projects,
            include_text,
            force,
        } => scan::run(scan::ScanOpts {
            root,
            out,
            since,
            projects,
            include_text,
            force,
        }),
        Command::Check { root, out } => check::run(&root, &out),
        Command::Enrich | Command::Stats => {
            eprintln!("not yet implemented");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
