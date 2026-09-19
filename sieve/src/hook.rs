//! `sieve hook <event>`: decide from hook JSON on stdin, answer on stdout.
//! Only PostToolUse for Bash decides anything today; every other event is
//! a silent exit 0.

use crate::ledger;
use crate::policy::Policy;
use crate::{Bounds, SpillNote, fold_text};
use serde_json::{Map, Value, json};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const RULE_BASH_SPILL: &str = "bash-spill";

/// What the hook decided: JSON to print (if any) and the ledger line.
#[derive(Debug, Default)]
pub struct Outcome {
    pub output: Option<Value>,
    pub ledger_line: Option<Map<String, Value>>,
}

/// Dispatch one event. Never fails: anything odd yields an empty outcome.
pub fn decide(event: &str, input: &Value, policy: &Policy, home: &Path, ts: &str) -> Outcome {
    if !policy.enabled {
        return Outcome::default();
    }
    match event {
        "PostToolUse" => post_tool_use(input, policy, home, ts),
        _ => Outcome::default(),
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn post_tool_use(input: &Value, policy: &Policy, home: &Path, ts: &str) -> Outcome {
    if str_field(input, "tool_name") != "Bash" {
        return Outcome::default();
    }
    let Some(response) = input.get("tool_response").and_then(Value::as_object) else {
        return Outcome::default();
    };
    let stdout = str_field(input.get("tool_response").unwrap(), "stdout");
    let stderr = str_field(input.get("tool_response").unwrap(), "stderr");
    let persisted_path = response.get("persistedOutputPath").and_then(Value::as_str).map(PathBuf::from);
    let persisted_size = response.get("persistedOutputSize").and_then(Value::as_u64);
    let total_out = persisted_size.unwrap_or(stdout.len() as u64);
    let bounds = policy.bash;

    let mut line = ledger::base(input, ts, "PostToolUse");
    line.insert("tool".into(), json!("Bash"));
    line.insert("rule".into(), json!(RULE_BASH_SPILL));
    line.insert("tool_use_id".into(), input.get("tool_use_id").cloned().unwrap_or(Value::Null));
    line.insert("output_bytes".into(), json!(total_out));
    line.insert("stderr_bytes".into(), json!(stderr.len()));
    line.insert("persisted".into(), json!(persisted_path.is_some()));

    if total_out <= bounds.cap as u64 && stderr.len() <= bounds.cap {
        line.insert("action".into(), json!("observe"));
        line.insert("shown_bytes".into(), json!(total_out));
        return Outcome { output: None, ledger_line: Some(line) };
    }

    let session = str_field(input, "session_id");
    let id = str_field(input, "tool_use_id");
    let spill_dir = home.join("spill").join(sanitize(session));
    let id = if id.is_empty() { "unknown".to_string() } else { sanitize(id) };

    let mut new_response = response.clone();
    new_response.remove("persistedOutputPath");
    new_response.remove("persistedOutputSize");
    let mut shown = total_out;

    if total_out > bounds.cap as u64 {
        let folded = match &persisted_path {
            Some(p) => {
                // The full output is already on disk; only the tail is missing.
                let tail = read_tail(p, bounds.tail);
                match tail {
                    Some(t) => fold_text(stdout, &t, total_out, bounds, SpillNote::Path(p)),
                    None => fold_text(stdout, stdout, total_out, bounds, SpillNote::Failed("persisted output file unreadable")),
                }
            }
            None => {
                let path = spill_dir.join(format!("{id}.out"));
                match write_spill(&path, stdout.as_bytes()) {
                    Ok(()) => fold_text(stdout, stdout, total_out, bounds, SpillNote::Path(&path)),
                    Err(e) => fold_text(stdout, stdout, total_out, bounds, SpillNote::Failed(&e)),
                }
            }
        };
        if let Some(f) = folded {
            shown = f.shown;
            new_response.insert("stdout".into(), Value::String(f.text));
        }
    }
    if stderr.len() > bounds.cap {
        let path = spill_dir.join(format!("{id}.err"));
        let folded = match write_spill(&path, stderr.as_bytes()) {
            Ok(()) => fold_text(stderr, stderr, stderr.len() as u64, bounds, SpillNote::Path(&path)),
            Err(e) => fold_text(stderr, stderr, stderr.len() as u64, bounds, SpillNote::Failed(&e)),
        };
        if let Some(f) = folded {
            new_response.insert("stderr".into(), Value::String(f.text));
        }
    }

    line.insert("action".into(), json!("rewrite"));
    line.insert("shown_bytes".into(), json!(shown));
    line.insert("spilled_bytes".into(), json!(total_out.saturating_sub(shown)));
    Outcome {
        output: Some(json!({
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "updatedToolOutput": Value::Object(new_response),
            }
        })),
        ledger_line: Some(line),
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c == '/' || c == '\\' || c == '\0' { '_' } else { c })
        .collect()
}

fn write_spill(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// The last `n` bytes of a file as text (lossy), or None if unreadable.
fn read_tail(path: &Path, n: usize) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(n as u64);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Default bounds, exposed for tests and the README.
pub fn default_bounds() -> Bounds {
    Bounds::default()
}
