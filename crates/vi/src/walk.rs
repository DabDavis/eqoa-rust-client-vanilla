//! `VIWalkControl`: the character body (decomp `viwalkcontrol.cpp`, SUPPORT
//! 0x00484930..0x00487BB0). A line-by-line port: the operation order, the
//! constants and the quirks (an unused `powf`, a pitch stepped from the wrong
//! field, a target stored unwrapped) are the guest's, and the decomp's
//! comments say why each is there. Laid out as the EE lays it out (0x1A0).

use crate::coll::{CollSphere, EnvRay};
use crate::csprite::{CSprite, Rand};
use crate::hsprite::AnimScene;
use crate::fpu::{F, Model, clamp, f};
use crate::scene::{Filter, FilterKind, Scene};
use crate::vect::{Vect2, Vect3};

/// `VIWalkFlags`.
pub mod flags {
    pub const ON_FLOOR: u32 = 0x0001;
    pub const B1: u32 = 0x0002;
    pub const IN_WATER: u32 = 0x0004;
    pub const AT_WATER_SURFACE: u32 = 0x0008;
    pub const IN_LAVA: u32 = 0x0010;
    pub const AT_LAVA_SURFACE: u32 = 0x0020;
    pub const IN_SLIME: u32 = 0x0040;
    pub const AT_SLIME_SURFACE: u32 = 0x0080;
    pub const TRACK_HEADING: u32 = 0x0100;
    pub const NO_TURN: u32 = 0x0200;
    pub const LANDSCAPE: u32 = 0x0400;
    pub const IN_MUD: u32 = 0x0800;
    pub const AT_MUD_SURFACE: u32 = 0x1000;
    pub const SLOW_RUN: u32 = 0x2000;
    /// Submerged in something.
    pub const SUBMERGED: u32 = 0x0854;
    /// At the surface of something.
    pub const AT_SURFACE: u32 = 0x10A8;
    pub const LIQUID: u32 = SUBMERGED | AT_SURFACE;
}
use flags::*;

/// One state of the rigid body (0x70 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WalkPhysics<M: Model> {
    /// `VIScene::GetTime` of this state (an EE `long`).
    pub time: i64,
    pub position: Vect3<M>,
    /// (heading, pitch, roll).
    pub angles: Vect3<M>,
    pub velocity: Vect3<M>,
    pub angular_vel: Vect3<M>,
    pub force: Vect3<M>,
    pub torque: Vect3<M>,
    pub flags: u32,
    pub floor: F<M>,
    pub loco_state: i32,
    pub height_offset: F<M>,
    pub target_heading: F<M>,
    pub landscape_normal: Vect3<M>,
}

impl<M: Model> WalkPhysics<M> {
    fn in_mud(&self) -> bool {
        self.flags & (IN_MUD | AT_MUD_SURFACE) != 0
    }
    fn in_liquid(&self) -> bool {
        self.flags & LIQUID != 0
    }
    fn on_floor_or_liquid(&self) -> bool {
        self.flags & (ON_FLOOR | B1) != 0 || self.in_liquid()
    }

    /// The render position: the body's own, raised by its height offset.
    pub fn smooth_location(&self) -> Vect3<M> {
        let mut v = self.position;
        v.y += self.height_offset;
        v
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WalkControl<M: Model> {
    pub tick_ms: i32,
    pub tick_alt_ms: i32,
    pub speed_threshold: F<M>,
    pub walk_speed: F<M>,
    pub run_speed: F<M>,
    pub parm14: F<M>,
    pub turn_rate: F<M>,
    pub pitch_rate: F<M>,
    pub swim_speed: F<M>,
    /// Step-up threshold for the height-offset smoothing.
    pub parm24: F<M>,
    /// Step-down tolerance (a fall shorter than this stays on the floor).
    pub parm28: F<M>,
    pub parm2c: F<M>,
    /// The body's height: the floor probe's reach and the headroom needed.
    pub parm30: F<M>,
    /// The collision sphere's radius.
    pub parm34: F<M>,
    /// Eye/centre height above the feet.
    pub parm38: F<M>,
    /// The floor probe's radius.
    pub parm3c: F<M>,
    pub move_damping: F<M>,
    /// The AIR drag.
    pub parm44: F<M>,
    pub swim_damping: F<M>,
    pub turn_damping: F<M>,
    pub pitch_damping: F<M>,
    /// Half-height of the liquid-surface probe.
    pub parm54: F<M>,
    pub move_deadzone: F<M>,
    pub run_threshold: F<M>,
    /// Gravity on/off.
    pub parm60: i32,
    /// The actor the collides ignore (the character's own).
    pub parm64: i32,
    pub min_pitch: F<M>,
    pub max_pitch: F<M>,
    pub auto_pitch_rate: F<M>,
    /// Heading only, no pitch.
    pub forward: Vect3<M>,
    pub right: Vect3<M>,
    /// Heading and pitch.
    pub facing: Vect3<M>,
    /// NOT normalized.
    pub up: Vect3<M>,
    pub p_a4: F<M>,
    /// The state being stepped.
    pub phys: WalkPhysics<M>,
    pub parm118: i32,
    pub walk_accel: F<M>,
    pub run_accel: F<M>,
    pub turn_accel: F<M>,
    pub pitch_accel: F<M>,
    pub swim_accel: F<M>,
    /// The committed state.
    pub committed: WalkPhysics<M>,
}

const PI: f32 = 3.14159274;
const TWO_PI: f32 = 6.28318548;
const PITCH_LIMIT: f32 = 1.5607964;
/// `InitParms`' run speed.
pub const RUN_SPEED: f32 = 8.833330;

/// `VIWalkControl::Gravity` and `WGravity`.
fn gravity<M: Model>() -> Vect3<M> {
    Vect3::of(0.0, -19.6, 0.0)
}
fn w_gravity<M: Model>() -> Vect3<M> {
    Vect3::of(0.0, -1.0, 0.0)
}

impl<M: Model> WalkControl<M> {
    pub fn init<S: Scene<M>>(&mut self, pos: Vect3<M>, angles: Vect3<M>, scene: &mut S) -> i32 {
        self.init_parms();
        self.committed.flags = 1;
        self.phys.flags = 1;
        self.set_location(pos, angles, scene);
        0
    }

    pub fn set_location<S: Scene<M>>(&mut self, pos: Vect3<M>, angles: Vect3<M>, scene: &mut S) -> i32 {
        let keep = self.committed.flags & TRACK_HEADING;
        let c = &mut self.committed;
        c.time = scene.time();
        c.position = pos;
        c.velocity.zero();
        c.force.zero();
        c.angles = angles;
        c.angles.y = F::ZERO;
        c.angles.z = F::ZERO;
        c.angular_vel.zero();
        c.torque.zero();
        c.flags = 1;
        c.loco_state = 0;
        c.height_offset = F::ZERO;
        c.floor = pos.y;
        c.target_heading = c.angles.x;
        c.landscape_normal.set(F::ZERO, F::ONE, F::ZERO);
        c.flags |= keep;
        self.phys = self.committed;
        self.init_intermediate();
        0
    }

    pub fn set_update_physics<S: Scene<M>>(&mut self, p: &WalkPhysics<M>, scene: &mut S) -> i32 {
        self.committed = *p;
        self.committed.time = scene.time();
        self.phys = self.committed;
        self.init_intermediate();
        if self.committed.flags & 1 != 0 {
            let mut from = self.committed.position;
            from.y += self.parm38;
            let mut to = Vect3::default();
            if self.calc_floor(scene, &from, &mut to) < 0 {
                return -1;
            }
            self.committed.position = to;
            self.phys.position = self.committed.position;
        }
        0
    }

    /// Step the body to the scene's clock in fixed ticks. A gap longer than
    /// `tick_alt_ms` is skipped, not simulated.
    pub fn process<S: Scene<M>>(&mut self, flags: i32, scene: &mut S) -> i32 {
        let old = self.phys.flags;
        let mut dt = scene.time() - self.committed.time;
        let tick = if flags != 0 { self.tick_alt_ms } else { self.tick_ms } as i64;
        while dt > 0 {
            let step;
            if (self.tick_alt_ms as i64) < dt {
                step = dt - self.tick_alt_ms as i64;
                self.phys.time = self.committed.time + step;
            } else {
                step = if tick < dt { tick } else { dt };
                self.phys.time = self.committed.time + step;
                let t = F::new(step as i32 as f32) * f(0.001);
                self.update_physics(t);
                self.auto_pitch(t);
                self.move_through(flags, scene);
                self.update_directions();
                self.update_locomotion(t);
            }
            dt -= step;
            self.committed = self.phys;
        }
        if flags == 0 {
            self.clear_forces();
        }
        self.parm118 = 0;
        if old & 1 == 0 && self.phys.flags & 1 != 0 && self.phys.flags & SUBMERGED == 0 && old & SUBMERGED == 0 {
            self.parm118 = 16;
        }
        0
    }

    /// `UpdateSprite`: hand the body's state to the character -- its
    /// locomotion animation (7 for a moment after landing), and a 400 ms
    /// blend of the centre transform: pitched with the body when submerged,
    /// sunk by the height the sprite's scale leaves out at a surface.
    pub fn update_sprite(&self, sprite: &mut CSprite<M>, sc: &AnimScene<M>, rand: &mut Rand) -> i32 {
        let s = sprite.h.default_scale;
        let center = Vect3::new(F::ZERO, self.parm38 * s, F::ZERO);
        if self.parm118 & 0x10 == 0 {
            sprite.set_locomotion(self.committed.loco_state, sc, rand);
        } else {
            sprite.set_locomotion(7, sc, rand);
        }
        let (hpr, trans);
        if self.phys.flags & SUBMERGED != 0 && self.phys.flags & AT_SURFACE == 0 {
            hpr = Vect3::new(F::ZERO, self.phys.angles.y, F::ZERO);
            trans = Vect3::default();
        } else {
            trans = if self.phys.flags & AT_SURFACE != 0 {
                Vect3::new(F::ZERO, self.parm38 - self.parm38 * sprite.h.default_scale, F::ZERO)
            } else {
                Vect3::default()
            };
            hpr = Vect3::default();
        }
        sprite.h.blend_to_center_transform(center, &hpr, F::ONE, trans, 400, sc);
        0
    }

    pub fn clear_forces(&mut self) -> i32 {
        self.phys.force.zero();
        self.phys.torque.zero();
        0
    }

    pub fn is_under_water(&self) -> bool {
        self.phys.flags & IN_WATER != 0 && self.phys.flags & AT_WATER_SURFACE == 0
    }
    pub fn is_under_liquid(&self) -> bool {
        self.phys.flags & SUBMERGED != 0 && self.phys.flags & AT_SURFACE == 0
    }
    pub fn is_at_liquid_surface(&self) -> bool {
        self.phys.flags & AT_SURFACE != 0
    }

    /// 93 constant stores.
    pub fn init_parms(&mut self) -> i32 {
        self.tick_ms = 32;
        self.tick_alt_ms = 200;
        self.speed_threshold = f(0.5);
        self.walk_speed = f(3.0);
        self.run_speed = f(RUN_SPEED);
        self.parm14 = f(4.416665);
        self.turn_rate = f(1.5707964);
        self.pitch_rate = f(0.7853982);
        self.swim_speed = f(3.0);
        self.parm24 = f(0.3);
        self.parm28 = f(0.6);
        self.parm2c = f(1.0471976);
        self.parm30 = f(1.8);
        self.parm38 = f(1.2);
        self.parm34 = f(0.6);
        self.parm3c = f(0.001);
        self.move_damping = f(0.02);
        self.parm44 = f(0.8);
        self.swim_damping = f(0.02);
        self.turn_damping = f(0.01);
        self.pitch_damping = f(0.01);
        self.parm54 = f(0.1);
        self.move_deadzone = f(0.01);
        self.run_threshold = f(0.7);
        self.parm60 = 1;
        self.parm64 = -1;
        self.min_pitch = f(-PITCH_LIMIT);
        self.max_pitch = f(PITCH_LIMIT);
        self.auto_pitch_rate = f(0.5235988);
        0
    }

    /// Angular thrust about the heading axis (Torque.x). No flag gate.
    pub fn turn_left(&mut self, t: F<M>) -> i32 {
        let t = clamp(t, f(-1.0), F::ONE);
        let a = self.turn_accel;
        let axis = Vect3::<M>::of(1.0, 0.0, 0.0);
        let mut dir = Vect3::<M>::default();
        dir.x += a * axis.x;
        dir.y += a * axis.y;
        dir.z += a * axis.z;
        self.phys.torque.add_scaled(&dir, t);
        0
    }

    pub fn swim_up(&mut self, t: F<M>) -> i32 {
        if self.phys.in_liquid() {
            let t = clamp(t, f(-1.0), F::ONE);
            let k = self.swim_accel * t;
            let up = Vect3::of(0.0, 1.0, 0.0);
            self.phys.force.add_scaled(&up, k);
        }
        0
    }

    /// A pitch everywhere but at a liquid surface, where it is a dive; the two
    /// branches clamp to different ranges.
    pub fn pitch_down(&mut self, t: F<M>) -> i32 {
        let mut dir = Vect3::<M>::default();
        if self.committed.flags & AT_SURFACE == 0 {
            let t = clamp(t, f(-1.0), F::ONE);
            let axis = Vect3::of(0.0, 1.0, 0.0);
            dir.add_scaled(&axis, self.pitch_accel);
            self.phys.torque.add_scaled(&dir, t);
        } else {
            let t = clamp(t, F::ZERO, F::ONE);
            let axis = Vect3::of(0.0, -1.0, 0.0);
            dir.add_scaled(&axis, self.swim_accel);
            self.phys.force.add_scaled(&dir, t);
        }
        0
    }

    /// Forward = HPR(heading, 0), Facing = HPR(heading, pitch), Right =
    /// normalize(Forward × up), Up = Facing × Right (not normalized).
    pub fn update_directions(&mut self) -> i32 {
        let a = self.phys.angles;
        self.forward.hpr(a.x, F::ZERO, a.z);
        self.facing.hpr(a.x, a.y, a.z);
        let up = Vect3::of(0.0, 1.0, 0.0);
        self.right = Vect3::cross(&self.forward, &up);
        self.right.normalize();
        self.up = Vect3::cross(&self.facing, &self.right);
        0
    }

    pub fn set_pitch_range(&mut self, lo: F<M>, hi: F<M>) -> i32 {
        self.min_pitch = clamp(lo, f(-PITCH_LIMIT), f(PITCH_LIMIT));
        self.max_pitch = clamp(hi, f(-PITCH_LIMIT), f(PITCH_LIMIT));
        self.phys.angles.y = clamp(self.phys.angles.y, self.min_pitch, self.max_pitch);
        self.committed.angles.y = clamp(self.committed.angles.y, self.min_pitch, self.max_pitch);
        0
    }

    /// A bang-bang turn toward `h`: the RAW target is stored, the wrapped one
    /// steered to.
    pub fn set_target_heading(&mut self, mut h: F<M>) -> i32 {
        self.phys.target_heading = h;
        let d = h - self.phys.angles.x;
        if f(PI) < d {
            h = h - f(TWO_PI);
        } else if d < f(-PI) {
            h = h + f(TWO_PI);
        }
        if self.phys.angles.x < h {
            self.turn_left(F::ONE);
        }
        if h < self.phys.angles.x {
            self.turn_left(f(-1.0));
        }
        0
    }

    /// The step is taken from the COMMITTED pitch and stored into the working
    /// one (a guest quirk, reproduced).
    pub fn auto_pitch(&mut self, dt: F<M>) -> i32 {
        let target = if self.phys.flags & AT_SURFACE != 0 {
            f(-0.08726646)
        } else if self.phys.flags & SUBMERGED != 0 {
            self.phys.angles.y
        } else {
            clamp(self.phys.angles.y, self.min_pitch, self.max_pitch)
        };
        let delta = target - self.phys.angles.y;
        let step = self.auto_pitch_rate * dt;
        if F::ZERO <= delta {
            self.phys.angles.y = if delta <= step { target } else { self.committed.angles.y + step };
        } else {
            self.phys.angles.y = if step <= delta { target } else { self.committed.angles.y - step };
        }
        0
    }

    /// x forward, y strafe. Only on a floor or in a liquid.
    pub fn walk(&mut self, x: F<M>, y: F<M>) -> i32 {
        if self.phys.on_floor_or_liquid() {
            let mut v = Vect3::<M>::default();
            v.x = clamp(x, f(-1.0), F::ONE);
            v.y = clamp(y, f(-1.0), F::ONE);
            v.z = F::ZERO;
            if F::ONE < v.magnitude() {
                v.normalize();
            }
            let dir = if !self.phys.in_liquid() { self.forward } else { self.facing };
            self.phys.force.add_scaled(&dir, self.walk_accel * v.x);
            let right = self.right;
            self.phys.force.add_scaled(&right, self.walk_accel * v.y);
        }
        0
    }

    /// The stick: nothing under the deadzone; the run/walk choice is made on
    /// the raw magnitude, the thrust is a unit vector.
    pub fn move_stick(&mut self, x: F<M>, y: F<M>) -> i32 {
        let mut v = Vect2::new(x, y);
        let m = v.magnitude();
        if self.move_deadzone < m {
            v.normalize();
            if self.run_threshold < m {
                self.run(v.x, v.y);
            } else {
                self.walk(v.x, v.y);
            }
        }
        0
    }

    pub fn run(&mut self, x: F<M>, y: F<M>) -> i32 {
        if self.phys.on_floor_or_liquid() {
            let mut v = Vect3::<M>::default();
            v.x = clamp(x, f(-1.0), F::ONE);
            v.y = clamp(y, f(-1.0), F::ONE);
            v.z = F::ZERO;
            if F::ONE < v.magnitude() {
                v.normalize();
            }
            let (fwd, facing, right) = (self.forward, self.facing, self.right);
            if self.run_threshold < x {
                if !self.phys.in_liquid() {
                    let a = if self.phys.flags & SLOW_RUN == 0 { self.run_accel } else { self.walk_accel };
                    self.phys.force.add_scaled(&fwd, a * v.x);
                } else if !self.phys.in_mud() {
                    self.phys.force.add_scaled(&facing, self.run_accel * v.x);
                } else {
                    self.phys.force.add_scaled(&facing, self.walk_accel * v.x);
                }
                self.phys.force.add_scaled(&right, self.run_accel * v.y);
            } else {
                let dir = if !self.phys.in_liquid() { fwd } else { facing };
                self.phys.force.add_scaled(&dir, self.walk_accel * v.x);
                self.phys.force.add_scaled(&right, self.walk_accel * v.y);
            }
        }
        0
    }

    /// The five accelerations: `accel = (speed - speed*d) / (d*dt)`, d =
    /// damping^dt. `powf` is called five times and four results are used.
    pub fn init_intermediate(&mut self) -> i32 {
        let dt = F::new(self.tick_ms as f32) * f(0.001);
        let move_d = self.move_damping.powf(dt);
        let _ = self.parm44.powf(dt);
        let turn_d = self.turn_damping.powf(dt);
        let swim_d = self.swim_damping.powf(dt);
        let pitch_d = self.pitch_damping.powf(dt);
        self.walk_accel = (self.walk_speed - self.walk_speed * move_d) / (move_d * dt);
        self.run_accel = (self.run_speed - self.run_speed * move_d) / (move_d * dt);
        self.turn_accel = (self.turn_rate - self.turn_rate * turn_d) / (turn_d * dt);
        self.swim_accel = (self.swim_speed - self.swim_speed * swim_d) / (swim_d * dt);
        self.pitch_accel = (self.pitch_rate - self.pitch_rate * pitch_d) / (pitch_d * dt);
        self.update_directions();
        self.parm118 = 0;
        0
    }

    /// The lean that puts feet on a slope: `v.x` is a heading in, `v.y` a
    /// pitch and `v.z` a roll out. The asin is `atan2f(s, sqrtf(1 - s*s))`.
    pub fn calc_pitch_roll_to_landscape(&self, v: &mut Vect3<M>) -> i32 {
        if self.committed.flags & LANDSCAPE != 0 {
            let n = self.committed.landscape_normal;
            let mut fwd = Vect3::<M>::default();
            fwd.hpr(v.x, F::ZERO, F::ZERO);
            let mut r = Vect3::cross(&n, &fwd);
            r.normalize();
            fwd = Vect3::cross(&r, &n);
            v.y = -fwd.y.atan2f((F::ONE - fwd.y * fwd.y).sqrt());
            v.z = r.y.atan2f((F::ONE - r.y * r.y).sqrt());
        } else {
            v.y = F::ZERO;
            v.z = F::ZERO;
        }
        0
    }

    /// What the physics DID, classified: the position change projected on
    /// Forward and Right, divided by dt. The chain's order is the function.
    pub fn update_locomotion(&mut self, dt: F<M>) -> i32 {
        let mut d = self.phys.position;
        d -= self.committed.position;
        let v_f = d.dot(&self.forward) / dt;
        let v_r = d.dot(&self.right) / dt;
        let st = self.speed_threshold;
        let p = &self.phys;
        let c = &self.committed;
        let state = if !p.in_liquid() {
            if p.flags & 1 != 0 {
                let not_walking = c.loco_state != 1 && c.loco_state != 2;
                if self.walk_speed + (self.run_speed - self.walk_speed) * f(0.1) < v_f {
                    3
                } else if st < v_r && v_r.abs() > v_f.abs() && not_walking {
                    11
                } else if v_r < -st && v_r.abs() > v_f.abs() && not_walking {
                    10
                } else if st < v_r && v_f.abs() * f(1.5) < v_r.abs() {
                    11
                } else if v_r < -st && v_f.abs() * f(1.5) < v_r.abs() {
                    10
                } else if st < v_f {
                    1
                } else if v_f < -st {
                    2
                } else if self.turn_rate * f(0.5) < p.angular_vel.x {
                    4
                } else if p.angular_vel.x < -(self.turn_rate * f(0.5)) {
                    5
                } else {
                    0
                }
            } else {
                6
            }
        } else if p.angular_vel.y.abs() > self.turn_rate * f(0.25)
            || self.turn_rate * f(0.25) < p.angular_vel.x.abs()
            || st < v_f
            || st < v_r.abs()
        {
            12
        } else {
            13
        };
        self.phys.loco_state = state;
        0
    }

    /// The integrator, a damped semi-implicit Euler step:
    ///   d = powf(damping, dt); pos += force*(dt*dt*d) + vel*(dt*d);
    ///   vel = (vel + force*dt) * d
    /// with the linear damping chosen by medium and gravity added to a COPY of
    /// the force.
    pub fn update_physics(&mut self, dt: F<M>) -> i32 {
        let p = &mut self.phys;
        let c = &self.committed;
        p.height_offset = c.height_offset;
        if F::ZERO < c.height_offset {
            p.height_offset = c.height_offset - dt * f(6.0);
            p.height_offset = clamp(p.height_offset, F::ZERO, self.parm28);
        } else if c.height_offset < F::ZERO {
            p.height_offset = c.height_offset + dt * f(6.0);
            p.height_offset = clamp(p.height_offset, -self.parm28, F::ZERO);
        }

        let mut damp = if !p.in_liquid() {
            if p.flags & 1 != 0 { self.move_damping.powf(dt) } else { self.parm44.powf(dt) }
        } else {
            self.swim_damping.powf(dt)
        };

        let mut fv = p.force;
        if self.parm60 != 0 {
            if !p.in_liquid() {
                let g = gravity::<M>();
                fv.x += g.x;
                fv.y += g.y;
                fv.z += g.z;
            } else if p.flags & AT_SURFACE == 0 {
                let g = w_gravity::<M>();
                fv.x += g.x;
                fv.y += g.y;
                fv.z += g.z;
            }
        }

        let dt2 = dt * dt;
        let tq = p.torque;

        p.position.add_scaled(&fv, dt2 * damp);
        let vel = p.velocity;
        p.position.add_scaled(&vel, dt * damp);
        p.velocity.add_scaled(&fv, dt);
        p.velocity *= damp;

        damp = self.turn_damping.powf(dt);
        p.angles.add_scaled(&tq, dt2 * damp);
        let av = p.angular_vel;
        p.angles.add_scaled(&av, dt * damp);
        p.angular_vel.add_scaled(&tq, dt);
        p.angular_vel *= damp;

        // The overshoot detector, the other half of SetTargetHeading.
        if p.flags & TRACK_HEADING != 0 {
            let mut wrapped = p.target_heading;
            if f(PI) < p.target_heading - p.angles.x {
                wrapped = p.target_heading - f(TWO_PI);
            } else if p.target_heading - p.angles.x < f(-PI) {
                wrapped = p.target_heading + f(TWO_PI);
            }
            if (F::ZERO < p.torque.x && wrapped < p.angles.x) || (p.torque.x < F::ZERO && p.angles.x < wrapped) {
                p.angles.x = p.target_heading;
                p.torque.x = F::ZERO;
                p.angular_vel.x = F::ZERO;
            }
        }

        p.angles.x = p.angles.x.fmodf(f(TWO_PI));

        if c.flags & AT_SURFACE != 0 {
            p.angles.y = c.angles.y;
        } else if c.flags & SUBMERGED != 0 {
            p.angles.y = clamp(p.angles.y, f(-PITCH_LIMIT), f(PITCH_LIMIT));
        } else {
            let y = p.angles.y;
            if y < self.min_pitch && y < c.angles.y {
                p.angles.y = c.angles.y;
            } else if self.max_pitch < y && c.angles.y < y {
                p.angles.y = c.angles.y;
            }
        }
        p.angles.z = F::ZERO;
        0
    }

    fn view_relative_basis(&self, r: &Vect3<M>, x: F<M>, y: F<M>) -> (F<M>, Vect2<M>, Vect3<M>, Vect3<M>) {
        let mut v = Vect2::new(x, y);
        let mag = v.magnitude();
        v.normalize();
        let mut u = Vect3::new(r.x, F::ZERO, r.z);
        u.normalize();
        let up = Vect3::of(0.0, 1.0, 0.0);
        let mut w = Vect3::cross(&u, &up);
        w.normalize();
        (mag, v, u, w)
    }

    /// Move relative to the camera's direction `r`, turning to face the way
    /// the stick points.
    pub fn view_relative_move(&mut self, r: &Vect3<M>, x: F<M>, y: F<M>) -> i32 {
        if self.phys.flags & NO_TURN != 0 {
            return self.move_stick(x, F::ZERO);
        }
        if !self.phys.on_floor_or_liquid() {
            return 0;
        }
        let (mag, v, u, w) = self.view_relative_basis(r, x, y);
        
        let h = if self.move_deadzone < mag {
            let mut dir = u;
            dir *= v.x;
            dir.add_scaled(&w, v.y);
            let a = if self.run_threshold < mag && !(self.phys.in_mud() || self.phys.flags & SLOW_RUN != 0) {
                self.run_accel
            } else {
                self.walk_accel
            };
            self.phys.force.add_scaled(&dir, a);
            dir.get_hpr().0
        } else {
            self.phys.target_heading
        };
        self.set_target_heading(h);
        0
    }

    pub fn view_relative_walk(&mut self, r: &Vect3<M>, x: F<M>, y: F<M>) -> i32 {
        if self.phys.flags & NO_TURN != 0 {
            return self.walk(x, F::ZERO);
        }
        if !self.phys.on_floor_or_liquid() {
            return 0;
        }
        let (mag, v, u, w) = self.view_relative_basis(r, x, y);
        
        let h = if self.move_deadzone < mag {
            let mut dir = u;
            dir *= v.x;
            dir.add_scaled(&w, v.y);
            let a = self.walk_accel;
            self.phys.force.add_scaled(&dir, a);
            dir.get_hpr().0
        } else {
            self.phys.target_heading
        };
        self.set_target_heading(h);
        0
    }

    /// `Move(int, VIScene &)`: carry the body from the committed position to
    /// the integrated one through the scene — liquid surface, sphere sweep with
    /// skitter, floor and headroom fit, then the environment flags.
    pub fn move_through<S: Scene<M>>(&mut self, flags: i32, scene: &mut S) -> i32 {
        let mut ok = true;
        let mut moving = true;
        let iters = if flags != 0 { 1 } else { 2 };
        let mut sphere = CollSphere::<M>::default();
        let mut normal = Vect3::<M>::default();

        let mut from = self.committed.position;
        from.y += self.parm38;
        let mut to = self.phys.position;
        to.y += self.parm38;

        if self.phys.in_liquid() {
            let above = Vect3::new(to.x, to.y + self.parm54, to.z);
            let below = Vect3::new(to.x, to.y - self.parm54, to.z);
            let mut ray = EnvRay::default();
            if ray.init(above, below, 15) < 0 {
                return -1;
            }
            if scene.collide_env(&mut ray) {
                if f(0.5) < ray.hit_time {
                    to = ray.hit_point();
                }
                let v = self.phys.velocity.y;
                self.phys.velocity.y = if v > F::ZERO { F::ZERO } else { v };
                let fy = self.phys.force.y;
                self.phys.force.y = if F::ZERO < fy { F::ZERO } else { fy };
            }
        }

        if flags != 0 {
            let delta = Vect3::new(to.x - from.x, F::ZERO, to.z - from.z);
            if delta.x * delta.x + delta.y * delta.y + delta.z * delta.z < f(0.0001) {
                moving = false;
            }
        }

        if moving {
            if sphere.init(from, to, self.parm34) < 0 {
                return -1;
            }
            let mut filter = Filter::new(FilterKind::Scene, self.parm64);
            if scene.collide_sphere(&mut sphere, &mut filter) {
                self.phys.flags |= 2;
                let skit = if self.phys.flags & 1 != 0 { Vect3::of(0.0, 1.0, 0.0) } else { Vect3::default() };
                for _ in 0..iters {
                    sphere.skitter(&skit);
                    if !scene.collide_sphere(&mut sphere, &mut filter) {
                        ok = false;
                        break;
                    }
                    if f(0.001) < sphere.ray.hit_time {
                        ok = false;
                    } else {
                        normal += sphere.ray.hit_normal;
                    }
                }
                if ok && self.phys.flags & 0x855 == 0 {
                    normal.normalize();
                    normal *= self.parm34;
                    to += normal;
                    self.phys.force.x = F::ZERO;
                    self.phys.force.z = F::ZERO;
                    self.phys.velocity.x = F::ZERO;
                    self.phys.velocity.z = F::ZERO;
                    sphere.init(from, to, self.parm34);
                    scene.collide_sphere(&mut sphere, &mut filter);
                }
                to = sphere.hit_point();
            } else {
                self.phys.flags &= !2;
                to = sphere.ray.end;
            }
        }

        let mut floor = Vect3::default();
        let mut head = Vect3::default();
        if self.calc_fit(flags, scene, &to, &mut floor, &mut head) < 0 {
            return -1;
        }

        if head.y - floor.y < self.parm30 {
            to = from;
        } else if !self.phys.in_liquid() {
            if to.y - floor.y < self.parm38 {
                to.y = floor.y + self.parm38;
                self.phys.floor = floor.y;
                self.phys.flags |= 1;
                self.phys.velocity.y = F::ZERO;
                if self.committed.flags & 1 != 0 {
                    let rise = floor.y - self.committed.floor;
                    if self.parm24 < rise && f(0.9) < self.phys.landscape_normal.y {
                        self.phys.height_offset -= rise;
                    }
                }
            } else if self.phys.floor - floor.y < self.parm28 {
                to.y = floor.y + self.parm38;
                self.phys.floor = floor.y;
                self.phys.flags |= 1;
                self.phys.velocity.y = F::ZERO;
                if self.committed.flags & 1 != 0 {
                    let rise = floor.y - self.committed.floor;
                    if rise < -self.parm24 && f(0.9) < self.phys.landscape_normal.y {
                        self.phys.height_offset -= rise;
                    }
                }
            } else {
                self.phys.flags &= !1;
            }
        } else if to.y - floor.y < self.parm38 {
            to.y = floor.y + self.parm38;
            self.phys.floor = floor.y;
            self.phys.velocity.y = F::ZERO;
            self.phys.flags |= 1;
        } else {
            self.phys.flags &= !1;
        }

        self.phys.position = to;

        let env = scene.env_flags(&self.phys.position);
        let set = |fl: &mut u32, on: bool, bit: u32| if on { *fl |= bit } else { *fl &= !bit };
        set(&mut self.phys.flags, env & 1 != 0, IN_WATER);
        set(&mut self.phys.flags, env & 4 != 0, IN_LAVA);
        set(&mut self.phys.flags, env & 2 != 0, IN_SLIME);
        set(&mut self.phys.flags, env & 8 != 0, IN_MUD);

        let above = Vect3::new(to.x, to.y + self.parm54, to.z);
        let below = Vect3::new(to.x, to.y - self.parm54, to.z);
        let up = scene.env_flags(&above);
        let dn = scene.env_flags(&below);
        set(&mut self.phys.flags, up & 1 == 0 && dn & 1 != 0, AT_WATER_SURFACE);
        set(&mut self.phys.flags, up & 4 == 0 && dn & 4 != 0, AT_LAVA_SURFACE);
        set(&mut self.phys.flags, up & 2 == 0 && dn & 2 != 0, AT_SLIME_SURFACE);
        set(&mut self.phys.flags, up & 8 == 0 && dn & 8 != 0, AT_MUD_SURFACE);

        self.phys.position.y -= self.parm38;
        let mud = scene.env_flags(&self.phys.position) & 8 != 0;
        set(&mut self.phys.flags, mud, SLOW_RUN);
        0
    }

    /// The floor probe (CalcFloor's body, repeated) and, unless `flags`, a
    /// headroom sweep up to twice the body height above the floor.
    pub fn calc_fit<S: Scene<M>>(&mut self, flags: i32, scene: &mut S, a: &Vect3<M>, b: &mut Vect3<M>, c: &mut Vect3<M>) -> i32 {
        self.calc_floor(scene, a, b);
        *c = *a;
        c.y = b.y;
        c.y += self.parm30 * f(2.0);
        if flags == 0 {
            let mut sphere = CollSphere::default();
            let mut filter = Filter::new(FilterKind::WalkFloor, self.parm64);
            let end = *c;
            if sphere.init(*a, end, self.parm3c) < 0 {
                return -1;
            }
            *c = if scene.collide_sphere(&mut sphere, &mut filter) { sphere.hit_point() } else { sphere.ray.end };
        }
        0
    }

    /// Drop a sphere from `from` down the body height; the floor goes in `to`.
    pub fn calc_floor<S: Scene<M>>(&mut self, scene: &mut S, from: &Vect3<M>, to: &mut Vect3<M>) -> i32 {
        let mut sphere = CollSphere::default();
        let mut filter = Filter::new(FilterKind::WalkFloor, self.parm64);
        self.phys.flags &= !LANDSCAPE;
        *to = *from;
        to.y -= self.parm30;
        if sphere.init(*from, *to, self.parm3c) < 0 {
            return -1;
        }
        if scene.collide_sphere(&mut sphere, &mut filter) {
            *to = sphere.hit_point();
            self.phys.landscape_normal = sphere.ray.hit_normal;
            if filter.hit_actor == -1 {
                self.phys.flags |= LANDSCAPE;
            }
        } else {
            *to = sphere.ray.end;
            self.phys.landscape_normal.zero();
        }
        0
    }
}

