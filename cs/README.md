# cs

A router: `cs <tool> [args...]` runs a tool from this repository, building it
on first use. It exists so the tools can be reached from any shell without
adding each `target/release` directory to PATH.

```sh
cargo install --path cs          # once; puts `cs` in ~/.cargo/bin

cs                               # list tools, built or not, with descriptions
cs trybox explore mlx            # runs trybox (builds it first if needed)
cs speccheck spec                # any tool, same way
cs --where trybox                # print the binary path
cs --build                       # build every tool; or cs --build trybox speccheck
```

`cs` knows two things: the repository root (fixed at build time from its own
package location, so reinstall after moving the checkout) and cargo's binary
layout. It resolves `<root>/<tool>/target/release/<tool>`, then `debug`, and
execs it with the remaining arguments untouched. Exit status, stdin, stdout
and signals are the tool's own.

It is not an umbrella application: no shared state, no shared flags, no
knowledge of what any tool does. Every tool remains fully usable on its own.
