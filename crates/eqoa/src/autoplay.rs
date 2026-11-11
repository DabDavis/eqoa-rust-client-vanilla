//! Auto-cast and auto targeting in play (`autocast`, the server owner's of
//! 2026-11-11): each of the game's frames what the player's side and its
//! target are, the choice made and sent as a press sends it (`Send0032`), the
//! target taken as the game takes one (0x0010); `/autocast`, `/autotarget`, a
//! right click, a long press and ○ or E held on a bar's slot. A child of
//! `play`, for `Play`'s fields.

use super::{Play, casting};
use crate::autocast::{self, Foe, Kind, Seen};

/// The server keeps a cast busy this many of VITimer's ticks past its end
/// (`autocast::BUSY_MS`, 983.04 ticks a second).
const BUSY_TICKS: i64 = autocast::BUSY_MS * 98304 / 100_000;

impl Play {
    /// The player's side besides itself: its pet, its group, its raid.
    fn auto_friends(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.group.members[..self.group.count()].iter().map(|m| m.id).collect();
        out.extend(self.group.raid.iter().map(|m| m.id));
        if self.pet_id != u32::MAX {
            out.push(self.pet_id);
        }
        out
    }

    /// A 0x00DB for the fights' record: who hit whom.
    pub(super) fn auto_hit(&mut self, attacker: u32, defender: u32) {
        let (me, friends, ms) = (self.self_id32(), self.auto_friends(), self.session.now() as i64);
        self.autocast.hit(attacker, defender, ms, me, &friends);
    }

    /// Whether creature `id` is in view and alive (not a corpse, not lying dead).
    fn auto_alive(&self, id: u32) -> bool {
        self.entities.slots.iter().flatten().find(|e| e.id == id).is_some_and(|e| e.p18 & 2 == 0 && e.creature.as_ref().is_some_and(|c| c.state != 14))
    }

    /// Whether creature `id` is one the player may fight: an NPC's plate (not a
    /// player's, not a pet's: a charmed creature is one), not a merchant's,
    /// a banker's or a trainer's (their `npc_flags`), or the player's duel's.
    fn auto_hostile(&self, e: &net::entity::Entity) -> bool {
        e.creature.as_ref().is_some_and(|c| (c.p256 == 0 && c.npc_flags & 0b1011 == 0) || self.is_dueling(e.id))
    }

    /// The ability `id` used as a press uses it, at `target`.
    fn auto_send(&mut self, id: i64, target: u32, kind: Kind) {
        let Some(slot) = self.abilities.get(&id).map(|a| a.slot) else { return };
        eprintln!("autocast: {} {:?} slot {slot} at {target:#x}", kind.word(), self.abilities.get(&id).map_or("", |a| a.name.as_str()));
        let mut w = net::stream::Writer::op(0x0032);
        w.u32(self.ability_token).u8(slot as u8).u32(target);
        if crate::s2::get().use_skill_record {
            w.bytes(&self.presses.tail());
        }
        self.session.send(&w.data);
    }

    /// One of the game's frames, the player where `walker` has it
    /// (`auto_frame_at`).
    pub(super) fn auto_frame(&mut self, walker: &crate::walker::Walker) {
        self.auto_frame_at(glam::Vec3::from(walker.cam.walk.phys.smooth_location().to_array()));
    }

    /// One of the game's frames, the player at `at`: whose the auto slots
    /// are, the effects seen, a hold's turn, then auto targeting and an auto
    /// cast.
    fn auto_frame_at(&mut self, at: glam::Vec3) {
        self.autocast.character(self.player.server_id, &self.dump.player.name);
        let ms = self.session.now() as i64;
        let names: Vec<String> = (0..self.effects.count.clamp(0, 8) as usize).map(|i| String::from_utf16_lossy(&self.effects.name[i])).collect();
        self.autocast.effects(&names, ms);
        self.hud_menus.auto = if crate::settings::auto_cast() { self.autocast.on.keys().copied().collect() } else { Vec::new() };
        if let Some(id) = self.autocast.held_long(ms)
            && let Some(a) = self.abilities.get(&id).cloned()
        {
            let line = self.autocast.toggle(&a);
            self.cues.push(crate::sound::Cue::Ui(11));
            self.add_chat(line, [1.0; 4]);
        }
        for line in std::mem::take(&mut self.autocast.said) {
            self.add_chat(line, [1.0, 0.85, 0.4, 1.0]);
        }
        if self.loading.up() || self.world_change.is_some() {
            return;
        }
        let me = self.self_id32();
        let self_e = me.and_then(|me| self.entities.slots.iter().flatten().find(|e| e.id == me));
        let dead = self.is_dead() || self_e.and_then(|e| e.creature.as_ref()).is_some_and(|c| c.state == 14);
        let invisible = self_e.and_then(|e| e.creature.as_ref()).is_some_and(|c| c.flags262 & 1 != 0);
        // auto targeting: none, or a dead one, and a creature fighting the side
        if crate::settings::auto_target() && !dead && !self.window_up() {
            let gone = self.target == u32::MAX || !self.auto_alive(self.target);
            let seen: Vec<Seen> = self
                .entities
                .slots
                .iter()
                .flatten()
                .filter(|e| Some(e.id) != me && e.id != self.pet_id)
                .map(|e| Seen { id: e.id, npc: self.auto_hostile(e), alive: self.auto_alive(e.id), distance: at.distance(glam::Vec3::from(e.actor_pos)) })
                .collect();
            if let Some(id) = self.autocast.pick_target(ms, gone, &seen) {
                eprintln!("autotarget: {id:#x} ({})", self.entities.slots.iter().flatten().find(|e| e.id == id).and_then(|e| e.creature.as_ref()).map_or("", |c| c.name.as_str()));
                self.target = id;
                self.target_info = None;
                let mut w = net::stream::Writer::op(0x0010);
                w.u32(0).u32(id);
                self.session.send(&w.data);
            }
        }
        if !crate::settings::auto_cast() || self.autocast.on.is_empty() {
            return;
        }
        let foe = self.entities.slots.iter().flatten().find(|e| e.id == self.target).filter(|e| Some(e.id) != me && self.auto_hostile(e) && self.auto_alive(e.id)).map(|e| Foe { distance: at.distance(glam::Vec3::from(e.actor_pos)) });
        let timer = self.session.timer();
        let now = autocast::Now {
            ms,
            dead,
            casting: casting(self.cast_window, timer) || (self.cast_window.1 > 0 && timer < self.cast_window.1 + BUSY_TICKS),
            window: self.window_up(),
            invisible,
            hp: self.stats.stat[7],
            hp_max: self.stats.stat[8],
            power: self.stats.stat[9],
            has_pet: self.pet_id != u32::MAX,
            attacking: self.auto_attack,
            foe,
            effects: names,
            bars: self.ability_bars(),
        };
        if let Some(c) = self.autocast.choose(&self.abilities, &now) {
            let target = if c.at_self { me.unwrap_or(self.target) } else { self.target };
            self.auto_send(c.id, target, c.kind);
        }
    }

    /// A right click or a long press at frame point `p` in the quick menus'
    /// own place: on an ability of a bar, its auto turned (said). True when
    /// it was one.
    pub(super) fn auto_point(&mut self, p: [f32; 2]) -> bool {
        let Some(id) = self.hud_menus.ability_at(p) else { return false };
        let Some(a) = self.abilities.get(&id).cloned() else { return false };
        self.autocast.character(self.player.server_id, &self.dump.player.name);
        let mut line = self.autocast.toggle(&a);
        if !crate::settings::auto_cast() {
            line.push_str(" (Auto-Cast is off: Extended > Combat, or /autocast on.)");
        }
        self.cues.push(crate::sound::Cue::Ui(11));
        self.add_chat(line, [1.0; 4]);
        self.hud_menus.auto = self.autocast.on.keys().copied().collect();
        true
    }

    /// ○ or E pressed or let go in play: held a second on a bar's ability,
    /// its auto turns (`auto_frame`).
    pub(super) fn auto_use_held(&mut self, down: bool) {
        let id = if down && (self.mode == 0 || self.mode == 38) { self.hud_menus.selected_ability() } else { None };
        self.autocast.hold(id, down, self.session.now() as i64);
    }

    /// `/autocast`: the list, a slot turned or named a kind, every slot off,
    /// the whole on or off (the Extended row, kept by Save Changes).
    pub(super) fn autocast_command(&mut self, arg: &str) {
        use crate::autocast::Command;
        self.autocast.character(self.player.server_id, &self.dump.player.name);
        let lines: Vec<String> = match crate::autocast::command(arg) {
            Command::Help => vec![crate::autocast::HELP.to_string()],
            Command::System(on) => {
                crate::settings::set_client(crate::settings::ClientOptions { auto_cast: on, ..crate::settings::client() });
                vec![if on { "Auto-cast is on." } else { "Auto-cast is off: no slot is used by itself." }.to_string()]
            }
            Command::Clear => {
                for id in self.autocast.on.keys().copied().collect::<Vec<_>>() {
                    self.autocast.set(id, None);
                }
                vec!["Auto-cast: every slot off.".to_string()]
            }
            Command::List => {
                let mut v: Vec<(i64, String)> = self
                    .autocast
                    .on
                    .iter()
                    .filter_map(|(id, k)| self.abilities.get(id).map(|a| (a.slot, format!("  {}{} ({}{})", if a.valid != 0 && (0..20).contains(&a.slot) { format!("{}: ", a.slot + 1) } else { String::new() }, a.name, k.word(), if a.valid == 0 { ", not on a bar" } else { "" }))))
                    .collect();
                v.sort();
                let mut out = vec![format!("Auto-cast is {}; {} auto slot{}.{}", if crate::settings::auto_cast() { "on" } else { "off" }, v.len(), if v.len() == 1 { "" } else { "s" }, if crate::settings::auto_target() { " Auto target is on." } else { "" })];
                out.extend(v.into_iter().map(|x| x.1));
                if out.len() == 1 {
                    out.push(crate::autocast::HELP.to_string());
                }
                out
            }
            Command::Slot { which, kind } => match crate::autocast::find(&self.abilities, &which).cloned() {
                None => vec![format!("Auto-cast: no ability \"{which}\" on your bars.")],
                Some(a) => match kind {
                    None => vec![self.autocast.toggle(&a)],
                    Some(None) => {
                        self.autocast.set(a.id, None);
                        vec![format!("Auto-cast off: {}.", a.name)]
                    }
                    Some(Some(k)) => {
                        self.autocast.set(a.id, Some(k));
                        vec![format!("Auto-cast on: {} ({}, {}).", a.name, k.word(), k.when())]
                    }
                },
            },
        };
        for l in lines {
            self.add_chat(l, [1.0; 4]);
        }
    }

    /// `/autotarget` (`on`, `off`, or none: the other): Extended > Combat >
    /// Auto Target, kept by Save Changes.
    pub(super) fn autotarget_command(&mut self, arg: &str) {
        let on = match arg.trim().to_ascii_lowercase().as_str() {
            "on" => true,
            "off" => false,
            _ => !crate::settings::auto_target(),
        };
        crate::settings::set_client(crate::settings::ClientOptions { auto_target: on, ..crate::settings::client() });
        let line = if on { "Auto target is on: with no target, the nearest creature fighting you (or your pet or group) is targeted." } else { "Auto target is off." };
        self.add_chat(line.to_string(), [1.0; 4]);
    }
}

#[cfg(test)]
mod tests {
    use super::super::commands::guaranteed;
    use super::super::*;
    use crate::autocast::Kind;

    /// Play on a session that records what it sends, the player 0x100 with
    /// 200 of 200 hit points and 100 power, the ability `ab` on the first bar.
    fn rig(ab: &[net::dump::Ability]) -> (Play, std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>) {
        struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
        impl net::session::Datagrams for Rec {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                self.0.borrow_mut().push(d.to_vec());
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        let mut p = Play::new(s, Player::default(), Box::default());
        // in the world: the login's loading screen down
        p.loading = Default::default();
        p.stats.stat[7] = 200;
        p.stats.stat[8] = 200;
        p.stats.stat[9] = 100;
        p.ability_token = 0x55;
        p.abilities = ab.iter().map(|a| (a.id, a.clone())).collect();
        p.auto_frame_at(glam::Vec3::ZERO);
        p.session.flush();
        sent.borrow_mut().clear();
        (p, sent)
    }

    fn ability(id: i64, slot: i64, name: &str, scope: i64) -> net::dump::Ability {
        net::dump::Ability { id, slot, valid: 1, name: name.into(), scope, power_cost: 10, cast_time: 1, range: 30.0, ..Default::default() }
    }

    /// The 0x0032s sent since the last look: (slot, target).
    fn uses(p: &mut Play, sent: &std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>) -> Vec<(u8, u32)> {
        p.session.flush();
        let out = guaranteed(&sent.borrow()).into_iter().filter(|m| m.starts_with(&[0x32, 0x00])).map(|m| (m[6], u32::from_le_bytes([m[7], m[8], m[9], m[10]]))).collect();
        sent.borrow_mut().clear();
        out
    }

    /// The 0x0010s (targets taken) sent since the last look.
    fn targets(p: &mut Play, sent: &std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>) -> Vec<u32> {
        p.session.flush();
        let out = guaranteed(&sent.borrow()).into_iter().filter(|m| m.starts_with(&[0x10, 0x00])).map(|m| u32::from_le_bytes([m[6], m[7], m[8], m[9]])).collect();
        sent.borrow_mut().clear();
        out
    }

    fn creature(id: u32, p256: u8, at: [f32; 3]) -> net::entity::Entity {
        net::entity::Entity { id, kind: net::entity::TYPE_CREATURE, p18: 1, actor_pos: at, pos: at, creature: Some(net::entity::Creature { p256, name: format!("c{id:x}"), ..Default::default() }), ..Default::default() }
    }

    /// An attack slot in a fight sends the press's 0x0032 (the token, its
    /// slot, the target); the controls: none on its timer (the server's
    /// lockout), none with Auto-Cast off, none out of a fight.
    #[test]
    fn an_auto_attack_goes_as_a_press_and_waits_on_its_timer() {
        crate::settings::set_client(crate::settings::ClientOptions::default());
        let fight = |p: &mut Play| {
            p.entities.slots[5] = Some(creature(0x300, 0, [5.0, 0.0, 0.0]));
            p.target = 0x300;
            p.auto_attack = true;
            p.autocast.set(7, Some(Kind::Attack));
        };
        let (mut p, sent) = rig(&[ability(7, 2, "Fiery Burst", 1)]);
        fight(&mut p);
        p.auto_frame_at(glam::Vec3::ZERO);
        assert_eq!(uses(&mut p, &sent), [(2, 0x300)], "a press's 0x0032");
        let (mut p, sent) = rig(&[net::dump::Ability { lockout: 3300, ..ability(7, 2, "Fiery Burst", 1) }]);
        fight(&mut p);
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "on its timer");
        let (mut p, sent) = rig(&[ability(7, 2, "Fiery Burst", 1)]);
        fight(&mut p);
        crate::settings::set_client(crate::settings::ClientOptions { auto_cast: false, ..Default::default() });
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "Auto-Cast off");
        crate::settings::set_client(crate::settings::ClientOptions::default());
        let (mut p, sent) = rig(&[ability(7, 2, "Fiery Burst", 1)]);
        fight(&mut p);
        p.auto_attack = false;
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "no fight: it does not pull");
        // a hit taken makes it a fight
        p.auto_hit(0x300, 0x100);
        p.auto_frame_at(glam::Vec3::ZERO);
        assert_eq!(uses(&mut p, &sent), [(2, 0x300)], "hit: a fight");
    }

    /// A buff slot goes at the player while its effect is not on it, and a
    /// summoning while there is no pet; the controls: the effect on, a pet out.
    #[test]
    fn a_buff_and_a_summoning_go_at_the_player_when_wanted() {
        crate::settings::set_client(crate::settings::ClientOptions::default());
        let (mut p, sent) = rig(&[ability(3, 0, "Deft Hands", 0)]);
        p.autocast.set(3, Some(Kind::Buff));
        p.effects.count = 1;
        p.effects.name[0] = "Deft Hands".encode_utf16().collect();
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "on: none");
        p.effects.count = 0;
        p.auto_frame_at(glam::Vec3::ZERO);
        assert_eq!(uses(&mut p, &sent), [(0, 0x100)], "gone: at the player");
        let (mut p, sent) = rig(&[ability(9, 1, "Fire Elementaling", 0)]);
        p.autocast.set(9, Some(Kind::Pet));
        p.pet_id = 0x400;
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "a pet out: none");
        p.pet_id = u32::MAX;
        p.auto_frame_at(glam::Vec3::ZERO);
        assert_eq!(uses(&mut p, &sent), [(1, 0x100)], "no pet: summoned");
    }

    /// Dead, or a window up: nothing, though wanted (the control: the same
    /// buff goes once they are not).
    #[test]
    fn dead_or_in_a_window_nothing_goes() {
        crate::settings::set_client(crate::settings::ClientOptions::default());
        let (mut p, sent) = rig(&[ability(3, 0, "Deft Hands", 0)]);
        p.autocast.set(3, Some(Kind::Buff));
        p.mode = 5;
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "the spell book up");
        p.mode = 0;
        p.entities.slots[0] = Some(net::entity::Entity { creature: Some(net::entity::Creature { state: 14, p256: 2, ..Default::default() }), ..creature(0x100, 2, [0.0; 3]) });
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(uses(&mut p, &sent).is_empty(), "lying dead");
        p.entities.slots[0] = Some(creature(0x100, 2, [0.0; 3]));
        p.auto_frame_at(glam::Vec3::ZERO);
        assert_eq!(uses(&mut p, &sent).len(), 1, "the control");
    }

    /// Auto Target: with no target, the creature that hit the player is
    /// targeted (0x0010), not a nearer one that hit no one, nor a player who
    /// hit it; the control: Auto Target off, nothing.
    #[test]
    fn auto_target_takes_the_attacker() {
        crate::settings::set_client(crate::settings::ClientOptions::default());
        let (mut p, sent) = rig(&[]);
        p.entities.slots[5] = Some(creature(0x300, 0, [8.0, 0.0, 0.0]));
        p.entities.slots[6] = Some(creature(0x301, 0, [2.0, 0.0, 0.0]));
        p.entities.slots[7] = Some(creature(0x302, 2, [1.0, 0.0, 0.0]));
        p.auto_hit(0x300, 0x100);
        p.auto_hit(0x302, 0x100);
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(targets(&mut p, &sent).is_empty() && p.target == u32::MAX, "Auto Target off");
        crate::settings::set_client(crate::settings::ClientOptions { auto_target: true, ..Default::default() });
        p.auto_frame_at(glam::Vec3::ZERO);
        assert_eq!((targets(&mut p, &sent), p.target), (vec![0x300], 0x300), "the attacker");
        p.auto_frame_at(glam::Vec3::ZERO);
        assert!(targets(&mut p, &sent).is_empty(), "held while it lives");
        crate::settings::set_client(crate::settings::ClientOptions::default());
    }

    /// A right click on a bar's ability turns its auto and marks it; on an
    /// empty slot, nothing (the world's click as ever).
    #[test]
    fn a_right_click_on_a_slot_turns_its_auto() {
        crate::settings::set_client(crate::settings::ClientOptions::default());
        let (mut p, _sent) = rig(&[net::dump::Ability { icon_front: 5, desc: "Target, Decrease Hitpoints by 84".into(), ..ability(7, 0, "Fiery Burst", 1) }]);
        let bars = p.hud_bars();
        p.hud_menus.build(&p.abilities, &p.dump.inventory, &p.gear, bars);
        // the first bar where the game puts it (`HudMenu::new`)
        let at = [430, 25];
        let row = |n: i32| [(at[0] + 170) as f32, (at[1] - 4 + 31 * n + 10) as f32];
        assert!(!p.auto_point(row(1)), "an empty slot: not the bar's");
        assert!(p.auto_point(row(0)));
        assert_eq!(p.autocast.on.get(&7), Some(&Kind::Attack));
        assert_eq!(p.hud_menus.auto, [7]);
        assert!(crate::settings::auto_cast_file().contains("7=attack"), "kept");
        assert!(p.auto_point(row(0)));
        assert!(p.autocast.on.is_empty(), "and off again");
    }
}
