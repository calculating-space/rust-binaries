use speccheck::*;
use std::collections::BTreeMap;

fn m1max() -> Spec {
    Spec {
        version: CONTRACT_VERSION,
        os: "macos".into(),
        arch: "aarch64".into(),
        chip: "Apple M1 Max".into(),
        cores: 10,
        memory_gb: 64,
        disk_free_gb: 300,
        disk_path: "/Users/x".into(),
        gpus: vec![Gpu {
            kind: GpuKind::Metal,
            name: "Apple M1 Max (unified memory)".into(),
            memory_gb: None,
        }],
        tools: BTreeMap::from([("uv".to_string(), "uv 0.11".to_string())]),
        audio_inputs: vec!["MacBook Pro Microphone".into()],
        voices: vec!["en_US".into(), "fr_CA".into(), "fr_FR".into()],
    }
}

fn reqs() -> Requirements {
    Requirements {
        version: CONTRACT_VERSION,
        subject: "mlx".into(),
        rules: vec![
            Rule {
                check: Check::Platform {
                    any_of: vec![
                        ("macos".into(), "aarch64".into()),
                        ("linux".into(), "x86_64".into()),
                    ],
                },
                severity: Severity::Blocks,
                why: "no wheels elsewhere".into(),
            },
            Rule {
                check: Check::Gpu {
                    any_of: vec![GpuKind::Metal],
                },
                severity: Severity::Pointless,
                why: "CPU-only otherwise".into(),
            },
            Rule {
                check: Check::Tool { name: "uv".into() },
                severity: Severity::Blocks,
                why: "builds the env".into(),
            },
            Rule {
                check: Check::MinMemoryGb { gb: 8 },
                severity: Severity::Blocks,
                why: "3B model".into(),
            },
            Rule {
                check: Check::MinMemoryGb { gb: 16 },
                severity: Severity::Recommended,
                why: "8B model".into(),
            },
        ],
        tiers: vec![
            Tier {
                name: "small".into(),
                enables: "3B".into(),
                checks: vec![Check::MinMemoryGb { gb: 8 }, Check::MinDiskFreeGb { gb: 4 }],
            },
            Tier {
                name: "huge".into(),
                enables: "70B".into(),
                checks: vec![
                    Check::MinMemoryGb { gb: 128 },
                    Check::MinDiskFreeGb { gb: 45 },
                ],
            },
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
    let gpu = v
        .findings
        .iter()
        .find(|f| matches!(f.check, Check::Gpu { .. }))
        .unwrap();
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
    let probes = Probes {
        tools: vec![
            "cargo".into(),
            "true".into(),
            "definitely-not-a-tool-xyz".into(),
        ],
        ..Default::default()
    };
    let s = spec::detect(std::path::Path::new("."), &probes);
    assert_eq!(s.version, CONTRACT_VERSION);
    assert!(s.cores >= 1);
    assert!(s.memory_gb >= 1);
    assert!(s.disk_free_gb >= 1);
    assert!(s.tools.contains_key("cargo"));
    // on PATH, but prints nothing useful for --version: still counts as installed
    assert!(s.tools.contains_key("true"));
    assert!(!s.tools.contains_key("definitely-not-a-tool-xyz"));
    // not asked for, not probed
    assert!(s.voices.is_empty() && s.audio_inputs.is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn detect_finds_voices_a_microphone_and_versionless_tools_on_a_mac() {
    let s = spec::detect(
        std::path::Path::new("."),
        &Probes::everything(vec!["say".into()]),
    );
    assert_eq!(s.tools.get("say").map(String::as_str), Some("found"));
    assert!(
        s.voices.iter().any(|v| v.starts_with("en_")),
        "voices: {:?}",
        s.voices
    );
    assert!(
        evaluate(
            &s,
            &Check::Voice {
                language: "en".into()
            }
        )
        .0
    );
    // every Mac that can run this has a built-in microphone, external or not
    assert!(
        !s.audio_inputs.is_empty(),
        "audio inputs: {:?}",
        s.audio_inputs
    );
}

#[test]
fn microphone_and_voice_checks() {
    let s = m1max();
    assert_eq!(
        evaluate(&s, &Check::AudioInput),
        (true, "MacBook Pro Microphone".into())
    );
    let fr = Check::Voice {
        language: "fr".into(),
    };
    assert_eq!(evaluate(&s, &fr), (true, "fr_CA, fr_FR".into()));
    let fr_ca = Check::Voice {
        language: "fr_CA".into(),
    };
    assert_eq!(evaluate(&s, &fr_ca), (true, "fr_CA".into()));
    let de = Check::Voice {
        language: "de".into(),
    };
    assert_eq!(evaluate(&s, &de), (false, "none of 3 voices".into()));
    let mut s = s;
    s.audio_inputs.clear();
    assert_eq!(
        evaluate(&s, &Check::AudioInput),
        (false, "no microphone".into())
    );
    assert_eq!(need(&Check::AudioInput), "a microphone");
    assert_eq!(need(&fr), "a `fr` speech voice");
    // wire shape
    assert_eq!(
        serde_json::to_value(&Check::AudioInput).unwrap()["kind"],
        "audio_input"
    );
    let j = serde_json::to_value(&fr).unwrap();
    assert_eq!(
        (j["kind"].as_str(), j["language"].as_str()),
        (Some("voice"), Some("fr"))
    );
    // a spec saved without the new fields still loads
    let mut j = serde_json::to_value(m1max()).unwrap();
    j.as_object_mut().unwrap().remove("voices");
    j.as_object_mut().unwrap().remove("audio_inputs");
    let old: Spec = serde_json::from_value(j).unwrap();
    assert!(old.voices.is_empty() && old.audio_inputs.is_empty());
    // probes follow the checks
    let mut r = reqs();
    assert_eq!(
        r.probes(),
        Probes {
            tools: vec!["uv".into()],
            ..Default::default()
        }
    );
    r.tiers.push(Tier {
        name: "french".into(),
        enables: "step 5".into(),
        checks: vec![fr],
    });
    assert!(r.probes().voices && !r.probes().audio_inputs);
    // a tier-only or recommended-only probe is skipped when only the outcome matters
    r.rules.push(Rule {
        check: Check::AudioInput,
        severity: Severity::Recommended,
        why: "step 7".into(),
    });
    assert!(r.probes().audio_inputs);
    let quick = r.outcome_probes(Severity::Pointless);
    assert!(!quick.voices && !quick.audio_inputs);
    assert_eq!(quick.tools, vec!["uv"]);
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
    s.tools.insert(
        "uv".into(),
        "uv 0.11.28 (Homebrew 2026-07-07 aarch64-apple-darwin)".into(),
    );
    let text = render(&check(&s, &reqs()));
    let line = text.lines().find(|l| l.contains("`uv` installed")).unwrap();
    assert!(line.contains("uv 0.11.28 (H…"), "{line}");
    assert!(line.ends_with("builds the env"));
}
