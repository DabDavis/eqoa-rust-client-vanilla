//! The map's markers (F3 pass 18, the user: "mark popular xp camps on map with
//! an XP icon and hover tooltip with level range, rare items, etc.", "also add
//! all coaches to the map", "let the coaches have a horseshoe icon like they
//! have on the stables"): EQOA-Data's XP camps and every coach, each where it
//! is in its world (`markers_data`, written by `tools/gen_map_markers.py`:
//! EQOA-Data's map guide, github.com/Jadiction/EQOA-Data, MIT, at the places
//! go-eqoa-server gives the camps and its coachmen stand).
//!
//! A camp is a red badge with "XP" in gold; a coach the horseshoe the game's
//! stable signs carry (`ICON`, off the player's own disc: `Icon`), or one of
//! ours drawn the same way until the disc's is to hand. A marker's card (what
//! it is: a camp's levels, the guide's note, the rare creatures near it and
//! notable drops; a coach's town, coachman and where it runs) shows for the
//! one under the mouse, the one a finger tapped (tapped again, the waypoint
//! goes on it; a click puts it there at once), and the one under the pad's
//! middle cross once the map is panned. Map Markers (Extended's World page,
//! and the map's own X or ○) hides them all.

use super::markers_data::{CAMPS, COACHES};
use super::view::{self, View};

/// An XP camp, as `markers_data` lists it.
#[derive(Clone, Copy, Debug)]
pub struct Camp {
    /// EQOA-Data's name for it
    pub name: &'static str,
    /// go-eqoa-server's words for where it is (its `#tele` list's)
    pub place: &'static str,
    /// `gWorldInfo`'s index
    pub world: i32,
    /// (x, z), game units
    pub at: [f32; 2],
    /// the guide's level range
    pub levels: (u8, u8),
    /// the guide's note
    pub note: &'static str,
    /// the rare and raid creatures EQOA-Data's map marks near it
    pub rare: &'static [&'static str],
    /// what those name as their own drops
    pub own: &'static [&'static str],
    /// its zone's biome loot table ("" in Odus, whose zones name none)
    pub table: &'static str,
    /// that table's notable items at its levels, the highest first
    pub drops: &'static [&'static str],
}

/// A coach, as `markers_data` lists it: its coachman where it stands.
#[derive(Clone, Copy, Debug)]
pub struct Coach {
    pub coachman: &'static str,
    /// the town the coach menus name ("" for a coach in none)
    pub town: &'static str,
    pub world: i32,
    pub at: [f32; 2],
    /// the towns it runs to
    pub to: &'static [&'static str],
}

/// A marker on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Camp(usize),
    Coach(usize),
}

/// The stable sign's texture (record 0x1000's DictID): the horseshoe on the
/// board by Coachman Ronks in Freeport (TUNARIA.ESF zone 84, sprite
/// 0x75498C8D, 14 units from him), on no other file of the disc.
pub const ICON: i32 = 0x9BE5_7886_u32 as i32;
/// Where that sign stands (world 0, x and z), so the map can read its zone.
pub const ICON_AT: [f32; 2] = [25281.0, 15707.0];
/// The name the horseshoe is kept under in the map's cache (`store`'s "icons").
pub const ICON_FILE: &str = "horseshoe.png";

/// The card's colours: its title, its lines, a rare creature, the drops.
pub const TITLE: [f32; 4] = [1.0, 0.86, 0.45, 1.0];
pub const TEXT: [f32; 4] = [0.95, 0.93, 0.86, 1.0];
pub const RARE: [f32; 4] = [1.0, 0.62, 0.35, 1.0];
pub const DROPS: [f32; 4] = [0.72, 0.9, 1.0, 1.0];
/// The XP badge: its fill and its letters.
pub const BADGE: [f32; 4] = [0.6, 0.07, 0.07, 1.0];
pub const BADGE_TEXT: [f32; 4] = [1.0, 0.86, 0.3, 1.0];

impl Marker {
    /// Where it is, (x, z).
    pub fn at(self) -> [f32; 2] {
        match self {
            Marker::Camp(i) => CAMPS[i].at,
            Marker::Coach(i) => COACHES[i].at,
        }
    }

    pub fn world(self) -> i32 {
        match self {
            Marker::Camp(i) => CAMPS[i].world,
            Marker::Coach(i) => COACHES[i].world,
        }
    }

    /// The card: its lines and their colours, the title first (wrapped when
    /// drawn, `wrap`).
    pub fn card(self) -> Vec<(String, [f32; 4])> {
        let list = |v: &[&str]| v.join(", ");
        let mut out = Vec::new();
        match self {
            Marker::Camp(i) => {
                let c = &CAMPS[i];
                out.push((c.name.to_string(), TITLE));
                // where it is, when that says more than its name ("Undead":
                // Northern Ro's)
                let words = |s: &str| -> Vec<String> {
                    s.to_lowercase().split(|ch: char| !ch.is_alphanumeric() && ch != '\'').filter(|w| !w.is_empty() && !["the", "and", "of"].contains(w)).map(str::to_string).collect()
                };
                let named = words(c.name);
                if words(c.place).iter().any(|w| !named.contains(w)) {
                    let mut place = c.place.chars();
                    let first = place.next().map(|ch| ch.to_uppercase().collect::<String>()).unwrap_or_default();
                    out.push((first + place.as_str(), TEXT));
                }
                out.push((
                    match c.levels {
                        (lo, hi) if lo == hi => format!("XP camp, level {lo}"),
                        (lo, hi) => format!("XP camp, levels {lo}-{hi}"),
                    },
                    TEXT,
                ));
                if !c.note.is_empty() {
                    out.push((c.note.to_string(), TEXT));
                }
                if !c.rare.is_empty() {
                    out.push((format!("Rare: {}", list(c.rare)), RARE));
                }
                if !c.own.is_empty() {
                    out.push((format!("Their drops: {}", list(c.own)), RARE));
                }
                if !c.drops.is_empty() {
                    out.push((format!("Drops ({} table): {}", c.table, list(c.drops)), DROPS));
                }
            }
            Marker::Coach(i) => {
                let c = &COACHES[i];
                if c.town.is_empty() {
                    out.push((c.coachman.to_string(), TITLE));
                    out.push(("A coach on no route of the coach map".to_string(), TEXT));
                } else {
                    out.push((format!("Coach: {}", c.town), TITLE));
                    out.push((c.coachman.to_string(), TEXT));
                    out.push((format!("To {}", list(c.to)), DROPS));
                }
            }
        }
        out
    }
}

/// Every marker of world `world`: the coaches, then the camps (drawn in that
/// order, so a camp's badge is over a coach's horseshoe).
pub fn of_world(world: i32) -> impl Iterator<Item = Marker> {
    let coaches = (0..COACHES.len()).filter(move |&i| COACHES[i].world == world).map(Marker::Coach);
    let camps = (0..CAMPS.len()).filter(move |&i| CAMPS[i].world == world).map(Marker::Camp);
    coaches.chain(camps)
}

/// The marker under window pixel `s` of view `v` in a `size` window, of world
/// `world`: the nearest within reach of its middle (`view::marker_reach`, a
/// finger's miss more), none when none is.
pub fn under(world: i32, v: View, size: (u32, u32), s: [f32; 2], finger: bool) -> Option<Marker> {
    let reach = view::marker_reach(size, finger);
    of_world(world)
        .map(|m| {
            let p = v.to_window(size, m.at());
            (m, (p[0] - s[0]).hypot(p[1] - s[1]))
        })
        .filter(|&(_, d)| d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(m, _)| m)
}

/// `text` broken at its spaces into lines no wider than `most` (the raster's
/// widths, frame pixels); a word wider than that alone on its line.
pub fn wrap(r: &dyn vi::ui::Raster, text: &str, most: i32) -> Vec<String> {
    let width = |s: &str| r.string_width(&vi::ui::wide(s));
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && width(&next) > most {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The stable's horseshoe: wanted, asked of the cache, read off the disc, on
/// the GPU (`Map::find_icon`).
#[derive(Default)]
pub struct Icon {
    /// the cache it is kept in (`store::open("icons")`), once asked
    pub store: Option<Box<dyn super::store::Store>>,
    /// the cache answered (with it or without)
    pub answered: bool,
    /// its zone asked of the map's builder (`Builder::peek`), once
    pub peeked: bool,
    /// RGBA, top row first, 64 across
    pub image: Option<(u32, u32, Vec<u8>)>,
    /// its texture on the map's layer (`Ui::add_texture`), made again with
    /// the layer
    pub tex: Option<usize>,
}

impl Icon {
    /// The horseshoe taken off the disc (a library's file holding it, or a
    /// zone the builder read), kept in the cache for the next time.
    pub fn take(&mut self, im: &esf::Image, how: &str) {
        if self.image.is_some() {
            return;
        }
        self.image = Some((im.width, im.height, im.rgba.clone()));
        crate::say(&format!("map: the stable sign's horseshoe ({}x{}) {how}", im.width, im.height));
        if let (Some(s), Some(png)) = (self.store.as_mut(), encode_png(im.width, im.height, &im.rgba)) {
            s.put(ICON_FILE, png);
        }
    }

    /// The cache's answer for `ICON_FILE`: the picture, when it is a PNG of
    /// the right shape.
    pub fn cached(&mut self, bytes: Option<Vec<u8>>) {
        self.answered = true;
        let Some(b) = bytes else { return };
        match decode_png(&b) {
            Some(im) if self.image.is_none() => {
                crate::say(&format!("map: the stable sign's horseshoe from the cache ({}x{})", im.0, im.1));
                self.image = Some(im);
            }
            Some(_) => {}
            None => crate::say("map: the horseshoe kept in the cache would not read; read off the disc again"),
        }
    }
}

// ---- drawing (frame points, over the map's picture) ----

use render::ui::{Quad, Ui};
use vi::ui::Raster as _;

/// Outlines, the stand-in horseshoe's board and its iron.
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
const BOARD: [f32; 4] = [0.42, 0.42, 0.4, 1.0];
const BRONZE: [f32; 4] = [0.86, 0.52, 0.2, 1.0];
/// The card's backing (under the game's panel art, which a library without
/// SUPPORT's atlas, `mapshot`'s, lacks) and its widest line, frame pixels.
const CARD_BACK: [f32; 4] = [0.05, 0.06, 0.08, 0.9];
const CARD_WIDTH: i32 = 250;
/// How far in the map's camp names show: world units a window pixel.
pub const NAMES_FROM: f32 = 3.5;

fn fill(ui: &mut Ui, white: usize, pos: [[f32; 2]; 4], color: [f32; 4]) {
    ui.push(Quad { tex: white, blend: 3, pos, uv: [[0.5, 0.5]; 4], color, clip: None });
}

fn rect(ui: &mut Ui, white: usize, r: [f32; 4], color: [f32; 4]) {
    fill(ui, white, [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]], color);
}

/// A coach at frame point `p`, `r` frame pixels from its middle to an edge
/// and `o` its outline: the stable sign off the disc (`icon`, the layer's
/// texture), else ours, a bronze horseshoe on a grey board, open at the foot
/// as the sign's is.
pub fn draw_horseshoe(ui: &mut Ui, white: usize, icon: Option<usize>, p: [f32; 2], r: f32, o: f32) {
    rect(ui, white, [p[0] - r - o, p[1] - r - o, p[0] + r + o, p[1] + r + o], OUTLINE);
    if let Some(t) = icon {
        let pos = [[p[0] - r, p[1] - r], [p[0] + r, p[1] - r], [p[0] + r, p[1] + r], [p[0] - r, p[1] + r]];
        ui.push(Quad { tex: t, blend: 3, pos, uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color: [1.0; 4], clip: None });
        return;
    }
    rect(ui, white, [p[0] - r, p[1] - r, p[0] + r, p[1] + r], BOARD);
    for (half, color) in [(0.2 * r, OUTLINE), (0.12 * r, BRONZE)] {
        for (a, b) in horseshoe_arc() {
            let at = |t: f32, rr: f32| [p[0] + rr * t.cos(), p[1] + rr * t.sin()];
            let (inner, outer) = (0.55 * r - half, 0.55 * r + half);
            fill(ui, white, [at(a, inner), at(a, outer), at(b, outer), at(b, inner)], color);
        }
    }
}

/// The stand-in horseshoe's arc in pieces (radians, the window's y down):
/// from the foot's left round over the top to its right, 270 degrees.
fn horseshoe_arc() -> impl Iterator<Item = (f32, f32)> {
    let (from, to, n) = (135f32.to_radians(), 405f32.to_radians(), 9);
    (0..n).map(move |i| (from + (to - from) * i as f32 / n as f32, from + (to - from) * (i + 1) as f32 / n as f32))
}

/// The camps' badges at their frame points: "XP" in gold on red, outlined,
/// the size of the map's names (the font `font`: its height and index on
/// the layer), and each camp's name under it once the map is in close
/// (`scale`, world units a window pixel, below `NAMES_FROM`).
pub fn draw_badges(ui: &mut Ui, assets: &crate::ui::UiAssets, vui: &vi::ui::Vui, white: usize, font: Option<(f32, usize)>, camps: &[(Marker, [f32; 2])], scale: f32) {
    let label = vi::ui::wide("XP");
    let (h, tw) = font.map_or((14.0, 20.0), |(h, fo)| (h, ui.fonts[fo].string_width(&label) as f32));
    let (hw, hh) = (tw / 2.0 + 4.0, h / 2.0);
    for &(_, p) in camps {
        rect(ui, white, [p[0] - hw - 1.5, p[1] - hh - 1.5, p[0] + hw + 1.5, p[1] + hh + 1.5], OUTLINE);
        rect(ui, white, [p[0] - hw, p[1] - hh, p[0] + hw, p[1] + hh], BADGE);
    }
    let mut r = assets.raster(ui);
    for &(m, p) in camps {
        vui.draw_centered_string_ex(&mut r, &label, p[0].round() as i32, p[1].round() as i32, BADGE_TEXT, 0, false);
        if let (true, Marker::Camp(i)) = (scale < NAMES_FROM, m) {
            vui.draw_centered_string_ex(&mut r, &vi::ui::wide(CAMPS[i].name), p[0].round() as i32, (p[1] + hh + h * 0.75).round() as i32, super::chrome::NAME, 0, true);
        }
    }
}

/// Marker `m`'s card beside its frame point `p`, kept inside the window's
/// frame corners `win`: its lines wrapped to `CARD_WIDTH` on a dark backing
/// under the game's panel (VIWndGoto's, as the map's chrome has it).
pub fn draw_card(ui: &mut Ui, assets: &crate::ui::UiAssets, vui: &vi::ui::Vui, white: usize, m: Marker, p: [f32; 2], win: [[f32; 2]; 2]) {
    let (lines, line, width) = {
        let r = assets.raster(ui);
        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
        for (text, color) in m.card() {
            lines.extend(wrap(&r, &text, CARD_WIDTH).into_iter().map(|l| (l, color)));
        }
        let width = lines.iter().map(|(l, _)| r.string_width(&vi::ui::wide(l))).max().unwrap_or(0);
        (lines, r.point_size() + 3, width)
    };
    let (pad, gap, edge) = (8.0, 16.0, 8.0);
    let (w, h) = (width as f32 + pad * 2.0, lines.len() as f32 * line as f32 + pad * 2.0 - 3.0);
    // right of the marker, else left of it; down from its middle, held inside
    let mut x = p[0] + gap;
    if x + w > win[1][0] - edge {
        x = p[0] - gap - w;
    }
    let x = x.clamp(win[0][0] + edge, (win[1][0] - edge - w).max(win[0][0] + edge));
    let y = (p[1] - h / 2.0).clamp(win[0][1] + edge, (win[1][1] - edge - h).max(win[0][1] + edge));
    rect(ui, white, [x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0], OUTLINE);
    rect(ui, white, [x, y, x + w, y + h], CARD_BACK);
    let mut r = assets.raster(ui);
    vui.stretch_blit_ex(&mut r, 61, [x.round() as i32, y.round() as i32], [w.round() as i32, h.round() as i32], 3, 0.75);
    for (k, (text, color)) in lines.iter().enumerate() {
        let at = [(x + pad).round() as i32, (y + pad + (k as i32 * line) as f32).round() as i32];
        vui.draw_string_ex(&mut r, &vi::ui::wide(text), at[0], at[1], *color, true);
    }
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().ok()?.write_image_data(rgba).ok()?;
    }
    Some(out)
}

fn decode_png(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut r = png::Decoder::new(std::io::Cursor::new(b)).read_info().ok()?;
    let mut px = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut px).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight || info.width == 0 || info.width > 256 || info.height == 0 || info.height > 256 {
        return None;
    }
    px.truncate(info.buffer_size());
    Some((info.width, info.height, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every marker is in its world and on its ground: Tunaria's inside its
    /// zones' grid (2,000 x 34,000), each camp's levels in order, each coach
    /// with a town but Rathe's Coachman Dilga, and every coach's town on
    /// another's route (the network is whole). The control: a coach of
    /// Tunaria put at Odus's places would be outside the grid.
    #[test]
    fn the_markers_lie_in_their_worlds() {
        let grid = |p: [f32; 2]| (2000.0..28000.0).contains(&p[0]) && (0.0..34000.0).contains(&p[1]);
        for c in CAMPS.iter() {
            assert!([0, 2].contains(&c.world), "{}", c.name);
            assert!(c.levels.0 <= c.levels.1 && c.levels.1 <= 62, "{}: {:?}", c.name, c.levels);
            if c.world == 0 {
                assert!(grid(c.at), "{} at {:?}", c.name, c.at);
            }
            assert!(!c.note.is_empty(), "{}: the guide's camps carry a note", c.name);
        }
        let towns: Vec<&str> = COACHES.iter().map(|c| c.town).filter(|t| !t.is_empty()).collect();
        for c in COACHES.iter() {
            match c.world {
                0 => {
                    assert!(grid(c.at), "{} at {:?}", c.coachman, c.at);
                    assert!(!c.town.is_empty() && !c.to.is_empty(), "{}", c.coachman);
                }
                1 => assert_eq!((c.coachman, c.town), ("Coachman Dilga", ""), "Rathe's one coach"),
                w => panic!("{} in world {w}", c.coachman),
            }
            for t in c.to {
                assert!(towns.contains(t) || ["Kerplunk Outpost", "Oggok"].contains(t), "{} runs to {t}, a town with no coachman here", c.coachman);
            }
        }
        let odus = CAMPS.iter().find(|c| c.world == 2).unwrap().at;
        assert!(!grid([odus[0] - 20000.0, odus[1] + 40000.0]), "the control: off the grid");
        assert_eq!(of_world(0).filter(|m| matches!(m, Marker::Coach(_))).count(), 30);
        assert_eq!(of_world(1).collect::<Vec<_>>().len(), 1, "Dilga");
        assert!(of_world(2).all(|m| matches!(m, Marker::Camp(_))), "Odus has camps, no coach here");
    }

    /// A camp's card: its name, its levels, the guide's note, its rare
    /// creatures and drops when it has them; a coach's: its town, coachman
    /// and route. The control: a camp with no rare creature has no Rare line.
    #[test]
    fn a_card_says_what_its_marker_is() {
        let dfc = CAMPS.iter().position(|c| c.name.starts_with("Deathfist Citadel")).unwrap();
        let card = Marker::Camp(dfc).card();
        assert_eq!(card[0].0, "Deathfist Citadel (DFC)");
        assert_eq!(card[1].0, "XP camp, levels 29-34");
        assert!(card[2].0.contains("best XP camps in the low 30s"), "{:?}", card[2]);
        assert!(card.iter().any(|l| l.0.starts_with("Drops (Desert table): ")), "{card:?}");
        assert!(!card.iter().any(|l| l.0.starts_with("Rare")), "the control: no rare creature at the DFC");
        let undead = CAMPS.iter().position(|c| c.name == "Undead").unwrap();
        assert_eq!(Marker::Camp(undead).card()[1].0, "Northern Ro's undead", "the place, its name saying too little");
        let vox = CAMPS.iter().position(|c| c.rare.iter().any(|r| r.starts_with("Lady Vox"))).unwrap();
        assert!(Marker::Camp(vox).card().iter().any(|l| l.0 == "Rare: Lady Vox (raid)" && l.1 == RARE));
        let ronks = COACHES.iter().position(|c| c.coachman == "Coachman Ronks").unwrap();
        let card = Marker::Coach(ronks).card();
        assert_eq!(card[0].0, "Coach: Freeport");
        assert_eq!(card[1].0, "Coachman Ronks");
        assert_eq!(card[2].0, "To Neriak, Bobble-by-Water, Highpass Hold, Muniel's Tea Garden");
    }

    /// A long line is broken at its spaces into lines that fit; one that fits
    /// stays whole. The control: the raster's widths decide (twice as wide a
    /// font, twice as many lines).
    #[test]
    fn a_card_line_wraps_to_fit() {
        use crate::pointer::testing::NullRaster;
        let note = CAMPS.iter().find(|c| c.name.starts_with("Deathfist Citadel")).unwrap().note;
        let r = NullRaster;
        let w = |s: &str| r.string_width(&vi::ui::wide(s));
        let lines = wrap(&r, note, 200);
        assert!(lines.len() > 1 && lines.iter().all(|l| w(l) <= 200), "{lines:?}");
        assert_eq!(lines.join(" "), note.split_whitespace().collect::<Vec<_>>().join(" "));
        assert_eq!(wrap(&r, "Coachman Ronks", 200), vec!["Coachman Ronks"]);
        let narrow = wrap(&r, note, 100);
        assert!(narrow.len() > lines.len(), "the control: narrower, more lines");
        use vi::ui::Raster;
    }

    /// The horseshoe kept in the cache comes back as it was taken; a file
    /// that is no PNG is passed over (read off the disc again).
    #[test]
    fn the_horseshoe_keeps_in_the_cache() {
        let im = esf::Image { width: 2, height: 2, rgba: (0..16).collect(), alpha: false };
        let png = encode_png(im.width, im.height, &im.rgba).unwrap();
        let mut i = Icon::default();
        i.cached(Some(png));
        assert_eq!(i.image, Some((2, 2, (0..16).collect())));
        let mut bad = Icon::default();
        bad.cached(Some(b"not a png".to_vec()));
        assert!(bad.answered && bad.image.is_none());
    }
}
