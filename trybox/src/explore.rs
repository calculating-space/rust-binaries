//! `trybox explore`: prepare quietly, run the hello world, then a guided menu.

use crate::agent;
use crate::backend::{self, exec_command};
use crate::manifest::{Manifest, load};
use crate::progress::{Progress, gb};
use crate::recipe::{Example, Recipe};
use crate::ui::{Choice, confirm, select};
use std::io::Write;
use std::path::Path;

/// Get or create the sandbox for a recipe. Creation output is hidden; only a one-line status shows.
pub fn ensure_sandbox(root: &Path, r: &Recipe, sandbox: &str) -> Result<Manifest, String> {
    if let Ok(m) = load(root, sandbox) {
        return Ok(m);
    }
    eprintln!("preparing {sandbox} (first time only; installing {} packages)...", r.packages.len());
    let m = Manifest::new(sandbox, r.backend, r.name, r.python, r.packages.iter().map(|p| p.to_string()).collect(), root.join(sandbox), crate::doctor::host_info());
    let mut plan = backend::prepare(&m);
    plan.files.push((m.work_dir().join("CLAUDE.md"), agent::briefing(&m, r)));
    if let Err(e) = backend::run_plan(&m, &plan, true) {
        let _ = std::fs::remove_dir_all(&m.dir);
        return Err(format!("{e}; rerun with `trybox create {sandbox} --recipe {}` to see the full output", r.name));
    }
    m.save()?;
    Ok(m)
}

/// Run one example: show what it teaches, the command, run it, then what to look for.
pub fn run_example(m: &Manifest, e: &Example) -> Result<bool, String> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "\n== {}\n   {}\n", e.title, e.learn).ok();
    for (i, l) in e.run.lines().enumerate() {
        writeln!(out, "   {} {l}", if i == 0 { "$" } else { " " }).ok();
    }
    writeln!(out).ok();
    out.flush().ok();
    let status = exec_command(m, &["sh".into(), "-c".into(), e.run.to_string()]).status().map_err(|e| format!("run: {e}"))?;
    writeln!(out, "\n   expected: {}{}\n", e.expect, if status.success() { "" } else { "\n   (the command did not exit cleanly)" }).ok();
    Ok(status.success())
}

/// What the user picked at the end of a tour session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Next {
    Agent,
    Quit,
    Disposed,
}

/// The tour menu: steps first (next undone one recommended), then agent, man page, dispose, back.
pub fn choices(r: &Recipe, p: &Progress, size_bytes: u64) -> Vec<Choice> {
    let next_undone = (0..r.tour.len()).find(|&i| !p.done(i));
    let mut out: Vec<Choice> = r
        .tour
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let c = Choice::new(format!("{}{}", if p.done(i) { "✓ " } else { "" }, e.title), e.learn);
            if Some(i) == next_undone { c.recommended() } else { c }
        })
        .collect();
    let mut agent = Choice::new("Hand over to the agent", "Open-ended: an agent inside the sandbox proposes and runs experiments with you");
    if next_undone.is_none() {
        agent = agent.recommended();
    }
    out.push(agent);
    out.push(Choice::new("Read the man page", format!("What {} is, requirements against this machine, every step with its command", r.name)));
    out.push(Choice::new("Dispose this sandbox", format!("Delete the environment and everything it downloaded, freeing {}", gb(size_bytes))));
    out.push(Choice::new("Back", "Keep the sandbox and return"));
    out
}

/// The interactive loop after the hello world.
pub fn tour(m: &Manifest, r: &Recipe) -> Result<Next, String> {
    let mut progress = Progress::load(&m.dir);
    if !progress.hello.as_ref().is_some_and(|h| h.ok) {
        println!("\n{}\n\n{}\n\nHELLO WORLD", r.name, r.what);
        let ok = run_example(m, &r.hello)?;
        progress.record(None, ok);
        progress.save(&m.dir)?;
    }
    loop {
        let size = backend::dir_size(&m.dir);
        let choices = choices(r, &progress, size);
        let steps = r.tour.len();
        let question = format!("{}: what next? ({})", m.name, progress.summary(steps));
        let Some(pick) = select(&question, &choices)? else { return Ok(Next::Quit) };
        match pick {
            i if i < steps => {
                let ok = run_example(m, &r.tour[i])?;
                progress.record(Some(i), ok);
                progress.save(&m.dir)?;
            }
            i if i == steps => {
                progress.agent_sessions += 1;
                progress.save(&m.dir)?;
                return Ok(Next::Agent);
            }
            i if i == steps + 1 => {
                let v = crate::recipe::check_here(r, &m.dir);
                print!("\n{}", crate::recipe::man(r, Some(&v)));
            }
            i if i == steps + 2 => {
                if confirm(&format!("Dispose {} and free {}?", m.name, gb(size)), "Yes, dispose it", "No, keep it")? {
                    for n in backend::destroy(m)? {
                        println!("{n}");
                    }
                    return Ok(Next::Disposed);
                }
            }
            _ => return Ok(Next::Quit),
        }
    }
}
