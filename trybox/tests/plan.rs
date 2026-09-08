use std::path::PathBuf;
use trybox::{Backend, HostInfo, Manifest, briefing, list, prepare, recipe, validate_name};

fn manifest(backend: Backend, dir: PathBuf) -> Manifest {
    let r = recipe("mlx").unwrap();
    Manifest::new(
        "t1",
        backend,
        "mlx",
        r.python,
        r.packages.iter().map(|s| s.to_string()).collect(),
        dir,
        HostInfo {
            os: "macos".into(),
            arch: "aarch64".into(),
            chip: "Apple M1 Max".into(),
            memory_gb: 64,
        },
    )
}

#[test]
fn names_are_boring() {
    assert!(validate_name("mlx-test_1").is_ok());
    assert!(validate_name("").is_err());
    assert!(validate_name("-x").is_err());
    assert!(validate_name("a/b").is_err());
    assert!(validate_name("a b").is_err());
}

#[test]
fn unknown_recipe_lists_known_ones() {
    let err = recipe("nope").unwrap_err();
    assert!(err.contains("mlx") && err.contains("bare"), "{err}");
}

#[test]
fn uv_plan_creates_venv_then_installs_inside_sandbox() {
    let m = manifest(Backend::Uv, PathBuf::from("/s/t1"));
    let plan = prepare(&m);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].program, "uv");
    assert_eq!(plan.steps[0].args[..3], ["venv", "--python", "3.12"]);
    assert_eq!(plan.steps[1].args[0..2], ["pip", "install"]);
    assert!(plan.steps[1].args.contains(&"mlx-lm".to_string()));
    // caches and HF home never leave the sandbox
    for step in &plan.steps {
        let env: std::collections::HashMap<_, _> = step.env.iter().cloned().collect();
        assert_eq!(env["HF_HOME"], "/s/t1/hf");
        assert_eq!(
            env["HF_HUB_VERBOSITY"], "error",
            "no HF_TOKEN nag in the sandbox"
        );
        assert_eq!(env["HF_HUB_DISABLE_XET"], "1");
        assert_eq!(env["TQDM_NCOLS"], "80");
        assert_eq!(env["UV_CACHE_DIR"], "/s/t1/cache/uv");
        assert!(env["PATH"].starts_with("/s/t1/.venv/bin:"));
    }
    let x = plan.files.iter().find(|(p, _)| p.ends_with("x")).unwrap();
    assert!(x.1.contains("HF_HOME=\"/s/t1/hf\""));
    assert!(x.1.contains("HF_HUB_VERBOSITY=error HF_HUB_DISABLE_XET=1 TQDM_NCOLS=80"));
}

#[test]
fn bare_plan_has_no_install_step() {
    let r = recipe("bare").unwrap();
    let m = Manifest::new(
        "b",
        Backend::Venv,
        "bare",
        r.python,
        vec![],
        PathBuf::from("/s/b"),
        HostInfo::default(),
    );
    let plan = prepare(&m);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].program, "python3");
}

#[test]
fn docker_plan_builds_tagged_image_from_generated_dockerfile() {
    let m = manifest(Backend::Docker, PathBuf::from("/s/t1"));
    let plan = prepare(&m);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(
        plan.steps[0].args,
        ["build", "-t", "trybox/t1", "/s/t1/docker"]
    );
    let df = plan
        .files
        .iter()
        .find(|(p, _)| p.ends_with("Dockerfile"))
        .unwrap();
    assert!(df.1.starts_with("FROM python:3.12-slim"));
    assert!(df.1.contains("pip install --no-cache-dir mlx mlx-lm huggingface_hub"));
    assert!(df.1.contains(
        "ENV HF_HOME=/hf PIP_NO_CACHE_DIR=1 HF_HUB_DISABLE_TELEMETRY=1 HF_HUB_VERBOSITY=error"
    ));
    let x = plan.files.iter().find(|(p, _)| p.ends_with("x")).unwrap();
    assert!(x.1.contains("docker run --rm"));
    assert!(x.1.contains("/s/t1/work:/work"));
    assert!(x.1.contains("/s/t1/hf:/hf"));
}

#[test]
fn briefing_tells_agent_where_it_is_and_what_to_try() {
    let r = recipe("mlx").unwrap();
    let m = manifest(Backend::Uv, PathBuf::from("/s/t1"));
    let text = briefing(&m, r);
    assert!(text.contains("Apple M1 Max"));
    assert!(text.contains("64 GB"));
    assert!(text.contains("mlx, mlx-lm"));
    assert!(text.contains("## Suggested experiments"));
    assert!(text.contains("1. Run a 30B-class 4-bit model"));
    assert!(text.contains("uv pip install"));
    assert!(!text.contains("no GPU inside the container"));

    let d = manifest(Backend::Docker, PathBuf::from("/s/t1"));
    let text = briefing(&d, r);
    assert!(text.contains("/s/t1/x python"));
    assert!(text.contains("no GPU inside the container"));
}

#[test]
fn manifest_roundtrips_and_lists() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let m = manifest(Backend::Uv, root.join("t1"));
    std::fs::create_dir_all(&m.dir).unwrap();
    m.save().unwrap();
    std::fs::create_dir_all(root.join("junk")).unwrap();
    let all = list(root).unwrap();
    assert_eq!(all, vec![m.clone()]);
    assert_eq!(trybox::load(root, "t1").unwrap(), m);
    assert!(trybox::load(root, "junk").is_err());
}

/// Real end-to-end with the venv backend and no packages: cheap, needs only python3.
#[test]
fn venv_sandbox_end_to_end() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: python3 missing");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let r = recipe("bare").unwrap();
    let m = Manifest::new(
        "e2e",
        Backend::Venv,
        "bare",
        r.python,
        vec![],
        tmp.path().join("e2e"),
        HostInfo::default(),
    );
    let mut plan = prepare(&m);
    plan.files
        .push((m.work_dir().join("CLAUDE.md"), briefing(&m, r)));
    trybox::run_plan(&m, &plan, true).unwrap();
    m.save().unwrap();
    assert!(m.venv_dir().join("bin/python").exists());
    assert!(m.work_dir().join("CLAUDE.md").exists());

    let out = trybox::exec_command(
        &m,
        &[
            "python".into(),
            "-c".into(),
            "import os,sys;print(sys.prefix);print(os.environ['HF_HOME']);print(os.getcwd())"
                .into(),
        ],
    )
    .output()
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        PathBuf::from(&lines[0]).canonicalize().unwrap(),
        m.venv_dir().canonicalize().unwrap()
    );
    assert_eq!(
        PathBuf::from(&lines[1]).canonicalize().unwrap(),
        m.hf_dir().canonicalize().unwrap()
    );
    assert_eq!(
        PathBuf::from(&lines[2]).canonicalize().unwrap(),
        m.work_dir().canonicalize().unwrap()
    );

    let notes = trybox::destroy(&m).unwrap();
    assert!(!m.dir.exists(), "{notes:?}");
}

#[test]
fn recipes_read_like_a_man_page() {
    let text = trybox::recipe::table();
    assert!(text.lines().next().unwrap().starts_with("RECIPE"));
    assert!(text.contains("mlx      11     Apple's machine learning framework"));
    let r = recipe("mlx").unwrap();
    let m = trybox::recipe::man(r, None);
    let order = [
        "TRYBOX(MLX)",
        "NAME",
        "WHAT IT IS",
        "https://github.com/ml-explore/mlx",
        "REQUIREMENTS",
        "8 GB memory",
        "What your machine unlocks",
        "HELLO WORLD",
        "MLX sees the GPU",
        "TOUR",
        "1. Arrays like NumPy",
        "2. Talk to a language model",
        "GO FURTHER",
        "CAVEATS",
        "TRY IT",
        "trybox explore mlx",
        "UNDER THE HOOD",
    ];
    let mut pos = 0;
    for needle in order {
        let at = m[pos..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing or out of order"));
        pos += at;
    }
    assert!(
        m.lines().all(|l| l.len() <= 160),
        "some line is too wide for a man page: {:?}",
        m.lines().find(|l| l.len() > 160)
    );
    // every recipe: hello + tour steps have all four parts, and go easy → advanced
    for r in trybox::recipe::RECIPES {
        assert!(
            !r.what.is_empty() && !r.repo.is_empty(),
            "{} lacks description",
            r.name
        );
        for e in std::iter::once(&r.hello).chain(r.tour.iter()) {
            assert!(
                !e.title.is_empty()
                    && !e.learn.is_empty()
                    && !e.run.is_empty()
                    && !e.expect.is_empty(),
                "{}: {:?}",
                r.name,
                e.title
            );
        }
        assert!(!r.tour.is_empty(), "{} has no tour", r.name);
        let m = trybox::recipe::man(r, None);
        assert!(
            m.lines().all(|l| l.chars().count() <= 160),
            "{}: man page line too wide: {:?}",
            r.name,
            m.lines().find(|l| l.chars().count() > 160)
        );
    }
}

#[test]
fn briefing_includes_hello_and_tour() {
    let r = recipe("mlx").unwrap();
    let m = manifest(Backend::Uv, PathBuf::from("/s/t1"));
    let text = briefing(&m, r);
    assert!(text.contains("## What this project is"));
    assert!(text.contains("## Known-good commands"));
    assert!(text.contains(r.hello.run));
    assert!(text.contains(r.tour.last().unwrap().run));
}

/// explore on a bare sandbox: creates it silently, runs the hello world, quits on EOF.
#[test]
fn explore_prepares_and_runs_hello() {
    if std::process::Command::new("uv")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: uv missing");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let r = recipe("bare").unwrap();
    let m = trybox::explore::ensure_sandbox(tmp.path(), r, "b").unwrap();
    assert!(m.work_dir().join("CLAUDE.md").exists());
    // second call loads instead of recreating
    assert_eq!(
        trybox::explore::ensure_sandbox(tmp.path(), r, "b").unwrap(),
        m
    );
    assert!(trybox::explore::run_example(&m, &r.hello).unwrap());
}

#[test]
fn requirements_matrix_is_annotated_with_the_verdict() {
    use speccheck::{Gpu, GpuKind, Outcome, Spec};
    let r = recipe("mlx").unwrap();
    let req = (r.requirements)();
    assert_eq!(req.subject, "mlx");
    assert_eq!(req.tool_names(), vec!["uv"]);
    let mut spec = Spec {
        version: speccheck::CONTRACT_VERSION,
        os: "macos".into(),
        arch: "aarch64".into(),
        chip: "Apple M1 Max".into(),
        cores: 10,
        memory_gb: 64,
        disk_free_gb: 60,
        disk_path: "/".into(),
        gpus: vec![Gpu {
            kind: GpuKind::Metal,
            name: "m1".into(),
            memory_gb: None,
        }],
        tools: [("uv".to_string(), "uv 0.11".to_string())]
            .into_iter()
            .collect(),
        audio_inputs: vec![],
        voices: vec![],
    };
    let v = speccheck::check(&spec, &req);
    assert_eq!(v.outcome, Outcome::CanRun);
    assert!(
        v.tiers.iter().all(|t| t.ok),
        "64 GB / 60 GB free unlocks every tier: {:?}",
        v.tiers
    );
    let text = trybox::recipe::man(r, Some(&v));
    assert!(text.contains("This machine can run it."));
    assert!(text.contains("ok 8 GB memory"));
    assert!(text.contains("[64 GB]"));
    assert!(text.contains("ok 70B-class models"));

    // Intel Mac: blocked; the plain matrix (no verdict) has no marks
    spec.arch = "x86_64".into();
    spec.gpus.clear();
    let v = speccheck::check(&spec, &req);
    assert_eq!(v.outcome, Outcome::CannotRun);
    assert!(trybox::recipe::man(r, Some(&v)).contains("This machine cannot run it"));
    let plain = trybox::recipe::man(r, None);
    assert!(!plain.contains("This machine"));
    for r in trybox::recipe::RECIPES {
        assert_eq!((r.requirements)().subject, r.name);
    }
    // whisper needs host features beyond tools: a French voice and a microphone, probed on demand
    let w = recipe("whisper").unwrap();
    assert_eq!(w.hello.models, ["mlx-community/whisper-tiny"]);
    assert!(
        trybox::recipe::man(w, None)
            .contains("Fetched first: mlx-community/whisper-large-v3-turbo")
    );
    assert!(
        recipe("bare")
            .unwrap()
            .tour
            .iter()
            .all(|e| e.models.is_empty())
    );
    let p = (w.requirements)().probes();
    assert!(p.voices && p.audio_inputs, "{p:?}");
    assert!(p.tools.contains(&"ffmpeg".to_string()) && p.tools.contains(&"say".to_string()));
    let p = req.probes();
    assert!(
        !p.voices && !p.audio_inputs,
        "mlx asks for nothing but tools: {p:?}"
    );
}

#[test]
fn stale_wrapper_is_refreshed_before_running() {
    let tmp = tempfile::tempdir().unwrap();
    let m = manifest(Backend::Uv, tmp.path().join("old"));
    std::fs::create_dir_all(&m.dir).unwrap();
    std::fs::write(m.dir.join("x"), "#!/bin/sh\nexec \"$@\"\n").unwrap();
    let _ = trybox::exec_command(&m, &["true".into()]);
    let x = std::fs::read_to_string(m.dir.join("x")).unwrap();
    assert!(x.contains("HF_HUB_VERBOSITY=error"), "{x}");
    assert!(
        x.starts_with("#!/bin/sh\n# run a command inside the "),
        "{x}"
    );
}

#[test]
fn dispose_removes_every_sandbox_and_reports() {
    let tmp = tempfile::tempdir().unwrap();
    for n in ["a", "b"] {
        let m = Manifest::new(
            n,
            Backend::Uv,
            "bare",
            "3.12",
            vec![],
            tmp.path().join(n),
            HostInfo::default(),
        );
        std::fs::create_dir_all(m.work_dir()).unwrap();
        std::fs::write(m.work_dir().join("big"), vec![0u8; 1 << 20]).unwrap();
        m.save().unwrap();
    }
    assert_eq!(list(tmp.path()).unwrap().len(), 2);
    for m in list(tmp.path()).unwrap() {
        let notes = trybox::destroy(&m).unwrap();
        assert!(notes[0].starts_with("removed"));
    }
    assert!(list(tmp.path()).unwrap().is_empty());
}

#[test]
fn overview_says_what_each_sandbox_is_and_how_far_you_got() {
    use trybox::progress::Progress;
    let tmp = tempfile::tempdir().unwrap();
    let m = manifest(Backend::Uv, tmp.path().join("mlx"));
    std::fs::create_dir_all(m.hf_dir()).unwrap();
    m.save().unwrap();
    assert!(trybox::overview(&[]).contains("no sandboxes yet"));
    let text = trybox::overview(std::slice::from_ref(&m));
    assert!(text.contains("not started"));
    assert!(text.contains("Apple's machine learning framework"));
    assert!(text.contains("just now"));

    let mut p = Progress::load(&m.dir);
    p.record(None, true);
    p.record(Some(0), true);
    p.record(Some(3), true);
    p.record(Some(5), false);
    p.save(&m.dir).unwrap();
    let text = trybox::overview(std::slice::from_ref(&m));
    assert!(text.contains("hello + 2/10 steps"), "{text}");
    assert!(text.contains("trybox explore mlx"));
    assert!(text.contains("trybox dispose t1"));
    assert!(
        text.lines().all(|l| l.len() <= 90),
        "a list line is too wide:\n{text}"
    );
    let st = trybox::status(&m);
    assert!(st.contains(" 1. Arrays like NumPy, on the GPU"));
    assert!(st.contains("✓ just now"));
    assert!(st.contains("✗ just now"));
    assert!(st.contains("of it models"));
    assert_eq!(Progress::load(&m.dir), p);
}

#[test]
fn select_prompt_puts_recommended_first_and_parses_answers() {
    use trybox::ui::{Choice, order, parse_fallback, render};
    let choices = vec![
        Choice::new("Alpha", "first"),
        Choice::new("Beta", "second").recommended(),
        Choice::new("Gamma", ""),
    ];
    let ord = order(&choices);
    assert_eq!(ord, vec![1, 0, 2]);
    let frame = render("Pick one", &choices, &ord, 0, false);
    let lines: Vec<_> = frame.lines().collect();
    assert_eq!(lines[0], "Pick one");
    assert_eq!(lines[1], "❯  1. Beta (Recommended)");
    assert_eq!(lines[2], "      second");
    assert_eq!(lines[3], "   2. Alpha");
    assert_eq!(parse_fallback("1", &choices, &ord), Some(1));
    assert_eq!(parse_fallback("3", &choices, &ord), Some(2));
    assert_eq!(parse_fallback("gam", &choices, &ord), Some(2));
    assert_eq!(parse_fallback("q", &choices, &ord), None);
    assert_eq!(parse_fallback("", &choices, &ord), None);
    assert_eq!(parse_fallback("9", &choices, &ord), None);
}

#[test]
fn tour_menu_recommends_next_undone_step_and_always_offers_dispose() {
    use trybox::progress::Progress;
    let r = recipe("mlx").unwrap();
    let mut p = Progress::default();
    p.record(None, true);
    p.record(Some(0), true);
    let c = trybox::explore::choices(r, &p, 5 << 30);
    assert_eq!(c.len(), r.tour.len() + 5);
    assert_eq!(
        c[0].label,
        format!("✓ {}", r.hello.title),
        "hello world stays available"
    );
    assert!(c[0].detail.starts_with("The hello world, any time."));
    assert!(c[1].label.starts_with("✓ "));
    assert!(!c[0].recommended && !c[1].recommended);
    assert!(c[2].recommended, "step 2 is the next undone step");
    assert_eq!(c[r.tour.len() + 1].label, "Hand over to the agent");
    assert!(c[r.tour.len() + 3].label.starts_with("Dispose"));
    assert!(c[r.tour.len() + 3].detail.contains("5.0 GB"));
    for i in 0..r.tour.len() {
        p.record(Some(i), true);
    }
    let c = trybox::explore::choices(r, &p, 0);
    assert!(
        c[r.tour.len() + 1].recommended,
        "agent is recommended once the tour is complete"
    );
    let failed = Progress::default();
    let c = trybox::explore::choices(r, &failed, 0);
    assert_eq!(
        c[0].label, r.hello.title,
        "an unfinished hello world is not ticked"
    );
}

#[test]
fn home_menu_recommends_most_recent_sandbox_and_offers_dispose_per_sandbox() {
    use trybox::home::{Action, choices};
    use trybox::progress::Progress;
    let tmp = tempfile::tempdir().unwrap();
    let mut old = manifest(Backend::Uv, tmp.path().join("old"));
    old.created_unix -= 3600;
    let mut fresh = manifest(Backend::Uv, tmp.path().join("fresh"));
    fresh.name = "fresh".into();
    for m in [&old, &fresh] {
        std::fs::create_dir_all(&m.dir).unwrap();
        m.save().unwrap();
    }
    let mut p = Progress::default();
    p.record(None, true);
    p.save(&fresh.dir).unwrap();
    let (empty, acts) = choices(&[]);
    assert!(empty[0].recommended && empty[0].label == "Start something new");
    assert_eq!(acts, vec![Action::StartNew, Action::Quit]);
    let (c, a) = choices(&[old.clone(), fresh.clone()]);
    assert_eq!(a[0], Action::Continue("fresh".into()));
    assert!(c[0].recommended);
    assert!(c[0].detail.contains("hello world"));
    assert_eq!(a[1], Action::Continue("t1".into()));
    assert_eq!(a[2], Action::StartNew);
    assert_eq!(a[3], Action::Dispose("fresh".into()));
    assert_eq!(a[4], Action::Dispose("t1".into()));
    assert_eq!(a[5], Action::Quit);
}

/// Dispose from inside the tour menu, through the non-terminal fallback.
#[test]
fn tour_dispose_deletes_the_sandbox() {
    let tmp = tempfile::tempdir().unwrap();
    let r = recipe("bare").unwrap();
    let m = Manifest::new(
        "b",
        Backend::Venv,
        "bare",
        r.python,
        vec![],
        tmp.path().join("b"),
        HostInfo::default(),
    );
    std::fs::create_dir_all(m.work_dir()).unwrap();
    m.save().unwrap();
    let c = trybox::explore::choices(r, &trybox::progress::Progress::default(), 0);
    let dispose = c
        .iter()
        .position(|c| c.label.starts_with("Dispose"))
        .unwrap();
    assert_eq!(dispose, r.tour.len() + 3);
    let notes = trybox::destroy(&m).unwrap();
    assert!(notes[0].starts_with("removed"));
    assert!(!m.dir.exists());
}

#[test]
fn agent_is_told_what_the_user_already_did() {
    use trybox::progress::Progress;
    let tmp = tempfile::tempdir().unwrap();
    let m = manifest(Backend::Uv, tmp.path().join("mlx"));
    std::fs::create_dir_all(&m.dir).unwrap();
    assert!(trybox::agent::progress_note(&m).contains("not run anything"));
    let mut p = Progress::default();
    p.record(None, true);
    p.save(&m.dir).unwrap();
    assert!(trybox::agent::progress_note(&m).contains("hello world and nothing else"));
    p.record(Some(1), true);
    p.record(Some(3), true);
    p.save(&m.dir).unwrap();
    let note = trybox::agent::progress_note(&m);
    assert!(
        note.contains("Talk to a language model; How fast is the GPU really"),
        "{note}"
    );
}
