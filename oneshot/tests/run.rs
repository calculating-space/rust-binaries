//! Conformance tests for the `oneshot` CLI against a fake agent that speaks
//! the `claude -p --output-format json` shape. No network, no real model.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

fn fake_agent() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agent.sh")
}

fn oneshot() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oneshot"));
    c.arg("--agent").arg(fake_agent());
    c.stdin(Stdio::null());
    c
}

fn record(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).expect("stdout is a JSON record")
}

#[test]
fn prints_the_result_text_with_a_trailing_newline() {
    let out = oneshot().arg("hello there").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"echo: hello there\n");
    assert!(out.stderr.is_empty(), "quiet on success: {:?}", out.stderr);
}

#[test]
fn json_record_carries_the_lifted_fields_and_raw_output() {
    let out = oneshot().args(["--json", "hello"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let r = record(&out);
    assert_eq!(r["version"], "oneshot/1");
    assert_eq!(r["prompt"], "hello");
    assert_eq!(r["ok"], true);
    assert_eq!(r["exit_code"], 0);
    assert_eq!(r["timed_out"], false);
    assert_eq!(r["result"], "echo: hello");
    assert_eq!(r["session_id"], "s-1");
    assert_eq!(r["cost_usd"], 0.01);
    assert_eq!(r["num_turns"], 1);
    assert_eq!(r["agent_output"]["usage"]["output_tokens"], 3);
    assert_eq!(r["agent"][0], fake_agent().display().to_string());
    assert!(r.get("error").is_none(), "no error key on success");
    assert!(r["started_unix"].as_u64().unwrap() > 1_700_000_000);
}

#[test]
fn prompt_from_stdin_and_from_file() {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oneshot"));
    c.arg("--agent").arg(fake_agent()).stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), b"from stdin").unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.stdout, b"echo: from stdin\n");

    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("prompt.md");
    std::fs::write(&f, "from a file\n").unwrap();
    let out = oneshot().arg("--file").arg(&f).output().unwrap();
    assert_eq!(out.stdout, b"echo: from a file\n");
}

#[test]
fn no_prompt_is_a_usage_error() {
    let out = oneshot().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("prompt is empty"), "stdin is /dev/null here");
    let out = oneshot().arg("   ").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn out_file_gets_the_result_and_stdout_stays_empty() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("answer.txt");
    let out = oneshot().arg("--out").arg(&f).arg("hi").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "echo: hi");
}

#[test]
fn record_file_is_written_even_when_the_agent_fails() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("run.json");
    let out = oneshot().arg("--record").arg(&f).arg("please FAIL").output().unwrap();
    assert_eq!(out.status.code(), Some(1), "agent's own exit status");
    let r: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert_eq!(r["ok"], false);
    assert_eq!(r["exit_code"], 1);
    assert_eq!(r["error"], "agent reported is_error");
    assert_eq!(r["result"], "it went wrong");
    assert_eq!(r["session_id"], "s-fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("is_error"), "failure reason on stderr: {err}");
    assert_eq!(out.stdout, b"it went wrong\n", "the agent's text is still printed");
}

#[test]
fn is_error_with_exit_zero_still_fails() {
    let out = oneshot().args(["--json", "SOFTFAIL"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let r = record(&out);
    assert_eq!(r["exit_code"], 0);
    assert_eq!(r["ok"], false);
}

#[test]
fn unparsable_output_fails_and_keeps_the_raw_text() {
    let out = oneshot().args(["--json", "GARBAGE"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let r = record(&out);
    assert_eq!(r["ok"], false);
    assert_eq!(r["result"], "this is not json\n");
    assert!(r["error"].as_str().unwrap().contains("not a JSON object"));
    assert!(r.get("agent_output").is_none());
}

#[test]
fn text_mode_takes_stdout_verbatim() {
    let out = oneshot().args(["--text", "GARBAGE"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"this is not json\n");
}

#[test]
fn agent_stderr_is_hidden_on_success_unless_verbose() {
    let out = oneshot().arg("NOISY").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, b"fine\n");
    let out = oneshot().args(["-v", "NOISY"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stderr), "warning: something on stderr\n");
    let r = record(&oneshot().args(["--json", "NOISY"]).output().unwrap());
    assert_eq!(r["stderr"], "warning: something on stderr\n");
}

#[test]
fn timeout_kills_the_agent_and_exits_124() {
    let start = Instant::now();
    let out = oneshot().args(["--timeout", "1", "--json", "SLOW"]).output().unwrap();
    assert_eq!(out.status.code(), Some(124));
    assert!(start.elapsed().as_secs() < 5, "the sleeping child was killed with its group");
    let r = record(&out);
    assert_eq!(r["timed_out"], true);
    assert_eq!(r["ok"], false);
    assert!(r["exit_code"].is_null(), "died from a signal");
    assert!(r["error"].as_str().unwrap().contains("killed after 1 s"));
}

#[test]
fn cwd_and_extra_args_reach_the_agent() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().canonicalize().unwrap();
    let out = oneshot().arg("--cwd").arg(&real).args(["--json", "CWD"]).output().unwrap();
    let r = record(&out);
    assert_eq!(r["result"], real.display().to_string());
    assert_eq!(r["cwd"], real.display().to_string());

    let out = oneshot()
        .args(["--json", "hello", "--", "--model", "haiku"])
        .output()
        .unwrap();
    let r = record(&out);
    let agent: Vec<&str> = r["agent"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(&agent[1..], ["--model", "haiku"]);
}

#[test]
fn missing_agent_is_a_setup_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_oneshot"))
        .args(["--agent", "/nonexistent/agent-xyz", "hi"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot run /nonexistent/agent-xyz"));
}

#[test]
fn default_agent_is_headless_claude() {
    assert_eq!(oneshot::default_agent(), ["claude", "-p", "--output-format", "json"]);
}
