//! `trybox` with no arguments: the guided front door.

use crate::backend;
use crate::manifest::{Manifest, list};
use crate::progress::{Progress, ago, gb};
use crate::recipe::{RECIPES, Recipe, check_here, recipe};
use crate::ui::{Choice, confirm, select};
use speccheck::Outcome;
use std::path::Path;

/// What the home menu offers.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Continue(String),
    Dispose(String),
    StartNew,
    Quit,
}

/// Build the home menu from existing sandboxes. Most recently used sandbox is recommended;
/// with none, starting something new is.
pub fn choices(all: &[Manifest]) -> (Vec<Choice>, Vec<Action>) {
    let mut rows: Vec<(&Manifest, Progress, u64)> = all.iter().map(|m| (m, Progress::load(&m.dir), backend::dir_size(&m.dir))).collect();
    rows.sort_by_key(|(m, p, _)| std::cmp::Reverse(p.last_unix().unwrap_or(m.created_unix)));
    let mut choices = Vec::new();
    let mut actions = Vec::new();
    for (i, (m, p, size)) in rows.iter().enumerate() {
        let (what, progress) = match recipe(&m.recipe) {
            Ok(r) => (r.summary.to_string(), p.summary(r.tour.len())),
            Err(_) => (format!("unknown recipe {}", m.recipe), "?".into()),
        };
        let c = Choice::new(format!("Continue exploring {}", m.name), format!("{what} · {progress} · {} · last used {}", gb(*size), ago(p.last_unix().unwrap_or(m.created_unix))));
        choices.push(if i == 0 { c.recommended() } else { c });
        actions.push(Action::Continue(m.name.clone()));
    }
    let start = Choice::new("Start something new", "Pick a recipe: a guided tour of one project");
    choices.push(if rows.is_empty() { start.recommended() } else { start });
    actions.push(Action::StartNew);
    for (m, _, size) in &rows {
        choices.push(Choice::new(format!("Dispose {}", m.name), format!("Delete the sandbox and everything it downloaded, freeing {}", gb(*size))));
        actions.push(Action::Dispose(m.name.clone()));
    }
    choices.push(Choice::new("Quit", ""));
    actions.push(Action::Quit);
    (choices, actions)
}

/// The recipe picker: every recipe with its verdict for this machine; first runnable one recommended.
pub fn recipe_choices(root: &Path, existing: &[Manifest]) -> Vec<(Choice, &'static Recipe, Outcome)> {
    let mut out = Vec::new();
    let mut recommended_done = false;
    for r in RECIPES {
        let v = check_here(r, root);
        let verdict = match v.outcome {
            Outcome::CanRun => "can run here".to_string(),
            Outcome::Pointless => {
                let why = v.findings.iter().find(|f| !f.pass && f.severity == speccheck::Severity::Pointless).map(|f| f.why.as_str()).unwrap_or("");
                format!("runs here but does not make sense: {why}")
            }
            Outcome::CannotRun => {
                let why = v.findings.iter().find(|f| !f.pass && f.severity == speccheck::Severity::Blocks).map(|f| f.why.as_str()).unwrap_or("");
                format!("cannot run here: {why}")
            }
        };
        let has = existing.iter().any(|m| m.recipe == r.name);
        let label = format!("{}{}", r.name, if has { " (sandbox exists)" } else { "" });
        let mut c = Choice::new(label, format!("{} · {} steps · {verdict}", r.summary, r.tour.len() + 1));
        if !recommended_done && v.outcome == Outcome::CanRun && !has {
            c = c.recommended();
            recommended_done = true;
        }
        out.push((c, r, v.outcome));
    }
    out
}

/// Run the home loop. Returns the sandbox to hand to the agent, if the user asked for that.
pub fn run(root: &Path) -> Result<Option<Manifest>, String> {
    loop {
        let all = list(root)?;
        let (choices, actions) = choices(&all);
        let Some(pick) = select("trybox: what do you want to do?", &choices)? else { return Ok(None) };
        match &actions[pick] {
            Action::Quit => return Ok(None),
            Action::Continue(name) => {
                let m = crate::manifest::load(root, name)?;
                let r = recipe(&m.recipe)?;
                match crate::explore::tour(&m, r)? {
                    crate::explore::Next::Agent => return Ok(Some(m)),
                    _ => continue,
                }
            }
            Action::Dispose(name) => {
                let m = crate::manifest::load(root, name)?;
                if confirm(&format!("Dispose {name} and free {}?", gb(backend::dir_size(&m.dir))), "Yes, dispose it", "No, keep it")? {
                    for n in backend::destroy(&m)? {
                        println!("{n}");
                    }
                }
            }
            Action::StartNew => {
                let options = recipe_choices(root, &all);
                let choices: Vec<Choice> = options.iter().map(|(c, _, _)| c.clone()).collect();
                let Some(pick) = select("Which project do you want to explore?", &choices)? else { continue };
                let (_, r, outcome) = &options[pick];
                match outcome {
                    Outcome::CannotRun => {
                        println!("{}", speccheck::render(&check_here(r, root)));
                        continue;
                    }
                    Outcome::Pointless => {
                        println!("{}", speccheck::render(&check_here(r, root)));
                        if !confirm("Continue anyway?", "Yes, I know it will run on the CPU", "No, go back")? {
                            continue;
                        }
                    }
                    Outcome::CanRun => {}
                }
                let m = crate::explore::ensure_sandbox(root, r, r.name)?;
                if crate::explore::tour(&m, r)? == crate::explore::Next::Agent {
                    return Ok(Some(m));
                }
            }
        }
    }
}
