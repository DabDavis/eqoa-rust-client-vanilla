//! A server's mercenaries in play (go-eqoa-server's `world.mercs`; a
//! GM's petitions 1475 and 1488, 2026-11-11): Community > Group >
//! Dismiss Mercenary, the client's own entry, there while the server says the
//! player's own merc is out (`net::session::Mercs`, 0x7E5B), sending 0x7E5C
//! (the server's `/merc dismiss`); a merc's name drawn in the pet's colour
//! tinted red (`Play::name_color`), and under it its hirer's tag (`merc_tag`:
//! "<Lonewulf's Mercenary>", `play::labels`' second line). On a server that tells nothing (the reference server,
//! a PS2's) neither: the menu is the game's and a merc's name the pet's. A
//! child of `play`, for `Play`'s fields.
//!
//! And the merc's windows on a server that offers them (go-eqoa-server's
//! 0x7E72-0x7E77, `net::mercwin`; the server owner's of 2026-11-15): its
//! inventory, the player's item menu over what it holds (`mercwnd`, mode
//! [`ITEMS_MODE`]), and its spell book with its ten slots, the player's spell
//! book over its book (`spellbook`, mode [`SPELLS_MODE`]). The server opens
//! them (the merc's menu's Items > Inventory and Spells, `/merc <name> inv`,
//! `/merc <name> spells`) and sends their state after every change; each move
//! is asked (0x7E73, 0x7E76) and shown only when the state comes back.

use super::Play;

/// The entry's message (none of the game's: its high half our opcode).
pub const MSG_DISMISS_MERC: u32 = 0x7E5C_0001;
/// The entry's words (the game has none): "Dismiss Mercenary" is cut short on the
/// menu's bar ("Dismiss Merc...").
pub const ENTRY: &str = "Dismiss Merc";

/// `VIWndGame`'s modes for the merc's windows: none of the game's (0-44),
/// nor the combat record's (60).
pub const ITEMS_MODE: i32 = 61;
pub const SPELLS_MODE: i32 = 62;

/// The start menu's entry as last set, and the merc's windows.
#[derive(Default)]
pub struct MercPlay {
    menu: Option<bool>,
    /// the merc's inventory as the server last sent it (0x7E74)
    pub window: Option<net::mercwin::MercWindow>,
    /// its book and ten slots as last sent (0x7E77)
    pub spells: Option<net::mercwin::MercSpells>,
    /// the inventory window: the player's item menu with the merc's tabs
    items: crate::itemmenu::ItemMenu,
    /// the spell window: the player's spell book over the merc's
    book: crate::spellbook::SpellBook,
    /// the windows to open with the next HUD frame, which has the raster
    items_open: bool,
    book_open: bool,
    keys: Vec<(i32, bool)>,
    /// an ask on its way (its answer 0x7E75 lets the window ask again)
    pub waiting: bool,
}

impl Play {
    /// `DoAction`'s part: Dismiss Mercenary asks the server to send the
    /// player's merc away (0x7E5C). True when the message was its.
    pub(super) fn merc_action(&mut self, id: u32) -> bool {
        if id != MSG_DISMISS_MERC {
            return false;
        }
        if self.session.mercs.as_ref().is_some_and(|m| m.own) {
            eprintln!("play: Dismiss Mercenary (0x7E5C)");
            let mut w = net::stream::Writer::op(net::session::OP_MERC_DISMISS);
            w.u8(1);
            self.session.send(&w.data);
        }
        true
    }

    /// Community > Group > Dismiss Mercenary, there while the server says the
    /// player's merc is out.
    pub(super) fn merc_menu(&mut self) {
        let on = self.session.mercs.as_ref().is_some_and(|m| m.own);
        if self.merc.menu == Some(on) {
            return;
        }
        let Some(s) = self.start.as_mut() else { return };
        s.set_merc(on);
        s.menu.main.refit(&s.menus);
        self.merc.menu = Some(on);
    }

    /// The line under a merc's name plate (the owner, 2026-11-11: "a tag under
    /// the nameplate above their heads that says <"player's" mercenary>"):
    /// "<Lonewulf's Mercenary>", its hirer's name as the server tells it
    /// (`net::session::Mercs::owner`, 0x7E5B's layout 2), for a creature whose
    /// name is drawn as a merc's (`Play::name_color`); none for any other (a
    /// merc's pet, an NPC or a player of a merc's name), and none from a server
    /// that names no hirer.
    pub(super) fn merc_tag(&self, c: &net::entity::Creature) -> Option<String> {
        if self.name_color(c) != super::MERC_COLOR {
            return None;
        }
        let owner = self.session.mercs.as_ref()?.owner(&c.name)?;
        Some(format!("<{owner}'s Mercenary>"))
    }

    /// Whether a creature of that name is the player's own merc: a merc
    /// whose hirer the server names as the player (0x7E5B's layout 2 or 3).
    pub(super) fn own_merc(&self, name: &str) -> bool {
        self.session.mercs.as_ref().and_then(|m| m.owner(name)).is_some_and(|o| !o.is_empty() && o == self.player.name)
    }

    /// Whether `TargetCentre` (the crosshair: the manual camera all the way
    /// in, first person's controller targeting) passes the creature over as
    /// one of a player's side (`m_p256` 3, a pet's): every pet, as the game
    /// does, but a merc (a GM's petition 2270, 2026-11-17: "players
    /// need to be able to mouse target or first person view controller target
    /// mercenaries", to heal and buff them as a group's member is): one whose
    /// name is drawn as a merc's (`Play::name_color`, the server's 0x7E5B).
    /// Tab's cycle passes them over still, as it does the group's members.
    pub(super) fn centre_skips_side(&self, c: &net::entity::Creature) -> bool {
        c.p256 == 3 && self.name_color(c) != super::MERC_COLOR
    }

    /// The menus came again (a CLIENT image read): the entry is put in again.
    pub(super) fn merc_menu_again(&mut self) {
        self.merc.menu = None;
        self.merc_menu();
    }

    /// A merc's window has the keys.
    pub fn merc_win_owns(&self) -> bool {
        self.mode == ITEMS_MODE || self.mode == SPELLS_MODE
    }

    /// A key for the merc's window up, handled with the next HUD frame.
    pub fn merc_win_key(&mut self, key: i32, down: bool) {
        self.merc.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the merc's window up: the keys to press.
    pub fn merc_win_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        match self.mode {
            ITEMS_MODE => self.merc.items.point(f),
            SPELLS_MODE => self.merc.book.point(f),
            _ => Vec::new(),
        }
    }

    /// Whether a window may open over what is up: play, or the merc's box the
    /// player answered (its menu's Inventory, Spells) and the NPC's words.
    fn merc_win_may_open(&self) -> bool {
        matches!(self.mode, 0 | 7 | 9 | ITEMS_MODE | SPELLS_MODE)
    }

    /// The windows' opcodes (0x7E74, 0x7E75, 0x7E77); true when handled.
    pub(super) fn merc_win_opcode(&mut self, op: u16, data: &[u8]) -> bool {
        use net::mercwin::{GONE, MercSpells, MercWindow, OPEN, OP_MERC_SPELLS_STATE, OP_MERC_WINDOW_RESULT, OP_MERC_WINDOW_STATE};
        match op {
            OP_MERC_WINDOW_STATE => match MercWindow::read(data) {
                Some(w) => {
                    eprintln!("play: {}'s inventory, {} records (flags {})", w.name, w.records.len(), w.flags);
                    if w.flags & GONE != 0 {
                        self.merc.window = None;
                        if self.mode == ITEMS_MODE {
                            self.mode = 0;
                        }
                        return true;
                    }
                    if w.flags & OPEN != 0 && self.merc_win_may_open() {
                        self.merc.items_open = true;
                        self.merc.keys.clear();
                        self.mode = ITEMS_MODE;
                    }
                    self.merc.window = Some(w);
                }
                None => eprintln!("play: a merc's inventory of {} bytes not read", data.len()),
            },
            OP_MERC_SPELLS_STATE => match MercSpells::read(data) {
                Some(s) => {
                    eprintln!("play: {}'s spells, {} in its book, slots {:?}", s.name, s.spells.len(), s.slots);
                    if s.flags & GONE != 0 {
                        self.merc.spells = None;
                        if self.mode == SPELLS_MODE {
                            self.mode = 0;
                        }
                        return true;
                    }
                    if s.flags & OPEN != 0 && self.merc_win_may_open() {
                        self.merc.book_open = true;
                        self.merc.keys.clear();
                        self.mode = SPELLS_MODE;
                    }
                    self.merc.spells = Some(s);
                }
                None => eprintln!("play: a merc's spells of {} bytes not read", data.len()),
            },
            OP_MERC_WINDOW_RESULT => {
                self.merc.waiting = false;
                let done = data.get(2).copied().unwrap_or(0) != 0;
                self.cues.push(crate::sound::Cue::Ui(if done { 5 } else { 10 }));
            }
            _ => return false,
        }
        true
    }

    /// The merc's book as the spell book reads abilities: by id, each on its
    /// slot as the ten hold it.
    fn merc_abilities(&self) -> std::collections::BTreeMap<i64, net::dump::Ability> {
        let mut out = std::collections::BTreeMap::new();
        if let Some(s) = &self.merc.spells {
            for sp in &s.spells {
                let mut a = sp.ability.clone();
                match s.slots.iter().position(|&id| id as i64 == a.id && id != 0) {
                    Some(k) => (a.valid, a.slot) = (1, k as i64),
                    None => (a.valid, a.slot) = (0, -1),
                }
                a.p14 = 1;
                out.insert(a.id, a);
            }
        }
        out
    }

    /// The give of the player's own inventory's Give to Merc: the pack record
    /// `i` to the merc whose window was last told (else the server's only one).
    pub(super) fn merc_give(&mut self, i: i32) {
        let Some(it) = self.dump.inventory.items.get(i.max(0) as usize).filter(|_| i >= 0) else { return };
        let slot = self.merc.window.as_ref().map_or(0, |w| w.slot);
        eprintln!("play: give {:?} to the merc of slot {slot} (0x7E73)", it.item.name);
        let msg = net::mercwin::ask(slot, net::mercwin::ASK_GIVE, it.inv_index, 0);
        self.session.send(&msg);
    }

    /// Give to Merc in the player's own inventory: while the server offers the
    /// windows and the player's merc is out.
    pub(super) fn merc_give_on(&self) -> bool {
        self.session.merc_windows && self.session.mercs.as_ref().is_some_and(|m| m.own)
    }

    /// The merc's windows' opens, keys and drawing, and what they ask.
    pub(super) fn run_merc_windows<R: crate::ui::Layout>(&mut self, vui: &vi::ui::Vui, r: &mut R) {
        if !self.merc_win_owns() {
            self.merc.keys.clear();
            if std::mem::take(&mut self.merc.items.visible) {
                // the inventory window down (△, or a mode over it): no stock's
                // change is sent it any more
                let slot = self.merc.window.as_ref().map_or(0, |w| w.slot);
                self.session.send(&net::mercwin::ask(slot, net::mercwin::ASK_CLOSE, 0, 0));
            }
            self.merc.book.visible = false;
            return;
        }
        match self.mode {
            ITEMS_MODE => self.run_merc_items(vui, r),
            _ => self.run_merc_spells(vui, r),
        }
    }

    fn run_merc_items<R: crate::ui::Layout>(&mut self, vui: &vi::ui::Vui, r: &mut R) {
        let Some(w) = self.merc.window.clone() else {
            self.mode = 0;
            return;
        };
        self.merc.items.tabs = Some(crate::mercwnd::tabs(&w));
        if std::mem::take(&mut self.merc.items_open) || !self.merc.items.visible {
            self.merc.items.open(vui, &*r);
            self.merc.waiting = false;
        }
        let flags = net::session::EXPANSION_FLAGS;
        let v = crate::mercwnd::view(&w, &self.dump.inventory);
        let (class, race, level) = (w.class as i64, w.race as i64, w.level as i64);
        let built = crate::mercwnd::build(
            &v,
            crate::inventory::CAPACITY,
            &|it| crate::inventory::item_icon(it, flags) as i32,
            &|it| crate::inventory::can_use(it, class, race, level),
        );
        self.merc.items.set_built(built);
        let class_name = crate::infotext::CLASS_SHORT.get(w.class as usize).map_or_else(String::new, |&id| vui.words.get(id, vui.language).to_string());
        self.merc.items.header = crate::mercwnd::header(&w, &class_name);
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let mut cues = Vec::new();
        let mut outs = Vec::new();
        {
            let info = crate::infotext::Info {
                words: &vui.words,
                inv: &v.list,
                hp: [w.hp as i32, w.max_hp as i32, w.power as i32, w.max_power as i32],
                base: &v.base,
                class,
                level,
                attack_words: &attack_words,
            };
            for (k, down) in std::mem::take(&mut self.merc.keys) {
                let mut o = Vec::new();
                self.merc.items.key(vui, r, k, down, &v.list, &info, self.merc.waiting, &mut cues, &mut o);
                outs.extend(o);
            }
            self.merc.items.draw(vui, r, &info, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in outs {
            if o == crate::itemmenu::ItemOut::Back {
                // △ at the list: the window closed (said at the next frame), back to play
                self.mode = 0;
                continue;
            }
            if let Some(a) = crate::mercwnd::ask_of(&o, &v) {
                eprintln!("play: {:?} of {}'s inventory (0x7E73)", a, w.name);
                self.session.send(&crate::mercwnd::ask_bytes(a, w.slot));
                self.merc.waiting = true;
            }
        }
    }

    fn run_merc_spells<R: crate::ui::Layout>(&mut self, vui: &vi::ui::Vui, r: &mut R) {
        use crate::spellbook::Out;
        use net::mercwin::{SPELLS_CLEAR, SPELLS_SET, spells_ask};
        let Some(slot) = self.merc.spells.as_ref().map(|s| s.slot) else {
            self.mode = 0;
            return;
        };
        let now = vui.time;
        if std::mem::take(&mut self.merc.book_open) || !self.merc.book.visible {
            // two bars: a player's ten (the expansion's bit 1)
            self.merc.book.open(vui, &*r, 2, now);
        }
        let abilities = self.merc_abilities();
        let class = self.merc.spells.as_ref().map_or(0, |s| s.class as i32);
        let mut out = Vec::new();
        for (k, down) in std::mem::take(&mut self.merc.keys) {
            out.extend(self.merc.book.key(&abilities, 2, k, down, now));
        }
        out.extend(self.merc.book.draw(vui, r, &abilities, class, 2, now));
        for o in out {
            match o {
                Out::Sound(n) => self.cues.push(crate::sound::Cue::Ui(n)),
                Out::Word(id) => self.show_word(id),
                Out::Red(id) => self.warn(id),
                // Memorize onto a slot, or Unmemorize (its own slot again)
                Out::Send0033(index, at) => {
                    let Some(a) = abilities.values().find(|a| a.display_index == index) else { continue };
                    let id = if a.valid != 0 && a.slot == at { 0 } else { a.id as u32 };
                    eprintln!("play: the merc's slot {at}: {:?} (0x7E76)", if id == 0 { "none" } else { a.name.as_str() });
                    self.session.send(&spells_ask(slot, SPELLS_SET, at.clamp(0, 9) as u8, id));
                }
                Out::Send00FE => self.session.send(&spells_ask(slot, SPELLS_CLEAR, 0, 0)),
                Out::Send003A(..) => self.add_chat("A mercenary keeps its book in its own order.".into(), [1.0, 0.25, 0.25, 1.0]),
                Out::Close => {
                    self.merc.book.visible = false;
                    self.mode = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::{Mercs, Player, Session};

    fn play() -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        Play::new(s, Player::default(), Box::default())
    }

    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        p.session.record.as_mut().map(std::mem::take).unwrap_or_default().into_iter().map(|(_, d)| d).collect()
    }

    /// The entry's message sends 0x7E5C 1 while the server says the player's
    /// merc is out; with none out, or from a server that never told (the
    /// control), nothing goes. Another message is not its.
    #[test]
    fn dismiss_mercenary_sends_while_ones_merc_is_out() {
        let mut p = play();
        assert!(p.merc_action(MSG_DISMISS_MERC));
        assert!(sent(&mut p).is_empty(), "the control: no server's word, nothing sent");
        p.session.mercs = Some(Mercs { own: false, names: vec!["Ralthas".into()], ..Default::default() });
        p.merc_action(MSG_DISMISS_MERC);
        assert!(sent(&mut p).is_empty(), "no merc of one's own: nothing sent");
        p.session.mercs = Some(Mercs { own: true, names: vec!["Ralthas".into()], ..Default::default() });
        p.merc_action(MSG_DISMISS_MERC);
        assert_eq!(sent(&mut p), vec![vec![0x5C, 0x7E, 1]]);
        assert!(!p.merc_action(crate::record::MSG_RECORD), "another entry's message is not its");
    }

    /// Petition 2270: the crosshair (`TargetCentre`) takes a merc, where it
    /// passes a pet over (a player's side, `m_p256` 3). The controls: a pet
    /// (a side creature of no merc's name) is passed over, and from a server
    /// that told no mercs a merc's name is a pet's.
    #[test]
    fn the_crosshair_takes_a_merc_but_not_a_pet() {
        let mut p = play();
        let side = |name: &str| net::entity::Creature { name: name.into(), p256: 3, ..Default::default() };
        assert!(p.centre_skips_side(&side("Ralthas")), "the control: no server's word, a pet's");
        p.session.mercs = Some(Mercs { own: false, names: vec!["Ralthas".into()], ..Default::default() });
        assert!(!p.centre_skips_side(&side("Ralthas")), "a merc: taken");
        assert!(p.centre_skips_side(&side("Fluffy")), "the control: a pet, passed over");
        assert!(!p.centre_skips_side(&net::entity::Creature { name: "Gnoll".into(), ..Default::default() }), "not a side creature at all");
    }

    /// Group > Dismiss Mercenary (petition 1475) is in every Group menu while
    /// the server says the player's merc is out, after Leave and after Raid
    /// when that is there, kept through the live client's rebuild of the Group
    /// menu, and goes when the merc does; a server that never told (the
    /// control) has none. Skipped without CLIENT's image (`run/client.bin`).
    #[test]
    fn the_group_menu_has_dismiss_mercenary_while_ones_merc_is_out() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(img) = std::fs::read(root.join("run/client.bin")) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        let img = crate::client_image::ClientImage::from_bytes(img).unwrap();
        let mut p = play();
        p.start = Some(crate::startmenu::StartData::from_image(&img));
        let groups = |p: &Play| -> Vec<Vec<u32>> {
            let s = p.start.as_ref().unwrap();
            s.menus.list.iter().filter(|m| m.x04 == 107).map(|m| m.entries.iter().map(|e| e.msg).collect()).collect()
        };
        let has = |p: &Play| groups(p).iter().map(|g| g.contains(&MSG_DISMISS_MERC)).collect::<Vec<_>>();
        p.merc_menu();
        assert!(!groups(&p).is_empty() && has(&p).iter().all(|h| !h), "the control: no word, no entry");
        p.session.mercs = Some(Mercs { own: true, names: vec!["Ralthas".into()], ..Default::default() });
        p.session.raid_offer = true;
        p.raid_menu();
        p.merc_menu();
        crate::live::with_features(Some(0x1B), || {
            let s = p.start.as_mut().unwrap();
            for k in 0..s.menus.list.len() {
                if s.menus.list[k].x04 == 107 {
                    super::super::liveplay::group_menu(&mut s.menus);
                }
            }
        });
        let s = p.start.as_ref().unwrap();
        for m in s.menus.list.iter().filter(|m| m.x04 == 107) {
            let leave = m.entries.iter().position(|e| e.word == 87).expect("Leave");
            let at = m.entries.iter().position(|e| e.msg == MSG_DISMISS_MERC).expect("Dismiss Mercenary in every Group menu");
            assert_eq!(at, leave + 2, "after Leave and Raid: {:?}", m.entries.iter().map(|e| (e.word, e.msg)).collect::<Vec<_>>());
        }
        assert_eq!(s.menus.labels.get(&(MSG_DISMISS_MERC, 0)).map(|l| String::from_utf16_lossy(&l.text)), Some(ENTRY.to_string()));
        p.session.mercs = Some(Mercs { own: false, names: Vec::new(), ..Default::default() });
        p.merc_menu();
        assert!(has(&p).iter().all(|h| !h), "the merc gone, the entry gone");
    }

    /// A merc's name, a pet-side creature the server named a merc, is
    /// `MERC_COLOR`: the pet's with more red; a pet of the same side not named
    /// is the pet's; an NPC of a merc's name (not pet-side) keeps the NPC's;
    /// with no word from the server (the control) a merc is the pet's.
    #[test]
    fn a_mercs_name_is_the_pets_tinted_red() {
        use super::super::{MERC_COLOR, NAME_COLORS};
        let mut p = play();
        let merc = net::entity::Creature { p256: 3, name: "Ralthas".into(), ..Default::default() };
        let pet = net::entity::Creature { p256: 3, name: "Fire Elemental".into(), ..Default::default() };
        let npc = net::entity::Creature { p256: 0, name: "Ralthas".into(), ..Default::default() };
        assert_eq!(p.name_color(&merc), NAME_COLORS[1], "the control: no word from the server");
        p.session.mercs = Some(Mercs { own: false, names: vec!["Ralthas".into()], ..Default::default() });
        assert_eq!(p.name_color(&merc), MERC_COLOR);
        assert_eq!(p.name_color(&pet), NAME_COLORS[1]);
        assert_eq!(p.name_color(&npc), NAME_COLORS[0]);
        let [r, g, b, _] = MERC_COLOR;
        let [pr, pg, pb, _] = NAME_COLORS[1];
        assert!(r > pr && g <= pg && b <= pb && b > r, "the pet's blue, tinted red: {MERC_COLOR:?}");
    }

    /// A merc's tag (the owner, 2026-11-11): "<Lonewulf's Mercenary>" under a
    /// merc the server named with its hirer (layout 2); none for its pet, for
    /// an NPC of its name, or from a server that named no hirer (layout 1, the
    /// control).
    #[test]
    fn a_merc_is_tagged_with_its_hirer() {
        let mut p = play();
        let merc = net::entity::Creature { p256: 3, name: "Ralthas".into(), ..Default::default() };
        let pet = net::entity::Creature { p256: 3, name: "Fire Elemental".into(), ..Default::default() };
        let npc = net::entity::Creature { p256: 0, name: "Ralthas".into(), ..Default::default() };
        p.session.mercs = Some(Mercs { own: false, names: vec!["Ralthas".into()], ..Default::default() });
        assert_eq!(p.merc_tag(&merc), None, "the control: no hirer named");
        p.session.mercs = Some(Mercs { own: false, names: vec!["Ralthas".into()], owners: vec!["Lonewulf".into()], ..Default::default() });
        assert_eq!(p.merc_tag(&merc).as_deref(), Some("<Lonewulf's Mercenary>"));
        assert_eq!((p.merc_tag(&pet), p.merc_tag(&npc)), (None, None));
    }

    /// □ on a merc (the owner, 2026-11-12): its details box shows its class
    /// (and a healer's mode) under its name where the server tells them
    /// (layout 3); on one's own merc the press talks (0x0004), which opens its
    /// menu on the server, and opens no details box (it stayed on screen beside
    /// the menu). The controls: another's merc is only shown, and
    /// a merc the server told no class of (layout 2) shows its name and level.
    #[test]
    fn square_on_a_merc_shows_its_class_and_talks_to_ones_own() {
        let mut p = play();
        p.words = Some(crate::groupsearch::tests::numbered());
        p.player.name = "Jibrish".into();
        let merc = |id: u32, name: &str| net::entity::Entity {
            id,
            creature: Some(net::entity::Creature { name: name.into(), class: 9, level: 60, p256: 3, ..Default::default() }),
            ..Default::default()
        };
        p.entities.slots[0] = Some(merc(0x30, "Jorador"));
        p.entities.slots[1] = Some(merc(0x31, "Haldor"));
        p.session.mercs = Some(Mercs {
            own: true,
            names: vec!["Jorador".into(), "Haldor".into()],
            owners: vec!["Jibrish".into(), "Tmac".into()],
            classes: vec![(9, 1), (9, 0)],
        });
        p.target = 0x30;
        assert_eq!(p.target_info_text().as_deref(), Some("Jorador\nw174 (Defensive)\nw503 60"));
        sent(&mut p);
        p.interact_with_target();
        assert!(!p.info_wanted(), "one's own: no details box beside its menu (it stayed up)");
        assert_eq!(sent(&mut p), vec![vec![0x04, 0x00, 0x30, 0, 0, 0]], "one's own: a talk");
        p.target = 0x31;
        assert_eq!(p.target_info_text().as_deref(), Some("Haldor\nw174\nw503 60"));
        p.interact_with_target();
        assert!(p.info_wanted(), "another's merc: its details box, as before");
        assert!(sent(&mut p).is_empty(), "another's merc: shown, not talked to");
        p.session.mercs.as_mut().unwrap().classes.clear();
        assert_eq!(p.target_info_text().as_deref(), Some("Haldor\nw503 60"), "no class told: name and level");
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};
    use net::mercwin::testing::{item, spells_body, window_body};
    use net::mercwin::{CARRIED, GONE, OPEN, SET, SLOTS, WORN};

    fn sent(p: &Play) -> Vec<Vec<u8>> {
        p.session.record.as_ref().map(|r| r.iter().map(|(_, d)| d.clone()).filter(|d| d.get(1) == Some(&0x7E)).collect()).unwrap_or_default()
    }

    /// A key pressed and let go, a HUD frame each.
    fn press(p: &mut Play, k: i32) {
        let (vui, mut r) = (vui(), NullRaster);
        p.merc_win_key(k, true);
        p.run_merc_windows(&vui, &mut r);
        p.merc_win_key(k, false);
        p.run_merc_windows(&vui, &mut r);
    }

    /// The inventory window: a state with the open flag opens it over play
    /// (none without), its keys walk the player's item menu with the merc's
    /// tabs, and Equip on a carried record asks the server (0x7E73) and waits
    /// for the answer; nothing it holds changes until the state comes back.
    /// △ closes it; a state of a hire gone closes it too.
    #[test]
    fn the_inventory_window_opens_asks_and_closes() {
        let mut p = crate::play::liveplay::tests::play();
        let recs = vec![item(7, WORN, "Mithril Breastplate", 5, 5), item(9, CARRIED, "Buckler", 13, -1)];
        assert!(p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_STATE, &window_body(0, 1, "Kel", &recs)));
        assert_eq!(p.mode, 0, "a state without the open flag opens nothing");
        assert!(p.merc.window.is_some());
        p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_STATE, &window_body(OPEN, 1, "Kel", &recs));
        assert_eq!(p.mode, ITEMS_MODE);
        let (vui, mut r) = (vui(), NullRaster);
        p.run_merc_windows(&vui, &mut r);
        assert!(p.merc.items.visible);
        // Carried, the buckler's popup, Equip
        press(&mut p, 137);
        press(&mut p, 134);
        press(&mut p, 134);
        assert_eq!(sent(&p), vec![vec![0x73, 0x7E, 1, 3, 9, 0, 0, 0, 0xFF]], "Equip asked");
        assert!(p.merc.waiting);
        assert_eq!(p.merc.window.as_ref().unwrap().records[1].item.equip_slot, -1, "nothing changed before the state");
        p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_RESULT, &[0x73, 3, 1]);
        assert!(!p.merc.waiting);
        // △ twice: the popup, then the window, said to the server
        for _ in 0..3 {
            press(&mut p, 132);
        }
        assert_eq!(p.mode, 0, "△ closed it");
        assert_eq!(sent(&p).last(), Some(&vec![0x73, 0x7E, 1, 7]), "the close said");
        p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_STATE, &window_body(OPEN, 1, "Kel", &recs));
        assert_eq!(p.mode, ITEMS_MODE);
        p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_STATE, &window_body(GONE, 1, "Kel", &[]));
        assert_eq!(p.mode, 0, "the hire gone: closed");
        assert!(p.merc.window.is_none());
    }

    /// A pointer on the inventory window, routed to it while it is up: its
    /// second tab, drawn, is the right step to it; before the draw, and with
    /// the window down, nothing.
    #[test]
    fn a_pointer_on_the_inventory_window() {
        let mut p = crate::play::liveplay::tests::play();
        let recs = vec![item(9, CARRIED, "Buckler", 13, -1)];
        p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_STATE, &window_body(OPEN, 1, "Kel", &recs));
        let at = [240.0, 20.0];
        assert!(p.window_point(at).is_empty(), "not drawn yet: no hits");
        let (vui, mut r) = (vui(), NullRaster);
        p.run_merc_windows(&vui, &mut r);
        assert_eq!(p.window_point(at), vec![crate::pointer::key::RIGHT], "the Carried tab");
        p.mode = 0;
        assert!(p.window_point(at).is_empty(), "the window down");
    }

    /// Give to Merc in the player's own inventory asks the give of the
    /// record's key to the merc last told; it is offered only while the
    /// server offers the windows and the player's merc is out.
    #[test]
    fn give_to_merc_from_the_inventory() {
        let mut p = crate::play::liveplay::tests::play();
        let mut it = net::dump::NetItem { quantity: 1, equip_slot: -1, quick_slot: -1, inv_index: 42, ..Default::default() };
        it.item.name = "Greatsword".into();
        p.dump.inventory.items.push(it);
        assert!(!p.merc_give_on(), "the control: never offered");
        p.session.merc_windows = true;
        p.session.mercs = net::session::Mercs::read(&[1, 1, 1, 0, 3, b'K', b'e', b'l']);
        assert!(p.merc_give_on());
        p.merc_win_opcode(net::mercwin::OP_MERC_WINDOW_STATE, &window_body(0, 2, "Kel", &[]));
        p.item_outs(vec![crate::itemmenu::ItemOut::Custom(crate::mercwnd::MSG_GIVE, 0)]);
        assert_eq!(sent(&p), vec![vec![0x73, 0x7E, 2, 2, 42, 0, 0, 0]]);
    }

    /// The spell window: the player's spell book over the merc's, ten slots;
    /// Memorize onto a slot asks it set (0x7E76), Unmemorize All asks the ten
    /// cleared; nothing moves until the state comes back. △ closes it.
    #[test]
    fn the_spell_window_sets_slots() {
        let mut p = crate::play::liveplay::tests::play();
        let mut slots = [0; SLOTS];
        slots[1] = 951;
        p.merc_win_opcode(net::mercwin::OP_MERC_SPELLS_STATE, &spells_body(OPEN | SET, &[(950, 5, -1), (951, 8, 1)], slots));
        assert_eq!(p.mode, SPELLS_MODE);
        let ab = p.merc_abilities();
        assert_eq!((ab[&950].valid, ab[&951].valid, ab[&951].slot), (0, 1, 1), "on its slot as the ten hold it");
        let (vui, mut r) = (vui(), NullRaster);
        p.run_merc_windows(&vui, &mut r);
        // the first row's popup, Memorize, the first slot
        press(&mut p, 134);
        press(&mut p, 134);
        press(&mut p, 134);
        assert_eq!(sent(&p), vec![vec![0x76, 0x7E, 1, 2, 0, 0xB6, 3, 0, 0]], "Ice Bolt (950) onto slot 0");
        for _ in 0..3 {
            press(&mut p, 132);
        }
        assert_eq!(p.mode, 0, "△ closed it");
    }
}
