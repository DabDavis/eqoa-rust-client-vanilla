//! Tell-grams (the in-game mail) and the tell and reply forms.
//!
//! `Grams` is `VIClientPart2EFD0` (`viclientpart2efd0.cpp`), the gram client:
//! a request goes out as message 0x0D01 carrying a sub-op byte and, as a blob,
//! an inner 0x0D41 with the same sub-op, then its fields; replies come back
//! on 0x0D41, a sub-op and a status. Sub-ops: 1 send, 2 the inbox, 3 open a
//! gram, 4 delete, 5 (in) new mail. `m_state` is the reply waited for (1-4)
//! and goes negative on a failure.
//!
//! The windows: `VIWndComposeGram` (`Compose`: To, Subject, the body, Send;
//! the start menu's Mail > Compose is mode 23, and the reader and the buddy
//! list each carry one as a reply), `VIWndGramInbox` (`Inbox`, mode 24),
//! `VIWndReadGram` (`Read`, the inbox's reader) and `VIWndTell` /
//! `VIWndReply` (`Tell`, modes 28 and 29: a name and a line, then "/t name
//! line" to the chat line).

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::startmenu::KeyFlash;
use crate::textlist::{Message, PROMPT_OK};
use crate::wholist::OptionList;
use net::stream::{Reader, Writer};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VIDate`: seven bytes on the wire (`VIDate`/`VITime`'s stream operators).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Date {
    pub year: i16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Date {
    /// `VIDate::Format`: "%04i-%02i-%02i %02i:%02i PST".
    pub fn format(&self) -> String {
        format!("{:04}-{:02}-{:02} {:02}:{:02} PST", self.year, self.month, self.day, self.hour, self.minute)
    }

    fn read(r: &mut Reader) -> Self {
        Date { year: r.u16() as i16, month: r.u8(), day: r.u8(), hour: r.u8(), minute: r.u8(), second: r.u8() }
    }

    #[cfg(test)]
    pub fn write(&self, w: &mut Writer) {
        w.bytes(&self.year.to_le_bytes()).u8(self.month).u8(self.day).u8(self.hour).u8(self.minute).u8(self.second);
    }
}

/// `VIGramHeader`: a row of the inbox.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub x00: u32,
    pub id: u32,
    pub x08: u32,
    /// 8-bit, `ReadString(char *, 24)`
    pub from: Vec<u8>,
    /// `ReadString(wide, 128)`: 63 characters at most
    pub subject: Vec<u16>,
    pub date: Date,
    /// set when its text arrives
    pub read: u8,
}

impl Header {
    /// `operator>>(VIMemStream &, VIGramHeader &)`: the id comes first.
    fn read(r: &mut Reader) -> Self {
        let id = r.u32();
        let x00 = r.u32();
        let x08 = r.u32();
        let from = r.string(24);
        let subject = wide(&r.wide(64));
        let date = Date::read(r);
        let read = r.u8();
        Header { x00, id, x08, from, subject, date, read }
    }

    /// `operator<<`: the server's half.
    #[cfg(test)]
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.id).u32(self.x00).u32(self.x08).string(&self.from).wide(&String::from_utf16_lossy(&self.subject));
        self.date.write(w);
        w.u8(self.read);
    }
}

/// `VIClientPart2EFD0`: the gram client. What it sends waits in `out`;
/// the words it shows (`ShowText`) in `shown`.
#[derive(Debug)]
pub struct Grams {
    pub state: i32,
    /// `m_x08`: set by sub-op 5 (new mail), cleared by an inbox
    pub x08: i32,
    /// -1 until the first inbox; the list's count, though 100 rows at most
    /// are kept
    pub count: i32,
    pub headers: Vec<Header>,
    /// `m_message`: the gram opened, its row and its text
    pub message: Header,
    pub text: Vec<u16>,
    pub out: Vec<Vec<u8>>,
    pub shown: Vec<i32>,
}

impl Default for Grams {
    fn default() -> Self {
        Grams { state: 0, x08: 0, count: -1, headers: Vec::new(), message: Header::default(), text: Vec::new(), out: Vec::new(), shown: Vec::new() }
    }
}

/// The request: the 0x0D01 envelope with `sub`, then the inner 0x0D41 with
/// it again as a blob (a zigzag length, three bytes), then the fields.
fn request(sub: u8) -> Writer {
    let inner = [0x41, 0x0D, sub];
    let mut w = Writer::op(0x0D01);
    w.u8(sub).signed(inner.len() as i64).bytes(&inner);
    w
}

impl Grams {
    /// The header at `i`; past the 100 kept, an empty one (the game reads
    /// past its array).
    pub fn header(&self, i: i32) -> Header {
        self.headers.get(i.max(0) as usize).filter(|_| i >= 0).cloned().unwrap_or_default()
    }

    /// `OnConnect` (the pump's 0xFF): ask again for what was waited for.
    pub fn on_connect(&mut self) {
        match self.state {
            1 => self.request_inbox(),
            2 => self.request_message(self.message.id),
            3 | 4 => self.state = 0,
            _ => {}
        }
    }

    /// Sub-op 2.
    pub fn request_inbox(&mut self) {
        self.state = 1;
        self.out.push(request(2).data);
    }

    /// Sub-op 3: the row is copied to `m_message` (its text emptied) if it is
    /// listed, and the id asked for either way.
    pub fn request_message(&mut self, id: u32) {
        if let Some(h) = self.headers.iter().take(self.count.max(0) as usize).find(|h| h.id == id) {
            self.message = h.clone();
            self.text.clear();
        }
        let mut w = request(3);
        w.signed(id as i32 as i64);
        self.out.push(w.data);
        self.state = 2;
    }

    /// Sub-op 4: the row goes at once (and the count drops even when the id
    /// is not listed), the server is told after.
    pub fn delete_message(&mut self, id: u32) {
        let n = self.count.max(0) as usize;
        let i = self.headers.iter().take(n).position(|h| h.id == id).unwrap_or(n);
        self.count -= 1;
        if i < self.headers.len() && (i as i32) < self.count + 1 {
            self.headers.remove(i);
        }
        let mut w = request(4);
        w.signed(id as i32 as i64);
        self.out.push(w.data);
        self.state = 4;
    }

    /// Sub-op 1: the name (8-bit), the subject and the text (wide).
    pub fn send_gram(&mut self, to: &[u8], subject: &[u16], text: &[u16]) {
        self.state = 3;
        let mut w = request(1);
        w.string(to).wide(&String::from_utf16_lossy(subject)).wide(&String::from_utf16_lossy(text));
        self.out.push(w.data);
    }

    /// `Recv0D41`: the reply's sub-op picks the handler, which takes it only
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
            1 => self.recv_send(&mut r),
            4 => self.recv_delete(&mut r),
            5 => self.x08 = 1,
            _ => {}
        }
    }

    fn recv_inbox(&mut self, r: &mut Reader) {
        if self.state != 1 {
            return;
        }
        if r.u8() != 2 {
            self.state = -6;
            return;
        }
        self.x08 = 0;
        self.state = 0;
        self.count = r.signed() as i32;
        self.headers.clear();
        for _ in 0..self.count.clamp(0, 100) {
            self.headers.push(Header::read(r));
        }
    }

    fn recv_message(&mut self, r: &mut Reader) {
        if self.state != 2 {
            return;
        }
        if r.u8() != 3 {
            self.state = -7;
            return;
        }
        self.state = 0;
        let id = r.signed() as i32 as u32;
        if id != self.message.id {
            return;
        }
        let n = self.count.max(0) as usize;
        for h in self.headers.iter_mut().take(n) {
            if h.id == id {
                h.read = 1;
            }
        }
        self.text = wide(&r.wide(512));
    }

    fn recv_send(&mut self, r: &mut Reader) {
        if self.state != 3 {
            return;
        }
        self.state = match r.u8() {
            1 => {
                // "Your mail has been sent."
                self.shown.push(839);
                0
            }
            9 => -1,
            10 => -2,
            11 => -3,
            12 => -4,
            _ => -5,
        };
    }

    fn recv_delete(&mut self, r: &mut Reader) {
        if self.state != 4 {
            return;
        }
        if r.u8() == 13 {
            // "Message deleted."
            self.shown.push(840);
            self.state = 0;
        } else {
            self.state = -8;
        }
    }
}

fn word(vui: &Vui, id: i32) -> String {
    vui.words.get(id, vui.language).to_string()
}

/// The "..." after a status line: one dot more every 15 frames, three at
/// most (`sDots`, `sTicks`, one pair per window class).
#[derive(Clone, Copy, Debug, Default)]
struct Dots {
    dots: i8,
    ticks: i8,
}

impl Dots {
    fn step(&mut self) {
        self.ticks = self.ticks.wrapping_sub(1);
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
    }

    fn line(&self, vui: &Vui, id: i32) -> Vec<u16> {
        wide(&format!("{}{}", word(vui, id), ".".repeat(self.dots as usize + 1)))
    }
}

/// The status window over a gram window while the client works.
fn draw_status(vui: &Vui, r: &mut dyn Raster, status: &mut Window, line: &[u16]) {
    let back = wide(&format!("|t{}", word(vui, 161)));
    status.draw(vui, r);
    let w = status.size[0];
    status.text_box(vui, r, line, 40, 1, w - 40, 1, UI_COLORS[0], 0);
    status.text_box(vui, r, &back, 40, 3, w - 40, 1, UI_COLORS[0], 0);
}

/// An embedded `VIWndEdit` for one field: rows, multi-line, length, centred
/// on the window, the field's text, the focus.
fn edit_field(e: &mut EditBox, rows: i32, multi: bool, max: usize, center: [i32; 2], text: &[u16], now: i64) {
    e.set_rows(rows);
    e.set_multi_line(multi);
    e.set_center(center);
    e.set_max_length(max);
    e.set_text(text);
    e.focus(now);
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

/// `strcpy16` into a buffer of `n` characters (the terminator counts).
fn capped(s: &[u16], n: usize) -> Vec<u16> {
    s.iter().copied().take_while(|&c| c != 0).take(n - 1).collect()
}

/// What a tell or reply form asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum TellUp {
    /// `ClosePanel6F4` / `ClosePanel6F8`: back to play (mode 0)
    Close,
    /// `ChatLine("/t name text")`, then closed
    Send(String),
}

/// `VIWndTell` (mode 28) and `VIWndReply` (mode 29): the same form; the
/// tell's title is word 425 and it refuses an empty name in a message box,
/// the reply's is 534 and sends whatever it has.
pub struct Tell {
    pub reply: bool,
    /// `m_mode` / `m_sel`: 0 the name, 1 the text, 2 Send
    pub sel: i32,
    pub name: Vec<u16>,
    pub text: Vec<u16>,
    pub window: Window,
    pub edit: EditBox,
    pub editing: bool,
    msg: Message,
}

impl Tell {
    pub fn new(reply: bool) -> Self {
        Tell { reply, sel: 0, name: Vec::new(), text: Vec::new(), window: Window::default(), edit: EditBox::default(), editing: false, msg: Message::default() }
    }

    /// `Open(name)` / `Start(to)`, then the focus (295): a name starts on the
    /// text; the text is emptied.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, name: &[u16]) {
        self.name.clear();
        if name.first().is_some_and(|&c| c != 0) {
            self.name = capped(name, 24);
            self.sel = 1;
        } else {
            self.sel = 0;
        }
        self.window.init(vui, r, [320, 224], 500, 8, 0);
        let title = wide(&word(vui, if self.reply { 534 } else { 425 }));
        self.window.set_title(Some(&title));
        self.window.open(true);
        self.text.clear();
        self.editing = false;
        self.msg.visible = false;
    }

    /// The editor or the message box has the keys.
    pub fn typing(&self) -> bool {
        self.editing
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<TellUp> {
        if self.editing {
            if !down {
                self.edit.key_up(key);
                return None;
            }
            match self.edit.key(vui, r, key, now, cues) {
                // 259: TakeText, then 260
                EditUp::Done(_) => {
                    match self.sel {
                        0 => {
                            self.name = capped(&self.edit.text, 24);
                            self.sel = 1;
                        }
                        1 => {
                            self.text = capped(&self.edit.text, 64);
                            self.sel = 2;
                        }
                        _ => {}
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
        if self.msg.visible {
            // 307: back to the form
            self.msg.key(key, now, cues);
            return None;
        }
        match key {
            19 | 138 => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | 139 => {
                if self.sel < 2 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            13 | 134 => return self.select(vui, r, now, cues),
            27 | 132 => {
                cues.push(8);
                return Some(TellUp::Close);
            }
            _ => {}
        }
        None
    }

    fn select(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, cues: &mut Vec<i32>) -> Option<TellUp> {
        let center = self.window.center;
        match self.sel {
            0 => {
                let name = self.name.clone();
                edit_field(&mut self.edit, 1, false, 24, center, &name, now);
                self.editing = true;
            }
            1 => {
                let text = self.text.clone();
                edit_field(&mut self.edit, 2, false, 64, center, &text, now);
                self.editing = true;
            }
            _ => {
                if !self.reply && self.name.is_empty() {
                    // "Tell Failed" / "No recipient specified."
                    let (t, x) = (wide(&word(vui, 1631)), wide(&word(vui, 1632)));
                    self.msg.setup(vui, r, &t, &x, PROMPT_OK, center, 300);
                } else {
                    let line: Vec<u16> = wide("/t ").into_iter().chain(self.name.iter().copied()).chain([b' ' as u16]).chain(self.text.iter().copied()).take(119).collect();
                    cues.push(8);
                    return Some(TellUp::Send(String::from_utf16_lossy(&line)));
                }
            }
        }
        None
    }

    /// `OnDrawSelf`, then the editor or the message box over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, cues: &mut Vec<i32>) {
        let c = |on: bool| UI_COLORS[on as usize];
        self.window.draw(vui, r);
        let w = self.window.size[0];
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 881))), 40, 1, c(self.sel == 0), 0);
        self.window.text_box(vui, r, &self.name, 140, 1, w - 180, 1, c(false), 6);
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 522))), 40, 3, c(self.sel == 1), 0);
        self.window.text_box(vui, r, &self.text, 140, 3, w - 180, 1, c(false), 6);
        self.window.text(vui, r, &wide(&word(vui, 885)), 40, 5, c(self.sel == 2), 0);
        let legend = wide(&format!("|x{}   |t{}", word(vui, 212), word(vui, 161)));
        self.window.text(vui, r, &legend, 40, 7, c(false), 0);
        self.window.tick_r(vui, r, 36, self.sel * 2 + 1, true);
        if self.editing {
            self.edit.draw(vui, r, now, cues);
        }
        if self.msg.visible {
            self.msg.draw(vui, r);
        }
    }
}

/// What a compose window asks of its parent.
#[derive(Clone, Debug, PartialEq)]
pub enum ComposeUp {
    /// `Cancel`: a reply back to its parent (260), a new gram closes the
    /// panel (`ClosePanel6E0`, back to the start menu)
    Close,
    /// the gram went (state 0 after sending): a reply's 309, else the panel
    /// closes
    Sent,
    /// `AddText(word, red)`: a field is missing
    Warn(i32),
}

/// `VIWndComposeGram`.
pub struct Compose {
    pub reply: bool,
    pub visible: bool,
    pub sel: i32,
    pub to: Vec<u16>,
    pub subject: Vec<u16>,
    pub body: Vec<u16>,
    /// `m_x7B4`: 1 once a send is seen going; the error it ended on
    x7b4: i32,
    window: Window,
    status: Window,
    pub edit: EditBox,
    pub editing: bool,
    message: Message,
    mbox: MBox,
    mbox_up: bool,
    dots: Dots,
}

impl Compose {
    pub fn new(reply: bool) -> Self {
        let mut mbox = MBox::default();
        mbox.kind = 1;
        mbox.pos = [100, 100];
        Compose { reply, visible: false, sel: 0, to: Vec::new(), subject: Vec::new(), body: Vec::new(), x7b4: 0, window: Window::default(), status: Window::default(), edit: EditBox::default(), editing: false, message: Message::default(), mbox, mbox_up: false, dots: Dots::default() }
    }

    /// 295, the focus from elsewhere: the windows, the cursor (a reply's on
    /// the body), the fields emptied.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 500, 14, 0);
        let title = wide(&word(vui, if self.reply { 894 } else { 895 }));
        self.window.set_title(Some(&title));
        self.window.open(true);
        self.status.init(vui, r, [320, 180], 375, 4, 0);
        self.status.set_title(Some(&wide(&word(vui, 271))));
        self.status.open(true);
        self.x7b4 = 0;
        self.mbox_up = false;
        self.editing = false;
        self.message.visible = false;
        self.sel = if self.reply { 2 } else { 0 };
        self.subject.clear();
        self.body.clear();
        self.to.clear();
    }

    /// `Refresh`: back to the first field.
    pub fn refresh(&mut self) {
        self.sel = 0;
    }

    pub fn set_to(&mut self, s: &[u16]) {
        self.to = capped(s, 24);
    }

    pub fn set_subject(&mut self, s: &[u16]) {
        self.subject = capped(s, 64);
    }

    pub fn set_body(&mut self, s: &[u16]) {
        self.body = capped(s, 512);
    }

    pub fn typing(&self) -> bool {
        self.editing
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &mut Grams, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<ComposeUp> {
        if self.editing {
            if !down {
                self.edit.key_up(key);
                return None;
            }
            match self.edit.key(vui, r, key, now, cues) {
                // 259: TakeText (the cursor stays), then 260
                EditUp::Done(_) => {
                    match self.sel {
                        0 => self.to = capped(&self.edit.text, 24),
                        1 => self.subject = capped(&self.edit.text, 64),
                        2 => self.body = capped(&self.edit.text, 512),
                        _ => {}
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
                // 268: yes, leave it
                Some(BoxAnswer::Yes(..)) => {
                    self.mbox_up = false;
                    return Some(self.cancel(cues));
                }
                Some(BoxAnswer::No(..)) => self.mbox_up = false,
                None => {}
            }
            return None;
        }
        if self.message.visible {
            // 307
            if self.message.key(key, now, cues).is_some() && self.x7b4 != 0 {
                self.x7b4 = 0;
            }
            return None;
        }
        let busy = grams.state > 0;
        match key {
            19 | 138 if !busy => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | 139 if !busy => {
                if self.sel < 3 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            13 | 134 if !busy => return self.select(grams, now),
            27 | 132 => {
                if self.body.is_empty() {
                    return Some(self.cancel(cues));
                }
                // "Message has not been sent.  Continue exit?"
                let t = wide(&word(vui, 866));
                self.mbox.prompt(&t, 1, 0, 0, now);
                self.mbox.pos = [100, 100];
                self.mbox_up = true;
            }
            _ => {}
        }
        None
    }

    fn select(&mut self, grams: &mut Grams, now: i64) -> Option<ComposeUp> {
        let center = self.window.center;
        match self.sel {
            0 => {
                let t = self.to.clone();
                edit_field(&mut self.edit, 1, false, 24, center, &t, now);
                self.editing = true;
            }
            1 => {
                let t = self.subject.clone();
                edit_field(&mut self.edit, 1, false, 32, center, &t, now);
                self.editing = true;
            }
            2 => {
                let t = self.body.clone();
                edit_field(&mut self.edit, 4, true, 512, center, &t, now);
                self.editing = true;
            }
            _ => {
                if self.to.is_empty() {
                    return Some(ComposeUp::Warn(1632));
                } else if self.subject.is_empty() {
                    return Some(ComposeUp::Warn(1808));
                } else if self.body.is_empty() {
                    return Some(ComposeUp::Warn(873));
                }
                // str16to8 into 24 bytes, then CapitalizeName
                let mut to: Vec<u8> = self.to.iter().take(23).map(|&c| if c < 256 { c as u8 } else { b'?' }).collect();
                to.make_ascii_uppercase();
                if to.len() > 1 {
                    to[1..].make_ascii_lowercase();
                }
                grams.send_gram(&to, &self.subject, &self.body);
            }
        }
        None
    }

    /// `Cancel`.
    fn cancel(&mut self, cues: &mut Vec<i32>) -> ComposeUp {
        cues.push(8);
        ComposeUp::Close
    }

    /// `OnDrawSelf`, then the editor, the message and the box over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &Grams, now: i64, cues: &mut Vec<i32>) -> Option<ComposeUp> {
        self.dots.step();
        let c = |on: bool| UI_COLORS[on as usize];
        vui.blit_rect(r, 32, [-1, -1], [641, 332], 3, 1.0);
        self.window.draw(vui, r);
        let w = self.window.size[0];
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 881))), 40, 1, c(self.sel == 0), 0);
        self.window.text_box(vui, r, &self.to, 140, 1, w - 180, 1, c(false), 6);
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 883))), 40, 3, c(self.sel == 1), 0);
        self.window.text_box(vui, r, &self.subject, 140, 3, w - 180, 1, c(false), 6);
        self.window.text(vui, r, &wide(&format!("{}:", word(vui, 522))), 40, 5, c(self.sel == 2), 0);
        self.window.text_box(vui, r, &self.body, 140, 5, w - 180, 5, c(false), 6);
        self.window.text(vui, r, &wide(&word(vui, 885)), 40, 11, c(self.sel == 3), 0);
        let legend = wide(&format!("|x{}   |t{}", word(vui, 212), word(vui, 213)));
        self.window.text(vui, r, &legend, 40, 13, c(false), 0);
        let row = [1, 3, 5, 11][self.sel.clamp(0, 3) as usize];
        self.window.tick_r(vui, r, 36, row, true);
        let mut up = None;
        let fail = |me: &mut Self, vui: &Vui, r: &mut dyn Raster, id: i32, x: i32| {
            if me.x7b4 == 1 {
                me.x7b4 = x;
                let (t, s) = (wide(&word(vui, 889)), wide(&word(vui, id)));
                me.message.setup(vui, r, &t, &s, PROMPT_OK, [320, 180], 300);
            }
        };
        match grams.state {
            3 => {
                let line = self.dots.line(vui, 886);
                draw_status(vui, r, &mut self.status, &line);
                self.x7b4 = 1;
            }
            1 => draw_status(vui, r, &mut self.status, &self.dots.line(vui, 844)),
            2 => draw_status(vui, r, &mut self.status, &self.dots.line(vui, 845)),
            4 => draw_status(vui, r, &mut self.status, &self.dots.line(vui, 887)),
            -1 => fail(self, vui, r, 890, -1),
            -2 => fail(self, vui, r, 891, -2),
            -3 => fail(self, vui, r, 892, -3),
            -4 => fail(self, vui, r, 1806, -4),
            // the code sets -3 here, not -5
            -5 => fail(self, vui, r, 893, -3),
            0 if self.x7b4 != 0 => {
                self.x7b4 = 0;
                cues.push(8);
                up = Some(ComposeUp::Sent);
            }
            _ => {}
        }
        if self.editing {
            self.edit.draw(vui, r, now, cues);
        }
        if self.message.visible {
            self.message.draw(vui, r);
        }
        if self.mbox_up {
            self.mbox.draw(vui, r, now);
        }
        up
    }
}

/// What the reader asks of the inbox.
#[derive(Clone, Debug, PartialEq)]
pub enum ReadUp {
    /// 260: back to the list
    Close,
    /// the reply's `AddText`
    Warn(i32),
}

/// `VIWndReadGram`: one gram, From/Subject/Received over its text in one
/// box, scrolled seven rows at a time; Enter offers Reply (its own compose,
/// addressed back with "Re: " before the subject) and Delete.
pub struct Read {
    pub visible: bool,
    pub from: Vec<u16>,
    pub subject: Vec<u16>,
    pub date: Vec<u16>,
    pub text: Vec<u16>,
    pub id: u32,
    full: Vec<u16>,
    dirty: bool,
    rows: i32,
    top: i32,
    deleting: i32,
    window: Window,
    status: Window,
    options: OptionList,
    message: Message,
    pub compose: Compose,
    keys: KeyFlash,
    dots: Dots,
}

impl Default for Read {
    fn default() -> Self {
        Read { visible: false, from: Vec::new(), subject: Vec::new(), date: Vec::new(), text: Vec::new(), id: 0, full: Vec::new(), dirty: true, rows: 0, top: 0, deleting: 0, window: Window::default(), status: Window::default(), options: OptionList::default(), message: Message::default(), compose: Compose::new(true), keys: KeyFlash::default(), dots: Dots::default() }
    }
}

impl Read {
    pub fn set_from(&mut self, s: &[u16]) {
        self.from = capped(s, 24);
        self.dirty = true;
    }

    pub fn set_subject(&mut self, s: &[u16]) {
        self.subject = capped(s, 64);
        self.dirty = true;
    }

    pub fn set_date(&mut self, s: &[u16]) {
        self.date = capped(s, 64);
        self.dirty = true;
    }

    pub fn set_text(&mut self, s: &[u16]) {
        self.text = capped(s, 512);
        self.dirty = true;
    }

    /// 295 (the focus from the inbox): the window, its option list (Reply,
    /// Delete) and the status window; every child hidden.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.close(true);
        self.window.init(vui, r, [320, 180], 440, 10, 0);
        self.window.set_title(Some(&wide(&word(vui, 935))));
        self.window.open(false);
        let lines = vec![wide(&word(vui, 534)), wide(&word(vui, 986))];
        self.options.setup(vui, r, &wide(&word(vui, 422)), lines, [320, 180], 256);
        self.status.init(vui, r, [320, 180], 375, 4, 0);
        self.status.set_title(Some(&wide(&word(vui, 271))));
        self.status.open(true);
        self.deleting = 0;
        self.options.visible = false;
        self.message.visible = false;
        self.compose.visible = false;
        self.keys.reset();
    }

    pub fn typing(&self) -> bool {
        self.compose.visible && self.compose.typing()
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &mut Grams, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<ReadUp> {
        if self.compose.visible {
            match self.compose.key(vui, r, grams, key, down, now, cues) {
                // 260 and 309: the reply put away
                Some(ComposeUp::Close) | Some(ComposeUp::Sent) => self.compose.visible = false,
                Some(ComposeUp::Warn(id)) => return Some(ReadUp::Warn(id)),
                None => {}
            }
            return None;
        }
        if !down {
            match key {
                19 | 138 => self.keys.release(138),
                20 | 139 => self.keys.release(139),
                _ => {}
            }
            return None;
        }
        if self.message.visible {
            // 307
            if self.message.key(key, now, cues).is_some() && self.deleting != 0 {
                self.deleting = 0;
            }
            return None;
        }
        if self.options.visible {
            // 308: the line, and whether it was taken
            if let Some((line, enter)) = self.options.key(key, now, cues)
                && enter
            {
                match line {
                    1 => grams.delete_message(self.id),
                    0 => {
                        self.compose.visible = true;
                        self.compose.focus(vui, r);
                        let s: Vec<u16> = wide(&format!("{}: {}", word(vui, 936), String::from_utf16_lossy(&self.subject)));
                        self.compose.refresh();
                        let from = self.from.clone();
                        self.compose.set_to(&from);
                        self.compose.set_subject(&s);
                        self.compose.set_body(&[]);
                    }
                    _ => {}
                }
            }
            return None;
        }
        if grams.state > 0 && matches!(key, 19 | 138 | 20 | 139 | 13 | 134) {
            return None;
        }
        match key {
            19 | 138 => self.keys.press(138),
            20 | 139 => self.keys.press(139),
            13 | 134 => {
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

    /// `OnDrawSelf`: the text, the legend, the status while deleting; then
    /// the children.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &Grams, now: i64, cues: &mut Vec<i32>) -> Option<ReadUp> {
        self.dots.step();
        self.window.draw(vui, r);
        let w = self.window.size[0];
        if self.dirty {
            let s = format!(
                "{}: {}\n{}: {}\n{}: {}\n\n{}",
                word(vui, 882),
                String::from_utf16_lossy(&self.from),
                word(vui, 883),
                String::from_utf16_lossy(&self.subject),
                word(vui, 884),
                String::from_utf16_lossy(&self.date),
                String::from_utf16_lossy(&self.text)
            );
            self.full = wide(&s).into_iter().take(1023).collect();
            self.rows = self.window.count_lines(vui, r, &self.full, 360);
            self.top = 0;
            self.dirty = false;
        }
        let t = if self.rows < 8 { -1.0 } else { self.top as f32 / (self.rows - 7) as f32 };
        self.window.draw_frame_vscroll(vui, r, 40, 1, w - 80, 7, t);
        self.window.text_box(vui, r, &self.full, 40, 1, w - 80, 7, UI_COLORS[0], (self.top as u32) << 24);
        let legend = wide(&format!("|x{}   |t{}", word(vui, 422), word(vui, 217)));
        self.window.text(vui, r, &legend, 40, 9, UI_COLORS[0], 0);
        let mut up = None;
        match grams.state {
            4 => {
                let line = self.dots.line(vui, 988);
                draw_status(vui, r, &mut self.status, &line);
                self.deleting = 1;
            }
            -8 if self.deleting == 1 => {
                self.deleting = -1;
                let (t, s) = (wide(&word(vui, 934)), wide(&word(vui, 893)));
                self.message.setup(vui, r, &t, &s, PROMPT_OK, [320, 180], 300);
            }
            0 if self.deleting != 0 => {
                self.deleting = 0;
                cues.push(8);
                up = Some(ReadUp::Close);
            }
            _ => {}
        }
        if self.options.visible {
            self.options.draw(vui, r);
        }
        if self.message.visible {
            self.message.draw(vui, r);
        }
        if self.compose.visible
            && matches!(self.compose.draw(vui, r, grams, now, cues), Some(ComposeUp::Sent))
        {
            // 309
            self.compose.visible = false;
        }
        if self.keys.pulse(138) {
            if self.top > 0 {
                self.top -= 1;
                cues.push(11);
            } else {
                cues.push(9);
            }
        }
        if self.keys.pulse(139) {
            if self.top < self.rows - 7 {
                self.top += 1;
                cues.push(11);
            } else {
                cues.push(9);
            }
        }
        up
    }
}

/// What the inbox asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum InboxUp {
    /// `ClosePanel6E4`: back to the start menu
    Close,
    /// the reader's reply's `AddText`
    Warn(i32),
}

/// `VIWndGramInbox` (mode 24): eleven rows of From, Subject and the date
/// received; X reads the selected one, square asks for the list again.
#[derive(Default)]
pub struct Inbox {
    pub sel: i32,
    pub top: i32,
    /// `m_open`: the text was asked for, the reader opens when it arrives
    open: bool,
    window: Window,
    status: Window,
    flash: KeyFlash,
    pub read: Read,
    dots: Dots,
}

impl Inbox {
    /// `OpenGramInbox` (mode 24): `Refresh`, then the focus (295).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, grams: &mut Grams) {
        self.refresh(grams);
        self.window.init(vui, r, [320, 180], 560, 14, 0);
        self.window.set_title(Some(&wide(&word(vui, 914))));
        self.window.open(true);
        self.status.init(vui, r, [320, 180], 375, 4, 0);
        self.status.set_title(Some(&wide(&word(vui, 271))));
        self.status.open(true);
        self.open = false;
        self.flash.reset();
        self.read.visible = false;
    }

    /// `Refresh`: the list asked for when the client is free.
    pub fn refresh(&mut self, grams: &mut Grams) {
        if grams.state <= 0 {
            grams.request_inbox();
        }
        self.top = 0;
        self.sel = 0;
    }

    pub fn typing(&self) -> bool {
        self.read.visible && self.read.typing()
    }

    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &mut Grams, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<InboxUp> {
        if self.read.visible {
            match self.read.key(vui, r, grams, key, down, now, cues) {
                Some(ReadUp::Close) => self.read.visible = false,
                Some(ReadUp::Warn(id)) => return Some(InboxUp::Warn(id)),
                None => {}
            }
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
        let busy = grams.state > 0;
        match key {
            19 | 138 if !busy => self.flash.press(138),
            20 | 139 if !busy => self.flash.press(139),
            13 | 134 if !busy => self.read_selected(vui, grams),
            81 | 113 | 133 if !busy => self.refresh(grams),
            27 | 132 => {
                cues.push(8);
                return Some(InboxUp::Close);
            }
            _ => {}
        }
        None
    }

    /// `Read`: the selected row's text asked for; its header to the reader
    /// now, its text when it arrives.
    fn read_selected(&mut self, vui: &Vui, grams: &mut Grams) {
        let _ = vui;
        if grams.count == 0 {
            return;
        }
        let h = grams.header(self.sel);
        grams.request_message(h.id);
        let from: Vec<u16> = h.from.iter().map(|&b| b as u16).collect();
        self.read.set_from(&from);
        self.read.set_subject(&h.subject);
        self.read.id = h.id;
        let date = wide(&grams.message.date.format());
        self.read.set_date(&date);
    }

    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &Grams, focused: bool, now: i64, cues: &mut Vec<i32>) {
        if !focused {
            self.window.reset_tick_pulse(now);
        }
        self.dots.step();
        vui.blit_rect(r, 32, [-1, -1], [641, 332], 3, 1.0);
        self.window.draw(vui, r);
        let w = self.window.size[0];
        let x = [58, 160, w - 150];
        let c0 = UI_COLORS[0];
        self.window.text(vui, r, &wide(&word(vui, 882)), x[0], 0, c0, 0);
        self.window.text(vui, r, &wide(&word(vui, 883)), x[1], 0, c0, 0);
        self.window.text(vui, r, &wide(&word(vui, 884)), x[2], 0, c0, 0);
        let n = grams.count;
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
        let end = if self.top + 11 > n { n } else { self.top + 11 };
        for i in self.top..end {
            let h = grams.header(i);
            let c = UI_COLORS[(self.sel == i) as usize];
            let row = i - self.top + 1;
            let from: Vec<u16> = h.from.iter().map(|&b| b as u16).collect();
            self.window.text_box(vui, r, &from, x[0], row, x[1] - x[0] - 10, 1, c, 2);
            self.window.text_box(vui, r, &h.subject, x[1], row, x[2] - x[1] - 10, 1, c, 2);
            let date: Vec<u16> = wide(&h.date.format()).into_iter().take(10).collect();
            self.window.text_box(vui, r, &date, x[2], row, w - 44 - x[2], 1, c, 2);
            if self.sel == i {
                self.window.tick_r(vui, r, 56, row, true);
            }
        }
        let legend = wide(&format!("|x{}   |t{}           |s{}", word(vui, 913), word(vui, 161), word(vui, 843)));
        self.window.text(vui, r, &legend, 40, 13, c0, 0);
        match grams.state {
            1 => draw_status(vui, r, &mut self.status, &self.dots.line(vui, 844)),
            2 => {
                draw_status(vui, r, &mut self.status, &self.dots.line(vui, 845));
                self.open = true;
            }
            3 => draw_status(vui, r, &mut self.status, &self.dots.line(vui, 888)),
            4 => draw_status(vui, r, &mut self.status, &self.dots.line(vui, 887)),
            0 if self.open => {
                self.open = false;
                let text = grams.text.clone();
                self.read.set_text(&text);
                self.read.visible = true;
                self.read.focus(vui, r);
            }
            _ => {}
        }
        if self.read.visible {
            if self.read.draw(vui, r, grams, now, cues) == Some(ReadUp::Close) {
                self.read.visible = false;
            }
        } else {
            if self.flash.pulse(138) {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            if self.flash.pulse(139) {
                if self.sel < grams.count - 1 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(id: u32, from: &str, subject: &str) -> Header {
        Header { x00: 7, id, x08: 9, from: from.as_bytes().to_vec(), subject: wide(subject), date: Date { year: 2026, month: 10, day: 19, hour: 21, minute: 5, second: 3 }, read: 0 }
    }

    fn inbox_reply(hs: &[Header], count: i64) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(2).u8(2).signed(count);
        for h in hs {
            h.write(&mut w);
        }
        w.data
    }

    /// The requests are the 0x0D01 envelope with the sub-op twice (the
    /// inner 0x0D41 as a three-byte blob), then the fields: ids zigzag, the
    /// name 8-bit and the subject and text wide, each with a u32 length.
    #[test]
    fn requests_are_the_envelope_and_the_blob() {
        let mut g = Grams::default();
        g.request_inbox();
        assert_eq!(g.out[0], [0x01, 0x0D, 2, 6, 0x41, 0x0D, 2]);
        assert_eq!(g.state, 1);
        g.request_message(5);
        assert_eq!(g.out[1], [0x01, 0x0D, 3, 6, 0x41, 0x0D, 3, 10]);
        g.send_gram(b"Bob", &wide("Hi"), &wide("yo"));
        assert_eq!(g.out[2], [0x01, 0x0D, 1, 6, 0x41, 0x0D, 1, 3, 0, 0, 0, b'B', b'o', b'b', 2, 0, 0, 0, b'H', 0, b'i', 0, 2, 0, 0, 0, b'y', 0, b'o', 0]);
        assert_eq!(g.state, 3);
    }

    /// A reply is taken only when it was waited for (the control: the same
    /// inbox while sending changes nothing); the count is the list's, the
    /// rows at most 100; the text marks its row read.
    #[test]
    fn replies_are_gated_by_the_state() {
        let mut g = Grams::default();
        let hs = [header(11, "Ann", "One"), header(12, "Bob", "Two")];
        g.send_gram(b"X", &[], &[]);
        g.recv(&inbox_reply(&hs, 2));
        assert_eq!((g.count, g.state), (-1, 3), "not waited for: ignored");
        g.request_inbox();
        g.x08 = 1;
        g.recv(&inbox_reply(&hs, 2));
        assert_eq!((g.count, g.state, g.x08), (2, 0, 0));
        assert_eq!(g.headers, hs);
        g.request_message(12);
        assert_eq!(g.message.from, b"Bob");
        let mut w = Writer::default();
        w.u8(3).u8(3).signed(12).wide("hello");
        g.recv(&w.data);
        assert_eq!((g.state, g.headers[1].read, g.headers[0].read), (0, 1, 0));
        assert_eq!(g.text, wide("hello"));
        // a failed status
        g.request_inbox();
        g.recv(&[2, 9]);
        assert_eq!(g.state, -6);
        // 150 listed: 100 kept, the count the list's
        g.request_inbox();
        let many: Vec<Header> = (0..100).map(|i| header(i, "A", "S")).collect();
        g.recv(&inbox_reply(&many, 150));
        assert_eq!((g.count, g.headers.len()), (150, 100));
        assert_eq!(g.header(120), Header::default());
    }

    /// Deleting drops the row at once and the count even for an id not
    /// listed (as shipped); the status words.
    #[test]
    fn delete_and_send_status() {
        let mut g = Grams::default();
        g.request_inbox();
        g.recv(&inbox_reply(&[header(1, "A", "a"), header(2, "B", "b"), header(3, "C", "c")], 3));
        g.delete_message(2);
        assert_eq!(g.count, 2);
        assert_eq!(g.headers.iter().map(|h| h.id).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(*g.out.last().unwrap(), [0x01, 0x0D, 4, 6, 0x41, 0x0D, 4, 4]);
        g.recv(&[4, 13]);
        assert_eq!((g.state, g.shown.clone()), (0, vec![840]));
        g.delete_message(99);
        assert_eq!((g.count, g.headers.len()), (1, 2), "the count drops, no row goes");
        g.recv(&[4, 1]);
        assert_eq!(g.state, -8);
        for (status, state) in [(1u8, 0), (9, -1), (10, -2), (11, -3), (12, -4), (7, -5)] {
            g.send_gram(b"A", &[], &[]);
            g.recv(&[1, status]);
            assert_eq!(g.state, state, "status {status}");
        }
        assert_eq!(g.shown, vec![840, 839]);
        g.recv(&[5]);
        assert_eq!(g.x08, 1);
    }

    #[test]
    fn date_formats_as_the_game() {
        assert_eq!(header(0, "", "").date.format(), "2026-10-19 21:05 PST");
    }
}
