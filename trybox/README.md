# trybox

Try a project without reading its docs or touching your machine. Run
`trybox`, pick from a menu, press Enter.

```text
trybox: what do you want to do?
❯  1. Continue exploring mlx (Recommended)
      Apple's machine learning framework for Apple Silicon, with mlx-lm for running LLMs locally · hello + 7/10 steps · 2.3 GB · last used 3 min ago
   2. Start something new
      Pick a recipe: a guided tour of one project
   3. Dispose mlx
      Delete the sandbox and everything it downloaded, freeing 2.3 GB
   4. Quit
↑/↓ or j/k to move · Enter to choose · number to jump · q to go back
```

Every menu looks like this. One option is recommended and comes first. Arrow
keys or j/k move, Enter chooses, a number jumps, q goes back. Whenever a
sandbox exists, disposing it is one of the options.

## Your first five minutes

**Install.** On an Apple Silicon Mac, once. The first line installs whatever
prerequisites are missing (Xcode tools, Homebrew, uv, ffmpeg) and drops the
prebuilt `trybox` from the latest release into `/opt/homebrew/bin`; the second
block is the same by hand. No Rust needed.

```sh
curl -fsSL https://raw.githubusercontent.com/calculating-space/rust-binaries/main/bootstrap.sh | sh
```

```sh
xcode-select --install 2>/dev/null || true
command -v brew >/dev/null || /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
brew install uv ffmpeg
curl -fsSL https://github.com/calculating-space/rust-binaries/releases/latest/download/trybox-macos-aarch64.tar.gz | tar -xzf - -C /opt/homebrew/bin
```

Then `trybox` is the command, and `trybox doctor` says which recipes this
machine can run before anything is downloaded. To build from source instead,
`cargo install --path cs` in the repository gives you `cs`, and `cs trybox`
builds and runs this package; that is the only route on Linux and Intel Macs,
where the `bare` recipe is the one that runs, since the others need the Mac GPU.

**Start something new.** Run `cs trybox`, choose *Start something new*, and
pick a project. Each one says what it is, how many steps its tour has, and
whether your machine can run it.

```text
Which project do you want to explore?
❯  1. torch (Recommended)
      PyTorch, the most widely used deep learning framework, with Apple GPU acceleration · 4 steps · can run here
   2. mlx
      Apple's machine learning framework for Apple Silicon, with mlx-lm for running LLMs locally · 11 steps · can run here
   3. bare
      An empty Python environment to explore anything not covered by a recipe · 3 steps · can run here
```

**Watch it set up.** trybox checks your machine against the project's
requirements, prepares a private environment (about a minute the first
time, one status line, no wall of installer output), and runs the project's
hello world so you see it working before you decide anything:

```text
== MLX sees the GPU
   Imports MLX and asks which device it computes on.
   $ python -c 'import mlx.core as mx; print("mlx", mx.__version__, "on", mx.default_device())'

mlx 0.32.2 on Device(gpu, 0)

   expected: A version and `Device(gpu, 0)`.
```

**Take the tour.** Then the menu of steps. Each step says what it will show
you and what it will download, before you commit. Completed steps are ticked;
the next one is recommended. The hello world stays on the menu, so you can run
it again whenever you like.

```text
mlx: what next? (hello + 2/10 steps)
❯  1. Talk to a language model (Recommended)
      Downloads a 3B model in 4-bit (about 1.8 GB, once) and generates text. The last lines report tokens per second; that number is the point of MLX.
   2. ✓ MLX sees the GPU
      The hello world, any time. Imports MLX and asks which device it computes on.
   3. ✓ Arrays like NumPy, on the GPU
      The API is NumPy-shaped. Work is lazy: nothing computes until you ask for a value.
   4. Chat with it
      An interactive chat loop on the same model. Type `q` to leave.
   5. How fast is the GPU really
      A single large matrix multiply, timed after a warmup. Compare with the CPU number below.
   ...
  12. Hand over to the agent
      Open-ended: an agent inside the sandbox proposes and runs experiments with you
  13. Read the man page
  14. Dispose this sandbox
      Delete the environment and everything it downloaded, freeing 0.6 GB
  15. Back
```

Pick one, see it run, see what to look for, back to the menu. Quit whenever
you like; the sandbox keeps your progress and your downloads, and `trybox`
offers to continue next time.

## What a tour is

A tour is a recipe for one project, written so you never need to know how the
environment was built:

1. **What it is.** A paragraph on what the project does and why you would
   care.
2. **Hello world.** One command that proves it works on your machine.
3. **Steps, easy to advanced.** Each with a title, what it shows, the models
   it needs (fetched first, as a visible setup phase, so downloads never mix
   with the step's own output), the command, and what you should see. The mlx tour goes from
   arrays on the GPU, to talking with a language model, to measuring the GPU,
   to training a model from scratch, to quantizing a model yourself, to
   serving it as an API.
4. **The agent.** When the steps run out, hand over to a Claude Code session
   living in the sandbox. It arrives briefed on the project, the hardware,
   and what you have already done, proposes experiments, and waits for you to
   pick one.
5. **Dispose.** One choice deletes the environment, the downloaded models,
   and your work files, and tells you how much it freed.

Projects with a tour today:

| Recipe | What it is | Steps |
| --- | --- | --- |
| `mlx` | Apple's machine learning framework for Apple Silicon, with mlx-lm for running LLMs locally | 11 |
| `whisper` | OpenAI's Whisper speech-to-text on the Mac GPU through MLX | 8 |
| `torch` | PyTorch with Apple GPU acceleration | 4 |
| `bare` | An empty Python, for anything without a recipe yet | 3 |

## Will it run on my machine?

trybox tells you before it does anything. Every recipe carries a requirements
matrix, and the answer is one of three: **can run**, **runs but does not make
sense here** (say, a GPU framework on a machine with no GPU), or **cannot
run**. It also says which capability tiers your machine unlocks, so you know
whether a 70B model is realistic before you download one.

```text
mlx: CAN RUN

      NEED             SEVERITY     THIS MACHINE   WHY
  ok  macos/aarch64    blocks       macos/aarch64  no MLX wheels for Intel Macs or Windows
  ok  metal GPU        pointless    metal          without Apple Silicon MLX runs on the CPU ...
  ok  8 GB memory      blocks       64 GB          the hello world and a 3B model need about 4 GB
  ok  5 GB free disk   blocks       58 GB free     packages plus the first 3B model (1.8 GB)

  TIERS
  ok  small models        hello world and tour steps 1-5, 7, 8 (3B model)
  ok  8B models           tour step 6
  ok  30B-class models    go further: ~17 GB of weights
  ok  70B-class models    go further: ~40 GB of weights, tight even at 64 GB
```

A blocked project is shown with the reason and you are returned to the menu. A
pointless one asks whether you want to go ahead anyway. `trybox check mlx`
prints the table on its own.

## Nothing touches your machine

Everything a tour needs lives in one directory, `~/.trybox/<name>`: the Python
interpreter, the packages, the package caches, the downloaded models, and the
files you and the agent make. Nothing goes to your system Python, your global
caches, or `~/.cache/huggingface`. Disposing deletes that one directory.

Models are the big thing. Every step that downloads one says the size first.
`trybox` shows each sandbox's size in the menu, and `trybox status mlx` shows
how much of it is models.

Because the environment runs directly on your Mac, the GPU works. That is the
point for projects like MLX, and it is the reason trybox does not put the
environment in a container by default. (There is a `docker` backend for
things that truly need Linux. On a Mac it has no GPU, and the tool says so.)

## The agent

*Hand over to the agent* opens Claude Code inside the sandbox. It is told to
stay there and never write outside it. Its briefing, written into the
sandbox's `CLAUDE.md`, carries what the project is, your hardware, what is
installed, every tour command, the open-ended experiments the recipe suggests
(for mlx: bigger models, 4-bit versus 8-bit, prompt versus generation speed,
LoRA fine-tuning), and the caveats it must mention, such as reporting the
second run rather than the first. Its opening move is to confirm the setup,
rank a few experiments, and wait for you.

## Reference

Everything above is reachable from the menu. These are the same things as
commands, for scripts or for going straight to a point.

| Command | What it does |
| --- | --- |
| `trybox` | The menu |
| `trybox explore mlx` | Straight into one project's tour (creates the sandbox if needed) |
| `trybox recipes` | The projects with a tour |
| `trybox recipes mlx` | The tour as a man page, requirements marked against this machine |
| `trybox check mlx` | Can this machine run it? Exit `0` yes, `2` pointless, `3` cannot |
| `trybox list` | Every sandbox: what it is, progress, size, last used |
| `trybox status mlx` | One sandbox: every step's state, size, models, host |
| `trybox agent mlx` | The agent, directly. `agent mlx -- --permission-mode acceptEdits` forwards flags to claude |
| `trybox run mlx -- python -V` | One command inside the sandbox |
| `trybox dispose mlx -y` | Delete a sandbox. Several names, or `--all`. Asks unless `-y` |
| `trybox create NAME --recipe mlx` | Prepare without the tour; `--dry-run` prints the plan as JSON, `--notify` posts a desktop notification, `--agent` launches the agent when ready |
| `trybox doctor` | Which backends and the claude CLI are usable, and which recipes this machine can run (exit `3` when none) |

Outside a terminal (a pipe, a script) every menu prints numbered and reads a
line from stdin, so `printf '1\nq\n' | trybox explore mlx` runs the first
option and quits.

**Sandbox layout.** `~/.trybox/<name>` (override with `--root` or
`TRYBOX_ROOT`) holds `manifest.json` (what was created, on which host),
`progress.json` (which steps ran), `x` (a wrapper: `./x cmd` runs `cmd`
inside the environment), `.venv/`, `hf/` (models), `cache/` (uv and pip),
`work/` (your files and the agent's briefing), and for docker a `docker/`
with the generated Dockerfile.

**Backends.** `uv` (default; full Mac GPU access), `venv` (plain python3, no
uv needed), `docker` (Linux container, no GPU on macOS). A recipe names its
backend; `create --backend` overrides.

**Recipes are data.** Each lives in its own module under
[src/recipe/](src/recipe/) (`mlx.rs`, `whisper.rs`, ...); adding a
project is adding an entry: what it is, the hello world, the steps, the
agent's suggestions, the caveats, and the requirements matrix. The matrix uses
the [speccheck](../speccheck) contract, so it composes with the standalone
checker: `trybox recipes mlx --requirements | speccheck check --requirements -
--spec other-machine.json`.

**Exit status.** `0` success, `1` error, `2` dispose declined. `check` uses
speccheck's codes. `run` passes through the inner command's.

## How it compares

Other tools do one or two parts of this. None does all of it. The main reason is
that the Mac's GPU only works for programs running directly on the Mac, and
tools that wall an agent off do so by running it in a small Linux machine, where
the GPU is not available. trybox stays on the Mac so the GPU works, and pays for
that with weaker walls (see below).

| Tool | What it is | Explains the project and walks you through it | Its own environment, Mac GPU works | Hands over to an agent | Deletes everything in one go |
| --- | --- | --- | --- | --- | --- |
| **trybox** | this tool | yes | yes | yes, with a briefing | yes, models included |
| [try](https://github.com/tobi/try) | keeps dated experiment folders and finds them fast | no | folder only, no environment | no | folder only |
| [uv](https://docs.astral.sh/uv/) | installs Python packages into a separate environment | no | yes, if you set the cache locations yourself | no | delete the folder yourself |
| [pixi](https://pixi.sh), [devbox](https://www.jetify.com/devbox), [flox](https://flox.dev) | one environment per project, reproducible | no | yes, downloads go to a shared cache | no | partly, shared cache stays |
| [Claude Code sandbox](https://code.claude.com/docs/en/sandboxing), [agent-safehouse](https://github.com/eugene1g/agent-safehouse) | keeps an agent inside one folder | no | does not create one | yes, no briefing | no |
| [agentbox](https://github.com/madarco/agentbox), [Docker Sandboxes](https://github.com/docker/sbx-releases) | runs an agent in a small Linux machine | no | yes, but no GPU | yes | yes |
| [runme](https://runme.dev) | runs the commands in a markdown file step by step | steps only, you write the text | no | no | no |
| [tldr](https://tldr.sh), [navi](https://github.com/denisidoro/navi) | short command cheat sheets | text only, nothing runs | no | no | no |

What trybox does not do: with the `uv` and `venv` backends, nothing stops the
agent from writing outside the sandbox except its instructions. For a hard wall,
turn on Claude Code's own sandbox when running `agent` (GPU still works), or use
the `docker` backend (GPU does not). The full research with sources is in
[docs/prior-art.md](docs/prior-art.md).
