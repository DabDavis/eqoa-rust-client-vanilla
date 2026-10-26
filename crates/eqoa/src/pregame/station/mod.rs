//! STATION (`station_main`, `VIStation`, `VIWndGenericRenderer`): the Station
//! login, STATION.PAK's pages. `VIStation::Init` fades in from black over
//! half a second; each frame the window manager hands the keys to the window
//! with the focus (the renderer, or the text editor a page's field opens)
//! and draws the windows, the fader over them; once the renderer has a
//! result and the fade out is whole, the module returns it: 1 with the
//! login (on to CLIENT), 2 to bring the network down (back to the title).
//!
//! STATION draws at the display's 59.94 Hz (`VIRaster::Flip` waits one
//! vertical blank; nothing caps it lower), where SETUP and CLIENT run at 30:
//! the pregame calls a stage once a frame of the game's (30 a second), so
//! each call runs two of STATION's frames, the first drawn into nothing
//! (`Quiet`), so that what STATION counts in frames (a window opening, a
//! page's pulse, the news' waits, the save's three frames) keeps its time.
//!
//! Not here: the memory card (`McErr`'s checks at the first load and on a
//! failed save, the card's copy of station.pak: the disc's is read), the
//! network interface's state (`VINetStatus`: a computer's is up), and
//! customer service (`VICSInterface`, `renderer`'s header).

mod api;
mod dialog;
mod fader;
mod page;
mod pak;
mod renderer;
pub mod xml;

use super::{Ctx, Login, Next, Shared, res::Res};
use crate::sound::Cue;
use crate::ui::Layout;
use fader::Fader;
use renderer::{Focus, Renderer};
use render::ui::Anchor;
use vi::ui::{Raster, Vui};

/// What `VIStation::Init` reads after the base files.
pub const FILES: &[&str] = &["DATA2/STATUI.CSF", "DATA2/PREGAME.CSF", "STATION.PAK"];

/// One of STATION's frames at 59.94 Hz, in ms (the first of a pair runs
/// this long before the second).
const HALF_FRAME_MS: i64 = 17;

#[derive(Default)]
pub struct Station {
    /// `VIStation` while it runs (made at the first frame: it needs the clock)
    run: Option<Box<Run>>,
    /// what `start` was handed: the server and the saved login
    server: String,
    saved: (String, String, bool),
    /// keys that came since the last frame, for the window manager
    keys: Vec<(i32, bool)>,
    /// a pointer moved the page's selection: the move's sound, next frame
    moved: bool,
}

struct Run {
    r: Renderer,
    fader: Fader,
    /// `m_running`: `FadeOut` clears it
    running: bool,
    /// the last key was the keyboard's, not the pad's (`gKeyboardAttached`
    /// as the client can know it): with the options' Soft Keyboard, the
    /// editor's compact face
    keyboard: bool,
    /// the options' Soft Keyboard (`m_x91`)
    compact: bool,
}

impl Station {
    pub fn new() -> Self {
        Self::default()
    }

    /// The page up and its editor not (nothing being typed): `Pregame::quiet`.
    pub fn quiet(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.running && r.r.focus == Focus::Renderer)
    }

    /// `station_main`: what the run starts from (the renderer is made at
    /// the first frame, its `Load` at its first draw).
    pub fn start(&mut self, _res: &mut Res, shared: &mut Shared) {
        self.run = None;
        self.keys.clear();
        self.moved = false;
        self.server = shared.server.clone().unwrap_or_default();
        self.saved = crate::settings::saved_login();
        shared.login = None;
    }

    pub fn stop(&mut self) {
        self.run = None;
        self.keys.clear();
    }

    /// `VIStation::Init`: the renderer loaded off the disc's STATION.PAK,
    /// the fader from black.
    fn init(&self, cx: &Ctx) -> Run {
        let mut r = Renderer::new(cx.now, cx.vui.words.clone(), cx.vui.language);
        let pak = cx.res.raw("STATION.PAK").unwrap_or(&[]);
        let port = super::port("EQOA_AUTH_PORT", super::AUTH_PORT);
        r.load(pak, self.saved.clone(), &self.server, port as i32);
        let mut fader = Fader::new(1.0, cx.now);
        fader.begin(0.0, 0.5);
        let compact = crate::settings::game_options().compact != 0;
        Run { r, fader, running: true, keyboard: false, compact }
    }

    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        if self.run.is_none() {
            self.run = Some(Box::new(self.init(cx)));
        }
        let run = self.run.as_mut().unwrap();
        let mut cues = Vec::new();
        if std::mem::take(&mut self.moved) {
            cues.push(11);
        }
        let keys = std::mem::take(&mut self.keys);
        let done = run.process(cx.vui, &mut Quiet(&mut *cx.r), cx.now - HALF_FRAME_MS, keys, &mut cues).or_else(|| run.process(cx.vui, &mut *cx.r, cx.now, Vec::new(), &mut cues));
        cx.cues.extend(cues.into_iter().map(Cue::Ui));
        match done {
            None => Next::Stay,
            Some(1) => {
                let r = &run.r;
                cx.shared.login = Some(Login { user: r.login.clone(), token: net::auth::udp_token(&r.password) });
                Next::Client
            }
            Some(_) => Next::Setup,
        }
    }

    /// A key, down or up: the window manager hands it on at the next frame.
    pub fn key(&mut self, key: i32, down: bool) {
        self.keys.push((key, down));
    }

    /// The editor has the keys (a page's box: a name, a password).
    pub fn typing(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.r.result == 0 && run.r.focus == Focus::Editor && run.r.editor_visible)
    }

    /// A pointer at frame point `p`: the editor's keys while it is up, the
    /// dialog's prompt while one is, else a page's object (moved to, and X
    /// on the one already there) or a button glyph drawn on the page.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let Some(run) = self.run.as_mut() else { return Vec::new() };
        let r = &mut run.r;
        if r.result != 0 {
            return Vec::new();
        }
        if r.focus == Focus::Editor {
            return r.editor.point(p).flatten().into_iter().collect();
        }
        if r.dialog.is_open() {
            return r.dialog.point(p).into_iter().collect();
        }
        match page::Page::hit(&r.hits, p) {
            Some(page::Hit::Key(k)) => vec![k],
            Some(page::Hit::Obj(i)) => match r.pages.last_mut().map(|pg| pg.select_obj(i)) {
                Some(true) => {
                    self.moved = true;
                    vec![crate::pointer::MOVED]
                }
                Some(false) => vec![crate::pointer::key::CROSS],
                None => Vec::new(),
            },
            None => Vec::new(),
        }
    }

}

impl Run {
    /// `VIStation::Process`, one of STATION's frames: the keys to the window
    /// with the focus, the renderer drawn (and with it the API, the news),
    /// the editor over it, the fader over all. The renderer's result once
    /// the fade out is whole.
    fn process(&mut self, vui: &Vui, r: &mut dyn Layout, now: i64, keys: Vec<(i32, bool)>, cues: &mut Vec<i32>) -> Option<i32> {
        // `VIWindowManager::Update`: the keys
        for (k, down) in keys {
            self.keyboard = k < 128;
            match (self.r.focus, down) {
                (Focus::Editor, true) => match self.r.editor.key(vui, r, k, now, cues) {
                    crate::editbox::EditUp::Done(_) => self.r.edit_done(true),
                    crate::editbox::EditUp::Cancel => self.r.edit_done(false),
                    crate::editbox::EditUp::None => {}
                },
                (Focus::Editor, false) => self.r.editor.key_up(k),
                (Focus::Renderer, true) => self.r.key_down(k, now, cues),
                (Focus::Renderer, false) => {}
            }
            if self.r.result != 0 && self.running {
                // `FadeOut`, as the action that set the result calls it
                self.running = false;
                self.fader.begin(1.0, 0.5);
            }
        }
        // the windows: the renderer, the editor (its child) over it
        crate::editbox::set_pad_only(self.keyboard && self.compact);
        r.anchor(Anchor::Middle);
        self.r.draw(vui, r, now);
        if self.r.editor_visible {
            self.r.editor.draw(vui, r, now, cues);
        }
        if self.r.result != 0 && self.running {
            self.running = false;
            self.fader.begin(1.0, 0.5);
        }
        self.fader.update(r, now);
        (!self.running && self.fader.alpha == 1.0).then_some(self.r.result)
    }
}

/// The first of a game frame's two STATION frames: everything it steps runs
/// (the API, the windows' animations, the frame counts), what it draws is
/// dropped. What a window measures (a string's width, the font's height,
/// the viewport, the frame's sides) is the real raster's.
struct Quiet<'a>(&'a mut dyn Layout);

impl Raster for Quiet<'_> {
    fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
    fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
    fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
    fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
    fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
    fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
    fn string_width(&self, s: &[u16]) -> i32 {
        self.0.string_width(s)
    }
    fn point_size(&self) -> i32 {
        self.0.point_size()
    }
    fn viewport(&self) -> [i32; 4] {
        self.0.viewport()
    }
    fn set_viewport(&mut self, v: [i32; 4]) {
        self.0.set_viewport(v)
    }
    fn has(&self, id: u32) -> bool {
        self.0.has(id)
    }
}

impl Layout for Quiet<'_> {
    fn side(&self) -> i32 {
        self.0.side()
    }
    fn anchor(&mut self, a: Anchor) {
        self.0.anchor(a)
    }
    fn fill_blended(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid};

    /// A Station on the disc's login page, drawn once (the page notes where
    /// its objects went).
    fn station() -> Option<(Station, Vui)> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut i = iso::Iso::open(&iso).unwrap();
        let words = vi::strtab::StringTable::parse(&i.read("DATA/WORDS.TXT").unwrap()).unwrap();
        let mut r = Renderer::new(0, words.clone(), 0);
        r.load(&i.read("STATION.PAK").unwrap(), (String::new(), String::new(), false), "192.0.2.1", 9735);
        let vui = Vui::new(words, Default::default(), Vec::new());
        r.draw(&vui, &mut NullRaster, 17);
        let run = Run { r, fader: Fader::new(0.0, 0), running: true, keyboard: false, compact: true };
        Some((Station { run: Some(Box::new(run)), ..Default::default() }, vui))
    }

    /// Where object `i` of the page on top was drawn.
    fn rect_of(s: &Station, i: usize, vui: &Vui) -> [i32; 4] {
        let o = &s.run.as_ref().unwrap().r.page().unwrap().objs[i];
        let x0 = o.x as i32 + o.image_off[0] as i32;
        let (w, h) = if o.width > 0 { (o.width as i32, o.height as i32) } else { (24, vui.font_height(&NullRaster)) };
        [x0, o.y as i32, x0 + w, o.y as i32 + h]
    }

    /// A pointer on the login page: a field not selected is moved to (the
    /// move's sound at the next frame), the one selected is pressed (X), the
    /// legend's glyphs are their buttons; with a dialog up only its prompt
    /// is. The controls: nothing under the pointer is no key, and a page
    /// not yet drawn has nothing to hit.
    #[test]
    fn a_pointer_on_the_pages() {
        let Some((mut s, vui)) = station() else { return };
        let page = |s: &Station| s.run.as_ref().unwrap().r.page().unwrap().clone();
        let pw = page(&s).find("StationPassword").unwrap();
        let at = mid(rect_of(&s, pw, &vui));
        assert_eq!(s.point(at), vec![crate::pointer::MOVED]);
        assert!(s.moved, "the move sounds");
        assert_eq!(page(&s).grid.selected(), Some(pw));
        assert_eq!(s.point(at), vec![crate::pointer::key::CROSS], "the selection: X");
        let x = page(&s).objs.iter().position(|o| o.texture == 173).unwrap();
        let tri = page(&s).objs.iter().position(|o| o.texture == 175).unwrap();
        assert_eq!(s.point(mid(rect_of(&s, x, &vui))), vec![134]);
        assert_eq!(s.point(mid(rect_of(&s, tri, &vui))), vec![132]);
        assert_eq!(s.point([630.0, 5.0]), Vec::<i32>::new(), "the control: nothing there");
        // a dialog up: only its prompt
        let r = &mut s.run.as_mut().unwrap().r;
        r.on_action("Disconnect", 0);
        r.draw(&vui, &mut NullRaster, 34);
        assert_eq!(s.point(at), Vec::<i32>::new(), "the page under a dialog");
        let d = &s.run.as_ref().unwrap().r.dialog;
        let prompt = crate::pointer::rows_rect(&d.window, &vui, &NullRaster, 153, 10, 40, 1);
        assert_eq!(s.point(mid(prompt)), vec![132], "a confirmation's prompt is Cancel");
        // the control: before a draw, nothing was noted
        let Some((mut fresh, _)) = station() else { return };
        fresh.run.as_mut().unwrap().r.hits.clear();
        assert_eq!(fresh.point(at), Vec::<i32>::new());
    }
}
