//! Conformance tests for `sieve run`: transparent under the cap, exact
//! head/marker/tail over it, exit codes preserved, signals forwarded.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn sieve() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_sieve"));
    c.stdin(Stdio::null());
    c
}

fn run(dir: &Path, id: &str, bounds: (usize, usize, usize), command: &str) -> Output {
    sieve()
        .args(["run", "--cap", &bounds.0.to_string()])
        .args(["--head", &bounds.1.to_string(), "--tail", &bounds.2.to_string()])
        .args(["--spill-dir"])
        .arg(dir)
        .args(["--id", id, "--", command])
        .output()
        .unwrap()
}

fn bare(command: &str) -> Output {
    Command::new("sh")
        .args(["-c", command])
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn spill_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.out"))
}

#[test]
fn under_cap_matches_bare_sh_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = [
        "printf 'a\\nb'",
        "printf 'no newline at all'",
        "echo one; echo two; echo three",
        "printf '\\xff\\xfe\\x00binary\\x01'",
        "true",
        "exit 3",
        "seq 1 100",
        "echo ok; exit 42",
        "printf 'x%.0s' $(seq 1 16384)",
        "nonexistent-command-xyz 2>/dev/null; echo after",
    ];
    for (i, cmd) in corpus.iter().enumerate() {
        let id = format!("c{i}");
        let ours = run(dir.path(), &id, (16384, 8192, 4096), cmd);
        let theirs = bare(cmd);
        assert_eq!(ours.stdout, theirs.stdout, "stdout differs for {cmd:?}");
        assert!(ours.stderr.is_empty(), "stderr not empty for {cmd:?}: {:?}", ours.stderr);
        assert_eq!(ours.status.code(), theirs.status.code(), "exit differs for {cmd:?}");
        assert!(!spill_path(dir.path(), &id).exists(), "spill kept under cap for {cmd:?}");
    }
}

#[test]
fn stderr_is_merged_into_stdout_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), "m", (16384, 8192, 4096), "echo a; echo b >&2; echo c");
    assert_eq!(out.stdout, b"a\nb\nc\n");
    assert!(out.stderr.is_empty());
}

#[test]
fn over_cap_prints_head_marker_tail_and_spills_everything() {
    let dir = tempfile::tempdir().unwrap();
    let cmd = "seq 1 100000; exit 7";
    let out = run(dir.path(), "big", (1000, 300, 200), cmd);
    let full = bare("seq 1 100000").stdout;
    let spill = spill_path(dir.path(), "big");

    assert_eq!(out.status.code(), Some(7));
    assert_eq!(std::fs::read(&spill).unwrap(), full, "spill file is not the full output");

    let text = &out.stdout;
    assert_eq!(&text[..300], &full[..300], "head differs");
    let rest = &text[300..];
    let newline = if full[299] == b'\n' { "" } else { "\n" };
    let expected_marker = format!(
        "{newline}[sieve: {} total, {} not shown; full output in {}; Grep it or Read a range]\n",
        sieve::human(full.len() as u64),
        sieve::human((full.len() - 500) as u64),
        spill.display()
    );
    assert!(
        rest.starts_with(expected_marker.as_bytes()),
        "marker differs:\n{}",
        String::from_utf8_lossy(rest)
    );
    let tail = &rest[expected_marker.len()..];
    assert_eq!(tail, &full[full.len() - 200..], "tail differs");
    assert_eq!(text.len(), 300 + expected_marker.len() + 200);
}

#[test]
fn marker_does_not_add_a_newline_after_one() {
    let dir = tempfile::tempdir().unwrap();
    // head=2 ends exactly on the newline after "a".
    let out = run(dir.path(), "nl", (4, 2, 2), "printf 'a\\nbcdefgh'");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("a\n[sieve: "), "{text:?}");
    assert!(text.ends_with("]\ngh"), "{text:?}");
}

#[test]
fn keep_flag_retains_spill_under_cap() {
    let dir = tempfile::tempdir().unwrap();
    let out = sieve()
        .args(["run", "--keep", "--spill-dir"])
        .arg(dir.path())
        .args(["--id", "k", "--", "echo kept"])
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"kept\n");
    assert_eq!(std::fs::read(spill_path(dir.path(), "k")).unwrap(), b"kept\n");
}

#[test]
fn session_makes_a_subdirectory_and_ids_are_sanitized() {
    let dir = tempfile::tempdir().unwrap();
    let out = sieve()
        .args(["run", "--keep", "--cap", "4", "--head", "2", "--tail", "2", "--spill-dir"])
        .arg(dir.path())
        .args(["--session", "s1", "--id", "a/b", "--", "seq 1 50"])
        .output()
        .unwrap();
    let spill = dir.path().join("s1").join("a_b.out");
    assert!(spill.exists());
    assert!(String::from_utf8_lossy(&out.stdout).contains(&spill.display().to_string()));
}

#[test]
fn several_words_are_joined_into_one_command() {
    let dir = tempfile::tempdir().unwrap();
    let out = sieve()
        .args(["run", "--spill-dir"])
        .arg(dir.path())
        .args(["--", "echo", "x", "&&", "echo", "y"])
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"x\ny\n");
}

#[test]
fn signal_exit_code_is_128_plus_signal() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), "sig", (16384, 8192, 4096), "kill -TERM $$");
    assert_eq!(out.status.code(), Some(143));
}

#[test]
fn sigterm_is_forwarded_and_the_runner_exits_promptly() {
    let dir = tempfile::tempdir().unwrap();
    let child = sieve()
        .args(["run", "--spill-dir"])
        .arg(dir.path())
        .args(["--keep", "--id", "slow", "--", "sleep 30"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // The spill file appears once the handler is installed; wait for it.
    let spill = spill_path(dir.path(), "slow");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !spill.exists() {
        assert!(Instant::now() < deadline, "runner did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    let started = Instant::now();
    let ok = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let out = child.wait_with_output().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "runner did not exit after SIGTERM"
    );
    assert_eq!(out.status.code(), Some(143), "expected the child's signal status");
    assert!(out.stdout.is_empty());
}

#[test]
fn bad_bounds_are_a_usage_error() {
    let out = sieve()
        .args(["run", "--cap", "10", "--head", "8", "--tail", "8", "--", "true"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--cap 10 is smaller"));
}

#[test]
fn unwritable_spill_dir_still_runs_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"").unwrap();
    let out = run(&file, "x", (4, 2, 2), "echo still runs; exit 5");
    assert_eq!(out.status.code(), Some(5));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("st\n[sieve: 11 B total, 7 B not shown; full output not saved: "), "{text:?}");
}
