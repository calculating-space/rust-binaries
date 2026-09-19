//! The one analysis pass. Produces a `Report`, which is also the JSON
//! contract (`report_version`); every renderer reads only this struct.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use crate::model::*;
use crate::pricing::Pricing;
use crate::util::{quantiles, ratio, tilde, Quantiles};

pub const REPORT_VERSION: &str = "1";

#[derive(Debug, Clone)]
pub struct Options {
    /// Where the dataset was read from, for the header.
    pub dataset_dir: String,
    /// Rows kept in ranked tables before folding the rest into "other".
    pub top: usize,
    pub pricing: Option<Pricing>,
    pub filter_from: Option<i64>,
    pub filter_to: Option<i64>,
    pub filter_projects: Vec<String>,
    /// Sessions in the dataset before the filter, for the header.
    pub sessions_in_dataset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub report_version: &'static str,
    pub dataset: DatasetInfo,
    pub filter: FilterInfo,
    pub totals: Totals,
    pub projects: Vec<ProjectRow>,
    pub models: Vec<ModelRow>,
    pub days: Vec<DayRow>,
    /// Cycles started per local weekday (Mon=0) and hour.
    pub week_hours: Vec<Vec<i64>>,
    pub tools: Vec<ToolRow>,
    pub categories: Vec<CategoryRow>,
    pub cycles: CycleShape,
    pub sessions: Vec<SessionSummary>,
    pub friction: Friction,
    pub cost: Option<Cost>,
    /// Caveats a reader must know before trusting a number.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasetInfo {
    pub dir: String,
    pub generator: String,
    pub schema_version: String,
    pub generated_at: String,
    pub local_tz: String,
    pub coverage_start: Option<String>,
    pub coverage_end: Option<String>,
    pub idle_threshold_ms: i64,
    pub include_text: bool,
    pub sessions_in_dataset: usize,
    pub sources: usize,
    pub skipped_lines: u64,
    pub unknown_types: BTreeMap<String, u64>,
    pub incomplete_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilterInfo {
    pub from: Option<String>,
    pub to: Option<String>,
    pub projects: Vec<String>,
    pub sessions_selected: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Tokens {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub thinking: i64,
}

impl Tokens {
    fn add_session(&mut self, s: &Session) {
        self.input += s.tokens_input;
        self.output += s.tokens_output;
        self.cache_read += s.tokens_cache_read;
        self.cache_write += s.tokens_cache_write;
        self.thinking += s.tokens_thinking;
    }
    /// Share of all input-side tokens that were served from cache.
    pub fn cache_hit_ratio(&self) -> f64 {
        ratio(
            self.cache_read,
            self.input + self.cache_read + self.cache_write,
        )
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub sessions: i64,
    pub incomplete_sessions: i64,
    pub cycles: i64,
    pub api_calls: i64,
    /// From the tool_calls table when present (sidechain calls included),
    /// else the session rollups.
    pub tool_calls: i64,
    pub sidechain_tool_calls: i64,
    pub tool_errors: i64,
    pub denials: i64,
    pub retries: i64,
    pub sidechains: i64,
    pub interrupted_cycles: i64,
    pub tokens: Tokens,
    pub wall_ms: i64,
    pub agent_active_ms: i64,
    pub file_touches: i64,
    pub files_touched: i64,
    pub active_days: i64,
    pub first_activity: Option<String>,
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectRow {
    pub slug: String,
    pub label: String,
    pub sessions: i64,
    pub cycles: i64,
    pub tool_calls: i64,
    pub active_ms: i64,
    pub wall_ms: i64,
    pub tokens: Tokens,
    pub tool_errors: i64,
    pub denials: i64,
    pub interrupted_cycles: i64,
    pub files_touched: i64,
    pub last_active: Option<String>,
    pub share_active: f64,
    /// How many projects this row stands for (1, or the folded remainder).
    pub folded: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelRow {
    pub model: String,
    pub api_calls: i64,
    pub tokens: Tokens,
    pub share_output: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DayRow {
    pub date: String,
    pub sessions: i64,
    pub cycles: i64,
    pub active_ms: i64,
    pub tokens_output: i64,
    pub tool_calls: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolRow {
    pub tool: String,
    pub category: String,
    pub calls: i64,
    pub errors: i64,
    pub denied: i64,
    pub interrupted: i64,
    pub retries: i64,
    pub sidechain: i64,
    pub duration: Quantiles,
    pub result_bytes: i64,
    pub share: f64,
    pub folded: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CategoryRow {
    pub category: String,
    pub calls: i64,
    pub errors: i64,
    pub share: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CycleShape {
    pub cycles: i64,
    pub prompt_kinds: BTreeMap<String, i64>,
    pub ended_by: BTreeMap<String, i64>,
    pub interrupted: i64,
    pub with_sidechains: i64,
    pub with_edits: i64,
    pub with_rework: i64,
    pub first_response_ms: Quantiles,
    pub agent_active_ms: Quantiles,
    pub human_dwell_ms: Quantiles,
    /// Cycles whose dwell was above the idle threshold (no dwell recorded).
    pub dwell_idle: i64,
    pub prompt_chars: Quantiles,
    pub tool_calls_per_cycle: Quantiles,
    pub api_calls_per_cycle: Quantiles,
    pub tokens_output_per_cycle: Quantiles,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub project: String,
    pub start: String,
    pub wall_ms: i64,
    pub active_ms: i64,
    pub cycles: i64,
    pub tool_calls: i64,
    pub tokens_output: i64,
    pub tokens_cache_read: i64,
    pub tool_errors: i64,
    pub denials: i64,
    pub interrupted_cycles: i64,
    pub files_touched: i64,
    pub is_complete: bool,
    pub entrypoint: Option<String>,
    pub git_branch: Option<String>,
    pub models: Vec<String>,
    pub overlap_sessions: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Friction {
    pub tool_error_rate: f64,
    pub denial_rate: f64,
    pub interrupted_cycle_rate: f64,
    pub retry_rate: f64,
    pub rework_rate: f64,
    pub error_tools: Vec<ErrorTool>,
    pub sessions: Vec<FrictionSession>,
    pub stop_reasons: BTreeMap<String, i64>,
    pub efforts: BTreeMap<String, i64>,
    pub entrypoints: BTreeMap<String, i64>,
    pub cli_versions: BTreeMap<String, i64>,
    pub file_ops: BTreeMap<String, i64>,
    pub file_exts: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ErrorTool {
    pub tool: String,
    pub errors: i64,
    pub calls: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FrictionSession {
    pub session_id: String,
    pub project: String,
    pub start: String,
    pub tool_errors: i64,
    pub denials: i64,
    pub interrupted_cycles: i64,
    pub cycles: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Cost {
    pub pricing_version: String,
    pub currency: String,
    pub total: f64,
    pub priced_calls: i64,
    pub unpriced_calls: i64,
    pub by_model: Vec<CostRow>,
    pub by_project: Vec<CostRow>,
    pub unpriced_models: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CostRow {
    pub name: String,
    pub cost: f64,
    pub api_calls: i64,
}

/// Local-time helpers bound to the dataset's time zone.
pub struct Clock {
    pub tz: Tz,
}

impl Clock {
    pub fn new(name: &str) -> (Clock, Option<String>) {
        match name.parse::<Tz>() {
            Ok(tz) => (Clock { tz }, None),
            Err(_) => (
                Clock { tz: chrono_tz::UTC },
                Some(format!(
                    "manifest local_tz {name:?} is unknown; local times are shown in UTC"
                )),
            ),
        }
    }
    fn local(&self, ms: i64) -> DateTime<Tz> {
        let utc: DateTime<Utc> = Utc
            .timestamp_millis_opt(ms)
            .single()
            .unwrap_or_else(|| Utc.timestamp_millis_opt(0).unwrap());
        utc.with_timezone(&self.tz)
    }
    pub fn date(&self, ms: i64) -> NaiveDate {
        self.local(ms).date_naive()
    }
    pub fn date_str(&self, ms: i64) -> String {
        self.date(ms).format("%Y-%m-%d").to_string()
    }
    pub fn minute_str(&self, ms: i64) -> String {
        self.local(ms).format("%Y-%m-%d %H:%M").to_string()
    }
    pub fn weekday_hour(&self, ms: i64) -> (usize, usize) {
        let l = self.local(ms);
        (
            l.weekday().num_days_from_monday() as usize,
            l.hour() as usize,
        )
    }
}

pub fn iso_utc(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

fn project_label(slug: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(c) if !c.is_empty() => tilde(c),
        _ => slug.to_string(),
    }
}

pub fn analyze(ds: &Dataset, opts: &Options) -> Report {
    let m = &ds.manifest;
    let (clock, tz_note) = Clock::new(&m.local_tz);
    let mut notes: Vec<String> = Vec::new();
    notes.extend(tz_note);

    // Project label: the most common cwd of the project's sessions.
    let mut cwd_votes: HashMap<&str, BTreeMap<&str, i64>> = HashMap::new();
    for s in &ds.sessions {
        if let Some(c) = s.cwd.as_deref() {
            *cwd_votes
                .entry(&s.project)
                .or_default()
                .entry(c)
                .or_default() += 1;
        }
    }
    let label_of = |slug: &str| -> String {
        let best = cwd_votes.get(slug).and_then(|votes| {
            votes
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(c, _)| *c)
        });
        project_label(slug, best)
    };
    let session_project: HashMap<&str, &str> = ds
        .sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s.project.as_str()))
        .collect();

    // Tool counts per session from the tool_calls table, which includes
    // sidechain calls that the session rollups leave out.
    #[derive(Default, Clone, Copy)]
    struct ToolCounts {
        calls: i64,
        sidechain: i64,
        errors: i64,
        denials: i64,
        retries: i64,
    }
    let mut tool_counts: HashMap<&str, ToolCounts> = HashMap::new();
    for t in &ds.tool_calls {
        let c = tool_counts.entry(&t.session_id).or_default();
        c.calls += 1;
        c.sidechain += t.is_sidechain as i64;
        c.errors += t.is_error as i64;
        c.denials += t.is_denied as i64;
        c.retries += t.is_retry as i64;
    }
    let have_tools = !ds.tool_calls.is_empty();
    let counts_of = |s: &Session| -> ToolCounts {
        if have_tools {
            tool_counts
                .get(s.session_id.as_str())
                .copied()
                .unwrap_or_default()
        } else {
            ToolCounts {
                calls: s.n_tool_calls,
                sidechain: 0,
                errors: s.n_tool_errors,
                denials: s.n_denials,
                retries: s.n_retries,
            }
        }
    };

    // Totals.
    let mut totals = Totals::default();
    for s in &ds.sessions {
        let tc = counts_of(s);
        totals.sessions += 1;
        if !s.is_complete {
            totals.incomplete_sessions += 1;
        }
        totals.cycles += s.n_cycles;
        totals.api_calls += s.n_api_calls;
        totals.tool_calls += tc.calls;
        totals.sidechain_tool_calls += tc.sidechain;
        totals.tool_errors += tc.errors;
        totals.denials += tc.denials;
        totals.retries += tc.retries;
        totals.sidechains += s.n_sidechains;
        totals.interrupted_cycles += s.n_interrupted_cycles;
        totals.tokens.add_session(s);
        totals.wall_ms += s.wall_ms;
        totals.agent_active_ms += s.sum_agent_active_ms;
    }
    totals.file_touches = ds.file_touches.len() as i64;
    totals.files_touched = ds
        .file_touches
        .iter()
        .map(|f| f.path_sha256.as_str())
        .collect::<BTreeSet<_>>()
        .len() as i64;
    if let (Some(first), Some(last)) = (
        ds.sessions.first(),
        ds.sessions.iter().map(|s| s.end_ts).max(),
    ) {
        totals.first_activity = Some(clock.minute_str(first.start_ts));
        totals.last_activity = Some(clock.minute_str(last));
    }

    // Projects.
    let mut by_project: BTreeMap<&str, ProjectRow> = BTreeMap::new();
    let mut project_files: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for f in &ds.file_touches {
        if let Some(p) = session_project.get(f.session_id.as_str()) {
            project_files.entry(p).or_default().insert(&f.path_sha256);
        }
    }
    for s in &ds.sessions {
        let row = by_project.entry(&s.project).or_insert_with(|| ProjectRow {
            slug: s.project.clone(),
            label: label_of(&s.project),
            folded: 1,
            ..Default::default()
        });
        let tc = counts_of(s);
        row.sessions += 1;
        row.cycles += s.n_cycles;
        row.tool_calls += tc.calls;
        row.active_ms += s.sum_agent_active_ms;
        row.wall_ms += s.wall_ms;
        row.tokens.add_session(s);
        row.tool_errors += tc.errors;
        row.denials += tc.denials;
        row.interrupted_cycles += s.n_interrupted_cycles;
        let d = clock.date_str(s.end_ts);
        if row.last_active.as_deref().is_none_or(|x| x < d.as_str()) {
            row.last_active = Some(d);
        }
    }
    for (slug, row) in by_project.iter_mut() {
        row.files_touched = project_files.get(slug).map_or(0, |s| s.len() as i64);
        row.share_active = ratio(row.active_ms, totals.agent_active_ms);
    }
    let mut projects: Vec<ProjectRow> = by_project.into_values().collect();
    projects.sort_by(|a, b| b.active_ms.cmp(&a.active_ms).then(a.slug.cmp(&b.slug)));
    let projects = fold_tail(projects, opts.top, |rest| {
        let mut o = ProjectRow {
            slug: "other".into(),
            label: format!("other ({} projects)", rest.len()),
            folded: rest.len(),
            ..Default::default()
        };
        for r in rest {
            o.sessions += r.sessions;
            o.cycles += r.cycles;
            o.tool_calls += r.tool_calls;
            o.active_ms += r.active_ms;
            o.wall_ms += r.wall_ms;
            o.tokens.input += r.tokens.input;
            o.tokens.output += r.tokens.output;
            o.tokens.cache_read += r.tokens.cache_read;
            o.tokens.cache_write += r.tokens.cache_write;
            o.tokens.thinking += r.tokens.thinking;
            o.tool_errors += r.tool_errors;
            o.denials += r.denials;
            o.interrupted_cycles += r.interrupted_cycles;
            o.files_touched += r.files_touched;
            if r.last_active > o.last_active {
                o.last_active = r.last_active.clone();
            }
        }
        o.share_active = ratio(o.active_ms, totals.agent_active_ms);
        o
    });

    // Models.
    let mut by_model: BTreeMap<&str, ModelRow> = BTreeMap::new();
    let mut output_total = 0;
    for a in &ds.api_calls {
        let r = by_model.entry(&a.model).or_insert_with(|| ModelRow {
            model: a.model.clone(),
            ..Default::default()
        });
        r.api_calls += 1;
        r.tokens.input += a.input_tokens;
        r.tokens.output += a.output_tokens;
        r.tokens.cache_read += a.cache_read_tokens;
        r.tokens.cache_write += a.cache_write_tokens;
        r.tokens.thinking += a.thinking_tokens;
        output_total += a.output_tokens;
    }
    let mut models: Vec<ModelRow> = by_model.into_values().collect();
    for r in models.iter_mut() {
        r.share_output = ratio(r.tokens.output, output_total);
    }
    models.sort_by(|a, b| {
        b.tokens
            .output
            .cmp(&a.tokens.output)
            .then(a.model.cmp(&b.model))
    });

    // Days and week x hour grid, from cycle start times (local).
    let mut by_day: BTreeMap<NaiveDate, DayRow> = BTreeMap::new();
    let mut day_sessions: HashMap<NaiveDate, BTreeSet<&str>> = HashMap::new();
    let mut week_hours = vec![vec![0i64; 24]; 7];
    for c in &ds.cycles {
        let d = clock.date(c.prompt_ts);
        let row = by_day.entry(d).or_default();
        row.cycles += 1;
        row.active_ms += c.agent_active_ms;
        row.tokens_output += c.tokens_output;
        day_sessions.entry(d).or_default().insert(&c.session_id);
        let (wd, h) = clock.weekday_hour(c.prompt_ts);
        week_hours[wd][h] += 1;
    }
    for t in &ds.tool_calls {
        by_day.entry(clock.date(t.ts)).or_default().tool_calls += 1;
    }
    totals.active_days = by_day.values().filter(|d| d.cycles > 0).count() as i64;
    let mut days = Vec::new();
    if let (Some(&first), Some(&last)) = (by_day.keys().next(), by_day.keys().next_back()) {
        let mut d = first;
        while d <= last {
            let mut row = by_day.remove(&d).unwrap_or_default();
            row.date = d.format("%Y-%m-%d").to_string();
            row.sessions = day_sessions.get(&d).map_or(0, |s| s.len() as i64);
            days.push(row);
            d += Duration::days(1);
        }
    }

    // Tools.
    let mut by_tool: BTreeMap<&str, (ToolRow, Vec<i64>)> = BTreeMap::new();
    let mut by_cat: BTreeMap<&str, CategoryRow> = BTreeMap::new();
    for t in &ds.tool_calls {
        let (r, durs) = by_tool.entry(&t.tool_name).or_insert_with(|| {
            (
                ToolRow {
                    tool: t.tool_name.clone(),
                    category: t.tool_category.clone(),
                    folded: 1,
                    ..Default::default()
                },
                Vec::new(),
            )
        });
        r.calls += 1;
        r.errors += t.is_error as i64;
        r.denied += t.is_denied as i64;
        r.interrupted += t.is_interrupted as i64;
        r.retries += t.is_retry as i64;
        r.sidechain += t.is_sidechain as i64;
        r.result_bytes += t.result_bytes;
        if let Some(d) = t.duration_ms {
            durs.push(d);
        }
        let c = by_cat
            .entry(&t.tool_category)
            .or_insert_with(|| CategoryRow {
                category: t.tool_category.clone(),
                ..Default::default()
            });
        c.calls += 1;
        c.errors += t.is_error as i64;
    }
    let n_tool_rows = ds.tool_calls.len() as i64;
    let mut tools: Vec<ToolRow> = by_tool
        .into_values()
        .map(|(mut r, durs)| {
            r.duration = quantiles(durs);
            r.share = ratio(r.calls, n_tool_rows);
            r
        })
        .collect();
    tools.sort_by(|a, b| b.calls.cmp(&a.calls).then(a.tool.cmp(&b.tool)));
    let error_tools: Vec<ErrorTool> = {
        let mut v: Vec<&ToolRow> = tools.iter().filter(|t| t.errors > 0).collect();
        v.sort_by(|a, b| b.errors.cmp(&a.errors).then(a.tool.cmp(&b.tool)));
        v.iter()
            .take(5)
            .map(|t| ErrorTool {
                tool: t.tool.clone(),
                errors: t.errors,
                calls: t.calls,
            })
            .collect()
    };
    let tools = fold_tail(tools, opts.top, |rest| {
        let mut o = ToolRow {
            tool: format!("other ({} tools)", rest.len()),
            category: "-".into(),
            folded: rest.len(),
            ..Default::default()
        };
        for r in rest {
            o.calls += r.calls;
            o.errors += r.errors;
            o.denied += r.denied;
            o.interrupted += r.interrupted;
            o.retries += r.retries;
            o.sidechain += r.sidechain;
            o.result_bytes += r.result_bytes;
        }
        o.share = ratio(o.calls, n_tool_rows);
        o
    });
    let mut categories: Vec<CategoryRow> = by_cat.into_values().collect();
    for c in categories.iter_mut() {
        c.share = ratio(c.calls, n_tool_rows);
    }
    categories.sort_by(|a, b| b.calls.cmp(&a.calls).then(a.category.cmp(&b.category)));

    // Cycle shape.
    let mut shape = CycleShape {
        cycles: ds.cycles.len() as i64,
        ..Default::default()
    };
    let (mut fr, mut act, mut dwell, mut chars, mut tpc, mut apc, mut opc) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    for c in &ds.cycles {
        *shape.prompt_kinds.entry(c.prompt_kind.clone()).or_default() += 1;
        *shape.ended_by.entry(c.ended_by.clone()).or_default() += 1;
        shape.interrupted += c.interrupted as i64;
        shape.with_sidechains += (c.n_sidechains > 0) as i64;
        shape.with_edits += (c.files_touched > 0) as i64;
        shape.with_rework += (c.files_retouched > 0) as i64;
        if let Some(v) = c.first_response_ms {
            fr.push(v);
        }
        act.push(c.agent_active_ms);
        match c.human_dwell_ms {
            Some(v) => dwell.push(v),
            None => shape.dwell_idle += 1,
        }
        chars.push(c.prompt_chars);
        tpc.push(c.n_tool_calls);
        apc.push(c.n_api_calls);
        opc.push(c.tokens_output);
    }
    shape.first_response_ms = quantiles(fr);
    shape.agent_active_ms = quantiles(act);
    shape.human_dwell_ms = quantiles(dwell);
    shape.prompt_chars = quantiles(chars);
    shape.tool_calls_per_cycle = quantiles(tpc);
    shape.api_calls_per_cycle = quantiles(apc);
    shape.tokens_output_per_cycle = quantiles(opc);

    // Sessions, ranked by active time.
    let mut session_files: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for f in &ds.file_touches {
        session_files
            .entry(&f.session_id)
            .or_default()
            .insert(&f.path_sha256);
    }
    let summarize = |s: &Session| {
        let tc = counts_of(s);
        SessionSummary {
            session_id: s.session_id.clone(),
            project: label_of(&s.project),
            start: clock.minute_str(s.start_ts),
            wall_ms: s.wall_ms,
            active_ms: s.sum_agent_active_ms,
            cycles: s.n_cycles,
            tool_calls: tc.calls,
            tokens_output: s.tokens_output,
            tokens_cache_read: s.tokens_cache_read,
            tool_errors: tc.errors,
            denials: tc.denials,
            interrupted_cycles: s.n_interrupted_cycles,
            files_touched: session_files
                .get(s.session_id.as_str())
                .map_or(s.files_touched, |f| f.len() as i64),
            is_complete: s.is_complete,
            entrypoint: s.entrypoint.clone(),
            git_branch: s.git_branch.clone(),
            models: s.models.clone(),
            overlap_sessions: s.overlap_sessions,
        }
    };
    let mut ranked: Vec<&Session> = ds.sessions.iter().collect();
    ranked.sort_by(|a, b| {
        b.sum_agent_active_ms
            .cmp(&a.sum_agent_active_ms)
            .then(a.session_id.cmp(&b.session_id))
    });
    let sessions: Vec<SessionSummary> =
        ranked.iter().take(opts.top).map(|s| summarize(s)).collect();

    // Friction.
    let mut friction = Friction {
        tool_error_rate: ratio(totals.tool_errors, totals.tool_calls),
        denial_rate: ratio(totals.denials, totals.tool_calls),
        interrupted_cycle_rate: ratio(totals.interrupted_cycles, totals.cycles),
        retry_rate: ratio(totals.retries, totals.tool_calls),
        rework_rate: ratio(shape.with_rework, shape.with_edits),
        error_tools,
        ..Default::default()
    };
    let friction_score = |s: &Session| {
        let tc = counts_of(s);
        tc.errors + tc.denials + s.n_interrupted_cycles
    };
    let mut fs: Vec<&Session> = ds
        .sessions
        .iter()
        .filter(|s| friction_score(s) > 0)
        .collect();
    fs.sort_by(|a, b| {
        friction_score(b)
            .cmp(&friction_score(a))
            .then(a.session_id.cmp(&b.session_id))
    });
    friction.sessions = fs
        .iter()
        .take(5)
        .map(|s| {
            let tc = counts_of(s);
            FrictionSession {
                session_id: s.session_id.clone(),
                project: label_of(&s.project),
                start: clock.minute_str(s.start_ts),
                tool_errors: tc.errors,
                denials: tc.denials,
                interrupted_cycles: s.n_interrupted_cycles,
                cycles: s.n_cycles,
            }
        })
        .collect();
    for a in &ds.api_calls {
        *friction
            .stop_reasons
            .entry(a.stop_reason.clone())
            .or_default() += 1;
        if let Some(e) = &a.effort {
            *friction.efforts.entry(e.clone()).or_default() += 1;
        }
    }
    for s in &ds.sessions {
        *friction
            .entrypoints
            .entry(s.entrypoint.clone().unwrap_or_else(|| "unknown".into()))
            .or_default() += 1;
        *friction
            .cli_versions
            .entry(s.cli_version.clone().unwrap_or_else(|| "unknown".into()))
            .or_default() += 1;
    }
    let mut exts: BTreeMap<String, i64> = BTreeMap::new();
    for f in &ds.file_touches {
        *friction.file_ops.entry(f.op.clone()).or_default() += 1;
        *exts
            .entry(f.ext.clone().unwrap_or_else(|| "(none)".into()))
            .or_default() += 1;
    }
    let mut exts: Vec<(String, i64)> = exts.into_iter().collect();
    exts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    exts.truncate(opts.top);
    friction.file_exts = exts;

    // Cost, only with a price table.
    let cost = opts.pricing.as_ref().map(|pricing| {
        let mut c = Cost {
            pricing_version: pricing.version.clone(),
            currency: pricing.currency.clone(),
            ..Default::default()
        };
        let mut by_model: BTreeMap<&str, CostRow> = BTreeMap::new();
        let mut by_project: BTreeMap<&str, CostRow> = BTreeMap::new();
        let mut unpriced: BTreeSet<&str> = BTreeSet::new();
        for a in &ds.api_calls {
            let Some(p) = pricing.price_for(&a.model) else {
                unpriced.insert(&a.model);
                c.unpriced_calls += 1;
                continue;
            };
            let (w5, w1) = if a.cache_write_5m_tokens + a.cache_write_1h_tokens == 0 {
                (a.cache_write_tokens, 0)
            } else {
                (a.cache_write_5m_tokens, a.cache_write_1h_tokens)
            };
            let v = p.cost(a.input_tokens, a.output_tokens, a.cache_read_tokens, w5, w1);
            c.total += v;
            c.priced_calls += 1;
            let r = by_model.entry(&a.model).or_insert_with(|| CostRow {
                name: a.model.clone(),
                ..Default::default()
            });
            r.cost += v;
            r.api_calls += 1;
            let slug = session_project
                .get(a.session_id.as_str())
                .copied()
                .unwrap_or("unknown");
            let r = by_project.entry(slug).or_insert_with(|| CostRow {
                name: label_of(slug),
                ..Default::default()
            });
            r.cost += v;
            r.api_calls += 1;
        }
        let rank = |m: BTreeMap<&str, CostRow>| -> Vec<CostRow> {
            let mut v: Vec<CostRow> = m.into_values().collect();
            v.sort_by(|a, b| {
                b.cost
                    .partial_cmp(&a.cost)
                    .unwrap()
                    .then(a.name.cmp(&b.name))
            });
            v
        };
        c.by_model = rank(by_model);
        c.by_project = rank(by_project);
        c.by_project.truncate(opts.top);
        c.unpriced_models = unpriced.into_iter().map(String::from).collect();
        c
    });

    // Notes.
    if totals.incomplete_sessions > 0 {
        notes.push(format!(
            "{} session(s) were still being written when the dataset was scanned; their numbers are partial",
            totals.incomplete_sessions
        ));
    }
    let skipped: u64 = m.sources.iter().map(|s| s.skipped_lines).sum();
    if skipped > 0 {
        notes.push(format!(
            "{skipped} transcript line(s) could not be parsed and are not counted"
        ));
    }
    if !m.unknown_types.is_empty() {
        let total: u64 = m.unknown_types.values().sum();
        notes.push(format!(
            "{total} entries of {} unknown type(s) were ignored by the scanner ({})",
            m.unknown_types.len(),
            m.unknown_types
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !m.config.include_text {
        notes.push(
            "dataset holds no message text or file paths (scanned without --include-text); files are counted by hash"
                .into(),
        );
    }
    if shape.dwell_idle > 0 {
        notes.push(format!(
            "human dwell excludes {} cycle(s) followed by more than {} of inactivity (idle threshold)",
            shape.dwell_idle,
            crate::util::fmt_ms(m.config.idle_threshold_ms)
        ));
    }
    if let Some(c) = &cost {
        if c.unpriced_calls > 0 {
            notes.push(format!(
                "{} API call(s) to unpriced model(s) are missing from the cost figure: {}",
                c.unpriced_calls,
                c.unpriced_models.join(", ")
            ));
        }
    }
    notes.extend(ds.warnings.iter().cloned());

    Report {
        report_version: REPORT_VERSION,
        dataset: DatasetInfo {
            dir: opts.dataset_dir.clone(),
            generator: format!("{} {}", m.generator.name, m.generator.version),
            schema_version: m.schema_version.clone(),
            generated_at: m.generated_at.clone(),
            local_tz: clock.tz.name().to_string(),
            coverage_start: m.coverage.start.clone(),
            coverage_end: m.coverage.end.clone(),
            idle_threshold_ms: m.config.idle_threshold_ms,
            include_text: m.config.include_text,
            sessions_in_dataset: opts.sessions_in_dataset,
            sources: m.sources.len(),
            skipped_lines: skipped,
            unknown_types: m.unknown_types.clone(),
            incomplete_sessions: m.incomplete_sessions.len(),
        },
        filter: FilterInfo {
            from: opts.filter_from.map(iso_utc),
            to: opts.filter_to.map(iso_utc),
            projects: opts.filter_projects.clone(),
            sessions_selected: ds.sessions.len(),
        },
        totals,
        projects,
        models,
        days,
        week_hours,
        tools,
        categories,
        cycles: shape,
        sessions,
        friction,
        cost,
        notes,
    }
}

/// Keep the first `top` rows and fold the rest into one row built by `fold`.
/// A tail of one row is kept as is; folding it would hide, not summarize.
fn fold_tail<T>(mut rows: Vec<T>, top: usize, fold: impl FnOnce(Vec<T>) -> T) -> Vec<T> {
    if rows.len() <= top + 1 {
        return rows;
    }
    let rest = rows.split_off(top);
    rows.push(fold(rest));
    rows
}
