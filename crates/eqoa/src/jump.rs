//! A jump (the user, 2026-11-09: "lets add jump to the game"): the client's own,
//! as a GM's flight is; the game has none (`VIWalkControl` gives a body on land no
//! way up). On a server that offers it (go-eqoa-server's `world.jump`, opcode
//! 0x7E28: `net::session::OP_JUMP`, `Session::jumps`) the jump key (Space, and V,
//! its key before; a row of the Controls page, `binds`) asks the walk for one (`Play::jump_key`,
//! `Walker::ask_jump`), which `walker.rs` gives on its console frames: a start
//! speed up and then CLIENT's own fall and landing, 0.9 high, 0.6 s on flat ground.
//! Never on a server that does not offer it: Sandstorm's and eqoa.live's checks
//! of a player's moves would read the rise as a body held up by nothing, as
//! go-eqoa-server's does for a client it never offered the jump. A jump waits for
//! a report standing to have gone out since the body last landed or was moved
//! (`controls::Client::stood_sent`): the server judges a jump from one.
//!
//! What others see. CLIENT puts every other creature on the floor within 1.5 of
//! it, at each update and each frame (`VICreatureEntity::Sync` and
//! `ExtrapolateMove`, both through `ExtrapolateGround`'s cast 1.5 up and 1.5 down),
//! unless the server gives it a walk of its own (`m_p255`, `VICreatureEntity::Walk`),
//! which go-eqoa-server never does for a player (channel byte 141 is 0): so a jump's
//! 0.9 never shows on a PS2, which draws the jumper on the ground playing the fall
//! (the channel's gait, byte 58, 6) and then standing again; and this client, which
//! ports both, drew it the same. On a server that offers jumps this client draws a
//! player in the air (`aloft`: its gait 6) at the height the server sends, carried
//! along its velocity between updates as `ExtrapolateMove` carries it, its speed up
//! or down under the walk's gravity (the 0x40's box tops out at 3 up, and a speed
//! held for a whole update overshot the top by 0.3), and stands it on a floor only
//! once it comes down to one (`ground`). Anywhere else, and every NPC, as CLIENT
//! does.
//!
//! The fall from the top (the user, 2026-11-09: "the jump needs a falling
//! animation once you hit the zenith"). The jumper's own walk gives the body
//! the fall's gait (6) from its first frame in the air and the landing's (7) as
//! it comes down (`UpdateSprite`), and `SetAnimation` blends each in over 250
//! ms: the fall is wholly in 0.27 s after leaving the ground, the top 0.29 s,
//! and it plays on to the landing. Another player's jump is drawn from its
//! first update in the air, which over a network comes 0.1 to 0.3 s after it
//! left the ground (its 0x40 goes every 0.1 s, then the server's turn, then the
//! wire), near its top or past it: the fall started then and was half in when
//! it came down, so its rise and top were drawn standing or running, and no
//! landing played at all (its gait is never 7). Now (`Air`, by slot) its fall
//! is wholly in by the top of the arc as drawn, or within `LATE_MS` where that
//! is past (`fall_by_top`), and once it comes down to a floor the landing
//! plays, once, as the jumper's own does (`draw_air`); a player falling off a
//! ledge there is aloft too, and takes the same (its fall in within `LATE_MS`,
//! its landing as it lands, as its player sees them). The jumper's own fall
//! keeps the same rule (`Walker`'s `in_jump`), which changes nothing in a jump
//! that reaches its top, and brings the fall in by a top come early (a jump
//! into a ceiling).

use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::hsprite::AnimScene;
use vi::scene::Scene;
use vi::vect::Vect3;

/// The gait `UpdateLocomotion` gives a body with no floor and no liquid under it:
/// a jump's, a fall's.
pub const AIR_GAIT: u8 = 6;

/// A player in the air on a server with jumps (the module's doc): its channel's
/// gait is the air's and it is a player (`m_p256` 2, or 4 for a PvP foe as
/// go-eqoa-server shows one).
pub fn aloft(jumps: bool, c: &net::entity::Creature) -> bool {
    jumps && c.state == AIR_GAIT && matches!(c.p256, 2 | 4)
}

/// Whether an update of a player `aloft` keeps its blend from where it was drawn
/// though its speed came unchanged (`was`), where CLIENT drops it
/// (`ChannelEntity`: an unchanged speed is where its update says): the 0x40's box
/// stops a jump's speed up at 3, so the report it left the ground in and its first
/// in the air carry the same speed, and the jumper went 0.6 up in one frame.
pub fn keeps_its_blend(jumps: bool, c: &net::entity::Creature, was: [f32; 3]) -> bool {
    aloft(jumps, c) && c.velocity == was
}

/// `ExtrapolateGround` for a creature (`skel` its skeleton's type); for one
/// `aloft`, onto the floor only once it has come down to one (the cast finds a
/// floor at or over its feet), else left at its height, upright. Whether it
/// was stood on the floor (one not aloft always is).
pub fn ground<S: Scene<Ee>>(scene: &mut S, skel: i32, aloft: bool, pos: &mut Vect3<Ee>, hpr: &mut Vect3<Ee>) -> bool {
    if !aloft {
        vi::creature::extrapolate_ground(scene, -1, skel, None, pos, hpr);
        return true;
    }
    let (mut p, mut h) = (*pos, *hpr);
    vi::creature::extrapolate_ground(scene, -1, skel, None, &mut p, &mut h);
    if pos.y <= p.y {
        (*pos, *hpr) = (p, h);
        true
    } else {
        hpr.y = F::ZERO;
        hpr.z = F::ZERO;
        false
    }
}

/// The walk's gravity (`VIWalkControl::Gravity`).
const GRAVITY: f32 = 19.6;

/// `ExtrapolateMove`: `dt` seconds along `vel`, then onto the ground as `ground`
/// stands it. For one aloft, `vy` is its speed up or down (its update's, then
/// carried), which the walk's gravity slows the while, in place of `vel`'s; with
/// none, CLIENT's own, call for call. Whether it was stood on a floor
/// (`ground`).
#[allow(clippy::too_many_arguments)]
pub fn extrapolate_move<S: Scene<Ee>>(scene: &mut S, skel: i32, vy: Option<&mut F<Ee>>, vel: &Vect3<Ee>, dt: F<Ee>, pos: &mut Vect3<Ee>, hpr: &mut Vect3<Ee>) -> bool {
    let Some(vy) = vy else {
        vi::creature::extrapolate_move(scene, -1, skel, None, vel, dt, pos, hpr);
        return true;
    };
    let g = F::<Ee>::new(GRAVITY);
    pos.x += vel.x * dt;
    pos.y += *vy * dt - g * dt * dt * F::new(0.5);
    pos.z += vel.z * dt;
    *vy -= g * dt;
    ground(scene, skel, true, pos, hpr)
}

/// The landing's gait: what `UpdateSprite` gives a body the moment it comes
/// down to a floor (`m_parm118 & 0x10`), a clip played once.
pub const LANDING: i32 = 7;

/// The gaits of a body on a floor (`UpdateLocomotion`: stood, walked either
/// way, ran, turned, stepped aside); not the air's (6), not liquid's (12, 13).
const FLOOR_GAITS: [u8; 8] = [0, 1, 2, 3, 4, 5, 10, 11];

/// How soon a jumper's fall comes wholly in once the top of its arc is past
/// (ms): a blend, not a jump cut (`SetAnimation`'s is 250).
pub const LATE_MS: i64 = 100;

/// A player's jump as this client draws it (`Play::airs`, by slot), on a
/// server with jumps (the module's doc). CLIENT has none of this: a PS2
/// draws the jumper on the ground playing the fall.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Air {
    /// on the ground, or never aloft
    #[default]
    Ground,
    /// aloft (`aloft`) and not come down yet: its fall wholly in by the top
    /// of its arc (`fall_by_top`)
    Up,
    /// come down to a floor as drawn, or said so by its update (a gait of the
    /// floor): the landing due (`draw_air`)
    Down,
    /// the landing given; on the ground at its next update not aloft
    Landed,
}

impl Air {
    /// An update of the player: `aloft` or not (`aloft`), its gait `state`
    /// and its speed up `vy`. A rise in the air after its landing is a jump
    /// again (the server's movement check allows no other).
    pub fn update(self, aloft: bool, state: u8, vy: f32) -> Air {
        match (self, aloft) {
            (Air::Ground, true) => Air::Up,
            (Air::Down | Air::Landed, true) if vy > 0.0 => Air::Up,
            (Air::Up, false) if FLOOR_GAITS.contains(&state) => Air::Down,
            (Air::Up, false) | (Air::Landed, false) => Air::Ground,
            (a, _) => a,
        }
    }

    /// As drawn: `stood` on a floor (`ground`) on its way down (`vy`, its
    /// speed up, none left).
    pub fn drawn(self, stood: bool, vy: F<Ee>) -> Air {
        if self == Air::Up && stood && vy <= F::ZERO { Air::Down } else { self }
    }
}

/// The fall (`AIR_GAIT`'s entry), where the body plays it and is still
/// blending it in (`SetAnimation`'s 250 ms), wholly in by the top of the arc:
/// the blend cut to end when the speed up `vy` has run out under the walk's
/// gravity, or within `LATE_MS` where that is past or nearer. A blend that
/// would end sooner is left as it is.
pub fn fall_by_top(cs: &mut CSprite<Ee>, vy: F<Ee>, sc: &AnimScene<Ee>) {
    let top_ms = (vy.0.max(0.0) / GRAVITY * 1000.0) as i64;
    let layers = if cs.skel_type == 1 { 2 } else { 1 };
    for l in &cs.cur[..layers] {
        if l.anim != AIR_GAIT as i32 || l.play < 0 {
            continue;
        }
        let p = cs.h.plays.get_mut(l.play);
        if p.blend_mode != 1 {
            continue;
        }
        let end = sc.time - p.blend_start + top_ms.max(LATE_MS);
        p.blend_duration = p.blend_duration.min(end);
    }
}

/// A player's jump on its body, after `SetLocomotion` of its update's gait:
/// up, its fall wholly in by the top (`fall_by_top`, `vy` its speed up as
/// drawn); come down, the landing once (`LANDING`, a clip no gait of the floor
/// outranks while it plays, as the player's own landing plays).
pub fn draw_air(air: &mut Air, vy: Option<F<Ee>>, cs: &mut CSprite<Ee>, sc: &AnimScene<Ee>, rand: &mut Rand) {
    match *air {
        Air::Up => {
            if let Some(vy) = vy {
                fall_by_top(cs, vy, sc);
            }
        }
        Air::Down => {
            cs.set_locomotion(LANDING, sc, rand);
            *air = Air::Landed;
        }
        _ => {}
    }
}

/// How far the play on a body's layer (the upper's, of a body with two) is
/// blended in, 0 to 1: the `[watch]` lines'.
pub fn blended(cs: &CSprite<Ee>, sc: &AnimScene<Ee>) -> f32 {
    let l = cs.cur[if cs.skel_type == 1 { 1 } else { 0 }];
    if l.play < 0 {
        return 0.0;
    }
    let p = cs.h.plays.get(l.play);
    if p.blend_mode != 1 || p.blend_duration <= 0 {
        return 1.0;
    }
    ((sc.time - p.blend_start) as f32 / p.blend_duration as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A jump as others draw it: up at its first update aloft, down at an
    /// update on a floor's gait or drawn stood on a floor on its way down,
    /// on the ground at the next update not aloft; a rise after its landing
    /// is a jump again. The controls: stood on a floor on its way up is not
    /// down, an update in liquid (a jump into water) plays no landing, and
    /// not aloft nothing happens at all.
    #[test]
    fn a_jump_drawn_goes_up_and_comes_down_once() {
        let f = F::<Ee>::new;
        let a = Air::Ground.update(true, AIR_GAIT, 2.95);
        assert_eq!(a, Air::Up);
        assert_eq!(a.drawn(false, f(-1.0)), Air::Up, "in the air");
        assert_eq!(a.drawn(true, f(1.0)), Air::Up, "the control: stood on its way up");
        let down = a.drawn(true, f(-0.5));
        assert_eq!(down, Air::Down, "drawn down to a floor");
        assert_eq!(down.update(true, AIR_GAIT, -2.0), Air::Down, "still aloft by its update, falling");
        assert_eq!(a.update(false, 0, 0.0), Air::Down, "its update: on a floor");
        assert_eq!(a.update(false, 3, 0.0), Air::Down, "running");
        assert_eq!(a.update(false, 12, 0.0), Air::Ground, "the control: into liquid");
        assert_eq!(Air::Landed.update(false, 0, 0.0), Air::Ground);
        assert_eq!(Air::Landed.update(true, AIR_GAIT, -1.0), Air::Landed, "its fall's tail");
        assert_eq!(Air::Landed.update(true, AIR_GAIT, 2.95), Air::Up, "a jump again");
        assert_eq!(Air::Ground.update(false, 3, 0.0), Air::Ground, "the control: not aloft");
    }

    /// The human body (HumanMale) off the disc, stood idle at 1000 ms; None
    /// without `run/eqoa.iso`.
    fn human() -> Option<(CSprite<Ee>, AnimScene<Ee>)> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", crate::stream::open_file(&iso, "CHAR.ESF").unwrap());
        let r = lib.lookup(crate::viewer::HUMAN).unwrap();
        let mut sc = AnimScene::<Ee>::default();
        let mut cs = vi::character::load_csprite(&lib, r, &mut sc, &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        sc.time = 1000;
        cs.set_locomotion(0, &sc, &mut Rand(1));
        cs.h.process(0, &sc);
        Some((cs, sc))
    }

    /// Another player's jump on the human body, frame by frame (33 ms) as
    /// `Play` draws it: stood, its first update aloft at 2000 ms (0.1 s after
    /// it left the ground, its speed up the 0x40's 2.95, the top of its arc
    /// as drawn 150 ms on), drawn down to the floor at 2396, its update on
    /// the floor at 2528. Its fall is wholly in by the top, and the landing
    /// plays once down, on both layers, through its clip (a third of a
    /// second), and then the idle. Its first update past the top (a network
    /// slower still): the fall wholly in `LATE_MS` on.
    /// The control: `SetLocomotion` alone, as before, has the fall a little
    /// over half in at the top (its 250 ms blend), under half in 100 ms after
    /// an update past it, and plays no landing.
    #[test]
    fn a_jumpers_fall_is_in_by_its_top_and_its_landing_plays() {
        if human().is_none() {
            return;
        }
        let run = |drawn: bool, first_vy: f32| -> Vec<(i64, i32, i32, f32)> {
            let (mut cs, mut sc) = human().unwrap();
            let mut rand = Rand(1);
            let (mut air, mut vy) = (Air::Ground, F::<Ee>::ZERO);
            let mut out = Vec::new();
            for k in -30..40i64 {
                let t = 2000 + 33 * k;
                sc.time = t;
                let aloft = (0..16).contains(&k);
                let state = if aloft { AIR_GAIT } else { 0 };
                if k == 0 {
                    vy = F::new(first_vy);
                } else if aloft {
                    vy -= F::new(GRAVITY * 0.033);
                }
                air = air.update(aloft, state, vy.0);
                if k >= 12 && aloft {
                    air = air.drawn(true, vy);
                }
                cs.set_locomotion(state as i32, &sc, &mut rand);
                if drawn {
                    draw_air(&mut air, aloft.then_some(vy), &mut cs, &sc, &mut rand);
                }
                cs.h.process(0, &sc);
                out.push((t, cs.cur[0].anim, cs.cur[1].anim, blended(&cs, &sc)));
            }
            out
        };
        let at = |v: &[(i64, i32, i32, f32)], t: i64| *v.iter().rev().find(|r| r.0 <= t).unwrap();
        let now = run(true, 2.95);
        let top = at(&now, 2165);
        assert_eq!((top.1, top.2), (6, 6), "the fall at the top");
        assert!(top.3 >= 1.0, "wholly in by the top: {top:?}");
        assert!(now.iter().filter(|r| (2165..2396).contains(&r.0)).all(|r| r.1 == 6 && r.2 == 6 && r.3 >= 1.0), "and on down: {now:?}");
        let landed: Vec<_> = now.iter().filter(|r| r.1 == LANDING && r.2 == LANDING).map(|r| r.0).collect();
        assert!(landed.first() == Some(&2396) && landed.last().is_some_and(|&l| l >= 2396 + 300), "the landing from where it came down, played through: {landed:?}");
        assert_eq!((at(&now, 3290).1, at(&now, 3290).2), (0, 0), "then the idle");
        let then = run(false, 2.95);
        let top = at(&then, 2165);
        assert!(top.3 < 0.7, "the control: {top:?}");
        assert!(then.iter().all(|r| r.1 != LANDING), "the control: no landing");
        // first seen past its top
        let late = at(&run(true, -0.3), 2000 + LATE_MS + 33);
        assert!((late.1, late.2, late.3) == (6, 6, 1.0), "past the top: {late:?}");
        let late = at(&run(false, -0.3), 2000 + LATE_MS + 33);
        assert!(late.3 < 0.6, "the control, past the top: {late:?}");
    }

    /// The jumper's own fall (`Walker`'s): in a jump that reaches its top the
    /// blend `SetLocomotion` gave the fall is left as it was (its speed up at
    /// the first frame in the air 5.67, its top 289 ms on, past the blend's
    /// 250); with the top come early (a ceiling: no speed up left) it ends
    /// within `LATE_MS`; a blend that would end sooner is not lengthened, and
    /// a body not playing the fall is not touched.
    #[test]
    fn the_own_fall_is_left_as_it_was_in_a_jump_that_reaches_its_top() {
        let Some((mut cs, mut sc)) = human() else { return };
        sc.time = 2016;
        cs.set_locomotion(6, &sc, &mut Rand(1));
        let durations = |cs: &CSprite<Ee>| [0, 1].map(|l| cs.h.plays.get(cs.cur[l].play).blend_duration);
        assert_eq!(durations(&cs), [250, 250]);
        fall_by_top(&mut cs, F::new(5.67), &sc);
        assert_eq!(durations(&cs), [250, 250], "a jump that reaches its top");
        sc.time = 2116;
        fall_by_top(&mut cs, F::ZERO, &sc);
        assert_eq!(durations(&cs), [100 + LATE_MS; 2], "a ceiling 100 ms in");
        fall_by_top(&mut cs, F::new(7.0), &sc);
        assert_eq!(durations(&cs), [100 + LATE_MS; 2], "never lengthened");
        let (mut idle, _) = human().unwrap();
        let before = durations(&idle);
        fall_by_top(&mut idle, F::ZERO, &sc);
        assert_eq!(durations(&idle), before, "not playing the fall");
    }

    fn creature(state: u8, p256: u8) -> net::entity::Creature {
        net::entity::Creature { state, p256, ..Default::default() }
    }

    /// Aloft is a player (or a PvP foe) whose gait is the air's, on a server
    /// with jumps; no NPC, no pet, no other gait, nothing on another server. One
    /// aloft keeps its blend when its speed came unchanged, where CLIENT drops
    /// it; the controls: a speed that changed (the blend kept anyway, by
    /// CLIENT's own rule), and one not aloft (CLIENT's).
    #[test]
    fn aloft_is_a_player_in_the_air_on_a_server_with_jumps() {
        assert!(aloft(true, &creature(AIR_GAIT, 2)));
        assert!(aloft(true, &creature(AIR_GAIT, 4)));
        for (why, jumps, c) in [("an NPC", true, creature(6, 0)), ("a pet", true, creature(6, 3)), ("running", true, creature(3, 2)), ("standing", true, creature(0, 2)), ("no jumps here", false, creature(6, 2))] {
            assert!(!aloft(jumps, &c), "{why}");
        }
        let up = |state: u8| net::entity::Creature { velocity: [0.0, 2.95, 0.0], ..creature(state, 2) };
        assert!(keeps_its_blend(true, &up(6), [0.0, 2.95, 0.0]), "aloft, its speed unchanged");
        assert!(!keeps_its_blend(true, &up(6), [0.0, 1.0, 0.0]), "its speed changed: CLIENT's own keeps it");
        assert!(!keeps_its_blend(true, &up(0), [0.0, 2.95, 0.0]), "standing: CLIENT's");
        assert!(!keeps_its_blend(false, &up(6), [0.0, 2.95, 0.0]), "no jumps here: CLIENT's");
    }

    /// A player aloft is drawn at the height the server sends, 0.8 over the
    /// open ground, upright; carried along its velocity; and once it has come
    /// down under the floor (sunk 0.3, or fallen past it) stood on the floor,
    /// where CLIENT stands it. The control: not aloft, CLIENT's own puts it on
    /// the floor from 0.8 up, and `extrapolate_move` is CLIENT's bit for bit.
    #[test]
    fn a_player_aloft_is_drawn_at_its_height_and_stood_on_the_floor_once_down() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let Some(mut world) = crate::walker::tests::tunaria_round(x, z) else { return };
        let floor = crate::walker::ground(&mut world, x, z).unwrap();
        let on_floor = {
            let (mut p, mut h) = (Vect3::of(x, floor + 0.8, z), Vect3::of(1.0, 0.3, 0.2));
            vi::creature::extrapolate_ground(&mut world, -1, 0, None, &mut p, &mut h);
            p
        };
        assert!((on_floor.y.0 - floor).abs() < 0.2, "the control: CLIENT stands it at {:?} on {floor}", on_floor.y);

        let (mut p, mut h) = (Vect3::of(x, floor + 0.8, z), Vect3::of(1.0, 0.3, 0.2));
        ground(&mut world, 0, true, &mut p, &mut h);
        assert_eq!((p.y.0, h.x.0, h.y.0, h.z.0), (floor + 0.8, 1.0, 0.0, 0.0), "aloft at its height, upright");
        let (mut q, mut hq) = (Vect3::of(x, floor + 0.8, z), Vect3::of(1.0, 0.3, 0.2));
        ground(&mut world, 0, false, &mut q, &mut hq);
        assert_eq!(q.y.bits(), on_floor.y.bits(), "the control: not aloft, CLIENT's");

        let (mut p, mut h) = (Vect3::of(x, floor - 0.3, z), Vect3::default());
        ground(&mut world, 0, true, &mut p, &mut h);
        assert_eq!(p.y.bits(), on_floor.y.bits(), "sunk under the floor: stood on it");

        // carried across, rising at its speed up, which gravity slows: at 2.94 up
        // (its update's) it tops out 0.22 over where it was, 0.15 s on
        let (mut p, mut h) = (Vect3::of(x, floor + 0.5, z), Vect3::default());
        let mut vy = F::new(2.94);
        let (mut top, mut at) = (p.y.0, 0);
        for k in 1..=10 {
            extrapolate_move(&mut world, 0, Some(&mut vy), &Vect3::of(2.0, 2.94, 0.0), F::new(0.03), &mut p, &mut h);
            if p.y.0 > top {
                (top, at) = (p.y.0, k);
            }
        }
        // (ten adds of 0.06 at x 22000, each chopped to the float's step, 0.002)
        assert!((p.x.0 - (x + 0.6)).abs() < 0.03, "carried across: {:?}", p.to_array());
        assert!((top - (floor + 0.5) - 0.22).abs() < 0.01 && at == 5, "its top {} over where it was, at step {at}", top - floor - 0.5);
        assert!(vy.0 < -2.9, "its speed after 0.3 s: {:?}", vy);
        // the control: not aloft, CLIENT's own stands it on the floor
        let (mut q, mut hq) = (Vect3::of(x, floor + 0.5, z), Vect3::default());
        for _ in 1..=5 {
            extrapolate_move(&mut world, 0, None, &Vect3::of(0.0, 2.94, 0.0), F::new(0.03), &mut q, &mut hq);
        }
        assert!(q.y.0 < floor + 0.2, "the control: not aloft, on the floor at {:?}", q.y);
        // falling, a frame at a time (60 a second): past the floor, on it, and
        // kept there
        let mut vy = F::new(-3.0);
        for _ in 0..30 {
            extrapolate_move(&mut world, 0, Some(&mut vy), &Vect3::of(0.0, -3.0, 0.0), F::new(1.0 / 60.0), &mut p, &mut h);
        }
        assert!((p.y.0 - floor).abs() < 0.2, "fallen past the floor: on it at {:?}", p.y);

        let (vel, dt) = (Vect3::of(1.5, 0.4, -2.0), F::new(0.07));
        let (mut a, mut ha) = (Vect3::of(x, floor + 0.8, z), Vect3::of(0.5, 0.0, 0.0));
        let (mut b, mut hb) = (a, ha);
        extrapolate_move(&mut world, 0, None, &vel, dt, &mut a, &mut ha);
        vi::creature::extrapolate_move(&mut world, -1, 0, None, &vel, dt, &mut b, &mut hb);
        assert_eq!([a.x.bits(), a.y.bits(), a.z.bits(), ha.y.bits()], [b.x.bits(), b.y.bits(), b.z.bits(), hb.y.bits()], "not aloft: CLIENT's own");
    }
}
