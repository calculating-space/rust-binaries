# Prior art: does trybox need to exist?

Researched 2026-09-08. Verdict: yes, narrowly. No existing tool covers the
combination trybox is built for, and the two halves of the market are separated
by a hardware constraint, not by a missing feature. The mechanism is thin;
the value is in the recipes and in the glue.

## What trybox is, in five features

- F1 recipes: built-in man page per project (what it is, hello world, tour, caveats)
- F2 isolated env: per-sandbox venv with its own HF_HOME and uv/pip caches, host GPU intact
- F3 tour: run the hello world, step through numbered commands interactively
- F4 agent handoff: exec Claude Code in the sandbox with a generated CLAUDE.md briefing
- F5 destroy: one command deletes env, models, work files, image

## The constraint that splits the field

Metal is only available to host processes. Every VM-based sandbox on macOS is
CPU-only for MLX and PyTorch MPS:

- Apple `container`: maintainer says GPU passthrough is unsupported; Apple GPUs
  lack IOMMU. https://github.com/apple/container/discussions/62
- Docker Sandboxes `sbx` (runs claude/codex/gemini/opencode in Arm Linux
  microVMs): no GPU or Metal mentioned anywhere. https://github.com/docker/sbx-releases
- Lima, OrbStack, BoxLite: no GPU. Colima 0.10 exposes Vulkan via libkrun,
  which helps llama.cpp but not MLX or MPS. https://colima.run/announcements/colima-v0.10.0-release/

So any tool that gives real isolation cannot run the mlx or torch recipes
usefully, and any tool that keeps Metal is a host process. trybox's uv backend
is the second kind. Its docker backend is the first kind and is honest about it.

## Closest existing tools, by feature

| Tool | What it is | F1 | F2 | F3 | F4 | F5 |
| --- | --- | --- | --- | --- | --- | --- |
| tobi/try (4k stars, Ruby) https://github.com/tobi/try | dated experiment dirs with fuzzy picker | no | dir only | no | no | dir delete |
| timofurrer/try (unmaintained since 2022) https://github.com/timofurrer/try | `try requests` in a temp venv, auto-deleted | no | venv, shared caches | no | no | yes |
| uv (`uv run --with`, `UV_CACHE_DIR`) https://docs.astral.sh/uv/concepts/cache/ | the env primitive trybox wraps | no | yes with manual env vars | no | no | rm -rf |
| pixi / devbox / flox / devenv | per-project reproducible shells with tasks | no | env yes, caches global | tasks, not stepwise | no | shared store |
| Claude Code sandbox + srt https://github.com/anthropic-experimental/sandbox-runtime | Seatbelt confinement, Metal intact | no | no | no | confines, no briefing | no |
| agent-safehouse, nono, cco | Seatbelt or Docker wrappers around claude | no | no | no | confines, no briefing | no |
| agentbox https://github.com/madarco/agentbox | Docker boxes per project, `destroy` | no | Docker, no Metal | no | yes | yes |
| Docker sbx | microVM agent sandboxes, network policy | templates | no Metal | no | yes | `sbx rm` |
| runme https://github.com/runmedev/runme | executes markdown code cells stepwise | content-less | no | yes | no | no |
| mask, xc, just | markdown/task runners | no | no | named tasks | no | no |
| navi, tldr, cheat | static cheatsheets | static | no | no | no | no |
| mlx-examples, mlx-lm | the raw recipe content, as repos | READMEs | no | no | no | no |

Nothing found ships recipes in trybox's sense: a man-page tour with hello world,
expected output, download sizes and caveats, tied to an environment.

## Replacing trybox with a composition

uv (venv under a chosen root, `UV_CACHE_DIR` and `HF_HOME` inside it)
+ tobi/try (directory management)
+ runme (markdown tour, stepwise)
+ Claude Code `/sandbox` or agent-safehouse (confine claude to the dir, Metal intact)
+ `rm -rf` (destroy)

What that still lacks:

1. Recipe content. You would write the mlx and torch tours yourself.
2. Cache isolation by default. Every piece needs the env vars wired by hand.
3. A generated briefing. No tool writes a per-sandbox CLAUDE.md with host
   hardware, installed packages, known-good commands and caveats.
4. A warning that the Docker path is CPU-only for MLX and MPS.
5. One-shot destroy that also covers models and caches.

## Honest weaknesses

- The "sandbox" in the uv and venv backends is not a sandbox. Nothing but the
  system prompt stops the agent writing outside the directory. Claude Code's
  own sandbox settings or agent-safehouse would add real confinement at no
  cost to Metal; trybox does not use them yet.
- The docker backend duplicates Docker sbx and agentbox with less isolation and
  the same lack of GPU. It is the weakest part of the tool.
- The `bare` recipe is `uv venv` plus env vars. On its own it does not justify
  the binary.
- Recipes are Rust consts. Adding one means a rebuild, and model names and
  package APIs in the tours will go stale. The value of the tool scales with
  recipe count and freshness, which is a content maintenance job, not code.

## Conclusion

Keep it. The reason to exist is: recipe-driven, Metal-preserving experiment
environments with isolated caches, a stepwise tour, an agent briefing and one
destroy. No single tool does that and the VM-based alternatives cannot.
The reason is only as strong as the recipes, so the investment should go into
recipes and into real confinement for the agent, not into the docker backend.
