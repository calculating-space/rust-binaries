//! Arrow record-batch construction and Parquet IO for the fact tables.
//!
//! Column names/types follow spec §4 exactly. `utf8 dict` columns are
//! Dictionary(Int32, Utf8); timestamps are Timestamp(ms, UTC).

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow::array::{
    ArrayRef, BooleanBuilder, Int32Builder, Int64Builder, ListBuilder, StringBuilder,
    StringDictionaryBuilder, TimestampMillisecondBuilder,
};
use arrow::datatypes::Int32Type;
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use parquet::file::reader::{FileReader, SerializedFileReader};

use crate::session::{ApiCallRow, CycleRow, FileTouchRow, SessionRow, TextRow, ToolCallRow};

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const FACT_TABLES: &[&str] = &[
    "sessions",
    "cycles",
    "api_calls",
    "tool_calls",
    "file_touches",
];

fn ts_builder() -> TimestampMillisecondBuilder {
    TimestampMillisecondBuilder::new().with_timezone("UTC")
}

fn dict() -> StringDictionaryBuilder<Int32Type> {
    StringDictionaryBuilder::new()
}

macro_rules! finish {
    ($($name:expr => $builder:expr),+ $(,)?) => {
        RecordBatch::try_from_iter(vec![
            $(($name, Arc::new($builder.finish()) as ArrayRef),)+
        ])
    };
}

pub fn sessions_batch(rows: &[SessionRow]) -> Result<RecordBatch> {
    let (mut session_id, mut project, mut cwd, mut git_branch) = (
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    let (mut entrypoint, mut cli_version) = (dict(), dict());
    let (mut start_ts, mut end_ts) = (ts_builder(), ts_builder());
    let mut is_complete = BooleanBuilder::new();
    let mut models = ListBuilder::new(StringBuilder::new());
    let mut i32s: Vec<Int32Builder> = (0..9).map(|_| Int32Builder::new()).collect();
    let mut i64s: Vec<Int64Builder> = (0..7).map(|_| Int64Builder::new()).collect();
    let mut overlap = Int32Builder::new();
    for r in rows {
        session_id.append_value(&r.session_id);
        project.append_value(&r.project);
        cwd.append_option(r.cwd.as_deref());
        git_branch.append_option(r.git_branch.as_deref());
        entrypoint.append_option(r.entrypoint.as_deref());
        cli_version.append_option(r.cli_version.as_deref());
        start_ts.append_value(r.start_ts);
        end_ts.append_value(r.end_ts);
        is_complete.append_value(r.is_complete);
        for m in &r.models {
            models.values().append_value(m);
        }
        models.append(true);
        for (b, v) in i32s.iter_mut().zip([
            r.n_cycles,
            r.n_api_calls,
            r.n_tool_calls,
            r.n_tool_errors,
            r.n_denials,
            r.n_retries,
            r.n_sidechains,
            r.n_interrupted_cycles,
            r.files_touched,
        ]) {
            b.append_value(v);
        }
        for (b, v) in i64s.iter_mut().zip([
            r.tokens_input,
            r.tokens_output,
            r.tokens_cache_read,
            r.tokens_cache_write,
            r.tokens_thinking,
            r.wall_ms,
            r.sum_agent_active_ms,
        ]) {
            b.append_value(v);
        }
        overlap.append_value(r.overlap_sessions);
    }
    let mut it = i32s.into_iter();
    let (
        mut n_cycles,
        mut n_api,
        mut n_tool,
        mut n_err,
        mut n_den,
        mut n_ret,
        mut n_side,
        mut n_int,
        mut files,
    ) = (
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    );
    let mut it = i64s.into_iter();
    let (mut t_in, mut t_out, mut t_cr, mut t_cw, mut t_th, mut wall, mut active) = (
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    );
    Ok(finish![
        "session_id" => session_id, "project" => project, "cwd" => cwd,
        "git_branch" => git_branch, "entrypoint" => entrypoint, "cli_version" => cli_version,
        "start_ts" => start_ts, "end_ts" => end_ts, "is_complete" => is_complete,
        "models" => models,
        "n_cycles" => n_cycles, "n_api_calls" => n_api, "n_tool_calls" => n_tool,
        "n_tool_errors" => n_err, "n_denials" => n_den, "n_retries" => n_ret,
        "n_sidechains" => n_side, "n_interrupted_cycles" => n_int,
        "tokens_input" => t_in, "tokens_output" => t_out, "tokens_cache_read" => t_cr,
        "tokens_cache_write" => t_cw, "tokens_thinking" => t_th,
        "files_touched" => files, "wall_ms" => wall, "sum_agent_active_ms" => active,
        "overlap_sessions" => overlap,
    ]?)
}

pub fn cycles_batch(rows: &[CycleRow]) -> Result<RecordBatch> {
    let (mut cycle_id, mut session_id, mut prompt_id, mut prompt_sha256) = (
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    let mut prompt_ts = ts_builder();
    let (mut prompt_kind, mut ended_by) = (dict(), dict());
    let mut prompt_chars = Int64Builder::new();
    let (mut first_response_ms, mut agent_active_ms, mut human_dwell_ms) = (
        Int64Builder::new(),
        Int64Builder::new(),
        Int64Builder::new(),
    );
    let mut i32s: Vec<Int32Builder> = (0..9).map(|_| Int32Builder::new()).collect();
    let mut interrupted = BooleanBuilder::new();
    let mut i64s: Vec<Int64Builder> = (0..5).map(|_| Int64Builder::new()).collect();
    for r in rows {
        cycle_id.append_value(&r.cycle_id);
        session_id.append_value(&r.session_id);
        prompt_id.append_value(&r.prompt_id);
        prompt_ts.append_value(r.prompt_ts);
        prompt_kind.append_value(r.prompt_kind);
        prompt_chars.append_value(r.prompt_chars);
        prompt_sha256.append_value(&r.prompt_sha256);
        first_response_ms.append_option(r.first_response_ms);
        agent_active_ms.append_value(r.agent_active_ms);
        human_dwell_ms.append_option(r.human_dwell_ms);
        for (b, v) in i32s.iter_mut().zip([
            r.n_api_calls,
            r.n_tool_calls,
            r.n_tool_errors,
            r.n_denials,
            r.n_retries,
            r.n_sidechains,
            r.sidechain_tool_calls,
            r.files_touched,
            r.files_retouched,
        ]) {
            b.append_value(v);
        }
        interrupted.append_value(r.interrupted);
        for (b, v) in i64s.iter_mut().zip([
            r.tokens_input,
            r.tokens_output,
            r.tokens_cache_read,
            r.tokens_cache_write,
            r.tokens_thinking,
        ]) {
            b.append_value(v);
        }
        ended_by.append_value(r.ended_by);
    }
    let mut it = i32s.into_iter();
    let (
        mut n_api,
        mut n_tool,
        mut n_err,
        mut n_den,
        mut n_ret,
        mut n_side,
        mut side_tools,
        mut files,
        mut refiles,
    ) = (
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    );
    let mut it = i64s.into_iter();
    let (mut t_in, mut t_out, mut t_cr, mut t_cw, mut t_th) = (
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    );
    Ok(finish![
        "cycle_id" => cycle_id, "session_id" => session_id, "prompt_id" => prompt_id,
        "prompt_ts" => prompt_ts, "prompt_kind" => prompt_kind,
        "prompt_chars" => prompt_chars, "prompt_sha256" => prompt_sha256,
        "first_response_ms" => first_response_ms, "agent_active_ms" => agent_active_ms,
        "human_dwell_ms" => human_dwell_ms,
        "n_api_calls" => n_api, "n_tool_calls" => n_tool, "n_tool_errors" => n_err,
        "n_denials" => n_den, "n_retries" => n_ret,
        "n_sidechains" => n_side, "sidechain_tool_calls" => side_tools,
        "interrupted" => interrupted,
        "tokens_input" => t_in, "tokens_output" => t_out, "tokens_cache_read" => t_cr,
        "tokens_cache_write" => t_cw, "tokens_thinking" => t_th,
        "files_touched" => files, "files_retouched" => refiles,
        "ended_by" => ended_by,
    ]?)
}

pub fn api_calls_batch(rows: &[ApiCallRow]) -> Result<RecordBatch> {
    let (mut request_id, mut session_id, mut cycle_id) = (
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    let mut ts = ts_builder();
    let (mut model, mut effort, mut service_tier, mut speed, mut stop_reason) =
        (dict(), dict(), dict(), dict(), dict());
    let mut i64s: Vec<Int64Builder> = (0..7).map(|_| Int64Builder::new()).collect();
    let (mut n_tool_uses, mut n_text_blocks) = (Int32Builder::new(), Int32Builder::new());
    for r in rows {
        request_id.append_value(&r.request_id);
        session_id.append_value(&r.session_id);
        cycle_id.append_option(r.cycle_id.as_deref());
        ts.append_value(r.ts);
        model.append_value(&r.model);
        effort.append_option(r.effort.as_deref());
        service_tier.append_option(r.service_tier.as_deref());
        speed.append_option(r.speed.as_deref());
        stop_reason.append_value(&r.stop_reason);
        for (b, v) in i64s.iter_mut().zip([
            r.input_tokens,
            r.output_tokens,
            r.cache_read_tokens,
            r.cache_write_tokens,
            r.cache_write_1h_tokens,
            r.cache_write_5m_tokens,
            r.thinking_tokens,
        ]) {
            b.append_value(v);
        }
        n_tool_uses.append_value(r.n_tool_uses);
        n_text_blocks.append_value(r.n_text_blocks);
    }
    let mut it = i64s.into_iter();
    let (mut t_in, mut t_out, mut t_cr, mut t_cw, mut t_cw1, mut t_cw5, mut t_th) = (
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    );
    Ok(finish![
        "request_id" => request_id, "session_id" => session_id, "cycle_id" => cycle_id,
        "ts" => ts, "model" => model, "effort" => effort,
        "service_tier" => service_tier, "speed" => speed, "stop_reason" => stop_reason,
        "input_tokens" => t_in, "output_tokens" => t_out,
        "cache_read_tokens" => t_cr, "cache_write_tokens" => t_cw,
        "cache_write_1h_tokens" => t_cw1, "cache_write_5m_tokens" => t_cw5,
        "thinking_tokens" => t_th,
        "n_tool_uses" => n_tool_uses, "n_text_blocks" => n_text_blocks,
    ]?)
}

pub fn tool_calls_batch(rows: &[ToolCallRow]) -> Result<RecordBatch> {
    let (
        mut tool_call_id,
        mut session_id,
        mut cycle_id,
        mut event_uuid,
        mut tool_name,
        mut retry_of,
    ) = (
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    let mut ts = ts_builder();
    let mut tool_category = dict();
    let (mut is_sidechain, mut is_error, mut is_denied, mut is_interrupted) = (
        BooleanBuilder::new(),
        BooleanBuilder::new(),
        BooleanBuilder::new(),
        BooleanBuilder::new(),
    );
    let (mut duration_ms, mut input_bytes, mut result_bytes) = (
        Int64Builder::new(),
        Int64Builder::new(),
        Int64Builder::new(),
    );
    for r in rows {
        tool_call_id.append_value(&r.tool_call_id);
        session_id.append_value(&r.session_id);
        cycle_id.append_option(r.cycle_id.as_deref());
        event_uuid.append_value(&r.event_uuid);
        ts.append_value(r.ts);
        tool_name.append_value(&r.tool_name);
        tool_category.append_value(r.tool_category);
        is_sidechain.append_value(r.is_sidechain);
        duration_ms.append_option(r.duration_ms);
        input_bytes.append_value(r.input_bytes);
        result_bytes.append_value(r.result_bytes);
        is_error.append_value(r.is_error);
        is_denied.append_value(r.is_denied);
        is_interrupted.append_value(r.is_interrupted);
        retry_of.append_option(r.retry_of.as_deref());
    }
    Ok(finish![
        "tool_call_id" => tool_call_id, "session_id" => session_id, "cycle_id" => cycle_id,
        "event_uuid" => event_uuid, "ts" => ts,
        "tool_name" => tool_name, "tool_category" => tool_category,
        "is_sidechain" => is_sidechain, "duration_ms" => duration_ms,
        "input_bytes" => input_bytes, "result_bytes" => result_bytes,
        "is_error" => is_error, "is_denied" => is_denied, "is_interrupted" => is_interrupted,
        "retry_of" => retry_of,
    ]?)
}

pub fn file_touches_batch(rows: &[FileTouchRow]) -> Result<RecordBatch> {
    let (mut session_id, mut cycle_id, mut path_sha256, mut path) = (
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    let mut ts = ts_builder();
    let (mut ext, mut op) = (dict(), dict());
    for r in rows {
        session_id.append_value(&r.session_id);
        cycle_id.append_option(r.cycle_id.as_deref());
        ts.append_value(r.ts);
        path_sha256.append_value(&r.path_sha256);
        path.append_option(r.path.as_deref());
        ext.append_option(r.ext.as_deref());
        op.append_value(r.op);
    }
    Ok(finish![
        "session_id" => session_id, "cycle_id" => cycle_id, "ts" => ts,
        "path_sha256" => path_sha256, "path" => path, "ext" => ext, "op" => op,
    ]?)
}

pub fn texts_batch(rows: &[TextRow]) -> Result<RecordBatch> {
    let (mut event_uuid, mut session_id, mut cycle_id, mut kind, mut text) = (
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    let mut seq = Int32Builder::new();
    let mut ts = ts_builder();
    for r in rows {
        event_uuid.append_value(&r.event_uuid);
        seq.append_value(r.seq);
        session_id.append_value(&r.session_id);
        cycle_id.append_option(r.cycle_id.as_deref());
        ts.append_value(r.ts);
        kind.append_value(&r.kind);
        text.append_value(&r.text);
    }
    Ok(finish![
        "event_uuid" => event_uuid, "seq" => seq, "session_id" => session_id,
        "cycle_id" => cycle_id, "ts" => ts, "kind" => kind, "text" => text,
    ]?)
}

/// Write one batch to `path`, stamping the grindstone KV metadata (spec §3).
pub fn write_parquet(path: &Path, batch: &RecordBatch) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_key_value_metadata(Some(vec![
            KeyValue::new(
                "grindstone.schema_version".to_string(),
                SCHEMA_VERSION.to_string(),
            ),
            KeyValue::new(
                "grindstone.generator_version".to_string(),
                GENERATOR_VERSION.to_string(),
            ),
        ]))
        .build();
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut w = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    w.write(batch)?;
    w.close()?;
    Ok(())
}

/// Footer-only read: (num_rows, has grindstone KV metadata).
pub fn parquet_footer_info(path: &Path) -> Result<(i64, bool)> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = SerializedFileReader::new(file)?;
    let meta = reader.metadata().file_metadata();
    let has_kv = meta
        .key_value_metadata()
        .is_some_and(|kv| kv.iter().any(|k| k.key == "grindstone.schema_version"));
    Ok((meta.num_rows(), has_kv))
}
