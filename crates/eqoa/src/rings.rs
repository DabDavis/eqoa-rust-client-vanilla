//! The selection rings round the target (`VIClient::TrackTarget`, called by
//! `UpdateGame` after the scene is drawn while there is a target with an
//! actor and the HUD is up; without one, `UpdateGame` resets their state).
//!
//! They are UI.CSF's "Selection_Rings", an HSprite of two quads on two spinning
//! nodes: the first (material 0, two additive layers of the ring texture) takes
//! the con colour, the second (material 1, a white glow) only flashes as a
//! target is taken. Each frame, unless the player is the target in first
//! person (or with the camera inside their head), or the target is off the
//! screen: the rings go at the target's slot 4 (a character) or its position,
//! pulled toward the camera to at most 9 away (at least 2), facing the camera
//! (the view's rotation transposed); `m_pC150` falls from 1.2 to 0.2 at 2.5 a
//! second; every layer of the palette is set additive, its alpha from that
//! clock, the first material's colour eased toward the con colour
//! (`sColors[m_p2BB70]`, once a LAYER while the target info is valid), and
//! the Z test off; then the sprite is rastered twice, as a ring growing to 2.2
//! while it fades, and as the ring growing to its own size, coming up to half
//! alpha in the con colour while the glow fades.
//!
//! The game logic runs on `vi`'s float model; what is drawn goes to the
//! renderer's overlay (`render::overlay`): world-space triangles over the scene.

use crate::walker::Walker;
use render::overlay::{Corner, Tri};
use vi::fpu::{Ee, F, Model, f, vimax};
use vi::geom::{Matrix44, Vect4};
use vi::hsprite::{AnimScene, HSprite};
use vi::vect::Vect3;

/// `TrackTarget`'s `sColors`: the con colours, by the target info's second
/// byte (`m_p2BB70`).
pub const CON_COLORS: [[u8; 3]; 6] = [[255, 0, 0], [255, 255, 0], [128, 128, 128], [0, 0, 255], [0, 190, 255], [0, 128, 0]];

/// `m_pC158` as the constructor and every reset leave it.
fn grey<M: Model>() -> [F<M>; 4] {
    [f(0.5), f(0.5), f(0.5), F::ZERO]
}

/// The scene particle system's frame delta, which `TrackTarget` reads
/// (`VIParticleSystem::Render`: the clock's step capped at 0.1 s, the mean of
/// the last eight, the history starting at a thirtieth).
#[derive(Clone, Debug)]
pub struct Delta<M: Model> {
    time: Option<i64>,
    history: [F<M>; 8],
}

impl<M: Model> Default for Delta<M> {
    fn default() -> Self {
        Delta { time: None, history: [F::ONE / f(30.0); 8] }
    }
}

impl<M: Model> Delta<M> {
    /// The delta for a frame at `now` ms.
    pub fn step(&mut self, now: i64) -> F<M> {
        let mut dt = F::<M>::from_int(now.wrapping_sub(self.time.unwrap_or(now)) as i32) * f(0.001);
        self.time = Some(now);
        dt = if f::<M>(0.1) < dt { f(0.1) } else { dt };
        let mut sum = F::ZERO;
        for h in &self.history[..7] {
            sum += *h;
        }
        sum = (sum + dt) * f(0.125);
        for i in 0..7 {
            self.history[7 - i] = self.history[6 - i];
        }
        self.history[0] = dt;
        sum
    }
}

/// The camera as `TrackTarget` reads it: `m_position` and `m_view`'s axes
/// (`VICamera::CalcAxes`).
#[derive(Clone, Copy, Debug)]
pub struct Look<M: Model> {
    pub position: Vect3<M>,
    pub right: Vect3<M>,
    pub up: Vect3<M>,
    pub fwd: Vect3<M>,
}

impl<M: Model> Look<M> {
    /// `CalcAxes(dir)`: forward normalised, right = forward × world up (the x
    /// axis when that degenerates), up = right × forward.
    pub fn new(position: Vect3<M>, dir: Vect3<M>) -> Self {
        let mut fwd = dir;
        fwd.normalize();
        let mut right = Vect3::cross(&fwd, &Vect3::of(0.0, 1.0, 0.0));
        if right.normalize() == 0 {
            right = Vect3::cross(&fwd, &Vect3::of(1.0, 0.0, 0.0));
            right.normalize();
        }
        let up = Vect3::cross(&right, &fwd);
        Look { position, right, up, fwd }
    }
}

/// One rastering of the rings: the scale on x and y, and each palette
/// layer's colour (material, layer) as the pass leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass<M: Model> {
    pub scale: F<M>,
    pub colors: Vec<Vec<[u8; 4]>>,
}

/// `m_pC144`, `m_pC150`, `m_pC154`, `m_pC158`, and the rings' palette as
/// `SetLayerColor` leaves it.
#[derive(Clone, Debug)]
pub struct Track<M: Model> {
    /// the target the rings were last put on (`u32::MAX`: none)
    pub id: u32,
    /// falls from 1.2 to 0.2 at 2.5 a second as a target is taken
    pub t: F<M>,
    /// zeroed with the others; nothing in CLIENT reads it
    pub p154: i32,
    /// the first material's colour, eased toward the con colour
    pub color: [F<M>; 4],
    /// the palette's layer colours, by material then layer
    pub layers: Vec<Vec<[u8; 4]>>,
    /// the passes the last of the game's frames made, which the frames
    /// between draw (the first material's second layer eased twice that
    /// frame, its first once: they cannot be made again without easing)
    last: Option<[Pass<M>; 2]>,
}

impl<M: Model> Track<M> {
    /// The constructor's (and `EnterGameLoad`'s -1) state over a palette.
    pub fn new(layers: Vec<Vec<[u8; 4]>>) -> Self {
        Track { id: u32::MAX, t: f(1.2), p154: 0, color: grey(), layers, last: None }
    }

    /// `UpdateGame` with nothing to track (no target, no actor, the HUD off).
    pub fn reset(&mut self) {
        self.id = u32::MAX;
        self.t = f(1.2);
        self.color = grey();
        self.p154 = 0;
        self.last = None;
    }

    /// `TrackTarget` for target `id`, its rings at `at`: the billboard (the
    /// view's rotation transposed, at the point pulled toward the camera) and
    /// the two passes, or None when nothing is drawn. `hide_self`: the player
    /// is the target and the view is first person (or the manual camera's, or
    /// inside the player's head). `on_screen`: `at` projects inside the
    /// screen. `con`: `m_p2BB70` while the target info is valid. `dt`: the
    /// particle system's frame delta. `step`: one of the game's frames
    /// (`frames.rs`); the clock and the colour's easing (a twentieth a call)
    /// move only then, and between them the rings are placed for the view
    /// with the passes as the last frame left them.
    #[allow(clippy::too_many_arguments)]
    pub fn track(&mut self, id: u32, hide_self: bool, at: &Vect3<M>, on_screen: bool, look: &Look<M>, con: Option<u8>, dt: F<M>, step: bool) -> Option<(Matrix44<M>, [Pass<M>; 2])> {
        if hide_self {
            self.t = f(0.2);
            self.color = grey();
            self.id = id;
            self.p154 = 0;
            self.last = None;
            return None;
        }
        if !on_screen {
            return None;
        }
        if id != self.id {
            self.t = f(1.2);
            self.color = grey();
            self.id = id;
            self.p154 = 0;
            self.last = None;
        }
        let cp = look.position;
        let mut d = Vect3::new(at.x - cp.x, at.y - cp.y, at.z - cp.z);
        let mut dist = d.magnitude();
        if f::<M>(10.0) < dist {
            dist = f(10.0);
        }
        dist -= F::ONE;
        dist = vimax(f(2.0), dist);
        d.normalize();
        d.x = d.x * dist + cp.x;
        d.y = d.y * dist + cp.y;
        d.z = d.z * dist + cp.z;
        let z = F::ZERO;
        let rot = Matrix44 {
            m: [
                Vect4::new(look.right.x, look.right.y, look.right.z, z),
                Vect4::new(look.up.x, look.up.y, look.up.z, z),
                Vect4::new(look.fwd.x, look.fwd.y, look.fwd.z, z),
                Vect4::new(d.x, d.y, d.z, F::ONE),
            ],
        };
        if !step && let Some(p) = &self.last {
            return Some((rot, p.clone()));
        }
        if step {
            self.t -= dt * f(2.5);
            self.t = vimax(f(0.2), self.t);
        }
        let t = self.t - f(0.2);
        let t2 = t * t;
        // the rings' alpha from the clock; the first material's colour eased
        // toward the con colour, once for each of its layers
        for (i, mat) in self.layers.iter_mut().enumerate() {
            for c in mat.iter_mut() {
                c[3] = byte(t2 * f(255.0));
                if i == 0 {
                    if let Some(w) = con.and_then(|k| CON_COLORS.get(k as usize)).filter(|_| step) {
                        for (k, &w) in w.iter().enumerate() {
                            let want = F::<M>::from_int(w as i32) / f(255.0);
                            self.color[k] += (want - self.color[k]) * f(0.05);
                        }
                    }
                    for (b, v) in c.iter_mut().zip(&self.color).take(3) {
                        *b = byte(*v * f(255.0));
                    }
                }
            }
        }
        let s = F::ONE - t2;
        let first = Pass { scale: s * f(2.2), colors: self.layers.clone() };
        for (i, mat) in self.layers.iter_mut().enumerate() {
            for c in mat.iter_mut() {
                c[3] = if i == 0 { byte((F::ONE - (self.t - f(0.2))) * f(128.0)) } else { byte((F::ONE - s) * f(255.0)) };
            }
        }
        let second = Pass { scale: F::ONE - (self.t - f(0.2)), colors: self.layers.clone() };
        if step {
            self.last = Some([first.clone(), second.clone()]);
        }
        Some((rot, [first, second]))
    }
}

/// `(int)v` stored in a `VIColor32`'s byte.
fn byte<M: Model>(v: F<M>) -> u8 {
    v.0 as i32 as u8
}

/// Whether `at` projects inside the screen through `view_proj` (the
/// renderer's, whose depth runs 0..1 where the PS2's runs -1..1): what
/// `TrackTarget` asks of the camera's `m_viewProj` before drawing, the x
/// range the window's own (as eqoa-port widens it on a wide screen).
pub fn on_screen(view_proj: glam::Mat4, at: [f32; 3]) -> bool {
    let h = view_proj * glam::Vec4::new(at[0], at[1], at[2], 1.0);
    if h.w == 0.0 {
        return false;
    }
    let inv = 1.0 / h.w;
    let (x, y, z) = (h.x * inv, h.y * inv, h.z * inv);
    x.abs() <= 1.0 && y.abs() <= 1.0 && (0.0..=1.0).contains(&z)
}

/// The target as `UpdateGame` finds it: its id, its actor's position, and
/// slot 4 when its sprite is a character's (`CalcSlotWorldTransform`).
#[derive(Clone, Copy, Debug)]
pub struct Aim {
    pub id: u32,
    pub pos: [f32; 3],
    pub slot4: Option<[f32; 3]>,
}

/// One attached quad of the sprite: its node and its mesh (palette indices
/// on its batches).
struct Part {
    node: i32,
    model: esf::Model,
}

struct Sprite {
    hs: HSprite<Ee>,
    anims: AnimScene<Ee>,
    parts: Vec<Part>,
}

impl Sprite {
    /// UI.CSF's "Selection_Rings" (`m_ringsIndex`), with its attachments'
    /// meshes; None until that file is in.
    fn load(lib: &esf::Library) -> Option<Self> {
        let r = lib.lookup(vi::hsprite::hash_ref_id("Selection_Rings"))?;
        let mut anims = AnimScene::default();
        let mut cache = vi::character::AnimCache::new();
        let (hs, attach) = vi::character::load_hsprite(lib, r, &mut anims, &mut cache)?;
        let parts: Vec<Part> = attach.iter().filter_map(|&(sid, node)| Some(Part { node, model: lib.build_model(lib.lookup(sid)?)? })).collect();
        (!parts.is_empty()).then_some(Sprite { hs, anims, parts })
    }

    /// The palette's layer colours by material index, out of the meshes'
    /// batches (every material the quads use).
    fn palette(&self) -> Vec<Vec<[u8; 4]>> {
        let mut out: Vec<Vec<[u8; 4]>> = Vec::new();
        for p in &self.parts {
            for b in &p.model.batches {
                let Ok(i) = usize::try_from(b.material) else { continue };
                if out.len() <= i {
                    out.resize(i + 1, Vec::new());
                }
                out[i] = b.layers.iter().map(|l| l.color).collect();
            }
        }
        out
    }
}

/// The rings' state across frames.
#[derive(Default)]
pub struct Rings {
    sprite: Option<Sprite>,
    /// how many files the library held when the sprite was last looked for
    looked: usize,
    track: Option<Track<Ee>>,
    delta: Delta<Ee>,
    /// `m_p2BB70`: the con colour of the last target info (the
    /// constructor's 2)
    pub con: Option<u8>,
}

impl Rings {
    /// The art swapped (F9, `era`): the rings made again out of the art now,
    /// as they are next drawn.
    pub fn art_changed(&mut self) {
        self.sprite = None;
        self.track = None;
        self.looked = usize::MAX;
    }

    /// `UpdateGame`'s part: `TrackTarget` for `aim` (None resets), the
    /// rings' triangles to draw over the scene through `view_proj`. `me` is
    /// the player's id; `info_valid` is `m_targetInfoValid`; `cam_inside` is
    /// `m_p00088` (the follow camera inside the player's head this frame);
    /// `modal` is `IsModal` (the manual camera, mode 38).
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, lib: &esf::Library, walker: &Walker, aim: Option<Aim>, me: Option<u32>, info_valid: bool, cam_inside: bool, modal: bool, view_proj: glam::Mat4, step: bool) -> Vec<Tri> {
        let now = walker.scene.time;
        // the particle system's delta, a frame of the game's at a time
        let dt = if step { self.delta.step(now) } else { F::ZERO };
        if self.sprite.is_none() && self.looked != lib.files.len() {
            self.looked = lib.files.len();
            self.sprite = Sprite::load(lib);
            if let Some(s) = &self.sprite {
                self.track = Some(Track::new(s.palette()));
            }
        }
        let (Some(sprite), Some(track)) = (self.sprite.as_mut(), self.track.as_mut()) else { return Vec::new() };
        let Some(aim) = aim else {
            track.reset();
            return Vec::new();
        };
        // TrackTarget's first test
        let hide_self = (modal || walker.cam.camera_mode == 0 || cam_inside) && Some(aim.id) == me;
        let at = aim.slot4.unwrap_or(aim.pos);
        let look = Look::new(walker.eye.position, walker.eye.direction);
        let con = if info_valid { Some(self.con.unwrap_or(2)) } else { None };
        let Some((rot, passes)) = track.track(aim.id, hide_self, &Vect3::of(at[0], at[1], at[2]), on_screen(view_proj, at), &look, con, dt, step) else { return Vec::new() };
        // VIHSprite::Raster, twice: the pose at the scene's time, the centre
        // transform, each attachment at its node
        sprite.anims.time = now;
        let mut tris = Vec::new();
        for (k, pass) in passes.iter().enumerate() {
            let mut sc = Matrix44::<Ee>::identity();
            sc.m[0].x = pass.scale;
            sc.m[1].y = pass.scale;
            let w = Matrix44::mul(&sc, &rot);
            if k == 0 {
                let mut rel = w;
                rel.m[3].x -= walker.eye.position.x;
                rel.m[3].y -= walker.eye.position.y;
                rel.m[3].z -= walker.eye.position.z;
                let lod = sprite.hs.calc_lod_level_index(&rel);
                sprite.hs.process(lod, &sprite.anims);
            }
            let centre = sprite.hs.calc_trans_about_center(&w, &sprite.anims);
            for part in &sprite.parts {
                let m = match usize::try_from(part.node).ok().and_then(|n| sprite.hs.nodes.get(n)) {
                    Some(n) => Matrix44::mul(&n.world, &centre),
                    None => centre,
                };
                tris.extend(part_tris(&part.model, &m, &pass.colors));
            }
        }
        tris
    }

    /// The colour the first material's layers are easing toward, and the
    /// clock (the window's title, a shot's last line, a report).
    pub fn describe(&self) -> String {
        match &self.track {
            Some(t) if t.id != u32::MAX => format!("rings on {:#x}, clock {:.2}, colour {:.2?}", t.id, t.t.0, t.color.map(|c| c.0)),
            Some(_) => "rings: none".into(),
            None => "rings: not loaded".into(),
        }
    }
}

/// One attachment's triangles for a pass: each batch's layers in order
/// (one pass a layer, as the strip program draws them), the quads' corners
/// through `m`, the layer's UV transform, and the colour the pass gave the
/// layer (additive: `SetLayerBlendMode(j, 1)` on every layer). The quads'
/// vertex colours are white, so the strip program's light saturates at 1
/// whatever the scene's lights, and the colour is the layer's.
fn part_tris(model: &esf::Model, m: &Matrix44<Ee>, colors: &[Vec<[u8; 4]>]) -> Vec<Tri> {
    let mut out = Vec::new();
    for b in &model.batches {
        let Some(cs) = usize::try_from(b.material).ok().and_then(|i| colors.get(i)) else { continue };
        for (j, l) in b.layers.iter().enumerate() {
            let Some(c) = cs.get(j) else { continue };
            let light = |i: usize| model.color.get(i).map_or(1.0, |v| v[0].min(v[1]).min(v[2]).min(1.0));
            let corner = |i: u32| {
                let i = i as usize;
                let p = model.pos[i];
                let w = m.mul_point(&Vect3::of(p[0], p[1], p[2]));
                let k = light(i);
                Corner { pos: w.to_array(), uv: l.apply_uv(model.uv[i]), color: [c[0] as f32 / 255.0 * k, c[1] as f32 / 255.0 * k, c[2] as f32 / 255.0 * k, c[3] as f32 / 255.0] }
            };
            for t in b.indices.as_chunks::<3>().0 {
                out.push(Tri { texture: l.texture, blend: 1, wrap: l.wrap, corners: [corner(t[0]), corner(t[1]), corner(t[2])] });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::fpu::Ieee;

    /// The rings' palette: material 0 two layers, material 1 one (UI.CSF's).
    fn palette() -> Vec<Vec<[u8; 4]>> {
        vec![vec![[0, 0, 255, 255], [0, 0, 255, 255]], vec![[255, 255, 255, 89]]]
    }

    fn look() -> Look<Ieee> {
        Look::new(Vect3::of(0.0, 0.0, 0.0), Vect3::of(0.0, 0.0, 1.0))
    }

    /// From a new target the rings scale in over 0.4 s: the first pass from
    /// nothing to 2.2 as it fades out, the second to its own size as the ring
    /// comes up to half alpha and the glow fades; the clock stops at 0.2.
    #[test]
    fn the_rings_scale_in_and_settle() {
        let mut t = Track::<Ieee>::new(palette());
        let at = Vect3::of(0.0, 0.0, 5.0);
        let (_, p) = t.track(7, false, &at, true, &look(), None, f(0.0), true).unwrap();
        assert_eq!((p[0].scale.0, p[0].colors[0][0][3], p[0].colors[1][0][3]), (0.0, 255, 255));
        assert_eq!((p[1].scale.0, p[1].colors[0][0][3], p[1].colors[1][0][3]), (0.0, 0, 255));
        // 0.2 s on: the clock at 0.7, t - 0.2 = 0.5, its square a quarter
        let (_, p) = t.track(7, false, &at, true, &look(), None, f(0.2), true).unwrap();
        assert!((t.t.0 - 0.7).abs() < 1e-6, "{:?}", t.t);
        assert!((p[0].scale.0 - 0.75 * 2.2).abs() < 1e-6, "{:?}", p[0].scale);
        assert_eq!((p[0].colors[0][0][3], p[0].colors[1][0][3]), (63, 63));
        assert!((p[1].scale.0 - 0.5).abs() < 1e-6);
        // 1.2 is not exact in single precision: 1 - (t - 0.2) is 0.49999994,
        // so the ring's alpha is 63, not 64
        assert_eq!((p[1].colors[0][0][3], p[1].colors[0][1][3], p[1].colors[1][0][3]), (63, 63, 63));
        // and settled: the ring at its size and half alpha, the rest gone
        let (_, p) = t.track(7, false, &at, true, &look(), None, f(1.0), true).unwrap();
        assert_eq!(t.t.0, 0.2);
        assert_eq!((p[0].scale.0, p[0].colors[0][0][3], p[0].colors[1][0][3]), (2.2, 0, 0));
        assert_eq!((p[1].scale.0, p[1].colors[0][0][3], p[1].colors[1][0][3]), (1.0, 128, 0));
        // control: the alpha is the square of the clock, not the clock (a
        // linear fade would give 127 at the midpoint)
        assert_ne!(byte::<Ieee>(f(0.5) * f(255.0)), 63);
    }

    /// The first material's colour eases a twentieth of the way to the con
    /// colour for EACH of its layers (the easing is inside the layer loop):
    /// the second layer takes two steps a frame, and only while the target
    /// info is valid. Its other materials keep their own colour.
    #[test]
    fn the_first_material_eases_once_a_layer() {
        let mut t = Track::<Ieee>::new(palette());
        let at = Vect3::of(0.0, 0.0, 5.0);
        let (_, p) = t.track(7, false, &at, true, &look(), None, f(0.0), true).unwrap();
        assert_eq!(p[0].colors[0][0][..3], [127, 127, 127], "grey until the info comes");
        let (_, p) = t.track(7, false, &at, true, &look(), Some(0), f(0.0), true).unwrap();
        // red: 0.5 -> 0.525 -> 0.54875; green and blue 0.5 -> 0.475 -> 0.45125
        let one = 0.5f32 + (1.0 - 0.5) * 0.05;
        let two = one + (1.0 - one) * 0.05;
        assert_eq!(p[0].colors[0][0][0], (one * 255.0) as i32 as u8);
        assert_eq!(p[0].colors[0][1][0], (two * 255.0) as i32 as u8);
        assert_eq!(p[0].colors[0][1][1], ((0.5f32 * 0.95 * 0.95) * 255.0) as i32 as u8);
        assert_eq!(p[1].colors[0][1][..3], p[0].colors[0][1][..3], "the second pass keeps the first's colour");
        assert_eq!(p[0].colors[1][0][..3], [255, 255, 255]);
        // control: one step a frame would leave the second layer at the first's
        assert_ne!((one * 255.0) as i32 as u8, (two * 255.0) as i32 as u8);
        // an index past the table eases nothing (the game reads past it)
        let before = t.color;
        t.track(7, false, &at, true, &look(), Some(9), f(0.0), true).unwrap();
        assert_eq!(t.color, before);
    }

    /// Stepped on the game's frames, a fifth of a second of a 165 Hz display
    /// eases the colour and runs the clock as at 30; between them the passes
    /// are the last frame's. Stepped every display frame (before 2026-10-20)
    /// the colour went five times as many steps.
    #[test]
    fn the_rings_keep_the_games_frames_at_any_rate() {
        let run = |hz: f64, gated: bool| {
            let mut t = Track::<Ieee>::new(palette());
            let mut d = Delta::<Ieee>::default();
            let mut clock = crate::frames::Frames::default();
            let at = Vect3::of(0.0, 0.0, 5.0);
            let mut last = None;
            for k in 0..(hz as i64 / 5) {
                let ms = (k as f64 * 1000.0 / hz) as i64;
                let step = clock.due(ms as f64) || !gated;
                let dt = if step { d.step(ms) } else { F::ZERO };
                let (_, p) = t.track(7, false, &at, true, &look(), Some(0), dt, step).unwrap();
                if !step {
                    assert_eq!(Some(&p), last.as_ref(), "between the game's frames, the last frame's passes");
                }
                last = Some(p);
            }
            (t.color[0].0, t.t.0)
        };
        let (red30, clock30) = run(30.0, true);
        let (red165, clock165) = run(165.0, true);
        assert!((red30 - red165).abs() < 0.01, "{red30} at 30 Hz, {red165} at 165");
        assert!((clock30 - clock165).abs() < 0.05, "{clock30} {clock165}");
        // control: stepped every frame, the colour all but there
        let (red, _) = run(165.0, false);
        assert!(red > red30 + 0.1, "{red} against {red30}");
    }

    /// The rings go along the line to the target, at its distance less one,
    /// but no further than 9 and no nearer than 2.
    #[test]
    fn the_rings_are_pulled_toward_the_camera() {
        let at_dist = |d: f32| {
            let mut t = Track::<Ieee>::new(palette());
            let (rot, _) = t.track(7, false, &Vect3::of(0.0, 0.0, d), true, &look(), None, f(0.0), true).unwrap();
            rot.m[3].z.0
        };
        assert_eq!(at_dist(50.0), 9.0);
        assert_eq!(at_dist(5.0), 4.0);
        assert_eq!(at_dist(1.5), 2.0, "past a target nearer than 3");
        // control: unclamped it would sit just short of the target
        assert_ne!(at_dist(50.0), 49.0);
    }

    /// The billboard is the view's rotation transposed: its rows are the
    /// camera's right, up and forward (`CalcAxes`), so the quads face it.
    #[test]
    fn the_rings_face_the_camera() {
        let l = Look::<Ieee>::new(Vect3::of(1.0, 2.0, 3.0), Vect3::of(1.0, 0.0, 0.0));
        let mut t = Track::<Ieee>::new(palette());
        let (rot, _) = t.track(7, false, &Vect3::of(11.0, 2.0, 3.0), true, &l, None, f(0.0), true).unwrap();
        // looking along +x: forward x, right = x × y = z, up = z × x = y
        assert_eq!(rot.m[0].xyz().to_array(), [0.0, 0.0, 1.0]);
        assert_eq!(rot.m[1].xyz().to_array(), [0.0, 1.0, 0.0]);
        assert_eq!(rot.m[2].xyz().to_array(), [1.0, 0.0, 0.0]);
        assert_eq!(rot.m[3].xyz().to_array(), [10.0, 2.0, 3.0]);
    }

    /// The player targeted in first person: nothing drawn, and the rings
    /// left settled on the player (grey), so a third-person view later shows
    /// them at once; off the screen nothing at all changes; a new target
    /// starts them over; without a target they reset.
    #[test]
    fn hidden_off_screen_new_and_none() {
        let mut t = Track::<Ieee>::new(palette());
        let at = Vect3::of(0.0, 0.0, 5.0);
        assert!(t.track(1, true, &at, true, &look(), Some(5), f(0.0), true).is_none());
        assert_eq!((t.id, t.t.0, t.color[0].0), (1, 0.2, 0.5));
        let (_, p) = t.track(1, false, &at, true, &look(), None, f(0.0), true).unwrap();
        assert_eq!(p[1].colors[0][0][3], 128, "settled at once");
        t.track(2, false, &at, true, &look(), None, f(0.1), true).unwrap();
        let clock = t.t;
        assert!(t.track(2, false, &at, false, &look(), Some(5), f(0.1), true).is_none());
        assert_eq!((t.id, t.t), (2, clock), "off the screen the clock stands still");
        assert!((clock.0 - 1.2).abs() < 1e-6 || clock.0 < 1.2);
        t.reset();
        assert_eq!((t.id, t.t.0, t.p154), (u32::MAX, 1.2, 0));
        // control: had the hidden branch drawn, the first call returns Some
        assert!(Track::<Ieee>::new(palette()).track(1, false, &at, true, &look(), None, f(0.0), true).is_some());
    }

    /// The renderer's depth runs 0..1: a point in front of the camera is on
    /// the screen, one behind it or off to the side is not.
    #[test]
    fn on_screen_through_the_view() {
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 16.0 / 9.0, 0.3, 6000.0);
        let view = glam::camera::rh::view::look_to_mat4(glam::Vec3::ZERO, glam::Vec3::Z, glam::Vec3::Y);
        let vp = proj * view;
        assert!(on_screen(vp, [0.0, 0.0, 10.0]));
        assert!(!on_screen(vp, [0.0, 0.0, -10.0]), "behind");
        assert!(!on_screen(vp, [100.0, 0.0, 10.0]), "to the side");
        // ten away, a 16:9 screen reaches 7.4 across, a 4:3 one 5.5
        assert!(on_screen(vp, [6.0, 0.0, 10.0]), "a wide screen's edge is on it");
        let narrow = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0) * view;
        assert!(!on_screen(narrow, [6.0, 0.0, 10.0]), "control: off a 4:3 one");
    }

    /// UI.CSF's "Selection_Rings" as `TrackTarget` finds it: two quads on
    /// two nodes, the first in material 0 (two layers), the second in
    /// material 1, white vertex colours; and its clip turns them in their own
    /// plane (z stays 0 under the nodes and the centre transform), so the
    /// billboard keeps them facing the camera. SKIPs without the disc.
    #[test]
    fn the_rings_sprite_off_the_disc() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("UI.CSF", esf::Container::open(disc.read("DATA2/UI.CSF").unwrap()).unwrap());
        let mut s = Sprite::load(&lib).expect("Selection_Rings in UI.CSF");
        assert_eq!(s.palette(), palette());
        assert_eq!(s.parts.iter().map(|p| (p.node, p.model.batches.len(), p.model.batches[0].material)).collect::<Vec<_>>(), vec![(1, 1, 0), (2, 1, 1)]);
        assert!(s.parts.iter().all(|p| p.model.color.iter().all(|c| *c == [1.0; 4])), "white: the strip light saturates");
        let mut angles = Vec::new();
        for ms in (0..3000).step_by(50) {
            s.anims.time = ms;
            s.hs.process(-1, &s.anims);
            let c = s.hs.calc_trans_about_center(&Matrix44::identity(), &s.anims);
            for (k, p) in s.parts.iter().enumerate() {
                let m = Matrix44::mul(&s.hs.nodes[p.node as usize].world, &c);
                for v in &p.model.pos {
                    let w = m.mul_point(&Vect3::of(v[0], v[1], v[2]));
                    assert!(w.z.0.abs() < 1e-4, "{ms} ms: {w:?}");
                }
                let w = m.mul_point(&Vect3::of(p.model.pos[0][0], p.model.pos[0][1], 0.0));
                if k == 0 {
                    angles.push(w.y.0.atan2(w.x.0));
                }
            }
        }
        // control: they do turn, so a still pose would not pass for the clip
        assert!(angles.iter().any(|a| (a - angles[0]).abs() > 0.1), "{angles:?}");
    }

    /// A pass's triangles: each layer of the batch's material over every
    /// triangle, the corners through the transform, the pass's colour for
    /// that layer (over 255) times the white light, additive whatever the
    /// layer said.
    #[test]
    fn a_pass_draws_each_layer_over_the_quad() {
        let layer = |tex: i32, blend: i32| esf::Layer { flags: 0, texture: tex, wrap: 0, blend, color: [0; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0], lod_bias: 0.8, uv_rate: [0.0; 2] };
        let model = esf::Model {
            pos: vec![[1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, 1.0, 0.0], [-1.0, -1.0, 0.0]],
            uv: vec![[1.0, 0.0], [1.0, 1.0], [0.0, 0.0], [0.0, 1.0]],
            normal: vec![[0.0, 0.0, 1.0]; 4],
            color: vec![[1.0; 4]; 4],
            batches: vec![esf::Batch { material: 0, layers: vec![layer(11, 3), layer(12, 0)], indices: vec![0, 2, 1, 1, 2, 3], lod: None }],
            ..Default::default()
        };
        let mut m = Matrix44::<Ee>::identity();
        m.m[3].x = f(10.0);
        let colors = vec![vec![[255, 0, 0, 128], [0, 255, 0, 255]]];
        let t = part_tris(&model, &m, &colors);
        assert_eq!(t.len(), 4, "two layers over two triangles");
        assert_eq!((t[0].texture, t[2].texture), (11, 12), "layer by layer");
        assert!(t.iter().all(|t| t.blend == 1));
        assert_eq!(t[0].corners[0].pos, [11.0, 1.0, 0.0]);
        assert_eq!(t[0].corners[0].color, [1.0, 0.0, 0.0, 128.0 / 255.0]);
        assert_eq!(t[2].corners[1].uv, [0.0, 0.0]);
        // a material the pass has no colours for draws nothing
        assert!(part_tris(&model, &m, &[]).is_empty());
    }

    /// The frame delta is the particle system's: capped at a tenth, averaged
    /// over eight frames starting from thirtieths.
    #[test]
    fn the_delta_is_the_particle_systems() {
        let mut d = Delta::<Ieee>::default();
        let first = d.step(1000);
        assert!((first.0 - (7.0 / 30.0) / 8.0).abs() < 1e-6, "{first:?}");
        let second = d.step(1500);
        assert!((second.0 - (6.0 / 30.0 + 0.1) / 8.0).abs() < 1e-6, "capped at a tenth: {second:?}");
    }
}
