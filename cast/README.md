# cast

A tiny script language for spawning agents. A script is one expression;
its list holds the items and its `map` names the role each agent plays.
`cast` runs one headless Claude Code per item, all at the same time:

```sh
cast '[paper1.pdf, paper2.pdf].map(p => researcher)'
```

That starts two agents, one per paper, each with the `researcher` prompt and
its paper filled in, and prints both answers when the slower one finishes.

```sh
cast '[a, b, c].map(p => researcher)' --out results/     # results/researcher-a.md + .json each
cast '[a, b].map(p => reviewer)' --dry-run                # show the prompts, run nothing
cast '[a, b].map(p => r)' --json                          # one JSON array of records
cast '[a, b, c, d].map(p => r)' --jobs 2 --timeout 600    # at most two at once, 10 min each
cast '[a, b].map(p => r)' -- --model haiku --max-turns 8  # flags after -- go to claude
```

`cs cast ...` from a source checkout. Each run is a [oneshot](../oneshot),
so everything oneshot says about the agent command, the record and Claude
Code's own flags applies here.

## The expression

`[item, item, ...].map(param => role)`

- **items** are whatever text you want each agent to work on: file paths,
  directory names, URLs, titles. Quotes are optional; commas separate.
- **role** names the prompt. If `<role>.md` exists in the current directory
  (or in `--prompts DIR`), that file is the prompt, with every `{param}`
  replaced by the item. A file without the placeholder gets `Input: <item>`
  appended. Without a file, a built-in prompt is used: *You are the role.
  Your input is: item. If the input names a file or directory, read it first.
  Do the job the role name implies and answer in Markdown.*
- Also accepted: `->` for `=>`, `(p) =>`, `role(p)` as the body, and
  `.map(role)` with no lambda (the parameter is then `p`).

A `researcher.md` might be:

```markdown
Read {p} in full. Summarise the claim, the method, the evidence and the
weakest point in under 300 words. Quote page numbers.
```

## Output

By default the results print to stdout in list order, each under a
`## role(item)` heading, once every agent has finished. Progress goes to
stderr as agents start and finish, with the cost and turn count of each, and
one closing line: `2 of 2 ok, $0.06, 16 s`.

`--out DIR` writes `DIR/<role>-<item>.md` (the answer) and
`DIR/<role>-<item>.json` (the full oneshot record plus `role` and `item`) per
item, and prints the `.md` paths. `--json` prints all records as one array.

A failed agent does not stop the others: its slot shows the error and
whatever text it produced, the rest print normally, and the exit status is 1.

## Exit status

| status | meaning |
| --- | --- |
| 0 | every agent answered |
| 1 | at least one agent failed or timed out |
| 2 | `cast` could not start: bad expression, unreadable prompt file, agent not found |

## Scripts: `cast run`

When one map is not enough, write the loop in TypeScript and let `cast`
run it. The script imports two calls from `"ai"`; everything else is the
language you already know.

```sh
cast run loop.ai
```

```ts
import { agent, bench } from "ai"

const config = {
  model: "claude-fable-5-1",
  system: "Improve naive-grep so the benchmark loss drops. One change per run.",
  tools: ["Read", "Edit", "Bash"],
  cwd: "naive-grep",
  budget: { runs: 20, usd: 10 },
}
const judge = "cs judge naive-grep naive-grep/target/release/naive-grep --json"
const target = 0.9

const coder = agent(config)
let result = await bench(judge)
while (result.loss > target && config.budget.runs-- > 0) {
  await coder.run(`loss ${result.loss}, target ${target}`)
  result = await bench(judge)
}
console.log(result.loss <= target ? "resolved" : "out of budget", result.loss)
```

- **`agent(config).run(prompt)`** runs one headless Claude Code through
  [oneshot](../oneshot) and resolves to its record: `ok`, `result`,
  `cost_usd`, `num_turns`, `error`. The config's `model`, `system`, `tools`,
  `maxTurns`, `cwd` and `timeout` become the agent's flags (`--model`,
  `--append-system-prompt`, `--allowed-tools`, `--max-turns`, `--cwd`,
  `--timeout`). `budget.usd` is enforced: once that much is spent, the next
  `run` throws. `spent.usd` and `spent.runs` on the agent say where it stands.
- **`bench(command)`** runs a shell string (or an argv array) and returns its
  stdout as JSON, or `{ loss }` from a `METRIC loss=<float>` line, which is
  what [judge](../judge) prints. A command that prints neither is a failed
  candidate, not a crash: `{ loss: Infinity, exit, error }`.

Both print one progress line per call to stderr, like `cast` does. The
script's stdout, stderr and exit status pass straight through.

The `ai` module is embedded in `cast`. `cast run` copies the script next to
it in a temporary directory, so the bare import resolves, and runs the copy
with [bun](https://bun.sh) (or node 24 or newer) from the current directory.
No transpiler, no package manager: the script is TypeScript, the calls are
binaries. `--agent` swaps the agent command as it does for a map, `--runtime`
names the runtime, `--oneshot` names the binary when it is not next to
`cast`, in its source tree, or on PATH. Keep the script to one file; it
cannot import its neighbours from the temporary directory.

## What it does not do

No `filter`, `reduce` or chaining; one map per invocation. No retries, no
caching of finished runs, no model calls of its own. Pipe the `--out` files
into the next `cast` or `oneshot` for a second stage.

## Tests

`cargo test --release` runs conformance tests against
`tests/fixtures/fake-agent.sh`, a stand-in that speaks the
`claude -p --output-format json` shape: parallel timing with and without
`--jobs`, prompt files and placeholders, `--out`, `--json`, a failing item,
pass-through flags. `cast run` is tested against `fake-oneshot.sh` and
`fake-bench.sh` with `tests/fixtures/loop.ai`: the loop runs until the loss
resolves, both budgets, the METRIC fallback, failing commands and agents,
and the flags that reach oneshot; those tests skip with a note when neither
bun nor node is installed. No test calls a model. The default path was run once for
real against Claude Code 2.1.263 with two toy papers on haiku: 16 s, $0.06.
