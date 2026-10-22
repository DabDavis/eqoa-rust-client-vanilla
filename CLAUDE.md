# eqoa-rust-client-vanilla

The Rust client without mods, and the only public EQOA client repository: eqoa-rs (`../eqoa-rs`,
github.com/DabDavis/eqoa-rs) less its ambient occlusion, its harvester and its bug and crash
reports. eqoa-rs, eqoa-port and the retired Go client (eqoa-client-v1) are private since
2026-10-22. README.md is the plan, the crate table and what "vanilla" keeps (README "Vanilla");
keep it current.

- Vanilla: nothing that changes the game's picture or play beyond what README "Vanilla" keeps
  (widescreen, the untethered frame rate, the mouse and the touch screen, the keyboard with WASD and
  the Controls page, 1,000 lines of chat, no green backdrop behind the start menu's windows, the
  Extended page, the map). A new client-side addition is the user's call: ask first. A fix to the
  game's logic, sound or network goes both ways with eqoa-rs by `git cherry-pick` (remote `eqoa-rs`,
  `git@github.com:DabDavis/eqoa-rs.git`). Link no private repository from anything public here.
- Commit as Winfro <164797966+DabDavis@users.noreply.github.com> (the repo-local config; never the
  global address). `.git/hooks/pre-commit` refuses private strings, binaries and files over 2 MB.
- Releases are CI only (`.github/workflows/dev.yml`): every push to `master` builds Linux, Windows and
  macOS and replaces the one `dev` release (its tag moved to the commit, its assets replaced, the run's
  artifacts deleted), so no older build can be downloaded. No versioned releases, no other channel.
- Windows: a program's main thread has the stack its header reserves (1 MB by default), and the game's
  first frame needs about that much, its frames holding their state by value: `crates/eqoa/build.rs`
  links 8 MB, as Linux and macOS give, and `dev.yml` refuses a build with less. Wine does not show an
  overflow there (its main thread gets more); a Windows machine does
  (`../claude-notes/windows-main-stack-20261022.md`: GitHub's Windows runners, a stand-in disc).

- Toolchain: rustup in `~/.cargo`. `/tmp` is a small partition that old session scratchpads fill: building with `TMPDIR` under `~/.cache` avoids it.
- `run/` is untracked: `run/eqoa.iso` links the Frontiers disc. Never commit game data.
- A player's disc (`discs.rs`): the game (`eqoa`, `boot`, `eqoa <image>`) takes the named image, `EQOA_ISO`,
  `disc.opt` (the last pick, in the config directory), `run/eqoa.iso`; with none it asks in rfd's native dialog
  inside `resumed`, before the window (not before the event loop: a Mac has no running application there), judges
  the pick by SYSTEM.CNF as the page does, and keeps it. `EQOA_OFFSCREEN` never shows a dialog. Test the Linux
  dialogs on :99 with a private bus (`dbus-run-session`, xdg-desktop-portal-gtk) or none (zenity): the session
  bus's portal draws on the user's own screen.
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
- `ee`'s adder has the EE's one guard bit (PCSX2's recompiler, `FPU_ADD_SUB`); go-eqoa-vi measured through
  the tracer's `call`, which runs PCSX2's INTERPRETER (every bit kept): that adder is `ee::interp`.
  `ee::interp::libm` (the same newlib over it) is held to `tools/vimath-corpus` (Go, needs `../go-eqoa-vi`):
  regenerate the corpus with `go run . > ../../crates/ee/tests/libm_corpus.txt` (Go build: `TMPDIR`/`GOTMPDIR`
  under `~/.cache`). It is text and committed: the pre-commit hook refuses binaries, and `*.bin` is ignored.
- The game runs 30 frames a second; the client draws at the display's rate. What the game counts in frames
  (the HUD and every window, `KeyFlash`, `TextScroll`, the rings' easing, the death camera, the shake) steps
  on `frames.rs`'s clock (`game_frame`, `Play::hud`), never once a frame drawn; a new frame-counted thing
  goes there, with a test at 30 and 165 Hz. What moves by the clock runs every frame drawn.
- Input (README "the pad", "the mouse and the touch screen"): the pad is the game's own path
  (`vi::pad` held to oracle ops 50-52; `input.rs` libpad on the host, `PadKeys`, `Play::set_key`,
  `controls.rs` UpdateControls). Headless: `EQOA_PAD` (`start@2500;stick l 0 -1@1500+2000`),
  `EQOA_MOUSE` (`click @<name>@ms`, `click X Y@ms`, `rclick`, `wheel N`, `press l|r`, `release l|r`,
  `move DX DY`: a drag past the slop takes the pointer as a window would, `mouse::Grab`), `EQOA_TOUCH=1`,
  `EQOA_TOUCHES`, `EQOA_TAP=<name|*>@ms` in a playshot; `drive.py --gamepad/--mobile/--touch/--mouse`
  in a page; `EQOA_PAD_TRACE=1` / `?padtrace`; `EQOA_GLYPHS=1` prints the button glyphs a frame drew.
  A DualSense is plugged in here and gilrs drives its motors: rumble is as real as the speakers.
  The local server spawns a character at a random heading each login, so click by name, not pixel.
  The player's own keys and buttons (`binds.rs`, the options' Controls page) work at the edge only:
  a moved key becomes its row's default key before the keyboard dispatch (never while typing), the
  controllers' buttons the game's fields before `VIPad::Read`. A key added to the dispatch needs a
  row in `binds::ROWS` (a test reads `viewer.rs` for them).
- A pointer on the windows (`pointer.rs`, `Play::point`): every button glyph is its button (the
  HUD's raster notes them), and each window notes where it drew its rows (`pointer::Hits`) and
  answers `point(p) -> Vec<i32>`: the keys to press, `pointer::MOVED` for a selection it moved
  itself. Keys go through `Play::press`'s queue, one a frame, held through the next HUD frame
  (`KeyFlash` acts only on a key still down when the window draws). A new window gets a `point`,
  its route in `Play::window_point` (as `window_key` routes keys) and a test through
  `pointer::testing`'s null raster that fails without its hit notes.
- The pregame (`crates/eqoa/src/pregame/`, README "the pregame"): `eqoa` (or `eqoa boot [server]`) runs the game
  from the disc's boot in SLUS's scene order (`boot.rs` the loading pictures, `setup/`, `station/`, `client/` with
  `client/create/`), then hands `Play` the session (`host.rs`); a logout or a lost connection comes back to CLIENT
  (`Pregame::back`). Port from `../decomp/modules/{setup,station,client}/src`; eqoa-port runs the real screens (pad
  scripts in `~/.cache/vanilla-boot/ref{1,2}.pad`, `EQOA_NO_SETUP=1` for STATION on): compare shots. Files go through
  `pregame::files` and the network through `pregame::link` (natively threads; in a page its reader and the relay),
  so nothing in a stage is platform code. Headless: `DISPLAY=:99 EQOA_AUDIO=0 EQOA_OFFSCREEN=WxH EQOA_KEYS='enter@3000;
  text name@4000' EQOA_PAD=... EQOA_SHOTS='3000=a.png' EQOA_QUIT=s eqoa boot <server>` (shots in play's frames too);
  in a page `web/drive.py --boot --keys/--type`. One login at a time per account, and a run that is killed holds its
  session about a minute: give each run's account time, or another account. `eqoa play <server> <account>` still goes
  straight into the world for scripts and playshots.
- The browser build (`web/`, README "In a browser"): `web/build.sh`, then `web/drive.py` (headless
  Chromium over the DevTools protocol, the disc image handed to the page's file input). Platform
  code goes behind `net::session::Datagrams`, `net::auth::Exchange`, `iso::Source`/`iso::Sparse`
  and `crates/eqoa/src/web/`; std's `Instant` panics in a browser, so clocks are `web_time`'s.
- A build is `web/dist/v/<build>/`, never changed under its name (the host serves it immutable);
  only the page and `version.json` are revalidated, and `web/deploy.sh` publishes them in that
  order, after the build. Every file a page loads must be in its build (a new one goes into
  `build.sh`'s copy list). Notices are the page's (`web/notices.js`): the client raises them
  through `web::problem` (a part gone: the player can dismiss it) and `web::crash` (the client
  stopped: a Reload), the browser build only. Nothing is sent anywhere: no reports, no telemetry.
  Never log a credential. `web/pagetest.py` checks the page's half.
