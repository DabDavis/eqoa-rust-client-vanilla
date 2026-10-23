//! The auto-attack swing timer (CLIENT's `m_p071E8`: `ToggleAutoAttack` sets
//! it to now with the 0x000F, `AutoAttackTick` adds each swing's delay), and
//! the client's own following of the server's rounds after a swap.
//!
//! The game's timer never looks at the server once a fight is on: the swing
//! the player sees and the server's rounds, whose hits bring the damage
//! numbers, keep whatever phase they had. A weapon swap parts them (bow to
//! melee, F3 20261023-082928; the console does it too): the swing changes on
//! the client's next due tick, the server's rounds change on its own
//! schedule, and the two stay apart for the rest of the fight. So here, once
//! the swing a due tick makes is not the one the tick before made, the
//! server's own rounds re-phase the timer for the rest of the fight: the
//! first hit of each round (the player's own 0x00DB, the first after a gap
//! of at least `ROUND_GAP`) puts the next swing one of the server's rounds
//! after where a fresh press would have swung for that round,
//! `PRESS_TO_FIRST_HIT` before its first hit arrived. The server's round is
//! the time from its last round's first hit, when that is within half to
//! twice the swing's delay, else the swing's delay: a server may keep the
//! old weapon's rounds after a swap (go-eqoa-server keeps the swing of the
//! last 0x000F), and the swing then keeps the server's. A fresh press (the timer run out, 0x000F
//! sent) starts both ends together again and ends the following. Without a
//! swap the timer is the game's, tick for tick. Following the server is the
//! client's own, the user's choice (the game has nothing like it).

/// How long before a round's first hit arrives a fresh press swings:
/// Sandstorm's first hit came 0.56-0.76 s after the 0x000F in 17 of 20
/// fights (`claude-notes/training-dummies-20261021.md`).
pub const PRESS_TO_FIRST_HIT: u32 = 660;

/// The least gap between a round's first hit and the hit before it: a
/// round's hits are at most 1 s apart (Attack2's at 0.5 and 1.5 s) and
/// rounds at least 3 s (an Attack2 round of 4 s), as Sandstorm lands them.
pub const ROUND_GAP: u32 = 2000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttackClock {
    /// `m_p071E8`: the next swing's time, 0 before the first press
    pub due: u32,
    /// the swing the last due tick made (`GetSwing`), None since a press
    kind: Option<i32>,
    /// a swap in this fight: the server's rounds re-phase the timer
    follow: bool,
    /// the last own hit's arrival
    hit_at: Option<u32>,
    /// the last round's first hit's arrival
    round_at: Option<u32>,
}

impl AttackClock {
    /// `ToggleAutoAttack`'s test: the timer has run out, or never ran.
    pub fn may_press(&self, now: u32) -> bool {
        self.due == 0 || (self.due.wrapping_sub(now) as i32) <= 0
    }

    /// `ToggleAutoAttack`: the timer from now, with the 0x000F that starts
    /// the server's rounds: both ends start together, and no swap is left
    /// to follow.
    pub fn press(&mut self, now: u32) {
        self.due = now;
        self.kind = None;
        self.follow = false;
    }

    /// `AutoAttackTick`'s test: a swing is due.
    pub fn is_due(&self, now: u32) -> bool {
        (now.wrapping_sub(self.due) as i32) >= 0
    }

    /// A due tick made swing `kind`: one other than the last tick's is a
    /// swap, and the timer follows the server's rounds from here.
    pub fn swing(&mut self, kind: i32) {
        if self.kind.is_some_and(|k| k != kind) {
            self.follow = true;
        }
        self.kind = Some(kind);
    }

    /// The swing the last due tick made.
    pub fn kind(&self) -> Option<i32> {
        self.kind
    }

    /// `m_p071E8 += delay`.
    pub fn add(&mut self, delay: u32) {
        self.due = self.due.wrapping_add(delay);
    }

    /// The player's own hit arrived at `now` (0x00DB, the player the
    /// attacker), the swing's delay `delay`: after a swap, the first hit of a
    /// round puts the next swing where a fresh press puts it, one of the
    /// server's rounds on.
    pub fn own_hit(&mut self, now: u32, delay: u32) {
        let first = self.hit_at.is_none_or(|t| now.wrapping_sub(t) >= ROUND_GAP);
        if first {
            if self.follow {
                let round = self.round_at.map(|t| now.wrapping_sub(t)).filter(|&r| (delay / 2..=delay * 2).contains(&r)).unwrap_or(delay);
                self.due = now.wrapping_sub(PRESS_TO_FIRST_HIT).wrapping_add(round);
            }
            self.round_at = Some(now);
        }
        self.hit_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fight against a server that lands rounds as Sandstorm does: the
    /// 0x000F at 0 starts the first round `first` ms later, each round's hits
    /// at 0.5 and 1.5 s into it (Attack2's), a round every 5 s (a bow's
    /// Range2) until `swap_at`, after which the server's next round comes
    /// `after` ms after its last (4000, the melee weapon's Attack2; or 5000,
    /// a server that keeps the bow's rounds). The client presses at
    /// 0, its frames 33 ms apart, each first handing it the hits that have
    /// arrived (when `follow`), then ticking: a due swing is the bow's
    /// (kind 9) before the swap and the melee weapon's (kind 4) after.
    /// Returns the client's swings and the server's round starts.
    fn fight(first: u32, swap_at: Option<u32>, after: u32, follow: bool) -> (Vec<u32>, Vec<u32>) {
        let delay = |kind: i32| if kind == 9 { 5000 } else { 4000 };
        let mut rounds = vec![first];
        while *rounds.last().unwrap() < 40_000 {
            let last = *rounds.last().unwrap();
            let swapped = swap_at.is_some_and(|s| s <= last + 5000);
            rounds.push(last + if swapped { after } else { 5000 });
        }
        let mut hits: Vec<u32> = rounds.iter().flat_map(|&r| [r + 500, r + 1500]).collect();
        hits.sort();
        let mut c = AttackClock::default();
        c.press(0);
        let mut swings = Vec::new();
        let mut next_hit = 0;
        for t in (0..40_000).step_by(33) {
            while next_hit < hits.len() && hits[next_hit] <= t {
                if follow {
                    let d = c.kind().map_or(5000, delay);
                    c.own_hit(t, d);
                }
                next_hit += 1;
            }
            if c.is_due(t) {
                let kind = if swap_at.is_some_and(|s| t >= s) { 4 } else { 9 };
                c.swing(kind);
                swings.push(t);
                c.add(delay(kind));
            }
        }
        (swings, rounds)
    }

    /// A swing in phase with its round: a fresh press's lead (160 ms here),
    /// to a tenth of a second (the hits handed over and the swing ticked on
    /// 33 ms frames, the server's round measured between two hits).
    const IN_PHASE: std::ops::RangeInclusive<i64> = 60..=200;

    /// Each round's start less the client's swing nearest it.
    fn leads(swings: &[u32], rounds: &[u32]) -> Vec<i64> {
        rounds.iter().filter(|&&r| r < 38_000).map(|&r| swings.iter().map(|&s| r as i64 - s as i64).min_by_key(|d| d.abs()).unwrap()).collect()
    }

    /// No swap: the timer is the game's, swing for swing, with the server's
    /// hits handed to it or not, in phase with the server (each swing
    /// leading its round by 160 ms, to a frame) or not (a server that
    /// starts its first round 1.4 s after the 0x000F: the swings stay where
    /// the game puts them, though following would move them).
    #[test]
    fn without_a_swap_the_game_s_timer() {
        for first in [160, 1400] {
            let (game, rounds) = fight(first, None, 5000, false);
            let (ours, _) = fight(first, None, 5000, true);
            assert_eq!(ours, game, "first round at {first}: the game's swings, unchanged");
            if first == 160 {
                assert!(leads(&ours, &rounds).iter().all(|&d| (127..=160).contains(&d)), "{:?}", leads(&ours, &rounds));
            }
        }
    }

    /// A swap from a bow to a melee weapon: the game's timer keeps the
    /// bow's phase, and every swing after lags the server's round by 840 ms
    /// (the control); following the server's rounds, from the second round
    /// after the swap each swing leads its round as a fresh press's does
    /// (`IN_PHASE`).
    #[test]
    fn after_a_swap_the_swing_follows_the_server() {
        let swap = 12_000;
        let (game, rounds) = fight(160, Some(swap), 4000, false);
        let late = |d: &i64| (-840 - 33..=-840 + 33).contains(d);
        let after: Vec<i64> = leads(&game, &rounds).into_iter().skip(4).collect();
        assert!(!after.is_empty() && after.iter().all(late), "the game's timer: {after:?}");
        let (ours, rounds) = fight(160, Some(swap), 4000, true);
        let led = leads(&ours, &rounds);
        assert!(led[..3].iter().all(|&d| (127..=160).contains(&d)), "before the swap, as the game: {led:?}");
        assert!(led[5..].iter().all(|&d| IN_PHASE.contains(&d)), "after it, in phase again: {led:?}");
    }

    /// A server that keeps the bow's rounds after the swap (5 s, where the
    /// melee weapon swings every 4): the game's timer drifts a second a
    /// round against it (the control), and following the server's rounds
    /// the swing keeps its round's pace and phase.
    #[test]
    fn the_swing_keeps_the_server_s_pace() {
        let swap = 12_000;
        let (game, rounds) = fight(160, Some(swap), 5000, false);
        let off: Vec<i64> = leads(&game, &rounds).into_iter().skip(4).collect();
        assert!(off.iter().any(|d| !IN_PHASE.contains(d)), "the game's timer drifts: {off:?}");
        let (ours, rounds) = fight(160, Some(swap), 5000, true);
        let led = leads(&ours, &rounds);
        assert!(led[5..].iter().all(|&d| IN_PHASE.contains(&d)), "the server's pace: {led:?}");
    }

    /// A press while the timer runs is refused (`ToggleAutoAttack` sends no
    /// 0x000F then), and once it has run out starts afresh, the following
    /// ended; a hit 1 s after another is the same round's, and moves
    /// nothing.
    #[test]
    fn a_press_starts_afresh_and_a_round_counts_once() {
        let mut c = AttackClock::default();
        assert!(c.may_press(5));
        c.press(0);
        c.swing(9);
        c.add(5000);
        assert!(!c.may_press(4000) && c.may_press(5000));
        c.swing(4);
        c.own_hit(6000, 4000);
        assert_eq!(c.due, 6000 - PRESS_TO_FIRST_HIT + 4000, "a round's first hit re-phases");
        c.own_hit(7000, 4000);
        assert_eq!(c.due, 6000 - PRESS_TO_FIRST_HIT + 4000, "its second does not");
        c.press(10_000);
        c.own_hit(12_000, 4000);
        assert_eq!(c.due, 10_000, "a press ends the following");
    }
}
