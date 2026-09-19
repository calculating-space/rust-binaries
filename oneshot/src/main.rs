use clap::Parser;
use oneshot::{Job, Parse, Record, default_agent, run};
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "oneshot",
    version,
    about = "Hand a prompt to an agent and get its result back",
    long_about = "Hand a prompt to an agent and get its result back.\n\n\
The prompt comes from the argument, --file, or stdin. The agent is \
`claude -p --output-format json` unless --agent says otherwise; it gets the \
prompt on stdin. The result text goes to stdout (or --out). --json prints the \
whole record instead: prompt, agent, exit status, duration, cost, turns, and \
the agent's raw output.\n\n\
Exit status: 0 the agent answered; 124 it ran out of time; 2 oneshot itself \
could not run (bad arguments, agent not found); otherwise the agent's own \
exit status, or 1 when it exited 0 but reported an error."
)]
struct Cli {
    /// The prompt. Omit it (or pass -) to read the prompt from stdin.
    prompt: Option<String>,
    /// Read the prompt from this file instead
    #[arg(short, long, value_name = "FILE", conflicts_with = "prompt")]
    file: Option<PathBuf>,
    /// Agent command; the prompt is written to its stdin. Repeat for each argument
    #[arg(long, value_name = "ARG", num_args = 1)]
    agent: Vec<String>,
    /// Treat the agent's stdout as the result verbatim instead of Claude Code's JSON
    #[arg(long)]
    text: bool,
    /// Working directory for the agent [default: current directory]
    #[arg(long, value_name = "DIR")]
    cwd: Option<PathBuf>,
    /// Kill the agent after this many seconds
    #[arg(long, value_name = "SECS")]
    timeout: Option<u64>,
    /// Write the result text here instead of stdout
    #[arg(short, long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Print the full JSON record on stdout instead of the result text
    #[arg(long)]
    json: bool,
    /// Also write the full JSON record to this file
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,
    /// Show the agent's stderr even when it succeeded
    #[arg(short, long)]
    verbose: bool,
    /// Extra arguments appended to the agent command, e.g. -- --model haiku --max-turns 3
    #[arg(last = true, value_name = "AGENT ARGS")]
    extra: Vec<String>,
}

fn read_prompt(cli: &Cli) -> Result<String, String> {
    if let Some(f) = &cli.file {
        return std::fs::read_to_string(f).map_err(|e| format!("cannot read {}: {e}", f.display()));
    }
    match cli.prompt.as_deref() {
        Some("-") | None => {
            let stdin = io::stdin();
            if stdin.is_terminal() {
                return Err("no prompt: pass it as an argument, with --file, or on stdin".into());
            }
            let mut s = String::new();
            stdin
                .lock()
                .read_to_string(&mut s)
                .map_err(|e| format!("cannot read stdin: {e}"))?;
            Ok(s)
        }
        Some(p) => Ok(p.to_string()),
    }
}

fn write_record(path: &PathBuf, record: &Record) -> Result<(), String> {
    let mut s = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    s.push('\n');
    std::fs::write(path, s).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code as u8),
        Err(msg) => {
            eprintln!("oneshot: {msg}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<i32, String> {
    let cli = Cli::parse();
    let prompt = read_prompt(&cli)?;
    if prompt.trim().is_empty() {
        return Err("the prompt is empty".into());
    }
    let mut agent = if cli.agent.is_empty() { default_agent() } else { cli.agent.clone() };
    agent.extend(cli.extra.iter().cloned());
    let job = Job {
        prompt,
        agent,
        parse: if cli.text { Parse::Text } else { Parse::Json },
        cwd: cli.cwd.clone(),
        timeout: cli.timeout.map(Duration::from_secs),
    };
    let record = run(&job).map_err(|e| e.to_string())?;

    if let Some(p) = &cli.record {
        write_record(p, &record)?;
    }
    if !record.ok || cli.verbose {
        if let Some(e) = &record.error {
            eprintln!("oneshot: {e}");
        }
        if !record.stderr.is_empty() {
            eprint!("{}", record.stderr);
            if !record.stderr.ends_with('\n') {
                eprintln!();
            }
        }
    }
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    if cli.json {
        let mut s = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
        s.push('\n');
        stdout.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    } else if let Some(p) = &cli.out {
        std::fs::write(p, &record.result).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
    } else if !record.result.is_empty() {
        stdout.write_all(record.result.as_bytes()).map_err(|e| e.to_string())?;
        if !record.result.ends_with('\n') {
            stdout.write_all(b"\n").map_err(|e| e.to_string())?;
        }
    }
    Ok(record.exit_code())
}
