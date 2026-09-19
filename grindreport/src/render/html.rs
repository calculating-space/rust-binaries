//! Self-contained HTML report: one file, no network, inline SVG charts,
//! light and dark palettes, sortable tables. Meant to be opened locally.

use std::fmt::Write as _;

use crate::analyze::Report;
use crate::util::*;

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(ch),
        }
    }
    o
}

const CSS: &str = r#"
:root {
  color-scheme: light;
  --page: #f9f9f7; --surface: #fcfcfb; --ink: #0b0b0b; --ink-2: #52514e; --muted: #898781;
  --grid: #e1e0d9; --axis: #c3c2b7; --border: rgba(11,11,11,0.10);
  --series-1: #2a78d6; --series-2: #eb6834; --series-3: #1baf7a;
  --critical: #d03b3b; --warning: #fab219;
  --seq-0: transparent; --seq-1: #cde2fb; --seq-2: #9ec5f4; --seq-3: #6da7ec; --seq-4: #3987e5;
  --seq-5: #256abf; --seq-6: #184f95; --seq-7: #0d366b;
}
@media (prefers-color-scheme: dark) {
  :root {
    color-scheme: dark;
    --page: #0d0d0d; --surface: #1a1a19; --ink: #ffffff; --ink-2: #c3c2b7; --muted: #898781;
    --grid: #2c2c2a; --axis: #383835; --border: rgba(255,255,255,0.10);
    --series-1: #3987e5; --series-2: #d95926; --series-3: #199e70;
    --critical: #e66767; --warning: #c98500;
    --seq-1: #0d366b; --seq-2: #104281; --seq-3: #184f95; --seq-4: #256abf;
    --seq-5: #3987e5; --seq-6: #6da7ec; --seq-7: #9ec5f4;
  }
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--page); color: var(--ink); font: 14px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width: 1120px; margin: 0 auto; padding: 24px 20px 64px; }
h1 { font-size: 22px; margin: 0 0 4px; }
h2 { font-size: 16px; margin: 36px 0 10px; padding-top: 12px; border-top: 1px solid var(--grid); }
h3 { font-size: 13px; color: var(--ink-2); font-weight: 600; margin: 18px 0 6px; text-transform: uppercase; letter-spacing: .04em; }
p, li { color: var(--ink-2); }
.meta { color: var(--muted); font-size: 12px; margin: 0 0 4px; }
.tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 10px; margin: 16px 0; }
.tile { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; padding: 10px 12px; }
.tile .v { font-size: 22px; font-weight: 600; color: var(--ink); }
.tile .k { font-size: 12px; color: var(--muted); }
.tile .s { font-size: 12px; color: var(--ink-2); }
.card { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; padding: 12px 14px; margin: 10px 0; overflow-x: auto; }
figure { margin: 0; }
figcaption { font-size: 12px; color: var(--muted); margin-bottom: 6px; }
svg { display: block; max-width: 100%; height: auto; }
svg text { font: 11px system-ui, -apple-system, "Segoe UI", sans-serif; fill: var(--muted); font-variant-numeric: tabular-nums; }
svg text.lbl { fill: var(--ink-2); }
svg text.val { fill: var(--ink-2); }
svg .bar { fill: var(--series-1); }
svg .bar:hover { opacity: .8; }
svg .bar.err { fill: var(--critical); }
svg .grid { stroke: var(--grid); stroke-width: 1; }
svg .axis { stroke: var(--axis); stroke-width: 1; }
svg .cell { stroke: var(--surface); stroke-width: 2; }
svg .cell.z { fill: var(--surface); stroke: var(--grid); stroke-width: 1; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
th, td { text-align: left; padding: 5px 8px; border-bottom: 1px solid var(--grid); white-space: nowrap; vertical-align: top; }
th { color: var(--muted); font-weight: 600; font-size: 12px; cursor: pointer; user-select: none; position: sticky; top: 0; background: var(--surface); }
th:hover { color: var(--ink); }
th.r, td.r { text-align: right; }
td.w { white-space: normal; }
tr:last-child td { border-bottom: 0; }
code { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--ink-2); }
.notes li { margin: 3px 0; }
.legend { font-size: 12px; color: var(--muted); }
.legend i { display: inline-block; width: 10px; height: 10px; border-radius: 2px; vertical-align: -1px; margin-right: 4px; }
.split { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 12px; }
"#;

const JS: &str = r#"
document.querySelectorAll('table.sortable th').forEach(function (th) {
  th.addEventListener('click', function () {
    var table = th.closest('table'), tbody = table.tBodies[0];
    var idx = Array.prototype.indexOf.call(th.parentNode.children, th);
    var dir = th.dataset.dir === 'asc' ? 'desc' : 'asc';
    table.querySelectorAll('th').forEach(function (h) { delete h.dataset.dir; });
    th.dataset.dir = dir;
    var rows = Array.prototype.slice.call(tbody.rows);
    rows.sort(function (a, b) {
      var ca = a.cells[idx], cb = b.cells[idx];
      var na = ca.dataset.n, nb = cb.dataset.n, r;
      if (na !== undefined && nb !== undefined) r = parseFloat(na) - parseFloat(nb);
      else r = ca.textContent.localeCompare(cb.textContent);
      return dir === 'asc' ? r : -r;
    });
    rows.forEach(function (r) { tbody.appendChild(r); });
  });
});
"#;

/// A table cell: display text plus an optional numeric sort key.
pub struct Cell(pub String, pub Option<f64>);

fn n(text: String, v: i64) -> Cell {
    Cell(text, Some(v as f64))
}
fn f(text: String, v: f64) -> Cell {
    Cell(text, Some(v))
}
fn s<S: Into<String>>(text: S) -> Cell {
    Cell(text.into(), None)
}

/// Headers ending in `>` are right-aligned, like `table::Table`.
fn table(headers: &[&str], rows: &[Vec<Cell>]) -> String {
    let mut o = String::from("<table class=\"sortable\"><thead><tr>");
    let aligns: Vec<bool> = headers.iter().map(|h| h.ends_with('>')).collect();
    for (h, right) in headers.iter().zip(&aligns) {
        let h = h.trim_end_matches('>');
        let _ = write!(
            o,
            "<th{}>{}</th>",
            if *right { " class=\"r\"" } else { "" },
            esc(h)
        );
    }
    o.push_str("</tr></thead><tbody>");
    for r in rows {
        o.push_str("<tr>");
        for (c, right) in r.iter().zip(&aligns) {
            let cls = if *right { " class=\"r\"" } else { "" };
            match c.1 {
                Some(v) => {
                    let _ = write!(o, "<td{cls} data-n=\"{v}\">{}</td>", esc(&c.0));
                }
                None => {
                    let _ = write!(o, "<td{cls}>{}</td>", esc(&c.0));
                }
            }
        }
        o.push_str("</tr>");
    }
    o.push_str("</tbody></table>");
    o
}

fn tile(value: &str, key: &str, sub: &str) -> String {
    format!(
        "<div class=\"tile\"><div class=\"v\">{}</div><div class=\"k\">{}</div><div class=\"s\">{}</div></div>",
        esc(value),
        esc(key),
        esc(sub)
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep: String = s.chars().skip(s.chars().count() - (max - 1)).collect();
    format!("…{keep}")
}

/// Horizontal single-series bars with direct value labels.
/// Each item: (label, value, tooltip).
fn hbars(items: &[(String, i64, String)], fmt: &dyn Fn(i64) -> String) -> String {
    if items.is_empty() {
        return String::new();
    }
    let max = items.iter().map(|i| i.1).max().unwrap_or(1).max(1);
    let (label_w, val_w, row_h, w) = (230.0, 70.0, 22.0, 900.0);
    let plot_w = w - label_w - val_w - 8.0;
    let h = row_h * items.len() as f64 + 4.0;
    let mut o = format!("<svg viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\">");
    for (i, (label, v, tip)) in items.iter().enumerate() {
        let y = 2.0 + i as f64 * row_h;
        let bw = (*v as f64 / max as f64 * plot_w).max(if *v > 0 { 2.0 } else { 0.0 });
        let _ = write!(
            o,
            "<text class=\"lbl\" x=\"{}\" y=\"{}\" text-anchor=\"end\">{}</text>",
            label_w - 8.0,
            y + 15.0,
            esc(&truncate(label, 36))
        );
        let _ = write!(
            o,
            "<rect class=\"bar\" x=\"{label_w}\" y=\"{}\" width=\"{bw:.1}\" height=\"{}\" rx=\"3\"><title>{}</title></rect>",
            y + 3.0,
            row_h - 6.0,
            esc(tip)
        );
        let _ = write!(
            o,
            "<text class=\"val\" x=\"{:.1}\" y=\"{}\">{}</text>",
            label_w + bw + 6.0,
            y + 15.0,
            esc(&fmt(*v))
        );
    }
    o.push_str("</svg>");
    o
}

/// Vertical bars per day, single series, with a sparse date axis.
fn day_bars(r: &Report) -> String {
    let days = &r.days;
    if days.is_empty() {
        return String::new();
    }
    let max = days.iter().map(|d| d.cycles).max().unwrap_or(1).max(1);
    let (w, h, pad_l, pad_b, pad_t) = (900.0, 180.0, 36.0, 22.0, 8.0);
    let plot_w = w - pad_l - 8.0;
    let plot_h = h - pad_b - pad_t;
    let slot = plot_w / days.len() as f64;
    let bw = (slot - 2.0).max(1.0);
    let mut o = format!("<svg viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\">");
    for k in 0..=2 {
        let y = pad_t + plot_h * (1.0 - k as f64 / 2.0);
        let v = max as f64 * k as f64 / 2.0;
        let _ = write!(
            o,
            "<line class=\"{}\" x1=\"{pad_l}\" x2=\"{w}\" y1=\"{y:.1}\" y2=\"{y:.1}\"/><text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            if k == 0 { "axis" } else { "grid" },
            pad_l - 6.0,
            y + 4.0,
            v.round() as i64
        );
    }
    let label_every = (days.len() / 8).max(1);
    for (i, d) in days.iter().enumerate() {
        let x = pad_l + i as f64 * slot + 1.0;
        let bh = d.cycles as f64 / max as f64 * plot_h;
        if d.cycles > 0 {
            let _ = write!(
                o,
                "<rect class=\"bar\" x=\"{x:.1}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"{bh:.1}\" rx=\"{}\"><title>{}: {} cycles, {} sessions, {} active, {} tool calls, {} output tokens</title></rect>",
                pad_t + plot_h - bh,
                (bw / 2.0).min(3.0),
                d.date,
                d.cycles,
                d.sessions,
                fmt_ms(d.active_ms),
                fmt_int(d.tool_calls),
                fmt_compact(d.tokens_output)
            );
        } else {
            let _ = write!(
                o,
                "<rect x=\"{x:.1}\" y=\"{}\" width=\"{bw:.1}\" height=\"{plot_h}\" fill=\"transparent\"><title>{}: no activity</title></rect>",
                pad_t, d.date
            );
        }
        let last = i + 1 == days.len();
        if last || (i % label_every == 0 && days.len() - 1 - i > label_every / 2) {
            let anchor = if last { "end" } else { "start" };
            let _ = write!(
                o,
                "<text x=\"{x:.1}\" y=\"{}\" text-anchor=\"{anchor}\">{}</text>",
                h - 6.0,
                &d.date[5..]
            );
        }
    }
    o.push_str("</svg>");
    o
}

fn heatmap(r: &Report) -> String {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let max = super::week_max(r);
    if max == 0 {
        return String::new();
    }
    let (cw, ch, pad_l, pad_t) = (30.0, 22.0, 36.0, 16.0);
    let w = pad_l + 24.0 * cw;
    let h = pad_t + 7.0 * ch;
    let mut o = format!("<svg viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\">");
    for hour in (0..24).step_by(3) {
        let _ = write!(
            o,
            "<text x=\"{:.1}\" y=\"11\">{hour:02}</text>",
            pad_l + hour as f64 * cw + 2.0
        );
    }
    for (d, row) in r.week_hours.iter().enumerate() {
        let y = pad_t + d as f64 * ch;
        let _ = write!(
            o,
            "<text class=\"lbl\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            pad_l - 6.0,
            y + 15.0,
            DAYS[d]
        );
        for (hour, &v) in row.iter().enumerate() {
            let x = pad_l + hour as f64 * cw;
            let level = if v <= 0 {
                0
            } else {
                (((v as f64 / max as f64) * 7.0).ceil() as usize).clamp(1, 7)
            };
            let style = if level == 0 {
                " class=\"cell z\"".to_string()
            } else {
                format!(" class=\"cell\" style=\"fill:var(--seq-{level})\"")
            };
            let _ = write!(
                o,
                "<rect{style} x=\"{x}\" y=\"{y}\" width=\"{cw}\" height=\"{ch}\"><title>{} {hour:02}:00 – {:02}:00: {v} cycles</title></rect>",
                DAYS[d],
                hour + 1
            );
        }
    }
    o.push_str("</svg>");
    o
}

pub fn render(r: &Report) -> String {
    let t = &r.totals;
    let mut o = String::new();
    let _ = write!(
        o,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>grindstone usage report</title><style>{CSS}</style></head><body><main>"
    );
    let _ = write!(
        o,
        "<h1>grindstone usage report</h1><p class=\"meta\">{}</p><p class=\"meta\">dataset <code>{}</code> · {} · schema {} · generated {}</p>",
        esc(&super::coverage_line(r)),
        esc(&r.dataset.dir),
        esc(&r.dataset.generator),
        esc(&r.dataset.schema_version),
        esc(&r.dataset.generated_at)
    );
    let mut filt = Vec::new();
    if let Some(f) = &r.filter.from {
        filt.push(format!("from {f}"));
    }
    if let Some(f) = &r.filter.to {
        filt.push(format!("to {f}"));
    }
    if !r.filter.projects.is_empty() {
        filt.push(format!("project matches {}", r.filter.projects.join(", ")));
    }
    let _ = write!(
        o,
        "<p class=\"meta\">{}</p>",
        esc(&if filt.is_empty() {
            format!("all {} sessions", r.dataset.sessions_in_dataset)
        } else {
            format!(
                "{} of {} sessions: {}",
                r.filter.sessions_selected,
                r.dataset.sessions_in_dataset,
                filt.join(", ")
            )
        })
    );
    if t.sessions == 0 {
        o.push_str("<p>No sessions match the selection.</p></main></body></html>\n");
        return o;
    }

    // Tiles.
    o.push_str("<div class=\"tiles\">");
    o.push_str(&tile(
        &fmt_int(t.sessions),
        "sessions",
        &format!("{} active days", t.active_days),
    ));
    o.push_str(&tile(
        &fmt_int(t.cycles),
        "cycles",
        &format!("{} interrupted", t.interrupted_cycles),
    ));
    o.push_str(&tile(
        &fmt_hours(t.agent_active_ms),
        "agent active",
        &format!("of {} wall", fmt_hours(t.wall_ms)),
    ));
    o.push_str(&tile(
        &fmt_int(t.tool_calls),
        "tool calls",
        &format!(
            "{} errors, {} denied, {} in sidechains",
            fmt_pct(r.friction.tool_error_rate),
            fmt_pct(r.friction.denial_rate),
            fmt_int(t.sidechain_tool_calls)
        ),
    ));
    o.push_str(&tile(
        &fmt_compact(t.tokens.output),
        "output tokens",
        &format!("{} thinking", fmt_compact(t.tokens.thinking)),
    ));
    o.push_str(&tile(
        &fmt_compact(t.tokens.cache_read),
        "cache read tokens",
        &format!("{} hit ratio", fmt_pct(t.tokens.cache_hit_ratio())),
    ));
    o.push_str(&tile(
        &fmt_int(t.files_touched),
        "files touched",
        &format!("{} touches", fmt_int(t.file_touches)),
    ));
    if let Some(c) = &r.cost {
        o.push_str(&tile(
            &fmt_usd(c.total),
            &format!("cost ({})", c.currency),
            &format!("pricing {}", c.pricing_version),
        ));
    }
    o.push_str("</div>");

    // Activity.
    if !r.days.is_empty() {
        o.push_str("<h2>Activity</h2><div class=\"card\"><figure><figcaption>Cycles started per day, local time");
        if let Some(p) = super::peak_day(r) {
            let _ = write!(o, " · busiest day {}", esc(&p));
        }
        o.push_str("</figcaption>");
        o.push_str(&day_bars(r));
        o.push_str("</figure></div>");
        if super::week_max(r) > 0 {
            o.push_str("<div class=\"card\"><figure><figcaption>Cycles by weekday and hour, stronger colour is more</figcaption>");
            o.push_str(&heatmap(r));
            o.push_str("</figure></div>");
        }
    }

    // Cost.
    if let Some(c) = &r.cost {
        let _ = write!(
            o,
            "<h2>Cost</h2><p>{} {} over {} priced API calls, pricing table <code>{}</code>.</p><div class=\"split\">",
            fmt_usd(c.total),
            esc(&c.currency),
            fmt_int(c.priced_calls),
            esc(&c.pricing_version)
        );
        let rows: Vec<Vec<Cell>> = c
            .by_model
            .iter()
            .map(|m| {
                vec![
                    s(m.name.clone()),
                    f(fmt_usd(m.cost), m.cost),
                    n(fmt_int(m.api_calls), m.api_calls),
                ]
            })
            .collect();
        let _ = write!(
            o,
            "<div class=\"card\"><h3>by model</h3>{}</div>",
            table(&["model", "cost>", "api calls>"], &rows)
        );
        let rows: Vec<Vec<Cell>> = c
            .by_project
            .iter()
            .map(|m| {
                vec![
                    s(m.name.clone()),
                    f(fmt_usd(m.cost), m.cost),
                    n(fmt_int(m.api_calls), m.api_calls),
                ]
            })
            .collect();
        let _ = write!(
            o,
            "<div class=\"card\"><h3>by project</h3>{}</div></div>",
            table(&["project", "cost>", "api calls>"], &rows)
        );
    }

    // Projects.
    o.push_str("<h2>Projects</h2><p>Ranked by agent-active time. The label is the working directory.</p><div class=\"card\"><figure><figcaption>Agent-active hours per project</figcaption>");
    let items: Vec<(String, i64, String)> = r
        .projects
        .iter()
        .map(|p| {
            (
                p.label.clone(),
                p.active_ms,
                format!(
                    "{}: {} active over {} sessions, {} cycles, {} tool calls",
                    p.label,
                    fmt_ms(p.active_ms),
                    p.sessions,
                    p.cycles,
                    fmt_int(p.tool_calls)
                ),
            )
        })
        .collect();
    o.push_str(&hbars(&items, &fmt_hours));
    o.push_str("</figure></div><div class=\"card\">");
    let rows: Vec<Vec<Cell>> = r
        .projects
        .iter()
        .map(|p| {
            vec![
                s(p.label.clone()),
                n(fmt_int(p.sessions), p.sessions),
                n(fmt_int(p.cycles), p.cycles),
                n(fmt_hours(p.active_ms), p.active_ms),
                f(fmt_pct(p.share_active), p.share_active),
                n(fmt_int(p.tool_calls), p.tool_calls),
                n(fmt_compact(p.tokens.output), p.tokens.output),
                n(fmt_compact(p.tokens.cache_read), p.tokens.cache_read),
                n(fmt_int(p.tool_errors), p.tool_errors),
                n(fmt_int(p.denials), p.denials),
                n(fmt_int(p.interrupted_cycles), p.interrupted_cycles),
                n(fmt_int(p.files_touched), p.files_touched),
                s(p.last_active.clone().unwrap_or_default()),
            ]
        })
        .collect();
    o.push_str(&table(
        &[
            "project",
            "sessions>",
            "cycles>",
            "active>",
            "share>",
            "tool calls>",
            "out tokens>",
            "cache read>",
            "errors>",
            "denied>",
            "interrupted>",
            "files>",
            "last active",
        ],
        &rows,
    ));
    o.push_str("</div>");

    // Models.
    o.push_str("<h2>Models</h2><div class=\"card\"><figure><figcaption>Output tokens per model</figcaption>");
    let items: Vec<(String, i64, String)> = r
        .models
        .iter()
        .map(|m| {
            (
                m.model.clone(),
                m.tokens.output,
                format!(
                    "{}: {} output tokens over {} calls ({} of output)",
                    m.model,
                    fmt_int(m.tokens.output),
                    fmt_int(m.api_calls),
                    fmt_pct(m.share_output)
                ),
            )
        })
        .collect();
    o.push_str(&hbars(&items, &fmt_compact));
    o.push_str("</figure></div><div class=\"card\">");
    let rows: Vec<Vec<Cell>> = r
        .models
        .iter()
        .map(|m| {
            vec![
                s(m.model.clone()),
                n(fmt_int(m.api_calls), m.api_calls),
                n(fmt_int(m.tokens.input), m.tokens.input),
                n(fmt_int(m.tokens.output), m.tokens.output),
                n(fmt_int(m.tokens.cache_read), m.tokens.cache_read),
                n(fmt_int(m.tokens.cache_write), m.tokens.cache_write),
                n(fmt_int(m.tokens.thinking), m.tokens.thinking),
                f(fmt_pct(m.share_output), m.share_output),
            ]
        })
        .collect();
    o.push_str(&table(
        &[
            "model",
            "api calls>",
            "in>",
            "out>",
            "cache read>",
            "cache write>",
            "thinking>",
            "out share>",
        ],
        &rows,
    ));
    o.push_str("</div>");

    // Tools.
    o.push_str("<h2>Tools</h2><div class=\"card\"><figure><figcaption>Tool calls per tool");
    let cats: Vec<String> = r
        .categories
        .iter()
        .map(|c| format!("{} {}", c.category, fmt_pct(c.share)))
        .collect();
    if !cats.is_empty() {
        let _ = write!(o, " · by category: {}", esc(&cats.join(", ")));
    }
    o.push_str("</figcaption>");
    let items: Vec<(String, i64, String)> = r
        .tools
        .iter()
        .map(|tl| {
            (
                tl.tool.clone(),
                tl.calls,
                format!(
                    "{}: {} calls, {} errors, {} denied, p50 {}",
                    tl.tool,
                    fmt_int(tl.calls),
                    tl.errors,
                    tl.denied,
                    if tl.duration.n > 0 {
                        fmt_ms(tl.duration.p50)
                    } else {
                        "-".into()
                    }
                ),
            )
        })
        .collect();
    o.push_str(&hbars(&items, &fmt_int));
    o.push_str("</figure></div><div class=\"card\">");
    let rows: Vec<Vec<Cell>> = r
        .tools
        .iter()
        .map(|tl| {
            let (p50, p90) = if tl.duration.n > 0 {
                (
                    n(fmt_ms(tl.duration.p50), tl.duration.p50),
                    n(fmt_ms(tl.duration.p90), tl.duration.p90),
                )
            } else {
                (s("-"), s("-"))
            };
            vec![
                s(tl.tool.clone()),
                s(tl.category.clone()),
                n(fmt_int(tl.calls), tl.calls),
                f(fmt_pct(tl.share), tl.share),
                n(fmt_int(tl.errors), tl.errors),
                n(fmt_int(tl.denied), tl.denied),
                n(fmt_int(tl.retries), tl.retries),
                n(fmt_int(tl.sidechain), tl.sidechain),
                p50,
                p90,
                n(fmt_compact(tl.result_bytes), tl.result_bytes),
            ]
        })
        .collect();
    o.push_str(&table(
        &[
            "tool",
            "category",
            "calls>",
            "share>",
            "errors>",
            "denied>",
            "retries>",
            "in sidechains>",
            "p50>",
            "p90>",
            "result bytes>",
        ],
        &rows,
    ));
    o.push_str("</div>");

    // Cycles.
    let c = &r.cycles;
    let kv = |m: &std::collections::BTreeMap<String, i64>| -> String {
        let mut v: Vec<(&String, &i64)> = m.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        if v.is_empty() {
            return "-".into();
        }
        v.iter()
            .map(|(k, n)| format!("{k} {}", fmt_int(**n)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = write!(
        o,
        "<h2>Cycles</h2><p>A cycle is one human prompt and everything until the next. {} cycles: prompt kind {}; ended by {}. Edits in {} cycles, rework in {} ({} of editing cycles); sidechains in {}; interrupted {}.</p><div class=\"card\">",
        fmt_int(c.cycles),
        esc(&kv(&c.prompt_kinds)),
        esc(&kv(&c.ended_by)),
        fmt_int(c.with_edits),
        fmt_int(c.with_rework),
        fmt_pct(r.friction.rework_rate),
        fmt_int(c.with_sidechains),
        fmt_int(c.interrupted)
    );
    let qrow = |name: &str, q: &Quantiles, fm: &dyn Fn(i64) -> String| -> Vec<Cell> {
        if q.n == 0 {
            vec![s(name), n("0".into(), 0), s("-"), s("-"), s("-"), s("-")]
        } else {
            vec![
                s(name),
                n(fmt_int(q.n), q.n),
                n(fm(q.p50), q.p50),
                n(fm(q.p90), q.p90),
                n(fm(q.max), q.max),
                n(fm(q.mean), q.mean),
            ]
        }
    };
    let rows = vec![
        qrow("first response", &c.first_response_ms, &fmt_ms),
        qrow("agent active per cycle", &c.agent_active_ms, &fmt_ms),
        qrow("human dwell (idle excluded)", &c.human_dwell_ms, &fmt_ms),
        qrow("prompt length (chars)", &c.prompt_chars, &fmt_int),
        qrow("tool calls per cycle", &c.tool_calls_per_cycle, &fmt_int),
        qrow("api calls per cycle", &c.api_calls_per_cycle, &fmt_int),
        qrow(
            "output tokens per cycle",
            &c.tokens_output_per_cycle,
            &fmt_int,
        ),
    ];
    o.push_str(&table(
        &["distribution", "n>", "p50>", "p90>", "max>", "mean>"],
        &rows,
    ));
    let _ = write!(
        o,
        "<p class=\"meta\">human dwell excludes {} cycles above the idle threshold of {}</p></div>",
        fmt_int(c.dwell_idle),
        fmt_ms(r.dataset.idle_threshold_ms)
    );

    // Friction.
    let fr = &r.friction;
    o.push_str("<h2>Friction</h2><div class=\"split\"><div class=\"card\">");
    let rows = vec![
        vec![
            s("tool error rate"),
            f(fmt_pct(fr.tool_error_rate), fr.tool_error_rate),
            s("tool calls whose result was an error"),
        ],
        vec![
            s("denial rate"),
            f(fmt_pct(fr.denial_rate), fr.denial_rate),
            s("tool calls the person refused"),
        ],
        vec![
            s("retry rate"),
            f(fmt_pct(fr.retry_rate), fr.retry_rate),
            s("tool calls repeating an earlier call"),
        ],
        vec![
            s("interrupted cycle rate"),
            f(
                fmt_pct(fr.interrupted_cycle_rate),
                fr.interrupted_cycle_rate,
            ),
            s("cycles the person cut short"),
        ],
        vec![
            s("rework rate"),
            f(fmt_pct(fr.rework_rate), fr.rework_rate),
            s("editing cycles that re-edited a file"),
        ],
    ];
    o.push_str(&table(&["signal", "rate>", "meaning"], &rows));
    o.push_str("</div><div class=\"card\"><ul class=\"notes\">");
    if !fr.error_tools.is_empty() {
        let s_: Vec<String> = fr
            .error_tools
            .iter()
            .map(|e| format!("{} {}/{}", e.tool, fmt_int(e.errors), fmt_int(e.calls)))
            .collect();
        let _ = write!(
            o,
            "<li>most errors (errors/calls): {}</li>",
            esc(&s_.join(", "))
        );
    }
    let _ = write!(o, "<li>stop reasons: {}</li>", esc(&kv(&fr.stop_reasons)));
    let _ = write!(o, "<li>effort: {}</li>", esc(&kv(&fr.efforts)));
    let _ = write!(o, "<li>entrypoints: {}</li>", esc(&kv(&fr.entrypoints)));
    let _ = write!(o, "<li>cli versions: {}</li>", esc(&kv(&fr.cli_versions)));
    let _ = write!(o, "<li>file touch ops: {}</li>", esc(&kv(&fr.file_ops)));
    if !fr.file_exts.is_empty() {
        let s_: Vec<String> = fr
            .file_exts
            .iter()
            .map(|(e, n)| format!("{e} {}", fmt_int(*n)))
            .collect();
        let _ = write!(
            o,
            "<li>file touches by extension: {}</li>",
            esc(&s_.join(", "))
        );
    }
    o.push_str("</ul></div></div>");
    if !fr.sessions.is_empty() {
        o.push_str(
            "<div class=\"card\"><h3>sessions with the most errors, denials and interruptions</h3>",
        );
        let rows: Vec<Vec<Cell>> = fr
            .sessions
            .iter()
            .map(|x| {
                vec![
                    s(x.session_id.clone()),
                    s(x.project.clone()),
                    s(x.start.clone()),
                    n(fmt_int(x.tool_errors), x.tool_errors),
                    n(fmt_int(x.denials), x.denials),
                    n(fmt_int(x.interrupted_cycles), x.interrupted_cycles),
                    n(fmt_int(x.cycles), x.cycles),
                ]
            })
            .collect();
        o.push_str(&table(
            &[
                "session",
                "project",
                "start",
                "errors>",
                "denied>",
                "interrupted>",
                "cycles>",
            ],
            &rows,
        ));
        o.push_str("</div>");
    }

    // Sessions.
    let _ = write!(
        o,
        "<h2>Sessions</h2><p>Top {} by agent-active time. Click a header to sort.</p><div class=\"card\">",
        r.sessions.len()
    );
    let rows: Vec<Vec<Cell>> = r
        .sessions
        .iter()
        .map(|x| {
            vec![
                s(x.session_id.clone()),
                s(x.project.clone()),
                s(x.start.clone()),
                n(fmt_ms(x.wall_ms), x.wall_ms),
                n(fmt_ms(x.active_ms), x.active_ms),
                n(fmt_int(x.cycles), x.cycles),
                n(fmt_int(x.tool_calls), x.tool_calls),
                n(fmt_compact(x.tokens_output), x.tokens_output),
                n(fmt_int(x.files_touched), x.files_touched),
                n(fmt_int(x.tool_errors), x.tool_errors),
                n(fmt_int(x.denials), x.denials),
                n(fmt_int(x.interrupted_cycles), x.interrupted_cycles),
                s(x.entrypoint.clone().unwrap_or_default()),
                s(x.git_branch.clone().unwrap_or_default()),
                s(x.models.join(" ")),
                s(if x.is_complete { "" } else { "yes" }),
            ]
        })
        .collect();
    o.push_str(&table(
        &[
            "session",
            "project",
            "start",
            "wall>",
            "active>",
            "cycles>",
            "tool calls>",
            "out tokens>",
            "files>",
            "errors>",
            "denied>",
            "interrupted>",
            "entrypoint",
            "branch",
            "models",
            "live",
        ],
        &rows,
    ));
    o.push_str("</div>");

    // Notes.
    o.push_str("<h2>Notes</h2><ul class=\"notes\">");
    if r.notes.is_empty() {
        o.push_str("<li>none</li>");
    }
    for nt in &r.notes {
        let _ = write!(o, "<li>{}</li>", esc(nt));
    }
    o.push_str("</ul>");
    let _ = write!(
        o,
        "<p class=\"meta\">report_version {} · rendered by grindreport {}</p></main><script>{JS}</script></body></html>",
        r.report_version,
        env!("CARGO_PKG_VERSION")
    );
    o.push('\n');
    o
}
