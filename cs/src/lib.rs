//! `cs`: a thin router over the tools in this repository.
//!
//! `cs trybox explore mlx` finds the `trybox` binary and execs it with the rest
//! of the arguments. Nothing is shared between tools; `cs` only knows the
//! repository layout (one package per directory) and where cargo puts binaries.

use std::path::{Path, PathBuf};

/// Where the repository is: the parent of this package's directory, fixed at build time.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: String,
    pub dir: PathBuf,
    pub description: String,
    /// The release binary, if built.
    pub binary: Option<PathBuf>,
}

fn description_of(manifest: &Path) -> String {
    std::fs::read_to_string(manifest)
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                let l = l.trim();
                l.strip_prefix("description")?.trim_start().strip_prefix('=').map(|v| v.trim().trim_matches('"').to_string())
            })
        })
        .unwrap_or_default()
}

/// Candidate binary locations for a tool, most preferred first.
pub fn candidates(root: &Path, name: &str) -> Vec<PathBuf> {
    let dir = root.join(name);
    vec![dir.join("target/release").join(name), dir.join("target/debug").join(name)]
}

/// One tool by name, if a package directory of that name exists.
pub fn tool(root: &Path, name: &str) -> Option<Tool> {
    let dir = root.join(name);
    let manifest = dir.join("Cargo.toml");
    if name.is_empty() || name.starts_with('.') || name.contains('/') || !manifest.is_file() {
        return None;
    }
    let binary = candidates(root, name).into_iter().find(|p| p.is_file());
    Some(Tool { name: name.to_string(), dir, description: description_of(&manifest), binary })
}

/// Every package directory under the root except `cs` itself, sorted by name.
pub fn tools(root: &Path) -> Vec<Tool> {
    let mut out: Vec<Tool> = std::fs::read_dir(root)
        .map(|rd| rd.flatten().filter_map(|e| tool(root, &e.file_name().to_string_lossy())).filter(|t| t.name != "cs").collect())
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The cargo invocation that builds a tool's release binary.
pub fn build_command(t: &Tool) -> Vec<String> {
    vec!["cargo".into(), "build".into(), "--release".into(), "--manifest-path".into(), t.dir.join("Cargo.toml").display().to_string()]
}

/// Text for `cs` with no arguments.
pub fn listing(root: &Path) -> String {
    let all = tools(root);
    let width = all.iter().map(|t| t.name.len()).max().unwrap_or(4).max(4);
    let mut out = format!("tools in {}\n\n", root.display());
    for t in &all {
        out.push_str(&format!("  {:<width$}  {}  {}\n", t.name, if t.binary.is_some() { "built" } else { "     " }, t.description));
    }
    out.push_str("\nusage: cs <tool> [args...]     runs the tool, building it first if needed\n       cs --where <tool>       prints the binary path\n       cs --build [tool...]    builds one, several, or all tools\n");
    out
}
