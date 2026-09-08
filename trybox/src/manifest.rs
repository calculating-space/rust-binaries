use crate::backend::Backend;
use crate::doctor::HostInfo;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MANIFEST_FILE: &str = "manifest.json";

/// Stable description of one sandbox. Written once by `create`, read by everything else.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub name: String,
    pub backend: Backend,
    pub recipe: String,
    pub python: String,
    pub packages: Vec<String>,
    pub created_unix: u64,
    pub dir: PathBuf,
    pub host: HostInfo,
}

impl Manifest {
    pub fn new(name: &str, backend: Backend, recipe: &str, python: &str, packages: Vec<String>, dir: PathBuf, host: HostInfo) -> Self {
        let created_unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Self { version: 1, name: name.to_string(), backend, recipe: recipe.to_string(), python: python.to_string(), packages, created_unix, dir, host }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(MANIFEST_FILE)
    }
    /// Files the agent and the user work in. Mounted at /work for docker.
    pub fn work_dir(&self) -> PathBuf {
        self.dir.join("work")
    }
    /// Hugging Face home. Models land here and die with the sandbox.
    pub fn hf_dir(&self) -> PathBuf {
        self.dir.join("hf")
    }
    pub fn venv_dir(&self) -> PathBuf {
        self.dir.join(".venv")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }
    pub fn docker_dir(&self) -> PathBuf {
        self.dir.join("docker")
    }
    pub fn docker_tag(&self) -> String {
        format!("trybox/{}", self.name)
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(self.path(), json + "\n").map_err(|e| format!("write manifest: {e}"))
    }
}

pub fn load(root: &Path, name: &str) -> Result<Manifest, String> {
    let path = root.join(name).join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("no sandbox {name:?} at {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("corrupt manifest {}: {e}", path.display()))
}

/// Every sandbox under the root, sorted by name. Directories without a manifest are skipped.
pub fn list(root: &Path) -> Result<Vec<Manifest>, String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return Ok(out) };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Ok(m) = load(root, &name) {
            out.push(m);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
