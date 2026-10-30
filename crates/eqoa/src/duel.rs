//! Duels as SOE's live game had them (its final CLIENT, eqoa-live-1500,
//! read in a retail save state; the Frontiers disc has none of it): the
//! client's duel part (`VIClient+0x50DE8`, its `GetDuelPart`), its two
//! opcodes and the words they show. `duelwnd.rs` is its window
//! (`VIWndDuel`), `duelplay.rs` ties both to play.
//!
//! 0x1501 goes up and 0x1502 comes down, `{u8 sub, u32 duel id, ...}`: the
//! part keeps the id the server last gave it (+0x08) and sends it back with
//! every request. Down: 1 the id; 2 an invitation (race, class, level, name,
//! guild, title: shown at once while the HUD is in play, else queued, a tenth
//! one declined); 3 an invitation gone; 4 and 5 a declined challenge; 6 the
//! rest of the queue declined; 7 the duel's state (on, set, and the two ids
//! it may fight: the opponent and its pet, `IsDueling`); 8 its centre and
//! world; 10 and 11 an item staked or taken back; 12 both sides' tunar and
//! accepts; 13 the wager window opened (the inventory copied); 14 it closed;
//! 15 the accepts (both set: the window closes). Up: 1 a challenge at the
//! target, 2 decline, 3 accept, 4 surrender, 5 the tunar staked, 6 and 7 an
//! item staked or taken back, 8 the wager accepted (both sides' item keys),
//! 9 the accept withdrawn, 10 the duel called off.
//!
//! The challenge and the surrender go only to a server whose feature word
//! (0x07F5, `VIClient+0x813C`) has bit 0x10: SOE's sent 0x1B, go-eqoa-server
//! 0x10; the disc has no case for 0x07F5, and Sandstorm sends none.
//!
//! go-eqoa-server offers these messages with its own 0x7E09 (`net::session::
//! OP_DUEL_WINDOW`, answered at once) and otherwise duels a client by boxes.
//!
//! The words: the live client's WORDS.TXT had 76 rows past the disc's 1,814,
//! and the duel's are among them (1859-1869, 1872, 1883). The client
//! carries the live words it shows (`live_word`) and takes each from the
//! table whenever the table has that row (a server's own WORDS.TXT), as
//! `gear.rs` carries the two the live toolbars need.

use net::dump::{ItemList, NetItem};
use net::stream::{Reader, Writer};
use vi::strtab::StringTable;

/// The client's duel opcode.
pub const OP_OUT: u16 = 0x1501;
/// The server's.
pub const OP_IN: u16 = 0x1502;
/// The server's feature word (`VIClient+0x813C`, the old docs' "Camera2").
pub const OP_FEATURES: u16 = 0x07F5;
/// The feature word's duel bit: without it a challenge and a surrender send
/// nothing.
pub const FEATURE_DUELS: u32 = 0x10;
/// The start menu's Community > Duel > Invite and Surrender (`DoAction`).
pub const MSG_INVITE: u32 = 0xAB00_0001;
pub const MSG_SURRENDER: u32 = 0xAB00_0002;
/// An invitation's time, in the client's clock's ticks (VITimer's, 983.04 a
/// second): the record's timer as sub 2 makes it.
pub const INVITE_TIME: u32 = 20000;
/// The invitations kept waiting while a window is up; one more is declined.
pub const QUEUE: usize = 10;
/// The duel's lines (`VIColor32F` through 0x012D29F8): orange its news, red
/// a refusal.
pub const ORANGE: [f32; 4] = [1.0, 0.64, 0.28, 1.0];
pub const RED: [f32; 4] = [1.0, 0.25, 0.25, 1.0];

/// The live client's words the duel shows, by WORDS.TXT row.
pub fn live_word(id: i32) -> Option<&'static str> {
    Some(match id {
        1859 => "The duel invitation has expired.",
        1860 => "Your duel invitation has been declined.",
        1861 => "%s declined your duel invitation.",
        1862 => "You cannot duel yourself.",
        1863 => "Invalid target.",
        1864 => "Duel",
        1865 => "Surrender",
        1866 => "%s challenges you to a duel.",
        1867 => "Wager",
        1868 => "You",
        1869 => "Opponent",
        1872 => "Cancel duel?",
        1883 => "%s is in a duel.",
        _ => return None,
    })
}

/// Word `id`: the table's, else (a row it lacks, "UNUSED" or empty) the live
/// client's.
pub fn word(words: &StringTable, id: i32, language: i32) -> String {
    let w = words.get(id, language);
    match live_word(id) {
        Some(live) if w.is_empty() || w == "(null)" || w.eq_ignore_ascii_case("unused") => live.to_string(),
        _ => w.to_string(),
    }
}

/// `word` with no table at hand: the live word, else none.
pub fn word_of(words: Option<&StringTable>, id: i32) -> String {
    match words {
        Some(w) => word(w, id, 0),
        None => live_word(id).map_or_else(|| format!("(word {id})"), str::to_string),
    }
}

/// An invitation (sub 2's 300-byte record; the window copies the first 292):
/// the challenger and the timer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invite {
    /// +0x00: the invitation's id (the duel's, while it is asked)
    pub id: u32,
    /// +0x04, +0x05, +0x06
    pub race: u8,
    pub class: u8,
    pub level: u8,
    /// +0x07: up to 23 characters
    pub name: String,
    /// +0x20, +0xA0: wide, up to 63 each
    pub guild: Vec<u16>,
    pub title: Vec<u16>,
    /// +0x120: ticks left, counted down while it waits and while it is shown
    pub timer: u32,
}

/// What the part asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Do {
    /// `VIWndGame::ShowDuelInvite`: the invitation window (mode 43)
    ShowInvite(Invite),
    /// `CloseDuelInvite`: the window put away, back to play
    CloseInvite,
    /// `OpenDuelWager`: the wager window (mode 44)
    OpenWager,
    /// `CloseDuelWager`
    CloseWager,
    /// a line in the chat box, in its colour
    Line(String, [f32; 4]),
    /// a 0x1501 to the server
    Send(Vec<u8>),
}

/// What the part reads off the client when a message comes.
pub struct Ctx<'a> {
    /// `VIWndGame`'s mode: 0 play (`IsLootable`), 43 the invitation up, 44
    /// the wager
    pub mode: i32,
    pub words: Option<&'a StringTable>,
    /// the player's name, as a decline carries it
    pub name: &'a str,
    /// the inventory (sub 13 copies it)
    pub inv: &'a ItemList,
}

/// The duel part.
#[derive(Clone, Debug)]
pub struct Part {
    /// `VIClient+0x813C`: the server's feature word (0x07F5)
    pub features: u32,
    /// +0x08: the duel's id as the server last gave it
    pub duel_id: u32,
    /// +0x0C, +0x0D: a fight on, a duel set
    pub running: bool,
    pub set: bool,
    /// +0x10, +0x1C: the duel's centre and world (the compass's patch)
    pub centre: [f32; 3],
    pub world: u8,
    /// +0x20, +0x24: who the player may fight (`IsDueling`): the opponent and
    /// its pet; -1 none
    pub a: u32,
    pub b: u32,
    /// +0x28: the invitations waiting
    pub queue: Vec<Invite>,
    /// +0x40, +0x44: the tunar staked, the player's and the opponent's
    pub my_tunar: u32,
    pub their_tunar: u32,
    /// +0x48, +0x49: the wager accepted, by the player and the opponent
    pub my_acc: bool,
    pub their_acc: bool,
    /// +0x4C: the copy of the inventory the player stakes from; +0x64, +0x7C
    /// the player's offer and the opponent's
    pub inv: Vec<NetItem>,
    pub mine: Vec<NetItem>,
    pub theirs: Vec<NetItem>,
}

impl Default for Part {
    fn default() -> Self {
        Part {
            features: 0,
            duel_id: 0,
            running: false,
            set: false,
            centre: [0.0; 3],
            world: 0,
            a: u32::MAX,
            b: u32::MAX,
            queue: Vec::new(),
            my_tunar: 0,
            their_tunar: 0,
            my_acc: false,
            their_acc: false,
            inv: Vec::new(),
            mine: Vec::new(),
            theirs: Vec::new(),
        }
    }
}

/// `IsMine` (0x012E4910): a `DoAction` id of the duel's.
pub fn is_mine(msg: u32) -> bool {
    msg & 0xFF00_0000 == 0xAB00_0000
}

/// A 0x1501: the sub-op and the id held.
fn up(sub: u8, id: u32) -> Writer {
    let mut w = Writer::op(OP_OUT);
    w.u8(sub).u32(id);
    w
}

/// The player's name as `GetPlayerName` copies it into 24 bytes.
fn name24(name: &str) -> Vec<u8> {
    name.as_bytes().iter().copied().take(23).collect()
}

/// `VIPool<int>` on a stream: its size and each value, zigzag.
fn pool(w: &mut Writer, v: &[u32]) {
    w.signed(v.len() as i64);
    for &k in v {
        w.signed(k as i32 as i64);
    }
}

impl Part {
    /// `Reset` (0x012E47B8): on a new game connection.
    pub fn reset(&mut self) {
        let features = self.features;
        *self = Part { features, ..Part::default() };
    }

    /// The server lets the player duel (its feature word's bit).
    pub fn on(&self) -> bool {
        self.features & FEATURE_DUELS != 0
    }

    /// `IsDueling` (0x012E6980): `id` may be fought, the duel being on.
    pub fn is_dueling(&self, id: u32) -> bool {
        self.running && (id == self.a || id == self.b)
    }

    /// `Process(dt)` (0x012E4850, `UpdateGame`'s, `dt` the client clock's
    /// ticks since the last): with the HUD in play the first invitation
    /// waiting is shown (and stays queued until it is answered or gone);
    /// every one waiting counts down.
    pub fn process(&mut self, dt: u32, mode: i32) -> Option<Do> {
        let show = (mode == 0).then(|| self.queue.first().cloned()).flatten().map(Do::ShowInvite);
        for r in &mut self.queue {
            r.timer = r.timer.wrapping_sub(dt);
        }
        show
    }

    /// `Recv` (0x012E4988): a 0x1502's body (from its sub-op), what it asks
    /// of the game in order. A message cut short is taken as far as it reads,
    /// as the stream's zeroes read past its end.
    pub fn recv(&mut self, data: &[u8], cx: &Ctx) -> Vec<Do> {
        let mut r = Reader::new(data);
        let sub = r.u8();
        let mut out = Vec::new();
        let word = |id: i32| word_of(cx.words, id);
        match sub {
            1 => self.duel_id = r.u32(),
            2 => {
                let id = r.u32();
                let race = r.u8();
                let class = r.u8();
                let level = r.u8();
                let name = String::from_utf8_lossy(&r.string(24)).into_owned();
                let guild = r.wide(64).encode_utf16().collect();
                let title = r.wide(64).encode_utf16().collect();
                let rec = Invite { id, race, class, level, name, guild, title, timer: INVITE_TIME };
                if cx.mode == 0 {
                    out.push(Do::ShowInvite(rec));
                } else if self.queue.len() < QUEUE {
                    self.queue.push(rec);
                } else {
                    let mut w = up(2, self.duel_id);
                    w.u32(rec.id).string(&name24(cx.name));
                    out.push(Do::Send(w.data));
                }
            }
            3 => {
                let id = r.u32();
                self.queue.retain(|q| q.id != id);
                if cx.mode == 43 {
                    out.push(Do::Line(word(1859), ORANGE));
                    out.push(Do::CloseInvite);
                }
            }
            4 => {
                self.duel_id = r.u32();
                out.push(Do::Line(word(1860), ORANGE));
            }
            5 => {
                self.duel_id = r.u32();
                let name = String::from_utf8_lossy(&r.string(24)).into_owned();
                out.push(Do::Line(printf_s(&word(1861), &name), ORANGE));
            }
            6 => {
                self.duel_id = r.u32();
                out.extend(self.decline_all(cx.name));
            }
            7 => {
                self.duel_id = r.u32();
                self.running = r.u8() != 0;
                self.set = r.u8() != 0;
                self.a = r.u32();
                self.b = r.u32();
            }
            8 => {
                self.duel_id = r.u32();
                self.centre = [r.f32(), r.f32(), r.f32()];
                self.world = r.u8();
            }
            10 => {
                self.duel_id = r.u32();
                self.my_acc = r.u8() != 0;
                self.their_acc = r.u8() != 0;
                let mine = r.u8() != 0;
                let key = r.u32();
                let n = r.u32() as i64;
                let item = net::dump::read_net_item(&mut r);
                self.item_added(mine, key, n, item);
            }
            11 => {
                self.duel_id = r.u32();
                self.my_acc = r.u8() != 0;
                self.their_acc = r.u8() != 0;
                let mine = r.u8() != 0;
                let key = r.u32();
                let item = net::dump::read_net_item(&mut r);
                self.item_removed(mine, key, item);
            }
            12 => {
                self.duel_id = r.u32();
                self.my_tunar = r.u32();
                self.their_tunar = r.u32();
                self.my_acc = r.u8() != 0;
                self.their_acc = r.u8() != 0;
            }
            13 => {
                self.duel_id = r.u32();
                self.inv = cx.inv.items.clone();
                self.mine.clear();
                self.theirs.clear();
                out.push(Do::OpenWager);
            }
            14 => {
                self.duel_id = r.u32();
                if cx.mode == 44 {
                    out.push(Do::CloseWager);
                }
            }
            15 => {
                self.duel_id = r.u32();
                self.set = r.u8() != 0;
                self.my_acc = r.u8() != 0;
                self.their_acc = r.u8() != 0;
                if cx.mode == 44 && self.my_acc && self.their_acc {
                    out.push(Do::CloseWager);
                }
            }
            // 9, and anything past 15: nothing read
            _ => {}
        }
        out
    }

    /// Sub 10 (0x012E5340): an item staked. The player's own comes out of the
    /// inventory copy (by its key, while the copy holds that many): onto the
    /// offer's record of that key, else a new one while there are fewer than
    /// four, the copy's record taken down (gone when all of it went); the
    /// opponent's onto its offer the same way.
    fn item_added(&mut self, mine: bool, key: u32, n: i64, mut item: NetItem) {
        if mine {
            let Some(i) = self.inv.iter().position(|it| it.inv_index == key) else { return };
            if self.inv[i].quantity < n {
                return;
            }
            match self.mine.iter().position(|it| it.inv_index == key) {
                Some(j) => self.mine[j].quantity += n,
                None if self.mine.len() < 4 => {
                    item.quantity = n;
                    self.mine.push(item);
                }
                None => return,
            }
            if self.inv[i].quantity == n {
                self.inv.remove(i);
            } else {
                self.inv[i].quantity -= n;
            }
        } else {
            match self.theirs.iter().position(|it| it.inv_index == key) {
                Some(j) => self.theirs[j].quantity += n,
                None if self.theirs.len() < 4 => {
                    item.quantity = n;
                    self.theirs.push(item);
                }
                None => {}
            }
        }
    }

    /// Sub 11 (0x012E5648): an item taken back. The player's own leaves the
    /// offer (by its key) and goes back into the inventory copy: onto the
    /// first record of the same item, up to the item's stack (as shipped,
    /// whatever its stack), else a record of its own; the opponent's leaves
    /// its offer.
    fn item_removed(&mut self, mine: bool, key: u32, item: NetItem) {
        let offer = if mine { &mut self.mine } else { &mut self.theirs };
        let Some(j) = offer.iter().position(|it| it.inv_index == key) else { return };
        offer.remove(j);
        if !mine {
            return;
        }
        match self.inv.iter().position(|it| it.item.id == item.item.id) {
            Some(k) => self.inv[k].quantity = (self.inv[k].quantity + item.quantity).min(item.item.max_stack),
            None => self.inv.push(item),
        }
    }

    /// `Challenge(target)` (0x012E5898): nothing unless the server lets the
    /// player duel; at the player's own id word 1862 and at no target 1863,
    /// in red; else sub 1 with the target.
    pub fn challenge(&self, target: u32, me: Option<u32>, words: Option<&StringTable>) -> Option<Do> {
        if !self.on() {
            return None;
        }
        if Some(target) == me {
            return Some(Do::Line(word_of(words, 1862), RED));
        }
        if target == u32::MAX {
            return Some(Do::Line(word_of(words, 1863), RED));
        }
        let mut w = up(1, self.duel_id);
        w.u32(target);
        Some(Do::Send(w.data))
    }

    /// `Accept(id)` (0x012E5A38): the invitation out of the queue, sub 3, the
    /// rest of the queue declined, the window put away.
    pub fn accept(&mut self, id: u32, name: &str) -> Vec<Do> {
        self.queue.retain(|q| q.id != id);
        let mut w = up(3, self.duel_id);
        w.u32(id);
        let mut out = vec![Do::Send(w.data)];
        out.extend(self.decline_all(name));
        out.push(Do::CloseInvite);
        out
    }

    /// `Decline(id)` (0x012E5BE8): the invitation out of the queue, sub 2 with
    /// the player's name, the window put away.
    pub fn decline(&mut self, id: u32, name: &str) -> Vec<Do> {
        self.queue.retain(|q| q.id != id);
        let mut w = up(2, self.duel_id);
        w.u32(id).string(&name24(name));
        vec![Do::Send(w.data), Do::CloseInvite]
    }

    /// 0x012E6600: every invitation waiting declined, the queue emptied.
    pub fn decline_all(&mut self, name: &str) -> Vec<Do> {
        let out = self
            .queue
            .iter()
            .map(|q| {
                let mut w = up(2, self.duel_id);
                w.u32(q.id).string(&name24(name));
                Do::Send(w.data)
            })
            .collect();
        self.queue.clear();
        out
    }

    /// `Surrender` (0x012E5DB8): sub 4, where the server lets the player duel.
    pub fn surrender(&self) -> Option<Do> {
        self.on().then(|| Do::Send(up(4, self.duel_id).data))
    }

    /// 0x012E5EB0 (the deposit's OK): the tunar staked, no more than the
    /// purse holds, until the player accepts.
    pub fn wager_tunar(&self, v: u32, purse: u32) -> Option<Do> {
        if self.my_acc {
            return None;
        }
        let mut w = up(5, self.duel_id);
        w.u32(purse.min(v));
        Some(Do::Send(w.data))
    }

    /// 0x012E63C8: `n` of the inventory record `key` staked.
    pub fn add_item(&self, key: u32, n: u32) -> Do {
        let mut w = up(6, self.duel_id);
        w.u32(key).u32(n);
        Do::Send(w.data)
    }

    /// 0x012E64F8: the staked record `key` taken back.
    pub fn remove_item(&self, key: u32) -> Do {
        let mut w = up(7, self.duel_id);
        w.u32(key);
        Do::Send(w.data)
    }

    /// 0x012E5FD8: the wager accepted, with the keys of both offers as the
    /// player sees them.
    pub fn accept_wager(&self) -> Do {
        let mut w = up(8, self.duel_id);
        pool(&mut w, &self.mine.iter().map(|it| it.inv_index).collect::<Vec<_>>());
        pool(&mut w, &self.theirs.iter().map(|it| it.inv_index).collect::<Vec<_>>());
        Do::Send(w.data)
    }

    /// 0x012E61E0: the accept withdrawn.
    pub fn unaccept(&self) -> Do {
        Do::Send(up(9, self.duel_id).data)
    }

    /// 0x012E62D0 ("Cancel duel?" Yes): the window put away first, then sub 10.
    pub fn cancel_duel(&self) -> Vec<Do> {
        vec![Do::CloseWager, Do::Send(up(10, self.duel_id).data)]
    }

    /// `Use(msg)` (0x012E4928): Invite challenges the target, Surrender
    /// surrenders.
    pub fn use_msg(&self, msg: u32, target: u32, me: Option<u32>, words: Option<&StringTable>) -> Option<Do> {
        match msg {
            MSG_INVITE => self.challenge(target, me, words),
            MSG_SURRENDER => self.surrender(),
            _ => None,
        }
    }
}

/// `snprintf16(fmt, s)` for the duel's one-name words: the first %s (or %S)
/// made `s`.
pub fn printf_s(fmt: &str, s: &str) -> String {
    match (fmt.find("%s"), fmt.find("%S")) {
        (Some(a), Some(b)) if b < a => fmt.replacen("%S", s, 1),
        (Some(_), _) => fmt.replacen("%s", s, 1),
        (None, Some(_)) => fmt.replacen("%S", s, 1),
        (None, None) => fmt.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(mode: i32, inv: &'a ItemList) -> Ctx<'a> {
        Ctx { mode, words: None, name: "Ltcomdata", inv }
    }

    /// A 0x1502 as go-eqoa-server's `BuildDuel` writes it (after the opcode).
    fn down(sub: u8, id: u32, rest: &[u8]) -> Vec<u8> {
        let mut v = vec![sub];
        v.extend(id.to_le_bytes());
        v.extend(rest);
        v
    }

    /// Sub 2's body: race 6, class 0, level 40, "Lanerender", guild "Ours",
    /// title "Warlord".
    fn invitation(id: u32) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(6).u8(0).u8(40).string(b"Lanerender").wide("Ours").wide("Warlord");
        down(2, id, &w.data)
    }

    /// A `VINetItem` on the stream as `read_net_item` reads it: the six
    /// fields, the item's 23 zigzag words, an empty name and text, no stats.
    fn net_item(w: &mut Writer, it: &NetItem) {
        for v in [it.quantity, it.cur_hp, it.charges, it.equip_slot, it.quick_slot] {
            w.signed(v);
        }
        w.u32(it.inv_index);
        let i = &it.item;
        for v in [i.id, i.value, i.p08, i.icon, i.icon2, i.wear_slot, i.p18, i.no_trade, i.rent, i.p24, i.attack_type, i.damage, i.range, i.level_req, i.max_stack, i.max_hp, i.durability, i.class_mask, i.race_mask, i.p4c, i.lore, i.p58, i.p54] {
            w.signed(v);
        }
        w.u32(0).u32(0).signed(0);
    }

    fn sent(d: &[Do]) -> Vec<Vec<u8>> {
        d.iter().filter_map(|d| if let Do::Send(m) = d { Some(m.clone()) } else { None }).collect()
    }

    /// Sub 2 in play shows the invitation at once with its record (the id
    /// its own, not the part's), its timer at 20000 ticks; over another
    /// window it waits, up to ten; the eleventh is declined at once with the
    /// player's name, the part's id first. The control: the same message
    /// over a window queues instead of showing.
    #[test]
    fn an_invitation_shows_waits_or_is_declined() {
        let inv = ItemList::default();
        let mut p = Part { duel_id: 7, ..Default::default() };
        let d = p.recv(&invitation(0x21), &cx(0, &inv));
        let rec = Invite { id: 0x21, race: 6, class: 0, level: 40, name: "Lanerender".into(), guild: "Ours".encode_utf16().collect(), title: "Warlord".encode_utf16().collect(), timer: INVITE_TIME };
        assert_eq!(d, vec![Do::ShowInvite(rec.clone())]);
        assert_eq!((p.duel_id, p.queue.len()), (7, 0), "sub 2's id is the invitation's");
        for k in 0..QUEUE as u32 {
            assert!(p.recv(&invitation(0x30 + k), &cx(13, &inv)).is_empty(), "queued over the start menu");
        }
        assert_eq!(p.queue.len(), QUEUE);
        let d = p.recv(&invitation(0x99), &cx(13, &inv));
        let mut want = vec![0x01, 0x15, 2, 7, 0, 0, 0, 0x99, 0, 0, 0, 9, 0, 0, 0];
        want.extend(b"Ltcomdata");
        assert_eq!(sent(&d), vec![want], "the eleventh declined: {{2, held id, its id, name}}");
        assert_eq!(p.queue.len(), QUEUE);
    }

    /// `Process`: in play the first waiting invitation is shown and stays
    /// queued; every waiting one counts down by the ticks; over a window
    /// nothing is shown (the control).
    #[test]
    fn the_queue_shows_its_first_in_play_and_counts_down() {
        let inv = ItemList::default();
        let mut p = Part::default();
        p.recv(&invitation(1), &cx(13, &inv));
        p.recv(&invitation(2), &cx(13, &inv));
        assert_eq!(p.process(500, 13), None, "a window up: nothing shown");
        let Some(Do::ShowInvite(r)) = p.process(1000, 0) else { panic!("shown in play") };
        assert_eq!((r.id, r.timer), (1, INVITE_TIME - 500), "shown with the ticks it waited");
        assert_eq!(p.queue.iter().map(|q| (q.id, q.timer)).collect::<Vec<_>>(), vec![(1, INVITE_TIME - 1500), (2, INVITE_TIME - 1500)]);
    }

    /// Sub 3 takes its invitation out of the queue and, with the invitation
    /// window up (mode 43, whichever it shows), says 1859 in orange and puts
    /// the window away; in play only the queue changes (the control).
    #[test]
    fn a_lapsed_invitation_closes_the_window() {
        let inv = ItemList::default();
        let mut p = Part::default();
        p.recv(&invitation(1), &cx(13, &inv));
        p.recv(&invitation(2), &cx(13, &inv));
        let d = p.recv(&down(3, 1, &[]), &cx(43, &inv));
        assert_eq!(d, vec![Do::Line("The duel invitation has expired.".into(), ORANGE), Do::CloseInvite]);
        assert_eq!(p.queue.iter().map(|q| q.id).collect::<Vec<_>>(), vec![2]);
        assert!(p.recv(&down(3, 2, &[]), &cx(0, &inv)).is_empty());
        assert!(p.queue.is_empty());
    }

    /// Subs 4 and 5: the challenger told, in orange, the live words; 5 with
    /// the name. Each keeps the id it carries.
    #[test]
    fn a_declined_challenge_is_said() {
        let inv = ItemList::default();
        let mut p = Part::default();
        assert_eq!(p.recv(&down(4, 9, &[]), &cx(0, &inv)), vec![Do::Line("Your duel invitation has been declined.".into(), ORANGE)]);
        let mut w = Writer::default();
        w.string(b"Lanerender");
        assert_eq!(p.recv(&down(5, 10, &w.data), &cx(0, &inv)), vec![Do::Line("Lanerender declined your duel invitation.".into(), ORANGE)]);
        assert_eq!(p.duel_id, 10);
    }

    /// Sub 7 sets whom the player may fight: `IsDueling` is the opponent and
    /// its pet while the duel is on; off ({0,0,-1,-1}), nobody. The control:
    /// another id, and the same ids with the duel not running.
    #[test]
    fn the_duel_names_whom_the_player_may_fight() {
        let inv = ItemList::default();
        let mut p = Part::default();
        let mut w = Writer::default();
        w.u8(1).u8(1).u32(0x0186_A1).u32(0x0186_B0);
        p.recv(&down(7, 4, &w.data), &cx(0, &inv));
        assert!(p.is_dueling(0x0186_A1) && p.is_dueling(0x0186_B0));
        assert!(!p.is_dueling(0x0186_A2), "anyone else");
        assert!(!p.is_dueling(u32::MAX), "no pet: -1 is nobody while b is set");
        let mut off = Writer::default();
        off.u8(0).u8(1).u32(0x0186_A1).u32(0x0186_B0);
        p.recv(&down(7, 4, &off.data), &cx(0, &inv));
        assert!(!p.is_dueling(0x0186_A1), "not running");
        // sub 8: the centre and the world
        let mut c = Writer::default();
        c.u32(1.5f32.to_bits()).u32(2.0f32.to_bits()).u32((-3.25f32).to_bits()).u8(2);
        p.recv(&down(8, 4, &c.data), &cx(0, &inv));
        assert_eq!((p.centre, p.world), ([1.5, 2.0, -3.25], 2));
    }

    /// The requests' bytes, against the live code (each after the opcode
    /// 0x1501: the sub-op and the id held).
    #[test]
    fn the_requests_sent() {
        let mut p = Part { features: FEATURE_DUELS, duel_id: 0x0102_0304, ..Default::default() };
        let op = [0x01, 0x15];
        let msg = |rest: &[u8]| [&op[..], rest].concat();
        assert_eq!(p.challenge(0x55, Some(0x100), None), Some(Do::Send(msg(&[1, 4, 3, 2, 1, 0x55, 0, 0, 0]))));
        assert_eq!(p.surrender(), Some(Do::Send(msg(&[4, 4, 3, 2, 1]))));
        assert_eq!(p.wager_tunar(500, 120), Some(Do::Send(msg(&[5, 4, 3, 2, 1, 120, 0, 0, 0]))), "at most the purse");
        assert_eq!(p.wager_tunar(50, 120), Some(Do::Send(msg(&[5, 4, 3, 2, 1, 50, 0, 0, 0]))));
        assert_eq!(p.add_item(9, 2), Do::Send(msg(&[6, 4, 3, 2, 1, 9, 0, 0, 0, 2, 0, 0, 0])));
        assert_eq!(p.remove_item(9), Do::Send(msg(&[7, 4, 3, 2, 1, 9, 0, 0, 0])));
        assert_eq!(p.unaccept(), Do::Send(msg(&[9, 4, 3, 2, 1])));
        assert_eq!(p.cancel_duel(), vec![Do::CloseWager, Do::Send(msg(&[10, 4, 3, 2, 1]))]);
        // the accept: both offers' keys, VIPool<int>s (zigzag count, values)
        p.mine.push(NetItem { inv_index: 3, ..Default::default() });
        p.theirs.push(NetItem { inv_index: 70, ..Default::default() });
        p.theirs.push(NetItem { inv_index: 1, ..Default::default() });
        assert_eq!(p.accept_wager(), Do::Send(msg(&[8, 4, 3, 2, 1, 2, 6, 4, 140, 1, 2])));
        // accepted: no tunar goes
        p.my_acc = true;
        assert_eq!(p.wager_tunar(10, 120), None);
        // accept and decline take the invitation's id; the name is cut to 23
        let mut q = Part { duel_id: 5, ..Default::default() };
        let long = "A".repeat(30);
        let d = q.decline(0x21, &long);
        let mut want = msg(&[2, 5, 0, 0, 0, 0x21, 0, 0, 0, 23, 0, 0, 0]);
        want.extend(vec![b'A'; 23]);
        assert_eq!(d, vec![Do::Send(want), Do::CloseInvite]);
        q.queue = vec![Invite { id: 0x22, ..Default::default() }, Invite { id: 0x23, ..Default::default() }];
        let d = q.accept(0x22, "Zed");
        assert_eq!(sent(&d), vec![msg(&[3, 5, 0, 0, 0, 0x22, 0, 0, 0]), msg(&[2, 5, 0, 0, 0, 0x23, 0, 0, 0, 3, 0, 0, 0, b'Z', b'e', b'd'])], "accepted, the rest declined");
        assert_eq!(d.last(), Some(&Do::CloseInvite));
        assert!(q.queue.is_empty());
    }

    /// The feature word gates the challenge and the surrender (the control: a
    /// server without bit 0x10, Sandstorm's none); the refusals are the live
    /// words in red, sent nowhere.
    #[test]
    fn a_challenge_needs_the_servers_duel_bit() {
        let off = Part { features: 0x0B, ..Default::default() };
        assert_eq!(off.challenge(0x55, Some(0x100), None), None, "0x1B less 0x10: nothing");
        assert_eq!(off.surrender(), None);
        let on = Part { features: 0x1B, ..Default::default() };
        assert_eq!(on.challenge(0x100, Some(0x100), None), Some(Do::Line("You cannot duel yourself.".into(), RED)));
        assert_eq!(on.challenge(u32::MAX, Some(0x100), None), Some(Do::Line("Invalid target.".into(), RED)));
        assert!(matches!(on.use_msg(MSG_INVITE, 0x55, Some(0x100), None), Some(Do::Send(_))));
        assert!(matches!(on.use_msg(MSG_SURRENDER, 0x55, Some(0x100), None), Some(Do::Send(_))));
        assert!(is_mine(MSG_INVITE) && is_mine(MSG_SURRENDER) && !is_mine(0x0B06_0000));
    }

    /// The wager: 13 copies the inventory and opens the window; 12 both sides'
    /// tunar and accepts; 15 the accepts, both set closing it in mode 44 (the
    /// control: one accept leaves it open); 14 closes it.
    #[test]
    fn the_wager_window_opens_and_closes() {
        let inv = ItemList { p04: 0, items: vec![NetItem { inv_index: 4, quantity: 3, ..Default::default() }] };
        let mut p = Part::default();
        assert_eq!(p.recv(&down(13, 3, &[]), &cx(0, &inv)), vec![Do::OpenWager]);
        assert_eq!(p.inv.len(), 1);
        let mut w = Writer::default();
        w.u32(100).u32(250).u8(0).u8(1);
        p.recv(&down(12, 3, &w.data), &cx(44, &inv));
        assert_eq!((p.my_tunar, p.their_tunar, p.my_acc, p.their_acc), (100, 250, false, true));
        assert!(p.recv(&down(15, 3, &[1, 0, 1]), &cx(44, &inv)).is_empty(), "one accept: still open");
        assert_eq!(p.recv(&down(15, 3, &[1, 1, 1]), &cx(44, &inv)), vec![Do::CloseWager]);
        assert!(p.set);
        assert_eq!(p.recv(&down(14, 3, &[]), &cx(44, &inv)), vec![Do::CloseWager]);
        assert!(p.recv(&down(14, 3, &[]), &cx(0, &inv)).is_empty(), "not up: nothing");
    }

    /// Subs 10 and 11 move the player's staked items between the inventory
    /// copy and the offer (by key, up to four, the copy's record taken down
    /// and gone when emptied) and the opponent's onto and off its offer.
    #[test]
    fn staked_items_move_by_key() {
        let item = |key: u32, id: i64, qty: i64| {
            let mut n = NetItem { inv_index: key, quantity: qty, ..Default::default() };
            n.item.id = id;
            n.item.max_stack = 20;
            n
        };
        let rec = |mine: u8, key: u32, n: Option<u32>, it: &NetItem| {
            let mut w = Writer::default();
            w.u8(0).u8(0).u8(mine).u32(key);
            if let Some(n) = n {
                w.u32(n);
            }
            net_item(&mut w, it);
            w.data
        };
        let inv = ItemList::default();
        let mut p = Part { inv: vec![item(4, 100, 5)], ..Default::default() };
        p.recv(&down(10, 1, &rec(1, 4, Some(2), &item(4, 100, 5))), &cx(44, &inv));
        assert_eq!((p.mine.len(), p.mine[0].quantity, p.inv[0].quantity), (1, 2, 3));
        p.recv(&down(10, 1, &rec(1, 4, Some(3), &item(4, 100, 3))), &cx(44, &inv));
        assert_eq!((p.mine[0].quantity, p.inv.len()), (5, 0), "all of it staked: the copy's record gone");
        p.recv(&down(10, 1, &rec(1, 4, Some(1), &item(4, 100, 1))), &cx(44, &inv));
        assert_eq!(p.mine[0].quantity, 5, "the copy no longer holds it: nothing");
        p.recv(&down(11, 1, &rec(1, 4, None, &item(4, 100, 5))), &cx(44, &inv));
        assert_eq!((p.mine.len(), p.inv.len(), p.inv[0].quantity), (0, 1, 5), "taken back into the copy");
        for k in 0..5 {
            p.recv(&down(10, 1, &rec(0, 10 + k, Some(1), &item(10 + k, 200, 1))), &cx(44, &inv));
        }
        assert_eq!(p.theirs.len(), 4, "four at most");
        p.recv(&down(11, 1, &rec(0, 11, None, &item(11, 200, 1))), &cx(44, &inv));
        assert_eq!(p.theirs.iter().map(|i| i.inv_index).collect::<Vec<_>>(), vec![10, 12, 13]);
    }

    /// The live words stand in where the table lacks the row (the disc's
    /// WORDS.TXT ends at 1,814), never over a word the table has.
    #[test]
    fn the_live_words_fill_the_rows_the_table_lacks() {
        let mut lines = vec!["x"; 1814];
        lines[85] = "Invite";
        let t = StringTable::parse(lines.join("\n").as_bytes()).unwrap();
        assert_eq!(t.get(1864, 0), "(null)", "past the table");
        assert_eq!(word(&t, 1864, 0), "Duel");
        assert_eq!(word(&t, 1872, 0), "Cancel duel?");
        assert_eq!(word(&t, 86, 0), "Invite", "a row the table has is its own");
        let mut more = lines.clone();
        more.extend(vec!["y"; 60]);
        more[1863] = "A server's duel";
        let t2 = StringTable::parse(more.join("\n").as_bytes()).unwrap();
        assert_eq!(word(&t2, 1864, 0), "A server's duel", "a WORDS.TXT that has it");
        assert_eq!(word_of(None, 1866), "%s challenges you to a duel.");
        assert_eq!(printf_s("%s declined your duel invitation.", "Bob"), "Bob declined your duel invitation.");
    }
}
