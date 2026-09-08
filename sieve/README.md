# sieve

What passes into an agent's context has to get through it. `sieve` is the
context governor for Claude Code sessions. Today it ships one piece, the
bounded runner; the hook rules that decide per tool call are the next step.

```sh
sieve run -- 'cargo test 2>&1'                   # output bounded to 16 KB; the rest on disk
sieve run --cap 4096 --head 3072 --tail 1024 --id toolu_01 --session s1 -- 'seq 1 100000'
```

## `sieve run`

Runs the command with `sh -c` in the current directory and environment,
stdout and stderr merged in arrival order, stdin inherited. The full output
streams to a spill file while it runs. Exit status is the command's; a
command killed by a signal exits 128 plus the signal number, as a shell would.

**Under the cap the wrapper is invisible.** If the command writes at most
`--cap` bytes, `sieve run` prints exactly those bytes and exits with the same
status as the bare command; the spill file is deleted (keep it with `--keep`).
A conformance test asserts byte identity against `sh -c` for a corpus of
commands, including binary output and missing trailing newlines.

**Over the cap** it prints the first `--head` bytes, one marker line, and the
last `--tail` bytes:

```text
<first 8192 bytes>
[sieve: 412.3 KB total, 400.3 KB not shown; full output in /Users/me/.sieve/spill/s1/toolu_01.out; Grep it or Read a range]
<last 4096 bytes>
```

The marker starts on its own line: a newline is added before it only when the
head does not already end with one. Nothing is added after the tail. If the
spill file cannot be written the command still runs and the marker says
`full output not saved: <reason>` instead of the path.

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
spaces, so `sieve run -- ls -la` and `sieve run -- 'ls -la'` are the same.

`SIGINT`, `SIGTERM` and `SIGHUP` sent to `sieve run` are forwarded to the
shell it started, including a signal that arrives before the shell exists.
The shell decides what its own children see; `sh -c 'one command'` execs the
command directly on bash and dash, so the signal reaches it.

Exit status: the command's; `1` for a usage error (bad bounds, no command) or
when the shell could not be started, with a message on stderr.

## Cost

One `fork`/`exec` and a 64 KB read loop; `sieve run -- true` finishes in
about 10 ms on an Apple Silicon Mac. Spill files are never removed by `run`
when the output spilled; delete `~/.sieve/spill` when it grows.

## Not here yet

The hook side (`sieve hook <event>`, the read-range, budget and duplicate-read
rules, the per-session ledger, `sieve install`) is specified but not built.
The runner is deliberately first: it is the only rule that bounds a result
without costing a round trip, and it is useful on its own.

## Build and test

```sh
cargo test --release --manifest-path sieve/Cargo.toml
cs sieve run -- 'echo hi'          # from the repo router
```
