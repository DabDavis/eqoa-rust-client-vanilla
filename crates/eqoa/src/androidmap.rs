//! Android's input as the game reads it (`android`'s mappings, here so the
//! host's tests check them too): a controller's keys and axes as libpad's
//! buttons and sticks, a key's code and character as winit's logical key.

use crate::input::{Report, bit};
use winit::keyboard::{Key, NamedKey};

/// A controller's axes as Android names them: the sticks (AXIS_X, AXIS_Y;
/// the right one AXIS_Z and AXIS_RZ, or AXIS_RX and AXIS_RY where it has no
/// AXIS_RZ, as the activity picks), the hat (the d-pad on most controllers)
/// and the triggers, -1..1 (+y down) and 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Axes {
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub hat_x: f32,
    pub hat_y: f32,
    pub lt: f32,
    pub rt: f32,
}

/// How far a trigger goes before it counts as L2 or R2 (as gilrs's are
/// read, `input`).
const TRIGGER: f32 = 0.33;

/// A controller's key as libpad's button (`input::bit`), by position:
/// Android names a controller's buttons as an Xbox controller's are laid
/// out (A the bottom one, ✕), as SDL's layout does.
pub fn key_bit(code: i32) -> u16 {
    match code {
        96 => bit::CROSS,     // KEYCODE_BUTTON_A
        97 => bit::CIRCLE,    // KEYCODE_BUTTON_B
        99 => bit::SQUARE,    // KEYCODE_BUTTON_X
        100 => bit::TRIANGLE, // KEYCODE_BUTTON_Y
        102 => bit::L1,       // KEYCODE_BUTTON_L1
        103 => bit::R1,       // KEYCODE_BUTTON_R1
        104 => bit::L2,       // KEYCODE_BUTTON_L2
        105 => bit::R2,       // KEYCODE_BUTTON_R2
        106 => bit::L3,       // KEYCODE_BUTTON_THUMBL
        107 => bit::R3,       // KEYCODE_BUTTON_THUMBR
        108 => bit::START,    // KEYCODE_BUTTON_START
        109 => bit::SELECT,   // KEYCODE_BUTTON_SELECT
        19 => bit::UP,        // KEYCODE_DPAD_UP
        20 => bit::DOWN,      // KEYCODE_DPAD_DOWN
        21 => bit::LEFT,      // KEYCODE_DPAD_LEFT
        22 => bit::RIGHT,     // KEYCODE_DPAD_RIGHT
        _ => 0,
    }
}

/// A controller as one source's report: the buttons its keys hold (`keys`,
/// `key_bit`'s), its hat as the d-pad and its triggers past a third as L2
/// and R2, its sticks in libpad's order (the right one, then the left).
pub fn report(keys: u16, a: &Axes) -> Report {
    let mut buttons = keys;
    for (on, b) in [(a.hat_x < -0.5, bit::LEFT), (a.hat_x > 0.5, bit::RIGHT), (a.hat_y < -0.5, bit::UP), (a.hat_y > 0.5, bit::DOWN), (a.lt > TRIGGER, bit::L2), (a.rt > TRIGGER, bit::R2)] {
        if on {
            buttons |= b;
        }
    }
    Report { buttons, sticks: [a.rx, a.ry, a.lx, a.ly] }
}

/// A key's code (Android's `KEYCODE_*`) and the character it types (0 for
/// none) as the game's dispatch takes keys: winit's logical key, and its
/// text.
pub fn key_of(code: i32, unicode: u32) -> Option<(Key, Option<String>)> {
    let named = |n: NamedKey| Some((Key::Named(n), None));
    match code {
        66 | 160 => return Some((Key::Named(NamedKey::Enter), Some("\r".into()))), // ENTER, NUMPAD_ENTER
        67 => return named(NamedKey::Backspace),                                  // DEL
        112 => return named(NamedKey::Delete),                                    // FORWARD_DEL
        61 => return Some((Key::Named(NamedKey::Tab), Some("\t".into()))),        // TAB
        111 => return named(NamedKey::Escape),                                    // ESCAPE
        62 => return Some((Key::Named(NamedKey::Space), Some(" ".into()))),       // SPACE
        19 => return named(NamedKey::ArrowUp),
        20 => return named(NamedKey::ArrowDown),
        21 => return named(NamedKey::ArrowLeft),
        22 => return named(NamedKey::ArrowRight),
        92 => return named(NamedKey::PageUp),
        93 => return named(NamedKey::PageDown),
        122 => return named(NamedKey::Home),
        123 => return named(NamedKey::End),
        124 => return named(NamedKey::Insert),
        59 | 60 => return named(NamedKey::Shift),
        113 | 114 => return named(NamedKey::Control),
        57 | 58 => return named(NamedKey::Alt),
        131..=142 => {
            const F: [NamedKey; 12] = [
                NamedKey::F1,
                NamedKey::F2,
                NamedKey::F3,
                NamedKey::F4,
                NamedKey::F5,
                NamedKey::F6,
                NamedKey::F7,
                NamedKey::F8,
                NamedKey::F9,
                NamedKey::F10,
                NamedKey::F11,
                NamedKey::F12,
            ];
            return named(F[(code - 131) as usize]);
        }
        _ => {}
    }
    let c = char::from_u32(unicode).filter(|c| !c.is_control())?;
    Some((Key::Character(c.to_string().into()), Some(c.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A controller's face buttons by position, its d-pad from its keys or
    /// its hat, its triggers past a third of their travel, its sticks in
    /// libpad's order.
    #[test]
    fn a_controller_reads_as_libpads_buttons() {
        assert_eq!(key_bit(96), bit::CROSS);
        assert_eq!(key_bit(100), bit::TRIANGLE);
        assert_eq!(key_bit(108), bit::START);
        assert_eq!(key_bit(4), 0, "BACK is not a controller's button");
        let a = Axes { lx: 0.5, ly: -1.0, rx: 0.25, ry: 0.0, hat_x: 1.0, hat_y: 0.0, lt: 0.2, rt: 0.9 };
        let r = report(bit::CROSS, &a);
        assert_eq!(r.buttons, bit::CROSS | bit::RIGHT | bit::R2);
        assert_eq!(r.sticks, [0.25, 0.0, 0.5, -1.0]);
        // the control: nothing held reads as nothing
        assert_eq!(report(0, &Axes::default()), Report::default());
    }

    /// A key as the dispatch takes it: named keys by name, a character as
    /// itself with its text, nothing for a key that types nothing.
    #[test]
    fn keys_read_as_winits() {
        assert_eq!(key_of(66, 10), Some((Key::Named(NamedKey::Enter), Some("\r".into()))));
        assert_eq!(key_of(67, 0), Some((Key::Named(NamedKey::Backspace), None)));
        assert_eq!(key_of(29, 'a' as u32), Some((Key::Character("a".into()), Some("a".into()))));
        assert_eq!(key_of(32, 'D' as u32), Some((Key::Character("D".into()), Some("D".into()))), "a shifted letter as the character it types");
        assert_eq!(key_of(139, 0), Some((Key::Named(NamedKey::F9), None)));
        assert_eq!(key_of(0, 0), None);
        assert_eq!(key_of(5, 0), None, "CALL types nothing");
    }
}
