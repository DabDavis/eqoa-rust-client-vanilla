//! The auto-attack swing (CLIENT's VISwing.cpp and `AttackAnim`): which
//! swing a class and level make with what is worn, its spell effect, its
//! delay, and a weapon's swing animation. Held to the decomp over their
//! whole domains by `tests/oracle_swing.rs`.

use crate::hash::hash_resource_id;

/// `GetSwing`. What is worn, by equip slot: 14 the primary hand, 15 the
/// secondary, 16 two-handed, 18 ranged, 17 a shield, 19 a held item that
/// takes no swing, 20 a second weapon (the decomp's argument order).
#[allow(clippy::too_many_arguments)]
pub fn get_swing(cls: i32, level: i32, b14: bool, b15: bool, b16: bool, b18: bool, b17: bool, b19: bool, b20: bool) -> i32 {
    if b16 && (b14 || b15 || b18) || b18 && (b14 || b15 || b16) {
        return 0;
    }
    let ranged = || if level < 20 { 8 } else if level < 35 { 9 } else { 11 };
    match cls {
        7..=14 => {
            if b18 {
                8
            } else if b16 {
                1
            } else if b19 {
                12
            } else {
                0
            }
        }
        2 | 3 => {
            if b18 {
                ranged()
            } else if b16 {
                if level < 20 { 1 } else if level < 35 { 3 } else { 7 }
            } else if b19 {
                12
            } else if level < 20 {
                0
            } else {
                2
            }
        }
        0 | 1 | 4 | 5 | 6 => {
            let fast = matches!(cls, 5 | 6);
            if b18 {
                ranged()
            } else if b16 {
                if level < 20 || fast { 1 } else if level < 35 { 3 } else { 7 }
            } else if b19 {
                12
            } else if level < 20 {
                0
            } else if level < 35 {
                if !b17 && !b20 { 4 } else { 0 }
            } else if !b17 && !b20 {
                5
            } else {
                2
            }
        }
        _ => 0,
    }
}

/// `SwingEffect`: the swing's spell effect id, 0 for none.
pub fn swing_effect(swing: i32) -> u32 {
    let name = match swing {
        0 | 1 => "Attack1",
        4 => "Attack2",
        2 | 3 | 6 | 7 => "Attack3",
        5 => "Attack4",
        8 => "Range1",
        9 => "Range2",
        10 => "Range3",
        11 => "Range4",
        _ => return 0,
    };
    hash_resource_id(name)
}

/// `SwingDelay`, ms.
pub fn swing_delay(swing: i32) -> i32 {
    match swing {
        1 | 8 => 3500,
        2 | 4 | 6 => 4000,
        3 | 7 | 9 | 10 | 11 => 5000,
        5 => 6000,
        _ => 3000,
    }
}

/// `AttackAnim`: a weapon's swing animation.
pub fn attack_anim(attack_type: i32, alt: bool) -> i32 {
    match attack_type {
        3 => if alt { 40 } else { 41 },
        1 => if alt { 24 } else { 25 },
        4 => 42,
        2 => 26,
        5 => if alt { 27 } else { 28 },
        6 => 29,
        7 => 31,
        10 => 36,
        8 => if alt { 32 } else { 35 },
        9 => 33,
        _ => 23,
    }
}
