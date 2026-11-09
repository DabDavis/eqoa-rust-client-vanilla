//! A jump (the user, 2026-11-09: "lets add jump to the game"): the client's own,
//! as a GM's flight is; the game has none (`VIWalkControl` gives a body on land no
//! way up). On a server that offers it (go-eqoa-server's `world.jump`, opcode
//! 0x7E28: `net::session::OP_JUMP`, `Session::jumps`) the jump key (V, a row of the
//! Controls page, `binds`) asks the walk for one (`Play::jump_key`,
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
//! along its velocity between updates as `ExtrapolateMove` carries it, and stands it
//! on a floor only once it comes down to one (`ground`). Anywhere else, and every
//! NPC, as CLIENT does.

use vi::fpu::{Ee, F};
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

/// `ExtrapolateGround` for a creature (`skel` its skeleton's type); for one
/// `aloft`, onto the floor only once it has come down to one (the cast finds a
/// floor at or over its feet), else left at its height, upright.
pub fn ground<S: Scene<Ee>>(scene: &mut S, skel: i32, aloft: bool, pos: &mut Vect3<Ee>, hpr: &mut Vect3<Ee>) {
    if !aloft {
        vi::creature::extrapolate_ground(scene, -1, skel, None, pos, hpr);
        return;
    }
    let (mut p, mut h) = (*pos, *hpr);
    vi::creature::extrapolate_ground(scene, -1, skel, None, &mut p, &mut h);
    if pos.y <= p.y {
        (*pos, *hpr) = (p, h);
    } else {
        hpr.y = F::ZERO;
        hpr.z = F::ZERO;
    }
}

/// `ExtrapolateMove`: `dt` seconds along `vel`, then onto the ground as `ground`
/// stands it (for one not `aloft`, CLIENT's own, call for call).
#[allow(clippy::too_many_arguments)]
pub fn extrapolate_move<S: Scene<Ee>>(scene: &mut S, skel: i32, aloft: bool, vel: &Vect3<Ee>, dt: F<Ee>, pos: &mut Vect3<Ee>, hpr: &mut Vect3<Ee>) {
    if !aloft {
        vi::creature::extrapolate_move(scene, -1, skel, None, vel, dt, pos, hpr);
        return;
    }
    pos.x += vel.x * dt;
    pos.y += vel.y * dt;
    pos.z += vel.z * dt;
    ground(scene, skel, true, pos, hpr);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature(state: u8, p256: u8) -> net::entity::Creature {
        net::entity::Creature { state, p256, ..Default::default() }
    }

    /// Aloft is a player (or a PvP foe) whose gait is the air's, on a server
    /// with jumps; no NPC, no pet, no other gait, nothing on another server.
    #[test]
    fn aloft_is_a_player_in_the_air_on_a_server_with_jumps() {
        assert!(aloft(true, &creature(AIR_GAIT, 2)));
        assert!(aloft(true, &creature(AIR_GAIT, 4)));
        for (why, jumps, c) in [("an NPC", true, creature(6, 0)), ("a pet", true, creature(6, 3)), ("running", true, creature(3, 2)), ("standing", true, creature(0, 2)), ("no jumps here", false, creature(6, 2))] {
            assert!(!aloft(jumps, &c), "{why}");
        }
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

        let (mut p, mut h) = (Vect3::of(x, floor + 0.8, z), Vect3::default());
        extrapolate_move(&mut world, 0, true, &Vect3::of(2.0, 0.0, 0.0), F::new(0.1), &mut p, &mut h);
        assert!((p.x.0 - (x + 0.2)).abs() < 0.01 && p.y.0 == floor + 0.8, "carried across at its height: {:?}", p.to_array());
        extrapolate_move(&mut world, 0, true, &Vect3::of(0.0, -3.0, 0.0), F::new(0.5), &mut p, &mut h);
        assert!((p.y.0 - floor).abs() < 0.2, "fallen past the floor: on it at {:?}", p.y);

        let (vel, dt) = (Vect3::of(1.5, 0.4, -2.0), F::new(0.07));
        let (mut a, mut ha) = (Vect3::of(x, floor + 0.8, z), Vect3::of(0.5, 0.0, 0.0));
        let (mut b, mut hb) = (a, ha);
        extrapolate_move(&mut world, 0, false, &vel, dt, &mut a, &mut ha);
        vi::creature::extrapolate_move(&mut world, -1, 0, None, &vel, dt, &mut b, &mut hb);
        assert_eq!([a.x.bits(), a.y.bits(), a.z.bits(), ha.y.bits()], [b.x.bits(), b.y.bits(), b.z.bits(), hb.y.bits()], "not aloft: CLIENT's own");
    }
}
