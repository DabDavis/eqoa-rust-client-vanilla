//! The quick-chat menus L2 opens (`VIWndGame` mode 30; the keyboard's Z):
//! `VIWndMenu4` (CLIENT viwndmenu4.cpp), one `VITextSet`'s four choices on
//! the d-pad's four directions, and `VIWndMenu4Edit` (viwndmenu4edit.cpp),
//! the editor that holding a direction opens on its choice, with an edit box
//! of its own (`VIWndEdit`).
//!
//! A tap runs the choice: a line (`|<n>` is quick command n, anything else
//! is said, `VIClientText::Expand`'s `%1`..`%4` filled in) and the menus
//! close; or a branch, whose set opens in a menu of its own over this one,
//! which dims (three deep at most). Held for 16 of the game's frames, the
//! direction opens the editor: rename the choice, make it a line (a text,
//! or one of the 34 commands from a list that glides a row at a time) or a
//! branch, or back; Done sends the change (0x006C the choice, 0x006A a new
//! branch's empty set, 0x006B each set a branch made a line drops) for the
//! server to keep for the next world entry. The tree is `vi::textset`'s,
//! held to the decomp by its oracle test.
//!
//! As in the game the menu windows form a chain (each has at most one menu
//! open over it and one editor), and the keys go to the one at its end: a
//! menu passes what it does not take to the menu under it, the root to the
//! game window (`Out::Key`). What the game does differently here: a set the
//! dump did not give (no root) reads as an empty one, where the game reads
//! through a null pointer; and a choice with both a line and a branch runs
//! the line and closes, where the game then also leaves the hidden branch
//! menu with the keys.

use crate::editbox::{EditBox, EditUp};
use crate::pointer::{Hits, key, rows_rect};
use vi::textset::{Record, TextPair, TextSets, child_id, make_id};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VIWndMenu4::OnKeyUp`'s quick commands: `|n` is entry n, the message the
/// game window acts on (`DoAction`) and its argument.
pub const COMMANDS: [(u32, i32); 34] = [
    (0x0601_0000, 0),
    (0x0602_0000, 0),
    (0x0603_0000, 0),
    (0x0604_0000, 0),
    (0x0701_0000, 0),
    (0x0704_0000, 0),
    (0x0706_0000, 0),
    (0x0B05_3001, 0),
    (0x0B05_3002, 0),
    (0x0B05_3000, 0),
    (0x0B03_0003, 0),
    (0x0B03_0003, 1),
    (0x0B05_100C, 0),
    (0x0B05_100C, 1),
    (0x0B01_0000, 0),
    (0x0B03_0000, 0),
    (0x0B03_0001, 0),
    (0x0B03_0002, 0),
    (0x0B0D_0001, 0),
    (0x0B0D_0000, 0),
    (0x0301_0000, 0),
    (0x0B09_0000, 0),
    (0x0B0E_0006, 0),
    (0x0B0E_0003, 0),
    (0x050A_0000, 0),
    (0x050B_0000, 0),
    (0x050D_0000, 0),
    (0x050C_0000, 0),
    (0x050E_0000, 0),
    (0x050F_0000, 0),
    (0x050F_1000, 0),
    (0x0B0E_0007, 0),
    (0x0B0E_0008, 0),
    (0x0B0E_0009, 0),
];

/// `sCommandWords`: each command's name in WORDS.TXT (Wave, Bow, ... Alarm).
const COMMAND_WORDS: [i32; 34] = [41, 991, 992, 31, 86, 993, 994, 995, 996, 997, 998, 999, 1008, 1009, 106, 107, 108, 512, 534, 425, 522, 1000, 1010, 737, 1001, 1002, 1003, 1004, 1005, 1006, 1007, 1194, 1197, 1198];

/// What the menus ask of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// `ChatLine`, once `VIClientText::Expand` has filled in its `%` codes
    Say(Vec<u16>),
    /// 258 to `VIWndGame`: the menus close (mode 0), then `DoAction(msg,
    /// arg)` unless msg is -1
    Done(i32, i32),
    /// 0x006A, 0x006B or 0x006C for the server
    Send(Vec<u8>),
    /// a key the menus pass up to the game window (down, or let go)
    Key(i32, bool),
}

/// The direction a key is (0 up, 1 left, 2 right, 3 down): the game's
/// cursor keys and the d-pad's.
fn direction(key: i32) -> Option<i32> {
    match key {
        19 | 138 => Some(0),
        17 | 136 => Some(1),
        18 | 137 => Some(2),
        20 | 139 => Some(3),
        _ => None,
    }
}

/// A direction's d-pad key.
fn direction_key(dir: i32) -> i32 {
    [key::UP, key::LEFT, key::RIGHT, key::DOWN][dir as usize]
}

/// `sscanf(s, "|%d", &n)`: the number after a '|' (white space, then a
/// sign, skipped), or none.
fn command_number(s: &[u16]) -> Option<i32> {
    if s.first() != Some(&(b'|' as u16)) {
        return None;
    }
    let mut i = 1;
    while s.get(i).is_some_and(|&c| matches!(c, 9..=13 | 32)) {
        i += 1;
    }
    let neg = match s.get(i).copied() {
        Some(c) if c == b'-' as u16 => {
            i += 1;
            true
        }
        Some(c) if c == b'+' as u16 => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut n: i32 = 0;
    while let Some(&c) = s.get(i)
        && (b'0' as u16..=b'9' as u16).contains(&c)
    {
        n = n.wrapping_mul(10).wrapping_add((c - b'0' as u16) as i32);
        i += 1;
    }
    (i > start).then_some(if neg { n.wrapping_neg() } else { n })
}

/// `strcpy16(dst, bytes, s)` into a fixed field: at most `chars - 1`.
fn field(s: Option<&[u16]>, chars: usize) -> Vec<u16> {
    s.map_or_else(Vec::new, |s| s.iter().copied().take_while(|&c| c != 0).take(chars - 1).collect())
}

/// The wide `WriteString`: a u32 character count, then UTF-16LE.
fn put_wide(w: &mut net::stream::Writer, s: &[u16]) {
    w.u32(s.len() as u32);
    for c in s {
        w.bytes(&c.to_le_bytes());
    }
}

/// `Send006A`: a set and its four records.
pub fn send_006a(id: i32, recs: &[Record; 4]) -> Vec<u8> {
    let mut w = net::stream::Writer::op(0x6A);
    w.signed(id as i64);
    for r in recs {
        w.signed(r.slot as i64);
        put_wide(&mut w, &r.text);
        put_wide(&mut w, &r.label);
    }
    w.data
}

/// `Send006B`: a set dropped.
pub fn send_006b(id: i32) -> Vec<u8> {
    let mut w = net::stream::Writer::op(0x6B);
    w.signed(id as i64);
    w.data
}

/// `Send006C`: one of set `id`'s records.
pub fn send_006c(id: i32, r: &Record) -> Vec<u8> {
    let mut w = net::stream::Writer::op(0x6C);
    w.signed(id as i64).signed(r.slot as i64);
    put_wide(&mut w, &r.text);
    put_wide(&mut w, &r.label);
    w.data
}

/// `VIClient::SetTextSet(id, l0, t0, .. l3, t3)`: a set from narrow strings,
/// each optional, sent (the root as four 0x006C, any other set as one
/// 0x006A) and put in the table.
fn set_text_set(sets: &mut TextSets, id: i32, pairs: [(Option<&str>, Option<&str>); 4], out: &mut Vec<Vec<u8>>) {
    let recs: [Record; 4] = std::array::from_fn(|k| Record {
        slot: k as i32,
        text: pairs[k].1.map_or_else(Vec::new, |t| field(Some(&wide(t)), 128)),
        label: pairs[k].0.map_or_else(Vec::new, |l| field(Some(&wide(l)), 64)),
    });
    if id == 0 {
        out.extend(recs.iter().map(|r| send_006c(0, r)));
    } else {
        out.push(send_006a(id, &recs));
    }
    if let Some(s) = sets.get(id) {
        for (k, (l, t)) in pairs.iter().enumerate() {
            if let Some(l) = l {
                s.pairs[k].label = Some(wide(l));
            }
            if let Some(t) = t {
                s.pairs[k].text = Some(wide(t));
            }
        }
    }
}

/// `VIClient::DefaultTextSets` ("Def. Menu4", 0x0B080000): every set out of
/// the table (each but the root's dropped, 0x006B), then the built-in tree,
/// sent as it is made. Its commands are numbered as shipped: Passive is
/// `|22`, which the command list names Assist Target.
pub fn default_text_sets(sets: &mut TextSets) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for i in 0..256 {
        if sets.set(i).is_some() {
            if i > 0 {
                out.push(send_006b(i));
            }
            sets.remove(i as u8);
        }
    }
    type P<'a> = (Option<&'a str>, Option<&'a str>);
    let l = |s: &'static str| -> P<'static> { (Some(s), None) };
    let lt = |a: &'static str, b: &'static str| -> P<'static> { (Some(a), Some(b)) };
    let none: P<'static> = (None, None);
    let tree: [(i32, [P<'static>; 4]); 20] = [
        (0, [l("Adventure"), l("Social"), l("Commands"), l("Custom")]),
        (make_id(0, -1, -1), [l("Needs"), l("Orders"), l("Pets"), l("Questions")]),
        (make_id(0, 0, -1), [lt("Help", "I need help!"), lt("Healing", "I need healing!"), lt("Power", "I need power!"), lt("Evacuate", "I need to get out of here!")]),
        (make_id(0, 1, -1), [lt("Attack", "Attack %4!"), lt("Retreat", "Retreat! We can't win this one."), lt("Back off", "Back off of %3."), lt("Follow", "Follow me!")]),
        (make_id(0, 2, -1), [lt("Passive", "|22"), lt("Defensive", "|23"), lt("Aggressive", "|24"), l("More...")]),
        (make_id(0, 2, 3), [lt("Neutral", "|25"), lt("Dismiss", "|26"), none, none]),
        (make_id(0, 3, -1), [lt("Help?", "Do you need help?"), lt("Confused", "What should I do?"), lt("Where to?", "Where should we go?"), lt("Location?", "Where are you?")]),
        (make_id(1, -1, -1), [l("Questions"), l("Needs"), l("Answers"), l("Emote")]),
        (make_id(1, 0, -1), [lt("Class/Level", "What level and class are you?"), l("Requests"), none, none]),
        (make_id(1, 0, 1), [lt("Group", "I am a %2 level %3 looking for a group."), lt("Trade", "Are you interested in some of the great items I have for sale?"), lt("Teleport", "I am looking to travel a long distance. Can you help me?"), none]),
        (make_id(1, 1, -1), [lt("Food", "I need food."), lt("Directions", "I need directions."), none, none]),
        (make_id(1, 2, -1), [lt("Class/Level", "I am a level %2 %3."), lt("Yes", "Yes"), lt("No", "No"), lt("Maybe", "I'm not sure.")]),
        (make_id(1, 3, -1), [l("Fun"), l("Happy"), l("Sad"), l("Physical")]),
        (make_id(1, 3, 3), [lt("Wave", "|0"), lt("Bow", "|1"), lt("Point", "|2"), lt("Cheer", "|3")]),
        (make_id(2, -1, -1), [l("Group"), l("Privacy"), l("Custom setup"), l("Chat")]),
        (make_id(2, 0, -1), [lt("Invite", "|4"), lt("Boot Member", "|5"), none, lt("Leave Group", "|6")]),
        (make_id(2, 1, -1), [l("Ignore Target"), l("Stop Ignoring"), l("Privacy List"), l("More...")]),
        (make_id(2, 1, 3), [lt("Ignore Shouts", "|10"), lt("Restore Shouts", "|11"), lt("Ignore Guild", "|12"), lt("Restore Guild", "|13")]),
        (make_id(2, 3, -1), [l("Modes"), lt("Reply", "|18"), lt("Tell", "|19"), l("Message")]),
        (make_id(2, 3, 0), [lt("Say", "|14"), lt("Group", "|15"), lt("Shout", "|16"), lt("Guild", "|17")]),
    ];
    for (id, pairs) in tree {
        set_text_set(sets, id, pairs, &mut out);
    }
    out
}

/// What the menus need of the client to run and draw.
pub struct Ctx<'a> {
    pub vui: &'a Vui,
    pub now: i64,
    pub cues: &'a mut Vec<i32>,
    pub outs: &'a mut Vec<Out>,
}

fn word(vui: &Vui, id: i32) -> Vec<u16> {
    wide(vui.words.get(id, vui.language))
}

/// What the editor tells the menu it is for (`VIWnd::OnMessage` to it).
#[derive(Clone, Copy, Debug, PartialEq)]
enum EditorUp {
    None,
    /// 259
    Done,
    /// 260
    Cancel,
}

/// `VIWndMenu4Edit`: a menu over the choice (the name; the line, or with a
/// branch "back to a line"; "make it a branch" above depth 3; Done), the
/// "text or command" window and the command list, all three about the
/// screen's middle, and the edit box at (32, 190) for a name or a text.
pub struct Menu4Edit {
    /// `m_pos`: where the window flies in from (the game stores it and
    /// draws nothing from it)
    pub pos: [i32; 2],
    pub sel: i32,
    edit: Option<EditBox>,
    /// the edit box is up (and has the keys)
    edit_open: bool,
    /// `m_text`, at most 127 characters, and `m_title`, at most 31
    pub text: Vec<u16>,
    pub title: Vec<u16>,
    pub has_sub: bool,
    kind_open: bool,
    cmd_open: bool,
    window: Window,
    center: [i32; 2],
    kind_window: Window,
    kind_center: [i32; 2],
    /// 0 a text, 1 a command
    pub kind: i32,
    cmd_window: Window,
    cmd_center: [i32; 2],
    /// the command list's entry, 0..33 (a line's own number when it opens)
    pub cmd: i32,
    scroll: f32,
    scroll_dir: i32,
    pub depth: i32,
    /// where the rows were drawn: 0.. the menu's, 100.. the kind's, 200 + i
    /// the command list's row i
    hits: Hits,
}

impl Menu4Edit {
    /// `VIWndMenu4Edit()`
    fn new() -> Self {
        let center = [320, 240];
        Menu4Edit {
            pos: [0, 0],
            sel: 0,
            edit: None,
            edit_open: false,
            text: Vec::new(),
            title: Vec::new(),
            has_sub: false,
            kind_open: false,
            cmd_open: false,
            window: Window::default(),
            center,
            kind_window: Window::default(),
            kind_center: center,
            kind: 0,
            cmd_window: Window::default(),
            cmd_center: center,
            cmd: 0,
            scroll: 0.0,
            scroll_dir: 0,
            depth: 0,
            hits: Hits::default(),
        }
    }

    /// `SetTitle`/`SetText` (nothing for a null string).
    fn set_title(&mut self, s: Option<&[u16]>) {
        if let Some(s) = s {
            self.title = field(Some(s), 32);
        }
    }

    fn set_text(&mut self, s: Option<&[u16]>) {
        if let Some(s) = s {
            self.text = field(Some(s), 128);
        }
    }

    /// The last row, Done's: 2 with a branch or at depth 3, else 3.
    fn last_row(&self) -> i32 {
        if self.has_sub {
            2
        } else if self.depth < 3 {
            3
        } else {
            2
        }
    }

    /// The edit box with `text`, at (32, 190), given the keys (its focus
    /// starts a fresh session).
    fn open_edit(&mut self, text: &[u16], now: i64) {
        let e = self.edit.get_or_insert_with(EditBox::default);
        e.set_text(text);
        e.set_pos([32, 190]);
        e.focus(now);
        self.edit_open = true;
    }

    /// `OnKeyDown`: the command list's (a press starts a row's glide, a
    /// second while one is under way finishes it), the kind window's while
    /// it is fully open, else the menu's.
    fn key_down(&mut self, key: i32, ctx: &mut Ctx) -> EditorUp {
        let cues = &mut *ctx.cues;
        let now = ctx.now;
        if self.cmd_open {
            match key {
                19 | 138 => {
                    if self.cmd > 0 {
                        cues.push(11);
                        if self.cmd == 1 && ee::eq(self.scroll, 0.0) && self.scroll_dir == 0 {
                            self.scroll = 0.0;
                            self.scroll_dir = -1;
                        } else if self.cmd == 1 && ee::lt(0.0, self.scroll) && self.scroll_dir == -1 {
                            self.scroll = 0.0;
                            self.scroll_dir = 0;
                            self.cmd = 0;
                        }
                        if self.cmd > 1 {
                            if self.scroll_dir == -1 {
                                self.cmd -= 1;
                            }
                            self.scroll_dir = -1;
                            self.scroll = 0.0;
                        }
                    }
                }
                20 | 139 => {
                    if self.cmd < 33 {
                        cues.push(11);
                        if self.cmd == 32 && ee::eq(self.scroll, 0.0) && self.scroll_dir == 0 {
                            self.scroll = 0.0;
                            self.scroll_dir = 1;
                        } else if self.cmd == 32 && ee::lt(0.0, self.scroll) && self.scroll_dir == 1 {
                            self.scroll = 0.0;
                            self.cmd = 33;
                            self.scroll_dir = 0;
                        }
                        if self.cmd < 32 {
                            if self.scroll_dir == 1 {
                                self.cmd += 1;
                            }
                            self.scroll_dir = 1;
                            self.scroll = 0.0;
                        }
                    }
                }
                13 | 134 => {
                    self.pick_command();
                    cues.push(13);
                }
                27 | 132 => {
                    self.cmd_window.close(false);
                    self.kind_open = true;
                    self.kind_window.open(false);
                    cues.push(8);
                }
                _ => {}
            }
        } else if self.kind_open && self.kind_window.is_open() {
            match key {
                19 | 138 => {
                    self.kind = (self.kind - 1).max(0);
                    cues.push(11);
                }
                20 | 139 => {
                    self.kind = (self.kind + 1).min(1);
                    cues.push(11);
                }
                13 | 134 => {
                    self.accept_kind(now);
                    cues.push(13);
                }
                27 | 132 => {
                    self.kind_window.close(false);
                    cues.push(8);
                }
                _ => {}
            }
        } else {
            match key {
                19 | 138 => {
                    self.sel = (self.sel - 1).max(0);
                    cues.push(11);
                }
                20 | 139 => {
                    self.sel = self.last_row().min(self.sel + 1);
                    cues.push(11);
                }
                13 | 134 => {
                    let up = self.accept(now);
                    cues.push(13);
                    return up;
                }
                27 | 132 => {
                    cues.push(8);
                    return EditorUp::Cancel;
                }
                _ => {}
            }
        }
        EditorUp::None
    }

    /// `Accept`: ✕ on the menu.
    fn accept(&mut self, now: i64) -> EditorUp {
        if self.sel == 0 {
            let t = self.title.clone();
            self.open_edit(&t, now);
        } else if !self.has_sub && self.sel == 1 {
            self.kind_open = true;
            self.kind = (self.text.first() == Some(&(b'|' as u16))) as i32;
            self.kind_window.open(false);
        } else if self.has_sub {
            if self.sel == 1 {
                self.has_sub = false;
                self.sel = 2;
            } else if self.sel == 2 {
                return EditorUp::Done;
            }
        } else if self.sel == 2 && self.depth < 3 {
            self.has_sub = true;
            self.sel = 1;
            self.text.clear();
        } else if self.sel == self.last_row() {
            return EditorUp::Done;
        }
        EditorUp::None
    }

    /// `AcceptKind`: a text edits the line as text (a command's dropped
    /// first); a command opens the list at the line's own number.
    fn accept_kind(&mut self, now: i64) {
        let n = command_number(&self.text);
        match self.kind {
            0 => {
                if self.text.first() == Some(&(b'|' as u16)) {
                    self.text.clear();
                }
                let t = self.text.clone();
                self.open_edit(&t, now);
                self.kind_window.close(false);
            }
            1 => {
                self.kind_window.close(false);
                self.cmd_open = true;
                self.cmd_window.open(false);
                self.reset_command();
                self.cmd = n.unwrap_or(0);
            }
            _ => {}
        }
    }

    /// `PickCommand`: the line is `|<the entry>`.
    fn pick_command(&mut self) {
        self.text = field(Some(&wide(&format!("|{}", self.cmd))), 128);
        self.cmd_window.close(false);
    }

    /// `ResetCommand`
    fn reset_command(&mut self) {
        self.cmd = 0;
        self.scroll = 0.0;
        self.scroll_dir = 0;
    }

    /// The edit box's keys: done takes its text into the name (the menu's
    /// first row) or the line (the second, without a branch); done or
    /// cancelled, the editor has the keys again.
    fn edit_key(&mut self, key: i32, down: bool, vui: &Vui, r: &dyn Raster, ctx: &mut Ctx) {
        let Some(e) = self.edit.as_mut() else { return };
        if !down {
            e.key_up(key);
            return;
        }
        match e.key(vui, r, key, ctx.now, ctx.cues) {
            EditUp::Done(_) => {
                let t = e.text.clone();
                if self.sel == 0 {
                    self.title = field(Some(&t), 32);
                } else if self.sel == 1 && !self.has_sub {
                    self.text = field(Some(&t), 128);
                }
                self.edit_open = false;
            }
            EditUp::Cancel => self.edit_open = false,
            EditUp::None => {}
        }
    }

    /// `OnDrawSelf`: the menu, then the kind window, then the command list
    /// (each of the last two shut once it has closed); and the edit box, a
    /// child, over them.
    fn draw(&mut self, r: &mut dyn Raster, ctx: &mut Ctx) {
        let vui = ctx.vui;
        let hint = {
            let mut h = wide("|x");
            h.extend(word(vui, 212));
            h.extend(wide("   |t"));
            h.extend(word(vui, 213));
            h.truncate(63);
            h
        };
        let c0 = UI_COLORS[0];
        let row = |me: &Self, i: i32| UI_COLORS[(me.sel == i) as usize];
        self.hits.clear();
        if self.has_sub {
            self.window.init(vui, r, self.center, 220, 7, 0);
            self.window.open(false);
            if self.window.draw(vui, r) {
                let w = self.window.size[0];
                let rows: [(i32, i32, [f32; 4]); 4] = [(920, 0, c0), (921, 2, row(self, 0)), (922, 3, row(self, 1)), (280, 4, row(self, 2))];
                for (wd, rw, c) in rows {
                    self.window.text_box(vui, r, &word(vui, wd), 20, rw, w - 40, 1, c, 8);
                }
                self.window.text_box(vui, r, &hint, 20, 6, w - 40, 1, c0, 0);
                for (k, rw) in [2, 3, 4].into_iter().enumerate() {
                    self.hits.push(rows_rect(&self.window, vui, r, 20, rw, w - 40, 1), k as i32);
                }
            }
        } else {
            let deep = self.depth < 3;
            self.window.init(vui, r, self.center, 220, if deep { 8 } else { 7 }, 0);
            self.window.open(false);
            if self.window.draw(vui, r) {
                let w = self.window.size[0];
                self.window.text_box(vui, r, &word(vui, 923), 20, 0, w - 40, 1, c0, 8);
                self.window.text_box(vui, r, &word(vui, 921), 20, 2, w - 40, 1, row(self, 0), 8);
                self.window.text_box(vui, r, &word(vui, 924), 20, 3, w - 40, 1, row(self, 1), 8);
                if deep {
                    self.window.text_box(vui, r, &word(vui, 925), 20, 4, w - 40, 1, row(self, 2), 8);
                }
                let (done, last) = if deep { (5, 3) } else { (4, 2) };
                self.window.text_box(vui, r, &word(vui, 280), 20, done, w - 40, 1, row(self, last), 8);
                self.window.text_box(vui, r, &hint, 20, if deep { 7 } else { 6 }, w - 40, 1, c0, 0);
                for k in 0..=last {
                    self.hits.push(rows_rect(&self.window, vui, r, 20, 2 + k, w - 40, 1), k);
                }
            }
        }
        if self.kind_open {
            self.kind_window.init(vui, r, self.kind_center, 230, 4, 0);
            self.kind_window.draw(vui, r);
            if self.kind_window.is_open() {
                let w = self.kind_window.size[0];
                let kc = |me: &Self, i: i32| UI_COLORS[(me.kind == i) as usize];
                self.kind_window.text_box(vui, r, &word(vui, 926), 20, 0, w - 40, 1, kc(self, 0), 8);
                self.kind_window.text_box(vui, r, &word(vui, 927), 20, 1, w - 40, 1, kc(self, 1), 8);
                self.kind_window.text_box(vui, r, &hint, 20, 3, w - 40, 1, c0, 0);
                for k in 0..2 {
                    self.hits.push(rows_rect(&self.kind_window, vui, r, 20, k, w - 40, 1), 100 + k);
                }
            }
            if self.kind_window.open == 0.0 {
                self.kind_open = false;
            }
        }
        if self.cmd_open {
            self.cmd_window.init(vui, r, self.cmd_center, 230, 9, 0);
            self.cmd_window.draw(vui, r);
            if self.cmd_window.is_open() {
                let w = self.cmd_window.size[0];
                self.cmd_window.text_box(vui, r, &word(vui, 928), 20, 0, w - 40, 1, c0, 8);
                self.draw_commands(r, vui);
                self.cmd_window.text_box(vui, r, &hint, 20, 8, w - 40, 1, c0, 0);
            }
            if self.cmd_window.open == 0.0 {
                self.cmd_open = false;
            }
        }
        if self.edit_open
            && let Some(e) = self.edit.as_mut()
        {
            e.draw(vui, r, ctx.now, ctx.cues);
        }
    }

    /// `DrawCommands`: seven rows about the entry, fading out to the
    /// window's edges (none on the first and last), gliding by the glide's
    /// eased fraction while a step is under way, which ends a fifth of a
    /// row's way a frame.
    ///
    /// The glide is the console's arithmetic (`ee`): its adder truncates, so
    /// five steps of 0.2 make 0.99999994 and a row takes six frames (five on
    /// the host's).
    fn draw_commands(&mut self, r: &mut dyn Raster, vui: &Vui) {
        let rc: Vec<[i32; 2]> = (0..8).map(|row| self.cmd_window.row_center(vui, r, row)).collect();
        let left = ee::sub(1.0, self.scroll);
        let glide = ee::mul((rc[1][1] - rc[0][1]) as f32, ee::sub(1.0, ee::mul(left, left))) as i32;
        let (top, mid, bottom) = (rc[1], rc[4], rc[7]);
        let (x0, width) = (self.cmd_window.top_left()[0] + 20, self.cmd_window.size[0]);
        let h = (vui.font_height(r) - 4) / 2;
        // the entries from three before the current, a row each
        for (i, k) in (0..7).zip(self.cmd - 3..) {
            if (k as u32) < 34 {
                let mut p = rc[i as usize + 1];
                p[0] += 5;
                p[1] -= glide * self.scroll_dir;
                let c = if p[1] <= top[1] || p[1] >= bottom[1] {
                    [0.0; 4]
                } else {
                    let (num, den) = if p[1] < mid[1] { (p[1] - top[1], mid[1] - top[1]) } else { (p[1] - bottom[1], mid[1] - bottom[1]) };
                    let t = ee::div(num as f32, den as f32);
                    [0.0, 0.0, 0.0, ee::mul(t, t)]
                };
                let s = word(vui, COMMAND_WORDS[k as usize]);
                r.draw_centered_string(p, c, &s, 0);
                // the rows faded out to nothing take no click
                if c[3] > 0.0 {
                    self.hits.push([x0, p[1] - h, x0 + width - 40, p[1] + h], 200 + i);
                }
            }
        }
        if self.scroll_dir != 0 {
            self.scroll = ee::add(self.scroll, 0.2);
            if !ee::lt(self.scroll, 1.0) {
                let cmd = self.cmd + self.scroll_dir;
                self.scroll_dir = 0;
                self.scroll = 0.0;
                self.cmd = cmd.clamp(0, 33);
            }
        }
    }

    /// A pointer on the editor: on its edit box the soft keyboard's key; on
    /// a row of the menu or the kind window the keys that move the selection
    /// there, and ✕ on the selection; on a command's row the steps to it,
    /// and ✕ on the entry.
    fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.edit_open {
            return self.edit.as_mut().and_then(|e| e.point(p).flatten()).into_iter().collect();
        }
        let Some(hit) = self.hits.at(p) else { return Vec::new() };
        let to = |from: i32, to: i32| if from == to { vec![key::CROSS] } else { crate::pointer::steps(from, to, key::UP, key::DOWN) };
        if self.cmd_open {
            return match hit {
                200..=206 => to(3, hit - 200),
                _ => Vec::new(),
            };
        }
        if self.kind_open {
            return match hit {
                100 | 101 if self.kind_window.is_open() => to(self.kind, hit - 100),
                _ => Vec::new(),
            };
        }
        match hit {
            0..=3 => to(self.sel, hit),
            _ => Vec::new(),
        }
    }
}

/// `VIWndMenu4`
pub struct Menu4 {
    /// `m_pos`
    pub pos: [i32; 2],
    /// `m_set`: its set's id
    pub set: u8,
    sub: Option<Box<Menu4>>,
    alpha: f32,
    /// `m_fade`: > 0 fading in, else out (to 0.3)
    fade: f32,
    /// `m_hold`: frames a direction has been held, -1 none
    hold: i32,
    editor: Option<Box<Menu4Edit>>,
    /// `m_dir`: the direction pressed (0 up, 1 left, 2 right, 3 down), -1 none
    pub dir: i32,
    pub depth: i32,
    pub visible: bool,
    /// where the four choices' bars were drawn (pointer), by direction
    hits: Hits,
}

impl Menu4 {
    /// `VIWndMenu4()`
    fn new() -> Self {
        Menu4 { pos: [0, 0], set: 0, sub: None, alpha: 0.0, fade: 1.0, hold: -1, editor: None, dir: -1, depth: 0, visible: true, hits: Hits::default() }
    }

    /// `SetSet`: the set, its open branch hidden.
    fn set_set(&mut self, set: u8) {
        self.set = set;
        if let Some(s) = self.sub.as_mut() {
            s.visible = false;
        }
    }

    /// The pair `dir` of this menu's set (an empty one where the table has
    /// no such set).
    fn pair(&self, sets: &TextSets, dir: i32) -> TextPair {
        sets.set(self.set as i32).map_or_else(TextPair::default, |s| s.pairs[dir as usize].clone())
    }

    /// `OnMessage` 295, for the window the keys go to: from anything but its
    /// own editor it lights up from nothing (and an editor it had goes).
    fn focused(&mut self) {
        self.hold = -1;
        self.alpha = 0.0;
        self.fade = 1.0;
        self.editor = None;
    }

    /// `OnMessage` 295, for the window the keys leave: to anything but its
    /// own editor it dims from full (to 0.3).
    fn unfocused(&mut self) {
        self.hold = -1;
        self.alpha = 1.0;
        self.fade = -1.0;
    }

    /// `OnKeyDown`: a direction is pressed (and the hold begins); L3 and,
    /// with none pressed, every other key go to the parent. True when it
    /// passes the key on.
    fn key_down(&mut self, key: i32) -> bool {
        self.hold = 0;
        if key == key::L3 {
            return true;
        }
        if let Some(d) = direction(key) {
            self.dir = d;
        }
        self.dir == -1
    }

    /// `OnKeyUp`: the tap. The pressed direction let go runs its line (a
    /// command is the game window's message 258; a text is said, then 258
    /// without one) and opens its branch over this menu. True when it
    /// passes the key on (L3's release goes nowhere).
    fn key_up(&mut self, key: i32, sets: &TextSets, ctx: &mut Ctx) -> bool {
        self.hold = -1;
        if key == key::L3 {
            return false;
        }
        if self.dir == -1 {
            return true;
        }
        if direction(key) != Some(self.dir) {
            return true;
        }
        let dir = self.dir;
        let pair = self.pair(sets, dir);
        let mut closed = false;
        if let Some(text) = &pair.text {
            ctx.cues.push(13);
            if text.first() == Some(&(b'|' as u16)) {
                if let Some((msg, arg)) = command_number(text).and_then(|n| COMMANDS.get(usize::try_from(n).ok()?)) {
                    ctx.outs.push(Out::Done(*msg as i32, *arg));
                    closed = true;
                }
            } else {
                ctx.outs.push(Out::Say(text.clone()));
                ctx.outs.push(Out::Done(-1, 0));
                closed = true;
            }
        }
        if let Some(child) = sets.child(self.set, dir)
            && !closed
        {
            let (depth, pos) = (self.depth, self.pos);
            let sub = self.sub.get_or_insert_with(|| {
                let mut m = Menu4::new();
                m.depth = depth + 1;
                m.pos = [pos[0] + 16, pos[1] + 16];
                Box::new(m)
            });
            ctx.cues.push(11);
            // SetActiveWindow(m_sub): the branch lights up, this one dims
            sub.focused();
            sub.set_set(child);
            sub.visible = true;
            self.unfocused();
        }
        self.dir = -1;
        false
    }

    /// `OnMessage` 259 from the editor: the choice is what it says. A choice
    /// made a branch keeps its name and gets an empty set (0x006C, then that
    /// set's 0x006A); a branch made a line drops its whole tree (0x006C,
    /// then a 0x006B for each set); otherwise the name and the line (0x006C).
    /// Then, as for 260, the menu has the keys again and the editor goes.
    fn editor_done(&mut self, sets: &mut TextSets, ctx: &mut Ctx) {
        let Some(ed) = self.editor.take() else { return };
        if (self.dir as u32) < 4 {
            let dir = self.dir;
            let set_id = self.set;
            // a set the dump never gave is made here (the game's is there)
            let had_child = sets.child(set_id, dir).is_some();
            let Some(set) = sets.get(set_id as i32) else { return };
            let pair = &mut set.pairs[dir as usize];
            pair.label = None;
            pair.text = None;
            let name = (!ed.title.is_empty()).then(|| ed.title.clone());
            if ed.has_sub && !had_child {
                pair.label = name;
                let rec = fill_rec(pair, dir);
                sets.get_child(set_id, dir);
                ctx.outs.push(Out::Send(send_006c(set_id as i32, &rec)));
                let empty: [Record; 4] = std::array::from_fn(|k| Record { slot: k as i32, ..Default::default() });
                ctx.outs.push(Out::Send(send_006a(child_id(set_id, dir), &empty)));
            } else if !ed.has_sub && had_child {
                pair.label = name;
                pair.text = (!ed.text.is_empty()).then(|| ed.text.clone());
                let rec = fill_rec(pair, dir);
                ctx.outs.push(Out::Send(send_006c(set_id as i32, &rec)));
                if let Some(c) = sets.child(set_id, dir) {
                    sets.mark(c, true);
                }
                for i in 0..256 {
                    if sets.set(i).is_some_and(|s| s.p01 == 2) {
                        sets.remove(i as u8);
                        ctx.outs.push(Out::Send(send_006b(i)));
                    }
                }
            } else {
                pair.label = name;
                pair.text = (!ed.text.is_empty()).then(|| ed.text.clone());
                let rec = fill_rec(pair, dir);
                ctx.outs.push(Out::Send(send_006c(set_id as i32, &rec)));
            }
        }
        self.closed_editor();
    }

    /// 260's: the menu has the keys again (from its editor: its light is
    /// kept), the editor gone, no direction pressed.
    fn closed_editor(&mut self) {
        self.hold = -1;
        self.editor = None;
        self.dir = -1;
    }

    /// `OnDrawSelf` + `DrawChoice`: the light (in fast while it has the
    /// keys, out slowly to 0.3), the four bars with the d-pad's arrows, the
    /// four choices, and the hold: 16 frames on a direction open the editor
    /// on its choice.
    fn draw(&mut self, sets: &TextSets, r: &mut dyn Raster, ctx: &mut Ctx) {
        let vui = ctx.vui;
        let mut a = self.alpha;
        if self.fade > 0.0 {
            a += 0.32;
            a = if a < 1.0 { a } else { 1.0 };
        } else {
            a -= 0.105;
            if a < 0.3 {
                a = 0.3;
            }
        }
        self.alpha = a;
        let at = self.pos;
        let bars: [([i32; 2], i32, [i32; 2], u32); 4] = [
            ([at[0] + 80, at[1] + 4], 225, [75, -16], 0),
            ([at[0], at[1] + 29], 224, [-16, 5], 0),
            ([at[0] + 162, at[1] + 29], 224, [162, 5], 1),
            ([at[0] + 80, at[1] + 54], 225, [75, 25], 2),
        ];
        let bar_h = vui.entry(25).uv1[1].max(vui.font_height(r));
        self.hits.clear();
        for (dir, (p, glyph, off, flags)) in bars.into_iter().enumerate() {
            vui.blit_bar(r, &[], [0.0; 4], p, 25, 26, 162, 3, self.alpha, 0, true);
            vui.blit_ex(r, glyph, [p[0] + off[0], p[1] + off[1]], 3, self.alpha, flags);
            self.hits.push([p[0], p[1], p[0] + 162, p[1] + bar_h], dir as i32);
        }
        let choices = [[at[0] + 160, at[1] + 15], [at[0] + 80, at[1] + 39], [at[0] + 243, at[1] + 39], [at[0] + 160, at[1] + 65]];
        let words = |n: i32| Some(word(vui, n));
        for (i, c) in choices.into_iter().enumerate() {
            let i = i as i32;
            let pair = self.pair(sets, i);
            let child = sets.child(self.set, i).is_some();
            if let Some(buf) = vi::textset::expand(pair.label.as_deref(), 256, &words) {
                let mut w = Window::default();
                let y = if (1..=2).contains(&i) { c[1] + 3 } else { c[1] + 2 };
                w.init(vui, r, [c[0], y], 128, 1, 0);
                w.open(true);
                let width = w.size[0];
                let col = if self.dir == i { [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, self.alpha] } else { [0.0, 0.0, 0.0, self.alpha] };
                w.text_box(vui, r, &buf, 0, 0, width, 1, col, 10);
            }
            if child {
                vui.blit_ex(r, 123, [c[0] + 68, c[1] + 4], 3, self.alpha, 0);
            }
            if pair.label.is_none() && pair.text.is_none() && !child {
                vui.blit_ex(r, 124, [c[0] - 5, c[1] - 6], 3, self.alpha, 0);
            }
        }
        if self.hold >= 0 {
            self.hold += 1;
            if self.hold >= 16 {
                self.hold = -1;
                if (self.dir as u32) < 4 {
                    let d = self.dir;
                    let pair = self.pair(sets, d);
                    let mut e = Menu4Edit::new();
                    e.pos = choices[d as usize];
                    e.set_title(pair.label.as_deref());
                    e.set_text(pair.text.as_deref());
                    e.has_sub = sets.child(self.set, d).is_some();
                    e.depth = self.depth;
                    // SetActiveWindow(m_editor): the menu keeps its light
                    self.editor = Some(Box::new(e));
                    ctx.cues.push(9);
                }
            }
        }
    }
}

/// `FillRec`: the pair as its slot's record (as shipped, the slot is the
/// direction pressed).
fn fill_rec(pair: &TextPair, dir: i32) -> Record {
    Record { slot: dir, text: field(pair.text.as_deref(), 128), label: field(pair.label.as_deref(), 64) }
}

/// Which window in the chain has the keys: the menu at depth d, its editor,
/// or the editor's edit box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Active {
    Menu(usize),
    Editor(usize),
    Edit(usize),
}

/// The quick chat: the sets, `VIWndGame`'s `m_menu4` and what hangs off it,
/// and the keys waiting for the next of the game's frames.
pub struct QuickChat {
    pub sets: TextSets,
    root: Menu4,
    keys: Vec<(i32, bool)>,
}

impl Default for QuickChat {
    fn default() -> Self {
        // VIWndGame::Init: m_visible 0, at (175, 18)
        let mut root = Menu4::new();
        root.visible = false;
        root.pos = [175, 18];
        QuickChat { sets: TextSets::default(), root, keys: Vec::new() }
    }
}

impl QuickChat {
    /// `LoadTextSets`, from the world entry's records.
    pub fn load(&mut self, sets: &[net::dump::TextSet]) {
        let recs: Vec<(i32, Vec<Record>)> = sets
            .iter()
            .map(|s| {
                let rec = |e: &net::dump::TextEntry| Record { slot: e.id as i32, text: e.text.encode_utf16().collect(), label: e.label.encode_utf16().collect() };
                (s.id as i32, s.entries.iter().map(rec).collect())
            })
            .collect();
        self.sets.load(&recs);
    }

    pub fn visible(&self) -> bool {
        self.root.visible
    }

    /// `VIWndGame::OnKeyDown` L2 (Z) in play: the root's set, shown, with
    /// the keys. False when it was up already.
    pub fn open(&mut self) -> bool {
        if self.root.visible {
            return false;
        }
        self.root.set_set(0);
        self.root.visible = true;
        self.root.focused();
        self.keys.clear();
        true
    }

    /// `m_menu4->SetVisible(false)`: the menus down (an editor open on one,
    /// which the game leaves to be found again, goes).
    pub fn close(&mut self) {
        let mut m = Some(&mut self.root);
        while let Some(x) = m {
            x.editor = None;
            x.hold = -1;
            x.dir = -1;
            m = x.sub.as_deref_mut().filter(|s| s.visible);
        }
        self.root.visible = false;
        self.keys.clear();
    }

    /// The menus open, root first: the root and each branch shown over it.
    fn chain_len(&self) -> usize {
        let mut n = 0;
        let mut m = Some(&self.root);
        while let Some(x) = m.filter(|x| x.visible) {
            n += 1;
            m = x.sub.as_deref();
        }
        n
    }

    fn menu(&mut self, depth: usize) -> &mut Menu4 {
        menu_at(&mut self.root, depth)
    }

    /// The window with the keys.
    pub fn active(&mut self) -> Option<Active> {
        let n = self.chain_len();
        if n == 0 {
            return None;
        }
        let d = n - 1;
        Some(match self.menu(d).editor.as_ref() {
            Some(e) if e.edit_open => Active::Edit(d),
            Some(_) => Active::Editor(d),
            None => Active::Menu(d),
        })
    }

    /// The editor's edit box has the keys: the keyboard types into it.
    pub fn typing(&mut self) -> bool {
        matches!(self.active(), Some(Active::Edit(_)))
    }

    /// A menu (not its editor) has the keys: what the d-pad does there.
    pub fn menu_has_keys(&mut self) -> bool {
        matches!(self.active(), Some(Active::Menu(_)))
    }

    /// A key (the game's code) for the menus, handled with the next of the
    /// game's frames, before they are drawn.
    pub fn key(&mut self, key: i32, down: bool) {
        self.keys.push((key, down));
    }

    /// The keys, then the menus drawn (`OnDraw`: each menu, its editor and
    /// the editor's edit box, then the branch open over it), on one of the
    /// game's frames.
    pub fn run(&mut self, r: &mut dyn Raster, ctx: &mut Ctx) {
        for (key, down) in std::mem::take(&mut self.keys) {
            if !self.root.visible {
                // closed by an earlier key: a key let go still reaches the game
                if !down {
                    ctx.outs.push(Out::Key(key, false));
                }
                continue;
            }
            self.dispatch(key, down, r, ctx);
            if ctx.outs.iter().any(|o| matches!(o, Out::Done(..)) || matches!(o, Out::Key(k, true) if closes(*k))) {
                // the game window closes the menus at once (258, △, L2, ...)
                self.root.visible = false;
            }
        }
        if !self.root.visible {
            return;
        }
        let sets = &self.sets;
        let mut m = Some(&mut self.root);
        while let Some(x) = m {
            x.draw(sets, r, ctx);
            if let Some(e) = x.editor.as_mut() {
                e.draw(r, ctx);
            }
            m = x.sub.as_deref_mut().filter(|s| s.visible);
        }
    }

    /// One key to the window that has it, as `SetKey` hands it on.
    fn dispatch(&mut self, key: i32, down: bool, r: &mut dyn Raster, ctx: &mut Ctx) {
        let Some(active) = self.active() else { return };
        match active {
            Active::Edit(d) => {
                let vui = ctx.vui;
                if let Some(e) = self.menu(d).editor.as_mut() {
                    e.edit_key(key, down, vui, r, ctx);
                }
            }
            Active::Editor(d) => {
                if !down {
                    return;
                }
                let up = self.menu(d).editor.as_mut().map_or(EditorUp::None, |e| e.key_down(key, ctx));
                let (root, sets) = (&mut self.root, &mut self.sets);
                match up {
                    EditorUp::Done => menu_at(root, d).editor_done(sets, ctx),
                    EditorUp::Cancel => menu_at(root, d).closed_editor(),
                    EditorUp::None => {}
                }
            }
            Active::Menu(d) => {
                // from the menu with the keys down to the root, then the game
                for depth in (0..=d).rev() {
                    let (root, sets) = (&mut self.root, &self.sets);
                    let m = menu_at(root, depth);
                    let pass = if down { m.key_down(key) } else { m.key_up(key, sets, ctx) };
                    if !pass {
                        return;
                    }
                }
                ctx.outs.push(Out::Key(key, down));
            }
        }
    }

    /// A pointer on the menus: the window with the keys takes it (a menu's
    /// choice tapped, the editor's rows and the edit box's keys).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.active() {
            Some(Active::Menu(d)) => self.menu(d).hits.at(p).map(direction_key).into_iter().collect(),
            Some(Active::Editor(d) | Active::Edit(d)) => self.menu(d).editor.as_mut().map_or_else(Vec::new, |e| e.point(p)),
            None => Vec::new(),
        }
    }

    /// The d-pad key of the choice under `p`, when a menu has the keys (a
    /// press held there is that direction held: 16 frames open its editor).
    pub fn choice_key(&mut self, p: [f32; 2]) -> Option<i32> {
        match self.active() {
            Some(Active::Menu(d)) => self.menu(d).hits.at(p).map(direction_key),
            _ => None,
        }
    }
}

/// The menu at `depth` in the chain from `root` (each a branch open over the
/// one under it).
fn menu_at(root: &mut Menu4, depth: usize) -> &mut Menu4 {
    let mut m = root;
    for _ in 0..depth {
        m = m.sub.as_deref_mut().expect("a branch in the chain");
    }
    m
}

/// The keys `VIWndGame::OnKeyDown` closes the menus with in mode 30:
/// Escape, △, L2 and Z (and Enter and R3, which open the chat line in
/// their place).
fn closes(key: i32) -> bool {
    matches!(key, 27 | 132 | 141 | 90 | 122 | 13 | 147)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// A tree like a server's: the root (Responses, Options, a line, an
    /// empty choice), Responses' set with a line and a command.
    fn chat() -> QuickChat {
        let mut q = QuickChat::default();
        let rec = |slot: i32, text: &str, label: &str| Record { slot, text: w(text), label: w(label) };
        q.sets.load(&[
            (0, vec![rec(0, " ", "Responses"), rec(1, " ", "Options"), rec(2, "Hello %1!", "Hello"), rec(3, "", "")]),
            (make_id(0, -1, -1), vec![rec(0, "Yes", "Yes"), rec(1, "|0", "Wave"), rec(2, "|99", "Nothing"), rec(3, "", "")]),
            (make_id(1, -1, -1), vec![rec(0, "", ""), rec(1, "", ""), rec(2, "", ""), rec(3, "", "")]),
        ]);
        q
    }

    struct Run {
        q: QuickChat,
        cues: Vec<i32>,
        outs: Vec<Out>,
    }

    /// What went to the server.
    fn sent(outs: &[Out]) -> Vec<Vec<u8>> {
        outs.iter().filter_map(|o| if let Out::Send(m) = o { Some(m.clone()) } else { None }).collect()
    }

    impl Run {
        fn new(q: QuickChat) -> Self {
            Run { q, cues: Vec::new(), outs: Vec::new() }
        }
        /// One of the game's frames, with these keys first.
        fn frame(&mut self, keys: &[(i32, bool)]) {
            let v = vui();
            let mut r = NullRaster;
            for &(k, d) in keys {
                self.q.key(k, d);
            }
            let mut ctx = Ctx { vui: &v, now: 0, cues: &mut self.cues, outs: &mut self.outs };
            self.q.run(&mut r, &mut ctx);
        }
        fn tap(&mut self, k: i32) {
            self.frame(&[(k, true)]);
            self.frame(&[(k, false)]);
        }
    }

    /// A tap on a line says it and closes; on a command, the command's
    /// message; on a branch, its set in a menu over this one, which dims; a
    /// number past the 34 commands does nothing (and stays open).
    #[test]
    fn a_tap_runs_a_line_or_opens_a_branch() {
        let mut q = chat();
        assert!(q.open());
        let mut t = Run::new(q);
        t.frame(&[]);
        t.tap(key::RIGHT);
        assert_eq!(t.outs, vec![Out::Say(w("Hello %1!")), Out::Done(-1, 0)]);
        assert!(!t.q.visible(), "said: closed");
        // a branch, then its command
        t.outs.clear();
        assert!(t.q.open());
        t.frame(&[]);
        t.tap(key::UP);
        assert!(t.outs.is_empty());
        assert_eq!(t.q.active(), Some(Active::Menu(1)));
        assert_eq!(t.q.menu(1).set, 1);
        for _ in 0..10 {
            t.frame(&[]);
        }
        assert!((t.q.root.alpha - 0.3).abs() < 1e-6, "the root dimmed");
        assert_eq!(t.q.menu(1).alpha, 1.0, "the branch lit");
        t.tap(key::RIGHT);
        assert!(t.outs.is_empty() && t.q.visible(), "|99: no command, still open");
        t.tap(key::LEFT);
        assert_eq!(t.outs, vec![Out::Done(0x0601_0000, 0)]);
        assert!(!t.q.visible());
        assert!(t.cues.contains(&13) && t.cues.contains(&11));
        // control: a branch with nothing in the table opens no menu
        let mut q = chat();
        q.sets.remove(1);
        q.open();
        let mut t = Run::new(q);
        t.tap(key::UP);
        assert_eq!(t.q.active(), Some(Active::Menu(0)));
    }

    /// △, L2 and the keys the menus do not take go up to the game window;
    /// a branch's goes through the menu under it.
    #[test]
    fn keys_not_taken_go_to_the_game() {
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        t.tap(key::UP);
        t.frame(&[(key::CROSS, true), (key::CROSS, false)]);
        assert_eq!(t.outs, vec![Out::Key(key::CROSS, true), Out::Key(key::CROSS, false)]);
        assert!(t.q.visible());
        t.outs.clear();
        t.frame(&[(key::TRIANGLE, true), (key::TRIANGLE, false)]);
        assert_eq!(t.outs, vec![Out::Key(key::TRIANGLE, true), Out::Key(key::TRIANGLE, false)]);
        assert!(!t.q.visible(), "△ closed it");
        // L3's release goes nowhere
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        t.frame(&[(key::L3, true), (key::L3, false)]);
        assert_eq!(t.outs, vec![Out::Key(key::L3, true)]);
    }

    /// Held for 16 of the game's frames a direction opens the editor on its
    /// choice, with the menu's light kept; let go sooner it is a tap. The
    /// frames are the game's: at 165 Hz (one in five or six drawn) the hold
    /// takes the console's half second, not 0.1 s.
    #[test]
    fn a_hold_opens_the_editor_after_sixteen_frames() {
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        t.frame(&[(key::DOWN, true)]);
        for _ in 0..14 {
            t.frame(&[]);
        }
        assert!(t.q.root.editor.is_none(), "15 frames");
        t.frame(&[]);
        assert_eq!(t.q.active(), Some(Active::Editor(0)), "the 16th");
        assert!(t.cues.contains(&9));
        let e = t.q.root.editor.as_ref().unwrap();
        assert!(e.title.is_empty() && e.text.is_empty() && !e.has_sub, "the empty choice");
        assert_eq!(t.q.root.alpha, 1.0);
        // the release goes to the editor: nothing
        t.frame(&[(key::DOWN, false)]);
        assert!(t.outs.is_empty());
        // on a display at 165 Hz, the game's frames by frames.rs's clock
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        let mut clock = crate::frames::Frames::default();
        let mut opened = None;
        for k in 0..200 {
            let ms = k as f64 * 1000.0 / 165.0;
            if clock.due(ms) {
                t.frame(if k == 0 { &[(key::DOWN, true)] } else { &[] });
                if opened.is_none() && t.q.root.editor.is_some() {
                    opened = Some(ms);
                }
            }
        }
        let ms = opened.expect("the editor opened");
        assert!((480.0..=540.0).contains(&ms), "opened after {ms:.0} ms");
    }

    /// The editor: the name through the edit box, the line made a command
    /// from the list, Done; the change goes to the server as 0x006C.
    #[test]
    fn the_editor_renames_and_sends_the_choice() {
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        t.frame(&[(key::DOWN, true)]);
        for _ in 0..16 {
            t.frame(&[]);
        }
        t.frame(&[(key::DOWN, false)]);
        // Edit Label: the edit box, typed, Enter
        t.tap(key::CROSS);
        assert_eq!(t.q.active(), Some(Active::Edit(0)));
        let typed: Vec<(i32, bool)> = "Hi".chars().map(|c| (c as i32, true)).chain([(13, true)]).collect();
        t.frame(&typed);
        assert_eq!(t.q.active(), Some(Active::Editor(0)));
        assert_eq!(t.q.root.editor.as_ref().unwrap().title, w("Hi"));
        // Edit Action: Command, the list from 0, two down, pick
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        for _ in 0..8 {
            t.frame(&[]);
        }
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        assert!(t.q.root.editor.as_ref().unwrap().cmd_open);
        for _ in 0..8 {
            t.frame(&[]);
        }
        t.tap(key::DOWN);
        t.tap(key::DOWN);
        for _ in 0..8 {
            t.frame(&[]);
        }
        assert_eq!(t.q.root.editor.as_ref().unwrap().cmd, 2);
        t.tap(key::CROSS);
        assert_eq!(t.q.root.editor.as_ref().unwrap().text, w("|2"));
        for _ in 0..8 {
            t.frame(&[]);
        }
        // Done (row 3)
        t.tap(key::DOWN);
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        assert_eq!(t.q.active(), Some(Active::Menu(0)));
        let rec = Record { slot: 3, text: w("|2"), label: w("Hi") };
        assert_eq!(sent(&t.outs), vec![send_006c(0, &rec)]);
        // ✕ let go after Done reaches the game window (OnKeyUp 134)
        assert_eq!(t.outs.last(), Some(&Out::Key(key::CROSS, false)));
        let p = &t.q.sets.set(0).unwrap().pairs[3];
        assert_eq!((p.label.as_deref(), p.text.as_deref()), (Some(&w("Hi")[..]), Some(&w("|2")[..])));
        // and the choice runs as command 2 (Point)
        t.outs.clear();
        t.tap(key::DOWN);
        assert_eq!(t.outs, vec![Out::Done(0x0603_0000, 0)]);
    }

    /// A line made a branch gets an empty set (0x006C, 0x006A for the new
    /// id); a branch made a line drops its tree (0x006B for each set).
    #[test]
    fn a_branch_is_made_and_dropped() {
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        let hold = |t: &mut Run, k: i32| {
            t.frame(&[(k, true)]);
            for _ in 0..16 {
                t.frame(&[]);
            }
            t.frame(&[(k, false)]);
        };
        // the line "Hello" (right) made a branch: Change to Branch, Done
        hold(&mut t, key::RIGHT);
        t.tap(key::DOWN);
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        assert!(t.q.root.editor.as_ref().unwrap().has_sub);
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        let new = child_id(0, 2);
        let empty: [Record; 4] = std::array::from_fn(|k| Record { slot: k as i32, ..Default::default() });
        assert_eq!(sent(&t.outs), vec![send_006c(0, &Record { slot: 2, text: vec![], label: w("Hello") }), send_006a(new, &empty)]);
        assert!(t.q.sets.set(new).is_some());
        // Responses (up), a branch, made a line: its set and its own are dropped
        t.outs.clear();
        t.q.sets.get(child_id(1, 0));
        hold(&mut t, key::UP);
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        assert!(!t.q.root.editor.as_ref().unwrap().has_sub, "Change to Action");
        t.tap(key::DOWN);
        t.tap(key::CROSS);
        assert_eq!(sent(&t.outs), vec![send_006c(0, &Record { slot: 0, text: vec![], label: w("Responses") }), send_006b(1), send_006b(child_id(1, 0))]);
        assert!(t.q.sets.set(1).is_none() && t.q.sets.set(child_id(1, 0)).is_none());
        assert!(t.q.sets.set(5).is_some(), "Options' set kept");
    }

    /// △ in the editor is cancel: the menu has the keys, nothing sent.
    #[test]
    fn the_editor_cancels() {
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        t.frame(&[(key::UP, true)]);
        for _ in 0..16 {
            t.frame(&[]);
        }
        t.tap(key::TRIANGLE);
        assert_eq!(t.q.active(), Some(Active::Menu(0)));
        assert!(sent(&t.outs).is_empty());
        assert!(t.cues.contains(&8));
    }

    /// The command list glides: a press starts a row's step, a second while
    /// it is under way finishes the first and starts its own; the list stops
    /// at 0 and 33.
    #[test]
    fn the_command_list_glides() {
        let mut e = Menu4Edit::new();
        let v = vui();
        let mut r = NullRaster;
        let (mut cues, mut outs) = (Vec::new(), Vec::new());
        let mut ctx = Ctx { vui: &v, now: 0, cues: &mut cues, outs: &mut outs };
        e.cmd_open = true;
        e.cmd = 5;
        e.key_down(key::DOWN, &mut ctx);
        assert_eq!((e.cmd, e.scroll_dir), (5, 1));
        e.key_down(key::DOWN, &mut ctx);
        assert_eq!((e.cmd, e.scroll_dir), (6, 1), "the first step finished");
        for _ in 0..5 {
            e.draw_commands(&mut r, &v);
        }
        assert_eq!((e.cmd, e.scroll_dir), (6, 1), "the EE's 0.2 five times is 0.99999994");
        e.draw_commands(&mut r, &v);
        assert_eq!((e.cmd, e.scroll_dir), (7, 0), "six frames a row");
        // control: the host's adder would make it five
        assert!((0..5).fold(0.0f32, |a, _| a + 0.2) >= 1.0);
        e.cmd = 33;
        e.key_down(key::DOWN, &mut ctx);
        assert_eq!(e.scroll_dir, 0, "the end");
        e.cmd = 1;
        e.key_down(key::UP, &mut ctx);
        assert_eq!((e.cmd, e.scroll_dir), (1, -1));
        e.draw_commands(&mut r, &v);
        e.key_down(key::UP, &mut ctx);
        assert_eq!((e.cmd, e.scroll_dir), (0, 0), "finished at the top");
    }

    /// The defaults: every set dropped (the root's not sent), the built-in
    /// tree sent as made, the root as four 0x006C.
    #[test]
    fn the_defaults_replace_every_set() {
        let mut q = chat();
        let out = default_text_sets(&mut q.sets);
        assert_eq!(out[0], send_006b(1));
        assert_eq!(out[1], send_006b(5));
        let root: Vec<&Vec<u8>> = out.iter().filter(|m| m[..2] == [0x6C, 0]).collect();
        assert_eq!(root.len(), 4);
        assert_eq!(out.iter().filter(|m| m[..2] == [0x6A, 0]).count(), 19);
        let s = q.sets.set(make_id(1, 3, 3)).unwrap();
        assert_eq!(s.pairs[2].label.as_deref(), Some(&w("Point")[..]));
        assert_eq!(s.pairs[2].text.as_deref(), Some(&w("|2")[..]));
        assert_eq!(q.sets.set(make_id(2, 0, -1)).unwrap().pairs[2], TextPair::default());
    }

    /// The command list under a pointer: a row is the steps to it, the entry
    /// ✕; the rows faded out at its ends take no click.
    #[test]
    fn a_pointer_steps_the_command_list() {
        let v = vui();
        let mut r = NullRaster;
        let (mut cues, mut outs) = (Vec::new(), Vec::new());
        let mut ctx = Ctx { vui: &v, now: 0, cues: &mut cues, outs: &mut outs };
        let mut e = Menu4Edit::new();
        e.cmd_open = true;
        e.cmd = 10;
        e.cmd_window.open(true);
        e.draw(&mut r, &mut ctx);
        let w = e.cmd_window.size[0];
        let row = |e: &Menu4Edit, i: i32| mid(rows_rect(&e.cmd_window, &v, &NullRaster, 20, i + 1, w - 40, 1));
        assert_eq!(e.point(row(&e, 5)), vec![key::DOWN, key::DOWN]);
        assert_eq!(e.point(row(&e, 2)), vec![key::UP]);
        assert_eq!(e.point(row(&e, 3)), vec![key::CROSS]);
        assert!(e.point(row(&e, 0)).is_empty() && e.point(row(&e, 6)).is_empty(), "faded out");
        // control: without the hit notes
        e.hits.clear();
        assert!(e.point(row(&e, 5)).is_empty());
    }

    /// `|%d` as sscanf reads it.
    #[test]
    fn command_numbers() {
        assert_eq!(command_number(&w("|22")), Some(22));
        assert_eq!(command_number(&w("| 7x")), Some(7));
        assert_eq!(command_number(&w("|-3")), Some(-3));
        assert_eq!(command_number(&w("|")), None);
        assert_eq!(command_number(&w("|x")), None);
        assert_eq!(command_number(&w("22")), None);
    }

    /// A pointer: a choice's bar is its direction (tapped); on the editor a
    /// row moves the selection there, and ✕ on the selection. Without the
    /// hit notes neither answers.
    #[test]
    fn a_pointer_taps_choices_and_picks_rows() {
        let mut q = chat();
        q.open();
        let mut t = Run::new(q);
        t.frame(&[]);
        // the bars, 162 wide: up at (80, 4) from the menu's place, left
        // (0, 29), right (162, 29), down (80, 54)
        let at = t.q.root.pos;
        let bar = |dx: i32, dy: i32| [(at[0] + dx + 81) as f32, (at[1] + dy + 4) as f32];
        assert_eq!(t.q.point(bar(162, 29)), vec![key::RIGHT]);
        assert_eq!(t.q.choice_key(bar(80, 4)), Some(key::UP));
        assert_eq!(t.q.point(bar(0, 29)), vec![key::LEFT]);
        assert!(t.q.point([5.0, 400.0]).is_empty());
        // the editor on the empty choice (down): its four rows from row 2
        t.frame(&[(key::DOWN, true)]);
        for _ in 0..24 {
            t.frame(&[]);
        }
        let v = vui();
        let e = t.q.root.editor.as_ref().unwrap();
        let w = e.window.size[0];
        let row = |k: i32| mid(rows_rect(&e.window, &v, &NullRaster, 20, 2 + k, w - 40, 1));
        let (r0, r3) = (row(0), row(3));
        assert_eq!(t.q.point(r3), vec![key::DOWN, key::DOWN, key::DOWN]);
        assert_eq!(t.q.point(r0), vec![key::CROSS]);
        // control: without the hit notes
        t.q.root.editor.as_mut().unwrap().hits.clear();
        t.q.root.hits.clear();
        assert!(t.q.point(r3).is_empty());
        assert!(t.q.choice_key(bar(162, 29)).is_none());
    }
}
