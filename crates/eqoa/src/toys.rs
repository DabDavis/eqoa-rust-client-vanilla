//! Toymaker Tinsel's Christmas toys, as this client draws them (go-eqoa-server's
//! own; the server owner, 2026-11-17: "lets add string of lights candy cane
//! staff", then "add gingerbread illusion too, its clay golem with white eyes,
//! smile, liek the cookie"). Each rides an aura kind of the entity channel's
//! (`net::npctint::Aura`), so a client before them reads the default with no
//! glow, nothing, and a PS2 is told none of it:
//!
//! - **The String of Lights** ([`Aura::LIGHTS`] on the worn cape's aura,
//!   `aura::CAPE`): a cape item that is a string of twinkling bulbs as well.
//!   The bulbs are laid on the cape as fitted to the wearer (`cape::model`,
//!   every body that wears one, illusions too): round its whole outline (up
//!   one side, across the collar over the shoulders, down the other side and
//!   back along the hem), down its middle and in a zigzag across it
//!   ([`cape_lights`]). Each bulb takes the skin of the cape vertex it lies
//!   by, so it moves with the wearer's bones; each frame its place is skinned
//!   there ([`placed`]) and it is drawn as a decoration's bulb is
//!   (`decorlights`: the soft flare, additive, in the "Christmas lights"
//!   template's colours and twinkle, on a thin green cord), over a solid bead
//!   of its colour ([`glass`]) that shows by day.
//! - **The Candy Cane Staff** ([`Aura::STRIPES`], a held item's aura, the
//!   staff's second: its first the monk staff's twinkle at its tip): the held
//!   model cut fine and each piece painted red or white by where it lies on a
//!   spiral round the staff's length ([`stripe`]).
//! - **The Gingerbread Man** ([`Aura::ICING`] on the body's aura, while the
//!   toy's illusion is the body shown): the body baked gingerbread brown and
//!   iced ([`ice`]): two round white eyes and a smile on the face, three
//!   buttons down the chest, a wavy line of icing round each wrist and ankle,
//!   each bead a little white ball skinned to the node it sits on.

use esf::Model;
use glam::{Mat4, Vec3};
use net::npctint::Aura;
use vi::csprite::CSprite;
use vi::fpu::Ee;
use vi::geom::Matrix44;

/// The "Christmas lights" template's colours (go-eqoa-server's
/// tools/christmas_lights_20261116.json): red, green, blue, gold, white, pink.
pub const COLORS: [u32; 6] = [0xFF2A_2AFF, 0x2AE0_4AFF, 0x3A78_FFFF, 0xFFC2_3AFF, 0xFFFF_FFFF, 0xFF4F_D0FF];

/// The template's cord, dark green, and its cycle.
pub const CORD: u32 = 0x1E3F_1EFF;
pub const PERIOD: f32 = 1.8;
pub const BRIGHT: f32 = 1.5;

/// A bulb's glow across, as a share of the cape's height (a long knight's cape
/// some 1.6 high: 0.19 across). The owner (2026-11-18), of the first string
/// (0.075, its sides' top 60% alone): "looks like a normal cape. should look
/// like a cape with the christmas lights outline it and in the middle".
pub const BULB_SHARE: f32 = 0.12;
/// Each bulb's glass, a solid bead under its glow ([`glass`]), as a share of
/// the glow across: its half width (its length 1.7 times it).
pub const GLASS_SHARE: f32 = 0.16;
/// The cord's thickness and the spacing of the bulbs, the same way.
pub const CORD_SHARE: f32 = 0.009;
pub const SPACING_SHARE: f32 = 0.07;
/// How far off the cape a bulb is hung (out from the body), of its height.
pub const OFF_SHARE: f32 = 0.025;
/// The middle's string: its centre line from this far under the cape's top
/// to this far over its hem, and its zigzag between these two depths (each
/// of the height, from the top), across [`ZIG_SPAN`] of the cape's width there
/// in [`ZIGS`] legs.
pub const CENTRE_FROM: f32 = 0.12;
pub const CENTRE_TO: f32 = 0.06;
pub const ZIG_TOP: f32 = 0.38;
pub const ZIG_BOTTOM: f32 = 0.56;
pub const ZIG_SPAN: f32 = 0.84;
pub const ZIGS: usize = 6;

/// One bulb of a worn string: its place in the wearer's bind pose (model
/// space), the nodes and weights it is skinned by, its colour (of [`COLORS`],
/// with `net::decor::BREAK` where a new run of the cord starts) and its
/// phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bulb {
    pub at: Vec3,
    pub bones: [u8; 4],
    pub weights: [u8; 4],
    pub color: u8,
    pub phase: u8,
}

/// A worn string of lights: its bulbs and its size (a bulb's glow across, the
/// cord's thickness).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lights {
    pub bulbs: Vec<Bulb>,
    pub size: f32,
    pub cord: f32,
}

/// Whether the cape's aura is the String of Lights.
pub fn lights_worn(auras: &[net::entity::PieceAura; net::entity::AURAS]) -> bool {
    auras[crate::aura::CAPE].kind == Aura::LIGHTS
}

/// A small mixing hash (a bulb's colour and phase by its place on the string).
fn hash(i: u32, salt: u32) -> u32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

/// Whether a batch is drawn at the nearest level (or at every level).
fn nearest(b: &esf::Batch) -> bool {
    b.lod.is_none_or(|l| l == 0)
}

/// A point on the cape a bulb hangs by: where, the cape's normal there, and
/// the cape vertex whose skin it takes.
type On = (Vec3, Vec3, usize);

/// The string of lights on `cape` (a cape as `cape::model` fits it: in the
/// wearer's bind pose, skinned to its nodes, hanging behind the body, -z), in
/// three runs of one cord: its whole outline as seen from behind ([`outline`]:
/// up its left side, across its top over the shoulders, down its right side
/// and back along its hem; the cape's own vertices, so a cape cut with its
/// lining, or drawn from both sides, is one), a line down its middle from
/// under the collar to over the hem, and a zigzag across its middle, side to
/// side (both on the cape's outer face, [`back_hit`]). A bulb every
/// [`SPACING_SHARE`] of its height along each; each skinned as the cape vertex
/// it lies by, and off the cape, out from the body, by [`OFF_SHARE`]. None for
/// a cape of no height.
pub fn cape_lights(cape: &Model) -> Option<Lights> {
    let used = used_vertices(cape);
    let (lo, hi) = used.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(cape.pos[v][1]), hi.max(cape.pos[v][1])));
    let h = hi - lo;
    if !(h > 0.0) {
        return None;
    }
    let normal = |v: usize| cape.normal.get(v).map(|n| Vec3::from(*n)).unwrap_or(Vec3::NEG_Z);
    let at = |v: usize| -> On { (Vec3::from(cape.pos[v]), normal(v), v) };
    let tris: Vec<[u32; 3]> = cape.batches.iter().filter(|b| nearest(b)).flat_map(|b| b.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect::<Vec<_>>()).collect();
    let step = SPACING_SHARE * h;
    let mut runs: Vec<Vec<On>> = Vec::new();
    // the outline, a vertex path: a bulb every step along it
    let ring: Vec<On> = outline(cape, &used, lo, hi, h).into_iter().map(at).collect();
    runs.push(spaced(&ring, step));
    // the middle: points laid in the plane seen from behind, then put on
    // the cape's outer face there
    // the cape's width at a height: where its triangles' edges cross it
    let span = |y: f32| {
        let mut r: Option<(f32, f32)> = None;
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (cape.pos[t[k] as usize], cape.pos[t[(k + 1) % 3] as usize]);
                if (a[1] - y) * (b[1] - y) > 0.0 || a[1] == b[1] {
                    continue;
                }
                let x = a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]);
                r = Some(r.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))));
            }
        }
        r
    };
    let onto = |pts: &[(f32, f32)]| -> Vec<On> { pts.iter().filter_map(|&(x, y)| back_hit(cape, &tris, x, y)).collect() };
    let (top, bottom) = (hi - CENTRE_FROM * h, lo + CENTRE_TO * h);
    let mut centre = Vec::new();
    let n = ((top - bottom) / (0.25 * step)).ceil().max(1.0) as usize;
    for k in 0..=n {
        let y = top + (bottom - top) * k as f32 / n as f32;
        if let Some((a, b)) = span(y) {
            centre.push(((a + b) * 0.5, y));
        }
    }
    runs.push(spaced(&onto(&centre), step));
    let mut zig = Vec::new();
    for k in 0..=ZIGS {
        let y = hi - h * if k % 2 == 0 { ZIG_TOP } else { ZIG_BOTTOM };
        let Some((a, b)) = span(y) else { continue };
        let f = k as f32 / ZIGS as f32;
        zig.push(((a + b) * 0.5 + (b - a) * ZIG_SPAN * (f - 0.5), y));
    }
    // each leg cut fine, so its points follow the cape's face
    let mut fine = Vec::new();
    for w in zig.windows(2) {
        for k in 0..8 {
            let f = k as f32 / 8.0;
            fine.push((w[0].0 + (w[1].0 - w[0].0) * f, w[0].1 + (w[1].1 - w[0].1) * f));
        }
    }
    fine.extend(zig.last().copied());
    runs.push(spaced(&onto(&fine), step));
    let off = OFF_SHARE * h;
    let skin = |v: usize| (cape.bones.get(v).copied().unwrap_or([0, 255, 255, 255]), cape.weights.get(v).copied().unwrap_or([127, 0, 0, 0]));
    let mut bulbs = Vec::new();
    for run in runs.iter().filter(|r| r.len() >= 2) {
        for (k, &(p, nrm, near)) in run.iter().enumerate() {
            // out from the body: the cape's normal turned away from the spine
            // (behind it, -z; over the shoulders, up and back)
            let mut nrm = nrm.normalize_or(Vec3::NEG_Z);
            let out = Vec3::new(p.x, 0.0, p.z).normalize_or(Vec3::NEG_Z) + Vec3::NEG_Z;
            if nrm.dot(out) < 0.0 {
                nrm = -nrm;
            }
            let i = bulbs.len() as u32;
            let (bones, weights) = skin(near);
            let brk = if k == 0 { net::decor::BREAK } else { 0 };
            bulbs.push(Bulb { at: p + nrm * off, bones, weights, color: (hash(i, 7) % COLORS.len() as u32) as u8 | brk, phase: hash(i, 13) as u8 });
        }
    }
    (bulbs.len() >= 2).then_some(Lights { bulbs, size: BULB_SHARE * h, cord: CORD_SHARE * h })
}

/// Points every `step` along a path (its first, then each `step` on): each
/// skinned as the nearer of the two path points it lies between.
fn spaced(path: &[On], step: f32) -> Vec<On> {
    let mut out = Vec::new();
    let mut left = 0.0f32;
    for pair in path.windows(2) {
        let ((a, na, va), (b, nb, vb)) = (pair[0], pair[1]);
        let len = (b - a).length();
        let mut d = left;
        while d <= len {
            let f = if len > 0.0 { d / len } else { 0.0 };
            out.push(if f < 0.5 { (a.lerp(b, f), na, va) } else { (a.lerp(b, f), nb, vb) });
            d += step;
        }
        left = d - len;
    }
    out
}

/// The back-most hit (least z: the cape hangs behind a body facing +z, and
/// its outer face is the one seen from behind) of a ray from behind at
/// (x, y) on the triangles `tris` of `m`: the place, the triangle's normal,
/// and its vertex nearest the hit.
fn back_hit(m: &Model, tris: &[[u32; 3]], x: f32, y: f32) -> Option<On> {
    let mut best: Option<On> = None;
    for t in tris {
        let p: [Vec3; 3] = t.map(|i| Vec3::from(m.pos[i as usize]));
        let d = (p[1].y - p[2].y) * (p[0].x - p[2].x) + (p[2].x - p[1].x) * (p[0].y - p[2].y);
        if d.abs() < 1e-9 {
            continue;
        }
        let a = ((p[1].y - p[2].y) * (x - p[2].x) + (p[2].x - p[1].x) * (y - p[2].y)) / d;
        let b = ((p[2].y - p[0].y) * (x - p[2].x) + (p[0].x - p[2].x) * (y - p[2].y)) / d;
        let c = 1.0 - a - b;
        if a < -1e-4 || b < -1e-4 || c < -1e-4 {
            continue;
        }
        let z = a * p[0].z + b * p[1].z + c * p[2].z;
        if best.is_some_and(|(q, _, _)| q.z <= z) {
            continue;
        }
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let near = if a >= b && a >= c { t[0] } else if b >= c { t[1] } else { t[2] };
        best = Some((Vec3::new(x, y, z), n, near as usize));
    }
    best
}

/// The vertices the nearest level's triangles use, once each, in order.
fn used_vertices(m: &Model) -> Vec<usize> {
    let mut seen = vec![false; m.pos.len()];
    for b in m.batches.iter().filter(|b| nearest(b)) {
        for &i in &b.indices {
            if let Some(s) = seen.get_mut(i as usize) {
                *s = true;
            }
        }
    }
    (0..m.pos.len()).filter(|&v| seen[v]).collect()
}

/// How many bands a cape's sides are read in, and its top and hem.
const SIDE_BANDS: usize = 16;
const TOP_BINS: usize = 9;

/// A cape's whole outline seen from behind, as its vertices in order, round:
/// its left side (least x, a vertex a band of height) from its hem up, its
/// top (the highest vertex of each of [`TOP_BINS`] across, of those in its
/// top fifth) from left to right, its right side (most x) down, and its hem
/// (the lowest of each bin across, of those in its bottom fifth) from right
/// to left; a vertex never twice in a row.
fn outline(m: &Model, used: &[usize], lo: f32, hi: f32, h: f32) -> Vec<usize> {
    let p = |v: usize| m.pos[v];
    let band = |k: usize| {
        let (a, b) = (lo + h * k as f32 / SIDE_BANDS as f32, lo + h * (k + 1) as f32 / SIDE_BANDS as f32);
        used.iter().copied().filter(move |&v| p(v)[1] >= a && p(v)[1] <= b)
    };
    let mut left = Vec::new();
    let mut right = Vec::new();
    for k in 0..SIDE_BANDS {
        if let Some(v) = band(k).min_by(|&a, &b| p(a)[0].total_cmp(&p(b)[0])) {
            left.push(v);
        }
        if let Some(v) = band(k).max_by(|&a, &b| p(a)[0].total_cmp(&p(b)[0])) {
            right.push(v);
        }
    }
    // the highest (the top's) or lowest (the hem's) vertex of each bin across
    let across = |near: &dyn Fn(usize) -> bool, top: bool| {
        let pick: Vec<usize> = used.iter().copied().filter(|&v| near(v)).collect();
        let (x0, x1) = pick.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(p(v)[0]), b.max(p(v)[0])));
        let mut out = Vec::new();
        if x1 > x0 {
            for k in 0..TOP_BINS {
                let (a, b) = (x0 + (x1 - x0) * k as f32 / TOP_BINS as f32, x0 + (x1 - x0) * (k + 1) as f32 / TOP_BINS as f32);
                let bin = pick.iter().copied().filter(|&v| p(v)[0] >= a && p(v)[0] <= b);
                let v = if top { bin.max_by(|&a, &b| p(a)[1].total_cmp(&p(b)[1])) } else { bin.min_by(|&a, &b| p(a)[1].total_cmp(&p(b)[1])) };
                out.extend(v);
            }
        }
        out
    };
    let top = across(&|v| p(v)[1] >= hi - 0.2 * h, true);
    let hem = across(&|v| p(v)[1] <= lo + 0.2 * h, false);
    let mut out: Vec<usize> = Vec::new();
    for v in left.into_iter().chain(top).chain(right.into_iter().rev()).chain(hem.into_iter().rev()) {
        if out.last() != Some(&v) {
            out.push(v);
        }
    }
    // round: back to where it began
    if let Some(&first) = out.first()
        && out.last() != Some(&first)
    {
        out.push(first);
    }
    out
}

/// Each bulb's glass of the strings of `d` (`placed`'s): a solid bead, its
/// colour (as lit now, `ms`), standing where its glow is, so the string
/// reads as a string of bulbs by day as well as by night (its glow, additive,
/// fades into a bright sky). Drawn as the decorations' model bulbs are: not
/// lit by the scene, a bulb is its own light.
pub fn glass(d: &net::decor::Decor, ms: i64) -> Vec<render::billboard::Strips> {
    let t = ms as f64 / 1000.0;
    let mut strips = Vec::new();
    let mut colors = Vec::new();
    for s in &d.strings {
        let r = GLASS_SHARE * s.size;
        for b in &s.bulbs {
            let c = s.colors.get((b.color & net::decor::COLOR) as usize).copied().unwrap_or(0xFFFF_FFFF);
            let lit = net::decor::lit(s.effect, s.period, b.phase, t);
            let k = 0.7 + 0.5 * lit;
            let col = [((c >> 24) & 0xFF) as f32 / 255.0 * k, ((c >> 16) & 0xFF) as f32 / 255.0 * k, ((c >> 8) & 0xFF) as f32 / 255.0 * k, 1.0];
            let p = Vec3::from(b.at);
            // an eight-sided bead, longer than wide: two tips and a ring of four
            let (up, down) = (p + Vec3::Y * r * 1.0, p - Vec3::Y * r * 1.7);
            let ring = [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z].map(|d| p + d * r - Vec3::Y * r * 0.2);
            for k in 0..4 {
                let (a, bq) = (ring[k], ring[(k + 1) % 4]);
                for tri in [[up, a, bq], [down, bq, a]] {
                    strips.push(tri.iter().map(|q| ([0.0, 0.0], q.to_array())).collect());
                    colors.push(vec![col; 3]);
                }
            }
        }
    }
    if strips.is_empty() {
        return Vec::new();
    }
    vec![render::billboard::Strips { texture: 0, blend: 0, ztest: true, zwrite: true, color: [1.0; 4], strips, colors }]
}

/// The skin palette's matrices of `cs` placed by `centre` (glam's): a node's
/// `inv_bind_world × world × centre`, so a bind-pose point skinned to it is
/// where the body drawn there has it.
pub fn skins(cs: &CSprite<Ee>, centre: &Matrix44<Ee>) -> Vec<Mat4> {
    cs.h.nodes.iter().map(|n| Mat4::from_cols_array_2d(&Matrix44::mul(&n.world, centre).to_cols()) * Mat4::from_cols_array_2d(&n.inv_bind_world.to_cols())).collect()
}

/// A bind-pose point skinned by `bones` and `weights` through `skins`.
pub fn skinned(skins: &[Mat4], at: Vec3, bones: [u8; 4], weights: [u8; 4]) -> Vec3 {
    let (mut q, mut sum) = (Vec3::ZERO, 0.0);
    for k in 0..4 {
        if bones[k] != 255 && weights[k] > 0 && let Some(m) = skins.get(bones[k] as usize) {
            q += m.transform_point3(at) * weights[k] as f32;
            sum += weights[k] as f32;
        }
    }
    if sum > 0.0 { q / sum } else { at }
}

/// The string as a decoration's string of world `world`, its bulbs where the
/// body (`cs`, placed by `centre`) has them now: for `decorlights` to draw.
pub fn placed(l: &Lights, cs: &CSprite<Ee>, centre: &Matrix44<Ee>, world: u8) -> net::decor::LightString {
    let s = skins(cs, centre);
    let bulbs = l.bulbs.iter().map(|b| net::decor::Bulb { at: skinned(&s, b.at, b.bones, b.weights).to_array(), color: b.color, phase: b.phase }).collect();
    net::decor::LightString {
        anchor: net::decor::Anchor::World(world),
        effect: net::decor::Effect::Twinkle,
        period: PERIOD,
        size: l.size,
        bright: BRIGHT,
        cord: CORD,
        cord_width: l.cord,
        cord_sag: 0.06,
        colors: COLORS.to_vec(),
        bulbs,
        look: Default::default(),
    }
}

/// The candy cane's colours: red and white.
pub const CANDY_RED: [u8; 4] = [0xD0, 0x10, 0x20, 0xFF];
pub const CANDY_WHITE: [u8; 4] = [0xF8, 0xF8, 0xF4, 0xFF];

/// How many red stripes wind along a held item unless its aura says.
pub const STRIPES: u32 = 9;

/// The longest a triangle's edge is left when a model is cut for its stripes,
/// as a share of a stripe's width.
const CUT_SHARE: f32 = 0.25;

/// `m` (a held item, rigid) striped as a candy cane: cut fine ([`CUT_SHARE`]
/// of a stripe), each piece red ([`CANDY_RED`], or `color` when it lights) or
/// white ([`CANDY_WHITE`]) by where its middle lies on a spiral round the
/// item's longest axis, `count` red stripes along it (0: [`STRIPES`]), its
/// textures dropped (the stripes are its whole art) and no glow of its own.
pub fn stripe(m: &mut Model, color: Option<[u8; 3]>, count: u8) {
    if m.pos.is_empty() || m.batches.is_empty() {
        return;
    }
    let (lo, hi) = m.pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
    let span = hi - lo;
    let axis = if span.x >= span.y && span.x >= span.z { 0 } else if span.y >= span.z { 1 } else { 2 };
    let len = span[axis];
    if len <= 0.0 {
        return;
    }
    let count = if count == 0 { STRIPES } else { count as u32 };
    let pitch = len / count as f32;
    let mid = (lo + hi) * 0.5;
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    // a point's place on the spiral: along the axis, plus a turn round it
    let phase = |p: Vec3| {
        let a = (p[v] - mid[v]).atan2(p[u] - mid[u]) / std::f32::consts::TAU;
        ((p[axis] - lo[axis]) / pitch + a).rem_euclid(1.0)
    };
    let most = CUT_SHARE * pitch * 0.5;
    let red = color.map_or(CANDY_RED, |c| [c[0], c[1], c[2], 0xFF]);
    let batch = |c: [u8; 4]| {
        let mut b = esf::Batch { indices: Vec::new(), ..m.batches[0].clone() };
        b.layers.truncate(1);
        for l in &mut b.layers {
            l.texture = 0;
            l.color = c;
            l.blend = 0;
            l.emissive = [0; 4];
        }
        b
    };
    let (mut reds, mut whites) = (batch(red), batch(CANDY_WHITE));
    let tris: Vec<[u32; 3]> = m.batches.iter().filter(|b| nearest(b)).flat_map(|b| b.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect::<Vec<_>>()).collect();
    let mut stack = tris;
    let mut guard = 0usize;
    while let Some(t) = stack.pop() {
        let p: [Vec3; 3] = t.map(|i| Vec3::from(m.pos[i as usize]));
        let e = [(p[1] - p[0]).length(), (p[2] - p[1]).length(), (p[0] - p[2]).length()];
        let (k, &longest) = e.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        if longest > most && guard < 400_000 {
            guard += 1;
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let mid = midpoint(m, a, b);
            stack.push([a, mid, c]);
            stack.push([mid, b, c]);
            continue;
        }
        let centre = (p[0] + p[1] + p[2]) / 3.0;
        if phase(centre) < 0.5 { &mut reds } else { &mut whites }.indices.extend_from_slice(&t);
    }
    m.batches = [reds, whites].into_iter().filter(|b| !b.indices.is_empty()).collect();
    m.triangles = m.batches.iter().map(|b| b.indices.len() as u32 / 3).sum();
}

/// A new vertex of `m` halfway between `a` and `b`, every one of its arrays
/// carried.
fn midpoint(m: &mut Model, a: u32, b: u32) -> u32 {
    let (a, b) = (a as usize, b as usize);
    let half = |x: &[f32], y: &[f32]| -> Vec<f32> { x.iter().zip(y).map(|(p, q)| (p + q) * 0.5).collect() };
    let p = half(&m.pos[a], &m.pos[b]);
    m.pos.push([p[0], p[1], p[2]]);
    if m.uv.len() > b.max(a) {
        let q = half(&m.uv[a], &m.uv[b]);
        m.uv.push([q[0], q[1]]);
    }
    if m.normal.len() > b.max(a) {
        let q = Vec3::from_slice(&half(&m.normal[a], &m.normal[b])).normalize_or_zero();
        m.normal.push(q.to_array());
    }
    if m.color.len() > b.max(a) {
        let q = half(&m.color[a], &m.color[b]);
        m.color.push([q[0], q[1], q[2], q[3]]);
    }
    if m.bones.len() > b.max(a) {
        let k = if m.weights[a].iter().map(|&w| w as u32).max() >= m.weights[b].iter().map(|&w| w as u32).max() { a } else { b };
        m.bones.push(m.bones[k]);
        m.weights.push(m.weights[k]);
    }
    (m.pos.len() - 1) as u32
}

/// A gingerbread man's brown (#A0612B), as the gain the clay golem's own
/// clay brown is multiplied by to bake to it (of 128: a little more red, less
/// blue), so the texture's own shading stays.
pub const GINGER_GAIN: [u8; 3] = [150, 118, 82];
/// The icing's white.
pub const ICING: [u8; 4] = [0xFA, 0xF8, 0xF2, 0xFF];

/// Where the face's icing goes (`ice`), read off the clay golem's face (its
/// dark eye sockets under the brow, its mouth over the chin: the owner,
/// 2026-11-18, "the gingerbread man face needs to move down and be in his eye
/// sockets and mouth area"): the eyes this far under the head node, of the
/// head node's height over the jaw's, and this far out, of the face's half
/// width there; the smile's middle this far over the jaw, its corners curled
/// up this much more, across this much of the half width.
pub const EYES_UNDER: f32 = 0.15;
pub const EYES_APART: f32 = 0.5;
pub const SMILE_OVER: f32 = 0.0;
pub const SMILE_CURL: f32 = 0.2;
pub const SMILE_WIDE: f32 = 0.38;

/// A node's bind-pose world place.
fn node_at(cs: &CSprite<Ee>, node: i32) -> Option<Vec3> {
    let n = cs.h.nodes.get(usize::try_from(node).ok()?)?;
    Some(Mat4::from_cols_array_2d(&n.inv_bind_world.to_cols()).inverse().w_axis.truncate())
}

/// Whether `node` is `top` or a node under it.
fn under(cs: &CSprite<Ee>, mut node: i32, top: i32) -> bool {
    for _ in 0..cs.h.nodes.len() {
        if node == top {
            return true;
        }
        match usize::try_from(node).ok().and_then(|i| cs.h.nodes.get(i)) {
            Some(n) => node = n.parent,
            None => return false,
        }
    }
    false
}

/// A vertex's heaviest bone (255 none).
fn heaviest(m: &Model, v: usize) -> u8 {
    let (Some(b), Some(w)) = (m.bones.get(v), m.weights.get(v)) else { return 255 };
    let k = (0..4).max_by_key(|&k| w[k]).unwrap_or(0);
    if w[k] > 0 { b[k] } else { 255 }
}

/// The front-most hit (largest z: a body faces +z) of a ray from the front at
/// (x, y) on the triangles of `m` whose vertices `keep` says, with its
/// triangle's normal (turned to the front) and the nearest vertex's heaviest
/// bone.
fn front_hit(m: &Model, tris: &[[u32; 3]], keep: &dyn Fn(usize) -> bool, x: f32, y: f32) -> Option<(Vec3, Vec3, u8)> {
    let mut best: Option<(Vec3, Vec3, u8)> = None;
    for t in tris {
        if !t.iter().all(|&v| keep(v as usize)) {
            continue;
        }
        let p: [Vec3; 3] = t.map(|i| Vec3::from(m.pos[i as usize]));
        // barycentric in the xy plane
        let d = (p[1].y - p[2].y) * (p[0].x - p[2].x) + (p[2].x - p[1].x) * (p[0].y - p[2].y);
        if d.abs() < 1e-9 {
            continue;
        }
        let a = ((p[1].y - p[2].y) * (x - p[2].x) + (p[2].x - p[1].x) * (y - p[2].y)) / d;
        let b = ((p[2].y - p[0].y) * (x - p[2].x) + (p[0].x - p[2].x) * (y - p[2].y)) / d;
        let c = 1.0 - a - b;
        if a < -1e-4 || b < -1e-4 || c < -1e-4 {
            continue;
        }
        let z = a * p[0].z + b * p[1].z + c * p[2].z;
        if best.is_some_and(|(q, _, _)| q.z >= z) {
            continue;
        }
        let mut n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n.z < 0.0 {
            n = -n;
        }
        let near = if a >= b && a >= c { t[0] } else if b >= c { t[1] } else { t[2] };
        best = Some((Vec3::new(x, y, z), n, heaviest(m, near as usize)));
    }
    best
}

/// The wrist (the ankle) a hand's item node (a foot's toe node) `tip` hangs
/// from, and the node over it (the forearm, the shin).
fn wrist(cs: &CSprite<Ee>, tip: i32) -> Option<(i32, i32)> {
    let joint = cs.h.nodes.get(usize::try_from(tip).ok()?)?.parent;
    let up = cs.h.nodes.get(usize::try_from(joint).ok()?)?.parent;
    (up >= 0).then_some((joint, up))
}

/// How far along the bone into a wrist (an ankle) its ring is looked for, of
/// the bone's length: from this far above the joint to this far past it.
const RING_FROM: f32 = 0.45;
const RING_TO: f32 = 0.05;
const RING_BINS: usize = 12;
/// How far off the bone a vertex of another node may be and still be the
/// limb's skin, of the bone's length.
const LIMB_REACH: f32 = 0.6;

/// Where round the limb into `joint` (at `j`, from `up` at `u`) a ring of
/// icing goes: the limb's skin (the vertices of the two nodes and those
/// under the joint, or any as near the bone as [`LIMB_REACH`]) read in [`RING_BINS`] slices along the bone, from
/// [`RING_FROM`] of it above the joint to [`RING_TO`] past it, each slice's
/// radius its farthest vertex off the bone; the narrowest slice (the wrist
/// above the fist, the ankle above the foot). Its middle, the bone's
/// direction and that radius.
fn narrowest(m: &Model, cs: &CSprite<Ee>, joint: i32, up: i32, j: Vec3, u: Vec3) -> Option<(Vec3, Vec3, f32)> {
    let len = (j - u).length();
    let axis = (j - u).normalize_or_zero();
    if axis == Vec3::ZERO {
        return None;
    }
    let (from, to) = (-RING_FROM * len, RING_TO * len);
    let mut far = [0.0f32; RING_BINS];
    let mut count = [0usize; RING_BINS];
    for (v, p) in m.pos.iter().enumerate() {
        let d = Vec3::from(*p) - j;
        let along = d.dot(axis);
        if along < from || along >= to {
            continue;
        }
        // the limb's own skin: its two nodes' and the hand's (the foot's),
        // or (a low body's shin skinned to its hips) any vertex as near the
        // bone as the limb is thick
        let b = heaviest(m, v);
        let own = b != 255 && (b as i32 == up || under(cs, b as i32, joint));
        if !own && (d - axis * along).length() > LIMB_REACH * len {
            continue;
        }
        let k = (((along - from) / (to - from)) * RING_BINS as f32) as usize;
        let k = k.min(RING_BINS - 1);
        far[k] = far[k].max((d - axis * along).length());
        count[k] += 1;
    }
    let k = (0..RING_BINS).filter(|&k| count[k] >= 2).min_by(|&a, &b| far[a].total_cmp(&far[b]))?;
    let along = from + (to - from) * (k as f32 + 0.5) / RING_BINS as f32;
    Some((j + axis * along, axis, far[k]))
}

/// How far above the joint a ring goes on a limb with no vertex by it, of
/// the bone's length.
const RING_ABOVE: f32 = 0.2;

/// The nearest hit, as its distance, of a ray from `from` along `dir` on the
/// triangles `tris` of `m` (the limb's skin seen from inside it).
fn ray_out(m: &Model, tris: &[[u32; 3]], from: Vec3, dir: Vec3) -> Option<f32> {
    let mut best: Option<f32> = None;
    for t in tris {
        let p: [Vec3; 3] = t.map(|i| Vec3::from(m.pos[i as usize]));
        let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
        let q = dir.cross(e2);
        let det = e1.dot(q);
        if det.abs() < 1e-9 {
            continue;
        }
        let o = from - p[0];
        let u = o.dot(q) / det;
        if !(-1e-4..=1.0 + 1e-4).contains(&u) {
            continue;
        }
        let r = o.cross(e1);
        let v = dir.dot(r) / det;
        if v < -1e-4 || u + v > 1.0 + 1e-4 {
            continue;
        }
        let d = e2.dot(r) / det;
        if d > 1e-5 && best.is_none_or(|b| d < b) {
            best = Some(d);
        }
    }
    best
}

/// A white bead (a ball `r` across `scale` times flatter along `n`) at `at`
/// on `m`, skinned wholly to `node`, into `into` (a batch's indices).
fn bead(m: &mut Model, into: &mut Vec<u32>, at: Vec3, n: Vec3, r: f32, flat: f32, node: u8, color: [f32; 4]) {
    let (rings, segs) = (6u32, 8u32);
    let n = n.normalize_or(Vec3::Z);
    let t = n.any_orthonormal_vector();
    let s = n.cross(t);
    let first = m.pos.len() as u32;
    for i in 0..=rings {
        let th = std::f32::consts::PI * i as f32 / rings as f32;
        for j in 0..=segs {
            let ph = std::f32::consts::TAU * j as f32 / segs as f32;
            let d = n * th.cos() * flat + (t * ph.cos() + s * ph.sin()) * th.sin();
            m.pos.push((at + d * r).to_array());
            if !m.uv.is_empty() {
                m.uv.push([0.0, 0.0]);
            }
            if !m.normal.is_empty() {
                m.normal.push((n * th.cos() + (t * ph.cos() + s * ph.sin()) * th.sin()).to_array());
            }
            if !m.color.is_empty() {
                m.color.push(color);
            }
            if !m.bones.is_empty() {
                m.bones.push([node, 255, 255, 255]);
                m.weights.push([127, 0, 0, 0]);
            }
        }
    }
    for i in 0..rings {
        for j in 0..segs {
            let a = first + i * (segs + 1) + j;
            let b = a + segs + 1;
            into.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
}

/// `model` (a body, `cs` its sprite, in its bind pose) as a gingerbread man:
/// its layers baked by [`GINGER_GAIN`] (each layer's colour times it, the texture's own
/// shading kept) and iced ([`ICING`]): two round eyes in the face's eye
/// sockets and a smile at its mouth (by the head node and the jaw's,
/// [`EYES_UNDER`]), down the chest three buttons, round each wrist and ankle
/// (just above the hand's and the foot's joint, where the limb is narrowest:
/// [`narrowest`]) a wavy ring of beads. Each bead is skinned wholly to the node the body is under it there,
/// so the icing moves as the cookie does. False (and `model` only browned)
/// where the body has no head node or no face to put the eyes on.
pub fn ice(model: &mut Model, cs: &CSprite<Ee>) -> bool {
    for l in model.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        for k in 0..3 {
            l.color[k] = ((l.color[k] as u32 * GINGER_GAIN[k] as u32 / 128).min(255)) as u8;
        }
    }
    if model.batches.is_empty() || model.bones.is_empty() {
        return false;
    }
    // the body's own vertex colour (its light's share: a bead lit as the body
    // is, not a lamp by night)
    let shade = if model.color.is_empty() {
        [1.0; 4]
    } else {
        let n = model.color.len() as f32;
        let sum = model.color.iter().fold([0.0f32; 4], |a, c| std::array::from_fn(|k| a[k] + c[k]));
        sum.map(|x| x / n)
    };
    let head = cs.node_ids[7];
    let tris: Vec<[u32; 3]> = model.batches.iter().filter(|b| nearest(b)).flat_map(|b| b.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect::<Vec<_>>()).collect();
    let on_head: Vec<bool> = (0..model.pos.len()).map(|v| {
        let b = heaviest(model, v);
        b != 255 && under(cs, b as i32, head)
    }).collect();
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for (v, p) in model.pos.iter().enumerate() {
        if on_head[v] {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
        }
    }
    if lo.x > hi.x {
        return false;
    }
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    let cx = (lo.x + hi.x) * 0.5;
    let mut white = esf::Batch { material: -1, indices: Vec::new(), ..model.batches[0].clone() };
    white.layers.truncate(1);
    for l in &mut white.layers {
        l.texture = 0;
        l.color = ICING;
        l.blend = 0;
        l.emissive = [0; 4];
    }
    white.lod = None;
    let mut idx = Vec::new();
    let r_eye = 0.12 * w;
    let mut iced = false;
    // the face: the head's front, the neck's skin with it (a golem's brow,
    // eye sockets, jaw and chin hang on its neck as much as on its head);
    // its eye sockets are at the head node's own height, a little under it,
    // its mouth at the jaw's (the head's lowest child node, at the chin's
    // front; without one, a third of the way down under the head node)
    let neck_node = usize::try_from(head).ok().and_then(|i| cs.h.nodes.get(i)).map_or(-1, |n| n.parent);
    let on_face: Vec<bool> = (0..model.pos.len()).map(|v| {
        let b = heaviest(model, v);
        on_head[v] || (b != 255 && neck_node >= 0 && b as i32 == neck_node)
    }).collect();
    let keep_face = |v: usize| on_face[v];
    let head_y = node_at(cs, head).map_or(lo.y + 0.5 * h, |p| p.y);
    let jaw_y = cs.h.nodes.iter().enumerate().filter(|(_, n)| n.parent == head).filter_map(|(i, _)| node_at(cs, i as i32)).map(|p| p.y).filter(|&y| y < head_y).fold(None, |m: Option<f32>, y| Some(m.map_or(y, |m| m.min(y)))).unwrap_or(head_y - 0.33 * h);
    let span = head_y - jaw_y;
    let eye_y = head_y - EYES_UNDER * span;
    // the face's half width at the eyes: its front vertices that high
    let half = model.pos.iter().enumerate().filter(|(v, p)| on_face[*v] && (p[1] - eye_y).abs() <= 0.15 * span && p[2] > 0.0).map(|(_, p)| (p[0] - cx).abs()).fold(0.0f32, f32::max);
    let half = if half > 0.0 { half } else { 0.5 * w };
    // the eyes
    for side in [-1.0f32, 1.0] {
        if let Some((p, n, b)) = front_hit(model, &tris, &keep_face, cx + side * EYES_APART * half, eye_y) {
            bead(model, &mut idx, p + n * r_eye * 0.2, n, r_eye, 0.45, if b == 255 { head as u8 } else { b }, shade);
            iced = true;
        }
    }
    if !iced {
        return false;
    }
    // the smile: beads on an upturned arc at the mouth
    let smile = 9;
    for k in 0..smile {
        let f = k as f32 / (smile - 1) as f32 * 2.0 - 1.0;
        let x = cx + f * SMILE_WIDE * half;
        let y = jaw_y + (SMILE_OVER + SMILE_CURL * f * f) * span;
        // the front-most skin there, whichever node it hangs on (the clay
        // golem's chin and mouth are its chest's)
        if let Some((p, n, b)) = front_hit(model, &tris, &|_| true, x, y) {
            bead(model, &mut idx, p + n * r_eye * 0.1, n, r_eye * 0.4, 0.5, if b == 255 { head as u8 } else { b }, shade);
        }
    }
    // the buttons, down the chest from under the head to the waist
    let waist = node_at(cs, 0).map_or(lo.y - h, |p| p.y);
    let neck = lo.y.min(jaw_y);
    let off_head = |v: usize| !on_head[v];
    for k in 0..3 {
        let y = neck - (neck - waist) * (0.32 + 0.24 * k as f32);
        if let Some((p, n, b)) = front_hit(model, &tris, &off_head, cx, y) {
            bead(model, &mut idx, p + n * r_eye * 0.2, n, r_eye * 1.1, 0.45, if b == 255 { 0 } else { b }, shade);
        }
    }
    // wavy icing round the wrists and ankles: the wrist (the ankle) is the
    // node the hand's item node (the toe's) hangs from, the forearm (the
    // shin) the bone into it; the ring goes round the limb where it is
    // narrowest just above that joint, out to the limb's own skin there
    for &slot in &[0usize, 1, 5, 6] {
        let Some((joint, up)) = wrist(cs, cs.node_ids[slot]) else { continue };
        let (Some(j), Some(u)) = (node_at(cs, joint), node_at(cs, up)) else { continue };
        // where along the limb: its narrowest slice by the joint; a limb
        // with no vertex there (the clay golem's shins are one long box from
        // knee to foot) a little above the joint
        let axis = (j - u).normalize_or_zero();
        if axis == Vec3::ZERO {
            continue;
        }
        let (centre, r) = match narrowest(model, cs, joint, up, j, u) {
            Some((c, _, r)) => (c, Some(r)),
            None => (j - axis * RING_ABOVE * (j - u).length(), None),
        };
        // each bead out to the limb's skin that way, a ray from the bone
        // onto the body's own triangles
        let t = axis.any_orthonormal_vector();
        let s = axis.cross(t);
        let beads = 22;
        let outs: Vec<Vec3> = (0..beads).map(|k| {
            let a = std::f32::consts::TAU * k as f32 / beads as f32;
            t * a.cos() + s * a.sin()
        }).collect();
        let hits: Vec<Option<f32>> = outs.iter().map(|&o| ray_out(model, &tris, centre, o)).collect();
        let mut found: Vec<f32> = hits.iter().flatten().copied().collect();
        found.sort_by(f32::total_cmp);
        let Some(r) = found.get(found.len() / 2).copied().or(r) else { continue };
        let br = (0.2 * r).max(0.004);
        for (k, (&out, hit)) in outs.iter().zip(&hits).enumerate() {
            let wave = if k % 2 == 0 { 1.0 } else { -1.0 } * br * 0.7;
            bead(model, &mut idx, centre + out * (hit.unwrap_or(r) + br * 0.3) + axis * wave, out, br, 0.7, up as u8, shade);
        }
    }
    white.indices = idx;
    model.batches.push(white);
    model.triangles = model.batches.iter().map(|b| b.indices.len() as u32 / 3).sum();
    true
}

/// Whether the body's aura is a gingerbread man's icing.
pub fn iced(auras: &[net::entity::PieceAura; net::entity::AURAS]) -> bool {
    auras[crate::aura::BODY].kind == Aura::ICING
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip of a cape: a 1 by 2 sheet in the xy plane behind the body
    /// (z -0.2), cut in quads; the bulbs go round its whole outline (its
    /// sides top to bottom, its top and its hem), down its middle and across
    /// it, three runs of the cord (each begun with a break), each bulb off the
    /// sheet away from the spine. The control: a sheet of no height, none.
    #[test]
    fn lights_round_a_sheet_and_across_it() {
        let mut m = Model::default();
        let (cols, rows) = (4u32, 8u32);
        for r in 0..=rows {
            for c in 0..=cols {
                m.pos.push([c as f32 / cols as f32 - 0.5, 2.0 * r as f32 / rows as f32, -0.2]);
                m.normal.push([0.0, 0.0, 1.0]);
                m.bones.push([1, 255, 255, 255]);
                m.weights.push([127, 0, 0, 0]);
            }
        }
        let mut idx = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let a = r * (cols + 1) + c;
                let b = a + cols + 1;
                idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
            }
        }
        m.batches.push(esf::Batch { material: 0, layers: Vec::new(), indices: idx, lod: None });
        let l = cape_lights(&m).expect("an outline");
        // round 6 (a bulb every 0.14), down 1.64, across some 2: 70 or so
        assert!(l.bulbs.len() >= 55, "{} bulbs", l.bulbs.len());
        assert!(l.bulbs.iter().all(|b| b.at.z < -0.2), "hung behind the sheet, off the body");
        let near = |x: f32, y: f32| l.bulbs.iter().any(|b| (b.at.x - x).abs() < 0.08 && (b.at.y - y).abs() < 0.08);
        assert!(near(-0.5, 1.0) && near(0.5, 1.0), "down both sides");
        assert!(near(-0.5, 0.1) && near(0.5, 0.1), "the sides to the hem");
        assert!(near(0.0, 2.0) && near(0.0, 0.0), "the top and the hem");
        assert!(near(0.0, 1.3) && near(0.0, 0.5), "down the middle");
        let mid: Vec<&Bulb> = l.bulbs.iter().filter(|b| b.at.x.abs() > 0.15 && b.at.x.abs() < 0.35 && b.at.y > 0.8 && b.at.y < 1.4).collect();
        assert!(mid.len() >= 4, "the zigzag across the middle: {}", mid.len());
        assert_eq!(l.bulbs.iter().filter(|b| b.color & net::decor::BREAK != 0).count(), 3, "three runs");
        assert!(l.bulbs.iter().all(|b| b.bones[0] == 1));
        let colours: std::collections::HashSet<u8> = l.bulbs.iter().map(|b| b.color & net::decor::COLOR).collect();
        assert!(colours.len() >= 4, "many colours: {colours:?}");
        // each bulb a solid bead too, of its colour
        let s = net::decor::LightString { anchor: net::decor::Anchor::World(0), effect: net::decor::Effect::Twinkle, period: PERIOD, size: l.size, bright: BRIGHT, cord: CORD, cord_width: l.cord, cord_sag: 0.0, colors: COLORS.to_vec(), bulbs: l.bulbs.iter().map(|b| net::decor::Bulb { at: b.at.to_array(), color: b.color, phase: b.phase }).collect(), look: Default::default() };
        let g = glass(&net::decor::Decor { strings: vec![s], ..Default::default() }, 0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].strips.len(), 8 * l.bulbs.len());
        // the control: a flat sheet of no height has none
        let mut flat = Model::default();
        flat.pos = vec![[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0]];
        flat.batches.push(esf::Batch { material: 0, layers: Vec::new(), indices: vec![0, 1, 2], lod: None });
        assert!(cape_lights(&flat).is_none());
    }

    /// A stick along y striped: red and white both, every piece small, the
    /// two colours in turn along it.
    #[test]
    fn a_stick_striped() {
        let mut m = Model::default();
        // a square tube 0.06 across, 1.6 long, 4 faces of 2 triangles
        for y in [0.0f32, 1.6] {
            for (x, z) in [(-0.03, -0.03), (0.03, -0.03), (0.03, 0.03), (-0.03, 0.03)] {
                m.pos.push([x, y, z]);
                m.uv.push([0.0, y]);
            }
        }
        let mut idx = Vec::new();
        for k in 0..4u32 {
            let (a, b) = (k, (k + 1) % 4);
            idx.extend_from_slice(&[a, b, a + 4, b, b + 4, a + 4]);
        }
        let layer = esf::Layer { flags: 0, texture: 1234, wrap: 0, blend: 0, color: [128; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [9, 9, 9, 0] };
        m.batches.push(esf::Batch { material: 0, layers: vec![layer], indices: idx, lod: None });
        stripe(&mut m, None, 8);
        assert_eq!(m.batches.len(), 2);
        assert_eq!(m.batches[0].layers[0].color, CANDY_RED);
        assert_eq!(m.batches[1].layers[0].color, CANDY_WHITE);
        assert!(m.batches.iter().all(|b| b.layers[0].texture == 0 && b.layers[0].emissive == [0; 4]));
        let tris: usize = m.batches.iter().map(|b| b.indices.len() / 3).sum();
        assert!(tris > 100, "cut fine: {tris}");
        assert_eq!(m.pos.len(), m.uv.len());
        // along one face's middle line, the colours alternate about 16 times
        let red: Vec<Vec3> = m.batches[0].indices.chunks_exact(3).map(|t| t.iter().map(|&i| Vec3::from(m.pos[i as usize])).sum::<Vec3>() / 3.0).collect();
        let reds_low = red.iter().filter(|p| p.y < 0.8).count();
        let reds_high = red.iter().filter(|p| p.y >= 0.8).count();
        assert!(reds_low > 0 && reds_high > 0, "red all along");
    }
}
