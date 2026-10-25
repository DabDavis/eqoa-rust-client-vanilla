//! PlayStation controllers on Windows, read over HID (`input::Gamepads`).
//!
//! Windows reads a DualShock 4 or a DualSense through Windows.Gaming.Input,
//! which answers only the program whose window is in front, and XInput does
//! not know them (`xinput`): two windows of the client could not each read
//! its own PlayStation controller (F3 20261024-165502, "cant use 2 different
//! controllers to control each box"). A HID device opened for reading
//! answers every program that opened it, whichever window is in front, each
//! with its own copy of every input report. So the client reads them itself:
//! the DualShock 4 (054C:05C4, 09CC, and the 0BA0 wireless adapter) and the
//! DualSense (054C:0CE6, the Edge 0DF2), over USB or Bluetooth, their input
//! reports as SDL's HIDAPI drivers lay them out (`SDL_hidapi_ps4.c`,
//! `SDL_hidapi_ps5.c`). gilrs's reading of the same controller is passed
//! over (by USB vendor and product, as for XInput). A controller that cannot
//! be opened (another program holds it alone) stays gilrs's.
//!
//! Its motors are set here too (`Hid::rumble`, `rumble_report`): Windows.
//! Gaming.Input takes a PlayStation controller's vibration and does nothing
//! with it, so the game's rumble never reached one (F3 pass 16: "I get no
//! vibration on my controller (ps4)"). Nothing else is sent: a controller is
//! written to only once it is to rumble, its light and its sound are left as
//! they are, and a Bluetooth one stays in the reduced reports it starts in
//! (they hold every button, the triggers and the sticks) until its first
//! rumble, which turns it to its full reports, as SDL and Steam do.

use crate::input::{Latch, Report, bit};

/// The controllers read here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    DualShock4,
    DualSense,
    DualSenseEdge,
}

const SONY: u16 = 0x054C;

/// The DualShock 4's wireless adapter: a controller only while one answers
/// through it.
pub const DS4_ADAPTER: u16 = 0x0BA0;

impl Model {
    /// The model of a USB vendor and product, if one read here.
    pub fn of(vendor: u16, product: u16) -> Option<Model> {
        match (vendor, product) {
            (SONY, 0x05C4 | 0x09CC | DS4_ADAPTER) => Some(Model::DualShock4),
            (SONY, 0x0CE6) => Some(Model::DualSense),
            (SONY, 0x0DF2) => Some(Model::DualSenseEdge),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Model::DualShock4 => "DualShock 4",
            Model::DualSense => "DualSense",
            Model::DualSenseEdge => "DualSense Edge",
        }
    }
}

/// What a controller holds, as its input reports have it: the sticks (left
/// x and y, right x and y, each 0 up or left to 255 down or right, about 128
/// at rest), the triggers' travel (0..255), and the three button bytes (the
/// d-pad's hat in the first one's low four bits: 0 up, then clockwise, 8 at
/// rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PsPad {
    pub sticks: [u8; 4],
    pub triggers: [u8; 2],
    pub buttons: [u8; 3],
}

impl Default for PsPad {
    /// At rest.
    fn default() -> Self {
        PsPad { sticks: [128; 4], triggers: [0; 2], buttons: [8, 0, 0] }
    }
}

/// The first button byte's face buttons, each as libpad's (by position).
const FACE: [(u8, u16); 4] = [(0x10, bit::SQUARE), (0x20, bit::CROSS), (0x40, bit::CIRCLE), (0x80, bit::TRIANGLE)];

/// The hat's eight ways, up first, then clockwise.
const HAT: [u16; 8] = [bit::UP, bit::UP | bit::RIGHT, bit::RIGHT, bit::DOWN | bit::RIGHT, bit::DOWN, bit::DOWN | bit::LEFT, bit::LEFT, bit::UP | bit::LEFT];

/// The second button byte (its 0x04 and 0x08 are the triggers', read from
/// their travel instead): Share or Create is Select, Options Start.
const MIDDLE: [(u8, u16); 6] = [(0x01, bit::L1), (0x02, bit::R1), (0x10, bit::SELECT), (0x20, bit::START), (0x40, bit::L3), (0x80, bit::R3)];

impl PsPad {
    /// As libpad's report, as gilrs's reading of the same controller is
    /// (`input::report_of`, in SDL's layout): the face buttons by position,
    /// the hat's ways (a diagonal both), the shoulders, Share and Options as
    /// Select and Start, the stick buttons, the triggers past a third of
    /// their travel as L2 and R2, and the sticks in -1..1 with +y down, as
    /// SDL scales them. The PS button, the touch pad's click and the
    /// DualSense's microphone button are none of the game's, as through
    /// gilrs.
    pub fn report(&self) -> Report {
        let mut r = Report::default();
        let [b0, b1, _] = self.buttons;
        for (m, k) in FACE {
            if b0 & m != 0 {
                r.buttons |= k;
            }
        }
        r.buttons |= HAT.get((b0 & 0x0F) as usize).copied().unwrap_or(0);
        for (m, k) in MIDDLE {
            if b1 & m != 0 {
                r.buttons |= k;
            }
        }
        if self.triggers[0] as f32 / 255.0 > 0.33 {
            r.buttons |= bit::L2;
        }
        if self.triggers[1] as f32 / 255.0 > 0.33 {
            r.buttons |= bit::R2;
        }
        let axis = |v: u8| ((v as i32 * 257 - 32768) as f32 / 32767.0).clamp(-1.0, 1.0);
        let [lx, ly, rx, ry] = self.sticks;
        r.sticks = [axis(rx), axis(ry), axis(lx), axis(ly)];
        r
    }
}

/// A DualShock 4's state at `p` (SDL's `PS4StatePacket_t`: the sticks, the
/// three button bytes, the triggers), with SDL's rule for a trigger whose
/// button says pressed with no travel (some fight sticks): all the way.
fn ds4_state(p: &[u8]) -> Option<PsPad> {
    let p = p.get(..9)?;
    let mut triggers = [p[7], p[8]];
    if p[5] & 0x04 != 0 && triggers[0] == 0 {
        triggers[0] = 255;
    }
    if p[5] & 0x08 != 0 && triggers[1] == 0 {
        triggers[1] = 255;
    }
    Some(PsPad { sticks: [p[0], p[1], p[2], p[3]], triggers, buttons: [p[4], p[5], p[6]] })
}

/// A DualSense's state at `p` (SDL's `PS5StatePacketCommon_t`: the sticks,
/// the triggers, a counter, the button bytes).
fn ds5_state(p: &[u8]) -> Option<PsPad> {
    let p = p.get(..10)?;
    Some(PsPad { sticks: [p[0], p[1], p[2], p[3]], triggers: [p[4], p[5]], buttons: [p[7], p[8], p[9]] })
}

/// A DualSense's reduced state at `p` (`PS5SimpleStatePacket_t`, the
/// DualShock 4's order: the sticks, the button bytes, the triggers).
fn ds5_simple(p: &[u8]) -> Option<PsPad> {
    let p = p.get(..9)?;
    Some(PsPad { sticks: [p[0], p[1], p[2], p[3]], triggers: [p[7], p[8]], buttons: [p[4], p[5], p[6]] })
}

/// A Bluetooth input report's CRC-32 (its last four bytes, little-endian),
/// over the HID header 0xA1 and the rest of the report.
fn crc_ok(report: &[u8]) -> bool {
    let Some(at) = report.len().checked_sub(4) else { return false };
    let (body, crc) = report.split_at(at);
    let mut h = flate2::Crc::new();
    h.update(&[0xA1]);
    h.update(body);
    crc.try_into().is_ok_and(|c| h.sum() == u32::from_le_bytes(c))
}

/// The motors a controller is told to run (`rumble_report`): the large one's
/// level and the small one's, each 0..255, as `VIPad`'s two actuators are a
/// DualShock 2's (the large one at a level, the small one on or off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motors {
    pub large: u8,
    pub small: u8,
}

impl Motors {
    pub const OFF: Motors = Motors { large: 0, small: 0 };

    /// `VIPad`'s `m_act`: [0] the small motor on or off, [1] the large one's
    /// level.
    pub fn of_act(act: [u8; 2]) -> Motors {
        Motors { large: act[1], small: if act[0] != 0 { 255 } else { 0 } }
    }
}

/// The DualSense firmware that has the newer rumble (Linux's
/// `hid-playstation`: "feature version 2.21 introduced new vibration
/// method"); the Edge has it whatever its firmware says.
pub const DUALSENSE_RUMBLE_V2: u16 = 0x0215;

/// The output report that sets a controller's motors and nothing else (its
/// light and its sound left as they are), as Linux's `hid-playstation` and
/// SDL's HIDAPI drivers lay it out. The DualShock 4: report 0x05 over USB, 32
/// bytes, its flags' 0x01 (the motors), the small motor at 4 and the large
/// at 5; over Bluetooth report 0x11, 78 bytes, two control bytes first (0xC0:
/// HID data and a CRC follow, 4: the 4 ms report interval SDL asks for), so
/// the flags at 3 and the motors at 6 and 7. The DualSense: report 0x02 over
/// USB, 48 bytes, the motors at 3 and 4; over Bluetooth 0x31, 78 bytes, a
/// sequence number in the top half of byte 1 (`seq`) and the tag 0x10 at 2,
/// so its block starts at 3. Its block's flags choose the rumble: the newer
/// one (`v2`, flag 0x04 at the block's 38 and "haptics select" 0x02 at 0),
/// or the first firmware's (0x01 | 0x02 at 0). A Bluetooth report ends in
/// the CRC-32 of 0xA2 (the HID header) and its other 74 bytes.
pub fn rumble_report(model: Model, bluetooth: bool, m: Motors, seq: u8, v2: bool) -> Vec<u8> {
    let mut r;
    match model {
        Model::DualShock4 => {
            if bluetooth {
                r = vec![0u8; 78];
                r[0] = 0x11;
                r[1] = 0xC0 | 4;
                r[3] = 0x01;
                r[6] = m.small;
                r[7] = m.large;
            } else {
                r = vec![0u8; 32];
                r[0] = 0x05;
                r[1] = 0x01;
                r[4] = m.small;
                r[5] = m.large;
            }
        }
        Model::DualSense | Model::DualSenseEdge => {
            let at = if bluetooth { 3 } else { 1 };
            r = vec![0u8; if bluetooth { 78 } else { 48 }];
            r[0] = if bluetooth { 0x31 } else { 0x02 };
            if bluetooth {
                r[1] = (seq & 0x0F) << 4;
                r[2] = 0x10;
            }
            if v2 || model == Model::DualSenseEdge {
                r[at] = 0x02;
                r[at + 38] = 0x04;
            } else {
                r[at] = 0x01 | 0x02;
            }
            r[at + 2] = m.small;
            r[at + 3] = m.large;
        }
    }
    if bluetooth {
        let n = r.len() - 4;
        let mut h = flate2::Crc::new();
        h.update(&[0xA2]);
        h.update(&r[..n]);
        let crc = h.sum().to_le_bytes();
        r[n..].copy_from_slice(&crc);
    }
    r
}

/// A DualSense's firmware version out of its firmware feature report (0x20,
/// read with its ID first): bytes 44 and 45, little-endian (SDL's and
/// `hid-playstation`'s reading); None for a report too short.
pub fn dualsense_firmware(feature: &[u8]) -> Option<u16> {
    (feature.first() == Some(&0x20) && feature.len() >= 46).then(|| u16::from_le_bytes([feature[44], feature[45]]))
}

/// What an input report holds, as SDL's drivers read it (`IsPacketValid`
/// and `UpdateDevice`): `data` as a read returns it, the report ID first
/// (Windows returns the collection's longest input report, the rest zeros),
/// from a controller on Bluetooth or not (`bluetooth`). None for a report
/// that holds no state: another report, the adapter with no controller (byte
/// 31's 0x04), a Bluetooth report without its HID data or, a DualSense's,
/// failing its CRC.
///
/// A DualSense's report 0x01 is its full state over USB and its reduced one
/// over Bluetooth, told apart by the link as Linux's `hid-playstation` tells
/// them apart by the bus: SDL's rule, the read's length (10, or 78 as
/// Windows pads it), took two DualSenses on Bluetooth for full ones on a
/// Windows machine (F3 20261024-204603: the d-pad held up, triangle read as
/// L2, the shoulders at random: the reduced report's bytes in the full one's
/// places), so the length decides only off Bluetooth.
pub fn parse(model: Model, bluetooth: bool, data: &[u8]) -> Option<PsPad> {
    let n = data.len();
    match (model, *data.first()?) {
        // USB, and Bluetooth's reduced report
        (Model::DualShock4, 0x01) if n == 10 || (n >= 64 && data[31] & 0x04 == 0) => ds4_state(&data[1..]),
        // Bluetooth's full reports: two bytes more, the first saying the
        // HID data is there
        (Model::DualShock4, 0x11..=0x19) if n >= 78 && data[1] & 0x80 != 0 => ds4_state(&data[3..]),
        // Bluetooth's reduced report, at any length a read pads it to (and
        // SDL's 10 or 78 where the link is not known to be Bluetooth)
        (Model::DualSense | Model::DualSenseEdge, 0x01) if bluetooth || n == 10 || n == 78 => ds5_simple(&data[1..]),
        (Model::DualSense | Model::DualSenseEdge, 0x01) => ds5_state(&data[1..]),
        // Bluetooth's full report: a byte more, and a CRC
        (Model::DualSense | Model::DualSenseEdge, 0x31) if n >= 78 && crc_ok(&data[..78]) => ds5_state(&data[2..]),
        _ => None,
    }
}

/// A report as a read returned it, on what the controller last said
/// (`pad`) and the buttons it pressed since a window last read it (`latch`):
/// its state, if it holds one (`parse`), is the controller's now, and its
/// buttons are kept for that read, so one pressed and let go before it
/// still reaches the game (`Latch`).
pub fn take(model: Model, bluetooth: bool, data: &[u8], pad: &mut PsPad, latch: &mut Latch) -> Option<PsPad> {
    let p = parse(model, bluetooth, data)?;
    *pad = p;
    latch.saw(p.report().buttons);
    Some(p)
}

/// A report as a controller first sent it, for the log and a problem
/// report: its ID, the length the read returned, and its first bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstReport {
    pub id: u8,
    pub len: usize,
    pub head: Vec<u8>,
    /// whether it held a state (`parse`)
    pub state: bool,
}

/// How many of a report's bytes `FirstReport` keeps: the ID and the state's
/// first eleven (a DualSense's reduced report is ten in all).
const HEAD: usize = 12;

impl FirstReport {
    pub fn of(data: &[u8], state: bool) -> Option<FirstReport> {
        Some(FirstReport { id: *data.first()?, len: data.len(), head: data[..data.len().min(HEAD)].to_vec(), state })
    }

    /// As the log and a problem report say it: `report 0x01, 78 bytes: 01 80
    /// 7f ..` (and "no state" for one `parse` read nothing in).
    pub fn describe(&self) -> String {
        let bytes: Vec<String> = self.head.iter().map(|b| format!("{b:02x}")).collect();
        format!("report {:#04x}, {} bytes{}: {}", self.id, self.len, if self.state { "" } else { " (no state)" }, bytes.join(" "))
    }
}

/// A controller's name among the client's windows (`multibox`), the same in
/// every program that reads it: its USB ids and a hash (FNV-1a) of its HID
/// interface's path, which Windows keeps while it stays on the same port
/// (USB) or paired (Bluetooth).
pub fn name(path: &str, ids: (u16, u16)) -> String {
    let mut h: u32 = 0x811C_9DC5;
    for b in path.bytes().map(|b| b.to_ascii_lowercase()) {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    format!("hid-{:04x}-{:04x}-{h:08x}", ids.0, ids.1)
}

/// Whether a HID interface's path is a Bluetooth one (the HID profile's
/// service class).
pub fn bluetooth(path: &str) -> bool {
    path.to_ascii_lowercase().contains("{00001124-0000-1000-8000-00805f9b34fb}")
}

#[cfg(windows)]
pub use device::Hid;

/// A Mac's PlayStation controllers' motors (`Gamepads::rumble`): gilrs reads
/// them through IOKit and has no force feedback there, so their motors are
/// set here, by `IOHIDDeviceSetReport` with `rumble_report`'s bytes. Over
/// USB only: a Bluetooth one's first output report turns it to its full
/// reports, which gilrs's reading (the HID descriptor's report 0x01) cannot
/// read, and the controller would stop answering the game. One thread owns
/// IOKit's side (its objects stay on it): each ask written to every USB
/// controller with those USB ids (looked for again at most once a second),
/// the motors stopped when their time is up or the program ends.
#[cfg(target_os = "macos")]
pub mod mac {
    use super::{DUALSENSE_RUMBLE_V2, Model, Motors, dualsense_firmware, rumble_report};
    use objc2_core_foundation::{CFNumber, CFRetained, CFString};
    use objc2_io_kit::{IOHIDDevice, IOHIDManager, IOHIDReportType, kIOHIDOptionsTypeNone, kIOHIDProductIDKey, kIOHIDTransportKey, kIOHIDVendorIDKey, kIOReturnSuccess};
    use std::ptr::NonNull;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// How long past its time a motor runs before the thread stops it
    /// itself (the game stops it on time while its frames come).
    const GRACE_MS: u64 = 100;

    /// The thread, and what it is asked: a controller's USB ids, its motors,
    /// for at most this many ms.
    pub struct Mac {
        tx: mpsc::Sender<((u16, u16), Motors, u32)>,
        done: mpsc::Receiver<()>,
    }

    impl Mac {
        pub fn start() -> Option<Mac> {
            let (tx, rx) = mpsc::channel();
            let (ended, done) = mpsc::channel();
            std::thread::Builder::new()
                .name("hid motors".into())
                .spawn(move || {
                    run(&rx);
                    let _ = ended.send(());
                })
                .map_err(|e| crate::say(&format!("gamepad: no thread for the motors: {e}")))
                .ok()?;
            Some(Mac { tx, done })
        }

        /// The motors of the USB PlayStation controllers with these ids run
        /// as `m`, for at most `ms`.
        pub fn rumble(&self, ids: (u16, u16), m: Motors, ms: u32) {
            let _ = self.tx.send((ids, m, ms));
        }
    }

    impl Drop for Mac {
        /// Every motor stopped as the program ends (a quarter of a second at
        /// most).
        fn drop(&mut self) {
            let (gone, _) = mpsc::channel();
            drop(std::mem::replace(&mut self.tx, gone));
            let _ = self.done.recv_timeout(Duration::from_millis(250));
        }
    }

    /// A USB PlayStation controller opened to be written to.
    struct Pad {
        dev: CFRetained<IOHIDDevice>,
        ids: (u16, u16),
        model: Model,
        v2: bool,
        /// what it was last told, and until when
        on: Motors,
        until: Option<Instant>,
    }

    fn number(dev: &IOHIDDevice, key: &std::ffi::CStr) -> Option<i32> {
        let key = CFString::from_str(key.to_str().ok()?);
        dev.property(&key)?.downcast::<CFNumber>().ok()?.as_i32()
    }

    fn text(dev: &IOHIDDevice, key: &std::ffi::CStr) -> Option<String> {
        let key = CFString::from_str(key.to_str().ok()?);
        Some(dev.property(&key)?.downcast::<CFString>().ok()?.to_string())
    }

    /// A DualSense's firmware version (its feature report 0x20).
    fn firmware(dev: &IOHIDDevice) -> Option<u16> {
        let mut buf = [0u8; 64];
        buf[0] = 0x20;
        let mut len: objc2_core_foundation::CFIndex = buf.len() as _;
        // SAFETY: a buffer and its length, both alive for the call
        let r = unsafe { dev.report(IOHIDReportType::Feature, 0x20, NonNull::new(buf.as_mut_ptr())?, NonNull::from(&mut len)) };
        if r == kIOReturnSuccess { dualsense_firmware(&buf[..(len.max(0) as usize).min(64)]) } else { None }
    }

    /// The USB PlayStation controllers present now, each opened (not
    /// alone: gilrs reads it too).
    fn look() -> Vec<Pad> {
        let manager = IOHIDManager::new(None, kIOHIDOptionsTypeNone);
        // SAFETY: no dictionary: every device, the Sony ones kept below
        unsafe { manager.set_device_matching(None) };
        if manager.open(kIOHIDOptionsTypeNone) != kIOReturnSuccess {
            return Vec::new();
        }
        let Some(set) = manager.devices() else { return Vec::new() };
        let n = set.count().max(0) as usize;
        let mut values: Vec<*const std::ffi::c_void> = vec![std::ptr::null(); n];
        // SAFETY: room for every value of the set
        unsafe { set.values(values.as_mut_ptr()) };
        let mut out = Vec::new();
        for v in values {
            // SAFETY: the manager's set holds IOHIDDevices, alive while it is;
            // each kept is retained
            let Some(dev) = NonNull::new(v as *mut IOHIDDevice).map(|p| unsafe { CFRetained::retain(p) }) else { continue };
            let (Some(vendor), Some(product)) = (number(&dev, kIOHIDVendorIDKey), number(&dev, kIOHIDProductIDKey)) else { continue };
            let ids = (vendor as u16, product as u16);
            let Some(model) = Model::of(ids.0, ids.1) else { continue };
            if text(&dev, kIOHIDTransportKey).is_none_or(|t| t != "USB") {
                continue;
            }
            if dev.open(kIOHIDOptionsTypeNone) != kIOReturnSuccess {
                continue;
            }
            let v2 = model != Model::DualSense || firmware(&dev).is_none_or(|v| v >= DUALSENSE_RUMBLE_V2);
            out.push(Pad { dev, ids, model, v2, on: Motors::OFF, until: None });
        }
        out
    }

    fn write(p: &mut Pad, m: Motors) {
        let mut r = rumble_report(p.model, false, m, 0, p.v2);
        let Some(ptr) = NonNull::new(r.as_mut_ptr()) else { return };
        // SAFETY: the report's bytes, alive for the call (it is synchronous)
        let res = unsafe { p.dev.set_report(IOHIDReportType::Output, r[0] as _, ptr, r.len() as _) };
        if res != kIOReturnSuccess && p.on == Motors::OFF {
            crate::say(&format!("gamepad: a {} took no report of its motors (IOKit {res:#x})", p.model.label()));
        }
        p.on = m;
    }

    fn run(rx: &mpsc::Receiver<((u16, u16), Motors, u32)>) {
        let mut pads: Vec<Pad> = Vec::new();
        let mut looked: Option<Instant> = None;
        let mut said = false;
        loop {
            let next = pads.iter().filter_map(|p| p.until).min();
            let ask = match next {
                Some(t) => match rx.recv_timeout(t.saturating_duration_since(Instant::now())) {
                    Ok(a) => Some(a),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() {
                    Ok(a) => Some(a),
                    Err(_) => break,
                },
            };
            let now = Instant::now();
            // the motors whose time is up
            for p in pads.iter_mut().filter(|p| p.until.is_some_and(|t| t <= now)) {
                write(p, Motors::OFF);
                p.until = None;
            }
            let Some((ids, m, ms)) = ask else { continue };
            if m != Motors::OFF && looked.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1)) {
                // the ones still running carried over, the new ones taken
                let mut fresh = look();
                for f in fresh.iter_mut() {
                    if let Some(old) = pads.iter().find(|p| p.ids == f.ids && p.on != Motors::OFF) {
                        (f.on, f.until) = (old.on, old.until);
                    }
                }
                pads = fresh;
                looked = Some(now);
                if !said && pads.iter().any(|p| p.ids == ids) {
                    crate::say(&format!("gamepad: {} USB PlayStation controller(s) rumble over IOKit", pads.len()));
                    said = true;
                }
            }
            for p in pads.iter_mut().filter(|p| p.ids == ids) {
                if p.on != m {
                    write(p, m);
                }
                p.until = (m != Motors::OFF).then(|| now + Duration::from_millis(ms as u64 + GRACE_MS));
            }
        }
        for p in pads.iter_mut().filter(|p| p.on != Motors::OFF) {
            write(p, Motors::OFF);
        }
    }
}

/// Windows' side: the HID interfaces present, looked through once a second on
/// a thread of its own (a walk of the system's device lists takes
/// milliseconds), and each controller's reports read on the game's thread
/// with an overlapped read drained every frame, the last one kept.
#[cfg(windows)]
mod device {
    use super::{FirstReport, Model, PsPad, bluetooth, name, take};
    use crate::input::Latch;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{HIDD_ATTRIBUTES, HidD_GetAttributes, HidD_GetHidGuid};
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_DEV_NOT_EXIST, ERROR_DEVICE_NOT_CONNECTED, ERROR_FILE_NOT_FOUND, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_NO_SUCH_DEVICE, ERROR_PATH_NOT_FOUND, GENERIC_READ, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile};
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED};

    /// A Sony controller's HID interface present now, and whether it is a
    /// Bluetooth one (`bluetooth`: its reports are read by it, `parse`).
    #[derive(Clone, Debug, PartialEq)]
    struct Listed {
        path: String,
        ids: (u16, u16),
        model: Model,
        bluetooth: bool,
    }

    /// Every HID interface present, by its path.
    fn interfaces() -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: HidD_GetHidGuid fills the GUID; each interface's detail is
        // read into a buffer of the size SetupAPI asked for, aligned as its
        // struct; the set is destroyed at the end
        unsafe {
            let mut guid: windows_sys::core::GUID = std::mem::zeroed();
            HidD_GetHidGuid(&mut guid);
            let set = SetupDiGetClassDevsW(&guid, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
            if set == -1 {
                return out;
            }
            for i in 0u32.. {
                let mut d = SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
                if SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &guid, i, &mut d) == 0 {
                    break;
                }
                let mut need = 0u32;
                SetupDiGetDeviceInterfaceDetailW(set, &d, std::ptr::null_mut(), 0, &mut need, std::ptr::null_mut());
                if !(8..=65536).contains(&need) {
                    continue;
                }
                let mut buf = vec![0u32; (need as usize).div_ceil(4)];
                let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &d, detail, need, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                    continue;
                }
                // the path's characters follow cbSize, to the size asked for
                let chars = std::slice::from_raw_parts(std::ptr::addr_of!((*detail).DevicePath) as *const u16, (need as usize - 4) / 2);
                let len = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
                out.push(String::from_utf16_lossy(&chars[..len]));
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    /// A HID interface opened, shared with every other program.
    fn open(path: &str, access: u32, flags: u32) -> Option<HANDLE> {
        let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
        // SAFETY: a nul-terminated path; no security attributes, no template
        let h = unsafe { CreateFileW(w.as_ptr(), access, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, flags, std::ptr::null_mut()) };
        (h != INVALID_HANDLE_VALUE && !h.is_null()).then_some(h)
    }

    /// A HID interface's USB vendor and product, asked of it opened for
    /// neither reading nor writing (as any program may, a keyboard's too).
    fn ids(path: &str) -> Option<(u16, u16)> {
        let h = open(path, 0, 0)?;
        let mut a = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, VendorID: 0, ProductID: 0, VersionNumber: 0 };
        // SAFETY: an open HID handle and the attributes it fills; then closed
        let ok = unsafe { HidD_GetAttributes(h, &mut a) };
        unsafe { CloseHandle(h) };
        ok.then_some((a.VendorID, a.ProductID))
    }

    /// Each HID interface's USB ids, asked once while it stays (a failed ask
    /// again after 5 s).
    type Known = HashMap<String, (Option<(u16, u16)>, Instant)>;

    /// The controllers present now, in their interfaces' order.
    fn look_once(known: &mut Known) -> Vec<Listed> {
        let now = interfaces();
        known.retain(|p, _| now.contains(p));
        let mut found = Vec::new();
        for p in now {
            let stale = known.get(&p).is_none_or(|(ids, at)| ids.is_none() && at.elapsed() >= Duration::from_secs(5));
            if stale {
                known.insert(p.clone(), (ids(&p), Instant::now()));
            }
            if let Some((Some(ids), _)) = known.get(&p)
                && let Some(model) = Model::of(ids.0, ids.1)
            {
                let bluetooth = bluetooth(&p);
                found.push(Listed { path: p, ids: *ids, model, bluetooth });
            }
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        found
    }

    /// The controllers present, once a second until `stop`.
    fn look(mut known: Known, listed: &Mutex<Vec<Listed>>, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            for _ in 0..10 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let found = look_once(&mut known);
            if let Ok(mut l) = listed.lock() {
                *l = found;
            }
        }
    }

    /// A read in flight: its buffer and OVERLAPPED stay put (boxed) until it
    /// completes or is cancelled.
    struct Pending {
        ov: OVERLAPPED,
        buf: [u8; 1024],
        busy: bool,
    }

    /// A controller open for reading, and what it last said.
    struct Device {
        listed: Listed,
        handle: HANDLE,
        read: Box<Pending>,
        pad: PsPad,
        /// the buttons it pressed since the last read (`take`)
        latch: Latch,
        /// when a report last held its state
        valid: Option<Instant>,
        /// the first report of each ID it sent (`FirstReport`), each said
        /// once in the log
        first: Vec<FirstReport>,
        /// its motors' writer, from the first time they are to run
        motors: Option<writer::Writer>,
    }

    impl Device {
        fn open(l: &Listed) -> Option<Device> {
            let handle = open(&l.path, GENERIC_READ, FILE_FLAG_OVERLAPPED)?;
            Some(Device { listed: l.clone(), handle, read: Box::new(Pending { ov: OVERLAPPED::default(), buf: [0; 1024], busy: false }), pad: PsPad::default(), latch: Latch::default(), valid: None, first: Vec::new(), motors: None })
        }

        /// Every report waiting read, the last state kept and every state's
        /// buttons latched (`take`); false when the controller is gone (a
        /// read failed).
        fn pump(&mut self) -> bool {
            // the driver keeps a few dozen reports for a reader: a bound past them
            for _ in 0..256 {
                let r = &mut *self.read;
                if !r.busy {
                    r.ov = OVERLAPPED::default();
                    // as hidapi does: what the driver does not write of a
                    // read is zero, not the last report's
                    r.buf.fill(0);
                    // SAFETY: the buffer and OVERLAPPED are boxed and outlive
                    // the read (`Drop` cancels a pending one and waits)
                    let ok = unsafe { ReadFile(self.handle, r.buf.as_mut_ptr(), r.buf.len() as u32, std::ptr::null_mut(), &mut r.ov) };
                    if ok == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
                        return false;
                    }
                    r.busy = true;
                }
                let mut n = 0u32;
                // SAFETY: the read just made, or still pending; no wait
                if unsafe { GetOverlappedResult(self.handle, &r.ov, &mut n, 0) } == 0 {
                    return unsafe { GetLastError() } == ERROR_IO_INCOMPLETE;
                }
                r.busy = false;
                let data = &r.buf[..(n as usize).min(r.buf.len())];
                let state = take(self.listed.model, self.listed.bluetooth, data, &mut self.pad, &mut self.latch);
                if state.is_some() {
                    self.valid = Some(Instant::now());
                }
                if let Some(f) = FirstReport::of(data, state.is_some())
                    && !self.first.iter().any(|g| g.id == f.id)
                {
                    crate::say(&format!("gamepad: {} {}{}: {}", self.listed.model.label(), name(&self.listed.path, self.listed.ids), if self.listed.bluetooth { " on Bluetooth" } else { "" }, f.describe()));
                    self.first.push(f);
                }
            }
            true
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: a pending read cancelled and waited for (half a second
            // at most) before its box goes; one the driver has not let go of
            // by then keeps its box for good, as the program ends without
            // waiting on it; then the handle closed
            unsafe {
                if self.read.busy {
                    CancelIoEx(self.handle, &self.read.ov);
                    let mut n = 0u32;
                    if GetOverlappedResultEx(self.handle, &self.read.ov, &mut n, 500, 0) == 0 && GetLastError() == WAIT_TIMEOUT {
                        std::mem::forget(std::mem::replace(&mut self.read, Box::new(Pending { ov: OVERLAPPED::default(), buf: [0; 1024], busy: false })));
                    }
                }
                CloseHandle(self.handle);
            }
        }
    }

    /// A controller's motors (`Hid::rumble`), set on a thread of its own
    /// over a handle of their own: a write can take milliseconds (on
    /// Bluetooth more), and the game's frames do not wait on it.
    mod writer {
        use super::super::{DUALSENSE_RUMBLE_V2, Model, Motors, dualsense_firmware, rumble_report};
        use super::{Listed, name, open};
        use std::sync::mpsc;
        use std::time::{Duration, Instant};
        use windows_sys::Win32::Devices::HumanInterfaceDevice::{HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData, HidD_GetFeature, HidD_GetPreparsedData, HidD_SetOutputReport, HidP_GetCaps};
        use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, GetLastError, HANDLE};
        use windows_sys::Win32::Storage::FileSystem::WriteFile;

        /// How long past its time a motor runs before the writer stops it
        /// itself: the game stops it on time while its frames come
        /// (`VIPad::Process`), this when they do not (the game ended, or
        /// held up).
        const GRACE_MS: u64 = 100;

        enum Ask {
            /// the motors as they are to run, for at most this many ms
            Run(Motors, u32),
            Stop,
        }

        /// The thread that sets one controller's motors.
        pub struct Writer {
            tx: mpsc::Sender<Ask>,
            /// told as the thread ends (its last report written)
            done: mpsc::Receiver<()>,
        }

        impl Writer {
            pub fn start(l: &Listed) -> Option<Writer> {
                let (tx, rx) = mpsc::channel();
                let (ended, done) = mpsc::channel();
                let own = l.clone();
                let spawned = std::thread::Builder::new().name("hid motors".into()).spawn(move || {
                    run(&own, &rx);
                    let _ = ended.send(());
                });
                match spawned {
                    Ok(_) => Some(Writer { tx, done }),
                    Err(e) => {
                        crate::say(&format!("gamepad: no thread for a {}'s motors: {e}", l.model.label()));
                        None
                    }
                }
            }

            pub fn run(&self, m: Motors, ms: u32) {
                let _ = self.tx.send(if m == Motors::OFF { Ask::Stop } else { Ask::Run(m, ms) });
            }

            pub fn stop(&self) {
                let _ = self.tx.send(Ask::Stop);
            }
        }

        impl Drop for Writer {
            /// The motors stopped as the controller is let go or the program
            /// ends, waited for a moment (a quarter of a second at most).
            fn drop(&mut self) {
                let (gone, _) = mpsc::channel();
                drop(std::mem::replace(&mut self.tx, gone));
                let _ = self.done.recv_timeout(Duration::from_millis(250));
            }
        }

        /// The interface's capabilities, as its HID descriptor gives them
        /// (the longest output report a write must be).
        fn caps(h: HANDLE) -> Option<HIDP_CAPS> {
            let mut pp = 0;
            // SAFETY: an open HID handle; its preparsed data read, then freed
            unsafe {
                if !HidD_GetPreparsedData(h, &mut pp) {
                    return None;
                }
                let mut c = HIDP_CAPS::default();
                let ok = HidP_GetCaps(pp, &mut c) == HIDP_STATUS_SUCCESS;
                HidD_FreePreparsedData(pp);
                ok.then_some(c)
            }
        }

        /// A DualSense's firmware version (its feature report 0x20).
        fn firmware(h: HANDLE, caps: Option<&HIDP_CAPS>) -> Option<u16> {
            let len = caps.map_or(64, |c| (c.FeatureReportByteLength as usize).max(64));
            let mut buf = vec![0u8; len];
            buf[0] = 0x20;
            // SAFETY: an open HID handle and a buffer of the length given
            let ok = unsafe { HidD_GetFeature(h, buf.as_mut_ptr().cast(), len as u32) };
            if ok { dualsense_firmware(&buf) } else { None }
        }

        /// One report written as hidapi writes one on Windows: the
        /// interface's longest output report (`out_len`), the rest zeros, by
        /// `WriteFile`; failing that, the report alone by
        /// `HidD_SetOutputReport` (what hidapi does on Windows 7, over
        /// Bluetooth). The error of the write, if both fail.
        fn write(h: HANDLE, report: &[u8], out_len: usize) -> Result<(), u32> {
            let mut buf = report.to_vec();
            if buf.len() < out_len {
                buf.resize(out_len, 0);
            }
            let mut n = 0u32;
            // SAFETY: a handle opened for writing, not overlapped; the buffer
            // outlives the call
            if unsafe { WriteFile(h, buf.as_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) } != 0 {
                return Ok(());
            }
            // SAFETY: the error the write just left
            let e = unsafe { GetLastError() };
            // SAFETY: as above, the report as it is
            if unsafe { HidD_SetOutputReport(h, report.as_ptr().cast(), report.len() as u32) } {
                return Ok(());
            }
            Err(e)
        }

        /// The thread: each ask written (one that changes nothing only moves
        /// the time it ends), the motors stopped when their time is up, and
        /// at the end.
        fn run(l: &Listed, rx: &mpsc::Receiver<Ask>) {
            let who = format!("{} {}{}", l.model.label(), name(&l.path, l.ids), if l.bluetooth { " on Bluetooth" } else { "" });
            let Some(h) = open(&l.path, GENERIC_WRITE, 0) else {
                // SAFETY: the error CreateFileW just left
                crate::say(&format!("gamepad: the {who}'s motors could not be opened over HID (error {}): it does not rumble", unsafe { GetLastError() }));
                while rx.recv().is_ok() {}
                return;
            };
            let c = caps(h);
            let out_len = c.map_or(0, |c| c.OutputReportByteLength as usize);
            let v2 = match l.model {
                // as SDL takes a DualSense whose firmware it cannot read:
                // a new one
                Model::DualSense => firmware(h, c.as_ref()).is_none_or(|v| v >= DUALSENSE_RUMBLE_V2),
                _ => true,
            };
            let mut seq = 0u8;
            let mut last = Motors::OFF;
            let mut until: Option<Instant> = None;
            let mut failed = false;
            let mut send = |m: Motors, seq: &mut u8| {
                let report = rumble_report(l.model, l.bluetooth, m, *seq, v2);
                *seq = seq.wrapping_add(1);
                match write(h, &report, out_len) {
                    Ok(()) if !failed && m != Motors::OFF && *seq == 1 => {
                        crate::say(&format!("gamepad: the {who} rumbles over HID (report {:#04x}, {} bytes, written as {})", report[0], report.len(), out_len.max(report.len())));
                    }
                    Ok(()) => {}
                    Err(e) if !failed => {
                        crate::say(&format!("gamepad: the {who} took no report of its motors (error {e})"));
                        failed = true;
                    }
                    Err(_) => {}
                }
            };
            loop {
                let ask = match until {
                    Some(t) => match rx.recv_timeout(t.saturating_duration_since(Instant::now())) {
                        Ok(a) => Some(a),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    },
                    None => match rx.recv() {
                        Ok(a) => Some(a),
                        Err(_) => break,
                    },
                };
                let (m, end) = match ask {
                    Some(Ask::Run(m, ms)) => (m, Some(Instant::now() + Duration::from_millis(ms as u64 + GRACE_MS))),
                    // stopped, or its time up
                    Some(Ask::Stop) | None => (Motors::OFF, None),
                };
                if m != last {
                    send(m, &mut seq);
                    last = m;
                }
                until = end;
            }
            if last != Motors::OFF {
                send(Motors::OFF, &mut seq);
            }
            // SAFETY: the handle opened above, no longer used
            unsafe { CloseHandle(h) };
        }
    }

    /// A controller read over HID: its name among the client's windows,
    /// model, USB ids, state, whether it is on Bluetooth, and the first
    /// report of each ID it sent (for a problem report).
    pub struct Found {
        pub name: String,
        pub model: Model,
        pub ids: (u16, u16),
        pub pad: PsPad,
        /// the buttons pressed and let go since the last read, which `pad`
        /// no longer holds (`input::Latch`)
        pub between: u16,
        pub bluetooth: bool,
        pub reports: Vec<FirstReport>,
    }

    /// The PlayStation controllers present, read over HID.
    pub struct Hid {
        listed: Arc<Mutex<Vec<Listed>>>,
        stop: Arc<AtomicBool>,
        devices: Vec<Device>,
        /// each one that could not be opened or failed a read, when, and
        /// whether the open was refused: it is tried again 2 s on (a refused
        /// one stays gilrs's meanwhile; a failed read is most likely one
        /// unplugged)
        failed: HashMap<String, (Instant, bool)>,
        /// the ones whose refusal was said (once each)
        said: Vec<String>,
        /// the names `read` last gave, in its order (`rumble`'s places)
        order: Vec<String>,
    }

    impl Hid {
        /// The controllers present now (so the first frame reads them, and
        /// gilrs's reading of one is passed over from the start), then looked
        /// for from now on (its thread).
        pub fn start() -> Hid {
            let mut known = Known::new();
            let listed = Arc::new(Mutex::new(look_once(&mut known)));
            let stop = Arc::new(AtomicBool::new(false));
            let (l, s) = (listed.clone(), stop.clone());
            if let Err(e) = std::thread::Builder::new().name("hid pads".into()).spawn(move || look(known, &l, &s)) {
                crate::say(&format!("gamepad: no HID controllers: {e}"));
            }
            Hid { listed, stop, devices: Vec::new(), failed: HashMap::new(), said: Vec::new(), order: Vec::new() }
        }

        /// The controllers present and open, in their interfaces' order
        /// (the same in every program), each with its last state: a new one
        /// opened, a gone one closed, and every report waiting read. The
        /// DualShock 4's adapter counts only while a controller answers
        /// through it.
        pub fn read(&mut self) -> Vec<Found> {
            let listed = self.listed.lock().map(|l| l.clone()).unwrap_or_default();
            self.devices.retain(|d| listed.iter().any(|l| l.path == d.listed.path));
            let now = Instant::now();
            self.failed.retain(|p, (t, _)| listed.iter().any(|l| l.path == *p) && now.duration_since(*t) < Duration::from_secs(2));
            for l in &listed {
                if self.devices.iter().any(|d| d.listed.path == l.path) || self.failed.contains_key(&l.path) {
                    continue;
                }
                match Device::open(l) {
                    Some(d) => self.devices.push(d),
                    None => {
                        // SAFETY: the error CreateFileW just left
                        let e = unsafe { GetLastError() };
                        // gone since the last look (unplugged): not refused
                        let gone = matches!(e, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_DEV_NOT_EXIST | ERROR_NO_SUCH_DEVICE | ERROR_DEVICE_NOT_CONNECTED);
                        if !gone && !self.said.contains(&l.path) {
                            crate::say(&format!("gamepad: a {} could not be opened over HID (error {e}: another program may hold it alone); Windows.Gaming.Input reads it, in front only", l.model.label()));
                            self.said.push(l.path.clone());
                        }
                        self.failed.insert(l.path.clone(), (now, !gone));
                    }
                }
            }
            let mut k = 0;
            while k < self.devices.len() {
                if self.devices[k].pump() {
                    k += 1;
                } else {
                    let d = self.devices.remove(k);
                    self.failed.insert(d.listed.path.clone(), (now, false));
                }
            }
            self.devices.sort_by(|a, b| a.listed.path.cmp(&b.listed.path));
            let mut out = Vec::new();
            for d in self.devices.iter_mut() {
                // taken every read, a controller counted or not, so nothing
                // waits in its latch for a later one
                let between = d.latch.take(d.pad.report().buttons);
                if d.listed.ids.1 == super::DS4_ADAPTER && d.valid.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1)) {
                    continue;
                }
                out.push(Found { name: name(&d.listed.path, d.listed.ids), model: d.listed.model, ids: d.listed.ids, pad: d.pad, between, bluetooth: d.listed.bluetooth, reports: d.first.clone() });
            }
            self.order = out.iter().map(|f| f.name.clone()).collect();
            out
        }

        /// The motors of the controller at place `k` in the last `read`'s
        /// list run as `m`, for at most `ms` (`Gamepads::rumble`): its
        /// writer started the first time they are to run, so a controller
        /// never asked to rumble is never written to (one on Bluetooth
        /// changes to its full reports at its first, as for SDL and Steam,
        /// which `parse` reads).
        pub fn rumble(&mut self, k: usize, m: super::Motors, ms: u32) {
            let Some(wanted) = self.order.get(k) else { return };
            let Some(d) = self.devices.iter_mut().find(|d| name(&d.listed.path, d.listed.ids) == *wanted) else { return };
            if d.motors.is_none() && m != super::Motors::OFF {
                d.motors = writer::Writer::start(&d.listed);
            }
            if let Some(w) = &d.motors {
                w.run(m, ms);
            }
        }

        /// Every motor this program set going stopped (the controllers a
        /// window reads changed).
        pub fn stop_motors(&mut self) {
            for w in self.devices.iter().filter_map(|d| d.motors.as_ref()) {
                w.stop();
            }
        }
    }

    impl Hid {
        /// Whether a controller with these USB ids could not be opened
        /// lately: gilrs's reading of it stays gilrs's.
        pub fn refused(&self, ids: (u16, u16)) -> bool {
            self.listed.lock().is_ok_and(|l| l.iter().any(|x| x.ids == ids && self.failed.get(&x.path).is_some_and(|f| f.1)))
        }
    }

    impl Drop for Hid {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The motors' reports as SDL's HIDAPI drivers and Linux's
    /// `hid-playstation` lay them out: the DualShock 4's over USB and over
    /// Bluetooth, the motors' flag alone (the light left as it is), the
    /// small motor before the large; the Bluetooth one's CRC as Python's
    /// zlib computes it over 0xA2 and the report. The controls: the input
    /// reports' seed (0xA1) gives another CRC, and the motors swapped differ.
    #[test]
    fn a_dualshock_4_is_told_its_motors() {
        let m = Motors { large: 0x80, small: 0xFF };
        let usb = rumble_report(Model::DualShock4, false, m, 0, true);
        assert_eq!(usb.len(), 32);
        assert_eq!(&usb[..6], &[0x05, 0x01, 0, 0, 0xFF, 0x80], "report 5, the motors' flag, the small motor, the large");
        assert!(usb[6..].iter().all(|&b| b == 0), "the light and the rest untouched");
        let bt = rumble_report(Model::DualShock4, true, m, 0, true);
        assert_eq!(bt.len(), 78);
        assert_eq!(&bt[..8], &[0x11, 0xC4, 0, 0x01, 0, 0, 0xFF, 0x80], "report 0x11: HID data and a CRC follow, 4 ms reports; the flags at 3, the motors at 6 and 7");
        assert!(bt[8..74].iter().all(|&b| b == 0));
        assert_eq!(&bt[74..], &0xAE49_F571u32.to_le_bytes(), "zlib.crc32(b'\\xa2' + report[:74])");
        // the controls
        let mut seed = flate2::Crc::new();
        seed.update(&[0xA1]);
        seed.update(&bt[..74]);
        assert_ne!(seed.sum(), 0xAE49_F571, "the input reports' seed gives another CRC");
        let swapped = rumble_report(Model::DualShock4, false, Motors { large: 0xFF, small: 0x80 }, 0, true);
        assert_ne!(swapped, usb);
        assert_eq!(rumble_report(Model::DualShock4, false, Motors::OFF, 0, true)[..6], [0x05, 0x01, 0, 0, 0, 0], "stopped: the flag with the motors at 0");
    }

    /// The DualSense's: report 0x02 over USB (48 bytes, its block from 1),
    /// 0x31 over Bluetooth (the sequence number in byte 1's top half, the tag
    /// 0x10, its block from 3, the CRC). The newer rumble is "haptics select"
    /// with flag 0x04 at the block's 38, the first firmware's 0x01 | 0x02;
    /// the Edge always has the newer. The controls: another sequence number
    /// moves the CRC, and the first firmware's flags differ.
    #[test]
    fn a_dualsense_is_told_its_motors() {
        let m = Motors { large: 0x80, small: 0xFF };
        let usb = rumble_report(Model::DualSense, false, m, 0, true);
        assert_eq!(usb.len(), 48);
        assert_eq!(&usb[..5], &[0x02, 0x02, 0, 0xFF, 0x80], "report 2: haptics select, the small motor at 3, the large at 4");
        assert_eq!(usb[1 + 38], 0x04, "the newer rumble's flag");
        assert_eq!(usb.iter().filter(|&&b| b != 0).count(), 5, "nothing else");
        let old = rumble_report(Model::DualSense, false, m, 0, false);
        assert_eq!((old[1], old[39]), (0x03, 0), "the first firmware's rumble");
        assert_eq!(rumble_report(Model::DualSenseEdge, false, m, 0, false), usb, "the Edge has the newer whatever it is told");
        let bt = rumble_report(Model::DualSense, true, m, 5, true);
        assert_eq!(bt.len(), 78);
        assert_eq!(&bt[..7], &[0x31, 0x50, 0x10, 0x02, 0, 0xFF, 0x80]);
        assert_eq!(bt[3 + 38], 0x04);
        assert_eq!(&bt[74..], &0x5FFB_8A7Du32.to_le_bytes(), "zlib.crc32(b'\\xa2' + report[:74])");
        // the controls
        let other = rumble_report(Model::DualSense, true, m, 6, true);
        assert_ne!(other[74..], bt[74..], "another sequence number, another CRC");
        assert_eq!(rumble_report(Model::DualSense, true, m, 16 + 5, true), bt, "the sequence number is four bits");
    }

    /// A DualSense's firmware version is bytes 44 and 45 of its report 0x20.
    /// The controls: another report's ID, a short report.
    #[test]
    fn a_dualsense_says_its_firmware() {
        let mut f = vec![0u8; 64];
        f[0] = 0x20;
        f[44] = 0x24;
        f[45] = 0x02;
        assert_eq!(dualsense_firmware(&f), Some(0x0224));
        assert!(dualsense_firmware(&f).is_some_and(|v| v >= DUALSENSE_RUMBLE_V2));
        f[45] = 0x01;
        assert!(dualsense_firmware(&f).is_some_and(|v| v < DUALSENSE_RUMBLE_V2), "1.24: the first rumble");
        f[0] = 0x05;
        assert_eq!(dualsense_firmware(&f), None, "another report");
        assert_eq!(dualsense_firmware(&[0x20; 45]), None, "too short");
    }

    /// `VIPad`'s motors as a controller's: the small one on (any nonzero) is
    /// all the way; the large one's level as it is.
    #[test]
    fn the_dualshock_2s_motors_are_a_controllers() {
        assert_eq!(Motors::of_act([1, 220]), Motors { large: 220, small: 255 });
        assert_eq!(Motors::of_act([0, 127]), Motors { large: 127, small: 0 });
        assert_eq!(Motors::of_act([0, 0]), Motors::OFF);
    }

    /// A DualShock 4 USB report (64 bytes, ID 1) with the packet's first
    /// nine bytes as given.
    fn ds4_usb(p: [u8; 9]) -> Vec<u8> {
        let mut d = vec![0u8; 64];
        d[0] = 0x01;
        d[1..10].copy_from_slice(&p);
        d
    }

    /// The DualShock 4 over USB and over Bluetooth: the same packet read at
    /// its offset in each. The controls: the adapter's "no controller" bit,
    /// a Bluetooth report without its HID data, and a report of another ID,
    /// each read as nothing.
    #[test]
    fn a_dualshock_4_reads_over_usb_and_bluetooth() {
        // sticks: left full left and down, right at rest; cross with the
        // hat up-right; L1 and Options; PS; L2 most of the way, R2 a little
        let p = [0, 255, 128, 128, 0x20 | 1, 0x01 | 0x20, 0x01, 200, 40];
        let want = PsPad { sticks: [0, 255, 128, 128], triggers: [200, 40], buttons: [0x21, 0x21, 0x01] };
        assert_eq!(parse(Model::DualShock4, false, &ds4_usb(p)), Some(want));
        // Bluetooth's reduced report, as Windows reads it (the longest
        // report's size) and as hidapi elsewhere would (10 bytes)
        let mut small = ds4_usb(p);
        small.truncate(10);
        assert_eq!(parse(Model::DualShock4, true, &small), Some(want));
        let mut bt = vec![0u8; 547];
        bt[0] = 0x11;
        bt[1] = 0xC0;
        bt[3..12].copy_from_slice(&p);
        assert_eq!(parse(Model::DualShock4, true, &bt), Some(want));
        // the controls
        let mut adapter = ds4_usb(p);
        adapter[31] |= 0x04;
        assert_eq!(parse(Model::DualShock4, false, &adapter), None, "the adapter with no controller");
        bt[1] = 0x00;
        assert_eq!(parse(Model::DualShock4, true, &bt), None, "no HID data");
        let mut other = ds4_usb(p);
        other[0] = 0x05;
        assert_eq!(parse(Model::DualShock4, false, &other), None, "another report");
        assert_eq!(parse(Model::DualShock4, false, &[]), None);
    }

    /// A fight stick's trigger button with no travel reads as all the way
    /// (SDL's rule); with travel the travel counts. The control: the bit
    /// clear leaves no travel.
    #[test]
    fn a_trigger_button_with_no_travel_is_all_the_way() {
        let r = |b1: u8, l2: u8| parse(Model::DualShock4, false, &ds4_usb([128, 128, 128, 128, 8, b1, 0, l2, 0])).unwrap().triggers;
        assert_eq!(r(0x04, 0), [255, 0]);
        assert_eq!(r(0x08, 0), [0, 255]);
        assert_eq!(r(0x04, 30), [30, 0], "travel there: its own");
        assert_eq!(r(0x00, 0), [0, 0], "the control");
    }

    /// A DualSense report: `id`, `n` bytes, `at` where the packet starts.
    fn ds5(id: u8, n: usize, at: usize, p: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; n];
        d[0] = id;
        d[at..at + p.len()].copy_from_slice(p);
        d
    }

    /// Bluetooth's CRC over a report, put in its last four bytes.
    fn seal(d: &mut [u8]) {
        let at = d.len() - 4;
        let mut h = flate2::Crc::new();
        h.update(&[0xA1]);
        h.update(&d[..at]);
        d[at..].copy_from_slice(&h.sum().to_le_bytes());
    }

    /// The DualSense's three reports: USB's full one (64 bytes), Bluetooth's
    /// reduced one (the DualShock 4's order) and its full one (a byte further
    /// on, with a CRC). The controls: the same bytes read in the other layout
    /// differ, and a Bluetooth report whose CRC fails is nothing.
    #[test]
    fn a_dualsense_reads_its_three_reports() {
        // sticks, triggers, counter, buttons: square, hat left; R1, Create; mute
        let full = [10, 20, 30, 40, 250, 0, 7, 0x10 | 6, 0x02 | 0x10, 0x04];
        let want = PsPad { sticks: [10, 20, 30, 40], triggers: [250, 0], buttons: [0x16, 0x12, 0x04] };
        for m in [Model::DualSense, Model::DualSenseEdge] {
            assert_eq!(parse(m, false, &ds5(0x01, 64, 1, &full)), Some(want), "{m:?} USB");
            let mut bt = ds5(0x31, 78, 2, &full);
            seal(&mut bt);
            assert_eq!(parse(m, true, &bt), Some(want), "{m:?} Bluetooth");
            bt[40] ^= 1;
            assert_eq!(parse(m, true, &bt), None, "{m:?}: a CRC that fails");
        }
        // the reduced report: sticks, buttons, triggers; off Bluetooth by
        // SDL's lengths (10, 78) only
        let simple = [10, 20, 30, 40, 0x16, 0x12, 0x04, 250, 0];
        for n in [10, 78] {
            assert_eq!(parse(Model::DualSense, false, &ds5(0x01, n, 1, &simple)), Some(want), "{n} bytes");
        }
        // the control: the full report's bytes at 78 are read as reduced
        assert_ne!(parse(Model::DualSense, false, &ds5(0x01, 78, 1, &full)), Some(want));
    }

    /// On Bluetooth a DualSense's report 0x01 is its reduced one at whatever
    /// length a read pads it to: 10 (hidapi off Windows), 78 (Windows, as
    /// SDL has it), 128 (SDL's buffer) or 547 (a Bluetooth descriptor's
    /// longest, as the DualShock 4's): triangle is triangle and the d-pad at
    /// rest. The control is F3 20261024-204603 itself: by its length alone
    /// (SDL's rule, before) the padded report is read in the full layout,
    /// and the pad holds the d-pad up whatever is pressed and reads triangle
    /// as L2.
    #[test]
    fn a_dualsense_on_bluetooth_reads_its_reduced_report_at_any_length() {
        // at rest but for triangle: sticks in the middle, the hat at rest
        let simple = [128, 128, 128, 128, 0x80 | 8, 0, 0, 0, 0];
        let want = PsPad { sticks: [128; 4], triggers: [0, 0], buttons: [0x88, 0, 0] };
        for n in [10, 78, 128, 547] {
            let r = ds5(0x01, n, 1, &simple);
            assert_eq!(parse(Model::DualSense, true, &r), Some(want), "{n} bytes");
            assert_eq!(parse(Model::DualSense, true, &r).unwrap().report().buttons, bit::TRIANGLE, "{n} bytes: triangle alone");
        }
        // the control: the report's own symptoms, read by its length
        for n in [128, 547] {
            let got = parse(Model::DualSense, false, &ds5(0x01, n, 1, &simple)).unwrap().report().buttons;
            assert_eq!(got, bit::UP | bit::L2, "{n} bytes by length: the d-pad up held, triangle as L2");
        }
    }

    /// A genuine report off a DualSense on USB (the one plugged in here,
    /// read through hidraw), at rest: the sticks near the middle, the hat at
    /// rest, nothing pressed, its motion sensors and timestamps after. The
    /// control: the same bytes read as Bluetooth's reduced report hold the
    /// d-pad up (the L2 trigger's 0 in the hat's place).
    #[test]
    fn a_real_dualsense_report_at_rest() {
        let hex = "01827e7f7e00002a0800000087324c44fdff08000300eeff371fee05d4388a7a0f963fe62f9a82413ffb09090000000000df498a7a290c00dfdf0140c9482d15";
        let r: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        assert_eq!(r.len(), 64);
        let p = parse(Model::DualSense, false, &r).unwrap();
        assert_eq!(p, PsPad { sticks: [0x82, 0x7e, 0x7f, 0x7e], triggers: [0, 0], buttons: [0x08, 0x00, 0x00] });
        assert_eq!(p.report().buttons, 0, "nothing pressed");
        assert!(p.report().sticks.iter().all(|s| s.abs() < 0.03), "{:?}", p.report().sticks);
        assert_eq!(parse(Model::DualSense, true, &r).unwrap().report().buttons & bit::UP, bit::UP, "the control: as Bluetooth's");
    }

    /// The first report of each ID as the log and a problem report say it:
    /// its ID, its length, its first twelve bytes, and "no state" where it
    /// held none. The control: a report of no bytes is none.
    #[test]
    fn a_first_report_says_what_came() {
        let r = ds5(0x01, 78, 1, &[128, 127, 129, 128, 8, 0, 0, 0, 0]);
        let f = FirstReport::of(&r, true).unwrap();
        assert_eq!(f.describe(), "report 0x01, 78 bytes: 01 80 7f 81 80 08 00 00 00 00 00 00");
        let short = FirstReport::of(&[0x31, 0x40], false).unwrap();
        assert_eq!(short.describe(), "report 0x31, 2 bytes (no state): 31 40");
        assert_eq!(FirstReport::of(&[], false), None, "the control");
    }

    /// The report the game gets: every button where libpad has it, the hat's
    /// eight ways (a diagonal both), the triggers past a third, the sticks
    /// with +y down, ends at ±1, the middle at rest. The controls: the hat at
    /// rest (8) and past it (15) hold no way, and the PS, touch pad and
    /// microphone buttons are none of the game's.
    #[test]
    fn a_playstation_pad_reads_as_libpads_report() {
        let b = |b0: u8, b1: u8| PsPad { buttons: [b0, b1, 0], ..Default::default() }.report().buttons;
        for (m, k) in FACE {
            assert_eq!(b(m | 8, 0), k, "{m:#04x}");
        }
        for (m, k) in MIDDLE {
            assert_eq!(b(8, m), k, "{m:#04x}");
        }
        let ways = [bit::UP, bit::UP | bit::RIGHT, bit::RIGHT, bit::DOWN | bit::RIGHT, bit::DOWN, bit::DOWN | bit::LEFT, bit::LEFT, bit::UP | bit::LEFT];
        for (h, w) in ways.iter().enumerate() {
            assert_eq!(b(h as u8, 0), *w, "hat {h}");
        }
        assert_eq!((b(8, 0), b(15, 0)), (0, 0), "the hat at rest");
        assert_eq!(b(8, 0x04 | 0x08), 0, "the triggers' bits: their travel counts");
        assert_eq!(PsPad { buttons: [8, 0, 0x01 | 0x02 | 0x04], ..Default::default() }.report().buttons, 0, "PS, touch pad, microphone");
        let t = |l: u8, r: u8| PsPad { triggers: [l, r], ..Default::default() }.report().buttons;
        assert_eq!((t(84, 85), t(85, 84)), (bit::R2, bit::L2), "a third of 255 is 84.15");
        let s = PsPad { sticks: [0, 0, 255, 128], ..Default::default() }.report().sticks;
        assert_eq!(s[2..], [-1.0, -1.0], "left stick: full left, full up is -1");
        assert_eq!(s[0], 1.0, "right stick: full right");
        assert!(s[1].abs() < 0.01, "the middle is at rest: {}", s[1]);
        assert_eq!(PsPad::default().report().buttons, 0);
        // as an Xbox pad pressed alike reads (`xinput`): through the game's
        // pad path a cross is a cross wherever it comes from
        let x = crate::xinput::XPad { buttons: 0x1000 | 0x0100, ..Default::default() }.report().buttons;
        assert_eq!(b(0x20 | 8, 0x01), x);
    }

    /// A DualSense on USB, its reports as a window's reads drain them
    /// (`Hid::read`: every report waiting, through `take`, then the last
    /// state and what was let go since, as `Gamepads::poll` merges them),
    /// through the game's pad path (`input::Console`): cross tapped between
    /// two reads among a thousand reports a second is pressed once and let
    /// go once; a report that holds no state (another ID) changes nothing.
    /// The control: the last state alone, as the client read it before (F3
    /// 20261025-111324), never presses it.
    #[test]
    fn a_tap_between_two_reads_reaches_the_game() {
        use crate::input::{Console, Latch, Report};
        let usb = |b0: u8| ds5(0x01, 64, 1, &[128, 128, 128, 128, 0, 0, 7, b0, 0, 0]);
        let rest = usb(8);
        let cross = usb(0x20 | 8);
        let other = ds5(0x05, 64, 1, &[0x20 | 8; 10]);
        // 33 reports between two reads (a frame at 30 a second): the tap
        // 8 ms long in the middle of the first frame
        let frame = |tap: bool| -> Vec<Vec<u8>> { (0..33).map(|i| if tap && (12..20).contains(&i) { cross.clone() } else { rest.clone() }).collect() };
        let reads = [frame(true), vec![other.clone()], frame(false), frame(false)];
        let keys = |latched: bool| -> Vec<(i32, bool)> {
            let (mut pad, mut latch) = (PsPad::default(), Latch::default());
            let mut c = Console::new();
            c.poll(Report::default(), true, false, false);
            let mut out = Vec::new();
            for reports in &reads {
                for r in reports {
                    take(Model::DualSense, false, r, &mut pad, &mut latch);
                }
                let mut report = pad.report();
                let between = latch.take(report.buttons);
                if latched {
                    report.buttons |= between;
                }
                out.extend(c.poll(report, true, false, false));
            }
            out
        };
        assert_eq!(keys(true), [(134, true), (134, false)]);
        assert!(keys(false).is_empty(), "the control: the last report alone");
        // a report that holds no state: nothing taken
        let (mut pad, mut latch) = (PsPad::default(), Latch::default());
        assert_eq!(take(Model::DualSense, false, &other, &mut pad, &mut latch), None);
        assert_eq!((pad, latch.take(0)), (PsPad::default(), 0));
    }

    /// The models by USB ids, and a controller's name: the same for its
    /// path in any case, another for another path. The control: an Xbox
    /// controller's ids are none of these.
    #[test]
    fn models_and_names() {
        assert_eq!(Model::of(0x054C, 0x05C4), Some(Model::DualShock4));
        assert_eq!(Model::of(0x054C, 0x09CC), Some(Model::DualShock4));
        assert_eq!(Model::of(0x054C, DS4_ADAPTER), Some(Model::DualShock4));
        assert_eq!(Model::of(0x054C, 0x0CE6), Some(Model::DualSense));
        assert_eq!(Model::of(0x054C, 0x0DF2), Some(Model::DualSenseEdge));
        assert_eq!(Model::of(0x045E, 0x028E), None, "the control: an Xbox 360 pad");
        assert_eq!([Model::DualShock4, Model::DualSense, Model::DualSenseEdge].map(Model::label), ["DualShock 4", "DualSense", "DualSense Edge"]);
        let p = r"\\?\HID#VID_054C&PID_05C4&MI_03#7&1a2b3c4d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert_eq!(name(p, (0x054C, 0x05C4)), name(&p.to_ascii_lowercase(), (0x054C, 0x05C4)));
        assert!(name(p, (0x054C, 0x05C4)).starts_with("hid-054c-05c4-"));
        assert_ne!(name(p, (0x054C, 0x05C4)), name(&p.replace("&0&0000", "&0&0001"), (0x054C, 0x05C4)));
        assert!(!bluetooth(p));
        assert!(bluetooth(r"\\?\HID#{00001124-0000-1000-8000-00805F9B34FB}_VID&0002054c_PID&09cc#9&2c1d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"));
    }
}
