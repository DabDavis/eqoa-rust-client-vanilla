//! The title and STATION's pages in a window wider than 4:3 (the client's
//! own: the game drew them for a 4:3 picture). Each draws its backdrop as one
//! 640x448 picture in two halves (SETUP's `splash00`/`splash10`, STATION's
//! `stbk00`/`stbk10` and `2stbk00`/`2stbk10`, each half the top-left 320x448
//! of a 512x512 surface), banded the same way: stone, a jewelled bar, the
//! painting, a bar, stone. Past each side of the frame the half carries its
//! picture on, mirrored at the frame's edge, except in the painting's rows,
//! which take the picture's own stone (the top band's, every other copy
//! upside down): the stone and the bars run the window's width, and the
//! painting stands in the wall between them. Mirrored, the painting itself
//! was a mirror image at each edge (a second city, two pyramids).
//!
//! Each side also covers the picture's own outermost column (`stbk00`'s is
//! black in the stone below the bars: a line no TV showed, its overscan, nor
//! a 4:3 window beside black), and reads no further in than `STRIP` columns:
//! a wider window takes the strip back and forth again, so the logos in the
//! pictures' middles never come out at the sides.

use crate::ui::Layout;
use std::sync::OnceLock;

/// A backdrop picture's rows (frame pixels): the top stone ends at `stone`,
/// where the top bar begins; the painting is `painting`, between the bars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picture {
    pub stone: i32,
    pub painting: [i32; 2],
}

/// SETUP's title (SETUP.CSF): stone to row 75, its bars' gold rows 77-78
/// and 97-98 (an orange edge at 76), 337-338 and 377-378.
pub const SETUP: Picture = Picture { stone: 76, painting: [99, 337] };

/// STATION's pages (STATUI.CSF, both pictures), their bars' gold rows 52 and
/// 54, 72 and 74, 377 and 379, 399 and 401.
pub const STATION: Picture = Picture { stone: 52, painting: [75, 377] };

/// The columns in from a half's outer edge that its side is made of.
pub const STRIP: i32 = 160;

/// Which side of the frame a half is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Half {
    Left,
    Right,
}

/// The halves, by DictID (STATION's pages name theirs by the whole path).
fn halves() -> &'static [(u32, Half, Picture)] {
    static H: OnceLock<Vec<(u32, Half, Picture)>> = OnceLock::new();
    H.get_or_init(|| {
        let ui = |n: &str| format!(r"P:\studio\eqps2\Game_Assets\UI\{n}");
        [
            ("splash00.tga".to_string(), Half::Left, SETUP),
            ("splash10.tga".to_string(), Half::Right, SETUP),
            (ui("stbk00.tga"), Half::Left, STATION),
            (ui("stbk10.tga"), Half::Right, STATION),
            (ui("2stbk00.tga"), Half::Left, STATION),
            (ui("2stbk10.tga"), Half::Right, STATION),
        ]
        .into_iter()
        .map(|(n, h, p)| (vi::hash::hash_resource_id(&n), h, p))
        .collect()
    })
}

/// Backdrop half `id`, just drawn at `at` in blend `mode`: its side of the
/// window past the frame, on a target wider than 4:3. Nothing for any other
/// surface, or a half anywhere but its own place.
pub fn half(r: &mut dyn Layout, id: u32, at: [i32; 2], mode: i32) {
    let side = r.side();
    if side <= 0 {
        return;
    }
    let Some(&(_, half, pic)) = halves().iter().find(|h| h.0 == id) else { return };
    // from `edge`, a column inside the frame (over the picture's outermost),
    // `dir` away from the frame; `inner`: the texel edge there, the
    // outermost column's inner side
    let (edge, dir, inner) = match (half, at) {
        (Half::Left, [0, 0]) => (1.0, -1.0, 1.0),
        (Half::Right, [320, 0]) => (639.0, 1.0, 319.0),
        _ => return,
    };
    // that column, the whole frame pixels past the side and the target's
    // fraction of one, in strips of `STRIP` columns, every other one back
    // the way it came
    let w = side + 2;
    let mut d = 0;
    let mut k = 0;
    while d < w {
        let sw = STRIP.min(w - d);
        // texel offsets in from `inner` at the strip's near and far edges
        let (t0, t1) = if k % 2 == 0 { (0, sw) } else { (STRIP, STRIP - sw) };
        let x = [edge + dir * d as f32, edge + dir * (d + sw) as f32];
        let t = [inner - dir * t0 as f32, inner - dir * t1 as f32];
        let piece = |r: &mut dyn Layout, y: [i32; 2], v: [i32; 2]| {
            let (y, v) = (y.map(|a| a as f32), v.map(|a| a as f32));
            r.beyond(id, mode, [[x[0], y[0]], [x[1], y[0]], [x[1], y[1]], [x[0], y[1]]], [[t[0], v[0]], [t[1], v[0]], [t[1], v[1]], [t[0], v[1]]]);
        };
        // the stone and the bar above the painting, the bar and the stone below
        piece(r, [0, pic.painting[0]], [0, pic.painting[0]]);
        piece(r, [pic.painting[1], 448], [pic.painting[1], 448]);
        // the painting's rows: the top stone, stacked, every other copy upside
        // down so that each meets the next on the same row
        let (mut y, mut flip) = (pic.painting[0], false);
        while y < pic.painting[1] {
            let h = pic.stone.min(pic.painting[1] - y);
            piece(r, [y, y + h], if flip { [pic.stone, pic.stone - h] } else { [0, h] });
            y += h;
            flip = !flip;
        }
        d += sw;
        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::ui::Raster;

    /// One `beyond` quad: surface, blend mode, corners, texels.
    type Piece = (u32, i32, [[f32; 2]; 4], [[f32; 2]; 4]);

    /// A target `.0` frame pixels past each side that records `beyond`'s
    /// quads.
    struct Rec(i32, Vec<Piece>);

    impl Layout for Rec {
        fn side(&self) -> i32 {
            self.0
        }
        fn beyond(&mut self, id: u32, mode: i32, corners: [[f32; 2]; 4], texels: [[f32; 2]; 4]) {
            self.1.push((id, mode, corners, texels));
        }
    }

    impl Raster for Rec {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    fn id(n: &str) -> u32 {
        vi::hash::hash_resource_id(n)
    }

    /// The rows each quad covers and the texel rows it shows, top to bottom.
    fn rows(q: &[Piece]) -> Vec<([f32; 2], [f32; 2])> {
        q.iter().map(|(_, _, c, t)| ([c[0][1], c[2][1]], [t[0][1], t[2][1]])).collect()
    }

    /// The columns each quad covers and the texel columns it shows.
    fn cols(q: &[Piece]) -> Vec<([f32; 2], [f32; 2])> {
        q.iter().map(|(_, _, c, t)| ([c[0][0], c[1][0]], [t[0][0], t[1][0]])).collect()
    }

    /// At 16:9 (106 whole frame pixels a side) the title's left half goes
    /// from frame x 1 (over the picture's column 0) to 107 past the left
    /// edge, mirrored at its column 0's inner side (texel 1 at x 1, 109 at
    /// the outside), its stone and top bar down to row 99 and its bar and
    /// stone from 337; the painting's rows 99-337 are the stone's 76 rows,
    /// alternately upside down, the last cut to fit. The right half mirrors
    /// from frame 639 (texel 319) out to 747 (211). Controls: a 4:3 target
    /// draws nothing, nor a surface that is no half, nor a half at the
    /// other's place.
    #[test]
    fn the_sides_carry_the_picture_on_and_stone_the_painting() {
        let mut r = Rec(106, Vec::new());
        half(&mut r, id("splash00.tga"), [0, 0], 0);
        let q = std::mem::take(&mut r.1);
        assert!(q.iter().all(|(i, m, ..)| *i == id("splash00.tga") && *m == 0));
        assert!(cols(&q).iter().all(|c| *c == ([1.0, -107.0], [1.0, 109.0])), "{:?}", cols(&q));
        assert_eq!(
            rows(&q),
            vec![
                ([0.0, 99.0], [0.0, 99.0]),
                ([337.0, 448.0], [337.0, 448.0]),
                ([99.0, 175.0], [0.0, 76.0]),
                ([175.0, 251.0], [76.0, 0.0]),
                ([251.0, 327.0], [0.0, 76.0]),
                ([327.0, 337.0], [76.0, 66.0]),
            ]
        );
        half(&mut r, id("splash10.tga"), [320, 0], 0);
        assert!(cols(&r.1).iter().all(|c| *c == ([639.0, 747.0], [319.0, 211.0])), "{:?}", cols(&r.1));
        assert_eq!(r.1.len(), 6);
        // STATION's pictures: its stone's 52 rows over the painting's 75-377
        let mut s = Rec(106, Vec::new());
        half(&mut s, id(r"P:\studio\eqps2\Game_Assets\UI\2stbk00.tga"), [0, 0], 3);
        let covered: i32 = rows(&s.1).iter().skip(2).map(|(y, _)| (y[1] - y[0]) as i32).sum();
        assert_eq!((rows(&s.1)[0], covered), (([0.0, 75.0], [0.0, 75.0]), 377 - 75));
        // the controls
        for (side, name, at) in [(0, "splash00.tga", [0, 0]), (106, "splash01.tga", [0, 0]), (106, "splash00.tga", [320, 0])] {
            let mut n = Rec(side, Vec::new());
            half(&mut n, id(name), at, 0);
            assert!(n.1.is_empty(), "{side} {name} {at:?}");
        }
    }

    /// At 21:9 (202 a side) the side is two strips: the first `STRIP`
    /// columns mirrored, then back the way they came (161 at the strip's
    /// meeting, 117 at the window's edge), so nothing past column 161 of the
    /// half is read; the right side the same from column 319. The control:
    /// one strip at 16:9.
    #[test]
    fn a_wider_window_takes_the_strip_back_and_forth() {
        let mut r = Rec(202, Vec::new());
        half(&mut r, id("splash00.tga"), [0, 0], 0);
        let mut c = cols(&r.1);
        c.dedup();
        assert_eq!(c, vec![([1.0, -159.0], [1.0, 161.0]), ([-159.0, -203.0], [161.0, 117.0])]);
        let mut r = Rec(202, Vec::new());
        half(&mut r, id("splash10.tga"), [320, 0], 0);
        let mut c = cols(&r.1);
        c.dedup();
        assert_eq!(c, vec![([639.0, 799.0], [319.0, 159.0]), ([799.0, 843.0], [159.0, 203.0])]);
        let mut one = Rec(106, Vec::new());
        half(&mut one, id("splash00.tga"), [0, 0], 0);
        let mut c = cols(&one.1);
        c.dedup();
        assert_eq!(c.len(), 1);
    }

    /// The cuts are the disc's pictures (`run/eqoa.iso`, skipped without
    /// it): over the columns a side reads (1-160 of a left half, 159-318 of a
    /// right one) the rows above `stone` hold no bar's orange and row `stone`
    /// is the bar's; in the 16 outer columns the last gold row above the
    /// middle is the row before the painting and the first below is its end.
    /// The control: the row after the stone is orange, so a cut one row lower
    /// tiles the bar's edge into the stone (the lines seen at 77).
    #[test]
    fn the_cuts_are_the_pictures() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut d) = iso::Iso::open(&iso) else {
            eprintln!("SKIP: no disc at {}", iso.display());
            return;
        };
        let ui = |n: &str| format!(r"P:\studio\eqps2\Game_Assets\UI\{n}");
        for (file, name, half, want) in [
            ("DATA2/SETUP.CSF", "splash00.tga".to_string(), Half::Left, SETUP),
            ("DATA2/SETUP.CSF", "splash10.tga".to_string(), Half::Right, SETUP),
            ("DATA2/STATUI.CSF", ui("stbk00.tga"), Half::Left, STATION),
            ("DATA2/STATUI.CSF", ui("stbk10.tga"), Half::Right, STATION),
            ("DATA2/STATUI.CSF", ui("2stbk00.tga"), Half::Left, STATION),
            ("DATA2/STATUI.CSF", ui("2stbk10.tga"), Half::Right, STATION),
        ] {
            let c = esf::Container::open(d.read(file).unwrap()).unwrap();
            let s = c.surface(id(&name) as i32).unwrap_or_else(|| panic!("{name} not in {file}"));
            let img = s.decode(&c.data).unwrap();
            assert_eq!((img.width, img.height), (512, 512), "{name}");
            let px = |x: u32, y: u32| -> [u8; 3] {
                let i = ((y * img.width + x) * 4) as usize;
                [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]]
            };
            let orange = |p: [u8; 3]| p[0] > 140 && p[0] as i32 > p[2] as i32 + 60;
            let gold = |p: [u8; 3]| p[0] > 170 && p[1] > 110 && p[2] < 90;
            let strip: Vec<u32> = if half == Half::Left { (1..1 + STRIP as u32).collect() } else { (319 - STRIP as u32..319).collect() };
            let outer: Vec<u32> = if half == Half::Left { (0..16).collect() } else { (304..320).collect() };
            let oranges = |y: u32| strip.iter().filter(|&&x| orange(px(x, y))).count();
            let stone = (0..448).find(|&y| oranges(y) > strip.len() / 4).unwrap();
            assert!((0..stone).all(|y| oranges(y) == 0), "{name}: orange in the stone");
            let golds: Vec<u32> = (0..448).filter(|&y| outer.iter().filter(|&&x| gold(px(x, y))).count() > 10).collect();
            let above = golds.iter().copied().filter(|&y| y < 224).max().unwrap();
            let below = golds.iter().copied().find(|&y| y > 224).unwrap();
            assert_eq!(Picture { stone: stone as i32, painting: [above as i32 + 1, below as i32] }, want, "{name}");
            // the control
            assert!(oranges(want.stone as u32) > 0, "{name}: the row after the stone is the bar's");
        }
    }
}
