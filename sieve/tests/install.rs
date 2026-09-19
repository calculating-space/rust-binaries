//! `sieve install` / `uninstall` against a temporary HOME.

use serde_json::{Value, json};
use std::process::{Command, Output};

fn sieve(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sieve"))
        .args(args)
        .env("HOME", home)
        .current_dir(home)
        .output()
        .unwrap()
}

fn settings(home: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(home.join(".claude/settings.json")).unwrap()).unwrap()
}

#[test]
fn dry_run_shows_diff_and_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    let existing = json!({"permissions": {"allow": ["Bash(ls:*)"]}});
    std::fs::write(home.path().join(".claude/settings.json"), existing.to_string()).unwrap();
    let out = sieve(home.path(), &["install", "--dry-run", "--command", "/opt/bin/sieve hook"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    let added: Vec<&str> = text.lines().filter(|l| l.starts_with('+')).collect();
    assert!(added.iter().any(|l| l.contains("\"matcher\": \"Bash\"")), "{text}");
    assert!(added.iter().any(|l| l.contains("\"command\": \"/opt/bin/sieve hook PostToolUse\"")), "{text}");
    assert!(!text.lines().any(|l| l.starts_with('-') && l.contains("Bash(ls:*)")), "{text}");
    assert!(text.contains("(dry run: nothing written)"));
    assert_eq!(settings(home.path()), existing);
}

#[test]
fn install_is_idempotent_and_uninstall_restores() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    let existing = json!({"hooks": {"PostToolUse": [{"matcher": "Edit", "hooks": [{"type": "command", "command": "fmt"}]}]}, "model": "opus"});
    std::fs::write(home.path().join(".claude/settings.json"), existing.to_string()).unwrap();

    let out = sieve(home.path(), &["install", "--command", "sieve hook"]);
    assert_eq!(out.status.code(), Some(0));
    let s = settings(home.path());
    assert_eq!(s["model"], "opus");
    assert_eq!(s["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);
    assert_eq!(s["hooks"]["PostToolUse"][1]["hooks"][0]["command"], "sieve hook PostToolUse");
    assert_eq!(s["hooks"]["PostToolUse"][1]["hooks"][0]["timeout"], 5);

    let out = sieve(home.path(), &["install", "--command", "sieve hook"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("already installed"));
    assert_eq!(settings(home.path()), s);

    let out = sieve(home.path(), &["uninstall"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(settings(home.path()), existing);
}

#[test]
fn default_command_is_this_binary() {
    let home = tempfile::tempdir().unwrap();
    let out = sieve(home.path(), &["install", "--dry-run"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains(env!("CARGO_BIN_EXE_sieve")), "{text}");
}

#[test]
fn project_flag_targets_local_settings() {
    let home = tempfile::tempdir().unwrap();
    let out = sieve(home.path(), &["install", "--project", "--command", "sieve hook"]);
    assert_eq!(out.status.code(), Some(0));
    let p = home.path().join(".claude/settings.json");
    assert!(p.exists());
    assert!(!home.path().join("x").exists());
}

#[test]
fn invalid_settings_json_is_an_error_not_an_overwrite() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/settings.json"), "{ not json").unwrap();
    let out = sieve(home.path(), &["install", "--command", "sieve hook"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(home.path().join(".claude/settings.json")).unwrap(), "{ not json");
}
