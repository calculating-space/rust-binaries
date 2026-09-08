# rust-binaries

A collection of focused deterministic tools that people and agents can use on
their own or compose through files, stdin/stdout and library calls. Each package
has a concrete job, its own CLI, tests and documented output contract.

| Package | Job | Usable independently |
| --- | --- | --- |
| [trybox](trybox) | Guided tour of a project: menu-driven sandbox, requirements check, tour steps, resident agent | Yes |
| [speccheck](speccheck) | Machine spec vs requirements matrix: can run, pointless, or cannot run | Yes; JSON in, verdict out |
| [grindstone](grindstone) | Claude Code usage metrics in Parquet; dataset checks | Yes |
| [cs](cs) | Router: `cs <tool> ...` runs any tool above, building it on first use | Yes |

More tools from the same workshop will be published here as they settle.

## Quick start

On an Apple Silicon Mac, paste this once. It installs the prerequisites that
are missing (Xcode tools, Homebrew, Rust, uv, ffmpeg), clones the repository,
builds the `cs` router, and opens the trybox menu. About ten minutes on a
machine with nothing on it, mostly compiling and downloading.

```sh
curl -fsSL https://raw.githubusercontent.com/calculating-space/rust-binaries/main/bootstrap.sh | sh
```

Or by hand:

```sh
# Apple Silicon Mac. Linux and Intel: only the `bare` recipe runs today.
xcode-select --install 2>/dev/null || true
command -v brew >/dev/null || /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
command -v cargo >/dev/null || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
brew install uv ffmpeg
git clone https://github.com/calculating-space/rust-binaries.git ~/rust-binaries   # keep it here: cs remembers this path
cd ~/rust-binaries && cargo install --path cs
cs trybox
```

`cs trybox` is a menu: start a guided tour of a project (Whisper, MLX,
PyTorch), see it run on your machine, hand over to an agent, dispose of it when
done. The whisper tour downloads about 2 GB into `~/.trybox/whisper`; disposing
from the menu removes all of it. `cs trybox doctor` says what your machine can
run before anything is downloaded. See [trybox](trybox).

Have an agent do it instead: send it this sentence.

> Clone https://github.com/calculating-space/rust-binaries into ~/rust-binaries
> and follow its AGENTS.md until `cs trybox check whisper` passes, then tell me
> what to run.

## Boundaries

- Prefer adding a focused tool over expanding an unrelated CLI. A library and
  thin CLI should expose the same deterministic operation.
- Outputs describe the source. Consumer database IDs, graph schemas, UI titles,
  review states, model prompts, and publication workflows belong to the consumer.
- Programs compose via versioned data contracts and explicit exit statuses.
  They never parse each other's terminal prose or implicitly run source code.
- Reuse small implementation modules when there is a real shared need; avoid a
  central framework that every binary must depend on.
- There is no umbrella runtime CLI. `cs` is a router only: it locates a tool's
  binary and execs it, with no shared state, flags, or knowledge of the tools.
  A future root Cargo workspace would be build organization only, not an
  architectural dependency or shared application state.
- New tools get their own conformance tests.

Current dependency direction:

```text
trybox -> speccheck (requirements contract and pure checker)
grindstone -> its own metrics implementation
cs -> nothing; it only knows the directory layout
```

Every package builds on its own with
`cargo build --release --manifest-path <package>/Cargo.toml`. On macOS the
source-reading tools may enforce a no-network/no-exec sandbox; see each
package for its constraints.

## License

MIT, see [LICENSE](LICENSE).
