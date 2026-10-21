//! The guild, as CLIENT keeps it: `VIClientGuild` (VIClient+0x2EC84: the
//! name, the message of the day and the roster, from 0x0468),
//! `VIClientPart2EF24` (VIClient+0x2EF24: the commands, all 0x0578 with a
//! sub-opcode, their `Can*` checks, the invitation from 0x057A and the
//! results in 0x0579) and `VIWndGuild` (the roster window, modes 20 and 21:
//! six columns, eleven rows, the member menu and its confirmation box). A
//! pointer (`pointer`) goes where the keys go: the box's buttons, the menu's
//! rows (off the menu, it closes), a member's row.
//!
//! The invitation box's yes (268) sends sub 9 and its no, cancel or 45-second
//! timeout (260) sub 10 with 1; the busy and already-in-a-guild refusals are
//! sub 10 with 2 and 4. So 9 is the JOIN and 10 the refusal, and the decomp's
//! provisional names for the two senders (`Decline` for 9, `Accept` for 10)
//! are the wrong way round; the behaviour is the game's.

use crate::client_image::Menus;
use crate::dialog::{BoxAnswer, Key, MBox};
use crate::pointer::{Hits, MOVED, key};
use crate::startmenu::KeyFlash;
use crate::wndlist::{ListUp, MiniMenu};
use net::stream::{Reader, Writer};
use std::collections::BTreeMap;
use vi::ui::{Raster, Vui, Window, wide};

/// 1400: every guild command's opcode.
pub const OP: u16 = 0x0578;
/// `gGuildCost`: tunar to found a guild.
pub const COST: i64 = 5000;
/// The guild colour `Chat` lines are added in.
pub const CHAT_COLOR: [f32; 4] = [0.25, 1.0, 0.8, 1.0];

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// `VIGuildMember`: the player record, its rank (0 member .. 3 leader) and
/// whether it is online.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub id: i32,
    pub name: String,
    pub class: i32,
    pub race: i32,
    pub level: i32,
    pub rank: i32,
    pub online: bool,
}

/// What the guild asks of the game's text.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// `ShowMsg(word)`
    Msg(i32),
    /// `ShowText`: a word with arguments, formatted by the game
    Word(i32, Vec<String>),
    /// `AddText(text, colour)`: the guild's chat in its colour
    Chat(String),
}

/// `VIClientGuild`.
#[derive(Clone, Debug, Default)]
pub struct Guild {
    pub in_guild: bool,
    pub name: String,
    pub motd: String,
    /// `m_members`, by id
    pub members: BTreeMap<i32, Member>,
    /// `m_showChat`
    pub show_chat: bool,
}

impl Guild {
    /// `Recv0468`: by its first byte.
    pub fn recv_0468(&mut self, data: &[u8]) -> Option<Out> {
        let mut r = Reader::new(data);
        let op = r.u8();
        Some(match op {
            1 => {
                // ReadInfo
                self.in_guild = true;
                self.name = r.wide(64);
                self.motd = r.wide(256);
                return None;
            }
            2 => {
                self.reset();
                return None;
            }
            3 => {
                let m = Member { id: r.signed() as i32, name: cstr(&r.string(24)), class: r.signed() as i32, race: r.signed() as i32, level: r.signed() as i32, rank: r.signed() as i32, online: r.u8() != 0 };
                self.members.insert(m.id, m);
                return None;
            }
            4 => {
                let id = r.signed() as i32;
                self.members.remove(&id);
                return None;
            }
            6 => Out::Msg(578),
            7 | 8 => Out::Msg(579),
            9 => Out::Msg(580),
            10 => Out::Msg(581),
            11 => Out::Msg(582),
            16 => Out::Msg(1041),
            12 => Out::Msg(583),
            13 => {
                self.show_chat = true;
                Out::Msg(647)
            }
            14 => {
                self.show_chat = false;
                Out::Msg(648)
            }
            5 => {
                // Chat: who it is from, then the line
                let _from = r.u32();
                Out::Chat(r.wide(256))
            }
            15 => {
                // Added: word 717, "%s has added %s to the guild."
                let who = cstr(&r.string(24));
                let whom = cstr(&r.string(24));
                Out::Word(717, vec![who, whom])
            }
            _ => return None,
        })
    }

    /// `Reset`.
    pub fn reset(&mut self) {
        self.in_guild = false;
        self.name.clear();
        self.motd.clear();
        self.members.clear();
    }

    /// `Member(n)`: the n-th in id order.
    pub fn member(&self, n: i32) -> Option<&Member> {
        self.members.values().nth(usize::try_from(n).ok()?)
    }

    pub fn count(&self) -> i32 {
        self.members.len() as i32
    }

    /// `GetRank(id)`: 0 for one not in the roster.
    pub fn rank(&self, id: i32) -> i32 {
        self.members.get(&id).map_or(0, |m| m.rank)
    }
}

/// `VIClientPart2EF24`: the invitation being answered.
#[derive(Clone, Debug)]
pub struct Cmds {
    /// `m_inviter`, -1 for none
    pub inviter: u32,
    pub name: String,
    pub guild: String,
}

impl Default for Cmds {
    fn default() -> Self {
        Cmds { inviter: u32::MAX, name: String::new(), guild: String::new() }
    }
}

/// What a received invitation does (`InviteReceived`).
#[derive(Clone, Debug, PartialEq)]
pub enum Invited {
    /// the game is busy: word 650 with the name, and refused (2)
    Busy(Vec<u8>),
    /// already in a guild: refused (4)
    InGuild(Vec<u8>),
    /// `OpenGuild`: the box
    Show,
}

fn cmd(sub: u8) -> Writer {
    let mut w = Writer::op(OP);
    w.u8(sub);
    w
}

impl Cmds {
    /// `CanInvite`: 3 not in one, 11 full (128), 5 a plain member.
    pub fn can_invite(g: &Guild, me: i32) -> i32 {
        if !g.in_guild {
            3
        } else if g.count() >= 128 {
            11
        } else if g.rank(me) < 1 {
            5
        } else {
            0
        }
    }

    /// `CanCreate`: 4 in one, 6 short of the cost, 7 not leading a full
    /// group.
    pub fn can_create(g: &Guild, tunar: i64, group_full: bool, group_leader: bool) -> i32 {
        if g.in_guild {
            4
        } else if tunar < COST {
            6
        } else if !group_full || !group_leader {
            7
        } else {
            0
        }
    }

    /// `CanDisband` and `CanSetMotd`: the leader only.
    pub fn can_lead(g: &Guild, me: i32) -> i32 {
        if !g.in_guild {
            3
        } else if g.rank(me) != 3 {
            5
        } else {
            0
        }
    }

    /// `CanQuit`, `CanList`, `CanChat`.
    pub fn can_member(g: &Guild) -> i32 {
        if g.in_guild { 0 } else { 3 }
    }

    /// `CanPromote(id)`: two ranks above it, or the leader over anyone below.
    pub fn can_promote(g: &Guild, me: i32, id: i32) -> i32 {
        if !g.in_guild {
            return 3;
        }
        let (mine, theirs) = (g.rank(me), g.rank(id));
        if theirs + 1 < mine || (mine == 3 && theirs < 3) { 0 } else { 5 }
    }

    /// `CanDemote(id)`: 12 a plain member; above it otherwise.
    pub fn can_demote(g: &Guild, me: i32, id: i32) -> i32 {
        if !g.in_guild {
            return 3;
        }
        let (mine, theirs) = (g.rank(me), g.rank(id));
        if theirs == 0 {
            12
        } else if theirs > 0 && theirs < mine {
            0
        } else {
            5
        }
    }

    /// `CanKick(id)`: above it.
    pub fn can_kick(g: &Guild, me: i32, id: i32) -> i32 {
        if !g.in_guild {
            return 3;
        }
        if g.rank(id) < g.rank(me) { 0 } else { 5 }
    }

    /// 1 `Create(name)`.
    pub fn create(name: &str) -> Vec<u8> {
        let mut w = cmd(1);
        w.wide(name);
        w.data
    }

    /// 2 `Disband`, 3 `Quit`.
    pub fn bare(sub: u8) -> Vec<u8> {
        cmd(sub).data
    }

    /// 4 `Promote`, 5 `Demote`, 6 `Kick`: the member's id, zigzagged.
    pub fn member_op(sub: u8, id: i32) -> Vec<u8> {
        let mut w = cmd(sub);
        w.signed(id as i64);
        w.data
    }

    /// 11 `SetMotd`, 12 `Say`.
    pub fn text(sub: u8, s: &str) -> Vec<u8> {
        let mut w = cmd(sub);
        w.wide(s);
        w.data
    }

    /// 8 `Invite(name)`: the name and the guild's; inviting oneself is word
    /// 679 in red instead.
    pub fn invite(me: &str, name: &str, guild: &Guild) -> Result<Vec<u8>, i32> {
        if me.eq_ignore_ascii_case(name) {
            return Err(679);
        }
        let mut w = cmd(8);
        w.string(name.as_bytes()).wide(&guild.name);
        Ok(w.data)
    }

    /// 13 `SetChat(on)`.
    pub fn set_chat(on: bool) -> Vec<u8> {
        let mut w = cmd(13);
        w.u8(on as u8);
        w.data
    }

    /// 9 (the decomp's `Decline`): join, with the inviter's id; the
    /// invitation is gone.
    pub fn join(&mut self) -> Vec<u8> {
        let mut w = cmd(9);
        w.u32(self.inviter);
        self.inviter = u32::MAX;
        w.data
    }

    /// 10 (the decomp's `Accept`): refuse, with the inviter's id and why (1
    /// declined, 2 busy, 4 in a guild).
    pub fn refuse(&mut self, how: u8) -> Vec<u8> {
        let mut w = cmd(10);
        w.u32(self.inviter).u8(how);
        self.inviter = u32::MAX;
        w.data
    }

    /// `Recv057A`'s 1, `InviteReceived`: `idle` is `IsLootable` (the game
    /// in mode 0).
    pub fn invite_received(&mut self, r: &mut Reader, guild: &Guild, idle: bool) -> Invited {
        let _ = r.signed();
        self.inviter = r.u32();
        self.name = cstr(&r.string(24));
        self.guild = r.wide(64);
        if !idle {
            Invited::Busy(self.refuse(2))
        } else if guild.in_guild {
            Invited::InGuild(self.refuse(4))
        } else {
            Invited::Show
        }
    }

    /// `Recv057A`: 1 an invitation, 2 one accepted (word 557 with the
    /// name), 3 one declined (by its reason).
    pub fn recv_057a(&mut self, data: &[u8], guild: &Guild, idle: bool) -> (Option<Invited>, Option<Out>) {
        let mut r = Reader::new(data);
        match r.u8() {
            1 => (Some(self.invite_received(&mut r, guild, idle)), None),
            2 => {
                let _id = r.signed();
                let name = cstr(&r.string(24));
                (None, Some(Out::Word(557, vec![name])))
            }
            3 => {
                let why = r.u8();
                let name = cstr(&r.string(24));
                let word = match why {
                    2 => 559,
                    3 => 560,
                    4 => 576,
                    5 => 587,
                    6 => 589,
                    7 => 642,
                    _ => 558,
                };
                (None, Some(Out::Word(word, vec![name])))
            }
            _ => (None, None),
        }
    }

    /// `Recv0579`: an operation and its result (0 it worked).
    pub fn recv_0579(data: &[u8]) -> Vec<Out> {
        let mut r = Reader::new(data);
        let (op, result) = (r.u8(), r.u8());
        let mut out = Vec::new();
        if result == 0 {
            return out;
        }
        match op {
            1 => {
                out.push(Out::Msg(563));
                match result {
                    1 => out.push(Out::Msg(565)),
                    2 => out.push(Out::Msg(566)),
                    6 => out.push(Out::Word(567, vec![COST.to_string()])),
                    7 => out.push(Out::Msg(568)),
                    _ => {}
                }
            }
            2 => {
                out.push(Out::Msg(564));
                match result {
                    5 => out.push(Out::Msg(546)),
                    8 => out.push(Out::Msg(570)),
                    _ => {}
                }
            }
            3..=7 | 11 | 12 => match result {
                8 => out.push(Out::Msg(570)),
                5 => out.push(Out::Msg(546)),
                11 => out.push(Out::Msg(584)),
                12 => out.push(Out::Msg(745)),
                _ => {}
            },
            _ => {}
        }
        out
    }
}

/// `GetRace(id)->m_shortStrID` and `GetClass(id)->m_shortStrID`.
fn race_short(race: i32) -> Option<i32> {
    [317, 318, 319, 320, 321, 322, 323, 324, 325, 1191].get(usize::try_from(race).ok()?).copied()
}

fn class_short(class: i32) -> Option<i32> {
    [303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 1190].get(usize::try_from(class).ok()?).copied()
}

/// The rank names (`0x00CE9530`), indexed by rank (unclamped there).
const RANK_WORDS: [i32; 4] = [541, 540, 539, 538];

/// The member menu (`sMemberMenu`): (word, message, id).
const MEMBER_MENU: [(i32, u32, i32); 4] = [(522, 0x0B05_1002, 0), (526, 0x0B05_1003, 0), (527, 0x0B05_1004, 0), (516, 0x0B05_1005, 0)];

/// What the roster window asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Up {
    /// Escape: `HideInvite` (`CloseGuildMenu`, back to the start menu)
    Hide,
    /// 305: the edit box holding "/t name "
    Tell(String),
    /// a member operation confirmed (4 promote, 5 demote, 6 kick) with the
    /// member's id
    Member(u8, i32),
    /// the invitation box: its yes joins (9), its no refuses (10, 1)
    Join,
    Refuse,
    /// a text to the chat (`ShowText(word)`) with the window kept
    Text(i32),
    /// "not in a guild": sound 8, word 570, `HideInvite`
    NotInGuild,
}

/// `VIWndGuild`.
pub struct WndGuild {
    /// `m_mode`: 1 the invitation, 2 the roster
    pub mode: i32,
    sel: i32,
    top: i32,
    count: i32,
    focus: bool,
    time: i64,
    fade_in: i32,
    alpha: f32,
    window: Window,
    pub mbox: MBox,
    pub mbox_visible: bool,
    pub menu: MiniMenu,
    flash: KeyFlash,
    /// the members' rows drawn, by index (for a pointer)
    hits: Hits,
}

impl Default for WndGuild {
    fn default() -> Self {
        let mut menu = MiniMenu::fixed(&MEMBER_MENU);
        menu.kind = 1;
        let mut mbox = MBox::default();
        mbox.pos = [80, 115];
        WndGuild { mode: 1, sel: 0, top: 0, count: 0, focus: true, time: 0, fade_in: 1, alpha: 1.0, window: Window::default(), mbox, mbox_visible: false, menu, flash: KeyFlash::default(), hits: Hits::default() }
    }
}

fn box_key(key: i32) -> Option<Key> {
    Some(match key {
        13 | 134 => Key::Enter,
        27 | 132 => Key::Escape,
        17 | 136 => Key::Left,
        18 | 137 => Key::Right,
        _ => return None,
    })
}

impl WndGuild {
    /// A pointer at frame point `p` (`pointer`), to the child with the
    /// focus: the box's buttons (the invitation's too), the member menu's
    /// rows, or off the menu the menu closed (△) and the row under the
    /// pointer selected; in the roster a member selected, or the
    /// selection's menu opened (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.mbox_visible {
            return self.mbox.point(p).then_some(key::CROSS).into_iter().collect();
        }
        if self.menu.visible {
            if let Some(k) = self.menu.point(p) {
                return k.into_iter().collect();
            }
            let mut keys = Vec::new();
            if let Some(row) = self.hits.at(p).filter(|&row| row != self.sel) {
                self.select(row);
                keys.push(MOVED);
            }
            keys.push(key::TRIANGLE);
            return keys;
        }
        if self.mode != 2 {
            return Vec::new();
        }
        match self.hits.at(p) {
            Some(row) if row == self.sel => vec![key::CROSS],
            Some(row) => {
                self.select(row);
                vec![MOVED]
            }
            None => Vec::new(),
        }
    }

    /// The selection on `row` as `Move` leaves it: the marker's blink
    /// started over.
    fn select(&mut self, row: i32) {
        self.sel = row;
        self.fade_in = 1;
        self.alpha = 1.0;
    }
    /// 295 on gaining the focus: the window at (320, 180), 600 wide.
    fn gain_focus(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.focus = true;
        self.fade_in = 1;
        self.alpha = 1.0;
        self.time = now;
        self.flash.reset();
        self.window.init(vui, r, [320, 180], 600, 14, 0);
        self.window.open(true);
    }

    /// `ShowInvite` (`UpdateMode` 20): the box with words 535, the name,
    /// the guild and 520, 45 seconds to answer.
    pub fn show_invite(&mut self, words: &dyn Fn(i32) -> String, cmds: &Cmds, now: i64) {
        let text = format!("{}\n{}\n{}\n{}", words(535), cmds.name, cmds.guild, words(520));
        self.mode = 1;
        self.menu.visible = false;
        self.mbox.prompt(&wide(&text), 1, 1, 0, now);
        self.mbox.timeout = now + 45000;
        self.mbox_visible = true;
    }

    /// `Close` (`UpdateMode` 21): the roster, from the top, with the focus.
    pub fn close(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.mode = 2;
        self.mbox_visible = false;
        self.menu.visible = false;
        self.sel = 0;
        self.top = 0;
        self.gain_focus(vui, r, now);
    }

    /// `Reset`.
    pub fn reset(&mut self) {
        self.mbox_visible = false;
        self.menu.visible = false;
        self.mode = 1;
    }

    /// The box's answer (268 yes / 260 no): p1 1 the invitation, else a
    /// member operation (p1 its message, p2 the member).
    fn answered(&mut self, a: BoxAnswer, g: &Guild) -> Option<Up> {
        self.mbox_visible = false;
        match a {
            BoxAnswer::Yes(1, _) => Some(Up::Join),
            BoxAnswer::No(1, _) => Some(Up::Refuse),
            BoxAnswer::Yes(p1, _) => {
                let id = g.member(self.sel).map_or(-1, |m| m.id);
                let sub = match p1 as u32 {
                    0x0B05_1003 => 4,
                    0x0B05_1004 => 5,
                    0x0B05_1005 => 6,
                    _ => return None,
                };
                Some(Up::Member(sub, id))
            }
            BoxAnswer::No(..) => None,
        }
    }

    /// The member menu's choice (261): Tell, or a check and its box.
    fn chose(&mut self, msg: u32, g: &Guild, me: i32, words: &dyn Fn(i32) -> String, now: i64, cues: &mut Vec<i32>) -> Option<Up> {
        self.menu.visible = false;
        cues.push(13);
        let id = g.member(self.sel).map_or(-1, |m| m.id);
        let (err, word) = match msg {
            0x0B05_1002 => return Some(Up::Tell(g.member(self.sel).map(|m| m.name.clone()).unwrap_or_default())),
            0x0B05_1003 => (Cmds::can_promote(g, me, id), 544),
            0x0B05_1004 => (Cmds::can_demote(g, me, id), 545),
            0x0B05_1005 => (Cmds::can_kick(g, me, id), 536),
            _ => return None,
        };
        match err {
            0 => {
                self.mbox.prompt(&wide(&words(word)), 1, msg as i32, id, now);
                self.mbox_visible = true;
                None
            }
            3 => Some(Up::Text(570)),
            5 => Some(Up::Text(546)),
            12 => Some(Up::Text(745)),
            // VIASSERTF: unexpected
            _ => None,
        }
    }

    /// `OnKeyDown`/`OnKeyUp`, the focused child's while one is up.
    #[allow(clippy::too_many_arguments)]
    pub fn key(&mut self, menus: &Menus, g: &Guild, me: i32, words: &dyn Fn(i32) -> String, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<Up> {
        if self.mbox_visible {
            if !down {
                return None;
            }
            let a = self.mbox.key(box_key(key)?, cues)?;
            return self.answered(a, g);
        }
        if self.menu.visible {
            match self.menu.key(menus, key, down, cues) {
                ListUp::Action(msg, _) => return self.chose(msg, g, me, words, now, cues),
                ListUp::Back => self.menu.visible = false,
                _ => {}
            }
            return None;
        }
        if self.mode != 2 {
            return None;
        }
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return None;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                return Some(Up::Hide);
            }
            13 | 134 => {
                if let Some(m) = g.member(self.sel).filter(|_| self.sel < g.count()) {
                    cues.push(13);
                    // ShowMemberMenu(name)
                    self.menu.list.pos = [170, 50];
                    self.menu.title = wide(&m.name);
                    self.menu.reset_scroll();
                    self.menu.visible = true;
                    self.menu.focus(now);
                } else {
                    cues.push(10);
                }
            }
            19 | 138 => self.flash.press(138),
            20 | 139 => self.flash.press(139),
            _ => {}
        }
        None
    }

    /// `Move(d)`.
    fn step(&mut self, d: i32, n: i32, now: i64, cues: &mut Vec<i32>) {
        if self.sel + d >= n && n > 0 {
            cues.push(9);
            self.sel = n - 1;
        } else if self.sel + d < 0 || n == 0 {
            cues.push(9);
            self.sel = 0;
        } else {
            self.sel += d;
            cues.push(11);
        }
        if self.top + 10 < self.sel {
            self.top = self.sel - 10;
        }
        if self.sel < self.top {
            self.top = self.sel;
        }
        self.fade_in = 1;
        self.alpha = 1.0;
        self.time = now;
    }

    /// `Clamp`.
    fn clamp(&mut self, n: i32) {
        if n - 11 < self.top && n > 11 {
            self.top -= 1;
        } else if n == 11 {
            self.top = 0;
        }
        if self.top < 0 {
            self.top = 0;
        }
        if self.sel >= n {
            self.sel -= 1;
        }
        if self.sel < 0 {
            self.sel = 0;
        }
        self.count = n;
    }

    /// `OnDrawSelf` in the roster, then the children (the box, its timeout
    /// answered here, and the menu).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, g: &Guild, now: i64, cues: &mut Vec<i32>) -> Option<Up> {
        let mut up = None;
        self.hits.clear();
        if self.mode == 2 {
            up = self.draw_roster(vui, r, g, now, cues);
        }
        if self.mbox_visible
            && let Some(a) = self.mbox.draw(vui, r, now)
        {
            up = self.answered(a, g).or(up);
        }
        if self.menu.visible {
            self.menu.draw(vui, r, menus, now, cues);
        }
        up
    }

    fn draw_roster(&mut self, vui: &Vui, r: &mut dyn Raster, g: &Guild, now: i64, cues: &mut Vec<i32>) -> Option<Up> {
        if !g.in_guild {
            cues.push(8);
            return Some(Up::NotInGuild);
        }
        if self.flash.pulse(138) {
            self.step(-1, g.count(), now, cues);
        }
        if self.flash.pulse(139) {
            self.step(1, g.count(), now, cues);
        }
        self.clamp(g.count());
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let hl = [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let line_h = vui.font_height(r) - 4;
        crate::ui::window_backdrop(vui, r, 642);
        self.window.set_title(Some(&wide(&g.name)));
        self.window.draw(vui, r);
        let mut legend = wide("|x");
        legend.extend(word(212));
        legend.extend(wide("   |t"));
        legend.extend(word(161));
        self.window.text(vui, r, &legend, 20, 13, black, 0);
        let count = g.count();
        let t = if count > 11 { self.top as f32 / (count - 11) as f32 } else { 0.0 };
        let width = self.window.size[0];
        self.window.draw_frame_vscroll(vui, r, 20, 1, width - 50, 11, t);
        let mut y = self.window.row_center(vui, r, 0)[1];
        let x = [65, 225, 325, 395, 465, 530];
        let cell = |r: &mut dyn Raster, s: &[u16], x: i32, y: i32, c: [f32; 4]| r.draw_centered_string([x, y], c, s, 1);
        for (id, x) in [509, 537, 510, 511, 503, 521].into_iter().zip(x) {
            cell(r, &word(id), x, y, black);
        }
        y += line_h;
        for i in 0..11 {
            let n = i + self.top;
            let c = if n == self.sel {
                let e = vui.entry(85);
                vui.blit_ex(r, e.x00, [x[0] - 25, y - e.uv1[1] / 2], 3, self.alpha, 0);
                hl
            } else {
                black
            };
            let Some(m) = g.member(n) else { break };
            // the row, from the marker to the last column's end
            self.hits.push([x[0] - 27, y - line_h / 2, x[5] + 60, y + line_h - line_h / 2], n);
            // a name past eleven letters ends "..."
            let mut name: Vec<u8> = m.name.bytes().take(63).collect();
            if name.len() > 11 {
                name.truncate(11);
                name.extend(b"...");
            }
            cell(r, &wide(&String::from_utf8_lossy(&name)), x[0], y, c);
            let rank = RANK_WORDS.get(m.rank.clamp(0, 3) as usize).copied().unwrap_or(541);
            cell(r, &word(rank), x[1], y, c);
            let Some(race) = race_short(m.race) else { continue };
            cell(r, &word(race), x[2], y, c);
            let Some(class) = class_short(m.class) else { continue };
            cell(r, &word(class), x[3], y, c);
            cell(r, &wide(&(m.level as u32).to_string()), x[4], y, c);
            cell(r, &word(if m.online { 21 } else { 22 }), x[5], y, c);
            y += line_h;
        }
        if self.focus {
            if self.fade_in == -1 {
                self.alpha = (self.alpha - dt * 2.0).max(0.0);
                if self.alpha == 0.0 {
                    self.fade_in = 1;
                }
            } else {
                self.alpha = (self.alpha + dt * 3.0).min(1.0);
                if self.alpha == 1.0 {
                    self.fade_in = -1;
                }
            }
        } else {
            self.alpha = 1.0;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: i64, name: &str, rank: i64, online: bool) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(3).signed(id).string(name.as_bytes()).signed(6).signed(0).signed(12).signed(rank).u8(online as u8);
        w.data
    }

    /// 0x0468: 1 joins (name and motd), 3 adds or replaces by id, the roster
    /// is in id order, 4 removes, 2 leaves and clears; the words and the
    /// chat flag; 5 is a line in the guild colour; 15 is word 717.
    #[test]
    fn the_roster_follows_0468() {
        let mut g = Guild::default();
        let mut info = Writer::default();
        info.u8(1).wide("Knights").wide("Be nice");
        assert_eq!(g.recv_0468(&info.data), None);
        assert!(g.in_guild && g.name == "Knights" && g.motd == "Be nice");
        g.recv_0468(&member(40, "Zed", 0, false));
        g.recv_0468(&member(7, "Amy", 3, true));
        g.recv_0468(&member(40, "Zed", 1, true));
        assert_eq!(g.count(), 2);
        assert_eq!(g.member(0).unwrap().name, "Amy");
        assert_eq!((g.member(1).unwrap().rank, g.member(1).unwrap().online), (1, true));
        assert_eq!(g.member(2), None);
        assert_eq!(g.member(-1), None);
        assert_eq!((g.rank(7), g.rank(40), g.rank(99)), (3, 1, 0));
        g.recv_0468(&[4, 14]);
        assert_eq!(g.count(), 1);
        assert_eq!(g.recv_0468(&[7]), Some(Out::Msg(579)));
        assert_eq!(g.recv_0468(&[8]), Some(Out::Msg(579)));
        assert_eq!(g.recv_0468(&[16]), Some(Out::Msg(1041)));
        assert_eq!(g.recv_0468(&[13]), Some(Out::Msg(647)));
        assert!(g.show_chat);
        g.recv_0468(&[14]);
        assert!(!g.show_chat);
        let mut chat = Writer::default();
        chat.u8(5).u32(7).wide("hello");
        assert_eq!(g.recv_0468(&chat.data), Some(Out::Chat("hello".into())));
        let mut added = Writer::default();
        added.u8(15).string(b"Amy").string(b"Bob");
        assert_eq!(g.recv_0468(&added.data), Some(Out::Word(717, vec!["Amy".into(), "Bob".into()])));
        assert_eq!(g.recv_0468(&[2]), None);
        assert!(!g.in_guild && g.count() == 0 && g.name.is_empty());
        assert_eq!(g.recv_0468(&[0x63]), None, "an unknown sub is ignored");
    }

    /// The checks: promote needs two ranks above (or the leader), demote
    /// refuses a plain member (12), kick needs to be above; a negative
    /// control for each, and 3 out of a guild.
    #[test]
    fn the_rank_checks() {
        let mut g = Guild::default();
        assert_eq!(Cmds::can_promote(&g, 1, 2), 3);
        g.in_guild = true;
        for (id, rank) in [(1, 3), (2, 0), (3, 1), (4, 2)] {
            g.members.insert(id, Member { id, name: String::new(), class: 0, race: 0, level: 1, rank, online: true });
        }
        assert_eq!(Cmds::can_promote(&g, 1, 4), 0, "the leader promotes an officer");
        assert_eq!(Cmds::can_promote(&g, 4, 2), 0, "two ranks above");
        assert_eq!(Cmds::can_promote(&g, 4, 3), 5, "one rank above is not enough");
        assert_eq!(Cmds::can_demote(&g, 1, 2), 12);
        assert_eq!(Cmds::can_demote(&g, 4, 3), 0);
        assert_eq!(Cmds::can_demote(&g, 3, 4), 5);
        assert_eq!(Cmds::can_kick(&g, 3, 2), 0);
        assert_eq!(Cmds::can_kick(&g, 2, 3), 5);
        assert_eq!(Cmds::can_invite(&g, 2), 5);
        assert_eq!(Cmds::can_invite(&g, 3), 0);
        assert_eq!(Cmds::can_lead(&g, 4), 5);
        assert_eq!(Cmds::can_lead(&g, 1), 0);
        assert_eq!(Cmds::can_create(&g, 9999, true, true), 4);
        g.in_guild = false;
        assert_eq!(Cmds::can_create(&g, 4999, true, true), 6);
        assert_eq!(Cmds::can_create(&g, 5000, true, false), 7);
        assert_eq!(Cmds::can_create(&g, 5000, true, true), 0);
    }

    /// The commands on the wire: 0x0578, the sub-opcode, then the operands;
    /// an invitation busy, in a guild, or shown; Accept and Decline take the
    /// inviter and forget it.
    #[test]
    fn the_commands_on_the_wire() {
        assert_eq!(Cmds::bare(3), vec![0x78, 0x05, 3]);
        assert_eq!(Cmds::member_op(6, 40), vec![0x78, 0x05, 6, 80]);
        assert_eq!(Cmds::set_chat(true), vec![0x78, 0x05, 13, 1]);
        assert_eq!(Cmds::text(12, "hi"), vec![0x78, 0x05, 12, 2, 0, 0, 0, b'h', 0, b'i', 0]);
        let g = Guild { name: "K".into(), ..Default::default() };
        assert_eq!(Cmds::invite("Me", "me", &g), Err(679));
        assert_eq!(Cmds::invite("Me", "Bo", &g), Ok(vec![0x78, 0x05, 8, 2, 0, 0, 0, b'B', b'o', 1, 0, 0, 0, b'K', 0]));
        let mut inv = Writer::default();
        inv.u8(1).signed(0).u32(0x1234).string(b"Kim").wide("Knights");
        let mut c = Cmds::default();
        assert_eq!(c.recv_057a(&inv.data, &g, true).0, Some(Invited::Show));
        assert_eq!((c.inviter, c.name.as_str(), c.guild.as_str()), (0x1234, "Kim", "Knights"));
        assert_eq!(c.join(), vec![0x78, 0x05, 9, 0x34, 0x12, 0, 0]);
        assert_eq!(c.inviter, u32::MAX);
        assert_eq!(c.recv_057a(&inv.data, &g, false).0, Some(Invited::Busy(vec![0x78, 0x05, 10, 0x34, 0x12, 0, 0, 2])));
        let member = Guild { in_guild: true, ..Default::default() };
        assert_eq!(c.recv_057a(&inv.data, &member, true).0, Some(Invited::InGuild(vec![0x78, 0x05, 10, 0x34, 0x12, 0, 0, 4])));
        let mut dec = Writer::default();
        dec.u8(3).u8(9).string(b"Kim");
        assert_eq!(c.recv_057a(&dec.data, &g, true).1, Some(Out::Word(558, vec!["Kim".into()])), "an unknown reason is 558");
        assert_eq!(Cmds::recv_0579(&[1, 0]), vec![]);
        assert_eq!(Cmds::recv_0579(&[1, 6]), vec![Out::Msg(563), Out::Word(567, vec!["5000".into()])]);
        assert_eq!(Cmds::recv_0579(&[2, 8]), vec![Out::Msg(564), Out::Msg(570)]);
        assert_eq!(Cmds::recv_0579(&[6, 11]), vec![Out::Msg(584)]);
        assert_eq!(Cmds::recv_0579(&[9, 5]), vec![], "an op with no case says nothing");
    }

    /// The invitation box: yes joins (9), no and the timeout refuse (10);
    /// a member box's yes sends the operation, its no nothing.
    #[test]
    fn the_invitation_box_answers() {
        let mut w = WndGuild::default();
        let g = Guild::default();
        let c = Cmds { name: "Kim".into(), guild: "K".into(), ..Default::default() };
        w.show_invite(&|_| String::new(), &c, 0);
        assert_eq!(w.mbox.timeout, 45000);
        assert_eq!(w.answered(BoxAnswer::Yes(1, 0), &g), Some(Up::Join));
        assert_eq!(w.answered(BoxAnswer::No(1, 0), &g), Some(Up::Refuse));
        assert_eq!(w.answered(BoxAnswer::Yes(0x0B05_1005, 7), &g), Some(Up::Member(6, -1)));
        assert_eq!(w.answered(BoxAnswer::No(0x0B05_1005, 7), &g), None);
    }

    /// A pointer on the roster: a member selected, then the member menu
    /// (✕); its rows used at once; off it closed (△) with the row under the
    /// pointer selected; the invitation's box takes its buttons and the
    /// roster is not there.
    #[test]
    fn a_pointer_on_the_roster() {
        use crate::pointer::testing::{NullRaster, vui};
        use crate::pointer::{MOVED, key};
        let vui = vui();
        let mut r = NullRaster;
        let menus = Menus::default();
        let mut g = Guild { in_guild: true, name: "K".into(), ..Default::default() };
        for id in 1..=4 {
            g.members.insert(id, Member { id, name: format!("M{id}"), class: 0, race: 0, level: 1, rank: 0, online: true });
        }
        let mut w = WndGuild::default();
        w.close(&vui, &r, 0);
        let mut cues = Vec::new();
        w.draw(&vui, &mut r, &menus, &g, 0, &mut cues);
        // member i is drawn on the window's row i + 1, columns from x 65
        let at = |w: &WndGuild, i: i32, x: f32| [x, w.window.row_center(&vui, &NullRaster, i + 1)[1] as f32];
        assert_eq!(w.point(at(&w, 3, 300.0)), vec![MOVED]);
        assert_eq!(w.sel, 3);
        assert_eq!(w.point(at(&w, 3, 500.0)), vec![key::CROSS], "any column of the selection");
        assert!(w.point(at(&w, 4, 300.0)).is_empty(), "past the roster");
        assert!(w.point(at(&w, -1, 300.0)).is_empty(), "the headers");
        // the member menu
        w.key(&menus, &g, 1, &|_| String::new(), 134, true, 0, &mut cues);
        assert!(w.menu.visible);
        w.draw(&vui, &mut r, &menus, &g, 16, &mut cues);
        let m = w.menu.list.pos;
        assert_eq!(w.point([(m[0] + 60) as f32, (m[1] + 48 + 2 * 19) as f32]), vec![key::CROSS]);
        assert_eq!(w.menu.list.sel, 2, "Demote");
        assert_eq!(w.point(at(&w, 0, 60.0)), vec![MOVED, key::TRIANGLE]);
        assert_eq!(w.sel, 0);
        // the invitation (mode 1): the box's buttons, no roster
        let c = Cmds { name: "Kim".into(), guild: "K".into(), ..Default::default() };
        w.show_invite(&|_| String::new(), &c, 0);
        w.draw(&vui, &mut r, &menus, &g, 32, &mut cues);
        assert!(w.point([630.0, 440.0]).is_empty());
        let found = (0..112).flat_map(|y| (0..160).map(move |x| [x as f32 * 4.0, y as f32 * 4.0])).any(|p| w.point(p) == vec![key::CROSS]);
        assert!(found, "the box's buttons");
        w.mbox_visible = false;
        assert!(w.point(at(&w, 1, 300.0)).is_empty(), "mode 1 has no roster to pick from");
    }

}
