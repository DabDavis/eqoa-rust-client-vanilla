//! The XM player: SNDDRV's `xm.cpp` and `module.cpp`, line by line.
//!
//! A module is a FastTracker 2 song re-packed by the EE tools (a 28-byte
//! header, the order table, 12-byte pattern descriptors, 224-byte instruments,
//! 24-byte sample records, then the patterns' rows in XM's own packing), with
//! its samples already in SPU RAM as ADPCM. It stays in IOP memory as bytes and
//! every field is read there through its address, as SNDDRV reads it once
//! `Relocate` has made the offsets pointers. A player is stepped once per
//! tracker tick: it reads a row or runs the effects, writes what changed to its
//! voices, and sends the tick's key-ons and key-offs in one switch write a core.
//!
//! The C's widths are kept: `u8`/`s8`/`short`/`u16` fields wrap as they do on
//! the IOP, and signed division truncates.

use crate::iop::Iop;

// ---- the module, by offset ---------------------------------------------------------

const M_LENGTH: u32 = 0x04;
const M_CHANNELS: u32 = 0x08;
const M_PATTERNS: u32 = 0x0A;
const M_INSTRUMENTS: u32 = 0x0C;
const M_FLAGS: u32 = 0x0E;
const M_SPEED: u32 = 0x10;
const M_TEMPO: u32 = 0x12;
const M_ORDER: u32 = 0x1C;
const M_PATTERN: u32 = 0x11C;
const M_INSTRUMENT: u32 = 0xD1C;
const M_SAMPLE: u32 = 0x7D1C;
const M_ROWS_DATA: u32 = 0x891C;

const PAT_SIZE: u32 = 12;
const PAT_ROWS: u32 = 0x04;
const PAT_BYTES: u32 = 0x06;
const PAT_DATA: u32 = 0x08;

const INS_SIZE: u32 = 224;
const INS_SAMPLES: u32 = 0x00;
const INS_MAP: u32 = 0x04;
const INS_VOLENV: u32 = 0x64;
const INS_PANENV: u32 = 0x9C;
const INS_FADEOUT: u32 = 0xD8;
const INS_FIRST: u32 = 0xDA;
const INS_SAMPLE: u32 = 0xDC;

const SMP_SIZE: u32 = 24;
const SMP_LENGTH: u32 = 0x00;
const SMP_LOOPSTART: u32 = 0x04;
const SMP_LOOPEND: u32 = 0x08;
const SMP_FLAGS: u32 = 0x0C;
const SMP_VOLUME: u32 = 0x10;
const SMP_PAN: u32 = 0x11;
const SMP_FINETUNE: u32 = 0x12;
const SMP_RELNOTE: u32 = 0x13;
const SMP_SPU: u32 = 0x14;

const ENV_POINTS: u32 = 0x00;
const ENV_SUSTAIN: u32 = 0x01;
const ENV_LOOPSTART: u32 = 0x02;
const ENV_LOOPEND: u32 = 0x03;
const ENV_FLAGS: u32 = 0x04;
const ENV_PT: u32 = 0x08;

/// `XmModule::Relocate`: patterns' rows at +0x891C, instruments' sample lists
/// into the sample table, each sample's SPU address offset by where its ADPCM
/// went; lengths and loop points become bytes, 28 samples to a 16-byte block.
pub fn relocate(io: &mut Iop, m: u32, spu_addr: u32) {
    let patterns = io.r16(m + M_PATTERNS) as i16 as i32;
    for i in 0..patterns {
        let p = m + M_PATTERN + i as u32 * PAT_SIZE;
        let off = io.r32(p + PAT_DATA);
        io.w32(p + PAT_DATA, m.wrapping_add(off.wrapping_add(M_ROWS_DATA)));
    }
    let instruments = io.r16(m + M_INSTRUMENTS) as i16 as i32;
    for i in 0..instruments {
        let ins = m + M_INSTRUMENT + i as u32 * INS_SIZE;
        let first = io.r16(ins + INS_FIRST) as i16;
        let ptr = if first == -1 { 0 } else { m.wrapping_add(M_SAMPLE).wrapping_add((first as i32 as u32).wrapping_mul(SMP_SIZE)) };
        io.w32(ins + INS_SAMPLE, ptr);
    }
    for i in 0..instruments {
        let ins = m + M_INSTRUMENT + i as u32 * INS_SIZE;
        let n = io.r8(ins + INS_SAMPLES) as i32;
        for j in 0..n {
            let s = io.r32(ins + INS_SAMPLE).wrapping_add(j as u32 * SMP_SIZE);
            let spu = io.r32(s + SMP_SPU);
            let len = io.r32(s + SMP_LENGTH) as i32;
            let ls = io.r32(s + SMP_LOOPSTART) as i32;
            let le = io.r32(s + SMP_LOOPEND) as i32;
            let flags = io.r32(s + SMP_FLAGS) as i32;
            io.w32(s + SMP_SPU, if len != 0 { spu_addr.wrapping_add(spu) } else { 0 });
            io.w32(s + SMP_LENGTH, len.wrapping_sub(if flags & 3 != 0 { 1 } else { 2 }).wrapping_mul(28) as u32);
            io.w32(s + SMP_LOOPSTART, if ls == -1 { -1i32 as u32 } else { ls.wrapping_sub(1).wrapping_mul(28) as u32 });
            io.w32(s + SMP_LOOPEND, if le == -1 { -1i32 as u32 } else { le.wrapping_sub(1).wrapping_mul(28) as u32 });
        }
    }
}

// ---- the voices ---------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct XmVoice {
    voice: i32,
    loop_start: i32,
    /// the sample's flags & 3; SetOffset uses it as the loop OFFSET, a shipped
    /// bug (loops start at 0..3)
    loop_type: u32,
    /// the sample's SPU address, 0 = silent
    addr: u32,
}

/// The voices a player owns, one per channel of its slot's mask, and the
/// key-on/key-off masks a tick collects.
#[derive(Clone)]
pub struct XmVoices {
    key_on: [u32; 2],
    key_off: [u32; 2],
    ch: [XmVoice; 32],
}

fn core_voice(v: i32) -> (u16, u16) {
    if v < 24 { (0, v as u16) } else { (1, (v - 24) as u16) }
}

impl XmVoices {
    fn init(lo: u32, hi: u32) -> XmVoices {
        let mut x = XmVoices { key_on: [0; 2], key_off: [0; 2], ch: [XmVoice::default(); 32] };
        let mut n = 0;
        for i in 0..24 {
            if (1u32 << i) & lo != 0 {
                x.ch[n].voice = i;
                n += 1;
            }
        }
        for i in 0..24 {
            if (1u32 << i) & hi != 0 {
                x.ch[n].voice = i + 24;
                n += 1;
            }
        }
        x
    }

    fn flush(&mut self, io: &mut Iop) {
        if self.key_on[0] != 0 {
            io.sd_set_switch(0x1500, self.key_on[0]);
        }
        if self.key_on[1] != 0 {
            io.sd_set_switch(0x1501, self.key_on[1]);
        }
        if self.key_off[0] != 0 {
            io.sd_set_switch(0x1600, self.key_off[0]);
        }
        if self.key_off[1] != 0 {
            io.sd_set_switch(0x1601, self.key_off[1]);
        }
        self.key_on = [0; 2];
        self.key_off = [0; 2];
    }

    #[allow(clippy::too_many_arguments)]
    fn key_on(&mut self, io: &mut Iop, ch: usize, addr: u32, loop_type: u32, loop_start: i32, rate: u32, vol_l: i32, vol_r: i32) {
        let v = &mut self.ch[ch];
        v.addr = addr;
        if addr == 0 {
            return;
        }
        let (core, voice) = core_voice(v.voice);
        self.key_on[core as usize] |= 1 << voice;
        let e = core | (voice << 1);
        io.sd_set_param(e, (vol_l << 6) as u16);
        io.sd_set_param(e | 0x100, (vol_r << 6) as u16);
        io.sd_set_param(e | 0x200, (rate.wrapping_mul(4096) as i32 / 48000) as u16);
        io.sd_set_addr(e | 0x2040, addr);
        v.loop_start = loop_start;
        v.loop_type = loop_type;
    }

    /// Compiled and never called: SNDDRV's key-off (note 97, effect K) only
    /// lets the channel's envelope fall, so the voice runs on silent.
    #[allow(dead_code)]
    fn key_off(&mut self, ch: usize) {
        let (core, voice) = core_voice(self.ch[ch].voice);
        self.key_off[core as usize] |= 1 << voice;
    }

    fn set_pitch(&mut self, io: &mut Iop, ch: usize, rate: u32) {
        let v = &self.ch[ch];
        if v.addr != 0 {
            let (core, voice) = core_voice(v.voice);
            io.sd_set_param(core | (voice << 1) | 0x200, (rate.wrapping_mul(4096) as i32 / 48000) as u16);
        }
    }

    fn set_volume(&mut self, io: &mut Iop, ch: usize, vol_l: i32, vol_r: i32) {
        let v = &self.ch[ch];
        if v.addr != 0 {
            let (core, voice) = core_voice(v.voice);
            io.sd_set_param(core | (voice << 1), (vol_l << 6) as u16);
            io.sd_set_param(core | (voice << 1) | 0x100, (vol_r << 6) as u16);
        }
    }

    /// A sample offset in samples becomes ADPCM bytes: 28 samples to a 16-byte
    /// block, rounded down to two blocks.
    fn set_offset(&mut self, io: &mut Iop, ch: usize, samples: u32) {
        let v = &self.ch[ch];
        if v.addr != 0 {
            let (core, voice) = core_voice(v.voice);
            let mut off = samples.wrapping_mul(2);
            off /= 7;
            off = off.wrapping_mul(2);
            off &= !0x1f;
            io.sd_set_addr(core | (voice << 1) | 0x2040, v.addr.wrapping_add(off));
            if v.loop_type != 0 {
                let mut off = v.loop_type;
                off *= 2;
                off /= 7;
                off &= !0x1f;
                io.sd_set_addr(core | (voice << 1) | 0x2140, v.addr.wrapping_add(off));
            }
        }
    }
}

// ---- a channel --------------------------------------------------------------------

/// One tracker channel. Pointers into the module are IOP addresses, 0 = none.
#[derive(Clone, Copy, Default)]
struct XmChannel {
    note: u8,
    ins: u8,
    vol: u8,
    fx: u8,
    param: u8,
    /// note - 1
    key: u8,
    /// this tick's, after tremolo
    out_volume: i16,
    /// 0..64
    volume: i16,
    /// 0..255
    pan: i16,
    inst: u32,
    vol_env: u32,
    pan_env: u32,
    sample: u32,
    rel_note: u8,
    finetune: i16,
    /// this tick's, after vibrato/arpeggio
    out_period: i32,
    period: i32,
    /// the note's own period: portamento's goal
    target: i32,
    /// 32767 at key-on, falls by `fadeout`
    fade_vol: i32,
    fadeout: i32,
    /// a tone portamento this row: no retrigger
    porta: i8,
    key_on: i8,
    /// the key-on has gone to the voice
    keyed: i8,
    env_vol: i16,
    vol_env_pos: u16,
    vol_env_a: u8,
    vol_env_b: u8,
    env_pan: i16,
    pan_env_pos: u16,
    pan_env_a: u8,
    pan_env_b: u8,
    slide: u16,
    porta_speed: u16,
    vib_depth: u16,
    vib_speed: u16,
    vib_pos: i8,
    trem_depth: u16,
    trem_speed: u16,
    trem_pos: i8,
    offset: i32,
    last_offset: i32,
    /// vibrato waveform low nibble, tremolo high
    waves: u16,
    vol_slide: u8,
    pan_slide: u8,
    vol_col_slide: u8,
    pan_col_slide: u8,
    fine_porta: u8,
    glissando: u8,
    period_set: u8,
    volume_set: u8,
}

static XM_SINE: [u8; 32] = [
    0, 24, 49, 74, 97, 120, 141, 161, 180, 197, 212, 224, 235, 244, 250, 253, 255, 253, 250, 244, 235, 224, 212, 197, 180, 161, 141, 120, 97, 74,
    49, 24,
];

/// Linear interpolation of y between (x0, y0) and (x1, y1), in shorts.
fn interp(x: i16, x0: i16, x1: i16, y0: i16, y1: i16) -> i16 {
    if x0 == x1 {
        return y0;
    }
    let dy = y1.wrapping_sub(y0);
    let dx = x1.wrapping_sub(x0);
    let dp = x.wrapping_sub(x0);
    let q = (dp as i32 * dy as i32).checked_div(dx as i32).unwrap_or(0);
    (y0 as i32 + q) as i16
}

impl XmChannel {
    fn set_key(&mut self, on: bool) {
        if on {
            self.key_on = 1;
            self.keyed = 0;
        } else {
            self.key_on = 0;
            self.keyed = 1;
        }
    }

    fn reset(&mut self) {
        self.vol_env_pos = 0;
        self.vol_env_a = 0;
        self.vol_env_b = 1;
        self.pan_env_pos = 0;
        self.pan_env_a = 0;
        self.pan_env_b = 1;
    }

    fn envelopes(&mut self, io: &Iop) {
        if self.vol_env != 0 {
            let (mut p, mut a, mut b) = (self.vol_env_pos, self.vol_env_a, self.vol_env_b);
            self.env_vol = self.env_step(io, self.vol_env, &mut p, &mut a, &mut b);
            (self.vol_env_pos, self.vol_env_a, self.vol_env_b) = (p, a, b);
        } else {
            self.env_vol = 64;
        }
        if self.pan_env != 0 {
            let (mut p, mut a, mut b) = (self.pan_env_pos, self.pan_env_a, self.pan_env_b);
            self.env_pan = ((self.env_step(io, self.pan_env, &mut p, &mut a, &mut b) as i32) << 2) as i16;
            (self.pan_env_pos, self.pan_env_a, self.pan_env_b) = (p, a, b);
        } else {
            self.env_pan = 128;
        }
    }

    /// One tick of an envelope: the value between its two current points, then
    /// the position moves on unless a held key is sitting on the sustain point.
    fn env_step(&self, io: &Iop, e: u32, ppos: &mut u16, pa: &mut u8, pb: &mut u8) -> i16 {
        let px = |k: u8| io.r16(e + ENV_PT + k as u32 * 4) as i16;
        let py = |k: u8| io.r16(e + ENV_PT + k as u32 * 4 + 2) as i16;
        let points = io.r8(e + ENV_POINTS) as i32;
        let flags = io.r32(e + ENV_FLAGS);
        let mut a = (*pa as i32).min(points - 1) as u8;
        let mut b = (*pb as i32).min(points - 1) as u8;
        let mut pos = *ppos;
        let v = interp(pos as i16, px(a), px(b), py(a), py(b));
        if flags & 2 != 0 && self.key_on != 0 && a == io.r8(e + ENV_SUSTAIN) && pos as i32 == px(a) as i32 {
        } else {
            pos = pos.wrapping_add(1);
            if pos as i32 >= px(b) as i32 {
                a = b;
                b = b.wrapping_add(1);
                if flags & 4 != 0 {
                    if b > io.r8(e + ENV_LOOPEND) {
                        a = io.r8(e + ENV_LOOPSTART);
                        b = a.wrapping_add(1);
                        pos = px(a) as u16;
                    }
                } else if b as i32 >= points {
                    b = b.wrapping_sub(1);
                    pos = pos.wrapping_sub(1);
                }
            }
        }
        *pa = a;
        *pb = b;
        *ppos = pos;
        v
    }

    fn porta(&mut self) {
        let d = self.out_period.wrapping_sub(self.target);
        if d == 0 || self.porta_speed as i32 > d.wrapping_abs() {
            self.out_period = self.target;
        } else if d > 0 {
            self.out_period -= self.porta_speed as i32;
        } else {
            self.out_period += self.porta_speed as i32;
        }
        self.period = self.out_period;
    }

    fn wave(waves: u16, pos_reg: i8) -> u16 {
        let mut pos: u8 = ((pos_reg as u8) >> 2) & 0x1f;
        match waves & 3 {
            0 => XM_SINE[pos as usize] as u16,
            1 => {
                pos = pos.wrapping_shl(3);
                if pos_reg < 0 {
                    pos = 255 - pos;
                }
                pos as u16
            }
            2 => 255,
            _ => 0,
        }
    }

    fn vibrato(&mut self, tick: i32) {
        let mut v = Self::wave(self.waves, self.vib_pos);
        v = v.wrapping_mul(self.vib_depth);
        v >>= 7;
        v = v.wrapping_shl(2);
        if self.vib_pos >= 0 {
            self.out_period = self.period + v as i32;
        } else {
            self.out_period = self.period - v as i32;
        }
        if tick != 0 {
            self.vib_pos = (self.vib_pos as i32 + self.vib_speed as i32) as i8;
        }
    }

    fn tremolo(&mut self, tick: i32) {
        let mut v = Self::wave(self.waves >> 4, self.trem_pos);
        v = v.wrapping_mul(self.trem_depth);
        v >>= 6;
        if self.trem_pos >= 0 {
            self.out_volume = (self.volume as i32 + v as i32) as i16;
            if self.out_volume > 64 {
                self.out_volume = 64;
            }
        } else {
            self.out_volume = (self.volume as i32 - v as i32) as i16;
            if self.out_volume < 0 {
                self.out_volume = 0;
            }
        }
        if tick != 0 {
            self.trem_pos = (self.trem_pos as i32 + self.trem_speed as i32) as i8;
        }
    }

    fn vol_slide(&mut self, tick: i32) {
        if tick != 0 {
            self.volume = (self.volume as i32 + (self.vol_slide >> 4) as i32) as i16;
            if self.volume > 64 {
                self.volume = 64;
            }
            self.volume = (self.volume as i32 - (self.vol_slide & 0xf) as i32) as i16;
            if self.volume < 0 {
                self.volume = 0;
            }
        }
    }

    fn pan_slide(&mut self, tick: i32, param: u8) {
        if param != 0 {
            self.pan_slide = param;
        }
        if tick != 0 {
            let mut lo = self.pan_slide & 0xf;
            let hi = self.pan_slide >> 4;
            if hi != 0 {
                lo = 0;
            }
            self.pan = (self.pan as i32 - lo as i32) as i16;
            self.pan = (self.pan as i32 + hi as i32) as i16;
            self.pan = (self.pan as i32).clamp(0, 255) as i16;
        }
    }
}

// ---- the player ---------------------------------------------------------------------

#[rustfmt::skip]
static XM_LINEAR: [i32; 768] = include!("xm_linear.in");

#[rustfmt::skip]
static XM_PERIOD: [u16; 104] = [
    29024, 28800, 28608, 28384, 28192, 28000, 27776, 27584, 27392, 27200, 27008, 26816, 26624, 26432, 26240, 26048,
    25856, 25664, 25472, 25312, 25120, 24928, 24768, 24576, 24384, 24224, 24064, 23872, 23712, 23552, 23360, 23200,
    23040, 22880, 22688, 22528, 22368, 22208, 22048, 21888, 21696, 21600, 21440, 21280, 21120, 20960, 20832, 20672,
    20480, 20352, 20224, 20096, 19936, 19808, 19648, 19520, 19328, 19232, 19104, 18944, 18816, 18688, 18560, 18400,
    18240, 18144, 18016, 17888, 17760, 17632, 17504, 17376, 17216, 17120, 17024, 16896, 16768, 16640, 16512, 16416,
    16256, 16160, 16064, 15936, 15808, 15712, 15584, 15488, 15360, 15264, 15168, 15040, 14944, 14816, 14720, 14624,
    14496, 14400, 14304, 14176, 14080, 13984, 13888, 13792,
];

/// One player's whole state.
#[derive(Clone)]
#[derive(Default)]
pub struct XmPlayer {
    /// 1 once the song has ended (no repeat)
    done: i32,
    /// the module in IOP memory, relocated
    module: u32,
    voices: Option<Box<XmVoices>>,
    /// tick within the row
    tick: i32,
    row: i32,
    pattern: i32,
    /// position in the order table
    order: i32,
    /// the pattern's packed rows, and the read offset into them
    row_data: u32,
    row_pos: i32,
    /// the pattern's packed size: 0 = empty
    rows: i32,
    /// 0..128
    global_vol: i32,
    /// -1, or the row a pattern break goes to
    break_row: i32,
    /// -1, or the order a position jump goes to
    jump_order: i32,
    seek: i32,
    /// ticks per row, and BPM
    speed: i16,
    tempo: i16,
    repeat: i32,
    /// 0..255, from the EE
    volume: i32,
    /// 0..255, 128 centre
    pan: i32,
    /// 4096 = 1.0
    pitch: i32,
    ch: [XmChannel; 32],
}


impl XmPlayer {
    pub fn set_repeat(&mut self, r: i32) {
        self.repeat = r;
    }
    pub fn tempo(&self) -> i32 {
        self.tempo as i32
    }
    pub fn set_volume(&mut self, v: i32) {
        self.volume = v;
    }
    pub fn set_pan(&mut self, p: i32) {
        self.pan = p;
    }
    pub fn set_pitch(&mut self, p: i32) {
        self.pitch = p;
    }
    pub fn pitch(&self) -> i32 {
        self.pitch
    }

    fn channels(&self, io: &Iop) -> i32 {
        io.r16(self.module + M_CHANNELS) as i16 as i32
    }

    pub fn play(&mut self, io: &Iop, lo: u32, module: u32, hi: u32) {
        self.module = module;
        self.voices = Some(Box::new(XmVoices::init(lo, hi)));
        self.repeat = 0;
        self.restart(io);
        self.done = 0;
        self.volume = 255;
        self.pan = 128;
        self.pitch = 4096;
    }

    pub fn stop(&mut self) {
        self.voices = None;
    }

    fn pattern_addr(&self, p: i32) -> u32 {
        self.module.wrapping_add(M_PATTERN).wrapping_add((p as u32).wrapping_mul(PAT_SIZE))
    }

    fn restart(&mut self, io: &Iop) {
        let m = self.module;
        self.speed = io.r16(m + M_SPEED) as i16;
        self.tempo = io.r16(m + M_TEMPO) as i16;
        self.tick = self.speed as i32;
        self.row = -1;
        self.order = 0;
        self.seek = 1;
        self.pattern = io.r8(m + M_ORDER + self.order as u32) as i32;
        let p = self.pattern_addr(self.pattern);
        self.row_data = io.r32(p + PAT_DATA);
        self.row_pos = 0;
        self.rows = io.r16(p + PAT_BYTES) as i16 as i32;
        self.break_row = -1;
        self.jump_order = -1;
        self.global_vol = 128;
        self.ch = [XmChannel::default(); 32];
    }

    /// One tracker tick: false once the song is over (or never started).
    pub fn process(&mut self, io: &mut Iop) -> bool {
        if self.done == 1 {
            return false;
        }
        if self.voices.is_none() {
            return false;
        }
        self.tick(io);
        let mut v = self.voices.take().unwrap();
        v.flush(io);
        self.voices = Some(v);
        true
    }

    fn tick(&mut self, io: &mut Iop) {
        let m = self.module;
        self.tick += 1;
        if self.tick >= self.speed as i32 {
            self.tick = 0;
            self.row += 1;
            let mut p = io.r8(m + M_ORDER + self.order as u32) as i32;
            self.seek = 0;
            let length = io.r16(m + M_LENGTH) as i16 as i32;
            if self.row == io.r16(self.pattern_addr(p) + PAT_ROWS) as i16 as i32 || self.break_row != -1 || self.jump_order != -1 {
                self.order += 1;
                if self.order >= length {
                    if self.repeat != 0 {
                        self.restart(io);
                    } else {
                        self.done = 1;
                    }
                    return;
                }
                p = io.r8(m + M_ORDER + self.order as u32) as i32;
                self.pattern = p;
                if self.break_row != -1 {
                    self.row = self.break_row;
                    if self.row != 0 {
                        self.seek = 1;
                    }
                } else {
                    self.row = 0;
                }
                if self.jump_order != -1 {
                    self.row = 0;
                    if self.jump_order < length {
                        self.order = self.jump_order;
                    }
                }
                self.tick = 0;
                let pa = self.pattern_addr(self.pattern);
                self.row_data = io.r32(pa + PAT_DATA);
                self.row_pos = 0;
                self.rows = io.r16(pa + PAT_BYTES) as i16 as i32;
                self.break_row = -1;
                self.jump_order = -1;
            }
            if self.rows != 0 {
                // a pattern break into the middle of a pattern skips to its
                // row -- but it is handed m_rowPos, which was just set to 0
                if self.seek != 0 {
                    self.skip_rows(io, self.row_pos);
                }
                for c in 0..self.channels(io) {
                    self.read_row(io, c as usize);
                }
            }
        } else {
            self.tick_effects(io);
        }
        self.update(io);
    }

    /// Step the packed pointer over `rows` rows. An unpacked note is skipped as
    /// SIX bytes, its five plus the flag byte counted twice.
    fn skip_rows(&mut self, io: &Iop, rows: i32) {
        let mut p = self.row_data;
        for _ in 0..rows {
            for _ in 0..self.channels(io) {
                let flags = io.r8(p);
                p = p.wrapping_add(1);
                if flags == 0x80 {
                    continue;
                }
                if flags & 0x80 != 0 {
                    for bit in [1, 2, 4, 8, 0x10] {
                        if flags & bit != 0 {
                            p = p.wrapping_add(1);
                        }
                    }
                } else {
                    p = p.wrapping_add(5);
                }
            }
        }
        self.row_pos = p.wrapping_sub(self.row_data) as i32;
    }

    /// One channel's slot of the row: XM's packing, a new note and instrument
    /// looked up, then the effects run for tick 0.
    fn read_row(&mut self, io: &Iop, chan: usize) {
        let m = self.module;
        let mut p = self.row_data.wrapping_add(self.row_pos as u32);
        let mut next = || {
            let b = io.r8(p);
            p = p.wrapping_add(1);
            b
        };
        let flags = next();
        let c = &mut self.ch[chan];
        c.note = 0;
        c.ins = 0;
        c.vol = 0;
        c.fx = 0;
        c.param = 0;
        if flags != 0x80 {
            if flags & 0x80 != 0 {
                if flags & 1 != 0 {
                    c.note = next();
                }
                if flags & 2 != 0 {
                    c.ins = next();
                }
                if flags & 4 != 0 {
                    c.vol = next();
                }
                if flags & 8 != 0 {
                    c.fx = next();
                }
                if flags & 0x10 != 0 {
                    c.param = next();
                }
            } else {
                c.note = flags;
                c.ins = next();
                c.vol = next();
                c.fx = next();
                c.param = next();
            }
            if c.note == 97 {
                self.key_off(chan);
            } else {
                let c = &mut self.ch[chan];
                if c.note != 0 {
                    c.fade_vol = 0x7fff;
                    c.key = c.note.wrapping_sub(1);
                    c.set_key(true);
                }
                if c.ins != 0 {
                    c.sample = 0;
                    if c.ins as i32 <= io.r16(m + M_INSTRUMENTS) as i16 as i32 {
                        c.inst = m.wrapping_add(M_INSTRUMENT).wrapping_add((c.ins as u32 - 1) * INS_SIZE);
                        if io.r8(c.inst + INS_SAMPLES) != 0 {
                            let first = io.r16(c.inst + INS_FIRST) as i16 as i32;
                            let map = io.r8(c.inst + INS_MAP + c.key as u32) as i32;
                            c.sample = self.module.wrapping_add(M_SAMPLE).wrapping_add(((first + map) as u32).wrapping_mul(SMP_SIZE));
                            if c.sample != 0 {
                                c.pan = io.r8(c.sample + SMP_PAN) as i16;
                                c.volume = io.r8(c.sample + SMP_VOLUME) as i16;
                                c.rel_note = io.r8(c.sample + SMP_RELNOTE);
                                c.finetune = io.r8(c.sample + SMP_FINETUNE) as i8 as i16;
                                c.reset();
                            }
                        }
                    }
                }
                if c.note != 0 {
                    let (key, rel, ft) = (c.key, c.rel_note, c.finetune);
                    let t = self.period(io, key.wrapping_add(rel), ft);
                    let c = &mut self.ch[chan];
                    c.target = t;
                    c.period = c.target;
                    if c.inst != 0 {
                        c.vol_env = if (io.r32(c.inst + INS_VOLENV + ENV_FLAGS) % 2) as u8 != 0 { c.inst + INS_VOLENV } else { 0 };
                        c.pan_env = if (io.r32(c.inst + INS_PANENV + ENV_FLAGS) % 2) as u8 != 0 { c.inst + INS_PANENV } else { 0 };
                        c.fadeout = io.r16(c.inst + INS_FADEOUT) as i16 as i32;
                    }
                }
            }
            self.effects(io, chan);
        }
        self.row_pos = p.wrapping_sub(self.row_data) as i32;
    }

    /// The ticks between rows run only the effects.
    fn tick_effects(&mut self, io: &Iop) {
        for i in 0..self.channels(io) {
            self.effects(io, i as usize);
        }
    }

    /// Tick 0 runs a note's effects with `tick` 0; slides act on later ticks.
    /// The volume column is decoded here too.
    fn effects(&mut self, io: &Iop, chan: usize) {
        let tick = self.tick;
        let c = &mut self.ch[chan];
        c.period_set = 0;
        c.volume_set = 0;
        let vol = c.vol;
        let fx = c.fx;
        let param = c.param;
        match vol >> 4 {
            6 => {
                if vol & 0xf != 0 {
                    c.vol_col_slide = vol & 0xf;
                }
                if tick != 0 {
                    c.volume = (c.volume as i32 - c.vol_col_slide as i32) as i16;
                    c.volume = c.volume.max(0);
                }
            }
            7 => {
                if vol & 0xf != 0 {
                    c.vol_col_slide = vol & 0xf;
                }
                if tick != 0 {
                    c.volume = (c.volume as i32 + c.vol_col_slide as i32) as i16;
                    c.volume = c.volume.min(64);
                }
            }
            8 => {
                if vol & 0xf != 0 {
                    c.vol_col_slide = vol & 0xf;
                }
                if tick == 0 {
                    c.volume = (c.volume as i32 - c.vol_col_slide as i32) as i16;
                    c.volume = c.volume.max(0);
                }
            }
            9 => {
                if vol & 0xf != 0 {
                    c.vol_col_slide = vol & 0xf;
                }
                if tick == 0 {
                    c.volume = (c.volume as i32 + c.vol_col_slide as i32) as i16;
                    c.volume = c.volume.min(64);
                }
            }
            10 => self.fx(io, chan, 4, vol.wrapping_shl(4)),
            11 => self.fx(io, chan, 4, vol & 0xf),
            12 => c.pan = ((vol & 0xf) << 4) as i16,
            13 => {
                if vol & 0xf != 0 {
                    c.pan_col_slide = vol & 0xf;
                }
                if tick != 0 {
                    c.pan = (c.pan as i32 - ((c.pan_col_slide as i32) << 2)) as i16;
                    c.pan = c.pan.max(0);
                }
            }
            14 => {
                if vol & 0xf != 0 {
                    c.pan_col_slide = vol & 0xf;
                }
                if tick != 0 {
                    c.pan = (c.pan as i32 + ((c.pan_col_slide as i32) << 2)) as i16;
                    c.pan = c.pan.min(255);
                }
            }
            15 => self.fx(io, chan, 3, vol.wrapping_shl(4)),
            _ => {
                if (0x10..=0x50).contains(&vol) {
                    self.fx(io, chan, 12, vol - 0x10);
                }
            }
        }
        match fx {
            // G: global volume
            0x10 => self.global_vol = (param as i32) << 1,
            // K: key off
            0x14 => self.key_off(chan),
            // P: panning slide
            0x19 => self.ch[chan].pan_slide(tick, param),
            // H, L, R, T, X: not done
            0x11 | 0x15 | 0x1b | 0x1d | 0x21 => {}
            _ => {
                if fx < 0x10 {
                    self.fx(io, chan, fx, param);
                }
            }
        }
        let c = &mut self.ch[chan];
        if c.period_set == 0 {
            c.out_period = c.period;
        }
        if c.volume_set == 0 {
            c.out_volume = c.volume;
        }
    }

    /// Effects 0..F.
    fn fx(&mut self, io: &Iop, chan: usize, cmd: u8, arg: u8) {
        let tick = self.tick;
        match cmd {
            // arpeggio
            0 => {
                if arg != 0 {
                    let c = &self.ch[chan];
                    let mut note = c.key;
                    match tick % 3 {
                        1 => note = note.wrapping_add(arg >> 4),
                        2 => note = note.wrapping_add(arg & 0xf),
                        _ => {}
                    }
                    let (rel, ft) = (c.rel_note, c.finetune);
                    let per = self.period(io, note.wrapping_add(rel), ft);
                    let c = &mut self.ch[chan];
                    c.out_period = per;
                    c.period_set = 1;
                }
            }
            // portamento up
            1 => {
                let c = &mut self.ch[chan];
                if arg != 0 {
                    c.slide = (arg as u16) << 2;
                }
                if tick != 0 {
                    c.period -= c.slide as i32;
                }
            }
            // portamento down
            2 => {
                let c = &mut self.ch[chan];
                if arg != 0 {
                    c.slide = (arg as u16) << 2;
                }
                if tick != 0 {
                    c.period += c.slide as i32;
                }
            }
            // tone portamento
            3 => {
                let c = &mut self.ch[chan];
                c.porta = 1;
                if arg != 0 {
                    c.porta_speed = arg as u16;
                    c.porta_speed <<= 2;
                }
                c.porta();
                c.period_set = 1;
            }
            // vibrato
            4 => {
                let c = &mut self.ch[chan];
                if arg & 0xf != 0 {
                    c.vib_depth = (arg & 0xf) as u16;
                }
                if arg & 0xf0 != 0 {
                    c.vib_speed = ((arg & 0xf0) >> 2) as u16;
                }
                c.vibrato(tick);
                c.period_set = 1;
            }
            // tone portamento + volume slide
            5 => {
                let c = &mut self.ch[chan];
                c.porta = 1;
                c.porta();
                if arg != 0 {
                    c.vol_slide = arg;
                }
                c.vol_slide(tick);
                c.period_set = 1;
            }
            // vibrato + volume slide
            6 => {
                let c = &mut self.ch[chan];
                c.vibrato(tick);
                if arg != 0 {
                    c.vol_slide = arg;
                }
                c.vol_slide(tick);
                c.period_set = 1;
            }
            // tremolo
            7 => {
                let c = &mut self.ch[chan];
                if arg & 0xf != 0 {
                    c.trem_depth = (arg & 0xf) as u16;
                }
                if arg & 0xf0 != 0 {
                    c.trem_speed = ((arg & 0xf0) >> 2) as u16;
                }
                c.tremolo(tick);
                c.volume_set = 1;
            }
            // panning
            8 => self.ch[chan].pan = arg as i16,
            // sample offset
            9 => {
                if tick == 0 {
                    let c = &mut self.ch[chan];
                    c.offset = if arg != 0 { arg as i32 * 256 } else { c.last_offset };
                }
            }
            // volume slide
            10 => {
                let c = &mut self.ch[chan];
                if arg != 0 {
                    c.vol_slide = arg;
                }
                c.vol_slide(tick);
            }
            // position jump
            11 => self.jump_order = arg as i32,
            // set volume
            12 => {
                if tick == 0 {
                    self.ch[chan].volume = if arg <= 64 { arg as i16 } else { 64 };
                }
            }
            // pattern break, the row in BCD
            13 => self.break_row = (arg >> 4) as i32 * 10 + (arg & 0xf) as i32,
            14 => self.ext_fx(chan, arg),
            // speed below 32, else BPM
            15
                if tick == 0 => {
                    if arg < 32 {
                        self.speed = arg as i16;
                    } else {
                        self.tempo = arg as i16;
                    }
                    self.tick = 0;
                }
            _ => {}
        }
    }

    /// Effects E0..EF. Fine portamento runs the other way from XM's: E1 adds.
    fn ext_fx(&mut self, chan: usize, param: u8) {
        let tick = self.tick;
        let c = &mut self.ch[chan];
        let lo = param & 0xf;
        match param >> 4 {
            1 => {
                if tick == 0 {
                    if lo != 0 {
                        c.fine_porta = lo;
                    }
                    c.period += (c.fine_porta as i32) << 2;
                }
            }
            2 => {
                if tick == 0 {
                    if lo != 0 {
                        c.fine_porta = lo;
                    }
                    c.period -= (c.fine_porta as i32) << 2;
                }
            }
            3 => c.glissando = lo,
            4 => {
                c.waves &= 0xf0;
                c.waves |= lo as u16;
            }
            7 => {
                c.waves &= 0xf;
                c.waves |= (lo as u16) << 4;
            }
            10 => {
                if tick == 0 {
                    if lo != 0 {
                        c.vol_col_slide = lo;
                    }
                    c.volume = (c.volume as i32 + c.vol_col_slide as i32) as i16;
                    c.volume = c.volume.min(64);
                }
            }
            11 => {
                if tick == 0 {
                    if lo != 0 {
                        c.vol_col_slide = lo;
                    }
                    c.volume = (c.volume as i32 - c.vol_col_slide as i32) as i16;
                    c.volume = c.volume.max(0);
                }
            }
            // note cut
            12
                if tick >= lo as i32 => {
                    c.volume = 0;
                }
            _ => {}
        }
    }

    /// The tick's result to the voices: volume through the envelope, fadeout,
    /// global and the EE's volume; pan through the EE's pan and the pan
    /// envelope. A fresh key-on retriggers the sample, anything else only
    /// updates it.
    fn update(&mut self, io: &mut Iop) {
        let mut voices = self.voices.take().unwrap();
        for i in 0..self.channels(io) as usize {
            self.ch[i].envelopes(io);
            let c = self.ch[i];
            let mut c = c;
            if c.key_on == 0 && c.vol_env != 0 {
                c.fade_vol -= c.fadeout;
                if c.fade_vol < 0 {
                    c.fade_vol = 0;
                }
            }
            let mut pan = c.pan as i32;
            pan = pan.clamp(0, 255);
            pan += (self.pan - 128).wrapping_mul(128 - (pan - 128).abs()) / 128;
            pan += (c.env_pan as i32 - 128).wrapping_mul(128 - (pan - 128).abs()) / 128;
            let mut vol = c.out_volume as i32 * 255 / 64;
            vol = c.env_vol as i32 * vol / 64;
            vol = vol.wrapping_mul(c.fade_vol) / 0x7fff;
            let mut l = (255 - pan).wrapping_mul(vol) >> 8;
            let mut r = vol.wrapping_mul(pan) >> 8;
            l = l.wrapping_mul(self.global_vol) >> 7;
            r = r.wrapping_mul(self.global_vol) >> 7;
            l = l.wrapping_mul(self.volume) >> 8;
            r = r.wrapping_mul(self.volume) >> 8;
            l >>= 2;
            r >>= 2;
            if c.key_on != 0 && c.keyed == 0 && c.porta == 0 {
                if c.sample != 0 && io.r32(c.sample + SMP_LENGTH) as i32 > 0 {
                    let length = io.r32(c.sample + SMP_LENGTH) as i32;
                    if c.offset >= length {
                        c.offset = length - 1;
                    }
                    let rate = (self.rate(io, c.out_period).wrapping_mul(self.pitch) >> 12) as u32;
                    let spu = io.r32(c.sample + SMP_SPU);
                    let loop_type = io.r32(c.sample + SMP_FLAGS) & 3;
                    let loop_start = io.r32(c.sample + SMP_LOOPSTART) as i32;
                    voices.key_on(io, i, spu, loop_type, loop_start, rate, l, r);
                    voices.set_offset(io, i, c.offset as u32);
                }
            } else {
                voices.set_volume(io, i, l, r);
                let rate = (self.rate(io, c.out_period).wrapping_mul(self.pitch) >> 12) as u32;
                voices.set_pitch(io, i, rate);
            }
            c.keyed = c.key_on;
            c.offset = 0;
            c.porta = 0;
            self.ch[i] = c;
        }
        self.voices = Some(voices);
    }

    /// Amiga periods: the table's octave 0, finetune interpolated between two
    /// of its columns, shifted down by the octave. A negative result is tried
    /// again with finetune 0.
    fn amiga_period(note: u8, mut finetune: i16) -> i32 {
        loop {
            let n = note % 12;
            let oct = note / 12;
            let idx = (finetune >> 4) as i32 + ((n as i32) << 3);
            // finetune below 0 on C reads one entry before the table, which
            // on the IOP is the high half of the linear table's last entry
            let tab = |k: i32| if k < 0 { (XM_LINEAR[767] as u32 >> 16) as u16 } else { XM_PERIOD.get(k as usize).copied().unwrap_or(0) };
            let lo = tab(idx) as i32;
            let hi = tab(idx + 1) as i32;
            let period = (interp(finetune / 16, 0, 15, lo as i16, hi as i16) as i32) >> oct;
            if period < 0 && finetune != 0 {
                finetune = 0;
                continue;
            }
            return period;
        }
    }

    /// Linear periods: 10 octaves of 768.
    fn linear_period(note: u8, finetune: i16) -> i32 {
        7680 - note as i32 * 64 - (finetune / 2) as i32
    }

    fn linear(&self, io: &Iop) -> bool {
        (io.r16(self.module + M_FLAGS) % 2) as u8 != 0
    }

    fn period(&self, io: &Iop, note: u8, finetune: i16) -> i32 {
        if self.linear(io) { Self::linear_period(note, finetune) } else { Self::amiga_period(note, finetune) }
    }

    /// A period as an SPU rate, 4096 = 48 kHz before the player's pitch.
    fn rate(&self, io: &Iop, period: i32) -> i32 {
        if self.linear(io) {
            let k = period % 768;
            let v = if (0..768).contains(&k) { XM_LINEAR[k as usize] } else { 0 };
            v.wrapping_shr((period / 768) as u32)
        } else {
            let rate = if period == 0 { 0 } else { (0xa3ba68 / period) >> 3 };
            rate.wrapping_mul(48000) / 4096
        }
    }

    fn key_off(&mut self, chan: usize) {
        let c = &mut self.ch[chan];
        c.set_key(false);
        if c.vol_env == 0 {
            c.volume = 0;
        }
    }
}

/// A slot of `gXmSlot[8]`: the voice masks the status reply carries, the
/// module's address while playing, and the tick accumulator.
#[derive(Clone, Default)]
pub struct XmSlot {
    pub lo: u32,
    pub hi: u32,
    pub active: u32,
    pub player: XmPlayer,
    pub tick: i32,
}
