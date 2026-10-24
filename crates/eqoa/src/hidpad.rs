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
//! `SDL_hidapi_ps5.c`). Read only: nothing is sent to a controller, so a
//! Bluetooth one stays in the reduced reports it starts in (they hold every
//! button, the triggers and the sticks), and its light is left as it is.
//! gilrs's reading of the same controller is passed over (by USB vendor and
//! product, as for XInput); its motors stay Windows.Gaming.Input's
//! (`Gamepads::rumble`). A controller that cannot be opened (another program
//! holds it alone) stays gilrs's.

use crate::input::{Report, bit};

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

/// What an input report holds, as SDL's drivers read it (`IsPacketValid`
/// and `UpdateDevice`): `data` as a read returns it, the report ID first
/// (Windows returns the collection's longest input report, the rest zeros).
/// None for a report that holds no state: another report, the adapter with
/// no controller (byte 31's 0x04), a Bluetooth report without its HID data
/// or, a DualSense's, failing its CRC.
pub fn parse(model: Model, data: &[u8]) -> Option<PsPad> {
    let n = data.len();
    match (model, *data.first()?) {
        // USB, and Bluetooth's reduced report
        (Model::DualShock4, 0x01) if n == 10 || (n >= 64 && data[31] & 0x04 == 0) => ds4_state(&data[1..]),
        // Bluetooth's full reports: two bytes more, the first saying the
        // HID data is there
        (Model::DualShock4, 0x11..=0x19) if n >= 78 && data[1] & 0x80 != 0 => ds4_state(&data[3..]),
        // Bluetooth's reduced report (10 bytes, 78 as Windows reads it)
        (Model::DualSense | Model::DualSenseEdge, 0x01) if n == 10 || n == 78 => ds5_simple(&data[1..]),
        (Model::DualSense | Model::DualSenseEdge, 0x01) => ds5_state(&data[1..]),
        // Bluetooth's full report: a byte more, and a CRC
        (Model::DualSense | Model::DualSenseEdge, 0x31) if n >= 78 && crc_ok(&data[..78]) => ds5_state(&data[2..]),
        _ => None,
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

/// Windows' side: the HID interfaces present, looked through once a second on
/// a thread of its own (a walk of the system's device lists takes
/// milliseconds), and each controller's reports read on the game's thread
/// with an overlapped read drained every frame, the last one kept.
#[cfg(windows)]
mod device {
    use super::{Model, PsPad, bluetooth, name, parse};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{HIDD_ATTRIBUTES, HidD_GetAttributes, HidD_GetHidGuid};
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, GENERIC_READ, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile};
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

    /// A Sony controller's HID interface present now.
    #[derive(Clone, Debug, PartialEq)]
    struct Listed {
        path: String,
        ids: (u16, u16),
        model: Model,
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
                found.push(Listed { path: p, ids: *ids, model });
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
        /// when a report last held its state
        valid: Option<Instant>,
    }

    impl Device {
        fn open(l: &Listed) -> Option<Device> {
            let handle = open(&l.path, GENERIC_READ, FILE_FLAG_OVERLAPPED)?;
            Some(Device { listed: l.clone(), handle, read: Box::new(Pending { ov: OVERLAPPED::default(), buf: [0; 1024], busy: false }), pad: PsPad::default(), valid: None })
        }

        /// Every report waiting read, the last state kept; false when the
        /// controller is gone (a read failed).
        fn pump(&mut self) -> bool {
            // the driver keeps a few dozen reports for a reader: a bound past them
            for _ in 0..256 {
                let r = &mut *self.read;
                if !r.busy {
                    r.ov = OVERLAPPED::default();
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
                if let Some(p) = parse(self.listed.model, &r.buf[..(n as usize).min(r.buf.len())]) {
                    self.pad = p;
                    self.valid = Some(Instant::now());
                }
            }
            true
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: a pending read cancelled and waited for before its box
            // goes; then the handle closed
            unsafe {
                if self.read.busy {
                    CancelIoEx(self.handle, &self.read.ov);
                    let mut n = 0u32;
                    GetOverlappedResult(self.handle, &self.read.ov, &mut n, 1);
                }
                CloseHandle(self.handle);
            }
        }
    }

    /// A controller read over HID: its name among the client's windows,
    /// model, USB ids, state, and whether it is on Bluetooth.
    pub struct Found {
        pub name: String,
        pub model: Model,
        pub ids: (u16, u16),
        pub pad: PsPad,
        pub bluetooth: bool,
    }

    /// The PlayStation controllers present, read over HID.
    pub struct Hid {
        listed: Arc<Mutex<Vec<Listed>>>,
        stop: Arc<AtomicBool>,
        devices: Vec<Device>,
        /// each one that could not be opened or failed a read, and when: it
        /// is tried again 2 s on (and stays gilrs's meanwhile)
        failed: HashMap<String, Instant>,
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
            Hid { listed, stop, devices: Vec::new(), failed: HashMap::new() }
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
            self.failed.retain(|p, t| listed.iter().any(|l| l.path == *p) && now.duration_since(*t) < Duration::from_secs(2));
            for l in &listed {
                if self.devices.iter().any(|d| d.listed.path == l.path) || self.failed.contains_key(&l.path) {
                    continue;
                }
                match Device::open(l) {
                    Some(d) => self.devices.push(d),
                    None => {
                        self.failed.insert(l.path.clone(), now);
                    }
                }
            }
            let mut k = 0;
            while k < self.devices.len() {
                if self.devices[k].pump() {
                    k += 1;
                } else {
                    let d = self.devices.remove(k);
                    self.failed.insert(d.listed.path.clone(), now);
                }
            }
            self.devices.sort_by(|a, b| a.listed.path.cmp(&b.listed.path));
            self.devices
                .iter()
                .filter(|d| d.listed.ids.1 != super::DS4_ADAPTER || d.valid.is_some_and(|t| now.duration_since(t) < Duration::from_secs(1)))
                .map(|d| Found { name: name(&d.listed.path, d.listed.ids), model: d.listed.model, ids: d.listed.ids, pad: d.pad, bluetooth: bluetooth(&d.listed.path) })
                .collect()
        }
    }

    impl Hid {
        /// Whether a controller with these USB ids could not be opened (or
        /// failed a read) lately: gilrs's reading of it stays gilrs's.
        pub fn refused(&self, ids: (u16, u16)) -> bool {
            self.listed.lock().is_ok_and(|l| l.iter().any(|x| x.ids == ids && self.failed.contains_key(&x.path)))
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
        assert_eq!(parse(Model::DualShock4, &ds4_usb(p)), Some(want));
        // Bluetooth's reduced report, as Windows reads it (the longest
        // report's size) and as hidapi elsewhere would (10 bytes)
        let mut small = ds4_usb(p);
        small.truncate(10);
        assert_eq!(parse(Model::DualShock4, &small), Some(want));
        let mut bt = vec![0u8; 547];
        bt[0] = 0x11;
        bt[1] = 0xC0;
        bt[3..12].copy_from_slice(&p);
        assert_eq!(parse(Model::DualShock4, &bt), Some(want));
        // the controls
        let mut adapter = ds4_usb(p);
        adapter[31] |= 0x04;
        assert_eq!(parse(Model::DualShock4, &adapter), None, "the adapter with no controller");
        bt[1] = 0x00;
        assert_eq!(parse(Model::DualShock4, &bt), None, "no HID data");
        let mut other = ds4_usb(p);
        other[0] = 0x05;
        assert_eq!(parse(Model::DualShock4, &other), None, "another report");
        assert_eq!(parse(Model::DualShock4, &[]), None);
    }

    /// A fight stick's trigger button with no travel reads as all the way
    /// (SDL's rule); with travel the travel counts. The control: the bit
    /// clear leaves no travel.
    #[test]
    fn a_trigger_button_with_no_travel_is_all_the_way() {
        let r = |b1: u8, l2: u8| parse(Model::DualShock4, &ds4_usb([128, 128, 128, 128, 8, b1, 0, l2, 0])).unwrap().triggers;
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
    /// reduced one (the DualShock 4's order, 10 or 78 bytes) and its full one
    /// (a byte further on, with a CRC). The controls: the same bytes read in
    /// the other layout differ, and a Bluetooth report whose CRC fails is
    /// nothing.
    #[test]
    fn a_dualsense_reads_its_three_reports() {
        // sticks, triggers, counter, buttons: square, hat left; R1, Create; mute
        let full = [10, 20, 30, 40, 250, 0, 7, 0x10 | 6, 0x02 | 0x10, 0x04];
        let want = PsPad { sticks: [10, 20, 30, 40], triggers: [250, 0], buttons: [0x16, 0x12, 0x04] };
        for m in [Model::DualSense, Model::DualSenseEdge] {
            assert_eq!(parse(m, &ds5(0x01, 64, 1, &full)), Some(want), "{m:?} USB");
            let mut bt = ds5(0x31, 78, 2, &full);
            seal(&mut bt);
            assert_eq!(parse(m, &bt), Some(want), "{m:?} Bluetooth");
            bt[40] ^= 1;
            assert_eq!(parse(m, &bt), None, "{m:?}: a CRC that fails");
        }
        // the reduced report: sticks, buttons, triggers
        let simple = [10, 20, 30, 40, 0x16, 0x12, 0x04, 250, 0];
        for n in [10, 78] {
            assert_eq!(parse(Model::DualSense, &ds5(0x01, n, 1, &simple)), Some(want), "{n} bytes");
        }
        // the control: the full report's bytes at 78 are read as reduced
        assert_ne!(parse(Model::DualSense, &ds5(0x01, 78, 1, &full)), Some(want));
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
