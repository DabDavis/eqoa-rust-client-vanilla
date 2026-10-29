//! Tell-grams (the in-game mail) and the tell and reply forms.
//!
//! `Grams` is `VIClientPart2EFD0` (`viclientpart2efd0.cpp`), the gram client:
//! a request goes out as message 0x0D01 carrying a sub-op byte and, as a blob,
//! an inner 0x0D41 with the same sub-op, then its fields; replies come back
//! on 0x0D41, a sub-op and a status. Sub-ops: 1 send, 2 the inbox, 3 open a
//! gram, 4 delete, 5 (in) new mail. `m_state` is the reply waited for (1-4)
//! and goes negative on a failure. A server that offers SOE's final live
//! client's forms (`OP_LIVE_MAIL`, `Grams::live`) gets those instead: a gram
//! to several names, the ones unknown or not delivered to named back, the To
//! line in the reader and its Reply All.
//!
//! The windows: `VIWndComposeGram` (`Compose`: To, Subject, the body, Send;
//! the start menu's Mail > Compose is mode 23, and the reader and the buddy
//! list each carry one as a reply), `VIWndGramInbox` (`Inbox`, mode 24),
//! `VIWndReadGram` (`Read`, the inbox's reader) and `VIWndTell` /
//! `VIWndReply` (`Tell`, modes 28 and 29: a name and a line, then "/t name
//! line" to the chat line).
//!
//! A pointer (`pointer`) goes where the keys go: an editor's soft keyboard,
//! a box's buttons, a message's choice, the reader's option list (off it,
//! △ closes it); else a form's field is selected and the selected one used
//! (✕: its editor, or Send), and an inbox's row is selected and the
//! selected one read. Nothing a pad cannot do: not while the client waits.

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::pointer::{Hits, MOVED, key, rows_rect};
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

    pub(crate) fn read(r: &mut Reader) -> Self {
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
///
/// It speaks the disc's forms unless `live`: then SOE's final live client's
/// (`c:/eqoa-live-1500`, read off Corsten's retail state where its gram
/// client is `VIClient+0x30910`), which a server says it takes by offering
/// it (go-eqoa-server's [`OP_LIVE_MAIL`]; Sandstorm and every server built
/// for the disc never do, and keep the disc's). The live forms: a send
/// names several recipients and carries no envelope, an open carries none
/// either, a send's answer can name the recipients unknown (status 9) or
/// not delivered to and why (status 4), and an opened gram comes with its
/// To line. The inbox, delete and new-mail messages are the disc's in both.
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
    /// the server takes the live client's forms (its offer)
    pub live: bool,
    /// live: the gram opened's To line (`m_message`'s `to`, +0xB0)
    pub to: Vec<u16>,
    /// live: the last send's names (+0x08, `VIArray<char[24]>`): the ones
    /// unknown (status 9) or not delivered to (status 4)
    pub names: Vec<Vec<u8>>,
    /// live: each name's reason (+0x10), as `RecvSend` keeps it: -2 a full
    /// mailbox (10), -3 too many unread (11), -5 any other
    pub codes: Vec<i8>,
    /// live: someone got the gram (+0x18)
    pub delivered: bool,
}

/// go-eqoa-server's own, no retail message: the server takes the live
/// client's mail forms, a u32 after the opcode, both ways. The server
/// offers it (1) once the player is in the world; this client answers 1 and
/// speaks the live forms from then on (`Grams::live`). The server tells the
/// two by the request itself (a disc request carries the envelope, `06 41 0D
/// <sub>` after the sub-op, which a live one cannot), so the answer only says
/// what is coming. A PS2 ignores the offer, as it ignores 0x7E00's.
pub const OP_LIVE_MAIL: u16 = 0x7E0A;

/// `VIMAIL_RECIPIENTS_LENGTH`: the live compose's To, 512 characters with
/// its terminator (`m_to`, +0x840; the disc's took 24).
pub const RECIPIENTS: usize = 512;

impl Default for Grams {
    fn default() -> Self {
        Grams { state: 0, x08: 0, count: -1, headers: Vec::new(), message: Header::default(), text: Vec::new(), out: Vec::new(), shown: Vec::new(), live: false, to: Vec::new(), names: Vec::new(), codes: Vec::new(), delivered: false }
    }
}

/// The live client's `ParseRecipients` (0x0133DDD8): the To line to 8-bit
/// (`str16to8`, 511 characters) and upper case; letters A-Z make a name (23
/// at most, the rest dropped), a ',' or ';' ends one, an empty one is
/// dropped and every other character skipped; each name capitalised
/// (`strlwr(name + 1)`). A name given twice is kept twice, and there is no
/// cap on how many.
pub fn parse_recipients(to: &[u16]) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let bytes = crate::admin::str16to8(to);
    for c in bytes.iter().take(RECIPIENTS - 1).map(|c| c.to_ascii_uppercase()) {
        match c {
            b'A'..=b'Z' => {
                if cur.len() + 1 < 24 {
                    cur.push(c);
                }
            }
            b',' | b';' => {
                if !cur.is_empty() {
                    names.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
            _ => {}
        }
    }
    if !cur.is_empty() {
        names.push(cur);
    }
    for n in &mut names {
        n[1..].make_ascii_lowercase();
    }
    names
}

/// `VIString::strcat16(unsigned short *, size, const char *)`: 8-bit text
/// onto `out` while it stays under `size` bytes with its terminator, each
/// byte sign-extended (the shipped overload's plain assignment, `vistring.cpp`).
pub(crate) fn strcat8(out: &mut Vec<u16>, s: &[u8], size: usize) {
    for &b in s.iter().take_while(|&&b| b != 0) {
        if (out.len() + 1) * 2 >= size {
            break;
        }
        out.push(b as i8 as i16 as u16);
    }
}

/// `VIString::strcat16(unsigned short *, size, const unsigned short *)`.
pub(crate) fn strcat16(out: &mut Vec<u16>, s: &[u16], size: usize) {
    for &c in s.iter().take_while(|&&c| c != 0) {
        if (out.len() + 1) * 2 >= size {
            break;
        }
        out.push(c);
    }
}

/// The live client's `JoinNames` (0x0133DF80): the names joined with ";"
/// (0x013F7198) into `size` bytes, the terminator counted.
pub fn join_names(names: &[Vec<u8>], size: usize) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            strcat8(&mut out, b";", size);
        }
        strcat8(&mut out, n, size);
    }
    out
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
    /// listed, and the id asked for either way. Live (`RequestMessage`,
    /// 0x012F4F78): `03, zz id`, no envelope.
    pub fn request_message(&mut self, id: u32) {
        if let Some(h) = self.headers.iter().take(self.count.max(0) as usize).find(|h| h.id == id) {
            self.message = h.clone();
            self.text.clear();
            self.to.clear();
        }
        let mut w = if self.live {
            let mut w = Writer::op(0x0D01);
            w.u8(3);
            w
        } else {
            request(3)
        };
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

    /// The live sub-op 1 (`SendGram`, 0x012F5390): no envelope, a zigzag
    /// count of names and each 8-bit, then the subject and the text (wide).
    pub fn send_gram_live(&mut self, names: &[Vec<u8>], subject: &[u16], text: &[u16]) {
        self.state = 3;
        let mut w = Writer::op(0x0D01);
        w.u8(1).signed(names.len() as i64);
        for n in names {
            w.string(n);
        }
        w.wide(&String::from_utf16_lossy(subject)).wide(&String::from_utf16_lossy(text));
        self.out.push(w.data);
    }

    /// The names of a live answer (a zigzag count, then each
    /// `ReadString(char *, 24)`); a count below 0 keeps none.
    fn read_names(r: &mut Reader) -> Vec<Vec<u8>> {
        let n = r.signed();
        let mut names = Vec::new();
        for _ in 0..n.clamp(0, 4096) {
            let s = r.string(24);
            if r.error {
                break;
            }
            names.push(s);
        }
        names
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
        // live (0x012F55D8): the To line first, then the text, each in 1,024
        // bytes
        if self.live {
            self.to = wide(&r.wide(512));
        }
        self.text = wide(&r.wide(512));
    }

    /// The live `RecvSend` (0x012F56D0, its table 0x013ED750): the names,
    /// the codes and the delivered flag emptied, then by status: 1 sent (the
    /// window says so), 9 some names unknown (sent to none), 4 some not
    /// delivered to, each with its reason (10, 11 or another, kept as -2,
    /// -3, -5) and whether anyone got it, 12 the mail system busy, any other
    /// (10 and 11 too, the disc's whole-send ones) an unknown error.
    fn recv_send_live(&mut self, r: &mut Reader) {
        self.names.clear();
        self.codes.clear();
        self.delivered = false;
        self.state = match r.u8() {
            1 => 0,
            9 => {
                self.names = Self::read_names(r);
                -1
            }
            4 => {
                self.names = Self::read_names(r);
                let m = r.signed();
                for _ in 0..m.clamp(0, 4096) {
                    let c = r.u8();
                    if r.error {
                        break;
                    }
                    self.codes.push(match c {
                        10 => -2,
                        11 => -3,
                        _ => -5,
                    });
                }
                self.delivered = r.u8() != 0;
                -9
            }
            12 => -4,
            _ => -5,
        };
    }

    fn recv_send(&mut self, r: &mut Reader) {
        if self.state != 3 {
            return;
        }
        if self.live {
            return self.recv_send_live(r);
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

pub(crate) fn word(vui: &Vui, id: i32) -> String {
    vui.words.get(id, vui.language).to_string()
}

/// The "..." after a status line: one dot more every 15 frames, three at
/// most (`sDots`, `sTicks`, one pair per window class).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Dots {
    dots: i8,
    ticks: i8,
}

impl Dots {
    pub(crate) fn step(&mut self) {
        self.ticks = self.ticks.wrapping_sub(1);
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
    }

    pub(crate) fn line(&self, vui: &Vui, id: i32) -> Vec<u16> {
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
pub(crate) fn edit_field(e: &mut EditBox, rows: i32, multi: bool, max: usize, center: [i32; 2], text: &[u16], now: i64) {
    e.set_rows(rows);
    e.set_multi_line(multi);
    e.set_center(center);
    e.set_max_length(max);
    e.set_text(text);
    e.focus(now);
}

pub(crate) fn box_key(key: i32) -> Option<BoxKey> {
    match key {
        13 | 134 => Some(BoxKey::Enter),
        27 | 132 => Some(BoxKey::Escape),
        17 | 136 => Some(BoxKey::Left),
        18 | 137 => Some(BoxKey::Right),
        _ => None,
    }
}

/// `strcpy16` into a buffer of `n` characters (the terminator counts).
pub(crate) fn capped(s: &[u16], n: usize) -> Vec<u16> {
    s.iter().copied().take_while(|&c| c != 0).take(n - 1).collect()
}

/// A pointer on a form's field or a list's row (`hit`, from the window's
/// hits) whose selection is `sel`: the selection moved there as up and down
/// move it (the tick's pulse started over at `now`; `MOVED`), or ✕ on the
/// selection already there. Nothing off them.
pub(crate) fn field_click(sel: &mut i32, window: &mut Window, now: i64, hit: Option<i32>) -> Vec<i32> {
    match hit {
        Some(f) if f == *sel => vec![key::CROSS],
        Some(f) => {
            *sel = f;
            window.reset_tick_pulse(now);
            vec![MOVED]
        }
        None => Vec::new(),
    }
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
    /// the fields drawn (name, text, Send), for a pointer, and the frame's
    /// time, the tick's to restart
    hits: Hits,
    time: i64,
}

impl Tell {
    pub fn new(reply: bool) -> Self {
        Tell { reply, sel: 0, name: Vec::new(), text: Vec::new(), window: Window::default(), edit: EditBox::default(), editing: false, msg: Message::default(), hits: Hits::default(), time: 0 }
    }

    /// A pointer at frame point `p`, where the keys go: the editor's soft
    /// keyboard, the message's choice; else a field (the name, the text,
    /// Send) selected as up and down select it, and the selected one used
    /// (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.editing {
            return self.edit.point(p).flatten().into_iter().collect();
        }
        if self.msg.visible {
            return self.msg.point(p).into_iter().collect();
        }
        field_click(&mut self.sel, &mut self.window, self.time, self.hits.at(p))
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
        self.time = now;
        self.hits.clear();
        for (field, row) in [(0, 1), (1, 3), (2, 5)] {
            self.hits.push(rows_rect(&self.window, vui, r, 40, row, w - 80, 1), field);
        }
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
    /// the fields drawn (To, Subject, the body, Send), for a pointer; the
    /// frame's time; the client busy (the fields take no keys then)
    hits: Hits,
    time: i64,
    busy: bool,
    /// the live client's window (`Grams::live`, as each key and frame find
    /// it): a To of several names, the live answers' boxes and lines
    live: bool,
    /// what the window puts in the chat (`ShowText`, `AddText`): its
    /// owner's to add
    pub said: Vec<(String, [f32; 4])>,
}

/// `AddText`'s red (1, .25, .25, 1), as the game's warnings are.
const RED: [f32; 4] = [1.0, 0.25, 0.25, 1.0];
/// `ShowText`'s white.
const WHITE: [f32; 4] = [1.0; 4];

/// A live form's word (`livewords::live`: the live client's rows where the
/// table lacks them, its text for a row it changed).
fn live_word(vui: &Vui, id: i32) -> String {
    crate::livewords::live(&vui.words, id, vui.language)
}

/// The live window's names joined with ", " after `start`, in `size` bytes
/// (`strcat16`), for the unknown recipients' box and the undelivered lines.
fn names_line(start: &[u16], names: &[&Vec<u8>], size: usize) -> Vec<u16> {
    let mut s: Vec<u16> = start.iter().copied().take(size / 2 - 1).collect();
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            strcat8(&mut s, b", ", size);
        }
        strcat8(&mut s, n, size);
    }
    s
}

impl Compose {
    pub fn new(reply: bool) -> Self {
        let mut mbox = MBox::default();
        mbox.kind = 1;
        mbox.pos = [100, 100];
        Compose { reply, visible: false, sel: 0, to: Vec::new(), subject: Vec::new(), body: Vec::new(), x7b4: 0, window: Window::default(), status: Window::default(), edit: EditBox::default(), editing: false, message: Message::default(), mbox, mbox_up: false, dots: Dots::default(), hits: Hits::default(), time: 0, busy: false, live: false, said: Vec::new() }
    }

    /// `m_to`'s size in characters: the live client's 512, the disc's 24.
    fn to_size(&self) -> usize {
        if self.live { RECIPIENTS } else { 24 }
    }

    /// A pointer at frame point `p`, where the keys go: the editor's soft
    /// keyboard, the box's buttons, the message's choice; else, the client
    /// free, a field (To, Subject, the body, Send) selected as up and down
    /// select it, and the selected one used (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.editing {
            return self.edit.point(p).flatten().into_iter().collect();
        }
        if self.mbox_up {
            return self.mbox.point(p).then_some(key::CROSS).into_iter().collect();
        }
        if self.message.visible {
            return self.message.point(p).into_iter().collect();
        }
        if self.busy {
            return Vec::new();
        }
        field_click(&mut self.sel, &mut self.window, self.time, self.hits.at(p))
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
        self.to = capped(s, self.to_size());
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
        self.live = grams.live;
        if self.editing {
            if !down {
                self.edit.key_up(key);
                return None;
            }
            match self.edit.key(vui, r, key, now, cues) {
                // 259: TakeText (the cursor stays), then 260
                EditUp::Done(_) => {
                    match self.sel {
                        0 => self.to = capped(&self.edit.text, self.to_size()),
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

    /// `Select`; the live one (0x0133D8D0) edits a To of 512 and a body of
    /// 511 (the disc's 24 and 512), and sends to every name `ParseRecipients`
    /// finds, none found refused as an empty To is.
    fn select(&mut self, grams: &mut Grams, now: i64) -> Option<ComposeUp> {
        let center = self.window.center;
        match self.sel {
            0 => {
                let (t, size) = (self.to.clone(), self.to_size());
                edit_field(&mut self.edit, 1, false, size, center, &t, now);
                self.editing = true;
            }
            1 => {
                let t = self.subject.clone();
                edit_field(&mut self.edit, 1, false, 32, center, &t, now);
                self.editing = true;
            }
            2 => {
                let t = self.body.clone();
                edit_field(&mut self.edit, 4, true, if self.live { 511 } else { 512 }, center, &t, now);
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
                if self.live {
                    let names = parse_recipients(&self.to);
                    if names.is_empty() {
                        return Some(ComposeUp::Warn(1632));
                    }
                    grams.send_gram_live(&names, &self.subject, &self.body);
                    return None;
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

    /// The live state -9 (`OnDrawSelf`, 0x0133CF70), all in red: 839 when
    /// anyone got it, then 1870 and ":\n", then a line for each reason a
    /// name has, in the order {-2, -3, -5} (0x013F6FF8) with its word {891,
    /// 892, 893} (0x013F7000): its names, ", " between, then " (<word>)";
    /// each line in 512 bytes.
    fn undelivered(&mut self, vui: &Vui, grams: &Grams) {
        if grams.delivered {
            self.said.push((word(vui, 839), RED));
        }
        let mut head = Vec::new();
        strcat16(&mut head, &wide(&live_word(vui, 1870)), 512);
        strcat8(&mut head, b":\n", 512);
        self.said.push((String::from_utf16_lossy(&head), RED));
        for (code, id) in [(-2i8, 891), (-3, 892), (-5, 893)] {
            // codes[i] for each name (the game reads past a short array)
            let names: Vec<&Vec<u8>> = grams.names.iter().enumerate().filter(|(i, _)| grams.codes.get(*i) == Some(&code)).map(|(_, n)| n).collect();
            if names.is_empty() {
                continue;
            }
            let mut s = names_line(&[], &names, 512);
            strcat8(&mut s, b" (", 512);
            strcat16(&mut s, &wide(&word(vui, id)), 512);
            strcat8(&mut s, b")", 512);
            self.said.push((String::from_utf16_lossy(&s), RED));
        }
    }

    /// `OnDrawSelf`, then the editor, the message and the box over it. The
    /// live one (0x0133C7E0, its states 0x013F7020) shows a send's answer
    /// its own way: sent says 839 itself, the unknown names in the box (890
    /// and the names), the undelivered ones as red lines and then closes,
    /// and -2 and -3 are no state of its.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, grams: &Grams, now: i64, cues: &mut Vec<i32>) -> Option<ComposeUp> {
        self.live = grams.live;
        self.dots.step();
        let c = |on: bool| UI_COLORS[on as usize];
        crate::ui::window_backdrop(vui, r, 641);
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
        self.time = now;
        self.busy = grams.state > 0;
        self.hits.clear();
        for (field, row, rows) in [(0, 1, 1), (1, 3, 1), (2, 5, 5), (3, 11, 1)] {
            self.hits.push(rows_rect(&self.window, vui, r, 40, row, w - 80, rows), field);
        }
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
            -1 if self.live => {
                if self.x7b4 == 1 {
                    self.x7b4 = -1;
                    let mut start = Vec::new();
                    strcat16(&mut start, &wide(&live_word(vui, 890)), 512);
                    strcat8(&mut start, b": ", 512);
                    let s = names_line(&start, &grams.names.iter().collect::<Vec<_>>(), 512);
                    let t = wide(&word(vui, 889));
                    self.message.setup(vui, r, &t, &s, PROMPT_OK, [320, 180], 300);
                }
            }
            -2 | -3 if self.live => {}
            -9 if self.live => {
                if self.x7b4 == 1 {
                    self.x7b4 = 0;
                    self.undelivered(vui, grams);
                    cues.push(8);
                    up = Some(ComposeUp::Sent);
                }
            }
            -1 => fail(self, vui, r, 890, -1),
            -2 => fail(self, vui, r, 891, -2),
            -3 => fail(self, vui, r, 892, -3),
            -4 => fail(self, vui, r, 1806, -4),
            // the code sets -3 here, not -5
            -5 => fail(self, vui, r, 893, -3),
            0 if self.x7b4 != 0 => {
                self.x7b4 = 0;
                if self.live {
                    // ShowText(839): the live window says it, not RecvSend
                    self.said.push((word(vui, 839), WHITE));
                }
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
/// addressed back with "Re: " before the subject) and Delete. The live one
/// (with `Grams::live`) shows the gram's To line under From and offers
/// Reply All between them.
pub struct Read {
    pub visible: bool,
    pub from: Vec<u16>,
    pub subject: Vec<u16>,
    pub date: Vec<u16>,
    pub text: Vec<u16>,
    /// live: the To line (+0x191C, 512)
    pub to: Vec<u16>,
    pub id: u32,
    /// the player's own name (`GetPlayer()+4`), which Reply All puts the
    /// sender in place of
    pub me: Vec<u8>,
    /// the live window (set before its focus)
    pub live: bool,
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
        Read { visible: false, from: Vec::new(), subject: Vec::new(), date: Vec::new(), text: Vec::new(), to: Vec::new(), id: 0, me: Vec::new(), live: false, full: Vec::new(), dirty: true, rows: 0, top: 0, deleting: 0, window: Window::default(), status: Window::default(), options: OptionList::default(), message: Message::default(), compose: Compose::new(true), keys: KeyFlash::default(), dots: Dots::default() }
    }
}

impl Read {
    pub fn set_from(&mut self, s: &[u16]) {
        self.from = capped(s, 24);
        self.dirty = true;
    }

    /// The live `SetTo` (0x01398270): 1,024 bytes.
    pub fn set_to(&mut self, s: &[u16]) {
        self.to = capped(s, RECIPIENTS);
        self.dirty = true;
    }

    /// `StartReply(to)` (live 0x01397CF0; the disc's Reply inline): the
    /// compose shown with the focus (its 295), then Refresh, To, the subject
    /// "Re: <subject>" in 64 characters and an empty body.
    fn start_reply(&mut self, vui: &Vui, r: &mut dyn Raster, to: &[u16]) {
        self.compose.live = self.live;
        self.compose.visible = true;
        self.compose.focus(vui, r);
        let s: Vec<u16> = wide(&format!("{}: {}", word(vui, 936), String::from_utf16_lossy(&self.subject)));
        self.compose.refresh();
        self.compose.set_to(to);
        self.compose.set_subject(&s);
        self.compose.set_body(&[]);
    }

    /// Reply All (live `OnMessage`, 0x0139804C): the gram's To line parsed
    /// as the compose parses one, the first name that is the player's own
    /// (`strcasecmp`) made the sender's, or the sender added at the end
    /// when none is; joined with ";" in 1,024 bytes.
    pub fn reply_all_to(&self) -> Vec<u16> {
        let mut names = parse_recipients(&self.to);
        let from = crate::admin::str16to8(&self.from).into_iter().take(23).collect::<Vec<u8>>();
        match names.iter().position(|n| n.eq_ignore_ascii_case(&self.me)) {
            Some(i) => names[i] = from,
            None => names.push(from),
        }
        join_names(&names, 2 * RECIPIENTS)
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
    /// Delete; the live one's Reply, Reply All, Delete, 0x01397E1C) and the
    /// status window; every child hidden.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.close(true);
        self.window.init(vui, r, [320, 180], 440, 10, 0);
        self.window.set_title(Some(&wide(&word(vui, 935))));
        self.window.open(false);
        let lines = if self.live { vec![wide(&word(vui, 534)), wide(&live_word(vui, 1871)), wide(&word(vui, 986))] } else { vec![wide(&word(vui, 534)), wide(&word(vui, 986))] };
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

    /// A pointer at frame point `p`, where the keys go: the reply's compose,
    /// the message's choice, the option list (a line taken at once; off it,
    /// △ closes it). The text itself takes nothing (the wheel scrolls it,
    /// its legend's ✕ offers the options). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.compose.visible {
            return self.compose.point(p);
        }
        if self.message.visible {
            return self.message.point(p).into_iter().collect();
        }
        if self.options.visible {
            return match self.options.point(p) {
                Some(k) => k.into_iter().collect(),
                None => vec![key::TRIANGLE],
            };
        }
        Vec::new()
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
                match (line, self.live) {
                    (1, false) | (2, true) => grams.delete_message(self.id),
                    (0, _) => {
                        let from = self.from.clone();
                        self.start_reply(vui, r, &from);
                    }
                    (1, true) => {
                        let to = self.reply_all_to();
                        self.start_reply(vui, r, &to);
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
            let from = format!("{}: {}\n", word(vui, 882), String::from_utf16_lossy(&self.from));
            // live (0x01397730): the To line under From, in 2,480 bytes
            let (to, size) = if self.live { (format!("{}: {}\n", word(vui, 881), String::from_utf16_lossy(&self.to)), 2480) } else { (String::new(), 2048) };
            let s = format!(
                "{from}{to}{}: {}\n{}: {}\n\n{}",
                word(vui, 883),
                String::from_utf16_lossy(&self.subject),
                word(vui, 884),
                String::from_utf16_lossy(&self.date),
                String::from_utf16_lossy(&self.text)
            );
            self.full = wide(&s).into_iter().take(size / 2 - 1).collect();
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
    /// the rows drawn, by index, for a pointer; the frame's time; the client
    /// busy (the rows take no keys then)
    hits: Hits,
    time: i64,
    busy: bool,
}

impl Inbox {
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
        crate::ui::window_backdrop(vui, r, 641);
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
        self.time = now;
        self.busy = grams.state > 0;
        self.hits.clear();
        for i in self.top..end {
            let h = grams.header(i);
            let c = UI_COLORS[(self.sel == i) as usize];
            let row = i - self.top + 1;
            self.hits.push(rows_rect(&self.window, vui, r, x[0], row, w - 44 - x[0], 1), i);
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
                // live (0x013610B8): its To line too
                self.read.live = grams.live;
                if grams.live {
                    self.read.set_to(&grams.to);
                }
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
                self.prev(grams.count, now, cues);
            }
            if self.flash.pulse(139) {
                self.next(grams.count, now, cues);
            }
        }
    }

    /// The live `Prev` (0x01361618): up a row, or from the top round to
    /// the last (the first with none listed) and the key's repeat let go
    /// (`VIKeyFlash::Reset`); the tick restarted and sound 11 either way.
    /// The disc's stopped at the top, with sound 9.
    fn prev(&mut self, count: i32, now: i64, cues: &mut Vec<i32>) {
        if self.sel > 0 {
            self.sel -= 1;
        } else {
            self.sel = (count - 1).max(0);
            self.flash.reset();
        }
        self.window.reset_tick_pulse(now);
        cues.push(11);
    }

    /// The live `Next` (0x01361590): down a row, or from the last round to
    /// the first, as `prev`.
    fn next(&mut self, count: i32, now: i64, cues: &mut Vec<i32>) {
        if self.sel < count - 1 {
            self.sel += 1;
        } else {
            self.sel = 0;
            self.flash.reset();
        }
        self.window.reset_tick_pulse(now);
        cues.push(11);
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

    use crate::pointer::testing::{NullRaster, mid, vui};

    /// The tell form: a field is selected by a click (the tick restarted)
    /// and used by a second; nothing off the fields; while the message box
    /// is up the fields take nothing and its choice takes the click; while
    /// the editor is up (the compact face here) neither do they.
    #[test]
    fn a_pointer_on_the_tell_form() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut cues = Vec::new();
        let mut t = Tell::new(false);
        t.open(&vui, &r, &wide("Bob"));
        assert_eq!(t.sel, 1, "a name given: on the text");
        t.draw(&vui, &mut r, 1000, &mut cues);
        let w = t.window.size[0];
        let field = |t: &Tell, row: i32| mid(rows_rect(&t.window, &vui, &NullRaster, 40, row, w - 80, 1));
        assert_eq!(t.point(field(&t, 1)), vec![MOVED]);
        assert_eq!((t.sel, t.window.pulse_time), (0, 1000), "the name, the tick started over");
        assert_eq!(t.point(field(&t, 1)), vec![key::CROSS], "the selection: its editor");
        assert_eq!(t.point(field(&t, 5)), vec![MOVED]);
        assert_eq!(t.sel, 2, "Send");
        assert!(t.point([5.0, 5.0]).is_empty(), "off the form");
        assert!(t.point(field(&t, 7)).is_empty(), "the legend's row is no field");
        // the message box: its choice, not the fields. Its window is 300
        // wide round the form's centre (320, 224), four rows and titled
        // (96 high), so its OK row (3) is centred on y 241
        t.msg.setup(&vui, &mut r, &wide("Tell Failed"), &wide("x"), PROMPT_OK, t.window.center, 300);
        t.draw(&vui, &mut r, 1016, &mut cues);
        assert!(t.point(field(&t, 1)).is_empty());
        assert_eq!(t.sel, 2, "the fields took nothing");
        assert_eq!(t.point([310.0, 241.0]), vec![key::CROSS]);
        // the editor
        t.msg.visible = false;
        t.editing = true;
        assert!(t.point(field(&t, 1)).is_empty());
        assert_eq!(t.sel, 2);
    }

    /// The compose window: four fields (the body five rows), each selected
    /// and then used; while the client waits nothing; the box's Yes.
    #[test]
    fn a_pointer_on_the_compose_window() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut cues = Vec::new();
        let mut g = Grams::default();
        let mut c = Compose::new(false);
        c.visible = true;
        c.focus(&vui, &r);
        c.draw(&vui, &mut r, &g, 2000, &mut cues);
        let w = c.window.size[0];
        let at = |c: &Compose, row: i32, rows: i32| mid(rows_rect(&c.window, &vui, &NullRaster, 40, row, w - 80, rows));
        assert_eq!(c.point(at(&c, 5, 5)), vec![MOVED], "the body's middle row");
        assert_eq!(c.sel, 2);
        assert_eq!(c.point(at(&c, 9, 1)), vec![key::CROSS], "its last row is the body too");
        assert_eq!(c.point(at(&c, 11, 1)), vec![MOVED]);
        assert_eq!(c.sel, 3, "Send");
        assert!(c.point(at(&c, 13, 1)).is_empty(), "the legend's row");
        // sending: the fields take nothing
        g.state = 3;
        c.draw(&vui, &mut r, &g, 2016, &mut cues);
        assert!(c.point(at(&c, 1, 1)).is_empty());
        assert_eq!(c.sel, 3);
        // the box at (100, 100): its Yes at the first quarter of its width,
        // the fields not
        g.state = 0;
        c.mbox.prompt(&wide("x"), 1, 0, 0, 2032);
        c.mbox_up = true;
        c.draw(&vui, &mut r, &g, 2032, &mut cues);
        assert!(c.point(at(&c, 1, 1)).is_empty());
        assert_eq!(c.point([134.0, 146.0]), vec![key::CROSS]);
    }

    /// The inbox: rows selected and read; the rows not listed and a busy
    /// client take nothing; the reader's option list takes a line at once
    /// and off it is closed (△).
    #[test]
    fn a_pointer_on_the_inbox_and_the_reader() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut cues = Vec::new();
        let mut g = Grams::default();
        let mut ib = Inbox::default();
        ib.open(&vui, &r, &mut g);
        g.state = 0;
        g.count = 3;
        g.headers = vec![header(1, "Ann", "a"), header(2, "Bob", "b"), header(3, "Cy", "c")];
        ib.draw(&vui, &mut r, &g, true, 3000, &mut cues);
        let w = ib.window.size[0];
        let row = |ib: &Inbox, row: i32| mid(rows_rect(&ib.window, &vui, &NullRaster, 58, row, w - 44 - 58, 1));
        assert_eq!(ib.point(row(&ib, 2)), vec![MOVED]);
        assert_eq!(ib.sel, 1);
        assert_eq!(ib.point(row(&ib, 2)), vec![key::CROSS], "read");
        assert!(ib.point(row(&ib, 4)).is_empty(), "three listed: no fourth");
        g.state = 2;
        ib.draw(&vui, &mut r, &g, true, 3016, &mut cues);
        assert!(ib.point(row(&ib, 1)).is_empty(), "waiting: nothing");
        assert_eq!(ib.sel, 1);
        // the reader up: the rows behind it take nothing; its options do
        g.state = 0;
        ib.read.visible = true;
        ib.read.focus(&vui, &r);
        ib.draw(&vui, &mut r, &g, true, 3032, &mut cues);
        assert!(ib.point(row(&ib, 1)).is_empty());
        assert_eq!(ib.sel, 1);
        ib.read.options.visible = true;
        ib.draw(&vui, &mut r, &g, true, 3048, &mut cues);
        // the option list: 300 wide round (320, 180), titled, a line a row
        // from row 1 (centred on y 167): Delete is row 2
        assert_eq!(ib.point([310.0, 179.0]), vec![key::CROSS]);
        assert_eq!(ib.read.options.sel, 1);
        assert_eq!(ib.point([10.0, 10.0]), vec![key::TRIANGLE], "off it: closed");
    }

    /// The disc's mail words (the other rows numbered), unfilled: the live
    /// rows (1870, 1871) and the live 890 come from `livewords`.
    fn mail_vui() -> Vui {
        let mut lines: Vec<String> = (1..=1814).map(|i| format!("w{i}")).collect();
        for (id, s) in [(422, "Options"), (534, "Reply"), (839, "Your mail has been sent."), (881, "To"), (882, "From"), (883, "Subject"), (884, "Received"), (889, "Send Failed"), (890, "Unknown recipient."), (891, "Recipient's mailbox is full."), (892, "Recipient has too many unread messages from you."), (893, "Unknown error."), (936, "Re"), (986, "Delete")] {
            lines[id - 1] = s.into();
        }
        Vui::new(vi::strtab::StringTable::parse(lines.join("\r\n").as_bytes()).unwrap(), Default::default(), Vec::new())
    }

    fn live() -> Grams {
        Grams { live: true, ..Default::default() }
    }

    /// 0x0D41 sub-op 1 as a live server writes it: the status, then for 9
    /// and 4 the names, for 4 the codes and the delivered byte.
    fn send_answer(status: u8, names: &[&str], codes: &[u8], delivered: u8) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(1).u8(status);
        if status == 9 || status == 4 {
            w.signed(names.len() as i64);
            for n in names {
                w.string(n.as_bytes());
            }
        }
        if status == 4 {
            w.signed(codes.len() as i64);
            for &c in codes {
                w.u8(c);
            }
            w.u8(delivered);
        }
        w.data
    }

    /// The live forms (`SendGram` 0x012F5390, `RequestMessage` 0x012F4F78):
    /// a send carries no envelope, a zigzag count and the names 8-bit; an
    /// open no envelope; the inbox and delete keep theirs, as the live
    /// client's do. A disc request has 06 41 0D after its sub-op, which a
    /// live one cannot (the server's rule). The control: `live` off sends
    /// the disc's (`requests_are_the_envelope_and_the_blob`).
    #[test]
    fn live_requests_drop_the_envelope() {
        let mut g = live();
        g.request_message(5);
        assert_eq!(g.out[0], [0x01, 0x0D, 3, 10]);
        g.send_gram_live(&[b"Ann".to_vec(), b"Bob".to_vec()], &wide("Hi"), &wide("yo"));
        assert_eq!(g.out[1], [0x01, 0x0D, 1, 4, 3, 0, 0, 0, b'A', b'n', b'n', 3, 0, 0, 0, b'B', b'o', b'b', 2, 0, 0, 0, b'H', 0, b'i', 0, 2, 0, 0, 0, b'y', 0, b'o', 0]);
        assert_eq!(g.state, 3);
        g.request_inbox();
        g.delete_message(7);
        assert_eq!(g.out[2], [0x01, 0x0D, 2, 6, 0x41, 0x0D, 2], "the inbox keeps its envelope");
        assert_eq!(g.out[3], [0x01, 0x0D, 4, 6, 0x41, 0x0D, 4, 14], "and so does delete");
        for m in &g.out[..2] {
            assert_ne!(m.get(3..6), Some(&[0x06, 0x41, 0x0D][..]), "no envelope for the server to find");
        }
        let mut d = Grams::default();
        d.request_message(5);
        assert_eq!(d.out[0][3..6], [0x06, 0x41, 0x0D], "the control: the disc's has it");
    }

    /// The live `RecvSend` (0x012F56D0): 1 sent, nothing shown (the window
    /// says it); 9 the unknown names; 4 the names not delivered to, each
    /// reason kept as -2 (10), -3 (11) or -5 (any other), and whether anyone
    /// got it; 12 busy; any other an unknown error, 10 and 11 too. Each
    /// answer empties the last one's names first. The control: the disc's
    /// 10 is a state of its own.
    #[test]
    fn live_send_answers() {
        let mut g = live();
        let send = |g: &mut Grams| g.send_gram_live(&[b"A".to_vec()], &wide("s"), &wide("b"));
        send(&mut g);
        g.recv(&send_answer(9, &["Ann", "Cy"], &[], 0));
        assert_eq!((g.state, g.names.clone()), (-1, vec![b"Ann".to_vec(), b"Cy".to_vec()]));
        send(&mut g);
        g.recv(&send_answer(4, &["Ann", "Bob", "Cy", "Di"], &[10, 11, 7, 99], 1));
        assert_eq!((g.state, g.codes.clone(), g.delivered, g.names.len()), (-9, vec![-2, -3, -5, -5], true, 4));
        send(&mut g);
        g.recv(&send_answer(1, &[], &[], 0));
        assert_eq!((g.state, g.names.len(), g.codes.len(), g.delivered), (0, 0, 0, false), "emptied first");
        assert!(g.shown.is_empty(), "the live RecvSend shows nothing");
        for (status, state) in [(12u8, -4), (10, -5), (11, -5), (2, -5)] {
            send(&mut g);
            g.recv(&send_answer(status, &[], &[], 0));
            assert_eq!(g.state, state, "status {status}");
        }
        g.recv(&send_answer(9, &["Zed"], &[], 0));
        assert_eq!((g.state, g.names.len()), (-5, 0), "not waited for: ignored");
        let mut d = Grams::default();
        d.send_gram(b"A", &[], &[]);
        d.recv(&[1, 10]);
        assert_eq!(d.state, -2, "the control: the disc's");
    }

    /// A live gram opened comes with its To line, read before its text
    /// (0x012F55D8). The control: the disc's reads the first string as the
    /// text.
    #[test]
    fn a_live_gram_comes_with_its_to_line() {
        let mut w = Writer::default();
        w.u8(3).u8(3).signed(12).wide("Ann;Bob").wide("hello");
        for (mut g, to, text) in [(live(), "Ann;Bob", "hello"), (Grams::default(), "", "Ann;Bob")] {
            g.request_inbox();
            g.recv(&inbox_reply(&[header(12, "Bob", "Hi")], 1));
            g.request_message(12);
            g.recv(&w.data);
            assert_eq!((g.to.clone(), g.text.clone(), g.headers[0].read), (wide(to), wide(text), 1), "live {}", g.live);
        }
    }

    /// `ParseRecipients` (0x0133DDD8): letters make names (A-Z after upper
    /// casing, 23 at most), ',' and ';' end them, the rest is skipped;
    /// capitalised; empty names dropped, one given twice kept twice.
    #[test]
    fn recipients_parse_as_the_live_client() {
        let p = |s: &str| parse_recipients(&wide(s)).into_iter().map(|n| String::from_utf8(n).unwrap()).collect::<Vec<_>>();
        assert_eq!(p("bob, ANN;;  carl-x;d4e,;"), ["Bob", "Ann", "Carlx", "De"]);
        assert_eq!(p("Ann;ann"), ["Ann", "Ann"]);
        assert_eq!(p(" ,;;"), Vec::<String>::new());
        assert_eq!(p(&"a".repeat(30)), ["A".to_string() + &"a".repeat(22)]);
        assert_eq!(p("Zo\u{eb} Bo"), ["Zobo"], "a letter past A-Z is skipped, a space too");
    }

    /// `JoinNames` (0x0133DF80): ";" between, in the bytes given.
    #[test]
    fn names_join_with_semicolons() {
        assert_eq!(join_names(&[b"Ann".to_vec(), b"Bob".to_vec()], 1024), wide("Ann;Bob"));
        assert_eq!(join_names(&[], 1024), Vec::<u16>::new());
        let many: Vec<Vec<u8>> = (0..100).map(|_| b"Abcdefghij".to_vec()).collect();
        assert_eq!(join_names(&many, 1024).len(), 511, "1,024 bytes");
    }

    /// Reply All (0x0139804C): the sender in the first place that is the
    /// player's own (any case), else last; an empty To line is the sender
    /// alone, as Reply.
    #[test]
    fn reply_all_puts_the_sender_in_my_place() {
        let mut rd = Read::default();
        rd.me = b"Me".to_vec();
        rd.set_from(&wide("Ann"));
        let mut to = |s: &str| {
            rd.set_to(&wide(s));
            String::from_utf16_lossy(&rd.reply_all_to())
        };
        assert_eq!(to("me;Bob;Cy"), "Ann;Bob;Cy");
        assert_eq!(to("Bob;Me;Me"), "Bob;Ann;Me", "the first only");
        assert_eq!(to("Bob, Cy"), "Bob;Cy;Ann", "not in it: the sender last");
        assert_eq!(to(""), "Ann");
    }

    /// The live compose: a To of several names (512 long), sent to each
    /// `ParseRecipients` finds; a To with none refused as an empty one. Then
    /// the answers: the unknown names in the box (the live 890 and ": "),
    /// the undelivered ones as red lines (839 when anyone got it, 1870 and
    /// ":\n", each reason's names with its word) and the window closed, a
    /// sent one 839 said by the window. The control: the disc's window says
    /// nothing at 0 (its RecvSend did).
    #[test]
    fn the_live_compose_sends_to_several_and_shows_what_came_back() {
        let (vui, mut r) = (mail_vui(), NullRaster);
        let mut cues = Vec::new();
        let compose = |g: &mut Grams, to: &str| {
            let mut c = Compose::new(false);
            c.visible = true;
            c.focus(&vui, &NullRaster);
            c.key(&vui, &mut NullRaster, g, 0, false, 0, &mut Vec::new());
            c.set_to(&wide(to));
            c.subject = wide("Hi");
            c.body = wide("yo");
            c.sel = 3;
            c
        };
        let mut g = live();
        let mut c = compose(&mut g, ",;");
        assert_eq!(c.key(&vui, &mut r, &mut g, 13, true, 0, &mut cues), Some(ComposeUp::Warn(1632)));
        assert!(g.out.is_empty(), "no name: nothing sent");
        let long: String = (0..40).map(|i| format!("N{i};")).collect();
        let mut c = compose(&mut g, &long);
        assert_eq!(c.to.len(), long.len().min(511), "the live To is 512 long");
        let mut c2 = compose(&mut g, "ann, Cy;bob");
        c2.key(&vui, &mut r, &mut g, 13, true, 0, &mut cues);
        let sent = g.out.pop().unwrap();
        assert_eq!(sent[..4], [0x01, 0x0D, 1, 6], "three names");
        c = c2;
        // sending, then the unknown names: the box
        c.draw(&vui, &mut r, &g, 0, &mut cues);
        g.recv(&send_answer(9, &["Cy", "Bob"], &[], 0));
        assert_eq!(c.draw(&vui, &mut r, &g, 16, &mut cues), None, "the window stays");
        let (title, text) = c.message.texts();
        assert_eq!((String::from_utf16_lossy(title), String::from_utf16_lossy(text)), ("Send Failed".into(), "Unknown recipient(s): Cy, Bob".into()));
        // sent again: some not delivered to
        c.message.visible = false;
        c.key(&vui, &mut r, &mut g, 13, true, 0, &mut cues);
        c.draw(&vui, &mut r, &g, 32, &mut cues);
        g.recv(&send_answer(4, &["Ann", "Bob", "Cy"], &[10, 50, 10], 1));
        assert_eq!(c.draw(&vui, &mut r, &g, 48, &mut cues), Some(ComposeUp::Sent), "closed");
        let said: Vec<(String, [f32; 4])> = std::mem::take(&mut c.said);
        assert_eq!(
            said,
            [
                ("Your mail has been sent.".to_string(), RED),
                ("Unable to deliver to the following recipients:\n".to_string(), RED),
                ("Ann, Cy (Recipient's mailbox is full.)".to_string(), RED),
                ("Bob (Unknown error.)".to_string(), RED),
            ]
        );
        // sent: the window says so
        let mut c = compose(&mut g, "Ann");
        c.key(&vui, &mut r, &mut g, 13, true, 0, &mut cues);
        c.draw(&vui, &mut r, &g, 64, &mut cues);
        g.recv(&send_answer(1, &[], &[], 0));
        assert_eq!(c.draw(&vui, &mut r, &g, 80, &mut cues), Some(ComposeUp::Sent));
        assert_eq!(c.said, [("Your mail has been sent.".to_string(), WHITE)]);
        // the control: the disc's
        let mut d = Grams::default();
        let mut c = compose(&mut d, "ann");
        c.key(&vui, &mut r, &mut d, 13, true, 0, &mut cues);
        assert_eq!(d.out[0][..7], [0x01, 0x0D, 1, 6, 0x41, 0x0D, 1], "one name, the envelope");
        assert_eq!(d.out[0][7..14], [3, 0, 0, 0, b'A', b'n', b'n']);
        c.draw(&vui, &mut r, &d, 96, &mut cues);
        d.recv(&[1, 1]);
        assert_eq!(c.draw(&vui, &mut r, &d, 112, &mut cues), Some(ComposeUp::Sent));
        assert!(c.said.is_empty());
        assert_eq!(d.shown, [839], "RecvSend said it");
        assert_eq!(compose(&mut d, &long).to.len(), 23, "the disc's To is 24 long");
    }

    /// The live reader: its text has the To line under From, its options
    /// are Reply, Reply All and Delete; Reply All opens the reply addressed
    /// to everyone but the player, and the sender. The control: the disc's
    /// has no To line and Delete second.
    #[test]
    fn the_live_reader_replies_to_all() {
        let (vui, mut r) = (mail_vui(), NullRaster);
        let mut cues = Vec::new();
        let mut g = live();
        let mut rd = Read { live: true, me: b"Me".to_vec(), ..Default::default() };
        rd.focus(&vui, &r);
        rd.set_from(&wide("Ann"));
        rd.set_to(&wide("Bob;Me"));
        rd.set_subject(&wide("Plans"));
        rd.set_text(&wide("hi all"));
        rd.visible = true;
        rd.draw(&vui, &mut r, &g, 0, &mut cues);
        assert!(String::from_utf16_lossy(&rd.full).starts_with("From: Ann\nTo: Bob;Me\nSubject: Plans\nReceived: \n\nhi all"));
        rd.key(&vui, &mut r, &mut g, 13, true, 0, &mut cues);
        for k in [139, 134] {
            rd.key(&vui, &mut r, &mut g, k, true, 0, &mut cues);
        }
        assert!(rd.compose.visible, "Reply All: the reply");
        assert_eq!((String::from_utf16_lossy(&rd.compose.to), String::from_utf16_lossy(&rd.compose.subject)), ("Bob;Ann".into(), "Re: Plans".into()));
        // Delete is the third
        rd.compose.visible = false;
        rd.key(&vui, &mut r, &mut g, 13, true, 0, &mut cues);
        for k in [139, 139, 134] {
            rd.key(&vui, &mut r, &mut g, k, true, 0, &mut cues);
        }
        assert_eq!(g.out.pop().map(|m| m[2]), Some(4), "deleted");
        // the control: the disc's reader
        let mut d = Grams::default();
        let mut rd = Read { me: b"Me".to_vec(), ..Default::default() };
        rd.focus(&vui, &r);
        rd.set_from(&wide("Ann"));
        rd.set_to(&wide("Bob;Me"));
        rd.draw(&vui, &mut r, &d, 0, &mut cues);
        assert!(String::from_utf16_lossy(&rd.full).starts_with("From: Ann\nSubject: "));
        rd.key(&vui, &mut r, &mut d, 13, true, 0, &mut cues);
        for k in [139, 134] {
            rd.key(&vui, &mut r, &mut d, k, true, 0, &mut cues);
        }
        assert!(!rd.compose.visible && d.out.pop().map(|m| m[2]) == Some(4), "the disc's second line deletes");
    }

    /// The live inbox's Next and Prev (0x01361590, 0x01361618) go round:
    /// from the top to the last row, from the last to the first, the key's
    /// repeat let go, sound 11 (the disc's stopped there with 9). Between
    /// the ends they move as before.
    #[test]
    fn the_inbox_goes_round() {
        let (vui, mut r) = (mail_vui(), NullRaster);
        let mut g = Grams::default();
        let mut ib = Inbox::default();
        ib.open(&vui, &r, &mut g);
        g.state = 0;
        g.count = 3;
        g.headers = vec![header(1, "Ann", "a"), header(2, "Bob", "b"), header(3, "Cy", "c")];
        let mut step = |ib: &mut Inbox, key: u8, now: i64| {
            let mut cues = Vec::new();
            ib.flash.press(key);
            ib.draw(&vui, &mut r, &g, true, now, &mut cues);
            ib.flash.release(key);
            (ib.sel, cues)
        };
        assert_eq!(step(&mut ib, 138, 0), (2, vec![11]), "up from the top: the last");
        assert_eq!(step(&mut ib, 138, 16), (1, vec![11]), "up a row");
        assert_eq!(step(&mut ib, 139, 32), (2, vec![11]));
        assert_eq!(step(&mut ib, 139, 48), (0, vec![11]), "down from the last: the first");
        ib.flash.press(139);
        ib.sel = 2;
        let mut cues = Vec::new();
        ib.draw(&vui, &mut r, &g, true, 64, &mut cues);
        assert_eq!(ib.sel, 0);
        ib.draw(&vui, &mut r, &g, true, 80, &mut cues);
        assert_eq!((ib.sel, cues.len()), (0, 1), "the held key let go at the wrap");
    }
}
