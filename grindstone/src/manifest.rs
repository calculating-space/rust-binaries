//! `manifest.json` (spec §6, the dataset contract) plus the internal
//! `state.json` sidecar used for incremental re-scan bookkeeping.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub dataset: String,
    pub schema_version: String,
    pub generator: Generator,
    pub generated_at: String,
    pub local_tz: String,
    pub config: Config,
    pub coverage: Coverage,
    pub sources: Vec<Source>,
    pub tables: BTreeMap<String, TableStats>,
    pub unknown_types: BTreeMap<String, u64>,
    pub incomplete_sessions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Generator {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub idle_threshold_ms: i64,
    #[serde(default)]
    pub queue_wait_threshold_ms: i64,
    pub include_text: bool,
    pub tool_category_map_version: String,
    pub denial_patterns_version: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Coverage {
    pub start: Option<String>,
    pub end: Option<String>,
    pub projects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Path relative to the scan root.
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows_emitted: u64,
    pub skipped_lines: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TableStats {
    pub rows: i64,
    pub files: u64,
}

impl Manifest {
    pub fn load(out_dir: &Path) -> Result<Option<Manifest>> {
        let p = out_dir.join("manifest.json");
        if !p.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
        Ok(Some(serde_json::from_slice(&data).context("parsing manifest.json")?))
    }

    pub fn save(&self, out_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(out_dir)?;
        let p = out_dir.join("manifest.json");
        std::fs::write(&p, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("writing {}", p.display()))?;
        Ok(())
    }
}

/// Internal incremental-scan state (not part of the §6 contract): per-session
/// spans for `overlap_sessions`, per-source unknown-type tallies, and the
/// parquet files each session emitted (so re-emits can clean up stale files).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScanState {
    pub sessions: BTreeMap<String, SessionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub project: String,
    pub start_ms: i64,
    pub end_ms: i64,
    #[serde(default)]
    pub unknown_types: BTreeMap<String, u64>,
    #[serde(default)]
    pub emitted_files: Vec<String>,
}

impl ScanState {
    pub fn load(out_dir: &Path) -> Result<ScanState> {
        let p = out_dir.join("state.json");
        if !p.exists() {
            return Ok(ScanState::default());
        }
        let data = std::fs::read(&p)?;
        Ok(serde_json::from_slice(&data).context("parsing state.json")?)
    }

    pub fn save(&self, out_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(out_dir)?;
        std::fs::write(out_dir.join("state.json"), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
