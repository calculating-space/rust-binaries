//! Renderers over `Report`. Each one is a pure function of the report.

pub mod html;
pub mod markdown;
pub mod table;
pub mod text;

use crate::analyze::Report;
use crate::util::{fmt_hours, fmt_ms, spark};

/// Text lines shared by the text and Markdown renderers.
pub(crate) fn week_grid(r: &Report) -> Vec<String> {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut out = vec!["     0     3     6     9     12    15    18    21".to_string()];
    for (i, row) in r.week_hours.iter().enumerate() {
        out.push(format!("{}  {}", DAYS[i], spark(row)));
    }
    out
}

pub(crate) fn week_max(r: &Report) -> i64 {
    r.week_hours
        .iter()
        .flat_map(|row| row.iter())
        .copied()
        .max()
        .unwrap_or(0)
}

pub(crate) fn peak_day(r: &Report) -> Option<String> {
    r.days
        .iter()
        .max_by(|a, b| a.active_ms.cmp(&b.active_ms).then(b.date.cmp(&a.date)))
        .filter(|d| d.cycles > 0)
        .map(|d| {
            format!(
                "{}: {} cycles, {} active",
                d.date,
                d.cycles,
                fmt_ms(d.active_ms)
            )
        })
}

pub(crate) fn coverage_line(r: &Report) -> String {
    match (&r.totals.first_activity, &r.totals.last_activity) {
        (Some(a), Some(b)) => format!("{} to {} ({})", a, b, r.dataset.local_tz),
        _ => "no sessions selected".to_string(),
    }
}

pub(crate) fn duty_cycle(r: &Report) -> String {
    let t = &r.totals;
    if t.wall_ms == 0 {
        return "0h".into();
    }
    format!(
        "{} of {} wall ({:.0}%)",
        fmt_hours(t.agent_active_ms),
        fmt_hours(t.wall_ms),
        100.0 * t.agent_active_ms as f64 / t.wall_ms as f64
    )
}
