//! Duels in play: the live game's duel part and window (`duel.rs`,
//! `duelwnd.rs`) tied to the session (0x1502 in, 0x1501 out, the feature word
//! 0x07F5), `VIWndGame`'s modes (43 the invitation, 44 the wager), the chat
//! box, the start menu's Community > Duel and `DoAction`'s Invite and
//! Surrender. Who the player may fight while a duel is on (`IsDueling`) is
//! asked by `TargetInRange`, `CycleTarget` and `TargetCentre` in `play`. A
//! child of `play`, for `Play`'s fields.

use super::Play;
use crate::duel::{Ctx, Do, Part};
use crate::duelwnd::{DuelWnd, WndOut};
use crate::walker::Walker;

/// The duel part and its window.
#[derive(Default)]
pub struct Duel {
    pub part: Part,
    pub wnd: DuelWnd,
    /// the window's focus given (`UpdateMode` 43 or 44's `SetActiveWindow`:
    /// its 295), done with the next HUD frame, which has the raster
    activate: bool,
    keys: Vec<(i32, bool)>,
    /// the client's clock as `Process` last counted the queue down
    last: Option<i64>,
    /// the start menu's Duel entry as last set (`duel_menu`)
    menu: Option<bool>,
}

/// The modes the duel window owns.
pub fn owns(mode: i32) -> bool {
    matches!(mode, 43 | 44)
}

impl Play {
    /// A key for the duel window, handled with the next HUD frame.
    pub fn duel_key(&mut self, key: i32, down: bool) {
        self.duel.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the duel window: the keys to press.
    pub fn duel_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        self.duel.wnd.point(f)
    }

    /// The mode is the duel window's.
    pub fn duel_owns(&self) -> bool {
        owns(self.mode)
    }

    /// The wager's chat line is up (it takes the keyboard).
    pub fn duel_typing(&self) -> bool {
        self.duel_owns() && self.duel.wnd.typing()
    }

    /// `IsDueling(id)`: `id` (the opponent, or its pet) may be fought.
    pub fn is_dueling(&self, id: u32) -> bool {
        self.duel.part.is_dueling(id)
    }

    /// The duel part, for a look (the report's trace, tests).
    pub fn duel_part(&self) -> &Part {
        &self.duel.part
    }

    /// The compass's duel patch (`DrawPatch`'s tail, live 0x0135E124): the
    /// duel's centre (sub 8) while it is on and in the player's world.
    pub(super) fn duel_patch(&self) -> Option<[f32; 3]> {
        let p = &self.duel.part;
        (p.running && p.world as i32 == self.world_id()).then_some(p.centre)
    }

    /// The window as the report's trace gives it.
    pub(super) fn duel_window_line(&self) -> String {
        let w = &self.duel.wnd;
        match (&w.invite, w.wager, w.visible) {
            (_, _, false) => String::new(),
            (Some(r), _, true) => format!("; the invitation from {} up, {} ticks left", r.name, r.timer),
            (None, true, true) => format!("; the wager up, focus {}", w.focus),
            (None, false, true) => "; the window up, empty".to_string(),
        }
    }

    /// The player's name as a decline carries it.
    fn duel_name(&self) -> String {
        self.dump.player.name.clone()
    }

    /// The opcodes the duel reads: 0x1502 (`Recv`) and the feature word
    /// 0x07F5; true when handled.
    pub(super) fn duel_opcode(&mut self, op: u16, data: &[u8], walker: Option<&mut Walker>) -> bool {
        match op {
            crate::duel::OP_IN => {
                let name = self.duel_name();
                let d = {
                    let cx = Ctx { mode: self.mode, words: self.words.as_ref(), name: &name, inv: &self.dump.inventory };
                    self.duel.part.recv(data, &cx)
                };
                eprintln!("play: 0x1502 sub {} ({} bytes) in mode {}: {d:?}", data.first().copied().unwrap_or(0), data.len(), self.mode);
                self.duel_do(d, walker);
            }
            crate::duel::OP_FEATURES => {
                let mut r = net::stream::Reader::new(data);
                let v = r.u32();
                if !r.error {
                    eprintln!("play: the server's feature word {v:#x} (duels {})", if v & crate::duel::FEATURE_DUELS != 0 { "on" } else { "off" });
                    self.duel.part.features = v;
                    self.duel_menu();
                }
            }
            _ => return false,
        }
        true
    }

    /// The start menu's Community > Duel (the live client's, `duel_menu`),
    /// there while the server lets the player duel.
    pub(super) fn duel_menu(&mut self) {
        let on = self.duel.part.on();
        if let Some(s) = self.start.as_mut()
            && self.duel.menu != Some(on)
        {
            s.set_duels(on);
            self.duel.menu = Some(on);
        }
    }

    /// The menus came again (a CLIENT image read): the Duel entry is put in
    /// again.
    pub(super) fn duel_menu_again(&mut self) {
        self.duel.menu = None;
        self.duel_menu();
    }

    /// What the part asked of the game, in order.
    fn duel_do(&mut self, list: Vec<Do>, mut walker: Option<&mut Walker>) {
        for d in list {
            match d {
                // VIWndGame::ShowDuelInvite: the record shown, mode 43, the
                // window's focus (UpdateMode)
                Do::ShowInvite(rec) => {
                    let mut cues = Vec::new();
                    self.duel.wnd.show_invite(rec, self.session.timer(), &mut cues);
                    self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
                    self.mode = 43;
                    self.duel.activate = true;
                }
                // CloseDuelInvite: put away, back to play
                Do::CloseInvite => {
                    self.duel.wnd.close();
                    self.mode = 0;
                }
                // OpenDuelWager: the manual camera ended, whatever was up put
                // away (SetMode(0)), the wager, mode 44: the chat box fixed
                // (UpdateMode), the window's focus
                Do::OpenWager => {
                    if self.mode == 38
                        && let Some(w) = walker.as_deref_mut()
                    {
                        self.reset_view(w);
                    }
                    self.set_mode_zero();
                    self.duel.wnd.open_wager();
                    self.mode = 44;
                    if let Some(c) = self.wnd_chat.as_mut() {
                        c.mode = 1;
                    }
                    self.duel.activate = true;
                }
                // CloseDuelWager: the chat box live again, put away, play
                Do::CloseWager => {
                    if let Some(c) = self.wnd_chat.as_mut() {
                        c.mode = 0;
                    }
                    self.duel.wnd.close();
                    self.mode = 0;
                }
                Do::Line(text, color) => {
                    eprintln!("chat: {text}");
                    self.add_chat(text, color);
                }
                Do::Send(m) => self.session.send(&m),
            }
        }
    }

    /// `UpdateGame`'s `Process(dt)`: the waiting invitations counted down by
    /// the client's clock, and the first shown once the HUD is back in play.
    pub(super) fn duel_tick(&mut self) {
        let clock = self.session.timer();
        let dt = self.duel.last.map_or(0, |l| clock - l);
        self.duel.last = Some(clock);
        if let Some(d) = self.duel.part.process(dt as u32, self.mode) {
            self.duel_do(vec![d], None);
        }
    }

    /// `DoAction` for the duel's ids (`IsMine`, `Use`): Invite challenges the
    /// target, Surrender surrenders. True when it was one of them.
    pub(super) fn duel_action(&mut self, id: u32) -> bool {
        if !crate::duel::is_mine(id) {
            return false;
        }
        let me = self.self_id32();
        if let Some(d) = self.duel.part.use_msg(id, self.target, me, self.words.as_ref()) {
            self.duel_do(vec![d], None);
        }
        true
    }

    /// `DoAction`'s Quit (0x0A010000): a duel on or set is surrendered first.
    pub(super) fn duel_logout(&mut self) {
        let p = &self.duel.part;
        if (p.running || p.set)
            && let Some(d) = p.surrender()
        {
            self.duel_do(vec![d], None);
        }
    }

    /// A new game connection (`PumpMessages`' connect): the part reset.
    pub(super) fn duel_reset(&mut self) {
        self.duel.part.reset();
        if self.duel_owns() {
            self.duel.wnd.close();
            self.mode = 0;
        }
    }

    /// The window's focus, keys, drawing, and what it asks.
    pub(super) fn run_duel(&mut self, vui: &vi::ui::Vui, r: &mut dyn crate::ui::Layout) {
        let clock = self.session.timer();
        if std::mem::take(&mut self.duel.activate) && self.duel_owns() {
            self.duel.wnd.activate(vui, r, clock);
        }
        if !self.duel_owns() {
            self.duel.keys.clear();
            return;
        }
        let purse = self.dump.player.p34 as i32;
        let flags = net::session::EXPANSION_FLAGS;
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let (class, race, level) = (pl.class, pl.race, pl.level);
        let mut cues = Vec::new();
        let mut outs = Vec::new();
        {
            let info = crate::infotext::Info {
                words: &vui.words,
                inv: &self.dump.inventory,
                hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
                base: &self.stats.base,
                class,
                level,
                attack_words: &attack_words,
            };
            let icon = |it: &net::dump::NetItem| crate::inventory::item_icon(it, flags) as i32;
            let can_use = |it: &net::dump::NetItem| crate::inventory::can_use(it, class, race, level);
            let d = &mut self.duel;
            for (k, down) in std::mem::take(&mut d.keys) {
                d.wnd.key(vui, r, &d.part, purse, k, down, &mut cues, &mut outs);
            }
            d.wnd.draw(vui, r, &d.part, &info, &icon, &can_use, clock, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        let name = self.duel_name();
        for o in outs {
            eprintln!("play: duel window {o:?}");
            let d = match o {
                WndOut::Accept(id) => self.duel.part.accept(id, &name),
                WndOut::Decline(id) => self.duel.part.decline(id, &name),
                WndOut::Add(key, n) => vec![self.duel.part.add_item(key, n)],
                WndOut::Remove(key) => vec![self.duel.part.remove_item(key)],
                WndOut::Tunar(v) => self.duel.part.wager_tunar(v, purse.max(0) as u32).into_iter().collect(),
                WndOut::AcceptWager => vec![self.duel.part.accept_wager()],
                WndOut::Unaccept => vec![self.duel.part.unaccept()],
                WndOut::CancelDuel => self.duel.part.cancel_duel(),
                WndOut::Chat(text) => {
                    if !text.is_empty() {
                        self.chat_line(&text);
                    }
                    Vec::new()
                }
            };
            self.duel_do(d, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};
    use net::session::{Player, Session};
    use net::stream::Writer;

    /// Play on a session nothing answers, recording what it sends (as
    /// `npcw`'s tests make it), the player 0x100 called Ltcomdata.
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
        let mut p = Play::new(s, Player::default(), Box::default());
        p.dump.player.name = "Ltcomdata".into();
        p
    }

    /// What `p` has sent since the last look, the 0x1501s alone.
    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        p.session.record.as_mut().map(std::mem::take).unwrap_or_default().into_iter().map(|(_, d)| d).filter(|d| d.starts_with(&[0x01, 0x15])).collect()
    }

    /// What `p` has sent since the last look, every message.
    fn sent_all(p: &mut Play) -> Vec<Vec<u8>> {
        p.session.record.as_mut().map(std::mem::take).unwrap_or_default().into_iter().map(|(_, d)| d).collect()
    }

    /// A 0x1502's body as go-eqoa-server writes it.
    fn down(sub: u8, id: u32, rest: &[u8]) -> Vec<u8> {
        let mut v = vec![sub];
        v.extend(id.to_le_bytes());
        v.extend(rest);
        v
    }

    fn invitation(id: u32) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(6).u8(0).u8(40).string(b"Lanerender").wide("").wide("");
        down(2, id, &w.data)
    }

    /// The HUD's frame for the duel window: its focus, keys, drawing.
    fn hud(p: &mut Play) {
        let (v, mut r) = (vui(), NullRaster);
        p.run_duel(&v, &mut r);
    }

    fn press(p: &mut Play, key: i32) {
        p.duel_key(key, true);
        p.duel_key(key, false);
        hud(p);
    }

    /// An invitation in play opens the window (mode 43, its 295 with the next
    /// HUD frame); ✕ accepts it: 0x1501 sub 3 with the invitation's id, back
    /// to play. One that comes over another window waits until play and is
    /// then shown by `Process`; △ declines it with the player's name. The
    /// control: in the start menu nothing opens.
    #[test]
    fn an_invitation_opens_the_window_and_is_answered() {
        let mut p = play();
        assert!(p.duel_opcode(0x1502, &invitation(0x21), None));
        assert_eq!(p.mode, 43);
        hud(&mut p);
        assert!(p.duel.wnd.visible && p.duel_owns());
        assert!(p.cues.iter().any(|c| matches!(c, crate::sound::Cue::Ui(15))), "ShowInvite's sound");
        press(&mut p, 134);
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x15, 3, 0, 0, 0, 0, 0x21, 0, 0, 0]]);
        assert_eq!(p.mode, 0);
        assert!(!p.duel.wnd.visible);
        // over the start menu it waits
        p.mode = 13;
        p.duel_opcode(0x1502, &invitation(0x22), None);
        assert_eq!((p.mode, p.duel.part.queue.len()), (13, 1), "the control: queued, nothing opened");
        p.mode = 0;
        p.duel_tick();
        assert_eq!(p.mode, 43, "Process shows it once in play");
        hud(&mut p);
        press(&mut p, 27);
        let mut want = vec![0x01, 0x15, 2, 0, 0, 0, 0, 0x22, 0, 0, 0, 9, 0, 0, 0];
        want.extend(b"Ltcomdata");
        assert_eq!(sent(&mut p), vec![want]);
        assert_eq!((p.mode, p.duel.part.queue.len()), (0, 0));
        // the server's lapse while it is up: the live word, the window away
        p.duel_opcode(0x1502, &invitation(0x23), None);
        hud(&mut p);
        p.duel_opcode(0x1502, &down(3, 0x23, &[]), None);
        assert_eq!(p.mode, 0);
        assert_eq!(p.chat.back().map(|l| l.0.as_str()), Some("The duel invitation has expired."));
    }

    /// The wager (13) opens over play with the chat box fixed, mode 44; both
    /// accepts (15) close it, the chat box live again. The control: one
    /// accept leaves it up.
    #[test]
    fn the_wager_opens_and_closes() {
        let mut p = play();
        p.wnd_chat = Some(crate::chat::WndChat::new());
        p.duel_opcode(0x1502, &down(13, 5, &[]), None);
        assert_eq!((p.mode, p.wnd_chat.as_ref().unwrap().mode), (44, 1));
        hud(&mut p);
        assert!(p.duel.wnd.visible && p.duel.wnd.wager);
        p.duel_opcode(0x1502, &down(15, 5, &[1, 1, 0]), None);
        assert_eq!(p.mode, 44, "the control: one accept");
        p.duel_opcode(0x1502, &down(15, 5, &[1, 1, 1]), None);
        assert_eq!((p.mode, p.wnd_chat.as_ref().unwrap().mode), (0, 0));
        assert!(!p.duel.wnd.visible);
        // the accepts are the server's to clear: 13 leaves them (and the
        // tunar) as the last 12 or 15 set them, so a second wager starts
        // accepted until the server says otherwise, as the live part does
        p.duel_opcode(0x1502, &down(13, 6, &[]), None);
        assert!(p.duel.part.my_acc && p.duel.part.their_acc);
        hud(&mut p);
        press(&mut p, 134);
        assert!(sent(&mut p).is_empty(), "accepted: ✕ stakes nothing");
        p.duel_opcode(0x1502, &down(15, 6, &[0, 0, 0]), None);
        // its tunar: right to the offer, down to the tunar, ✕ the deposit
        // (the purse its limit), up a unit, ✕ staked
        p.dump.player.p34 = 60;
        for k in [137, 139, 134] {
            press(&mut p, k);
        }
        assert!(sent(&mut p).is_empty(), "the deposit up, nothing staked yet");
        press(&mut p, 138);
        press(&mut p, 134);
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x15, 5, 6, 0, 0, 0, 1, 0, 0, 0]]);
    }

    /// Community > Duel > Invite challenges the target and Surrender
    /// surrenders, once the server's feature word has the duel bit; before
    /// it (the control: a server without duels) nothing goes.
    #[test]
    fn the_menu_challenges_and_surrenders() {
        let mut p = play();
        p.target = 0x55;
        p.do_action(crate::duel::MSG_INVITE, 0);
        p.do_action(crate::duel::MSG_SURRENDER, 0);
        assert!(sent(&mut p).is_empty(), "the control: no feature word, nothing");
        assert!(p.duel_opcode(0x07F5, &0x10u32.to_le_bytes(), None));
        p.do_action(crate::duel::MSG_INVITE, 0);
        p.do_action(crate::duel::MSG_SURRENDER, 0);
        assert_eq!(sent(&mut p), vec![vec![0x01, 0x15, 1, 0, 0, 0, 0, 0x55, 0, 0, 0], vec![0x01, 0x15, 4, 0, 0, 0, 0]]);
        // at the player: the live refusal, in red, nothing sent
        p.target = 0x100;
        p.do_action(crate::duel::MSG_INVITE, 0);
        assert!(sent(&mut p).is_empty());
        assert_eq!(p.chat.back(), Some(&("You cannot duel yourself.".to_string(), crate::duel::RED)));
    }

    /// The compass's duel patch: sub 8's centre while sub 7 has the duel on
    /// and the player is in its world. The controls: off, or another world,
    /// none.
    #[test]
    fn the_compass_has_the_duels_centre() {
        let mut p = play();
        let mut c = Writer::default();
        c.u32(25274.5f32.to_bits()).u32(54.1f32.to_bits()).u32(15723.0f32.to_bits()).u8(0);
        p.duel_opcode(0x1502, &down(8, 9, &c.data), None);
        assert_eq!(p.duel_patch(), None, "the control: the duel not on");
        let mut w = Writer::default();
        w.u8(1).u8(1).u32(0x55).u32(u32::MAX);
        p.duel_opcode(0x1502, &down(7, 9, &w.data), None);
        assert_eq!(p.duel_patch(), Some([25274.5, 54.1, 15723.0]));
        p.player.world = 2;
        assert_eq!(p.duel_patch(), None, "the control: another world");
    }

    /// Quit with a duel on surrenders it first (the live `DoAction`'s
    /// LOGOUT), before the logout's 0x09B0; with none, the logout alone (the
    /// control).
    #[test]
    fn a_logout_surrenders_a_duel() {
        let mut q = play();
        q.duel_opcode(0x07F5, &0x10u32.to_le_bytes(), None);
        q.do_action(0x0A01_0000, 0);
        assert_eq!(sent_all(&mut q), vec![vec![0xB0, 0x09]], "the control: no duel");
        let mut p = play();
        p.duel_opcode(0x07F5, &0x10u32.to_le_bytes(), None);
        let mut w = Writer::default();
        w.u8(1).u8(1).u32(0x55).u32(u32::MAX);
        p.duel_opcode(0x1502, &down(7, 9, &w.data), None);
        p.do_action(0x0A01_0000, 0);
        assert_eq!(sent_all(&mut p), vec![vec![0x01, 0x15, 4, 9, 0, 0, 0], vec![0xB0, 0x09]]);
    }

    /// `TargetInRange` lets a player hit the player and the pet the duel
    /// names (sub 7), as the live client asks `IsDueling`; anyone else of a
    /// player's side stays out of reach (the control, the disc's rule), and
    /// so does the opponent once the duel is over. Needs the disc (SKIPs
    /// without): a walk on Freeport's ground.
    #[test]
    fn the_duel_opponent_may_be_fought() {
        let (x, z) = (25275.115f32, 15766.627f32);
        let Some(world) = crate::walker::tests::tunaria_round(x, z) else { return };
        let walker = crate::walker::Walker::new(world, x, z, 0.0);
        let at = walker.cam.walk.committed.position.to_array();
        let mut p = play();
        let ent = |id: u32, side: u8, dx: f32| net::entity::Entity {
            id,
            kind: 130,
            p19: 50,
            pos: [at[0] + dx, at[1], at[2]],
            actor_pos: [at[0] + dx, at[1], at[2]],
            creature: Some(net::entity::Creature { p256: side, name: format!("{id:x}"), ..Default::default() }),
            ..Default::default()
        };
        p.entities.slots[0] = Some(ent(0x100, 2, 0.0));
        p.entities.slots[1] = Some(ent(0x55, 2, 1.0));
        p.entities.slots[2] = Some(ent(0x56, 3, 1.5));
        p.entities.slots[3] = Some(ent(0x57, 2, 1.0));
        let reach = |p: &mut Play, id: u32| {
            p.target = id;
            p.target_in_range(&walker, -1)
        };
        assert!(!reach(&mut p, 0x55) && !reach(&mut p, 0x56), "the control: no duel, players and pets out of reach");
        let mut w = Writer::default();
        w.u8(1).u8(1).u32(0x55).u32(0x56);
        p.duel_opcode(0x1502, &down(7, 9, &w.data), None);
        assert!(reach(&mut p, 0x55), "the opponent");
        assert!(reach(&mut p, 0x56), "its pet");
        assert!(!reach(&mut p, 0x57), "another player");
        let mut off = Writer::default();
        off.u8(0).u8(0).u32(u32::MAX).u32(u32::MAX);
        p.duel_opcode(0x1502, &down(7, 9, &off.data), None);
        assert!(!reach(&mut p, 0x55), "the duel over");
    }

    /// `CycleTarget` passes over a pet (of a player's side), but the duel's:
    /// with the duel naming it, R1 takes it. The control: without a duel the
    /// only creature in view is cycled to and the pet never is.
    #[test]
    fn the_cycle_takes_the_duel_pet() {
        let mut p = play();
        let ent = |id: u32, side: u8, at: [f32; 3]| net::entity::Entity {
            id,
            kind: 130,
            p18: 1,
            p19: 1,
            pos: at,
            actor_pos: at,
            creature: Some(net::entity::Creature { p256: side, name: format!("{id:x}"), ..Default::default() }),
            ..Default::default()
        };
        p.entities.slots[0] = Some(ent(0x56, 3, [0.0, 0.0, -10.0]));
        p.entities.slots[1] = Some(ent(0x70, 0, [3.0, 0.0, -30.0]));
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0);
        let vp = proj * glam::camera::rh::view::look_to_mat4(glam::Vec3::ZERO, glam::Vec3::NEG_Z, glam::Vec3::Y);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..3 {
            p.cycle_target(false, glam::Vec3::ZERO, glam::Vec3::NEG_Z, vp, vp);
            seen.insert(p.target);
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![0x70], "the control: the pet passed over");
        let mut w = Writer::default();
        w.u8(1).u8(1).u32(0x55).u32(0x56);
        p.duel_opcode(0x1502, &down(7, 9, &w.data), None);
        p.cycle_target(false, glam::Vec3::ZERO, glam::Vec3::NEG_Z, vp, vp);
        assert_eq!(p.target, 0x56, "the duel's pet, nearer, first");
    }

    /// The feature word puts Community > Duel (Invite, Surrender) in the
    /// start menu sixth, after Chat Modes, as the live menu has it, and takes
    /// it out again; the disc's Community (the control) has none. Needs
    /// `run/client.bin` (SKIPs without).
    #[test]
    fn the_feature_word_puts_duel_in_community() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/client.bin");
        let Some(img) = std::fs::read(&path).ok().and_then(crate::client_image::ClientImage::from_bytes) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        let mut p = play();
        p.client_data(&img);
        let community = |p: &Play| {
            let s = p.start.as_ref().unwrap();
            let m = s.menus.list.iter().find(|m| m.x04 == 420).unwrap();
            m.entries.iter().map(|e| (e.word, e.sub.map(|k| s.menus.list[k].entries.iter().map(|e| (e.word, e.msg)).collect::<Vec<_>>()))).collect::<Vec<_>>()
        };
        let disc = community(&p);
        assert_eq!(disc.len(), 11);
        assert!(disc.iter().all(|(w, _)| *w != 1864), "the control: the disc's Community has no Duel");
        p.duel_opcode(0x07F5, &0x10u32.to_le_bytes(), None);
        let live = community(&p);
        assert_eq!(live.len(), 12);
        assert_eq!(disc[4].0, 530, "Chat Modes fifth");
        assert_eq!(live[5], (1864, Some(vec![(86, crate::duel::MSG_INVITE), (1865, crate::duel::MSG_SURRENDER)])), "Duel sixth");
        assert_eq!([&live[..5], &live[6..]].concat(), disc, "the rest as the disc's");
        // a server without duels takes it out; the menus read again keep it
        p.duel_opcode(0x07F5, &0x1Bu32.to_le_bytes(), None);
        assert_eq!(community(&p).len(), 12, "0x1B has the duel bit");
        p.duel_opcode(0x07F5, &0x0Bu32.to_le_bytes(), None);
        assert_eq!(community(&p), disc);
        p.duel_opcode(0x07F5, &0x10u32.to_le_bytes(), None);
        p.client_data(&img);
        assert_eq!(community(&p).len(), 12, "the menus read again: still there");
    }
}
