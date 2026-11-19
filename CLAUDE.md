# eqoa-rust-client-vanilla

The Rust client for *EverQuest Online Adventures: Frontiers*, without mods, ported from the matching decomp
([eqoa-decomp](https://github.com/DabDavis/eqoa-decomp)) and held to it. This file is the contributor guide;
README.md is the plan, the crate table and what "vanilla" keeps (README "Vanilla"): keep it current.

## What belongs here

- Vanilla: nothing that changes the game's picture or play beyond what README "Vanilla" keeps (widescreen, the
  untethered frame rate, the mouse and the touch screen, the keyboard with WASD and the Controls page, 1,000 lines
  of chat, no green backdrop behind the start menu's windows, the Extended options, the updater, the 2002 disc's
  art on F9, Hide UI on H, names from farther and smaller zoomed out (and every one in a crowd, Overlapping Names
  Show), Ignore Shouts kept, Update Notices, Text Sharp or Smooth, Loading Screens Longer or Instant, Render Scale
  x1, x2 or x4 (off unless chosen), HUD Nearest or Sharp (Nearest, the game's, by default: Sharp has a cost)).
  A new client-side addition is the maintainer's call: ask first. Sharp filtering is for the 2D layer and the text
  only: the world's textures are drawn smooth, as the game filters them.
- Never commit game data (`*.iso`, ESF/CSF files, decrypted modules, anything read out of them) or binaries. The
  repository's `.gitignore` covers the usual ones; `run/` is untracked (`run/eqoa.iso` links your Frontiers disc).
- Never run `cargo fmt`: the code is not rustfmt-styled and a format pass rewrites hundreds of files.
- Stage files by name, never `git add -A`.

## Releases and update notes

- Releases are CI only (`.github/workflows/dev.yml`): every push to `master` builds Linux, Windows and macOS and
  replaces the one `dev` release (its tag moved to the commit, its assets replaced, the run's artifacts deleted),
  so no older build can be downloaded. No versioned releases, no other channel.
- **A push to `master` reaches every player at their next start**: the updater (`update.rs`) reads the release's
  `update.txt` (dev.yml writes it, uploaded after the programs) and puts a build of another commit in place before
  the title; while the game runs it looks again (3 minutes after the start, then every 10) and asks the player to
  restart into a newer one in a game window listing what is new (`update::Offer`, `updatewnd.rs`: the release's
  `update-notes.md`, which dev.yml uploads before update.txt, against the build's own UPDATE-NOTES.md). So master
  is always a build fit to play: test before pushing, and never push a half-done change. Only a build that carries
  `EQOA_UPDATE_CHANNEL` and `EQOA_BUILD_COMMIT` (CI's) updates itself; dev.yml refuses one without its commit. Test
  the updater with `EQOA_UPDATE_URL=<base>` against a server of your own, never by pushing.
- **Update notes**: a push that changes the program adds a section on top of `UPDATE-NOTES.md` (`## <date> ·
  <title>`, then bullets: what a player sees, in a player's words), in the same push. The release page shows them,
  newest first (`release/notes.sh`): the sections new since the last build marked as this build's, and a build with
  none lists its commits there instead ("not in the notes yet"). An edit to the notes alone goes on the page
  through `.github/workflows/notes.yml`. The game shows them too: the update's question lists the release's
  sections its own copy lacks, and a page's Reload notice the new build's (its `update-notes.md`): write them for
  players.

## Platforms

- Windows: a program's main thread has the stack its header reserves (1 MB by default), and the game's first frame
  needs about that much, its frames holding their state by value: `crates/eqoa/build.rs` links 8 MB, as Linux and
  macOS give, and `dev.yml` refuses a build with less. Wine does not show an overflow there (its main thread gets
  more); a Windows machine does: test Windows builds on GitHub's Windows runners, not Wine.
  It is a GUI program (`main.rs`'s `windows_subsystem`; `dev.yml` refuses a console one): no terminal beside the
  game. Run from a terminal it writes there (`attach_console`), but neither cmd's `>` nor PowerShell's capture
  reaches it: capture a command's output with `Start-Process -Wait -NoNewWindow -RedirectStandardOutput`.
- Toolchain: rustup. If `/tmp` is a small partition, build with `TMPDIR` on a larger disk.
- A player's disc (`discs.rs`): the game (`eqoa`, `boot`, `eqoa <image>`) takes the named image, `EQOA_ISO`,
  `disc.opt` (the last pick, in the config directory), `run/eqoa.iso`; with none it asks in rfd's native dialog
  inside `resumed`, before the window (not before the event loop: a Mac has no running application there), judges
  the pick by SYSTEM.CNF as the page does, and keeps it. In Steam's Game Mode (`discs::game_mode`: gamescope, which
  shows no portal's window) a missing disc is first looked for where a Deck keeps games (`deck_places`), then asked
  for in zenity's dialog. `EQOA_OFFSCREEN` never shows a dialog. Test the Linux dialogs on a virtual display with a
  private bus (`dbus-run-session`, xdg-desktop-portal-gtk) or none (zenity): the session bus's portal draws on your
  own screen.

## Fidelity rules

- Game-logic floats go through `ee::*`, never host `f32` operators; every compare too.
- Port from the decomp (`decomp/src`, `decomp/modules/*/src`), not from older Go clients or other reference
  implementations.
- Each crate states what it is checked against; a port without a check is not done.
- `vi` is the game logic: port from the decomp line by line, generic over `Model`, `#[repr(C)]` in the EE layout,
  and add the method to the oracle (`test/oracle/oracle.cpp` in the decomp's native build) and
  `crates/vi/tests/oracle.rs` in the same change. A new test needs a negative control.
- The oracle binaries are built from the decomp as a native program (`make oracle`) and are not distributed here;
  point `EQOA_ORACLE` at `build/oracle`. Without them the oracle tests skip, loudly. `make oracle` also builds:
  - `build/oracle_drdp`, the decomp's DRDP library with a recording socket (stateful ops 700-710), which
    `crates/drdp/tests/oracle_drdp.rs` drives step by step;
  - `build/oracle_ui`: SUPPORT's UI TUs (viui, viuiwindow, viuitextscroll, vifont) compiled over a RECORDING
    raster (libsupport is `-Bsymbolic`, so interposing on it does not work; compile the TUs in instead).
    `crates/vi/tests/oracle_ui.rs` compares call for call. A CLIENT free function is reached by
    `dlopen("libclient.so")` in the oracle (op 800, the swing tables);
  - `build/oracle_snd`: SNDDRV's four TUs over an SPU2 on a deterministic coroutine IOP (timer every 480 samples,
    vblank every 800), and SUPPORT's VISoundDevice TUs in the EE layout above it (op 910).
    `crates/snd/tests/oracle_snd{,dev}.rs` compare sample for sample and packet for packet.
- The `vi` structs are `#[repr(C)]` in the EE layout because the oracle compares raw bytes; keep it that way while
  ports are being proven. Later (not urgent): native runtime types beside them, converted at the edges, the layout
  types kept for oracle tests.
- `ee`'s arithmetic is the native fast path; its first implementation is the reference in `ee/tests/reference.rs`
  (every unary input, 2M pairs a regime; `EE_PAIRS=100000000` for more). Change `ee` only with that test passing
  and a planted bug failing it.
- `ee`'s adder has the EE's one guard bit (PCSX2's recompiler, `FPU_ADD_SUB`); the console goldens were measured
  through a tracer that runs PCSX2's INTERPRETER (every bit kept): that adder is `ee::interp`. `ee::interp::libm`
  (the same newlib over it) is held to `crates/ee/tests/libm_corpus.txt` (generated by `tools/vimath-corpus`). It
  is text and committed: binaries are refused, and `*.bin` is ignored.
- The game runs 30 frames a second; the client draws at the display's rate. What the game counts in frames (the
  HUD and every window, `KeyFlash`, `TextScroll`, the rings' easing, the death camera, the shake) steps on
  `frames.rs`'s clock (`game_frame`, `Play::hud`), never once a frame drawn; a new frame-counted thing goes there,
  with a test at 30 and 165 Hz. What moves by the clock runs every frame drawn, but the walk: its integrator's
  speed grows as its step shrinks, so it keeps the console's own frames (33.37 ms) and is drawn on its way to the
  next (`walker.rs`, `Kept`); never step it a frame drawn.

## Game data read at run time

- CLIENT and SUPPORT (the start menu, gWorldInfo, the race cameras; the HUD's atlas, the UI sounds' volumes) are
  read at run time out of images of those modules (`eqoa/src/modules.rs`): the player's decrypted ones when given
  (`run/client.bin`, `run/support.bin`, untracked), else unpacked off the disc by `dnas` (natively on threads,
  kept in the platform cache's `eqoa-rs/dnas` or `$EQOA_CACHE`; in a page in workers, `web/unpack.js`, kept in
  OPFS). An image counts only when it is the Frontiers build's (`modules::check`). Never commit them or anything
  read from them (the HUD's atlas is SUPPORT's, `vi::ui::read_atlas`). CLIENT's pointers resolve through the SNR1
  relocations (`eqoa/src/client_image.rs`). `dnas` is held to the modules the game itself unpacked
  (`tests/discs.rs`: `run/`, and `$EQOA_BUILDS`) and builds at `opt-level = 3` in dev too (~110M interpreted
  instructions a module); tests that need SUPPORT take `run/support.bin`, else unpack it off the disc
  (`vi/tests/common`). Its natives (`src/native.rs`: SHA-1, RC6, CBC decrypt, libgcc's multiply and doubles,
  BSAFE's bignum, the buffer reader) stand in only where the function's CRC-32 is the proved one; a new one goes
  into `tests/natives.rs`, which checks every call of two whole unpacks against the interpreter (registers,
  result, every byte written). DNAS's own (self-decrypting) state machine stays interpreted, on purpose.
- The 2002 art (`era.rs`, README "The 2002 art"): on F9, a file off the player's USA disc of 2002 stands in for its
  namesake in the library's 2002 view (`esf::Library::stand_in`); loading screens and the title too, while Tunaria
  keeps Frontiers' ambient sounds and fog/dust/leaf/flock effects. Build a model in its own view
  (`lib.with_view(lib.view_for(id), ..)`); what walks the files by name for their surfaces walks `lib.in_view()`
  and takes a 2002 file's through `UiAssets::add_surfaces_of`; anything made out of the library is made again when
  the art turns (`Play::art_changed`, `viewer::rebuild_art`, `Host::art_changed`). UI.CSF's atlas sheet stays
  Frontiers' (`era::kept`). Nothing off the 2002 disc is committed; tests take it from `$EQOA_2002_ISO` and skip
  without. `EQOA_ART_AT=<ms>;<ms>` presses F9 in a scripted run.
- A server's patch (`s2.rs`, README "the server's patch"): what a server's patcher puts in its folder (its S2, its
  art file, WORDS.TXT) is read as the patch's SS stub applies S2 on a PS2, feature by feature at the addresses S2
  hooks; no S2, or one the stub refuses, is retail. Never commit an S2 or anything read out of one (it is another
  server's code); tests take them from `$EQOA_S2_BUILDS` and skip without. A test that needs rules runs under
  `s2::with_rules` (its thread's own), never `s2::set`. What a live S2 tells the server is sent as S2 sends it: the
  shard hello (`s2::Hello`, `Rules::shard_hello`) and UseSkill's record of the pad (`s2::Presses`), each from what
  this client really has: no hardware address, the install id kept as `ii` in the server's folder, the presses its
  own input made (`input::from_device`: a run with an input script says its presses were not the player's).
  Nothing invented to look like a PS2's; a new field S2 adds is the maintainer's call. The July S2's hello is not
  sent (go-eqoa-server reads 19 as its hard-disk loader's 30 worlds).

## Testing and scripted runs

- Speed: `EQOA_PERF=1` (page: `?perf`) prints CPU time by phase; `EQOA_OFFSCREEN=WxH` measures on the real GPU
  without a presentable display (e.g. under Xvfb); `EQOA_QUIT=s`, `EQOA_WALK` script a run. A renderer change is
  held to `shot` PNGs byte for byte (`EQOA_HOUR` pinned; a shot's layers scroll by its script time).
- Live audio tests: `EQOA_AUDIO=0` (the driver runs on the wall clock, silent), or `eqoa listen` to a WAV. In a page
  the driver runs on the game's thread and an AudioWorklet plays what `Audio::feed` mixes (`web/sound.js`); the
  music comes through `bgm.rs` (a BGM read whole before the device may start it, `Disc::ready`/`Link::bgm_ready`,
  always true natively). Headless Chromium: `web/drive.py` passes `--autoplay-policy=no-user-gesture-required`;
  `--js T=JSON.stringify(eqoaAudio.stats())` reads the worklet's counts (dry counts only after the first frames
  came).
- Input (README "the pad", "the mouse and the touch screen"): the pad is the game's own path (`vi::pad` held to
  oracle ops 50-52; `input.rs` libpad on the host, `PadKeys`, `Play::set_key`, `controls.rs` UpdateControls).
  Scripted: `EQOA_PAD` (`start@2500;stick l 0 -1@1500+2000`), `EQOA_MOUSE` (`click @<name>@ms`, `click X Y@ms`,
  `rclick`, `wheel N`, `press l|r`, `release l|r`, `move DX DY`: a drag past the slop takes the pointer as a window
  would, `mouse::Grab`), `EQOA_TOUCH=1`, `EQOA_TOUCHES`, `EQOA_TAP=<name|*>@ms`, `EQOA_HIDE_UI=<ms>[;<ms>]` in a
  playshot; `EQOA_WATCH=<name>` (that creature a frame as drawn, and the player's own 0x40s);
  `drive.py --gamepad/--mobile/--touch/--mouse` in a page; `EQOA_PAD_TRACE=1` / `?padtrace`; `EQOA_GLYPHS=1` prints
  the button glyphs a frame drew. A button pressed and let go between two frames reaches the game (`input::Latch`,
  every source but XInput): a new source keeps its presses through it.
  `eqoa rumble` runs each controller's motors through the game's path (gilrs drives a DualSense's motors);
  `EQOA_RUMBLE_TEST=<ms>` casts a vibrating spell effect on the player in a scripted run.
  A server may spawn a character at a random heading each login, so click by name, not pixel.
  The player's own keys and buttons (`binds.rs`, the options' Controls page) work at the edge only: a moved key
  becomes its row's default key before the keyboard dispatch (never while typing), the controllers' buttons the
  game's fields before `VIPad::Read`. A key added to the dispatch needs a row in `binds::ROWS` (a test reads
  `viewer.rs` for them).
  Multiboxing: each window reads the controllers its Controller Focus picks (`input::pick`, Extended's Controller
  page; per window by its place, `multibox.rs`: lock files in the config directory's `windows/`, Web Locks in a
  page; one place a program, never a thread's: a settings read on another thread took a second; let go at the
  loop's end). Windows' Xbox pads are read through XInput (`xinput.rs`) and its DualShock 4s and DualSenses over HID
  (`hidpad.rs`, which also sets their motors; gilrs's force feedback is off on Windows, and a Mac's PlayStation pads
  by USB get their motors through IOKit), both of which answer a window behind another; gilrs's
  Windows.Gaming.Input answers only the one in front. Test several windows natively with uinput pads
  (python-evdev, `/dev/uinput`) and a shared `EQOA_CONFIG`; Windows on GitHub's runners with ViGEmBus pads.
- A pointer on the windows (`pointer.rs`, `Play::point`): every button glyph is its button (the HUD's raster notes
  them), and each window notes where it drew its rows (`pointer::Hits`) and answers `point(p) -> Vec<i32>`: the
  keys to press, `pointer::MOVED` for a selection it moved itself. Keys go through `Play::press`'s queue, one a
  frame, held through the next HUD frame (`KeyFlash` acts only on a key still down when the window draws). A new
  window gets a `point`, its route in `Play::window_point` (as `window_key` routes keys) and a test through
  `pointer::testing`'s null raster that fails without its hit notes.
- The pregame (`crates/eqoa/src/pregame/`, README "the pregame"): `eqoa` (or `eqoa boot [server]`) runs the game
  from the disc's boot in SLUS's scene order (`boot.rs` the loading pictures, `setup/`, `station/`, `client/` with
  `client/create/`), then hands `Play` the session (`host.rs`); a logout or a lost connection comes back to CLIENT
  (`Pregame::back`). Port from `decomp/modules/{setup,station,client}/src`; compare shots against the real screens
  (`EQOA_NO_SETUP=1` for STATION on). Files go through `pregame::files` and the network through `pregame::link`
  (natively threads; in a page its reader and the relay), so nothing in a stage is platform code. Scripted:
  `EQOA_AUDIO=0 EQOA_OFFSCREEN=WxH EQOA_KEYS='enter@3000; text name@4000' EQOA_PAD=... EQOA_SHOTS='3000=a.png'
  EQOA_QUIT=s eqoa boot <server>` (shots in play's frames too); in a page `web/drive.py --boot --keys/--type`. One
  login at a time per account, and a run that is killed holds its session about a minute: give each run's account
  time, or another account. `eqoa play <server> <account>` still goes straight into the world for scripts and
  playshots.
- The patcher (README "the patcher"): SETUP's, ported (`patcher/`), run after the title's Play into the server's
  own folder (`servers::folder(host)`: `<config>/servers/<host>/`, the card's files flat plus `patch.json`); what
  reads a server's files reads them there (`servers::file`, `servers::words`), never another server's. Test it
  against a patch port of your own (go-eqoa-server's 7000, `EQOA_PATCH_PORT`) with `EQOA_CONFIG` under a scratch
  directory, not against someone else's server.
- The browser build (`web/`, README "In a browser"): `web/build.sh`, then `web/drive.py` (headless Chromium over the
  DevTools protocol, the disc image handed to the page's file input). Platform code goes behind
  `net::session::Datagrams`, `net::auth::Exchange`, `iso::Source`/`iso::Sparse` and `crates/eqoa/src/web/`; std's
  `Instant` panics in a browser, so clocks are `web_time`'s.
- A build is `web/dist/v/<build>/`, never changed under its name (the host serves it immutable); only the page and
  `version.json` are revalidated, and `web/deploy.sh` publishes them in that order, after the build. Every file a
  page loads must be in its build (a new one goes into `build.sh`'s copy list). `web/pagetest.py` checks the page's
  half. The page puts itself full screen at Start (`main.js` "full screen", README "In a browser"): on a touch
  device but iOS's (there the Home Screen app is), on a computer only where Keyboard Lock keeps Escape the game's,
  never while multiboxing; F11 stays the browser's. A full screen asked for uses the click's activation up: ask for
  it after anything else that click needs (asked first, it broke Start with a remembered disc the browser had to
  ask for). It installs as an app (`web/manifest.json`, the icons `tools/webicons` draws off `icon_art.rs`).
- Never log a credential: the account name and password stay out of every log and file the client writes.
