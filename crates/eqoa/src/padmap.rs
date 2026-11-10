//! The pads as a browser's Gamepad API gives them, laid out as libpad's
//! report (`input::Report`). The page reads them straight from
//! `navigator.getGamepads()` (`web::pad`), every button and axis and the
//! mapping the browser names; natively the pads are gilrs's (SDL's layout).
//!
//! A pad's `mapping` says how to read it:
//!
//! - "standard" is the spec's layout: buttons 0-3 the face's bottom, right,
//!   left and top (✕ ○ □ △), 4/5 the shoulders, 6/7 the triggers (analog,
//!   L2/R2 past a third of their travel, as the game reads them as buttons),
//!   8 back (Select), 9 Start, 10/11 the sticks' clicks, 12-15 the d-pad; axes
//!   0/1 the left stick and 2/3 the right, -1 left and up (libpad's way).
//! - Firefox on Linux reads evdev itself (dom/gamepad/linux/LinuxGamepad.cpp)
//!   and calls every device with BTN_GAMEPAD "standard", taking the kernel's
//!   codes at their word: BTN_NORTH is button 3, the top one, and BTN_WEST
//!   button 2. xpad, the driver of the wired Xbox pads and their clones, sends
//!   the left button (labelled X) as BTN_X, which is BTN_NORTH, and the top
//!   one (Y) as BTN_Y, which is BTN_WEST: there □ and △ come swapped (F3
//!   20261020-182839, a PowerA pad, "Generic X-Box pad"). hid-playstation (a
//!   DualSense, a DualShock 4) and hid-nintendo send them by position and come
//!   right. xpad's triggers are axes, ABS_Z and ABS_RZ, and it has no
//!   BTN_TL2/BTN_TR2, so buttons 6 and 7 never press there: Firefox puts the
//!   triggers after the four stick axes, at 4 and 5, -1 at rest.
//! - No mapping ("") is the device's own order, with a positional default:
//!   read as the standard layout, except the order xpad's devices have through
//!   joydev (Chrome on Linux, for a pad it has no mapping for: A B X Y LB RB
//!   Back Start Guide LS RS, axes LX LY LT RX RY RT and the hat), told by its
//!   triggers resting at -1 at axes 2 and 5 (not a hat's, below). Joydev is
//!   Linux's alone, so nowhere else is a pair of axes taken for triggers. Its d-pad
//!   is a hat where it has one (`PadMap::raw_dpad`): a third-party PS3 pad
//!   (twelve or thirteen buttons and a hat switch, a DragonRise, a ShanWan)
//!   has no buttons 12 to 15 for one, and read there its d-pad did nothing
//!   (2026-11-10, "some third party ps3 controllers dpads arent being
//!   registered"). Through joydev (Chrome on Linux) and Firefox's evdev
//!   (Firefox on Linux, for a pad with no BTN_GAMEPAD, which it does not
//!   call "standard") a pad's axes come in the order of their codes, so
//!   the hat switch's ABS_HAT0X and ABS_HAT0Y are its last two, -1, 0 or 1;
//!   Firefox on macOS lists them in its report's order, a hat switch on one
//!   axis as Chromium scales it (below), its rest past 1. Either is the
//!   d-pad once it is told from a stick: the pair at the end once it has
//!   rested at 0 together and given nothing but -1, 0 and 1 (a stick's or a
//!   trigger's axis gives the values between), the one axis once it has
//!   rested past ±1 and given nothing a hat switch does not. Safari leaves
//!   such a pad's hat out altogether (WebKit's `GenericHIDGamepad`), and a
//!   pad passed off as Sony's DualShock 3 (054c:0268) is "standard" with
//!   its d-pad on the buttons that DualShock 3 has, in Chromium and Firefox
//!   alike: neither can be read here.
//! - No mapping in Chromium outside Linux (Chrome and Edge on Windows and
//!   macOS, which map few DirectInput pads and none of 8BitDo's), and in
//!   Firefox on Windows, is the HID report as it comes
//!   (raw_input_gamepad_device_win.cc, WindowsGamepad.cpp): the report's
//!   button n at index n - 1, and each axis at its usage less X's (X 0, Y 1,
//!   Z 2, Rx 3, Ry 4, Rz 5, the hat switch 9). The right stick is Z and Rz, 2
//!   and 5, as Chromium's own DirectInput mappers read it, and the d-pad is
//!   the hat: -1 up, on clockwise in eighths to 1 (up and left), past 1 at
//!   rest, 0 before its first report (`DpadFromAxis`, whose bands `hat`
//!   keeps). Firefox on Windows was read as the standard layout until
//!   2026-11-10: such a pad's d-pad did nothing, and its right stick's up and
//!   down was Rx's. Read as the standard layout, an 8BitDo Pro 2 in its D-input mode
//!   had ✕ on its right face button and ○ on the bottom one, Select and
//!   Start on ZL and ZR, the right stick's up and down on axis 3 (no stick's),
//!   no d-pad (so no way down the Options window to Save Changes but the
//!   keyboard or the mouse), and, once its right stick went up and left (-1
//!   at 2 and 5 together), joydev's order, L2 and R2 held, for the rest of the
//!   session (a player's report, 2026-11-04: Windows 11, Chrome 156, "it sets
//!   his controller up wrong" and the Controls page "won't save"). A pad
//!   whose own order SDL's database gives is read in it (`own_order`:
//!   8BitDo's D-input pads put the right face button first, the bottom one
//!   second, the top fourth and the left fifth), on Linux through joydev too
//!   (its right stick at 2 and 3 there, its hat at 6 and 7). Chrome's id names
//!   the vendor and product of a pad it has no mapping for as "<name>
//!   (Vendor: 2dc8 Product: 6006)".
//!
//! Which pad xpad drives, the page sees only by its id, "vvvv-pppp-<the
//! kernel's name>" in Firefox: a name with "X-Box" or "Xbox" in it (xpad's
//! name for a device it has no entry for is "Generic X-Box pad", and a third of
//! its 312 entries have it), or a vendor in the list xpad takes by interface
//! (`XPAD_VENDORS`: Microsoft, Logitech, Mad Catz, PDP, Hori, Razer, PowerA,
//! 8BitDo and the rest of its table). Sony and Nintendo are in neither. The
//! page's `?padxy=swap` or `?padxy=keep` says otherwise for a pad the rule gets
//! wrong.
//!
//! Firefox reads no axis's value until the axis sends an event, so every
//! axis reads 0 until it first moves, a trigger at rest included (seen on
//! Firefox 140 with a clone of the tester's pad on uinput). On a pad xpad
//! drives axes 4 and 5 are its triggers, and each counts once it has moved
//! from what was first seen. On any other pair counts only once both have
//! read -1 together, as triggers at rest do: a stick's axis there (a generic
//! HID pad puts its right stick on ABS_Z and ABS_RZ, so at axes 4 and 5) rests
//! at 0, and read as a trigger it would hold L2 down for good. An axis a
//! browser gives as no number reads as 0.

use crate::input::{Report, bit, hat_ways};

/// One pad as the Gamepad API gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WebPad {
    /// `Gamepad.index`
    pub index: u32,
    pub id: String,
    pub mapping: String,
    /// each button's `pressed` and `value`
    pub buttons: Vec<(bool, f32)>,
    pub axes: Vec<f32>,
}

/// How a pad's buttons and axes are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// "standard", as the spec lays it out
    Standard,
    /// Firefox on Linux's "standard": evdev's codes at their word
    Evdev,
    /// no mapping: the device's own order
    Raw,
    /// no mapping in Chromium outside Linux: the HID report's own order,
    /// each axis at its usage
    Usage,
}

/// The browser, as far as the layout goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Browser {
    /// Firefox reading evdev itself (on Linux; not Android's)
    pub firefox_linux: bool,
    /// Firefox on Windows, which reads a pad it has no mapping for by its
    /// HID usages as Chromium does (`WindowsGamepad.cpp`)
    pub firefox_windows: bool,
    /// on Linux or ChromeOS (not Android): a pad with no mapping comes
    /// through joydev
    pub linux: bool,
    /// Chromium's (Chrome, Edge, Opera): its own fetchers' layouts
    pub chromium: bool,
    /// the page's `?padxy=`: `swap` (Some(true)) or `keep` (Some(false))
    pub xy: Option<bool>,
}

impl Browser {
    /// From `navigator.userAgent` and the page's `?padxy=`.
    pub fn new(user_agent: &str, padxy: Option<&str>) -> Browser {
        let android = user_agent.contains("Android");
        let firefox = user_agent.contains("Firefox/");
        let firefox_linux = firefox && user_agent.contains("Linux") && !android;
        let firefox_windows = firefox && user_agent.contains("Windows");
        let linux = (user_agent.contains("Linux") || user_agent.contains("CrOS")) && !android;
        let chromium = user_agent.contains("Chrome/") || user_agent.contains("Chromium/");
        let xy = match padxy {
            Some("swap") => Some(true),
            Some("keep") => Some(false),
            _ => None,
        };
        Browser { firefox_linux, firefox_windows, linux, chromium, xy }
    }
}

/// The game's buttons in the order a pad's own order lists them
/// (`own_order`): ✕ ○ □ △ L1 R1 L2 R2 Select Start L3 R3.
const ORDER_BITS: [u16; 12] = [bit::CROSS, bit::CIRCLE, bit::SQUARE, bit::TRIANGLE, bit::L1, bit::R1, bit::L2, bit::R2, bit::SELECT, bit::START, bit::L3, bit::R3];

/// The standard layout's places for them, the default for a pad with no
/// mapping.
const POSITIONAL: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

/// 8BitDo's pads in their D-input mode, as SDL's database maps every one of
/// them on Windows (a:b1 b:b0 x:b4 y:b3, the shoulders b6 and b7, the
/// triggers b8 and b9, back b10, start b11, the sticks' clicks b13 and b14):
/// the right face button first, the bottom one second, the top fourth, the
/// left fifth.
const EIGHTBITDO_DINPUT: [u8; 12] = [1, 0, 4, 3, 6, 7, 8, 9, 10, 11, 13, 14];

/// Their products (vendor 2dc8) with that order in SDL's database: the Pro 2
/// is 6003, 6103 and 6006, the Pro 3 6009, the SN30 Pros 6001 and 6101.
const EIGHTBITDO_DINPUT_PRODUCTS: &[u16] = &[
    0x2820, 0x2840, 0x2865, 0x3001, 0x3101, 0x3102, 0x3103, 0x3104, 0x3810, 0x3820, 0x5111, 0x5112, 0x6000, 0x6001, 0x6002, 0x6003, 0x6006, 0x6009, 0x6100, 0x6101,
    0x6102, 0x6103, 0x9000, 0x9001, 0x9015, 0xab11, 0xab20,
];

/// A pad's own button order, where SDL's database gives it: the Gamepad
/// API's index of each of `ORDER_BITS`. Read only when the browser has no
/// mapping for the pad.
pub fn own_order(pad: &WebPad) -> Option<[u8; 12]> {
    let (vendor, product, _) = id_parts(&pad.id);
    (vendor == Some(0x2dc8) && product.is_some_and(|p| EIGHTBITDO_DINPUT_PRODUCTS.contains(&p))).then_some(EIGHTBITDO_DINPUT)
}

/// The d-pad from a hat switch's axis as Chromium gives it, in
/// `DpadFromAxis`'s bands: -1 up, on clockwise in eighths to 1 (up and
/// left); past 1 (or below -1) at rest, and 0 before the first report.
fn hat(v: f32) -> u16 {
    if v == 0.0 {
        return 0;
    }
    let mut k = 0;
    if (-1.0..-0.7).contains(&v) || (0.95..=1.0).contains(&v) {
        k |= bit::UP;
    }
    if (-0.75..-0.1).contains(&v) {
        k |= bit::RIGHT;
    }
    if (-0.2..0.45).contains(&v) {
        k |= bit::DOWN;
    }
    if (0.4..=1.0).contains(&v) {
        k |= bit::LEFT;
    }
    k
}

/// The vendors xpad takes a device from by its interface (its
/// `XPAD_XBOX360_VENDOR` and `XPAD_XBOXONE_VENDOR` lines, Linux 6.x): an
/// Xbox-protocol pad from one of them is xpad's, whatever its name.
pub const XPAD_VENDORS: [u16; 58] = [
    0x0079, 0x0351, 0x03eb, 0x03f0, 0x044f, 0x045e, 0x046d, 0x0502, 0x056e, 0x06a3, 0x0738, 0x07ff, 0x0b05, 0x0c12, 0x0db0, 0x0e6f, 0x0f0d, 0x1038, 0x10f5, 0x11c9,
    0x11ff, 0x1209, 0x12ab, 0x1430, 0x146b, 0x1532, 0x15e4, 0x162e, 0x1689, 0x16d0, 0x17ef, 0x1949, 0x1a86, 0x1b1c, 0x1bad, 0x1ee9, 0x20bc, 0x20d6, 0x2345, 0x24c6,
    0x2563, 0x260d, 0x294b, 0x2993, 0x2c22, 0x2dc8, 0x2e24, 0x2e95, 0x2f24, 0x31e3, 0x3285, 0x3507, 0x3537, 0x3651, 0x366c, 0x37d7, 0x3958, 0x413d,
];

/// An id's vendor, product and name: Firefox's "054c-0ce6-<name>", Chrome's
/// "<name> (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)", and Chrome's for a
/// pad it has no mapping for, "<name> (Vendor: 2dc8 Product: 6006)".
pub fn id_parts(id: &str) -> (Option<u16>, Option<u16>, &str) {
    let hex = |s: &str| (s.len() == 4).then(|| u16::from_str_radix(s, 16).ok()).flatten();
    let mut it = id.splitn(3, '-');
    if let (Some(v), Some(p), Some(name)) = (it.next(), it.next(), it.next())
        && let (Some(v), Some(p)) = (hex(v), hex(p))
    {
        return (Some(v), Some(p), name);
    }
    if let Some(at) = id.find("Vendor: ") {
        let rest = &id[at + 8..];
        let v = rest.get(..4).and_then(hex);
        let p = rest.find("Product: ").and_then(|i| rest.get(i + 9..i + 13)).and_then(hex);
        let name = id[..at].rsplit_once(" (").map_or(id[..at].trim_end(), |(n, _)| n);
        return (v, p, name);
    }
    (None, None, id)
}

/// Whether xpad drives the pad with this id (the module's notes).
pub fn is_xpad(id: &str) -> bool {
    let (vendor, _, name) = id_parts(id);
    let name = name.to_ascii_lowercase();
    name.contains("x-box") || name.contains("xbox") || vendor.is_some_and(|v| XPAD_VENDORS.contains(&v))
}

/// The layout a pad's mapping means in this browser.
pub fn layout(pad: &WebPad, b: Browser) -> Layout {
    match pad.mapping.as_str() {
        "standard" if b.firefox_linux => Layout::Evdev,
        "standard" => Layout::Standard,
        _ if (b.chromium && !b.linux) || b.firefox_windows => Layout::Usage,
        _ => Layout::Raw,
    }
}

/// Where a pad's d-pad is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dpad {
    /// buttons 12 to 15, the standard layout's places
    Buttons,
    /// a hat switch on one axis, as Chromium and Firefox scale it (`hat`)
    Pov(usize),
    /// a hat on two axes, its x at this one and its y at the next, -1 left
    /// and up, 0 at rest (joydev's and evdev's ABS_HAT0X and ABS_HAT0Y)
    Axes(usize),
}

/// Whether an axis's value is one a hat on two axes gives: -1, 0 or 1.
fn hat_value(v: f32) -> bool {
    [-1.0, 0.0, 1.0].iter().any(|h| (v - h).abs() < 1e-4)
}

/// Whether an axis's value is one a hat switch on one axis gives (`hat`):
/// one of its eight ways, 0 before its first report, or its rest past ±1.
fn pov_value(v: f32) -> bool {
    v.abs() > POV_REST || v == 0.0 || (0..8).any(|k| (v - (k as f32 * 2.0 / 7.0 - 1.0)).abs() < 1e-3)
}

/// Past this an axis is a hat switch at rest: its null state, which the
/// browsers scale past the end of its eight ways (Chromium's and Firefox's
/// 9/7 for a hat of 0 to 7 and a null of 8; a stick's axis never goes
/// past ±1 by more than a step of its own).
const POV_REST: f32 = 1.1;

/// Whether the pad's X and Y come swapped: Firefox's evdev layout on a pad
/// xpad drives, unless the page says otherwise.
pub fn swaps_xy(pad: &WebPad, b: Browser) -> bool {
    b.xy.unwrap_or_else(|| layout(pad, b) == Layout::Evdev && is_xpad(&pad.id))
}

/// A trigger's axis past a third of its travel (-1 at rest, 1 held down).
fn trigger_axis(v: f32) -> bool {
    v > -1.0 / 3.0
}

/// One pad's reading: what has been learned of it across frames.
#[derive(Clone, Debug, Default)]
pub struct PadMap {
    /// its trigger pair has read -1 together (the module's notes)
    pub triggers: bool,
    /// the axes as first seen, and which have moved from it since
    first: Option<Vec<f32>>,
    moved: Vec<bool>,
    /// each axis has read a value no hat on two axes gives (`hat_value`)
    analog: Vec<bool>,
    /// each axis has read a value no hat switch on one axis gives
    /// (`pov_value`)
    not_pov: Vec<bool>,
    /// the axis that has read a hat switch's rest (`POV_REST`), and nothing
    /// such a hat does not give since
    pov: Option<usize>,
    /// the pair of axes that has rested at 0 together, as a hat does: its
    /// first
    rested: Option<usize>,
    /// how it was last said to be read (`changed`)
    said: String,
}

/// The fewest axes a pad with a hat on two axes and two sticks has (the
/// sticks' four, then the hat's two).
const HAT_PAD_AXES: usize = 6;

impl PadMap {
    /// The axes the triggers are at, for this layout, if it has them:
    /// Firefox's evdev pads', and joydev's xpad order (Linux's alone, and
    /// never a pad whose own order is known).
    fn trigger_pair(pad: &WebPad, layout: Layout, b: Browser) -> Option<(usize, usize)> {
        match layout {
            Layout::Evdev if pad.axes.len() >= 6 => Some((4, 5)),
            Layout::Raw if b.linux && pad.axes.len() >= 6 && own_order(pad).is_none() => Some((2, 5)),
            _ => None,
        }
    }

    /// Whether axis `i` is one of a hat's two at the end of the pad's axes:
    /// they have rested at 0 together and given nothing but -1, 0 and 1
    /// (`raw_dpad`'s test, whatever its buttons).
    fn hat_axis(&self, pad: &WebPad, i: usize) -> bool {
        let n = pad.axes.len();
        let digital = |i: usize| !self.analog.get(i).copied().unwrap_or(true);
        n >= HAT_PAD_AXES && self.rested == Some(n - 2) && digital(n - 2) && digital(n - 1) && (i == n - 2 || i == n - 1)
    }

    /// What a read learns of the pad's axes: which give values no hat
    /// gives, which one has read a hat switch's rest, and which pair has
    /// rested at 0 together.
    fn learn(&mut self, pad: &WebPad) {
        let n = pad.axes.len();
        let ax = |i: usize| pad.axes.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        self.analog.resize(n, false);
        self.not_pov.resize(n, false);
        for i in 0..n {
            self.analog[i] |= !hat_value(ax(i));
            self.not_pov[i] |= !pov_value(ax(i));
        }
        if self.pov.is_none_or(|k| self.not_pov.get(k).copied().unwrap_or(true)) {
            self.pov = (0..n).find(|&i| !self.not_pov[i] && ax(i).abs() > POV_REST);
        }
        if n >= 2 && ax(n - 2) == 0.0 && ax(n - 1) == 0.0 {
            self.rested = Some(n - 2);
        }
    }

    /// Where a pad read positionally in its own order (`Layout::Raw`, no
    /// order of its own known) has its d-pad. Its hat, where it has one,
    /// on any axis that has read a hat switch's rest (Firefox on macOS puts
    /// a pad's axes in the order its report lists them, a hat switch among
    /// them on one axis); else, for a pad with too few buttons for a d-pad
    /// at 12 to 15 (a PS3 pad's twelve or thirteen), on its last two axes
    /// when they have rested at 0 together and read nothing but -1, 0 and
    /// 1 (joydev and Firefox's evdev give a pad's axes in the order of
    /// their codes, ABS_HAT0X and ABS_HAT0Y after the sticks); else buttons
    /// 12 to 15.
    fn raw_dpad(&self, pad: &WebPad) -> Dpad {
        if let Some(k) = self.pov.filter(|&k| !self.not_pov.get(k).copied().unwrap_or(true)) {
            return Dpad::Pov(k);
        }
        let n = pad.axes.len();
        if pad.buttons.len() < 16 && n >= HAT_PAD_AXES && self.hat_axis(pad, n - 2) {
            return Dpad::Axes(n - 2);
        }
        Dpad::Buttons
    }

    /// The d-pad's bits, read where `dpad` says.
    fn dpad_bits(pad: &WebPad, dpad: Dpad) -> u16 {
        let ax = |i: usize| pad.axes.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        match dpad {
            Dpad::Buttons => [(12, bit::UP), (13, bit::DOWN), (14, bit::LEFT), (15, bit::RIGHT)].iter().filter(|p| pad.buttons.get(p.0).is_some_and(|b| b.0)).fold(0, |a, p| a | p.1),
            Dpad::Pov(k) => hat(ax(k)),
            Dpad::Axes(k) => hat_ways(ax(k), ax(k + 1)),
        }
    }

    /// The d-pad in words, for `how`.
    fn dpad_words(dpad: Dpad) -> String {
        match dpad {
            Dpad::Buttons => ", the d-pad on buttons 12 to 15".to_string(),
            Dpad::Pov(k) => format!(", the d-pad on the hat at axis {k}"),
            Dpad::Axes(k) => format!(", the d-pad on the hat at axes {k} and {}", k + 1),
        }
    }

    /// The pad as libpad's report, its sticks as they came (no dead zone).
    pub fn read(&mut self, pad: &WebPad, b: Browser) -> Report {
        let layout = layout(pad, b);
        self.learn(pad);
        if let Some(r) = Self::read_own(pad, layout, b) {
            return r;
        }
        let ax = |i: usize| pad.axes.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        let pressed = |i: usize| pad.buttons.get(i).is_some_and(|b| b.0);
        let value = |i: usize| pad.buttons.get(i).map_or(0.0, |b| b.1);
        let now: Vec<f32> = (0..pad.axes.len()).map(ax).collect();
        let first = self.first.get_or_insert_with(|| now.clone());
        self.moved.resize(now.len(), false);
        for (i, v) in now.iter().enumerate() {
            if first.get(i) != Some(v) {
                self.moved[i] = true;
            }
        }
        let pair = Self::trigger_pair(pad, layout, b);
        // not, positionally, where the second is a hat's (a PS3 pad through
        // joydev, X Y Z Rz and its hat: the right stick left with the d-pad
        // up turned it into xpad's order for the rest of the session, its
        // d-pad read where it has none and L2 and R2 held); Firefox's evdev
        // layout has its hat on buttons 12 to 15
        if let Some((l, r)) = pair
            && ax(l) <= -0.9
            && ax(r) <= -0.9
            && (layout != Layout::Raw || !self.hat_axis(pad, r))
        {
            self.triggers = true;
        }
        // a trigger's axis that counts: xpad's once it has moved, any
        // other once the pair has rested at -1 together
        let xpad = layout == Layout::Evdev && is_xpad(&pad.id);
        let live = |axis: usize| self.triggers || (xpad && self.moved.get(axis).copied().unwrap_or(false));
        let (square, triangle) = if swaps_xy(pad, b) { (bit::TRIANGLE, bit::SQUARE) } else { (bit::SQUARE, bit::TRIANGLE) };
        let mut r = Report::default();
        let mut set = |on: bool, k: u16| {
            if on {
                r.buttons |= k;
            }
        };
        set(pressed(0), bit::CROSS);
        set(pressed(1), bit::CIRCLE);
        set(pressed(2), square);
        set(pressed(3), triangle);
        set(pressed(4), bit::L1);
        set(pressed(5), bit::R1);
        if layout == Layout::Raw && self.triggers {
            // xpad's own order (joydev's): the hat for the d-pad
            set(pressed(6), bit::SELECT);
            set(pressed(7), bit::START);
            set(pressed(9), bit::L3);
            set(pressed(10), bit::R3);
            set(trigger_axis(ax(2)), bit::L2);
            set(trigger_axis(ax(5)), bit::R2);
            set(ax(7) < -0.5, bit::UP);
            set(ax(7) > 0.5, bit::DOWN);
            set(ax(6) < -0.5, bit::LEFT);
            set(ax(6) > 0.5, bit::RIGHT);
            r.sticks = [ax(3), ax(4), ax(0), ax(1)];
            // the triggers' axes rest at -1: their pull
            r.triggers = [((ax(2) + 1.0) / 2.0).clamp(0.0, 1.0), ((ax(5) + 1.0) / 2.0).clamp(0.0, 1.0)];
            return r;
        }
        let trig = |i: usize, axis: usize| value(i) > 1.0 / 3.0 || (layout == Layout::Evdev && pair.is_some() && live(axis) && trigger_axis(ax(axis)));
        set(trig(6, 4), bit::L2);
        set(trig(7, 5), bit::R2);
        // the triggers' pull: their buttons' travel (an evdev pad's axes
        // say none here, and its buttons then stand for it)
        r.triggers = [value(6).clamp(0.0, 1.0), value(7).clamp(0.0, 1.0)];
        set(pressed(8), bit::SELECT);
        set(pressed(9), bit::START);
        set(pressed(10), bit::L3);
        set(pressed(11), bit::R3);
        // the d-pad: buttons 12 to 15, or a positional pad's hat
        let dpad = if layout == Layout::Raw { self.raw_dpad(pad) } else { Dpad::Buttons };
        r.buttons |= Self::dpad_bits(pad, dpad);
        // the sticks on the first four axes that are not a hat switch's
        let mut sticks = (0..pad.axes.len().max(4)).filter(|&i| dpad != Dpad::Pov(i));
        let mut next = || sticks.next().map_or(0.0, ax);
        let (lx, ly, rx, ry) = (next(), next(), next(), next());
        r.sticks = [rx, ry, lx, ly];
        r
    }

    /// Where the d-pad and the right stick's up and down are, for a pad read
    /// in its own order or by its usages (`read_own`): the hat's axis (or
    /// joydev's pair from it), else buttons 12 to 15, and the axis.
    fn own_axes(pad: &WebPad, layout: Layout) -> (Dpad, usize) {
        let n = pad.axes.len();
        match layout {
            // by their usages: the hat switch at 9, Rz at 5
            Layout::Usage => (if n > 9 { Dpad::Pov(9) } else { Dpad::Buttons }, if n > 5 { 5 } else { 3 }),
            // joydev's order of a HID pad: X Y Z Rz Gas Brake Hat0X Hat0Y
            _ => (if n > 7 { Dpad::Axes(6) } else { Dpad::Buttons }, 3),
        }
    }

    /// A pad read in its own order (`own_order`) or, in Chromium outside
    /// Linux, by its HID usages (the module's notes): its buttons in that
    /// order, or the standard layout's places; the d-pad on its hat; the
    /// triggers by their buttons' travel; the right stick on Z and Rz. None
    /// for every other pad.
    fn read_own(pad: &WebPad, layout: Layout, b: Browser) -> Option<Report> {
        let order = matches!(layout, Layout::Raw | Layout::Usage).then(|| own_order(pad)).flatten();
        if layout != Layout::Usage && order.is_none() {
            return None;
        }
        let ax = |i: usize| pad.axes.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        let pressed = |i: usize| pad.buttons.get(i).is_some_and(|b| b.0);
        let value = |i: usize| pad.buttons.get(i).map_or(0.0, |b| b.1);
        let mut r = Report::default();
        let swap = swaps_xy(pad, b);
        for (&k, &i) in ORDER_BITS.iter().zip(order.unwrap_or(POSITIONAL).iter()) {
            let i = i as usize;
            let on = match k {
                bit::L2 | bit::R2 => pressed(i) || value(i) > 1.0 / 3.0,
                _ => pressed(i),
            };
            // the triggers' pull, their buttons' travel
            match k {
                bit::L2 => r.triggers[0] = value(i).clamp(0.0, 1.0),
                bit::R2 => r.triggers[1] = value(i).clamp(0.0, 1.0),
                _ => {}
            }
            let k = match k {
                bit::SQUARE if swap => bit::TRIANGLE,
                bit::TRIANGLE if swap => bit::SQUARE,
                k => k,
            };
            if on {
                r.buttons |= k;
            }
        }
        let (dpad, ry) = Self::own_axes(pad, layout);
        r.buttons |= Self::dpad_bits(pad, dpad);
        r.sticks = [ax(2), ax(ry), ax(0), ax(1)];
        Some(r)
    }

    /// How the pad is read: its layout, its d-pad where it is not on buttons
    /// 12 to 15 by the layout's own rule, X and Y swapped, its triggers.
    pub fn how(&self, pad: &WebPad, b: Browser) -> String {
        let layout = layout(pad, b);
        let own = matches!(layout, Layout::Raw | Layout::Usage) && own_order(pad).is_some();
        let mut how = match layout {
            Layout::Standard => "the standard layout".to_string(),
            Layout::Evdev => "Firefox's evdev layout".to_string(),
            Layout::Raw if own => "8BitDo's D-input order, through joydev".to_string(),
            Layout::Usage if own => "8BitDo's D-input order, by its HID usages".to_string(),
            Layout::Usage => "its own order, positionally, by its HID usages".to_string(),
            Layout::Raw if self.triggers => "xpad's own order".to_string(),
            Layout::Raw => "its own order, positionally".to_string(),
        };
        if own || layout == Layout::Usage {
            let (dpad, ry) = Self::own_axes(pad, layout);
            how.push_str(&match dpad {
                Dpad::Axes(x) => format!(", the d-pad on axes {x} and {}", x + 1),
                d => Self::dpad_words(d),
            });
            how.push_str(&format!(", the right stick at axes 2 and {ry}"));
        } else if layout == Layout::Raw && !self.triggers {
            how.push_str(&Self::dpad_words(self.raw_dpad(pad)));
        }
        if swaps_xy(pad, b) {
            how.push_str(", X and Y swapped");
        }
        // (none where the pair's second is a hat's: no triggers there)
        if let Some((l, r)) = Self::trigger_pair(pad, layout, b)
            && (self.triggers || layout != Layout::Raw || !self.hat_axis(pad, r))
        {
            let known = self.triggers || (layout == Layout::Evdev && is_xpad(&pad.id));
            how.push_str(&format!(", triggers at axes {l} and {r}{}", if known { "" } else { " (not yet at rest)" }));
        }
        how
    }

    /// The pad as `?padtrace` prints it: its id and
    /// mapping, how it is read, every axis and every button pressed (an
    /// analog one with its value).
    pub fn describe(&self, pad: &WebPad, b: Browser) -> String {
        let how = self.how(pad, b);
        let axes: Vec<String> = pad.axes.iter().map(|v| format!("{v:.3}")).collect();
        let held: Vec<String> = pad
            .buttons
            .iter()
            .enumerate()
            .filter(|(_, b)| b.0 || b.1 > 0.0)
            .map(|(i, b)| if b.1 > 0.0 && b.1 < 1.0 { format!("{i}={:.2}", b.1) } else { i.to_string() })
            .collect();
        format!("pad {}: {:?}, mapping {:?} ({how}), {} buttons: axes [{}], pressed [{}]", pad.index, pad.id, pad.mapping, pad.buttons.len(), axes.join(", "), held.join(", "))
    }

    /// How the pad is read (`how`) when that changed since it was last
    /// asked (the first time it is asked too): what a read learned of it,
    /// its hat or its triggers found, for the log to say once.
    pub fn changed(&mut self, pad: &WebPad, b: Browser) -> Option<String> {
        let how = self.how(pad, b);
        (how != self.said).then(|| {
            self.said = how.clone();
            how
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX_LINUX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";
    const CHROME_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36";
    const CHROME_LINUX: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36";
    /// The player's (2026-11-04): Chrome 156 on Windows 11, which names
    /// itself Windows NT 10.0.
    const CHROME_156_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36";

    /// Chromium's hat switch at its rest (the null state 8 of 0..7) and in
    /// its eight ways, up first and on clockwise.
    const HAT_REST: f32 = 9.0 / 7.0;
    const HAT: [f32; 8] = [-1.0, -5.0 / 7.0, -3.0 / 7.0, -1.0 / 7.0, 1.0 / 7.0, 3.0 / 7.0, 5.0 / 7.0, 1.0];

    /// An 8BitDo Pro 2 in its D-input mode as Chrome on Windows gives it, at
    /// rest: no mapping, its buttons by their usage, its axes at their usage
    /// (the sticks on X Y and Z Rz, the hat at 9 at rest, the analog
    /// triggers, which Chrome puts in the free slots 3 and 4, at -1).
    fn pro2_windows() -> WebPad {
        pad("8BitDo Pro 2 (Vendor: 2dc8 Product: 6006)", "", 16, &[0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, HAT_REST])
    }

    fn pad(id: &str, mapping: &str, buttons: usize, axes: &[f32]) -> WebPad {
        WebPad { index: 0, id: id.into(), mapping: mapping.into(), buttons: vec![(false, 0.0); buttons], axes: axes.to_vec() }
    }

    fn press(p: &mut WebPad, i: usize, v: f32) {
        p.buttons[i] = (v > 0.0, v);
    }

    /// The tester's PowerA pad as Firefox on Linux gives it, at rest: the
    /// sticks where it rests (read off the pad: X 761, Y -2074, RX -1001,
    /// RY -4019 of -32768..32767), the triggers at -1.
    fn powera() -> WebPad {
        pad("20d6-2005-Generic X-Box pad", "standard", 17, &[0.023, -0.063, -0.031, -0.123, -1.0, -1.0])
    }

    #[test]
    fn the_browser_is_told_by_its_user_agent() {
        assert!(Browser::new(FIREFOX_LINUX, None).firefox_linux);
        assert!(!Browser::new(CHROME_WINDOWS, None).firefox_linux);
        assert!(!Browser::new("Mozilla/5.0 (Android 14; Mobile; rv:140.0) Gecko/140.0 Firefox/140.0", None).firefox_linux, "Firefox on Android maps pads itself");
        assert!(!Browser::new("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36", None).firefox_linux);
        assert_eq!(Browser::new(FIREFOX_LINUX, Some("keep")).xy, Some(false));
        assert_eq!(Browser::new(FIREFOX_LINUX, Some("swap")).xy, Some(true));
        assert_eq!(Browser::new(FIREFOX_LINUX, Some("bogus")).xy, None);
        // Chromium, and where (joydev is Linux's and ChromeOS's)
        let at = |ua: &str| {
            let b = Browser::new(ua, None);
            (b.chromium, b.linux)
        };
        assert_eq!(at(CHROME_156_WINDOWS), (true, false));
        assert_eq!(at("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36 Edg/156.0.0.0"), (true, false), "Edge");
        assert_eq!(at("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36"), (true, false));
        assert_eq!(at(CHROME_LINUX), (true, true));
        assert_eq!(at("Mozilla/5.0 (X11; CrOS x86_64 16181.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36"), (true, true), "ChromeOS");
        assert_eq!(at("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Mobile Safari/537.36"), (true, false), "Android maps pads itself");
        assert_eq!(at(FIREFOX_LINUX), (false, true));
        assert_eq!(at("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0"), (false, false));
        assert_eq!(at("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"), (false, false), "Safari");
    }

    /// Chromium's hat in `DpadFromAxis`'s bands: each of its eight ways, a
    /// diagonal both of its sides, and nothing at rest (past 1, or below -1
    /// for a hat whose ways are 1..8) or at 0, before its first report.
    #[test]
    fn a_hat_reads_as_chromium_reads_it() {
        let ways = [
            bit::UP,
            bit::UP | bit::RIGHT,
            bit::RIGHT,
            bit::RIGHT | bit::DOWN,
            bit::DOWN,
            bit::DOWN | bit::LEFT,
            bit::LEFT,
            bit::LEFT | bit::UP,
        ];
        for (v, want) in HAT.iter().zip(ways) {
            assert_eq!(hat(*v), want, "{v}");
        }
        assert_eq!(hat(HAT_REST), 0);
        assert_eq!(hat(-HAT_REST), 0);
        assert_eq!(hat(0.0), 0);
    }

    /// The player's pad (an 8BitDo Pro 2 in D-input, Chrome 156 on Windows
    /// 11): its own order, every button where SDL's database has it; the
    /// d-pad on the hat; the right stick on Z and Rz; at rest nothing held
    /// (its analog triggers at -1 are no buttons'). Its right stick up and
    /// left (-1 at 2 and 5 together, which turned the old reading into
    /// joydev's xpad order) leaves it as it was. The controls: the same
    /// buttons with no id it knows are positional (✕ at 0), and the same
    /// stick on Chrome on Linux with an xpad pad's joydev axes does turn it.
    #[test]
    fn chrome_on_windows_reads_an_8bitdo_in_dinput_in_its_own_order() {
        let chrome = Browser::new(CHROME_156_WINDOWS, None);
        let mut p = pro2_windows();
        let mut m = PadMap::default();
        assert_eq!(layout(&p, chrome), Layout::Usage);
        assert_eq!(m.read(&p, chrome), Report::default(), "at rest");
        // by its labels: B bottom, A right, Y left, X top; L R, ZL ZR, - +,
        // the sticks' clicks; 2, 5, 12 and 15 none of the game's (Home, the
        // paddles and Star)
        let want = [
            (0, bit::CIRCLE),
            (1, bit::CROSS),
            (2, 0),
            (3, bit::TRIANGLE),
            (4, bit::SQUARE),
            (5, 0),
            (6, bit::L1),
            (7, bit::R1),
            (8, bit::L2),
            (9, bit::R2),
            (10, bit::SELECT),
            (11, bit::START),
            (12, 0),
            (13, bit::L3),
            (14, bit::R3),
            (15, 0),
        ];
        for (i, k) in want {
            press(&mut p, i, 1.0);
            assert_eq!(m.read(&p, chrome).buttons, k, "button {i}");
            press(&mut p, i, 0.0);
        }
        let ways = [bit::UP, bit::UP | bit::RIGHT, bit::RIGHT, bit::RIGHT | bit::DOWN, bit::DOWN, bit::DOWN | bit::LEFT, bit::LEFT, bit::LEFT | bit::UP];
        for (v, k) in HAT.iter().zip(ways) {
            p.axes[9] = *v;
            assert_eq!(m.read(&p, chrome).buttons, k, "the hat at {v}");
        }
        p.axes[9] = HAT_REST;
        // the sticks: rx, ry, lx, ly
        p.axes = vec![0.5, -0.25, 0.75, -1.0, -1.0, -0.5, 0.0, 0.0, 0.0, HAT_REST];
        assert_eq!(m.read(&p, chrome).sticks, [0.75, -0.5, 0.5, -0.25]);
        // the right stick up and left, then back: no layout of joydev's
        p.axes = vec![0.0, 0.0, -1.0, -1.0, -1.0, -1.0, 0.0, 0.0, 0.0, HAT_REST];
        assert_eq!(m.read(&p, chrome).buttons, 0);
        let rest = pro2_windows();
        assert_eq!(m.read(&rest, chrome), Report::default(), "nothing held after");
        assert!(!m.triggers);
        let mut l = pro2_windows();
        press(&mut l, 6, 1.0);
        assert_eq!(m.read(&l, chrome).buttons, bit::L1, "L is still L1");
        assert!(m.how(&p, chrome).starts_with("8BitDo's D-input order, by its HID usages, the d-pad on the hat at axis 9, the right stick at axes 2 and 5"), "{}", m.how(&p, chrome));
        // the controls: an id it does not know is positional
        let mut other = pro2_windows();
        other.id = "Some pad (Vendor: 1234 Product: 5678)".into();
        press(&mut other, 0, 1.0);
        assert_eq!(PadMap::default().read(&other, chrome).buttons, bit::CROSS);
        // and joydev's xpad order turns on Linux alone
        let linux = Browser::new(CHROME_LINUX, None);
        let mut x = pad("Generic X-Box pad (Vendor: 20d6 Product: 2005)", "", 11, &[0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
        let mut mx = PadMap::default();
        mx.read(&x, linux);
        assert!(mx.triggers);
        press(&mut x, 7, 1.0);
        assert_eq!(mx.read(&x, linux).buttons, bit::START);
        let mut mw = PadMap::default();
        mw.read(&x, chrome);
        assert!(!mw.triggers, "on Windows no pair of axes is a trigger pair");
        assert_eq!(mw.read(&x, chrome).buttons, bit::R2, "button 7 by its place: R2");
    }

    /// The same 8BitDo through joydev (Chrome on Linux, hid-generic): its own
    /// order too, the d-pad on Hat0X and Hat0Y (6 and 7), the right stick on
    /// Z and Rz (2 and 3), the triggers on their buttons; its right stick
    /// left with L2's analog axis at rest (-1 at 2 and 5) turns nothing.
    #[test]
    fn chrome_on_linux_reads_an_8bitdo_in_dinput_through_joydev() {
        let linux = Browser::new(CHROME_LINUX, None);
        let mut p = pad("8BitDo Pro 2 (Vendor: 2dc8 Product: 6006)", "", 15, &[0.0, 0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0]);
        let mut m = PadMap::default();
        assert_eq!(layout(&p, linux), Layout::Raw);
        assert_eq!(m.read(&p, linux), Report::default());
        press(&mut p, 1, 1.0);
        assert_eq!(m.read(&p, linux).buttons, bit::CROSS);
        press(&mut p, 1, 0.0);
        press(&mut p, 11, 1.0);
        assert_eq!(m.read(&p, linux).buttons, bit::START);
        press(&mut p, 11, 0.0);
        p.axes[7] = -1.0;
        p.axes[6] = 1.0;
        assert_eq!(m.read(&p, linux).buttons, bit::UP | bit::RIGHT);
        p.axes[6] = 0.0;
        p.axes[7] = 0.0;
        p.axes[2] = -1.0;
        p.axes[3] = 0.5;
        let r = m.read(&p, linux);
        assert_eq!((r.buttons, r.sticks), (0, [-1.0, 0.5, 0.0, 0.0]));
        assert!(!m.triggers);
        assert!(m.how(&p, linux).starts_with("8BitDo's D-input order, through joydev, the d-pad on axes 6 and 7, the right stick at axes 2 and 3"), "{}", m.how(&p, linux));
    }

    /// Any pad Chromium has no mapping for, outside Linux: its buttons by the
    /// standard layout's places, the right stick on Z and Rz, the d-pad on the
    /// hat when it has one (then buttons 12 to 15 are not the d-pad), on
    /// buttons 12 to 15 when it has none (four axes: the right stick at 2 and
    /// 3 too).
    #[test]
    fn chromium_outside_linux_reads_any_unmapped_pad_by_its_usages() {
        let chrome = Browser::new(CHROME_WINDOWS, None);
        let mut p = pad("USB Gamepad (Vendor: 0079 Product: 0006)", "", 12, &[0.1, 0.2, 0.3, 0.0, 0.0, 0.4, 0.0, 0.0, 0.0, HAT_REST]);
        let mut m = PadMap::default();
        assert_eq!(layout(&p, chrome), Layout::Usage);
        assert_eq!(m.read(&p, chrome).sticks, [0.3, 0.4, 0.1, 0.2]);
        press(&mut p, 2, 1.0);
        assert_eq!(m.read(&p, chrome).buttons, bit::SQUARE);
        press(&mut p, 2, 0.0);
        press(&mut p, 9, 1.0);
        assert_eq!(m.read(&p, chrome).buttons, bit::START);
        press(&mut p, 9, 0.0);
        p.axes[9] = HAT[4];
        assert_eq!(m.read(&p, chrome).buttons, bit::DOWN);
        p.axes[9] = HAT_REST;
        let mut sixteen = pad("Other pad (Vendor: 1234 Product: 0001)", "", 16, &[0.0; 10]);
        press(&mut sixteen, 12, 1.0);
        assert_eq!(PadMap::default().read(&sixteen, chrome).buttons, 0, "a pad with a hat has no d-pad on its buttons");
        let mut four = pad("Four axes (Vendor: 1234 Product: 0002)", "", 16, &[0.1, 0.2, 0.3, 0.4]);
        press(&mut four, 12, 1.0);
        let r = PadMap::default().read(&four, chrome);
        assert_eq!((r.buttons, r.sticks), (bit::UP, [0.3, 0.4, 0.1, 0.2]));
        assert!(PadMap::default().how(&four, chrome).contains("the d-pad on buttons 12 to 15, the right stick at axes 2 and 3"));
    }

    const FIREFOX_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";
    const FIREFOX_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.5; rv:140.0) Gecko/20100101 Firefox/140.0";

    /// The d-pad's eight ways in `HAT`'s order (up first, on clockwise), and
    /// the same ways as a hat on two axes gives them (x, y; -1 left and up).
    const WAYS: [u16; 8] = [bit::UP, bit::UP | bit::RIGHT, bit::RIGHT, bit::RIGHT | bit::DOWN, bit::DOWN, bit::DOWN | bit::LEFT, bit::LEFT, bit::LEFT | bit::UP];
    const WAYS_XY: [(f32, f32); 8] = [(0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0), (-1.0, -1.0)];

    /// Firefox on Windows reads a pad it has no mapping for by its HID
    /// usages (WindowsGamepad.cpp: button n at n - 1, an axis at its usage
    /// less X's, the hat switch at 9, scaled as Chromium scales it): a
    /// third-party PS3 pad's d-pad is its hat at 9, each of its eight ways
    /// (a diagonal both of its sides), at rest past 1 (or, for a hat whose
    /// ways are 1 to 8, Firefox's unsigned sum far past it), nothing at 0
    /// before its first report; its right stick is Z and Rz. Read as the
    /// standard layout until now (the control: its buttons 12 to 15, which
    /// it does not have, and Rx for the stick's up and down), the d-pad did
    /// nothing. The same pad in Firefox on Linux is not read by its usages.
    #[test]
    fn firefox_on_windows_reads_an_unmapped_pads_hat_by_its_usages() {
        let ff = Browser::new(FIREFOX_WINDOWS, None);
        assert!(ff.firefox_windows && !ff.firefox_linux && !ff.chromium);
        let mut p = pad("0079-0006-Generic   USB  Joystick  ", "", 13, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, HAT_REST]);
        let mut m = PadMap::default();
        assert_eq!(layout(&p, ff), Layout::Usage);
        assert_eq!(m.read(&p, ff), Report::default(), "at rest");
        for (v, k) in HAT.iter().zip(WAYS) {
            p.axes[9] = *v;
            assert_eq!(m.read(&p, ff).buttons, k, "the hat at {v}");
        }
        for rest in [HAT_REST, 2.0 * 4_294_967_295.0 / 7.0 - 1.0, 0.0] {
            p.axes[9] = rest;
            assert_eq!(m.read(&p, ff).buttons, 0, "at rest: {rest}");
        }
        // the 13th button (PS) is none of the game's, and no d-pad
        press(&mut p, 12, 1.0);
        assert_eq!(m.read(&p, ff).buttons, 0);
        press(&mut p, 12, 0.0);
        p.axes = vec![0.25, -0.5, 0.75, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, HAT_REST];
        assert_eq!(m.read(&p, ff).sticks, [0.75, -1.0, 0.25, -0.5], "the right stick on Z and Rz");
        assert!(m.how(&p, ff).contains("the d-pad on the hat at axis 9, the right stick at axes 2 and 5"), "{}", m.how(&p, ff));
        // the control: the same pad on Linux is in its own order
        assert_eq!(layout(&p, Browser::new(FIREFOX_LINUX, None)), Layout::Raw);
        assert!(!Browser::new(FIREFOX_LINUX, None).firefox_windows);
    }

    /// A third-party PS3 pad (13 buttons, sticks on X Y and Z Rz, a hat)
    /// through joydev, as Chrome on Linux gives a pad it has no mapping for:
    /// its axes in the order of their codes, the hat's ABS_HAT0X and
    /// ABS_HAT0Y last (4 and 5), -1, 0 or 1. Its d-pad is the hat, each of
    /// its eight ways; its 13th button (PS) is no d-pad's up. The right stick
    /// left with the d-pad up (-1 at 2 and 5) no longer turns it into xpad's
    /// order (5 is its hat's): the d-pad stays, and nothing holds L2 or R2
    /// after. Its buttons 12 to 15 were the d-pad before, which it has not.
    #[test]
    fn chrome_on_linux_reads_a_ps3_pads_hat_on_its_last_two_axes() {
        let linux = Browser::new(CHROME_LINUX, None);
        let mut p = pad("PS3 GamePad (Vendor: 2563 Product: 0575)", "", 13, &[0.0; 6]);
        let mut m = PadMap::default();
        assert_eq!(layout(&p, linux), Layout::Raw);
        assert_eq!(m.read(&p, linux), Report::default(), "at rest");
        assert!(m.how(&p, linux).ends_with("the d-pad on the hat at axes 4 and 5"), "{}", m.how(&p, linux));
        for ((x, y), k) in WAYS_XY.iter().zip(WAYS) {
            (p.axes[4], p.axes[5]) = (*x, *y);
            assert_eq!(m.read(&p, linux).buttons, k, "the hat at ({x}, {y})");
        }
        (p.axes[4], p.axes[5]) = (0.0, 0.0);
        assert_eq!(m.read(&p, linux).buttons, 0, "let go");
        press(&mut p, 12, 1.0);
        assert_eq!(m.read(&p, linux).buttons, 0, "PS is no d-pad's up");
        press(&mut p, 12, 0.0);
        press(&mut p, 0, 1.0);
        assert_eq!(m.read(&p, linux).buttons, bit::CROSS, "its buttons by their places, as before");
        press(&mut p, 0, 0.0);
        // the right stick left (joydev's -1 at its end) with the d-pad up
        p.axes = vec![0.0, 0.0, -1.0, 0.0, 0.0, -1.0];
        assert_eq!(m.read(&p, linux).buttons, bit::UP);
        assert!(!m.triggers, "not xpad's order");
        p.axes = vec![0.0; 6];
        assert_eq!(m.read(&p, linux), Report::default(), "nothing held after");
        // the sticks where they were
        p.axes = vec![0.5, -0.25, 0.75, -0.5, 0.0, 0.0];
        assert_eq!(m.read(&p, linux).sticks, [0.75, -0.5, 0.5, -0.25]);
        // the controls: a joydev xpad pad still turns into xpad's order, its
        // eight axes, and its six with xpad's dpad_to_buttons (the triggers
        // last, resting at -1: no hat's)
        let mut x = pad("Generic X-Box pad (Vendor: 20d6 Product: 2005)", "", 11, &[0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
        let mut mx = PadMap::default();
        mx.read(&x, linux);
        assert!(mx.triggers);
        x.axes[7] = -1.0;
        assert_eq!(mx.read(&x, linux).buttons, bit::UP);
        let mut six = PadMap::default();
        six.read(&pad("Generic X-Box pad (Vendor: 20d6 Product: 2005)", "", 15, &[0.0, 0.0, -1.0, 0.0, 0.0, -1.0]), linux);
        assert!(six.triggers, "six axes, dpad_to_buttons");
    }

    /// Firefox on Linux reads a pad with no BTN_GAMEPAD (a PS3 pad through
    /// hid-generic: BTN_TRIGGER and on) through evdev with no mapping, its
    /// axes in the order of their codes and every one 0 until it first
    /// moves: the hat at its last two (6 and 7 of eight) is the d-pad from
    /// the start.
    #[test]
    fn firefox_on_linux_reads_a_ps3_pads_hat_on_its_last_two_axes() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let mut p = pad("2563-0575-SHANWAN PS3 GamePad", "", 13, &[0.0; 8]);
        let mut m = PadMap::default();
        assert_eq!(layout(&p, ff), Layout::Raw);
        assert_eq!(m.read(&p, ff), Report::default());
        p.axes[7] = 1.0;
        assert_eq!(m.read(&p, ff).buttons, bit::DOWN, "its first press");
        p.axes[6] = -1.0;
        assert_eq!(m.read(&p, ff).buttons, bit::DOWN | bit::LEFT);
        assert!(m.how(&p, ff).contains("the d-pad on the hat at axes 6 and 7"), "{}", m.how(&p, ff));
    }

    /// The last two axes are a hat only while they give what a hat gives
    /// and have rested at 0 together. A stick's axes there (a pad with the
    /// d-pad on its buttons) give the values between: once they have, they
    /// are never the d-pad again, a full push (-1) included, and the d-pad
    /// is on buttons 12 to 15. Triggers there resting at -1 never rested at
    /// 0: no d-pad held at rest. A pad with buttons 12 to 15 is read there,
    /// whatever its axes (the control).
    #[test]
    fn axes_that_give_what_no_hat_gives_are_no_d_pad() {
        let linux = Browser::new(CHROME_LINUX, None);
        let mut s = pad("Sticks (Vendor: 1234 Product: 0001)", "", 13, &[0.0; 6]);
        let mut m = PadMap::default();
        m.read(&s, linux);
        s.axes[5] = 0.4;
        m.read(&s, linux);
        s.axes[5] = -1.0;
        assert_eq!(m.read(&s, linux).buttons, 0, "a stick pushed all the way, once it has given 0.4");
        assert!(m.how(&s, linux).contains("the d-pad on buttons 12 to 15"), "{}", m.how(&s, linux));
        // triggers resting at -1 at the end
        let mut t = pad("Triggers (Vendor: 1234 Product: 0002)", "", 13, &[0.0, 0.0, 0.0, 0.0, -1.0, -1.0]);
        let mut mt = PadMap::default();
        assert_eq!(mt.read(&t, linux).buttons, 0, "at rest: nothing held");
        t.axes[4] = 1.0;
        assert_eq!(mt.read(&t, linux).buttons, 0, "pulled all the way at once");
        // sixteen buttons: the d-pad on 12 to 15, the axes left alone
        let mut b = pad("Sixteen (Vendor: 1234 Product: 0003)", "", 16, &[0.0; 6]);
        let mut mb = PadMap::default();
        b.axes[4] = 1.0;
        assert_eq!(mb.read(&b, linux).buttons, 0);
        press(&mut b, 13, 1.0);
        assert_eq!(mb.read(&b, linux).buttons, bit::DOWN);
        // four axes are two sticks: no hat
        let mut f = pad("Four (Vendor: 1234 Product: 0004)", "", 12, &[0.0; 4]);
        f.axes[3] = 1.0;
        assert_eq!(PadMap::default().read(&f, linux).buttons, 0);
    }

    /// Firefox on macOS lists an unmapped pad's axes in its report's order, a
    /// hat switch on one axis among them (here between the sticks), 0 until
    /// it first reports. Once it has rested past 1 it is the d-pad, each of
    /// its eight ways, and the sticks are the four axes that are not it. Its
    /// first press, before any rest was seen, is not the d-pad's (it could be
    /// a stick's). An axis that has given a stick's value is never a hat
    /// switch, past 1 or not (the control).
    #[test]
    fn firefox_on_macos_finds_a_hat_switch_among_the_axes() {
        let ff = Browser::new(FIREFOX_MAC, None);
        let mut p = pad("0079-0006-Generic   USB  Joystick  ", "", 12, &[0.0; 5]);
        let mut m = PadMap::default();
        assert_eq!(layout(&p, ff), Layout::Raw);
        p.axes[2] = HAT[0];
        assert_eq!(m.read(&p, ff).buttons, 0, "a press before its rest was seen");
        p.axes[2] = HAT_REST;
        assert_eq!(m.read(&p, ff).buttons, 0, "at rest");
        for (v, k) in HAT.iter().zip(WAYS) {
            p.axes[2] = *v;
            assert_eq!(m.read(&p, ff).buttons, k, "the hat at {v}");
        }
        p.axes = vec![0.25, -0.5, HAT_REST, 0.75, -1.0];
        assert_eq!(m.read(&p, ff).sticks, [0.75, -1.0, 0.25, -0.5], "the sticks round the hat");
        assert!(m.how(&p, ff).ends_with("the d-pad on the hat at axis 2"), "{}", m.how(&p, ff));
        // the control: a stick's value, then past 1: no hat switch
        let mut q = pad("Other (Vendor: 1234 Product: 0005)", "", 12, &[0.0; 5]);
        let mut n = PadMap::default();
        q.axes[3] = 0.5;
        n.read(&q, ff);
        q.axes[3] = 1.3;
        n.read(&q, ff);
        q.axes[3] = -1.0;
        assert_eq!(n.read(&q, ff).buttons, 0);
    }

    /// `changed` says how a pad is read the first time, and again once a
    /// read has found its hat; not otherwise.
    #[test]
    fn how_a_pad_is_read_is_said_when_it_changes() {
        let ff = Browser::new(FIREFOX_MAC, None);
        let mut p = pad("0079-0006-Generic   USB  Joystick  ", "", 12, &[0.0; 5]);
        let mut m = PadMap::default();
        m.read(&p, ff);
        assert!(m.changed(&p, ff).is_some_and(|h| h.ends_with("the d-pad on buttons 12 to 15")));
        m.read(&p, ff);
        assert_eq!(m.changed(&p, ff), None, "nothing new");
        p.axes[2] = HAT_REST;
        m.read(&p, ff);
        assert!(m.changed(&p, ff).is_some_and(|h| h.ends_with("the d-pad on the hat at axis 2")));
        assert_eq!(m.changed(&p, ff), None);
    }

    #[test]
    fn ids_give_their_vendor_product_and_name() {
        assert_eq!(id_parts("20d6-2005-Generic X-Box pad"), (Some(0x20d6), Some(0x2005), "Generic X-Box pad"));
        assert_eq!(id_parts("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), (None, None, "Xbox 360 Controller (XInput STANDARD GAMEPAD)"));
        assert_eq!(id_parts("DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)"), (Some(0x054c), Some(0x0ce6), "DualSense Wireless Controller"));
        // Chrome's id for a pad it has no mapping for: no space before Vendor
        assert_eq!(id_parts("8BitDo Pro 2 (Vendor: 2dc8 Product: 6006)"), (Some(0x2dc8), Some(0x6006), "8BitDo Pro 2"));
        assert_eq!(own_order(&pro2_windows()), Some(EIGHTBITDO_DINPUT));
        assert_eq!(own_order(&pad("8BitDo Ultimate 2C (Vendor: 2dc8 Product: 310a)", "", 16, &[])), None, "an 8BitDo in Xbox's order");
        assert_eq!(own_order(&pad("Xbox 360 Controller (XInput STANDARD GAMEPAD)", "standard", 17, &[])), None);
        // a name with dashes is still a name
        assert_eq!(id_parts("drive.py pad"), (None, None, "drive.py pad"));
        assert_eq!(id_parts("8BitDo-Pro-2"), (None, None, "8BitDo-Pro-2"));
        assert!(is_xpad("20d6-2005-Generic X-Box pad"));
        assert!(is_xpad("24c6-551a-PowerA FUSION Pro Controller"), "a vendor xpad takes, no Xbox in the name");
        assert!(is_xpad("045e-028e-Microsoft X-Box 360 pad"));
        assert!(!is_xpad("054c-0ce6-Sony Interactive Entertainment DualSense Wireless Controller"));
        assert!(!is_xpad("057e-2009-Nintendo Switch Pro Controller"));
    }

    /// Firefox on Linux, xpad's pad: its X (BTN_X == BTN_NORTH, index 3) is □
    /// and its Y (index 2) △; the triggers are axes 4 and 5. The same pad read
    /// as the standard layout (another browser) is the control: unswapped.
    #[test]
    fn firefox_on_linux_reads_xpads_x_as_square_and_its_trigger_axes() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let mut p = powera();
        let mut m = PadMap::default();
        assert_eq!(m.read(&p, ff).buttons, 0, "at rest nothing is held: the triggers at -1 are let go");
        assert!(m.triggers, "the triggers rested at -1 together");
        press(&mut p, 3, 1.0);
        assert_eq!(m.read(&p, ff).buttons, bit::SQUARE, "the left button");
        press(&mut p, 3, 0.0);
        press(&mut p, 2, 1.0);
        assert_eq!(m.read(&p, ff).buttons, bit::TRIANGLE, "the top button");
        // the control: the same buttons as the spec's layout
        let chrome = Browser::new(CHROME_WINDOWS, None);
        assert_eq!(PadMap::default().read(&p, chrome).buttons, bit::SQUARE, "index 2 is the left button in the standard layout");
        // and `?padxy=keep` leaves Firefox's as it came
        assert_eq!(PadMap::default().read(&p, Browser::new(FIREFOX_LINUX, Some("keep"))).buttons, bit::SQUARE);
        press(&mut p, 2, 0.0);
        // the triggers: past a third of their travel, either one
        for (l, r, want) in [(-0.5, -1.0, 0), (-0.2, -1.0, bit::L2), (-1.0, 1.0, bit::R2), (0.0, 0.0, bit::L2 | bit::R2)] {
            p.axes[4] = l;
            p.axes[5] = r;
            assert_eq!(m.read(&p, ff).buttons, want, "LT {l} RT {r}");
        }
        // the sticks as they came: rx, ry, lx, ly
        assert_eq!(m.read(&powera(), ff).sticks, [-0.031, -0.123, 0.023, -0.063]);
    }

    /// On a pad xpad does not drive, axes 4 and 5 that never read -1 together
    /// are not triggers: a generic HID pad's right stick is there (ABS_Z and
    /// ABS_RZ), resting at 0, which as a trigger would be L2 held.
    #[test]
    fn axes_that_never_rested_at_minus_one_are_no_triggers() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let mut p = pad("0810-0001-USB Gamepad", "standard", 17, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let mut m = PadMap::default();
        assert_eq!(m.read(&p, ff).buttons, 0);
        // one of them at -1 is not enough (a stick pushed one way)
        p.axes[4] = -1.0;
        assert_eq!(m.read(&p, ff).buttons, 0);
        p.axes[4] = 0.0;
        assert_eq!(m.read(&p, ff).buttons, 0, "still no trigger");
        // the control: both at -1 together, then a trigger's push counts
        p.axes[4] = -1.0;
        p.axes[5] = -1.0;
        m.read(&p, ff);
        p.axes[4] = 0.5;
        assert_eq!(m.read(&p, ff).buttons, bit::L2);
    }

    /// Firefox gives every axis as 0 until it sends an event (it reads no
    /// initial values), a trigger at rest included: on Firefox 140 the clone
    /// of the tester's pad read [0, 0, 0, 0, 0, 0] until each axis moved. On
    /// xpad's pad a trigger counts as soon as it moves from that, its first
    /// press included; at 0 it is no press (the control: the naive rule would
    /// hold L2 and R2 from the start). The same steps on a pad xpad does not
    /// drive (the other control) wait for the pair to rest at -1 together.
    #[test]
    fn firefox_gives_axes_as_zero_until_they_move_and_xpads_triggers_count_at_once() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let steps: [([f32; 2], u16); 5] = [([0.0, 0.0], 0), ([0.369, 0.0], bit::L2), ([-1.0, 0.0], 0), ([-1.0, 0.369], bit::R2), ([-1.0, -1.0], 0)];
        let mut p = pad("20d6-2005-Generic X-Box pad", "standard", 17, &[0.0; 6]);
        let mut m = PadMap::default();
        for (axes, want) in steps {
            p.axes[4] = axes[0];
            p.axes[5] = axes[1];
            assert_eq!(m.read(&p, ff).buttons, want, "xpad, LT {} RT {}", axes[0], axes[1]);
        }
        let mut q = pad("0810-0001-USB Gamepad", "standard", 17, &[0.0; 6]);
        let mut n = PadMap::default();
        for (axes, _) in steps {
            q.axes[4] = axes[0];
            q.axes[5] = axes[1];
            assert_eq!(n.read(&q, ff).buttons, 0, "a generic pad, LT {} RT {}", axes[0], axes[1]);
        }
        q.axes[4] = 0.5;
        assert_eq!(n.read(&q, ff).buttons, bit::L2, "once the pair has rested at -1");
    }

    /// Firefox on Linux, a DualSense: hid-playstation sends △ as BTN_NORTH and
    /// □ as BTN_WEST, so it comes right; its digital L2 is button 6. The same
    /// buttons with an xpad pad's id (the control) are swapped.
    #[test]
    fn firefox_on_linux_reads_a_dualsense_as_it_comes() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let mut p = pad("054c-0ce6-Sony Interactive Entertainment DualSense Wireless Controller", "standard", 17, &[0.0, 0.0, 0.0, 0.0, -1.0, -1.0]);
        let mut m = PadMap::default();
        press(&mut p, 3, 1.0);
        assert_eq!(m.read(&p, ff).buttons, bit::TRIANGLE);
        press(&mut p, 3, 0.0);
        press(&mut p, 2, 1.0);
        assert_eq!(m.read(&p, ff).buttons, bit::SQUARE);
        press(&mut p, 2, 0.0);
        // L2 by its button, and by its axis
        press(&mut p, 6, 1.0);
        assert_eq!(m.read(&p, ff).buttons, bit::L2);
        press(&mut p, 6, 0.0);
        p.axes[4] = 0.0;
        assert_eq!(m.read(&p, ff).buttons, bit::L2);
        p.axes[4] = -1.0;
        // the control: an xpad id on the same buttons
        press(&mut p, 3, 1.0);
        p.id = "045e-028e-Microsoft X-Box 360 pad".into();
        assert_eq!(m.read(&p, ff).buttons, bit::SQUARE);
    }

    /// The standard layout (Chrome, Edge, Safari, Firefox elsewhere): the
    /// spec's indices, an Xbox pad's X at 2 left as it is (the control for
    /// Firefox's rule), the triggers' values, the d-pad, the sticks.
    #[test]
    fn the_standard_layout_is_the_specs() {
        let chrome = Browser::new(CHROME_WINDOWS, None);
        let mut p = pad("Xbox 360 Controller (XInput STANDARD GAMEPAD)", "standard", 17, &[0.5, -0.25, -0.75, 1.0]);
        let mut m = PadMap::default();
        let names = [
            bit::CROSS,
            bit::CIRCLE,
            bit::SQUARE,
            bit::TRIANGLE,
            bit::L1,
            bit::R1,
            bit::L2,
            bit::R2,
            bit::SELECT,
            bit::START,
            bit::L3,
            bit::R3,
            bit::UP,
            bit::DOWN,
            bit::LEFT,
            bit::RIGHT,
        ];
        for (i, &k) in names.iter().enumerate() {
            press(&mut p, i, 1.0);
            assert_eq!(m.read(&p, chrome).buttons, k, "button {i}");
            press(&mut p, i, 0.0);
        }
        // meta (16) is none of the game's
        press(&mut p, 16, 1.0);
        assert_eq!(m.read(&p, chrome).buttons, 0);
        press(&mut p, 16, 0.0);
        // an analog trigger counts past a third
        press(&mut p, 7, 0.3);
        assert_eq!(m.read(&p, chrome).buttons, 0);
        press(&mut p, 7, 0.4);
        assert_eq!(m.read(&p, chrome).buttons, bit::R2);
        assert_eq!(m.read(&p, chrome).sticks, [-0.75, 1.0, 0.5, -0.25]);
        press(&mut p, 7, 0.0);
        // a standard pad has no trigger axes, even with six of them
        let mut six = pad("Some pad (STANDARD GAMEPAD Vendor: 1234 Product: 5678)", "standard", 17, &[0.0, 0.0, 0.0, 0.0, -1.0, -1.0]);
        let mut m6 = PadMap::default();
        m6.read(&six, chrome);
        six.axes[4] = 1.0;
        assert_eq!(m6.read(&six, chrome).buttons, 0);
        // `?padxy=swap` swaps it anywhere
        press(&mut p, 2, 1.0);
        assert_eq!(PadMap::default().read(&p, Browser::new(CHROME_WINDOWS, Some("swap"))).buttons, bit::TRIANGLE);
    }

    /// No mapping on Linux (joydev's, Chrome there): positionally as the
    /// standard layout; in xpad's own order once axes 2 and 5 rest at -1
    /// together: Back and Start at 6 and 7, the right stick at 3 and 4, the
    /// d-pad on the hat (6 and 7). A four-axis pad and one whose axes 2 and 5
    /// rest at 0 are the controls.
    #[test]
    fn no_mapping_is_positional_and_xpads_own_order_by_its_triggers() {
        let chrome = Browser::new(CHROME_LINUX, None);
        let mut x = pad("Generic X-Box pad (Vendor: 20d6 Product: 2005)", "", 11, &[0.1, 0.2, -1.0, 0.3, 0.4, -1.0, 0.0, 0.0]);
        let mut m = PadMap::default();
        press(&mut x, 7, 1.0);
        assert_eq!(m.read(&x, chrome).buttons, bit::START);
        press(&mut x, 7, 0.0);
        press(&mut x, 6, 1.0);
        assert_eq!(m.read(&x, chrome).buttons, bit::SELECT);
        press(&mut x, 6, 0.0);
        press(&mut x, 9, 1.0);
        assert_eq!(m.read(&x, chrome).buttons, bit::L3);
        press(&mut x, 9, 0.0);
        assert_eq!(m.read(&x, chrome).sticks, [0.3, 0.4, 0.1, 0.2]);
        x.axes[6] = -1.0;
        x.axes[2] = 1.0;
        assert_eq!(m.read(&x, chrome).buttons, bit::LEFT | bit::L2);
        // the controls: four axes, positional
        let mut four = pad("USB Gamepad (Vendor: 0079 Product: 0006)", "", 12, &[0.1, 0.2, 0.3, 0.4]);
        let mut m4 = PadMap::default();
        press(&mut four, 7, 1.0);
        assert_eq!(m4.read(&four, chrome).buttons, bit::R2);
        assert_eq!(m4.read(&four, chrome).sticks, [0.3, 0.4, 0.1, 0.2]);
        // six axes, 2 and 5 resting at 0: positional too
        let mut six = pad("Other pad", "", 12, &[0.1, 0.2, 0.0, 0.3, 0.4, 0.0]);
        let mut m6 = PadMap::default();
        press(&mut six, 9, 1.0);
        assert_eq!(m6.read(&six, chrome).buttons, bit::START);
        assert_eq!(m6.read(&six, chrome).sticks, [0.0, 0.3, 0.1, 0.2]);
    }

    #[test]
    fn an_axis_that_is_no_number_reads_as_rest() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let p = pad("20d6-2005-Generic X-Box pad", "standard", 17, &[f32::NAN, f32::INFINITY, 0.5]);
        assert_eq!(PadMap::default().read(&p, ff).sticks, [0.5, 0.0, 0.0, 0.0]);
        // a short pad: what it lacks is at rest
        assert_eq!(PadMap::default().read(&pad("x", "standard", 2, &[]), ff), Report::default());
    }

    /// The tester's pad at rest (its right stick 0.127 off its middle),
    /// through the host's dead zone and `VIPad`: the game reads nothing at
    /// all, no key and no stick. The control: the same pad with its right
    /// stick pushed halfway is a push to the game.
    #[test]
    fn the_testers_pad_at_rest_is_at_rest_to_the_game() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let mut m = PadMap::default();
        let game = |m: &mut PadMap, p: &WebPad| {
            let mut r = m.read(p, ff);
            r.rest(crate::input::STICK_DEAD_ZONE);
            let mut c = crate::input::Console::new();
            c.poll(r, true, false, false);
            (c.poll(r, true, false, false), c.sticks())
        };
        assert_eq!(game(&mut m, &powera()), (Vec::new(), [0.0; 4]));
        let mut pushed = powera();
        pushed.axes[2] = 0.5;
        let (keys, sticks) = game(&mut m, &pushed);
        assert_eq!(keys, vec![(128, true)], "rx's key");
        assert!(sticks[0] > 0.3, "{sticks:?}");
    }

    #[test]
    fn a_report_names_the_pad_its_layout_axes_and_buttons() {
        let ff = Browser::new(FIREFOX_LINUX, None);
        let mut p = powera();
        press(&mut p, 3, 1.0);
        p.buttons[7] = (true, 0.5);
        let mut m = PadMap::default();
        m.read(&p, ff);
        let d = m.describe(&p, ff);
        assert_eq!(
            d,
            "pad 0: \"20d6-2005-Generic X-Box pad\", mapping \"standard\" (Firefox's evdev layout, X and Y swapped, triggers at axes 4 and 5), 17 buttons: axes [0.023, -0.063, -0.031, -0.123, -1.000, -1.000], pressed [3, 7=0.50]"
        );
        assert!(PadMap::default().describe(&p, Browser::new(CHROME_WINDOWS, None)).contains("(the standard layout)"));
    }
}
