//! `VIWalkCamera` (decomp `viwalkcamera.cpp`, SUPPORT 0x00482020..0x00483EB8)
//! and the `VICameraEffect` it carries (`vicameraeffect.cpp`). The camera IS a
//! walk control (it derives from one) plus an orbit body: an angle, a pitch and
//! a distance, each a damped spring with snap modes, swept against the world.
//! Laid out as the EE lays it out (0x2C0).

use crate::coll::CollSphere;
use crate::fpu::{F, Model, clamp, f};
use crate::scene::{Filter, FilterKind, Scene};
use crate::vect::Vect3;
use crate::walk::WalkControl;

const PI: f32 = 3.14159274;
const TWO_PI: f32 = 6.28318548;

/// The orbit body (0x50 bytes), held twice: `cur` is stepped, `last` committed.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraState<M: Model> {
    pub time: i64,
    /// 1 snap-angle, 2 zoom to minimum, 4 zoom in, 8 snap-pitch.
    pub flags: u32,
    /// (angle, pitch, -).
    pub orbit: Vect3<M>,
    pub orbit_vel: Vect3<M>,
    pub orbit_force: Vect3<M>,
    pub dist: F<M>,
    pub dist_vel: F<M>,
    pub dist_force: F<M>,
    pub camera_pos: Vect3<M>,
    pub snap_angle: F<M>,
    pub snap_pitch: F<M>,
}

/// `VICameraEffect`: the shake and the scripted ("manual") camera (0x34).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraEffect<M: Model> {
    /// 0 none, 1 slow, 2 medium, 3 fast.
    pub speed: i32,
    pub sample: i32,
    pub amplitude: F<M>,
    pub amplitude_end: F<M>,
    pub amplitude_rate: F<M>,
    pub manual_xyz: Vect3<M>,
    pub manual_hpr: Vect3<M>,
    pub manual_terp: F<M>,
    pub manual_terp_rate: F<M>,
}

const SHAKE_COUNT: [i32; 4] = [0, 6, 4, 2];
const SHAKE_SAMPLE: [[f32; 6]; 4] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 0.7, -0.7, -1.0, -0.7, 0.7],
    [1.0, 1.0, -1.0, -1.0, 0.0, 0.0],
    [1.0, -1.0, 0.0, 0.0, 0.0, 0.0],
];

impl<M: Model> CameraEffect<M> {
    pub fn reset(&mut self) {
        self.speed = 0;
        self.sample = 0;
        self.amplitude = F::ZERO;
        self.amplitude_end = F::ZERO;
        self.amplitude_rate = F::ZERO;
    }

    pub fn shake(&mut self, speed: i32, amp: F<M>) {
        self.speed = speed;
        self.amplitude = amp;
        self.amplitude_end = amp;
        self.amplitude_rate = F::ZERO;
    }

    /// `ShakeTerp`: the shake's amplitude carried from where it was headed
    /// to `amp` over `time` seconds (at once, at the same speed, for 0).
    pub fn shake_terp(&mut self, amp: F<M>, time: F<M>) {
        if time == F::ZERO {
            self.shake(self.speed, amp);
            return;
        }
        let from = self.amplitude_end;
        self.amplitude_end = amp;
        self.amplitude = from;
        self.amplitude_rate = (amp - from) / time;
    }

    pub fn update(&mut self, dt: F<M>) {
        if self.manual_terp_rate != F::ZERO {
            self.manual_terp += self.manual_terp_rate * dt;
            if self.manual_terp_rate > F::ZERO {
                if self.manual_terp >= F::ONE {
                    self.manual_terp = F::ONE;
                    self.manual_terp_rate = F::ZERO;
                }
            } else if self.manual_terp <= F::ZERO {
                self.manual_terp = F::ZERO;
                self.manual_terp_rate = F::ZERO;
            }
        }
        if self.speed != 0 && self.amplitude_rate != F::ZERO {
            self.amplitude += self.amplitude_rate * dt;
            if self.amplitude_rate > F::ZERO {
                if self.amplitude >= self.amplitude_end {
                    self.amplitude = self.amplitude_end;
                    self.amplitude_rate = F::ZERO;
                }
            } else if self.amplitude <= self.amplitude_end {
                self.amplitude = self.amplitude_end;
                self.amplitude_rate = F::ZERO;
            }
        }
    }

    /// Bend the eye point and direction. NOT YET: the blend into a manual
    /// camera (0 < terp < 1), which slerps quaternions; it leaves the eye as
    /// is for now. A fully-in manual camera and the shake are ported.
    pub fn apply(&mut self, pos: &mut Vect3<M>, dir: &mut Vect3<M>) {
        // `!(t < 1)`, not `t >= 1`: it is the `else` of the guest's `t < 1`.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if F::ZERO < self.manual_terp && !(self.manual_terp < F::ONE) {
            *pos = self.manual_xyz;
            let h = self.manual_hpr;
            dir.x = h.x.sinf() * h.y.cosf();
            dir.y = -h.y.sinf();
            dir.z = h.x.cosf() * h.y.cosf();
        }
        if self.speed != 0 {
            dir.normalize();
            let s = self.speed as usize;
            self.sample %= SHAKE_COUNT[s];
            let k = f::<M>(SHAKE_SAMPLE[s][self.sample as usize]);
            pos.y += self.amplitude * f(0.025) * k;
            dir.y += self.amplitude * f(0.009) * k;
            self.sample += 1;
        }
    }
}

/// Where the camera is and where it looks: what `VICamera` is handed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Eye<M: Model> {
    pub position: Vect3<M>,
    pub direction: Vect3<M>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WalkCamera<M: Model> {
    pub walk: WalkControl<M>,
    pub cur: CameraState<M>,
    pub angle_accel: F<M>,
    pub dist_accel: F<M>,
    pub last: CameraState<M>,
    pub auto_orbit: i32,
    pub look_at_height: F<M>,
    pub def_orbit_angle: F<M>,
    pub def_orbit_distance: F<M>,
    pub angle_damping: F<M>,
    pub dist_damping: F<M>,
    pub min_orbit_distance: F<M>,
    /// 25.0, the zoom-out ceiling (the QOL patch moves it).
    pub max_orbit_distance: F<M>,
    pub def_orbit_pitch: F<M>,
    pub alt_orbit_pitch: F<M>,
    pub collision_radius: F<M>,
    pub target_height: F<M>,
    pub look_at_height_above: F<M>,
    /// 0 first person, 1 orbit, 2 fixed, 3 third person.
    pub camera_mode: i32,
    pub snap_speed: F<M>,
    pub dist_snap_speed: F<M>,
    pub min_collision_dist: F<M>,
    pub effect: CameraEffect<M>,
}

impl<M: Model> WalkCamera<M> {
    pub fn clear_camera_forces(&mut self) -> i32 {
        self.cur.orbit_force.zero();
        self.cur.dist_force = F::ZERO;
        0
    }

    pub fn set_orbit_pitch_angle(&mut self, p: F<M>) -> i32 {
        self.last.orbit.y = p;
        self.cur.orbit.y = p;
        0
    }

    pub fn set_def_orbit_pitch_angle(&mut self, p: F<M>) -> i32 {
        self.def_orbit_pitch = p;
        self.last.orbit.y = p;
        self.cur.orbit.y = p;
        0
    }

    pub fn set_orbit_distance(&mut self, d: F<M>) -> i32 {
        self.cur.dist = clamp(d, self.min_orbit_distance, self.max_orbit_distance);
        self.cur.dist_vel = F::ZERO;
        self.last.dist_vel = F::ZERO;
        self.cur.dist_force = F::ZERO;
        self.last.dist = self.cur.dist;
        self.last.dist_force = F::ZERO;
        0
    }

    pub fn set_orbit_angle_behind(&mut self) -> i32 {
        self.set_orbit_angle(self.walk.committed.angles.x + f(PI))
    }

    pub fn choose_orbit_angle<S: Scene<M>>(&mut self, a: F<M>, scene: &mut S) -> i32 {
        let mut best = F::ZERO;
        self.find_best_orbit_angle(a.fmodf(f(TWO_PI)), scene, &mut best);
        self.set_orbit_angle(best);
        0
    }

    /// Orbit input; only in the two orbiting modes.
    pub fn orbit_right(&mut self, t: F<M>) -> i32 {
        if self.camera_mode == 1 || self.camera_mode == 3 {
            let t = clamp(t, f(-1.0), F::ONE);
            let a = self.angle_accel;
            let axis = Vect3::<M>::of(1.0, 0.0, 0.0);
            let mut dir = Vect3::<M>::default();
            dir.x += a * axis.x;
            dir.y += a * axis.y;
            dir.z += a * axis.z;
            self.cur.orbit_force.add_scaled(&dir, t);
        }
        0
    }

    /// Zoom.
    pub fn orbit_in(&mut self, t: F<M>) -> i32 {
        if self.camera_mode == 1 || self.camera_mode == 3 {
            let k = -clamp(t, f(-1.0), F::ONE);
            self.cur.dist_force += self.dist_accel * k;
        }
        0
    }

    pub fn halt(&mut self) -> i32 {
        let w = &mut self.walk;
        w.committed.force.zero();
        w.committed.velocity.zero();
        w.committed.torque.zero();
        w.committed.angular_vel.zero();
        self.last.orbit_force.zero();
        self.last.orbit_vel.zero();
        self.last.dist_force = F::ZERO;
        self.last.dist_vel = F::ZERO;
        w.phys.force.zero();
        w.phys.velocity.zero();
        w.phys.torque.zero();
        w.phys.angular_vel.zero();
        self.cur.orbit_force.zero();
        self.cur.orbit_vel.zero();
        self.cur.dist_force = F::ZERO;
        self.cur.dist_vel = F::ZERO;
        0
    }

    pub fn init_camera_intermediate<S: Scene<M>>(&mut self, scene: &mut S) -> i32 {
        let dt = F::new(self.walk.tick_ms as f32) * f(0.001);
        let angle_d = self.angle_damping.powf(dt);
        let dist_d = self.dist_damping.powf(dt);
        self.angle_accel = (self.def_orbit_angle - self.def_orbit_angle * angle_d) / (angle_d * dt);
        self.dist_accel = (self.def_orbit_distance - self.def_orbit_distance * dist_d) / (dist_d * dt);
        self.move_camera(scene);
        self.last = self.cur;
        0
    }

    /// Mode 3 turns the character's target-heading tracking on; every other
    /// mode clears it.
    pub fn set_camera_mode(&mut self, mode: i32) -> i32 {
        self.camera_mode = mode;
        let w = &mut self.walk;
        if mode == 3 {
            w.phys.flags |= 0x100;
            w.committed.flags |= 0x100;
            w.phys.target_heading = w.phys.angles.x;
            w.committed.target_heading = w.committed.angles.x;
        } else {
            w.phys.flags &= !0x100;
            w.committed.flags &= !0x100;
        }
        0
    }

    pub fn set_location<S: Scene<M>>(&mut self, pos: Vect3<M>, angles: Vect3<M>, scene: &mut S) -> Eye<M> {
        self.walk.set_location(pos, angles, scene);
        let l = &mut self.last;
        l.time = scene.time();
        l.orbit.y = self.def_orbit_pitch;
        l.orbit.x = F::ZERO;
        l.flags = 0;
        l.orbit_vel.zero();
        l.orbit_force.zero();
        l.dist_vel = F::ZERO;
        l.dist_force = F::ZERO;
        l.camera_pos = pos;
        l.snap_angle = F::ZERO;
        l.snap_pitch = F::ZERO;
        self.cur = self.last;
        self.init_camera_intermediate(scene);
        self.set_camera()
    }

    /// `Init(pos, angles, cam, scene)`: the camera's parameter block, then
    /// `SetLocation`.
    pub fn init<S: Scene<M>>(&mut self, pos: Vect3<M>, angles: Vect3<M>, scene: &mut S) -> Eye<M> {
        self.walk.init_parms();
        self.look_at_height = f(1.68);
        self.auto_orbit = 1;
        self.min_orbit_distance = f(1.5);
        self.angle_damping = f(0.00999999978);
        self.camera_mode = 0;
        self.def_orbit_angle = f(1.57079637);
        self.max_orbit_distance = f(25.0);
        self.min_collision_dist = f(3.0);
        self.dist_snap_speed = f(23.5);
        self.collision_radius = f(0.5);
        self.def_orbit_distance = f(11.75);
        self.dist_damping = f(0.00999999978);
        self.def_orbit_pitch = f(0.523598790);
        self.alt_orbit_pitch = f(1.04719758);
        self.target_height = f(1.10000002);
        self.look_at_height_above = f(1.68);
        self.snap_speed = f(PI);
        self.last.dist = f(1.5);
        let eye = self.set_location(pos, angles, scene);
        self.effect.reset();
        eye
    }

    /// Set the orbit angle; in third person, move the eye there at once.
    pub fn set_orbit_angle(&mut self, a: F<M>) -> i32 {
        self.cur.orbit.x = a.fmodf(f(TWO_PI));
        self.last.orbit.x = a.fmodf(f(TWO_PI));
        if self.camera_mode == 3 {
            let mut dir = Vect3::<M>::default();
            dir.hpr(self.cur.orbit.x, -self.cur.orbit.y, F::ZERO);
            let mut p = self.walk.phys.position;
            p.x = p.x + dir.x * self.cur.dist;
            p.y = p.y + self.target_height + dir.y * self.cur.dist + self.walk.phys.height_offset;
            p.z = p.z + dir.z * self.cur.dist;
            self.last.camera_pos = p;
            self.cur.camera_pos = p;
        }
        0
    }

    /// Run the body, re-derive the orbit angle from where the eye ended up,
    /// catch the orbit up to the clock, and answer the eye.
    pub fn process<S: Scene<M>>(&mut self, scene: &mut S) -> Eye<M> {
        self.walk.process(0, scene);
        if self.camera_mode == 3 {
            let mut d = self.cur.camera_pos;
            d -= self.walk.phys.position;
            d.y = F::ZERO;
            self.cur.orbit.x = d.get_hpr().0;
        }
        let mut dt = scene.time() - self.last.time;
        self.effect.update(F::new(dt as f32) * f(0.001));
        if dt == 0 {
            self.move_camera(scene);
            self.last = self.cur;
        } else {
            while dt > 0 {
                let step;
                if (self.walk.tick_alt_ms as i64) < dt {
                    step = dt - self.walk.tick_alt_ms as i64;
                    self.cur.time = self.last.time + step;
                } else {
                    let tick = self.walk.tick_ms as i64;
                    step = if tick < dt { tick } else { dt };
                    self.cur.time = self.last.time + step;
                    self.update_camera_physics(F::new(step as i32 as f32) * f(0.001));
                    self.move_camera(scene);
                }
                self.last.time = self.cur.time;
                self.last = self.cur;
                dt -= step;
            }
        }
        self.clear_camera_forces();
        self.set_camera()
    }

    /// The orbit body's step: the angle on x alone (pitch and roll velocity
    /// are multiplied by the damping vector's zero y and z), the distance a
    /// scalar body with two snap modes.
    pub fn update_camera_physics(&mut self, dt: F<M>) -> i32 {
        let c = &mut self.cur;
        let mut damp = Vect3::<M>::default();
        damp.x = self.angle_damping.powf(dt);
        c.orbit.x = c.orbit.x + c.orbit_force.x * dt * dt * damp.x + c.orbit_vel.x * dt * damp.x;
        let force = c.orbit_force;
        c.orbit_vel.add_scaled(&force, dt);
        c.orbit_vel.x *= damp.x;
        c.orbit_vel.y *= damp.y;
        c.orbit_vel.z *= damp.z;

        if c.flags & 1 != 0 {
            let mut tgt = c.snap_angle;
            if f(PI) < c.snap_angle - c.orbit.x {
                tgt = c.snap_angle - f(TWO_PI);
            } else if c.snap_angle - c.orbit.x < f(-PI) {
                tgt = c.snap_angle + f(TWO_PI);
            }
            let cur = c.orbit.x;
            if cur < tgt {
                c.orbit.x = cur + self.snap_speed * dt;
                if tgt < c.orbit.x {
                    c.orbit.x = tgt;
                    c.flags &= !1;
                    self.walk.phys.flags &= !0x200;
                }
            } else if tgt <= cur {
                c.orbit.x = cur - self.snap_speed * dt;
                if c.orbit.x <= tgt {
                    c.orbit.x = tgt;
                    c.flags &= !1;
                    self.walk.phys.flags &= !0x200;
                }
            }
        }

        c.orbit.x = c.orbit.x.fmodf(f(TWO_PI));

        if c.flags & 8 != 0 {
            let tgt = c.snap_pitch;
            let cur = c.orbit.y;
            if cur < tgt {
                c.orbit.y = cur + self.snap_speed * dt;
                if tgt < c.orbit.y {
                    c.orbit.y = tgt;
                    c.flags &= !8;
                }
            } else if tgt <= cur {
                c.orbit.y = cur - self.snap_speed * dt;
                if c.orbit.y <= tgt {
                    c.orbit.y = tgt;
                    c.flags &= !8;
                }
            }
        }

        damp.x = self.dist_damping.powf(dt);
        c.dist = c.dist + c.dist_force * dt * dt * damp.x + c.dist_vel * dt * damp.x;
        c.dist_vel += c.dist_force * dt;
        c.dist_vel *= damp.x;

        if c.flags & 2 != 0 {
            c.dist = c.dist - self.dist_snap_speed * dt;
            if c.dist <= self.min_orbit_distance {
                c.flags &= !2;
            }
        } else if c.flags & 4 != 0 {
            c.dist = c.dist - self.dist_snap_speed * dt;
            if c.dist <= self.min_collision_dist {
                c.dist = self.min_collision_dist;
                c.flags &= !4;
            }
        }
        c.dist = clamp(c.dist, self.min_orbit_distance, self.max_orbit_distance);
        0
    }

    fn sweep<S: Scene<M>>(&self, scene: &mut S, from: Vect3<M>, to: Vect3<M>, sphere: &mut CollSphere<M>) -> bool {
        sphere.init(from, to, self.collision_radius);
        let mut filter = Filter::new(FilterKind::WalkCamera, self.walk.parm64);
        scene.collide_sphere(sphere, &mut filter)
    }

    /// Place the eye. In mode 3 the orbit is swept against the world first:
    /// clear at the default pitch snaps back to it; blocked zooms in while it
    /// can, then tries the alternate pitch, then hunts for another angle.
    pub fn move_camera<S: Scene<M>>(&mut self, scene: &mut S) -> i32 {
        let mut sphere = CollSphere::<M>::default();
        let mut dir = Vect3::<M>::default();
        let p = self.walk.phys;
        if self.camera_mode == 3 {
            if self.cur.flags & (1 | 4 | 8) == 0 {
                let mut from = p.position;
                from.y = from.y + self.target_height;
                let roll = dir.z;
                dir.hpr(self.cur.orbit.x, -self.def_orbit_pitch, roll);
                let mut to = p.position;
                to.y = to.y + self.target_height;
                to.add_scaled(&dir, self.cur.dist);
                from.y = from.y + p.height_offset;
                to.y = to.y + p.height_offset;
                if !self.sweep(scene, from, to, &mut sphere) {
                    if self.cur.orbit.y != self.def_orbit_pitch {
                        self.cur.snap_pitch = self.def_orbit_pitch;
                        self.cur.flags |= 8;
                    }
                } else if self.cur.dist > self.min_collision_dist {
                    self.cur.flags |= 4;
                } else {
                    let mut from = p.position;
                    from.y = from.y + self.target_height;
                    dir.hpr(self.cur.orbit.x, -self.alt_orbit_pitch, roll);
                    let mut to = p.position;
                    to.x = to.x + dir.x * self.cur.dist;
                    to.y = to.y + self.target_height + dir.y * self.cur.dist;
                    to.z = to.z + dir.z * self.cur.dist;
                    from.y = from.y + p.height_offset;
                    to.y = to.y + p.height_offset;
                    if !self.sweep(scene, from, to, &mut sphere) && p.landscape_normal.y < f(0.9) {
                        if self.cur.orbit.y != self.alt_orbit_pitch {
                            self.cur.snap_pitch = self.alt_orbit_pitch;
                            self.cur.flags |= 8;
                        }
                    } else {
                        let mut snap = self.cur.snap_angle;
                        let found = self.find_orbit_angle(self.cur.orbit.x, scene, &mut snap);
                        self.cur.snap_angle = snap;
                        if found == 0 {
                            self.cur.flags |= 2;
                        } else if self.cur.orbit.x != self.cur.snap_angle {
                            self.cur.flags |= 1;
                            self.walk.phys.flags |= 0x200;
                        }
                    }
                }
            }
            dir.hpr(self.cur.orbit.x, -self.cur.orbit.y, F::ZERO);
            self.cur.camera_pos = p.position;
            self.cur.camera_pos.y = self.cur.camera_pos.y + self.target_height;
            self.cur.camera_pos.add_scaled(&dir, self.cur.dist);
            self.cur.camera_pos.y = self.cur.camera_pos.y + p.height_offset;
        } else {
            let mut from = p.position;
            from.y = from.y + self.target_height;
            let mut to = p.position;
            to.y = to.y + self.target_height;
            if self.camera_mode == 2 {
                let z = dir.z;
                dir.hpr(p.angles.x, p.angles.y, z);
                to.sub_scaled(&dir, self.cur.dist);
            } else {
                let z = dir.z;
                dir.hpr(self.cur.orbit.x, -self.cur.orbit.y, z);
                to.add_scaled(&dir, self.cur.dist);
            }
            from.y = from.y + p.height_offset;
            to.y = to.y + p.height_offset;
            if self.auto_orbit != 0 {
                self.sweep(scene, from, to, &mut sphere);
                self.cur.camera_pos = sphere.hit_point();
            } else {
                self.cur.camera_pos = to;
            }
        }
        0
    }

    /// The eye and look direction for the camera, through the effect.
    pub fn set_camera(&mut self) -> Eye<M> {
        let p = self.walk.phys;
        let mut pos = Vect3::default();
        let mut dir = Vect3::default();
        match self.camera_mode {
            0 => {
                pos = p.position;
                pos.y = pos.y + self.look_at_height;
                dir.hpr(p.angles.x, p.angles.y, F::ZERO);
            }
            2 => {
                pos = self.cur.camera_pos;
                dir.hpr(p.angles.x, p.angles.y, F::ZERO);
            }
            1 | 3 => {
                pos = self.cur.camera_pos;
                dir = p.position;
                dir.y = dir.y + p.height_offset + self.look_at_height_above;
                dir -= self.cur.camera_pos;
            }
            _ => {}
        }
        self.effect.apply(&mut pos, &mut dir);
        Eye { position: pos, direction: dir }
    }

    fn candidates(a: F<M>) -> [F<M>; 8] {
        let w = f(TWO_PI);
        [
            a,
            (a + f(0.785398185)).fmodf(w),
            (a - f(0.785398185)).fmodf(w),
            (a + f(1.57079637)).fmodf(w),
            (a - f(1.57079637)).fmodf(w),
            (a + f(2.35619450)).fmodf(w),
            (a - f(2.35619450)).fmodf(w),
            (a + f(PI)).fmodf(w),
        ]
    }

    fn orbit_sweep<S: Scene<M>>(&self, scene: &mut S, angle: F<M>, sphere: &mut CollSphere<M>) -> bool {
        let c = &self.walk.committed;
        let mut from = c.position;
        from.y = from.y + self.target_height;
        let mut dir = Vect3::<M>::default();
        dir.hpr(angle, -self.last.orbit.y, F::ZERO);
        let mut to = c.position;
        to.x = to.x + dir.x * self.last.dist;
        to.y = to.y + self.target_height + dir.y * self.last.dist;
        to.z = to.z + dir.z * self.last.dist;
        from.y = from.y + c.height_offset;
        to.y = to.y + c.height_offset;
        self.sweep(scene, from, to, sphere)
    }

    /// The first clear angle of eight (a, ±45°, ±90°, ±135°, 180°).
    pub fn find_orbit_angle<S: Scene<M>>(&self, a: F<M>, scene: &mut S, out: &mut F<M>) -> i32 {
        let angles = Self::candidates(a);
        *out = angles[0];
        let mut sphere = CollSphere::default();
        for (i, &angle) in angles.iter().enumerate() {
            if !self.orbit_sweep(scene, angle, &mut sphere) {
                *out = angles[i];
                return 1;
            }
        }
        0
    }

    /// The clear angle, or else the one whose sweep got furthest.
    pub fn find_best_orbit_angle<S: Scene<M>>(&self, a: F<M>, scene: &mut S, out: &mut F<M>) -> i32 {
        let angles = Self::candidates(a);
        let mut best: i32 = -1;
        let mut best_dist = F::ZERO;
        let mut sphere = CollSphere::default();
        for (i, &angle) in angles.iter().enumerate() {
            if !self.orbit_sweep(scene, angle, &mut sphere) {
                best = i as i32;
                break;
            }
            if best == -1 || best_dist < sphere.ray.hit_distance() {
                best = i as i32;
                best_dist = sphere.ray.hit_distance();
            }
        }
        *out = angles[best as usize];
        0
    }
}
