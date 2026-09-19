//! One JSON line per decision, no text from the session: counts, bytes,
//! rule ids. `~/.sieve/ledger/<session_id>.jsonl`.

use serde_json::{Map, Value, json};
use std::path::Path;

pub const VERSION: &str = "1";

/// Append one line; failures are ignored (the ledger never blocks a hook).
pub fn append(home: &Path, session_id: &str, line: &Map<String, Value>) {
    let dir = home.join("ledger");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let name: String = session_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let path = dir.join(format!("{name}.jsonl"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(f, "{}", Value::Object(line.clone()));
    }
}

/// The fields every line starts with.
pub fn base(input: &Value, ts: &str, event: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("v".into(), json!(VERSION));
    m.insert("ts".into(), json!(ts));
    for k in ["session_id", "prompt_id", "agent_id"] {
        m.insert(k.into(), input.get(k).cloned().unwrap_or(Value::Null));
    }
    m.insert("event".into(), json!(event));
    m
}

/// RFC 3339 UTC timestamp with milliseconds, without a chrono dependency.
pub fn now_rfc3339() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let millis = d.subsec_millis();
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, mo, da) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{da:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
        assert_eq!(civil_from_days(20_704), (2026, 9, 8));
    }

    #[test]
    fn timestamp_shape() {
        let t = now_rfc3339();
        assert_eq!(t.len(), 24);
        assert!(t.ends_with('Z'));
    }
}
