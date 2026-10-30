//! SOE's live client's parts in play (`live`): its server feature word
//! (0x07F5) and its group search (`groupsearch`: the Group menu's Search,
//! Register and Unregister, `/lfg`, 0x0801 up and 0x0840 down, the window
//! in mode 42). A child of `play`, for `Play`'s fields.
//!
//! All of it is a live server's only: on one that sends no feature word
//! (Sandstorm, a PS2's) the Group menu is the disc's, `/lfg` goes to the
//! server as typed, and the group chat's Show and Hide send the disc's
//! 0x0B01.

use super::Play;
use crate::client_image::{MenuEntry, Menus};
use crate::groupsearch::{GroupSearch, Lfg, Out, View};

/// The Group menu's `VIMenuDef` on the Frontiers disc (its eight entries,
/// Invite to Follow).
const GROUP_MENU: u32 = 0x00CC_ED20;

/// The live Group menu's three more entries (0x013DEDA8, entries 8-10):
/// Search, Register, Unregister and their messages.
const LIVE_GROUP: [(i32, u32); 3] = [(1673, 0x0209_0000), (1851, 0x020A_0000), (1852, 0x020B_0000)];

/// go-eqoa-server's offer of the live auction form (not retail), a u32 after
/// the opcode both ways, as its duel window's 0x7E09: it sends 0x07F5 and then
/// offers (1) once the player is in the world, and the client answers 1 while
/// its sell form names a quantity (the feature word's bit 0x08: a Sell count
/// of 1 is one item), else 0 (a count of 1 is the whole bank record, as from
/// a disc client); again whenever a later 0x07F5 turns the bit on or off. An
/// offer of 0 is answered by nothing; a server that offers nothing (Sandstorm)
/// is sent nothing.
pub const OP_AUCTION_FORM: u16 = 0x7E0B;

/// `VIClientPart2EFC8`'s group search and `VIWndGroupSearch` (`Play::live`).
#[derive(Default)]
pub struct LivePlay {
    pub part: Lfg,
    pub wnd: GroupSearch,
    /// `OpenGroupSearch` asked: the window opens with the next HUD frame, a
    /// leader's (state 0) or a player's (1)
    open: Option<bool>,
    keys: Vec<(i32, bool)>,
    /// the server offered the live auction form (`OP_AUCTION_FORM`), and
    /// what the client last answered
    form_offered: bool,
    form_answered: Option<bool>,
}

/// The Group menu as the live client has it, on a server that sends its
/// feature word: the disc's eight entries, then Search, Register and
/// Unregister (in the place a server's patch puts its own: the live menu's).
/// With no word, the menu as CLIENT's image (and a patch) made it.
pub(super) fn group_menu(menus: &mut Menus) {
    if !crate::live::features().live() {
        return;
    }
    let Some(k) = menus.index_of(GROUP_MENU) else { return };
    let entries = &mut menus.list[k].entries;
    if entries.len() == 11 && entries[8..].iter().map(|e| (e.word, e.msg)).eq(LIVE_GROUP) {
        return;
    }
    let like = entries.first().cloned().unwrap_or_default();
    entries.truncate(8);
    for (word, msg) in LIVE_GROUP {
        entries.push(MenuEntry { word, msg, id: 0, sub: None, ..like.clone() });
    }
}

impl Play {
    /// The live client's messages: 0x07F5, the feature word (`Msg07F5`, live
    /// 0x012C9548: the u32 into VIClient+0x813C), kept for every window that
    /// reads it (`live::features`); 0x0840, the group search's players (the
    /// list made again with the next frame, when it may be).
    pub(super) fn live_opcode(&mut self, op: u16, data: &[u8]) {
        match op {
            0x07F5 => {
                if let Some(w) = crate::live::Features::parse(data) {
                    if crate::live::features() != crate::live::Features(Some(w)) {
                        eprintln!("live: the server's feature word {w:#x} (0x07F5)");
                    }
                    crate::live::set_features(crate::live::Features(Some(w)));
                    self.answer_auction_form();
                }
            }
            OP_AUCTION_FORM => {
                if let Some(v) = data.first_chunk::<4>().map(|b| u32::from_le_bytes(*b)) {
                    self.live.form_offered = v != 0;
                    self.live.form_answered = None;
                    self.answer_auction_form();
                }
            }
            0x0840 => {
                if self.live.part.recv(data) {
                    eprintln!("live: {} players found (0x0840)", self.live.part.found.len());
                    if self.live.wnd.refreshable() {
                        self.live.wnd.stale = true;
                    }
                }
            }
            _ => {}
        }
    }

    /// The answer to the auction form's offer (`OP_AUCTION_FORM`): 1 while
    /// the feature word has bit 0x08, else 0; sent when the server has
    /// offered and the answer is not the last one sent.
    fn answer_auction_form(&mut self) {
        if !self.live.form_offered {
            return;
        }
        let on = crate::live::features().has(crate::live::Features::STACKS);
        if self.live.form_answered != Some(on) {
            eprintln!("live: the auction's {} form (0x7E0B {})", if on { "live" } else { "disc's" }, on as u32);
            self.session.send_op_int(OP_AUCTION_FORM, on as u32);
            self.live.form_answered = Some(on);
        }
    }

    /// The group search's window has the keys (mode 42).
    pub fn lfg_owns(&self) -> bool {
        self.mode == 42
    }

    /// Its edit is up (it takes the keyboard).
    pub fn lfg_typing(&self) -> bool {
        self.live.wnd.typing()
    }

    /// A key for the window, handled with the next HUD frame.
    pub fn lfg_key(&mut self, key: i32, down: bool) {
        self.live.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the window: the keys to press.
    pub fn lfg_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        self.live.wnd.point(f)
    }

    /// In a group and not its leader: word 1848 in red (the live client's
    /// Open, Register and Unregister).
    fn lfg_not_leader(&mut self) -> bool {
        if self.group.in_group && !self.group.leader {
            self.warn(1848);
            return true;
        }
        false
    }

    /// `DoAction`'s group search entries (the live Group menu's) and its
    /// group chat's Show and Hide (0x07070000, id 1 and 0): the live client's
    /// sub 1 of 0x0801, the disc's 0x0B01 `{1, show}` on a server that sends
    /// no feature word (`VIClientPart2EFC8::Send`).
    pub(super) fn lfg_action(&mut self, id: u32, arg: i32) {
        let live = crate::live::features().live();
        match id {
            0x0707_0000 if live => self.session.send(&Lfg::show(arg != 0)),
            0x0707_0000 => {
                let mut w = net::stream::Writer::op(0x0B01);
                w.u8(1).u8((arg != 0) as u8);
                self.session.send(&w.data);
            }
            // OpenGroupSearch (0x012C7970 -> 0x0135BE08): refused when in a
            // group and not its leader (1848) or leading a full one (1847)
            0x0209_0000 if live => {
                if self.lfg_not_leader() {
                    return;
                }
                if self.group.in_group && self.group.count() >= 4 {
                    self.warn(1847);
                    return;
                }
                self.set_mode_zero();
                self.mode = 42;
                self.live.open = Some(self.group.in_group);
            }
            0x020A_0000 if live && !self.lfg_not_leader() => self.session.send(&Lfg::register()),
            0x020B_0000 if live && !self.lfg_not_leader() => self.session.send(&Lfg::unregister()),
            _ => {}
        }
    }

    /// `ChatLine`'s `/lfg` (0x012C8FAC), on a server that sends the feature
    /// word: `rest` is the line past the command. `register` or `on` is sub 2,
    /// `unregister` or `off` sub 3 (as the menu's, refused in a group one
    /// does not lead); any other word word 826 with the line. False when
    /// there is no word after the command's space (the line goes on as an
    /// unknown command, as the live client's does).
    pub(super) fn lfg_command(&mut self, after: &[u16]) -> bool {
        if after.len() < 2 {
            return false;
        }
        let arg: String = after[1..].iter().map(|&c| if c < 256 { c as u8 as char } else { '?' }).collect();
        match arg.as_str() {
            "register" | "on" => self.lfg_action(0x020A_0000, 0),
            "unregister" | "off" => self.lfg_action(0x020B_0000, 0),
            _ => {
                let w = self.words.as_ref().map_or_else(|| "(word 826)".to_string(), |w| w.get(826, 0).to_string());
                self.add_chat(format!("{w} \"/lfg {arg}\"."), [1.0; 4]);
            }
        }
        true
    }

    /// The levels `LevelRange` takes: each group member's, as its creature
    /// shows it, else one's own; out of a group, one's own.
    fn lfg_range(&self) -> (i32, i32) {
        let own = self.dump.player.level as i32;
        let levels: Vec<i32> = if self.group.in_group {
            self.group.members[..self.group.count()]
                .iter()
                .map(|m| self.entities.slots.iter().flatten().find(|e| e.id == m.id).and_then(|e| e.creature.as_ref()).map_or(own, |c| c.level as i32))
                .collect()
        } else {
            vec![own]
        };
        crate::groupsearch::level_range(&levels)
    }

    /// The window's opens, keys, drawing, and what it asks.
    pub(super) fn run_group_search(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, loc: &crate::startmenu::Location) {
        let now = self.session.now() as i64;
        if self.live.part.give_up(now) {
            eprintln!("live: no players came for the group search in {} s: given up", crate::groupsearch::SEARCH_GIVE_UP_MS / 1000);
        }
        let range = self.lfg_range();
        let bounds = self.start.as_ref().map_or([None; 6], |s| s.bounds);
        let v = View { now, range, loc, bounds: &bounds };
        let lp = &mut self.live;
        if let Some(leader) = lp.open.take() {
            lp.wnd.open(leader, &mut lp.part, vui, r, &v);
        }
        let mut cues = Vec::new();
        let mut out = Vec::new();
        for (k, down) in std::mem::take(&mut lp.keys) {
            if self.mode != 42 {
                break;
            }
            lp.wnd.key(vui, r, &mut lp.part, &v, k, down, &mut cues, &mut out);
        }
        if self.mode == 42 {
            lp.wnd.draw(vui, r, &mut lp.part, &v, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in out {
            match o {
                Out::Search(q) => {
                    if let Some(m) = self.live.part.search(&q, now) {
                        eprintln!("live: group search {q:?} (0x0801 4)");
                        self.session.send(&m);
                    }
                }
                // CloseGroupSearch: the players let go, the window down, the
                // start menu again
                Out::Close => {
                    self.live.part.found.clear();
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                Out::Invite(id, name) => self.invite_to_group(id, &name),
                Out::Chat(line) => self.chat_line(&line),
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::live::{Features, features};

    /// A session that sends nowhere and records what it would (as
    /// `adminplay`'s tests).
    pub(in crate::play) fn play() -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = net::session::Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        Play::new(s, Default::default(), Box::default())
    }

    /// A new session has no word (the disc's rules), 0x07F5 sets it, a
    /// short one is nothing, and the next session starts without.
    #[test]
    fn msg_07f5_keeps_the_word_for_the_session() {
        let mut p = play();
        assert_eq!(features(), Features(None));
        p.live_opcode(0x07F5, &0x1Bu32.to_le_bytes());
        assert_eq!(features(), Features(Some(0x1B)));
        p.live_opcode(0x07F5, &[8, 0]);
        assert_eq!(features(), Features(Some(0x1B)), "a short 0x07F5 is nothing");
        p.live_opcode(0x07F5, &0x10u32.to_le_bytes());
        assert_eq!(features(), Features(Some(0x10)));
        let _next = play();
        assert_eq!(features(), Features(None), "a new session is the disc's until a word");
    }

    /// The auction form's offer (0x7E0B) with the feature word's bit 0x08 is
    /// answered 1; a later 0x07F5 that turns the bit off is answered 0, one
    /// that leaves it as it was nothing; an offer of 0 nothing. The
    /// controls: an offer without the bit is answered 0, and no offer (a
    /// server like Sandstorm) is sent nothing whatever the word says.
    #[test]
    fn the_auction_forms_offer_is_answered_by_bit_0x08() {
        let offer = |v: u32| v.to_le_bytes();
        let mut p = play();
        p.live_opcode(0x07F5, &0x1Bu32.to_le_bytes());
        p.live_opcode(OP_AUCTION_FORM, &offer(1));
        assert_eq!(sent(&mut p), vec![vec![0x0B, 0x7E, 1, 0, 0, 0]]);
        p.live_opcode(0x07F5, &0x13u32.to_le_bytes());
        assert_eq!(sent(&mut p), vec![vec![0x0B, 0x7E, 0, 0, 0, 0]], "the bit off: the new answer");
        p.live_opcode(0x07F5, &0x11u32.to_le_bytes());
        assert!(sent(&mut p).is_empty(), "the answer unchanged: nothing");
        p.live_opcode(OP_AUCTION_FORM, &offer(0));
        p.live_opcode(0x07F5, &0x1Bu32.to_le_bytes());
        assert!(sent(&mut p).is_empty(), "an offer of 0: nothing, and nothing after");
        // the controls: an offer without the bit (answered 0, then 1 when a
        // word brings it), and no offer at all
        let mut p = play();
        p.live_opcode(0x07F5, &0x10u32.to_le_bytes());
        p.live_opcode(OP_AUCTION_FORM, &offer(1));
        assert_eq!(sent(&mut p), vec![vec![0x0B, 0x7E, 0, 0, 0, 0]]);
        p.live_opcode(0x07F5, &0x18u32.to_le_bytes());
        assert_eq!(sent(&mut p), vec![vec![0x0B, 0x7E, 1, 0, 0, 0]]);
        let mut p = play();
        p.live_opcode(0x07F5, &0x1Bu32.to_le_bytes());
        assert!(sent(&mut p).is_empty(), "no offer, nothing sent");
    }

    /// What the session was handed, in order (the guaranteed channel's).
    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        p.session.record.as_mut().map(std::mem::take).unwrap_or_default().into_iter().map(|(_, d)| d).collect()
    }

    fn last_line(p: &Play) -> String {
        p.chat.back().map(|(t, _)| t.clone()).unwrap_or_default()
    }

    fn grouped(p: &mut Play, leader: bool, members: usize) {
        p.group.in_group = true;
        p.group.leader = leader;
        p.group.members = (0..members).map(|k| crate::group::Member { id: 0x100 + k as u32, name: format!("m{k}"), ..Default::default() }).collect();
    }

    /// The Group menu on a server that sends the feature word: the disc's
    /// eight entries, then Search, Register and Unregister (1673, 1851, 1852)
    /// as the live client has them; again and again the same. The control:
    /// no word, the disc's eight. Skipped without CLIENT's image
    /// (`run/client.bin`).
    #[test]
    fn the_group_menu_gains_the_live_entries() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(img) = std::fs::read(root.join("run/client.bin")) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        let img = crate::client_image::ClientImage::from_bytes(img).unwrap();
        let entries = |word: Option<u32>| {
            crate::live::with_features(word, || {
                let mut d = crate::startmenu::StartData::from_image(&img);
                group_menu(&mut d.menus);
                group_menu(&mut d.menus);
                let k = d.menus.index_of(GROUP_MENU).unwrap();
                d.menus.list[k].entries.iter().map(|e| (e.word, e.msg)).collect::<Vec<_>>()
            })
        };
        let disc = entries(None);
        assert_eq!(disc.len(), 8);
        assert_eq!(disc[0], (86, 0x0701_0000), "Invite");
        let live = entries(Some(0x1B));
        assert_eq!(&live[..8], &disc[..]);
        assert_eq!(&live[8..], &LIVE_GROUP[..]);
    }

    /// Search's refusals as the live client's Open: in a group one does not
    /// lead, 1848 in red; leading a full group (4), 1847; else mode 42, a
    /// leader's search (with room) or a player's. With no feature word
    /// nothing (the entry is not there).
    #[test]
    fn search_opens_for_a_leader_with_room_or_a_player_alone() {
        let mut p = play();
        crate::live::set_features(Features(Some(0x1B)));
        p.words = Some(crate::groupsearch::tests::numbered());
        grouped(&mut p, false, 2);
        p.do_action(0x0209_0000, 0);
        assert_eq!((p.mode, last_line(&p), p.live.open), (0, "w1848".into(), None));
        grouped(&mut p, true, 4);
        p.do_action(0x0209_0000, 0);
        assert_eq!((p.mode, last_line(&p), p.live.open), (0, "w1847".into(), None));
        grouped(&mut p, true, 3);
        p.do_action(0x0209_0000, 0);
        assert_eq!((p.mode, p.live.open), (42, Some(true)));
        p.mode = 0;
        p.group.clear();
        p.do_action(0x0209_0000, 0);
        assert_eq!((p.mode, p.live.open.take()), (42, Some(false)));
        p.mode = 0;
        crate::live::set_features(Features(None));
        p.do_action(0x0209_0000, 0);
        assert_eq!((p.mode, p.live.open), (0, None), "no word, no search");
    }

    /// Register and Unregister (sub 2, 3; refused in a group one does not
    /// lead) and the group chat's Show and Hide: the live client's 0x0801 sub
    /// 1 with a word, the disc's 0x0B01 {1, show} without.
    #[test]
    fn register_and_the_group_chat_on_the_wire() {
        let mut p = play();
        crate::live::set_features(Features(Some(0x1B)));
        p.do_action(0x020A_0000, 0);
        p.do_action(0x020B_0000, 0);
        p.do_action(0x0707_0000, 1);
        p.do_action(0x0707_0000, 0);
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x08, 2], vec![0x01, 0x08, 3], vec![0x01, 0x08, 1, 1], vec![0x01, 0x08, 1, 0]]);
        grouped(&mut p, false, 2);
        p.do_action(0x020A_0000, 0);
        assert!(sent(&mut p).is_empty(), "not the leader: nothing");
        crate::live::set_features(Features(None));
        p.do_action(0x0707_0000, 1);
        p.do_action(0x020A_0000, 0);
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x0B, 1, 1]], "the disc's 0x0B01; no register without a word");
    }

    /// `/lfg on` and `register` are sub 2, `off` and `unregister` sub 3, any
    /// other word word 826 with the line (nothing sent), `/lfg` alone goes on
    /// to the server as typed; with no feature word every `/lfg` goes to the
    /// server as typed (the control).
    #[test]
    fn slash_lfg() {
        let mut p = play();
        p.words = Some(crate::groupsearch::tests::numbered());
        crate::live::set_features(Features(Some(0x1B)));
        p.chat_line("/lfg on");
        p.chat_line("/LFG register");
        p.chat_line("/lfg off");
        p.chat_line("/lfg unregister");
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x08, 2], vec![0x01, 0x08, 2], vec![0x01, 0x08, 3], vec![0x01, 0x08, 3]]);
        p.chat_line("/lfg Register");
        assert_eq!((last_line(&p), sent(&mut p).len()), ("w826 \"/lfg Register\".".into(), 0));
        p.chat_line("/lfg");
        let alone = sent(&mut p);
        assert_eq!(alone.len(), 1);
        assert_ne!(&alone[0][..2], &[0x01, 0x08], "as typed, not 0x0801");
        crate::live::set_features(Features(None));
        p.chat_line("/lfg on");
        let typed = sent(&mut p);
        assert!(typed.len() == 1 && &typed[0][..2] != [0x01, 0x08], "{typed:?}");
    }

    /// The window in play: Search opens it (mode 42) on a player's search;
    /// its Search sends sub 4 once; 0x0840 fills the list at the next frame;
    /// △ closes it and the players are let go.
    #[test]
    fn the_window_in_play() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let loc = crate::startmenu::Location { world: 0, pos: [0.0; 3], cols: 1, rows: 1, cell: 1.0, origin: [0.0; 3], bounds: None };
        let mut p = play();
        crate::live::set_features(Features(Some(0x1B)));
        p.dump.player.level = 30;
        p.do_action(0x0209_0000, 0);
        p.run_group_search(&vui, &mut r, &loc);
        assert_eq!((p.mode, p.live.wnd.state, p.live.wnd.min, p.live.wnd.max), (42, 1, 22, 40));
        // ↑ to Search (its repeat moves it as the window draws), then ✕
        for k in [138, 134] {
            p.window_key(k, true);
            p.window_key(k, false);
            p.run_group_search(&vui, &mut r, &loc);
        }
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x08, 4, 1, 1, 1, 22, 40]]);
        let mut w = net::stream::Writer::default();
        w.u8(0x40).signed(1).string(b"Amy").u8(25).signed(9).signed(6);
        for c in [1.0f32, 2.0, 3.0] {
            w.u32(c.to_bits());
        }
        w.u8(0).u32(0x55);
        p.live_opcode(0x0840, &w.data);
        assert!(!p.live.part.busy && p.live.wnd.stale);
        p.run_group_search(&vui, &mut r, &loc);
        assert!(!p.live.wnd.stale);
        assert_eq!(p.live.part.found.len(), 1);
        p.window_key(132, true);
        p.window_key(132, false);
        p.run_group_search(&vui, &mut r, &loc);
        // out of mode 42 (OpenStartMenu, which opens nothing in a rig without
        // CLIENT's menus)
        assert_eq!(p.mode, 0);
        assert!(p.live.part.found.is_empty());
    }
}
