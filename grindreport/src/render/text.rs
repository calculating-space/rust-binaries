//! Terminal report: short, aligned, no colour codes.

use super::table::Table;
use super::{coverage_line, duty_cycle, peak_day, week_grid, week_max};
use crate::analyze::Report;
use crate::util::*;

fn flags(errors: i64, denials: i64, interrupted: i64, complete: bool) -> String {
    let mut f = Vec::new();
    if errors > 0 {
        f.push(format!("{errors} err"));
    }
    if denials > 0 {
        f.push(format!("{denials} denied"));
    }
    if interrupted > 0 {
        f.push(format!("{interrupted} intr"));
    }
    if !complete {
        f.push("live".into());
    }
    f.join(", ")
}

pub fn render(r: &Report) -> String {
    let mut o = String::new();
    let t = &r.totals;
    let push = |o: &mut String, s: &str| {
        o.push_str(s);
        o.push('\n');
    };

    push(
        &mut o,
        &format!("grindstone usage report  {}", coverage_line(r)),
    );
    push(
        &mut o,
        &format!(
            "dataset {}  ({}, schema {}, generated {})",
            r.dataset.dir, r.dataset.generator, r.dataset.schema_version, r.dataset.generated_at
        ),
    );
    let mut filt = Vec::new();
    if let Some(f) = &r.filter.from {
        filt.push(format!("from {f}"));
    }
    if let Some(f) = &r.filter.to {
        filt.push(format!("to {f}"));
    }
    if !r.filter.projects.is_empty() {
        filt.push(format!("project ~ {}", r.filter.projects.join(", ")));
    }
    push(
        &mut o,
        &if filt.is_empty() {
            format!("all {} sessions", r.dataset.sessions_in_dataset)
        } else {
            format!(
                "{} of {} sessions: {}",
                r.filter.sessions_selected,
                r.dataset.sessions_in_dataset,
                filt.join(", ")
            )
        },
    );
    if t.sessions == 0 {
        push(&mut o, "\nnothing to report");
        return o;
    }

    push(&mut o, "\nTOTALS");
    let mut tb = Table::new(&["", "", "", ""]);
    tb.row(vec![
        "sessions".into(),
        format!(
            "{}{}",
            fmt_int(t.sessions),
            if t.incomplete_sessions > 0 {
                format!(" ({} live)", t.incomplete_sessions)
            } else {
                String::new()
            }
        ),
        "active days".into(),
        fmt_int(t.active_days),
    ]);
    tb.row(vec![
        "cycles".into(),
        fmt_int(t.cycles),
        "agent active".into(),
        duty_cycle(r),
    ]);
    tb.row(vec![
        "api calls".into(),
        fmt_int(t.api_calls),
        "tool calls".into(),
        format!(
            "{} ({} errors, {} denied, {} in sidechains)",
            fmt_int(t.tool_calls),
            fmt_pct(r.friction.tool_error_rate),
            fmt_pct(r.friction.denial_rate),
            fmt_int(t.sidechain_tool_calls)
        ),
    ]);
    tb.row(vec![
        "files".into(),
        format!("{} touched", fmt_int(t.files_touched)),
        "sidechains".into(),
        fmt_int(t.sidechains),
    ]);
    o.push_str(&tb.text("  "));
    push(
        &mut o,
        &format!(
            "  tokens    out {}  in {}  cache read {}  cache write {}  thinking {}  (cache hit {})",
            fmt_compact(t.tokens.output),
            fmt_compact(t.tokens.input),
            fmt_compact(t.tokens.cache_read),
            fmt_compact(t.tokens.cache_write),
            fmt_compact(t.tokens.thinking),
            fmt_pct(t.tokens.cache_hit_ratio())
        ),
    );

    if let Some(c) = &r.cost {
        push(&mut o, "\nCOST");
        push(
            &mut o,
            &format!(
                "  {} {} over {} priced calls (pricing {})",
                fmt_usd(c.total),
                c.currency,
                fmt_int(c.priced_calls),
                c.pricing_version
            ),
        );
        let mut tb = Table::new(&["model", "cost>", "calls>"]);
        for m in &c.by_model {
            tb.row(vec![m.name.clone(), fmt_usd(m.cost), fmt_int(m.api_calls)]);
        }
        o.push_str(&tb.text("  "));
        let mut tb = Table::new(&["project", "cost>", "calls>"]);
        for p in &c.by_project {
            tb.row(vec![p.name.clone(), fmt_usd(p.cost), fmt_int(p.api_calls)]);
        }
        o.push_str(&tb.text("  "));
    }

    if !r.days.is_empty() {
        push(&mut o, "\nACTIVITY  (cycles started per day, local time)");
        let cycles: Vec<i64> = r.days.iter().map(|d| d.cycles).collect();
        push(
            &mut o,
            &format!(
                "  {}  {}  {}",
                r.days[0].date,
                spark(&cycles),
                r.days[r.days.len() - 1].date
            ),
        );
        if let Some(p) = peak_day(r) {
            push(&mut o, &format!("  busiest day {p}"));
        }
        if week_max(r) > 0 {
            push(&mut o, "  week, cycles by weekday and hour:");
            for l in week_grid(r) {
                push(&mut o, &format!("  {l}"));
            }
        }
    }

    push(&mut o, "\nPROJECTS  (by agent-active time)");
    let mut tb = Table::new(&[
        "project",
        "sessions>",
        "cycles>",
        "active>",
        "share>",
        "tools>",
        "out tokens>",
        "errors>",
        "intr>",
        "last active",
    ]);
    for p in &r.projects {
        tb.row(vec![
            p.label.clone(),
            fmt_int(p.sessions),
            fmt_int(p.cycles),
            fmt_hours(p.active_ms),
            fmt_pct(p.share_active),
            fmt_int(p.tool_calls),
            fmt_compact(p.tokens.output),
            fmt_int(p.tool_errors),
            fmt_int(p.interrupted_cycles),
            p.last_active.clone().unwrap_or_default(),
        ]);
    }
    o.push_str(&tb.text("  "));

    push(&mut o, "\nMODELS");
    let mut tb = Table::new(&[
        "model",
        "calls>",
        "in>",
        "out>",
        "cache read>",
        "cache write>",
        "thinking>",
        "out share>",
    ]);
    for m in &r.models {
        tb.row(vec![
            m.model.clone(),
            fmt_int(m.api_calls),
            fmt_compact(m.tokens.input),
            fmt_compact(m.tokens.output),
            fmt_compact(m.tokens.cache_read),
            fmt_compact(m.tokens.cache_write),
            fmt_compact(m.tokens.thinking),
            fmt_pct(m.share_output),
        ]);
    }
    o.push_str(&tb.text("  "));

    push(&mut o, "\nTOOLS");
    let mut tb = Table::new(&[
        "tool", "category", "calls>", "share>", "errors>", "denied>", "p50>", "p90>",
    ]);
    for tl in &r.tools {
        let (p50, p90) = if tl.duration.n > 0 {
            (fmt_ms(tl.duration.p50), fmt_ms(tl.duration.p90))
        } else {
            ("-".into(), "-".into())
        };
        tb.row(vec![
            tl.tool.clone(),
            tl.category.clone(),
            fmt_int(tl.calls),
            fmt_pct(tl.share),
            fmt_int(tl.errors),
            fmt_int(tl.denied),
            p50,
            p90,
        ]);
    }
    o.push_str(&tb.text("  "));
    let cats: Vec<String> = r
        .categories
        .iter()
        .map(|c| format!("{} {}", c.category, fmt_pct(c.share)))
        .collect();
    if !cats.is_empty() {
        push(&mut o, &format!("  by category: {}", cats.join(" · ")));
    }

    let c = &r.cycles;
    push(
        &mut o,
        "\nCYCLES  (one human prompt and everything until the next)",
    );
    let kv = |m: &std::collections::BTreeMap<String, i64>| -> String {
        let mut v: Vec<(&String, &i64)> = m.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.iter()
            .map(|(k, n)| format!("{k} {}", fmt_int(**n)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    push(
        &mut o,
        &format!(
            "  {} cycles: prompt kind {} · ended by {}",
            fmt_int(c.cycles),
            kv(&c.prompt_kinds),
            kv(&c.ended_by)
        ),
    );
    let q = |name: &str, q: &Quantiles, f: &dyn Fn(i64) -> String| -> String {
        if q.n == 0 {
            format!("{name} -")
        } else {
            format!("{name} p50 {} p90 {} max {}", f(q.p50), f(q.p90), f(q.max))
        }
    };
    push(
        &mut o,
        &format!(
            "  {} · {}",
            q("first response", &c.first_response_ms, &fmt_ms),
            q("agent active", &c.agent_active_ms, &fmt_ms)
        ),
    );
    push(
        &mut o,
        &format!(
            "  {} ({} idle) · {}",
            q("human dwell", &c.human_dwell_ms, &fmt_ms),
            fmt_int(c.dwell_idle),
            q("prompt chars", &c.prompt_chars, &fmt_int)
        ),
    );
    push(
        &mut o,
        &format!(
            "  {} · {} · {}",
            q("tool calls/cycle", &c.tool_calls_per_cycle, &fmt_int),
            q("api calls/cycle", &c.api_calls_per_cycle, &fmt_int),
            q("out tokens/cycle", &c.tokens_output_per_cycle, &fmt_compact)
        ),
    );
    push(
        &mut o,
        &format!(
            "  edits in {} cycles, rework (file edited again) in {} ({} of editing cycles) · sidechains in {} · interrupted {}",
            fmt_int(c.with_edits),
            fmt_int(c.with_rework),
            fmt_pct(r.friction.rework_rate),
            fmt_int(c.with_sidechains),
            fmt_int(c.interrupted)
        ),
    );

    let f = &r.friction;
    push(&mut o, "\nFRICTION");
    push(
        &mut o,
        &format!(
            "  tool errors {} · denied {} · retried {} · interrupted cycles {} · rework {}",
            fmt_pct(f.tool_error_rate),
            fmt_pct(f.denial_rate),
            fmt_pct(f.retry_rate),
            fmt_pct(f.interrupted_cycle_rate),
            fmt_pct(f.rework_rate)
        ),
    );
    if !f.error_tools.is_empty() {
        let s: Vec<String> = f
            .error_tools
            .iter()
            .map(|e| format!("{} {}/{}", e.tool, fmt_int(e.errors), fmt_int(e.calls)))
            .collect();
        push(&mut o, &format!("  most errors: {}", s.join(", ")));
    }
    push(
        &mut o,
        &format!(
            "  stop reasons {} · effort {} · entrypoint {}",
            kv(&f.stop_reasons),
            if f.efforts.is_empty() {
                "-".into()
            } else {
                kv(&f.efforts)
            },
            kv(&f.entrypoints)
        ),
    );
    if !f.file_exts.is_empty() {
        let s: Vec<String> = f
            .file_exts
            .iter()
            .map(|(e, n)| format!("{e} {}", fmt_int(*n)))
            .collect();
        push(
            &mut o,
            &format!("  file touches by extension: {}", s.join(", ")),
        );
    }
    if !f.sessions.is_empty() {
        push(&mut o, "  sessions with the most friction:");
        let mut tb = Table::new(&[
            "session", "project", "start", "errors>", "denied>", "intr>", "cycles>",
        ]);
        for s in &f.sessions {
            tb.row(vec![
                short_id(&s.session_id),
                s.project.clone(),
                s.start.clone(),
                fmt_int(s.tool_errors),
                fmt_int(s.denials),
                fmt_int(s.interrupted_cycles),
                fmt_int(s.cycles),
            ]);
        }
        o.push_str(&tb.text("    "));
    }

    push(&mut o, "\nSESSIONS  (by agent-active time)");
    let mut tb = Table::new(&[
        "session",
        "project",
        "start",
        "wall>",
        "active>",
        "cycles>",
        "tools>",
        "out tokens>",
        "files>",
        "flags",
    ]);
    for s in &r.sessions {
        tb.row(vec![
            short_id(&s.session_id),
            s.project.clone(),
            s.start.clone(),
            fmt_ms(s.wall_ms),
            fmt_ms(s.active_ms),
            fmt_int(s.cycles),
            fmt_int(s.tool_calls),
            fmt_compact(s.tokens_output),
            fmt_int(s.files_touched),
            flags(
                s.tool_errors,
                s.denials,
                s.interrupted_cycles,
                s.is_complete,
            ),
        ]);
    }
    o.push_str(&tb.text("  "));

    if !r.notes.is_empty() {
        push(&mut o, "\nNOTES");
        for n in &r.notes {
            push(&mut o, &format!("  - {n}"));
        }
    }
    o
}
