//! `grindstone scan`: discover transcript files, sessionize new/changed ones,
//! write partitioned Parquet + manifest. Idempotent and incremental (design
//! rule 5): unchanged completed sources are skipped via sha256 + byte-length
//! checkpoints; still-growing files are re-emitted on the next run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rayon::prelude::*;

use crate::manifest::{
    Config, Coverage, Generator, Manifest, ScanState, SessionState, Source, TableStats,
};
use crate::session::{
    self, SessionOutput, DENIAL_PATTERNS_VERSION, IDLE_THRESHOLD_MS, TOOL_CATEGORY_MAP_VERSION,
};
use crate::tables;
use crate::util;

/// A file whose mtime is within this window of scan start is considered live
/// (still growing) and marked `is_complete = false`.
const LIVE_WINDOW_MS: i64 = 300_000;

pub struct ScanOpts {
    pub root: PathBuf,
    pub out: PathBuf,
    pub since: Option<String>,
    pub projects: Vec<String>,
    pub include_text: bool,
    pub force: bool,
}

struct SubFile {
    /// path relative to root, e.g. `<slug>/<session-id>/subagents/agent-x.jsonl`
    rel: String,
    abs: PathBuf,
}

/// One session = main transcript + its subagent transcripts (sidechains).
struct Candidate {
    /// path relative to root, e.g. `<project-slug>/<session-id>.jsonl`
    rel: String,
    abs: PathBuf,
    project: String,
    session_id: String,
    /// max mtime across main + subagent files
    mtime_ms: i64,
    subs: Vec<SubFile>,
}

#[allow(clippy::large_enum_variant)]
enum FileOutcome {
    Skipped(Vec<Source>),
    Processed {
        sources: Vec<Source>,
        output: SessionOutput,
        project: String,
        session_id: String,
    },
    Failed {
        rel: String,
        err: String,
    },
}

pub fn run(opts: ScanOpts) -> Result<()> {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
    let since_cutoff = opts
        .since
        .as_deref()
        .map(|s| util::parse_since(s, now_ms))
        .transpose()?;

    if !opts.root.is_dir() {
        bail!("root {} is not a directory", opts.root.display());
    }

    if opts.force {
        for sub in ["facts", "texts"] {
            let p = opts.out.join(sub);
            if p.exists() {
                std::fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
            }
        }
        for f in ["manifest.json", "state.json"] {
            let p = opts.out.join(f);
            if p.exists() {
                std::fs::remove_file(&p)?;
            }
        }
    }

    let prev_manifest = Manifest::load(&opts.out)?;
    if let Some(m) = &prev_manifest {
        if m.config.include_text != opts.include_text {
            bail!(
                "existing dataset was written with include_text={}, this run asks for {}; \
                 re-run with --force to rebuild",
                m.config.include_text,
                opts.include_text
            );
        }
    }
    let mut state = ScanState::load(&opts.out)?;
    let prev_sources: BTreeMap<String, Source> = prev_manifest
        .as_ref()
        .map(|m| {
            m.sources
                .iter()
                .map(|s| (s.path.clone(), s.clone()))
                .collect()
        })
        .unwrap_or_default();

    // ---- discover ----
    // Main transcripts live at <slug>/<session-id>.jsonl; the current CLI
    // stores sidechain (subagent) transcripts out-of-band at
    // <slug>/<session-id>/subagents/agent-*.jsonl — group them per session.
    let mut candidates = Vec::new();
    for proj_entry in std::fs::read_dir(&opts.root)? {
        let proj_entry = proj_entry?;
        if !proj_entry.file_type()?.is_dir() {
            continue;
        }
        let project = proj_entry.file_name().to_string_lossy().into_owned();
        if !opts.projects.is_empty() && !opts.projects.contains(&project) {
            continue;
        }
        let mut mains: Vec<Candidate> = Vec::new();
        let mut sub_dirs: Vec<(String, PathBuf)> = Vec::new(); // (session_id, subagents dir)
        for f in std::fs::read_dir(proj_entry.path())? {
            let f = f?;
            let name = f.file_name().to_string_lossy().into_owned();
            if f.file_type()?.is_dir() {
                let sd = f.path().join("subagents");
                if sd.is_dir() {
                    sub_dirs.push((name, sd));
                }
                continue;
            }
            if !name.ends_with(".jsonl") || !f.file_type()?.is_file() {
                continue;
            }
            mains.push(Candidate {
                rel: format!("{project}/{name}"),
                abs: f.path(),
                project: project.clone(),
                session_id: name.trim_end_matches(".jsonl").to_string(),
                mtime_ms: mtime_of(&f.metadata()?),
                subs: Vec::new(),
            });
        }
        for (session_id, sd) in sub_dirs {
            let Some(main) = mains.iter_mut().find(|c| c.session_id == session_id) else {
                eprintln!(
                    "warning: orphan subagent dir {} (no {session_id}.jsonl), skipping",
                    sd.display()
                );
                continue;
            };
            for f in std::fs::read_dir(&sd)? {
                let f = f?;
                let name = f.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".jsonl") || !f.file_type()?.is_file() {
                    continue;
                }
                let mtime = mtime_of(&f.metadata()?);
                main.mtime_ms = main.mtime_ms.max(mtime);
                main.subs.push(SubFile {
                    rel: format!("{project}/{session_id}/subagents/{name}"),
                    abs: f.path(),
                });
            }
            main.subs.sort_by(|a, b| a.rel.cmp(&b.rel));
        }
        candidates.extend(mains);
    }
    if let Some(cutoff) = since_cutoff {
        candidates.retain(|c| c.mtime_ms >= cutoff);
    }
    candidates.sort_by(|a, b| a.rel.cmp(&b.rel));

    // ---- parse (parallel per session group) ----
    let include_text = opts.include_text;
    let outcomes: Vec<FileOutcome> = candidates
        .par_iter()
        .map(|c| {
            let main = match std::fs::read(&c.abs) {
                Ok(b) => b,
                Err(e) => {
                    return FileOutcome::Failed {
                        rel: c.rel.clone(),
                        err: e.to_string(),
                    }
                }
            };
            let mut subs: Vec<(String, Vec<u8>)> = Vec::new();
            for s in &c.subs {
                match std::fs::read(&s.abs) {
                    Ok(b) => subs.push((s.rel.clone(), b)),
                    Err(e) => {
                        return FileOutcome::Failed {
                            rel: s.rel.clone(),
                            err: e.to_string(),
                        }
                    }
                }
            }
            let main_sha = util::sha256_hex(&main);
            let sub_shas: Vec<String> = subs.iter().map(|(_, b)| util::sha256_hex(b)).collect();
            let complete = now_ms - c.mtime_ms > LIVE_WINDOW_MS;

            // checkpoint: skip only if the whole group is unchanged & complete
            let unchanged = |rel: &str, sha: &str, bytes: u64| {
                prev_sources
                    .get(rel)
                    .is_some_and(|p| p.sha256 == sha && p.bytes == bytes && p.complete)
            };
            if unchanged(&c.rel, &main_sha, main.len() as u64)
                && subs
                    .iter()
                    .zip(&sub_shas)
                    .all(|((rel, b), sha)| unchanged(rel, sha, b.len() as u64))
            {
                let mut sources = vec![prev_sources[&c.rel].clone()];
                sources.extend(subs.iter().map(|(rel, _)| prev_sources[rel].clone()));
                return FileOutcome::Skipped(sources);
            }

            let output =
                session::process_session(&c.session_id, &c.project, &main, &subs, include_text);
            let total_rows = output.cycles.len()
                + output.api_calls.len()
                + output.tool_calls.len()
                + output.file_touches.len()
                + output.texts.len()
                + output.session.is_some() as usize;
            let sub_rows: u64 = output.sub_stats.iter().map(|s| s.rows_emitted).sum();
            let sub_skipped: u64 = output.sub_stats.iter().map(|s| s.skipped_lines).sum();
            let mut sources = vec![Source {
                path: c.rel.clone(),
                sha256: main_sha,
                bytes: main.len() as u64,
                rows_emitted: total_rows as u64 - sub_rows,
                skipped_lines: output.skipped_lines - sub_skipped,
                complete,
            }];
            for (stat, ((rel, bytes), sha)) in
                output.sub_stats.iter().zip(subs.iter().zip(&sub_shas))
            {
                debug_assert_eq!(&stat.path, rel);
                sources.push(Source {
                    path: rel.clone(),
                    sha256: sha.clone(),
                    bytes: bytes.len() as u64,
                    rows_emitted: stat.rows_emitted,
                    skipped_lines: stat.skipped_lines,
                    complete,
                });
            }
            FileOutcome::Processed {
                sources,
                output,
                project: c.project.clone(),
                session_id: c.session_id.clone(),
            }
        })
        .collect();

    // ---- overlap_sessions: needs every known span (spec §4.4) ----
    let mut spans: Vec<(String, i64, i64)> = Vec::new();
    let processed_ids: BTreeSet<&str> = outcomes
        .iter()
        .filter_map(|o| match o {
            FileOutcome::Processed { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        })
        .collect();
    for (sid, s) in &state.sessions {
        if !processed_ids.contains(sid.as_str()) {
            spans.push((sid.clone(), s.start_ms, s.end_ms));
        }
    }
    for o in &outcomes {
        if let FileOutcome::Processed {
            output, session_id, ..
        } = o
        {
            if let Some(s) = &output.session {
                spans.push((session_id.clone(), s.start_ts, s.end_ts));
            }
        }
    }

    // ---- emit ----
    let mut new_sources: Vec<Source> = Vec::new();
    let mut n_processed = 0u64;
    let mut n_skipped = 0u64;
    let mut n_failed = 0u64;
    let mut incomplete_now: BTreeSet<String> = prev_manifest
        .as_ref()
        .map(|m| m.incomplete_sessions.iter().cloned().collect())
        .unwrap_or_default();

    for o in outcomes {
        match o {
            FileOutcome::Skipped(srcs) => {
                n_skipped += 1;
                new_sources.extend(srcs);
            }
            FileOutcome::Failed { rel, err } => {
                n_failed += 1;
                eprintln!("warning: failed to read {rel}: {err}");
            }
            FileOutcome::Processed {
                sources,
                mut output,
                project,
                session_id,
            } => {
                n_processed += 1;
                let complete = sources[0].complete;
                if complete {
                    incomplete_now.remove(&session_id);
                } else {
                    incomplete_now.insert(session_id.clone());
                }
                let mut emitted: Vec<String> = Vec::new();
                if let Some(sess) = &mut output.session {
                    sess.is_complete = complete;
                    sess.overlap_sessions = spans
                        .iter()
                        .filter(|(sid, s, e)| {
                            sid != &session_id && *s < sess.end_ts && sess.start_ts < *e
                        })
                        .count() as i32;
                    let date = util::local_date(sess.start_ts);
                    let part = |table: &str| {
                        format!("facts/{table}/project={project}/date={date}/{session_id}.parquet")
                    };
                    let sess_slice = std::slice::from_ref(&*sess);
                    let writes: Vec<(String, arrow::record_batch::RecordBatch)> =
                        [
                            ("sessions", tables::sessions_batch(sess_slice)?),
                            ("cycles", tables::cycles_batch(&output.cycles)?),
                            ("api_calls", tables::api_calls_batch(&output.api_calls)?),
                            ("tool_calls", tables::tool_calls_batch(&output.tool_calls)?),
                            (
                                "file_touches",
                                tables::file_touches_batch(&output.file_touches)?,
                            ),
                        ]
                        .into_iter()
                        .map(|(t, b)| (part(t), b))
                        .chain(
                            include_text
                                .then(|| {
                                    Ok::<_, anyhow::Error>((
                            format!("texts/project={project}/date={date}/{session_id}.parquet"),
                            tables::texts_batch(&output.texts)?,
                        ))
                                })
                                .transpose()?,
                        )
                        .collect();
                    for (rel, batch) in writes {
                        if batch.num_rows() == 0 {
                            continue;
                        }
                        tables::write_parquet(&opts.out.join(&rel), &batch)?;
                        emitted.push(rel);
                    }
                }
                // clean up files a previous emit of this session wrote elsewhere
                if let Some(prev) = state.sessions.get(&session_id) {
                    for old in &prev.emitted_files {
                        if !emitted.contains(old) {
                            let _ = std::fs::remove_file(opts.out.join(old));
                        }
                    }
                }
                if let Some(sess) = &output.session {
                    state.sessions.insert(
                        session_id.clone(),
                        SessionState {
                            project,
                            start_ms: sess.start_ts,
                            end_ms: sess.end_ts,
                            unknown_types: output.unknown_types.clone(),
                            emitted_files: emitted,
                        },
                    );
                }
                new_sources.extend(sources);
            }
        }
    }

    // keep checkpoint entries for previously seen sources outside this run's
    // filter (e.g. --project/--since narrowed scans must not forget them)
    let seen: BTreeSet<String> = new_sources.iter().map(|s| s.path.clone()).collect();
    for (path, src) in &prev_sources {
        if !seen.contains(path) {
            new_sources.push(src.clone());
        }
    }
    new_sources.sort_by(|a, b| a.path.cmp(&b.path));

    // ---- recount tables from what is actually on disk ----
    let mut table_stats: BTreeMap<String, TableStats> = BTreeMap::new();
    for table in tables::FACT_TABLES {
        let dir = opts.out.join("facts").join(table);
        table_stats.insert(table.to_string(), count_parquet(&dir)?);
    }
    if include_text {
        table_stats.insert("texts".into(), count_parquet(&opts.out.join("texts"))?);
    }

    // ---- manifest ----
    let mut unknown_types: BTreeMap<String, u64> = BTreeMap::new();
    let mut projects: BTreeSet<String> = BTreeSet::new();
    let (mut cov_start, mut cov_end): (Option<i64>, Option<i64>) = (None, None);
    for s in state.sessions.values() {
        for (k, v) in &s.unknown_types {
            *unknown_types.entry(k.clone()).or_default() += v;
        }
        projects.insert(s.project.clone());
        cov_start = Some(cov_start.map_or(s.start_ms, |c: i64| c.min(s.start_ms)));
        cov_end = Some(cov_end.map_or(s.end_ms, |c: i64| c.max(s.end_ms)));
    }
    let skipped_lines_total: u64 = new_sources.iter().map(|s| s.skipped_lines).sum();

    let manifest = Manifest {
        dataset: "grindstone".into(),
        schema_version: tables::SCHEMA_VERSION.into(),
        generator: Generator {
            name: "grindstone".into(),
            version: tables::GENERATOR_VERSION.into(),
        },
        generated_at: util::ms_to_rfc3339(now_ms),
        local_tz: util::local_tz_name(),
        config: Config {
            idle_threshold_ms: IDLE_THRESHOLD_MS,
            queue_wait_threshold_ms: session::QUEUE_WAIT_THRESHOLD_MS,
            include_text,
            tool_category_map_version: TOOL_CATEGORY_MAP_VERSION.into(),
            denial_patterns_version: DENIAL_PATTERNS_VERSION.into(),
        },
        coverage: Coverage {
            start: cov_start.map(util::ms_to_rfc3339),
            end: cov_end.map(util::ms_to_rfc3339),
            projects: projects.into_iter().collect(),
        },
        sources: new_sources,
        tables: table_stats,
        unknown_types,
        incomplete_sessions: incomplete_now.into_iter().collect(),
    };
    manifest.save(&opts.out)?;
    state.save(&opts.out)?;

    // ---- summary ----
    let t = |name: &str| manifest.tables.get(name).copied().unwrap_or_default();
    println!(
        "grindstone scan v{} → {}",
        tables::GENERATOR_VERSION,
        opts.out.display()
    );
    println!(
        "  files:     {} candidate(s) — {} processed, {} unchanged (skipped), {} failed",
        n_processed + n_skipped + n_failed,
        n_processed,
        n_skipped,
        n_failed
    );
    println!(
        "  sessions:  {} ({} incomplete/live)",
        t("sessions").rows,
        manifest.incomplete_sessions.len()
    );
    println!(
        "  cycles:    {}   api_calls: {}   tool_calls: {}   file_touches: {}",
        t("cycles").rows,
        t("api_calls").rows,
        t("tool_calls").rows,
        t("file_touches").rows
    );
    if include_text {
        println!("  texts:     {}", t("texts").rows);
    }
    println!("  skipped lines: {}", skipped_lines_total);
    if manifest.unknown_types.is_empty() {
        println!("  unknown types: none");
    } else {
        let s: Vec<String> = manifest
            .unknown_types
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        println!("  unknown types: {}", s.join(", "));
    }
    Ok(())
}

fn mtime_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn count_parquet(dir: &Path) -> Result<TableStats> {
    let mut files = Vec::new();
    collect_parquet(dir, &mut files)?;
    let rows: i64 = files
        .par_iter()
        .map(|f| tables::parquet_footer_info(f).map(|(r, _)| r))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum();
    Ok(TableStats {
        rows,
        files: files.len() as u64,
    })
}

pub fn collect_parquet(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        if e.file_type()?.is_dir() {
            collect_parquet(&p, out)?;
        } else if p.extension().and_then(|x| x.to_str()) == Some("parquet") {
            out.push(p);
        }
    }
    Ok(())
}
