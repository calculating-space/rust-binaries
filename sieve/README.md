# sieve

What passes into an agent's context has to get through it. `sieve` governs
the size of Claude Code tool results from inside hooks. It ships two pieces:

- **`sieve hook PostToolUse`**: a Claude Code hook that folds any Bash result
  larger than 16 KB to its first 8 KB, one marker line, and its last 4 KB.
  The full output is on disk; the marker says where. No model calls, no
  network, no text stored beyond the spill file.
- **`sieve run`**: the same folding as a standalone command wrapper, for
  scripts, other harnesses, or people.

```sh
sieve install --dry-run       # the exact settings.json diff; nothing written
sieve install                 # ~/.claude/settings.json; --project for ./.claude/settings.json
sieve uninstall               # removes only sieve's entries
```

## Why this and not a setting

Claude Code (2.1.263) already saves a Bash result over 30,000 characters to
a file and shows the model a 2 KB preview and the path, and a setting lowers
that threshold to 4,000. Sieve differs in two ways that matter for test runs
and builds: it shows the **tail**, where the failure summary is, and it
writes a **ledger** so the effect can be measured. Between 16 KB and 30,000
characters it bounds output the CLI would pass through whole.

## The hook

Registered for `PostToolUse` with matcher `Bash`. Input is the hook JSON on
stdin; output is a `hookSpecificOutput.updatedToolOutput` that replaces the
result, or nothing. The command is never modified, so permission rules such
as `Bash(cargo test:*)` keep matching.

Per Bash result:

| result | sieve does |
| --- | --- |
| stdout and stderr each at most `cap_bytes` | nothing; one `observe` ledger line |
| stdout over the cap, already persisted by the CLI (`persistedOutputPath`) | reads the tail from that file; the marker names it; no copy is made |
| stdout over the cap, in memory | writes `~/.sieve/spill/<session_id>/<tool_use_id>.out`, folds |
| stderr over the cap | same, to `<tool_use_id>.err`, folded separately |

The folded text is `head`, a newline if the head lacks one, the marker, then
`tail`. Cuts land on character boundaries. All other fields of the tool
result (`interrupted`, `isImage`, `noOutputExpected`) pass through; the
CLI's `persistedOutputPath` and `persistedOutputSize` are dropped so the CLI
does not re-render the folded text as a preview.

```text
<first 8192 bytes>
[sieve: 106.3 KB total, 94.3 KB not shown; full output in /Users/me/.claude/projects/.../tool-results/btr3lr9jj.txt; Grep it or Read a range]
<last 4096 bytes>
```

**Fail open.** Malformed stdin, an unknown event, a tool other than Bash, a
missing persisted file, an unwritable spill directory, a broken policy file,
or an internal panic: the hook exits 0 and, where it can, still folds using
what it has. It never blocks or delays a tool call beyond its own runtime,
p99 under 30 ms on an Apple Silicon Mac.

**Off switch.** `SIEVE=off` in the environment, or `enabled = false` in the
policy.

## Policy

`~/.sieve/policy.toml`, then `<cwd>/.claude/sieve.toml` key by key. Missing
files mean the defaults below. Unknown keys or bounds with `cap_bytes <
head_bytes + tail_bytes` make that file invalid: it is skipped with a
warning on stderr and the previous values stand.

```toml
version = "1"
enabled = true

[bash]
cap_bytes = 16384
head_bytes = 8192
tail_bytes = 4096

[ledger]
enabled = true
```

## Ledger

`~/.sieve/ledger/<session_id>.jsonl`, one line per Bash result. Numbers and
ids only; no command text, no output text. `v` bumps on a breaking change.

```json
{"v":"1","ts":"2026-09-08T15:53:12.409Z","session_id":"…","prompt_id":"…","agent_id":null,
 "event":"PostToolUse","tool":"Bash","rule":"bash-spill","tool_use_id":"toolu_…",
 "output_bytes":108894,"stderr_bytes":0,"persisted":true,"action":"rewrite","shown_bytes":12288,"spilled_bytes":96606}
```

`action` is `observe` (under the cap) or `rewrite`. Grindstone can join
these to its sessions by `session_id` to compare result bytes per call
before and after the hook was installed.

`$SIEVE_HOME` moves the whole `~/.sieve` directory, for tests or a second
profile.

## `sieve run`

The standalone wrapper. Runs the command with `sh -c` in the current
directory and environment, stdout and stderr merged in arrival order, stdin
inherited. The full output streams to a spill file while it runs. Exit
status is the command's; a command killed by a signal exits 128 plus the
signal number.

Under the cap the wrapper is invisible: exactly the command's bytes, the
same exit status, and the spill file is deleted (keep it with `--keep`). A
conformance test asserts byte identity against `sh -c` for a corpus of
commands. Over the cap the layout is the same as the hook's.

| flag | default | meaning |
| --- | --- | --- |
| `--cap N` | 16384 | print everything when the output is at most N bytes; must be at least head + tail |
| `--head N` | 8192 | bytes shown from the start when over the cap |
| `--tail N` | 4096 | bytes shown from the end when over the cap |
| `--id ID` | time and pid | spill file name, `<ID>.out`; `/` is replaced by `_` |
| `--session S` | none | subdirectory for the spill file |
| `--spill-dir DIR` | `$SIEVE_HOME/spill`, else `~/.sieve/spill` | where spill files go |
| `--keep` | off | keep the spill file even under the cap |

Everything after `--` is the command; several words are joined with single
spaces. `SIGINT`, `SIGTERM` and `SIGHUP` are forwarded to the shell, including
one that arrives before the shell exists. Exit `1` for a usage error or when
the shell cannot start.

## Not here yet

Read-range denial, the context budget, duplicate-read and explore-delegate
nudges, and `sieve report`. The ledger comes first so those can be judged on
data. Note that a resumed session's `SessionStart` hook already receives
`context_tokens` and `prompt_cache_likely_expired` from the CLI (2.1.251+),
so the cold-cache note needs no transcript parsing when it is added.

## Build and test

```sh
cargo test --release --manifest-path sieve/Cargo.toml
```

`tests/fixtures/` holds redacted hook inputs recorded from a real Claude Code
2.1.263 session; the hook tests run the binary against them.

## Measuring on Terminal-Bench

Whether the hook helps an agent is an empirical question, and Terminal-Bench
answers it: the same tasks, scored by tests that do not care about the hook.
`harbor/agent.py` is a [Harbor](https://www.harborframework.com) agent,
Harbor's own `claude-code` agent plus the hook. `--ak hook=false` runs the
identical agent without it, so the two arms differ in exactly one thing.

```sh
cross build --release --target x86_64-unknown-linux-musl   # terminal-bench-2 images are amd64
cd .. && PYTHONPATH=. harbor run -d terminal-bench/terminal-bench-2 \
    -a sieve.harbor.agent:SieveClaudeCode -m anthropic/claude-sonnet-5 \
    -t terminal-bench/regex-log -y                 # one arm
PYTHONPATH=. harbor run ... --ak hook=false -y     # the other
```

Each trial's directory then holds the transcript under `agent/sessions/`
and the ledger under `agent/sieve/ledger/`. The hook's `--install-only`
path is verified; the A/B itself is described in `docs/harbor.md`.
