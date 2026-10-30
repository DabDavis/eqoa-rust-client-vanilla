//! Customer service: the request form, the inbox and the reader.
//!
//! `CsClient` is `VICSClient` (`vicsclient.cpp`, `vicsrecords.cpp`; VIClient's
//! `m_p33950`): a request goes out as message 0x0AA1, a sub-op byte, then its
//! fields; a reply comes back on 0x07B0, a sub-op byte and a status byte, then
//! its fields. Sub-ops: 0 a new request, 1 a reply to a message, 2 the inbox,
//! 3 a message's text, 4 delete, 5 (in) something new, 6 the categories.
//! `m_state` is the reply waited for, and the server's status as an error
//! code (`StatusToError`) when it fails.
//!
//! The windows: `VIWndCSRequest` (`Request`; Support's first entry, the game
//! window's mode 31, and the reader's reply), `VIWndCSInbox` (`Inbox`, mode
//! 32) and `VIWndReadCS` (`Read`, the inbox's reader). Each draws a status
//! window while the client waits, and acts when it is free again on what it
//! saw it wait for.
//!
//! A pointer (`pointer`) goes where the keys go: the editor's soft keyboard,
//! the box's buttons, a category picker's line (taken at once; off the
//! picker, △ closes it), the reader's option list; else a field of the form
//! or a row of the inbox is selected and the selected one used (✕). Nothing
//! a pad cannot do: not while the client waits, not a reply's category.

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::gram::{Date, Dots, box_key, capped, edit_field, field_click, word};
use crate::pointer::{Hits, key, rows_rect};
use crate::wholist::OptionList;
use net::stream::{Reader, Writer};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VICSMessageHeader`: an inbox row. The id and the two category ids go
/// raw, the date as `VIDate`, the flag as a byte.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub id: u32,
    pub date: Date,
    pub category: i32,
    pub subcategory: i32,
    /// cleared when the text arrives
    pub unread: u8,
}

impl Header {
    fn read(r: &mut Reader) -> Self {
        let id = r.u32();
        let date = Date::read(r);
        let category = r.u32() as i32;
        let subcategory = r.u32() as i32;
        let unread = r.u8();
        Header { id, date, category, subcategory, unread }
    }

    /// `operator<<`: the server's half.
    #[cfg(test)]
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.id);
        self.date.write(w);
        w.u32(self.category as u32).u32(self.subcategory as u32).u8(self.unread);
    }
}

/// `VICSSubcategory`: a zigzag id, a hidden flag (a byte), a wide name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subcategory {
    pub id: i32,
    pub hidden: bool,
    pub name: Vec<u16>,
}

/// `VICSCategory`: a subcategory's three fields and its subcategories.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Category {
    pub id: i32,
    pub hidden: bool,
    pub name: Vec<u16>,
    pub subs: Vec<Subcategory>,
}

/// `VIMemStreamVector.h`'s reader: a zigzag count, then each.
fn read_vector<T>(r: &mut Reader, mut one: impl FnMut(&mut Reader) -> T) -> Vec<T> {
    let n = r.signed();
    let mut v = Vec::new();
    for _ in 0..n.max(0) {
        if r.error {
            break;
        }
        v.push(one(r));
    }
    v
}

fn read_sub(r: &mut Reader) -> Subcategory {
    let id = r.signed() as i32;
    let hidden = r.u8() != 0;
    let name = wide(&r.wide(64));
    Subcategory { id, hidden, name }
}

fn read_category(r: &mut Reader) -> Category {
    let id = r.signed() as i32;
    let hidden = r.u8() != 0;
    let name = wide(&r.wide(64));
    let subs = read_vector(r, read_sub);
    Category { id, hidden, name, subs }
}

/// `VICSClient::StatusToError`.
pub fn status_to_error(status: u8) -> i32 {
    match status {
        0 => 0,
        1..=11 => -(status as i32),
        15 => -12,
        16 => -13,
        17 => -14,
        18 => -15,
        19 => -16,
        12 => -17,
        13 => -18,
        14 => -19,
        _ => -128,
    }
}

/// `VICSClient`. What it sends waits in `out`.
#[derive(Debug)]
pub struct CsClient {
    pub state: i32,
    /// `m_x08`: set by sub-op 5, cleared by an inbox
    pub x08: i32,
    /// -1 until the first inbox; the list's count, though 100 rows at most
    /// are kept
    pub count: i32,
    pub headers: Vec<Header>,
    /// `m_message`: the message opened (its row), and its text
    pub message: Header,
    pub text: Vec<u16>,
    pub categories: Vec<Category>,
    pub out: Vec<Vec<u8>>,
}

impl Default for CsClient {
    fn default() -> Self {
        CsClient { state: 0, x08: 0, count: -1, headers: Vec::new(), message: Header::default(), text: Vec::new(), categories: Vec::new(), out: Vec::new() }
    }
}

fn request(sub: u8) -> Writer {
    let mut w = Writer::op(0x0AA1);
    w.u8(sub);
    w
}

impl CsClient {
    /// `GetHeader(i)`; past those kept, an empty one.
    pub fn header(&self, i: i32) -> Header {
        self.headers.get(i.max(0) as usize).filter(|_| i >= 0).cloned().unwrap_or_default()
    }

    /// `Find(id)`: the count when it is not listed.
    pub fn find(&self, id: u32) -> i32 {
        let n = self.count.max(0);
        (0..n).find(|&i| self.header(i).id == id).unwrap_or(n)
    }

    /// `OnConnect`: ask again for what was waited for.
    pub fn on_connect(&mut self) {
        match self.state {
            1 => self.request_inbox(),
            2 => self.request_message(self.message.id),
            4 => {
                self.remove_entry(self.message.id);
                self.state = 0;
            }
            3 | 5 => self.state = 0,
            _ => {}
        }
    }

    /// Sub-op 2.
    pub fn request_inbox(&mut self) {
        self.state = 1;
        self.out.push(request(2).data);
    }

    /// Sub-op 6.
    pub fn request_categories(&mut self) {
        self.state = 6;
        self.out.push(request(6).data);
    }

    /// `m_message = m_headers[Find(id)]`: the row, its text emptied.
    fn take_message(&mut self, id: u32) {
        self.message = self.header(self.find(id));
        self.text.clear();
    }

    /// Sub-op 3: the row copied, the id (zigzag) asked for.
    pub fn request_message(&mut self, id: u32) {
        self.take_message(id);
        let mut w = request(3);
        w.signed(id as i32 as i64);
        self.out.push(w.data);
        self.state = 2;
    }

    /// Sub-op 4: the row goes only when the server says so.
    pub fn delete_message(&mut self, id: u32) {
        self.take_message(id);
        let mut w = request(4);
        w.signed(id as i32 as i64);
        self.out.push(w.data);
        self.state = 4;
    }

    /// Sub-op 0: the category and subcategory (zigzag), the text (wide).
    pub fn submit_request(&mut self, category: i32, subcategory: i32, text: &[u16]) {
        self.state = 3;
        let mut w = request(0);
        w.signed(category as i64).signed(subcategory as i64).wide(&String::from_utf16_lossy(text));
        self.out.push(w.data);
    }

    /// Sub-op 1: the message answered (zigzag) and the text.
    pub fn submit_reply(&mut self, id: u32, text: &[u16]) {
        self.state = 5;
        self.take_message(id);
        let mut w = request(1);
        w.signed(id as i32 as i64).wide(&String::from_utf16_lossy(text));
        self.out.push(w.data);
    }

    /// `Recv07B0`: the reply's sub-op picks the handler, which acts only
    /// when it was waited for.
    pub fn recv(&mut self, data: &[u8]) {
        let mut r = Reader::new(data);
        let op = r.u8();
        if r.error {
            return;
        }
        match op {
            2 => self.recv_inbox(&mut r),
            3 => self.recv_message(&mut r),
            0 => self.recv_status(&mut r, 3),
            1 => {
                // a reply sent, the message is fetched again
                if self.state == 5 {
                    self.state = status_to_error(r.u8());
                    if self.state == 0 {
                        let id = self.message.id;
                        self.request_message(id);
                    }
                }
            }
            4 => self.recv_delete(&mut r),
            5 => self.x08 = 1,
            6
                if self.state == 6 => {
                    self.state = status_to_error(r.u8());
                    if self.state == 0 {
                        self.categories = read_vector(&mut r, read_category);
                    }
                }
            _ => {}
        }
    }

    fn recv_status(&mut self, r: &mut Reader, waited: i32) {
        if self.state != waited {
            return;
        }
        self.state = status_to_error(r.u8());
    }

    fn recv_inbox(&mut self, r: &mut Reader) {
        if self.state != 1 {
            return;
        }
        self.state = status_to_error(r.u8());
        if self.state != 0 {
            return;
        }
        let _x = r.u32();
        let list = read_vector(r, Header::read);
        for (i, h) in list.iter().take(100).enumerate() {
            if i < self.headers.len() {
                self.headers[i] = h.clone();
            } else {
                self.headers.push(h.clone());
            }
        }
        self.x08 = 0;
        self.count = list.len() as i32;
    }

    fn recv_message(&mut self, r: &mut Reader) {
        if self.state != 2 {
            return;
        }
        self.state = status_to_error(r.u8());
        if self.state != 0 {
            return;
        }
        self.text = wide(&r.wide(8192));
        let i = self.find(self.message.id);
        if let Some(h) = self.headers.get_mut(i as usize) {
            h.unread = 0;
        }
    }

    fn recv_delete(&mut self, r: &mut Reader) {
        if self.state != 4 {
            return;
        }
        let status = r.u8();
        let id = r.signed() as i32 as u32;
        self.state = status_to_error(status);
        if self.state == 0 {
            self.remove_entry(id);
        }
    }

    /// `RemoveEntry`: the rows after it move up; the count drops even when
    /// the id is not listed.
    fn remove_entry(&mut self, id: u32) {
        let mut i = self.find(id);
        self.count -= 1;
        while i < self.count {
            let next = self.header(i + 1);
            if let Some(h) = self.headers.get_mut(i as usize) {
                *h = next;
            }
            i += 1;
        }
    }
}

/// A failure's text: word 865 and the code, `"%S. (%S: %i)"` in 512 bytes;
/// for -8, as SOE's final live client has it (its three CS windows,
/// `VIWndCSInbox::OnDrawSelf` 0x0133FB8C and the request's and reader's
/// alike), a new line and 1888: the petition has been addressed.
fn error_text(vui: &Vui, state: i32) -> Vec<u16> {
    let mut s = format!("{}. ({}: {})", word(vui, 865), word(vui, 819), state);
    if state == -8 {
        s.push('\n');
        s.push_str(&crate::livewords::word(&vui.words, 1888, vui.language));
    }
    wide(&s).into_iter().take(255).collect()
}

/// The inbox's first column header as SOE's final live client has it
/// (0x0133F354): `"%S"` and its word 841, "Ticket #", which reads the same
/// as the disc's `"%S #"` and "Ticket" did; with a table of its own 841 the
/// table's.
fn ticket_header(vui: &Vui) -> String {
    crate::livewords::live(&vui.words, 841, vui.language)
}

/// The status window over a CS window: while it waits, the line with its
/// dots (title 271); a failure, `error_text` (title 1012).
fn draw_status(vui: &Vui, r: &mut dyn Raster, status: &mut Window, state: i32, line: Option<Vec<u16>>) {
    let back = wide(&format!("|t{}", word(vui, 161)));
    let black = UI_COLORS[0];
    match line {
        Some(line) => {
            status.set_title(Some(&wide(&word(vui, 271))));
            status.draw(vui, r);
            let w = status.size[0];
            status.text_box(vui, r, &line, 40, 1, w - 60, 1, black, 0);
            status.text_box(vui, r, &back, 40, 3, w - 60, 1, black, 0);
        }
        None => {
            let s = error_text(vui, state);
            status.set_title(Some(&wide(&word(vui, 1012))));
            status.draw(vui, r);
            let w = status.size[0];
            status.text_box(vui, r, &s, 40, 1, w - 60, 2, black, 0);
            status.text_box(vui, r, &back, 40, 3, w - 60, 1, black, 0);
        }
    }
}

/// The waiting line for a state 1-6 (`words` in that order), or `None`.
fn waiting(dots: &Dots, vui: &Vui, state: i32, words: [i32; 6]) -> Option<Vec<u16>> {
    (1..=6).contains(&state).then(|| dots.line(vui, words[state as usize - 1]))
}

/// The category with this id; the last one looked at (the last of all) if
/// none (`FindCategory`).
fn find_category(cats: &[Category], id: i32) -> Option<&Category> {
    cats.iter().find(|c| c.id == id).or(cats.last())
}

/// What the request form asks of its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequestUp {
    /// `Cancel`: a reply back to the reader (260), a request closes the panel
    /// (`ClosePanel6FC`, back to the start menu)
    Close,
    /// sent: `ShowMsg(864)`, sound 8, then as `Close` (a reply's 309)
    Sent,
    /// `CheckFull`: the inbox holds 100; word 874 in red, the panel closed
    Full,
}

/// `VIWndCSRequest`: Category, Subcategory, the text and Submit, each picked
/// from the client's lists (the hidden ones left out); a reply starts on the
/// text and keeps the message's category.
pub struct Request {
    pub reply: bool,
    pub visible: bool,
    pub sel: i32,
    cat_sel: i32,
    sub_sel: i32,
    picking: bool,
    picking_sub: bool,
    /// the picker's top row
    x4cc: i32,
    pub category: Vec<u16>,
    pub subcategory: Vec<u16>,
    pub text: Vec<u16>,
    pub category_id: i32,
    pub subcategory_id: i32,
    pub reply_to: i32,
    cat_ids: Vec<i32>,
    sub_ids: Vec<i32>,
    /// a send seen going / categories seen coming / an inbox seen coming
    x4d0: bool,
    x4d4: bool,
    x4d8: bool,
    window: Window,
    list: Window,
    status: Window,
    pub edit: EditBox,
    pub editing: bool,
    mbox: MBox,
    mbox_up: bool,
    dots: Dots,
    /// the fields drawn (Category, Subcategory, the text, Submit) and the
    /// picker's (its window -1, its lines by index), for a pointer; the
    /// frame's time; the client busy (the fields take no keys then)
    hits: Hits,
    pick_hits: Hits,
    time: i64,
    busy: bool,
}

impl Request {
    pub fn new(reply: bool) -> Self {
        let mut mbox = MBox::default();
        mbox.pos = [100, 100];
        Request {
            reply,
            visible: false,
            sel: 0,
            cat_sel: 0,
            sub_sel: 0,
            picking: false,
            picking_sub: false,
            x4cc: 0,
            category: Vec::new(),
            subcategory: Vec::new(),
            text: Vec::new(),
            category_id: 0,
            subcategory_id: 0,
            reply_to: -1,
            cat_ids: Vec::new(),
            sub_ids: Vec::new(),
            x4d0: false,
            x4d4: false,
            x4d8: false,
            window: Window::default(),
            list: Window::default(),
            status: Window::default(),
            edit: EditBox::default(),
            editing: false,
            mbox,
            mbox_up: false,
            dots: Dots::default(),
            hits: Hits::default(),
            pick_hits: Hits::default(),
            time: 0,
            busy: false,
        }
    }

    /// A pointer at frame point `p`, where the keys go: the editor's soft
    /// keyboard, the box's buttons; a picker open, its line taken at once
    /// (off the picker, △ closes it); else, the client free, a field the
    /// form can reach selected as up and down select it (a reply's starts
    /// at its text), and the selected one used (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.editing {
            return self.edit.point(p).flatten().into_iter().collect();
        }
        if self.mbox_up {
            return self.mbox.point(p).then_some(key::CROSS).into_iter().collect();
        }
        if self.picking || self.picking_sub {
            // a picker still opening takes no keys
            if !self.list.is_open() {
                return Vec::new();
            }
            return match self.pick_hits.at(p) {
                Some(-1) => Vec::new(),
                Some(i) => {
                    if self.picking {
                        self.cat_sel = i;
                    } else {
                        self.sub_sel = i;
                    }
                    vec![key::CROSS]
                }
                None => vec![key::TRIANGLE],
            };
        }
        if self.busy {
            return Vec::new();
        }
        let first = if self.reply { 2 } else { 0 };
        field_click(&mut self.sel, &mut self.window, self.time, self.hits.at(p).filter(|&f| f >= first))
    }

    /// 295, the focus from elsewhere: the windows, and everything emptied.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 500, 14, 0);
        self.window.set_title(Some(&wide(&word(vui, if self.reply { 867 } else { 868 }))));
        self.window.open(true);
        self.list.init(vui, r, [320, 180], 330, 10, 0);
        self.list.close(true);
        self.status.init(vui, r, [320, 180], 375, 4, 0);
        self.status.set_title(Some(&wide(&word(vui, 271))));
        self.status.open(true);
        self.mbox_up = false;
        self.editing = false;
        self.cat_sel = 0;
        self.sub_sel = 0;
        self.sel = if self.reply { 2 } else { 0 };
        self.category.clear();
        self.subcategory.clear();
        self.text.clear();
        self.picking = false;
        self.picking_sub = false;
        self.cat_ids.clear();
        self.sub_ids.clear();
        self.x4d0 = false;
        self.x4d4 = false;
        self.x4d8 = false;
    }

    /// `Refresh`: back to the first field; a new request asks for the lists.
    pub fn refresh(&mut self, cs: &mut CsClient) {
        self.sel = if self.reply { 2 } else { 0 };
        if !self.reply && cs.state <= 0 {
            cs.request_categories();
        }
    }

    /// The reader's reply: the message's category and ids, an empty text,
    /// the message answered.
    pub fn set_reply(&mut self, category: &[u16], subcategory: &[u16], ids: (i32, i32), to: u32) {
        self.category = capped(category, 64);
        self.subcategory = capped(subcategory, 64);
        self.category_id = ids.0;
        self.subcategory_id = ids.1;
        self.text.clear();
        self.reply_to = if self.reply { to as i32 } else { -1 };
    }

    pub fn typing(&self) -> bool {
        self.editing
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &mut CsClient, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<RequestUp> {
        if self.editing {
            if !down {
                self.edit.key_up(key);
                return None;
            }
            match self.edit.key(vui, r, key, now, cues) {
                // 259: TakeText (only the text field has an editor)
                EditUp::Done(_) => {
                    if self.sel == 2 {
                        self.text = capped(&self.edit.text, 512);
                    }
                    self.editing = false;
                }
                EditUp::Cancel => self.editing = false,
                EditUp::None => {}
            }
            return None;
        }
        if !down {
            return None;
        }
        if self.mbox_up {
            match box_key(key).and_then(|k| self.mbox.key(k, cues)) {
                // 268: p1 1 leaves (Cancel), 0 closes the box
                Some(BoxAnswer::Yes(1, _)) => {
                    self.mbox_up = false;
                    return Some(self.cancel(cues));
                }
                Some(_) => self.mbox_up = false,
                None => {}
            }
            return None;
        }
        let state = cs.state;
        let cat_count = self.cat_ids.len() as i32;
        let sub_count = self.sub_ids.len() as i32;
        if self.picking && self.list.is_open() {
            match key {
                19 | 138 => {
                    self.cat_sel = (self.cat_sel - 1).max(0);
                    cues.push(11);
                }
                20 | 139 => {
                    self.cat_sel = (self.cat_sel + 1).min(cat_count - 1);
                    cues.push(11);
                }
                13 | 134 => self.pick_category(vui, cs, cues),
                27 | 132 => {
                    self.list.close(true);
                    self.picking = false;
                    cues.push(8);
                }
                _ => {}
            }
        } else if self.picking_sub && self.list.is_open() {
            match key {
                19 | 138 => {
                    self.sub_sel = (self.sub_sel - 1).max(0);
                    cues.push(11);
                }
                20 | 139 => {
                    self.sub_sel = (self.sub_sel + 1).min(sub_count - 1);
                    cues.push(11);
                }
                13 | 134 => self.pick_subcategory(cs, cues),
                27 | 132 => {
                    self.list.close(true);
                    self.picking_sub = false;
                    cues.push(8);
                }
                _ => {}
            }
        } else if !self.picking && !self.picking_sub {
            match key {
                19 | 138 if state == 0 => self.prev(now, cues),
                20 | 139 if state == 0 => self.next(now, cues),
                13 | 134 if state == 0 => self.select(vui, cs, now, cues),
                27 | 132 => {
                    if self.text.is_empty() {
                        return Some(self.cancel(cues));
                    }
                    self.show_message(&wide(&word(vui, 866)), 1, 1, now);
                }
                _ => {}
            }
        }
        None
    }

    fn next(&mut self, now: i64, cues: &mut Vec<i32>) {
        if self.sel < 3 {
            self.sel += 1;
            cues.push(11);
        } else {
            cues.push(9);
        }
        self.window.reset_tick_pulse(now);
    }

    fn prev(&mut self, now: i64, cues: &mut Vec<i32>) {
        let first = if self.reply { 2 } else { 0 };
        if first < self.sel {
            self.sel -= 1;
            cues.push(11);
        } else {
            cues.push(9);
        }
        self.window.reset_tick_pulse(now);
    }

    /// `ShowMessage(text, prompt, a, b)`: the box over the form.
    fn show_message(&mut self, text: &[u16], prompt: i32, a: i32, now: i64) {
        self.mbox.prompt(text, prompt, a, 0, now);
        self.mbox.pos = [100, 100];
        self.mbox_up = true;
    }

    /// `Select`: the field's picker or editor, or the submit.
    fn select(&mut self, vui: &Vui, cs: &mut CsClient, now: i64, cues: &mut Vec<i32>) {
        let mut sound = 13;
        let state = cs.state;
        match self.sel {
            0 => {
                if !self.reply && state != -19 {
                    self.picking = true;
                    self.list.set_title(Some(&wide(&word(vui, 869))));
                    self.list.open(false);
                    self.cat_sel = 0;
                    self.x4cc = 0;
                } else {
                    sound = 10;
                }
            }
            1 => {
                if !self.reply && state != -19 {
                    if self.category.is_empty() {
                        sound = 10;
                        self.show_message(&wide(&word(vui, 870)), 0, 0, now);
                    } else {
                        self.picking_sub = true;
                        self.list.set_title(Some(&wide(&word(vui, 871))));
                        self.list.open(false);
                        self.sub_sel = 0;
                        self.x4cc = 0;
                    }
                } else {
                    sound = 10;
                }
            }
            2 => {
                let text = self.text.clone();
                edit_field(&mut self.edit, 4, true, 512, self.window.center, &text, now);
                self.editing = true;
            }
            _ => {
                if self.category.is_empty() {
                    sound = 10;
                    self.show_message(&wide(&word(vui, 870)), 0, 0, now);
                } else if self.subcategory.is_empty() {
                    sound = 10;
                    self.show_message(&wide(&word(vui, 872)), 0, 0, now);
                } else if self.text.is_empty() {
                    sound = 10;
                    self.show_message(&wide(&word(vui, 873)), 0, 0, now);
                } else if self.reply {
                    cs.submit_reply(self.reply_to as u32, &self.text);
                } else {
                    cs.submit_request(self.category_id, self.subcategory_id, &self.text);
                }
            }
        }
        cues.push(sound);
    }

    /// `Cancel`.
    fn cancel(&mut self, cues: &mut Vec<i32>) -> RequestUp {
        cues.push(8);
        RequestUp::Close
    }

    /// `PickCategory`: the category under the cursor becomes the request's,
    /// and the subcategory picker opens.
    fn pick_category(&mut self, vui: &Vui, cs: &CsClient, cues: &mut Vec<i32>) {
        let want = self.cat_ids.get(self.cat_sel.max(0) as usize).copied();
        match (cs.categories.iter().find(|c| Some(c.id) == want), want) {
            (Some(c), Some(id)) => {
                self.category = capped(&c.name, 64);
                self.category_id = id;
            }
            _ => self.category.clear(),
        }
        cues.push(13);
        self.list.close(true);
        self.picking = false;
        self.picking_sub = true;
        self.list.set_title(Some(&wide(&word(vui, 871))));
        self.list.open(false);
        self.sub_sel = 0;
        self.x4cc = 0;
        self.subcategory_id = 0;
        self.subcategory.clear();
    }

    /// `PickSubcategory`: the subcategory under the cursor, in the picked
    /// category, becomes the request's; on to the text.
    fn pick_subcategory(&mut self, cs: &CsClient, cues: &mut Vec<i32>) {
        self.subcategory.clear();
        self.subcategory_id = -1;
        let want = self.sub_ids.get(self.sub_sel.max(0) as usize).copied();
        if let Some(c) = cs.categories.iter().find(|c| c.id == self.category_id)
            && let Some(s) = c.subs.iter().find(|s| Some(s.id) == want)
        {
            self.subcategory = capped(&s.name, 64);
            self.subcategory_id = s.id;
        }
        self.sel = 2;
        cues.push(13);
        self.list.close(true);
        self.picking_sub = false;
    }

    /// The picker's window, re-placed and drawn every frame; true when its
    /// rows are drawn (open, and the client free).
    fn draw_list(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &CsClient) -> bool {
        self.list.init(vui, r, [320, 180], 330, 10, 0);
        self.list.draw(vui, r);
        let (tl, br) = (self.list.top_left(), self.list.bottom_right());
        self.pick_hits.push([tl[0], tl[1], br[0], br[1]], -1);
        self.list.is_open() && cs.state == 0
    }

    /// A picker's rows (`names` the rows offered, `sel` its cursor).
    fn draw_picker(&mut self, vui: &Vui, r: &mut dyn Raster, names: &[Vec<u16>], sel: i32) {
        let n = names.len() as i32;
        if sel - self.x4cc > 6 {
            self.x4cc = sel - 6;
        } else if sel < self.x4cc {
            self.x4cc = sel;
        }
        let t = if n < 8 { 0.0 } else { self.x4cc as f32 / (n - 7) as f32 };
        let w = self.list.size[0];
        self.list.draw_frame_vscroll(vui, r, 25, 1, w - 60, 7, t);
        for i in self.x4cc..(self.x4cc + 7).min(n) {
            let row = i - self.x4cc + 1;
            self.pick_hits.push(rows_rect(&self.list, vui, r, 42, row, 250, 1), i);
            self.list.text_box(vui, r, &names[i as usize], 42, row, 250, 1, UI_COLORS[(sel == i) as usize], 2);
            if sel == i {
                self.list.tick_r(vui, r, 40, row, true);
            }
        }
        let legend = wide(&format!("|x{}   |t{}", word(vui, 212), word(vui, 161)));
        self.list.text(vui, r, &legend, 30, 9, UI_COLORS[0], 0);
    }

    /// `OnDrawSelf`, then the editor and the box over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &mut CsClient, now: i64, cues: &mut Vec<i32>) -> Option<RequestUp> {
        self.dots.step();
        let c = |on: bool| UI_COLORS[on as usize];
        crate::ui::window_backdrop(vui, r, 642);
        self.window.draw(vui, r);
        let w = self.window.size[0];
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 842))), 40, 1, c(self.sel == 0), 0);
        self.window.text_box(vui, r, &self.category, 140, 1, w - 180, 1, c(false), 6);
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 863))), 40, 3, c(self.sel == 1), 0);
        self.window.text_box(vui, r, &self.subcategory, 140, 3, w - 180, 1, c(false), 6);
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 522))), 40, 5, c(self.sel == 2), 0);
        self.window.text_box(vui, r, &self.text, 140, 5, w - 180, 5, c(false), 6);
        self.window.text(vui, r, &wide(&word(vui, 218)), 40, 11, c(self.sel == 3), 0);
        let picking = self.picking || self.picking_sub;
        let legend = if picking { format!("|-|x{}   |-|t{}", word(vui, 212), word(vui, 161)) } else { format!("|x{}   |t{}", word(vui, 212), word(vui, 161)) };
        self.window.text(vui, r, &wide(&legend), 40, 13, c(false), 0);
        if (0..4).contains(&self.sel) {
            self.window.tick_r(vui, r, 36, [1, 3, 5, 11][self.sel as usize], !picking);
        }
        self.time = now;
        self.hits.clear();
        self.pick_hits.clear();
        for (field, row, rows) in [(0, 1, 1), (1, 3, 1), (2, 5, 5), (3, 11, 1)] {
            self.hits.push(rows_rect(&self.window, vui, r, 40, row, w - 80, rows), field);
        }
        // the categories offered: the client's, less the hidden ones
        self.cat_ids = cs.categories.iter().filter(|c| !c.hidden).map(|c| c.id).collect();
        if self.picking && self.draw_list(vui, r, cs) {
            let names: Vec<Vec<u16>> = self.cat_ids.iter().map(|&id| find_category(&cs.categories, id).map(|c| c.name.clone()).unwrap_or_default()).collect();
            let sel = self.cat_sel;
            self.draw_picker(vui, r, &names, sel);
        }
        if self.picking_sub && self.draw_list(vui, r, cs) {
            // the picked category's subcategories, less the hidden ones
            let want = self.cat_ids.get(self.cat_sel.max(0) as usize).copied().unwrap_or(0);
            let cat = find_category(&cs.categories, want).cloned();
            self.sub_ids = cat.as_ref().map(|c| c.subs.iter().filter(|s| !s.hidden).map(|s| s.id).collect()).unwrap_or_default();
            let names: Vec<Vec<u16>> = self.sub_ids.iter().map(|&id| cat.as_ref().and_then(|c| c.subs.iter().find(|s| s.id == id).or(c.subs.last())).map(|s| s.name.clone()).unwrap_or_default()).collect();
            let sel = self.sub_sel;
            self.draw_picker(vui, r, &names, sel);
        }
        if cs.state == 0 {
            self.status.close(true);
        } else {
            self.status.open(false);
        }
        let mut up = None;
        match cs.state {
            0 => {
                if self.x4d0 {
                    self.x4d0 = false;
                    cues.push(8);
                    up = Some(RequestUp::Sent);
                } else if self.x4d4 {
                    self.x4d4 = false;
                    cs.request_inbox();
                } else if self.x4d8 {
                    self.x4d8 = false;
                    // CheckFull
                    if !self.reply && cs.count >= 100 {
                        up = Some(RequestUp::Full);
                    }
                }
            }
            s => {
                match s {
                    1 => self.x4d8 = true,
                    3 | 5 => self.x4d0 = true,
                    6 => self.x4d4 = true,
                    _ => {}
                }
                let line = waiting(&self.dots, vui, s, [844, 845, 886, 847, 848, 849]);
                draw_status(vui, r, &mut self.status, s, line);
            }
        }
        if self.editing {
            self.edit.draw(vui, r, now, cues);
        }
        if self.mbox_up {
            self.mbox.draw(vui, r, now);
        }
        // what the next keys will find (a request may have gone out above)
        self.busy = cs.state != 0;
        up
    }
}

/// The arrows' repeat in the inbox and the reader: the first step at once,
/// the next after 0.45 s, then every 0.05 s while held (`m_key`,
/// `m_repeat`, `m_time`).
#[derive(Clone, Copy, Debug, Default)]
struct Repeat {
    key: i32,
    repeat: f32,
    time: i64,
}

impl Repeat {
    fn press(&mut self, key: i32) {
        self.key = key;
        self.repeat = 0.45;
    }

    /// `OnKeyUp`.
    fn release(&mut self, key: i32) {
        if key == self.key || !matches!(key, 19 | 20 | 138 | 139) {
            self.key = 0;
        }
    }

    /// `OnDrawSelf`'s step: -1 up, 1 down, 0 neither.
    fn step(&mut self, now: i64) -> i32 {
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        if !matches!(self.key, 19 | 20 | 138 | 139) {
            return 0;
        }
        self.repeat -= dt;
        if self.repeat > 0.0 {
            return 0;
        }
        self.repeat = 0.05;
        if matches!(self.key, 19 | 138) { -1 } else { 1 }
    }
}

/// What the reader asks of the inbox.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadUp {
    /// 260: back to the list
    Close,
    /// deleted: `ShowMsg(987)`, sound 8, back to the list
    Deleted,
    /// the reply went: `ShowMsg(864)`
    Replied,
}

/// One row of the reader's text: where it starts in the buffer and how long
/// it is (`VIReadCSLine`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Line {
    pub start: usize,
    pub len: usize,
}

/// `VIWndReadCS::Wrap`: a row ends at a line break (kept in the row), or
/// before the character that makes it wider than `width` -- after the last
/// space, if the row has one. Carriage returns become spaces in the buffer.
/// The test measures the row before the current character, as shipped.
pub fn wrap(text: &mut [u16], len: usize, max_lines: usize, width: i32, measure: &mut dyn FnMut(&[u16]) -> i32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut space = 0usize;
    let mut i = 0usize;
    let mut start = 0usize;
    while i + 1 < len && lines.len() < max_lines && i < text.len() && text[i] != 0 {
        if text[i] == 13 {
            text[i] = 32;
        }
        if text[i] == 10 {
            lines.push(Line { start, len: i + 1 - start });
            i += 1;
            start = i;
            space = 0;
        } else {
            if text[i] == 32 {
                space = i;
            }
            let count = i - start;
            if measure(&text[start..i]) > width {
                if space != 0 {
                    lines.push(Line { start, len: space + 1 - start });
                    i = space + 1;
                    start = i;
                } else {
                    lines.push(Line { start, len: count });
                    start = i;
                }
                space = 0;
            } else {
                i += 1;
            }
        }
    }
    if i > start && lines.len() < max_lines {
        lines.push(Line { start, len: i - start });
    }
    lines
}

/// `VIWndReadCS`: one message, its category, subcategory, date and text in
/// one buffer, eight rows at a time; X offers Reply (the embedded request
/// form, as a reply) and Delete.
pub struct Read {
    pub visible: bool,
    pub id: u32,
    pub category_id: i32,
    pub subcategory_id: i32,
    pub category: Vec<u16>,
    pub subcategory: Vec<u16>,
    pub date: Vec<u16>,
    pub text: Vec<u16>,
    full: Vec<u16>,
    lines: Vec<Line>,
    rows: i32,
    top: i32,
    dirty: bool,
    deleting: bool,
    window: Window,
    status: Window,
    options: OptionList,
    pub request: Request,
    repeat: Repeat,
    dots: Dots,
}

impl Default for Read {
    fn default() -> Self {
        Read {
            visible: false,
            id: 0,
            category_id: 0,
            subcategory_id: 0,
            category: Vec::new(),
            subcategory: Vec::new(),
            date: Vec::new(),
            text: Vec::new(),
            full: Vec::new(),
            lines: Vec::new(),
            rows: 0,
            top: 0,
            dirty: true,
            deleting: false,
            window: Window::default(),
            status: Window::default(),
            options: OptionList::default(),
            request: Request::new(true),
            repeat: Repeat::default(),
            dots: Dots::default(),
        }
    }
}

impl Read {
    pub fn set_category(&mut self, s: &[u16]) {
        self.category = capped(s, 64);
        self.dirty = true;
    }

    pub fn set_subcategory(&mut self, s: &[u16]) {
        self.subcategory = capped(s, 64);
        self.dirty = true;
    }

    pub fn set_date(&mut self, s: &[u16]) {
        self.date = capped(s, 64);
        self.dirty = true;
    }

    pub fn set_text(&mut self, s: &[u16]) {
        self.text = capped(s, 8192);
        self.dirty = true;
    }

    /// 295, the focus from the inbox: the window, its option list (Reply,
    /// Delete), the status; every child hidden.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.window.close(true);
        self.window.init(vui, r, [320, 180], 500, 11, 0);
        self.window.set_title(Some(&wide(&word(vui, 933))));
        self.window.open(false);
        self.request.visible = false;
        let lines = vec![wide(&word(vui, 534)), wide(&word(vui, 986))];
        self.options.setup(vui, r, &wide(&word(vui, 422)), lines, [320, 180], 256);
        self.status.init(vui, r, [320, 180], 375, 4, 0);
        self.status.set_title(Some(&wide(&word(vui, 271))));
        self.status.open(true);
        self.deleting = false;
        self.repeat = Repeat { time: now, ..Default::default() };
        self.options.visible = false;
    }

    pub fn typing(&self) -> bool {
        self.request.visible && self.request.typing()
    }

    /// A pointer at frame point `p`, where the keys go: the reply form, the
    /// option list (a line taken at once; off it, △ closes it). The text
    /// itself takes nothing (the wheel scrolls it, its legend's ✕ offers
    /// the options). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.request.visible {
            return self.request.point(p);
        }
        if self.options.visible {
            return match self.options.point(p) {
                Some(k) => k.into_iter().collect(),
                None => vec![key::TRIANGLE],
            };
        }
        Vec::new()
    }

    fn scroll(&mut self, d: i32, cues: &mut Vec<i32>) {
        if d < 0 {
            if self.top > 0 {
                self.top -= 1;
                cues.push(11);
            } else {
                cues.push(9);
            }
        } else if self.top < self.rows - 8 {
            self.top += 1;
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &mut CsClient, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<ReadUp> {
        if self.request.visible {
            // 260: the reply put away
            if self.request.key(vui, r, cs, key, down, now, cues).is_some() {
                self.request.visible = false;
            }
            return None;
        }
        if !down {
            self.repeat.release(key);
            return None;
        }
        if self.options.visible {
            // 308: the line, and whether it was taken
            if let Some((line, enter)) = self.options.key(key, now, cues) {
                self.options.visible = false;
                if enter {
                    match line {
                        1 => cs.delete_message(self.id),
                        0 => {
                            self.request.refresh(cs);
                            self.request.visible = true;
                            self.request.focus(vui, r);
                            let (cat, sub) = (self.category.clone(), self.subcategory.clone());
                            self.request.set_reply(&cat, &sub, (self.category_id, self.subcategory_id), self.id);
                        }
                        _ => {}
                    }
                }
            }
            return None;
        }
        self.repeat.key = 0;
        let busy = cs.state > 0;
        match key {
            19 | 138 if !busy => {
                self.repeat.press(key);
                self.scroll(-1, cues);
            }
            20 | 139 if !busy => {
                self.repeat.press(key);
                self.scroll(1, cues);
            }
            13 | 134 if !busy => {
                cues.push(11);
                self.options.visible = true;
                self.options.focus(vui, r);
            }
            27 | 132 => {
                cues.push(8);
                return Some(ReadUp::Close);
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`: the text (only while this, or its option list, has the
    /// focus), the legend, the status; then the children.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &mut CsClient, now: i64, cues: &mut Vec<i32>) -> Option<ReadUp> {
        let d = self.repeat.step(now);
        if d != 0 {
            self.scroll(d, cues);
        }
        self.dots.step();
        self.window.draw(vui, r);
        let w = self.window.size[0];
        if self.dirty {
            let s = format!(
                "{}: {}\n{}: {}\n{}: {}\n\n{}",
                word(vui, 842),
                String::from_utf16_lossy(&self.category),
                word(vui, 863),
                String::from_utf16_lossy(&self.subcategory),
                word(vui, 985),
                String::from_utf16_lossy(&self.date),
                String::from_utf16_lossy(&self.text)
            );
            self.full = wide(&s).into_iter().take(8703).collect();
            let mut measure = |s: &[u16]| vui.string_width(r, s);
            let n = self.full.len();
            self.lines = wrap(&mut self.full, 8704.min(n + 1), 1024, w - 90, &mut measure);
            self.rows = self.lines.len() as i32;
            self.top = 0;
            self.dirty = false;
        }
        let t = if self.rows <= 8 { 0.0 } else { self.top as f32 / (self.rows - 8) as f32 };
        if !self.request.visible {
            self.window.draw_frame_vscroll(vui, r, 40, 1, w - 80, 8, t);
            for i in 0..8 {
                if let Some(l) = self.lines.get((i + self.top) as usize) {
                    let row = &self.full[l.start..l.start + l.len];
                    self.window.text_box(vui, r, row, 40, i + 1, w - 80, 1, UI_COLORS[0], 0);
                }
            }
        }
        let legend = wide(&format!("|x{}   |t{}", word(vui, 422), word(vui, 217)));
        self.window.text(vui, r, &legend, 40, 10, UI_COLORS[0], 0);
        if cs.state == 0 {
            self.status.close(true);
        } else {
            self.status.open(false);
        }
        let mut up = None;
        match cs.state {
            0 => {
                if self.deleting {
                    self.deleting = false;
                    cues.push(8);
                    up = Some(ReadUp::Deleted);
                }
            }
            s => {
                if s == 4 {
                    self.deleting = true;
                }
                let line = waiting(&self.dots, vui, s, [844, 845, 846, 847, 848, 849]);
                draw_status(vui, r, &mut self.status, s, line);
            }
        }
        if self.options.visible {
            self.options.draw(vui, r);
        }
        if self.request.visible {
            match self.request.draw(vui, r, cs, now, cues) {
                // 309: sent (ShowMsg 864 in the request), back to the reader
                Some(RequestUp::Sent) => {
                    self.request.visible = false;
                    up = Some(ReadUp::Replied);
                }
                Some(_) => self.request.visible = false,
                None => {}
            }
        }
        up
    }
}

/// What the inbox asks of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InboxUp {
    /// `ClosePanel700`: back to the start menu
    Close,
    /// `ShowMsg(id)`
    Show(i32),
}

/// `VIWndCSInbox` (mode 32): eleven rows of the message number and its
/// category, an envelope by each unread one; X reads the selected one,
/// square asks for the lists again.
#[derive(Default)]
pub struct Inbox {
    pub sel: i32,
    pub top: i32,
    column: i32,
    /// the text was asked for (m_read_), the categories (m_delete), the inbox
    /// (m_reload)
    read_flag: bool,
    delete_flag: bool,
    reload: bool,
    window: Window,
    status: Window,
    repeat: Repeat,
    pub read: Read,
    dots: Dots,
    /// the rows drawn, by index, for a pointer; the frame's time; the client
    /// busy (the rows take no keys then)
    hits: Hits,
    time: i64,
    busy: bool,
}

/// `FindCategory(category, subcategory)`: the two names (the category's even
/// when the subcategory is not found).
fn names(cats: &[Category], category: i32, subcategory: i32) -> (Vec<u16>, Vec<u16>) {
    for c in cats {
        if c.id == category {
            let name = capped(&c.name, 64);
            return match c.subs.iter().find(|s| s.id == subcategory) {
                Some(s) => (name, capped(&s.name, 64)),
                None => (name, Vec::new()),
            };
        }
    }
    (Vec::new(), Vec::new())
}

impl Inbox {
    /// `OpenCSInbox` (mode 32): `Refresh`, then the focus (295).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, cs: &mut CsClient, now: i64) {
        self.refresh(cs);
        self.window.init(vui, r, [320, 180], 560, 14, 0);
        self.window.set_title(Some(&wide(&word(vui, 862))));
        self.window.open(true);
        self.status.init(vui, r, [320, 180], 375, 4, 0);
        self.status.set_title(Some(&wide(&word(vui, 271))));
        self.status.open(true);
        self.read_flag = false;
        self.delete_flag = false;
        self.reload = false;
        self.read.visible = false;
        self.repeat = Repeat { time: now, ..Default::default() };
    }

    /// `Refresh`: back to the top, and the lists asked for when the client
    /// is free (the categories, then the inbox).
    pub fn refresh(&mut self, cs: &mut CsClient) {
        if cs.state <= 0 {
            cs.request_categories();
        }
        self.top = 0;
        self.sel = 0;
        self.column = 0;
    }

    pub fn typing(&self) -> bool {
        self.read.visible && self.read.typing()
    }

    /// A pointer at frame point `p`: to the reader while it is up; else,
    /// the client free, a row selected as up and down select it, and the
    /// selected one read (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.read.visible {
            return self.read.point(p);
        }
        if self.busy {
            return Vec::new();
        }
        field_click(&mut self.sel, &mut self.window, self.time, self.hits.at(p))
    }

    fn step(&mut self, d: i32, cs: &CsClient, now: i64, cues: &mut Vec<i32>) {
        if d < 0 {
            if self.sel > 0 {
                self.sel -= 1;
                self.window.reset_tick_pulse(now);
                cues.push(11);
            } else {
                cues.push(9);
            }
        } else if self.sel < cs.count - 1 {
            self.sel += 1;
            self.window.reset_tick_pulse(now);
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &mut CsClient, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<InboxUp> {
        if self.read.visible {
            match self.read.key(vui, r, cs, key, down, now, cues) {
                Some(ReadUp::Close) => self.read.visible = false,
                Some(ReadUp::Deleted) => {
                    self.read.visible = false;
                    return Some(InboxUp::Show(987));
                }
                Some(ReadUp::Replied) => return Some(InboxUp::Show(864)),
                None => {}
            }
            return None;
        }
        if !down {
            self.repeat.release(key);
            return None;
        }
        self.repeat.key = 0;
        let busy = cs.state > 0;
        match key {
            19 | 138 if !busy => {
                self.repeat.press(key);
                self.step(-1, cs, now, cues);
            }
            20 | 139 if !busy => {
                self.repeat.press(key);
                self.step(1, cs, now, cues);
            }
            81 | 113 | 133 if !busy => self.refresh(cs),
            13 | 134 if !busy && cs.state != -19 => self.select(cs),
            27 | 132 => {
                cues.push(8);
                return Some(InboxUp::Close);
            }
            _ => {}
        }
        None
    }

    /// `Select`: the selected message asked for; the reader gets its date
    /// and ids now and the rest when it is in.
    fn select(&mut self, cs: &mut CsClient) {
        if cs.count <= 0 {
            return;
        }
        let h = cs.header(self.sel);
        cs.request_message(h.id);
        self.read.id = cs.message.id;
        self.read.set_date(&wide(&h.date.format()));
        self.read.category_id = cs.message.category;
        self.read.subcategory_id = cs.message.subcategory;
    }

    /// `Read`: the text in, the reader shown with the focus.
    fn read_in(&mut self, vui: &Vui, r: &dyn Raster, cs: &CsClient, now: i64) {
        let text = cs.text.clone();
        self.read.set_text(&text);
        let (cat, sub) = names(&cs.categories, cs.message.category, cs.message.subcategory);
        self.read.set_category(&cat);
        self.read.set_subcategory(&sub);
        // SetActiveWindow(m_read): its 295 sets it up only when the focus
        // comes from elsewhere -- after a reply (the text fetched again) it
        // comes from the reply form, which then says it was sent and goes
        if !self.read.visible {
            self.read.visible = true;
            self.read.focus(vui, r, now);
        }
    }

    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cs: &mut CsClient, now: i64, cues: &mut Vec<i32>) -> Option<InboxUp> {
        let focused = !self.read.visible;
        let d = self.repeat.step(now);
        if d != 0 && focused {
            self.step(d, cs, now, cues);
        }
        self.dots.step();
        crate::ui::window_backdrop(vui, r, 642);
        self.window.draw(vui, r);
        let w = self.window.size[0];
        let x = [58, w - 340];
        let c0 = UI_COLORS[0];
        // the live client's header (0x0133F354): "%S" and its 841, "Ticket #"
        // (the disc's was "%S #" and "Ticket")
        self.window.text(vui, r, &wide(&ticket_header(vui)), if self.column != 0 { x[0] } else { x[0] - 12 }, 0, c0, 0);
        self.window.text(vui, r, &wide(&word(vui, 842)), if self.column != 1 { x[1] } else { x[1] - 12 }, 0, c0, 0);
        let n = cs.count;
        if self.sel > n - 1 {
            self.sel = n - 1;
        }
        self.sel = self.sel.max(0);
        if self.sel - self.top > 10 {
            self.top = self.sel - 10;
        } else if self.sel < self.top {
            self.top = self.sel;
        }
        let t = if n == 11 { 0.0 } else { self.top as f32 / (n - 11) as f32 };
        self.window.draw_frame_vscroll(vui, r, 40, 1, w - 80, 11, t);
        self.time = now;
        self.hits.clear();
        if focused {
            let end = if self.top + 11 > n { n } else { self.top + 11 };
            for i in self.top..end {
                let h = cs.header(i);
                let row = i - self.top + 1;
                self.hits.push(rows_rect(&self.window, vui, r, x[0], row, w - 44 - x[0], 1), i);
                let c = UI_COLORS[(self.sel == i) as usize];
                self.window.text_box(vui, r, &wide(&h.id.to_string()), x[0], row, x[1] - x[0] - 10, 1, c, 2);
                if h.unread != 0 {
                    let p = self.window.row_center(vui, r, row);
                    vui.blit_ex(r, 222, [x[1] + 15, p[1] - 5], 0, 1.0, 0);
                }
                let (cat, _) = names(&cs.categories, h.category, h.subcategory);
                self.window.text_box(vui, r, &cat, x[1], row, w - 44 - x[1], 1, c, 2);
                if self.sel == i {
                    self.window.tick_r(vui, r, 56, row, true);
                }
            }
        }
        let legend = wide(&format!("|x{}   |t{}           |s{}", word(vui, 212), word(vui, 161), word(vui, 843)));
        self.window.text(vui, r, &legend, 40, 13, c0, 0);
        if cs.state == 0 {
            self.status.close(true);
        } else {
            self.status.open(false);
        }
        match cs.state {
            0 => {
                if self.read_flag {
                    self.read_flag = false;
                    self.read_in(vui, r, cs, now);
                } else if self.delete_flag {
                    self.delete_flag = false;
                    cs.request_inbox();
                } else if self.reload {
                    self.reload = false;
                    self.sel = (cs.count - 1).max(0);
                }
            }
            s => {
                match s {
                    1 => self.reload = true,
                    2 => self.read_flag = true,
                    6 => self.delete_flag = true,
                    _ => {}
                }
                let line = waiting(&self.dots, vui, s, [844, 845, 846, 847, 848, 849]);
                draw_status(vui, r, &mut self.status, s, line);
            }
        }
        let mut up = None;
        if self.read.visible {
            match self.read.draw(vui, r, cs, now, cues) {
                Some(ReadUp::Deleted) => {
                    self.read.visible = false;
                    up = Some(InboxUp::Show(987));
                }
                Some(ReadUp::Replied) => up = Some(InboxUp::Show(864)),
                Some(ReadUp::Close) => self.read.visible = false,
                None => {}
            }
        }
        // what the next keys will find (a request may have gone out above)
        self.busy = cs.state > 0;
        up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(id: u32, cat: i32, sub: i32) -> Header {
        Header { id, date: Date { year: 2026, month: 10, day: 19, hour: 21, minute: 5, second: 3 }, category: cat, subcategory: sub, unread: 1 }
    }

    fn inbox_reply(hs: &[Header], status: u8) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(2).u8(status).u32(0).signed(hs.len() as i64);
        for h in hs {
            h.write(&mut w);
        }
        w.data
    }

    /// The requests: 0x0AA1, the sub-op, then the fields (ids zigzag, the
    /// text wide with a u32 length).
    #[test]
    fn requests_are_the_opcode_and_the_sub_op() {
        let mut c = CsClient::default();
        c.request_inbox();
        assert_eq!(c.out[0], [0xA1, 0x0A, 2]);
        assert_eq!(c.state, 1);
        c.request_categories();
        assert_eq!((c.out[1].clone(), c.state), (vec![0xA1, 0x0A, 6], 6));
        c.submit_request(3, -1, &wide("hi"));
        assert_eq!(c.out[2], [0xA1, 0x0A, 0, 6, 1, 2, 0, 0, 0, b'h', 0, b'i', 0]);
        assert_eq!(c.state, 3);
        c.request_message(5);
        assert_eq!((c.out[3].clone(), c.state), (vec![0xA1, 0x0A, 3, 10], 2));
    }

    /// A reply acts only when it was waited for (the control: an inbox while
    /// a request is out changes nothing); a failure is the status as an
    /// error; the categories, the inbox and the text.
    #[test]
    fn replies_are_gated_by_the_state() {
        let mut c = CsClient::default();
        c.submit_request(1, 2, &wide("x"));
        c.recv(&inbox_reply(&[header(7, 1, 2)], 0));
        assert_eq!((c.count, c.state), (-1, 3), "not waited for: ignored");
        c.recv(&[0, 0]);
        assert_eq!(c.state, 0);
        c.request_categories();
        let mut w = Writer::default();
        w.u8(6).u8(0).signed(2);
        w.signed(1).u8(0).wide("Bugs").signed(1).signed(10).u8(0).wide("Crash");
        w.signed(2).u8(1).wide("Hidden").signed(0);
        c.recv(&w.data);
        assert_eq!(c.state, 0);
        assert_eq!(c.categories.len(), 2);
        assert_eq!((c.categories[0].name.clone(), c.categories[0].subs[0].id, c.categories[1].hidden), (wide("Bugs"), 10, true));
        c.request_inbox();
        c.x08 = 1;
        c.recv(&inbox_reply(&[header(7, 1, 10), header(9, 1, 10)], 0));
        assert_eq!((c.count, c.state, c.x08), (2, 0, 0));
        c.request_message(9);
        let mut w = Writer::default();
        w.u8(3).u8(0).wide("hello");
        c.recv(&w.data);
        assert_eq!((c.state, c.headers[1].unread, c.headers[0].unread), (0, 0, 1));
        assert_eq!(c.text, wide("hello"));
        // a failed status: 14 is -19, 255 is -128
        c.request_inbox();
        c.recv(&inbox_reply(&[], 14));
        assert_eq!(c.state, -19);
        c.request_inbox();
        c.recv(&inbox_reply(&[], 255));
        assert_eq!(c.state, -128);
        assert_eq!(c.count, 2, "a failure keeps the list");
    }

    /// Deleting waits for the server; then the rows after it move up. The
    /// count drops even for an id not listed (as shipped). A reply fetches
    /// the message again.
    #[test]
    fn delete_and_reply() {
        let mut c = CsClient::default();
        c.request_inbox();
        c.recv(&inbox_reply(&[header(1, 0, 0), header(2, 0, 0), header(3, 0, 0)], 0));
        c.delete_message(2);
        assert_eq!(c.count, 3, "nothing goes before the server says so");
        assert_eq!(*c.out.last().unwrap(), [0xA1, 0x0A, 4, 4]);
        let mut w = Writer::default();
        w.u8(4).u8(0).signed(2);
        c.recv(&w.data);
        assert_eq!((c.state, c.count), (0, 2));
        assert_eq!(c.headers[..2].iter().map(|h| h.id).collect::<Vec<_>>(), [1, 3]);
        c.delete_message(99);
        let mut w = Writer::default();
        w.u8(4).u8(0).signed(99);
        c.recv(&w.data);
        assert_eq!(c.count, 1, "the count drops though 99 is not listed");
        c.submit_reply(1, &wide("ok"));
        assert_eq!(*c.out.last().unwrap(), [0xA1, 0x0A, 1, 2, 2, 0, 0, 0, b'o', 0, b'k', 0]);
        c.recv(&[1, 0]);
        assert_eq!((c.state, c.out.last().unwrap().clone()), (2, vec![0xA1, 0x0A, 3, 2]), "the message asked for again");
        c.recv(&[5]);
        assert_eq!(c.x08, 1);
    }

    #[test]
    fn status_codes() {
        let want = [(0u8, 0), (1, -1), (11, -11), (12, -17), (13, -18), (14, -19), (15, -12), (19, -16), (20, -128), (255, -128)];
        for (s, e) in want {
            assert_eq!(status_to_error(s), e, "status {s}");
        }
    }

    /// Wrap: a break ends a row (kept in it); a row too wide ends after its
    /// last space, or before the character with none. Every character here is
    /// 10 wide.
    #[test]
    fn wrap_rows() {
        let mut m = |s: &[u16]| s.len() as i32 * 10;
        let mut t = wide("ab cd\nefghij");
        let n = t.len();
        let lines = wrap(&mut t, n + 1, 64, 30, &mut m);
        let rows: Vec<String> = lines.iter().map(|l| String::from_utf16_lossy(&t[l.start..l.start + l.len])).collect();
        assert_eq!(rows, ["ab ", "cd\n", "efgh", "ij"]);
        let mut t = wide("a\rb");
        let n = t.len();
        wrap(&mut t, n + 1, 64, 300, &mut m);
        assert_eq!(t, wide("a b"), "a carriage return becomes a space");
    }

    use crate::pointer::testing::{NullRaster, mid, vui};
    use crate::pointer::{MOVED, key};

    fn categories() -> Vec<Category> {
        let sub = |id: i32, n: &str| Subcategory { id, hidden: false, name: wide(n) };
        vec![
            Category { id: 1, hidden: false, name: wide("Bugs"), subs: vec![sub(10, "Crash"), sub(11, "Art")] },
            Category { id: 2, hidden: false, name: wide("Help"), subs: vec![sub(20, "Stuck")] },
        ]
    }

    /// The request form: a field selected and then used (✕ on Category
    /// opens its picker); the picker takes nothing while it opens, then a
    /// line at once (the category picked, on to its subcategories), nothing
    /// for its frame, and off it △; the fields take nothing while the
    /// client waits.
    #[test]
    fn a_pointer_on_the_request_form() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut cues = Vec::new();
        let mut cs = CsClient { categories: categories(), ..Default::default() };
        let mut q = Request::new(false);
        q.refresh(&mut cs);
        cs.state = 0;
        q.visible = true;
        q.focus(&vui, &r);
        q.draw(&vui, &mut r, &mut cs, 1000, &mut cues);
        let w = q.window.size[0];
        let field = |q: &Request, row: i32, rows: i32| mid(rows_rect(&q.window, &vui, &NullRaster, 40, row, w - 80, rows));
        assert_eq!(q.point(field(&q, 3, 1)), vec![MOVED]);
        assert_eq!((q.sel, q.window.pulse_time), (1, 1000));
        assert_eq!(q.point(field(&q, 1, 1)), vec![MOVED]);
        assert_eq!(q.point(field(&q, 1, 1)), vec![key::CROSS], "Category: its picker");
        q.key(&vui, &mut r, &mut cs, key::CROSS, true, 1016, &mut cues);
        assert!(q.picking);
        q.draw(&vui, &mut r, &mut cs, 1016, &mut cues);
        let line = |q: &Request, row: i32| mid(rows_rect(&q.list, &vui, &NullRaster, 42, row, 250, 1));
        assert!(q.point(line(&q, 2)).is_empty(), "still opening");
        for t in 1..8 {
            q.draw(&vui, &mut r, &mut cs, 1016 + 16 * t, &mut cues);
        }
        assert_eq!(q.point(line(&q, 2)), vec![key::CROSS]);
        assert_eq!(q.cat_sel, 1, "Help");
        q.key(&vui, &mut r, &mut cs, key::CROSS, true, 1200, &mut cues);
        assert_eq!((q.category.clone(), q.picking_sub), (wide("Help"), true));
        for t in 0..8 {
            q.draw(&vui, &mut r, &mut cs, 1216 + 16 * t, &mut cues);
        }
        assert!(q.point(line(&q, 2)).is_empty(), "Help has one subcategory: the second line is the picker's frame");
        assert_eq!(q.point([5.0, 5.0]), vec![key::TRIANGLE], "off the picker");
        q.key(&vui, &mut r, &mut cs, key::TRIANGLE, true, 1400, &mut cues);
        assert!(!q.picking_sub);
        // waiting: the fields take nothing
        cs.state = 3;
        q.draw(&vui, &mut r, &mut cs, 1416, &mut cues);
        let sel = q.sel;
        assert!(q.point(field(&q, 11, 1)).is_empty());
        assert_eq!(q.sel, sel);
    }

    /// The inbox: rows selected and read, not past the list, not while the
    /// client waits; the reader its option list (a line at once, off it △);
    /// its Reply's form, which starts at the text and cannot reach the
    /// category.
    #[test]
    fn a_pointer_on_the_inbox_the_reader_and_the_reply() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut cues = Vec::new();
        let mut cs = CsClient { categories: categories(), ..Default::default() };
        let mut ib = Inbox::default();
        ib.open(&vui, &r, &mut cs, 0);
        cs.state = 0;
        cs.count = 2;
        cs.headers = vec![header(7, 1, 10), header(9, 2, 20)];
        ib.draw(&vui, &mut r, &mut cs, 1000, &mut cues);
        let w = ib.window.size[0];
        let row = |ib: &Inbox, row: i32| mid(rows_rect(&ib.window, &vui, &NullRaster, 58, row, w - 44 - 58, 1));
        assert_eq!(ib.point(row(&ib, 2)), vec![MOVED]);
        assert_eq!(ib.sel, 1);
        assert_eq!(ib.point(row(&ib, 2)), vec![key::CROSS], "read");
        assert!(ib.point(row(&ib, 3)).is_empty(), "two listed");
        // asked for: nothing while it comes, then the reader
        ib.key(&vui, &mut r, &mut cs, key::CROSS, true, 1016, &mut cues);
        assert_eq!(cs.state, 2);
        ib.draw(&vui, &mut r, &mut cs, 1032, &mut cues);
        assert!(ib.point(row(&ib, 1)).is_empty(), "waiting");
        assert_eq!(ib.sel, 1);
        cs.state = 0;
        ib.draw(&vui, &mut r, &mut cs, 1048, &mut cues);
        assert!(ib.read.visible);
        assert!(ib.point(row(&ib, 1)).is_empty(), "the reader's text takes nothing");
        assert_eq!(ib.sel, 1);
        ib.read.options.visible = true;
        ib.draw(&vui, &mut r, &mut cs, 1064, &mut cues);
        // the option list, 300 wide round (320, 180) and titled: Reply on
        // y 167, Delete on 179
        assert_eq!(ib.point([310.0, 179.0]), vec![key::CROSS]);
        assert_eq!(ib.read.options.sel, 1);
        assert_eq!(ib.point([10.0, 10.0]), vec![key::TRIANGLE], "off it: closed");
        // Reply: the form over the reader
        assert_eq!(ib.point([310.0, 167.0]), vec![key::CROSS]);
        ib.key(&vui, &mut r, &mut cs, key::CROSS, true, 1080, &mut cues);
        assert!(ib.read.request.visible);
        cs.state = 0;
        ib.draw(&vui, &mut r, &mut cs, 1096, &mut cues);
        let q = &ib.read.request;
        let rw = q.window.size[0];
        let field = |q: &Request, row: i32| mid(rows_rect(&q.window, &vui, &NullRaster, 40, row, rw - 80, 1));
        let (category, text, submit) = (field(q, 1), field(q, 5), field(q, 11));
        assert!(ib.point(category).is_empty(), "a reply's category is not the form's to pick");
        assert_eq!(ib.point(text), vec![key::CROSS], "a reply starts on its text");
        assert_eq!(ib.point(submit), vec![MOVED]);
        assert_eq!(ib.read.request.sel, 3);
    }

    /// The live -8 (`VIWndCSInbox::OnDrawSelf` 0x0133FB8C and its two
    /// siblings): the code's line, a new line, then 1888, the petition has
    /// been addressed (the table here lacks the row: `livewords`'). The
    /// control: any other failure is the disc's line alone. The inbox's
    /// header reads "Ticket #" off the disc's 841, "Ticket", and a table's
    /// own 841 as it is.
    #[test]
    fn an_addressed_petition_says_so() {
        let mut lines: Vec<String> = (1..=1814).map(|i| format!("w{i}")).collect();
        lines[818] = "Code".into();
        lines[840] = "Ticket".into();
        lines[864] = "System is temporarily unavailable".into();
        let table = |lines: &[String]| Vui::new(vi::strtab::StringTable::parse(lines.join("\r\n").as_bytes()).unwrap(), Default::default(), Vec::new());
        let vui = table(&lines);
        assert_eq!(String::from_utf16_lossy(&error_text(&vui, -3)), "System is temporarily unavailable. (Code: -3)");
        let s = String::from_utf16_lossy(&error_text(&vui, -8));
        assert_eq!(s, format!("System is temporarily unavailable. (Code: -8)\n{}", crate::livewords::text(1888).unwrap()));
        assert_eq!(ticket_header(&vui), "Ticket #");
        lines[840] = "Petition".into();
        assert_eq!(ticket_header(&table(&lines)), "Petition", "a table's own word");
    }
}
