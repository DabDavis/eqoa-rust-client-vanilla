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
  In a page the driver runs on the game's thread and an AudioWorklet plays what `Audio::feed`
  mixes (`web/sound.js`); the music comes through `bgm.rs` (a BGM read whole before the device may
  start it, `Disc::ready`/`Link::bgm_ready`, always true natively). Headless: `web/drive.py` passes
  `--autoplay-policy=no-user-gesture-required`; `--js T=JSON.stringify(eqoaAudio.stats())` reads the
  worklet's counts (dry counts only after the first frames came).
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
  `opt-level = 3` in dev too (~110M interpreted instructions a module); tests that need SUPPORT take
  `run/support.bin`, else unpack it off the disc (`vi/tests/common`). Its natives (`src/native.rs`:
  SHA-1, RC6, CBC decrypt, libgcc's multiply and doubles, BSAFE's bignum, the buffer reader) stand in
  only where the function's CRC-32 is the proved one; a new one goes into `tests/natives.rs`, which
  checks every call of two whole unpacks against the interpreter (registers, result, every byte
  written). DNAS's own (self-decrypting) state machine stays interpreted, on purpose.
- `ee`'s arithmetic is the native fast path; its first implementation is the reference in
  `ee/tests/reference.rs` (every unary input, 2M pairs a regime; `EE_PAIRS=100000000` for more). Change
  `ee` only with that test passing and a planted bug failing it.
- Speed: `EQOA_PERF=1` (page: `?perf`) prints CPU time by phase; `EQOA_OFFSCREEN=WxH` measures on the
  real GPU without a presentable display (here: `DISPLAY=:99`, the Xvfb, whose Vulkan has no DRI3);
  `EQOA_QUIT=s`, `EQOA_WALK` script a run. A renderer change is held to `shot` PNGs byte for byte
  (`EQOA_HOUR` pinned; a shot's layers scroll by its script time).
- The `vi` structs are `#[repr(C)]` in the EE layout because the oracle compares raw bytes; keep it that way while ports are being proven. Later (not urgent): native runtime types beside them, converted at the edges, the layout types kept for oracle tests.
- `ee::libm` is held to `tools/vimath-corpus` (Go, needs `../go-eqoa-vi`): regenerate the corpus with
  `go run . > ../../crates/ee/tests/libm_corpus.txt` (Go build: `TMPDIR`/`GOTMPDIR` under `~/.cache`).
  It is text and committed: the pre-commit hook refuses binaries, and `*.bin` is ignored.
- Input (README "the pad", "the mouse and the touch screen"): the pad is the game's own path
  (`vi::pad` held to oracle ops 50-52; `input.rs` libpad on the host, `PadKeys`, `Play::set_key`,
  `controls.rs` UpdateControls). Headless: `EQOA_PAD` (`start@2500;stick l 0 -1@1500+2000`),
  `EQOA_MOUSE` (`click @<name>@ms`, `click X Y@ms`, `rclick`, `wheel N`), `EQOA_TOUCH=1`,
  `EQOA_TOUCHES`, `EQOA_TAP=<name|*>@ms` in a playshot; `drive.py --gamepad/--mobile/--touch/--mouse`
  in a page; `EQOA_PAD_TRACE=1` / `?padtrace`; `EQOA_GLYPHS=1` prints the button glyphs a frame drew.
  A DualSense is plugged in here and gilrs drives its motors: rumble is as real as the speakers.
  The local server spawns a character at a random heading each login, so click by name, not pixel.
- A pointer on the windows (`pointer.rs`, `Play::point`): every button glyph is its button (the
  HUD's raster notes them), and each window notes where it drew its rows (`pointer::Hits`) and
  answers `point(p) -> Vec<i32>`: the keys to press, `pointer::MOVED` for a selection it moved
  itself. Keys go through `Play::press`'s queue, one a frame, held through the next HUD frame
  (`KeyFlash` acts only on a key still down when the window draws). A new window gets a `point`,
  its route in `Play::window_point` (as `window_key` routes keys) and a test through
  `pointer::testing`'s null raster that fails without its hit notes.
- The browser build (`web/`, README "In a browser"): `web/build.sh`, then `web/drive.py` (headless
  Chromium over the DevTools protocol, the disc image handed to the page's file input). Platform
  code goes behind `net::session::Datagrams`, `net::auth::Exchange`, `iso::Source`/`iso::Sparse`
  and `crates/eqoa/src/web/`; std's `Instant` panics in a browser, so clocks are `web_time`'s.
- A build is `web/dist/v/<build>/`, never changed under its name (the host serves it immutable);
  only the page and `version.json` are revalidated, and `web/deploy.sh` publishes them in that
  order, after the build. Every file a page loads must be in its build (a new one goes into
  `build.sh`'s copy list). Reports, crash reports and notices are the page's (`web/report.js`,
  to reportd's `/eqoa/reports/`); the client feeds them through `web::problem`, `web::crash`,
  `web::note_state` and `web::frame_drawn`, the browser build only. Never log a credential: the
  console's last 200 lines go with every report. `web/pagetest.py` checks the page's half.
