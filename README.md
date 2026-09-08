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

## Try something

```sh
cargo install --path cs && cs trybox
```

A menu: start a guided tour of a project (MLX, PyTorch), see it run on your
machine, hand over to an agent, dispose of it when done. See [trybox](trybox).

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
