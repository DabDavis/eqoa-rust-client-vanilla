//! Fishing in play: `VIClientPart3FC10` (`VIClient+0x3FC10`: 0x0F02 in,
//! 0x0F01 out) and `VIWndGame`'s mode 36, the fishing window
//! (`VIWndFishing`): `fishing.rs` is the window and the part; this ties them
//! to the session, the inventory, the chat box and the scene. A child of
//! `play`, for `Play`'s fields.

use super::Play;
use crate::fishing::{FishOut, FishingWnd, Part, Recv, SplashFx, state};
use crate::walker::Walker;
use esf::Library;
use vi::fpu::{Ee, F};
use vi::vect::Vect3;

/// `VIClientPart3FC10` and `VIWndFishing`.
#[derive(Default)]
pub struct Fishing {
    pub part: Part,
    pub wnd: FishingWnd,
    keys: Vec<(i32, bool)>,
    /// `m_p07190.m_keys[3]`: the pad's left stick, vertical, as `ScaleStick`
    /// left it (back is positive)
    ly: f32,
    /// `Splash`'s `FindWater`, for the next frame, which has the scene: the
    /// player's actor, the walk's committed place and its angles then
    water: Option<(i32, Vect3<Ee>, Vect3<Ee>)>,
    /// `LoadTargetEffect`'s splash, made the first time it is wanted (None
    /// with no texture for it)
    splash: Option<SplashFx>,
    splash_tried: bool,
    /// the splash was emitting the frame before (its trace)
    splash_was: bool,
    /// the mode the last HUD frame saw (the window's focus lost)
    was_up: bool,
}

/// The mode the fishing window owns.
pub fn owns(mode: i32) -> bool {
    mode == 36
}

impl Play {
    /// A key for the fishing window, handled with the next HUD frame.
    pub fn fishing_key(&mut self, key: i32, down: bool) {
        self.fish.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the fishing window (`pointer`): the
    /// keys to press (the edit box's, the catch list's rows).
    pub fn fishing_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        self.fish.wnd.point(f)
    }

    /// The mode is the fishing window's.
    pub fn fishing_owns(&self) -> bool {
        owns(self.mode)
    }

    /// The fishing window's edit box is up (it takes the keyboard).
    pub fn fishing_typing(&self) -> bool {
        self.fish.wnd.edit_visible
    }

    /// The pad's sticks as the game reads them (`input::Console::sticks`:
    /// rx, ry, lx, ly), a frame: the window reads the left one's vertical
    /// (`m_p07190.m_keys[3]`).
    pub fn fishing_pad(&mut self, sticks: [f32; 4]) {
        self.fish.ly = sticks[3];
    }

    /// The fishing session's state (`VIClientPart3FC10::m_state`).
    pub fn fishing_state(&self) -> i32 {
        self.fish.part.state
    }

    /// `FishingC(word)`: a line in the window's message box, on the clock
    /// the window reads.
    fn fishing_say(&mut self, id: i32) {
        let text = self.words.as_ref().map_or_else(|| format!("(word {id})"), |w| w.get(id, 0).to_string());
        let now = self.session.timer();
        self.fish.wnd.set_text(&crate::ui::wide(&text), now);
    }

    /// `VIWndGame::OpenFishing`: from play (`IsLootable`), mode 36 and the
    /// window's focus (`UpdateMode`); from any other window the session is
    /// cancelled.
    fn open_fishing(&mut self) {
        if self.mode == 0 {
            self.mode = 36;
            let mut cues = Vec::new();
            self.fish.wnd.focus(&mut cues);
            self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        } else {
            self.session.send(&crate::fishing::sub_msg(2));
        }
    }

    /// `VIWndGame::CloseFishing`.
    fn close_fishing(&mut self) {
        if self.mode == 36 {
            self.mode = 0;
            self.fish.wnd.blur();
        }
    }

    /// The opcodes fishing reads (0x0F02, `Recv0F02`); true when handled.
    pub(super) fn fishing_opcode(&mut self, op: u16, data: &[u8]) -> bool {
        if op != crate::fishing::OP_IN {
            return false;
        }
        let Some(m) = crate::fishing::read_0f02(data) else {
            eprintln!("play: 0x0F02 not read ({} bytes: {:02X?})", data.len(), &data[..data.len().min(16)]);
            return true;
        };
        eprintln!("play: 0x0F02 {m:?} in mode {} (state {})", self.mode, self.fish.part.state);
        match m {
            Recv::Start => {
                self.fish.part.catch = Default::default();
                self.fish.part.state = state::WAITING;
                self.open_fishing();
                self.fishing_say(1431);
            }
            Recv::Stop => {
                self.fish.part.state = state::OFF;
                self.fish.part.catch = Default::default();
                self.close_fishing();
            }
            Recv::Ready => self.fish.part.state = state::WAITING,
            Recv::Caught { slot, x } => self.fishing_caught(slot, x),
            Recv::Waiting => self.fish.part.state = state::CAUGHT,
            Recv::Bite => {
                self.fish.wnd.close_bait();
                self.fish.part.state = state::WAITING;
            }
            Recv::Loot(list) => {
                self.fish.part.catch = list;
                let mut cues = Vec::new();
                let now_ms = self.hud.as_ref().map_or(0, |h| h.0.time);
                self.fish.wnd.open_bait(self.wnd_chat.as_mut(), now_ms, &mut cues);
                self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
                self.fishing_say(1434);
                self.fish.part.state = state::CAUGHT;
            }
            Recv::Missed => {
                self.fishing_say(1435);
                self.fish.part.state = state::WAITING;
            }
            Recv::Lost => {
                self.fishing_say(1436);
                self.fish.part.state = state::WAITING;
            }
            Recv::Damaged => {
                self.fishing_say(1437);
                if Part::pole_broken(&self.dump.inventory) {
                    let text = self.words.as_ref().map_or_else(|| "(word 1437)".to_string(), |w| w.get(1437, 0).to_string());
                    self.add_chat(text, [1.0; 4]);
                }
                self.fish.part.state = state::WAITING;
            }
        }
        true
    }

    /// `Caught`: the catch at `slot` into the inventory as far as it goes
    /// (`AddItem` on its own stack limit, 40 slots; unequipped), that many
    /// `Took` off the catch, and state 3. `VIItemList::Get` hands over only
    /// a slot that holds something; another (an assert in `Took` that hangs
    /// the console) changes nothing here.
    fn fishing_caught(&mut self, slot: i32, x: i32) {
        let catch = &mut self.fish.part.catch;
        let Some(item) = usize::try_from(slot).ok().and_then(|s| catch.items.get(s)).filter(|it| it.quantity > 0) else {
            eprintln!("play: 0x0F02 Caught for slot {slot}, which holds nothing");
            return;
        };
        let mut item = item.clone();
        item.equip_slot = -1;
        let limit = item.item.max_stack;
        let n = crate::inventory::add_item(&mut self.dump.inventory, &item, limit, crate::inventory::CAPACITY);
        crate::inventory::took(catch, slot as usize, n);
        eprintln!("play: caught {n} of {} (the server's {x}; {} left on the line)", item.item.name, catch.items.len());
        self.fish.part.state = state::CAUGHT;
    }

    /// Msg00B2's tail: a fishing session going is cancelled.
    pub(super) fn fishing_respawn(&mut self) {
        if self.fish.part.state != state::OFF {
            self.session.send(&crate::fishing::sub_msg(2));
        }
    }

    /// `VIClient::PlayAction(action)`: 0x0047 with it while the player has
    /// an id, and the player's own sprite's action (with the next frame).
    fn play_action(&mut self, action: u8) {
        if self.session.self_id.is_some() {
            self.session.send_op_byte(0x0047, action);
        }
        self.emote = Some(action as i32);
    }

    /// The fishing window, each HUD frame: the focus lost when the mode left
    /// 36; its keys; drawn; what it asked. `walker` gives `Splash` the walk's
    /// committed place.
    pub(super) fn run_fishing(&mut self, vui: &vi::ui::Vui, r: &mut dyn crate::ui::Layout, walker: &Walker) {
        let up = owns(self.mode);
        if self.fish.was_up && !up {
            self.fish.wnd.blur();
        }
        self.fish.was_up = up;
        if !up {
            self.fish.keys.clear();
            self.fish.wnd.emitting = false;
            return;
        }
        let now = self.session.timer();
        let now_ms = vui.time;
        let mut cues = Vec::new();
        let mut out = Vec::new();
        for (k, down) in std::mem::take(&mut self.fish.keys) {
            self.fish.wnd.key(vui, r, &mut self.fish.part, self.wnd_chat.as_mut(), k, down, now, now_ms, &mut cues, &mut out);
        }
        let flags = net::session::EXPANSION_FLAGS;
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let info = crate::infotext::Info {
            words: &vui.words,
            inv: &self.dump.inventory,
            hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
            base: &self.stats.base,
            class: pl.class,
            level: pl.level,
            attack_words: &attack_words,
        };
        let icon = |it: &net::dump::NetItem| crate::inventory::item_icon(it, flags) as i32;
        let ly = F::<Ee>::new(self.fish.ly);
        self.fish.wnd.draw(vui, r, &mut self.fish.part, &info, &icon, now, now_ms, ly, &mut cues, &mut out);
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in out {
            match o {
                FishOut::Send(m) => {
                    eprintln!("play: 0x0F01 {:02X?}", &m[2..]);
                    self.session.send(&m);
                }
                FishOut::ChatLine(text) => self.chat_line(&text),
                FishOut::Splash => {
                    // PlayAction(65), then FindWater from the walk's
                    // committed place and its heading, with the scene
                    self.play_action(65);
                    let c = &walker.cam.walk.committed;
                    let actor = self.self_actor.map_or(-1, |a| a.actor as i32);
                    self.fish.water = Some((actor, c.smooth_location(), c.angles));
                }
            }
        }
    }

    /// The fishing window's splash, each frame drawn, with the scene:
    /// `Splash`'s `FindWater` owed; the emitter's births while the window
    /// plays it; its particles drawn with the spell effects'.
    pub(super) fn fishing_frame(&mut self, walker: &mut Walker, lib: &Library, game_ms: i64) {
        if let Some((actor, pos, hpr)) = self.fish.water.take() {
            let at = crate::fishing::find_water(&mut walker.scene, actor, pos, hpr);
            if !self.fish.splash_tried {
                self.fish.splash_tried = true;
                self.fish.splash = SplashFx::load(lib, game_ms);
            }
            let have = self.fish.splash.is_some();
            eprintln!("play: the cast lands {} (power {}, quality {})", at.map_or_else(|| "off the water".to_string(), |v| format!("on the water at {:.2?}", v.to_array())), self.fish.wnd.power, self.fish.wnd.quality);
            if let (Some(at), Some(s)) = (self.fish.wnd.water(at, have), self.fish.splash.as_mut()) {
                s.place(at);
            } else if !have {
                eprintln!("play: no splash (no surface {:#010x} in the library)", crate::fishing::SPLASH_TEXTURE);
            }
        }
        let Some(s) = self.fish.splash.as_mut() else { return };
        if self.fish.wnd.emitting {
            s.update();
        }
        if self.fish.splash_was && !self.fish.wnd.emitting {
            eprintln!("play: the splash's half second over, {} particles live", s.count());
        }
        self.fish.splash_was = self.fish.wnd.emitting;
        let batches = s.render(game_ms);
        if let Some(sp) = self.spells.as_mut() {
            sp.batches.extend(batches);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::{Item, NetItem};
    use net::session::{Player, Session};

    /// Play on a session nothing answers (as `play`'s own tests make it),
    /// recording what it sends.
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

    fn record(id: i64, quantity: i64, key: u32, max_stack: i64, name: &str) -> NetItem {
        let item = Item { id, max_stack, name: name.into(), wear_slot: -1, ..Default::default() };
        NetItem { quantity, equip_slot: -1, quick_slot: -1, inv_index: key, item, ..Default::default() }
    }

    /// A catch as 0x0F02 7 carries it (`VIItemList`'s write: the zigzag
    /// count, a word, each `ReadNetItem`), its records keyed 10, 11.
    fn loot_bytes(items: &[(i64, i64, &str)]) -> Vec<u8> {
        let mut w = net::stream::Writer::default();
        w.u8(7).signed(items.len() as i64).u32(items.len() as u32);
        for (k, &(id, n, name)) in items.iter().enumerate() {
            w.signed(n).signed(0).signed(0).signed(-1).signed(-1).u32(10 + k as u32);
            // ReadItem: the scalars, the name, the description, the stats
            for v in [id, 0, 0, 0, 0, -1, 0, 0, 1, 0, 0, 0, 0, 1, 50, 0, 0, 0x7FFF, 0x3FF, 0, 0, 0, 0] {
                w.signed(v);
            }
            w.wide(name).wide("").signed(0);
        }
        w.data
    }

    #[test]
    fn the_catch_is_read_as_the_server_writes_it() {
        let b = loot_bytes(&[(1329, 3, "grass carp"), (1223, 1, "sondereel fish soup")]);
        match crate::fishing::read_0f02(&b) {
            Some(Recv::Loot(l)) => {
                let got: Vec<_> = l.items.iter().map(|i| (i.item.id, i.quantity, i.inv_index, i.item.name.as_str())).collect();
                assert_eq!(got, vec![(1329, 3, 10, "grass carp"), (1223, 1, 11, "sondereel fish soup")]);
            }
            m => panic!("{m:?}"),
        }
        // the control: cut short, nothing
        assert!(crate::fishing::read_0f02(&b[..b.len() - 3]).is_none());
    }

    fn op(p: &mut Play, data: &[u8]) -> bool {
        p.fishing_opcode(crate::fishing::OP_IN, data)
    }

    fn says(p: &Play) -> String {
        String::from_utf16_lossy(&p.fish.wnd.text)
    }

    /// A session as the server drives it (`Recv0F02`'s ten cases): Start
    /// opens the window from play (mode 36, sound 15, "Let's go fishing!",
    /// state 1); the catch (7) opens the list (state 3); a caught item (4)
    /// goes into the inventory, unequipped, and off the line; 6 closes the
    /// list (state 1); 8, 9 and 10 their words (state 1), 10's twice with a
    /// broken pole worn; 5 and 3 the state; Stop (2) closes it (mode 0,
    /// state 0, the catch gone). The controls: a sub-op with no case, an
    /// empty message and a Caught for a slot the line does not hold change
    /// nothing; another opcode is not fishing's.
    #[test]
    fn a_session_from_start_to_stop() {
        let mut p = play();
        assert!(op(&mut p, &[1]));
        assert_eq!((p.mode, p.fish.part.state, says(&p)), (36, 1, "(word 1431)".into()));
        assert!(matches!(p.cues.last(), Some(crate::sound::Cue::Ui(15))));
        let catch = loot_bytes(&[(1329, 3, "grass carp"), (1223, 1, "sondereel fish soup")]);
        assert!(op(&mut p, &catch));
        assert_eq!((p.fish.part.state, p.fish.part.catch.items.len(), p.fish.wnd.list_visible, says(&p)), (3, 2, true, "(word 1434)".into()));
        // Caught(0): the carp in, the soup left on the line
        assert!(op(&mut p, &[4, 0, 0, 0, 0, 3, 0, 0, 0]));
        let inv: Vec<_> = p.dump.inventory.items.iter().map(|i| (i.item.id, i.quantity, i.equip_slot)).collect();
        assert_eq!(inv, vec![(1329, 3, -1)]);
        assert_eq!(p.fish.part.catch.items.len(), 1);
        // the controls: a slot the line does not hold, a sub-op with no case
        assert!(op(&mut p, &[4, 5, 0, 0, 0, 1, 0, 0, 0]));
        assert!(op(&mut p, &[11]));
        assert!(op(&mut p, &[]));
        assert_eq!((p.dump.inventory.items.len(), p.fish.part.catch.items.len(), p.fish.part.state), (1, 1, 3));
        assert!(op(&mut p, &[6]));
        assert_eq!((p.fish.part.state, p.fish.wnd.list_visible), (1, false));
        for (b, w) in [(8u8, 1435), (9, 1436), (10, 1437)] {
            let lines = p.chat.len();
            assert!(op(&mut p, &[b]));
            assert_eq!((p.fish.part.state, says(&p), p.chat.len()), (1, format!("(word {w})"), lines), "sub-op {b}");
        }
        // a worn pole of no hit points: 10 says it in the chat box too
        let mut pole = record(1806, 1, 9, 1, "Fishing Pole");
        pole.item.wear_slot = 20;
        pole.equip_slot = 14;
        p.dump.inventory.items.push(pole);
        assert!(op(&mut p, &[10]));
        assert_eq!(p.chat.back().map(|l| l.0.as_str()), Some("(word 1437)"));
        assert!(op(&mut p, &[5]));
        assert_eq!(p.fish.part.state, 3);
        assert!(op(&mut p, &[3]));
        assert_eq!(p.fish.part.state, 1);
        assert!(op(&mut p, &[2]));
        assert_eq!((p.mode, p.fish.part.state, p.fish.part.catch.items.len()), (0, 0, 0));
        assert!(!p.fishing_opcode(0x0F01, &[1]), "the control: not fishing's opcode");
        assert!(sent(&mut p).is_empty(), "nothing asked of the server");
    }

    /// `OpenFishing` from another window (not `IsLootable`): the session is
    /// cancelled (0x0F01 2) and the mode left as it was; Msg00B2 cancels a
    /// session going. The controls: from play it opens and sends nothing,
    /// and with no session Msg00B2 sends nothing.
    #[test]
    fn a_start_from_another_window_or_a_move_cancels() {
        let mut p = play();
        p.mode = 13;
        assert!(op(&mut p, &[1]));
        assert_eq!((p.mode, p.fish.part.state), (13, 1));
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x0F, 2]]);
        p.fishing_respawn();
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x0F, 2]], "Msg00B2 with a session");
        let mut q = play();
        assert!(op(&mut q, &[1]));
        assert_eq!(q.mode, 36);
        assert!(sent(&mut q).is_empty(), "the control: from play");
        q.fish.part.state = 0;
        q.fishing_respawn();
        assert!(sent(&mut q).is_empty(), "the control: no session");
    }

    /// The window's keys reach it in mode 36 only (`window_key`), and a
    /// mode left (Msg00B2's `SetMode(0)`) forgets the chat box's scroll as
    /// the focus going does; the control: the window keeps its keys for its
    /// own frame.
    #[test]
    fn the_window_has_the_keys_in_its_mode() {
        let mut p = play();
        assert!(op(&mut p, &[1]));
        assert!(p.window_key(134, true), "the window's");
        assert_eq!(p.fish.keys, vec![(134, true)], "the control: kept for its frame");
        p.fish.wnd.chat_on = true;
        p.fish.was_up = true;
        p.mode = 0;
        assert!(!p.fishing_owns());
        p.fish.keys.clear();
        // run_fishing's first lines, without a HUD: the focus lost
        let up = owns(p.mode);
        if p.fish.was_up && !up {
            p.fish.wnd.blur();
        }
        assert!(!p.fish.wnd.chat_on);
    }
}
