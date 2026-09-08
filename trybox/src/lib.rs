//! trybox: disposable experiment environments with an agent inside.
//!
//! A sandbox is one directory under the trybox root. Everything the
//! experiment needs (interpreter, packages, model caches, working files,
//! agent briefing) lives inside that directory, so `destroy` is one delete.

pub mod agent;
pub mod backend;
pub mod doctor;
pub mod explore;
pub mod home;
pub mod manifest;
pub mod progress;
pub mod recipe;
pub mod ui;

pub use agent::{agent_command, briefing};
pub use backend::{Backend, PreparePlan, destroy, exec_command, prepare, run_plan};
pub use doctor::{HostInfo, Report as DoctorReport, doctor, host_info};
pub use manifest::{Manifest, list, load};
pub use recipe::{Recipe, recipe};

use std::path::PathBuf;

/// Root directory that holds every sandbox. `TRYBOX_ROOT` overrides `~/.trybox`.
pub fn default_root() -> PathBuf {
    if let Some(root) = std::env::var_os("TRYBOX_ROOT") {
        return PathBuf::from(root);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".trybox")
}

/// Sandbox names become directory names and docker tags; keep them boring.
pub fn validate_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with('-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid sandbox name {name:?}: use [A-Za-z0-9_-], max 64 chars"
        ))
    }
}

/// The `list` view: a short block per sandbox saying what it is, how far you got, what it costs, when.
pub fn overview(all: &[Manifest]) -> String {
    use progress::{Progress, ago, gb};
    if all.is_empty() {
        return "no sandboxes yet\n\n  trybox recipes          see what you can explore\n  trybox explore mlx      start one\n".to_string();
    }
    let mut out = String::new();
    for m in all {
        let p = Progress::load(&m.dir);
        let (summary, what, recipe_name) = match recipe(&m.recipe) {
            Ok(r) => (p.summary(r.tour.len()), r.summary.to_string(), r.name),
            Err(_) => (
                p.summary(0),
                format!("unknown recipe {}", m.recipe),
                m.recipe.as_str(),
            ),
        };
        let last = ago(p.last_unix().unwrap_or(m.created_unix));
        out.push_str(&format!("{}\n", m.name));
        out.push_str(&recipe::wrap(&what, 4, 78));
        out.push_str(&format!(
            "    {summary}  ·  {}  ·  last used {last}\n",
            gb(backend::dir_size(&m.dir))
        ));
        for (cmd, what) in [
            (format!("trybox explore {recipe_name}"), "continue"),
            (format!("trybox status {}", m.name), "details"),
            (format!("trybox dispose {} -y", m.name), "free the disk"),
        ] {
            out.push_str(&format!("    {cmd:<30} {what}\n"));
        }
        out.push('\n');
    }
    out
}

/// The `status` view for one sandbox.
pub fn status(m: &Manifest) -> String {
    use progress::{Progress, ago, gb};
    let p = Progress::load(&m.dir);
    let mut out = format!("{}\n", m.name);
    match recipe(&m.recipe) {
        Ok(r) => {
            out.push_str(&format!(
                "  what      {}\n  progress  {}\n",
                r.summary,
                p.summary(r.tour.len())
            ));
            out.push_str(&format!(
                "  hello     {}\n",
                match &p.hello {
                    Some(run) if run.ok => format!("ok, {}", ago(run.at_unix)),
                    Some(run) => format!("failed, {}", ago(run.at_unix)),
                    None => "not run".into(),
                }
            ));
            for (i, e) in r.tour.iter().enumerate() {
                let mark = match p.steps.get(&i) {
                    Some(run) if run.ok => format!("✓ {}", ago(run.at_unix)),
                    Some(run) => format!("✗ {}", ago(run.at_unix)),
                    None => "·".into(),
                };
                out.push_str(&format!("  {:>2}. {:<44} {mark}\n", i + 1, e.title));
            }
            if p.agent_sessions > 0 {
                out.push_str(&format!("  agent     {} session(s)\n", p.agent_sessions));
            }
        }
        Err(_) => out.push_str(&format!(
            "  recipe    {} (not built into this trybox)\n",
            m.recipe
        )),
    }
    out.push_str(&format!(
        "  size      {} total, {} of it models\n",
        gb(backend::dir_size(&m.dir)),
        gb(backend::dir_size(&m.hf_dir()))
    ));
    out.push_str(&format!("  created   {}\n  host      {}, {} GB\n  where     {}\n  how       {} backend, python {}, {} packages\n", ago(m.created_unix), m.host.chip, m.host.memory_gb, m.dir.display(), m.backend.as_str(), m.python, m.packages.len()));
    out
}
