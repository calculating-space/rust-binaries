//! `grindstone check`: manifest / hash / schema integrity validation.
//!
//! Verifies: manifest parses and its schema_version major is known; every
//! source file still matches its recorded sha256 (missing files are warnings —
//! logs rotate; changed files mean the dataset is stale); every parquet file
//! is readable and stamped with grindstone KV metadata; per-table row counts
//! match the manifest.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Result};
use rayon::prelude::*;

use crate::manifest::Manifest;
use crate::scan::collect_parquet;
use crate::tables;
use crate::util;

pub fn run(root: &Path, out: &Path) -> Result<()> {
    let mut problems = 0u32;
    let mut warnings = 0u32;

    let Some(manifest) = Manifest::load(out)? else {
        bail!(
            "no manifest.json in {} — run `grindstone scan` first",
            out.display()
        );
    };

    // schema version
    let major = manifest.schema_version.split('.').next().unwrap_or("");
    let expected_major = tables::SCHEMA_VERSION.split('.').next().unwrap();
    if major != expected_major {
        println!(
            "FAIL  schema_version {} (this build knows major {})",
            manifest.schema_version, expected_major
        );
        problems += 1;
    } else {
        println!("ok    schema_version {}", manifest.schema_version);
    }

    // sources: existence + hash
    let results: Vec<(String, &'static str)> = manifest
        .sources
        .par_iter()
        .map(|src| {
            let p = root.join(&src.path);
            if !p.exists() {
                return (src.path.clone(), "missing");
            }
            match std::fs::read(&p) {
                Ok(bytes) => {
                    if util::sha256_hex(&bytes) == src.sha256 {
                        (src.path.clone(), "ok")
                    } else if src.complete {
                        (src.path.clone(), "changed")
                    } else {
                        (src.path.clone(), "grown") // live session, expected
                    }
                }
                Err(_) => (src.path.clone(), "unreadable"),
            }
        })
        .collect();
    let mut tally: BTreeMap<&str, u64> = BTreeMap::new();
    for (path, status) in &results {
        *tally.entry(status).or_default() += 1;
        match *status {
            "changed" | "unreadable" => {
                println!("STALE {path} ({status}) — re-run `grindstone scan`");
                warnings += 1;
            }
            "missing" => warnings += 1,
            _ => {}
        }
    }
    println!(
        "ok    sources: {} checked ({} ok, {} grown/live, {} missing, {} changed, {} unreadable)",
        results.len(),
        tally.get("ok").copied().unwrap_or(0),
        tally.get("grown").copied().unwrap_or(0),
        tally.get("missing").copied().unwrap_or(0),
        tally.get("changed").copied().unwrap_or(0),
        tally.get("unreadable").copied().unwrap_or(0),
    );

    // parquet integrity + row counts vs manifest
    let mut all_tables: Vec<(String, std::path::PathBuf)> = tables::FACT_TABLES
        .iter()
        .map(|t| (t.to_string(), out.join("facts").join(t)))
        .collect();
    if manifest.config.include_text {
        all_tables.push(("texts".into(), out.join("texts")));
    }
    for (name, dir) in all_tables {
        let mut files = Vec::new();
        collect_parquet(&dir, &mut files)?;
        let infos: Vec<Result<(i64, bool)>> = files
            .par_iter()
            .map(|f| tables::parquet_footer_info(f))
            .collect();
        let mut rows = 0i64;
        let mut bad = 0u64;
        let mut unstamped = 0u64;
        for (f, info) in files.iter().zip(infos) {
            match info {
                Ok((r, stamped)) => {
                    rows += r;
                    if !stamped {
                        unstamped += 1;
                        println!("FAIL  {} missing grindstone KV metadata", f.display());
                    }
                }
                Err(e) => {
                    bad += 1;
                    println!("FAIL  unreadable parquet {}: {e}", f.display());
                }
            }
        }
        let expected = manifest.tables.get(&name).copied().unwrap_or_default();
        let count_ok = rows == expected.rows && files.len() as i64 == expected.files as i64;
        if bad > 0 || unstamped > 0 || !count_ok {
            if !count_ok {
                println!(
                    "FAIL  {name}: {} rows in {} files on disk, manifest says {} rows in {} files",
                    rows,
                    files.len(),
                    expected.rows,
                    expected.files
                );
            }
            problems += 1;
        } else {
            println!("ok    {name}: {} rows in {} files", rows, files.len());
        }
    }

    if problems > 0 {
        bail!("check failed: {problems} problem(s), {warnings} warning(s)");
    }
    println!("check passed ({warnings} warning(s))");
    Ok(())
}
