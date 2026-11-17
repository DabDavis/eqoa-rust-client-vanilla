//! The dyers' colour picker in play (`dyepicker`, `net::dyepick`): go-eqoa-server
//! opens it when "Pick any colour..." is chosen at an Equipment Dyer, a Pet
//! Groomer or a Weapon Enchanter (0x7E95), and it takes the keys and the pointer
//! as its window (mode [`MODE`], the client's own) until OK or Cancel, which go
//! back to the server (0x7E96). While it is up the colour is previewed: on the
//! player's own character, its dye slots' colours (or a held weapon's aura)
//! drawn in it ([`preview`]), or on its pet, the area's dye (`petdye::preview`);
//! Cancel puts them back, OK keeps them a moment, until the server's own
//! comes. A child of `play`, for `Play`'s fields.

use super::Play;
use crate::dyepicker::{Picker, PickerUp};
use net::dyepick::{Message, Open, Target};

/// `VIWndGame`'s mode for the picker: none of the game's (0-44), nor the
/// record's (60) or the merc's (61, 62).
pub const MODE: i32 = 63;

/// How often (ms) the preview follows the colour while it changes: a dyed
/// body is built anew each time.
const PREVIEW_EVERY: i64 = 100;

/// How long (ms) a colour picked stays previewed after OK: until the
/// server's own is drawn.
const LINGER: i64 = 2500;

/// How long (ms) after a press a click at the window is the press's (a touch
/// gives both).
const SWALLOW: i64 = 600;

/// The picker, the server's open it is for and its preview.
#[derive(Default)]
pub struct DyePlay {
    pub picker: Picker,
    /// the open the picker is up for (its token, target, slots)
    pub open: Option<Open>,
    /// an open waiting for the game to be where it may come up
    pending: Option<Open>,
    keys: Vec<(i32, bool)>,
    /// the colour previewed and when it was set; whether it is in force
    shown: Option<(u32, i64)>,
    pub active: bool,
    linger_until: i64,
    /// the last pointer place (a release has none of its own) and when a
    /// press was last taken
    last_at: [f32; 2],
    pressed_at: Option<i64>,
    /// the recent colours, read once
    recent: Option<Vec<u32>>,
}

/// The player's own entity `e` as the picker previews it: its dye slots'
/// colours (or the aura's place) in the colour shown; None when nothing is
/// previewed on it (a pet's picker, none up, a server that sends no colours).
pub fn preview(d: &DyePlay, e: &net::entity::Entity) -> Option<net::entity::Entity> {
    let (c, _) = d.shown.filter(|_| d.active)?;
    let o = d.open.as_ref()?;
    if o.target == Target::Pet {
        return None;
    }
    let mut out = e.clone();
    let colors = out.creature.as_mut()?.colors.as_mut()?;
    paint(colors, o, c);
    Some(out)
}

/// A colour on the live channel's colours as the picker's open names them:
/// a dye slot's (go-eqoa-server's chardyes order), or the aura of a place.
pub fn paint(colors: &mut net::entity::LiveColors, o: &Open, c: u32) {
    let rgba = c.to_be_bytes();
    match o.target {
        Target::Character => {
            for &s in &o.slots {
                match s {
                    0..=5 => colors.armor[s as usize] = rgba,
                    6..=8 => colors.items[s as usize - 6] = rgba,
                    9 => colors.robe = rgba,
                    11..=16 => colors.trim[s as usize - 11] = rgba,
                    // the cape's (10) is the capes' list's, not the channel's
                    _ => {}
                }
            }
        }
        Target::Aura => {
            let (kind, source, nodes) = o.aura.unwrap_or_default();
            for &s in &o.slots {
                if let Some(a) = colors.auras.get_mut(s as usize) {
                    *a = net::entity::PieceAura { color: rgba, kind, source, nodes };
                }
            }
        }
        Target::Pet => {}
    }
}

impl Play {
    /// The picker's window has the keys.
    pub fn dyepick_owns(&self) -> bool {
        self.mode == MODE
    }

    /// A key for the picker, handled with the next HUD frame.
    pub fn dyepick_key(&mut self, key: i32, down: bool) {
        self.dyepick.keys.push((key, down));
    }

    /// A character typed at the picker (hex digits, '#', Backspace as '\u{8}').
    pub fn dyepick_char(&mut self, c: char) {
        if self.dyepick_owns() {
            self.dyepick.picker.type_char(c);
        }
    }

    /// A click at frame point `f` on the picker: a press and its release
    /// there, unless a press just took it (a touch gives both). Never keys.
    pub fn dyepick_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        let now = self.dyepick_now();
        if self.dyepick.pressed_at.take().is_some_and(|t| now - t < SWALLOW) {
            return Vec::new();
        }
        if self.dyepick.picker.press(f) {
            let mut cues = Vec::new();
            let up = self.dyepick.picker.release(f, &mut cues);
            self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
            if let Some(up) = up {
                self.dyepick_done(up);
            }
        }
        Vec::new()
    }

    /// A press at frame point `f` while the picker is up: true (the whole
    /// screen is the window's while it is up).
    pub(super) fn dyepick_press(&mut self, f: [f32; 2]) -> bool {
        self.dyepick.last_at = f;
        if self.dyepick.picker.press(f) {
            self.dyepick.pressed_at = Some(self.dyepick_now());
        }
        true
    }

    /// The pointer at `f` while a press of the picker is held. False with
    /// none.
    pub(super) fn dyepick_drag(&mut self, f: [f32; 2]) -> bool {
        if !self.dyepick_owns() || !self.dyepick.picker.pressed() {
            return false;
        }
        self.dyepick.last_at = f;
        self.dyepick.picker.drag(f)
    }

    /// The press let go. False with none held.
    pub(super) fn dyepick_release(&mut self) -> bool {
        if !self.dyepick.picker.pressed() {
            return false;
        }
        let mut cues = Vec::new();
        let up = self.dyepick.picker.release(self.dyepick.last_at, &mut cues);
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        if let Some(up) = up {
            self.dyepick_done(up);
        }
        true
    }

    /// The picker's clock (ms since the first ask): its held keys' repeats,
    /// the preview's pace.
    fn dyepick_now(&self) -> i64 {
        thread_local! {
            static START: web_time::Instant = web_time::Instant::now();
        }
        START.with(|s| s.elapsed().as_millis() as i64)
    }

    /// 0x7E95 after the offer: a picker opened (up at once where the game
    /// lets a window come, else when it does) or closed by the server.
    pub(super) fn dyepick_opcode(&mut self, data: &[u8]) {
        match net::dyepick::read(data) {
            Some(Message::Open(o)) => {
                eprintln!("play: the dyers' picker {:08X}: {:?} {:?}, now {:08X}", o.token, o.target, o.slots, o.current);
                self.dyepick.pending = Some(o);
            }
            Some(Message::Close(t)) => {
                if self.dyepick.pending.as_ref().is_some_and(|o| o.token == t) {
                    self.dyepick.pending = None;
                }
                if self.dyepick.open.as_ref().is_some_and(|o| o.token == t) {
                    eprintln!("play: the dyers' picker {t:08X} closed by the server");
                    self.dyepick_close(false);
                }
            }
            None => eprintln!("play: a dyers' picker message of {} bytes not read", data.len()),
        }
    }

    /// The picker down: back to play, its preview gone (`linger`: kept a moment).
    fn dyepick_close(&mut self, linger: bool) {
        self.dyepick.picker.close();
        self.dyepick.keys.clear();
        if !linger {
            self.dyepick.shown = None;
            self.dyepick.linger_until = 0;
            crate::petdye::preview(None);
        }
        if self.mode == MODE {
            self.mode = 0;
        }
    }

    /// OK or Cancel: told to the server, the picker down.
    fn dyepick_done(&mut self, up: PickerUp) {
        let Some(o) = self.dyepick.open.take() else {
            self.dyepick_close(false);
            return;
        };
        match up {
            PickerUp::Pick(c) => {
                eprintln!("play: the dyers' picker {:08X}: {c:08X} picked (0x7E96)", o.token);
                self.session.send(&net::dyepick::picked(o.token, c));
                let recent = self.dyepick.recent.get_or_insert_with(Vec::new);
                crate::dyepicker::remember(recent, c);
                crate::settings::save_dye_recent(&crate::dyepicker::write_recent(recent));
                let now = self.dyepick_now();
                self.dyepick.shown = Some((c, now));
                self.dyepick.linger_until = now + LINGER;
                self.dyepick.open = Some(o);
                self.dyepick_close(true);
            }
            PickerUp::Cancel => {
                eprintln!("play: the dyers' picker {:08X} cancelled (0x7E96)", o.token);
                self.session.send(&net::dyepick::cancelled(o.token));
                self.dyepick_close(false);
            }
        }
    }

    /// The picker's open, keys, drawing and preview, each HUD frame.
    pub(super) fn run_dyepick(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.dyepick_now();
        // an open waiting: up where a window may come (play, the dyer's box
        // answered, the chat line's history)
        if self.dyepick.pending.is_some() && matches!(self.mode, 0 | 7 | 9 | MODE) {
            let o = self.dyepick.pending.take().unwrap_or_default();
            if let Some(old) = self.dyepick.open.take().filter(|old| old.token != o.token && self.dyepick.picker.open) {
                self.session.send(&net::dyepick::cancelled(old.token));
            }
            let recent = self.dyepick.recent.get_or_insert_with(|| crate::dyepicker::read_recent(&crate::settings::dye_recent())).clone();
            self.dyepick.picker.open(&o.title, o.current, o.alpha, o.palette.clone(), recent);
            self.dyepick.open = Some(o);
            self.dyepick.keys.clear();
            self.mode = MODE;
            self.cues.push(crate::sound::Cue::Ui(5));
        }
        // another window took the screen (a death, a GM's window): cancelled
        if self.dyepick.picker.open && self.mode != MODE {
            self.dyepick_done(PickerUp::Cancel);
        }
        if self.mode == MODE {
            let mut cues = Vec::new();
            let mut ups = Vec::new();
            for (k, down) in std::mem::take(&mut self.dyepick.keys) {
                ups.extend(self.dyepick.picker.key(k, down, now, &mut cues));
            }
            self.dyepick.picker.tick(now);
            self.dyepick.picker.draw(vui, r);
            self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
            for up in ups {
                self.dyepick_done(up);
            }
        }
        // the preview: the colour now, at most every PREVIEW_EVERY while it moves
        let up = self.dyepick.picker.open;
        if up {
            let c = self.dyepick.picker.rgba();
            if self.dyepick.shown.is_none_or(|(s, at)| s != c && now - at >= PREVIEW_EVERY) {
                self.dyepick.shown = Some((c, now));
            }
        }
        let was = self.dyepick.active;
        self.dyepick.active = up || (self.dyepick.shown.is_some() && now < self.dyepick.linger_until);
        if was && !self.dyepick.active {
            self.dyepick.shown = None;
            crate::petdye::preview(None);
        }
        if self.dyepick.active
            && let (Some(o), Some((c, _))) = (self.dyepick.open.as_ref(), self.dyepick.shown)
            && let Some((id, d)) = o.pet.filter(|(id, _)| *id != 0 && o.target == Target::Pet)
        {
            let mut d = d;
            for &s in &o.slots {
                if let Some(a) = d.area.get_mut(s as usize) {
                    *a = c;
                }
            }
            crate::petdye::preview(Some((id, d)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::{Player, Session};

    fn play() -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        Play::new(s, Player::default(), Box::default())
    }

    fn open_bytes(o: &Open) -> Vec<u8> {
        let mut v = vec![net::dyepick::OPEN];
        v.extend(o.token.to_le_bytes());
        v.push(match o.target {
            Target::Character => 0,
            Target::Pet => 1,
            Target::Aura => 2,
        });
        v.push(o.alpha as u8);
        v.extend(o.current.to_le_bytes());
        v.push(o.title.len() as u8);
        v.extend(o.title.as_bytes());
        v.push(o.slots.len() as u8);
        v.extend(&o.slots);
        if let Some((id, d)) = o.pet {
            for x in [id, d.look, d.area[0], d.area[1]] {
                v.extend(x.to_le_bytes());
            }
            v.extend(d.mask.to_le_bytes());
            v.push(d.rule as u8 ^ 1);
        }
        if let Some((k, s, n)) = o.aura {
            v.push(k);
            v.extend(s.to_le_bytes());
            v.push(n);
        }
        v.push(o.palette.len() as u8);
        for c in &o.palette {
            v.extend(c.to_le_bytes());
        }
        v
    }

    fn sent(p: &Play) -> Vec<Vec<u8>> {
        p.session.record.as_ref().map(|r| r.iter().map(|(_, d)| d.clone()).filter(|d| d.starts_with(&[0x96, 0x7E])).collect()).unwrap_or_default()
    }

    /// A character with the live channel's colours.
    fn me() -> net::entity::Entity {
        let mut e = net::entity::Entity { id: 0x100, ..Default::default() };
        e.creature = Some(net::entity::Creature { colors: Some(net::entity::LiveColors { armor: [[0xFF; 4]; 6], ..Default::default() }), ..Default::default() });
        e
    }

    /// The server's open brings the picker up (mode 63) on the colour now;
    /// while it is up the character is previewed in the colour (its chest,
    /// nothing else); OK sends the colour with the token and the window goes,
    /// the preview kept a moment; Cancel sends the cancel and the preview
    /// goes at once. The controls: no open, no preview; a server's close of
    /// another token leaves it up.
    #[test]
    fn the_open_the_preview_and_the_answers() {
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut p = play();
        assert!(preview(&p.dyepick, &me()).is_none(), "nothing open");
        let o = Open { token: 0x7E95_0001, target: Target::Character, current: 0xD100_00FF, title: "Dye chest".into(), slots: vec![0], palette: vec![0x1010_10FF], ..Default::default() };
        p.dyepick_opcode(&open_bytes(&o));
        p.run_dyepick(&vui, &mut r);
        assert_eq!(p.mode, MODE);
        assert!(p.dyepick.picker.open && p.dyepick.picker.rgba() == 0xD100_00FF);
        p.dyepick.picker.set_rgba(0x3FA7_D6FF);
        p.dyepick.shown = None;
        p.run_dyepick(&vui, &mut r);
        let seen = preview(&p.dyepick, &me()).expect("previewed");
        let c = seen.creature.unwrap().colors.unwrap();
        assert_eq!(c.armor[0], [0x3F, 0xA7, 0xD6, 0xFF]);
        assert_eq!(c.armor[1], [0xFF; 4], "the chest alone");
        p.dyepick_opcode(&[net::dyepick::CLOSE, 9, 9, 9, 9]);
        assert!(p.dyepick.picker.open, "another token's close");
        p.dyepick_key(144, true);
        p.run_dyepick(&vui, &mut r);
        assert_eq!(p.mode, 0);
        assert_eq!(sent(&p), vec![net::dyepick::picked(0x7E95_0001, 0x3FA7_D6FF)]);
        assert!(preview(&p.dyepick, &me()).is_some(), "OK: kept a moment");
        // a second picker, cancelled by △
        p.session.record = Some(Vec::new());
        let o2 = Open { token: 0x7E95_0002, ..o.clone() };
        p.dyepick_opcode(&open_bytes(&o2));
        p.run_dyepick(&vui, &mut r);
        p.dyepick_key(132, true);
        p.run_dyepick(&vui, &mut r);
        assert_eq!(p.mode, 0);
        assert_eq!(sent(&p), vec![net::dyepick::cancelled(0x7E95_0002)]);
        assert!(preview(&p.dyepick, &me()).is_none(), "cancelled: the preview gone");
        // the server's close of the one up
        let o3 = Open { token: 0x7E95_0003, ..o };
        p.dyepick_opcode(&open_bytes(&o3));
        p.run_dyepick(&vui, &mut r);
        p.dyepick_opcode(&[net::dyepick::CLOSE, 3, 0, 0x95, 0x7E]);
        assert!(!p.dyepick.picker.open && p.mode == 0);
    }

    /// An aura's picker previews the aura of its place, its kind and source
    /// with the colour; a trim's and a weapon's slot are the channel's trim
    /// and item colours.
    #[test]
    fn the_slots_and_the_aura_painted() {
        let mut c = net::entity::LiveColors::default();
        let o = Open { target: Target::Character, slots: vec![6, 9, 13], ..Default::default() };
        paint(&mut c, &o, 0x3FA7_D6FF);
        assert_eq!((c.items[0], c.robe, c.trim[2]), ([0x3F, 0xA7, 0xD6, 0xFF], [0x3F, 0xA7, 0xD6, 0xFF], [0x3F, 0xA7, 0xD6, 0xFF]));
        assert_eq!(c.armor, [[0; 4]; 6]);
        let a = Open { target: Target::Aura, slots: vec![net::entity::MAIN_HAND_AURA2 as u8], aura: Some((7, 0x88EC_092D, 0)), ..Default::default() };
        paint(&mut c, &a, 0x00FF_00FF);
        assert_eq!(c.auras[net::entity::MAIN_HAND_AURA2], net::entity::PieceAura { color: [0, 0xFF, 0, 0xFF], kind: 7, source: 0x88EC_092D, nodes: 0 });
        assert_eq!(c.auras[net::entity::MAIN_HAND_AURA], Default::default());
    }

    /// A pet's picker previews the area on the pet of its id and body, the
    /// other area as it was; Cancel takes it off.
    #[test]
    fn a_pets_preview() {
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut p = play();
        let d = net::petdye::PetDye { look: 0x8A41_6241, area: [0x1111_11FF, 0], mask: 0, rule: true };
        let o = Open { token: 5, target: Target::Pet, alpha: true, slots: vec![1], pet: Some((0x0300_0001, d)), ..Default::default() };
        p.dyepick_opcode(&open_bytes(&o));
        p.run_dyepick(&vui, &mut r);
        p.dyepick.picker.set_rgba(0x3FA7_D680);
        p.dyepick.shown = None;
        p.run_dyepick(&vui, &mut r);
        assert_eq!(crate::petdye::of(0x0300_0001, 0x8A41_6241).map(|d| d.area), Some([0x1111_11FF, 0x3FA7_D680]));
        assert_eq!(crate::petdye::of(0x0300_0001, 0x1234), None, "another body");
        assert!(preview(&p.dyepick, &me()).is_none(), "a pet's: the character as it is");
        p.dyepick_key(132, true);
        p.run_dyepick(&vui, &mut r);
        assert_eq!(crate::petdye::of(0x0300_0001, 0x8A41_6241), None);
    }
}
