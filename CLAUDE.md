# eqoa-rs

The Rust client. README.md is the plan and the crate table; keep it current.

- Toolchain: rustup in `~/.cargo`. `/tmp` is a small partition that old session scratchpads fill: building with `TMPDIR` under `~/.cache` avoids it.
- `run/` is untracked: `run/eqoa.iso` links the Frontiers disc. Never commit game data.
- Game-logic floats go through `ee::*`, never host `f32` operators; every compare too.
- Port from the decomp (`../decomp/src`), not from the Go client: the Go client's
  differences from the original are listed in `../claude-notes/go-client-vs-decomp-audit-20261015.md`.
- Each crate states what it is checked against; a port without a check is not done.
- `vi` is the game logic: port from the decomp line by line, generic over `Model`, `#[repr(C)]`
  in the EE layout, and add the method to eqoa-port's `test/oracle/oracle.cpp` and
  `crates/vi/tests/oracle.rs` in the same change. A new test needs a negative control.
- The oracle binary is `../eqoa-port/build/oracle` (`make oracle` there, never `make build/oracle`).
- The `vi` structs are `#[repr(C)]` in the EE layout because the oracle compares raw bytes; keep it that way while ports are being proven. Later (not urgent): native runtime types beside them, converted at the edges, the layout types kept for oracle tests.
- `ee::libm` is held to `tools/vimath-corpus` (Go, needs `../go-eqoa-vi`): regenerate the corpus with
  `go run . > ../../crates/ee/tests/libm_corpus.bin` (Go build: `TMPDIR`/`GOTMPDIR` under `~/.cache`).
