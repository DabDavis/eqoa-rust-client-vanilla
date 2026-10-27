//! `VIWndEdit` (CLIENT `viwndedit.cpp`): the text editor a dozen start-menu
//! entries open (`VIWndGame` mode 2) — its title bar, frame and text box,
//! word-wrapped at 389 pixels with a blinking caret, and below it either the
//! pad's soft keyboard (6 rows of 11) or, with a USB keyboard and the
//! compact option, a one-line legend. A keyboard types into either.
//!
//! Which face is the game's one static, `sPadOnly`, set each frame from
//! `gKeyboardAttached` and the options' Soft Keyboard ([`set_pad_only`]): a
//! keyboard in use with the option on shows the compact face (the default
//! here, `EQOA_SOFT_KEYBOARD=1` turns the option off); a pad or the touch
//! screen in use shows the soft keyboard, driven by the d-pad, X, square
//! (back) and circle (space) (on a keyboard: the arrows, E, Backspace and
//! Space).

use vi::ui::{FrameParam, Raster, Vui, wide};

/// `sKeys`: the soft keyboard. A letter or digit types itself; 1..16 are
/// the special keys; 0 is no key. Cell 64 is the newline key, 6 while the
/// editor is multi-line.
const KEYS: [u8; 66] = [
    b'A', b'B', b'C', b'D', b'E', b'1', b'2', b'3', b'+', b'@', 5, //
    b'F', b'G', b'H', b'I', b'J', b'4', b'5', b'6', 11, 12, b'.', //
    b'K', b'L', b'M', b'N', b'O', b'7', b'8', b'9', b'*', 15, b'?', //
    b'P', b'Q', b'R', b'S', b'T', 2, b'0', b'%', b'/', b';', b'!', //
    b'U', b'V', b'W', b'X', b'Y', 1, 3, 7, b'=', b':', 5, //
    b'Z', 14, 16, 13, b'(', b')', 9, 8, 10, 0, 0,
];

/// `sPadOnly`: the compact face, no soft keyboard (1 in the game's image).
static PAD_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// `sPadOnly`, as `OnDrawSelf` sets it every frame: `gKeyboardAttached`
/// and the options' Soft Keyboard (`m_x91`).
pub fn set_pad_only(compact: bool) {
    PAD_ONLY.store(compact, std::sync::atomic::Ordering::Relaxed);
}

/// The compact face is showing.
pub fn pad_only() -> bool {
    PAD_ONLY.load(std::sync::atomic::Ordering::Relaxed)
}

/// What the editor tells its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditUp {
    None,
    /// 259: done, with its purpose (`m_x24`)
    Done(u32),
    /// 260: cancelled
    Cancel,
}

#[derive(Clone, Copy, Default)]
struct Line {
    start: usize,
    len: usize,
    brk: bool,
}

pub struct EditBox {
    pub text: Vec<u16>,
    max_len: usize,
    lines: Vec<Line>,
    cursor: usize,
    want_x: i32,
    rows: i32,
    top: i32,
    multi_line: bool,
    allow_spaces: bool,
    pub title: Vec<u16>,
    /// `m_x24`: what Done is for
    pub purpose: u32,
    /// `m_x18C`: every character drawn as a star (STATION's password
    /// fields; CLIENT never sets it)
    pub password: bool,
    pos: [i32; 2],
    /// the fade (`m_x168`), the Done key's pulse, the typed key's flash
    alpha: f32,
    pulse: f32,
    pulse_dir: i32,
    press: f32,
    pressed: [i32; 2],
    time: i64,
    last_input: i64,
    shift: bool,
    caps: bool,
    key: [i32; 2],
    last_key: [i32; 2],
    glide: f32,
    repeat_key: i32,
    repeat_delay: f32,
}

impl Default for EditBox {
    fn default() -> Self {
        let mut e = EditBox {
            text: Vec::new(),
            max_len: 64,
            lines: Vec::new(),
            cursor: 0,
            want_x: -1,
            rows: 2,
            top: 0,
            multi_line: false,
            allow_spaces: true,
            title: Vec::new(),
            purpose: 0,
            password: false,
            pos: [16, 40],
            alpha: 0.0,
            pulse: 0.0,
            pulse_dir: 1,
            press: 0.0,
            pressed: [0, 0],
            time: 0,
            last_input: 0,
            shift: false,
            caps: false,
            key: [0, 0],
            last_key: [0, 0],
            glide: 0.0,
            repeat_key: 0,
            repeat_delay: 0.0,
        };
        e.set_max_length(64);
        e
    }
}

impl EditBox {
    /// `SetMaxLength` (the terminator counts).
    pub fn set_max_length(&mut self, n: usize) {
        self.max_len = n.max(1);
        self.text.clear();
        self.top = 0;
        self.set_cursor(0);
    }

    /// `SetRows`.
    pub fn set_rows(&mut self, n: i32) {
        self.rows = n;
    }

    /// `SetMultiLine` (the soft keyboard's cell 64 is the newline key while
    /// it is on; `key_at` reads the flag).
    pub fn set_multi_line(&mut self, on: bool) {
        self.multi_line = on;
    }

    /// `SetPos`: the box's top left.
    pub fn set_pos(&mut self, p: [i32; 2]) {
        self.pos = p;
    }

    /// `SetAllowSpaces` (the character name's editor takes none).
    pub fn set_allow_spaces(&mut self, on: bool) {
        self.allow_spaces = on;
    }

    /// `GetPos`: the box's top left.
    pub fn pos(&self) -> [i32; 2] {
        self.pos
    }

    /// `GetEnd`: the box's bottom right (`m_pos + m_size`, the size its draw
    /// gives it for the face showing).
    pub fn end(&self) -> [i32; 2] {
        let h = if pad_only() { self.rows * 17 + 42 + 26 } else { self.rows * 17 + 203 };
        [self.pos[0] + 452, self.pos[1] + h]
    }

    /// `SetCenter`: the box placed round `p` for its rows.
    pub fn set_center(&mut self, p: [i32; 2]) {
        self.pos = [p[0] - 226, p[1] - ((self.rows * 17 + 203) >> 1)];
    }

    /// `Clear`.
    pub fn clear(&mut self) {
        self.text.clear();
        self.set_cursor(0);
    }

    /// `SetText`.
    pub fn set_text(&mut self, s: &[u16]) {
        self.text = s.iter().copied().take_while(|&c| c != 0).take(self.max_len - 1).collect();
        self.set_cursor(self.text.len() as isize);
    }

    /// `OnMessage` 295: the focus starts a fresh session.
    pub fn focus(&mut self, now: i64) {
        self.time = now;
        self.alpha = 0.0;
        self.pulse = 0.0;
        self.pulse_dir = 1;
        self.press = 0.0;
        self.pressed = [0, 0];
        self.caps = false;
        self.repeat_key = 0;
        self.repeat_delay = 0.0;
        self.key = [0, 0];
        self.shift = false;
    }

    /// `SetCursor`: clamped to the text; forgets the column.
    fn set_cursor(&mut self, pos: isize) -> bool {
        let c = pos.clamp(0, self.text.len() as isize) as usize;
        self.want_x = -1;
        let moved = c != self.cursor;
        self.cursor = c;
        moved
    }

    /// `InsertChar`: a space only where spaces are allowed, one short of
    /// the buffer.
    fn insert(&mut self, c: u16, now: i64) -> bool {
        if !self.allow_spaces && c == b' ' as u16 {
            return false;
        }
        if self.cursor + 1 < self.max_len && self.text.len() + 1 < self.max_len {
            self.last_input = now;
            self.text.insert(self.cursor, c);
            self.set_cursor(self.cursor as isize + 1);
            return true;
        }
        false
    }

    fn backspace(&mut self, now: i64) -> bool {
        self.last_input = now;
        if self.cursor > 0 {
            self.text.remove(self.cursor - 1);
            self.set_cursor(self.cursor as isize - 1);
            return true;
        }
        false
    }

    /// `PlaySound`: 11 for done, 9 for refused.
    fn sound(ok: bool, cues: &mut Vec<i32>) {
        cues.push(if ok { 11 } else { 9 });
    }

    /// `OnKeyDown`, in the game's codes: letters, digits and the listed
    /// punctuation type themselves; 8 back, 27/132 cancel, 13 done (a
    /// newline when multi-line), 143 done, 17-20 the caret, 136-139 the soft
    /// keyboard's cursor, 134 its key, 135 a space, 133 back, 142 shift.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, key: i32, now: i64, cues: &mut Vec<i32>) -> EditUp {
        self.repeat_key = 0;
        let c = key as u16;
        if (b'A' as i32..=b'Z' as i32).contains(&key) || (b'a' as i32..=b'z' as i32).contains(&key) || (b'0' as i32..=b'9' as i32).contains(&key) {
            let ok = self.insert(c, now);
            Self::sound(ok, cues);
        }
        match key {
            143 => {
                cues.push(11);
                return EditUp::Done(self.purpose);
            }
            4 => return EditUp::Done(self.purpose),
            32 | 33 | 34 | 37 | 39 | 40 | 41 | 43 | 44 | 45 | 46 | 47 | 58 | 59 | 61 | 63 | 64 | 95 | 126 => {
                let ok = self.insert(c, now);
                Self::sound(ok, cues);
            }
            8 => {
                let ok = self.backspace(now);
                Self::sound(ok, cues);
            }
            27 | 132 => {
                cues.push(8);
                return EditUp::Cancel;
            }
            13 => {
                if self.multi_line {
                    let ok = self.insert(b'\n' as u16, now);
                    Self::sound(ok, cues);
                } else {
                    return EditUp::Done(self.purpose);
                }
            }
            17 => {
                self.last_input = now;
                let ok = self.set_cursor(self.cursor as isize - 1);
                Self::sound(ok, cues);
            }
            18 => {
                self.last_input = now;
                let ok = self.set_cursor(self.cursor as isize + 1);
                Self::sound(ok, cues);
            }
            19 => {
                let ok = self.line_up(vui, r, now);
                Self::sound(ok, cues);
            }
            20 => {
                let ok = self.line_down(vui, r, now);
                Self::sound(ok, cues);
            }
            _ if pad_only() => {}
            142 => {
                cues.push(11);
                self.shift = true;
            }
            141 => {
                if self.multi_line {
                    let ok = self.insert(b'\n' as u16, now);
                    Self::sound(ok, cues);
                }
            }
            136..=139 => {
                let d = match key {
                    136 => [-1, 0],
                    137 => [1, 0],
                    138 => [0, -1],
                    _ => [0, 1],
                };
                self.move_key(d, cues);
                self.repeat_key = key;
                self.repeat_delay = 0.45;
            }
            135 => {
                let ok = self.insert(b' ' as u16, now);
                Self::sound(ok, cues);
            }
            133 => {
                if self.text.is_empty() {
                    cues.push(9);
                } else {
                    let ok = self.backspace(now);
                    Self::sound(ok, cues);
                }
            }
            134 => return self.press_key(vui, r, now, cues),
            _ => {}
        }
        EditUp::None
    }

    /// `OnKeyUp`: the repeat stops; R1 drops the shift.
    pub fn key_up(&mut self, key: i32) {
        if key == self.repeat_key {
            self.repeat_key = 0;
        }
        if pad_only() {
            return;
        }
        match key {
            136..=139 => {}
            142 => {
                self.shift = false;
                self.repeat_key = 0;
            }
            _ => self.repeat_key = 0,
        }
    }

    /// The soft keyboard's key under the cursor (`OnKeyDown` 134).
    fn press_key(&mut self, vui: &Vui, r: &dyn Raster, now: i64, cues: &mut Vec<i32>) -> EditUp {
        self.pressed = self.key;
        self.press = 1.0;
        let k = self.cell(self.key);
        let mut ins = |e: &mut Self, c: u8| {
            let ok = e.insert(c as u16, now);
            Self::sound(ok, cues);
        };
        match k {
            1 => {
                let ok = self.backspace(now);
                Self::sound(ok, cues);
            }
            2 => ins(self, b' '),
            3 => {
                cues.push(11);
                self.caps = !self.caps;
            }
            5 => {
                cues.push(11);
                return EditUp::Done(self.purpose);
            }
            6 => ins(self, b'\n'),
            7 => {
                self.line_up(vui, r, now);
            }
            8 => {
                self.line_down(vui, r, now);
            }
            9 => {
                self.last_input = now;
                self.set_cursor(self.cursor as isize - 1);
            }
            10 => {
                self.last_input = now;
                self.set_cursor(self.cursor as isize + 1);
            }
            11 => ins(self, b'-'),
            12 => ins(self, b'~'),
            13 => ins(self, b','),
            14 => ins(self, b'"'),
            15 => ins(self, b'_'),
            16 => ins(self, b'\''),
            c => {
                let c = if c.is_ascii_uppercase() && self.lower() { c + 32 } else { c };
                ins(self, c);
            }
        }
        EditUp::None
    }

    /// A letter key shows (and types) lower case: no shift and no caps, or
    /// caps with shift.
    fn lower(&self) -> bool {
        (!self.shift && !self.caps) || (self.caps && self.shift)
    }

    fn cell(&self, k: [i32; 2]) -> u8 {
        let c = KEYS[(k[0] + k[1] * 11) as usize];
        if k[0] + k[1] * 11 == 64 && self.multi_line { 6 } else { c }
    }

    /// A pointer on the box (frame point `p`, the client's own: the game
    /// takes the pad and the keyboard only). On a soft keyboard's key: the
    /// cursor there at once, and the key that presses it (X, 134); on the
    /// legend beside it: its key (R2 done, square back, circle a space,
    /// triangle cancel). `Some(None)` on the box elsewhere, `None` off it.
    pub fn point(&mut self, p: [f32; 2]) -> Option<Option<i32>> {
        self.point_with(p, !pad_only())
    }

    /// `point`, with the soft keyboard showing or not.
    fn point_with(&mut self, p: [f32; 2], soft: bool) -> Option<Option<i32>> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        let rows = self.rows;
        let h = if !soft { rows * 17 + 68 } else { rows * 17 + 203 };
        if x < self.pos[0] || x >= self.pos[0] + 452 || y < self.pos[1] - 28 || y >= self.pos[1] + h {
            return None;
        }
        if !soft {
            // the compact face's legend under the text: R2's Done, then
            // triangle's Cancel, each glyph with its word (a window in play
            // has the HUD's glyph notes too; the title's and Station's only
            // this)
            let (lx, ly) = (x - self.pos[0], self.pos[1] + rows * 17 + 28);
            if (ly - 2..ly + 24).contains(&y) {
                if (16..116).contains(&lx) {
                    return Some(Some(143));
                }
                if (121..231).contains(&lx) {
                    return Some(Some(132));
                }
            }
            return Some(None);
        }
        // the keys: 26-pixel cells 27 apart
        let base = [self.pos[0] + 16, self.pos[1] + rows * 17 + 28];
        let (dx, dy) = (x - base[0], y - base[1]);
        if dx >= 0 && dy >= 0 && dx % 27 < 26 && dy % 27 < 26 && dx / 27 < 11 && dy / 27 < 6 {
            let k = [dx / 27, dy / 27];
            if self.cell(k) != 0 {
                self.key = k;
                self.last_key = k;
                self.glide = 0.0;
                return Some(Some(134));
            }
        }
        // the legend, a row each 27 apart (Space's left out where spaces are)
        let lx = self.pos[0] + 320;
        let mut ly = self.pos[1] + rows * 17 + 28;
        for (k, key) in [None, Some(143), Some(133), Some(135), Some(132)].into_iter().enumerate() {
            if k == 3 && !self.allow_spaces {
                continue;
            }
            if k > 0 {
                ly += 27;
            }
            if (lx..lx + 120).contains(&x) && (ly - 2..ly + 24).contains(&y) {
                return Some(key);
            }
        }
        Some(None)
    }

    /// `MoveKey`: a step, wrapping at the edges and over empty cells.
    fn move_key(&mut self, d: [i32; 2], cues: &mut Vec<i32>) {
        cues.push(11);
        #[allow(clippy::needless_range_loop)]
        for i in 0..2 {
            if d[i] == 0 {
                continue;
            }
            self.glide = 0.0;
            if self.key != self.last_key {
                self.last_key = self.key;
            }
            self.key[i] += d[i];
            let (edge, wrap) = if d[i] < 0 { (-1, if i == 1 { 5 } else { 10 }) } else { (if i == 1 { 6 } else { 11 }, 0) };
            if self.key[i] == edge {
                self.key[i] = wrap;
                self.last_key = self.key;
            }
            while self.cell(self.key) == 0 {
                self.key[i] += d[i];
                if self.key[i] == edge {
                    self.key[i] = wrap;
                    self.last_key = self.key;
                }
            }
            if i == 0 && self.key[0] == 10 {
                self.repeat_delay = 0.25;
            }
        }
    }

    /// `WrapLines`: at a newline, or at the last space once a line passes
    /// 388 pixels (mid-word without one).
    fn wrap_lines(&mut self, vui: &Vui, r: &dyn Raster) {
        self.lines.clear();
        let t = &self.text;
        let (mut i, mut start, mut last_space) = (0usize, 0usize, 0usize);
        while i < t.len() {
            if t[i] == b'\n' as u16 {
                self.lines.push(Line { start, len: i + 1 - start, brk: true });
                i += 1;
                last_space = 0;
                start = i;
            } else {
                if t[i] == b' ' as u16 {
                    last_space = i;
                }
                if vui.string_width(r, &t[start..i]) > 388 {
                    if last_space != 0 {
                        self.lines.push(Line { start, len: last_space + 1 - start, brk: true });
                        i = last_space + 1;
                    } else {
                        self.lines.push(Line { start, len: i - start, brk: true });
                    }
                    start = i;
                    last_space = 0;
                } else {
                    i += 1;
                }
            }
        }
        if i > start {
            self.lines.push(Line { start, len: i - start, brk: false });
        }
    }

    /// `CursorLineCol`: (column, line) of the caret.
    fn cursor_line_col(&self) -> Option<[usize; 2]> {
        let mut start = 0;
        for (i, l) in self.lines.iter().enumerate() {
            if self.cursor <= start + l.len {
                return Some(if l.brk && self.cursor - start == l.len { [0, i + 1] } else { [self.cursor - start, i] });
            }
            start += l.len;
        }
        None
    }

    fn line(&self, i: usize) -> Line {
        self.lines.get(i).copied().unwrap_or_default()
    }

    fn line_text(&self, l: Line, n: usize) -> &[u16] {
        &self.text[l.start..(l.start + n).min(self.text.len())]
    }

    /// `LineUp`: the caret to the line above, at the column it wanted.
    fn line_up(&mut self, vui: &Vui, r: &dyn Raster, now: i64) -> bool {
        self.last_input = now;
        let p = self.cursor_line_col().unwrap_or([0, 0]);
        if p[1] == 0 {
            return false;
        }
        if p[1] >= self.lines.len() {
            self.cursor = (0..p[1] - 1).map(|i| self.line(i).len).sum();
            return true;
        }
        if self.want_x == -1 {
            self.want_x = vui.string_width(r, self.line_text(self.line(p[1]), p[0]));
        }
        let above = self.line(p[1] - 1);
        for i in 0..above.len {
            if self.want_x < vui.string_width(r, self.line_text(above, i + 1)) {
                self.cursor -= p[0] + (above.len - i);
                return true;
            }
        }
        self.cursor -= 1 + p[0];
        true
    }

    /// `LineDown`.
    fn line_down(&mut self, vui: &Vui, r: &dyn Raster, now: i64) -> bool {
        self.last_input = now;
        let p = self.cursor_line_col().unwrap_or([0, 0]);
        let cur = self.line(p[1]);
        if p[1] + 1 >= self.lines.len() {
            if cur.brk && p[1] < self.lines.len() {
                self.cursor += cur.len - p[0];
                return true;
            }
            return false;
        }
        if self.want_x == -1 {
            self.want_x = vui.string_width(r, self.line_text(cur, p[0]));
        }
        let below = self.line(p[1] + 1);
        for i in 0..below.len {
            if self.want_x < vui.string_width(r, self.line_text(below, i + 1)) {
                self.cursor += cur.len - p[0] + i;
                return true;
            }
        }
        self.cursor += cur.len - p[0] + below.len - 1;
        true
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, cues: &mut Vec<i32>) {
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        self.alpha = (self.alpha + dt * 5.0).min(1.0);
        let a = self.alpha;
        let text_c = [1.0, 1.0, 1.0, a];
        let shadow = [0.0, 0.0, 0.0, a];
        self.press = (self.press - dt * 5.0).max(0.0);
        if self.pulse_dir == -1 {
            self.pulse = (self.pulse - (dt + dt)).max(0.0);
            if self.pulse == 0.0 {
                self.pulse_dir = 1;
            }
        } else {
            self.pulse = (self.pulse + dt * 3.0).min(1.0);
            if self.pulse == 1.0 {
                self.pulse_dir = -1;
            }
        }
        let rows = self.rows;
        let mut size = [452, if pad_only() { rows * 17 + 42 } else { rows * 17 + 203 }];
        if pad_only() {
            size[1] += 26;
        }
        if !pad_only() {
            if self.key != self.last_key {
                self.glide += dt * 15.0;
                if self.glide >= 1.0 {
                    self.last_key = self.key;
                    self.glide = 0.0;
                }
            }
            if (136..140).contains(&self.repeat_key) {
                self.repeat_delay -= dt;
                if self.repeat_delay <= 0.0 {
                    self.repeat_delay = 0.05;
                    let d = match self.repeat_key {
                        138 => [0, -1],
                        139 => [0, 1],
                        136 => [-1, 0],
                        _ => [1, 0],
                    };
                    self.move_key(d, cues);
                }
            }
        }
        let where_ = self.pos;
        let mut pos = where_;
        if !self.title.is_empty() {
            let w = vui.string_width(r, &self.title) + 50;
            let tp = [pos[0] + (size[0] >> 1) - (w >> 1), pos[1] - 28];
            vui.blit_bar(r, &self.title, text_c, tp, 33, 34, w, 3, a, 0, true);
        }
        let fp0 = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (36, 1), bottom: (37, 0), bl: (36, 0), left: (42, 0) };
        let fp3 = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        vui.blit_rect(r, 51, pos, size, 3, a);
        vui.blit_frame(r, pos, size, &fp0, 3, a, true);
        vui.blit_fill_frame(r, [pos[0] + 16, pos[1] + 16], [420, rows * 17 + 8], &fp3, 96, 3, a, true);
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        if !pad_only() {
            // the legend under the box: Done, Cancel, Space, Newline, Back
            pos[1] += rows * 17 + 28;
            pos[0] += 325;
            let legend: [(i32, [i32; 2], i32); 5] = [(193, [-5, 4], 905), (194, [-5, 4], 909), (71, [3, 5], 908), (72, [3, 5], 907), (69, [3, 5], 753)];
            for (k, &(icon, off, w)) in legend.iter().enumerate() {
                if k == 3 && !self.allow_spaces {
                    continue;
                }
                if k > 0 {
                    pos[1] += 27;
                }
                vui.blit_ex(r, icon, [pos[0] + off[0], pos[1] + off[1]], 3, a * 0.75, 0);
                vui.draw_string_ex(r, &wide(&format!("= {}", word(w))), pos[0] + 30, pos[1], text_c, true);
            }
            // the keys
            let base = [where_[0] + 16, where_[1] + rows * 17 + 28];
            for row in 0..6 {
                for col in 0..11 {
                    let k = self.cell([col, row]);
                    let cell = [base[0] + col * 27, base[1] + row * 27];
                    if k == 0 {
                        continue;
                    }
                    vui.blit_ex(r, 107, cell, 3, a * 0.75, 0);
                    if k == 5 {
                        vui.blit_ex(r, 204, cell, 3, self.pulse * a * 0.75, 0);
                    }
                    if k >= 32 {
                        let c = if k.is_ascii_uppercase() && self.lower() { k + 32 } else { k };
                        let s = [c as u16];
                        r.draw_centered_string([cell[0] + 14, cell[1] + 12], shadow, &s, 0);
                        r.draw_centered_string([cell[0] + 13, cell[1] + 11], text_c, &s, 0);
                    } else {
                        let (icon, off, flags) = match k {
                            1 => (154, [4, 8], 0),
                            2 => (155, [4, 8], 0),
                            3 => (112, [7, 8], 0),
                            5 => (156, [4, 8], 0),
                            6 => (111, [7, 7], 0),
                            7 => (226, [7, 7], 0),
                            8 => (226, [7, 7], 2),
                            9 => (113, [7, 7], 0),
                            10 => (113, [7, 7], 1),
                            11 => (169, [9, 10], 0),
                            12 => (170, [7, 8], 0),
                            13 => (171, [10, 13], 0),
                            14 => (172, [7, 7], 0),
                            15 => (114, [8, 16], 0),
                            _ => (171, [10, 7], 0),
                        };
                        vui.blit_ex(r, icon, [cell[0] + off[0], cell[1] + off[1]], 3, a * 0.8, flags);
                    }
                    if self.pressed == [col, row] {
                        vui.blit_ex(r, 108, cell, 3, a * self.press * 0.6, 0);
                    }
                }
            }
            // the cursor ring, eased from the last key to this one
            let t = 1.0 - self.glide;
            let t = 1.0 - t * t;
            let from = [base[0] + self.last_key[0] * 27, base[1] + self.last_key[1] * 27];
            let to = [base[0] + self.key[0] * 27, base[1] + self.key[1] * 27];
            let ring = [from[0] + ((to[0] - from[0]) as f32 * t) as i32 - 4, from[1] + ((to[1] - from[1]) as f32 * t) as i32 + 5];
            vui.blit_ex(r, 89, ring, 3, a, 0);
        } else {
            let y = where_[1] + rows * 17 + 28;
            vui.blit_ex(r, 194, [where_[0] + 16 + 3, y + 4], 3, a * 0.75, 0);
            vui.draw_string_ex(r, &wide(&word(909)), where_[0] + 16 + 39, y, text_c, true);
            vui.blit_ex(r, 69, [where_[0] + 121 + 3, y + 5], 3, a * 0.75, 0);
            vui.draw_string_ex(r, &wide(&word(753)), where_[0] + 121 + 23, y, text_c, true);
            if self.multi_line {
                let s = format!("{}+{} = {}", word(905), word(906), word(909));
                vui.draw_string_ex(r, &wide(&s), where_[0] + 276, y, text_c, true);
            }
        }
        // the text, scrolled to keep the caret's line in view
        self.wrap_lines(vui, r);
        let lc = self.cursor_line_col().unwrap_or([0, 0]);
        let ly = lc[1] as i32;
        if ly < self.top {
            self.top = ly;
        } else if ly >= self.top + rows {
            self.top = ly + 1 - rows;
        }
        let mut y = self.pos[1] + 16;
        let mut caret = [self.pos[0] + 22, y + 5];
        let h = vui.font_height(r);
        for i in self.top..self.top + rows {
            let iu = i as usize;
            if iu < self.lines.len() {
                let l = self.lines[iu];
                let buf: Vec<u16> = self.text[l.start..l.start + l.len]
                    .iter()
                    .map(|&c| if c == b'\n' as u16 { b' ' as u16 } else if self.password { b'*' as u16 } else { c })
                    .collect();
                r.draw_string([self.pos[0] + 23, y + 1], shadow, &buf);
                r.draw_string([self.pos[0] + 22, y], text_c, &buf);
            }
            if lc[1] == iu {
                let l = self.line(iu);
                let n = lc[0].min(self.text.len().saturating_sub(l.start));
                // measured on the line as drawn: the stars, for a password
                let shown = if self.password { vec![b'*' as u16; n] } else { self.text[l.start..l.start + n].to_vec() };
                caret = [self.pos[0] + vui.string_width(r, &shown) + 22, y + 5];
            }
            y = y - 4 + h;
        }
        if ((now - self.last_input) / 500) & 1 == 0 {
            vui.fill_rectangle(r, [caret, [caret[0] + 2, caret[1] + h - 5]], text_c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MoveKey` steps over empty cells and wraps at the edges: from the
    /// top-left, up wraps to the bottom row's first key, left to the last
    /// column; the two empty cells at the bottom right are skipped.
    #[test]
    fn the_soft_keyboard_cursor_wraps_and_skips() {
        let mut e = EditBox::default();
        let mut cues = Vec::new();
        e.move_key([0, -1], &mut cues);
        assert_eq!(e.key, [0, 5]);
        e.key = [8, 5];
        e.move_key([1, 0], &mut cues);
        assert_eq!(e.key, [0, 5], "cells 64 and 65 are empty, so it wraps");
        e.multi_line = true;
        e.key = [8, 5];
        e.move_key([1, 0], &mut cues);
        assert_eq!(e.key, [9, 5], "multi-line: cell 64 is the newline key");
        e.key = [0, 0];
        e.move_key([-1, 0], &mut cues);
        assert_eq!(e.key, [10, 0]);
        assert!(cues.iter().all(|&c| c == 11));
    }

    /// A pointer on the soft keyboard: a key puts the cursor there and
    /// presses it, an empty cell or the gap between keys nothing, the
    /// legend its keys (Space's left out where spaces are not allowed);
    /// on the compact face its legend's two (Done, Cancel: Network Setup's
    /// submit with a mouse, F3 20261026-202044) and nothing on the text;
    /// off the box `None`.
    #[test]
    fn a_pointer_presses_the_key_or_legend_under_it() {
        let mut e = EditBox::default();
        let base = [e.pos[0] + 16, e.pos[1] + e.rows * 17 + 28];
        let at = |c: i32, r: i32| [(base[0] + c * 27 + 13) as f32, (base[1] + r * 27 + 13) as f32];
        assert_eq!(e.point_with(at(7, 1), true), Some(Some(134)));
        assert_eq!((e.key, e.cell(e.key)), ([7, 1], b'6'));
        assert_eq!(e.point_with(at(9, 5), true), Some(None), "cell 64 is empty in a one-line box");
        assert_eq!(e.point_with([at(1, 1)[0] + 13.5, at(1, 1)[1]], true), Some(None), "between two keys");
        let lx = e.pos[0] + 340;
        let legend = |row: i32| [lx as f32, (base[1] + row * 27 + 10) as f32];
        assert_eq!(e.point_with(legend(1), true), Some(Some(143)), "R2: done");
        assert_eq!(e.point_with(legend(3), true), Some(Some(135)), "circle: a space");
        e.allow_spaces = false;
        assert_eq!(e.point_with(legend(3), true), Some(Some(132)), "no Space row: triangle moves up");
        let text = [(e.pos[0] + 100) as f32, (e.pos[1] + 20) as f32];
        assert_eq!(e.point_with(text, false), Some(None), "the compact face: on the text, no key");
        let (cx, cy) = (e.pos[0], e.pos[1] + e.rows * 17 + 36);
        let compact = |dx: i32| [(cx + dx) as f32, cy as f32];
        assert_eq!(e.point_with(compact(60), false), Some(Some(143)), "the compact face's Done: R2");
        assert_eq!(e.point_with(compact(150), false), Some(Some(132)), "its Cancel: triangle");
        assert_eq!(e.point_with(compact(300), false), Some(None), "past its legend, no key");
        assert_eq!(e.point_with(at(7, 1), false), None, "the compact face is shorter: no keys there");
        assert_eq!(e.point_with([(lx - 345) as f32, at(0, 0)[1]], true), None, "left of the box");
    }

    /// `InsertChar`: spaces refused where not allowed, the buffer kept one
    /// short for the terminator.
    #[test]
    fn insert_keeps_room_for_the_terminator() {
        let mut e = EditBox::default();
        e.set_max_length(4);
        for c in b"abcd" {
            e.insert(*c as u16, 0);
        }
        assert_eq!(e.text, wide("abc"));
        e.allow_spaces = false;
        e.set_max_length(8);
        assert!(!e.insert(b' ' as u16, 0));
        assert!(e.insert(b'x' as u16, 0));
        e.backspace(0);
        assert!(e.text.is_empty() && e.cursor == 0);
    }
}
