//! `VIGUIDialog` (viguidialog.cpp): the message a page opens over itself, a
//! `VIUIWindow` of up to 1,024 characters scrolled seven rows at a time, a
//! prompt row ("X Continue", or "Triangle Cancel" while a confirmation is
//! up), and the action an accept answers with.

use crate::pointer::Hits;
use crate::ui::Layout;
use vi::ui::{Vui, Window, wide};

/// What a key on the dialog asks of the renderer.
#[derive(Clone, Debug, PartialEq)]
pub enum DOut {
    Sound(i32),
    Action(String),
}

#[derive(Clone, Debug, Default)]
pub struct Dialog {
    pub window: Window,
    text: Vec<u16>,
    pub action: String,
    /// X closes it, O sends the action, only a disconnect's
    pub confirm: bool,
    /// the text's rows (`CountLines(text, 320)`): counted where a raster is
    /// at hand, at the next draw
    rows: Option<i32>,
    scroll: i32,
    pub prompt: bool,
    inited: bool,
    /// the prompt row, for a pointer
    hits: Hits,
}

impl Dialog {
    /// `Init`: the window at the screen's middle, 400 wide, 11 rows.
    pub fn init(&mut self, vui: &Vui, r: &dyn Layout) {
        self.text.clear();
        self.prompt = true;
        self.confirm = false;
        self.rows = Some(0);
        self.scroll = 0;
        self.window.init(vui, r, [320, 224], 400, 11, 0);
        self.inited = true;
    }

    /// `SetText`: the text (1,024 at most), scrolled to the top.
    pub fn set_text(&mut self, s: &[u16]) {
        self.text = s.iter().copied().take_while(|&c| c != 0).take(1023).collect();
        self.rows = None;
        self.scroll = 0;
        self.prompt = true;
    }

    /// `SetText(const char *)`: widened a byte a character.
    pub fn set_text8(&mut self, s: &str) {
        self.set_text(&s.chars().map(|c| if (c as u32) < 0x10000 { c as u16 } else { b'?' as u16 }).collect::<Vec<_>>());
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    /// `IsOpen`: opening or open.
    pub fn is_open(&self) -> bool {
        self.window.is_opening() || self.window.is_open()
    }

    /// `Open(title, action)`: an empty title keeps the one it had; no action
    /// clears it.
    pub fn open(&mut self, title: &[u16], action: Option<&str>) {
        // the wide overload goes through str16to8 first
        if title.first().is_some_and(|&c| c != 0) {
            self.window.set_title(Some(title));
        }
        self.action = action.map(str::to_string).unwrap_or_default();
        self.window.open(false);
        self.prompt = true;
    }

    /// `Open(const char *title, action)`.
    pub fn open8(&mut self, title: &str, action: Option<&str>) {
        self.open(&wide(title), action);
    }

    /// `Close`: at once.
    pub fn close(&mut self) {
        self.window.close(true);
    }

    /// `Draw`: the frame and its scroll bar, the text from the scroll row,
    /// the prompt row; nothing while closed or closing.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout) {
        self.hits.clear();
        if !self.inited || !(self.window.is_opening() || self.window.is_open()) {
            return;
        }
        if self.rows.is_none() {
            let t = self.text.clone();
            self.rows = Some(self.window.count_lines(vui, r, &t, 320));
        }
        // `Resize(400, rows)`: the height from the rows and the title bar
        let h = vui.font_height(r);
        self.window.size = [400, if self.prompt { 11 } else { 9 } * (h - 4) + self.window.title_h * 2];
        self.window.draw(vui, r);
        let rows = self.rows.unwrap_or(0);
        let t = if rows < 8 { -1.0 } else { self.scroll as f32 / (rows - 7) as f32 };
        self.window.draw_frame_vscroll(vui, r, 20, 0, 360, 9, t);
        if !self.text.is_empty() {
            let text = self.text.clone();
            self.window.text_box(vui, r, &text, 20, 1, 370, 7, [0.0, 0.0, 0.0, 1.0], (self.scroll as u32) << 24);
        }
        if self.prompt {
            let (icon, word, key) = if self.confirm { ("|t", 753, 132) } else { ("|x", 1237, 134) };
            let s = wide(&format!("{icon} {}", vui.words.get(word, vui.language)));
            self.window.text(vui, r, &s, 153, 10, [0.0, 0.0, 0.0, 1.0], 0);
            let w = self.window.text_width(vui, r, &s);
            self.hits.push(crate::pointer::rows_rect(&self.window, vui, r, 153, 10, w, 1), key);
        }
    }

    /// `OnKeyDown`: while a confirmation is up, X cancels it and O sends a
    /// disconnect; otherwise O sends the action (a disconnect's on ANY key)
    /// and O or X closes it, and up and down scroll the text.
    pub fn key_down(&mut self, key: i32) -> Vec<DOut> {
        let mut out = Vec::new();
        if !self.prompt {
            return out;
        }
        if self.confirm {
            if key == 132 {
                self.confirm = false;
                out.push(DOut::Sound(8));
                self.window.close(false);
            } else if key == 134 && self.action == "DisconnectConfirmed" {
                out.push(DOut::Action(self.action.clone()));
            }
            return out;
        }
        if (key == 134 || self.action == "DisconnectConfirmed") && !self.action.is_empty() {
            out.push(DOut::Action(self.action.clone()));
        }
        if key == 134 || key == 132 {
            out.push(DOut::Sound(8));
            self.window.close(false);
        }
        let rows = self.rows.unwrap_or(0);
        if key == 138 {
            if self.scroll > 0 {
                self.scroll -= 1;
                out.push(DOut::Sound(11));
            } else {
                out.push(DOut::Sound(9));
            }
        }
        if key == 139 {
            if self.scroll < rows - 7 {
                self.scroll += 1;
                out.push(DOut::Sound(11));
            } else {
                out.push(DOut::Sound(9));
            }
        }
        out
    }

    /// A pointer at frame point `p`: the prompt row is its button.
    pub fn point(&self, p: [f32; 2]) -> Option<i32> {
        self.hits.at(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn open_dialog(action: Option<&str>, confirm: bool) -> Dialog {
        let (v, r) = (vui(), NullRaster);
        let mut d = Dialog::default();
        d.init(&v, &r);
        d.open8("Logging in", action);
        d.confirm = confirm;
        d
    }

    #[test]
    fn accept_sends_the_action_and_closes() {
        let mut d = open_dialog(Some("REALLYPlayEQOA"), false);
        assert!(d.is_open(), "opening counts as open");
        for _ in 0..7 {
            d.draw(&vui(), &mut NullRaster);
        }
        assert_eq!(d.key_down(134), vec![DOut::Action("REALLYPlayEQOA".into()), DOut::Sound(8)]);
        // closing: drawn one more frame, then gone
        assert!(d.is_open());
        d.draw(&vui(), &mut NullRaster);
        assert!(!d.is_open());
    }

    #[test]
    fn a_confirmation_takes_only_triangle_and_a_disconnects_accept() {
        let mut d = open_dialog(None, true);
        assert_eq!(d.key_down(134), vec![], "accept does nothing while confirming");
        assert_eq!(d.key_down(132), vec![DOut::Sound(8)]);
        assert!(!d.confirm);
        let mut d = open_dialog(Some("DisconnectConfirmed"), true);
        assert_eq!(d.key_down(134), vec![DOut::Action("DisconnectConfirmed".into())]);
        // unconfirmed, a disconnect's action goes on ANY key, as shipped
        let mut d = open_dialog(Some("DisconnectConfirmed"), false);
        assert_eq!(d.key_down(138)[0], DOut::Action("DisconnectConfirmed".into()));
        // the control: another action waits for accept
        let mut d = open_dialog(Some("REALLYPlayEQOA"), false);
        assert!(!d.key_down(138).contains(&DOut::Action("REALLYPlayEQOA".into())));
    }

    #[test]
    fn no_prompt_no_keys() {
        let mut d = open_dialog(Some("REALLYPlayEQOA"), false);
        d.prompt = false;
        assert!(d.key_down(134).is_empty());
        assert!(d.is_open());
    }

    #[test]
    fn the_title_stays_when_opened_without_one() {
        let mut d = open_dialog(None, false);
        d.open8("", Some("x"));
        assert_eq!(String::from_utf16_lossy(&d.window.title.chars()), "Logging in");
        assert_eq!(d.action, "x");
    }

    /// A pointer on the prompt row is its button; the hit is noted only as
    /// the row is drawn.
    #[test]
    fn a_pointer_on_the_prompt() {
        let (v, mut r) = (vui(), NullRaster);
        let mut d = open_dialog(None, false);
        assert_eq!(d.point([330.0, 330.0]), None, "nothing drawn, nothing to hit");
        d.draw(&v, &mut r);
        let rr = crate::pointer::rows_rect(&d.window, &v, &r, 153, 10, 40, 1);
        assert_eq!(d.point(crate::pointer::testing::mid(rr)), Some(134));
        d.confirm = true;
        d.draw(&v, &mut r);
        assert_eq!(d.point(crate::pointer::testing::mid(rr)), Some(132));
    }
}
