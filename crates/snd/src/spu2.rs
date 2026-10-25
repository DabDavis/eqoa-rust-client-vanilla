//! The SPU2: 2 MB of sound RAM, two cores of 24 voices, a 48 kHz stereo mix.
//!
//! Ported line by line from eqoa-port's `platform/spu2.c`, which the oracle
//! runs: a voice is the hardware's (PS-ADPCM decoded four samples a word, a
//! 4-tap Gaussian interpolator stepped by the 14-bit pitch, 0x1000 = 48 kHz,
//! the ADSR envelope, a volume that may sweep), in the per-sample order
//! eqoa-port measured against PCSX2: the envelope steps before it applies, the
//! four taps sum in 32 bits, the decoder runs at most 12 samples ahead and
//! re-reads the block header every sample, a key-on starts at SSA | 1, and the
//! prediction is one rounded sum.
//!
//! The mix is libsd's routing as `sceSdInit` leaves it: every voice to its
//! core's dry L/R, core 0's output into core 1's external input at AVOL, core
//! 1's master volume to the DAC. No reverb: SNDDRV turns the effect off.
//!
//! The structure of the C began as OpenGOAL's game/sound/common/voice.cpp and
//! envelope.cpp:
//!
//! > Copyright (c) 2021-2024 Ziemas. Permission to use, copy, modify, and/or
//! > distribute this software for any purpose with or without fee is hereby
//! > granted, provided that the above copyright notice and this permission
//! > notice appear in all copies. THE SOFTWARE IS PROVIDED "AS IS" AND THE
//! > AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL
//! > IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
//! > AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL
//! > DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR
//! > PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS
//! > ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
//! > THIS SOFTWARE. (ISC)
//!
//! Addresses are 16-bit words inside, bytes at the API, as libsd's are.

use crate::gauss::GAUSS;

pub const RAM: usize = 2 * 1024 * 1024;
pub const RATE: u32 = 48000;
const WORDS_MASK: u32 = (RAM / 2 - 1) as u32;

/// voice parameters, libsd's SD_VP_* numbering
pub const VP_VOLL: i32 = 0;
pub const VP_VOLR: i32 = 1;
pub const VP_PITCH: i32 = 2;
pub const VP_ADSR1: i32 = 3;
pub const VP_ADSR2: i32 = 4;
pub const VP_ENVX: i32 = 5;
pub const VP_VOLXL: i32 = 6;
pub const VP_VOLXR: i32 = 7;
/// core volumes, libsd's SD_P_* numbering
pub const P_MVOLL: i32 = 9;
pub const P_MVOLR: i32 = 10;
pub const P_AVOLL: i32 = 13;
pub const P_AVOLR: i32 = 14;
/// voice addresses, libsd's SD_VA_* numbering less 0x20
pub const VA_SSA: i32 = 0;
pub const VA_LSAX: i32 = 1;
pub const VA_NAX: i32 = 2;

// ---- envelopes ----------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Env {
    shift: i32,
    step: i32,
    exp: i32,
    decrease: i32,
    counter: u32,
    level: i32,
}

impl Env {
    /// the rate: a counter that steps the level when it passes 0x8000; shifts
    /// above 11 slow the counter, shifts below 11 enlarge the step
    fn step(&mut self) {
        let mut inc: u32 = 0x8000u32 >> (if self.shift > 11 { self.shift - 11 } else { 0 });
        let mut step: i32 = self.step.wrapping_shl(if self.shift < 11 { (11 - self.shift) as u32 } else { 0 });

        if self.exp != 0 {
            if self.decrease == 0 && self.level > 0x6000 {
                inc >>= 2;
            }
            if self.decrease != 0 {
                step = (step.wrapping_mul(self.level) >> 15) as i16 as i32;
            }
        }
        if inc == 0 {
            inc = 1;
        }
        self.counter = self.counter.wrapping_add(inc);
        if self.counter >= 0x8000 {
            self.counter = 0;
            self.level = self.level.wrapping_add(step);
            self.level = self.level.clamp(0, 0x7fff);
        }
    }
}

const ATTACK: i32 = 0;
const DECAY: i32 = 1;
const SUSTAIN: i32 = 2;
const RELEASE: i32 = 3;
const STOPPED: i32 = 4;

#[derive(Clone, Copy)]
struct Adsr {
    e: Env,
    /// ADSR2 << 16 | ADSR1
    reg: u32,
    phase: i32,
    target: i32,
}

impl Default for Adsr {
    fn default() -> Adsr {
        Adsr { e: Env::default(), reg: 0, phase: ATTACK, target: 0 }
    }
}

impl Adsr {
    fn settings(&mut self) {
        let r = self.reg;
        match self.phase {
            ATTACK => {
                self.e.exp = ((r >> 15) & 1) as i32;
                self.e.decrease = 0;
                self.e.shift = ((r >> 10) & 0x1f) as i32;
                self.e.step = 7 - ((r >> 8) & 3) as i32;
                self.target = 0x7fff;
            }
            DECAY => {
                self.e.exp = 1;
                self.e.decrease = 1;
                self.e.shift = ((r >> 4) & 0xf) as i32;
                self.e.step = -8;
                self.target = (((r & 0xf) + 1) << 11) as i32;
            }
            SUSTAIN => {
                self.e.exp = ((r >> 31) & 1) as i32;
                self.e.decrease = ((r >> 30) & 1) as i32;
                self.e.shift = ((r >> 24) & 0x1f) as i32;
                self.e.step = if self.e.decrease != 0 { -8 + ((r >> 22) & 3) as i32 } else { 7 - ((r >> 22) & 3) as i32 };
                self.target = 0;
            }
            RELEASE => {
                self.e.exp = ((r >> 21) & 1) as i32;
                self.e.decrease = 1;
                self.e.shift = ((r >> 16) & 0x1f) as i32;
                self.e.step = -8;
                self.target = 0;
            }
            _ => {}
        }
    }

    fn run(&mut self) {
        if self.phase == STOPPED {
            return;
        }
        self.e.step();
        if self.phase == SUSTAIN {
            if self.e.level == 0 {
                // a decreasing sustain that reaches silence
                self.phase = STOPPED;
            }
            return;
        }
        if (self.e.decrease == 0 && self.e.level >= self.target) || (self.e.decrease != 0 && self.e.level <= self.target) {
            if self.phase == ATTACK {
                self.phase = DECAY;
            } else if self.phase == DECAY {
                self.phase = SUSTAIN;
            } else if self.phase == RELEASE {
                self.phase = STOPPED;
                self.e.level = 0;
                return;
            }
            self.settings();
        }
    }
}

/// a volume register: bit 15 clear is a fixed level (bits 0..14, doubled), set
/// is a sweep
#[derive(Clone, Copy, Default)]
struct Vol {
    e: Env,
    reg: u16,
}

impl Vol {
    fn set(&mut self, reg: u16) {
        self.reg = reg;
        if reg & 0x8000 == 0 {
            self.e.level = (reg << 1) as i16 as i32;
            return;
        }
        self.e.exp = ((reg >> 14) & 1) as i32;
        self.e.decrease = ((reg >> 13) & 1) as i32;
        self.e.shift = ((reg >> 2) & 0x1f) as i32;
        self.e.step = if self.e.decrease != 0 { -8 + (reg & 3) as i32 } else { 7 - (reg & 3) as i32 };
    }

    fn run(&mut self) {
        if self.reg & 0x8000 != 0 {
            self.e.step();
        }
    }
}

fn apply(sample: i32, volume: i32) -> i32 {
    (sample.wrapping_mul(volume) >> 15) as i16 as i32
}

// ---- voices -------------------------------------------------------------------

const ADPCM_COEF: [[i32; 2]; 5] = [[0, 0], [60, 0], [115, -52], [98, -55], [122, -60]];

#[derive(Clone, Copy, Default)]
struct Voice {
    vol: [Vol; 2],
    adsr: Adsr,
    pitch: u16,
    /// word addresses
    ssa: u32,
    lsa: u32,
    nax: u32,
    custom_loop: bool,
    header: u16,
    hist1: i16,
    hist2: i16,
    fifo: [i16; 32],
    rd: u32,
    wr: u32,
    counter: u32,
}

#[derive(Clone, Copy, Default)]
struct Core {
    v: [Voice; 24],
    mvol: [Vol; 2],
    avol: [u16; 2],
    endx: u32,
}

pub struct Spu2 {
    /// little-endian, as the SPU2's own
    ram: Vec<u8>,
    core: [Core; 2],
    /// samples mixed since the start
    pub frames: u64,
}

impl Default for Spu2 {
    fn default() -> Spu2 {
        let mut s = Spu2 { ram: vec![0; RAM], core: [Core::default(); 2], frames: 0 };
        s.reset();
        s
    }
}

fn clamp16(x: i32) -> i32 {
    x.clamp(-32768, 32767)
}

impl Spu2 {
    pub fn reset(&mut self) {
        self.core = [Core::default(); 2];
        for c in 0..2 {
            for n in 0..24 {
                let v = &mut self.core[c].v[n];
                v.pitch = 0x3fff;
                v.ssa = 0x5000 >> 1;
                v.adsr.phase = STOPPED;
            }
        }
        self.core[1].avol = [0x7fff, 0x7fff];
    }

    fn word(ram: &[u8], w: u32) -> u16 {
        let i = ((w & WORDS_MASK) * 2) as usize;
        u16::from_le_bytes([ram[i], ram[i + 1]])
    }

    fn voice_header(&mut self, c: usize, n: usize) {
        let v = &mut self.core[c].v[n];
        v.header = Self::word(&self.ram, v.nax & !7);
        if v.header & 0x400 != 0 && !v.custom_loop {
            v.lsa = v.nax & !7;
        }
    }

    fn voice_decode(&mut self, c: usize, n: usize) {
        // the block header is read on every sample: SNDDRV writes the BGM's
        // loop flags into blocks a voice is already playing
        self.voice_header(c, n);
        let ram = &self.ram;
        let core = &mut self.core[c];
        let v = &mut core.v[n];
        if v.wr.wrapping_sub(v.rd) as i32 > 12 {
            return;
        }
        if v.adsr.phase != STOPPED {
            let mut data = Self::word(ram, v.nax) as u32;
            let shift = (v.header & 0xf) as u32;
            let filter = (((v.header >> 4) & 7) as usize).min(4);
            for i in 0..4 {
                // the prediction is ONE rounded sum, as the hardware does it
                let mut s = (((data & 0xf) << 12) as u16 as i16) as i32;
                s >>= shift;
                s += (ADPCM_COEF[filter][0] * v.hist1 as i32 + ADPCM_COEF[filter][1] * v.hist2 as i32 + 32) >> 6;
                let s = clamp16(s);
                v.hist2 = v.hist1;
                v.hist1 = s as i16;
                v.fifo[(v.wr.wrapping_add(i) & 31) as usize] = s as i16;
                data >>= 4;
            }
        }
        v.wr = v.wr.wrapping_add(4);
        v.nax = v.nax.wrapping_add(1);
        if v.nax & 7 == 0 {
            if v.header & 0x100 != 0 {
                // loop end
                core.endx |= 1 << n;
                v.nax = v.lsa;
                if v.header & 0x200 == 0 {
                    // no repeat: the voice ends here
                    v.adsr.phase = STOPPED;
                    v.adsr.e.level = 0;
                }
            }
            v.nax = v.nax.wrapping_add(1);
        }
    }

    fn voice_run(&mut self, c: usize, n: usize, l: &mut i32, r: &mut i32) {
        {
            let v = &mut self.core[c].v[n];
            v.vol[0].run();
            v.vol[1].run();
        }
        self.voice_decode(c, n);
        let v = &mut self.core[c].v[n];
        if v.adsr.phase != STOPPED {
            let idx = ((v.counter & 0x0ff0) >> 4) as usize;
            let g = &GAUSS[idx];
            let mut s = (v.fifo[(v.rd & 31) as usize] as i32 * g[0] as i32) >> 15;
            s += (v.fifo[(v.rd.wrapping_add(1) & 31) as usize] as i32 * g[1] as i32) >> 15;
            s += (v.fifo[(v.rd.wrapping_add(2) & 31) as usize] as i32 * g[2] as i32) >> 15;
            s += (v.fifo[(v.rd.wrapping_add(3) & 31) as usize] as i32 * g[3] as i32) >> 15;
            // the envelope steps BEFORE it applies
            v.adsr.run();
            s = s.wrapping_mul(v.adsr.e.level) >> 15;
            *l = l.wrapping_add(s.wrapping_mul(v.vol[0].e.level as i16 as i32) >> 15);
            *r = r.wrapping_add(s.wrapping_mul(v.vol[1].e.level as i16 as i32) >> 15);
        }
        v.counter += if v.pitch > 0x3fff { 0x3fff } else { v.pitch as u32 };
        v.rd = v.rd.wrapping_add(v.counter >> 12);
        v.counter &= 0xfff;
    }

    fn key_on1(&mut self, c: usize, n: usize) {
        let core = &mut self.core[c];
        let v = &mut core.v[n];
        v.nax = v.ssa | 1;
        v.custom_loop = false;
        v.header = 0;
        core.endx &= !(1u32 << n);
        v.adsr.phase = ATTACK;
        v.adsr.e.level = 0;
        v.adsr.e.counter = 0;
        v.adsr.settings();
        v.counter = 0;
        v.hist1 = 0;
        v.hist2 = 0;
        v.fifo = [0; 32];
        v.rd = 0;
        v.wr = 0;
    }

    fn key_off1(&mut self, c: usize, n: usize) {
        let v = &mut self.core[c].v[n];
        if v.adsr.phase == STOPPED {
            return;
        }
        v.adsr.phase = RELEASE;
        v.adsr.e.counter = 0;
        v.adsr.settings();
    }

    // ---- the API --------------------------------------------------------------

    pub fn set_param(&mut self, core: i32, voice: i32, param: i32, value: u16) {
        let c = &mut self.core[(core & 1) as usize];
        let v = &mut c.v[(voice % 24) as usize];
        match param {
            VP_VOLL => v.vol[0].set(value),
            VP_VOLR => v.vol[1].set(value),
            VP_PITCH => v.pitch = value,
            VP_ADSR1 => v.adsr.reg = (v.adsr.reg & 0xffff0000) | value as u32,
            VP_ADSR2 => v.adsr.reg = (v.adsr.reg & 0xffff) | ((value as u32) << 16),
            P_MVOLL => c.mvol[0].set(value),
            P_MVOLR => c.mvol[1].set(value),
            P_AVOLL => c.avol[0] = value,
            P_AVOLR => c.avol[1] = value,
            // MMIX, EVOL, BVOL: the mix is libsd's default
            _ => {}
        }
    }

    pub fn get_param(&self, core: i32, voice: i32, param: i32) -> u16 {
        let v = &self.core[(core & 1) as usize].v[(voice % 24) as usize];
        match param {
            VP_VOLL => v.vol[0].reg,
            VP_VOLR => v.vol[1].reg,
            VP_PITCH => v.pitch,
            VP_ADSR1 => v.adsr.reg as u16,
            VP_ADSR2 => (v.adsr.reg >> 16) as u16,
            VP_ENVX => v.adsr.e.level as u16,
            VP_VOLXL => v.vol[0].e.level as u16,
            VP_VOLXR => v.vol[1].e.level as u16,
            _ => 0,
        }
    }

    pub fn set_addr(&mut self, core: i32, voice: i32, which: i32, byteaddr: u32) {
        let v = &mut self.core[(core & 1) as usize].v[(voice % 24) as usize];
        let w = (byteaddr >> 1) & WORDS_MASK;
        match which {
            VA_SSA => v.ssa = w,
            VA_LSAX => {
                v.lsa = w;
                v.custom_loop = true;
            }
            VA_NAX => v.nax = w,
            _ => {}
        }
    }

    pub fn get_addr(&self, core: i32, voice: i32, which: i32) -> u32 {
        let v = &self.core[(core & 1) as usize].v[(voice % 24) as usize];
        let r = match which {
            VA_SSA => v.ssa,
            VA_LSAX => v.lsa,
            VA_NAX => v.nax,
            _ => 0,
        };
        r << 1
    }

    pub fn key_on(&mut self, core: i32, mask: u32) {
        for n in 0..24 {
            if mask & (1 << n) != 0 {
                self.key_on1((core & 1) as usize, n);
            }
        }
    }

    pub fn key_off(&mut self, core: i32, mask: u32) {
        for n in 0..24 {
            if mask & (1 << n) != 0 {
                self.key_off1((core & 1) as usize, n);
            }
        }
    }

    pub fn endx(&self, core: i32) -> u32 {
        self.core[(core & 1) as usize].endx
    }

    /// The sound RAM as it stands (what the tests read back).
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    pub fn write(&mut self, byteaddr: u32, src: &[u8]) {
        let addr = byteaddr as usize & (RAM - 1);
        let size = src.len().min(RAM - addr);
        self.ram[addr..addr + size].copy_from_slice(&src[..size]);
    }

    fn frame(&mut self) -> (i16, i16) {
        let (mut l0, mut r0, mut l1, mut r1) = (0i32, 0i32, 0i32, 0i32);
        for c in 0..2 {
            self.core[c].mvol[0].run();
            self.core[c].mvol[1].run();
        }
        for n in 0..24 {
            if self.core[0].v[n].adsr.phase != STOPPED {
                self.voice_run(0, n, &mut l0, &mut r0);
            }
        }
        for n in 0..24 {
            if self.core[1].v[n].adsr.phase != STOPPED {
                self.voice_run(1, n, &mut l1, &mut r1);
            }
        }
        // core 0's output, at its master volume, is core 1's external input
        let c0 = &self.core[0];
        let c1 = &self.core[1];
        let l0 = apply(clamp16(l0), c0.mvol[0].e.level as i16 as i32);
        let r0 = apply(clamp16(r0), c0.mvol[1].e.level as i16 as i32);
        let l1 = clamp16(l1) + apply(l0, c1.avol[0] as i16 as i32);
        let r1 = clamp16(r1) + apply(r0, c1.avol[1] as i16 as i32);
        (
            apply(clamp16(l1), c1.mvol[0].e.level as i16 as i32) as i16,
            apply(clamp16(r1), c1.mvol[1].e.level as i16 as i32) as i16,
        )
    }

    /// mix `out.len() / 2` stereo frames, interleaved L R, at 48 kHz
    pub fn mix(&mut self, out: &mut [i16]) {
        for f in out.as_chunks_mut::<2>().0 {
            let (l, r) = self.frame();
            f[0] = l;
            f[1] = r;
            self.frames += 1;
        }
    }
}
