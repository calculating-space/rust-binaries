# grindreport

Reads a [grindstone](../grindstone) dataset (Parquet facts plus
`manifest.json`) and renders a usage report. One analysis pass, four
renderings of the same numbers:

| format | for | what you get |
| --- | --- | --- |
| `text` (default) | a person at a terminal | aligned tables, sparklines, a weekday-by-hour grid |
| `md` | an agent | a Markdown digest with a glossary, stable headings and full session ids |
| `html` | a person in a browser | one self-contained file with inline SVG charts, sortable tables, light and dark |
| `json` | a program | the report struct itself, `report_version` stamped |

```
grindreport [--out ./out] [--format text|md|html|json] [--to FILE]
            [--since 30d] [--from YYYY-MM-DD] [--to-date YYYY-MM-DD]
            [--project TEXT]... [--top N] [--pricing pricing.toml]
```

```sh
grindstone scan --out ./out                       # build the dataset first
grindreport --out ./out                           # terminal summary
grindreport --out ./out -f html -o report.html    # open locally in a browser
grindreport --out ./out -f md --since 7d          # paste into an agent's context
grindreport --out ./out -f json | jq .totals
```

Exit 0 rendered, 1 usage or write error, 2 no dataset or a schema major this
reader does not understand.

## What the report contains

- **Totals**: sessions, cycles, API calls, tool calls, tokens by kind, agent
  active time against wall time, files touched.
- **Activity**: cycles per day, and cycles by weekday and local hour.
- **Projects**: ranked by agent-active time, labelled by working directory,
  the tail folded into `other`.
- **Models**: calls and tokens per model, output share.
- **Tools**: calls, share, errors, denials, retries, sidechain share, p50 and
  p90 duration per tool; totals per category.
- **Cycles**: prompt kinds, how cycles ended, and p50/p90/max/mean of first
  response, active time, human dwell, prompt length, tool calls and output
  tokens per cycle; rework and sidechain counts.
- **Friction**: error, denial, retry, interruption and rework rates; the
  tools and sessions with the most of them; stop reasons, effort, entrypoints.
- **Sessions**: the top N by active time with enough to find the transcript.
- **Cost**: only with `--pricing`, see below.
- **Notes**: every caveat that applies (live sessions, skipped lines, unknown
  entry types, the idle threshold, unpriced models, files without the
  grindstone metadata stamp).

Tool counts come from the `tool_calls` table, so subagent (sidechain) calls
are included; the sessions table's own rollups leave them out. Time windows
(`--since`) are anchored on the dataset's coverage end, not the clock, so the
same command on the same dataset always produces the same report.

## Pricing

grindstone stores token counts, never prices (spec §5). To see cost, hand in
a price table in the currency and units of the provider's price list, per
million tokens:

```toml
version = "2026-09"           # recorded in the report
currency = "USD"

[models."claude-opus-5"]      # exact model id, or a prefix (longest wins)
input = 15.0
output = 75.0
cache_read = 1.5
cache_write_5m = 18.75
cache_write_1h = 30.0
```

The values above are placeholders: copy the current numbers from the
provider's price list. Models without a matching entry are listed as
unpriced in the notes and left out of the total. JSON with the same shape is
accepted too.

## Reading the numbers

- A *session* is one transcript file; a *cycle* is one human prompt and
  everything until the next.
- *Agent active* sums prompt-to-last-event time over cycles and across
  parallel sessions, so a day can hold more than 24 hours. *Wall* is a
  session's first-to-last event.
- *Human dwell* is the gap to the next prompt; gaps over the idle threshold
  (in the manifest, default 30 minutes) are excluded and counted as idle.
- *Rework* is a file edited again two or more events later in the same
  session, a deterministic proxy, not a judgment.
- Percentiles are nearest-rank. Local times use the manifest's `local_tz`.

## What a report reveals

A report is a summary, not the dataset, but it still names your projects by
working directory (with your home folder collapsed to `~`), git branches,
session ids and the machine's time zone. It never carries prompt or message
text: grindstone's facts hold none, and grindreport reads only the facts.
The HTML file makes no network requests and loads nothing from outside
itself. Write reports next to the dataset (`grindstone/out/` is gitignored)
or somewhere else outside the repository; the tests use a synthetic dataset,
so nothing of yours is needed to build or check the tool.

## Contract

`--format json` serializes the `Report` struct; `report_version` bumps on any
breaking change to its shape. The reader accepts any grindstone dataset with
schema major 1 and refuses others. Columns are looked up by name and cast, so
dictionary encoding, integer widths and missing optional columns are all
fine; only `session_id` is required.

Tests: `cargo test --release` builds a small synthetic dataset with the real
column layout in a temporary directory and checks totals, rankings, filters,
pricing, every renderer, HTML escaping and the schema gate.
