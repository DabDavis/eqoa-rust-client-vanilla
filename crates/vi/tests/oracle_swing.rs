//! The swing tables against the decomp, over their whole domains: eqoa-port's
//! oracle loads CLIENT (build/libclient.so) and tabulates `GetSwing` for
//! class -2..16, level -1..70 and all 128 combinations of the seven worn
//! flags, then `SwingDelay`, `SwingEffect` and `AttackAnim` for -2..16; every
//! entry must equal `vi::swing`'s. Negative control: a table with one rule
//! changed (a monk past 35 with a two-hander swinging as a level-20 one)
//! must differ. Needs the oracle (see tests/common).

mod common;

use common::*;
use vi::swing::{attack_anim, get_swing, swing_delay, swing_effect};

fn ours(get: impl Fn(i32, i32, [bool; 7]) -> i32) -> Vec<i32> {
    let mut v = Vec::new();
    for c in -2..=16 {
        for l in -1..=70 {
            for m in 0..128 {
                let b: [bool; 7] = std::array::from_fn(|k| m >> k & 1 != 0);
                v.push(get(c, l, b));
            }
        }
    }
    for k in -2..=16 {
        v.push(swing_delay(k));
        v.push(swing_effect(k) as i32);
        v.push(attack_anim(k, false));
        v.push(attack_anim(k, true));
    }
    v
}

#[test]
fn the_swing_tables_match() {
    let Some(mut o) = Oracle::open() else { return };
    let res = o.call(800, &[]);
    assert_eq!(i32::from_le_bytes(res[0..4].try_into().unwrap()), 0, "the oracle could not load libclient.so");
    let theirs: Vec<i32> = res[4..].as_chunks::<4>().0.iter().map(|c| i32::from_le_bytes(*c)).collect();
    let mine = ours(|c, l, b| get_swing(c, l, b[0], b[1], b[2], b[3], b[4], b[5], b[6]));
    assert_eq!(theirs.len(), mine.len());
    if let Some(i) = (0..mine.len()).find(|&i| mine[i] != theirs[i]) {
        panic!("entry {i}: ours {} theirs {}", mine[i], theirs[i]);
    }
    let kinds: std::collections::BTreeSet<i32> = mine.iter().take(19 * 72 * 128).copied().collect();
    eprintln!("{} entries equal; swings seen {kinds:?}", mine.len());
    // Negative control.
    let broken = ours(|c, l, b| if c == 4 && l >= 35 && b[2] && !b[0] && !b[1] && !b[3] { 3 } else { get_swing(c, l, b[0], b[1], b[2], b[3], b[4], b[5], b[6]) });
    assert_ne!(broken, theirs, "a changed rule went unnoticed");
}
