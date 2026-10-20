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
- CLIENT and SUPPORT (the start menu, gWorldInfo, the race cameras; the HUD's atlas, the UI sounds'
  volumes) are read at run time out of images of those modules (`eqoa/src/modules.rs`): the player's
  decrypted ones when given (`run/client.bin`, `run/support.bin`, untracked; here links into
  `claude-eqoa-iso/decrypted_modules/`), else unpacked off the disc by `dnas` (natively on threads,
  kept in `~/.cache/eqoa-rs/dnas` or `$EQOA_CACHE`; in a page in workers, `web/unpack.js`, kept in
  OPFS). An image counts only when it is the Frontiers build's (`modules::check`). Never commit them
  or anything read from them: the HUD's atlas was built in for a day (2026-10-20, the user's "for
  now") and is SUPPORT's again (`vi::ui::read_atlas`). CLIENT's pointers resolve through the SNR1
  relocations (`eqoa/src/client_image.rs`). `dnas` is held to the modules the game itself unpacked
  (`tests/discs.rs`: `run/`, and `$EQOA_BUILDS`, default `../decomp-builds`) and builds at
  `opt-level = 3` in dev too (~300M interpreted instructions a module); tests that need SUPPORT take
  `run/support.bin`, else unpack it off the disc (`vi/tests/common`).
- The `vi` structs are `#[repr(C)]` in the EE layout because the oracle compares raw bytes; keep it that way while ports are being proven. Later (not urgent): native runtime types beside them, converted at the edges, the layout types kept for oracle tests.
- `ee::libm` is held to `tools/vimath-corpus` (Go, needs `../go-eqoa-vi`): regenerate the corpus with
  `go run . > ../../crates/ee/tests/libm_corpus.bin` (Go build: `TMPDIR`/`GOTMPDIR` under `~/.cache`).
- The browser build (`web/`, README "In a browser"): `web/build.sh`, then `web/drive.py` (headless
  Chromium over the DevTools protocol, the disc image handed to the page's file input). Platform
  code goes behind `net::session::Datagrams`, `net::auth::Exchange`, `iso::Source`/`iso::Sparse`
  and `crates/eqoa/src/web/`; std's `Instant` panics in a browser, so clocks are `web_time`'s.
