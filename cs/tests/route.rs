use cs::{build_command, candidates, listing, repo_root, tool, tools};
use std::path::Path;

fn fake_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, desc, built) in [
        ("alpha", "First tool", true),
        ("beta", "Second tool", false),
        ("cs", "the router", true),
    ] {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(dir.join("target/release")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\ndescription = \"{desc}\"\n"),
        )
        .unwrap();
        if built {
            std::fs::write(dir.join("target/release").join(name), "").unwrap();
        }
    }
    std::fs::create_dir_all(tmp.path().join("not-a-package")).unwrap();
    tmp
}

#[test]
fn lists_packages_with_build_state_and_skips_itself() {
    let repo = fake_repo();
    let all = tools(repo.path());
    assert_eq!(
        all.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    assert!(all[0].binary.is_some());
    assert!(all[1].binary.is_none());
    assert_eq!(all[1].description, "Second tool");
    let text = listing(repo.path());
    assert!(text.contains("alpha  built  First tool"));
    assert!(text.contains("beta          Second tool"));
    assert!(!text.contains("the router"));
}

#[test]
fn resolves_only_real_package_dirs() {
    let repo = fake_repo();
    assert!(tool(repo.path(), "alpha").is_some());
    assert!(tool(repo.path(), "not-a-package").is_none());
    assert!(tool(repo.path(), "missing").is_none());
    assert!(tool(repo.path(), "../alpha").is_none());
    assert!(tool(repo.path(), "").is_none());
    assert_eq!(
        candidates(Path::new("/r"), "x"),
        [
            Path::new("/r/x/target/release/x"),
            Path::new("/r/x/target/debug/x")
        ]
    );
}

#[test]
fn build_command_targets_the_tool_manifest() {
    let repo = fake_repo();
    let t = tool(repo.path(), "beta").unwrap();
    let argv = build_command(&t);
    assert_eq!(argv[..3], ["cargo", "build", "--release"]);
    assert!(argv[4].ends_with("beta/Cargo.toml"));
}

#[test]
fn repo_root_is_this_repository() {
    let root = repo_root();
    assert!(root.join("cs/Cargo.toml").is_file());
    assert!(root.join("trybox/Cargo.toml").is_file());
}
