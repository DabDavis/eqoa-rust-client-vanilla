//! Weapon trails in play: `VICSprite::ProcessWeaponTrails` over
//! [`vi::trail::Trails`]. A creature swinging (its layer-0 animation one of
//! the attack animations, which also names the hand) gets a trail of 25
//! segments living 0.4 s, begun with each new action; every frame a segment
//! goes in between the weapon's two point sprites (0x2D00, type 0 and 1, the
//! members of the item's group sprite), placed on the hand's node under the
//! centre transform. The trail ends when the action changes, and fades out.

use esf::Library;
use render::billboard::Strips;
use vi::csprite::CSprite;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::particle::Color;
use vi::trail::Trails;
use vi::vect::Vect3;

/// A weapon's two ends: the member transforms (in the item's space) of its
/// type-0 and type-1 point sprites, in the group's member order.
#[derive(Clone, Copy, Debug)]
pub struct Points {
    pub a: Matrix44<Ee>,
    pub b: Matrix44<Ee>,
}

/// The two point sprites of an item's group sprite (`CalcMemberTransform`:
/// `Transform(loc, scale, hpr)`, the member's record angles first).
pub fn weapon_points(lib: &Library, item: u32) -> Option<Points> {
    let r = lib.lookup(item as i32)?;
    let c = lib.file(r.file);
    if c.node(r.node).ty != 0x2C00 {
        return None;
    }
    let mem = c.child(r.node, 0x2C30)?;
    let d = &c.data;
    let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let i = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut p = c.node(mem).body;
    let n = i(p).clamp(0, 4096);
    p += 4;
    let (mut a, mut b) = (None, None);
    for _ in 0..n {
        if p + 32 > d.len() {
            break;
        }
        if let Some(m) = lib.lookup(i(p)) {
            let mc = lib.file(m.file);
            let node = mc.node(m.node);
            if node.ty == 0x2D00 && node.size >= 8 {
                let kind = i32::from_le_bytes(mc.data[node.body + 4..node.body + 8].try_into().unwrap());
                let t = Matrix44::<Ee>::transform(&Vect3::of(f(p + 4), f(p + 8), f(p + 12)), F::new(f(p + 16)), &Vect3::of(f(p + 20), f(p + 24), f(p + 28)));
                match kind {
                    0 if a.is_none() => a = Some(t),
                    1 if b.is_none() => b = Some(t),
                    _ => {}
                }
            }
        }
        p += 32;
    }
    Some(Points { a: a?, b: b? })
}

/// One creature's trail state (`m_trail`, `m_deathAnim` — the action the
/// trail began with).
#[derive(Clone, Copy, Debug)]
pub struct State {
    pub trail: Option<usize>,
    pub action: i32,
}

impl Default for State {
    fn default() -> Self {
        State { trail: None, action: -1 }
    }
}

/// `ProcessWeaponTrails`, with the trail slot on (the constructor's) and
/// its grey: `weapons` are the three hands' points, `centre` the frame's
/// centre transform.
pub fn process(trails: &mut Trails<Ee>, st: &mut State, cs: &CSprite<Ee>, weapons: &[Option<(i32, Points)>; 3], centre: &Matrix44<Ee>, now: i64) {
    if let Some(t) = st.trail
        && st.action != -1
        && st.action != cs.action
    {
        trails.end(t);
        st.trail = None;
    }
    let slot = match cs.cur[0].anim {
        23 | 25 | 26 | 28 | 29 | 41 | 42 | 44 => 0,
        22 | 24 | 27 | 40 => 1,
        30 => 2,
        _ => return,
    };
    if cs.action != st.action || cs.action == -1 {
        st.trail = trails.begin(now, 25, F::new(0.4));
        if st.trail.is_none() {
            return;
        }
        st.action = cs.action;
    }
    let (Some(t), Some((node, w))) = (st.trail, weapons[slot]) else { return };
    let Some(n) = cs.h.nodes.get(node.max(0) as usize).filter(|_| node >= 0) else { return };
    let at = |m: &Matrix44<Ee>| {
        let w = Matrix44::mul(&Matrix44::mul(m, &n.world), centre);
        Vect3::new(w.m[3].x, w.m[3].y, w.m[3].z)
    };
    let grey = Color { r: F::new(0.25), g: F::new(0.25), b: F::new(0.25), a: F::ONE };
    let black = Color { r: F::ZERO, g: F::ZERO, b: F::ZERO, a: F::ONE };
    trails.trails[t].add_segment(at(&w.a), at(&w.b), grey, black);
}

/// `VITrailFxRender`'s strips, as the renderer takes them (the trail
/// material: no texture, blend 1, Z test, no Z write).
pub fn render(trails: &mut Trails<Ee>, now: i64) -> Vec<Strips> {
    trails
        .render(now)
        .into_iter()
        .filter(|(_, strips)| !strips.is_empty())
        .map(|(_, strips)| Strips {
            texture: 0,
            blend: 1,
            ztest: true,
            zwrite: false,
            color: [1.0; 4],
            strips: strips.iter().map(|s| s.iter().map(|(_, v)| ([0.0, 0.0], v.to_array())).collect()).collect(),
            colors: strips.iter().map(|s| s.iter().map(|(c, _)| [c.r.0, c.g.0, c.b.0, c.a.0]).collect()).collect(),
        })
        .collect()
}
