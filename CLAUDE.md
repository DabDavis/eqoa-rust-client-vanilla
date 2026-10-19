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
  `make oracle` also builds `build/oracle_drdp`, the decomp's DRDP library with a recording socket
  (stateful ops 700-710), which `crates/drdp/tests/oracle_drdp.rs` drives step by step.
- `make oracle` also builds `build/oracle_ui`: SUPPORT's UI TUs (viui, viuiwindow, viuitextscroll,
  vifont) compiled over a RECORDING raster (libsupport is `-Bsymbolic`, so interposing on it does not
  work; compile the TUs in instead). `crates/vi/tests/oracle_ui.rs` compares call for call. A CLIENT
  free function is reached by `dlopen("libclient.so")` in the oracle (op 800, the swing tables).
- `make oracle` also builds `build/oracle_snd`: SNDDRV's four TUs over eqoa-port's SPU2 on a
  deterministic coroutine IOP (timer every 480 samples, vblank every 800), and SUPPORT's
  VISoundDevice TUs in the EE layout above it (op 910). `crates/snd/tests/oracle_snd{,dev}.rs`
  compare sample for sample and packet for packet. Live audio tests: `EQOA_AUDIO=0` (the driver
  runs on the wall clock, silent), or `eqoa listen` to a WAV; the user's speakers are real.
- `run/support.bin` (untracked) is the player's decrypted SUPPORT image; the HUD's atlas table is read
  from it at run time (eqoa-port's gendata does the same). Never commit it or anything read from it.
  `run/client.bin` is CLIENT's (a link to `claude-eqoa-iso/decrypted_modules/CLIENT_decompressed_SNR1.bin`):
  the menus and gWorldInfo, its pointers resolved through the SNR1 relocations (`eqoa/src/client_image.rs`).
- The `vi` structs are `#[repr(C)]` in the EE layout because the oracle compares raw bytes; keep it that way while ports are being proven. Later (not urgent): native runtime types beside them, converted at the edges, the layout types kept for oracle tests.
- `ee::libm` is held to `tools/vimath-corpus` (Go, needs `../go-eqoa-vi`): regenerate the corpus with
  `go run . > ../../crates/ee/tests/libm_corpus.bin` (Go build: `TMPDIR`/`GOTMPDIR` under `~/.cache`).
