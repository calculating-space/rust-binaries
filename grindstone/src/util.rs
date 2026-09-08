use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, TimeZone, Utc};
use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Parse an ISO-8601 timestamp to epoch milliseconds (UTC).
pub fn parse_ts_ms(ts: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
}

pub fn ms_to_rfc3339(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| ms.to_string())
}

/// Local-timezone date (YYYY-MM-DD) for a UTC epoch-ms instant; used for Hive partitioning.
pub fn local_date(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub fn local_tz_name() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "unknown".into())
}

/// Parse `--since` values: `30d`, `12h`, `45m`, `90s`, or an absolute
/// `YYYY-MM-DD` / RFC3339 date. Returns a UTC epoch-ms cutoff.
pub fn parse_since(s: &str, now_ms: i64) -> Result<i64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix(['d', 'h', 'm', 's']) {
        if let Ok(n) = num.parse::<i64>() {
            let unit_ms = match s.as_bytes()[s.len() - 1] {
                b'd' => 86_400_000,
                b'h' => 3_600_000,
                b'm' => 60_000,
                _ => 1_000,
            };
            return Ok(now_ms - n * unit_ms);
        }
    }
    if let Some(ms) = parse_ts_ms(s) {
        return Ok(ms);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(Utc.from_utc_datetime(&dt).timestamp_millis());
    }
    Err(anyhow!("cannot parse --since value: {s:?} (expected e.g. 30d, 12h, or YYYY-MM-DD)"))
}
