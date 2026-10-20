//! `VIPad` (decomp `vipad.cpp`, SUPPORT 0x004A1AC8 .. 0x004A2298): one
//! controller port over libpad, and the `VIConsolePad` it fills.
//!
//! `Read` runs libpad's state machine: a pad that is not ready is torn down
//! (its motors stopped, `m_state` -1), and the next time it is ready the
//! actuator handshake runs (two actuators, a DualShock, the small motor's
//! current 0 and the large one's above it: `scePadSetActAlign`), answered on
//! a later read by `scePadGetReqState`. A DualShock (`InfoMode` id 7) is then
//! read and unpacked: the four sticks through `ScaleStick` (a dead zone across
//! the middle of 0..255), each of the sixteen buttons one inverted bit as a
//! float. Anything else is asked into analog mode (`SetMainMode`).
//!
//! `VibrateLf`/`VibrateHf` stamp an expiry and mark the motors dirty;
//! `Process` is what talks to the pad, once a frame at most and only when
//! something changed. Both act only on a pad whose handshake completed
//! (`Active`).
//!
//! libpad is the platform's ([`Libpad`]). Laid out as the EE lays it out
//! (0x60 bytes: `long` is eight), so the oracle is handed its bytes.

use crate::fpu::{F, Model};

/// `VIConsolePad`'s fields after `m_a`, in order (84 bytes in all): what
/// `VIClient::PadKeys` turns into key code `128 + index` when one changes
/// between zero and not.
pub mod key {
    pub const RX: usize = 0;
    pub const RY: usize = 1;
    pub const LX: usize = 2;
    pub const LY: usize = 3;
    pub const TRIANGLE: usize = 4;
    pub const SQUARE: usize = 5;
    pub const CROSS: usize = 6;
    pub const CIRCLE: usize = 7;
    pub const LEFT: usize = 8;
    pub const RIGHT: usize = 9;
    pub const UP: usize = 10;
    pub const DOWN: usize = 11;
    pub const L1: usize = 12;
    pub const L2: usize = 13;
    pub const R1: usize = 14;
    pub const R2: usize = 15;
    pub const START: usize = 16;
    pub const SELECT: usize = 17;
    pub const L3: usize = 18;
    pub const R3: usize = 19;
}

/// `VIConsolePad`: the sticks (-1..1, 0 in the dead zone) and the buttons
/// (0 or 1), as `Read` writes them. `m_a` is never written by `Read`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ConsolePad<M: Model> {
    pub a: i32,
    pub keys: [F<M>; 20],
}

/// libpad's calls, as `VIPad` makes them (`scePad*`, each with the pad's port
/// and slot). The return values are libpad's: `get_state` 2 FINDCTP1 or 6
/// STABLE is ready; `get_req_state` 0 COMPLETE, 1 FAILED, 2 BUSY;
/// `info_mode(1, 0)` the controller id (7 a DualShock), `(2, 0)` the extended
/// one; `info_act(-1, 0)` how many actuators, `(n, 3)` actuator n's current.
pub trait Libpad {
    fn port_open(&mut self, port: i32, slot: i32) -> i32;
    fn port_close(&mut self, port: i32, slot: i32) -> i32;
    fn get_req_state(&mut self, port: i32, slot: i32) -> i32;
    fn get_state(&mut self, port: i32, slot: i32) -> i32;
    fn info_mode(&mut self, port: i32, slot: i32, term: i32, offs: i32) -> i32;
    fn info_act(&mut self, port: i32, slot: i32, actno: i32, term: i32) -> i32;
    fn set_act_align(&mut self, port: i32, slot: i32, align: &[u8; 6]) -> i32;
    fn set_act_direct(&mut self, port: i32, slot: i32, act: &[u8; 6]) -> i32;
    fn set_main_mode(&mut self, port: i32, slot: i32, offs: i32, lock: i32) -> i32;
    /// `scePadRead` into the pad's 32-byte buffer: [0] 0 on success, [2] and
    /// [3] the buttons active-low, [4..8] the sticks rx ry lx ly (128 at rest).
    fn read(&mut self, port: i32, slot: i32, data: &mut [u8; 32]) -> i32;
}

/// `sActAlign`: actuator 0 (the small motor) on byte 0, actuator 1 (the
/// large) on byte 1.
pub const ACT_ALIGN: [u8; 6] = [0, 1, 0xff, 0xff, 0xff, 0xff];

/// `VIPad`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Pad {
    pub port: i32,
    pub slot: i32,
    /// stamped by `Process`
    pub time: i64,
    /// 0 is off
    pub lf_end: i64,
    pub hf_end: i64,
    pub state: i32,
    pub act_aligned: i32,
    /// [0] the small motor (on or off), [1] the large one's level
    pub act: [u8; 6],
    _p0: [u8; 2],
    pub dirty: i32,
    /// 1 while a DualShock is being read
    pub mode: i32,
    pub open: i32,
    /// `scePadRead`'s buffer
    pub data: [u8; 32],
    _p1: [u8; 4],
}

/// `ScaleStick`: 88 and 167 bound a dead zone in the middle of 0..255, and
/// either side runs out to ±1 (1/88 written out as a multiply).
pub fn scale_stick<M: Model>(v: u8) -> F<M> {
    if v < 88 {
        return F::from_int(88 - v as i32) * F::new(-0.0113636364);
    }
    if v >= 168 {
        return F::from_int(v as i32 - 167) * F::new(0.0113636364);
    }
    F::ZERO
}

impl Pad {
    /// `Active`: the actuator handshake completed.
    pub fn active(&self) -> bool {
        self.state == 2 && self.act_aligned == 0
    }

    /// `Init`: -1 when the port does not open.
    pub fn init(&mut self, lib: &mut impl Libpad, port: i32, slot: i32) -> i32 {
        let r = lib.port_open(port, slot);
        if r != 1 {
            return -1;
        }
        self.open = r;
        self.port = port;
        self.slot = slot;
        self.time = 0;
        self.lf_end = 0;
        self.hf_end = 0;
        self.state = 0;
        self.act_aligned = 0;
        self.act = [0; 6];
        self.dirty = 0;
        self.mode = 0;
        0
    }

    /// `Close`.
    pub fn close(&mut self, lib: &mut impl Libpad) -> i32 {
        if self.open != 0 {
            self.open = 0;
            lib.port_close(self.port, self.slot);
        }
        self.mode = 0;
        0
    }

    /// `Process`: the motors whose time is up stopped, and the pad told
    /// when anything changed.
    pub fn process(&mut self, now: i64, lib: &mut impl Libpad) -> i32 {
        self.time = now;
        if self.active() {
            if self.lf_end != 0 && self.lf_end < now {
                self.act[1] = 0;
                self.dirty = 1;
                self.lf_end = 0;
            }
            if self.hf_end != 0 && self.hf_end < now {
                self.act[0] = 0;
                self.dirty = 1;
                self.hf_end = 0;
            }
            if self.dirty != 0 {
                lib.set_act_direct(self.port, self.slot, &self.act);
                self.dirty = 0;
            }
        }
        0
    }

    /// `VibrateLf`: the large motor at `level` for `ms`.
    pub fn vibrate_lf(&mut self, level: u8, ms: i32) -> i32 {
        if self.active() {
            self.act[1] = level;
            self.lf_end = ms as i64 + self.time;
            self.dirty = 1;
        }
        0
    }

    /// `VibrateHf`: the small motor on for `ms`.
    pub fn vibrate_hf(&mut self, ms: i32) -> i32 {
        if self.active() {
            self.act[0] = 1;
            self.hf_end = ms as i64 + self.time;
            self.dirty = 1;
        }
        0
    }

    /// `StopVibration`: whether or not the handshake completed.
    pub fn stop_vibration(&mut self, lib: &mut impl Libpad) {
        self.act[0] = 0;
        self.act[1] = 0;
        self.lf_end = 0;
        self.hf_end = 0;
        lib.set_act_direct(self.port, self.slot, &self.act);
    }

    /// `Read`: 1 with `p` filled from a DualShock's report, else 0 (`p` left
    /// as it was).
    pub fn read<M: Model>(&mut self, lib: &mut impl Libpad, p: &mut ConsolePad<M>) -> i32 {
        if self.act_aligned != 0 {
            let rs = lib.get_req_state(self.port, self.slot);
            if rs != 0 {
                if rs == 2 {
                    return 0;
                }
                self.state = 0;
            }
            self.act_aligned = 0;
        }
        let st = lib.get_state(self.port, self.slot);
        if st == 2 || st == 6 {
            let mut id = lib.info_mode(self.port, self.slot, 1, 0);
            if id != 0 {
                let ex = lib.info_mode(self.port, self.slot, 2, 0);
                if ex > 0 {
                    id = ex;
                }
            }
            if self.state == -1 {
                self.state = lib.info_act(self.port, self.slot, -1, 0);
                if self.state != 2 {
                    self.state = 0;
                } else if id != 7 {
                    self.state = 0;
                } else if lib.info_act(self.port, self.slot, 0, 3) != 0 {
                    self.state = 0;
                } else if lib.info_act(self.port, self.slot, 1, 3) <= 0 {
                    self.state = 0;
                } else {
                    let r = lib.set_act_align(self.port, self.slot, &ACT_ALIGN);
                    if r == 1 {
                        self.act_aligned = r;
                    } else {
                        self.state = 0;
                    }
                }
            }
            self.mode = 0;
            if id == 7 {
                if lib.read(self.port, self.slot, &mut self.data) <= 0 || self.data[0] != 0 {
                    return 0;
                }
                use key::*;
                let d = self.data;
                let bit = |b: u8, k: u32| F::<M>::from_int(((!b as i32) >> k) & 1);
                p.keys[RX] = scale_stick(d[4]);
                p.keys[RY] = scale_stick(d[5]);
                p.keys[LX] = scale_stick(d[6]);
                p.keys[LY] = scale_stick(d[7]);
                p.keys[TRIANGLE] = bit(d[3], 4);
                p.keys[SQUARE] = bit(d[3], 7);
                p.keys[CROSS] = bit(d[3], 6);
                p.keys[CIRCLE] = bit(d[3], 5);
                p.keys[LEFT] = bit(d[2], 7);
                p.keys[DOWN] = bit(d[2], 6);
                p.keys[RIGHT] = bit(d[2], 5);
                p.keys[UP] = bit(d[2], 4);
                p.keys[L1] = bit(d[3], 2);
                p.keys[R1] = bit(d[3], 3);
                p.keys[L2] = bit(d[3], 0);
                p.keys[R2] = bit(d[3], 1);
                p.keys[START] = bit(d[2], 3);
                p.keys[SELECT] = bit(d[2], 0);
                p.keys[L3] = bit(d[2], 1);
                p.keys[R3] = bit(d[2], 2);
                self.mode = 1;
                return 1;
            }
            lib.set_main_mode(self.port, self.slot, 1, 3);
            return 0;
        }
        if self.active() {
            self.act[0] = 0;
            self.act[1] = 0;
            lib.set_act_direct(self.port, self.slot, &self.act);
        }
        self.state = -1;
        self.lf_end = 0;
        self.hf_end = 0;
        self.act[0] = 0;
        self.act[1] = 0;
        self.dirty = 0;
        self.mode = 0;
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpu::Ee;

    #[test]
    fn the_layout_is_the_ee_one() {
        assert_eq!(std::mem::size_of::<Pad>(), 0x60);
        assert_eq!(std::mem::offset_of!(Pad, state), 0x20);
        assert_eq!(std::mem::offset_of!(Pad, act), 0x28);
        assert_eq!(std::mem::offset_of!(Pad, dirty), 0x30);
        assert_eq!(std::mem::offset_of!(Pad, data), 0x3C);
        assert_eq!(std::mem::size_of::<ConsolePad<Ee>>(), 84);
    }

    #[test]
    fn the_dead_zone_is_88_to_167_and_each_side_reaches_one() {
        for v in 88..=167u8 {
            assert_eq!(scale_stick::<Ee>(v).0, 0.0, "{v}");
        }
        assert!(scale_stick::<Ee>(87).0 < 0.0 && scale_stick::<Ee>(168).0 > 0.0);
        assert!((scale_stick::<Ee>(0).0 + 1.0).abs() < 1e-6);
        assert!((scale_stick::<Ee>(255).0 - 1.0).abs() < 1e-6);
    }
}
