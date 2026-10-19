//! The player's own state, channel 0x40: `VIClient::SendPosition` with
//! `WriteQuantized` (decomp `modules/client/src/viclient.cpp`) and the six
//! boxes it quantises against (`viclientutil.cpp`). 41 bytes: the world id,
//! position (3 bytes an axis), velocity and force (2), angles, angular
//! velocity and torque (1), the target heading (1), height over the floor
//! (2), then raw: walk flags, locomotion state, auto-attack, target.

use crate::fpu::{F, Model, f, vimax, vimin};
use crate::vect::Vect3;
use crate::walk::WalkPhysics;

const PI: f32 = std::f32::consts::PI;

pub const POS: ([f32; 3], [f32; 3]) = ([-4000.0, -1000.0, -4000.0], [32000.0, 1000.0, 32000.0]);
pub const VEL: ([f32; 3], [f32; 3]) = ([-15.3, -84.5, -15.3], [15.3, 3.0, 15.3]);
pub const ACCEL: ([f32; 3], [f32; 3]) = ([-62.6, -12.51, -62.52], [62.6, 12.51, 62.52]);
pub const HPR: ([f32; 3], [f32; 3]) = ([-3.1415927, -1.6, 0.0], [3.1415927, 1.6, 0.0]);
pub const AVEL: ([f32; 3], [f32; 3]) = ([-6.4, -0.8, 0.0], [6.4, 0.8, 0.0]);
pub const AACCEL: ([f32; 3], [f32; 3]) = ([-31.2, -3.9, 0.0], [31.2, 3.9, 0.0]);

/// `WriteQuantized`: `v` over [lo, hi] into `n` bytes, high byte first; an
/// empty range writes nothing.
pub fn write_quantized<M: Model>(out: &mut Vec<u8>, n: u32, v: F<M>, lo: f32, hi: f32) {
    let scale = 1u32 << (n * 8);
    let range = f::<M>(hi) - f(lo);
    if range == F::ZERO {
        return;
    }
    let t = vimin(range, vimax(F::ZERO, v - f(lo)));
    let mut q = (t * F::new(scale as f32) / range).0 as u32;
    if q > scale - 1 {
        q = scale - 1;
    }
    for i in 0..n {
        out.push((q >> ((n - i - 1) * 8)) as u8);
    }
}

fn write_vect<M: Model>(out: &mut Vec<u8>, v: &Vect3<M>, n: u32, b: ([f32; 3], [f32; 3])) {
    for (i, c) in [v.x, v.y, v.z].into_iter().enumerate() {
        write_quantized(out, n, c, b.0[i], b.1[i]);
    }
}

/// `WrapAngle`: into [-π, π).
pub fn wrap_angle<M: Model>(mut a: F<M>) -> F<M> {
    while a < f(-PI) {
        a += f(2.0 * PI);
    }
    while a >= f(PI) {
        a -= f(2.0 * PI);
    }
    a
}

/// `SendPosition`'s message for the committed walk state.
pub fn encode<M: Model>(world: u8, p: &WalkPhysics<M>, auto_attack: u8, target: u32) -> Vec<u8> {
    let mut out = vec![world];
    write_vect(&mut out, &p.position, 3, POS);
    write_vect(&mut out, &p.velocity, 2, VEL);
    write_vect(&mut out, &p.force, 2, ACCEL);
    let angles = Vect3::new(wrap_angle(p.angles.x), wrap_angle(p.angles.y), wrap_angle(p.angles.z));
    write_vect(&mut out, &angles, 1, HPR);
    write_vect(&mut out, &p.angular_vel, 1, AVEL);
    write_vect(&mut out, &p.torque, 1, AACCEL);
    write_quantized(&mut out, 1, wrap_angle(p.target_heading), -3.14159265, 3.14159265);
    write_quantized(&mut out, 2, p.position.y - p.floor, -2000.0, 2000.0);
    out.extend(p.flags.to_le_bytes());
    out.push(p.loco_state as u8);
    out.push(auto_attack);
    out.extend(target.to_le_bytes());
    out
}
