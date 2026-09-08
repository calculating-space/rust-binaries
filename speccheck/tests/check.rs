use speccheck::*;
use std::collections::BTreeMap;

fn m1max() -> Spec {
    Spec {
        version: 1,
        os: "macos".into(),
        arch: "aarch64".into(),
        chip: "Apple M1 Max".into(),
        cores: 10,
        memory_gb: 64,
        disk_free_gb: 300,
        disk_path: "/Users/x".into(),
        gpus: vec![Gpu { kind: GpuKind::Metal, name: "Apple M1 Max (unified memory)".into(), memory_gb: None }],
        tools: BTreeMap::from([("uv".to_string(), "uv 0.11".to_string())]),
    }
}

fn reqs() -> Requirements {
    Requirements {
        version: 1,
        subject: "mlx".into(),
        rules: vec![
            Rule { check: Check::Platform { any_of: vec![("macos".into(), "aarch64".into()), ("linux".into(), "x86_64".into())] }, severity: Severity::Blocks, why: "no wheels elsewhere".into() },
            Rule { check: Check::Gpu { any_of: vec![GpuKind::Metal] }, severity: Severity::Pointless, why: "CPU-only otherwise".into() },
            Rule { check: Check::Tool { name: "uv".into() }, severity: Severity::Blocks, why: "builds the env".into() },
            Rule { check: Check::MinMemoryGb { gb: 8 }, severity: Severity::Blocks, why: "3B model".into() },
            Rule { check: Check::MinMemoryGb { gb: 16 }, severity: Severity::Recommended, why: "8B model".into() },
        ],
        tiers: vec![
            Tier { name: "small".into(), enables: "3B".into(), checks: vec![Check::MinMemoryGb { gb: 8 }, Check::MinDiskFreeGb { gb: 4 }] },
            Tier { name: "huge".into(), enables: "70B".into(), checks: vec![Check::MinMemoryGb { gb: 128 }, Check::MinDiskFreeGb { gb: 45 }] },
        ],
    }
}

#[test]
fn m1_max_can_run_but_not_the_huge_tier() {
    let v = check(&m1max(), &reqs());
    assert_eq!(v.outcome, Outcome::CanRun);
    assert!(v.findings.iter().all(|f| f.pass));
    assert!(v.tiers[0].ok);
    assert!(!v.tiers[1].ok);
    assert_eq!(v.tiers[1].missing, vec!["128 GB memory"]);
    let text = render(&v);
    assert!(text.starts_with("mlx: CAN RUN"));
    assert!(text.contains("ok  small"));
    assert!(text.contains("--  huge") && text.contains("needs 128 GB memory"));
    assert_eq!(v.outcome.exit_code(), 0);
}

#[test]
fn intel_mac_cannot_run() {
    let mut s = m1max();
    s.arch = "x86_64".into();
    s.gpus.clear();
    let v = check(&s, &reqs());
    assert_eq!(v.outcome, Outcome::CannotRun);
    assert_eq!(v.outcome.exit_code(), 3);
    assert!(render(&v).contains("CANNOT RUN"));
}

#[test]
fn linux_without_gpu_is_pointless_not_blocked() {
    let mut s = m1max();
    s.os = "linux".into();
    s.arch = "x86_64".into();
    s.gpus.clear();
    let v = check(&s, &reqs());
    assert_eq!(v.outcome, Outcome::Pointless);
    assert_eq!(v.outcome.exit_code(), 2);
    let gpu = v.findings.iter().find(|f| matches!(f.check, Check::Gpu { .. })).unwrap();
    assert!(!gpu.pass);
    assert_eq!(gpu.actual, "no GPU");
}

#[test]
fn recommended_failures_do_not_change_outcome() {
    let mut s = m1max();
    s.memory_gb = 8;
    let v = check(&s, &reqs());
    assert_eq!(v.outcome, Outcome::CanRun);
    assert_eq!(v.findings.iter().filter(|f| !f.pass).count(), 1);
}

#[test]
fn missing_tool_blocks() {
    let mut s = m1max();
    s.tools.clear();
    let v = check(&s, &reqs());
    assert_eq!(v.outcome, Outcome::CannotRun);
    assert_eq!(reqs().tool_names(), vec!["uv"]);
}

#[test]
fn contracts_roundtrip_through_json() {
    let r = reqs();
    let back: Requirements = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back, r);
    let v = check(&m1max(), &r);
    let back: Verdict = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
    assert_eq!(back, v);
    // the wire shape stays readable
    let j = serde_json::to_value(&r).unwrap();
    assert_eq!(j["rules"][0]["check"]["kind"], "platform");
    assert_eq!(j["rules"][0]["severity"], "blocks");
}

#[test]
fn detect_produces_a_plausible_spec() {
    let s = spec::detect(std::path::Path::new("."), &["cargo".into(), "definitely-not-a-tool-xyz".into()]);
    assert_eq!(s.version, CONTRACT_VERSION);
    assert!(s.cores >= 1);
    assert!(s.memory_gb >= 1);
    assert!(s.disk_free_gb >= 1);
    assert!(s.tools.contains_key("cargo"));
    assert!(!s.tools.contains_key("definitely-not-a-tool-xyz"));
}

#[test]
fn disk_free_walks_up_to_an_existing_ancestor() {
    let tmp = tempfile_dir();
    let missing = tmp.join("not").join("yet").join("created");
    assert_eq!(spec::disk_free_gb(&missing), spec::disk_free_gb(&tmp));
    assert!(spec::disk_free_gb(&missing) >= 1);
}

fn tempfile_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

#[test]
fn render_keeps_columns_aligned_with_long_tool_versions() {
    let mut s = m1max();
    s.tools.insert("uv".into(), "uv 0.11.28 (Homebrew 2026-07-07 aarch64-apple-darwin)".into());
    let text = render(&check(&s, &reqs()));
    let line = text.lines().find(|l| l.contains("`uv` installed")).unwrap();
    assert!(line.contains("uv 0.11.28 (H…"), "{line}");
    assert!(line.ends_with("builds the env"));
}
