//! STATION.PAK's pages: the GUI objects (`VIGUIObject`, `VIGUIText`,
//! `VIGUIButton`, `VIGUIToggleButton`, `VIGUIComboBox`), the page that holds
//! them over its selection grid with a pulsing selector (`VIGUIPage`,
//! `VIGUISelectionGrid`), and the PS2GUIPage parser that builds one
//! (`VIGUIParser`'s handlers over `VIXMLParser`).
//!
//! Each object is one record here with its class (`Class`) instead of five
//! types: the decomp's virtuals are matches on it. What an object asks of the
//! renderer (a page to load, an action, the editor) comes back as `Out`s,
//! which the renderer carries out after the page is done with the key, as
//! the game's calls return to it.

use super::xml::{self, Element, Kind as Ev};
use crate::pointer::Hits;
use crate::ui::Layout;
use vi::ui::{FrameParam, Vui, Window, wide};

/// `VIGUIObject::m_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Text = 1,
    Toggle = 2,
    Object = 3,
    Button = 4,
    Combo = 5,
}

/// What a key or a message on an object asks of the renderer.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// `VIUI::SoundPlay(id)`
    Sound(i32),
    /// `PageBack` (Triangle on a page)
    Back,
    /// `LoadPage(name)`
    LoadPage(String),
    /// `OnAction(name)`
    Action(String),
    /// the renderer's editor opened on a text (`VIGUIText::OnKeyDown`)
    Edit { text: String, password: bool, max_len: usize, title: String },
}

/// One page element: `VIGUIObject` and, by its class, the text's, the
/// button's, the toggle's and the combo box's fields.
#[derive(Clone, Debug)]
pub struct Obj {
    pub name: String,
    pub class: Class,
    pub x: i16,
    pub y: i16,
    scroll_target: i16,
    normal_x: i16,
    focus_x: i16,
    pub width: i16,
    pub height: i16,
    /// 1 scrollleft, 2 fadealpha, 3 fade (the selector's)
    effect: i32,
    tileset: i32,
    tile: i32,
    pub depth: u8,
    pub texture: i32,
    focus_texture: i32,
    cap_texture: i32,
    pub image: u32,
    focus_image: i32,
    pub selected: bool,
    pub button: [i16; 2],
    pub image_off: [i16; 2],
    pub alpha: f32,
    alpha_target: f32,
    normal_alpha: f32,
    focus_alpha: f32,
    alpha_step: f32,
    bg_color: [f32; 4],
    // VIGUIText (Text, Button, Combo)
    /// the string model (`VIGUIStringDataModel`): None until set
    pub text: Option<String>,
    pub text_color: [f32; 4],
    roll_color: [f32; 4],
    center: bool,
    wrap: bool,
    pub password: bool,
    max_chars: i32,
    shadow: bool,
    label: Option<String>,
    // VIGUIButton
    pub next_page: Option<String>,
    pub action: Option<String>,
    // VIGUIToggleButton (its toggle model) and its texture
    pub value: bool,
    toggle_tex: i32,
    // VIGUIComboBox
    pub items: Vec<String>,
    pub sel: i32,
    top: i32,
    pub open: bool,
    popup_w: i32,
    popup_h: i32,
    rows: i32,
}

/// `VIGUIObject`'s tileset frames (`sTilesetFrame`): only Parchment.
/// A password field's editor, whatever its page's `MaxChars`: the 32 bytes the
/// login sends it in (`ValidateLogin`'s `des_soe_encrypt(password, 32)`, the
/// UDP token's AES block pair, `CreateAccount` and `ChangePassword` alike), the
/// editor counting the NUL as `VIWndEdit` does: 31 characters. The pages every
/// server ships say 16 (15 characters), where an account made on a server's
/// web page may have a longer one; more than 31 would be cut in the request.
pub const PASSWORD_LEN: usize = 32;

const TILESET_FRAME: FrameParam = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
const TILESET_FILL: i32 = 35;

impl Obj {
    /// `VIGUIObject::VIGUIObject` + `Init`, and the subclass's constructor.
    pub fn new(name: Option<&str>, class: Class) -> Obj {
        let text = matches!(class, Class::Text | Class::Button | Class::Combo);
        Obj {
            name: name.map(|n| n.chars().take(31).collect()).unwrap_or_default(),
            class,
            x: 0,
            y: 0,
            scroll_target: 0,
            normal_x: 0,
            focus_x: 0,
            width: -1,
            height: -1,
            effect: 0,
            tileset: -1,
            tile: 0,
            depth: 0,
            texture: 0,
            focus_texture: 0,
            cap_texture: 0,
            image: 0,
            focus_image: 0,
            selected: false,
            button: [-1, -1],
            image_off: [0, 0],
            alpha: 1.0,
            alpha_target: 1.0,
            normal_alpha: 1.0,
            focus_alpha: 1.0,
            alpha_step: 0.01,
            bg_color: [0.0; 4],
            text: None,
            // VIGUIText's: black, the roll colour (0, 0, 200) out of range as shipped
            text_color: if text { [0.0, 0.0, 0.0, 1.0] } else { [0.0; 4] },
            roll_color: if text { [0.0, 0.0, 200.0, 1.0] } else { [0.0; 4] },
            center: false,
            wrap: false,
            password: false,
            max_chars: -1,
            shadow: text,
            label: None,
            next_page: None,
            action: None,
            value: false,
            toggle_tex: 0,
            items: Vec::new(),
            sel: 0,
            top: 0,
            open: false,
            popup_w: -1,
            popup_h: 75,
            rows: 1,
        }
    }

    /// `SetPos`: the scroll effect comes back to where the object was put.
    pub fn set_pos(&mut self, x: i16, y: i16) {
        self.x = x;
        self.y = y;
        self.normal_x = x;
    }

    /// `SetAlpha`: the alpha now and the one it fades back to.
    pub fn set_alpha(&mut self, a: f32) {
        self.alpha = a;
        self.normal_alpha = a;
    }

    /// `UpdateAlpha`: the fade effect, a step toward the focused or the
    /// normal alpha (2), or toward the target the page pulses (3).
    fn update_alpha(&mut self) {
        if self.effect == 2 {
            self.alpha_target = if self.selected { self.focus_alpha } else { self.normal_alpha };
        }
        if matches!(self.effect, 2 | 3) && self.alpha != self.alpha_target {
            if self.alpha < self.alpha_target {
                self.alpha += self.alpha_step;
            } else {
                self.alpha -= self.alpha_step;
            }
        }
    }

    /// `UpdateScroll`: a pixel a frame toward the focused or the normal x.
    fn update_scroll(&mut self) {
        if self.effect == 1 {
            self.scroll_target = if self.selected { self.focus_x } else { self.normal_x };
            if self.x != self.scroll_target {
                if self.x < self.scroll_target {
                    self.x += 1;
                } else {
                    self.x -= 1;
                }
            }
        }
    }

    /// `VIGUIObject::Draw`: the texture (tiled as the tile mode says), else
    /// the tileset frame, else the background colour, else the image.
    fn draw_base(&mut self, vui: &Vui, r: &mut dyn Layout) {
        let mut tex = self.texture;
        if self.selected && self.focus_texture != 0 {
            tex = self.focus_texture;
        }
        self.update_alpha();
        self.update_scroll();
        let (x, y) = ((self.x + self.image_off[0]) as i32, (self.y + self.image_off[1]) as i32);
        let alpha = self.alpha;
        let (w, h) = (self.width as i32, self.height as i32);
        if tex != 0 {
            match self.tile {
                0 => vui.blit_ex(r, tex, [x, y], 3, alpha, 0),
                1 if self.cap_texture == 0 => vui.blit_horz(r, [x, y], tex, w, 3, alpha, 0),
                1 => vui.blit_bar(r, &[], [0.0; 4], [x, y], self.cap_texture, tex, w, 3, alpha, 0, true),
                2 => vui.blit_vert(r, [x, y], tex, h, 3, alpha, 0),
                3 => vui.blit_rect(r, tex, [x, y], [w, h], 3, alpha),
                _ => {}
            }
        } else if self.tileset != -1 {
            if w != -1 && h != -1 {
                vui.blit_fill_frame(r, [x, y], [w, h], &TILESET_FRAME, TILESET_FILL, 3, 1.0, true);
            }
        } else if self.bg_color[3] > 0.001 {
            // `FillRect`: its own solid material, blending (`SetLayerBlendMode(0, 3)`)
            r.fill_blended(self.bg_color, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]);
        } else {
            let mut image = self.image as i32;
            if self.selected && self.focus_image != -1 {
                image = self.focus_image;
            }
            vui.blit_id(r, image as u32, [x, y], 3, 1.0, 0);
            // a backdrop half in a window wider than 4:3: carried on past the frame
            crate::pregame::wide::half(r, image as u32, [x, y], 3);
        }
    }

    /// `VIGUIText::Fit`: `s` cut from the end until it is no wider than
    /// `width` (a negative width keeps it whole). The first pass stores the
    /// terminator over itself, so it cuts one character fewer than it tests.
    fn fit(&self, vui: &Vui, r: &dyn Layout, s: &str, width: i32) -> String {
        if width < 0 {
            return s.to_string();
        }
        let mut buf: Vec<char> = s.chars().collect();
        let len = buf.len();
        let mut i = 0;
        while width < vui.string_width(r, &to16(&buf)) && i < len {
            buf.truncate(len - i);
            i += 1;
        }
        buf.into_iter().collect()
    }

    /// `VIGUIText::Draw`: the text in the text colour or, selected, the roll
    /// colour; centred if asked; starred if a password; with a two-pixel
    /// shadow; or word-wrapped through a `VIUIWindow`.
    fn draw_text(&mut self, vui: &Vui, r: &mut dyn Layout) {
        let Some(text) = self.text.clone() else { return };
        self.draw_base(vui, r);
        let (mut x, mut y) = (self.x as i32, self.y as i32);
        let (w, h) = (self.width as i32, self.height as i32);
        if self.center {
            let sw = vui.string_width(r, &to16(&text.chars().collect::<Vec<_>>()));
            let fh = vui.font_height(r);
            x = self.x as i32 + w / 2 - sw / 2;
            y = self.y as i32 + h / 2 - fh / 2;
        }
        let mut c = if self.selected { self.roll_color } else { self.text_color };
        c[3] = 1.0;
        if !self.wrap {
            let mut s: Vec<char> = self.fit(vui, r, &text, w - 10).chars().collect();
            if self.password {
                s.iter_mut().for_each(|ch| *ch = '*');
            }
            let wbuf = to16(&s);
            if x < 0 {
                x = -x - vui.string_width(r, &wbuf);
            }
            if self.shadow {
                let sh = [0.0, 0.0, 0.0, 0.5];
                r.draw_string([x + 1, y + 1], sh, &wbuf);
                r.draw_string([x + 2, y + 2], sh, &wbuf);
            }
            r.draw_string([x, y], c, &wbuf);
        } else {
            let mut win = Window::default();
            let fh = vui.font_height(r).max(1);
            let rows = h / fh;
            win.init(vui, r, [self.x as i32 + w / 2, self.y as i32 + h / 2], w, rows, 0);
            win.text_box(vui, r, &wide(&text), 0, 0, w, rows, c, 0);
        }
    }

    /// `VIGUIToggleButton::Draw`: the object at full alpha, the toggle
    /// texture over it while on.
    fn draw_toggle(&mut self, vui: &Vui, r: &mut dyn Layout) {
        self.draw_base(vui, r);
        let (x, y) = ((self.x + self.image_off[0]) as i32, (self.y + self.image_off[1]) as i32);
        if self.value && self.toggle_tex != 0 {
            vui.blit(r, self.toggle_tex, [x, y]);
        }
    }

    /// `VIGUIComboBox::Draw`: the box as a text; open, the popup under it.
    fn draw_combo(&mut self, vui: &Vui, r: &mut dyn Layout) {
        let fh = vui.font_height(r).max(1);
        self.rows = self.popup_h / fh;
        self.draw_text(vui, r);
        if !self.open {
            return;
        }
        let (x, y) = ((self.x + self.image_off[0]) as i32, (self.y + self.image_off[1]) as i32);
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
        let w = if self.popup_w != -1 { self.popup_w } else { self.width as i32 };
        let h = self.height as i32;
        vui.fill_rectangle(r, [[x, y + h], [x + w, y + self.popup_h + h]], [0.5, 0.5, 0.5, 0.5]);
        vui.blit_frame(r, [x, y + h], [w, self.popup_h], &fp, 3, self.alpha, true);
        let first = self.top.max(0) as usize;
        let ty = self.y as i32 + h + 5;
        for (row, it) in (first..self.items.len()).take(self.rows.max(0) as usize).enumerate() {
            let c = if row as i32 + first as i32 == self.sel { self.roll_color } else { self.text_color };
            let s = &self.items[it];
            let line = if s.len() >= 1024 { s.clone() } else { self.fit(vui, r, s, w - 16) };
            r.draw_string([self.x as i32, ty + row as i32 * fh], c, &wide(&line));
        }
        if self.rows < self.items.len() as i32 {
            let top = y + h + 3;
            let bottom = y + self.popup_h + h - 3;
            vui.fill_rectangle(r, [[x + w - 10, top], [x + w - 5, bottom]], [0.7, 0.7, 0.7, 1.0]);
            let mut thumb = (self.popup_h as f32 / (self.items.len() as i32 - self.rows + 1) as f32) as i32;
            if thumb <= 1 {
                thumb = 2;
            }
            let range = bottom - thumb;
            let t = self.top as f32 / (self.items.len() as i32 - self.rows) as f32;
            let pos = (range as f64 * t as f64 + top as f64 * (1.0 - t as f64)) as i32;
            vui.fill_rectangle(r, [[x + w - 10, pos], [x + w - 5, pos + thumb]], [0.0, 0.0, 0.0, 1.0]);
        }
    }

    /// The draw this object's class makes (the virtual `Draw`).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout) {
        match self.class {
            Class::Object => self.draw_base(vui, r),
            Class::Text | Class::Button => self.draw_text(vui, r),
            Class::Toggle => self.draw_toggle(vui, r),
            Class::Combo => self.draw_combo(vui, r),
        }
    }

    /// `MoveSelection(d)`: kept inside the visible rows.
    fn move_selection(&mut self, d: i32) {
        let (top, bottom) = (self.top, self.top + self.rows - 1);
        let s = self.sel + d;
        if s >= 0 && s < self.items.len() as i32 {
            self.sel += d;
            if self.sel < top {
                self.top -= top - self.sel;
            } else if bottom < self.sel {
                self.top += self.sel - bottom;
            }
        }
    }

    /// `SyncSelection`: the option the model holds.
    fn sync_selection(&mut self) {
        let Some(cur) = self.text.clone() else { return };
        if let Some(i) = self.items.iter().position(|o| *o == cur) {
            self.sel = i as i32;
            self.move_selection(0);
        }
    }

    /// The virtual `OnKeyDown`: whether the object took the key (the page
    /// then does nothing more with it), and what it asks for.
    pub fn key_down(&mut self, key: i32, out: &mut Vec<Out>) -> bool {
        match self.class {
            Class::Object => true,
            Class::Text => {
                // Enter or the pad's accept: the renderer's editor on the text
                if (key == 13 || key == 134)
                    && let Some(s) = &self.text
                {
                    let max_len = if self.password {
                        PASSWORD_LEN
                    } else if self.max_chars > 0 {
                        self.max_chars as usize
                    } else {
                        128
                    };
                    out.push(Out::Edit { text: s.clone(), password: self.password, max_len, title: self.label.clone().unwrap_or_default() });
                }
                false
            }
            Class::Button => {
                if key == 134 {
                    if let Some(p) = &self.next_page {
                        out.push(Out::Sound(13));
                        out.push(Out::LoadPage(p.clone()));
                    }
                    if let Some(a) = &self.action {
                        out.push(Out::Sound(13));
                        out.push(Out::Action(a.clone()));
                    }
                }
                false
            }
            Class::Toggle => {
                if key == 13 || key == 134 {
                    self.value = !self.value;
                }
                false
            }
            Class::Combo => {
                if self.open {
                    match key {
                        138 => self.move_selection(-1),
                        139 => self.move_selection(1),
                        134 => {
                            if let Some(s) = self.items.get(self.sel.max(0) as usize) {
                                self.text = Some(s.clone());
                            }
                            self.open = false;
                        }
                        132 => self.open = false,
                        _ => {}
                    }
                    return true;
                }
                if key == 134 {
                    self.sync_selection();
                    self.open = !self.open;
                }
                self.open
            }
        }
    }

    /// The virtual `OnMessage`: the editor's answer to a text (259 with its
    /// text, 260 cancelled). Whether it took it.
    pub fn message(&mut self, msg: i32, text: Option<&str>) -> bool {
        if matches!(self.class, Class::Text | Class::Button | Class::Combo) {
            if msg == 259
                && let Some(t) = text
            {
                // str16to8 into 1,024
                self.text = Some(t.chars().take(1023).collect());
            }
            return true;
        }
        false
    }
}

/// A string as `str8to16` widens it.
pub fn to16(s: &[char]) -> Vec<u16> {
    s.iter().map(|&c| if (c as u32) < 0x10000 { c as u16 } else { b'?' as u16 }).collect()
}

/// `VIGUISelectionGrid`: the cells the pad moves over, objects by index.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    pub cols: i32,
    pub rows: i32,
    pub col: i32,
    pub row: i32,
    cells: Vec<Option<usize>>,
}

impl Grid {
    fn init(&mut self, cols: i32, rows: i32) {
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![None; (cols.max(0) * rows.max(0)) as usize];
    }

    fn cell(&self, c: i32, r: i32) -> Option<usize> {
        if c < 0 || r < 0 || c >= self.cols || r >= self.rows {
            return None;
        }
        self.cells[(c * self.rows + r) as usize]
    }

    /// `GetSelected`.
    pub fn selected(&self) -> Option<usize> {
        if self.cells.is_empty() {
            return None;
        }
        self.cell(self.col, self.row)
    }
}

/// `VIGUIPage`: the objects in drawing order (the deepest first), the grid
/// over them, the selector drawn on the selected one.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub name: String,
    pub objs: Vec<Obj>,
    /// the objects' indices in drawing order
    order: Vec<usize>,
    pub grid: Grid,
    pub selector: Option<Obj>,
    selector_x: i16,
    selector_y: i16,
    pulse: f32,
    pulse_step: f32,
}

impl Page {
    fn new() -> Page {
        Page { selector_x: -1, selector_y: -1, pulse: 1.0, pulse_step: 0.03, ..Default::default() }
    }

    /// `AddObject(o)`: before the first object deeper than it; and with a
    /// cell, into the grid (selected there if the grid's selection is that
    /// cell).
    fn add(&mut self, o: Obj, cell: Option<(i32, i32)>) -> usize {
        let i = self.objs.len();
        let depth = o.depth;
        let at = self.order.iter().position(|&k| depth < self.objs[k].depth).unwrap_or(self.order.len());
        self.objs.push(o);
        self.order.insert(at, i);
        if let Some((c, r)) = cell
            && c >= 0
            && r >= 0
            && c < self.grid.cols
            && r < self.grid.rows
            && !self.grid.cells.is_empty()
        {
            self.grid.cells[(c * self.grid.rows + r) as usize] = Some(i);
            if c == self.grid.col && r == self.grid.row {
                self.objs[i].selected = true;
            }
        }
        i
    }

    /// `SetSelector(o)`: it fades toward the pulse.
    fn set_selector(&mut self, mut o: Obj) {
        o.effect = 3;
        o.alpha_step = 0.05;
        self.selector = Some(o);
    }

    /// `FindObject(name)`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.order.iter().copied().find(|&i| self.objs[i].name == name)
    }

    /// `GetText(name)`: a text's or a combo box's string.
    pub fn text(&self, name: &str) -> Option<&str> {
        let o = &self.objs[self.find(name)?];
        matches!(o.class, Class::Text | Class::Combo).then_some(()).and(o.text.as_deref())
    }

    /// `SetText(name, s)`.
    pub fn set_text(&mut self, name: &str, s: &str) {
        if let Some(i) = self.find(name)
            && matches!(self.objs[i].class, Class::Text | Class::Combo)
        {
            self.objs[i].text = Some(s.to_string());
        }
    }

    /// `SetTextColor(name, c)`.
    pub fn set_text_color(&mut self, name: &str, c: [f32; 4]) {
        if let Some(i) = self.find(name)
            && matches!(self.objs[i].class, Class::Text | Class::Combo)
        {
            self.objs[i].text_color = c;
        }
    }

    /// `GetToggle(name)`.
    pub fn toggle(&self, name: &str) -> bool {
        self.find(name).is_some_and(|i| self.objs[i].class == Class::Toggle && self.objs[i].value)
    }

    /// `SetToggle(name, on)`.
    pub fn set_toggle(&mut self, name: &str, on: bool) {
        if let Some(i) = self.find(name)
            && self.objs[i].class == Class::Toggle
        {
            self.objs[i].value = on;
        }
    }

    /// `ClearOptions(name)`, a combo box's.
    pub fn clear_options(&mut self, name: &str) {
        if let Some(i) = self.find(name)
            && self.objs[i].class == Class::Combo
        {
            self.objs[i].items.clear();
        }
    }

    /// `AddOption(name, s)`, a combo box's.
    pub fn add_option(&mut self, name: &str, s: &str) {
        if let Some(i) = self.find(name)
            && self.objs[i].class == Class::Combo
        {
            self.objs[i].items.push(s.to_string());
        }
    }

    /// `Select(name)`: the grid's selection on the named object.
    pub fn select(&mut self, name: &str) {
        for c in 0..self.grid.cols {
            for r in 0..self.grid.rows {
                if let Some(o) = self.grid.cell(c, r)
                    && self.objs[o].name == name
                {
                    let cur = self.grid.selected();
                    self.grid.col = c;
                    self.grid.row = r;
                    if let Some(k) = cur
                        && k != o
                    {
                        self.objs[k].selected = false;
                    }
                    self.objs[o].selected = true;
                    return;
                }
            }
        }
    }

    /// `VIGUISelectionGrid::Move(key)`: 0 it moved, 1 no cell that way, 2
    /// no grid. Down searches the columns to the left of an empty cell (the
    /// last hit, the leftmost, wins); left steps again over a cell holding the
    /// same object.
    pub fn move_sel(&mut self, key: i32) -> i32 {
        let g = &mut self.grid;
        if g.cells.is_empty() {
            return 2;
        }
        let old = g.selected();
        let mut result = 2;
        match key {
            139 => {
                result = 1;
                if g.row + 1 < g.rows {
                    if g.cell(g.col, g.row + 1).is_some() {
                        g.row += 1;
                    } else {
                        let row = g.row + 1;
                        for c in (0..g.col).rev() {
                            if g.cell(c, row).is_some() {
                                g.row = row;
                                g.col = c;
                            }
                        }
                    }
                }
            }
            138 => {
                result = 1;
                if g.row > 0 && g.cell(g.col, g.row - 1).is_some() {
                    g.row -= 1;
                }
            }
            136 => {
                result = 1;
                loop {
                    if g.col > 0 && g.cell(g.col - 1, g.row).is_some() {
                        g.col -= 1;
                        if g.cell(g.col, g.row) == g.cell(g.col + 1, g.row) {
                            continue;
                        }
                    }
                    break;
                }
            }
            137 => {
                result = 1;
                if g.col + 1 < g.cols && g.cell(g.col + 1, g.row).is_some() {
                    g.col += 1;
                }
            }
            _ => {}
        }
        let cur = g.selected();
        if old != cur {
            result = 0;
            if let Some(o) = old {
                self.objs[o].selected = false;
            }
            if let Some(c) = cur {
                self.objs[c].selected = true;
            }
        }
        result
    }

    /// `VIGUIPage::Draw`: every object in depth order, the selector over the
    /// selection, then the pulse stepped (0.3 .. 1, 0.03 a frame). Where each
    /// grid object and each button glyph was drawn goes into `hits`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, hits: &mut Hits) {
        let sel = self.grid.selected();
        for k in 0..self.order.len() {
            let i = self.order[k];
            self.objs[i].draw(vui, r);
            let o = &self.objs[i];
            let x0 = o.x as i32 + o.image_off[0] as i32;
            let rect = |w: i32, h: i32| [x0, o.y as i32, x0 + w, o.y as i32 + h];
            if o.button[0] >= 0 && o.button[1] >= 0 {
                let (w, h) = if o.width > 0 { (o.width as i32, o.height as i32) } else { (vui.string_width(r, &wide(o.text.as_deref().unwrap_or(""))), vui.font_height(r)) };
                hits.push(rect(w, h), HIT_OBJ + i as i32);
            } else if let Some(k) = glyph_key(o.texture) {
                hits.push(rect(24, 24), k);
            }
        }
        if let (Some(s), Some(i)) = (self.selector.as_mut(), sel) {
            let (x, y) = (self.objs[i].x, self.objs[i].y);
            s.set_pos(if self.selector_x == -1 { x } else { self.selector_x }, if self.selector_y == -1 { y } else { self.selector_y });
            s.draw(vui, r);
        }
        self.pulse += self.pulse_step;
        if let Some(s) = self.selector.as_mut() {
            s.alpha_target = self.pulse;
        }
        if self.pulse > 1.0 {
            self.pulse = 1.0;
            self.pulse_step = -self.pulse_step;
        } else if self.pulse < 0.3 {
            self.pulse = 0.3;
            self.pulse_step = -self.pulse_step;
        }
    }

    /// `VIGUIPage::OnKeyDown`: the selection first; what it does not take,
    /// Triangle (back, the renderer's `PageBack`) and the arrows (the
    /// selection moved).
    pub fn key_down(&mut self, key: i32, out: &mut Vec<Out>) {
        let handled = match self.grid.selected() {
            Some(i) => self.objs[i].key_down(key, out),
            None => false,
        };
        if handled {
            return;
        }
        if key == 132 {
            out.push(Out::Sound(8));
            out.push(Out::Back);
            return;
        }
        match self.move_sel(key) {
            0 => out.push(Out::Sound(11)),
            1 => out.push(Out::Sound(9)),
            _ => {}
        }
    }

    /// `VIGUIPage::OnMessage`: to the selected object.
    pub fn message(&mut self, msg: i32, text: Option<&str>) {
        if let Some(i) = self.grid.selected() {
            self.objs[i].message(msg, text);
        }
    }

    /// The object a pointer at frame point `p` is on (a grid object), or
    /// the button its glyph stands for.
    pub fn hit(hits: &Hits, p: [f32; 2]) -> Option<Hit> {
        let h = hits.at(p)?;
        Some(if h >= HIT_OBJ { Hit::Obj((h - HIT_OBJ) as usize) } else { Hit::Key(h) })
    }

    /// The pointer's selection: the grid's on object `i`, as `Select` puts it.
    pub fn select_obj(&mut self, i: usize) -> bool {
        let before = self.grid.selected();
        if before == Some(i) {
            return false;
        }
        // by cell, not by name: several texts have no name
        for c in 0..self.grid.cols {
            for r in 0..self.grid.rows {
                if self.grid.cell(c, r) == Some(i) {
                    self.grid.col = c;
                    self.grid.row = r;
                    if let Some(k) = before {
                        self.objs[k].selected = false;
                    }
                    self.objs[i].selected = true;
                    return true;
                }
            }
        }
        false
    }
}

/// What a pointer is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Obj(usize),
    Key(i32),
}

/// Hit codes: an object's index past this, else a key.
const HIT_OBJ: i32 = 1000;

/// The pad glyph a page draws for its legend (`StationTri`, `StationX`,
/// `StationSqr`, `StationCir`): the button it stands for.
fn glyph_key(texture: i32) -> Option<i32> {
    match texture {
        175 => Some(132),
        173 => Some(134),
        174 => Some(133),
        176 => Some(135),
        _ => None,
    }
}

/// `VIGUITextureId`: a UI texture's name in a page, else its number. The
/// first "DarkTile" and "BlueTile" compare with case; "LoginBarTop" and
/// "DarkTile" are in the list twice.
pub fn texture_id(name: &str) -> i32 {
    if name == "DarkTile" {
        return 91;
    }
    if name == "BlueTile" {
        return 32;
    }
    const IDS: [(&str, i32); 37] = [
        ("GrayTile", 51),
        ("MetalTopTile", 37),
        ("TitleTile", 34),
        ("MetalLeftTile", 42),
        ("Scrollwork", 4),
        ("BluePointerR", 89),
        ("BluePointerL", 88),
        ("BluePointerU", 90),
        ("BluePointerD", 87),
        ("LoginBarTop", 163),
        ("LoginBarVert", 161),
        ("LoginBarBott", 162),
        ("LoginBarTop", 163),
        ("TopBlueGem", 145),
        ("TopRedGem", 144),
        ("BottomBlueGem", 147),
        ("BottomRedGem", 146),
        ("LoginParchScroll", 143),
        ("LoginPlaqueLeft", 148),
        ("LoginPlaqueTile", 167),
        ("ParchIndentCap", 141),
        ("ParchIndentTile", 150),
        ("DarkTile", 91),
        ("TargetCap", 25),
        ("TargetTile", 26),
        ("StationX", 173),
        ("StationSqr", 174),
        ("StationTri", 175),
        ("StationCir", 176),
        ("TextCapL", 177),
        ("TextCapR", 178),
        ("TextTile", 179),
        ("CBox", 180),
        ("CBoxCheck", 181),
        ("ButtonCapL", 182),
        ("ButtonCapR", 183),
        ("ButtonTile", 184),
    ];
    IDS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map_or_else(|| xml::atoi(name), |&(_, id)| id)
}

/// `ExpandText`: "|<n>" in a page's text is WORDS.TXT entry n. The
/// character after the number is copied twice (once as the one read after
/// the digits, once by the loop), as shipped; the result is cut at 511.
pub fn expand_text(word: &dyn Fn(i32) -> Vec<u16>, s: &str) -> String {
    let src: Vec<char> = s.chars().take(511).collect();
    let mut out: Vec<char> = Vec::new();
    let mut p = 0;
    while p < src.len() {
        let mut c = src[p];
        p += 1;
        if c == '|' {
            let mut id: i32 = 0;
            while p < src.len() && src[p].is_ascii_digit() {
                id = id.wrapping_mul(10).wrapping_add(src[p] as i32 - '0' as i32);
                p += 1;
            }
            c = src.get(p).copied().unwrap_or('\0');
            if id != 0 {
                let w = word(id);
                let room = 512usize.saturating_sub(out.len() + 1);
                out.extend(w.iter().take_while(|&&u| u != 0).take(room).map(|&u| if u < 256 { u as u8 as char } else { '?' }));
            }
        }
        if c == '\0' {
            break;
        }
        out.push(c);
    }
    out.truncate(511);
    out.into_iter().collect()
}

/// The parser's handlers (`PageHandler`, `ElementHandler`, and each
/// element's), on `VIXMLParser`'s stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handler {
    Page,
    Element,
    Object,
    Text,
    Button,
    Option,
    Toggle,
}

/// `ComboBox`'s namedRange="states".
const STATES: [&str; 55] = [
    "AL - Alabama", "AK - Alaska", "AZ - Arizona", "AR - Arkansas", "AA - Armed Forces South America", "AE - Armed Forces Europe", "AP - Armed Forces Pacific", "CA - California", "CO - Colorado", "CT - Connecticut", "DE - Delaware",
    "DC - District of Columbia", "FL - Florida", "GA - Georgia", "HI - Hawaii", "ID - Idaho", "IL - Illinois", "IN - Indiana", "IA - Iowa", "KS - Kansas", "KY - Kentucky", "LA - Louisiana", "ME - Maine", "MD - Maryland",
    "MA - Massachusetts", "MI - Michigan", "MN - Minnesota", "MS - Mississippi", "MO - Missouri", "MT - Montana", "NE - Nebraska", "NV - Nevada", "NH - New Hampshire", "NJ - New Jersey", "NM - New Mexico", "NY - New York",
    "NC - North Carolina", "ND - North Dakota", "OH - Ohio", "OK - Oklahoma", "OR - Oregon", "OT - Other (Non USA)", "PA - Pennsylvania", "RI - Rhode Island", "SC - South Carolina", "SD - South Dakota", "TN - Tennessee",
    "TX - Texas ", "UT - Utah", "VT - Vermont", "VA - Virginia", "WA - Washington", "WV - West Virginia", "WI - Wisconsin", "WY - Wyoming",
];

struct Parser<'w> {
    page: Option<Page>,
    object: Option<Obj>,
    stack: Vec<Handler>,
    word: &'w dyn Fn(i32) -> Vec<u16>,
}

fn color(e: &Element, alpha: bool) -> [f32; 4] {
    let c = |k: &str| e.prop_int(k, 0) as f32 / 255.0;
    [c("r"), c("g"), c("b"), if alpha { c("a") } else { 1.0 }]
}

impl Parser<'_> {
    fn pop(&mut self) {
        self.stack.pop();
    }

    /// An object done: into the page, by its cell when it has one.
    fn add_object(&mut self, grid: bool) {
        let Some(o) = self.object.take() else { return };
        let Some(page) = self.page.as_mut() else { return };
        let cell = (grid && o.button[0] != -1 && o.button[1] != -1).then_some((o.button[0] as i32, o.button[1] as i32));
        page.add(o, cell);
    }

    fn event(&mut self, ev: Ev, e: &Element) {
        let Some(&h) = self.stack.last() else { return };
        match h {
            Handler::Page => self.page_handler(ev, e),
            Handler::Element => self.element_handler(ev, e),
            Handler::Object => self.object_handler(ev, e),
            Handler::Text => self.text_handler(ev, e),
            Handler::Button => self.button_handler(ev, e),
            Handler::Option => self.option_handler(ev, e),
            Handler::Toggle => self.toggle_handler(ev, e),
        }
    }

    /// The root: the page made, its grid sized.
    fn page_handler(&mut self, ev: Ev, e: &Element) {
        if ev == Ev::Start && e.name.eq_ignore_ascii_case("PS2GUIPage") {
            let mut p = Page::new();
            p.grid.init(e.prop_int("gridWidth", 0), e.prop_int("gridHeight", 0));
            self.page = Some(p);
            self.stack.push(Handler::Element);
        }
    }

    /// The page's elements: each makes its object and pushes the handler
    /// for its children.
    fn element_handler(&mut self, ev: Ev, e: &Element) {
        let n = &e.name;
        let expand = |s: &str| expand_text(self.word, s);
        if ev == Ev::Start {
            if n.eq_ignore_ascii_case("Text") || n.eq_ignore_ascii_case("ComboBox") || n.eq_ignore_ascii_case("Button") {
                let class = if n.eq_ignore_ascii_case("Text") {
                    Class::Text
                } else if n.eq_ignore_ascii_case("ComboBox") {
                    Class::Combo
                } else {
                    Class::Button
                };
                let mut o = Obj::new(e.prop("name"), class);
                if let Some(v) = e.prop("val") {
                    o.text = Some(expand(v));
                }
                if class == Class::Button {
                    o.next_page = e.prop("nextPage").map(str::to_string);
                    o.action = e.prop("action").map(str::to_string);
                }
                if e.prop("alignment").is_some_and(|a| a.eq_ignore_ascii_case("center")) {
                    o.center = true;
                }
                self.object = Some(o);
                self.stack.push(match class {
                    Class::Text => Handler::Text,
                    Class::Combo => Handler::Option,
                    _ => Handler::Button,
                });
            } else if n.eq_ignore_ascii_case("Toggle") {
                self.object = Some(Obj::new(e.prop("name"), Class::Toggle));
                self.stack.push(Handler::Toggle);
            } else if n.eq_ignore_ascii_case("Object") || n.eq_ignore_ascii_case("Selector") {
                self.object = Some(Obj::new(e.prop("name"), Class::Object));
                self.stack.push(Handler::Object);
            }
        } else if n.eq_ignore_ascii_case("Text") {
            self.pop();
        }
    }

    /// Any object's children; "Object" and "Selector" end here.
    fn object_handler(&mut self, ev: Ev, e: &Element) {
        let n = e.name.as_str();
        let is = |s: &str| n.eq_ignore_ascii_case(s);
        if ev == Ev::End {
            if is("Object") {
                self.pop();
                self.add_object(false);
            } else if is("Selector") {
                self.pop();
                if let (Some(o), Some(p)) = (self.object.take(), self.page.as_mut()) {
                    p.set_selector(o);
                }
            }
            return;
        }
        if is("SelectorPos") {
            if let Some(p) = self.page.as_mut() {
                p.selector_x = e.prop_int("x", -1) as i16;
                p.selector_y = e.prop_int("y", -1) as i16;
            }
            return;
        }
        let Some(o) = self.object.as_mut() else { return };
        if is("Position") {
            o.set_pos(e.prop_int("x", 0) as i16, e.prop_int("y", 0) as i16);
        } else if is("SelectEffect") {
            if let Some(t) = e.prop("type") {
                if t.eq_ignore_ascii_case("scrollleft") {
                    o.effect = 1;
                } else if t.eq_ignore_ascii_case("fadealpha") {
                    o.effect = 2;
                }
            }
            o.focus_x = e.prop_int("xtarget", 0) as i16;
            o.focus_alpha = e.prop_int("alphatarget", 255) as f32 / 255.0;
        } else if is("ButtonPos") {
            o.button = [e.prop_int("x", 0) as i16, e.prop_int("y", 0) as i16];
        } else if is("ImageOffset") {
            o.image_off = [e.prop_int("x", 0) as i16, e.prop_int("y", 0) as i16];
        } else if is("BGColor") {
            o.bg_color = color(e, true);
        } else if is("Image") {
            o.set_alpha(e.prop_int("alpha", 255) as f32 / 255.0);
            if let Some(image) = e.prop("imageName") {
                o.image = vi::hash::hash_resource_id(image);
            } else if let Some(t) = e.prop("imageType") {
                o.texture = texture_id(t);
            }
            if let Some(t) = e.prop("tile") {
                if t.eq_ignore_ascii_case("horiz") {
                    o.tile = 1;
                } else if t.eq_ignore_ascii_case("vert") {
                    o.tile = 2;
                } else if t.eq_ignore_ascii_case("both") {
                    o.tile = 3;
                }
            }
            if let Some(c) = e.prop("capTexture") {
                o.cap_texture = texture_id(c);
            }
        } else if is("Size") {
            let (w, h) = (e.prop_int("width", -1), e.prop_int("height", -1));
            if w != -1 && h != -1 {
                o.width = w as i16;
                o.height = h as i16;
            }
        } else if is("Tileset") {
            if e.prop("type").is_some_and(|t| t.eq_ignore_ascii_case("Parchment")) {
                o.tileset = 0;
            }
        } else if is("Depth") {
            o.depth = e.prop_int("index", 0) as u8;
        }
    }

    /// A Text's, a Button's or a ComboBox's own children; the rest goes on
    /// to the object's handler.
    fn text_handler(&mut self, ev: Ev, e: &Element) {
        let n = e.name.as_str();
        let is = |s: &str| n.eq_ignore_ascii_case(s);
        if self.object.as_ref().is_none_or(|o| !matches!(o.class, Class::Text | Class::Button | Class::Combo)) {
            return;
        }
        if ev == Ev::Start {
            let o = self.object.as_mut().unwrap();
            if is("TextColor") {
                o.text_color = color(e, false);
                return;
            }
            if is("RollColor") {
                o.roll_color = color(e, false);
                return;
            }
            if is("EditLabel") {
                if let Some(t) = e.prop("title") {
                    o.label = Some(t.to_string());
                }
            } else if is("MaxChars") {
                let n = e.prop_int("size", -1);
                if n > -1 {
                    o.max_chars = n;
                }
            } else if is("Shadow") {
                if let Some(a) = e.prop("active") {
                    o.shadow = !a.eq_ignore_ascii_case("false");
                }
            } else if is("Wrap") {
                if let Some(m) = e.prop("mode") {
                    o.wrap = m.eq_ignore_ascii_case("true");
                }
            } else if is("Password")
                && let Some(m) = e.prop("mode")
            {
                o.password = m.eq_ignore_ascii_case("true");
            }
        } else if is("Text") {
            self.pop();
            self.add_object(true);
        }
        self.object_handler(ev, e);
    }

    fn button_handler(&mut self, ev: Ev, e: &Element) {
        if self.object.as_ref().is_none_or(|o| o.class != Class::Button) {
            return;
        }
        if ev == Ev::Start {
            self.text_handler(ev, e);
        } else if e.name.eq_ignore_ascii_case("Button") {
            self.pop();
            self.add_object(true);
        }
    }

    /// A ComboBox's options: a name, a numeric range, or the states.
    fn option_handler(&mut self, ev: Ev, e: &Element) {
        if self.object.as_ref().is_none_or(|o| o.class != Class::Combo) {
            return;
        }
        let is = |s: &str| e.name.eq_ignore_ascii_case(s);
        if ev == Ev::Start {
            if is("Option") {
                let o = self.object.as_mut().unwrap();
                if let Some(n) = e.prop("name") {
                    o.items.push(n.to_string());
                }
                let (lo, hi) = (e.prop_int("minRange", 0), e.prop_int("maxRange", 0));
                if hi != lo {
                    for i in lo..=hi {
                        o.items.push(i.to_string());
                    }
                }
                if e.prop("namedRange").is_some_and(|n| n.eq_ignore_ascii_case("states")) {
                    o.items.extend(STATES.iter().map(|s| s.to_string()));
                }
            } else if is("PopupSize") {
                let o = self.object.as_mut().unwrap();
                o.popup_w = e.prop_int("width", -1);
                o.popup_h = e.prop_int("height", 75);
            }
        } else if is("ComboBox") {
            self.pop();
            self.add_object(true);
            return;
        }
        self.text_handler(ev, e);
    }

    fn toggle_handler(&mut self, ev: Ev, e: &Element) {
        if self.object.as_ref().is_none_or(|o| o.class != Class::Toggle) {
            return;
        }
        if ev == Ev::Start {
            if e.name.eq_ignore_ascii_case("ToggleImage") {
                if let Some(t) = e.prop("imageType") {
                    self.object.as_mut().unwrap().toggle_tex = texture_id(t);
                }
                return;
            }
        } else if e.name.eq_ignore_ascii_case("Toggle") {
            self.pop();
            self.add_object(true);
        }
        self.object_handler(ev, e);
    }
}

/// `VIGUIParser::LoadMemory`: the page a file builds, named `name`; None for
/// a file that is not XML or whose root is not a PS2GUIPage.
pub fn load(bytes: &[u8], name: &str, word: &dyn Fn(i32) -> Vec<u16>) -> Option<Page> {
    let root = xml::parse(bytes)?;
    let mut p = Parser { page: None, object: None, stack: vec![Handler::Page], word };
    xml::walk(&root, &mut |ev, e| p.event(ev, e));
    let mut page = p.page?;
    page.name = name.chars().take(63).collect();
    Some(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_words(_: i32) -> Vec<u16> {
        Vec::new()
    }

    const LOGIN: &str = r#"<PS2GUIPage gridWidth="1" gridHeight="3">
        <Object><Position x="0" y="0"/><Image imageName="P:\studio\eqps2\Game_Assets\UI\stbk00.tga"/></Object>
        <Text name="StationName" val="">
            <ButtonPos x="0" y="0"/><Position x="230" y="100"/><Size width="162" height="25"/>
            <Image tile="horiz" capTexture="TextCapL" imageType="TextTile"/><ImageOffset x="-10"/>
            <SelectEffect type="scrollleft" xtarget="216"/><EditLabel title="User Name"/>
        </Text>
        <Text name="StationPassword" val=""><ButtonPos x="0" y="1"/><Password mode="true"/><MaxChars size="16"/></Text>
        <Toggle name="SaveAccountToggle"><ButtonPos x="0" y="2"/><Image imageType="CBox"/><ToggleImage imageType="CBoxCheck"/></Toggle>
        <Text val="Back"><Position x="265" y="378"/></Text>
        <Selector><Image imageType="BluePointerR"/><SelectorPos x="40"/><ImageOffset y="5"/></Selector>
    </PS2GUIPage>"#;

    #[test]
    fn a_page_builds_its_objects_grid_and_selector() {
        let p = load(LOGIN.as_bytes(), "data/ui/login.xml", &no_words).unwrap();
        assert_eq!(p.objs.len(), 5);
        let n = &p.objs[p.find("StationName").unwrap()];
        assert_eq!((n.x, n.y, n.width, n.height, n.texture, n.cap_texture, n.tile, n.effect, n.focus_x), (230, 100, 162, 25, 179, 177, 1, 1, 216));
        assert_eq!((n.label.as_deref(), n.text.as_deref(), n.selected), (Some("User Name"), Some(""), true), "the grid's first cell is selected as it is added");
        let pw = &p.objs[p.find("StationPassword").unwrap()];
        assert!(pw.password && pw.max_chars == 16 && !pw.selected);
        assert_eq!(p.objs[p.find("SaveAccountToggle").unwrap()].toggle_tex, 181);
        assert_eq!(p.objs[0].image, vi::hash::hash_resource_id(r"P:\studio\eqps2\Game_Assets\UI\stbk00.tga"));
        let s = p.selector.as_ref().unwrap();
        assert_eq!((s.texture, s.effect, p.selector_x, p.selector_y), (89, 3, 40, -1));
        assert_eq!(p.grid.selected(), p.find("StationName"));
        // the control: a page whose root is not PS2GUIPage is none
        assert!(load(b"<Page><Text val=\"x\"/></Page>", "x", &no_words).is_none());
    }

    #[test]
    fn the_grid_moves_as_the_game_moves_it() {
        let mut p = load(LOGIN.as_bytes(), "login", &no_words).unwrap();
        let mut out = Vec::new();
        p.key_down(139, &mut out);
        assert_eq!(p.grid.row, 1);
        assert_eq!(out, vec![Out::Sound(11)]);
        assert!(p.objs[p.find("StationPassword").unwrap()].selected && !p.objs[p.find("StationName").unwrap()].selected);
        p.key_down(139, &mut out);
        out.clear();
        p.key_down(139, &mut out);
        assert_eq!((p.grid.row, out), (2, vec![Out::Sound(9)]), "past the last row: refused");
        // Triangle goes back; the toggle flips on accept and lets the page go on
        let mut out = Vec::new();
        p.key_down(132, &mut out);
        assert_eq!(out, vec![Out::Sound(8), Out::Back]);
        assert!(!p.toggle("SaveAccountToggle"));
        p.key_down(134, &mut out);
        assert!(p.toggle("SaveAccountToggle"));
        // accept on a text opens the editor with its label and limit
        p.select("StationPassword");
        let mut out = Vec::new();
        p.key_down(134, &mut out);
        assert_eq!(out, vec![Out::Edit { text: String::new(), password: true, max_len: PASSWORD_LEN, title: String::new() }]);
        p.message(259, Some("hunter2"));
        assert_eq!(p.text("StationPassword"), Some("hunter2"));
    }

    #[test]
    fn a_password_field_holds_what_the_login_carries() {
        // the pages' 16 (15 characters) and a page's 64 (more than the request holds) both edit 31
        for size in [16, 64] {
            let src = format!(
                r#"<PS2GUIPage gridWidth="1" gridHeight="2">
                <Text name="StationName" val=""><ButtonPos x="0" y="0"/><MaxChars size="{size}"/></Text>
                <Text name="StationPassword" val=""><ButtonPos x="0" y="1"/><Password mode="true"/><MaxChars size="{size}"/></Text>
            </PS2GUIPage>"#
            );
            let mut p = load(src.as_bytes(), "data/ui/login.xml", &no_words).unwrap();
            p.select("StationPassword");
            let mut out = Vec::new();
            p.key_down(134, &mut out);
            assert_eq!(out, vec![Out::Edit { text: String::new(), password: true, max_len: PASSWORD_LEN, title: String::new() }]);
            // the control: a field that is no password keeps its page's limit
            p.select("StationName");
            let mut out = Vec::new();
            p.key_down(134, &mut out);
            assert_eq!(out, vec![Out::Edit { text: String::new(), password: false, max_len: size as usize, title: String::new() }]);
        }
        // the editor at that limit takes 31 characters and refuses a 32nd (the NUL's place)
        let mut e = crate::editbox::EditBox::default();
        e.set_max_length(PASSWORD_LEN);
        let long: Vec<u16> = "abcdefghijklmnopqrstuvwxyz0123456789".encode_utf16().collect();
        e.set_text(&long);
        assert_eq!(e.text.len(), 31);
    }

    #[test]
    fn down_finds_the_leftmost_column_with_a_cell_on_the_next_row() {
        let src = r#"<PS2GUIPage gridWidth="3" gridHeight="2">
            <Text name="a" val=""><ButtonPos x="0" y="0"/></Text>
            <Text name="b" val=""><ButtonPos x="1" y="0"/></Text>
            <Text name="c" val=""><ButtonPos x="2" y="0"/></Text>
            <Text name="d" val=""><ButtonPos x="0" y="1"/></Text>
            <Text name="e" val=""><ButtonPos x="1" y="1"/></Text>
        </PS2GUIPage>"#;
        let mut p = load(src.as_bytes(), "x", &no_words).unwrap();
        p.select("c");
        assert_eq!(p.move_sel(139), 0);
        assert_eq!((p.grid.col, p.grid.row), (0, 1), "the LAST hit of the leftward search wins: column 0, not 1");
    }

    #[test]
    fn depth_orders_the_drawing() {
        let src = r#"<PS2GUIPage gridWidth="1" gridHeight="1">
            <ComboBox name="deep" val="x"><Depth index="6"/><Option name="x"/></ComboBox>
            <Text name="flat" val="y"/>
            <ComboBox name="mid" val="z"><Depth index="3"/><Option minRange="1" maxRange="3"/></ComboBox>
        </PS2GUIPage>"#;
        let p = load(src.as_bytes(), "x", &no_words).unwrap();
        let names: Vec<&str> = p.order.iter().map(|&i| p.objs[i].name.as_str()).collect();
        assert_eq!(names, ["flat", "mid", "deep"]);
        assert_eq!(p.objs[p.find("mid").unwrap()].items, ["1", "2", "3"]);
    }

    #[test]
    fn expand_text_doubles_the_character_after_a_number() {
        let w = |id: i32| if id == 790 { wide("Play") } else { Vec::new() };
        assert_eq!(expand_text(&w, "|790"), "Play");
        assert_eq!(expand_text(&w, "a |790 b"), "a Play  b", "the space after the number twice");
        assert_eq!(expand_text(&w, "x|y"), "xyy", "no number: the next character twice");
        assert_eq!(expand_text(&w, "plain"), "plain");
    }

    #[test]
    fn texture_names_and_numbers() {
        assert_eq!([texture_id("StationTri"), texture_id("textcapl"), texture_id("darktile"), texture_id("DarkTile"), texture_id("12")], [175, 177, 91, 91, 12]);
        assert_eq!(texture_id("bluetile"), 0, "BlueTile compares with case: a lower-case one is a number, 0");
    }

    /// The combo box: accept opens it on the model's option; up and down
    /// move inside its rows; accept takes the option; Triangle closes.
    #[test]
    fn a_combo_box_picks() {
        let src = r#"<PS2GUIPage gridWidth="1" gridHeight="1">
            <ComboBox name="m" val="Feb"><ButtonPos x="0" y="0"/><Option name="Jan"/><Option name="Feb"/><Option name="Mar"/></ComboBox>
        </PS2GUIPage>"#;
        let mut p = load(src.as_bytes(), "x", &no_words).unwrap();
        let i = p.find("m").unwrap();
        p.objs[i].rows = 2;
        let mut out = Vec::new();
        p.key_down(134, &mut out);
        assert!(p.objs[i].open);
        assert_eq!(p.objs[i].sel, 1);
        p.key_down(139, &mut out);
        p.key_down(134, &mut out);
        assert_eq!((p.objs[i].open, p.text("m")), (false, Some("Mar")));
        p.key_down(134, &mut out);
        p.key_down(132, &mut out);
        assert!(!p.objs[i].open && p.text("m") == Some("Mar"), "Triangle closes it, the model kept");
    }

    /// Every page of the disc's STATION.PAK parses, and each has a grid and
    /// its objects in it.
    #[test]
    fn every_page_on_the_disc_parses() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let b = iso::Iso::open(&iso).unwrap().read("STATION.PAK").unwrap();
        let pak = super::super::pak::Pak::load(&b);
        let pages: Vec<String> = pak.paths().into_iter().filter(|p| p.ends_with(".xml")).collect();
        assert_eq!(pages.len(), 23);
        for name in &pages {
            let p = load(pak.find(name).unwrap(), name, &no_words);
            // the disc's ChangeCC.xml leaves a <Text> open (its line 170 is
            // there twice), so libxml 2.3.6 gives no document and the game
            // no page (`LoadPage` then names a null page, as shipped)
            if name == "data/ui/ChangeCC.xml" {
                assert!(p.is_none(), "a page the console refuses");
                continue;
            }
            let p = p.unwrap_or_else(|| panic!("{name}"));
            assert!(p.grid.cols > 0 && p.grid.rows > 0, "{name}");
            assert!(p.grid.selected().is_some(), "{name}: something is selected");
            assert!(p.selector.is_some(), "{name}");
        }
        let login = load(pak.find("data/ui/login.xml").unwrap(), "data/ui/login.xml", &no_words).unwrap();
        assert_eq!(login.objs.iter().filter(|o| o.button[0] >= 0).count(), 7);
        let i = login.find("LogInButton").unwrap();
        assert_eq!(login.objs[i].action.as_deref(), Some("SubmitStationLogin"));
    }
}
