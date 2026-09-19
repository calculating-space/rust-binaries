mod script;

use clap::{Args, Parser, Subcommand};
use oneshot::{Job, Parse, Record, default_agent, run};
use cast::{Expr, builtin_template, parse, prompt_for, slug};
use serde::Serialize;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "cast",
    version,
    about = "Spawn agents from a script: [a,b].map(x => role) runs one per item, in parallel",
    long_about = "Spawn agents from a script.\n\n\
The expression is `[item, item, ...].map(p => role)`. Each item becomes one \
headless Claude Code run (via oneshot) with the role's prompt; all of them run \
at the same time unless --jobs caps it. The prompt is <role>.md from --prompts \
(default: the current directory) with {p} replaced by the item, or a built-in \
one-liner when no such file exists.\n\n\
Results go to stdout in list order under a `## role(item)` heading, or to \
--out DIR as <role>-<item>.md plus a .json record each. Exit status 0 when \
every agent answered, 1 when any failed, 2 when cast could not start.\n\n\
`cast run <script>` runs a TypeScript script instead; see `cast run --help`."
)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// The expression, e.g. '[paper1,paper2].map(p => researcher)'
    expr: Option<String>,
    /// Directory holding <role>.md prompt files [default: current directory]
    #[arg(long, value_name = "DIR")]
    prompts: Option<PathBuf>,
    /// Use this prompt file for the role instead of <role>.md
    #[arg(long, value_name = "FILE")]
    prompt: Option<PathBuf>,
    /// Run at most this many agents at once [default: all of them]
    #[arg(short, long, value_name = "N")]
    jobs: Option<usize>,
    /// Write each result to DIR/<role>-<item>.md and its record to .json
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Print a JSON array of records on stdout instead of the result texts
    #[arg(long)]
    json: bool,
    /// Print the prompts that would be sent and exit without running anything
    #[arg(long)]
    dry_run: bool,
    /// Kill an agent after this many seconds
    #[arg(long, value_name = "SECS")]
    timeout: Option<u64>,
    /// Working directory for the agents [default: current directory]
    #[arg(long, value_name = "DIR")]
    cwd: Option<PathBuf>,
    /// Agent command instead of `claude -p --output-format json`; repeat per argument
    #[arg(long, value_name = "ARG", num_args = 1)]
    agent: Vec<String>,
    /// Treat the agent's stdout as the result verbatim instead of Claude Code's JSON
    #[arg(long)]
    text: bool,
    /// Extra arguments appended to the agent command, e.g. -- --model haiku --max-turns 10
    #[arg(last = true, value_name = "AGENT ARGS")]
    extra: Vec<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a TypeScript script that imports { agent, bench } from "ai"
    #[command(long_about = "Run a TypeScript script that imports { agent, bench } from \"ai\".\n\n\
`agent(config).run(prompt)` runs one headless Claude Code through oneshot and \
resolves to its record (ok, result, cost_usd, num_turns). The config's model, \
system, tools, maxTurns, cwd and timeout become the agent's flags; budget.usd \
stops further runs once spent. `bench(command)` runs a shell command and \
returns its stdout as JSON, or { loss } from a `METRIC loss=<float>` line; a \
command that prints neither is { loss: Infinity, error }.\n\n\
The script is copied next to the embedded ai module in a temporary directory \
and run with bun, or node 24+, from the current directory. Its stdout, stderr \
and exit status are passed through.")]
    Run(RunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// The script, e.g. loop.ai
    script: PathBuf,
    /// Agent command for agent().run() instead of `claude -p --output-format json`; repeat per argument
    #[arg(long, value_name = "ARG", num_args = 1)]
    agent: Vec<String>,
    /// TypeScript runtime [default: bun on PATH, else node]
    #[arg(long, value_name = "PROGRAM")]
    runtime: Option<PathBuf>,
    /// The oneshot binary [default: next to cast, its source tree, or PATH]
    #[arg(long, value_name = "FILE")]
    oneshot: Option<PathBuf>,
}

/// One slot per item, filled in by whichever worker ran it.
type Outcomes = Vec<Option<Result<Record, String>>>;

#[derive(Serialize)]
struct Entry {
    role: String,
    item: String,
    #[serde(flatten)]
    record: Record,
}

fn load_template(cli: &Cli, expr: &Expr) -> Result<(String, String), String> {
    if let Some(f) = &cli.prompt {
        let t = std::fs::read_to_string(f).map_err(|e| format!("cannot read {}: {e}", f.display()))?;
        return Ok((t, f.display().to_string()));
    }
    let dir = cli.prompts.clone().unwrap_or_else(|| PathBuf::from("."));
    let f = dir.join(format!("{}.md", expr.role));
    if f.is_file() {
        let t = std::fs::read_to_string(&f).map_err(|e| format!("cannot read {}: {e}", f.display()))?;
        return Ok((t, f.display().to_string()));
    }
    Ok((builtin_template(&expr.role, &expr.param), "built-in prompt".into()))
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code as u8),
        Err(msg) => {
            eprintln!("cast: {msg}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<i32, String> {
    let cli = Cli::parse();
    if let Some(Cmd::Run(args)) = cli.cmd {
        return script::run(&script::Options {
            script: args.script,
            agent: args.agent,
            runtime: args.runtime,
            oneshot: args.oneshot,
        });
    }
    let src = cli.expr.as_deref().ok_or("usage: cast '[a, b].map(p => role)' or cast run <script>; see --help")?;
    let expr = parse(src)?;
    let (template, source) = load_template(&cli, &expr)?;
    let prompts: Vec<String> = expr
        .items
        .iter()
        .map(|item| prompt_for(&template, &expr.param, item))
        .collect();

    if cli.dry_run {
        let mut out = io::stdout().lock();
        for (item, prompt) in expr.items.iter().zip(&prompts) {
            writeln!(out, "## {}({item})\n\n{}", expr.role, prompt.trim_end()).map_err(|e| e.to_string())?;
            writeln!(out).map_err(|e| e.to_string())?;
        }
        return Ok(0);
    }

    let mut agent = if cli.agent.is_empty() { default_agent() } else { cli.agent.clone() };
    agent.extend(cli.extra.iter().cloned());
    let jobs: Vec<Job> = prompts
        .iter()
        .map(|prompt| Job {
            prompt: prompt.clone(),
            agent: agent.clone(),
            parse: if cli.text { Parse::Text } else { Parse::Json },
            cwd: cli.cwd.clone(),
            timeout: cli.timeout.map(Duration::from_secs),
        })
        .collect();
    if let Some(dir) = &cli.out {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }

    let n = jobs.len();
    let workers = cli.jobs.unwrap_or(n).clamp(1, n);
    eprintln!("cast: {n} x {} ({source}), {workers} at a time", expr.role);
    let started = Instant::now();
    let jobs = Arc::new(jobs);
    let items = Arc::new(expr.items.clone());
    let role = Arc::new(expr.role.clone());
    let next = Arc::new(Mutex::new(0usize));
    let results: Arc<Mutex<Outcomes>> = Arc::new(Mutex::new((0..n).map(|_| None).collect()));

    let handles: Vec<_> = (0..workers)
        .map(|_| {
            let (jobs, items, role, next, results) =
                (jobs.clone(), items.clone(), role.clone(), next.clone(), results.clone());
            thread::spawn(move || {
                loop {
                    let i = {
                        let mut g = next.lock().unwrap();
                        if *g >= jobs.len() {
                            return;
                        }
                        *g += 1;
                        *g - 1
                    };
                    let label = format!("{role}({})", items[i]);
                    eprintln!("cast: {label} started");
                    let t = Instant::now();
                    let r = run(&jobs[i]).map_err(|e| e.to_string());
                    match &r {
                        Ok(rec) if rec.ok => eprintln!(
                            "cast: {label} done in {} s{}{}",
                            t.elapsed().as_secs(),
                            rec.cost_usd.map(|c| format!(", ${c:.2}")).unwrap_or_default(),
                            rec.num_turns.map(|k| format!(", {k} turns")).unwrap_or_default(),
                        ),
                        Ok(rec) => eprintln!(
                            "cast: {label} FAILED after {} s: {}",
                            t.elapsed().as_secs(),
                            rec.error.as_deref().unwrap_or("unknown error")
                        ),
                        Err(e) => eprintln!("cast: {label} could not start: {e}"),
                    }
                    results.lock().unwrap()[i] = Some(r);
                }
            })
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }

    let results = Arc::try_unwrap(results).map_err(|_| "worker still running")?.into_inner().unwrap();
    let mut entries = Vec::with_capacity(n);
    let mut failures = 0;
    let mut cost = 0.0;
    for (i, r) in results.into_iter().enumerate() {
        let record = match r {
            Some(Ok(rec)) => rec,
            Some(Err(e)) => return Err(e),
            None => return Err("an agent produced no record".into()),
        };
        if !record.ok {
            failures += 1;
        }
        cost += record.cost_usd.unwrap_or(0.0);
        entries.push(Entry { role: expr.role.clone(), item: expr.items[i].clone(), record });
    }
    eprintln!(
        "cast: {} of {n} ok, ${cost:.2}, {} s",
        n - failures,
        started.elapsed().as_secs()
    );

    let mut out = io::stdout().lock();
    if cli.json {
        let mut s = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
        s.push('\n');
        out.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    } else if let Some(dir) = &cli.out {
        for e in &entries {
            let base = format!("{}-{}", e.role, slug(&e.item));
            let md = dir.join(format!("{base}.md"));
            let js = dir.join(format!("{base}.json"));
            std::fs::write(&md, &e.record.result).map_err(|x| format!("cannot write {}: {x}", md.display()))?;
            let mut s = serde_json::to_string_pretty(e).map_err(|x| x.to_string())?;
            s.push('\n');
            std::fs::write(&js, s).map_err(|x| format!("cannot write {}: {x}", js.display()))?;
            writeln!(out, "{}", md.display()).map_err(|x| x.to_string())?;
        }
    } else {
        for e in &entries {
            writeln!(out, "## {}({})\n", e.role, e.item).map_err(|x| x.to_string())?;
            let body = if e.record.ok {
                e.record.result.trim_end().to_string()
            } else {
                format!(
                    "(failed: {})\n{}",
                    e.record.error.as_deref().unwrap_or("unknown error"),
                    e.record.result.trim_end()
                )
            };
            writeln!(out, "{body}\n").map_err(|x| x.to_string())?;
        }
    }
    Ok(if failures == 0 { 0 } else { 1 })
}
