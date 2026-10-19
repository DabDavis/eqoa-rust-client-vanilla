# eqoa-rs

The Rust client. README.md is the plan and the crate table; keep it current.

- Toolchain: rustup in `~/.cargo`. `/tmp` is full: point `TMPDIR` at a directory under `~/.cache`.
- `run/` is untracked: `run/eqoa.iso` links the Frontiers disc. Never commit game data.
- Game-logic floats go through `ee::*`, never host `f32` operators; every compare too.
- Port from the decomp (`../decomp/src`), not from the Go client: the Go client's
  differences from the original are listed in `../claude-notes/go-client-vs-decomp-audit-20261015.md`.
- Each crate states what it is checked against; a port without a check is not done.
