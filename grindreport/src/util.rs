//! Formatting and small numeric helpers shared by every renderer.

use serde::Serialize;

/// Milliseconds as a short human duration: `320ms`, `45s`, `12m 05s`,
/// `2h 13m`, `3d 4h`.
pub fn fmt_ms(ms: i64) -> String {
    if ms < 0 {
        return format!("-{}", fmt_ms(-ms));
    }
    let s = ms / 1000;
    if ms < 1000 {
        format!("{ms}ms")
    } else if s < 60 {
        format!("{}.{}s", s, (ms % 1000) / 100)
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else if s < 86_400 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d {}h", s / 86_400, (s % 86_400) / 3600)
    }
}

/// Milliseconds as decimal hours with one digit: `12.5h`.
pub fn fmt_hours(ms: i64) -> String {
    format!("{:.1}h", ms as f64 / 3_600_000.0)
}

/// Integer with thousands separators.
pub fn fmt_int(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if neg {
        out.insert(0, '-');
    }
    out
}

/// Compact count: `912`, `34.1k`, `1.2M`, `3.4G`.
pub fn fmt_compact(n: i64) -> String {
    let a = n.abs() as f64;
    let (v, suffix) = if a >= 1e9 {
        (a / 1e9, "G")
    } else if a >= 1e6 {
        (a / 1e6, "M")
    } else if a >= 1e3 {
        (a / 1e3, "k")
    } else {
        return fmt_int(n);
    };
    let s = if v >= 100.0 {
        format!("{v:.0}{suffix}")
    } else {
        format!("{v:.1}{suffix}")
    };
    if n < 0 {
        format!("-{s}")
    } else {
        s
    }
}

pub fn ratio(num: i64, den: i64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

pub fn fmt_pct(r: f64) -> String {
    format!("{:.1}%", r * 100.0)
}

pub fn fmt_usd(v: f64) -> String {
    if v >= 100.0 {
        format!("${}", fmt_int(v.round() as i64))
    } else {
        format!("${v:.2}")
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
pub struct Quantiles {
    pub n: i64,
    pub p50: i64,
    pub p90: i64,
    pub max: i64,
    pub mean: i64,
}

/// Nearest-rank percentiles over a sample. `n == 0` gives all zeros.
pub fn quantiles(mut v: Vec<i64>) -> Quantiles {
    if v.is_empty() {
        return Quantiles::default();
    }
    v.sort_unstable();
    let n = v.len();
    let rank = |p: f64| -> i64 {
        let idx = ((p * n as f64).ceil() as usize).clamp(1, n) - 1;
        v[idx]
    };
    let sum: i128 = v.iter().map(|&x| x as i128).sum();
    Quantiles {
        n: n as i64,
        p50: rank(0.5),
        p90: rank(0.9),
        max: v[n - 1],
        mean: (sum / n as i128) as i64,
    }
}

/// Unicode sparkline, one block per value, scaled to the maximum.
pub fn spark(values: &[i64]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = values.iter().copied().max().unwrap_or(0);
    values
        .iter()
        .map(|&v| {
            if max <= 0 || v <= 0 {
                BARS[0]
            } else {
                let i = ((v as f64 / max as f64) * 7.0).round() as usize;
                BARS[i.min(7)]
            }
        })
        .collect()
}

pub fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Collapse `/Users/<name>` or `/home/<name>` to `~` for display.
pub fn tilde(path: &str) -> String {
    for prefix in ["/Users/", "/home/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let mut it = rest.splitn(2, '/');
            let _user = it.next();
            return match it.next() {
                Some(tail) => format!("~/{tail}"),
                None => "~".to_string(),
            };
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(fmt_ms(320), "320ms");
        assert_eq!(fmt_ms(45_300), "45.3s");
        assert_eq!(fmt_ms(725_000), "12m 05s");
        assert_eq!(fmt_ms(7_980_000), "2h 13m");
        assert_eq!(fmt_ms(273_600_000), "3d 4h");
    }

    #[test]
    fn numbers() {
        assert_eq!(fmt_int(21636), "21,636");
        assert_eq!(fmt_int(-1234567), "-1,234,567");
        assert_eq!(fmt_compact(912), "912");
        assert_eq!(fmt_compact(34_100), "34.1k");
        assert_eq!(fmt_compact(1_234_567), "1.2M");
        assert_eq!(fmt_compact(123_456_789), "123M");
    }

    #[test]
    fn quantile_ranks() {
        let q = quantiles(vec![5, 1, 4, 2, 3, 10, 9, 8, 7, 6]);
        assert_eq!((q.n, q.p50, q.p90, q.max, q.mean), (10, 5, 9, 10, 5));
        assert_eq!(quantiles(vec![]), Quantiles::default());
    }

    #[test]
    fn home_collapses() {
        assert_eq!(tilde("/Users/x/Projects/a"), "~/Projects/a");
        assert_eq!(tilde("/home/x"), "~");
        assert_eq!(tilde("/tmp/z"), "/tmp/z");
    }
}
