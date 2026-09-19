//! Conformance tests for `cast run` against a fake oneshot and a fake
//! benchmark. No network, no real model. They need a TypeScript runtime (bun
//! or node) and are skipped with a note when neither is installed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn have_runtime() -> bool {
    ["bun", "node"].iter().any(|r| {
        Command::new(r)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

fn cast() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_cast"));
    c.arg("run").stdin(Stdio::null());
    c
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

struct Run {
    out: Output,
    /// One line per oneshot call: its arguments.
    calls: Vec<String>,
    work: String,
}

/// Run tests/fixtures/loop.ai with the fake oneshot and the fake benchmark.
fn run_loop(env: &[(&str, &str)], extra: &[&str]) -> Option<Run> {
    if !have_runtime() {
        eprintln!("skipped: no bun or node on PATH");
        return None;
    }
    let work = tempfile::tempdir().unwrap();
    let log = work.path().join("calls.log");
    let mut c = cast();
    c.arg(fixture("loop.ai")).arg("--oneshot").arg(fixture("fake-oneshot.sh")).args(extra);
    c.env("BENCH", fixture("fake-bench.sh"))
        .env("STATE", work.path().join("state"))
        .env("FAKE_LOG", &log)
        .env("WORK", work.path())
        .env_remove("RUNS")
        .env_remove("USD")
        .env_remove("MODE");
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().unwrap();
    let calls = std::fs::read_to_string(&log).unwrap_or_default().lines().map(String::from).collect();
    Some(Run { out, calls, work: work.path().display().to_string() })
}

#[test]
fn the_agent_runs_until_the_benchmark_resolves() {
    let Some(r) = run_loop(&[], &[]) else { return };
    assert_eq!(r.out.status.code(), Some(0), "{}", err(&r.out));
    let s = text(&r.out);
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 4, "{s}");
    let flags = format!(
        "args: --json --cwd {} -- --model haiku --append-system-prompt Make the loss drop. --allowed-tools Read,Edit",
        r.work
    );
    assert_eq!(lines[0], format!("{flags} | prompt: loss 3: case 3 failing"));
    assert_eq!(lines[1], format!("{flags} | prompt: loss 2: case 2 failing"));
    assert_eq!(lines[2], format!("{flags} | prompt: loss 1: case 1 failing"));
    assert_eq!(lines[3], "resolved 0 0.03");
    assert_eq!(r.calls.len(), 3);
    let e = err(&r.out);
    assert!(e.contains("cast: running"), "{e}");
    assert!(e.contains("ai: bench loss 3 in"), "{e}");
    assert!(e.contains("ai: agent run 1 started"), "{e}");
    assert!(e.contains("ai: agent run 3 done in 0 s, $0.01, 1 turns"), "{e}");
}

#[test]
fn the_run_budget_stops_the_loop() {
    let Some(r) = run_loop(&[("RUNS", "2")], &[]) else { return };
    assert_eq!(r.out.status.code(), Some(0), "{}", err(&r.out));
    assert!(text(&r.out).ends_with("out of budget 1 0.02\n"), "{}", text(&r.out));
    assert_eq!(r.calls.len(), 2);
}

#[test]
fn the_dollar_budget_refuses_to_start_another_run() {
    let Some(r) = run_loop(&[("USD", "0.02")], &[]) else { return };
    assert_eq!(r.out.status.code(), Some(1), "{}", err(&r.out));
    assert!(err(&r.out).contains("ai: budget: $0.02 spent of $0.02"), "{}", err(&r.out));
    assert_eq!(r.calls.len(), 2);
}

#[test]
fn a_metric_line_counts_as_a_result() {
    let Some(r) = run_loop(&[("MODE", "METRIC")], &[]) else { return };
    assert_eq!(r.out.status.code(), Some(0), "{}", err(&r.out));
    assert!(text(&r.out).ends_with("resolved 0 0.03\n"), "{}", text(&r.out));
    assert!(text(&r.out).contains("| prompt: loss 3: \n"), "{}", text(&r.out));
}

#[test]
fn the_agent_flag_reaches_oneshot() {
    let Some(r) = run_loop(&[("RUNS", "1")], &["--agent", "my-agent", "--agent=--flag"]) else { return };
    assert_eq!(r.out.status.code(), Some(0), "{}", err(&r.out));
    assert!(r.calls[0].contains("--agent my-agent --agent --flag -- --model haiku"), "{:?}", r.calls);
}

#[test]
fn a_failing_bench_command_and_a_failing_agent_are_results_not_crashes() {
    if !have_runtime() {
        eprintln!("skipped: no bun or node on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("edge.ai");
    std::fs::write(
        &script,
        r#"import { agent, bench } from "ai"
const b = await bench("echo boom >&2; exit 3")
console.log(b.loss, b.exit, JSON.stringify(b.error))
const m = await bench(["sh", "-c", "echo 'METRIC loss=0.25'"])
console.log(m.loss)
const rec = await agent({}).run("please FAIL")
console.log(rec.ok, rec.error, rec.cost_usd)
"#,
    )
    .unwrap();
    let out = cast().arg(&script).arg("--oneshot").arg(fixture("fake-oneshot.sh")).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert_eq!(
        text(&out),
        "Infinity 3 \"bench exited 3: boom\"\n0.25\nfalse agent reported is_error 0.5\n"
    );
    let e = err(&out);
    assert!(e.contains("boom"), "{e}");
    assert!(e.contains("ai: agent run 1 FAILED after 0 s: agent reported is_error"), "{e}");
}

#[test]
fn a_missing_script_or_oneshot_exits_2() {
    let out = cast().arg("nope.ai").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(err(&out).contains("cannot read nope.ai"), "{}", err(&out));
    let out = cast().arg(fixture("loop.ai")).args(["--oneshot", "/nonexistent/oneshot"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(err(&out).contains("--oneshot /nonexistent/oneshot: no such file"), "{}", err(&out));
}

#[test]
fn no_arguments_at_all_is_a_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_cast")).stdin(Stdio::null()).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(err(&out).contains("cast run <script>"), "{}", err(&out));
}
