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
//!   triggers resting at -1 at axes 2 and 5.
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

use crate::input::{Report, bit};

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
}

/// The browser, as far as the layout goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Browser {
    /// Firefox reading evdev itself (on Linux; not Android's)
    pub firefox_linux: bool,
    /// the page's `?padxy=`: `swap` (Some(true)) or `keep` (Some(false))
    pub xy: Option<bool>,
}

impl Browser {
    /// From `navigator.userAgent` and the page's `?padxy=`.
    pub fn new(user_agent: &str, padxy: Option<&str>) -> Browser {
        let firefox_linux = user_agent.contains("Firefox/") && user_agent.contains("Linux") && !user_agent.contains("Android");
        let xy = match padxy {
            Some("swap") => Some(true),
            Some("keep") => Some(false),
            _ => None,
        };
        Browser { firefox_linux, xy }
    }
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
/// "<name> (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)".
pub fn id_parts(id: &str) -> (Option<u16>, Option<u16>, &str) {
    let hex = |s: &str| (s.len() == 4).then(|| u16::from_str_radix(s, 16).ok()).flatten();
    let mut it = id.splitn(3, '-');
    if let (Some(v), Some(p), Some(name)) = (it.next(), it.next(), it.next())
        && let (Some(v), Some(p)) = (hex(v), hex(p))
    {
        return (Some(v), Some(p), name);
    }
    if let Some(at) = id.find(" Vendor: ") {
        let rest = &id[at + 9..];
        let v = rest.get(..4).and_then(hex);
        let p = rest.find("Product: ").and_then(|i| rest.get(i + 9..i + 13)).and_then(hex);
        let name = id[..at].rsplit_once(" (").map_or(&id[..at], |(n, _)| n);
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
        _ => Layout::Raw,
    }
}

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
}

impl PadMap {
    /// The axes the triggers are at, for this layout, if it has them.
    fn trigger_pair(pad: &WebPad, layout: Layout) -> Option<(usize, usize)> {
        match layout {
            Layout::Evdev if pad.axes.len() >= 6 => Some((4, 5)),
            Layout::Raw if pad.axes.len() >= 6 => Some((2, 5)),
            _ => None,
        }
    }

    /// The pad as libpad's report, its sticks as they came (no dead zone).
    pub fn read(&mut self, pad: &WebPad, b: Browser) -> Report {
        let layout = layout(pad, b);
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
        let pair = Self::trigger_pair(pad, layout);
        if let Some((l, r)) = pair
            && ax(l) <= -0.9
            && ax(r) <= -0.9
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
            return r;
        }
        let trig = |i: usize, axis: usize| value(i) > 1.0 / 3.0 || (layout == Layout::Evdev && pair.is_some() && live(axis) && trigger_axis(ax(axis)));
        set(trig(6, 4), bit::L2);
        set(trig(7, 5), bit::R2);
        set(pressed(8), bit::SELECT);
        set(pressed(9), bit::START);
        set(pressed(10), bit::L3);
        set(pressed(11), bit::R3);
        set(pressed(12), bit::UP);
        set(pressed(13), bit::DOWN);
        set(pressed(14), bit::LEFT);
        set(pressed(15), bit::RIGHT);
        r.sticks = [ax(2), ax(3), ax(0), ax(1)];
        r
    }

    /// How the pad is read: its layout, X and Y swapped, its triggers.
    pub fn how(&self, pad: &WebPad, b: Browser) -> String {
        let layout = layout(pad, b);
        let mut how = match layout {
            Layout::Standard => "the standard layout".to_string(),
            Layout::Evdev => "Firefox's evdev layout".to_string(),
            Layout::Raw if self.triggers => "xpad's own order".to_string(),
            Layout::Raw => "its own order, positionally".to_string(),
        };
        if swaps_xy(pad, b) {
            how.push_str(", X and Y swapped");
        }
        if let Some((l, r)) = Self::trigger_pair(pad, layout) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX_LINUX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";
    const CHROME_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36";

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
    }

    #[test]
    fn ids_give_their_vendor_product_and_name() {
        assert_eq!(id_parts("20d6-2005-Generic X-Box pad"), (Some(0x20d6), Some(0x2005), "Generic X-Box pad"));
        assert_eq!(id_parts("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), (None, None, "Xbox 360 Controller (XInput STANDARD GAMEPAD)"));
        assert_eq!(id_parts("DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)"), (Some(0x054c), Some(0x0ce6), "DualSense Wireless Controller"));
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

    /// No mapping: positionally as the standard layout; in xpad's own order
    /// (joydev's) once axes 2 and 5 rest at -1 together: Back and Start at 6
    /// and 7, the right stick at 3 and 4, the d-pad on the hat (6 and 7). A
    /// four-axis pad and one whose axes 2 and 5 rest at 0 are the controls.
    #[test]
    fn no_mapping_is_positional_and_xpads_own_order_by_its_triggers() {
        let chrome = Browser::new(CHROME_WINDOWS, None);
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
