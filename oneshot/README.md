# oneshot

Hand a prompt to an agent, get its result back. `oneshot` is the piece between
"I have a question in a file" and "I have the answer in a file": it runs
Claude Code headless (`claude -p`), waits, and returns the result as text or as
one JSON record with the exit status, duration, cost and turn count. Scripts,
cron jobs, other tools and agents can call it without knowing how `claude`
prints things.

```sh
oneshot "Summarise this repository in five lines"            # result text on stdout
oneshot --file task.md --out answer.md                        # prompt from a file, answer to a file
oneshot --file task.md --json > run.json                      # the whole record instead
oneshot "..." --cwd ~/project --timeout 600 -- --model haiku  # where, how long, which model
cat task.md | oneshot --record run.json                       # text on stdout and the record on disk
```

`cs oneshot ...` from a source checkout.

## What it does

1. Reads the prompt from the argument, `--file`, or stdin.
2. Runs the agent command with the prompt on its stdin, in `--cwd` (default:
   here), killing the whole process group after `--timeout` seconds.
3. Parses the agent's stdout as the single JSON object that
   `claude -p --output-format json` prints, and lifts out `result`, `is_error`,
   `session_id`, `total_cost_usd` and `num_turns`. With `--text`, stdout is
   the result as is.
4. Prints the result text (or writes it to `--out`), or with `--json` prints
   the record. `--record FILE` writes the record as well, whichever way the
   run ended.

The agent is `claude -p --output-format json` unless `--agent` names another
program; repeat `--agent` for each argument. Anything after `--` is appended
to the agent command, so Claude Code's own flags (`--model`, `--max-turns`,
`--allowed-tools`, `--permission-mode`, `--append-system-prompt`) pass
straight through. `oneshot` does not add flags of its own: permissions, tools
and model are whatever the agent and its settings decide.

## The record

```json
{
  "version": "oneshot/1",
  "prompt": "Reply with the single word pong and nothing else.",
  "agent": ["claude", "-p", "--output-format", "json", "--model", "haiku"],
  "cwd": "/Users/me/project",
  "started_unix": 1788974400,
  "duration_ms": 9120,
  "exit_code": 0,
  "timed_out": false,
  "ok": true,
  "result": "pong",
  "stderr": "",
  "session_id": "740ceee1-...",
  "cost_usd": 0.0199,
  "num_turns": 1,
  "agent_output": { "type": "result", "subtype": "success", "usage": { "...": "..." } }
}
```

`ok` is true when the agent exited 0, was not killed, and (for JSON agents)
printed an object without `is_error: true`. When it is false, `error` says
why in one line. `agent_output` is the agent's JSON untouched, for fields not
lifted above (`usage`, `modelUsage`, `permission_denials`, `stop_reason`).
`result` is always present: the lifted text, or raw stdout when the output
could not be parsed. Optional fields are omitted rather than null.

## Exit status

| status | meaning |
| --- | --- |
| 0 | the agent answered |
| 124 | the agent ran past `--timeout` and was killed |
| 2 | `oneshot` could not start: no prompt, unreadable file, agent not found |
| other | the agent's own exit status; 1 when it exited 0 but set `is_error` or printed something that is not JSON |

On failure the reason and the agent's stderr go to `oneshot`'s stderr; the
result text (if any) still goes to stdout, so a partial answer is not lost.
`-v` shows the agent's stderr on success too.

## What it does not do

No retries, no queue, no model calls of its own, no network. It does not
change the prompt or add a system prompt. Nothing is stored except the
`--out` and `--record` files you asked for; Claude Code keeps its own session
under `~/.claude` as it always does.

## Tests

`cargo test --release` runs conformance tests against
`tests/fixtures/fake-agent.sh`, a shell stand-in that speaks the
`claude -p --output-format json` shape: success, `is_error` with exit 1 and
with exit 0, non-JSON output, stderr noise, a sleeper for the timeout, and the
working-directory and pass-through flags. No test calls a model. The default
path was run once for real against Claude Code 2.1.263 when the tool was
written.
