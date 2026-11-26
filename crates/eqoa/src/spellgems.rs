//! Rearrange the horizontal PC spell bars with the mouse.

use super::Play;
use crate::hudlayout::{self, Beyond, Piece};
use crate::mouse::Click;
use net::dump::Ability;
use render::ui::{Anchor, Vertical, frame_from_window};
use vi::ui::Vui;

const PICKUP_MS: u32 = 2000;
const REPLY_MS: u32 = 3000;

#[derive(Clone, Copy)]
enum Left {
    Hold { slot: i64, id: i64, since: u32 },
    Background([f32; 2]),
    Taken,
}

#[derive(Clone, Copy)]
enum After {
    Removed { destination: Option<i64> },
    Placed,
    Returned,
}

#[derive(Clone, Copy)]
struct Change {
    id: i64,
    index: i64,
    slot: i64,
    sent: u32,
    after: After,
}

struct Swap {
    slot: i64,
    placed: i64,
}

struct Carried {
    id: i64,
    origin: i64,
    swaps: Vec<Swap>,
    returning: bool,
}

impl Carried {
    fn new(id: i64, origin: i64) -> Self {
        Self { id, origin, swaps: Vec::new(), returning: false }
    }
}

#[derive(Default)]
pub(super) struct SpellGems {
    left: Option<Left>,
    cursor: Option<[f32; 2]>,
    carried: Option<Carried>,
    change: Option<Change>,
}

impl Play {
    fn spell_gems_active(&self) -> bool {
        self.mode == 0 && crate::binds::pc_shortcuts() && self.hud_menus.stacked
            && self.typing.is_none() && !self.search.open && !self.map.open
            && !self.loading.up() && !crate::hideui::hidden() && !self.hud_moving()
    }

    fn pc_gem_at(&self, at: [f32; 2], size: (u32, u32)) -> Option<i64> {
        let frame = frame_from_window(size, at);
        let bar = hudlayout::local_point(Piece::SpellBars, frame, Beyond::of(size));
        let slot = self.hud_menus.pc_slot_at(bar)?;
        (slot < i64::from(self.pc_slot_capacity()?)).then_some(slot)
    }

    pub(super) fn gem_in_slot(&self, slot: i64) -> Option<&Ability> {
        self.abilities.values().find(|a| a.valid != 0 && a.slot == slot)
    }

    pub(super) fn carried_gem(&self) -> Option<i64> {
        self.pc_gems.carried.as_ref().filter(|_| crate::binds::pc_shortcuts()).map(|gem| gem.id)
    }

    /// A gem press owns its release too. A carried gem is placed on a new
    /// slot press, so letting go after pickup leaves it on the pointer.
    pub fn spell_gem_press(&mut self, at: [f32; 2], size: (u32, u32), now: u32) -> bool {
        if !self.spell_gems_active() {
            return false;
        }
        let slot = self.pc_gem_at(at, size);
        let left = if self.pc_gems.carried.is_some() {
            if let Some(slot) = slot {
                if self.pc_gems.change.is_none() {
                    self.finish_gem(Some(slot));
                }
                Left::Taken
            } else {
                Left::Background(at)
            }
        } else {
            let Some(slot) = slot else { return false };
            let Some(id) = self.gem_in_slot(slot).map(|a| a.id) else { return false };
            Left::Hold { slot, id, since: now }
        };
        self.pc_gems.left = Some(left);
        self.pc_gems.cursor = Some(at);
        self.keyboard = true;
        self.touched = false;
        true
    }

    pub fn spell_gem_release(&mut self, at: [f32; 2], size: (u32, u32), now: u32, view: Option<glam::Mat4>) -> bool {
        if self.pc_gems.left.is_none() {
            return false;
        }
        self.gem_pointer(Some(at), size);
        self.spell_gem_frame(now);
        match self.pc_gems.left.take() {
            Some(Left::Hold { .. }) => {
                self.point(Click::Target(at[0] as f64, at[1] as f64), size, false);
            }
            Some(Left::Background(pressed)) if self.spell_gems_active() => {
                if let Some(view) = view && self.empty_gem_space(pressed, size, view) {
                    self.return_gem();
                }
            }
            _ => {}
        }
        true
    }

    pub(super) fn gem_pointer(&mut self, at: Option<[f32; 2]>, size: (u32, u32)) {
        self.pc_gems.cursor = at;
        let moved = match self.pc_gems.left {
            Some(Left::Hold { slot, .. }) => at.and_then(|p| self.pc_gem_at(p, size)) != Some(slot),
            Some(Left::Background([x, y])) => at.is_none_or(|[px, py]|
                f64::from(px - x).hypot(f64::from(py - y)) > crate::mouse::SLOP),
            _ => false,
        };
        if moved {
            self.pc_gems.left = Some(Left::Taken);
        }
    }

    pub fn spell_gem_leave(&mut self) {
        self.pc_gems.left = None;
        self.pc_gems.cursor = None;
    }

    /// The right button pressed while a gem press holds the left: the two
    /// are the mouse's run, not a cast or a pickup. The left's press is given
    /// back to the mouse (true) and its release is no longer the gem's.
    pub fn spell_gem_handoff(&mut self) -> bool {
        self.pc_gems.left.take().is_some()
    }

    pub fn spell_gem_right(&mut self, at: (f64, f64), size: (u32, u32), view: glam::Mat4) -> bool {
        if self.pc_gems.carried.is_none() || !self.spell_gems_active() {
            return false;
        }
        if self.pc_gems.change.is_none() && self.empty_gem_space([at.0 as f32, at.1 as f32], size, view) {
            self.finish_gem(None);
        }
        true
    }

    fn empty_gem_space(&self, at: [f32; 2], size: (u32, u32), view: glam::Mat4) -> bool {
        let frame = frame_from_window(size, at);
        let over_hud = hudlayout::drawn_all().iter().any(|&(_, [x0, y0, x1, y1])|
            frame[0] >= x0 && frame[0] < x1 && frame[1] >= y0 && frame[1] < y1);
        !over_hud && !self.links.covers(frame)
            && !self.raid.ready.covers(frame)
            && !self.rolls_cover(at, size)
            && self.creature_at((at[0] as f64, at[1] as f64), size, view).is_none()
    }

    fn return_gem(&mut self) {
        let Some(gem) = self.pc_gems.carried.as_mut() else { return };
        gem.returning = true;
        if self.pc_gems.change.is_none() {
            self.return_next_gem();
        }
    }

    fn gem_returning(&self) -> bool {
        self.pc_gems.carried.as_ref().is_some_and(|gem| gem.returning)
    }

    /// Put the displaced gem back first. This displaces the gem that replaced
    /// it, which can then undo the preceding placement.
    fn return_next_gem(&mut self) {
        let Some(gem) = self.pc_gems.carried.as_ref() else { return };
        let id = gem.id;
        let (slot, expected) = gem.swaps.last().map_or((gem.origin, None), |swap| (swap.slot, Some(swap.placed)));
        let occupant = self.gem_in_slot(slot).map(|a| a.id);
        if expected.is_none() && occupant == Some(id) {
            self.pc_gems.carried = None;
            return;
        }
        let Some(a) = self.abilities.get(&id).filter(|_| occupant == expected) else {
            self.interrupt_gem_return();
            return;
        };
        let (next_slot, after) = if a.valid == 0 {
            (slot, After::Returned)
        } else {
            // A placement already sent before cancellation can still succeed.
            (a.slot, After::Removed { destination: Some(slot) })
        };
        self.request_gem_change(id, next_slot, after);
    }

    fn interrupt_gem_return(&mut self) {
        if let Some(gem) = self.pc_gems.carried.as_mut() {
            gem.returning = false;
        }
        self.add_chat("The spell bar changed while the gem was being returned.".into(), [1.0, 0.25, 0.25, 1.0]);
    }

    fn gem_returned(&mut self, slot: i64, displaced: Option<i64>) {
        let Some(gem) = self.pc_gems.carried.as_mut() else { return };
        let expected = gem.swaps.pop().map(|swap| swap.placed);
        match displaced {
            Some(id) if displaced == expected => {
                gem.id = id;
                self.return_next_gem();
            }
            Some(id) => {
                *gem = Carried::new(id, slot);
                self.interrupt_gem_return();
            }
            None => {
                self.pc_gems.carried = None;
                if expected.is_some() {
                    self.interrupt_gem_return();
                }
            }
        }
    }

    pub(super) fn spell_gem_frame(&mut self, now: u32) {
        if self.pc_gems.change.is_some_and(|c| now.wrapping_sub(c.sent) >= REPLY_MS) {
            self.pc_gems.change = None;
            if let Some(gem) = self.pc_gems.carried.as_mut() {
                gem.returning = false;
            }
            self.add_chat("The server has not confirmed the spell gem change.".into(), [1.0, 0.25, 0.25, 1.0]);
        }
        if self.pc_gems.carried.as_ref().is_some_and(|gem| !self.abilities.contains_key(&gem.id)) {
            self.pc_gems.carried = None;
        }
        if let Some(Left::Hold { slot, id, since }) = self.pc_gems.left {
            if !self.spell_gems_active() || self.gem_in_slot(slot).is_none_or(|a| a.id != id) {
                self.pc_gems.left = Some(Left::Taken);
            } else if now.wrapping_sub(since) >= PICKUP_MS {
                self.pc_gems.left = Some(Left::Taken);
                self.pc_gems.carried = Some(Carried::new(id, slot));
            }
        }
    }

    fn finish_gem(&mut self, destination: Option<i64>) {
        let Some(a) = self.pc_gems.carried.as_ref().and_then(|gem| self.abilities.get(&gem.id)) else { return };
        if a.valid == 0 {
            self.place_gem(a.id, destination);
        } else if Some(a.slot) == destination {
            // Putting the gem back reveals its original slot.
            self.pc_gems.carried = None;
        } else {
            self.request_gem_change(a.id, a.slot, After::Removed { destination });
        }
    }

    fn place_gem(&mut self, id: i64, destination: Option<i64>) {
        if let Some(slot) = destination {
            self.request_gem_change(id, slot, After::Placed);
        } else {
            self.pc_gems.carried = None;
        }
    }

    fn request_gem_change(&mut self, id: i64, slot: i64, after: After) {
        let Some(a) = self.abilities.get(&id) else { return };
        let index = a.display_index;
        self.pc_gems.change = Some(Change { id, index, slot, after, sent: self.session.now() });
        self.send_memorize(index, slot);
    }

    /// Each 0x0033 toggles the spell and clears the destination. Wait for the
    /// source to come off before sending a move's second toggle.
    pub(super) fn spell_gem_acknowledged(&mut self, index: i64, slot: i64, displaced: Option<i64>) {
        let Some(c) = self.pc_gems.change.filter(|c| (c.index, c.slot) == (index, slot)) else {
            // The book can memorize a carried spell while its window is open.
            if self.pc_gems.carried.as_ref().and_then(|gem| self.abilities.get(&gem.id)).is_some_and(|a|
                a.display_index == index && a.valid != 0)
            {
                self.pc_gems.carried = None;
            }
            return;
        };
        self.pc_gems.change = None;
        let valid = self.abilities.get(&c.id).filter(|a| a.display_index == index && a.slot == slot).map(|a| a.valid != 0);
        match (c.after, valid) {
            (After::Removed { destination: Some(_) }, Some(false)) if self.gem_returning() => self.return_next_gem(),
            (After::Removed { destination }, Some(false)) => self.place_gem(c.id, destination),
            (After::Placed, Some(true)) => {
                if let Some(gem) = self.pc_gems.carried.as_mut() && let Some(id) = displaced {
                    gem.id = id;
                    gem.swaps.push(Swap { slot, placed: c.id });
                } else if !self.gem_returning() {
                    self.pc_gems.carried = None;
                }
                if self.gem_returning() {
                    self.return_next_gem();
                }
            }
            (After::Returned, Some(true)) => self.gem_returned(slot, displaced),
            _ if self.gem_returning() => self.interrupt_gem_return(),
            _ => {}
        }
    }

    pub(super) fn draw_cursor_gem(&self, vui: &Vui, r: &mut crate::ui::UiRaster<'_>, size: (u32, u32)) {
        if self.gem_returning() || self.pc_gems.change.is_some_and(|c| matches!(c.after, After::Removed { destination: None })) {
            return;
        }
        let Some(gem) = self.carried_gem().and_then(|id| self.abilities.get(&id)) else { return };
        let Some(at) = self.pc_gems.cursor.filter(|_| self.spell_gems_active()) else { return };
        let (anchor, vertical) = (r.ui.anchor, r.ui.vertical);
        (r.ui.anchor, r.ui.vertical) = (Anchor::Middle, Vertical::Middle);
        crate::hudmenu::draw_cursor_gem(vui, r, gem, frame_from_window(size, at), Beyond::of(size).screen());
        (r.ui.anchor, r.ui.vertical) = (anchor, vertical);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::{Datagrams, Player, Session};
    use render::ui::window_from_frame;

    const SIZE: (u32, u32) = (1280, 720);

    struct Offline;
    impl Datagrams for Offline {
        fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
        fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> { None }
    }

    fn pointer(x: f32, y: f32) -> [f32; 2] {
        window_from_frame(SIZE, hudlayout::frame_point(Piece::SpellBars, [x, y], Beyond::of(SIZE)))
    }

    fn fixture() -> Play {
        crate::settings::testing::reset(false);
        crate::settings::set_client(crate::settings::ClientOptions {
            pc_mode: true, pc_action_rows: 4, ..crate::settings::client()
        });
        hudlayout::reset();
        let mut session = Session::connect_with(Box::new(Offline), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        session.expansion = 2;
        session.self_id = Some(0x100);
        let mut play = Play::new(session, Player::default(), Box::default());
        play.loading = Default::default();
        play.mode = 0;
        play.ability_token = 0x55;
        play.session.record = Some(Vec::new());
        for (id, display_index, slot) in [(10, 3, 0), (20, 4, 39)] {
            play.abilities.insert(id, Ability { id, display_index, slot, scope: 1, valid: 1, ..Default::default() });
        }
        let bars = play.hud_bars();
        play.hud_menus.build(&play.abilities, &play.dump.inventory, &play.gear, bars);
        play.hud_menus.sync_pc_slots(&play.abilities, None);
        play.hud_menus.stacked = true;
        play.hud_menus.draw_pc_bars(&crate::pointer::testing::vui(), &mut crate::pointer::testing::NullRaster, 440, 150, bars, 4);
        play
    }

    fn packets(play: &mut Play) -> Vec<Vec<u8>> {
        std::mem::take(play.session.record.as_mut().unwrap()).into_iter().map(|(_, data)| data).collect()
    }

    fn click(play: &mut Play, at: [f32; 2], now: u32) {
        assert!(play.spell_gem_press(at, SIZE, now));
        assert!(play.spell_gem_release(at, SIZE, now, Some(glam::Mat4::IDENTITY)));
    }

    fn pick_first(play: &mut Play, start: u32) {
        let source = pointer(167.0, 433.0);
        assert!(play.spell_gem_press(source, SIZE, start));
        assert!(play.spell_gem_release(source, SIZE, start + PICKUP_MS, None));
        assert_eq!(play.carried_gem(), Some(10));
    }

    #[test]
    fn short_click_casts_and_holding_picks_up_without_casting() {
        let mut play = fixture();
        let start = play.session.now();
        let source = pointer(167.0, 433.0);
        assert!(play.spell_gem_press(source, SIZE, start));
        assert!(play.spell_gem_release(source, SIZE, start + 1999, None));
        assert_eq!(packets(&mut play), [vec![0x32, 0, 0x55, 0, 0, 0, 0, 255, 255, 255, 255]]);

        assert!(play.spell_gem_press(source, SIZE, start + 3000));
        play.spell_gem_frame(start + 4999);
        assert_eq!(play.carried_gem(), None);
        play.spell_gem_frame(start + 5000);
        assert_eq!(play.carried_gem(), Some(10));
        assert!(play.spell_gem_release(source, SIZE, start + 5100, None));
        assert_eq!(play.carried_gem(), Some(10));
        play.bar_slot_key(0, 0);
        assert!(packets(&mut play).is_empty());

        assert!(play.spell_gem_press(source, SIZE, start + 5200));
        assert!(play.spell_gem_release(source, SIZE, start + 5300, None));
        assert_eq!(play.carried_gem(), None);
        assert_eq!(play.gem_in_slot(0).unwrap().id, 10);
        assert!(packets(&mut play).is_empty(), "putting a gem back needs no server change");
    }

    #[test]
    fn the_right_button_during_a_gem_press_is_the_run_not_a_cast() {
        let mut play = fixture();
        let start = play.session.now();
        let source = pointer(167.0, 433.0);
        assert!(!play.spell_gem_handoff(), "no gem press, nothing to hand back");
        assert!(play.spell_gem_press(source, SIZE, start));
        assert!(play.spell_gem_handoff());
        play.spell_gem_frame(start + PICKUP_MS);
        assert_eq!(play.carried_gem(), None, "no pickup while running");
        assert!(!play.spell_gem_release(source, SIZE, start + PICKUP_MS, None), "the release is the mouse's");
        assert!(packets(&mut play).is_empty(), "nothing cast");
    }

    #[test]
    fn a_confirmed_replacement_passes_the_displaced_gem_to_the_cursor() {
        let mut play = fixture();
        let start = play.session.now();
        let source = pointer(167.0, 433.0);
        let occupied = pointer(473.0, 346.0);
        let empty = pointer(201.0, 433.0);
        assert_eq!(play.pc_gem_at(occupied, SIZE), Some(39));
        assert!(play.spell_gem_press(source, SIZE, start));
        assert!(play.spell_gem_release(source, SIZE, start + 2000, None));
        assert!(play.spell_gem_press(occupied, SIZE, start + 2200));
        assert!(play.spell_gem_release(occupied, SIZE, start + 2250, None));
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 0]]);
        play.ability_slot(3, 0);
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 39]]);
        assert_eq!(play.gem_in_slot(39).unwrap().id, 20, "the destination hasn't been confirmed yet");
        play.ability_slot(3, 39);
        assert_eq!(play.gem_in_slot(39).unwrap().id, 10);
        assert_eq!(play.carried_gem(), Some(20));

        assert!(play.spell_gem_press(empty, SIZE, start + 2500));
        assert!(play.spell_gem_release(empty, SIZE, start + 2550, None));
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 4, 1]]);
        play.ability_slot(4, 1);
        assert_eq!(play.gem_in_slot(1).unwrap().id, 20);
        assert_eq!(play.carried_gem(), None);
    }

    #[test]
    fn moving_away_cancels_the_hold_and_only_a_blank_right_click_discards() {
        let mut play = fixture();
        let start = play.session.now();
        let source = pointer(167.0, 433.0);
        let [x, y] = source;
        let f = frame_from_window(SIZE, source);
        hudlayout::frame_begin();
        hudlayout::note(Piece::SpellBars, [f[0] - 10.0, f[1] - 10.0, f[0] + 10.0, f[1] + 10.0], [0.0; 2]);
        hudlayout::frame_end();
        let blank = pointer(400.0, 200.0);
        assert!(play.spell_gem_press(source, SIZE, start));
        play.gem_pointer(Some(blank), SIZE);
        play.gem_pointer(Some(source), SIZE);
        assert!(play.spell_gem_release(source, SIZE, start + 2100, None));
        assert_eq!(play.carried_gem(), None);
        assert!(!play.spell_gem_right((640.0, 360.0), SIZE, glam::Mat4::IDENTITY));
        assert!(packets(&mut play).is_empty());

        assert!(play.spell_gem_press(source, SIZE, start + 3000));
        assert!(play.spell_gem_release(source, SIZE, start + 5000, None));
        assert!(play.spell_gem_right((f64::from(x), f64::from(y)), SIZE, glam::Mat4::IDENTITY));
        assert_eq!(play.carried_gem(), Some(10));
        assert!(packets(&mut play).is_empty());
        assert!(play.spell_gem_right((640.0, 360.0), SIZE, glam::Mat4::IDENTITY));
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 0]]);
        play.ability_slot(3, 0);
        assert!(play.gem_in_slot(0).is_none());
        assert_eq!(play.carried_gem(), None);
    }

    #[test]
    fn an_unconfirmed_destination_leaves_the_gem_available_for_another_slot() {
        let mut play = fixture();
        let start = play.session.now();
        let source = pointer(167.0, 433.0);
        let destination = pointer(473.0, 346.0);
        assert!(play.spell_gem_press(source, SIZE, start));
        assert!(play.spell_gem_release(source, SIZE, start + 2000, None));
        assert!(play.spell_gem_press(destination, SIZE, start + 2200));
        assert!(play.spell_gem_release(destination, SIZE, start + 2250, None));
        play.ability_slot(3, 0);
        assert_eq!(packets(&mut play).len(), 2);
        let timeout = play.pc_gems.change.unwrap().sent + REPLY_MS;
        play.spell_gem_frame(timeout);
        assert_eq!(play.carried_gem(), Some(10));
        assert_eq!(play.gem_in_slot(39).unwrap().id, 20);
        assert!(packets(&mut play).is_empty());
        assert!(play.spell_gem_press(source, SIZE, timeout + 50));
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 0]]);
        play.ability_slot(3, 0);
        assert_eq!(play.gem_in_slot(0).unwrap().id, 10);
        assert_eq!(play.carried_gem(), None);
    }

    #[test]
    fn an_empty_space_click_returns_the_gem_but_a_drag_or_hud_click_keeps_it() {
        let mut play = fixture();
        let start = play.session.now();
        let blank = [640.0, 360.0];
        let hud = [950.0, 160.0];
        let [x, y] = frame_from_window(SIZE, hud);
        hudlayout::frame_begin();
        hudlayout::note(Piece::Player, [x - 12.0, y - 12.0, x + 12.0, y + 12.0], [0.0; 2]);
        hudlayout::frame_end();
        assert!(!play.spell_gem_press(blank, SIZE, start));
        pick_first(&mut play, start);

        click(&mut play, hud, start + 2100);
        assert_eq!(play.carried_gem(), Some(10));
        assert!(play.spell_gem_press(blank, SIZE, start + 2200));
        play.gem_pointer(Some([660.0, 360.0]), SIZE);
        assert!(play.spell_gem_release(blank, SIZE, start + 2300, Some(glam::Mat4::IDENTITY)));
        assert_eq!(play.carried_gem(), Some(10));

        click(&mut play, blank, start + 2400);
        assert_eq!(play.carried_gem(), None);
        assert_eq!(play.gem_in_slot(0).map(|a| a.id), Some(10));
        assert!(packets(&mut play).is_empty());
    }

    #[test]
    fn returning_after_repeated_swaps_restores_the_original_layout() {
        let mut play = fixture();
        let start = play.session.now();
        play.abilities.insert(30, Ability { id: 30, display_index: 5, slot: 1, valid: 1, ..Default::default() });
        let second = pointer(201.0, 433.0);
        let last = pointer(473.0, 346.0);
        pick_first(&mut play, start);
        click(&mut play, last, start + 2100);
        play.ability_slot(3, 0);
        play.ability_slot(3, 39);
        click(&mut play, second, start + 2200);
        play.ability_slot(4, 1);
        click(&mut play, last, start + 2300);
        play.ability_slot(5, 39);
        assert_eq!(play.carried_gem(), Some(10));
        assert_eq!(packets(&mut play).len(), 4);

        click(&mut play, [640.0, 360.0], start + 2400);
        assert!(play.gem_returning());
        for (index, slot) in [(3, 39), (5, 1), (4, 39), (3, 0)] {
            assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, index, slot]]);
            play.ability_slot(i64::from(index), i64::from(slot));
        }
        assert_eq!(play.carried_gem(), None);
        assert!(packets(&mut play).is_empty());
        assert_eq!(play.gem_in_slot(0).map(|a| a.id), Some(10));
        assert_eq!(play.gem_in_slot(39).map(|a| a.id), Some(20));
        assert_eq!(play.gem_in_slot(1).map(|a| a.id), Some(30));
    }

    #[test]
    fn a_cancel_while_the_source_is_coming_off_returns_it_after_confirmation() {
        let mut play = fixture();
        let start = play.session.now();
        pick_first(&mut play, start);
        click(&mut play, pointer(473.0, 346.0), start + 2100);
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 0]]);

        click(&mut play, [640.0, 360.0], start + 2200);
        assert!(play.gem_returning());
        assert!(packets(&mut play).is_empty());
        play.ability_slot(3, 0);
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 0]]);
        play.ability_slot(3, 0);
        assert_eq!(play.carried_gem(), None);
        assert_eq!(play.gem_in_slot(0).map(|a| a.id), Some(10));
        assert_eq!(play.gem_in_slot(39).map(|a| a.id), Some(20));
    }

    #[test]
    fn a_cancel_during_an_unconfirmed_empty_slot_drop_returns_the_gem() {
        let mut play = fixture();
        let start = play.session.now();
        pick_first(&mut play, start);
        click(&mut play, pointer(201.0, 433.0), start + 2100);
        play.ability_slot(3, 0);
        assert_eq!(packets(&mut play).len(), 2);

        click(&mut play, [640.0, 360.0], start + 2200);
        play.ability_slot(3, 1);
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 1]]);
        play.ability_slot(3, 1);
        assert_eq!(packets(&mut play), [vec![0x33, 0, 0x55, 0, 0, 0, 3, 0]]);
        play.ability_slot(3, 0);
        assert_eq!(play.carried_gem(), None);
        assert_eq!(play.gem_in_slot(0).map(|a| a.id), Some(10));
        assert!(play.gem_in_slot(1).is_none());
    }

    #[test]
    fn returning_preserves_an_unrelated_spell_book_change() {
        let mut play = fixture();
        let start = play.session.now();
        pick_first(&mut play, start);
        click(&mut play, pointer(473.0, 346.0), start + 2100);
        play.ability_slot(3, 0);
        play.ability_slot(3, 39);
        assert_eq!(packets(&mut play).len(), 2);

        play.abilities.insert(30, Ability { id: 30, display_index: 5, slot: -1, ..Default::default() });
        play.ability_slot(5, 39);
        click(&mut play, [640.0, 360.0], start + 2200);
        assert!(packets(&mut play).is_empty());
        assert_eq!(play.gem_in_slot(39).map(|a| a.id), Some(30));
        assert_eq!(play.carried_gem(), Some(20));
        assert!(!play.gem_returning());
    }
}
