//! Sessionizer: turns one transcript JSONL file into fact rows.
//!
//! Implements the prompt-cycle model from spec §2: a cycle is one human prompt
//! (`user` entry with a fresh `promptId`, `isMeta = false`, no `tool_result`
//! blocks) plus everything until the next human prompt. Sidechain traffic
//! attaches to the enclosing cycle but is tallied separately.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::model::{self, RawEntry, KNOWN_TYPES};
use crate::util::sha256_hex;

pub const IDLE_THRESHOLD_MS: i64 = 30 * 60 * 1000;
/// The CLI enqueues/dequeues *every* prompt internally (observed passthrough
/// p99 ≈ 88ms); only a dequeue whose matching enqueue waited longer than this
/// means the prompt actually sat in the queue behind agent work.
pub const QUEUE_WAIT_THRESHOLD_MS: i64 = 2_000;
pub const TOOL_CATEGORY_MAP_VERSION: &str = "1";
pub const DENIAL_PATTERNS_VERSION: &str = "1";

/// Pattern list v1 (spec §4.1: false negatives acceptable, false positives not).
const DENIAL_PATTERNS: &[&str] = &[
    "has been denied",
    "was denied",
    "denied by the Claude Code auto mode classifier",
    "The user doesn't want to proceed",
    "User rejected",
    "user declined",
];

const INTERRUPT_MARKER: &str = "[Request interrupted by user";

/// Tool → category map v1 (spec §4.1; version recorded in the manifest).
pub fn tool_category(name: &str) -> &'static str {
    if name.starts_with("mcp__") {
        return "mcp";
    }
    match name {
        "Read" | "NotebookRead" | "LS" | "ListMcpResourcesTool" | "ReadMcpResourceTool" => "read",
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "edit",
        "Bash" | "BashOutput" | "KillShell" | "KillBash" => "exec",
        "Grep" | "Glob" | "ToolSearch" => "search",
        "Task" | "Agent" | "Workflow" | "SendMessage" | "ListAgents" | "TaskOutput"
        | "TaskStop" => "agent",
        "WebFetch" | "WebSearch" => "web",
        _ => "other",
    }
}

// ---------------------------------------------------------------------------
// Row structs (Serialize so fixtures can snapshot them as JSON)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub project: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    pub cli_version: Option<String>,
    pub start_ts: i64,
    pub end_ts: i64,
    pub is_complete: bool,
    pub models: Vec<String>,
    pub n_cycles: i32,
    pub n_api_calls: i32,
    pub n_tool_calls: i32,
    pub n_tool_errors: i32,
    pub n_denials: i32,
    pub n_retries: i32,
    pub n_sidechains: i32,
    pub n_interrupted_cycles: i32,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_cache_read: i64,
    pub tokens_cache_write: i64,
    pub tokens_thinking: i64,
    pub files_touched: i32,
    pub wall_ms: i64,
    pub sum_agent_active_ms: i64,
    /// Count of *other* sessions active during this session's span; filled in
    /// by the scanner once all spans are known.
    pub overlap_sessions: i32,
}

#[derive(Debug, Serialize)]
pub struct CycleRow {
    pub cycle_id: String,
    pub session_id: String,
    pub prompt_id: String,
    pub prompt_ts: i64,
    pub prompt_kind: &'static str, // chat | slash_command | queued
    pub prompt_chars: i64,
    pub prompt_sha256: String,
    pub first_response_ms: Option<i64>,
    pub agent_active_ms: i64,
    pub human_dwell_ms: Option<i64>,
    pub n_api_calls: i32,
    pub n_tool_calls: i32,
    pub n_tool_errors: i32,
    pub n_denials: i32,
    pub n_retries: i32,
    pub n_sidechains: i32,
    pub sidechain_tool_calls: i32,
    pub interrupted: bool,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_cache_read: i64,
    pub tokens_cache_write: i64,
    pub tokens_thinking: i64,
    pub files_touched: i32,
    pub files_retouched: i32,
    pub ended_by: &'static str, // next_prompt | interrupt | session_end
}

#[derive(Debug, Serialize)]
pub struct ApiCallRow {
    pub request_id: String,
    pub session_id: String,
    pub cycle_id: Option<String>,
    pub ts: i64,
    pub model: String,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
    pub speed: Option<String>,
    pub stop_reason: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_write_1h_tokens: i64,
    pub cache_write_5m_tokens: i64,
    pub thinking_tokens: i64,
    pub n_tool_uses: i32,
    pub n_text_blocks: i32,
}

#[derive(Debug, Serialize)]
pub struct ToolCallRow {
    pub tool_call_id: String,
    pub session_id: String,
    pub cycle_id: Option<String>,
    pub event_uuid: String,
    pub ts: i64,
    pub tool_name: String,
    pub tool_category: &'static str,
    pub is_sidechain: bool,
    pub duration_ms: Option<i64>,
    pub input_bytes: i64,
    pub result_bytes: i64,
    pub is_error: bool,
    pub is_denied: bool,
    pub is_interrupted: bool,
    pub retry_of: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileTouchRow {
    pub session_id: String,
    pub cycle_id: Option<String>,
    pub ts: i64,
    pub path_sha256: String,
    /// Raw path, only populated with `--include-text` (spec §4.5).
    pub path: Option<String>,
    pub ext: Option<String>,
    pub op: &'static str, // delta | snapshot
}

#[derive(Debug, Serialize)]
pub struct TextRow {
    pub event_uuid: String,
    pub seq: i32,
    pub session_id: String,
    pub cycle_id: Option<String>,
    pub ts: i64,
    pub kind: String, // prompt | assistant_text | thinking | tool_result
    pub text: String,
}

#[derive(Debug, Default, Serialize)]
pub struct SessionOutput {
    pub session: Option<SessionRow>,
    pub cycles: Vec<CycleRow>,
    pub api_calls: Vec<ApiCallRow>,
    pub tool_calls: Vec<ToolCallRow>,
    pub file_touches: Vec<FileTouchRow>,
    pub texts: Vec<TextRow>,
    pub skipped_lines: u64,
    pub unknown_types: BTreeMap<String, u64>,
    /// Per-subagent-file bookkeeping for manifest source rows.
    pub sub_stats: Vec<SubagentStats>,
}

#[derive(Debug, Serialize)]
pub struct SubagentStats {
    pub path: String,
    pub rows_emitted: u64,
    pub skipped_lines: u64,
}

// ---------------------------------------------------------------------------
// Sessionizer internals
// ---------------------------------------------------------------------------

struct CycleAcc {
    prompt_id: String,
    cycle_id: String,
    prompt_ts: i64,
    prompt_kind: &'static str,
    prompt_chars: i64,
    prompt_sha256: String,
    first_response_ts: Option<i64>,
    last_ts: i64,
    interrupted: bool,
    n_sidechains: i32,
    sidechain_tool_calls: i32,
    /// (tool_name, target_key) -> tool_call index of last *errored* call,
    /// for the retry_of linkage rule (§4.1).
    last_error_by_target: HashMap<(String, String), usize>,
}

/// Parse and derive one session: the main transcript plus any subagent
/// transcripts (`<session>/subagents/agent-*.jsonl`), which are the current
/// CLI's storage for sidechain traffic. Iteration is single-pass in file
/// order; result matching and retry linkage are resolved as results arrive.
pub fn process_session(
    session_id: &str,
    project: &str,
    content: &[u8],
    subagents: &[(String, Vec<u8>)],
    include_text: bool,
) -> SessionOutput {
    let mut out = SessionOutput::default();

    // envelope facts (first non-null wins; cli_version: last wins, tracks upgrades)
    let mut cwd: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut entrypoint: Option<String> = None;
    let mut cli_version: Option<String> = None;
    let mut models: Vec<String> = Vec::new();
    let mut start_ts: Option<i64> = None;
    let mut end_ts: Option<i64> = None;

    let mut cycles: Vec<CycleAcc> = Vec::new();
    let mut api_index: HashMap<String, usize> = HashMap::new(); // request_id -> api_calls idx
    let mut tool_index: HashMap<String, usize> = HashMap::new(); // tool_use_id -> tool_calls idx
    let mut tool_target: Vec<(String, String)> = Vec::new(); // per tool_call: (name, target key)
    let mut touch_entry_idx: Vec<usize> = Vec::new(); // per file_touch: global entry index
    let mut enqueue_fifo: std::collections::VecDeque<i64> = Default::default();
    let mut pending_queued = false;
    let mut entry_idx: usize = 0; // global index over successfully parsed entries

    for line in content.split(|&b| b == b'\n') {
        let trimmed = trim_ascii(line);
        if trimmed.is_empty() {
            continue;
        }
        let entry: RawEntry = match serde_json::from_slice(trimmed) {
            Ok(e) => e,
            Err(_) => {
                out.skipped_lines += 1;
                continue;
            }
        };
        entry_idx += 1;
        let kind = entry.kind.as_deref().unwrap_or("<none>");
        if !KNOWN_TYPES.contains(&kind) {
            *out.unknown_types.entry(kind.to_string()).or_default() += 1;
            continue;
        }

        let ts = entry.ts_ms();
        if let Some(t) = ts {
            start_ts = Some(start_ts.map_or(t, |s: i64| s.min(t)));
            end_ts = Some(end_ts.map_or(t, |e: i64| e.max(t)));
        }
        if cwd.is_none() {
            cwd = entry.cwd.clone();
        }
        if git_branch.is_none() {
            git_branch = entry.git_branch.clone();
        }
        if entrypoint.is_none() {
            entrypoint = entry.entrypoint.clone();
        }
        if entry.version.is_some() {
            cli_version = entry.version.clone();
        }

        let current = cycles.len().checked_sub(1);
        let current_cycle_id = current.map(|i| cycles[i].cycle_id.clone());

        match kind {
            "user" => {
                let content = model::extract_user_content(entry.message.as_ref());
                let has_tool_result = !content.tool_results.is_empty();

                if has_tool_result {
                    // Result carrier: match to pending tool_use blocks.
                    for tr in &content.tool_results {
                        let Some(&ti) = tool_index.get(tr.tool_use_id) else {
                            continue;
                        };
                        let tc = &mut out.tool_calls[ti];
                        if let Some(t) = ts {
                            tc.duration_ms = Some((t - tc.ts).max(0));
                        }
                        tc.result_bytes = tr.content_bytes;
                        tc.is_error = tr.is_error;
                        let denied = entry.tool_denial_kind.is_some()
                            || DENIAL_PATTERNS.iter().any(|p| tr.content_text.contains(p));
                        tc.is_denied = denied;
                        // retry_of bookkeeping: an errored/denied call becomes a
                        // retry target; a successful one clears the slot.
                        if let Some(ci) = current {
                            let key = tool_target[ti].clone();
                            if tc.is_error || denied {
                                cycles[ci].last_error_by_target.insert(key, ti);
                            } else {
                                cycles[ci].last_error_by_target.remove(&key);
                            }
                        }
                        if include_text {
                            out.texts.push(TextRow {
                                event_uuid: entry.uuid.clone().unwrap_or_default(),
                                seq: out.texts.len() as i32,
                                session_id: session_id.to_string(),
                                cycle_id: current_cycle_id.clone(),
                                ts: ts.unwrap_or(0),
                                kind: "tool_result".into(),
                                text: tr.content_text.clone(),
                            });
                        }
                    }
                    if let (Some(ci), Some(t)) = (current, ts) {
                        cycles[ci].last_ts = cycles[ci].last_ts.max(t);
                    }
                } else if content.text.starts_with(INTERRUPT_MARKER) {
                    if let Some(ci) = current {
                        cycles[ci].interrupted = true;
                        if let Some(t) = ts {
                            cycles[ci].last_ts = cycles[ci].last_ts.max(t);
                        }
                    }
                } else if entry.is_meta {
                    // caveat/meta wrapper — not a human prompt, not agent work
                } else if entry.is_sidechain {
                    // subagent prompt: parallelism signal for the enclosing cycle
                    if let Some(ci) = current {
                        cycles[ci].n_sidechains += 1;
                        if let Some(t) = ts {
                            cycles[ci].last_ts = cycles[ci].last_ts.max(t);
                        }
                    }
                } else {
                    let fresh = current
                        .map(|i| entry.prompt_id.as_deref().unwrap_or("") != cycles[i].prompt_id)
                        .unwrap_or(true);
                    if fresh {
                        // A human prompt: open a new cycle.
                        let prompt_id = entry
                            .prompt_id
                            .clone()
                            .or_else(|| entry.uuid.clone())
                            .unwrap_or_else(|| format!("line-{entry_idx}"));
                        let mut cycle_id = format!("{session_id}:{prompt_id}");
                        if cycles.iter().any(|c| c.cycle_id == cycle_id) {
                            cycle_id = format!("{cycle_id}#{}", cycles.len());
                        }
                        let kind = if content.text.contains("<command-name>") {
                            "slash_command"
                        } else if pending_queued {
                            "queued"
                        } else {
                            "chat"
                        };
                        pending_queued = false;
                        let t = ts.unwrap_or(0);
                        if include_text {
                            out.texts.push(TextRow {
                                event_uuid: entry.uuid.clone().unwrap_or_default(),
                                seq: out.texts.len() as i32,
                                session_id: session_id.to_string(),
                                cycle_id: Some(cycle_id.clone()),
                                ts: t,
                                kind: "prompt".into(),
                                text: content.text.clone(),
                            });
                        }
                        cycles.push(CycleAcc {
                            prompt_id,
                            cycle_id,
                            prompt_ts: t,
                            prompt_kind: kind,
                            prompt_chars: content.text.chars().count() as i64,
                            prompt_sha256: sha256_hex(content.text.as_bytes()),
                            first_response_ts: None,
                            last_ts: t,
                            interrupted: false,
                            n_sidechains: 0,
                            sidechain_tool_calls: 0,
                            last_error_by_target: HashMap::new(),
                        });
                    } else if let (Some(ci), Some(t)) = (current, ts) {
                        // continuation entry within the same promptId
                        cycles[ci].last_ts = cycles[ci].last_ts.max(t);
                    }
                }
            }
            "assistant" => {
                let msg = model::extract_assistant_message(entry.message.as_ref(), include_text);
                let ci = cycles.len().checked_sub(1);
                if let (Some(i), Some(t)) = (ci, ts) {
                    let c = &mut cycles[i];
                    c.last_ts = c.last_ts.max(t);
                    if !entry.is_sidechain && c.first_response_ts.is_none() {
                        c.first_response_ts = Some(t);
                    }
                }
                if let Some(m) = msg.model {
                    if !models.iter().any(|x| x == m) {
                        models.push(m.to_string());
                    }
                }

                // api_calls: one row per requestId; streaming writes one entry
                // per content block with identical usage, so first entry wins
                // and later entries only add block tallies / fill gaps.
                let request_id = entry
                    .request_id
                    .clone()
                    .or_else(|| entry.uuid.clone())
                    .unwrap_or_else(|| format!("req-{entry_idx}"));
                let u = msg.usage;
                let idx = *api_index.entry(request_id.clone()).or_insert_with(|| {
                    out.api_calls.push(ApiCallRow {
                        request_id,
                        session_id: session_id.to_string(),
                        cycle_id: ci.map(|i| cycles[i].cycle_id.clone()),
                        ts: ts.unwrap_or(0),
                        model: msg.model.unwrap_or("unknown").to_string(),
                        effort: entry.effort.clone(),
                        service_tier: model::usage_str(u, "service_tier").map(String::from),
                        speed: model::usage_str(u, "speed").map(String::from),
                        stop_reason: String::new(),
                        input_tokens: model::usage_i64(u, "input_tokens"),
                        output_tokens: model::usage_i64(u, "output_tokens"),
                        cache_read_tokens: model::usage_i64(u, "cache_read_input_tokens"),
                        cache_write_tokens: model::usage_i64(u, "cache_creation_input_tokens"),
                        cache_write_1h_tokens: model::usage_nested_i64(
                            u,
                            "cache_creation",
                            "ephemeral_1h_input_tokens",
                        ),
                        cache_write_5m_tokens: model::usage_nested_i64(
                            u,
                            "cache_creation",
                            "ephemeral_5m_input_tokens",
                        ),
                        thinking_tokens: model::usage_nested_i64(
                            u,
                            "output_tokens_details",
                            "thinking_tokens",
                        ),
                        n_tool_uses: 0,
                        n_text_blocks: 0,
                    });
                    out.api_calls.len() - 1
                });
                {
                    let row = &mut out.api_calls[idx];
                    row.n_tool_uses += msg.tool_uses.len() as i32;
                    row.n_text_blocks += msg.n_text_blocks;
                    if let Some(sr) = msg.stop_reason {
                        row.stop_reason = sr.to_string();
                    }
                }

                // tool_calls: one row per tool_use block
                for tu in &msg.tool_uses {
                    let input_json = tu
                        .input
                        .map(|v| serde_json::to_string(v).unwrap_or_default())
                        .unwrap_or_default();
                    let target = normalized_target(tu.name, tu.input, &input_json);
                    let key = (tu.name.to_string(), target);
                    let retry_of = ci
                        .and_then(|i| cycles[i].last_error_by_target.get(&key))
                        .map(|&pi| out.tool_calls[pi].tool_call_id.clone());
                    if entry.is_sidechain {
                        if let Some(i) = ci {
                            cycles[i].sidechain_tool_calls += 1;
                        }
                    }
                    tool_index.insert(tu.id.to_string(), out.tool_calls.len());
                    tool_target.push(key);
                    out.tool_calls.push(ToolCallRow {
                        tool_call_id: tu.id.to_string(),
                        session_id: session_id.to_string(),
                        cycle_id: ci.map(|i| cycles[i].cycle_id.clone()),
                        event_uuid: entry.uuid.clone().unwrap_or_default(),
                        ts: ts.unwrap_or(0),
                        tool_name: tu.name.to_string(),
                        tool_category: tool_category(tu.name),
                        is_sidechain: entry.is_sidechain,
                        duration_ms: None,
                        input_bytes: input_json.len() as i64,
                        result_bytes: 0,
                        is_error: false,
                        is_denied: false,
                        is_interrupted: false,
                        retry_of,
                    });
                }

                if include_text {
                    for (kind, text) in msg.texts {
                        out.texts.push(TextRow {
                            event_uuid: entry.uuid.clone().unwrap_or_default(),
                            seq: out.texts.len() as i32,
                            session_id: session_id.to_string(),
                            cycle_id: ci.map(|i| cycles[i].cycle_id.clone()),
                            ts: ts.unwrap_or(0),
                            kind: kind.to_string(),
                            text,
                        });
                    }
                }
            }
            "system" | "attachment" => {
                if let (Some(ci), Some(t)) = (current, ts) {
                    cycles[ci].last_ts = cycles[ci].last_ts.max(t);
                }
            }
            "queue-operation" => match entry.operation.as_deref() {
                Some("enqueue") => {
                    if let Some(t) = ts {
                        enqueue_fifo.push_back(t);
                    }
                }
                Some("dequeue") => {
                    if let (Some(t), Some(enq)) = (ts, enqueue_fifo.pop_front()) {
                        if t - enq > QUEUE_WAIT_THRESHOLD_MS {
                            pending_queued = true;
                        }
                    }
                }
                Some("remove") => {
                    enqueue_fifo.pop_back();
                }
                _ => {}
            },
            "file-history-delta" => {
                if let Some(path) = &entry.tracking_path {
                    touch_entry_idx.push(entry_idx);
                    out.file_touches.push(make_touch(
                        session_id,
                        current_cycle_id.clone(),
                        ts.unwrap_or(0),
                        path,
                        "delta",
                        include_text,
                    ));
                }
            }
            "file-history-snapshot" => {
                let snap_ts = entry
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.get("timestamp"))
                    .and_then(Value::as_str)
                    .and_then(crate::util::parse_ts_ms)
                    .or(ts)
                    .unwrap_or(0);
                if let Some(Value::Object(backups)) = entry
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.get("trackedFileBackups"))
                {
                    for path in backups.keys() {
                        touch_entry_idx.push(entry_idx);
                        out.file_touches.push(make_touch(
                            session_id,
                            current_cycle_id.clone(),
                            snap_ts,
                            path,
                            "snapshot",
                            include_text,
                        ));
                    }
                }
            }
            "permission-mode" => {} // modeled (friction signal), no fact rows in v0
            _ => unreachable!(),
        }
    }

    // ---- sidechain pass: subagent transcripts attach to the parent cycle ----
    // A whole subagent file is attributed to the cycle active at its first
    // timestamped entry (spec §2: sidechains attach to their parent cycle but
    // are tallied separately — they are the parallelism signal).
    for (sub_path, sub_content) in subagents {
        let rows_before = out.api_calls.len() + out.tool_calls.len() + out.texts.len();
        let skipped_before = out.skipped_lines;
        let mut sub_cycle: Option<usize> = None; // index into `cycles`
        let mut counted = false;
        let mut sub_tool_index: HashMap<String, usize> = HashMap::new();
        let mut sub_api_index: HashMap<String, usize> = HashMap::new();
        let mut sub_last_error: HashMap<(String, String), usize> = HashMap::new();
        let mut sub_entry_idx = 0usize;
        for line in sub_content.split(|&b| b == b'\n') {
            let trimmed = trim_ascii(line);
            if trimmed.is_empty() {
                continue;
            }
            let entry: RawEntry = match serde_json::from_slice(trimmed) {
                Ok(e) => e,
                Err(_) => {
                    out.skipped_lines += 1;
                    continue;
                }
            };
            sub_entry_idx += 1;
            let kind = entry.kind.as_deref().unwrap_or("<none>");
            if !KNOWN_TYPES.contains(&kind) {
                *out.unknown_types.entry(kind.to_string()).or_default() += 1;
                continue;
            }
            let ts = entry.ts_ms();
            if let Some(t) = ts {
                start_ts = Some(start_ts.map_or(t, |s: i64| s.min(t)));
                end_ts = Some(end_ts.map_or(t, |e: i64| e.max(t)));
                if !counted {
                    // attribute the subagent to the cycle active at launch
                    sub_cycle = cycles.iter().rposition(|c| c.prompt_ts <= t);
                    if let Some(ci) = sub_cycle {
                        cycles[ci].n_sidechains += 1;
                    }
                    counted = true;
                }
                if let Some(ci) = sub_cycle {
                    cycles[ci].last_ts = cycles[ci].last_ts.max(t);
                }
            }
            let cycle_id = sub_cycle.map(|ci| cycles[ci].cycle_id.clone());
            match kind {
                "assistant" => {
                    let msg =
                        model::extract_assistant_message(entry.message.as_ref(), include_text);
                    if let Some(m) = msg.model {
                        if !models.iter().any(|x| x == m) {
                            models.push(m.to_string());
                        }
                    }
                    let request_id = entry
                        .request_id
                        .clone()
                        .or_else(|| entry.uuid.clone())
                        .unwrap_or_else(|| format!("{sub_path}-req-{sub_entry_idx}"));
                    let u = msg.usage;
                    let idx = *sub_api_index.entry(request_id.clone()).or_insert_with(|| {
                        out.api_calls.push(ApiCallRow {
                            request_id,
                            session_id: session_id.to_string(),
                            cycle_id: cycle_id.clone(),
                            ts: ts.unwrap_or(0),
                            model: msg.model.unwrap_or("unknown").to_string(),
                            effort: entry.effort.clone(),
                            service_tier: model::usage_str(u, "service_tier").map(String::from),
                            speed: model::usage_str(u, "speed").map(String::from),
                            stop_reason: String::new(),
                            input_tokens: model::usage_i64(u, "input_tokens"),
                            output_tokens: model::usage_i64(u, "output_tokens"),
                            cache_read_tokens: model::usage_i64(u, "cache_read_input_tokens"),
                            cache_write_tokens: model::usage_i64(u, "cache_creation_input_tokens"),
                            cache_write_1h_tokens: model::usage_nested_i64(
                                u,
                                "cache_creation",
                                "ephemeral_1h_input_tokens",
                            ),
                            cache_write_5m_tokens: model::usage_nested_i64(
                                u,
                                "cache_creation",
                                "ephemeral_5m_input_tokens",
                            ),
                            thinking_tokens: model::usage_nested_i64(
                                u,
                                "output_tokens_details",
                                "thinking_tokens",
                            ),
                            n_tool_uses: 0,
                            n_text_blocks: 0,
                        });
                        out.api_calls.len() - 1
                    });
                    {
                        let row = &mut out.api_calls[idx];
                        row.n_tool_uses += msg.tool_uses.len() as i32;
                        row.n_text_blocks += msg.n_text_blocks;
                        if let Some(sr) = msg.stop_reason {
                            row.stop_reason = sr.to_string();
                        }
                    }
                    for tu in &msg.tool_uses {
                        let input_json = tu
                            .input
                            .map(|v| serde_json::to_string(v).unwrap_or_default())
                            .unwrap_or_default();
                        let target = normalized_target(tu.name, tu.input, &input_json);
                        let key = (tu.name.to_string(), target);
                        let retry_of = sub_last_error
                            .get(&key)
                            .map(|&pi| out.tool_calls[pi].tool_call_id.clone());
                        if let Some(ci) = sub_cycle {
                            cycles[ci].sidechain_tool_calls += 1;
                        }
                        sub_tool_index.insert(tu.id.to_string(), out.tool_calls.len());
                        tool_target.push(key);
                        out.tool_calls.push(ToolCallRow {
                            tool_call_id: tu.id.to_string(),
                            session_id: session_id.to_string(),
                            cycle_id: cycle_id.clone(),
                            event_uuid: entry.uuid.clone().unwrap_or_default(),
                            ts: ts.unwrap_or(0),
                            tool_name: tu.name.to_string(),
                            tool_category: tool_category(tu.name),
                            is_sidechain: true,
                            duration_ms: None,
                            input_bytes: input_json.len() as i64,
                            result_bytes: 0,
                            is_error: false,
                            is_denied: false,
                            is_interrupted: false,
                            retry_of,
                        });
                    }
                    if include_text {
                        for (k, text) in msg.texts {
                            out.texts.push(TextRow {
                                event_uuid: entry.uuid.clone().unwrap_or_default(),
                                seq: out.texts.len() as i32,
                                session_id: session_id.to_string(),
                                cycle_id: cycle_id.clone(),
                                ts: ts.unwrap_or(0),
                                kind: format!("sidechain_{k}"),
                                text,
                            });
                        }
                    }
                }
                "user" => {
                    let content = model::extract_user_content(entry.message.as_ref());
                    for tr in &content.tool_results {
                        let Some(&ti) = sub_tool_index.get(tr.tool_use_id) else {
                            continue;
                        };
                        let tc = &mut out.tool_calls[ti];
                        if let Some(t) = ts {
                            tc.duration_ms = Some((t - tc.ts).max(0));
                        }
                        tc.result_bytes = tr.content_bytes;
                        tc.is_error = tr.is_error;
                        tc.is_denied = entry.tool_denial_kind.is_some()
                            || DENIAL_PATTERNS.iter().any(|p| tr.content_text.contains(p));
                        let key = tool_target[ti].clone();
                        if tc.is_error || tc.is_denied {
                            sub_last_error.insert(key, ti);
                        } else {
                            sub_last_error.remove(&key);
                        }
                        if include_text {
                            out.texts.push(TextRow {
                                event_uuid: entry.uuid.clone().unwrap_or_default(),
                                seq: out.texts.len() as i32,
                                session_id: session_id.to_string(),
                                cycle_id: cycle_id.clone(),
                                ts: ts.unwrap_or(0),
                                kind: "sidechain_tool_result".into(),
                                text: tr.content_text.clone(),
                            });
                        }
                    }
                    if content.tool_results.is_empty() && include_text && !content.text.is_empty() {
                        out.texts.push(TextRow {
                            event_uuid: entry.uuid.clone().unwrap_or_default(),
                            seq: out.texts.len() as i32,
                            session_id: session_id.to_string(),
                            cycle_id: cycle_id.clone(),
                            ts: ts.unwrap_or(0),
                            kind: "sidechain_prompt".into(),
                            text: content.text.clone(),
                        });
                    }
                }
                _ => {} // system/attachment/etc: timing already handled above
            }
        }
        out.sub_stats.push(SubagentStats {
            path: sub_path.clone(),
            rows_emitted: (out.api_calls.len() + out.tool_calls.len() + out.texts.len()
                - rows_before) as u64,
            skipped_lines: out.skipped_lines - skipped_before,
        });
    }

    for a in &mut out.api_calls {
        if a.stop_reason.is_empty() {
            a.stop_reason = "unknown".into();
        }
    }

    // ---- post-pass: interrupted tool calls (no result + interrupted cycle) ----
    let interrupted_cycles: HashSet<&str> = cycles
        .iter()
        .filter(|c| c.interrupted)
        .map(|c| c.cycle_id.as_str())
        .collect();
    for tc in &mut out.tool_calls {
        if tc.duration_ms.is_none()
            && tc
                .cycle_id
                .as_deref()
                .is_some_and(|c| interrupted_cycles.contains(c))
        {
            tc.is_interrupted = true;
        }
    }

    // ---- post-pass: files_retouched ----
    // A file counts as retouched for the cycle where a touch has *another*
    // touch of the same file ≥2 parsed-entries later in this session (§4.3).
    let mut touches_by_path: HashMap<&str, Vec<(usize, usize)>> = HashMap::new(); // sha -> (entry_idx, touch_idx)
    for (i, ft) in out.file_touches.iter().enumerate() {
        touches_by_path
            .entry(ft.path_sha256.as_str())
            .or_default()
            .push((touch_entry_idx[i], i));
    }
    let mut retouched_by_cycle: HashMap<String, HashSet<String>> = HashMap::new();
    for (sha, idxs) in &touches_by_path {
        for (k, (eidx, tidx)) in idxs.iter().enumerate() {
            let has_later = idxs[k + 1..].iter().any(|(e2, _)| *e2 >= eidx + 2);
            if has_later {
                if let Some(cid) = &out.file_touches[*tidx].cycle_id {
                    retouched_by_cycle
                        .entry(cid.clone())
                        .or_default()
                        .insert(sha.to_string());
                }
            }
        }
    }
    let mut touched_by_cycle: HashMap<&str, HashSet<&str>> = HashMap::new();
    for ft in &out.file_touches {
        if let Some(cid) = &ft.cycle_id {
            touched_by_cycle
                .entry(cid.as_str())
                .or_default()
                .insert(ft.path_sha256.as_str());
        }
    }

    // ---- rollups: cycles ----
    let mut per_cycle_api: HashMap<&str, Vec<&ApiCallRow>> = HashMap::new();
    for a in &out.api_calls {
        if let Some(cid) = &a.cycle_id {
            per_cycle_api.entry(cid.as_str()).or_default().push(a);
        }
    }
    let mut per_cycle_tools: HashMap<&str, Vec<&ToolCallRow>> = HashMap::new();
    for t in &out.tool_calls {
        if let Some(cid) = &t.cycle_id {
            per_cycle_tools.entry(cid.as_str()).or_default().push(t);
        }
    }

    let n = cycles.len();
    for (i, c) in cycles.iter().enumerate() {
        let api = per_cycle_api
            .get(c.cycle_id.as_str())
            .map_or(&[][..], |v| v);
        let tools = per_cycle_tools
            .get(c.cycle_id.as_str())
            .map_or(&[][..], |v| v);
        let next_prompt_ts = cycles.get(i + 1).map(|nc| nc.prompt_ts);
        let human_dwell_ms = next_prompt_ts.and_then(|np| {
            let d = (np - c.last_ts).max(0);
            (d <= IDLE_THRESHOLD_MS).then_some(d)
        });
        out.cycles.push(CycleRow {
            cycle_id: c.cycle_id.clone(),
            session_id: session_id.to_string(),
            prompt_id: c.prompt_id.clone(),
            prompt_ts: c.prompt_ts,
            prompt_kind: c.prompt_kind,
            prompt_chars: c.prompt_chars,
            prompt_sha256: c.prompt_sha256.clone(),
            first_response_ms: c.first_response_ts.map(|t| (t - c.prompt_ts).max(0)),
            agent_active_ms: (c.last_ts - c.prompt_ts).max(0),
            human_dwell_ms,
            n_api_calls: api.len() as i32,
            n_tool_calls: tools.iter().filter(|t| !t.is_sidechain).count() as i32,
            n_tool_errors: tools.iter().filter(|t| t.is_error).count() as i32,
            n_denials: tools.iter().filter(|t| t.is_denied).count() as i32,
            n_retries: tools.iter().filter(|t| t.retry_of.is_some()).count() as i32,
            n_sidechains: c.n_sidechains,
            sidechain_tool_calls: c.sidechain_tool_calls,
            interrupted: c.interrupted,
            tokens_input: api.iter().map(|a| a.input_tokens).sum(),
            tokens_output: api.iter().map(|a| a.output_tokens).sum(),
            tokens_cache_read: api.iter().map(|a| a.cache_read_tokens).sum(),
            tokens_cache_write: api.iter().map(|a| a.cache_write_tokens).sum(),
            tokens_thinking: api.iter().map(|a| a.thinking_tokens).sum(),
            files_touched: touched_by_cycle
                .get(c.cycle_id.as_str())
                .map_or(0, |s| s.len() as i32),
            files_retouched: retouched_by_cycle
                .get(c.cycle_id.as_str())
                .map_or(0, |s| s.len() as i32),
            ended_by: if c.interrupted {
                "interrupt"
            } else if i + 1 < n {
                "next_prompt"
            } else {
                "session_end"
            },
        });
    }

    // ---- rollup: session ----
    if entry_idx > 0 {
        let start = start_ts.unwrap_or(0);
        let end = end_ts.unwrap_or(start);
        let all_files: HashSet<&str> = out
            .file_touches
            .iter()
            .map(|f| f.path_sha256.as_str())
            .collect();
        out.session = Some(SessionRow {
            session_id: session_id.to_string(),
            project: project.to_string(),
            cwd,
            git_branch,
            entrypoint,
            cli_version,
            start_ts: start,
            end_ts: end,
            is_complete: true, // scanner may flip for still-growing files
            models,
            n_cycles: out.cycles.len() as i32,
            n_api_calls: out.api_calls.len() as i32,
            n_tool_calls: out.tool_calls.iter().filter(|t| !t.is_sidechain).count() as i32,
            n_tool_errors: out.tool_calls.iter().filter(|t| t.is_error).count() as i32,
            n_denials: out.tool_calls.iter().filter(|t| t.is_denied).count() as i32,
            n_retries: out
                .tool_calls
                .iter()
                .filter(|t| t.retry_of.is_some())
                .count() as i32,
            n_sidechains: out.cycles.iter().map(|c| c.n_sidechains).sum(),
            n_interrupted_cycles: out.cycles.iter().filter(|c| c.interrupted).count() as i32,
            tokens_input: out.api_calls.iter().map(|a| a.input_tokens).sum(),
            tokens_output: out.api_calls.iter().map(|a| a.output_tokens).sum(),
            tokens_cache_read: out.api_calls.iter().map(|a| a.cache_read_tokens).sum(),
            tokens_cache_write: out.api_calls.iter().map(|a| a.cache_write_tokens).sum(),
            tokens_thinking: out.api_calls.iter().map(|a| a.thinking_tokens).sum(),
            files_touched: all_files.len() as i32,
            wall_ms: (end - start).max(0),
            sum_agent_active_ms: out.cycles.iter().map(|c| c.agent_active_ms).sum(),
            overlap_sessions: 0, // filled by the scanner
        });
    }

    out
}

fn make_touch(
    session_id: &str,
    cycle_id: Option<String>,
    ts: i64,
    path: &str,
    op: &'static str,
    include_text: bool,
) -> FileTouchRow {
    FileTouchRow {
        session_id: session_id.to_string(),
        cycle_id,
        ts,
        path_sha256: sha256_hex(path.as_bytes()),
        path: include_text.then(|| path.to_string()),
        ext: std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase()),
        op,
    }
}

/// Normalized retry target (§4.1): same file for file tools, same trimmed
/// command for Bash, else the full input JSON.
fn normalized_target(name: &str, input: Option<&Value>, input_json: &str) -> String {
    let from_key = |k: &str| {
        input
            .and_then(|i| i.get(k))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    match name {
        "Bash" => from_key("command")
            .map(|c| c.trim().to_string())
            .unwrap_or_else(|| input_json.to_string()),
        _ => from_key("file_path")
            .or_else(|| from_key("notebook_path"))
            .or_else(|| from_key("path"))
            .unwrap_or_else(|| input_json.to_string()),
    }
}

fn trim_ascii(b: &[u8]) -> &[u8] {
    let start = b
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(start, |e| e + 1);
    &b[start..end]
}
