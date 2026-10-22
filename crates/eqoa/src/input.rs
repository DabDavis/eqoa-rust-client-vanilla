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
/// `EQOA_DEADZONE`.
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

/// `EQOA_PAD_TRACE=1` (in a page `?padtrace`): each pad key as the game
/// gets it, and gilrs's events and the controllers' report as they change.
pub fn trace() -> bool {
    #[cfg(target_arch = "wasm32")]
    return crate::web::page_flag("padtrace");
    #[cfg(not(target_arch = "wasm32"))]
    return std::env::var_os("EQOA_PAD_TRACE").is_some();
}

/// The controllers: natively through gilrs (SDL's layout), in a page
/// straight from the Gamepad API (`padmap`: every button and axis, and the
/// layout each browser gives). Every connected pad as one report.
pub struct Gamepads {
    #[cfg(not(target_arch = "wasm32"))]
    gilrs: Option<gilrs::Gilrs>,
    #[cfg(not(target_arch = "wasm32"))]
    effect: Option<gilrs::ff::Effect>,
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
        let gilrs = if std::env::var("EQOA_GAMEPAD").is_ok_and(|v| v == "0") {
            None
        } else {
            match gilrs::GilrsBuilder::new().with_force_feedback(true).build() {
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
        }
    }

    /// Every connected pad as one report; whether there is one, and whether
    /// one can rumble.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn poll(&mut self) -> (Report, bool, bool) {
        let Some(g) = self.gilrs.as_mut() else { return (Report::default(), false, false) };
        while let Some(ev) = g.next_event() {
            if self.trace {
                crate::say(&format!("[gilrs] {:?}", ev.event));
            }
        }
        let mut out = Report::default();
        let (mut any, mut motors) = (false, false);
        for (_, pad) in g.gamepads() {
            if !pad.is_connected() {
                continue;
            }
            any = true;
            motors |= pad.is_ff_supported();
            let name = pad.name().to_string();
            if !self.seen.contains(&name) {
                crate::say(&format!("gamepad: {name}{}", if pad.is_ff_supported() { " (rumble)" } else { "" }));
                self.seen.push(name);
            }
            let mut r = report_of(&pad);
            r.rest(self.dead_zone);
            out.merge(&r);
        }
        if self.trace && out != self.last {
            crate::say(&format!("[pad] report {out:?}"));
            self.last = out;
        }
        (out, any, motors)
    }

    /// Every connected pad as one report, read off the Gamepad API (each
    /// pad laid out by `padmap`); whether there is one, and whether one can
    /// rumble (the page's motors, `window.eqoaRumble`).
    #[cfg(target_arch = "wasm32")]
    pub fn poll(&mut self) -> (Report, bool, bool) {
        let pads = crate::web::pad::gamepads();
        let mut out = Report::default();
        let mut traced = Vec::new();
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
            if self.trace {
                // `?padtrace`: a pad as it changes, its axes to 0.05
                let coarse = format!("{} {:?} {:?}", p.index, p.buttons.iter().map(|b| b.0).collect::<Vec<_>>(), p.axes.iter().map(|v| (v * 20.0).round() as i32).collect::<Vec<_>>());
                if !self.traced.contains(&coarse) {
                    crate::say(&format!("[pad] {}", map.describe(p, self.browser)));
                }
                traced.push(coarse);
            }
            r.rest(self.dead_zone);
            out.merge(&r);
        }
        self.traced = traced;
        if self.trace && out != self.last {
            crate::say(&format!("[pad] report {out:?}"));
            self.last = out;
        }
        let any = !pads.is_empty();
        (out, any, any && web_can_rumble())
    }

    /// The motors as `VIPad` set them: the small one on or off, the large
    /// one's level, for at most `ms` (the longer one's time left; they are
    /// set again when it ends).
    pub fn rumble(&mut self, act: [u8; 2], ms: i64) {
        let ms = ms.clamp(0, 5000) as u32;
        #[cfg(target_arch = "wasm32")]
        crate::web::rumble(act[1] as f64 / 255.0, if act[0] != 0 { 1.0 } else { 0.0 }, ms);
        #[cfg(not(target_arch = "wasm32"))]
        {
            use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
            self.effect = None;
            let Some(g) = self.gilrs.as_mut() else { return };
            if act == [0, 0] || ms == 0 {
                return;
            }
            let ids: Vec<gilrs::GamepadId> = g.gamepads().filter(|(_, p)| p.is_connected() && p.is_ff_supported()).map(|(id, _)| id).collect();
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
}

/// One gamepad as libpad's report: positional face buttons, the shoulders,
/// the triggers past a third of their travel (the game reads L2 and R2 as
/// buttons), the d-pad, and the sticks with +y down. Natively: SDL's layout
/// through gilrs.
#[cfg(not(target_arch = "wasm32"))]
fn report_of(p: &gilrs::Gamepad<'_>) -> Report {
    use gilrs::{Axis, Button};
    let pressed = |b: Button| p.is_pressed(b) || p.button_data(b).is_some_and(|d| d.value() > 0.33 && matches!(b, Button::LeftTrigger2 | Button::RightTrigger2));
    let mut r = Report::default();
    for (b, k) in [
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
    ] {
        if pressed(b) {
            r.buttons |= k;
        }
    }
    // gilrs reports y up; libpad's runs down
    r.sticks = [p.value(Axis::RightStickX), -p.value(Axis::RightStickY), p.value(Axis::LeftStickX), -p.value(Axis::LeftStickY)];
    r
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
