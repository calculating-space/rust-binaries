# For agents: setting this up and working on it

This repository is a set of small deterministic Rust tools. The one people
come for is **trybox**, a guided tour of a machine-learning project that runs
in a disposable sandbox on an Apple Silicon Mac. Read this whole file before
running anything; it is short.

## Platform

The mlx and whisper tours need an Apple Silicon Mac, because they use the
Metal GPU. On Linux or an Intel Mac only the `bare` recipe runs. Say so to the
person before spending time on the other steps.

## Set it up

The prebuilt binary is the normal route on an Apple Silicon Mac; no Rust, no
clone. Each line is idempotent; skip what is already there.

```sh
xcode-select -p >/dev/null 2>&1 || xcode-select --install      # a dialog; wait for it
command -v brew >/dev/null || /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
brew install uv ffmpeg
curl -fsSL https://github.com/calculating-space/rust-binaries/releases/latest/download/trybox-macos-aarch64.tar.gz | tar -xzf - -C /opt/homebrew/bin
```

`curl -fsSL https://raw.githubusercontent.com/calculating-space/rust-binaries/main/bootstrap.sh | sh`
does the same with one status line per step, and falls back to a source
build when no release binary is available.

From source, which is the only route on Linux or an Intel Mac, and the route
for working on the code:

```sh
command -v cargo >/dev/null || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
git clone https://github.com/calculating-space/rust-binaries.git ~/rust-binaries
cd ~/rust-binaries && cargo install --path cs
cs --build trybox
```

Keep the clone at `~/rust-binaries`: `cs` records the path at build time and
stops working if the directory moves. Below, `trybox` means `cs trybox` when
you built from source.

## Verify

```sh
trybox doctor            # a table of recipes with can run / cannot run and the reason
trybox check whisper     # exit 0 means the whisper tour will run here
trybox recipes           # four recipes: mlx, whisper, torch, bare
```

If `check` fails, its table names the missing piece and the command that
installs it (for example `brew install ffmpeg`). Fix that and check again.

## Rules

- Install nothing into the system Python and never use `sudo` yourself.
  Homebrew and the Xcode installer ask for a password on their own.
- Do not move the clone after `cargo install --path cs`.
- Do not start the whisper hello world yourself. It is an interactive prompt
  where the person types a sentence and hears it; that moment is theirs.
  When `check whisper` passes, tell them to run `trybox`, pick *Start
  something new*, then *whisper*, and hand the terminal back.
- Everything a tour downloads lives under `~/.trybox/<name>`; disposing from
  the menu, or `trybox dispose <name> -y`, removes it.

## When something fails

- `trybox check <recipe>` prints the requirements table with the reason.
- `trybox create <name> --recipe <recipe>` shows the full installer output
  that the menu hides.
- `trybox doctor --json` gives the same report as data.

## Working on the code

- One Cargo package per directory, no workspace. Build and test one with
  `cargo test --release --manifest-path <package>/Cargo.toml`.
- `cs` only runs the binary that exists; after changing a package, rebuild it
  (`cargo build --release` in its directory, or `cs --build <package>`).
- Releases: tag `trybox-vX.Y.Z` on main and the release workflow publishes
  `trybox-macos-aarch64.tar.gz`, which bootstrap.sh and the README fetch.
  Bump `trybox/Cargo.toml`'s version to match before tagging.
- Recipes are data in `trybox/src/recipe/`, one module per recipe. Adding one
  is a new file, a `mod` line, and an entry in the `RECIPES` list. Every
  command in a recipe is run for real before it is written down.
- Machine requirements go through the `speccheck` contract, never through ad
  hoc checks in a recipe or an agent's head.
- Commits in this repository are authored by calculating-space.
