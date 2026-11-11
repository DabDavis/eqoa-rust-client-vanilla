//! A server's mercenaries in play (go-eqoa-server's `world.mercs`; the GM
//! Jibrish's petitions 1475 and 1488, 2026-11-11): Community > Group >
//! Dismiss Mercenary, the client's own entry, there while the server says the
//! player's own merc is out (`net::session::Mercs`, 0x7E5B), sending 0x7E5C
//! (the server's `/merc dismiss`); a merc's name drawn in the pet's colour
//! tinted red (`Play::name_color`), and under it its hirer's tag (`merc_tag`:
//! "<Lonewulf's Mercenary>", `play::labels`' second line). On a server that tells nothing (Sandstorm,
//! a PS2's) neither: the menu is the game's and a merc's name the pet's. A
//! child of `play`, for `Play`'s fields.

use super::Play;

/// The entry's message (none of the game's: its high half our opcode).
pub const MSG_DISMISS_MERC: u32 = 0x7E5C_0001;
/// The entry's words (the game has none): "Dismiss Mercenary" is cut short on the
/// menu's bar ("Dismiss Merc...").
pub const ENTRY: &str = "Dismiss Merc";

/// The start menu's entry as last set.
#[derive(Default)]
pub struct MercPlay {
    menu: Option<bool>,
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

    /// The menus came again (a CLIENT image read): the entry is put in again.
    pub(super) fn merc_menu_again(&mut self) {
        self.merc.menu = None;
        self.merc_menu();
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
        p.session.mercs = Some(Mercs { own: false, names: vec!["Ralthas".into()], owners: vec!["Lonewulf".into()] });
        assert_eq!(p.merc_tag(&merc).as_deref(), Some("<Lonewulf's Mercenary>"));
        assert_eq!((p.merc_tag(&pet), p.merc_tag(&npc)), (None, None));
    }
}
