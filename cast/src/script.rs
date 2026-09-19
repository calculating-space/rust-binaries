//! `cast run <script>`: run a TypeScript script that imports `agent` and
//! `bench` from `"ai"`.
//!
//! The `ai` module is embedded here (`ai.js`) and placed in front of the
//! script in a temporary directory, so the bare import resolves; a TypeScript
//! runtime (bun, else node) executes the copy. `agent` calls the `oneshot`
//! binary, `bench` runs whatever command the script names. cast adds no
//! language of its own: the script is TypeScript, the calls are binaries.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The `ai` module, verbatim.
pub const AI_MODULE: &str = include_str!("ai.js");

pub struct Options {
    pub script: PathBuf,
    /// Agent command for `agent().run()`; empty means oneshot's default.
    pub agent: Vec<String>,
    pub runtime: Option<PathBuf>,
    pub oneshot: Option<PathBuf>,
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// The oneshot binary: as given, next to cast, in the source tree beside
/// cast's package (`<root>/oneshot/target/release/oneshot`), or on PATH.
pub fn find_oneshot(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return if p.is_file() { Ok(p) } else { Err(format!("--oneshot {}: no such file", p.display())) };
    }
    let exe = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    if let Some(dir) = exe.as_deref().and_then(Path::parent) {
        let sibling = dir.join("oneshot");
        if sibling.is_file() {
            return Ok(sibling);
        }
        if let Some(root) = dir.ancestors().nth(3) {
            let built = root.join("oneshot/target/release/oneshot");
            if built.is_file() {
                return Ok(built);
            }
        }
    }
    on_path("oneshot").ok_or_else(|| "oneshot not found: build it with `cs --build oneshot` or put it on PATH".into())
}

/// The TypeScript runtime: as given, else bun on PATH, else node.
pub fn find_runtime(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    on_path("bun")
        .or_else(|| on_path("node"))
        .ok_or_else(|| "no TypeScript runtime: install bun (brew install oven-sh/bun/bun) or node 24 or newer".into())
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Run the script and return the runtime's exit status.
pub fn run(opts: &Options) -> Result<i32, String> {
    let source = std::fs::read_to_string(&opts.script)
        .map_err(|e| format!("cannot read {}: {e}", opts.script.display()))?;
    let stem = opts.script.file_stem().and_then(|s| s.to_str()).unwrap_or("script");
    let runtime = find_runtime(opts.runtime.clone())?;
    let oneshot = find_oneshot(opts.oneshot.clone())?;

    let dir = tempfile::Builder::new()
        .prefix("cast-")
        .tempdir()
        .map_err(|e| format!("cannot create a temporary directory: {e}"))?;
    let module = dir.path().join("node_modules/ai");
    std::fs::create_dir_all(&module).map_err(|e| format!("cannot create {}: {e}", module.display()))?;
    write(&dir.path().join("package.json"), "{\"type\":\"module\"}\n")?;
    write(&module.join("package.json"), "{\"name\":\"ai\",\"type\":\"module\",\"exports\":\"./index.js\"}\n")?;
    write(&module.join("index.js"), AI_MODULE)?;
    let copy = dir.path().join(format!("{stem}.ts"));
    write(&copy, &source)?;

    let runtime_name = runtime.file_name().and_then(|s| s.to_str()).unwrap_or("runtime");
    eprintln!("cast: running {} with {runtime_name}", opts.script.display());
    let status = Command::new(&runtime)
        .arg(&copy)
        .env("AI_ONESHOT", &oneshot)
        .env("AI_AGENT", serde_json::to_string(&opts.agent).map_err(|e| e.to_string())?)
        .status()
        .map_err(|e| format!("cannot start {}: {e}", runtime.display()))?;
    Ok(status.code().unwrap_or(1))
}
