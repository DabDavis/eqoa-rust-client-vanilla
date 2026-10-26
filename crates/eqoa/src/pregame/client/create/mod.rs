//! Character creation: `VIWndCharCreateBlank` (CLIENT `viwndcharcreateblank.cpp`)
//! and its five pages, each a window of its own: race and sex
//! (`VIWndCharCreateRace`, `race.rs`), class then attributes
//! (`VIWndCharCreateClass`, `class.rs`), the name (`VIWndCharCreateName`,
//! `name.rs`), the appearance (`VIWndCharCreateMisc`, `misc.rs`) and the
//! review (`VIWndCharCreateReview`, `review.rs`), in that order. The form
//! (`VINewChar`, here `net::session::NewChar`) is filled page by page as each
//! hands its part up (`OnMessage`: 262 the race, sex and city, 263 the class,
//! 265 the stats, 266 the name, 264 the look, 267 create; 260 a page went
//! back), and goes out as 0x002B (`SendNewChar`).
//!
//! Character select (`super`) opens it on an empty slot (`SelectCharacter`:
//! `Focus`, the race page up, the form as it was left), routes it the keys,
//! the pointer and the session's events, hands it the session and the stage
//! every frame, and draws its scene (`scene`). What `VIClient` does round it:
//!
//! - Back out of the race page is `BackToLogin`: creation closes and resets,
//!   character select comes back (`Out::Back`).
//! - Create is `VIClient::CreateCharacter`: creation closes (`Out::Done`) and
//!   0x002B goes out (sent here); character select then makes the form its
//!   next entry with no server id yet, selects it, shows its preview and word
//!   413 (`form()` is what was sent). The server's answer is character
//!   select's: a new list (0x002C, `CharacterList`, which also `reset`s this
//!   form) or a refusal (0x002E 269, 0x0065 828, 0x002F 829: the entry taken
//!   away and the word shown, `CharSelectMsg`), after which a new creation
//!   finds its pages as they were left, as on the console.
//! - The name page's Generate Name is `Send0012` (sent here); its answer
//!   (0x0012, `Event::RandomName`) comes back through `event`.
//!
//! The 3D: the race, appearance and review pages stand the model in its
//! race's set on the stage (`pregame::stage`, ARENA.CSF), the walk camera on
//! it, and render into `Viewport(358, 48, 620, 396)` after their 2D (the
//! review draws its starting city over it). The appearance page lights the
//! model with the scene's personal light (`SetPersonalLightMode(1)`,
//! `SetPersonalLight(eye, 5.0, (0.75, 0.69, 0.36))`), whose radius ramps up
//! at 0.002 a millisecond (`VIScene::Render`'s `m_p2A40`); the others turn it
//! off (mode 2).
//!
//! The pages' keys are handled at the start of the next frame (the window
//! manager reads them once a frame), with what they need to hand on.

mod class;
mod misc;
mod name;
mod race;
mod review;
pub mod tables;

use crate::client_image::ClientImage;
use crate::pregame::figure::Figure;
use crate::pregame::stage::{Stage, VIEWPORT};
use crate::pregame::{Ctx, Music, Scene};
use crate::sound::Cue;
use net::session::{Event, NewChar, Session};

/// What creation tells character select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Stay,
    /// backed out of the first page (260): character select again
    /// (`BackToLogin`); the form was reset
    Back,
    /// Create (267): the form went out as 0x002B (`form()`); character
    /// select takes it from here (`CreateCharacter`'s entry, preview and
    /// word 413; the server's answer)
    Done,
}

/// What a page hands its parent (`GetParent()->OnMessage(this, msg, p1)`).
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// 260: back
    Back,
    /// 262: the race, sex and city
    Race { race: i32, sex: i32, city: i32 },
    /// 263: the class
    Class(i32),
    /// 265: the stats, each less the race's and the class's bonus
    Stats([i32; 7]),
    /// 266: the name, capitalised
    Name(String),
    /// 264: face, hair style, hair length, hair colour
    Look([i32; 4]),
    /// 267: create
    Create,
}

/// The page up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Race,
    Class,
    Name,
    Misc,
    Review,
}

/// What the pages share besides the form: CLIENT's tables, the model in the
/// scene, the scene's personal light.
#[derive(Default)]
pub struct Env {
    /// CLIENT's image, once it has come (`modules`): the races' default looks
    pub client: Option<ClientImage>,
    /// its race cameras (`gRaceCameras`, `gRaceCamerasEx`)
    pub race_cams: Option<[Vec<crate::camstyle::RaceCamera>; 2]>,
    /// the model (`m_actor`; one at a time, as each page creates and
    /// releases its own)
    pub figure: Option<Figure>,
    /// `m_p2A40`: the personal light's radius now, ramping toward the one set
    pub light: f32,
    /// when the personal light last ran (`m_p2A38`)
    pub light_time: i64,
    /// this frame's personal light for the scene's view
    pub point: Option<(glam::Vec3, f32, glam::Vec3)>,
    /// whether this frame's page rendered the scene
    pub show3d: bool,
    /// the owned expansions (`VIClient::ExpansionFlags`)
    pub expansion: u32,
}

impl Env {
    /// `ReleaseActor`.
    pub fn release(&mut self) {
        self.figure = None;
    }

    /// `GetRaceCamera(race, ExpansionFlags())`.
    pub fn race_camera(&self, race: i32) -> Option<crate::camstyle::RaceCamera> {
        self.race_cams.as_ref().and_then(|t| crate::camstyle::race_camera(t, race, self.expansion))
    }

    /// `GetRaceInfo(race, ExpansionFlags())`; zeros until CLIENT's image is in.
    pub fn race_info(&self, race: i32) -> tables::RaceInfo {
        self.client.as_ref().map(|c| tables::race_info(c, race, self.expansion)).unwrap_or_default()
    }
}

pub struct Create {
    open: bool,
    /// `m_char`: the form
    form: NewChar,
    page: Page,
    /// the page that has just been made active (295 waits for the frame)
    activate: Option<Page>,
    race: race::RacePage,
    class: class::ClassPage,
    name: name::NamePage,
    misc: misc::MiscPage,
    review: review::ReviewPage,
    env: Env,
    keys: Vec<(i32, bool)>,
    /// 0x0012's answer, for the name page
    random_name: Option<String>,
    /// what Create sent
    sent: NewChar,
    /// the last key came from a keyboard (`gKeyboardAttached`), and the
    /// options' Soft Keyboard is off (`m_x91`): the name's editor then shows
    /// its compact face, else the soft keyboard (`sPadOnly`)
    keyboard: bool,
    compact: bool,
}

impl Default for Create {
    fn default() -> Self {
        let mut c = Create {
            open: false,
            form: NewChar::default(),
            page: Page::Race,
            activate: None,
            race: race::RacePage::default(),
            class: class::ClassPage::default(),
            name: name::NamePage::default(),
            misc: misc::MiscPage::default(),
            review: review::ReviewPage::default(),
            env: Env::default(),
            keys: Vec::new(),
            random_name: None,
            sent: NewChar::default(),
            keyboard: true,
            compact: true,
        };
        c.reset();
        c
    }
}

/// `VIWndCharCreateBlank::Reset`'s form: no name, city 0, and every choice
/// -1 (the pages then take the race's defaults), no points spent.
pub fn blank_form() -> NewChar {
    NewChar { name: String::new(), race: -1, class: -1, sex: -1, hair_color: -1, hair_length: -1, hair_style: -1, face: -1, city: 0, stats: [0; 7] }
}

impl Create {
    pub fn new() -> Self {
        Self::default()
    }

    /// `VIWndCharCreateBlank::Focus` (`SelectCharacter` on an empty slot): the
    /// race page up and active, the others down; the form as it was left.
    pub fn open(&mut self) {
        self.open = true;
        self.page = Page::Race;
        self.activate = Some(Page::Race);
        self.keys.clear();
        self.compact = crate::settings::game_options().compact != 0;
    }

    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// `VIWndCharCreateBlank::Reset`: the form and every page (character
    /// select calls it when the server's list comes, `CharacterList`).
    pub fn reset(&mut self) {
        self.form = blank_form();
        self.race.reset(&mut self.env);
        self.class.reset();
        self.name.reset();
        self.misc.reset(&mut self.env);
        self.review.reset(&mut self.env);
    }

    /// The form as Create sent it (0x002B): what character select makes its
    /// next entry from (`CreateCharacter`).
    pub fn sent(&self) -> &NewChar {
        &self.sent
    }

    /// The form being filled.
    #[cfg(test)]
    pub fn current(&self) -> &NewChar {
        &self.form
    }

    /// The name page's editor has the keys (its box of a name, no message
    /// over it): what a phone's own keyboard may type into.
    pub fn typing(&self) -> bool {
        self.open && self.page == Page::Name && !self.name.mbox_up
    }

    #[cfg(test)]
    pub fn page(&self) -> Page {
        self.page
    }

    /// One frame of the page up: the keys since the last (its requests go on
    /// `session`), then its drawing, and its model on `stage`.
    pub fn frame(&mut self, cx: &mut Ctx, session: &mut Session, stage: Option<&mut Stage>) -> Out {
        if !self.open {
            return Out::Stay;
        }
        if self.env.client.is_none() {
            self.env.client = crate::modules::get(crate::modules::Module::Client).and_then(|i| ClientImage::from_bytes(i.to_vec()));
            self.env.race_cams = self.env.client.as_ref().map(crate::camstyle::race_cameras);
        }
        // `VIClient::ExpansionFlags`: 0x07D1's mask and the disc's
        self.env.expansion = session.expansion & net::session::EXPANSION_FLAGS;
        let mut stage = stage;
        let mut sounds = Vec::new();
        let mut out = Out::Stay;
        if let Some(name) = self.random_name.take() {
            self.name.set_name(&name);
        }
        for (key, down) in std::mem::take(&mut self.keys) {
            if !self.open {
                break;
            }
            if !down {
                if self.page == Page::Name {
                    self.name.key_up(key);
                }
                continue;
            }
            let msg = match self.page {
                Page::Race => self.race.key(key, cx.now, &mut self.env, &mut sounds),
                Page::Class => self.class.key(key, cx.now, &self.form, self.env.expansion, &mut sounds, cx.vui),
                Page::Name => self.name.key(key, cx.now, cx.vui, &*cx.r, &self.form, session, &mut sounds),
                Page::Misc => self.misc.key(key, cx.now, &mut self.env, stage.as_deref_mut(), &mut sounds),
                Page::Review => self.review.key(key, &mut self.env, &mut sounds),
            };
            if let Some(m) = msg {
                out = self.message(m, session, cx.now);
            }
        }
        cx.cues.extend(sounds.drain(..).map(Cue::Ui));
        if out != Out::Stay {
            return out;
        }
        // the page made active since: its 295
        if let Some(p) = self.activate.take() {
            match p {
                Page::Race => self.race.activate(cx.now, &self.form, self.env.expansion),
                Page::Class => self.class.activate(cx.now, &self.form, self.env.expansion, cx.vui),
                Page::Name => self.name.activate(cx.now, cx.vui, &self.form),
                Page::Misc => self.misc.activate(cx.now, &self.form, &self.env),
                Page::Review => {}
            }
        }
        self.env.show3d = false;
        self.env.point = None;
        match self.page {
            Page::Race => self.race.draw(cx, &mut self.env, stage, &self.form, &mut sounds),
            Page::Class => self.class.draw(cx, &self.form, self.env.expansion),
            Page::Name => {
                // VIWndEdit::OnDrawSelf: sPadOnly from the keyboard in use and the option
                crate::editbox::set_pad_only(self.keyboard && self.compact);
                self.name.draw(cx, &mut sounds)
            }
            Page::Misc => self.misc.draw(cx, &mut self.env, stage, &self.form),
            Page::Review => self.review.draw(cx, &mut self.env, stage, &self.form),
        }
        cx.cues.extend(sounds.drain(..).map(Cue::Ui));
        Out::Stay
    }

    /// `VIWndCharCreateBlank::OnMessage`: a page's part into the form, and
    /// the next page up.
    fn message(&mut self, m: Msg, session: &mut Session, now: i64) -> Out {
        match m {
            Msg::Back => match self.page {
                // BackToLogin
                Page::Race => {
                    self.open = false;
                    self.reset();
                    return Out::Back;
                }
                Page::Class => self.show(Page::Race),
                Page::Misc => {
                    // the name page up, and its Activate (which makes it active)
                    self.page = Page::Name;
                    self.activate = Some(Page::Name);
                }
                Page::Name => self.show(Page::Class),
                Page::Review => {
                    self.misc.reactivate(&self.form, &mut self.env);
                    self.show(Page::Misc);
                }
            },
            Msg::Race { race, sex, city } => {
                self.form.race = race;
                self.form.sex = sex;
                self.form.city = city;
                self.class.set_race(race, city, &self.form, self.env.expansion, now);
                self.show(Page::Class);
            }
            Msg::Class(c) => self.form.class = c,
            Msg::Stats(s) => {
                self.form.stats = s;
                self.page = Page::Name;
                self.activate = Some(Page::Name);
            }
            Msg::Name(n) => {
                self.form.name = n;
                self.misc.reactivate(&self.form, &mut self.env);
                self.show(Page::Misc);
            }
            Msg::Look([face, style, length, color]) => {
                self.form.face = face;
                self.form.hair_style = style;
                self.form.hair_color = color;
                self.form.hair_length = length;
                self.show(Page::Review);
            }
            Msg::Create => {
                // VIClient::CreateCharacter: the form goes out; character
                // select takes the rest
                self.open = false;
                self.env.release();
                session.create_character(&self.form);
                self.sent = self.form.clone();
                return Out::Done;
            }
        }
        Out::Stay
    }

    /// `SetVisible(true)` and `SetActiveWindow(page)`: its 295 at the next frame.
    fn show(&mut self, p: Page) {
        self.page = p;
        self.activate = Some(p);
    }

    /// A session event the pages answer: 0x0012's name (`SetName`, into the
    /// name page's editor). Character select's own (the new list, a refusal)
    /// pass by.
    pub fn event(&mut self, e: &Event) -> Out {
        if let Event::RandomName(n) = e {
            self.random_name = Some(n.clone());
        }
        Out::Stay
    }

    pub fn key(&mut self, key: i32, down: bool) {
        if self.open {
            if down {
                // the pad's keys are 128 and up
                self.keyboard = key < 128;
            }
            self.keys.push((key, down));
        }
    }

    /// A pointer at frame point `p`: the keys the page under it asks for.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if !self.open {
            return Vec::new();
        }
        match self.page {
            Page::Race => self.race.point(p),
            Page::Class => self.class.point(p, &self.form, self.env.expansion),
            Page::Name => self.name.point(p),
            Page::Misc => self.misc.point(p),
            Page::Review => self.review.point(p),
        }
    }

    /// The art swapped (F9): the page's model made again in it where it
    /// stands (not released: a page that makes its model anew starts its
    /// race's music again).
    pub fn remake(&mut self, lib: &esf::Library, g: &mut crate::play::Gpu, now: i64) {
        if let Some(f) = self.env.figure.as_mut() {
            f.remake(lib, g, now);
        }
    }

    /// The page's 3D: its model in its race's set, through the stage's view,
    /// with the personal light the page set.
    pub fn scene<'a>(&'a self, stage: &'a Stage) -> Option<Scene<'a>> {
        if !self.open || !self.env.show3d {
            return None;
        }
        let f = self.env.figure.as_ref()?;
        let mut view = stage.view();
        view.point = self.env.point;
        Some(Scene { view, viewport: VIEWPORT, models: stage.models().chain(f.models()).collect() })
    }
}

/// The model (`CreateActor` + `SetHair` + `SetFace`), its sprite and its
/// look, at `at` facing `hpr`.
pub(crate) fn create_model(cx: &mut Ctx, model: u32, look: [i32; 4], at: [f32; 3], hpr: [f32; 3]) -> Option<Figure> {
    let [face, style, length, color] = look;
    let l = vi::cust::Look { hair: [color.clamp(0, 255) as u8, length.clamp(0, 255) as u8, style.clamp(0, 255) as u8], face: face.clamp(0, 255) as u8, robe: -1, ..Default::default() };
    let g = cx.gpu.as_mut()?;
    Figure::new(&cx.res.lib, g, model, l, [0; 3], at, hpr, cx.now)
}

/// The model's look changed (`SetHair`, `SetFace` on the actor): built again
/// in the new look, its sprite, its place and its clock kept.
pub(crate) fn redress(cx: &mut Ctx, f: &mut Figure, look: [i32; 4]) {
    let Some(g) = cx.gpu.as_mut() else { return };
    let [face, style, length, color] = look;
    let l = vi::cust::Look { hair: [color.clamp(0, 255) as u8, length.clamp(0, 255) as u8, style.clamp(0, 255) as u8], face: face.clamp(0, 255) as u8, robe: -1, ..Default::default() };
    if let Some(mut n) = Figure::new(&cx.res.lib, g, f.model, l, [0; 3], f.at, f.hpr, cx.now) {
        // the new body, the old sprite: its animation goes on
        std::mem::swap(&mut n.gpu, &mut f.gpu);
        f.look = l;
    }
}

/// The walk camera framed on the model: `m_targetHeight`,
/// `m_lookAtHeightAbove`, `SetDefOrbitPitchAngle`, `SetOrbitDistance`.
pub(crate) fn frame_camera(stage: &mut Stage, v: [f32; 4]) {
    use vi::fpu::{Ee, F};
    stage.cam.target_height = F::<Ee>::new(v[0]);
    stage.cam.look_at_height_above = F::<Ee>::new(v[1]);
    stage.cam.set_def_orbit_pitch_angle(F::new(v[2]));
    stage.cam.set_orbit_distance(F::new(v[3]));
}

/// `UpdateCamera`'s default framing when the race has no camera.
pub(crate) const DEFAULT_FRAMING: [f32; 4] = [1.1, 1.7, 0.698_131_74, 0.75];

/// `Music::Bgm` on slot 0 (`PlayBgm(0, bgm)`, gFeatureFlags 0x100).
pub(crate) fn play_bgm(cx: &mut Ctx, path: &str) {
    cx.music.push(Music::Bgm { slot: 0, path: path.to_string() });
}

/// A word of WORDS.TXT in the UI's language, wide.
pub(crate) fn word(vui: &vi::ui::Vui, id: i32) -> Vec<u16> {
    vi::ui::wide(vui.words.get(id, vui.language))
}

/// The legend bar every page ends with (`BlitBar("", black, (50, 408), 25,
/// 26, w)` and its glyphs): triangle and its word, cross and its word, in
/// black; where each glyph and word were drawn, as frame rectangles, for a
/// pointer.
pub(crate) fn legend(vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, back: i32, select: i32) -> [[i32; 4]; 2] {
    legend_sized(vui, r, back, select, select)
}

/// `legend`, the bar sized for word `sized` whatever `select` shows (the
/// class page measures "Select" on both its pages).
pub(crate) fn legend_sized(vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, back: i32, select: i32, sized: i32) -> [[i32; 4]; 2] {
    let black = [0.0, 0.0, 0.0, 1.0];
    let (wb, ws) = (r.string_width(&word(vui, back)), r.string_width(&word(vui, select)));
    let mut at = [50, 408];
    let w = wb + 110 + r.string_width(&word(vui, sized));
    vui.blit_bar(r, &[], black, at, 25, 26, w, 0, 1.0, 0, true);
    at = [at[0] + 20, at[1] + 4];
    let g = vui.entry(65).uv1;
    vui.blit(r, 65, at);
    let tri = [at[0], at[1], at[0] + 25 + wb, at[1] + g[1].max(16)];
    at[0] += 25;
    r.draw_centered_string([at[0], at[1] + 5], black, &word(vui, back), 1);
    at[0] += 20 + wb;
    vui.blit(r, 66, at);
    let cross = [at[0], at[1], at[0] + 25 + ws, at[1] + g[1].max(16)];
    at[0] += 25;
    r.draw_centered_string([at[0], at[1] + 5], black, &word(vui, select), 1);
    [tri, cross]
}

/// A `Vui` for the pages' pointer tests: no words, and an atlas whose every
/// entry is a 20 by 22 piece of itself (the pages' rows are as tall as their
/// bars' art).
#[cfg(test)]
pub(crate) fn tests_vui() -> vi::ui::Vui {
    let atlas = (0..252).map(|i| vi::ui::UiTexEntry { x00: i, row: 0, uv0: [0, 0], uv1: [20, 22], mode: 0, alpha: 1.0, flags: 0 }).collect();
    vi::ui::Vui::new(Default::default(), Default::default(), atlas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::key;

    /// The pages in the decomp's order, by the messages between them: race,
    /// class, name, appearance, review; each back goes where
    /// `OnMessage` 260 sends it.
    #[test]
    fn the_pages_go_in_the_forms_order_and_back() {
        let mut c = Create::new();
        let mut s = test_session();
        c.open();
        assert_eq!(c.page, Page::Race);
        assert_eq!(c.message(Msg::Race { race: 6, sex: 1, city: 0 }, &mut s, 0), Out::Stay);
        assert_eq!((c.page, c.form.race, c.form.sex, c.form.city), (Page::Class, 6, 1, 0));
        c.message(Msg::Class(8), &mut s, 0);
        assert_eq!((c.page, c.form.class), (Page::Class, 8), "263 keeps the class page up (its attributes)");
        c.message(Msg::Stats([1, 2, 3, 4, 5, 5, 0]), &mut s, 0);
        assert_eq!((c.page, c.form.stats), (Page::Name, [1, 2, 3, 4, 5, 5, 0]));
        c.message(Msg::Name("Brakk".into()), &mut s, 0);
        assert_eq!((c.page, c.form.name.as_str()), (Page::Misc, "Brakk"));
        c.message(Msg::Look([1, 2, 3, 4]), &mut s, 0);
        assert_eq!(c.page, Page::Review);
        assert_eq!((c.form.face, c.form.hair_style, c.form.hair_length, c.form.hair_color), (1, 2, 3, 4));
        // back, page by page: review -> appearance -> name -> class -> race -> out
        c.message(Msg::Back, &mut s, 0);
        assert_eq!(c.page, Page::Misc);
        c.message(Msg::Back, &mut s, 0);
        assert_eq!(c.page, Page::Name);
        c.message(Msg::Back, &mut s, 0);
        assert_eq!(c.page, Page::Class);
        c.message(Msg::Back, &mut s, 0);
        assert_eq!(c.page, Page::Race);
        assert_eq!(c.message(Msg::Back, &mut s, 0), Out::Back);
        assert!(!c.is_open());
        assert_eq!(c.form, blank_form(), "BackToLogin resets the form");
        // the control: a wrong order would not land on the name page after the stats
        assert_ne!(Page::Misc, Page::Name);
    }

    /// Create sends the form as 0x002B and hands it over; a refusal later
    /// finds the form kept (no reset until the server's list).
    #[test]
    fn create_sends_the_form_and_keeps_it() {
        let mut c = Create::new();
        let mut s = test_session();
        c.open();
        c.message(Msg::Race { race: 0, sex: 0, city: 2 }, &mut s, 0);
        c.message(Msg::Class(1), &mut s, 0);
        c.message(Msg::Stats([0; 7]), &mut s, 0);
        c.message(Msg::Name("Arwen".into()), &mut s, 0);
        c.message(Msg::Look([0, 1, 2, 3]), &mut s, 0);
        s.flush();
        let before = SENT.with(|v| v.borrow().len());
        assert_eq!(c.message(Msg::Create, &mut s, 0), Out::Done);
        s.flush();
        let after: Vec<Vec<u8>> = SENT.with(|v| v.borrow()[before..].to_vec());
        assert!(after.iter().any(|d| d.windows(5).any(|w| w == b"Arwen")), "0x002B went out with the name");
        assert_eq!(c.sent().name, "Arwen");
        assert_eq!((c.sent().race, c.sent().city, c.sent().class), (0, 2, 1));
        assert!(!c.is_open());
        // opened again (a refusal came): the form as it was left
        c.open();
        assert_eq!(c.current().name, "Arwen");
        c.reset();
        assert_eq!(c.current(), &blank_form(), "the control: the server's list resets it");
    }

    /// A pointer goes to the page up: the race page's legend while it is
    /// up, the class page's after Enter; a closed creation takes none.
    #[test]
    fn a_pointer_goes_to_the_page_up() {
        let mut c = Create::new();
        let mut s = test_session();
        let v = tests_vui();
        let mut r = crate::pointer::testing::NullRaster;
        let res = crate::pregame::res::Res::new();
        let mut shared = crate::pregame::Shared::default();
        let (mut cues, mut music) = (Vec::new(), Vec::new());
        assert!(c.point([100.0, 420.0]).is_empty(), "closed: nothing");
        c.open();
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        c.frame(&mut cx, &mut s, None);
        let legend: Vec<i32> = (40..400).step_by(2).flat_map(|x| c.point([x as f32, 420.0])).collect();
        assert!(legend.contains(&key::TRIANGLE) && legend.contains(&key::CROSS));
        // Enter through the pointer's key: the class page up
        c.key(key::CROSS, true);
        c.key(key::CROSS, false);
        let mut cx = Ctx { now: 40, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        c.frame(&mut cx, &mut s, None);
        assert_eq!(c.page(), Page::Class);
        assert_eq!(c.current().race, 6, "the barbarian, the list's first");
        assert!(cues.iter().any(|c| matches!(c, Cue::Ui(13))), "the select sound");
    }

    thread_local! {
        /// What the test session sent.
        static SENT: std::cell::RefCell<Vec<Vec<u8>>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// A session nothing answers, its datagrams kept (`SENT`).
    fn test_session() -> Session {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                SENT.with(|v| v.borrow_mut().push(d.to_vec()));
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap()
    }
}
