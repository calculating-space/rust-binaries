# speccheck

Capture a machine's spec and check it against a requirements matrix. The
answer is one of three outcomes: **can run**, **runs but does not make sense
here** (for example a GPU framework on a machine with no GPU), or **cannot
run**. Tiers say which capability levels the machine unlocks.

```sh
speccheck spec --tool uv --tool docker          # this machine, as JSON
speccheck check --requirements mlx.json         # verdict against this machine
speccheck check --requirements mlx.json --spec other-machine.json --json
```

Exit status of `check`: `0` can run, `2` pointless, `3` cannot run, `1` error.

## Contracts

All three are versioned JSON (`"version": 1`).

**Spec**: `os`, `arch`, `chip`, `cores`, `memory_gb`, `disk_free_gb` (at
`disk_path`), `gpus` (`kind` metal|cuda|other, `name`, `memory_gb` or null for
unified memory), `tools` (name to version line; absent means not found).

**Requirements**: `subject`, `rules`, `tiers`. A rule is a `check` plus a
`severity` (`blocks`, `pointless`, `recommended`) and a `why`. A tier is a
name, what it `enables`, and the checks that unlock it. Checks:

| kind | fields | passes when |
| --- | --- | --- |
| `platform` | `any_of: [[os, arch], ...]` | the machine's os/arch pair is listed |
| `gpu` | `any_of: [metal, cuda, other]` | any GPU of a listed kind is present |
| `tool` | `name` | the tool is in the spec's `tools` |
| `min_memory_gb` | `gb` | memory is at least `gb` |
| `min_disk_free_gb` | `gb` | free disk is at least `gb` |
| `min_cores` | `n` | logical cores are at least `n` |

**Verdict**: `outcome`, one finding per rule (`pass`, `actual`, `why`), one
result per tier (`ok`, `missing`).

The worst failed severity decides the outcome. Recommended failures only add a
note.

## Library

`speccheck::check(&spec, &requirements)` is pure. Only `speccheck::spec::detect`
touches the machine (sysctl or /proc, `df -k`, `nvidia-smi`, and `--version`
of the tools the requirements ask about). trybox uses the library to gate
`explore` and to print the matrix on a recipe's man page.
