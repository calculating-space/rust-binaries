//! `sieve hook` against recorded Claude Code hook inputs (tests/fixtures,
//! redacted real stdin). Every path must exit 0; only Bash results over the
//! cap produce a decision.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fixture(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn run_hook(home: &Path, event: &str, stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sieve"))
        .args(["hook", event])
        .env("SIEVE_HOME", home)
        .env_remove("SIEVE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn decision(out: &Output) -> Option<Value> {
    assert_eq!(out.status.code(), Some(0), "hook must exit 0; stderr: {}", String::from_utf8_lossy(&out.stderr));
    if out.stdout.is_empty() {
        return None;
    }
    Some(serde_json::from_slice(&out.stdout).expect("stdout is JSON"))
}

fn ledger_lines(home: &Path, session: &str) -> Vec<Value> {
    let p = home.join("ledger").join(format!("{session}.jsonl"));
    match std::fs::read_to_string(p) {
        Ok(t) => t.lines().map(|l| serde_json::from_str(l).unwrap()).collect(),
        Err(_) => Vec::new(),
    }
}

fn updated_stdout(d: &Value) -> String {
    d["hookSpecificOutput"]["updatedToolOutput"]["stdout"].as_str().unwrap().to_string()
}

#[test]
fn small_bash_result_passes_and_is_observed() {
    let home = tempfile::tempdir().unwrap();
    let input = fixture("posttooluse-bash-small.json");
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    assert!(decision(&out).is_none());
    let lines = ledger_lines(home.path(), input["session_id"].as_str().unwrap());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["action"], "observe");
    assert_eq!(lines[0]["rule"], "bash-spill");
    assert_eq!(lines[0]["output_bytes"], 5);
    assert_eq!(lines[0]["v"], "1");
    assert_eq!(lines[0]["prompt_id"], input["prompt_id"]);
}

#[test]
fn persisted_bash_result_is_folded_with_tail_from_the_file() {
    let home = tempfile::tempdir().unwrap();
    let mut input = fixture("posttooluse-bash-persisted.json");
    // Recreate the persisted file the CLI wrote: the full seq 1 20000 output.
    let full: String = (1..=20000).map(|i| format!("{i}\n")).collect();
    let persisted = home.path().join("persisted.txt");
    std::fs::write(&persisted, &full).unwrap();
    assert_eq!(full.len() as u64, input["tool_response"]["persistedOutputSize"].as_u64().unwrap());
    input["tool_response"]["persistedOutputPath"] = json!(persisted.display().to_string());

    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    let d = decision(&out).expect("a decision");
    let hso = &d["hookSpecificOutput"];
    assert_eq!(hso["hookEventName"], "PostToolUse");
    let resp = &hso["updatedToolOutput"];
    assert!(resp.get("persistedOutputPath").is_none());
    assert!(resp.get("persistedOutputSize").is_none());
    assert_eq!(resp["interrupted"], false);
    assert_eq!(resp["isImage"], false);
    assert_eq!(resp["noOutputExpected"], false);
    assert_eq!(resp["stderr"], "");

    let text = updated_stdout(&d);
    let head = &full[..8192];
    assert!(text.starts_with(head), "head is not the first 8192 bytes");
    let tail = &full[full.len() - 4096..];
    assert!(text.ends_with(tail), "tail is not the last 4096 bytes of the persisted file");
    let middle = &text[head.len()..text.len() - tail.len()];
    let expected = format!(
        "{}[sieve: 106.3 KB total, 94.3 KB not shown; full output in {}; Grep it or Read a range]\n",
        if head.ends_with('\n') { "" } else { "\n" },
        persisted.display()
    );
    assert_eq!(middle, expected);

    let lines = ledger_lines(home.path(), input["session_id"].as_str().unwrap());
    assert_eq!(lines[0]["action"], "rewrite");
    assert_eq!(lines[0]["persisted"], true);
    assert_eq!(lines[0]["output_bytes"], 108894);
    assert_eq!(lines[0]["shown_bytes"], 8192 + 4096);
    // No spill copy: the CLI's file is the spill.
    assert!(!home.path().join("spill").exists());
}

#[test]
fn in_memory_result_over_cap_is_spilled_and_folded() {
    let home = tempfile::tempdir().unwrap();
    let mut input = fixture("posttooluse-bash-small.json");
    let big: String = (0..3000).map(|i| format!("line {i:05}\n")).collect(); // 30 KB
    input["tool_response"]["stdout"] = json!(big);
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    let d = decision(&out).expect("a decision");
    let text = updated_stdout(&d);
    let session = input["session_id"].as_str().unwrap();
    let id = input["tool_use_id"].as_str().unwrap();
    let spill: PathBuf = home.path().join("spill").join(session).join(format!("{id}.out"));
    assert_eq!(std::fs::read_to_string(&spill).unwrap(), big);
    assert!(text.starts_with(&big[..8192]));
    assert!(text.ends_with(&big[big.len() - 4096..]));
    assert!(text.contains(&format!("full output in {}", spill.display())));
    let lines = ledger_lines(home.path(), session);
    assert_eq!(lines[0]["spilled_bytes"], (big.len() - 8192 - 4096) as u64);
}

#[test]
fn long_stderr_is_folded_separately() {
    let home = tempfile::tempdir().unwrap();
    let mut input = fixture("posttooluse-bash-small.json");
    let noisy = "warning: something\n".repeat(2000);
    input["tool_response"]["stderr"] = json!(noisy);
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    let d = decision(&out).expect("a decision");
    let resp = &d["hookSpecificOutput"]["updatedToolOutput"];
    assert_eq!(resp["stdout"], "hello");
    let err = resp["stderr"].as_str().unwrap();
    assert!(err.contains("[sieve: ") && err.contains(".err; Grep it"));
}

#[test]
fn unreadable_persisted_file_still_folds_from_the_prefix() {
    let home = tempfile::tempdir().unwrap();
    let input = fixture("posttooluse-bash-persisted.json"); // redacted path, does not exist
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    let d = decision(&out).expect("a decision");
    let text = updated_stdout(&d);
    assert!(text.contains("full output not saved: persisted output file unreadable]"));
}

#[test]
fn other_tools_and_events_are_silent() {
    let home = tempfile::tempdir().unwrap();
    for (event, name) in [
        ("PostToolUse", "posttooluse-read.json"),
        ("PreToolUse", "pretooluse-bash.json"),
        ("SessionStart", "sessionstart.json"),
        ("UserPromptSubmit", "userpromptsubmit.json"),
        ("Stop", "stop.json"),
    ] {
        let out = run_hook(home.path(), event, &fixture(name).to_string());
        assert!(decision(&out).is_none(), "{event} {name} produced output");
    }
    assert!(!home.path().join("ledger").exists() || ledger_lines(home.path(), "x").is_empty());
}

#[test]
fn fails_open_on_garbage() {
    let home = tempfile::tempdir().unwrap();
    for stdin in ["", "not json", "{\"tool_name\": \"Bash\"}", "{\"tool_name\": \"Bash\", \"tool_response\": \"a string\"}", "[1,2,3]"] {
        let out = run_hook(home.path(), "PostToolUse", stdin);
        assert_eq!(out.status.code(), Some(0), "stdin {stdin:?}");
        assert!(out.stdout.is_empty(), "stdin {stdin:?} produced output");
    }
    let out = run_hook(home.path(), "NoSuchEvent", &fixture("posttooluse-bash-small.json").to_string());
    assert!(decision(&out).is_none());
}

#[test]
fn policy_file_changes_the_cap_and_off_disables() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("policy.toml"), "version = \"1\"\n[bash]\ncap_bytes = 100\nhead_bytes = 40\ntail_bytes = 20\n").unwrap();
    let mut input = fixture("posttooluse-bash-small.json");
    input["tool_response"]["stdout"] = json!("x".repeat(200));
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    let text = updated_stdout(&decision(&out).unwrap());
    assert!(text.starts_with(&"x".repeat(40)));
    assert!(text.contains("[sieve: 200 B total, 140 B not shown;"));

    std::fs::write(home.path().join("policy.toml"), "enabled = false\n").unwrap();
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    assert!(decision(&out).is_none());

    // A broken policy file warns and falls back to defaults: 200 B is under 16 KB.
    std::fs::write(home.path().join("policy.toml"), "[bash]\nnonsense = 1\n").unwrap();
    let out = run_hook(home.path(), "PostToolUse", &input.to_string());
    assert!(decision(&out).is_none());
    assert!(String::from_utf8_lossy(&out.stderr).contains("policy"));
}

#[test]
fn env_off_disables() {
    let home = tempfile::tempdir().unwrap();
    let mut input = fixture("posttooluse-bash-small.json");
    input["tool_response"]["stdout"] = json!("y".repeat(50_000));
    let out = Command::new(env!("CARGO_BIN_EXE_sieve"))
        .args(["hook", "PostToolUse"])
        .env("SIEVE_HOME", home.path())
        .env("SIEVE", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(input.to_string().as_bytes())?;
            c.wait_with_output()
        })
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}
