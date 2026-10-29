//! `VIWndDuel`, the live game's duel window (eqoa-live-1500's CLIENT: vtable
//! 0x013F87D0, 1,160 bytes; the Frontiers disc has no such window): the
//! invitation (`VIWndGame` mode 43) and the wager (mode 44), the part's
//! (`duel.rs`) two windows in one.
//!
//! The invitation (`DrawInvite`, 0x013487C8) is a window titled "Duel" in the
//! middle of the screen: who challenges (word 1866), their guild, race,
//! class (the Master Class title where they have one) and level, scrolling
//! when that is seven lines or more; the seconds left once under ten; ✕
//! Accept, △ Decline. It counts its time down by the client's clock.
//!
//! The wager (`DrawWager`, 0x01348A58) is the trade window (`trade.rs`)
//! copied, its title "Wager": the inventory, the player's offer and the
//! opponent's, both sums of tunar; ✕ stakes the item under the cursor (a
//! stack asks how many), takes an offered one back or opens the tunar
//! (`VIWndDeposit` in mode 4, the purse its limit); □ accepts; △ takes the
//! accept back, or asks "Cancel duel?"; R3 (Space) the chat line. Every
//! choice is a request to the server (0x1501, `duel.rs`), which answers with
//! what both sides now stake; an accepted side glows with the window's
//! pulse.
//!
//! A pointer (`pointer`) works as the trade window's: the child with the keys
//! takes it; on the wager a click on another list (or the tunar) moves the
//! focus there and picks the row, a click on the list with the focus uses
//! the row. The legend's glyphs are their buttons.

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::duel::{Invite, Part, word};
use crate::editbox::{EditBox, EditUp};
use crate::infotext::Info;
use crate::pointer::{Hits, MOVED, key};
use crate::quantity::{Quantity, QuantityUp};
use crate::trade::{Deposit, DepositUp};
use crate::wndlist::WndList;
use net::dump::NetItem;
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// What the window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum WndOut {
    /// ✕ on the invitation: `Accept(id)`
    Accept(u32),
    /// △ on it: `Decline(id)`
    Decline(u32),
    /// an inventory record staked: its key and how many (sub 6)
    Add(u32, u32),
    /// an offered record taken back, by its key (sub 7)
    Remove(u32),
    /// the deposit's tunar (sub 5)
    Tunar(u32),
    /// □: the wager accepted (sub 8)
    AcceptWager,
    /// △ while accepted: withdrawn (sub 9)
    Unaccept,
    /// "Cancel duel?" Yes: `CancelDuel` (the window away, sub 10)
    CancelDuel,
    /// the chat line's text (`ChatLine`)
    Chat(String),
}

/// Which child has the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Child {
    Wnd,
    MBox,
    Deposit,
    Quantity,
    Edit,
}

pub struct DuelWnd {
    /// +0x24: -1 none, 0 the inventory, 1 the player's offer, 2 the tunar, 3
    /// the opponent's offer (`VIWndTrade`'s `m_focus`)
    pub focus: i32,
    list: WndList,
    mine: WndList,
    theirs: WndList,
    mbox: MBox,
    deposit: Deposit,
    quantity: Quantity,
    edit: EditBox,
    child: Child,
    /// +0x48, +0x1C8
    window: Window,
    offer: Window,
    /// +0x348, +0x34C: the invitation's first line shown, its lines
    scroll: i32,
    lines: i32,
    /// +0x350: the invitation shown (its id -1, none)
    pub invite: Option<Invite>,
    /// +0x478: the client's clock as the invitation last counted down
    last: i64,
    /// +0x480: the wager (1) rather than the invitation (0)
    pub wager: bool,
    pub visible: bool,
    /// the three lists' frames and the tunar where they were drawn, by the
    /// focus each is, for a pointer
    hits: Hits,
}

impl Default for DuelWnd {
    fn default() -> Self {
        let list = WndList::with_rows([6, 37], 0, 0);
        let mut mine = WndList::with_rows([155, 37], 0, 0);
        mine.x64 = false;
        let mut theirs = WndList::with_rows([295, 37], 0, 0);
        theirs.x64 = false;
        DuelWnd {
            focus: -1,
            list,
            mine,
            theirs,
            mbox: MBox::default(),
            deposit: Deposit::default(),
            quantity: Quantity::default(),
            edit: EditBox::default(),
            child: Child::Wnd,
            window: Window::default(),
            offer: Window::default(),
            scroll: 0,
            lines: 0,
            invite: None,
            last: 0,
            wager: false,
            visible: false,
            hits: Hits::default(),
        }
    }
}

fn box_key(key: i32) -> Option<BoxKey> {
    match key {
        13 | 134 => Some(BoxKey::Enter),
        27 | 132 => Some(BoxKey::Escape),
        17 | 136 => Some(BoxKey::Left),
        18 | 137 => Some(BoxKey::Right),
        _ => None,
    }
}

/// `RefreshLists` (0x013491A0): a list's rows off the part's records, as the
/// trade's (0x01349468: a stack shows its count, a worn item the arrows, one
/// the player cannot use the red ones), the rest empty.
fn fill(list: &mut WndList, items: &[NetItem], cap: usize, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool) {
    let n = items.len().min(cap);
    for (i, it) in items.iter().take(n).enumerate() {
        let mut flags = if it.quantity >= 2 { 5 } else { 1 };
        let mut mark = -1;
        if it.equip_slot != -1 {
            mark = 138;
            flags |= 8;
        }
        if it.item.wear_slot != -1 && !can_use(it) {
            mark = 140;
            flags |= 8;
        }
        list.set_item(i, flags, icon(it), mark, -1, it.quantity as i32);
    }
    for i in n..cap {
        list.set_item(i, 0, -1, -1, -1, 0);
    }
}

/// `BuildInviteText` (0x01348590): who challenges (1866 with the name), a
/// blank line, then "Guild: " (512) when there is one, "Race: " (510), "Class: "
/// (511, the title where there is one, else the class's word) and "Level: "
/// (503), a line each.
pub fn invite_text(vui: &Vui, rec: &Invite) -> String {
    let w = |id: i32| word(&vui.words, id, vui.language);
    let mut s = crate::duel::printf_s(&w(1866), &rec.name);
    s += "\n\n";
    if !rec.guild.is_empty() {
        s += &format!("{}: {}\n", w(512), String::from_utf16_lossy(&rec.guild));
    }
    s += &format!("{}: {}\n", w(510), w(crate::auctionwnd::race_word(rec.race as i32)));
    let class = if rec.title.is_empty() { w(crate::auctionwnd::class_word(rec.class as i32)) } else { String::from_utf16_lossy(&rec.title) };
    s += &format!("{}: {}\n", w(511), class);
    s += &format!("{}: {}\n", w(503), rec.level);
    s
}

impl DuelWnd {
    /// `ShowInvite` (0x01348398, `VIWndGame::ShowDuelInvite`): the record
    /// copied, the window closed (to open), the scroll at the top, the
    /// invitation half, the clock noted; sound 15.
    pub fn show_invite(&mut self, rec: Invite, clock: i64, cues: &mut Vec<i32>) {
        self.invite = Some(rec);
        self.window.close(true);
        self.scroll = 0;
        self.lines = 0;
        self.wager = false;
        self.last = clock;
        cues.push(15);
    }

    /// `OpenWager` (0x013484A8): no invitation, the inventory list lit, the
    /// offers from the top, the wager half.
    pub fn open_wager(&mut self) {
        self.invite = None;
        self.focus = 0;
        self.wager = true;
        self.list.set_x64(true);
        self.list.reset_scroll();
        self.mine.set_x64(false);
        self.mine.reset_scroll();
        self.theirs.set_x64(false);
        self.theirs.reset_scroll();
    }

    /// The focus come to the window from outside it (`OnMessage` 295, as
    /// `UpdateMode` 43 and 44 give it): the window at (320, 180) closed (to
    /// open), the scroll at the top, the clock noted, every child put away,
    /// the lists' rows made (40, 4, 4); shown.
    pub fn activate(&mut self, vui: &Vui, r: &dyn Raster, clock: i64) {
        self.window.init(vui, r, [320, 180], 350, 8, 0);
        self.window.close(true);
        self.scroll = 0;
        self.last = clock;
        self.child = Child::Wnd;
        self.deposit.visible = false;
        self.quantity.visible = false;
        self.list.init(40);
        self.mine.init(4);
        self.theirs.init(4);
        self.visible = true;
    }

    /// `CloseDuelInvite` / `CloseDuelWager`: put away.
    pub fn close(&mut self) {
        self.visible = false;
        self.child = Child::Wnd;
        self.deposit.visible = false;
        self.quantity.visible = false;
    }

    /// The chat line has the keys.
    pub fn typing(&self) -> bool {
        self.visible && self.child == Child::Edit
    }

    /// The invitation's time left, in the client's clock's ticks.
    #[cfg(test)]
    pub fn timer(&self) -> Option<u32> {
        self.invite.as_ref().map(|r| r.timer)
    }

    /// A pointer at frame point `p`: to the child with the keys; on the
    /// wager as the trade window takes it (`Trade::point`). The keys to
    /// press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.child {
            Child::MBox => return self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Child::Deposit => return self.deposit.point(p),
            Child::Quantity => return self.quantity.point(p),
            Child::Edit => return self.edit.point(p).flatten().into_iter().collect(),
            Child::Wnd => {}
        }
        if !self.wager {
            return Vec::new();
        }
        let Some(target) = self.hits.at(p) else { return Vec::new() };
        let row = match target {
            0 => self.list.row_at(p),
            1 => self.mine.row_at(p),
            3 => self.theirs.row_at(p),
            _ => None,
        };
        if target == self.focus {
            return match (target, row) {
                (2, _) => vec![key::CROSS],
                (0, Some(r)) => vec![self.list.pick(r)],
                (1, Some(r)) => vec![self.mine.pick(r)],
                (3, Some(r)) => vec![self.theirs.pick(r)],
                _ => Vec::new(),
            };
        }
        self.focus_to(target);
        if let Some(r) = row {
            let (l, rows) = match target {
                0 => (&mut self.list, 6),
                1 => (&mut self.mine, 3),
                _ => (&mut self.theirs, 3),
            };
            l.pick(r);
            if l.top + rows - 1 < r {
                l.top = r - rows + 1;
            }
            l.top = l.top.min(r);
        }
        vec![MOVED]
    }

    /// The focus moved to `target` as the d-pad moves it (`Trade::focus_to`).
    fn focus_to(&mut self, target: i32) {
        let mut cues = Vec::new();
        for _ in 0..4 {
            match (self.focus, target) {
                (f, t) if f == t => break,
                (0, _) | (1 | 2, 3) => self.right(&mut cues),
                (2, 1) => self.up(&mut cues),
                (1, 2) => {
                    self.focus = 2;
                    self.mine.set_x64(false);
                }
                (1..=3, _) => self.left(&mut cues),
                _ => break,
            }
        }
    }

    /// `OnKeyDown` (0x01347430) / `OnKeyUp` (0x01347598), or the keys of the
    /// child that has them. `purse` is the tunar carried (`VIClient+0xD440`),
    /// `clock` the client's clock.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Part, purse: i32, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<WndOut>) {
        let now = vui.time;
        match self.child {
            Child::MBox => {
                if !down {
                    return;
                }
                let Some(k) = box_key(key) else { return };
                match self.mbox.key(k, cues) {
                    // 268: CancelDuel
                    Some(BoxAnswer::Yes(..)) => {
                        self.child = Child::Wnd;
                        out.push(WndOut::CancelDuel);
                    }
                    // 260
                    Some(BoxAnswer::No(..)) => self.child = Child::Wnd,
                    None => {}
                }
            }
            Child::Deposit => match self.deposit.key(key, down, now, cues) {
                Some(DepositUp::Value(v)) => {
                    // 294
                    out.push(WndOut::Tunar(v as u32));
                    self.deposit.visible = false;
                    self.deposit.carried = 0;
                    self.child = Child::Wnd;
                }
                Some(DepositUp::Cancel) => {
                    self.deposit.visible = false;
                    self.deposit.carried = 0;
                    self.child = Child::Wnd;
                }
                _ => {}
            },
            Child::Quantity => match self.quantity.key(key, down, cues) {
                Some(QuantityUp::Value(n, id)) => {
                    // 293: staked, with sound 13
                    if self.focus == 0 && n > 0 {
                        out.push(WndOut::Add(id as u32, n as u32));
                        cues.push(13);
                    }
                    self.quantity.visible = false;
                    self.child = Child::Wnd;
                }
                Some(QuantityUp::Cancel) => {
                    self.quantity.visible = false;
                    self.child = Child::Wnd;
                }
                None => {}
            },
            Child::Edit => {
                if !down {
                    self.edit.key_up(key);
                    return;
                }
                match self.edit.key(vui, r, key, now, cues) {
                    // 259: SendChat
                    EditUp::Done(_) => {
                        out.push(WndOut::Chat(String::from_utf16_lossy(&self.edit.text)));
                        self.child = Child::Wnd;
                    }
                    EditUp::Cancel => self.child = Child::Wnd,
                    EditUp::None => {}
                }
            }
            Child::Wnd => {
                if !down {
                    // OnKeyUp: up and down to the list with the focus
                    if matches!(key, 19 | 138 | 20 | 139) {
                        match self.focus {
                            0 => self.list.key_up_down(key, false),
                            1 => self.mine.key_up_down(key, false),
                            3 => self.theirs.key_up_down(key, false),
                            _ => {}
                        }
                    }
                    return;
                }
                if self.wager {
                    match key {
                        13 | 134 => self.select(vui, r, part, purse, cues, out),
                        27 | 132 => self.cancel(vui, part, cues, out),
                        19 | 138 => self.up(cues),
                        20 | 139 => self.down(part, cues),
                        17 | 136 => self.left(cues),
                        18 | 137 => self.right(cues),
                        81 | 113 | 133 => {
                            // Accept
                            if (0..4).contains(&self.focus) {
                                cues.push(13);
                                if !part.my_acc {
                                    out.push(WndOut::AcceptWager);
                                }
                            }
                        }
                        32 | 147 => {
                            // Chat
                            self.edit.set_rows(1);
                            self.edit.set_multi_line(false);
                            self.edit.set_center([320, 180]);
                            self.edit.set_max_length(127);
                            self.edit.purpose = 0;
                            self.edit.focus(now);
                            self.child = Child::Edit;
                        }
                        _ => {}
                    }
                } else {
                    let id = self.invite.as_ref().map(|r| r.id);
                    match key {
                        13 | 134 => {
                            cues.push(13);
                            out.extend(id.map(WndOut::Accept));
                        }
                        27 | 132 => {
                            cues.push(8);
                            out.extend(id.map(WndOut::Decline));
                        }
                        19 | 138 if self.lines >= 7 => {
                            if self.scroll > 0 {
                                cues.push(11);
                                self.scroll -= 1;
                            } else {
                                cues.push(9);
                                self.scroll = 0;
                            }
                        }
                        20 | 139 if self.lines >= 7 => {
                            if self.scroll < self.lines - 6 {
                                cues.push(11);
                                self.scroll += 1;
                            } else {
                                cues.push(9);
                            }
                        }
                        _ => {}
                    }
                }
                self.window.reset_tick_pulse(now);
            }
        }
    }

    /// The wager's `Up` (0x01347BD8).
    fn up(&mut self, cues: &mut Vec<i32>) {
        match self.focus {
            0 => self.list.key_up_down(138, true),
            1 => self.mine.key_up_down(138, true),
            2 => {
                self.focus = 1;
                self.mine.set_x64(true);
                cues.push(11);
            }
            3 => self.theirs.key_up_down(138, true),
            _ => {}
        }
    }

    /// `Down` (0x01347AA8): off the bottom of the player's offer onto the
    /// tunar.
    fn down(&mut self, part: &Part, cues: &mut Vec<i32>) {
        match self.focus {
            0 => self.list.key_up_down(139, true),
            1 => {
                if self.mine.sel < part.mine.len() as i32 - 1 && self.mine.count() > 0 {
                    self.mine.key_up_down(139, true);
                } else {
                    self.focus = 2;
                    self.mine.set_x64(false);
                    cues.push(11);
                }
            }
            2 => cues.push(9),
            3 => self.theirs.key_up_down(139, true),
            _ => {}
        }
    }

    /// `Left` (0x01347CA8).
    fn left(&mut self, cues: &mut Vec<i32>) {
        match self.focus {
            1 | 2 => {
                self.focus = 0;
                self.list.set_x64(true);
                self.mine.set_x64(false);
                cues.push(11);
            }
            3 => {
                cues.push(11);
                self.focus = 1;
                self.theirs.set_x64(false);
                self.mine.set_x64(true);
            }
            _ => {}
        }
    }

    /// `Right` (0x01347D90).
    fn right(&mut self, cues: &mut Vec<i32>) {
        match self.focus {
            0 => {
                self.focus = 1;
                self.list.set_x64(false);
                self.mine.reset_scroll();
                self.mine.set_x64(true);
                cues.push(11);
            }
            1 | 2 => {
                self.focus = 3;
                self.mine.set_x64(false);
                self.theirs.set_x64(true);
                self.theirs.reset_scroll();
                cues.push(11);
            }
            _ => {}
        }
    }

    /// `Select` (0x013477C0): nothing once accepted (sound 9); an inventory
    /// record staked (a stack asks how many, by its key), an offered one
    /// taken back, the tunar entered in the deposit's mode 4 with the purse
    /// its limit; sound 13.
    fn select(&mut self, vui: &Vui, r: &dyn Raster, part: &Part, purse: i32, cues: &mut Vec<i32>, out: &mut Vec<WndOut>) {
        if part.my_acc {
            cues.push(9);
            return;
        }
        match self.focus {
            0 => {
                let i = self.list.sel;
                if let Some(it) = part.inv.get(i.max(0) as usize).filter(|_| i >= 0) {
                    if it.item.max_stack != 0 && it.quantity >= 2 {
                        self.quantity.open(vui, r, it.quantity as i32, it.inv_index as i32, -1);
                        self.child = Child::Quantity;
                    } else {
                        out.push(WndOut::Add(it.inv_index, 1));
                    }
                }
                cues.push(13);
            }
            1 => {
                let i = self.mine.sel;
                if let Some(it) = part.mine.get(i.max(0) as usize).filter(|_| i >= 0) {
                    out.push(WndOut::Remove(it.inv_index));
                }
                cues.push(13);
            }
            2 => {
                self.deposit.open_mode(vui, r, purse, 0, 4);
                cues.push(13);
                self.child = Child::Deposit;
            }
            _ => {}
        }
    }

    /// `Cancel` (0x01347958): accepted, the accept withdrawn; else "Cancel
    /// duel?" (1872) as a Yes/No box, sound 15.
    fn cancel(&mut self, vui: &Vui, part: &Part, cues: &mut Vec<i32>, out: &mut Vec<WndOut>) {
        if !(0..4).contains(&self.focus) {
            return;
        }
        if part.my_acc {
            out.push(WndOut::Unaccept);
        } else {
            cues.push(15);
            self.mbox.prompt(&wide(&word(&vui.words, 1872, vui.language)), 1, self.focus, 0, vui.time);
            self.child = Child::MBox;
        }
    }

    /// `OnDrawSelf` (0x013473E8): the invitation while there is one, else the
    /// wager in its half; then the child up. `clock` is the client's clock
    /// (VITimer's ticks), by which the invitation counts down.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Part, info: &Info, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool, clock: i64, cues: &mut Vec<i32>) {
        if !self.visible {
            return;
        }
        if self.invite.is_some() {
            self.draw_invite(vui, r, clock);
        } else if self.wager {
            self.draw_wager(vui, r, part, info, icon, can_use, cues);
        }
        let now = vui.time;
        match self.child {
            Child::MBox => {
                self.mbox.draw(vui, r, now);
            }
            Child::Deposit => self.deposit.draw(vui, r, cues),
            Child::Quantity => self.quantity.draw(vui, r, cues),
            Child::Edit => self.edit.draw(vui, r, now, cues),
            Child::Wnd => {}
        }
    }

    /// `DrawInvite` (0x013487C8).
    fn draw_invite(&mut self, vui: &Vui, r: &mut dyn Raster, clock: i64) {
        let Some(rec) = self.invite.clone() else { return };
        let text = wide(&invite_text(vui, &rec));
        let black = UI_COLORS[0];
        self.window.init(vui, r, [320, 194], 350, 8, 0);
        let title = wide(&word(&vui.words, 1864, vui.language));
        self.window.set_title(Some(&title));
        self.window.open(false);
        self.window.draw(vui, r);
        let w = self.window.size[0] - 70;
        let lines = self.window.count_lines(vui, r, &text, w);
        self.lines = lines;
        if lines < 7 {
            self.scroll = 0;
            self.window.draw_frame_vscroll(vui, r, 30, 0, w, 6, 0.0);
        } else {
            let t = self.scroll as f32 / (lines - 6) as f32;
            self.window.draw_frame_vscroll(vui, r, 30, 0, w, 6, t);
        }
        self.window.text_box(vui, r, &text, 30, 0, w, 6, black, (self.scroll as u32) << 24);
        // the seconds left, right of the legend, under ten
        if rec.timer < 10000 {
            let s = wide(&((rec.timer / 1000) & 0xFF).to_string());
            let width = self.window.size[0];
            self.window.text_box(vui, r, &s, width >> 1, 7, (width >> 1) - 40, 1, black, 0x10);
        }
        let legend = wide(&format!("|x{}   |t{}", word(&vui.words, 1779, vui.language), word(&vui.words, 157, vui.language)));
        self.window.text(vui, r, &legend, 30, 7, black, 0);
        // the time counted down by the clock since the last draw
        let dt = (clock - self.last) as i32 as u32;
        if let Some(inv) = self.invite.as_mut() {
            inv.timer = inv.timer.wrapping_sub(dt);
        }
        self.last = clock;
    }

    /// `DrawWager` (0x01348A58): the trade window's draw (`Trade::draw`) off
    /// the part, but its title (1867), its "Cancel duel?" box at (180, 145),
    /// its legend (Select, Submit, Cancel, Chat), the tunar unsigned, and an
    /// accepted side glowing with the window's pulse.
    fn draw_wager(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Part, info: &Info, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool, cues: &mut Vec<i32>) {
        let now = vui.time;
        self.hits.clear();
        self.mbox.pos = [180, 145];
        fill(&mut self.list, &part.inv, 40, icon, can_use);
        fill(&mut self.mine, &part.mine, 4, icon, can_use);
        fill(&mut self.theirs, &part.theirs, 4, icon, can_use);
        let white = [1.0; 4];
        crate::ui::window_backdrop(vui, r, 642);
        self.window.init(vui, r, [395, 183], 480, 15, 0);
        self.window.open(true);
        let w_ = |id: i32| wide(&word(&vui.words, id, vui.language));
        let banner = w_(1867);
        let w = vui.string_width(r, &banner) + 64;
        vui.blit_bar(r, &banner, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        self.offer.init(vui, r, [554, 162], 240, 14, 1);
        self.offer.open(true);
        self.offer.draw(vui, r);
        self.offer.init(vui, r, [300, 255], 310, 3, 1);
        self.offer.open(true);
        self.offer.draw(vui, r);
        self.window.text_box(vui, r, &w_(479), 30, 10, 105, 1, white, 1);
        self.window.text_box(vui, r, &wide(&part.my_tunar.to_string()), 30, 11, 105, 1, white, 5);
        self.hits.push(crate::pointer::rows_rect(&self.window, vui, r, 30, 10, 105, 2), 2);
        let focused = self.child == Child::Wnd;
        if self.focus == 2 {
            self.window.tick_r(vui, r, 20, 11, focused);
        }
        self.window.text_box(vui, r, &w_(479), 170, 10, 105, 1, white, 1);
        self.window.text_box(vui, r, &wide(&part.their_tunar.to_string()), 170, 11, 105, 1, white, 5);
        let pick = |items: &[NetItem], sel: i32| items.get(sel.max(0) as usize).filter(|_| sel >= 0).map(|it| info.trade_item(it));
        let desc = match self.focus {
            0 => pick(&part.inv, self.list.sel),
            1 | 2 => pick(&part.mine, self.mine.sel),
            3 => pick(&part.theirs, self.theirs.sel),
            _ => None,
        }
        .unwrap_or_default();
        self.window.text_box(vui, r, &wide(&desc), 295, -1, 170, 14, white, 3);
        let legend = wide(&format!("|x{}  |s{}  |t{}  |({}", word(&vui.words, 212, vui.language), word(&vui.words, 218, vui.language), word(&vui.words, 213, vui.language), word(&vui.words, 424, vui.language)));
        let w2 = vui.string_width(r, &legend);
        vui.blit_bar(r, &[], white, [165, 290], 25, 26, w2 + 64, 0, 1.0, 0, true);
        self.window.text_box(vui, r, &legend, 25, 14, w2 + 34, 1, UI_COLORS[0], 0);
        let glow = |on: bool| if on { [0.5, 1.0, 0.5, self.window.tick_pulse(now)] } else { white };
        let (mc, tc) = (glow(part.my_acc), glow(part.their_acc));
        self.list.x84 = focused;
        self.mine.x84 = focused;
        self.theirs.x84 = focused;
        // the lists' frames, as `VIWndList::OnDrawSelf` and
        // `VIWndTradeList::OnDrawSelf` size them
        let (stud, fill) = (vui.entry(36), vui.entry(82));
        let lw = stud.uv1[0] * 2 + fill.uv1[0] + 114;
        for (l, h, k) in [(&self.list, 264, 0), (&self.mine, 3 * 44 + 30, 1), (&self.theirs, 3 * 44 + 30, 3)] {
            self.hits.push([l.pos[0], l.pos[1], l.pos[0] + lw, l.pos[1] + stud.uv1[1] * 2 + h], k);
        }
        self.list.draw(vui, r, now, cues);
        self.mine.draw_trade(vui, r, 3, 474, mc, now, cues);
        self.theirs.draw_trade(vui, r, 3, 475, tc, now, cues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn it(id: i64, key: u32, qty: i64, max_stack: i64) -> NetItem {
        let mut n = NetItem { quantity: qty, inv_index: key, equip_slot: -1, quick_slot: -1, ..Default::default() };
        n.item.id = id;
        n.item.max_stack = max_stack;
        n.item.wear_slot = -1;
        n
    }

    fn rec(id: u32) -> Invite {
        Invite { id, race: 6, class: 0, level: 40, name: "Lanerender".into(), guild: Vec::new(), title: Vec::new(), timer: crate::duel::INVITE_TIME }
    }

    /// The invitation's text (`BuildInviteText`): the challenge, a blank
    /// line, the guild only when there is one, the race's and the class's
    /// words (the title in the class's place when there is one), the level.
    #[test]
    fn the_invitation_says_who_challenges() {
        // a table whose row N says wN, up to the disc's 1,814 rows
        let mut v = vui();
        let rows: Vec<String> = (1..=1814).map(|n| format!("w{n}")).collect();
        v.words = vi::strtab::StringTable::parse(rows.join("\n").as_bytes()).unwrap();
        let t = invite_text(&v, &rec(1));
        assert_eq!(t, "Lanerender challenges you to a duel.\n\nw510: w191\nw511: w165\nw503: 40\n", "a barbarian warrior, no guild");
        let mut r = rec(1);
        r.guild = "Ours".encode_utf16().collect();
        r.title = "Warlord".encode_utf16().collect();
        let t = invite_text(&v, &r);
        assert_eq!(t, "Lanerender challenges you to a duel.\n\nw512: Ours\nw510: w191\nw511: Warlord\nw503: 40\n", "the guild, and the title for the class");
    }

    /// The invitation's keys (`OnKeyDown`'s invitation half): ✕ accepts (13),
    /// △ declines (8), each with the invitation's id; up and down scroll only
    /// with seven lines or more, 11 a step, 9 at an end. The controls: with no
    /// invitation shown ✕ and △ ask nothing, and short text never scrolls.
    #[test]
    fn the_invitation_takes_its_keys() {
        let (v, mut r) = (vui(), NullRaster);
        let part = Part::default();
        let mut w = DuelWnd::default();
        let mut cues = Vec::new();
        w.show_invite(rec(0x21), 0, &mut cues);
        w.activate(&v, &r, 0);
        assert_eq!(cues, vec![15]);
        let press = |w: &mut DuelWnd, key: i32, r: &mut NullRaster| {
            let (mut cues, mut out) = (Vec::new(), Vec::new());
            w.key(&v, r, &part, 0, key, true, &mut cues, &mut out);
            (cues, out)
        };
        assert_eq!(press(&mut w, 134, &mut r), (vec![13], vec![WndOut::Accept(0x21)]));
        assert_eq!(press(&mut w, 27, &mut r), (vec![8], vec![WndOut::Decline(0x21)]));
        w.lines = 4;
        assert_eq!(press(&mut w, 139, &mut r), (vec![], vec![]), "six lines fit: no scroll");
        w.lines = 9;
        assert_eq!(press(&mut w, 139, &mut r).0, vec![11]);
        assert_eq!(press(&mut w, 139, &mut r).0, vec![11]);
        assert_eq!(press(&mut w, 139, &mut r).0, vec![11]);
        assert_eq!((press(&mut w, 139, &mut r).0, w.scroll), (vec![9], 3), "at the bottom: lines - 6");
        assert_eq!(press(&mut w, 138, &mut r).0, vec![11]);
        w.scroll = 0;
        assert_eq!(press(&mut w, 138, &mut r).0, vec![9], "at the top");
        w.invite = None;
        assert_eq!(press(&mut w, 134, &mut r), (vec![13], vec![]), "no invitation: nothing asked");
    }

    /// The invitation counts down by the clock as it draws; the seconds show
    /// only under ten. The window grows open over its frames (`Open(false)`
    /// after the 295's `Close(true)`).
    #[test]
    fn the_invitation_counts_down_as_it_draws() {
        let (v, mut r) = (vui(), NullRaster);
        let part = Part::default();
        let inv = net::dump::ItemList::default();
        let base = [0; 29];
        let info = Info { words: &v.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let mut w = DuelWnd::default();
        let mut cues = Vec::new();
        w.show_invite(rec(1), 1000, &mut cues);
        w.activate(&v, &r, 1000);
        let (icon, can_use) = (|_: &NetItem| 1, |_: &NetItem| true);
        w.draw(&v, &mut r, &part, &info, &icon, &can_use, 1000, &mut cues);
        assert_eq!(w.timer(), Some(20000));
        assert!(w.window.open > 0.0 && w.window.open < 1.0, "opening: {}", w.window.open);
        w.draw(&v, &mut r, &part, &info, &icon, &can_use, 11500, &mut cues);
        assert_eq!(w.timer(), Some(9500));
        for _ in 0..8 {
            w.draw(&v, &mut r, &part, &info, &icon, &can_use, 11500, &mut cues);
        }
        assert!(w.window.is_open());
    }

    /// The wager's keys: ✕ on an inventory record stakes it by its key (a
    /// stack asks how many first, the answer staked with sound 13), on the
    /// offer takes it back, on the tunar opens the deposit in mode 4 whose OK
    /// stakes the tunar; □ accepts (13) unless accepted; △ withdraws an accept,
    /// else asks "Cancel duel?" whose Yes calls the duel off. Once accepted ✕
    /// does nothing but sound 9 (the control for the stake).
    #[test]
    fn the_wager_takes_its_keys() {
        let (v, mut r) = (vui(), NullRaster);
        let mut part = Part { inv: vec![it(100, 3, 5, 20), it(200, 4, 1, 1)], mine: vec![it(300, 9, 1, 1)], ..Default::default() };
        let mut w = DuelWnd::default();
        w.open_wager();
        w.activate(&v, &r, 0);
        let mut press = |w: &mut DuelWnd, part: &Part, key: i32| {
            let (mut cues, mut out) = (Vec::new(), Vec::new());
            w.key(&v, &mut r, part, 750, key, true, &mut cues, &mut out);
            w.key(&v, &mut r, part, 750, key, false, &mut cues, &mut Vec::new());
            (cues, out)
        };
        // the second record: one, staked at once by its key
        w.list.sel = 1;
        assert_eq!(press(&mut w, &part, 134), (vec![13], vec![WndOut::Add(4, 1)]));
        // the stack: the quantity first
        w.list.sel = 0;
        assert_eq!(press(&mut w, &part, 134), (vec![13], vec![]));
        assert_eq!(w.child, Child::Quantity);
        let (_, out) = press(&mut w, &part, 134);
        assert!(matches!(out[..], [WndOut::Add(3, _)]), "{out:?}");
        assert_eq!(w.child, Child::Wnd);
        // the offer: taken back by its key
        press(&mut w, &part, 137);
        assert_eq!(w.focus, 1);
        assert_eq!(press(&mut w, &part, 134).1, vec![WndOut::Remove(9)]);
        // the tunar: the deposit, mode 4, the purse its limit
        press(&mut w, &part, 139);
        assert_eq!(w.focus, 2);
        press(&mut w, &part, 134);
        assert_eq!((w.child, w.deposit.mode, w.deposit.carried), (Child::Deposit, 4, 750));
        w.deposit.digits[8] = 5;
        assert_eq!(press(&mut w, &part, 134).1, vec![WndOut::Tunar(5)]);
        assert_eq!(w.child, Child::Wnd);
        // □ accepts; △ asks, Yes calls it off
        assert_eq!(press(&mut w, &part, 133), (vec![13], vec![WndOut::AcceptWager]));
        assert_eq!(press(&mut w, &part, 132).0, vec![15]);
        assert_eq!(w.child, Child::MBox);
        press(&mut w, &part, 136);
        assert_eq!(press(&mut w, &part, 134).1, vec![WndOut::CancelDuel]);
        // accepted: ✕ nothing but 9, □ only its sound, △ withdraws
        part.my_acc = true;
        assert_eq!(press(&mut w, &part, 134), (vec![9], vec![]));
        assert_eq!(press(&mut w, &part, 133), (vec![13], vec![]));
        assert_eq!(press(&mut w, &part, 27).1, vec![WndOut::Unaccept]);
        // R3: the chat line, 127 long
        press(&mut w, &part, 147);
        assert!(w.typing());
    }

    /// A pointer on the wager as on the trade window: another list takes the
    /// focus and the row; the list with the focus uses it. The control: on
    /// the invitation the window's own rows take no clicks.
    #[test]
    fn a_pointer_on_the_wager() {
        let (v, mut r) = (vui(), NullRaster);
        let part = Part { inv: vec![it(100, 3, 1, 1), it(200, 4, 1, 1)], ..Default::default() };
        let inv = net::dump::ItemList::default();
        let base = [0; 29];
        let info = Info { words: &v.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let (icon, can_use) = (|_: &NetItem| 1, |_: &NetItem| true);
        let mut cues = Vec::new();
        let mut w = DuelWnd::default();
        w.open_wager();
        w.activate(&v, &r, 0);
        w.draw(&v, &mut r, &part, &info, &icon, &can_use, 0, &mut cues);
        let inv_row = |i: i32| [6.0 + 57.0, 37.0 + 44.0 * i as f32 + 22.0];
        assert_eq!(w.point(inv_row(1)), vec![MOVED]);
        assert_eq!(w.list.sel, 1);
        assert_eq!(w.point(inv_row(1)), vec![key::CROSS]);
        let money = crate::pointer::testing::mid(crate::pointer::rows_rect(&w.window, &v, &r, 30, 10, 105, 2));
        assert_eq!(w.point(money), vec![MOVED]);
        assert_eq!(w.focus, 2);
        let mut i = DuelWnd::default();
        i.show_invite(rec(1), 0, &mut cues);
        i.activate(&v, &r, 0);
        i.draw(&v, &mut r, &part, &info, &icon, &can_use, 0, &mut cues);
        assert!(i.point(inv_row(0)).is_empty());
    }
}
