// The `ai` module that `cast run` puts in front of a script. Two calls, each
// a thin wrapper around one binary: `agent` runs `oneshot`, `bench` runs the
// command you give it. Nothing here talks to a model itself.
import { spawnSync } from "node:child_process"

const ONESHOT = process.env.AI_ONESHOT ?? "oneshot"
const AGENT = process.env.AI_AGENT ? JSON.parse(process.env.AI_AGENT) : []
const MAX_BUFFER = 256 * 1024 * 1024

let runs = 0

function seconds(since) {
  return Math.round((Date.now() - since) / 1000)
}

/// agent(config) -> { run(prompt), spent }
///
/// config: { model, system, tools, maxTurns, cwd, timeout, budget: { usd } }
/// run(prompt) resolves to the oneshot record: { ok, result, cost_usd,
/// num_turns, error, ... }. It refuses to start once budget.usd is spent.
export function agent(config = {}) {
  const spent = { usd: 0, runs: 0 }
  return {
    spent,
    async run(prompt) {
      const cap = config.budget?.usd
      if (cap !== undefined && spent.usd >= cap) {
        throw new Error(`ai: budget: $${spent.usd.toFixed(2)} spent of $${cap}`)
      }
      const args = ["--json"]
      if (config.cwd) args.push("--cwd", String(config.cwd))
      if (config.timeout) args.push("--timeout", String(config.timeout))
      for (const a of AGENT) args.push("--agent", a)
      const extra = []
      if (config.model) extra.push("--model", String(config.model))
      if (config.system) extra.push("--append-system-prompt", String(config.system))
      if (config.tools?.length) extra.push("--allowed-tools", config.tools.join(","))
      if (config.maxTurns) extra.push("--max-turns", String(config.maxTurns))
      if (extra.length) args.push("--", ...extra)

      const n = ++runs
      process.stderr.write(`ai: agent run ${n} started\n`)
      const t = Date.now()
      const r = spawnSync(ONESHOT, args, {
        input: String(prompt),
        maxBuffer: MAX_BUFFER,
        stdio: ["pipe", "pipe", "inherit"],
      })
      if (r.error) throw new Error(`ai: cannot start ${ONESHOT}: ${r.error.message}`)
      let rec
      try {
        rec = JSON.parse(r.stdout.toString())
      } catch {
        throw new Error(`ai: ${ONESHOT} printed no record (exit ${r.status})`)
      }
      spent.usd += rec.cost_usd ?? 0
      spent.runs += 1
      const cost = rec.cost_usd !== undefined ? `, $${rec.cost_usd.toFixed(2)}` : ""
      const turns = rec.num_turns !== undefined ? `, ${rec.num_turns} turns` : ""
      process.stderr.write(
        rec.ok
          ? `ai: agent run ${n} done in ${seconds(t)} s${cost}${turns}\n`
          : `ai: agent run ${n} FAILED after ${seconds(t)} s: ${rec.error ?? "unknown error"}\n`
      )
      return rec
    },
  }
}

/// bench(command) -> the benchmark's result
///
/// command: a shell string, or an argv array. The result is the command's
/// stdout parsed as JSON, or `{ loss }` from a `METRIC loss=<float>` line.
/// A command that prints neither is a failed candidate, not a crash:
/// `{ loss: Infinity, exit, error }` with the tail of what it said.
export async function bench(command) {
  const t = Date.now()
  const opts = { maxBuffer: MAX_BUFFER, stdio: ["ignore", "pipe", "pipe"] }
  const r = Array.isArray(command)
    ? spawnSync(command[0], command.slice(1), opts)
    : spawnSync("sh", ["-c", String(command)], opts)
  if (r.error) throw new Error(`ai: cannot start bench command: ${r.error.message}`)
  const out = r.stdout.toString()
  const err = r.stderr.toString()
  if (err) process.stderr.write(err)

  let result
  try {
    result = JSON.parse(out)
  } catch {
    const m = out.match(/METRIC loss=([-+0-9.eE]+|inf)/)
    if (m) result = { loss: m[1] === "inf" ? Infinity : Number(m[1]) }
  }
  if (result === null || typeof result !== "object") {
    const tail = (err || out).trim().split("\n").slice(-5).join("\n")
    result = { loss: Infinity, exit: r.status, error: `bench exited ${r.status}: ${tail}` }
  }
  process.stderr.write(`ai: bench loss ${result.loss} in ${seconds(t)} s\n`)
  return result
}
