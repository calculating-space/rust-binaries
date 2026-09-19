//! Conformance tests for the `cast` CLI against a fake agent that speaks
//! the `claude -p --output-format json` shape. No network, no real model.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

fn fake_agent() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agent.sh")
}

fn cast(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_cast"));
    c.arg("--agent").arg(fake_agent()).arg("--prompts").arg(dir);
    c.stdin(Stdio::null());
    c
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn one_agent_per_item_in_list_order_with_headings() {
    let dir = tempfile::tempdir().unwrap();
    let out = cast(dir.path()).arg("[paper1, paper2].map(p => researcher)").output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let s = text(&out);
    // built-in prompt: the item is on the first line, the fake echoes the last line
    assert!(s.starts_with("## researcher(paper1)\n\necho: "), "{s}");
    assert!(s.contains("## researcher(paper2)\n\necho: "), "{s}");
    assert!(s.find("paper1").unwrap() < s.find("paper2").unwrap());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("2 x researcher (built-in prompt), 2 at a time"), "{err}");
    assert!(err.contains("2 of 2 ok"), "{err}");
}

#[test]
fn role_prompt_file_is_used_with_the_item_substituted() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("reviewer.md"), "Review {x} carefully.\nAnswer: {x}").unwrap();
    let out = cast(dir.path()).args(["--dry-run", "[a.pdf, b.pdf].map(x => reviewer)"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let s = text(&out);
    assert_eq!(
        s,
        "## reviewer(a.pdf)\n\nReview a.pdf carefully.\nAnswer: a.pdf\n\n\
## reviewer(b.pdf)\n\nReview b.pdf carefully.\nAnswer: b.pdf\n\n"
    );
    // and for real: the fake echoes the prompt's last line
    let out = cast(dir.path()).arg("[a.pdf].map(x => reviewer)").output().unwrap();
    assert_eq!(text(&out), "## reviewer(a.pdf)\n\necho: Answer: a.pdf\n\n");
}

#[test]
fn prompt_without_placeholder_gets_the_item_appended() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("r.md"), "Do the thing.\n").unwrap();
    let out = cast(dir.path()).args(["--dry-run", "[one].map(p => r)"]).output().unwrap();
    assert_eq!(text(&out), "## r(one)\n\nDo the thing.\n\nInput: one\n\n");
}

#[test]
fn agents_run_in_parallel_unless_jobs_caps_them() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("s.md"), "SLEEP {p}").unwrap();
    let t = Instant::now();
    let out = cast(dir.path()).arg("[a,b,c].map(p => s)").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parallel = t.elapsed();
    assert!(parallel.as_millis() < 2500, "three 1 s agents took {parallel:?}; not parallel");

    let t = Instant::now();
    let out = cast(dir.path()).args(["--jobs", "1", "[a,b,c].map(p => s)"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let serial = t.elapsed();
    assert!(serial.as_millis() >= 3000, "with --jobs 1 they should run one after another: {serial:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 at a time"));
}

#[test]
fn a_failed_agent_is_reported_and_the_others_still_print() {
    let dir = tempfile::tempdir().unwrap();
    let out = cast(dir.path()).arg("[FAIL, fine].map(p => r)").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let s = text(&out);
    assert!(s.contains("## r(FAIL)\n\n(failed: agent reported is_error)\nit went wrong\n"), "{s}");
    assert!(s.contains("## r(fine)\n\necho: "), "{s}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("r(FAIL) FAILED"), "{err}");
    assert!(err.contains("1 of 2 ok, $0.51"), "{err}");
}

#[test]
fn out_dir_gets_one_markdown_and_one_record_per_item() {
    let dir = tempfile::tempdir().unwrap();
    let results = dir.path().join("results");
    let out = cast(dir.path())
        .arg("--out").arg(&results)
        .arg("[papers/one.pdf, two].map(p => researcher)")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let one = results.join("researcher-papers-one-pdf.md");
    let two = results.join("researcher-two.md");
    assert_eq!(text(&out), format!("{}\n{}\n", one.display(), two.display()));
    assert!(std::fs::read_to_string(&one).unwrap().starts_with("echo: "));
    let rec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(results.join("researcher-two.json")).unwrap()).unwrap();
    assert_eq!(rec["item"], "two");
    assert_eq!(rec["role"], "researcher");
    assert_eq!(rec["ok"], true);
    assert_eq!(rec["cost_usd"], 0.01);
    assert_eq!(rec["version"], "oneshot/1");
}

#[test]
fn json_prints_an_array_of_records_in_list_order() {
    let dir = tempfile::tempdir().unwrap();
    let out = cast(dir.path()).args(["--json", "[x, y].map(p => r)"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["item"], "x");
    assert_eq!(arr[1]["item"], "y");
    assert_eq!(arr[1]["num_turns"], 1);
    assert!(arr[0]["prompt"].as_str().unwrap().contains("You are the r."));
}

#[test]
fn cwd_and_extra_agent_args_pass_through() {
    let dir = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("w.md"), "CWD").unwrap();
    let out = cast(dir.path())
        .arg("--cwd").arg(work.path())
        .args(["[a].map(p => w)", "--", "--model", "haiku"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let s = text(&out);
    assert!(s.contains(&work.path().canonicalize().unwrap().display().to_string()), "{s}");
    let out = cast(dir.path()).args(["--json", "[a].map(p => w)", "--", "--model", "haiku"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let agent = v[0]["agent"].as_array().unwrap();
    assert_eq!(agent[agent.len() - 2], "--model");
    assert_eq!(agent[agent.len() - 1], "haiku");
}

#[test]
fn bad_expression_exits_2_with_a_hint() {
    let dir = tempfile::tempdir().unwrap();
    let out = cast(dir.path()).arg("paper1, paper2").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("[a, b].map(p => role)"));
    let out = cast(dir.path()).arg("[a, b]").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("expected .map("));
}
