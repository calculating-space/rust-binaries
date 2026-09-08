# grindstone

Turns Claude Code session transcripts (`~/.claude/projects/**/*.jsonl`) into a
Parquet metrics dataset. Spec: `ressource-allocation/research/log-analyzer.md`
(§4 schema and §6 manifest are the contract, schema_version 1.0.0).

```
grindstone scan   [--root ~/.claude/projects] [--out ./out] [--since 30d]
                  [--project <slug>]... [--include-text] [--force]
grindstone check  [--root ...] [--out ./out]   # manifest/hash/schema integrity
grindstone enrich | stats                      # v0 stubs, exit 2
```

Output: `out/facts/{sessions,cycles,api_calls,tool_calls,file_touches}/`
partitioned `project=<slug>/date=YYYY-MM-DD/<session-id>.parquet`, plus
`manifest.json`. No message bodies in facts — lengths and sha256 only;
`--include-text` writes a separate `texts/` table. Re-runs are incremental
no-ops via sha256+size checkpoints; live (still-growing) sessions are marked
`is_complete = false` and re-emitted next run.

Format notes discovered against CLI 2.1.x logs (2026-08):

- Sidechains are no longer inline `isSidechain` entries; subagent transcripts
  live at `<slug>/<session-id>/subagents/agent-*.jsonl` and are grouped into
  the parent session, attributed to the cycle active at their first timestamp.
- One assistant API response spans 1–4 `assistant` entries sharing a
  `requestId` (one per content block, identical usage) — `api_calls` groups.
- The CLI enqueues/dequeues *every* prompt (passthrough p99 ≈ 88 ms);
  `prompt_kind = queued` requires a queue wait > 2 s (threshold in manifest).
- Tool denials have a structured `toolDenialKind` on the result-carrying user
  entry, used in addition to the versioned text-pattern list.
- `state.json` is an internal checkpoint sidecar (session spans for
  `overlap_sessions`, emitted-file tracking); `manifest.json` is the contract.
  `overlap_sessions` is computed against all known sessions at emit time;
  already-emitted rows are not retroactively updated (use `--force` to
  recompute everything).

Tests: `cargo test` snapshot-verifies derived cycle/tool_call rows for three
redacted real transcripts under `tests/fixtures/` (regenerate snapshots with
`UPDATE_SNAPSHOTS=1`; fixtures were redacted via `tools/redact.py`).


## Related standalone tools

For source messages, completed exchanges and exact text spans, use
`claude-transcript` (not yet published). For immutable source capture,
validation and local evidence packs, use `sourcepack` (not yet published).
Each tool has its own library, CLI, tests and output contract. Grindstone has no
dependency on either tool or on a consuming application.
