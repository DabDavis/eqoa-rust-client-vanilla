//! The Plane of Sky's isles by name (the server owner's of 2026-11-14: "for the location of plane of sky islands,
//! for example dust is on night 1 lets make that the Location like Plane of Sky (Night Chain 1)"): where the map's
//! title and the start menu's Location name the place, world 4's is "Plane of Sky (Night Chain 1)", "Plane of Sky
//! (Main Isle)", "Plane of Sky (Isle of Wonder)".
//!
//! The table (`skyisles_gen.rs`) is go-eqoa-server's `tools/sky_isles.py`'s, the same as the server's
//! (`internal/skyisles`): each isle's outline seen from above, out of world 4's terrain, named from the chains'
//! essences and the bosses standing on it. Isles float apart, so a spot is the isle whose outline holds it; where
//! two overlap from above (snow 1's mountain over snow 2's igloo village) the height divides them; off every
//! outline, the nearest within `NEAR` (a fall, a jump, a bridge), else none.

#[path = "skyisles_gen.rs"]
mod table;

use table::{ISLES, SPLIT, SPLITS};

/// The Plane of Sky's world index (`gWorldInfo`'s PLANESKY).
pub const WORLD: i32 = 4;

/// The world's name as the place is shown (the server's lines say it too).
pub const NAME: &str = "Plane of Sky";

/// How far off an isle's outline a spot still names it.
const NEAR: f32 = 100.0;

/// The isle at (x, y, z) in the Plane of Sky ("Night Chain 1", "Main Isle"), none off every isle.
pub fn isle(x: f32, y: f32, z: f32) -> Option<&'static str> {
    if let Some(s) = SPLITS.iter().find(|s| x >= s.0 && x < s.0 + SPLIT && z >= s.1 && z < s.1 + SPLIT) {
        return Some(if y >= s.2 { s.3 } else { s.4 });
    }
    let mut best: Option<(&str, f32)> = None;
    for &(name, lo, hi, polys) in ISLES {
        if polys.iter().any(|p| inside(p, x, z)) {
            let off = (lo - y).max(y - hi).max(0.0);
            if best.is_none_or(|b| off < b.1) {
                best = Some((name, off));
            }
        }
    }
    if let Some((name, _)) = best {
        return Some(name);
    }
    let mut near: Option<(&str, f32)> = None;
    for &(name, _, _, polys) in ISLES {
        for p in polys {
            let d = edge_dist(p, x, z);
            if d < near.map_or(NEAR, |n| n.1) {
                near = Some((name, d));
            }
        }
    }
    near.map(|n| n.0)
}

/// "Plane of Sky (Night Chain 1)" for a spot of world 4 on an isle, none anywhere else.
pub fn place(world: i32, x: f32, y: f32, z: f32) -> Option<String> {
    (world == WORLD).then(|| isle(x, y, z)).flatten().map(|i| format!("{NAME} ({i})"))
}

/// Each isle's name and the spot its name goes on the map (x, z), the biggest isle first: the point of its
/// largest outline farthest from that outline's edges (its pole of inaccessibility, to 8 units), among the points
/// no other isle's outline holds (snow 2's name off snow 1's mountain, which stands over it), any inside point when
/// there is none such.
pub fn labels() -> &'static [(&'static str, [f32; 2])] {
    static LABELS: std::sync::OnceLock<Vec<(&'static str, [f32; 2])>> = std::sync::OnceLock::new();
    LABELS.get_or_init(|| {
        let boxes: Vec<Vec<[f32; 4]>> = ISLES.iter().map(|i| i.3.iter().map(|p| bbox(p)).collect()).collect();
        let mut out: Vec<(&str, [f32; 2], f32)> = ISLES
            .iter()
            .enumerate()
            .filter_map(|(k, &(name, _, _, polys))| {
                let p = polys.iter().max_by(|a, b| area(a).total_cmp(&area(b)))?;
                let others = |x: f32, z: f32| {
                    ISLES.iter().enumerate().any(|(j, o)| j != k && o.3.iter().zip(&boxes[j]).any(|(q, b)| x >= b[0] && x <= b[2] && z >= b[1] && z <= b[3] && inside(q, x, z)))
                };
                Some((name, pole(p, &others)?, area(p)))
            })
            .collect();
        out.sort_by(|a, b| b.2.total_cmp(&a.2));
        out.into_iter().map(|(n, at, _)| (n, at)).collect()
    })
}

/// Outline `p`'s bounding box (x0, z0, x1, z1): the cheap test before `inside`.
fn bbox(p: &[f32]) -> [f32; 4] {
    p.chunks(2).fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, c| [b[0].min(c[0]), b[1].min(c[1]), b[2].max(c[0]), b[3].max(c[1])])
}

/// Outline `p`'s area.
fn area(p: &[f32]) -> f32 {
    let n = p.len() / 2;
    (0..n).map(|i| { let j = (i + 1) % n; p[2 * i] * p[2 * j + 1] - p[2 * j] * p[2 * i + 1] }).sum::<f32>().abs() / 2.0
}

/// The point inside outline `p` farthest from its edges on an 8-unit grid, preferring points `taken` refuses.
fn pole(p: &[f32], taken: &dyn Fn(f32, f32) -> bool) -> Option<[f32; 2]> {
    let (xs, zs): (Vec<f32>, Vec<f32>) = p.chunks(2).map(|c| (c[0], c[1])).unzip();
    let (x0, x1) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
    let (z0, z1) = (zs.iter().copied().fold(f32::MAX, f32::min), zs.iter().copied().fold(f32::MIN, f32::max));
    let mut best: Option<(bool, f32, [f32; 2])> = None;
    let mut z = z0 + 4.0;
    while z < z1 {
        let mut x = x0 + 4.0;
        while x < x1 {
            if inside(p, x, z) {
                let c = (!taken(x, z), edge_dist(p, x, z), [x, z]);
                if best.is_none_or(|b| (c.0, c.1) > (b.0, b.1)) {
                    best = Some(c);
                }
            }
            x += 8.0;
        }
        z += 8.0;
    }
    best.map(|b| b.2)
}

/// Where the map writes names that are `rects` (frame x0, y0, x1, y1 round each name's middle, in the order
/// given, the most wanted first): each at its own spot, else just above or below it, else left out, so no two
/// overlap.
pub fn place_labels(rects: &[[f32; 4]]) -> Vec<Option<[f32; 4]>> {
    let mut put: Vec<[f32; 4]> = Vec::new();
    rects
        .iter()
        .map(|r| {
            let h = r[3] - r[1];
            let spot = [0.0, -h, h].into_iter().map(|d| [r[0], r[1] + d, r[2], r[3] + d]).find(|c| !put.iter().any(|q| c[0] < q[2] && q[0] < c[2] && c[1] < q[3] && q[1] < c[3]));
            if let Some(c) = spot {
                put.push(c);
            }
            spot
        })
        .collect()
}

/// The even-odd test of (x, z) against outline `p` (x, z pairs).
fn inside(p: &[f32], x: f32, z: f32) -> bool {
    let n = p.len() / 2;
    let mut c = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (ax, az, bx, bz) = (p[2 * i], p[2 * i + 1], p[2 * j], p[2 * j + 1]);
        if (az > z) != (bz > z) && x < (bx - ax) * (z - az) / (bz - az) + ax {
            c = !c;
        }
        j = i;
    }
    c
}

/// How far (x, z) is from outline `p`'s nearest edge.
fn edge_dist(p: &[f32], x: f32, z: f32) -> f32 {
    let n = p.len() / 2;
    let mut best = f32::MAX;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (ax, az, bx, bz) = (p[2 * i], p[2 * i + 1], p[2 * j], p[2 * j + 1]);
        let (dx, dz) = (bx - ax, bz - az);
        let l = dx * dx + dz * dz;
        let t = if l > 0.0 { (((x - ax) * dx + (z - az) * dz) / l).clamp(0.0, 1.0) } else { 0.0 };
        best = best.min((ax + t * dx - x).hypot(az + t * dz - z));
        j = i;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every known spot names its isle (as go-eqoa-server's `internal/skyisles` test has them): each chain's
    /// essences, the bosses the forum's walkthroughs name for each isle, the main isle's Island of Wonder portal and
    /// the Isle of Wonder's landing.
    #[test]
    fn known_spots() {
        for (what, x, y, z, want) in [
            ("the Island of Wonder portal", 4824.0, 250.0, 5497.0, "Main Isle"),
            ("Liztin Misseliania", 4685.0, 250.0, 5398.0, "Main Isle"),
            ("essence of rain (rain key)", 4482.5, 244.7, 5293.2, "Main Isle"),
            ("essence of night (key of the ancients)", 4621.4, 270.8, 5729.6, "Main Isle"),
            ("essence of sun (sun key)", 5137.3, 261.3, 5188.3, "Main Isle"),
            ("the Isle of Wonder landing", 5000.0, 333.5, 4998.0, "Isle of Wonder"),
            ("Mark of the Majestic", 4980.0, 333.0, 4985.0, "Isle of Wonder"),
            ("Construct of Storms", 5105.0, 237.0, 5745.0, "Isle of the Constructs"),
            ("essence of rain (Rain Key 1)", 4222.7, 233.0, 5714.6, "Rain Chain 1"),
            ("Artiokanis", 4234.0, 236.0, 5688.0, "Rain Chain 1"),
            ("essence of rain (Rain Key 2)", 4158.6, 180.9, 5122.5, "Rain Chain 2"),
            ("Trixie Longwings", 4169.0, 181.0, 5214.0, "Rain Chain 2"),
            ("essence of rain (Rain Key 3)", 4638.6, 119.7, 4655.2, "Rain Chain 3"),
            ("Granavik", 4318.0, -69.0, 4700.0, "Rain Chain 3"),
            ("essence of rain (Rain Key 4)", 4199.7, 80.9, 4148.3, "Rain Chain 4"),
            ("essence of rain (Rain Key 5)", 4516.5, 30.9, 4271.6, "Rain Chain 5"),
            ("Chimareon the Tyrant", 4556.0, 31.0, 4276.0, "Rain Chain 5"),
            ("essence of day (Day Key 1)", 5597.3, 235.9, 5343.1, "Day Chain 1"),
            ("brightstar princess", 5640.0, 257.0, 5427.0, "Day Chain 1"),
            ("essence of day (Day Key 2)", 5380.8, 282.6, 4755.9, "Day Chain 2"),
            ("Imerzan", 5406.0, 282.0, 4751.0, "Day Chain 2"),
            ("essence of day (Day Key 3)", 5459.6, 381.0, 4237.5, "Day Chain 3"),
            ("essence of day (Day Key 4)", 5054.4, 431.8, 4243.1, "Day Chain 4"),
            ("Madam Watrya", 5640.0, 278.0, 5120.0, "Watrya's Isle"),
            ("essence of night (Night Key 1)", 4160.7, 613.3, 6404.1, "Night Chain 1"),
            ("Ancient Jester", 4175.0, 612.0, 6216.0, "Night Chain 1"),
            ("essence of night (Night Key 2)", 4441.2, 449.6, 6436.7, "Night Chain 2"),
            ("essence of night (Night Key 3)", 4250.0, 671.5, 6806.4, "Night Chain 3"),
            ("essence of night (Night Key 4)", 4322.6, 354.0, 7754.4, "Night Chain 4"),
            ("dark baroness", 4155.0, 353.0, 7758.0, "Night Chain 4"),
            ("essence of night (Night Key 5)", 4400.0, 130.6, 7135.8, "Night Chain 5"),
            ("King Ruthra", 4499.0, 99.0, 7234.0, "Night Chain 5"),
            ("Ryini", 5668.0, 553.0, 6356.0, "Snow Chain 1"),
            ("essence of snow (Snow Key 2)", 5766.8, 238.3, 6702.1, "Snow Chain 2"),
            ("Tamis", 5550.0, 238.0, 6688.0, "Snow Chain 2"),
            ("essence of snow (Snow Key 3)", 5906.3, 177.5, 7570.5, "Snow Chain 3"),
            ("essence of snow (Snow Key 4)", 5525.1, 96.1, 7708.0, "Snow Chain 4"),
            ("essence of desolation (Desolation Key 1)", 4751.5, 568.9, 6418.5, "Desolation Chain 1"),
            ("essence of desolation (Desolation Key 2)", 5146.8, 616.6, 6620.1, "Desolation Chain 2"),
            ("essence of desolation (Desolation Key 3)", 5116.0, 713.5, 7174.8, "Desolation Chain 3"),
            ("essence of desolation (Desolation Key 4)", 4734.2, 884.5, 7688.3, "Desolation Chain 4"),
            ("Quintessence of Veeshan", 4728.0, 848.0, 7617.0, "Desolation Chain 4"),
        ] {
            assert_eq!(isle(x, y, z), Some(want), "{what} at ({x}, {y}, {z})");
        }
    }

    /// The words, world 4 alone; off every isle, none (the caller's own name stands).
    #[test]
    fn place_words() {
        assert_eq!(place(4, 4160.7, 613.3, 6404.1).as_deref(), Some("Plane of Sky (Night Chain 1)"));
        assert_eq!(place(4, 4824.0, 250.0, 5497.0).as_deref(), Some("Plane of Sky (Main Isle)"));
        assert_eq!(place(4, 0.0, 0.0, 0.0), None);
        assert_eq!(place(0, 4160.7, 613.3, 6404.1), None);
    }

    /// Each isle has a label, inside its own outline (one of them) and off every other isle's: snow 2's name is not
    /// on snow 1's mountain.
    #[test]
    fn each_label_is_inside_its_isle() {
        assert_eq!(labels().len(), ISLES.len());
        for &(name, [x, z]) in labels() {
            let (_, _, _, polys) = ISLES.iter().find(|i| i.0 == name).unwrap();
            assert!(polys.iter().any(|p| inside(p, x, z)), "{name}'s label at ({x}, {z}) is off its outline");
            let on = ISLES.iter().filter(|i| i.0 != name && i.3.iter().any(|p| inside(p, x, z))).map(|i| i.0).collect::<Vec<_>>();
            assert!(on.is_empty(), "{name}'s label at ({x}, {z}) is on {on:?} too");
        }
        let a = labels().iter().find(|l| l.0 == "Snow Chain 1").unwrap().1;
        let b = labels().iter().find(|l| l.0 == "Snow Chain 2").unwrap().1;
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) > 100.0, "snow 1's and 2's names apart: {a:?} {b:?}");
    }

    /// Names that would overlap move above or below, else are left out; the first keeps its spot. The control:
    /// names apart stay where they are.
    #[test]
    fn labels_never_overlap() {
        let r = |x: f32, y: f32| [x - 40.0, y - 8.0, x + 40.0, y + 8.0];
        let got = place_labels(&[r(100.0, 100.0), r(110.0, 100.0), r(120.0, 104.0), r(130.0, 100.0), r(400.0, 400.0)]);
        assert_eq!(got[0], Some(r(100.0, 100.0)));
        assert_eq!(got[1], Some(r(110.0, 84.0)), "above");
        assert_eq!(got[2], Some(r(120.0, 120.0)), "below");
        assert_eq!(got[3], None, "no room: left out");
        assert_eq!(got[4], Some(r(400.0, 400.0)), "the control");
        let put: Vec<_> = got.iter().flatten().collect();
        for (i, a) in put.iter().enumerate() {
            for b in &put[i + 1..] {
                assert!(!(a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]), "{a:?} over {b:?}");
            }
        }
    }

    /// Snow 1's mountain over snow 2's village: the same (x, z) by its height.
    #[test]
    fn overlap_by_height() {
        assert_eq!(isle(5766.8, 238.3, 6702.1), Some("Snow Chain 2"));
        assert_eq!(isle(5766.8, 520.0, 6702.1), Some("Snow Chain 1"));
    }
}
