//! The pad the game reads, on the host: libpad over the platform's
//! controllers, `VIConsole`'s pad half and `VIClient::PadKeys`.
//!
//! Each frame [`Console::poll`] runs SUPPORT's `VIPad::Read` (`vi::pad`, held
//! to the decomp) on a libpad whose port 0 is every source at once, OR'd as
//! eqoa-port's `platform/pad.c` ORs them: the gamepads (natively gilrs, SDL's
//! layout; in a page the Gamepad API read straight, `padmap`, which knows
//! each browser's layouts; both positional, A ✕, B ○, X □, Y △, Back Select),
//! the touch screen's pad (`touch`) and a script (`EQOA_PAD`). A pad that is not ready is read as all zeros, as
//! `VIConsole::GetInput` clears it; then `PadKeys`: each of the 20 fields whose
//! zero-ness changed becomes key `128 + i`, down or up, for `SetKey`. The
//! sticks stay for `UpdateControls` (`m_p07190`).
//!
//! A modern stick runs in a circle and a DualShock 2's nearly fills its
//! square, so each axis is scaled by 1.33 and clamped, as PCSX2's DualShock 2
//! is by default ("130% to 140% for recent controllers"): a full diagonal
//! runs instead of walking (`run_threshold` 0.7 on the stick's magnitude).
//! Before that a gamepad's sticks go through the host's own dead zone
//! ([`STICK_DEAD_ZONE`]): a modern stick does not come back to its middle,
//! and scaled by 1.33 its rest would leave `ScaleStick`'s.
//!
//! The motors are `VIPad`'s (`VibrateLf`/`VibrateHf` from `Rumble`, the spell
//! effects' vibrate events; `Process` sends them). The pad is found a read
//! after it is opened, as libpad finds one, so the actuator handshake runs and
//! a controller that can rumble does.

use crate::multibox::Seat;
use crate::settings::Focus;
use vi::fpu::{Ee, F};
use vi::pad::{ConsolePad, Libpad, Pad};

/// libpad's buttons, active-high: bit k of the low byte is `data[2]`'s, of
/// the high byte `data[3]`'s (what `VIPad::Read` inverts and unpacks).
pub mod bit {
    pub const SELECT: u16 = 1 << 0;
    pub const L3: u16 = 1 << 1;
    pub const R3: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const RIGHT: u16 = 1 << 5;
    pub const DOWN: u16 = 1 << 6;
    pub const LEFT: u16 = 1 << 7;
    pub const L2: u16 = 1 << 8;
    pub const R2: u16 = 1 << 9;
    pub const L1: u16 = 1 << 10;
    pub const R1: u16 = 1 << 11;
    pub const TRIANGLE: u16 = 1 << 12;
    pub const CIRCLE: u16 = 1 << 13;
    pub const CROSS: u16 = 1 << 14;
    pub const SQUARE: u16 = 1 << 15;

    /// The name of the first of `b`'s buttons (`named`'s), "?" for none.
    pub fn name(b: u16) -> &'static str {
        const NAMES: [&str; 16] = ["select", "l3", "r3", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"];
        NAMES.get(b.trailing_zeros() as usize).copied().unwrap_or("?")
    }

    /// By name, as eqoa-port's pad scripts spell them.
    pub fn named(s: &str) -> Option<u16> {
        Some(match s {
            "select" => SELECT,
            "l3" => L3,
            "r3" => R3,
            "start" => START,
            "up" => UP,
            "right" => RIGHT,
            "down" => DOWN,
            "left" => LEFT,
            "l2" => L2,
            "r2" => R2,
            "l1" => L1,
            "r1" => R1,
            "triangle" => TRIANGLE,
            "circle" => CIRCLE,
            "cross" => CROSS,
            "square" => SQUARE,
            _ => return None,
        })
    }
}

/// One source's state: the buttons (`bit`) and the sticks rx, ry, lx, ly in
/// -1..1, +y down (libpad's way up).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Report {
    pub buttons: u16,
    pub sticks: [f32; 4],
}

/// A stick axis as a DualShock 2's byte: 0 left or up, 255 right or down,
/// scaled by 1.33 and clamped (see the module's notes).
pub fn stick_byte(v: f32) -> u8 {
    ((v * 1.33).clamp(-1.0, 1.0) * 127.5 + 127.5).round() as u8
}

/// The host's dead zone on a gamepad's stick, radial, on the stick's own
/// travel: inside it the stick is at rest, outside it passes as it is, so
/// the game's own dead zone and response are what they were. `ScaleStick`
/// ignores bytes 88..167, which after the 1.33 is 0.236 of the travel
/// either way of its middle (natively gilrs's dead zone is the device's, an
/// xpad's 0.004; a page reads the Gamepad API's axes as they come, with none;
/// until 2026-10-20 a page read them through gilrs, which scaled them past a
/// radial 0.1 of its own, so this edge was at 0.37 of the travel there).
/// A modern stick does not come back to its middle that well (Microsoft asks
/// a game to ignore up to 0.24 of the left stick's travel and 0.265 of the
/// right's; a worn pad drifts further), and one resting past the game's edge
/// walks the player sideways with nothing held, or turns it slowly (F3
/// reports 20261020-103355 and -104019, Chrome, a PowerA pad). A DualShock 2
/// rests in its middle, and PCSX2 leaves its "Analog Deadzone" to the player
/// (none by default). In a page `?deadzone=<0..0.9>`, natively
/// `EQOA_DEADZONE`; the problem reports carry the sticks as they came.
/// A page's default is 0.37, the edge it had through gilrs (0.1 + 0.3 x
/// 0.9) when those reports were settled: a stick resting at 0.34 there stayed
/// still and would walk at 0.3.
#[cfg(not(target_arch = "wasm32"))]
pub const STICK_DEAD_ZONE: f32 = 0.3;
#[cfg(target_arch = "wasm32")]
pub const STICK_DEAD_ZONE: f32 = 0.37;

/// [`STICK_DEAD_ZONE`], or the player's.
pub fn dead_zone() -> f32 {
    #[cfg(target_arch = "wasm32")]
    let v = crate::web::page_value("deadzone");
    #[cfg(not(target_arch = "wasm32"))]
    let v = std::env::var("EQOA_DEADZONE").ok();
    v.and_then(|v| v.parse::<f32>().ok()).filter(|v| (0.0..=0.9).contains(v)).unwrap_or(STICK_DEAD_ZONE)
}

/// The buttons a controller pressed since a window last read it
/// (`Gamepads::poll`, once a frame drawn). A controller tells its state many
/// times a frame (a DualSense on USB a thousand times a second) and only the
/// last was kept, so a button pressed and let go between two frames, or
/// while a frame came late, never reached the game (F3 20261025-111324: the
/// first press in a window just brought to the front). Every state seen goes
/// through here, and a read gives the last state and each button pressed
/// since the last read that it no longer holds: such a press is down for
/// one read and up the next, a tap a frame long. Only a press counts (a
/// button going down), so one held through the last read and let go since
/// is up at once; one still held is the state's, so nothing is pressed
/// twice; a read clears what it took, so nothing stays down. Two presses
/// between the same two reads are one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Latch {
    /// the last state seen
    last: u16,
    /// the buttons pressed since the last read
    pressed: u16,
}

impl Latch {
    /// A state the controller sent (its buttons, `bit`'s).
    pub fn saw(&mut self, buttons: u16) {
        self.pressed |= buttons & !self.last;
        self.last = buttons;
    }

    /// A read: the buttons pressed since the last read that `now` (the
    /// state read) does not hold; the next read starts afresh.
    pub fn take(&mut self, now: u16) -> u16 {
        std::mem::take(&mut self.pressed) & !now
    }
}

impl Report {
    /// Each stick inside the radial dead zone `dz` put at rest.
    pub fn rest(&mut self, dz: f32) {
        for k in [0, 2] {
            let (x, y) = (self.sticks[k], self.sticks[k + 1]);
            if x * x + y * y < dz * dz {
                self.sticks[k] = 0.0;
                self.sticks[k + 1] = 0.0;
            }
        }
    }

    /// Another source on the same port: the buttons OR'd, each stick axis
    /// the one further from rest.
    pub fn merge(&mut self, o: &Report) {
        self.buttons |= o.buttons;
        for (s, v) in self.sticks.iter_mut().zip(o.sticks) {
            if v * v > *s * *s {
                *s = v;
            }
        }
    }

    /// `scePadRead`'s 32 bytes: success, analog mode, the buttons
    /// active-low, the sticks.
    pub fn data(&self) -> [u8; 32] {
        let mut d = [0u8; 32];
        d[1] = 0x73;
        d[2] = !(self.buttons as u8);
        d[3] = !((self.buttons >> 8) as u8);
        for i in 0..4 {
            d[4 + i] = stick_byte(self.sticks[i]);
        }
        d
    }
}

/// Whether gilrs drives motors (its force feedback). Not on Windows: there an
/// Xbox controller's are XInput's (`xinput`) and a PlayStation one's are set
/// over HID (`hidpad`), and gilrs's force feedback tells every controller it
/// has as a Windows.Gaming.Input gamepad its motors every 50 ms, at rest when
/// it plays nothing: a DualShock 4 was told "motors off" (and its light off)
/// twenty times a second, which stopped what HID had just set going (a
/// virtual DualShock 4 on GitHub's Windows machines read the reports back).
pub const GILRS_FF: bool = cfg!(not(windows));

/// libpad on the host: port 0, slot 0 answers from the merged report;
/// the motors set last are kept for the platform to play.
#[derive(Default)]
pub struct HostLibpad {
    pub report: Report,
    /// a source is there (else the pad reads as disconnected)
    pub connected: bool,
    /// it can rumble (the handshake finds two actuators)
    pub motors: bool,
    /// a read since the port opened (the first finds the pad)
    found: bool,
    /// `scePadSetActDirect`'s last motors: the small one, the large one's level
    pub act: [u8; 2],
    pub act_changed: bool,
}

impl Libpad for HostLibpad {
    fn port_open(&mut self, port: i32, slot: i32) -> i32 {
        self.found = false;
        (port == 0 && slot == 0) as i32
    }
    fn port_close(&mut self, _: i32, _: i32) -> i32 {
        1
    }
    fn get_req_state(&mut self, _: i32, _: i32) -> i32 {
        0
    }
    fn get_state(&mut self, port: i32, slot: i32) -> i32 {
        if port != 0 || slot != 0 || !self.connected {
            return 0;
        }
        // FINDPAD on the first read, as libpad answers right after it opens
        // a port; STABLE after
        if !std::mem::replace(&mut self.found, true) { 1 } else { 6 }
    }
    fn info_mode(&mut self, _: i32, _: i32, term: i32, _: i32) -> i32 {
        if term == 1 { 7 } else { 0 }
    }
    fn info_act(&mut self, _: i32, _: i32, actno: i32, _: i32) -> i32 {
        match actno {
            -1 => 2 * self.motors as i32,
            0 => 0,
            _ => 1,
        }
    }
    fn set_act_align(&mut self, _: i32, _: i32, _: &[u8; 6]) -> i32 {
        1
    }
    fn set_act_direct(&mut self, _: i32, _: i32, act: &[u8; 6]) -> i32 {
        self.act = [act[0], act[1]];
        self.act_changed = true;
        1
    }
    fn set_main_mode(&mut self, _: i32, _: i32, _: i32, _: i32) -> i32 {
        1
    }
    fn read(&mut self, _: i32, _: i32, data: &mut [u8; 32]) -> i32 {
        *data = self.report.data();
        32
    }
}

/// `VIConsole`'s pad and what `VIClient` keeps of it.
pub struct Console {
    pub pad: Pad,
    pub lib: HostLibpad,
    /// `m_p07190`: the last `VIConsolePad` handed to `PadKeys`
    pub state: ConsolePad<Ee>,
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

impl Console {
    /// `VIConsole::Init`'s `m_pad.Init(0, 0)`.
    pub fn new() -> Self {
        let mut c = Console { pad: Pad::default(), lib: HostLibpad::default(), state: ConsolePad::default() };
        c.pad.init(&mut c.lib, 0, 0);
        c
    }

    /// One frame's pad: `report` from every source (`connected` when there
    /// is one, `motors` when it can rumble), `VIPad::Read` (all zeros when
    /// it is not ready, as `GetInput` clears it), then `PadKeys`: the key
    /// codes whose field changed, in field order. `frozen` is `m_pC1B0`
    /// (the player dead): `PadKeys` does nothing.
    pub fn poll(&mut self, report: Report, connected: bool, motors: bool, frozen: bool) -> Vec<(i32, bool)> {
        self.lib.report = report;
        self.lib.connected = connected;
        self.lib.motors = motors;
        let mut p = self.state;
        if self.pad.read(&mut self.lib, &mut p) == 0 {
            p.keys = [F::ZERO; 20];
        }
        if frozen {
            return Vec::new();
        }
        pad_keys(&mut self.state, &p)
    }

    /// `VIPad::Process`: the motors whose time is up stopped. The motors to
    /// play when they changed: (small on, large level), and how long the
    /// longer has left (ms).
    pub fn process(&mut self, now: i64) -> Option<([u8; 2], i64)> {
        self.pad.process(now, &mut self.lib);
        if !std::mem::take(&mut self.lib.act_changed) {
            return None;
        }
        let left = |end: i64| if end != 0 { end - now } else { 0 };
        Some((self.lib.act, left(self.pad.lf_end).max(left(self.pad.hf_end))))
    }

    /// `VIPad`'s clock set to `now` before the frame's `Rumble`s: the game
    /// times a vibration from the last `Process`, the frame before, and a
    /// frame that came later than a vibration lasts (the world loading, a
    /// hitch; a PS2's frames never do) ended it before it began.
    pub fn now(&mut self, now: i64) {
        self.pad.time = now;
    }

    /// `Rumble` (CLIENT): motor 0 the small one for `ms`, 1 the large one at
    /// `level`; only with feature 0x200 (the Frontiers build has it) and the
    /// options' Vibration on.
    pub fn rumble(&mut self, motor: u8, level: u8, ms: i32, vibration: bool) {
        if !vibration {
            return;
        }
        match motor {
            0 => {
                self.pad.vibrate_hf(ms);
            }
            1 => {
                self.pad.vibrate_lf(level, ms);
            }
            _ => {}
        }
    }

    /// `m_p07190.m_keys[0..4]`: rx, ry, lx, ly as `ScaleStick` left them.
    pub fn sticks(&self) -> [f32; 4] {
        [self.state.keys[0].0, self.state.keys[1].0, self.state.keys[2].0, self.state.keys[3].0]
    }

    /// `m_console.m_pad.m_mode`: 1 while a DualShock is being read.
    pub fn mode(&self) -> i32 {
        self.pad.mode
    }
}

/// A pad key's name (`128 + i`, i a `VIConsolePad` field), for
/// `EQOA_PAD_TRACE`.
pub fn key_name(code: i32) -> &'static str {
    const NAMES: [&str; 20] = ["rx", "ry", "lx", "ly", "triangle", "square", "cross", "circle", "left", "right", "up", "down", "l1", "l2", "r1", "r2", "start", "select", "l3", "r3"];
    usize::try_from(code - 128).ok().and_then(|i| NAMES.get(i)).copied().unwrap_or("?")
}

/// `VIClient::PadKeys`: each field whose zero-ness changed becomes
/// `SetKey(128 + i, down)`, then the new pad is kept.
pub fn pad_keys(old: &mut ConsolePad<Ee>, new: &ConsolePad<Ee>) -> Vec<(i32, bool)> {
    let mut out = Vec::new();
    for i in 0..20 {
        let (was, is) = (old.keys[i].0 != 0.0, new.keys[i].0 != 0.0);
        if was != is {
            out.push((128 + i as i32, is));
        }
    }
    *old = *new;
    out
}

/// `EQOA_PAD`: a pad played by script, for headless runs and the window
/// alike. `;`-separated steps, each at a time in ms (16 ms frames round it):
/// `<button>@<ms>[+<hold>]` presses a button (eqoa-port's names: cross,
/// circle, square, triangle, up, down, left, right, l1, l2, r1, r2, start,
/// select, l3, r3) and holds it 100 ms or `hold`; `stick <l|r> <x> <y>@<ms>+<hold>`
/// holds a stick (-1..1, +y down).
#[derive(Clone, Debug, Default)]
pub struct Script {
    steps: Vec<(i64, i64, Report)>,
}

impl Script {
    pub fn from_env() -> Option<Script> {
        std::env::var("EQOA_PAD").ok().map(|s| Script::parse(&s))
    }

    pub fn parse(s: &str) -> Script {
        let mut steps = Vec::new();
        for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            let Some((what, when)) = part.rsplit_once('@') else { continue };
            let (at, hold) = match when.split_once('+') {
                Some((a, h)) => (a.trim().parse().ok(), h.trim().parse().ok()),
                None => (when.trim().parse().ok(), Some(100)),
            };
            let (Some(at), Some(hold)) = (at, hold) else { continue };
            let mut r = Report::default();
            let w: Vec<&str> = what.split_whitespace().collect();
            match w.as_slice() {
                ["stick", side, x, y] => {
                    let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) else { continue };
                    let k = if *side == "r" { 0 } else { 2 };
                    r.sticks[k] = x;
                    r.sticks[k + 1] = y;
                }
                [name] => match bit::named(name) {
                    Some(b) => r.buttons = b,
                    None => continue,
                },
                _ => continue,
            }
            steps.push((at, at + hold, r));
        }
        Script { steps }
    }

    /// The pad at `t` ms.
    pub fn at(&self, t: i64) -> Report {
        let mut r = Report::default();
        for (a, b, s) in &self.steps {
            if (*a..*b).contains(&t) {
                r.merge(s);
            }
        }
        r
    }
}

/// The scripts that press the game's keys in a run of the client's own.
const SCRIPTS: [&str; 17] = [
    "EQOA_PAD",
    "EQOA_KEYS",
    "EQOA_PLAY_KEYS",
    "EQOA_MOUSE",
    "EQOA_TAP",
    "EQOA_TOUCHES",
    "EQOA_HUD_KEYS",
    "EQOA_START_KEYS",
    "EQOA_BOX_KEYS",
    "EQOA_ACTION",
    "EQOA_GROUP_ACTION",
    "EQOA_TARGET",
    "EQOA_WALK",
    "EQOA_ATTACK",
    "EQOA_INTERACT",
    "EQOA_INVITE",
    "EQOA_ACCEPT",
];

/// Whether this run's presses are the player's own (a controller, the
/// keyboard, the mouse, the touch screen): not in a run any script presses
/// keys in, whose presses S2's record of the pad then says its own pad read
/// did not see (`s2::Presses`).
pub fn from_device() -> bool {
    static SCRIPTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    !*SCRIPTED.get_or_init(|| SCRIPTS.iter().any(|v| std::env::var_os(v).is_some()))
}

/// The spell effect `EQOA_RUMBLE_TEST` casts: Fiery Burst's, the visual
/// go-eqoa-server gives a harmful spell it has no visual for, whose caster's
/// list runs the large motor at 127 for 319 ms (and its target's at 220 for
/// 140 ms, then the small one for 1979 ms).
pub const RUMBLE_TEST_EFFECT: u32 = 0xE780_24D9;

/// `EQOA_RUMBLE_TEST=<ms>[,<effect>]` (a test's): that long after the
/// player's first frame in the world, the player casts that spell effect
/// (`RUMBLE_TEST_EFFECT` by default) on itself, once: the pad's motors
/// tested from a spell's vibrate events (`Rumble`, `VIPad`) to the
/// controller. `caster` is the player's id.
pub fn rumble_test(caster: u32) -> Option<crate::spells::Cast> {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};
    static SPEC: OnceLock<Option<(u128, u32)>> = OnceLock::new();
    static FIRST: OnceLock<web_time::Instant> = OnceLock::new();
    static CAST: AtomicBool = AtomicBool::new(false);
    let (at, effect) = (*SPEC.get_or_init(|| {
        let v = std::env::var("EQOA_RUMBLE_TEST").ok()?;
        let mut w = v.split(',');
        let at = w.next()?.trim().parse().ok()?;
        let effect = w.next().and_then(|e| u32::from_str_radix(e.trim().trim_start_matches("0x"), 16).ok()).unwrap_or(RUMBLE_TEST_EFFECT);
        Some((at, effect))
    }))?;
    let first = *FIRST.get_or_init(web_time::Instant::now);
    if first.elapsed().as_millis() < at || CAST.swap(true, Ordering::Relaxed) {
        return None;
    }
    crate::say(&format!("EQOA_RUMBLE_TEST: the player casts spell effect {effect:#010x}"));
    Some(crate::spells::Cast { effect, time: 0, caster, target: caster, targets: Vec::new() })
}

/// `eqoa rumble`: the controllers this window would read rumble as the game
/// rumbles them, the options' Vibration aside: each read a frame at a time
/// until `VIPad`'s handshake finds their motors (at most `wait_ms`), then the
/// large motor at 200 for 600 ms, a rest, the small one for 600 ms, each
/// through `Rumble`, `VIPad::Process` and the controller's own way
/// (`Gamepads::rumble`), said as it goes. An error when no controller came or
/// none that came can rumble.
#[cfg(not(target_arch = "wasm32"))]
pub fn check_motors(wait_ms: i64) -> Result<(), String> {
    let mut pads = Gamepads::new();
    let mut console = Console::new();
    let start = std::time::Instant::now();
    let (mut step, mut at, mut seen) = (0, 0i64, false);
    loop {
        let now = start.elapsed().as_millis() as i64;
        let (report, connected, motors) = pads.poll();
        seen |= connected;
        console.poll(report, connected, motors, false);
        console.now(now);
        match step {
            0 if console.pad.active() => {
                crate::say("rumble: the large motor at 200 for 600 ms");
                console.rumble(1, 200, 600, true);
                (step, at) = (1, now);
            }
            0 if now >= wait_ms => {
                return Err(if seen { "rumble: the controllers read cannot rumble here".into() } else { "rumble: no controller".into() });
            }
            1 if now - at >= 900 => {
                crate::say("rumble: the small motor for 600 ms");
                console.rumble(0, 0, 600, true);
                (step, at) = (2, now);
            }
            2 if now - at >= 900 => break,
            _ => {}
        }
        if let Some((act, ms)) = console.process(now) {
            crate::say(&format!("rumble: motors small {} large {}, for {ms} ms", act[0], act[1]));
            pads.rumble(act, ms);
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    crate::say("rumble: done");
    Ok(())
}

/// `EQOA_PAD_TRACE=1` (in a page `?padtrace`): each pad key as the game
/// gets it, and gilrs's events and the controllers' report as they change.
pub fn trace() -> bool {
    #[cfg(target_arch = "wasm32")]
    return crate::web::page_flag("padtrace");
    #[cfg(not(target_arch = "wasm32"))]
    return std::env::var_os("EQOA_PAD_TRACE").is_some();
}

/// A connected controller as a window reads it: its name among the client's
/// windows (`multibox`: natively a Linux device node, `xinput<i>` for one of
/// Windows' Xbox controllers, `hid-<ids>-<hash>` for one of its PlayStation
/// controllers (`hidpad::name`), else `pad<k>` by gilrs's slot; in a page
/// `pad<k>` by the Gamepad API's index), what it holds, whether it rumbles,
/// and what it calls itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Controller {
    pub name: String,
    pub report: Report,
    /// the buttons it pressed and let go since the last read, which
    /// `report` (its state now) no longer holds (`Latch`)
    pub between: u16,
    /// its sticks as they came, before the dead zone (for a report)
    pub raw: [f32; 4],
    pub rumbles: bool,
    pub label: String,
    source: Source,
    /// its place in the controllers' order: the first controller first
    order: (u8, u64),
    /// what a problem report says of it beyond its line: a controller read
    /// over HID, the first report of each ID it sent (`hidpad::FirstReport`)
    notes: Vec<String>,
}

/// Where a controller is read, and its motors driven.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    #[cfg(not(target_arch = "wasm32"))]
    Gilrs(gilrs::GamepadId),
    #[cfg(windows)]
    XInput(u32),
    /// a PlayStation controller read over HID (`hidpad`), by its place among
    /// them, its motors set there too; and gilrs's reading of it, if gilrs
    /// has one
    #[cfg(windows)]
    Hid(u32, Option<gilrs::GamepadId>),
    #[cfg(target_arch = "wasm32")]
    Page(u32),
}

impl Source {
    /// How a controller is read, as a report says it.
    fn how(&self) -> &'static str {
        match self {
            #[cfg(windows)]
            Source::Gilrs(_) => "Windows.Gaming.Input: only by the window in front",
            #[cfg(target_os = "macos")]
            Source::Gilrs(_) => "IOKit",
            #[cfg(all(not(windows), not(target_os = "macos"), not(target_arch = "wasm32")))]
            Source::Gilrs(_) => "evdev",
            #[cfg(windows)]
            Source::XInput(_) => "XInput: in front or behind",
            #[cfg(windows)]
            Source::Hid(..) => "HID: in front or behind",
            #[cfg(target_arch = "wasm32")]
            Source::Page(_) => "the Gamepad API",
        }
    }
}

/// What a window reads of the connected controllers (`pick`).
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    /// every one, OR'd on the one port
    All,
    /// that one alone, by its name
    One(String),
    /// none while some are connected: the pad at rest, not unplugged
    /// (Window's, another window in front)
    Idle,
    /// none: the pad unplugged, as the game sees it
    Nothing,
}

/// Which controllers a window reads (the Extended page's Controller Focus):
/// `names` the connected ones in their order (the first controller first),
/// `in_front` whether this window has the focus, `seat` it among the
/// client's other windows open, `bound` the one it reads by Auto (or waits
/// for: `Gamepads` lets go of one gone `WAIT_MS`).
///
/// Auto alone reads every one, as the client always did (on Windows while it
/// is in front, as Windows.Gaming.Input gave them: `Gamepads::poll`). Among
/// other windows each window's controller is worked out as every one of them
/// works it out: a Pad's controller is that window's; then each Auto window, in
/// place order, keeps the one it reads unless a window before it has it, and
/// waits for it while it is gone (a controller off a moment, its battery or
/// its radio, comes back to its own window, and no other moves); then each
/// that has none takes the controller as far down the order as it is down the
/// windows open (the first window the first controller, the second the next),
/// or else the first nobody reads. A Pad is the controller at that place in
/// the order, whatever else reads it. But an Auto window reads none (at
/// rest) while a window on Window is in front, which reads them all: two
/// windows set apart (one Window, one Auto) both took the one controller
/// while the Window one was in front, as the Steam Deck player's "both in
/// focus" may have been. `in_front` is this window's as `Gamepads::poll`
/// works it out: then no other window is in front, else the one that came
/// last (`Seat::front_other`).
pub fn pick(focus: Focus, in_front: bool, names: &[&str], seat: &Seat, bound: Option<&str>) -> Pick {
    match focus {
        Focus::Off => Pick::Nothing,
        Focus::Window if in_front => Pick::All,
        Focus::Window if names.is_empty() => Pick::Nothing,
        Focus::Window => Pick::Idle,
        Focus::Pad(n) => usize::from(n).checked_sub(1).and_then(|k| names.get(k)).map_or(Pick::Nothing, |n| Pick::One(n.to_string())),
        Focus::Auto if seat.alone() => Pick::All,
        Focus::Auto if !in_front && seat.front_other(None).is_some_and(|o| o.focus == Focus::Window) => {
            if names.is_empty() {
                Pick::Nothing
            } else {
                Pick::Idle
            }
        }
        Focus::Auto => {
            let mut taken: Vec<&str> = seat.others.iter().filter(|o| matches!(o.focus, Focus::Pad(_))).filter_map(|o| o.pad.as_deref()).collect();
            // the Auto windows in place order, this one among them, with
            // what each reads
            let mut autos: Vec<(usize, Option<&str>)> = seat.others.iter().filter(|o| o.focus == Focus::Auto).map(|o| (o.place, o.pad.as_deref())).collect();
            autos.push((seat.place, bound));
            autos.sort_by_key(|a| a.0);
            let mut got: Vec<Option<&str>> = vec![None; autos.len()];
            let mut waits = vec![false; autos.len()];
            for (k, &(_, b)) in autos.iter().enumerate() {
                match b {
                    Some(b) if !names.contains(&b) => waits[k] = true,
                    Some(b) if !taken.contains(&b) => {
                        taken.push(b);
                        got[k] = Some(b);
                    }
                    _ => {}
                }
            }
            for (k, &(place, _)) in autos.iter().enumerate() {
                if got[k].is_some() || waits[k] {
                    continue;
                }
                let rank = seat.others.iter().filter(|o| o.place < place).count() + usize::from(seat.place < place);
                let free = |n: &&&str| !taken.contains(*n);
                if let Some(&n) = names.get(rank).filter(free).or_else(|| names.iter().find(free)) {
                    taken.push(n);
                    got[k] = Some(n);
                }
            }
            let me = autos.iter().position(|a| a.0 == seat.place).unwrap_or(0);
            got[me].map_or(Pick::Nothing, |n| Pick::One(n.to_string()))
        }
    }
}

/// Whether a window is the one in front: `own` whether its system says it
/// has the focus, `since` since when (as it told the other windows), `seat`
/// the others as they told theirs. Another window that came to the front
/// after it is in front instead: a window is told when it loses the focus,
/// but a system can leave that out, and two windows both said they had it.
pub fn ahead(own: bool, since: Option<u64>, seat: &Seat) -> bool {
    own && seat.front_other(since).is_none()
}

/// Capture Controller (the Extended page, one setting for every window of
/// the client): on, a Pad that another window open also has as its
/// Controller Focus is read only by the window in front; a window behind
/// reads it at rest (`Idle`) until it is in front, so a press reaches the one
/// window it was meant for, and with none of them in front none reads it.
/// The rule is this window's own test of the focus, so it holds where the
/// system hands every window the controller (XInput answers a window behind
/// another). Off, on a Pad no other window has, and for Auto, Window and
/// Off: what `pick` gave.
pub fn captured(p: Pick, focus: Focus, in_front: bool, seat: &Seat, capture: bool) -> Pick {
    if capture && !in_front && shares_pad(focus, seat) && matches!(p, Pick::One(_)) { Pick::Idle } else { p }
}

/// Whether this window's Controller Focus is a Pad another window open has
/// as its own (a controller shared by several windows), or another window is
/// on Window (which reads every controller while it is in front: a Pad
/// window beside it took the controller with it there).
fn shares_pad(focus: Focus, seat: &Seat) -> bool {
    matches!(focus, Focus::Pad(_)) && seat.others.iter().any(|o| o.focus == focus || o.focus == Focus::Window)
}

/// How long a window waits for its controller to come back before it takes
/// another (`pick`): a wireless one waking takes a few seconds.
const WAIT_MS: u128 = 15_000;

/// How long gilrs's reading of a PlayStation controller is passed over while
/// HID has not read it (Windows: `hidpad` looks once a second).
#[cfg(any(windows, test))]
const HID_GRACE_MS: u128 = 2_000;

/// Whether gilrs's reading of a PlayStation controller that HID does not read
/// is passed over (Windows), `ms` after gilrs first had it: for a moment, in
/// case HID is about to (`HID_GRACE_MS`), but not when HID could not open it
/// (`hidpad::Hid::refused`): then it is gilrs's.
#[cfg(any(windows, test))]
fn wait_for_hid(refused: bool, ms: u128) -> bool {
    !refused && ms < HID_GRACE_MS
}

/// The controllers: natively through gilrs (SDL's layout) and, on Windows,
/// the Xbox controllers through XInput (`xinput`) and the PlayStation ones
/// over HID (`hidpad`), which every window reads, in front or not; in a page
/// straight from the Gamepad API (`padmap`: every button and axis, and the
/// layout each browser gives). The ones this window reads (`pick`) as one
/// report.
pub struct Gamepads {
    #[cfg(not(target_arch = "wasm32"))]
    gilrs: Option<gilrs::Gilrs>,
    #[cfg(not(target_arch = "wasm32"))]
    effect: Option<gilrs::ff::Effect>,
    #[cfg(windows)]
    xinput: Option<crate::xinput::XInput>,
    #[cfg(windows)]
    hid: Option<crate::hidpad::Hid>,
    /// a Mac's PlayStation controllers' motors (`hidpad::mac`), from the
    /// first time they are to run
    #[cfg(target_os = "macos")]
    mac: Option<crate::hidpad::mac::Mac>,
    /// gilrs's PlayStation controllers not (yet) read over HID, each since
    /// when gilrs has it (`read`)
    #[cfg(windows)]
    unmatched: std::collections::HashMap<gilrs::GamepadId, web_time::Instant>,
    /// each pad's reading, by its index and id (in a page)
    #[cfg(target_arch = "wasm32")]
    web: Vec<(u32, String, crate::padmap::PadMap)>,
    #[cfg(target_arch = "wasm32")]
    browser: crate::padmap::Browser,
    /// what `?padtrace` last said of each pad (in a page)
    #[cfg(target_arch = "wasm32")]
    traced: Vec<String>,
    /// the names of the pads seen, to say so once
    seen: Vec<String>,
    trace: bool,
    last: Report,
    /// [`dead_zone`]
    pub dead_zone: f32,
    /// natively, whether this window has the focus (as it opened, then the
    /// window's `Focused`); a page asks its document
    pub focused: bool,
    /// since when it has had the focus (`multibox::now_ms`), as the other
    /// windows are told; none without it
    front: Option<u64>,
    /// a press it had the focus for and read no controller said, since it
    /// last came to the front (`poll`)
    unread_said: bool,
    /// the controller this window reads by Auto among other windows, or
    /// waits for, gone since when
    bound: Option<String>,
    gone: Option<web_time::Instant>,
    /// what it reads, as last said
    picked: Option<Pick>,
    /// where the motors of the controllers it reads are
    rumble_to: Vec<Source>,
    /// the sticks of the controllers it reads as they came, before the dead
    /// zone (for a report)
    pub raw: [f32; 4],
    /// the buttons the last read gave that no controller it reads held any
    /// more: pressed and let go between two reads (`Latch`)
    pub between: u16,
    /// each gilrs controller's buttons since the last read, from its events
    /// (gilrs keeps only the last state)
    #[cfg(not(target_arch = "wasm32"))]
    gilrs_seen: std::collections::HashMap<gilrs::GamepadId, Latch>,
    /// a gilrs fault stepped over has been said (the log says it once)
    #[cfg(not(target_arch = "wasm32"))]
    gilrs_fault_said: bool,
    /// each pad as a report gives it: its id, mapping, layout, every axis
    /// and every button pressed (in a page)
    #[cfg(target_arch = "wasm32")]
    pub described: Vec<String>,
}

impl Default for Gamepads {
    fn default() -> Self {
        Self::new()
    }
}

impl Gamepads {
    pub fn new() -> Self {
        // `EQOA_GAMEPAD=0`: none (a script or the touch pad only)
        #[cfg(not(target_arch = "wasm32"))]
        let none = std::env::var("EQOA_GAMEPAD").is_ok_and(|v| v == "0");
        #[cfg(not(target_arch = "wasm32"))]
        let gilrs = if none {
            None
        } else {
            match gilrs::GilrsBuilder::new().with_force_feedback(GILRS_FF).build() {
                Ok(g) => Some(g),
                Err(e) => {
                    crate::say(&format!("no gamepads: {e}"));
                    None
                }
            }
        };
        Gamepads {
            #[cfg(not(target_arch = "wasm32"))]
            gilrs,
            #[cfg(not(target_arch = "wasm32"))]
            effect: None,
            #[cfg(windows)]
            xinput: (!none).then(crate::xinput::XInput::default),
            #[cfg(windows)]
            hid: (!none).then(crate::hidpad::Hid::start),
            #[cfg(target_os = "macos")]
            mac: None,
            #[cfg(windows)]
            unmatched: std::collections::HashMap::new(),
            #[cfg(target_arch = "wasm32")]
            web: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            browser: crate::web::pad::browser(),
            #[cfg(target_arch = "wasm32")]
            traced: Vec::new(),
            seen: Vec::new(),
            trace: trace(),
            last: Report::default(),
            dead_zone: dead_zone(),
            focused: true,
            front: None,
            unread_said: false,
            bound: None,
            gone: None,
            picked: None,
            rumble_to: Vec::new(),
            raw: [0.0; 4],
            between: 0,
            #[cfg(not(target_arch = "wasm32"))]
            gilrs_seen: std::collections::HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            gilrs_fault_said: false,
            #[cfg(target_arch = "wasm32")]
            described: Vec::new(),
        }
    }

    /// The controllers this window reads (`pick`, by its Controller Focus
    /// and the client's other windows open) as one report; whether it reads
    /// one (else the pad reads as unplugged), and whether one can rumble.
    pub fn poll(&mut self) -> (Report, bool, bool) {
        let pads = self.read();
        let focus = crate::settings::focus();
        let seat = crate::multibox::seat();
        let names: Vec<&str> = pads.iter().map(|c| c.name.as_str()).collect();
        // a controller gone this long is let go: the window takes another
        let now = web_time::Instant::now();
        match &self.bound {
            Some(b) if !names.contains(&b.as_str()) => {
                if self.gone.get_or_insert(now).elapsed().as_millis() >= WAIT_MS {
                    crate::say(&format!("controllers: {b} gone {} s: let go", WAIT_MS / 1000));
                    self.bound = None;
                    self.gone = None;
                }
            }
            _ => self.gone = None,
        }
        let capture = crate::settings::capture();
        // since when this window has had the focus, for the other windows; in
        // front unless another came to the front after it did
        // (`Seat::front_other`: a focus lost without a word)
        let own = self.has_focus();
        if own != self.front.is_some() {
            self.front = own.then(crate::multibox::now_ms);
        }
        let front = ahead(own, self.front, &seat);
        let pick = captured(pick(focus, front, &names, &seat, self.bound.as_deref()), focus, front, &seat, capture);
        // Windows gave a window its controllers only while it was in front
        // (Windows.Gaming.Input), so a window alone keeps that there: XInput,
        // which reads them behind, is for a window among others
        #[cfg(windows)]
        let pick = if pick == Pick::All && focus == Focus::Auto && !front { Pick::Idle } else { pick };
        // a controller pressed while this window has the focus and reads
        // none, said once a time it comes to the front: the presses a
        // player saw "ignored" (a second box, 2026-10-25) held back by these
        // rules, and why; a press that never came from the controller says
        // nothing here (the arrival line's first press is late instead)
        if !own {
            self.unread_said = false;
        } else if !self.unread_said && matches!(pick, Pick::Idle | Pick::Nothing) {
            let pressed = pads.iter().fold(0, |b, c| b | c.report.buttons | c.between);
            if pressed != 0 {
                let why = match seat.front_other(self.front) {
                    Some(o) => format!("window {} came to the front after this one", o.place + 1),
                    None => describe(&pick, &pads, &seat, focus, capture && shares_pad(focus, &seat)).replacen("controllers: ", "", 1),
                };
                crate::say(&format!("controllers: the controller's {} pressed while this window has the focus, not read: {why}", bit::name(pressed)));
                self.unread_said = true;
            }
        }
        let one = match &pick {
            Pick::One(n) => Some(n.clone()),
            _ => None,
        };
        let waiting = self.bound.as_ref().is_some_and(|b| !names.contains(&b.as_str()));
        // an Auto window at rest while a window on Window is in front keeps
        // the controller it reads, and says so: no other Auto window takes it
        // meanwhile, and it reads it again after
        let resting = focus == Focus::Auto && !seat.alone() && pick == Pick::Idle;
        if !resting && (focus != Focus::Auto || seat.alone() || !waiting) {
            self.bound = one.clone().filter(|_| focus == Focus::Auto);
        }
        // what it reads (by Auto, or waits for), and since when it has had
        // the focus, for the other windows
        crate::multibox::say(if focus == Focus::Auto { self.bound.as_deref() } else { one.as_deref() }, focus, self.front);
        let says = reads_text(&pick, &names, &seat, capture && shares_pad(focus, &seat));
        READS.with(|r| {
            if *r.borrow() != says {
                *r.borrow_mut() = says;
            }
        });
        let listed = list_controllers(
            &describe(&pick, &pads, &seat, focus, capture && shares_pad(focus, &seat)),
            &pads.iter().map(|c| (c.name.as_str(), c.label.as_str(), c.source.how(), c.rumbles, c.notes.as_slice())).collect::<Vec<_>>(),
            &pick,
            &seat,
        );
        if let Ok(mut l) = LISTED.lock()
            && *l != listed
        {
            *l = listed;
        }
        if self.picked.as_ref() != Some(&pick) {
            crate::say(&describe(&pick, &pads, &seat, focus, capture && shares_pad(focus, &seat)));
            // a controller this window no longer reads is not its to shake
            self.stop_motors();
            self.picked = Some(pick.clone());
        }
        let mut out = Report::default();
        let mut raw = Report::default();
        let (mut any, mut motors) = (false, false);
        let mut between = 0;
        self.rumble_to.clear();
        for c in pads.iter().filter(|c| match &pick {
            Pick::All => true,
            Pick::One(n) => *n == c.name,
            Pick::Idle | Pick::Nothing => false,
        }) {
            out.merge(&c.report);
            between |= c.between;
            raw.merge(&Report { buttons: 0, sticks: c.raw });
            any = true;
            motors |= c.rumbles;
            if c.rumbles {
                self.rumble_to.push(c.source);
            }
        }
        if pick == Pick::Idle {
            // still there, at rest: as it was found, motors and all
            any = true;
            motors = pads.iter().any(|c| c.rumbles);
        }
        self.raw = raw.sticks;
        // what was pressed and let go since the last read (`Latch`): down
        // for this read, as a tap a frame long
        self.between = between & !out.buttons;
        out.buttons |= self.between;
        if self.trace && self.between != 0 {
            crate::say(&format!("[pad] pressed and let go since the last frame: {:#06x}", self.between));
        }
        if self.trace && out != self.last {
            crate::say(&format!("[pad] report {out:?}"));
            self.last = out;
        }
        (out, any, motors)
    }

    /// Whether this window is the one in front: it has the focus, and no
    /// other window of the client came to the front after it did
    /// (`multibox::Seat::front_other`).
    pub fn in_front(&self) -> bool {
        ahead(self.has_focus(), self.front, &crate::multibox::seat())
    }

    /// Whether this window has the focus, as its system says.
    fn has_focus(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        return crate::web::has_focus();
        #[cfg(not(target_arch = "wasm32"))]
        return self.focused;
    }

    /// Every connected controller, in their order: on Windows the Xbox ones
    /// by user index, then the PlayStation ones read over HID (by their
    /// interfaces' order, the same in every window), then gilrs's; on Linux
    /// gilrs's by device node, the order the system made them in; elsewhere
    /// by gilrs's slot; each through the host's dead zone.
    #[cfg(not(target_arch = "wasm32"))]
    fn read(&mut self) -> Vec<Controller> {
        let mut out = Vec::new();
        // the XInput pads' USB ids, each to pass over one of gilrs's
        #[cfg(windows)]
        let mut xinput_ids: Vec<Option<(u16, u16)>> = Vec::new();
        #[cfg(windows)]
        if let Some(x) = self.xinput.as_mut() {
            for crate::xinput::Found { index: i, pad: p, ids } in x.read() {
                xinput_ids.push(ids);
                let mut report = p.report();
                let raw = report.sticks;
                report.rest(self.dead_zone);
                let name = format!("xinput{i}");
                if !self.seen.contains(&name) {
                    crate::say(&format!("gamepad: an Xbox controller, XInput's player {} (rumble)", i + 1));
                    self.seen.push(name.clone());
                }
                // XInput says the state at the moment asked, no more (its packet
                // number says only that something changed): a press let go
                // between two frames is not seen
                out.push(Controller { name, report, between: 0, raw, rumbles: true, label: format!("Xbox controller {}", i + 1), source: Source::XInput(i), order: (0, i as u64), notes: Vec::new() });
            }
        }
        // the PlayStation controllers read over HID, each with its USB ids:
        // gilrs's reading of the same one is passed over; their motors are
        // set over HID too (`hidpad::Hid::rumble`)
        #[cfg(windows)]
        let mut hid: Vec<(Controller, (u16, u16))> = Vec::new();
        #[cfg(windows)]
        if let Some(h) = self.hid.as_mut() {
            for (k, f) in h.read().into_iter().enumerate() {
                let mut report = f.pad.report();
                let raw = report.sticks;
                report.rest(self.dead_zone);
                if !self.seen.contains(&f.name) {
                    crate::say(&format!("gamepad: a {}{}, read over HID ({})", f.model.label(), if f.bluetooth { " on Bluetooth" } else { "" }, f.name));
                    self.seen.push(f.name.clone());
                }
                let notes = f.reports.iter().map(|r| format!("{}{}", if f.bluetooth { "on Bluetooth, " } else { "" }, r.describe())).collect();
                let c = Controller { name: f.name, report, between: f.between, raw, rumbles: true, label: f.model.label().to_string(), source: Source::Hid(k as u32, None), order: (1, k as u64), notes };
                hid.push((c, f.ids));
            }
        }
        if let Some(g) = self.gilrs.as_mut() {
            // gilrs panics on an event of a controller it holds no state for
            // yet (F3 crashes 20261026-154532 and -170104, twice in an hour
            // for a Steam player with two pads, at a window's focus change):
            // on Windows its polling thread reports a controller that has just
            // appeared (a Steam Input pad) before the added callback's
            // Connected, and `next_event` indexes its data by that id. Such an
            // event is dropped, said once, and the controller read once its
            // Connected comes.
            loop {
                let ev = match crate::report::caught(|| g.next_event()) {
                    Ok(Some(ev)) => ev,
                    Ok(None) => break,
                    Err(e) => {
                        if !self.gilrs_fault_said {
                            self.gilrs_fault_said = true;
                            crate::say(&format!("gamepad: an event of a controller gilrs has no state for yet, dropped ({e})"));
                        }
                        continue;
                    }
                };
                if self.trace {
                    crate::say(&format!("[gilrs] {:?}", ev.event));
                }
                // each state the controller went through, not only the last
                // (`next_event` has put the event in its state)
                if gilrs_event_buttons(&ev.event)
                    && let Some(pad) = g.connected_gamepad(ev.id)
                {
                    self.gilrs_seen.entry(ev.id).or_default().saw(report_of(&pad).buttons);
                }
            }
            for (id, pad) in g.gamepads() {
                if !pad.is_connected() {
                    continue;
                }
                // a controller with the USB ids of one read over HID is that
                // one: read there, its motors set there (Windows.Gaming.Input
                // takes a PlayStation controller's vibration and drops it)
                #[cfg(windows)]
                if let Some((c, _)) = hid.iter_mut().find(|(c, ids)| matches!(c.source, Source::Hid(_, None)) && Some(*ids) == pad.vendor_id().zip(pad.product_id())) {
                    if let Source::Hid(k, _) = c.source {
                        c.source = Source::Hid(k, Some(id));
                    }
                    continue;
                }
                // a PlayStation controller HID has not read yet (the interfaces
                // are looked through once a second: one just plugged in) is
                // passed over a moment, so no window takes gilrs's reading of
                // it, a name that would go; gilrs keeps one HID cannot open,
                // or does not find
                #[cfg(windows)]
                if let Some(h) = self.hid.as_ref()
                    && let Some(ids) = pad.vendor_id().zip(pad.product_id())
                    && crate::hidpad::Model::of(ids.0, ids.1).is_some()
                {
                    let since = *self.unmatched.entry(id).or_insert_with(web_time::Instant::now);
                    if wait_for_hid(h.refused(ids), since.elapsed().as_millis()) {
                        continue;
                    }
                }
                // a Windows.Gaming.Input Gamepad (gilrs gives it no uuid) with an
                // XInput pad's USB ids (any, where XInput does not say them) is
                // that pad, XInput's; another is gilrs's
                #[cfg(windows)]
                if pad.uuid() == [0; 16]
                    && let Some(k) = xinput_ids.iter().position(|x| x.is_none() || *x == pad.vendor_id().zip(pad.product_id()))
                {
                    xinput_ids.remove(k);
                    continue;
                }
                let label = pad.name().to_string();
                if !self.seen.contains(&label) {
                    crate::say(&format!("gamepad: {label}{}", if pad.is_ff_supported() { " (rumble)" } else { "" }));
                    self.seen.push(label.clone());
                }
                let slot = usize::from(id) as u64;
                #[cfg(target_os = "linux")]
                let (name, order) = {
                    use gilrs::LinuxGamepadExt;
                    let path = pad.devpath().to_string_lossy().into_owned();
                    let n = path.rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse::<u64>().ok());
                    (path, n.map_or((2, slot), |n| (1, n)))
                };
                // after XInput's and the HID ones
                #[cfg(windows)]
                let (name, order) = (format!("pad{slot}"), (2, slot));
                #[cfg(not(any(target_os = "linux", windows)))]
                let (name, order) = (format!("pad{slot}"), (1, slot));
                let mut report = report_of(&pad);
                let between = self.gilrs_seen.get_mut(&id).map_or(0, |l| l.take(report.buttons));
                let raw = report.sticks;
                report.rest(self.dead_zone);
                // a Mac's PlayStation controller rumbles through IOKit
                // (`hidpad::mac`), gilrs having no force feedback there
                #[cfg(target_os = "macos")]
                let rumbles = pad.is_ff_supported() || pad.vendor_id().zip(pad.product_id()).is_some_and(|(v, p)| crate::hidpad::Model::of(v, p).is_some());
                #[cfg(not(target_os = "macos"))]
                let rumbles = GILRS_FF && pad.is_ff_supported();
                out.push(Controller { name, report, between, raw, rumbles, label, source: Source::Gilrs(id), order, notes: Vec::new() });
            }
            // what the ones passed over pressed (read through XInput or HID
            // instead) is not kept for later; a controller gone, forgotten
            for l in self.gilrs_seen.values_mut() {
                l.take(0);
            }
            self.gilrs_seen.retain(|id, _| g.connected_gamepad(*id).is_some());
        }
        #[cfg(windows)]
        {
            let gone: Vec<gilrs::GamepadId> = self.unmatched.keys().copied().filter(|id| self.gilrs.as_ref().is_none_or(|g| g.connected_gamepad(*id).is_none())).collect();
            for id in gone {
                self.unmatched.remove(&id);
            }
            out.extend(hid.into_iter().map(|(c, _)| c));
        }
        out.sort_by_key(|c| c.order);
        out
    }

    /// Every connected pad, by the Gamepad API's index, each laid out by
    /// `padmap` and through the dead zone; whether one can rumble is the
    /// page's (its motors, `window.eqoaRumble`).
    #[cfg(target_arch = "wasm32")]
    fn read(&mut self) -> Vec<Controller> {
        let pads = crate::web::pad::gamepads();
        let rumbles = !pads.is_empty() && web_can_rumble();
        let mut out = Vec::new();
        let mut traced = Vec::new();
        let mut described = Vec::new();
        for p in &pads {
            // a pad's reading is kept by its slot, anew when another pad
            // takes the slot
            let k = match self.web.iter().position(|(i, id, _)| *i == p.index && *id == p.id) {
                Some(k) => k,
                None => {
                    self.web.retain(|(i, _, _)| *i != p.index);
                    self.web.push((p.index, p.id.clone(), crate::padmap::PadMap::default()));
                    self.web.len() - 1
                }
            };
            let map = &mut self.web[k].2;
            let mut r = map.read(p, self.browser);
            if !self.seen.contains(&p.id) {
                crate::say(&format!("gamepad: {} (mapping {:?}, {})", p.id, p.mapping, map.how(p, self.browser)));
                self.seen.push(p.id.clone());
            }
            let d = map.describe(p, self.browser);
            if self.trace {
                // `?padtrace`: a pad as it changes, its axes to 0.05
                let coarse = format!("{} {:?} {:?}", p.index, p.buttons.iter().map(|b| b.0).collect::<Vec<_>>(), p.axes.iter().map(|v| (v * 20.0).round() as i32).collect::<Vec<_>>());
                if !self.traced.contains(&coarse) {
                    crate::say(&format!("[pad] {d}"));
                }
                traced.push(coarse);
            }
            described.push(d);
            let raw = r.sticks;
            r.rest(self.dead_zone);
            out.push(Controller { name: format!("pad{}", p.index), report: r, between: 0, raw, rumbles, label: p.id.clone(), source: Source::Page(p.index), order: (1, p.index as u64), notes: Vec::new() });
        }
        self.traced = traced;
        self.described = described;
        out.sort_by_key(|c| c.order);
        out
    }

    /// The motors as `VIPad` set them, on the controllers this window reads:
    /// the small one on or off, the large one's level, for at most `ms` (the
    /// longer one's time left; they are set again when it ends). Natively an
    /// Xbox controller's through XInput (Windows), a PlayStation one's read
    /// over HID through its own reports (`hidpad`, Windows), any other's
    /// through gilrs (evdev's force feedback on Linux, Windows.Gaming.Input's
    /// vibration; macOS has none); in a page the Gamepad API's dual-rumble.
    pub fn rumble(&mut self, act: [u8; 2], ms: i64) {
        let ms = ms.clamp(0, 5000) as u32;
        #[cfg(target_arch = "wasm32")]
        {
            let pads: Vec<u32> = self.rumble_to.iter().map(|s| match s {
                Source::Page(i) => *i,
            }).collect();
            crate::web::rumble(act[1] as f64 / 255.0, if act[0] != 0 { 1.0 } else { 0.0 }, ms, &pads);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
            #[cfg(windows)]
            if let Some(x) = self.xinput.as_mut() {
                for s in &self.rumble_to {
                    if let Source::XInput(i) = s {
                        x.vibrate(*i, act, ms);
                    }
                }
            }
            #[cfg(windows)]
            if let Some(h) = self.hid.as_mut() {
                for s in &self.rumble_to {
                    if let Source::Hid(k, _) = s {
                        h.rumble(*k as usize, crate::hidpad::Motors::of_act(act), ms);
                    }
                }
            }
            self.effect = None;
            let Some(g) = self.gilrs.as_mut() else { return };
            #[cfg(target_os = "macos")]
            {
                let ps: Vec<(u16, u16)> = self
                    .rumble_to
                    .iter()
                    .filter_map(|s| match s {
                        Source::Gilrs(id) => g.connected_gamepad(*id).and_then(|p| p.vendor_id().zip(p.product_id())),
                    })
                    .filter(|&(v, p)| crate::hidpad::Model::of(v, p).is_some())
                    .collect();
                if !ps.is_empty() && self.mac.is_none() && act != [0, 0] {
                    self.mac = crate::hidpad::mac::Mac::start();
                }
                if let Some(mac) = &self.mac {
                    for ids in ps {
                        mac.rumble(ids, crate::hidpad::Motors::of_act(act), ms);
                    }
                }
            }
            if act == [0, 0] || ms == 0 || !GILRS_FF {
                return;
            }
            // gilrs's own controllers
            let ids: Vec<gilrs::GamepadId> = self
                .rumble_to
                .iter()
                .filter_map(|s| match s {
                    Source::Gilrs(id) => Some(*id),
                    #[cfg(windows)]
                    Source::Hid(..) | Source::XInput(_) => None,
                })
                .collect();
            if ids.is_empty() {
                return;
            }
            let play = Replay { play_for: Ticks::from_ms(ms), ..Default::default() };
            let mut b = EffectBuilder::new();
            b.add_effect(BaseEffect { kind: BaseEffectType::Strong { magnitude: act[1] as u16 * 257 }, scheduling: play, envelope: Default::default() });
            if act[0] != 0 {
                b.add_effect(BaseEffect { kind: BaseEffectType::Weak { magnitude: u16::MAX }, scheduling: play, envelope: Default::default() });
            }
            match b.gamepads(&ids).repeat(Repeat::For(Ticks::from_ms(ms))).finish(g) {
                Ok(e) => {
                    let _ = e.play();
                    self.effect = Some(e);
                }
                Err(e) => crate::say(&format!("rumble: {e}")),
            }
        }
    }

    /// The motors this window set going, stopped (its controllers changed).
    fn stop_motors(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if !self.rumble_to.is_empty() {
            self.rumble([0, 0], 0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.effect = None;
            #[cfg(windows)]
            if let Some(x) = self.xinput.as_mut() {
                x.stop();
            }
            #[cfg(windows)]
            if let Some(h) = self.hid.as_mut() {
                h.stop_motors();
            }
            #[cfg(target_os = "macos")]
            if self.mac.is_some() && !self.rumble_to.is_empty() {
                self.rumble([0, 0], 0);
            }
        }
    }
}

/// The controllers as this window last polled them (`describe_controllers`).
static LISTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The connected controllers as a report lists them (`report`): first what
/// this window reads (its Controller Focus and the windows open), then a
/// line each, in their order: its number, what it calls itself, how it is
/// read and its name there, whether it rumbles, and which window reads it
/// now. As this window last polled them; empty before it did.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn describe_controllers() -> Vec<String> {
    LISTED.lock().map(|l| l.clone()).unwrap_or_default()
}

/// `describe_controllers`' lines: `window` what this window reads
/// (`describe`), the other windows open with what each says it reads (a
/// window's place held by no window the player sees shows here), each pad as
/// (name, label, how it is read, rumbles, notes), its notes on lines of
/// their own after it.
fn list_controllers(window: &str, pads: &[(&str, &str, &str, bool, &[String])], pick: &Pick, seat: &Seat) -> Vec<String> {
    let mut out = vec![window.to_string()];
    if !seat.alone() {
        let others: Vec<String> = seat.others.iter().map(|o| format!("window {} ({}, reads {})", o.place + 1, focus_name(o.focus), o.pad.as_deref().unwrap_or("none"))).collect();
        out.push(format!("other windows: {}", others.join(", ")));
    }
    if pads.is_empty() {
        out.push("none connected".into());
    }
    let me = if seat.alone() { "this window".to_string() } else { format!("this window ({})", seat.place + 1) };
    for (k, &(name, label, how, rumbles, notes)) in pads.iter().enumerate() {
        let mut by = Vec::new();
        if match pick {
            Pick::All => true,
            Pick::One(n) => n == name,
            Pick::Idle | Pick::Nothing => false,
        } {
            by.push(me.clone());
        }
        let mut others: Vec<usize> = seat.others.iter().filter(|o| o.pad.as_deref() == Some(name)).map(|o| o.place + 1).collect();
        others.sort_unstable();
        by.extend(others.into_iter().map(|w| format!("window {w}")));
        let by = if by.is_empty() { "no window".to_string() } else { by.join(", ") };
        out.push(format!("controller {}: {label} ({how}; {name}){}, read by {by}", k + 1, if rumbles { ", rumbles" } else { "" }));
        out.extend(notes.iter().map(|n| format!("controller {} sent: {n}", k + 1)));
    }
    out
}

thread_local! {
    /// What this window reads, in a few words (`reads`).
    static READS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// What this window reads of the controllers, in a few words, as the
/// Extended page shows it beside its Back legend (empty before the first
/// frame).
pub fn reads() -> String {
    READS.with(|r| r.borrow().clone())
}

/// `reads`' words: the window's number among others, and the controller by
/// its place in the order; `captured` a Pad shared under Capture Controller,
/// read while the window is in front. Short enough for the page's legend row.
fn reads_text(pick: &Pick, names: &[&str], seat: &Seat, captured: bool) -> String {
    let who = if seat.alone() { "Reads".to_string() } else { format!("Window {}:", seat.place + 1) };
    match pick {
        Pick::All => format!("{who} every controller"),
        Pick::One(n) => match names.iter().position(|c| c == n) {
            Some(k) if captured => format!("{who} controller {} in front", k + 1),
            Some(k) => format!("{who} controller {}", k + 1),
            None => format!("{who} none"),
        },
        Pick::Idle => match seat.front_other(None) {
            Some(o) => format!("{who} none, window {} in front", o.place + 1),
            None => format!("{who} none, not in front"),
        },
        Pick::Nothing => format!("{who} none"),
    }
}

/// What a window reads, said as it changes.
fn describe(pick: &Pick, pads: &[Controller], seat: &Seat, focus: Focus, captured: bool) -> String {
    let window = if seat.alone() { String::new() } else { format!("window {} of {}, ", seat.place + 1, seat.others.len() + 1) };
    let what = match pick {
        Pick::All if focus == Focus::Window => "every one while this window is in front".to_string(),
        Pick::All => "every one".to_string(),
        Pick::One(n) => match pads.iter().position(|c| c.name == *n) {
            Some(k) if captured => format!("controller {} ({}, {n}) while this window is in front (Capture Controller)", k + 1, pads[k].label),
            Some(k) => format!("controller {} ({}, {n})", k + 1, pads[k].label),
            None => n.clone(),
        },
        Pick::Idle if captured => "none while this window is not in front (Capture Controller)".to_string(),
        Pick::Idle if focus == Focus::Auto && !seat.alone() => match seat.front_other(None) {
            Some(o) => format!("none while window {} (Window) is in front", o.place + 1),
            None => "none while a window on Window is in front".to_string(),
        },
        Pick::Idle => "none while this window is not in front".to_string(),
        Pick::Nothing if focus == Focus::Off => "none (Controller Focus Off)".to_string(),
        Pick::Nothing => "none".to_string(),
    };
    format!("controllers: {window}{}: {what}", focus_name(focus))
}

/// A Controller Focus as the Extended page shows it.
pub fn focus_name(f: Focus) -> String {
    match f {
        Focus::Auto => "Auto".into(),
        Focus::Window => "Window".into(),
        Focus::Pad(n) => format!("Pad {n}"),
        Focus::Off => "Off".into(),
    }
}

/// One gamepad as libpad's report: positional face buttons, the shoulders,
/// the triggers past a third of their travel (the game reads L2 and R2 as
/// buttons), the d-pad, and the sticks with +y down. Natively: SDL's layout
/// through gilrs.
#[cfg(not(target_arch = "wasm32"))]
fn report_of(p: &gilrs::Gamepad<'_>) -> Report {
    use gilrs::{Axis, Button};
    let pressed = |b: Button| p.is_pressed(b) || p.button_data(b).is_some_and(|d| d.value() > TRIGGER && matches!(b, Button::LeftTrigger2 | Button::RightTrigger2));
    let mut r = Report::default();
    for (b, k) in GILRS_BUTTONS {
        if pressed(b) {
            r.buttons |= k;
        }
    }
    // gilrs reports y up; libpad's runs down
    r.sticks = [p.value(Axis::RightStickX), -p.value(Axis::RightStickY), p.value(Axis::LeftStickX), -p.value(Axis::LeftStickY)];
    r
}

/// How far a trigger goes (of its travel) before it counts as L2 or R2.
#[cfg(not(target_arch = "wasm32"))]
const TRIGGER: f32 = 0.33;

/// gilrs's buttons (SDL's layout) as libpad's, by position.
#[cfg(not(target_arch = "wasm32"))]
const GILRS_BUTTONS: [(gilrs::Button, u16); 16] = {
    use gilrs::Button;
    [
        (Button::South, bit::CROSS),
        (Button::East, bit::CIRCLE),
        (Button::West, bit::SQUARE),
        (Button::North, bit::TRIANGLE),
        (Button::LeftTrigger, bit::L1),
        (Button::RightTrigger, bit::R1),
        (Button::LeftTrigger2, bit::L2),
        (Button::RightTrigger2, bit::R2),
        (Button::Select, bit::SELECT),
        (Button::Start, bit::START),
        (Button::LeftThumb, bit::L3),
        (Button::RightThumb, bit::R3),
        (Button::DPadUp, bit::UP),
        (Button::DPadDown, bit::DOWN),
        (Button::DPadLeft, bit::LEFT),
        (Button::DPadRight, bit::RIGHT),
    ]
};

/// Whether a gilrs event can change libpad's buttons (`report_of`): a
/// button pressed, let go or moved (a trigger's travel).
#[cfg(not(target_arch = "wasm32"))]
fn gilrs_event_buttons(e: &gilrs::EventType) -> bool {
    use gilrs::EventType;
    matches!(e, EventType::ButtonPressed(..) | EventType::ButtonReleased(..) | EventType::ButtonChanged(..))
}

/// In a page: whether a connected gamepad has a vibration actuator (the
/// page drives it, `window.eqoaRumble`).
#[cfg(target_arch = "wasm32")]
fn web_can_rumble() -> bool {
    crate::web::can_rumble()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a read gives of a controller that sent `states` since the last
    /// read (`Gamepads::poll`: the last state, and with `Latch` what was
    /// let go since), read after read.
    fn read_after_read(reads: &[&[u16]], latched: bool) -> Vec<u16> {
        let mut l = Latch::default();
        let mut now = 0;
        reads
            .iter()
            .map(|states| {
                for &s in *states {
                    l.saw(s);
                    now = s;
                }
                let between = l.take(now);
                if latched { now | between } else { now }
            })
            .collect()
    }

    /// A press let go before the next read is down for that read and up
    /// the next; one still held is the state's; one held through a read and
    /// let go since is up at once; two between the same reads are one; a
    /// tap before a stall (reads with nothing new) is down once, not for
    /// good. The controls: the last state alone (the client's before) loses
    /// the tap, and keeping every button seen (not only presses) holds a
    /// let-go button a read too long.
    #[test]
    fn a_press_between_two_reads_is_read_once() {
        let (x, o) = (bit::CROSS, bit::CIRCLE);
        assert_eq!(read_after_read(&[&[x, 0], &[], &[]], true), [x, 0, 0], "a tap between two reads");
        assert_eq!(read_after_read(&[&[x, 0], &[], &[]], false), [0, 0, 0], "the control: the last state alone loses it");
        assert_eq!(read_after_read(&[&[x], &[x], &[x, 0], &[]], true), [x, x, 0, 0], "held, then let go: up at once");
        assert_eq!(read_after_read(&[&[x, 0, x, 0], &[]], true), [x, 0], "two taps between the same reads: one");
        assert_eq!(read_after_read(&[&[x, 0], &[], &[], &[], &[]], true), [x, 0, 0, 0, 0], "a stall after it: down once");
        assert_eq!(read_after_read(&[&[o], &[o, o | x, o], &[o, 0]], true), [o, o | x, 0], "a tap while another is held");
        // the control: every button seen kept (a level, not a press): a
        // button let go since the last read stays down a read too long
        let mut seen = 0u16;
        let level: Vec<u16> = [&[x][..], &[x, 0][..], &[][..]]
            .iter()
            .map(|states| {
                let now = states.last().copied().unwrap_or(0);
                seen |= states.iter().fold(0, |a, s| a | s);
                now | std::mem::take(&mut seen)
            })
            .collect();
        assert_eq!(level, [x, x, 0], "the control: kept as a level, the release is a read late");
        assert_ne!(level, read_after_read(&[&[x], &[x, 0], &[]], true));
    }

    /// Through the game's own pad path (`VIPad::Read`, `PadKeys`): a tap
    /// between two reads is one press and one release, a held button one
    /// press however many reads it is held over. The control: the last
    /// state alone never presses it.
    #[test]
    fn a_tap_between_two_frames_reaches_the_game_once() {
        let keys = |reads: &[&[u16]], latched: bool| -> Vec<(i32, bool)> {
            let mut c = Console::new();
            c.poll(Report::default(), true, false, false);
            read_after_read(reads, latched).into_iter().flat_map(|b| c.poll(Report { buttons: b, sticks: [0.0; 4] }, true, false, false)).collect()
        };
        // cross is key 134 (`key_name`)
        assert_eq!(key_name(134), "cross");
        assert_eq!(keys(&[&[bit::CROSS, 0], &[], &[]], true), [(134, true), (134, false)]);
        assert_eq!(keys(&[&[bit::CROSS], &[bit::CROSS], &[bit::CROSS], &[0], &[]], true), [(134, true), (134, false)], "held: once");
        assert!(keys(&[&[bit::CROSS, 0], &[], &[]], false).is_empty(), "the control: lost");
    }

    /// A button's name is the one scripts give it (`named`), the first of
    /// several; none is "?".
    #[test]
    fn a_button_is_named_as_a_script_names_it() {
        for n in ["select", "l3", "r3", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"] {
            assert_eq!(bit::name(bit::named(n).unwrap()), n);
        }
        assert_eq!(bit::name(bit::CROSS | bit::SQUARE), "cross");
        assert_eq!(bit::name(0), "?");
    }

    #[test]
    fn a_stick_byte_rests_in_the_dead_zone_and_reaches_both_ends() {
        assert_eq!(stick_byte(0.0), 128);
        assert_eq!((stick_byte(-1.0), stick_byte(1.0)), (0, 255));
        // 1.33: three quarters of the way is the end, as on a DualShock 2
        assert_eq!((stick_byte(-0.76), stick_byte(0.76)), (0, 255));
        // a full diagonal runs: each axis past the 0.7 the walk runs above
        let d = vi::pad::scale_stick::<Ee>(stick_byte(std::f32::consts::FRAC_1_SQRT_2)).0;
        assert!((2.0 * d * d).sqrt() > 0.7, "{d}");
    }

    /// A stick resting as far off its middle as Microsoft allows (0.265 of
    /// the right stick's travel, or -0.29) reads as rest through the host's
    /// dead zone;
    /// without it the game reads it as pushed (`ScaleStick` past 167), which
    /// walks the player. A push past the dead zone passes as it was.
    #[test]
    fn a_stick_resting_off_its_middle_is_at_rest() {
        let game = |r: &Report| -> [f32; 4] {
            let d = r.data();
            std::array::from_fn(|i| vi::pad::scale_stick::<Ee>(d[4 + i]).0)
        };
        for v in [0.24, 0.265, -0.29] {
            let mut r = Report { buttons: 0, sticks: [v, 0.0, 0.0, v] };
            // the negative control: as it came, the game reads both pushed
            assert!(game(&r)[0] != 0.0 && game(&r)[3] != 0.0, "{v} with no dead zone is a push: {:?}", game(&r));
            r.rest(STICK_DEAD_ZONE);
            assert_eq!(game(&r), [0.0; 4], "{v}");
        }
        // past it, as it was: the walk's push and the run's
        for v in [0.35, 0.8] {
            let mut r = Report { buttons: 0, sticks: [0.0, v, -v, 0.0] };
            let before = game(&r);
            r.rest(STICK_DEAD_ZONE);
            assert_eq!(game(&r), before, "{v}");
            assert!(before[1] > 0.0 && before[2] < 0.0, "{v}: {before:?}");
        }
    }

    #[test]
    fn the_report_is_read_back_as_the_buttons_it_holds() {
        // through VIPad::Read: each named button lands on its own field
        let names = ["triangle", "square", "cross", "circle", "left", "right", "up", "down", "l1", "l2", "r1", "r2", "start", "select", "l3", "r3"];
        for (k, n) in names.iter().enumerate() {
            let mut c = Console::new();
            let r = Report { buttons: bit::named(n).unwrap(), sticks: [0.0; 4] };
            assert!(c.poll(r, true, false, false).is_empty(), "the first read finds the pad");
            let keys = c.poll(r, true, false, false);
            assert_eq!(keys, vec![(132 + k as i32, true)], "{n}");
        }
    }

    #[test]
    fn pad_keys_sends_each_change_once_and_a_lost_pad_lets_go() {
        let mut c = Console::new();
        let mut r = Report { buttons: bit::CROSS | bit::R1, sticks: [0.0, 0.0, 0.9, -0.2] };
        c.poll(r, true, false, false);
        assert_eq!(c.poll(r, true, false, false), vec![(130, true), (134, true), (142, true)]);
        assert!(c.poll(r, true, false, false).is_empty(), "held: nothing new");
        r.buttons = bit::R1;
        assert_eq!(c.poll(r, true, false, false), vec![(134, false)]);
        assert!(c.sticks()[2] > 0.8 && c.sticks()[3] == 0.0, "{:?}", c.sticks());
        // unplugged: every field to zero, so every key held goes up
        assert_eq!(c.poll(r, false, false, false), vec![(130, false), (142, false)]);
        assert_eq!(c.mode(), 0);
        // and a dead player's pad is not read into keys at all
        let mut d = Console::new();
        d.poll(r, true, false, false);
        assert!(d.poll(r, true, false, true).is_empty());
    }

    #[test]
    fn a_frame_late_does_not_swallow_a_vibration() {
        let r = Report::default();
        let ready = || {
            let mut c = Console::new();
            for _ in 0..3 {
                c.poll(r, true, true, false);
            }
            assert_eq!(c.process(1000), None, "the last Process, a frame before");
            c
        };
        // the control: the game's own way, the vibration timed from the last
        // Process; a frame 500 ms later and a 140 ms vibration is over before
        // it is played (the pad told "off" at once)
        let mut c = ready();
        c.rumble(1, 220, 140, true);
        assert_eq!(c.process(1500), Some(([0, 0], 0)), "swallowed");
        // timed from this frame, it runs its 140 ms
        let mut c = ready();
        c.now(1500);
        c.rumble(1, 220, 140, true);
        assert_eq!(c.process(1500), Some(([0, 220], 140)));
        assert_eq!(c.process(1641), Some(([0, 0], 0)), "and ends on time");
    }

    #[test]
    fn the_motors_run_only_after_the_handshake_and_stop_on_time() {
        let mut c = Console::new();
        let r = Report::default();
        c.poll(r, true, true, false); // found: the teardown, state -1
        c.poll(r, true, true, false); // the handshake: aligned
        c.rumble(1, 200, 100, true);
        assert_eq!(c.process(0), None, "not yet active: the request is answered on the next read");
        c.poll(r, true, true, false);
        c.rumble(1, 200, 100, true);
        c.rumble(0, 0, 50, true);
        assert_eq!(c.process(10), Some(([1, 200], 90)));
        assert_eq!(c.process(60), Some(([0, 200], 40)), "the small motor ends first");
        assert_eq!(c.process(80), None);
        assert_eq!(c.process(111), Some(([0, 0], 0)));
        // with the option off, nothing
        c.rumble(1, 255, 100, false);
        assert_eq!(c.process(120), None);
        // a pad that cannot rumble never completes the handshake
        let mut n = Console::new();
        for _ in 0..3 {
            n.poll(r, true, false, false);
        }
        n.rumble(1, 200, 100, true);
        assert_eq!(n.process(0), None);
    }

    fn seat(place: usize, others: &[(usize, Option<&str>, Focus)]) -> Seat {
        let others = others.iter().map(|&(place, pad, focus)| crate::multibox::Other { place, pad: pad.map(str::to_string), focus, front: None }).collect();
        Seat { place, others }
    }

    /// A seat whose other windows say since when they have had the focus.
    fn seat_at(place: usize, others: &[(usize, Option<&str>, Focus, Option<u64>)]) -> Seat {
        let others = others.iter().map(|&(place, pad, focus, front)| crate::multibox::Other { place, pad: pad.map(str::to_string), focus, front }).collect();
        Seat { place, others }
    }

    fn one(n: &str) -> Pick {
        Pick::One(n.to_string())
    }

    /// A report's controllers (`describe_controllers`, F3 20261024-165502
    /// said only that two controllers did not drive two windows): what this
    /// window reads, then each controller, how it is read and which windows
    /// read it; one no window reads says so, as does none connected.
    /// gilrs's reading of a PlayStation controller HID has not read is passed
    /// over for a moment, then kept; one HID could not open is kept at once.
    /// The control: past the moment it is not passed over.
    #[test]
    fn a_playstation_pad_waits_a_moment_for_hid() {
        assert!(wait_for_hid(false, 0));
        assert!(wait_for_hid(false, HID_GRACE_MS - 1));
        assert!(!wait_for_hid(false, HID_GRACE_MS), "the control: after the moment, gilrs's");
        assert!(!wait_for_hid(true, 0), "refused: gilrs's at once");
    }

    #[test]
    fn a_report_lists_the_controllers_and_who_reads_them() {
        let sent = ["on Bluetooth, report 0x01, 78 bytes: 01 80 7f 81 80 08 00 00 00 00 00 00".to_string()];
        let pads = [
            ("xinput0", "Xbox controller 1", "XInput: in front or behind", true, &[][..]),
            ("pad3", "DualSense Wireless Controller", "Windows.Gaming.Input: only by the window in front", false, &[][..]),
            ("hid-054c-0ce6-c5d9d8d5", "DualSense", "HID: in front or behind", false, &sent[..]),
        ];
        let two = seat(0, &[(1, Some("xinput0"), Focus::Pad(1))]);
        let l = list_controllers("controllers: window 1 of 2, Auto: none", &pads, &Pick::Nothing, &two);
        assert_eq!(
            l,
            vec![
                "controllers: window 1 of 2, Auto: none".to_string(),
                "other windows: window 2 (Pad 1, reads xinput0)".to_string(),
                "controller 1: Xbox controller 1 (XInput: in front or behind; xinput0), rumbles, read by window 2".to_string(),
                "controller 2: DualSense Wireless Controller (Windows.Gaming.Input: only by the window in front; pad3), read by no window".to_string(),
                "controller 3: DualSense (HID: in front or behind; hid-054c-0ce6-c5d9d8d5), read by no window".to_string(),
                "controller 3 sent: on Bluetooth, report 0x01, 78 bytes: 01 80 7f 81 80 08 00 00 00 00 00 00".to_string(),
            ]
        );
        let alone = list_controllers("controllers: Auto: every one", &pads, &Pick::All, &Seat::default());
        assert!(alone[1].ends_with("read by this window") && alone[2].ends_with("read by this window"), "{alone:?}");
        assert!(!alone.iter().any(|l| l.starts_with("other windows")), "alone: no other windows' line");
        let mine = list_controllers("x", &pads, &one("pad3"), &two);
        assert!(mine[2].ends_with("read by window 2") && mine[3].ends_with("read by this window (1)"), "{mine:?}");
        let three = seat(1, &[(0, Some("xinput0"), Focus::Auto), (2, None, Focus::Auto)]);
        assert_eq!(list_controllers("x", &[], &Pick::Nothing, &three)[1], "other windows: window 1 (Auto, reads xinput0), window 3 (Auto, reads none)");
        assert_eq!(list_controllers("x", &[], &Pick::Nothing, &Seat::default())[1], "none connected");
    }

    /// Off, Window and a Pad, each as it says; Auto alone reads every one,
    /// in front or not, as the client always did.
    #[test]
    fn a_window_reads_what_its_focus_says() {
        let pads = ["p1", "p2", "p3"];
        let alone = Seat::default();
        assert_eq!(pick(Focus::Auto, true, &pads, &alone, None), Pick::All);
        assert_eq!(pick(Focus::Auto, false, &pads, &alone, None), Pick::All, "in front or not");
        assert_eq!(pick(Focus::Off, true, &pads, &alone, None), Pick::Nothing);
        assert_eq!(pick(Focus::Window, true, &pads, &alone, None), Pick::All);
        assert_eq!(pick(Focus::Window, false, &pads, &alone, None), Pick::Idle, "at rest, not unplugged");
        assert_eq!(pick(Focus::Window, false, &[], &alone, None), Pick::Nothing);
        assert_eq!(pick(Focus::Pad(2), false, &pads, &alone, None), one("p2"));
        assert_eq!(pick(Focus::Pad(4), true, &pads, &alone, None), Pick::Nothing, "no fourth");
    }

    /// Auto among other windows, each worked out from its own seat: the
    /// first window the first controller, the second the next, a third none;
    /// one controller for two stays the first window's though it says
    /// nothing yet (it read every one, alone), and the control: with the
    /// first window Off, the second takes it.
    #[test]
    fn multiboxed_windows_take_a_controller_each_in_order() {
        let pads = ["p1", "p2"];
        let auto = Focus::Auto;
        assert_eq!(pick(auto, true, &pads, &seat(0, &[(1, None, auto)]), None), one("p1"));
        assert_eq!(pick(auto, false, &pads, &seat(1, &[(0, None, auto)]), None), one("p2"));
        assert_eq!(pick(auto, false, &pads, &seat(2, &[(0, Some("p1"), auto), (1, Some("p2"), auto)]), None), Pick::Nothing);
        assert_eq!(pick(auto, true, &["p1"], &seat(0, &[(1, None, auto)]), None), one("p1"));
        assert_eq!(pick(auto, true, &["p1"], &seat(1, &[(0, None, auto)]), None), Pick::Nothing, "the first window's");
        assert_eq!(pick(auto, true, &["p1"], &seat(1, &[(0, None, Focus::Off)]), None), one("p1"), "the control");
        assert_eq!(pick(auto, true, &pads, &seat(1, &[(0, None, Focus::Off)]), None), one("p2"), "still the second for the second window");
    }

    /// A controller lost and found again: the window that had it waits for
    /// it (reading none, and not the other window's, which keeps its own); it
    /// comes back to that window, and so does another found once the wait
    /// is over. The control: with what each read forgotten, the order alone
    /// would swap them.
    #[test]
    fn a_lost_controller_moves_no_other_window() {
        let auto = Focus::Auto;
        assert_eq!(pick(auto, true, &["p2"], &seat(0, &[(1, Some("p2"), auto)]), Some("p1")), Pick::Nothing);
        assert_eq!(pick(auto, true, &["p2"], &seat(1, &[(0, Some("p1"), auto)]), Some("p2")), one("p2"));
        assert_eq!(pick(auto, true, &["p1", "p2"], &seat(0, &[(1, Some("p2"), auto)]), Some("p1")), one("p1"), "back");
        // the wait over (let go), found again at a new place in the order
        assert_eq!(pick(auto, true, &["p2"], &seat(0, &[(1, Some("p2"), auto)]), None), Pick::Nothing);
        assert_eq!(pick(auto, true, &["p2", "p9"], &seat(0, &[(1, Some("p2"), auto)]), None), one("p9"));
        assert_eq!(pick(auto, true, &["p2", "p9"], &seat(1, &[(0, None, auto)]), Some("p2")), one("p2"));
        assert_eq!(pick(auto, true, &["p2", "p9"], &seat(1, &[(0, None, auto)]), None), one("p9"), "the control");
    }

    /// With a spare controller connected, a window whose own is gone a
    /// moment waits for it rather than take the spare, and another window
    /// sees it waiting (it says what it waits for) and leaves the spare be;
    /// let go, it takes the spare. The control is that last.
    #[test]
    fn a_window_waits_for_its_own_controller_before_a_spare() {
        let auto = Focus::Auto;
        assert_eq!(pick(auto, true, &["p1", "p3"], &seat(1, &[(0, Some("p1"), auto)]), Some("p2")), Pick::Nothing, "waits");
        assert_eq!(pick(auto, true, &["p1", "p3"], &seat(2, &[(0, Some("p1"), auto), (1, Some("p2"), auto)]), None), one("p3"), "a third window: the spare is free");
        assert_eq!(pick(auto, true, &["p1", "p2", "p3"], &seat(1, &[(0, Some("p1"), auto)]), Some("p2")), one("p2"), "its own back");
        assert_eq!(pick(auto, true, &["p1", "p3"], &seat(1, &[(0, Some("p1"), auto)]), None), one("p3"), "the control: let go");
    }

    /// A Pad is its window's whatever else: two windows on the same Pad are
    /// both driven by it (one controller for several windows); an Auto
    /// window passes over it, and one that read it by Auto lets it go; two
    /// Auto windows that said one controller at once leave it to the first;
    /// Window and Off windows take none but count in the order.
    #[test]
    fn a_chosen_controller_comes_first() {
        let auto = Focus::Auto;
        let pads = ["p1", "p2"];
        assert_eq!(pick(Focus::Pad(1), false, &pads, &seat(1, &[(0, Some("p1"), Focus::Pad(1))]), None), one("p1"), "shared, behind");
        assert_eq!(pick(Focus::Pad(1), true, &pads, &seat(0, &[(1, Some("p1"), Focus::Pad(1))]), None), one("p1"), "shared, in front");
        assert_eq!(pick(Focus::Pad(2), true, &pads, &seat(0, &[(1, None, auto)]), None), one("p2"));
        assert_eq!(pick(auto, true, &pads, &seat(1, &[(0, Some("p2"), Focus::Pad(2))]), None), one("p1"));
        assert_eq!(pick(auto, true, &pads, &seat(0, &[(1, Some("p1"), Focus::Pad(1))]), Some("p1")), one("p2"));
        assert_eq!(pick(auto, true, &pads, &seat(0, &[(1, Some("p1"), auto)]), Some("p1")), one("p1"));
        assert_eq!(pick(auto, true, &pads, &seat(1, &[(0, Some("p1"), auto)]), Some("p1")), one("p2"));
        assert_eq!(pick(auto, true, &pads, &seat(2, &[(0, None, Focus::Window), (1, None, Focus::Off)]), None), one("p1"), "the third window, two controllers: the first free");
    }

    /// Capture Controller: a Pad shared with another window is read only in
    /// front (behind, at rest, not unplugged); one no other window has, and
    /// Auto, Window and Off, are as `pick` gave them, captured or not. The
    /// control: off, both windows on the shared Pad read it.
    #[test]
    fn a_captured_pad_drives_only_the_window_in_front() {
        let pads = ["p1", "p2"];
        let shared = seat(0, &[(1, Some("p1"), Focus::Pad(1))]);
        let read = |focus: Focus, front: bool, seat: &Seat, capture: bool| captured(pick(focus, front, &pads, seat, None), focus, front, seat, capture);
        assert_eq!(read(Focus::Pad(1), true, &shared, true), one("p1"), "in front: its pad");
        assert_eq!(read(Focus::Pad(1), false, &shared, true), Pick::Idle, "behind: at rest");
        assert_eq!(read(Focus::Pad(1), false, &shared, false), one("p1"), "the control: off, every window on the pad reads it");
        assert_eq!(read(Focus::Pad(1), true, &shared, false), one("p1"));
        // a pad no other window has: as ever, behind or not
        let unshared = seat(0, &[(1, Some("p2"), Focus::Pad(2))]);
        assert_eq!(read(Focus::Pad(1), false, &unshared, true), one("p1"), "unshared: read behind");
        assert_eq!(read(Focus::Pad(1), false, &Seat::default(), true), one("p1"), "alone: read behind");
        // the other foci are untouched
        assert_eq!(read(Focus::Auto, false, &Seat::default(), true), Pick::All);
        assert_eq!(read(Focus::Window, false, &shared, true), Pick::Idle);
        assert_eq!(read(Focus::Window, true, &shared, true), Pick::All);
        assert_eq!(read(Focus::Off, true, &shared, true), Pick::Nothing);
        // beside a window on Window (it reads every controller in front),
        // captured, a Pad is read only in front too; not captured, as ever
        let beside_window = seat(0, &[(1, None, Focus::Window)]);
        assert_eq!(read(Focus::Pad(1), false, &beside_window, true), Pick::Idle, "beside a Window one, captured: behind, at rest");
        assert_eq!(read(Focus::Pad(1), true, &beside_window, true), one("p1"), "in front: its pad");
        assert_eq!(read(Focus::Pad(1), false, &beside_window, false), one("p1"), "the control: not captured, read behind");
        // a shared pad not connected stays unplugged, not at rest
        let three = seat(0, &[(1, None, Focus::Pad(3))]);
        assert_eq!(read(Focus::Pad(3), false, &three, true), Pick::Nothing);
        // the words beside the page's legend
        assert_eq!(reads_text(&one("p1"), &pads, &shared, true), "Window 1: controller 1 in front");
        assert_eq!(reads_text(&Pick::Idle, &pads, &shared, true), "Window 1: none, not in front");
        assert_eq!(reads_text(&one("p1"), &pads, &shared, false), "Window 1: controller 1");
        assert_eq!(reads_text(&Pick::All, &pads, &Seat::default(), false), "Reads every controller");
    }

    /// Two windows both told they have the focus (the first was never told
    /// it lost it, Steam's game mode on a Steam Deck: "swapping between
    /// focuses will end up having both in focus"): the one that came to the
    /// front last reads, the other rests, on Window and on a captured Pad
    /// alike; back again, the first reads. The control: windows that do not
    /// say since when (an older build) both read, as before.
    #[test]
    fn of_two_windows_told_they_have_the_focus_the_later_reads() {
        let pads = ["p1"];
        let win = Focus::Window;
        // window 1 came at 1,000 and was not told it lost the focus; window 2 came at 2,000
        let one_sees = seat_at(0, &[(1, None, win, Some(2_000))]);
        let two_sees = seat_at(1, &[(0, None, win, Some(1_000))]);
        assert!(!ahead(true, Some(1_000), &one_sees), "window 1: another came after it");
        assert!(ahead(true, Some(2_000), &two_sees), "window 2: the last to come");
        assert_eq!(pick(win, ahead(true, Some(1_000), &one_sees), &pads, &one_sees, None), Pick::Idle, "window 1 rests");
        assert_eq!(pick(win, ahead(true, Some(2_000), &two_sees), &pads, &two_sees, None), Pick::All, "window 2 reads");
        // back to window 1, told it has the focus again (at 3,000)
        let one_back = seat_at(0, &[(1, None, win, Some(2_000))]);
        let two_back = seat_at(1, &[(0, None, win, Some(3_000))]);
        assert!(ahead(true, Some(3_000), &one_back));
        assert!(!ahead(true, Some(2_000), &two_back));
        // a captured Pad: the same
        let pad = Focus::Pad(1);
        let one_pad = seat_at(0, &[(1, Some("p1"), pad, Some(2_000))]);
        let front = ahead(true, Some(1_000), &one_pad);
        assert_eq!(captured(pick(pad, front, &pads, &one_pad, None), pad, front, &one_pad, true), Pick::Idle, "the captured pad: window 1 rests");
        // told it lost the focus, a window says no time: the other is in front
        assert!(ahead(true, Some(2_000), &seat_at(1, &[(0, None, win, None)])));
        assert!(!ahead(false, None, &seat_at(1, &[(0, None, win, None)])), "without the focus, never");
        // the words: which window is in front
        assert_eq!(reads_text(&Pick::Idle, &pads, &one_sees, false), "Window 1: none, window 2 in front");
        // the control: no time said, both read
        let old = seat(0, &[(1, None, win)]);
        assert!(ahead(true, Some(1_000), &old), "the control: an older build's window says no time");
        assert_eq!(pick(win, ahead(true, Some(1_000), &old), &pads, &old, None), Pick::All);
    }

    /// One window on Window, another on Auto (Controller Focus is each
    /// window's own): the Auto one rests while the Window one is in front,
    /// which reads every controller, and reads its own again once that one is
    /// not; a window on Auto among Auto ones keeps what it reads. The
    /// control: before, it read the controller the Window one read in front.
    #[test]
    fn an_auto_window_rests_while_a_window_on_window_is_in_front() {
        let pads = ["p1"];
        let (auto, win) = (Focus::Auto, Focus::Window);
        let window_front = seat_at(1, &[(0, None, win, Some(5_000))]);
        assert_eq!(pick(auto, false, &pads, &window_front, None), Pick::Idle, "the Window one in front: at rest");
        assert_eq!(pick(auto, false, &[], &window_front, None), Pick::Nothing, "none connected: unplugged");
        assert_eq!(pick(auto, true, &pads, &window_front, None), one("p1"), "itself in front: its own");
        let window_behind = seat_at(1, &[(0, None, win, None)]);
        assert_eq!(pick(auto, false, &pads, &window_behind, None), one("p1"), "the Window one behind: its own, as ever");
        let auto_front = seat_at(1, &[(0, Some("p1"), auto, Some(5_000))]);
        assert_eq!(pick(auto, false, &["p1", "p2"], &auto_front, None), one("p2"), "an Auto one in front: each its own");
        // two controllers: at rest with its own kept (`Gamepads::poll` keeps
        // the one it read), then its own again, not the first by the order
        assert_eq!(pick(auto, false, &["p1", "p2"], &window_front, Some("p1")), Pick::Idle);
        assert_eq!(pick(auto, false, &["p1", "p2"], &window_behind, Some("p1")), one("p1"), "its own again");
        assert_eq!(pick(auto, false, &["p1", "p2"], &window_behind, None), one("p2"), "the control: forgotten, the second by the order");
        // the words
        assert_eq!(describe(&Pick::Idle, &[], &window_front, auto, false), "controllers: window 2 of 2, Auto: none while window 1 (Window) is in front");
        assert_eq!(reads_text(&Pick::Idle, &pads, &window_front, false), "Window 2: none, window 1 in front");
        // the control: the rule this replaces (no time: the Window one taken
        // as behind) read the controller with it
        assert_eq!(pick(auto, false, &pads, &seat(1, &[(0, None, win)]), None), one("p1"));
    }

    #[test]
    fn a_script_holds_each_step_for_its_time() {
        let s = Script::parse("start@32;cross@100+300; stick l 0 -1@200+100; bogus@5");
        assert_eq!(s.at(0), Report::default());
        assert_eq!(s.at(32).buttons, bit::START);
        assert_eq!(s.at(132).buttons, bit::CROSS, "start let go at 132, cross held from 100");
        let r = s.at(250);
        assert_eq!((r.buttons, r.sticks), (bit::CROSS, [0.0, 0.0, 0.0, -1.0]));
        assert_eq!(s.at(400), Report::default());
    }
}
