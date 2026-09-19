//! Row types for the grindstone fact tables (spec §4) and the manifest (§6).
//! Every optional column is an `Option` so a dataset with a missing or null
//! column still loads; the report says what it could not compute.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The schema major this reader understands. Datasets with another major
/// are refused, as the manifest contract asks consumers to do.
pub const SUPPORTED_SCHEMA_MAJOR: u64 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub dataset: String,
    pub schema_version: String,
    pub generator: Generator,
    pub generated_at: String,
    pub local_tz: String,
    pub config: Config,
    pub coverage: Coverage,
    pub sources: Vec<Source>,
    pub tables: BTreeMap<String, TableStats>,
    pub unknown_types: BTreeMap<String, u64>,
    pub incomplete_sessions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Generator {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub idle_threshold_ms: i64,
    pub queue_wait_threshold_ms: i64,
    pub include_text: bool,
    pub tool_category_map_version: String,
    pub denial_patterns_version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Coverage {
    pub start: Option<String>,
    pub end: Option<String>,
    pub projects: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows_emitted: u64,
    pub skipped_lines: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TableStats {
    pub rows: i64,
    pub files: u64,
}

impl Manifest {
    pub fn schema_major(&self) -> Option<u64> {
        self.schema_version.split('.').next()?.parse().ok()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Session {
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
    pub n_cycles: i64,
    pub n_api_calls: i64,
    pub n_tool_calls: i64,
    pub n_tool_errors: i64,
    pub n_denials: i64,
    pub n_retries: i64,
    pub n_sidechains: i64,
    pub n_interrupted_cycles: i64,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_cache_read: i64,
    pub tokens_cache_write: i64,
    pub tokens_thinking: i64,
    pub files_touched: i64,
    pub wall_ms: i64,
    pub sum_agent_active_ms: i64,
    pub overlap_sessions: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Cycle {
    pub cycle_id: String,
    pub session_id: String,
    pub prompt_ts: i64,
    pub prompt_kind: String,
    pub prompt_chars: i64,
    pub first_response_ms: Option<i64>,
    pub agent_active_ms: i64,
    pub human_dwell_ms: Option<i64>,
    pub n_api_calls: i64,
    pub n_tool_calls: i64,
    pub n_tool_errors: i64,
    pub n_denials: i64,
    pub n_retries: i64,
    pub n_sidechains: i64,
    pub sidechain_tool_calls: i64,
    pub interrupted: bool,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_cache_read: i64,
    pub tokens_cache_write: i64,
    pub tokens_thinking: i64,
    pub files_touched: i64,
    pub files_retouched: i64,
    pub ended_by: String,
}

#[derive(Debug, Clone, Default)]
pub struct ApiCall {
    pub session_id: String,
    pub ts: i64,
    pub model: String,
    pub effort: Option<String>,
    pub stop_reason: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_write_1h_tokens: i64,
    pub cache_write_5m_tokens: i64,
    pub thinking_tokens: i64,
    pub n_tool_uses: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCall {
    pub session_id: String,
    pub ts: i64,
    pub tool_name: String,
    pub tool_category: String,
    pub is_sidechain: bool,
    pub duration_ms: Option<i64>,
    pub input_bytes: i64,
    pub result_bytes: i64,
    pub is_error: bool,
    pub is_denied: bool,
    pub is_interrupted: bool,
    pub is_retry: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FileTouch {
    pub session_id: String,
    pub ts: i64,
    pub path_sha256: String,
    pub path: Option<String>,
    pub ext: Option<String>,
    pub op: String,
}

/// Everything the report is computed from, after the time/project filter.
#[derive(Debug, Default)]
pub struct Dataset {
    pub manifest: Manifest,
    pub sessions: Vec<Session>,
    pub cycles: Vec<Cycle>,
    pub api_calls: Vec<ApiCall>,
    pub tool_calls: Vec<ToolCall>,
    pub file_touches: Vec<FileTouch>,
    /// Non-fatal problems met while reading (a file without the grindstone
    /// metadata stamp, an unreadable file). Rendered as notes.
    pub warnings: Vec<String>,
}
