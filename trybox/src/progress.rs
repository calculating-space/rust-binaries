//! What has been tried in a sandbox. Lives in `<sandbox>/progress.json`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROGRESS_FILE: &str = "progress.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub ok: bool,
    pub at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Progress {
    pub hello: Option<Run>,
    /// Tour step index (0-based) to its most recent run.
    pub steps: BTreeMap<usize, Run>,
    pub agent_sessions: u32,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Progress {
    pub fn load(sandbox_dir: &Path) -> Progress {
        std::fs::read_to_string(sandbox_dir.join(PROGRESS_FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, sandbox_dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(sandbox_dir.join(PROGRESS_FILE), json + "\n")
            .map_err(|e| format!("write progress: {e}"))
    }
    pub fn record(&mut self, step: Option<usize>, ok: bool) {
        let run = Run { ok, at_unix: now() };
        match step {
            None => self.hello = Some(run),
            Some(i) => {
                self.steps.insert(i, run);
            }
        }
    }
    pub fn done(&self, step: usize) -> bool {
        self.steps.get(&step).is_some_and(|r| r.ok)
    }
    pub fn steps_done(&self) -> usize {
        self.steps.values().filter(|r| r.ok).count()
    }
    /// Most recent activity, if any.
    pub fn last_unix(&self) -> Option<u64> {
        self.hello
            .iter()
            .chain(self.steps.values())
            .map(|r| r.at_unix)
            .max()
    }
    /// "not started", "hello world", "hello + 3/10 steps"
    pub fn summary(&self, total_steps: usize) -> String {
        match (self.hello.as_ref().is_some_and(|r| r.ok), self.steps_done()) {
            (false, 0) => "not started".into(),
            (true, 0) => "hello world".into(),
            (_, n) if n >= total_steps => "tour complete".into(),
            (_, n) => format!("hello + {n}/{total_steps} steps"),
        }
    }
}

/// "just now", "5 min ago", "3 h ago", "2 d ago"
pub fn ago(then_unix: u64) -> String {
    let d = now().saturating_sub(then_unix);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86399 => format!("{} h ago", d / 3600),
        _ => format!("{} d ago", d / 86400),
    }
}

pub fn gb(bytes: u64) -> String {
    let g = bytes as f64 / (1u64 << 30) as f64;
    if g < 0.1 {
        format!("{} MB", bytes >> 20)
    } else {
        format!("{g:.1} GB")
    }
}
