//! `sieve install`: add the hooks block to a settings.json, idempotently,
//! never touching another tool's entries. `--dry-run` shows the diff.

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub const MARK: &str = "sieve hook";

/// The hook entries sieve registers. `command` is the absolute hook command
/// prefix, e.g. `/usr/local/bin/sieve hook`.
pub fn entries(command_prefix: &str) -> Vec<(&'static str, Option<&'static str>)> {
    let _ = command_prefix;
    vec![("PostToolUse", Some("Bash"))]
}

fn entry(event_command: &str, matcher: Option<&str>) -> Value {
    let mut m = Map::new();
    if let Some(mt) = matcher {
        m.insert("matcher".into(), json!(mt));
    }
    m.insert(
        "hooks".into(),
        json!([{ "type": "command", "command": event_command, "timeout": 5 }]),
    );
    Value::Object(m)
}

fn is_ours(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hs| {
            hs.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.contains(MARK))
            })
        })
}

/// Settings with sieve's hooks added. Unchanged if already present.
pub fn with_hooks(settings: &Value, command_prefix: &str) -> Value {
    let mut root = settings.as_object().cloned().unwrap_or_default();
    let mut hooks = root
        .get("hooks")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (event, matcher) in entries(command_prefix) {
        let mut list = hooks
            .get(event)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !list.iter().any(is_ours) {
            list.push(entry(&format!("{command_prefix} {event}"), matcher));
        }
        hooks.insert(event.into(), Value::Array(list));
    }
    root.insert("hooks".into(), Value::Object(hooks));
    Value::Object(root)
}

/// Settings with sieve's hooks removed; other entries untouched.
pub fn without_hooks(settings: &Value) -> Value {
    let mut root = settings.as_object().cloned().unwrap_or_default();
    if let Some(Value::Object(mut hooks)) = root.remove("hooks") {
        let events: Vec<String> = hooks.keys().cloned().collect();
        for ev in events {
            if let Some(Value::Array(list)) = hooks.get(&ev) {
                let kept: Vec<Value> = list.iter().filter(|e| !is_ours(e)).cloned().collect();
                if kept.is_empty() {
                    hooks.remove(&ev);
                } else {
                    hooks.insert(ev, Value::Array(kept));
                }
            }
        }
        if !hooks.is_empty() {
            root.insert("hooks".into(), Value::Object(hooks));
        }
    }
    Value::Object(root)
}

pub fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

/// Minimal unified-style line diff (LCS); inputs are small settings files.
pub fn diff(before: &str, after: &str, name: &str) -> String {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = format!("--- {name}\n+++ {name}\n");
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push_str(&format!(" {}\n", a[i]));
            i += 1;
            j += 1;
        } else if j < m && (i >= n || lcs[i][j + 1] > lcs[i + 1][j]) {
            out.push_str(&format!("+{}\n", b[j]));
            j += 1;
        } else {
            out.push_str(&format!("-{}\n", a[i]));
            i += 1;
        }
    }
    out
}

pub fn settings_path(project: bool, cwd: &Path, home_dir: &Path) -> PathBuf {
    if project {
        cwd.join(".claude").join("settings.json")
    } else {
        home_dir.join(".claude").join("settings.json")
    }
}

/// Read a settings file; a missing file is an empty object.
pub fn read_settings(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: not valid JSON: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_once_and_keeps_others() {
        let existing = json!({"permissions": {"allow": ["Bash(ls:*)"]},
            "hooks": {"PostToolUse": [{"matcher": "Edit", "hooks": [{"type": "command", "command": "fmt"}]}]}});
        let once = with_hooks(&existing, "/bin/sieve hook");
        let twice = with_hooks(&once, "/bin/sieve hook");
        assert_eq!(once, twice);
        let list = once["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["matcher"], "Edit");
        assert_eq!(list[1]["matcher"], "Bash");
        assert_eq!(list[1]["hooks"][0]["command"], "/bin/sieve hook PostToolUse");
        assert_eq!(once["permissions"]["allow"][0], "Bash(ls:*)");
        let removed = without_hooks(&once);
        assert_eq!(removed, existing);
    }

    #[test]
    fn remove_drops_empty_hooks_object() {
        let s = with_hooks(&json!({}), "sieve hook");
        assert_eq!(without_hooks(&s), json!({}));
    }

    #[test]
    fn diff_marks_added_lines() {
        let d = diff("a\nb\n", "a\nx\nb\n", "f");
        assert_eq!(d, "--- f\n+++ f\n a\n+x\n b\n");
    }
}
