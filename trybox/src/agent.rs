use crate::backend::{Backend, sandbox_env};
use crate::manifest::Manifest;
use crate::recipe::Recipe;
use std::process::Command;

/// The CLAUDE.md dropped into the sandbox's work dir. The agent reads it on start.
pub fn briefing(m: &Manifest, r: &Recipe) -> String {
    let mut out = String::new();
    out.push_str(&format!("# trybox sandbox `{}`\n\n", m.name));
    out.push_str("You are the resident agent of a disposable experiment environment. Your job: wait for instructions, propose concrete experiments, run them when asked, and report numbers plainly. Everything you do stays inside this sandbox and is deleted with `trybox destroy`.\n\n");
    out.push_str("## Environment\n\n");
    out.push_str(&format!("- Backend: {}\n- Recipe: {} ({})\n- Python: {}\n", m.backend.as_str(), r.name, r.summary, m.python));
    if m.packages.is_empty() {
        out.push_str("- Packages: none preinstalled\n");
    } else {
        out.push_str(&format!("- Packages: {}\n", m.packages.join(", ")));
    }
    out.push_str(&format!("- Host: {} on {}, {} GB memory, {}\n", m.host.chip, m.host.os, m.host.memory_gb, m.host.arch));
    out.push_str(&format!("- Sandbox dir: {}\n- Working dir (you are here): {}\n- Model cache (HF_HOME): {}\n\n", m.dir.display(), m.work_dir().display(), m.hf_dir().display()));
    out.push_str("## How to run things\n\n");
    match m.backend {
        Backend::Docker => out.push_str(&format!(
            "Every command must run inside the container. Prefix it with the wrapper:\n\n```sh\n{}/x python -c 'import sys; print(sys.version)'\n```\n\nThe working dir is mounted at /work and the model cache at /hf. Do not install anything on the host.\n\n",
            m.dir.display()
        )),
        _ => out.push_str("The virtualenv is already on PATH and HF_HOME points into the sandbox, so run `python`, `pip`, `uv pip install ...` and package CLIs directly. Install extra packages with `uv pip install <pkg>`; they stay inside the sandbox. Never use `pip install --user`, `brew`, or anything that writes outside this directory.\n\n"),
    }
    out.push_str("## Rules\n\n- Say what a step will download or how long it will take before running it if it is more than a few seconds or a few hundred MB.\n- Report measurements as a small table with the exact command used. Warmup runs do not count.\n- Keep scripts and results in this working dir so the user can read them later.\n- When idle, offer the next experiment; do not run it unprompted.\n\n");
    out.push_str("## What this project is\n\n");
    out.push_str(r.what);
    out.push_str(&format!("\n\nRepo: {}\n\n", r.repo));
    out.push_str("## Known-good commands\n\nThese come from the recipe, ordered from hello world to advanced. Each is a shell command valid in this environment; the user may already have run some via `trybox explore`.\n\n");
    for (i, e) in std::iter::once(&r.hello).chain(r.tour.iter()).enumerate() {
        out.push_str(&format!("{}. {} — {}\n```sh\n{}\n```\nExpect: {}\n\n", i, e.title, e.learn, e.run, e.expect));
    }
    if !r.suggestions.is_empty() {
        out.push_str("## Suggested experiments\n\n");
        for (i, sug) in r.suggestions.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, sug));
        }
        out.push('\n');
    }
    if !r.caveats.is_empty() {
        out.push_str("## Caveats\n\n");
        for c in r.caveats {
            out.push_str(&format!("- {c}\n"));
        }
        out.push('\n');
    }
    if m.backend == Backend::Docker && m.host.os == "macos" {
        out.push_str("Note: this is a docker backend on macOS. There is no GPU inside the container. Any GPU or Metal number here is meaningless; say so if asked to measure it.\n");
    }
    out
}

pub const DEFAULT_PROMPT: &str = "The environment is ready. Read CLAUDE.md, confirm in one or two lines what is installed and on what hardware, then propose three to five concrete experiments from the suggested list, ranked by usefulness, and wait for me to pick one.";

/// What the user has already done in this sandbox, for the agent's system prompt.
pub fn progress_note(m: &Manifest) -> String {
    let p = crate::progress::Progress::load(&m.dir);
    let Ok(r) = crate::recipe::recipe(&m.recipe) else { return String::new() };
    let done: Vec<&str> = r.tour.iter().enumerate().filter(|(i, _)| p.done(*i)).map(|(_, e)| e.title).collect();
    let hello = p.hello.as_ref().is_some_and(|h| h.ok);
    match (hello, done.is_empty()) {
        (false, true) => " The user has not run anything here yet.".to_string(),
        (true, true) => " The user has run the hello world and nothing else yet.".to_string(),
        _ => format!(" The user has already completed these tour steps, so do not repeat them unless asked: {}.", done.join("; ")),
    }
}

/// Build the `claude` invocation for the sandbox. Caller decides whether to exec or spawn.
pub fn agent_command(m: &Manifest, claude_bin: &str, prompt: &str, extra: &[String]) -> Command {
    let mut cmd = Command::new(claude_bin);
    cmd.current_dir(m.work_dir());
    cmd.envs(sandbox_env(m));
    cmd.arg("--append-system-prompt").arg(format!(
        "You are running inside trybox sandbox '{}' (backend {}). Treat CLAUDE.md in the working directory as your briefing. Stay inside {} and never modify the host outside it.{}",
        m.name,
        m.backend.as_str(),
        m.dir.display(),
        progress_note(m)
    ));
    cmd.args(extra);
    cmd.arg(prompt);
    cmd
}
