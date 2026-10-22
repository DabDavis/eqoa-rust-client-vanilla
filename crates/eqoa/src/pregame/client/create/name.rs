//! `VIWndCharCreateName` (CLIENT `viwndcharcreatename.cpp`): the name page.
//! The editor (`VIWndEdit`, the client's `editbox.rs`) at (90, 75), no
//! spaces, 13 characters with the terminator, titled "Character Name"; under
//! it a legend: circle Generate Name (0x0012 with the form's race and sex,
//! `Send0012`; the server's name comes back into the editor), cross Select,
//! triangle Back. The editor's Done validates the name (four letters or
//! more, A to Z only, else the message box at (80, 115) with word 875 or
//! 876) and hands it up capitalised (266); its cancel is the page's back.

use super::tables;
use super::{Msg, word};
use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::pointer::{Hits, key};
use crate::pregame::Ctx;
use net::session::{NewChar, Session};

const CIRCLE: i32 = 1;
const CROSS: i32 = 2;
const TRIANGLE: i32 = 3;

pub struct NamePage {
    pub edit: EditBox,
    pub mbox: MBox,
    /// the message box is up (and has the keys)
    pub mbox_up: bool,
    /// `m_name`: the name the editor gave, as checked
    pub name: String,
    hits: Hits,
}

impl Default for NamePage {
    fn default() -> Self {
        let mut edit = EditBox::default();
        edit.set_pos([90, 75]);
        edit.set_allow_spaces(false);
        NamePage { edit, mbox: MBox::default(), mbox_up: false, name: String::new(), hits: Hits::default() }
    }
}

/// `Validate`: four letters or more, A to Z only; the word refusing it.
pub fn validate(name: &str) -> Result<String, i32> {
    if name.len() < 4 {
        return Err(875);
    }
    if !name.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(876);
    }
    Ok(tables::capitalize_name(name))
}

impl NamePage {
    /// `Reset`.
    pub fn reset(&mut self) {
        self.name.clear();
        self.edit.clear();
        self.mbox_up = false;
    }

    /// `Activate`: the editor up on the form's name, if it has one.
    pub fn activate(&mut self, now: i64, vui: &vi::ui::Vui, form: &NewChar) {
        self.edit.set_max_length(13);
        self.edit.title = word(vui, 260);
        self.mbox_up = false;
        // SetActiveWindow(m_edit): its 295, a fresh session
        self.edit.focus(now);
        if form.name.is_empty() {
            self.edit.clear();
        } else {
            self.edit.set_text(&vi::ui::wide(&form.name));
        }
    }

    /// `SetName` (0x0012's answer): into the editor.
    pub fn set_name(&mut self, name: &str) {
        self.edit.set_text(&vi::ui::wide(name));
    }

    /// `OnKeyDown`: circle asks the server for a name; the rest go to the
    /// editor (or to the message box while it is up).
    #[allow(clippy::too_many_arguments)]
    pub fn key(&mut self, k: i32, now: i64, vui: &vi::ui::Vui, r: &dyn vi::ui::Raster, form: &NewChar, session: &mut Session, sounds: &mut Vec<i32>) -> Option<Msg> {
        if self.mbox_up {
            let bk = match k {
                13 | key::CROSS => Some(BoxKey::Enter),
                27 | key::TRIANGLE => Some(BoxKey::Escape),
                17 | key::LEFT => Some(BoxKey::Left),
                18 | key::RIGHT => Some(BoxKey::Right),
                _ => None,
            };
            // 268 and 260 alike: the editor back
            if let Some(BoxAnswer::Yes(..) | BoxAnswer::No(..)) = bk.and_then(|b| self.mbox.key(b, sounds)) {
                self.mbox_up = false;
            }
            return None;
        }
        if k == key::CIRCLE {
            session.random_name(form.race, form.sex);
            sounds.push(11);
            return None;
        }
        if k == key::L3 {
            return None;
        }
        match self.edit.key(vui, r, k, now, sounds) {
            EditUp::Done(_) => {
                let text: String = String::from_utf16_lossy(&self.edit.text).chars().take(23).collect();
                match validate(&text) {
                    Ok(n) => {
                        self.name = n.clone();
                        self.edit.clear();
                        Some(Msg::Name(n))
                    }
                    Err(w) => {
                        self.name = text;
                        self.mbox.prompt(&word(vui, w), 0, 0, 0, now);
                        self.mbox_up = true;
                        None
                    }
                }
            }
            EditUp::Cancel => {
                self.edit.clear();
                Some(Msg::Back)
            }
            EditUp::None => None,
        }
    }

    /// `OnKeyUp`.
    pub fn key_up(&mut self, k: i32) {
        self.edit.key_up(k);
    }

    pub fn typing(&self) -> bool {
        !self.mbox_up
    }

    /// `OnDrawSelf` and the children: the backdrop, the legend under the
    /// editor, then the editor (or the message box).
    pub fn draw(&mut self, cx: &mut Ctx, sounds: &mut Vec<i32>) {
        let vui = cx.vui;
        let r = &mut *cx.r;
        let black = [0.0, 0.0, 0.0, 1.0];
        self.hits.clear();
        vui.blit_rect(r, 32, [-1, -1], [642, 450], 0, 1.0);
        let (wg, ws, wb) = (r.string_width(&word(vui, 548)), r.string_width(&word(vui, 212)), r.string_width(&word(vui, 217)));
        let w = wg + 155 + ws + wb;
        let (p0, p1) = (self.edit.pos(), self.edit.end());
        let mut pos = [((p0[0] + p1[0]) >> 1) - (w >> 1), p1[1]];
        vui.blit_bar(r, &[], black, pos, 25, 26, w, 0, 1.0, 0, true);
        pos = [pos[0] + 20, pos[1] + 4];
        let gh = vui.entry(68).uv1[1].max(16);
        for (glyph, wd, width, what) in [(68, 548, wg, CIRCLE), (66, 212, ws, CROSS), (65, 217, wb, TRIANGLE)] {
            vui.blit(r, glyph, pos);
            self.hits.push([pos[0], pos[1], pos[0] + 25 + width, pos[1] + gh], what);
            pos[0] += 25;
            r.draw_centered_string([pos[0], pos[1] + 5], black, &word(vui, wd), 1);
            pos[0] += 20 + width;
        }
        if self.mbox_up {
            if let Some(BoxAnswer::Yes(..) | BoxAnswer::No(..)) = self.mbox.draw(vui, r, cx.now) {
                self.mbox_up = false;
            }
        } else {
            self.edit.draw(vui, r, cx.now, sounds);
        }
    }

    /// A pointer: the editor's keys and legend (`EditBox::point`), the
    /// message box's button, the page's legend.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.mbox_up {
            return if self.mbox.point(p) { vec![key::CROSS] } else { Vec::new() };
        }
        if let Some(h) = self.hits.at(p) {
            return vec![match h {
                CIRCLE => key::CIRCLE,
                CROSS => key::CROSS,
                _ => key::TRIANGLE,
            }];
        }
        match self.edit.point(p) {
            Some(Some(k)) => vec![k],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_four_letters_or_more_and_capitalised() {
        assert_eq!(validate("brakk"), Ok("Brakk".to_string()));
        assert_eq!(validate("BRAKK"), Ok("Brakk".to_string()));
        assert_eq!(validate("bra"), Err(875), "three letters");
        assert_eq!(validate("brakk2"), Err(876), "a digit");
        assert_eq!(validate("bra kk"), Err(876), "a space");
        // the control: four is enough
        assert_eq!(validate("abcd"), Ok("Abcd".to_string()));
    }

    #[test]
    fn the_editor_takes_no_spaces_and_thirteen_less_one() {
        let mut p = NamePage::default();
        let v = super::super::tests_vui();
        p.activate(0, &v, &super::super::blank_form());
        let r = crate::pointer::testing::NullRaster;
        let mut s = Vec::new();
        let mut sess = sess();
        for c in "abc defghijklmnop".bytes() {
            p.key(c as i32, 0, &v, &r, &super::super::blank_form(), &mut sess, &mut s);
        }
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "abcdefghijkl", "no space, twelve characters");
        // Enter: the name up, capitalised
        let m = p.key(13, 0, &v, &r, &super::super::blank_form(), &mut sess, &mut s);
        assert_eq!(m, Some(Msg::Name("Abcdefghijkl".into())));
        // a short one: the message box, which Enter takes down
        for c in "ab".bytes() {
            p.key(c as i32, 0, &v, &r, &super::super::blank_form(), &mut sess, &mut s);
        }
        assert_eq!(p.key(13, 0, &v, &r, &super::super::blank_form(), &mut sess, &mut s), None);
        assert!(p.mbox_up);
        assert!(!p.typing(), "the box has the keys");
        p.key(13, 0, &v, &r, &super::super::blank_form(), &mut sess, &mut s);
        assert!(!p.mbox_up && p.typing());
        // Escape in the editor is the page's back
        assert_eq!(p.key(27, 0, &v, &r, &super::super::blank_form(), &mut sess, &mut s), Some(Msg::Back));
    }

    #[test]
    fn coming_back_shows_the_forms_name_and_a_random_one_replaces_it() {
        let mut p = NamePage::default();
        let v = super::super::tests_vui();
        let form = NewChar { name: "Brakk".into(), ..super::super::blank_form() };
        p.activate(0, &v, &form);
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Brakk");
        p.set_name("Ghurt");
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Ghurt");
        // the control: a blank form, a blank editor
        p.activate(0, &v, &super::super::blank_form());
        assert!(p.edit.text.is_empty());
    }

    /// The legend's glyphs are their buttons; the box's button is Enter.
    #[test]
    fn a_pointer_presses_the_legend() {
        let mut p = NamePage::default();
        let v = super::super::tests_vui();
        p.activate(0, &v, &super::super::blank_form());
        let mut r = crate::pointer::testing::NullRaster;
        let res = crate::pregame::res::Res::new();
        let mut shared = crate::pregame::Shared::default();
        let (mut cues, mut music) = (Vec::new(), Vec::new());
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        let mut s = Vec::new();
        p.draw(&mut cx, &mut s);
        let end = p.edit.end();
        // the legend starts under the editor; its first glyph is circle
        let found: Vec<i32> = (0..640).step_by(4).flat_map(|x| p.point([x as f32, (end[1] + 12) as f32])).collect();
        assert!(found.contains(&key::CIRCLE) && found.contains(&key::CROSS) && found.contains(&key::TRIANGLE), "{found:?}");
        // the control: nothing there without the hit notes
        p.hits.clear();
        let none: Vec<i32> = (0..640).step_by(4).flat_map(|x| p.point([x as f32, (end[1] + 12) as f32])).collect();
        assert!(!none.contains(&key::CIRCLE));
    }

    fn sess() -> Session {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap()
    }
}
