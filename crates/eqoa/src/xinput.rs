//! Xbox controllers on Windows, read through XInput (`input::Gamepads`).
//!
//! gilrs reads Windows' controllers through Windows.Gaming.Input, which
//! answers only the program whose window is in front ("Windows Gaming Input
//! requires an in focus window", gilrs's README): two windows of the client
//! (multiboxing) could never each read its own controller at once, and every
//! controller drove the window in front. XInput answers every program,
//! whichever window is in front. It knows the Xbox controllers and those that
//! pass for one (Steam Input's, DS4Windows'), four at most, each at its user
//! index (the player number its light shows), which every program sees the
//! same. The client reads those through XInput, and gilrs's own reading of
//! them is passed over, matched by USB vendor and product (Windows.Gaming.Input
//! lays out a DualShock 4 as it does an Xbox controller, so the layout alone
//! does not tell them apart); every other controller (a DualShock 4, a
//! DualSense, a Switch pad) stays gilrs's, and reaches only the window in
//! front.

use crate::input::{Report, bit};

/// `XINPUT_GAMEPAD` as XInput fills it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XPad {
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

/// `XINPUT_GAMEPAD`'s buttons (xinput.h), each as libpad's: positional, A ✕,
/// B ○, X □, Y △, Back Select, the shoulders L1 and R1.
const BUTTONS: [(u16, u16); 14] = [
    (0x0001, bit::UP),
    (0x0002, bit::DOWN),
    (0x0004, bit::LEFT),
    (0x0008, bit::RIGHT),
    (0x0010, bit::START),
    (0x0020, bit::SELECT),
    (0x0040, bit::L3),
    (0x0080, bit::R3),
    (0x0100, bit::L1),
    (0x0200, bit::R1),
    (0x1000, bit::CROSS),
    (0x2000, bit::CIRCLE),
    (0x4000, bit::SQUARE),
    (0x8000, bit::TRIANGLE),
];

impl XPad {
    /// As libpad's report, as gilrs's reading of the same controller was:
    /// the buttons, the triggers past a third of their travel as L2 and R2,
    /// the sticks in -1..1 with +y down (XInput's +y is up).
    pub fn report(&self) -> Report {
        let mut r = Report::default();
        for (x, b) in BUTTONS {
            if self.buttons & x != 0 {
                r.buttons |= b;
            }
        }
        if self.lt as f32 / 255.0 > 0.33 {
            r.buttons |= bit::L2;
        }
        if self.rt as f32 / 255.0 > 0.33 {
            r.buttons |= bit::R2;
        }
        let axis = |v: i16| (v as f32 / 32767.0).clamp(-1.0, 1.0);
        r.sticks = [axis(self.rx), -axis(self.ry), axis(self.lx), -axis(self.ly)];
        r
    }
}

/// `XINPUT_CAPABILITIES_EX` as xinput1_4's unnamed export 108
/// (`XInputGetCapabilitiesEx`, Windows 8 on: SDL reads a controller's USB
/// ids with it) fills it.
#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct CapabilitiesEx {
    caps: windows_sys::Win32::UI::Input::XboxController::XINPUT_CAPABILITIES,
    vendor: u16,
    product: u16,
    version: u16,
    _unknown1: u16,
    _unknown2: u32,
}

#[cfg(windows)]
type GetCapabilitiesEx = unsafe extern "system" fn(u32, u32, u32, *mut CapabilitiesEx) -> u32;

/// A controller XInput has: its user index, what it holds, and its USB
/// vendor and product when XInput says.
#[cfg(windows)]
pub struct Found {
    pub index: u32,
    pub pad: XPad,
    pub ids: Option<(u16, u16)>,
}

/// XInput's four user indices.
#[cfg(windows)]
pub struct XInput {
    /// each index connected when last asked, and when that was
    connected: [bool; 4],
    asked: [Option<std::time::Instant>; 4],
    /// each one's USB vendor and product, when XInput says
    ids: [Option<(u16, u16)>; 4],
    capabilities_ex: Option<GetCapabilitiesEx>,
    /// the motors this window set going on each, and until when
    running: [Option<std::time::Instant>; 4],
}

#[cfg(windows)]
impl Default for XInput {
    fn default() -> Self {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
        let name: Vec<u16> = "xinput1_4.dll\0".encode_utf16().collect();
        // SAFETY: the library the program links already, and its export 108
        // by ordinal, `XInputGetCapabilitiesEx`'s shape since Windows 8
        let capabilities_ex = unsafe {
            let m = LoadLibraryW(name.as_ptr());
            (!m.is_null()).then(|| GetProcAddress(m, 108usize as *const u8)).flatten().map(|f| std::mem::transmute::<unsafe extern "system" fn() -> isize, GetCapabilitiesEx>(f))
        };
        XInput { connected: [false; 4], asked: [None; 4], ids: [None; 4], capabilities_ex, running: [None; 4] }
    }
}

#[cfg(windows)]
impl XInput {
    /// The controllers connected, by user index, each with its USB vendor
    /// and product when XInput says. An index with none is asked again only
    /// once a second: XInput is slow to answer for an empty one, and the
    /// client asks every frame. A motor whose time is up stops.
    pub fn read(&mut self) -> Vec<Found> {
        use windows_sys::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};
        let now = std::time::Instant::now();
        let mut out = Vec::new();
        for i in 0..4u32 {
            let k = i as usize;
            if self.running[k].is_some_and(|t| now >= t) {
                self.vibrate(i, [0, 0], 0);
            }
            if !self.connected[k] && self.asked[k].is_some_and(|t| now.duration_since(t).as_secs() < 1) {
                continue;
            }
            self.asked[k] = Some(now);
            let mut s = XINPUT_STATE::default();
            // SAFETY: `s` is an XINPUT_STATE for XInput to fill
            let r = unsafe { XInputGetState(i, &mut s) };
            if r != 0 {
                self.connected[k] = false;
                self.ids[k] = None;
                continue;
            }
            if !self.connected[k]
                && let Some(f) = self.capabilities_ex
            {
                let mut c = CapabilitiesEx::default();
                // SAFETY: `c` is the XINPUT_CAPABILITIES_EX it fills
                self.ids[k] = (unsafe { f(1, i, 0, &mut c) } == 0).then_some((c.vendor, c.product));
            }
            self.connected[k] = true;
            let g = s.Gamepad;
            let pad = XPad { buttons: g.wButtons, lt: g.bLeftTrigger, rt: g.bRightTrigger, lx: g.sThumbLX, ly: g.sThumbLY, rx: g.sThumbRX, ry: g.sThumbRY };
            out.push(Found { index: i, pad, ids: self.ids[k] });
        }
        out
    }

    /// Index `i`'s motors as `VIPad` set them, for at most `ms`: the large
    /// (left, low) one at `act[1]`'s level, the small (right, high) one on or
    /// off; both 0 stops them.
    pub fn vibrate(&mut self, i: u32, act: [u8; 2], ms: u32) {
        use windows_sys::Win32::UI::Input::XboxController::{XINPUT_VIBRATION, XInputSetState};
        let Some(k) = (i < 4).then_some(i as usize) else { return };
        let v = XINPUT_VIBRATION { wLeftMotorSpeed: act[1] as u16 * 257, wRightMotorSpeed: if act[0] != 0 { u16::MAX } else { 0 } };
        // SAFETY: `v` is an XINPUT_VIBRATION for XInput to read
        unsafe { XInputSetState(i, &v) };
        self.running[k] = (act != [0, 0]).then(|| std::time::Instant::now() + std::time::Duration::from_millis(ms as u64));
    }

    /// Every motor this window set going, stopped.
    pub fn stop(&mut self) {
        for i in 0..4u32 {
            if self.running[i as usize].is_some() {
                self.vibrate(i, [0, 0], 0);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for XInput {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each XInput button lands on libpad's by position, the triggers count
    /// past a third of their travel, and the sticks run down for up. The
    /// control: a button XInput does not define (0x0400, the guide's bit on
    /// some drivers) is nothing.
    #[test]
    fn an_xbox_pad_reads_as_libpads_report() {
        for (x, b) in BUTTONS {
            assert_eq!(XPad { buttons: x, ..Default::default() }.report().buttons, b, "{x:#06x}");
        }
        assert_eq!(XPad { buttons: 0x0400, ..Default::default() }.report().buttons, 0, "the control");
        let t = |lt: u8, rt: u8| XPad { lt, rt, ..Default::default() }.report().buttons;
        assert_eq!((t(84, 85), t(85, 84)), (bit::R2, bit::L2), "a third of 255 is 84.15");
        let r = XPad { lx: i16::MIN, ly: i16::MAX, rx: 16384, ry: -16384, ..Default::default() }.report();
        assert_eq!(r.sticks[2..], [-1.0, -1.0], "left stick: full left, full up is -1");
        assert!((r.sticks[0] - 0.5).abs() < 0.001 && (r.sticks[1] - 0.5).abs() < 0.001, "{:?}", r.sticks);
        // through the game's own pad path, as gilrs's reading of it went
        assert_eq!(XPad::default().report(), Report::default());
    }
}
