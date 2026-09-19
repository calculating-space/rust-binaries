//! Tolerant Parquet reading. Columns are looked up by name and cast to a
//! canonical type (utf8, int64, bool, list<utf8>), so dictionary encoding,
//! int32 widths and timestamp units never matter to the analysis. A missing
//! column reads as null; only the join keys are required.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use arrow::array::{Array, BooleanArray, Int64Array, ListArray, StringArray};
use arrow::compute::cast;
use arrow::datatypes::{DataType, Field};
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::model::*;

const KV_SCHEMA_VERSION: &str = "grindstone.schema_version";

pub fn walk_parquet(dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return v;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            v.extend(walk_parquet(&p));
        } else if p.extension().is_some_and(|x| x == "parquet") {
            v.push(p);
        }
    }
    v.sort();
    v
}

/// One batch's columns, already cast. Missing columns yield `None` values.
pub struct Cols {
    n: usize,
    strs: BTreeMap<String, Vec<Option<String>>>,
    ints: BTreeMap<String, Vec<Option<i64>>>,
    bools: BTreeMap<String, Vec<Option<bool>>>,
    lists: BTreeMap<String, Vec<Vec<String>>>,
}

impl Cols {
    fn from_batch(batch: &RecordBatch) -> Cols {
        let mut c = Cols {
            n: batch.num_rows(),
            strs: BTreeMap::new(),
            ints: BTreeMap::new(),
            bools: BTreeMap::new(),
            lists: BTreeMap::new(),
        };
        for (field, col) in batch.schema().fields().iter().zip(batch.columns()) {
            let name = field.name().to_string();
            match field.data_type() {
                DataType::Boolean => {
                    if let Some(a) = col.as_any().downcast_ref::<BooleanArray>() {
                        c.bools.insert(name, a.iter().collect());
                    }
                }
                DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
                | DataType::Timestamp(_, _)
                | DataType::Date32
                | DataType::Date64 => {
                    if let Ok(a) = cast(col, &DataType::Int64) {
                        if let Some(a) = a.as_any().downcast_ref::<Int64Array>() {
                            c.ints.insert(name, a.iter().collect());
                        }
                    }
                }
                DataType::List(_) | DataType::LargeList(_) => {
                    let target = DataType::List(std::sync::Arc::new(Field::new(
                        "item",
                        DataType::Utf8,
                        true,
                    )));
                    if let Ok(a) = cast(col, &target) {
                        if let Some(a) = a.as_any().downcast_ref::<ListArray>() {
                            let rows = (0..a.len())
                                .map(|i| {
                                    if a.is_null(i) {
                                        return Vec::new();
                                    }
                                    let v = a.value(i);
                                    v.as_any()
                                        .downcast_ref::<StringArray>()
                                        .map(|s| s.iter().flatten().map(str::to_string).collect())
                                        .unwrap_or_default()
                                })
                                .collect();
                            c.lists.insert(name, rows);
                        }
                    }
                }
                _ => {
                    if let Ok(a) = cast(col, &DataType::Utf8) {
                        if let Some(a) = a.as_any().downcast_ref::<StringArray>() {
                            c.strs
                                .insert(name, a.iter().map(|s| s.map(str::to_string)).collect());
                        }
                    }
                }
            }
        }
        c
    }

    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    pub fn has(&self, name: &str) -> bool {
        self.strs.contains_key(name)
            || self.ints.contains_key(name)
            || self.bools.contains_key(name)
            || self.lists.contains_key(name)
    }
    pub fn str(&self, name: &str, i: usize) -> Option<String> {
        self.strs.get(name).and_then(|v| v[i].clone())
    }
    pub fn int(&self, name: &str, i: usize) -> Option<i64> {
        self.ints.get(name).and_then(|v| v[i])
    }
    pub fn bool(&self, name: &str, i: usize) -> Option<bool> {
        self.bools.get(name).and_then(|v| v[i])
    }
    pub fn list(&self, name: &str, i: usize) -> Vec<String> {
        self.lists
            .get(name)
            .map(|v| v[i].clone())
            .unwrap_or_default()
    }
    pub fn int0(&self, name: &str, i: usize) -> i64 {
        self.int(name, i).unwrap_or(0)
    }
    pub fn boolf(&self, name: &str, i: usize) -> bool {
        self.bool(name, i).unwrap_or(false)
    }
    pub fn string(&self, name: &str, i: usize) -> String {
        self.str(name, i).unwrap_or_default()
    }
}

/// Read every parquet file under `table_dir`, calling `f` per batch.
/// Per-file problems become warnings; a schema major mismatch is fatal.
pub fn read_table(
    table_dir: &Path,
    table: &str,
    warnings: &mut Vec<String>,
    mut f: impl FnMut(&Cols) -> Result<()>,
) -> Result<()> {
    for path in walk_parquet(table_dir) {
        let short = path
            .strip_prefix(table_dir.parent().unwrap_or(table_dir))
            .unwrap_or(&path)
            .display()
            .to_string();
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                warnings.push(format!("{table}: cannot open {short}: {e}"));
                continue;
            }
        };
        let builder = match ParquetRecordBatchReaderBuilder::try_new(file) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(format!("{table}: {short}: not readable as parquet: {e}"));
                continue;
            }
        };
        let kv = builder.metadata().file_metadata().key_value_metadata();
        match kv.and_then(|kvs| kvs.iter().find(|k| k.key == KV_SCHEMA_VERSION)) {
            Some(k) => {
                let major = k
                    .value
                    .as_deref()
                    .and_then(|v| v.split('.').next())
                    .and_then(|m| m.parse::<u64>().ok());
                if major != Some(SUPPORTED_SCHEMA_MAJOR) {
                    bail!(
                        "{table}: {short} has schema_version {}; this reader understands major {SUPPORTED_SCHEMA_MAJOR}",
                        k.value.as_deref().unwrap_or("?")
                    );
                }
            }
            None => warnings.push(format!(
                "{table}: {short} carries no {KV_SCHEMA_VERSION} metadata; read anyway"
            )),
        }
        let reader = builder
            .build()
            .with_context(|| format!("{table}: reading {short}"))?;
        for batch in reader {
            let batch = batch.with_context(|| format!("{table}: decoding {short}"))?;
            let cols = Cols::from_batch(&batch);
            if !cols.has("session_id") {
                bail!("{table}: {short} has no session_id column");
            }
            f(&cols)?;
        }
    }
    Ok(())
}

/// Which sessions to keep: a time window on the session span and an optional
/// project match (substring, case-insensitive, against slug or cwd).
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub projects: Vec<String>,
}

impl Filter {
    fn keeps(&self, s: &Session) -> bool {
        if let Some(from) = self.from_ms {
            if s.end_ts < from {
                return false;
            }
        }
        if let Some(to) = self.to_ms {
            if s.start_ts >= to {
                return false;
            }
        }
        if self.projects.is_empty() {
            return true;
        }
        let slug = s.project.to_lowercase();
        let cwd = s.cwd.as_deref().unwrap_or("").to_lowercase();
        self.projects
            .iter()
            .any(|p| slug.contains(&p.to_lowercase()) || cwd.contains(&p.to_lowercase()))
    }
}

pub fn load_manifest(dir: &Path) -> Result<Manifest> {
    let p = dir.join("manifest.json");
    if !p.exists() {
        return Err(anyhow!(
            "{} has no manifest.json; point --out at a grindstone output directory",
            dir.display()
        ));
    }
    let data = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
    let m: Manifest = serde_json::from_slice(&data).context("parsing manifest.json")?;
    match m.schema_major() {
        Some(SUPPORTED_SCHEMA_MAJOR) => Ok(m),
        _ => Err(anyhow!(
            "manifest schema_version {:?} is not major {SUPPORTED_SCHEMA_MAJOR}",
            m.schema_version
        )),
    }
}

/// Load the dataset under `dir`, keeping only sessions the filter accepts and
/// the child rows that belong to them.
pub fn load(dir: &Path, filter: &Filter) -> Result<Dataset> {
    let manifest = load_manifest(dir)?;
    let facts = dir.join("facts");
    let mut ds = Dataset {
        manifest,
        ..Default::default()
    };
    let mut warnings = Vec::new();

    let mut sessions = Vec::new();
    read_table(&facts.join("sessions"), "sessions", &mut warnings, |c| {
        for i in 0..c.len() {
            sessions.push(Session {
                session_id: c.string("session_id", i),
                project: c.string("project", i),
                cwd: c.str("cwd", i),
                git_branch: c.str("git_branch", i),
                entrypoint: c.str("entrypoint", i),
                cli_version: c.str("cli_version", i),
                start_ts: c.int0("start_ts", i),
                end_ts: c.int0("end_ts", i),
                is_complete: c.bool("is_complete", i).unwrap_or(true),
                models: c.list("models", i),
                n_cycles: c.int0("n_cycles", i),
                n_api_calls: c.int0("n_api_calls", i),
                n_tool_calls: c.int0("n_tool_calls", i),
                n_tool_errors: c.int0("n_tool_errors", i),
                n_denials: c.int0("n_denials", i),
                n_retries: c.int0("n_retries", i),
                n_sidechains: c.int0("n_sidechains", i),
                n_interrupted_cycles: c.int0("n_interrupted_cycles", i),
                tokens_input: c.int0("tokens_input", i),
                tokens_output: c.int0("tokens_output", i),
                tokens_cache_read: c.int0("tokens_cache_read", i),
                tokens_cache_write: c.int0("tokens_cache_write", i),
                tokens_thinking: c.int0("tokens_thinking", i),
                files_touched: c.int0("files_touched", i),
                wall_ms: c.int0("wall_ms", i),
                sum_agent_active_ms: c.int0("sum_agent_active_ms", i),
                overlap_sessions: c.int0("overlap_sessions", i),
            });
        }
        Ok(())
    })?;
    if sessions.is_empty() {
        bail!(
            "no sessions table under {}; run `grindstone scan` first",
            facts.display()
        );
    }
    sessions.retain(|s| filter.keeps(s));
    sessions.sort_by(|a, b| {
        a.start_ts
            .cmp(&b.start_ts)
            .then(a.session_id.cmp(&b.session_id))
    });
    let keep: std::collections::BTreeSet<String> =
        sessions.iter().map(|s| s.session_id.clone()).collect();
    ds.sessions = sessions;

    read_table(&facts.join("cycles"), "cycles", &mut warnings, |c| {
        for i in 0..c.len() {
            let sid = c.string("session_id", i);
            if !keep.contains(&sid) {
                continue;
            }
            ds.cycles.push(Cycle {
                cycle_id: c.string("cycle_id", i),
                session_id: sid,
                prompt_ts: c.int0("prompt_ts", i),
                prompt_kind: c.str("prompt_kind", i).unwrap_or_else(|| "unknown".into()),
                prompt_chars: c.int0("prompt_chars", i),
                first_response_ms: c.int("first_response_ms", i),
                agent_active_ms: c.int0("agent_active_ms", i),
                human_dwell_ms: c.int("human_dwell_ms", i),
                n_api_calls: c.int0("n_api_calls", i),
                n_tool_calls: c.int0("n_tool_calls", i),
                n_tool_errors: c.int0("n_tool_errors", i),
                n_denials: c.int0("n_denials", i),
                n_retries: c.int0("n_retries", i),
                n_sidechains: c.int0("n_sidechains", i),
                sidechain_tool_calls: c.int0("sidechain_tool_calls", i),
                interrupted: c.boolf("interrupted", i),
                tokens_input: c.int0("tokens_input", i),
                tokens_output: c.int0("tokens_output", i),
                tokens_cache_read: c.int0("tokens_cache_read", i),
                tokens_cache_write: c.int0("tokens_cache_write", i),
                tokens_thinking: c.int0("tokens_thinking", i),
                files_touched: c.int0("files_touched", i),
                files_retouched: c.int0("files_retouched", i),
                ended_by: c.str("ended_by", i).unwrap_or_else(|| "unknown".into()),
            });
        }
        Ok(())
    })?;

    read_table(&facts.join("api_calls"), "api_calls", &mut warnings, |c| {
        for i in 0..c.len() {
            let sid = c.string("session_id", i);
            if !keep.contains(&sid) {
                continue;
            }
            ds.api_calls.push(ApiCall {
                session_id: sid,
                ts: c.int0("ts", i),
                model: c.str("model", i).unwrap_or_else(|| "unknown".into()),
                effort: c.str("effort", i),
                stop_reason: c.str("stop_reason", i).unwrap_or_else(|| "unknown".into()),
                input_tokens: c.int0("input_tokens", i),
                output_tokens: c.int0("output_tokens", i),
                cache_read_tokens: c.int0("cache_read_tokens", i),
                cache_write_tokens: c.int0("cache_write_tokens", i),
                cache_write_1h_tokens: c.int0("cache_write_1h_tokens", i),
                cache_write_5m_tokens: c.int0("cache_write_5m_tokens", i),
                thinking_tokens: c.int0("thinking_tokens", i),
                n_tool_uses: c.int0("n_tool_uses", i),
            });
        }
        Ok(())
    })?;

    read_table(
        &facts.join("tool_calls"),
        "tool_calls",
        &mut warnings,
        |c| {
            for i in 0..c.len() {
                let sid = c.string("session_id", i);
                if !keep.contains(&sid) {
                    continue;
                }
                ds.tool_calls.push(ToolCall {
                    session_id: sid,
                    ts: c.int0("ts", i),
                    tool_name: c.str("tool_name", i).unwrap_or_else(|| "unknown".into()),
                    tool_category: c.str("tool_category", i).unwrap_or_else(|| "other".into()),
                    is_sidechain: c.boolf("is_sidechain", i),
                    duration_ms: c.int("duration_ms", i),
                    input_bytes: c.int0("input_bytes", i),
                    result_bytes: c.int0("result_bytes", i),
                    is_error: c.boolf("is_error", i),
                    is_denied: c.boolf("is_denied", i),
                    is_interrupted: c.boolf("is_interrupted", i),
                    is_retry: c.str("retry_of", i).is_some(),
                });
            }
            Ok(())
        },
    )?;

    read_table(
        &facts.join("file_touches"),
        "file_touches",
        &mut warnings,
        |c| {
            for i in 0..c.len() {
                let sid = c.string("session_id", i);
                if !keep.contains(&sid) {
                    continue;
                }
                ds.file_touches.push(FileTouch {
                    session_id: sid,
                    ts: c.int0("ts", i),
                    path_sha256: c.string("path_sha256", i),
                    path: c.str("path", i),
                    ext: c.str("ext", i),
                    op: c.str("op", i).unwrap_or_else(|| "unknown".into()),
                });
            }
            Ok(())
        },
    )?;

    ds.cycles.sort_by(|a, b| {
        a.prompt_ts
            .cmp(&b.prompt_ts)
            .then(a.cycle_id.cmp(&b.cycle_id))
    });
    ds.api_calls.sort_by_key(|r| r.ts);
    ds.tool_calls.sort_by_key(|r| r.ts);
    ds.file_touches.sort_by_key(|r| r.ts);
    ds.warnings = warnings;
    Ok(ds)
}
