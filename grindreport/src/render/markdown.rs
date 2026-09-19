//! Markdown digest for agents: stable headings, GitHub tables, explicit
//! units, and a short glossary so a reader who has never seen grindstone
//! can use the numbers without guessing.

use super::table::Table;
use super::{coverage_line, duty_cycle, peak_day, week_grid, week_max};
use crate::analyze::Report;
use crate::util::*;

fn kv(m: &std::collections::BTreeMap<String, i64>) -> String {
    let mut v: Vec<(&String, &i64)> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    if v.is_empty() {
        return "-".into();
    }
    v.iter()
        .map(|(k, n)| format!("{k} {}", fmt_int(**n)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn q_row(tb: &mut Table, name: &str, q: &Quantiles, f: &dyn Fn(i64) -> String) {
    if q.n == 0 {
        tb.row(vec![
            name.to_string(),
            "0".into(),
            "-".into(),
            "-".into(),
            "-".into(),
            "-".into(),
        ]);
    } else {
        tb.row(vec![
            name.to_string(),
            fmt_int(q.n),
            f(q.p50),
            f(q.p90),
            f(q.max),
            f(q.mean),
        ]);
    }
}

pub fn render(r: &Report) -> String {
    let mut o = String::new();
    let t = &r.totals;
    let p = |o: &mut String, s: &str| {
        o.push_str(s);
        o.push('\n');
    };

    p(&mut o, "# grindstone usage report");
    p(&mut o, "");
    p(&mut o, &format!("- period: {}", coverage_line(r)));
    p(
        &mut o,
        &format!(
            "- dataset: `{}` ({}, schema {}, generated {})",
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
        filt.push(format!("project matches {}", r.filter.projects.join(", ")));
    }
    p(
        &mut o,
        &if filt.is_empty() {
            format!(
                "- selection: all {} sessions",
                r.dataset.sessions_in_dataset
            )
        } else {
            format!(
                "- selection: {} of {} sessions ({})",
                r.filter.sessions_selected,
                r.dataset.sessions_in_dataset,
                filt.join(", ")
            )
        },
    );
    p(
        &mut o,
        &format!(
            "- report_version: {} (`--format json` gives the same data as a document)",
            r.report_version
        ),
    );
    p(&mut o, "");
    p(&mut o, "## How to read this");
    p(&mut o, "");
    p(&mut o, "- A **session** is one Claude Code transcript file. A **cycle** is one human prompt and everything the agent did until the next prompt.");
    p(&mut o, "- **agent active** is prompt-to-last-event time summed over cycles; **wall** is first-to-last event of a session. Their ratio is the duty cycle. Both sum across parallel sessions, so a day can hold more than 24 hours.");
    p(&mut o, "- **human dwell** is the gap between a cycle's end and the next prompt; gaps above the idle threshold are excluded and counted as idle.");
    p(&mut o, "- **rework** means a file edited in a cycle was edited again two or more events later in the same session.");
    p(&mut o, "- **sidechains** are subagent transcripts attached to the cycle that spawned them. Their tool calls are included in tool counts.");
    p(&mut o, "- Token columns are raw API usage: `in` uncached input, `out` output (thinking included), `cache read`/`cache write` prompt-cache traffic.");
    p(&mut o, "- Times are local to the dataset's time zone; p50/p90 are nearest-rank percentiles; `k`/`M` mean thousands/millions.");
    p(&mut o, "");

    if t.sessions == 0 {
        p(&mut o, "## Totals");
        p(&mut o, "");
        p(&mut o, "No sessions match the selection.");
        return o;
    }

    p(&mut o, "## Totals");
    p(&mut o, "");
    let mut tb = Table::new(&["metric", "value>"]);
    tb.row(vec!["sessions".to_string(), fmt_int(t.sessions)]);
    tb.row(vec![
        "sessions still live at scan".to_string(),
        fmt_int(t.incomplete_sessions),
    ]);
    tb.row(vec!["active days".to_string(), fmt_int(t.active_days)]);
    tb.row(vec!["cycles".to_string(), fmt_int(t.cycles)]);
    tb.row(vec!["api calls".to_string(), fmt_int(t.api_calls)]);
    tb.row(vec!["tool calls".to_string(), fmt_int(t.tool_calls)]);
    tb.row(vec![
        "tool calls in sidechains".to_string(),
        fmt_int(t.sidechain_tool_calls),
    ]);
    tb.row(vec!["tool errors".to_string(), fmt_int(t.tool_errors)]);
    tb.row(vec!["tool denials".to_string(), fmt_int(t.denials)]);
    tb.row(vec!["tool retries".to_string(), fmt_int(t.retries)]);
    tb.row(vec!["sidechains".to_string(), fmt_int(t.sidechains)]);
    tb.row(vec![
        "interrupted cycles".to_string(),
        fmt_int(t.interrupted_cycles),
    ]);
    tb.row(vec!["agent active".to_string(), duty_cycle(r)]);
    tb.row(vec![
        "files touched (distinct)".to_string(),
        fmt_int(t.files_touched),
    ]);
    tb.row(vec!["file touches".to_string(), fmt_int(t.file_touches)]);
    tb.row(vec!["tokens in".to_string(), fmt_int(t.tokens.input)]);
    tb.row(vec!["tokens out".to_string(), fmt_int(t.tokens.output)]);
    tb.row(vec![
        "tokens cache read".to_string(),
        fmt_int(t.tokens.cache_read),
    ]);
    tb.row(vec![
        "tokens cache write".to_string(),
        fmt_int(t.tokens.cache_write),
    ]);
    tb.row(vec![
        "tokens thinking".to_string(),
        fmt_int(t.tokens.thinking),
    ]);
    tb.row(vec![
        "cache hit ratio".to_string(),
        fmt_pct(t.tokens.cache_hit_ratio()),
    ]);
    o.push_str(&tb.markdown());
    p(&mut o, "");

    if let Some(c) = &r.cost {
        p(&mut o, "## Cost");
        p(&mut o, "");
        p(
            &mut o,
            &format!(
                "Total **{} {}** over {} priced API calls, pricing table `{}`.",
                fmt_usd(c.total),
                c.currency,
                fmt_int(c.priced_calls),
                c.pricing_version
            ),
        );
        p(&mut o, "");
        let mut tb = Table::new(&["model", "cost>", "api calls>"]);
        for m in &c.by_model {
            tb.row(vec![m.name.clone(), fmt_usd(m.cost), fmt_int(m.api_calls)]);
        }
        o.push_str(&tb.markdown());
        p(&mut o, "");
        let mut tb = Table::new(&["project", "cost>", "api calls>"]);
        for pr in &c.by_project {
            tb.row(vec![
                pr.name.clone(),
                fmt_usd(pr.cost),
                fmt_int(pr.api_calls),
            ]);
        }
        o.push_str(&tb.markdown());
        p(&mut o, "");
    }

    if !r.days.is_empty() {
        p(&mut o, "## Activity over time");
        p(&mut o, "");
        let cycles: Vec<i64> = r.days.iter().map(|d| d.cycles).collect();
        p(
            &mut o,
            &format!(
                "Cycles started per day, {} to {}: `{}`",
                r.days[0].date,
                r.days[r.days.len() - 1].date,
                spark(&cycles)
            ),
        );
        if let Some(pk) = peak_day(r) {
            p(&mut o, &format!("Busiest day {pk}."));
        }
        p(&mut o, "");
        let mut tb = Table::new(&[
            "date",
            "sessions>",
            "cycles>",
            "active>",
            "tool calls>",
            "out tokens>",
        ]);
        for d in r.days.iter().filter(|d| d.cycles > 0 || d.tool_calls > 0) {
            tb.row(vec![
                d.date.clone(),
                fmt_int(d.sessions),
                fmt_int(d.cycles),
                fmt_ms(d.active_ms),
                fmt_int(d.tool_calls),
                fmt_compact(d.tokens_output),
            ]);
        }
        o.push_str(&tb.markdown());
        p(&mut o, "");
        if week_max(r) > 0 {
            p(&mut o, "Cycles by weekday and local hour (taller is more):");
            p(&mut o, "");
            p(&mut o, "```");
            for l in week_grid(r) {
                p(&mut o, &l);
            }
            p(&mut o, "```");
            p(&mut o, "");
        }
    }

    p(&mut o, "## Projects");
    p(&mut o, "");
    p(&mut o, "Ranked by agent-active time. The label is the working directory; `other` folds the remaining projects.");
    p(&mut o, "");
    let mut tb = Table::new(&[
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
    ]);
    for pr in &r.projects {
        tb.row(vec![
            pr.label.clone(),
            fmt_int(pr.sessions),
            fmt_int(pr.cycles),
            fmt_hours(pr.active_ms),
            fmt_pct(pr.share_active),
            fmt_int(pr.tool_calls),
            fmt_compact(pr.tokens.output),
            fmt_compact(pr.tokens.cache_read),
            fmt_int(pr.tool_errors),
            fmt_int(pr.denials),
            fmt_int(pr.interrupted_cycles),
            fmt_int(pr.files_touched),
            pr.last_active.clone().unwrap_or_default(),
        ]);
    }
    o.push_str(&tb.markdown());
    p(&mut o, "");

    p(&mut o, "## Models");
    p(&mut o, "");
    let mut tb = Table::new(&[
        "model",
        "api calls>",
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
            fmt_int(m.tokens.input),
            fmt_int(m.tokens.output),
            fmt_int(m.tokens.cache_read),
            fmt_int(m.tokens.cache_write),
            fmt_int(m.tokens.thinking),
            fmt_pct(m.share_output),
        ]);
    }
    o.push_str(&tb.markdown());
    p(&mut o, "");

    p(&mut o, "## Tools");
    p(&mut o, "");
    let mut tb = Table::new(&[
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
            fmt_int(tl.retries),
            fmt_int(tl.sidechain),
            p50,
            p90,
            fmt_compact(tl.result_bytes),
        ]);
    }
    o.push_str(&tb.markdown());
    p(&mut o, "");
    if !r.categories.is_empty() {
        let mut tb = Table::new(&["category", "calls>", "share>", "errors>"]);
        for c in &r.categories {
            tb.row(vec![
                c.category.clone(),
                fmt_int(c.calls),
                fmt_pct(c.share),
                fmt_int(c.errors),
            ]);
        }
        o.push_str(&tb.markdown());
        p(&mut o, "");
    }

    let c = &r.cycles;
    p(&mut o, "## Cycles");
    p(&mut o, "");
    p(&mut o, &format!("- cycles: {}", fmt_int(c.cycles)));
    p(&mut o, &format!("- prompt kind: {}", kv(&c.prompt_kinds)));
    p(&mut o, &format!("- ended by: {}", kv(&c.ended_by)));
    p(
        &mut o,
        &format!(
            "- cycles with file edits: {}; with rework: {} ({} of editing cycles)",
            fmt_int(c.with_edits),
            fmt_int(c.with_rework),
            fmt_pct(r.friction.rework_rate)
        ),
    );
    p(
        &mut o,
        &format!(
            "- cycles with sidechains: {}; interrupted: {}",
            fmt_int(c.with_sidechains),
            fmt_int(c.interrupted)
        ),
    );
    p(&mut o, "");
    let mut tb = Table::new(&["distribution", "n>", "p50>", "p90>", "max>", "mean>"]);
    q_row(&mut tb, "first response", &c.first_response_ms, &fmt_ms);
    q_row(
        &mut tb,
        "agent active per cycle",
        &c.agent_active_ms,
        &fmt_ms,
    );
    q_row(&mut tb, "human dwell", &c.human_dwell_ms, &fmt_ms);
    q_row(&mut tb, "prompt length (chars)", &c.prompt_chars, &fmt_int);
    q_row(
        &mut tb,
        "tool calls per cycle",
        &c.tool_calls_per_cycle,
        &fmt_int,
    );
    q_row(
        &mut tb,
        "api calls per cycle",
        &c.api_calls_per_cycle,
        &fmt_int,
    );
    q_row(
        &mut tb,
        "output tokens per cycle",
        &c.tokens_output_per_cycle,
        &fmt_int,
    );
    o.push_str(&tb.markdown());
    p(
        &mut o,
        &format!(
            "\nHuman dwell excludes {} cycle(s) above the idle threshold.",
            fmt_int(c.dwell_idle)
        ),
    );
    p(&mut o, "");

    let f = &r.friction;
    p(&mut o, "## Friction");
    p(&mut o, "");
    let mut tb = Table::new(&["signal", "rate>", "meaning"]);
    tb.row(vec![
        "tool error rate".to_string(),
        fmt_pct(f.tool_error_rate),
        "tool calls whose result was an error".into(),
    ]);
    tb.row(vec![
        "denial rate".to_string(),
        fmt_pct(f.denial_rate),
        "tool calls the person refused".into(),
    ]);
    tb.row(vec![
        "retry rate".to_string(),
        fmt_pct(f.retry_rate),
        "tool calls repeating an earlier call".into(),
    ]);
    tb.row(vec![
        "interrupted cycle rate".to_string(),
        fmt_pct(f.interrupted_cycle_rate),
        "cycles the person cut short".into(),
    ]);
    tb.row(vec![
        "rework rate".to_string(),
        fmt_pct(f.rework_rate),
        "editing cycles that re-edited a file".into(),
    ]);
    o.push_str(&tb.markdown());
    p(&mut o, "");
    if !f.error_tools.is_empty() {
        let s: Vec<String> = f
            .error_tools
            .iter()
            .map(|e| format!("{} {}/{}", e.tool, fmt_int(e.errors), fmt_int(e.calls)))
            .collect();
        p(
            &mut o,
            &format!("- most errors (errors/calls): {}", s.join(", ")),
        );
    }
    p(&mut o, &format!("- stop reasons: {}", kv(&f.stop_reasons)));
    p(&mut o, &format!("- effort: {}", kv(&f.efforts)));
    p(&mut o, &format!("- entrypoints: {}", kv(&f.entrypoints)));
    p(&mut o, &format!("- cli versions: {}", kv(&f.cli_versions)));
    p(&mut o, &format!("- file touch ops: {}", kv(&f.file_ops)));
    if !f.file_exts.is_empty() {
        let s: Vec<String> = f
            .file_exts
            .iter()
            .map(|(e, n)| format!("{e} {}", fmt_int(*n)))
            .collect();
        p(
            &mut o,
            &format!("- file touches by extension: {}", s.join(", ")),
        );
    }
    p(&mut o, "");
    if !f.sessions.is_empty() {
        p(
            &mut o,
            "Sessions with the most errors, denials and interruptions:",
        );
        p(&mut o, "");
        let mut tb = Table::new(&[
            "session",
            "project",
            "start",
            "errors>",
            "denied>",
            "interrupted>",
            "cycles>",
        ]);
        for s in &f.sessions {
            tb.row(vec![
                s.session_id.clone(),
                s.project.clone(),
                s.start.clone(),
                fmt_int(s.tool_errors),
                fmt_int(s.denials),
                fmt_int(s.interrupted_cycles),
                fmt_int(s.cycles),
            ]);
        }
        o.push_str(&tb.markdown());
        p(&mut o, "");
    }

    p(&mut o, "## Sessions");
    p(&mut o, "");
    p(&mut o, &format!("Top {} by agent-active time. Full ids, so a transcript can be found under `~/.claude/projects/<project>/<session>.jsonl`.", r.sessions.len()));
    p(&mut o, "");
    let mut tb = Table::new(&[
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
    ]);
    for s in &r.sessions {
        tb.row(vec![
            s.session_id.clone(),
            s.project.clone(),
            s.start.clone(),
            fmt_ms(s.wall_ms),
            fmt_ms(s.active_ms),
            fmt_int(s.cycles),
            fmt_int(s.tool_calls),
            fmt_compact(s.tokens_output),
            fmt_int(s.files_touched),
            fmt_int(s.tool_errors),
            fmt_int(s.denials),
            fmt_int(s.interrupted_cycles),
            s.entrypoint.clone().unwrap_or_default(),
            s.git_branch.clone().unwrap_or_default(),
            s.models.join(" "),
            if s.is_complete { "" } else { "yes" }.into(),
        ]);
    }
    o.push_str(&tb.markdown());
    p(&mut o, "");

    p(&mut o, "## Notes");
    p(&mut o, "");
    if r.notes.is_empty() {
        p(&mut o, "- none");
    }
    for n in &r.notes {
        p(&mut o, &format!("- {n}"));
    }
    o
}
