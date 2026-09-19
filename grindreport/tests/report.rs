//! Conformance: a small synthetic dataset written with the real grindstone
//! column layout, read back and rendered in every format.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Int32Array, Int64Array, ListBuilder, StringArray,
    StringDictionaryBuilder, TimestampMillisecondArray,
};
use arrow::datatypes::Int32Type;
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

use grindreport::{analyze, load, Filter, Format, Options, Pricing};

const H: i64 = 3_600_000;
const T0: i64 = 1_755_000_000_000; // 2025-08-12T12:00:00Z

fn ts(v: &[i64]) -> ArrayRef {
    Arc::new(TimestampMillisecondArray::from(v.to_vec()).with_timezone("UTC"))
}
fn i32s(v: &[i32]) -> ArrayRef {
    Arc::new(Int32Array::from(v.to_vec()))
}
fn i64s(v: &[i64]) -> ArrayRef {
    Arc::new(Int64Array::from(v.to_vec()))
}
fn i64o(v: &[Option<i64>]) -> ArrayRef {
    Arc::new(Int64Array::from(v.to_vec()))
}
fn strs(v: &[&str]) -> ArrayRef {
    Arc::new(StringArray::from(v.to_vec()))
}
fn stro(v: &[Option<&str>]) -> ArrayRef {
    Arc::new(StringArray::from(v.to_vec()))
}
fn dict(v: &[&str]) -> ArrayRef {
    let mut b = StringDictionaryBuilder::<Int32Type>::new();
    for s in v {
        b.append_value(s);
    }
    Arc::new(b.finish())
}
fn bools(v: &[bool]) -> ArrayRef {
    Arc::new(BooleanArray::from(v.to_vec()))
}

fn write(path: &Path, cols: Vec<(&str, ArrayRef)>, stamp: Option<&str>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let batch = RecordBatch::try_from_iter(cols).unwrap();
    let mut props = WriterProperties::builder();
    if let Some(v) = stamp {
        props = props.set_key_value_metadata(Some(vec![KeyValue::new(
            "grindstone.schema_version".into(),
            v.to_string(),
        )]));
    }
    let mut w = ArrowWriter::try_new(
        File::create(path).unwrap(),
        batch.schema(),
        Some(props.build()),
    )
    .unwrap();
    w.write(&batch).unwrap();
    w.close().unwrap();
}

/// Two projects, three sessions. Session C is live and in project beta.
fn dataset(dir: &Path, stamp: Option<&str>) {
    std::fs::write(
        dir.join("manifest.json"),
        r#"{
  "dataset": "grindstone", "schema_version": "1.0.0",
  "generator": {"name": "grindstone", "version": "0.1.0"},
  "generated_at": "2025-08-14T00:00:00Z", "local_tz": "Europe/Lisbon",
  "config": {"idle_threshold_ms": 1800000, "queue_wait_threshold_ms": 2000, "include_text": false,
             "tool_category_map_version": "1", "denial_patterns_version": "1"},
  "coverage": {"start": "2025-08-12T12:00:00Z", "end": "2025-08-13T18:00:00Z", "projects": ["alpha", "beta"]},
  "sources": [{"path": "alpha/A.jsonl", "sha256": "0", "bytes": 1, "rows_emitted": 1, "skipped_lines": 2, "complete": true}],
  "tables": {"sessions": {"rows": 3, "files": 3}},
  "unknown_types": {"frame-link": 3},
  "incomplete_sessions": ["C"]
}"#,
    )
    .unwrap();
    let f = dir.join("facts");
    let mut models = ListBuilder::new(arrow::array::StringBuilder::new());
    for m in [
        vec!["claude-opus-5"],
        vec!["claude-opus-5", "claude-sonnet-5"],
        vec![],
    ] {
        for x in m {
            models.values().append_value(x);
        }
        models.append(true);
    }
    write(
        &f.join("sessions/project=alpha/date=2025-08-12/A.parquet"),
        vec![
            ("session_id", strs(&["A", "B", "C"])),
            ("project", strs(&["alpha", "alpha", "beta"])),
            (
                "cwd",
                stro(&[
                    Some("/Users/me/src/alpha"),
                    Some("/Users/me/src/alpha"),
                    None,
                ]),
            ),
            ("git_branch", stro(&[Some("main"), None, None])),
            ("entrypoint", dict(&["cli", "claude-vscode", "cli"])),
            ("cli_version", dict(&["2.1.0", "2.1.0", "2.1.1"])),
            ("start_ts", ts(&[T0, T0 + 2 * H, T0 + 26 * H])),
            ("end_ts", ts(&[T0 + H, T0 + 3 * H, T0 + 30 * H])),
            ("is_complete", bools(&[true, true, false])),
            ("models", Arc::new(models.finish())),
            ("n_cycles", i32s(&[2, 1, 1])),
            ("n_api_calls", i32s(&[3, 1, 1])),
            ("n_tool_calls", i32s(&[3, 1, 0])),
            ("n_tool_errors", i32s(&[1, 0, 0])),
            ("n_denials", i32s(&[1, 0, 0])),
            ("n_retries", i32s(&[0, 0, 0])),
            ("n_sidechains", i32s(&[1, 0, 0])),
            ("n_interrupted_cycles", i32s(&[1, 0, 0])),
            ("tokens_input", i64s(&[100, 50, 10])),
            ("tokens_output", i64s(&[1000, 500, 100])),
            ("tokens_cache_read", i64s(&[10_000, 5_000, 0])),
            ("tokens_cache_write", i64s(&[300, 200, 0])),
            ("tokens_thinking", i64s(&[400, 0, 0])),
            ("files_touched", i32s(&[2, 1, 0])),
            ("wall_ms", i64s(&[H, H, 4 * H])),
            (
                "sum_agent_active_ms",
                i64s(&[30 * 60_000, 20 * 60_000, 60 * 60_000]),
            ),
            ("overlap_sessions", i32s(&[0, 0, 0])),
        ],
        stamp,
    );
    write(
        &f.join("cycles/project=alpha/date=2025-08-12/A.parquet"),
        vec![
            ("cycle_id", strs(&["A:1", "A:2", "B:1", "C:1"])),
            ("session_id", strs(&["A", "A", "B", "C"])),
            ("prompt_id", strs(&["1", "2", "1", "1"])),
            (
                "prompt_ts",
                ts(&[T0, T0 + 20 * 60_000, T0 + 2 * H, T0 + 26 * H]),
            ),
            (
                "prompt_kind",
                dict(&["chat", "slash_command", "chat", "chat"]),
            ),
            ("prompt_chars", i64s(&[40, 10, 80, 20])),
            ("prompt_sha256", strs(&["a", "b", "c", "d"])),
            (
                "first_response_ms",
                i64o(&[Some(2000), Some(4000), None, Some(1000)]),
            ),
            (
                "agent_active_ms",
                i64s(&[10 * 60_000, 20 * 60_000, 20 * 60_000, 60 * 60_000]),
            ),
            ("human_dwell_ms", i64o(&[Some(60_000), None, None, None])),
            ("n_api_calls", i32s(&[2, 1, 1, 1])),
            ("n_tool_calls", i32s(&[2, 1, 1, 0])),
            ("n_tool_errors", i32s(&[1, 0, 0, 0])),
            ("n_denials", i32s(&[1, 0, 0, 0])),
            ("n_retries", i32s(&[0, 0, 0, 0])),
            ("n_sidechains", i32s(&[1, 0, 0, 0])),
            ("sidechain_tool_calls", i32s(&[1, 0, 0, 0])),
            ("interrupted", bools(&[false, true, false, false])),
            ("tokens_input", i64s(&[60, 40, 50, 10])),
            ("tokens_output", i64s(&[600, 400, 500, 100])),
            ("tokens_cache_read", i64s(&[6000, 4000, 5000, 0])),
            ("tokens_cache_write", i64s(&[200, 100, 200, 0])),
            ("tokens_thinking", i64s(&[400, 0, 0, 0])),
            ("files_touched", i32s(&[2, 1, 1, 0])),
            ("files_retouched", i32s(&[0, 1, 0, 0])),
            (
                "ended_by",
                dict(&["next_prompt", "interrupt", "session_end", "session_end"]),
            ),
        ],
        stamp,
    );
    write(
        &f.join("api_calls/project=alpha/date=2025-08-12/A.parquet"),
        vec![
            ("request_id", strs(&["r1", "r2", "r3", "r4", "r5"])),
            ("session_id", strs(&["A", "A", "A", "B", "C"])),
            (
                "cycle_id",
                stro(&[
                    Some("A:1"),
                    Some("A:1"),
                    Some("A:2"),
                    Some("B:1"),
                    Some("C:1"),
                ]),
            ),
            (
                "ts",
                ts(&[
                    T0 + 1000,
                    T0 + 5000,
                    T0 + 21 * 60_000,
                    T0 + 2 * H + 1000,
                    T0 + 26 * H + 1000,
                ]),
            ),
            (
                "model",
                dict(&[
                    "claude-opus-5",
                    "claude-opus-5",
                    "claude-opus-5",
                    "claude-sonnet-5",
                    "mystery-model",
                ]),
            ),
            ("effort", dict(&["high", "high", "high", "low", "high"])),
            (
                "service_tier",
                dict(&["standard", "standard", "standard", "standard", "standard"]),
            ),
            (
                "speed",
                dict(&["standard", "standard", "standard", "standard", "standard"]),
            ),
            (
                "stop_reason",
                dict(&["tool_use", "end_turn", "end_turn", "end_turn", "end_turn"]),
            ),
            ("input_tokens", i64s(&[30, 30, 40, 50, 10])),
            ("output_tokens", i64s(&[300, 300, 400, 500, 100])),
            ("cache_read_tokens", i64s(&[3000, 3000, 4000, 5000, 0])),
            ("cache_write_tokens", i64s(&[100, 100, 100, 200, 0])),
            ("cache_write_1h_tokens", i64s(&[0, 0, 0, 200, 0])),
            ("cache_write_5m_tokens", i64s(&[100, 100, 100, 0, 0])),
            ("thinking_tokens", i64s(&[400, 0, 0, 0, 0])),
            ("n_tool_uses", i32s(&[2, 0, 1, 1, 0])),
            ("n_text_blocks", i32s(&[0, 1, 1, 1, 1])),
        ],
        stamp,
    );
    write(
        &f.join("tool_calls/project=alpha/date=2025-08-12/A.parquet"),
        vec![
            ("tool_call_id", strs(&["t1", "t2", "t3", "t4", "t5"])),
            ("session_id", strs(&["A", "A", "A", "A", "B"])),
            (
                "cycle_id",
                stro(&[
                    Some("A:1"),
                    Some("A:1"),
                    Some("A:1"),
                    Some("A:2"),
                    Some("B:1"),
                ]),
            ),
            ("event_uuid", strs(&["u1", "u2", "u3", "u4", "u5"])),
            (
                "ts",
                ts(&[
                    T0 + 2000,
                    T0 + 3000,
                    T0 + 4000,
                    T0 + 22 * 60_000,
                    T0 + 2 * H + 2000,
                ]),
            ),
            ("tool_name", strs(&["Bash", "Edit", "Read", "Bash", "Bash"])),
            (
                "tool_category",
                dict(&["exec", "edit", "read", "exec", "exec"]),
            ),
            ("is_sidechain", bools(&[false, false, true, false, false])),
            (
                "duration_ms",
                i64o(&[Some(100), Some(50), Some(10), Some(900), None]),
            ),
            ("input_bytes", i64s(&[10, 20, 30, 40, 50])),
            ("result_bytes", i64s(&[100, 200, 300, 400, 500])),
            ("is_error", bools(&[true, false, false, false, false])),
            ("is_denied", bools(&[false, false, false, true, false])),
            (
                "is_interrupted",
                bools(&[false, false, false, false, false]),
            ),
            ("retry_of", stro(&[None, None, None, Some("t1"), None])),
        ],
        stamp,
    );
    write(
        &f.join("file_touches/project=alpha/date=2025-08-12/A.parquet"),
        vec![
            ("session_id", strs(&["A", "A", "A", "B"])),
            (
                "cycle_id",
                stro(&[Some("A:1"), Some("A:1"), Some("A:2"), Some("B:1")]),
            ),
            (
                "ts",
                ts(&[T0 + 3000, T0 + 3500, T0 + 22 * 60_000, T0 + 2 * H + 3000]),
            ),
            ("path_sha256", strs(&["p1", "p2", "p1", "p3"])),
            ("path", stro(&[None, None, None, None])),
            (
                "ext",
                stro(&[Some("rs"), Some("md"), Some("rs"), Some("rs")]),
            ),
            ("op", dict(&["delta", "delta", "delta", "snapshot"])),
        ],
        stamp,
    );
}

fn opts(dir: &Path) -> Options {
    Options {
        dataset_dir: dir.display().to_string(),
        top: 12,
        pricing: None,
        filter_from: None,
        filter_to: None,
        filter_projects: vec![],
        sessions_in_dataset: 3,
    }
}

#[test]
fn totals_and_rankings() {
    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), Some("1.0.0"));
    let ds = load(tmp.path(), &Filter::default()).unwrap();
    assert!(ds.warnings.is_empty(), "{:?}", ds.warnings);
    let r = analyze(&ds, &opts(tmp.path()));
    let t = &r.totals;
    assert_eq!(
        (t.sessions, t.incomplete_sessions, t.cycles, t.api_calls),
        (3, 1, 4, 5)
    );
    // Tool counts come from the tool_calls table, sidechain call included.
    assert_eq!(
        (
            t.tool_calls,
            t.sidechain_tool_calls,
            t.tool_errors,
            t.denials,
            t.retries
        ),
        (5, 1, 1, 1, 1)
    );
    assert_eq!(t.tokens.output, 1600);
    assert_eq!(t.files_touched, 3);
    assert_eq!(t.file_touches, 4);
    assert_eq!(t.active_days, 2);
    assert_eq!(t.agent_active_ms, 110 * 60_000);

    // Projects ranked by active time; label from cwd, slug when none.
    assert_eq!(r.projects[0].label, "beta");
    assert_eq!(r.projects[1].label, "~/src/alpha");
    assert_eq!(r.projects[1].sessions, 2);
    assert_eq!(r.projects[1].files_touched, 3);
    assert_eq!(r.projects[1].tool_calls, 5);

    assert_eq!(r.models[0].model, "claude-opus-5");
    assert_eq!(r.models[0].tokens.output, 1000);
    assert_eq!(r.models.len(), 3);

    assert_eq!(r.tools[0].tool, "Bash");
    assert_eq!(r.tools[0].calls, 3);
    assert_eq!(r.tools[0].duration.n, 2);
    assert_eq!(r.tools[0].duration.p50, 100);
    assert_eq!(r.tools[0].duration.max, 900);
    assert_eq!(r.categories[0].category, "exec");

    let c = &r.cycles;
    assert_eq!(c.cycles, 4);
    assert_eq!(c.prompt_kinds["chat"], 3);
    assert_eq!(c.ended_by["session_end"], 2);
    assert_eq!(
        (
            c.with_edits,
            c.with_rework,
            c.with_sidechains,
            c.interrupted
        ),
        (3, 1, 1, 1)
    );
    assert_eq!(c.first_response_ms.n, 3);
    assert_eq!(c.human_dwell_ms.n, 1);
    assert_eq!(c.dwell_idle, 3);

    // Days are contiguous in local time (Lisbon = UTC+1 in August).
    let dates: Vec<&str> = r.days.iter().map(|d| d.date.as_str()).collect();
    assert_eq!(dates, ["2025-08-12", "2025-08-13"]);
    assert_eq!(r.days[0].cycles, 3);
    assert_eq!(r.days[0].sessions, 2);
    // 12:00Z on a Tuesday is 13:00 local.
    assert_eq!(r.week_hours[1][13], 2); // A:1 and A:2, 20 minutes apart

    assert_eq!(r.friction.error_tools[0].tool, "Bash");
    assert_eq!(r.friction.sessions[0].session_id, "A");
    assert_eq!(r.friction.entrypoints["cli"], 2);
    assert_eq!(r.friction.file_exts[0], ("rs".to_string(), 3));
    assert_eq!(r.sessions[0].session_id, "C");
    assert!(!r.sessions[0].is_complete);
    assert!(r.notes.iter().any(|n| n.contains("2 transcript line")));
    assert!(r.notes.iter().any(|n| n.contains("frame-link")));
    assert!(r.cost.is_none());
}

#[test]
fn filters_by_time_and_project() {
    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), Some("1.0.0"));
    let ds = load(
        tmp.path(),
        &Filter {
            from_ms: Some(T0 + 24 * H),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ds.sessions.len(), 1);
    assert_eq!(ds.sessions[0].session_id, "C");
    assert_eq!(ds.cycles.len(), 1);
    assert_eq!(ds.api_calls.len(), 1);
    assert!(ds.tool_calls.is_empty());

    let ds = load(
        tmp.path(),
        &Filter {
            projects: vec!["ALPHA".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ds.sessions.len(), 2);
    let r = analyze(&ds, &opts(tmp.path()));
    // With the session rollups only for session C gone, tool counts still
    // come from the table.
    assert_eq!(r.totals.tool_calls, 5);

    let ds = load(
        tmp.path(),
        &Filter {
            to_ms: Some(T0 - 1),
            ..Default::default()
        },
    )
    .unwrap();
    let r = analyze(&ds, &opts(tmp.path()));
    assert_eq!(r.totals.sessions, 0);
    for f in [Format::Text, Format::Markdown, Format::Html, Format::Json] {
        assert!(!f.render(&r).is_empty());
    }
}

#[test]
fn cost_with_pricing_table() {
    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), Some("1.0.0"));
    let toml = tmp.path().join("pricing.toml");
    std::fs::write(
        &toml,
        r#"version = "test"
[models."claude-opus"]
input = 15.0
output = 75.0
cache_read = 1.5
cache_write_5m = 18.75
cache_write_1h = 30.0
[models."claude-sonnet-5"]
input = 3.0
output = 15.0
cache_read = 0.3
cache_write_5m = 3.75
cache_write_1h = 6.0
"#,
    )
    .unwrap();
    let ds = load(tmp.path(), &Filter::default()).unwrap();
    let mut o = opts(tmp.path());
    o.pricing = Some(Pricing::load(&toml).unwrap());
    let r = analyze(&ds, &o);
    let c = r.cost.unwrap();
    assert_eq!(c.currency, "USD");
    assert_eq!((c.priced_calls, c.unpriced_calls), (4, 1));
    assert_eq!(c.unpriced_models, vec!["mystery-model".to_string()]);
    // opus (prefix match): 100 in, 1000 out, 10000 cache read, 300 5m writes.
    let opus = 100.0 * 15.0 + 1000.0 * 75.0 + 10_000.0 * 1.5 + 300.0 * 18.75;
    let sonnet = 50.0 * 3.0 + 500.0 * 15.0 + 5000.0 * 0.3 + 200.0 * 6.0;
    let want = (opus + sonnet) * 1e-6;
    assert!((c.total - want).abs() < 1e-9, "{} vs {want}", c.total);
    assert_eq!(c.by_model[0].name, "claude-opus-5");
    assert_eq!(c.by_project[0].name, "~/src/alpha");
    assert!(r.notes.iter().any(|n| n.contains("mystery-model")));
}

#[test]
fn renders_every_format_deterministically() {
    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), Some("1.0.0"));
    let ds = load(tmp.path(), &Filter::default()).unwrap();
    let r = analyze(&ds, &opts(tmp.path()));
    let text = Format::Text.render(&r);
    assert!(text.contains("PROJECTS"));
    assert!(text.contains("~/src/alpha"));
    assert!(text.contains("3 (1 live)"));
    let md = Format::Markdown.render(&r);
    assert!(md.starts_with("# grindstone usage report"));
    assert!(md.contains("## How to read this"));
    assert!(md.contains("| tool calls in sidechains | 1 |"));
    let html = Format::Html.render(&r);
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<svg"));
    assert!(
        !html.contains("http://") && !html.contains("https://"),
        "html must be self-contained"
    );
    let json = Format::Json.render(&r);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["report_version"], "1");
    assert_eq!(v["totals"]["tool_calls"], 5);
    assert_eq!(v["week_hours"].as_array().unwrap().len(), 7);
    // Same input, same bytes.
    let ds2 = load(tmp.path(), &Filter::default()).unwrap();
    assert_eq!(Format::Json.render(&analyze(&ds2, &opts(tmp.path()))), json);
}

#[test]
fn html_escapes_dataset_strings() {
    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), Some("1.0.0"));
    let mut ds = load(tmp.path(), &Filter::default()).unwrap();
    ds.sessions[0].git_branch = Some("<script>alert(1)</script>".into());
    let r = analyze(&ds, &opts(tmp.path()));
    let html = Format::Html.render(&r);
    assert!(!html.contains("<script>alert"));
    assert!(html.contains("&lt;script&gt;alert(1)"));
}

#[test]
fn schema_major_is_enforced_and_missing_stamp_is_a_note() {
    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), Some("2.0.0"));
    let err = load(tmp.path(), &Filter::default())
        .unwrap_err()
        .to_string();
    assert!(err.contains("schema_version 2.0.0"), "{err}");

    let tmp = tempfile::tempdir().unwrap();
    dataset(tmp.path(), None);
    let ds = load(tmp.path(), &Filter::default()).unwrap();
    assert_eq!(ds.sessions.len(), 3);
    assert!(ds
        .warnings
        .iter()
        .any(|w| w.contains("no grindstone.schema_version")));
    let r = analyze(&ds, &opts(tmp.path()));
    assert!(r
        .notes
        .iter()
        .any(|w| w.contains("no grindstone.schema_version")));
}

#[test]
fn missing_manifest_and_missing_columns() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(load(tmp.path(), &Filter::default()).is_err());

    dataset(tmp.path(), Some("1.0.0"));
    // A sessions table with only the required columns still loads.
    let p = tmp
        .path()
        .join("facts/sessions/project=alpha/date=2025-08-12/A.parquet");
    write(
        &p,
        vec![
            ("session_id", strs(&["A"])),
            ("project", strs(&["alpha"])),
            ("start_ts", ts(&[T0])),
            ("end_ts", ts(&[T0 + H])),
        ],
        Some("1.0.0"),
    );
    let ds = load(tmp.path(), &Filter::default()).unwrap();
    assert_eq!(ds.sessions.len(), 1);
    assert!(ds.sessions[0].is_complete);
    let r = analyze(&ds, &opts(tmp.path()));
    assert_eq!(r.totals.tool_calls, 4); // from the tool_calls table, session A only
    assert_eq!(r.projects[0].label, "alpha");
}
