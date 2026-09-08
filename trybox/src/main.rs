use clap::{Parser, Subcommand};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use trybox::{Backend, Manifest, agent, backend, default_root, doctor, host_info, list, load, recipe, validate_name};

#[derive(Parser)]
#[command(version, about = "Disposable experiment environments with an agent inside. Create, run, destroy.")]
struct Cli {
    /// Directory holding all sandboxes. Default: $TRYBOX_ROOT or ~/.trybox
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// With no subcommand: the guided menu.
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Report which backends and the agent CLI are usable on this machine.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// List recipes, or read one as a man page: what it is, requirements against this machine, hello world, tour.
    Recipes {
        name: Option<String>,
        #[arg(long)]
        json: bool,
        /// Print the recipe's requirements matrix as speccheck JSON (compose: `speccheck check --requirements -`)
        #[arg(long)]
        requirements: bool,
        /// Print directly instead of through $PAGER
        #[arg(long)]
        no_pager: bool,
    },
    /// Can this machine run a recipe, and does it make sense? Exit 0 yes, 2 pointless, 3 cannot.
    Check {
        recipe: String,
        #[arg(long)]
        json: bool,
    },
    /// Explore a recipe: check requirements, prepare its sandbox, run the hello world, then pick tour steps or hand over to the agent.
    Explore {
        recipe: String,
        /// Sandbox name (default: the recipe name)
        #[arg(long)]
        sandbox: Option<String>,
        /// Go ahead even if the requirements check says it cannot run or is pointless here
        #[arg(long)]
        force: bool,
    },
    /// Prepare a sandbox: interpreter, packages, isolated caches and an agent briefing.
    Create {
        name: String,
        /// Override the recipe's backend
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        /// How to run something; see `trybox recipes`
        #[arg(long, default_value = "bare")]
        recipe: String,
        /// Extra packages on top of the recipe
        #[arg(long = "pip", value_name = "PKG")]
        extra: Vec<String>,
        /// Python version (default comes from the recipe)
        #[arg(long)]
        python: Option<String>,
        /// Print the plan as JSON and do nothing
        #[arg(long)]
        dry_run: bool,
        /// Post a desktop notification when the sandbox is ready (macOS)
        #[arg(long)]
        notify: bool,
        /// Launch the agent as soon as the sandbox is ready
        #[arg(long)]
        agent: bool,
        #[arg(long)]
        quiet: bool,
    },
    /// List sandboxes.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show one sandbox's manifest and disk usage.
    Status { name: String },
    /// Run a command inside a sandbox: trybox run NAME -- python -c '...'
    Run {
        name: String,
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    /// Drop an interactive Claude Code agent into the sandbox. It waits for instructions and suggests experiments.
    Agent {
        name: String,
        /// Opening message for the agent
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long, default_value = "claude")]
        claude_bin: String,
        /// Extra flags passed to claude, e.g. -- --permission-mode acceptEdits
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Dispose of sandboxes: environment, downloaded models, work files, docker image. Frees the disk.
    #[command(alias = "destroy")]
    Dispose {
        names: Vec<String>,
        /// Every sandbox under the root
        #[arg(long)]
        all: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

/// Show long text through $PAGER (default `less -R`) when on a terminal, like man does.
fn page(text: &str, no_pager: bool) {
    use std::io::{IsTerminal, Write};
    if !no_pager && std::io::stdout().is_terminal() {
        let pager = std::env::var("PAGER").unwrap_or_else(|_| "less -R".into());
        let mut parts = pager.split_whitespace();
        if let Some(bin) = parts.next()
            && let Ok(mut child) = Command::new(bin).args(parts).stdin(std::process::Stdio::piped()).spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }
    print!("{text}");
}

fn notify(title: &str, body: &str) {
    if cfg!(target_os = "macos") {
        let script = format!("display notification \"{}\" with title \"{}\"", body.replace('"', "'"), title.replace('"', "'"));
        let _ = Command::new("/usr/bin/osascript").args(["-e", &script]).status();
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let root = cli.root.unwrap_or_else(default_root);
    let Some(command) = cli.command else {
        return match trybox::home::run(&root)? {
            Some(m) => {
                let err = agent::agent_command(&m, "claude", agent::DEFAULT_PROMPT, &[]).exec();
                Err(format!("cannot launch claude: {err}"))
            }
            None => Ok(ExitCode::SUCCESS),
        };
    };
    match command {
        Cmd::Doctor { json } => {
            let report = doctor();
            if json {
                println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            } else {
                println!("host: {} ({} GB, {} {})", report.host.chip, report.host.memory_gb, report.host.os, report.host.arch);
                for t in &report.tools {
                    println!("{:<14} {} {}", t.name, if t.available { "ok  " } else { "MISSING" }, t.detail);
                }
                for n in &report.notes {
                    println!("note: {n}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Recipes { name, json, requirements, no_pager } => {
            match name {
                Some(n) => {
                    let r = recipe(&n)?;
                    if requirements {
                        println!("{}", serde_json::to_string_pretty(&(r.requirements)()).map_err(|e| e.to_string())?);
                    } else if json {
                        println!("{}", serde_json::to_string_pretty(r).map_err(|e| e.to_string())?);
                    } else {
                        let verdict = trybox::recipe::check_here(r, &root);
                        page(&trybox::recipe::man(r, Some(&verdict)), no_pager);
                    }
                }
                None => {
                    if json { println!("{}", serde_json::to_string_pretty(trybox::recipe::RECIPES).map_err(|e| e.to_string())?) } else { print!("{}", trybox::recipe::table()) }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Check { recipe: recipe_name, json } => {
            let r = recipe(&recipe_name)?;
            let v = trybox::recipe::check_here(r, &root);
            if json {
                println!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
            } else {
                print!("{}", speccheck::render(&v));
            }
            Ok(ExitCode::from(v.outcome.exit_code()))
        }
        Cmd::Explore { recipe: recipe_name, sandbox, force } => {
            let r = recipe(&recipe_name)?;
            let sandbox = sandbox.unwrap_or_else(|| r.name.to_string());
            validate_name(&sandbox)?;
            if load(&root, &sandbox).is_err() {
                let v = trybox::recipe::check_here(r, &root);
                match v.outcome {
                    speccheck::Outcome::CanRun => {}
                    _ if force => eprintln!("warning: requirements not met, continuing because of --force"),
                    speccheck::Outcome::Pointless => {
                        eprint!("{}", speccheck::render(&v));
                        return Err(format!("{} would run here but not make sense; `trybox explore {} --force` to do it anyway", r.name, r.name));
                    }
                    speccheck::Outcome::CannotRun => {
                        eprint!("{}", speccheck::render(&v));
                        return Err(format!("{} cannot run on this machine", r.name));
                    }
                }
            }
            let m = trybox::explore::ensure_sandbox(&root, r, &sandbox)?;
            match trybox::explore::tour(&m, r)? {
                trybox::explore::Next::Agent => {
                    let err = agent::agent_command(&m, "claude", agent::DEFAULT_PROMPT, &[]).exec();
                    Err(format!("cannot launch claude: {err}"))
                }
                trybox::explore::Next::Quit => {
                    println!("sandbox {sandbox} kept; `trybox` or `trybox explore {}` resumes", r.name);
                    Ok(ExitCode::SUCCESS)
                }
                trybox::explore::Next::Disposed => Ok(ExitCode::SUCCESS),
            }
        }
        Cmd::Create { name, backend, recipe: recipe_name, extra, python, dry_run, notify: want_notify, agent: launch_agent, quiet } => {
            validate_name(&name)?;
            let r = recipe(&recipe_name)?;
            let be = backend.unwrap_or(r.backend);
            let dir = root.join(&name);
            if dir.exists() && !dry_run {
                return Err(format!("sandbox {name:?} already exists at {}; destroy it first", dir.display()));
            }
            let mut packages: Vec<String> = r.packages.iter().map(|p| p.to_string()).collect();
            packages.extend(extra);
            let m = Manifest::new(&name, be, r.name, python.as_deref().unwrap_or(r.python), packages, dir, host_info());
            let mut plan = backend::prepare(&m);
            plan.files.push((m.work_dir().join("CLAUDE.md"), agent::briefing(&m, r)));
            if dry_run {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "manifest": m, "plan": plan })).map_err(|e| e.to_string())?);
                return Ok(ExitCode::SUCCESS);
            }
            if be == Backend::Docker && m.host.os == "macos" && !quiet {
                eprintln!("warning: docker on macOS has no GPU access; GPU benchmarks in this sandbox will be CPU-only");
            }
            if let Err(e) = backend::run_plan(&m, &plan, quiet) {
                let _ = std::fs::remove_dir_all(&m.dir);
                return Err(e);
            }
            m.save()?;
            let msg = format!("sandbox {name} ready ({}, {} packages) at {}", be.as_str(), m.packages.len(), m.dir.display());
            println!("{msg}");
            println!("next: trybox agent {name}    or    trybox run {name} -- python -V");
            if want_notify {
                notify("trybox", &format!("sandbox {name} is ready"));
            }
            if launch_agent {
                let err = agent::agent_command(&m, "claude", agent::DEFAULT_PROMPT, &[]).exec();
                return Err(format!("cannot launch claude: {err}"));
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::List { json } => {
            let all = list(&root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?);
            } else {
                print!("{}", trybox::overview(&all));
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Status { name } => {
            let m = load(&root, &name)?;
            print!("{}", trybox::status(&m));
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Run { name, argv } => {
            let m = load(&root, &name)?;
            let status = backend::exec_command(&m, &argv).status().map_err(|e| format!("run: {e}"))?;
            Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
        }
        Cmd::Agent { name, prompt, claude_bin, extra } => {
            let m = load(&root, &name)?;
            let err = agent::agent_command(&m, &claude_bin, prompt.as_deref().unwrap_or(agent::DEFAULT_PROMPT), &extra).exec();
            Err(format!("cannot launch {claude_bin}: {err}"))
        }
        Cmd::Dispose { names, all, yes } => {
            let targets: Vec<Manifest> = if all {
                list(&root)?
            } else {
                if names.is_empty() {
                    return Err("give sandbox names, or --all".into());
                }
                names.iter().map(|n| load(&root, n)).collect::<Result<_, _>>()?
            };
            if targets.is_empty() {
                println!("nothing to dispose under {}", root.display());
                return Ok(ExitCode::SUCCESS);
            }
            let total: u64 = targets.iter().map(|m| backend::dir_size(&m.dir)).sum();
            let gb = total as f64 / (1u64 << 30) as f64;
            if !yes {
                for m in &targets {
                    eprintln!("  {:<20} {:<7} {:.2} GB", m.name, m.backend.as_str(), backend::dir_size(&m.dir) as f64 / (1u64 << 30) as f64);
                }
                eprint!("dispose {} sandbox(es), freeing {gb:.2} GB? [y/N] ", targets.len());
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
                if !matches!(line.trim(), "y" | "Y" | "yes") {
                    println!("kept");
                    return Ok(ExitCode::from(2));
                }
            }
            let mut failed = false;
            for m in &targets {
                match backend::destroy(m) {
                    Ok(notes) => notes.iter().for_each(|n| println!("{n}")),
                    Err(e) => {
                        failed = true;
                        eprintln!("trybox: {}: {e}", m.name);
                    }
                }
            }
            println!("freed {gb:.2} GB");
            Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("trybox: {e}");
            ExitCode::FAILURE
        }
    }
}
