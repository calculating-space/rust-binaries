use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{NaiveDate, TimeZone, Utc};
use clap::{Parser, ValueEnum};
use grindreport::{analyze, load, load_manifest, Filter, Format, Options, Pricing};

#[derive(Parser)]
#[command(
    version,
    about = "Render a grindstone dataset as a usage report for a person or an agent",
    after_help = "Exit 0 rendered, 1 error, 2 no dataset or unsupported schema.\n\
                  Time windows are measured against the dataset's coverage end, not the clock,\n\
                  so the same command on the same dataset always gives the same report."
)]
struct Cli {
    /// grindstone output directory (holds manifest.json and facts/)
    #[arg(long, default_value = "./out")]
    out: PathBuf,
    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = Fmt::Text)]
    format: Fmt,
    /// Write to this file instead of stdout
    #[arg(long, short = 'o')]
    to: Option<PathBuf>,
    /// Only the last N days/weeks/hours before the dataset's coverage end, e.g. 30d, 2w, 12h
    #[arg(long)]
    since: Option<String>,
    /// Sessions ending on or after this date (YYYY-MM-DD, UTC)
    #[arg(long)]
    from: Option<NaiveDate>,
    /// Sessions starting before this date (YYYY-MM-DD, UTC, exclusive)
    #[arg(long)]
    to_date: Option<NaiveDate>,
    /// Keep sessions whose project slug or working directory contains this text (repeatable)
    #[arg(long = "project")]
    projects: Vec<String>,
    /// Rows to keep in ranked tables before folding the rest into "other"
    #[arg(long, default_value_t = 12)]
    top: usize,
    /// Price table (TOML or JSON, USD per million tokens by model); adds a cost section
    #[arg(long)]
    pricing: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Fmt {
    /// Aligned terminal text
    Text,
    /// Markdown digest with a glossary, for agents
    Md,
    /// Self-contained HTML page with charts
    Html,
    /// The report struct as JSON
    Json,
}

fn parse_since(s: &str) -> Result<i64, String> {
    let (num, unit) = s.split_at(s.trim_end_matches(|c: char| c.is_ascii_alphabetic()).len());
    let n: i64 = num
        .parse()
        .map_err(|_| format!("--since {s:?}: want a number followed by h, d or w"))?;
    let ms = match unit {
        "h" => 3_600_000,
        "d" | "" => 86_400_000,
        "w" => 7 * 86_400_000,
        _ => return Err(format!("--since {s:?}: unit must be h, d or w")),
    };
    Ok(n * ms)
}

fn run(cli: Cli) -> Result<ExitCode, (u8, String)> {
    let manifest = load_manifest(&cli.out).map_err(|e| (2, format!("{e:#}")))?;
    let mut filter = Filter {
        projects: cli.projects.clone(),
        ..Default::default()
    };
    if let Some(d) = cli.from {
        filter.from_ms = Some(
            Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
                .timestamp_millis(),
        );
    }
    if let Some(d) = cli.to_date {
        filter.to_ms = Some(
            Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
                .timestamp_millis(),
        );
    }
    if let Some(s) = &cli.since {
        let span = parse_since(s).map_err(|e| (1, e))?;
        let end = manifest
            .coverage
            .end
            .as_deref()
            .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
            .map(|d| d.timestamp_millis())
            .ok_or_else(|| {
                (
                    2,
                    "manifest has no coverage.end to anchor --since on".to_string(),
                )
            })?;
        filter.from_ms = Some(filter.from_ms.map_or(end - span, |f| f.max(end - span)));
    }
    let pricing = match &cli.pricing {
        Some(p) => Some(Pricing::load(p).map_err(|e| (1, format!("{e:#}")))?),
        None => None,
    };
    let sessions_in_dataset = manifest
        .tables
        .get("sessions")
        .map(|t| t.rows as usize)
        .unwrap_or(0);
    let ds = load(&cli.out, &filter).map_err(|e| (2, format!("{e:#}")))?;
    let opts = Options {
        dataset_dir: cli.out.display().to_string(),
        top: cli.top.max(1),
        pricing,
        filter_from: filter.from_ms,
        filter_to: filter.to_ms,
        filter_projects: filter.projects.clone(),
        sessions_in_dataset: sessions_in_dataset.max(ds.sessions.len()),
    };
    let report = analyze(&ds, &opts);
    let format = match cli.format {
        Fmt::Text => Format::Text,
        Fmt::Md => Format::Markdown,
        Fmt::Html => Format::Html,
        Fmt::Json => Format::Json,
    };
    let body = format.render(&report);
    match &cli.to {
        Some(path) => {
            std::fs::write(path, body)
                .map_err(|e| (1, format!("writing {}: {e}", path.display())))?;
            eprintln!("wrote {}", path.display());
        }
        None => print!("{body}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(c) => c,
        Err((code, msg)) => {
            eprintln!("grindreport: {msg}");
            ExitCode::from(code)
        }
    }
}
