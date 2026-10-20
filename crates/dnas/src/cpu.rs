//! An interpreter for the Emotion Engine's integer core, enough of it to run
//! the DNAS library: MIPS III with the R5900's 128-bit registers (`lq`/`sq`
//! and the MMI that move them), the three-operand `mult`, COP0's count and
//! the FPU's moves and compares. Semantics are PCSX2's interpreter's, since
//! PCSX2 is what produced the images this is checked against.

pub const RAM_SIZE: usize = 32 << 20;
const SPR_SIZE: usize = 0x4000;

/// Why the interpreter stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    /// An access outside RAM and the scratchpad.
    Fault { pc: u32, va: u32 },
    /// An instruction this interpreter does not implement.
    Unimplemented { pc: u32, insn: u32 },
    Syscall { pc: u32, insn: u32 },
    Break { pc: u32, insn: u32 },
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Trap::Fault { pc, va } => write!(f, "bad address {va:#010x} at pc {pc:#010x}"),
            Trap::Unimplemented { pc, insn } => write!(f, "unimplemented instruction {insn:#010x} at pc {pc:#010x}"),
            Trap::Syscall { pc, insn } => write!(f, "syscall {insn:#010x} at pc {pc:#010x}"),
            Trap::Break { pc, insn } => write!(f, "break {insn:#010x} at pc {pc:#010x}"),
        }
    }
}

/// EE main memory (32 MB, seen through kseg0/kseg1 and the uncached
/// mirrors) and the 16 KB scratchpad.
pub struct Mem {
    pub ram: Vec<u8>,
    pub spr: Vec<u8>,
    /// The decoded words of a stretch of RAM (`watch_code`, the library's
    /// image), from `code_at`: a write there undoes the decoding of each word
    /// it touches, so code that is rewritten runs as rewritten.
    code: Vec<Insn>,
    code_at: usize,
    /// The functions computed natively (`native.rs`), entry and length: the
    /// entry decodes to `Op::Native`, its index here. A write into one's
    /// bytes leaves it to the interpreter from then on (its entry becomes
    /// `u32::MAX`), since what was proved is no longer the code.
    pub(crate) natives: Vec<(u32, u32, &'static str)>,
    /// While set, each write is first recorded here with the bytes it is
    /// about to replace, keyed by RAM offset (the scratchpad's from
    /// `SPR_KEY`): the undo log `Machine::check_native` compares runs by.
    pub(crate) log: Option<Vec<(usize, Vec<u8>)>>,
}

/// Where the scratchpad's offsets start in `Mem::log`'s keys: past RAM's,
/// within a 32-bit `usize` (the crate runs in a page's workers too).
pub(crate) const SPR_KEY: usize = 1 << 30;

impl Default for Mem {
    fn default() -> Self {
        Mem { ram: vec![0; RAM_SIZE], spr: vec![0; SPR_SIZE], code: Vec::new(), code_at: 0, natives: Vec::new(), log: None }
    }
}

impl Mem {
    /// The byte offset of `va` in RAM (`Ok`) or the scratchpad (`Err(Some)`).
    #[inline(always)]
    fn map(va: u32, len: usize) -> Result<usize, Option<usize>> {
        match va >> 28 {
            0x0 | 0x2 | 0x3 | 0x8 | 0x9 | 0xA | 0xB => {
                let p = (va & 0x1FFF_FFFF) as usize & 0x0FFF_FFFF;
                if p + len <= RAM_SIZE { Ok(p) } else { Err(None) }
            }
            0x7 => {
                let p = (va & 0x0FFF_FFFF) as usize;
                if p + len <= SPR_SIZE { Err(Some(p)) } else { Err(None) }
            }
            _ => Err(None),
        }
    }

    #[inline(always)]
    pub fn slice(&self, va: u32, len: usize) -> Option<&[u8]> {
        match Self::map(va, len) {
            Ok(p) => Some(&self.ram[p..p + len]),
            Err(Some(p)) => Some(&self.spr[p..p + len]),
            Err(None) => None,
        }
    }

    #[inline(always)]
    pub fn slice_mut(&mut self, va: u32, len: usize) -> Option<&mut [u8]> {
        match Self::map(va, len) {
            Ok(p) => {
                let at = p.wrapping_sub(self.code_at);
                if at < self.code.len() * 4 || (p < self.code_at && p + len > self.code_at) {
                    self.undecode(p, len);
                }
                if let Some(log) = &mut self.log {
                    log.push((p, self.ram[p..p + len].to_vec()));
                }
                Some(&mut self.ram[p..p + len])
            }
            Err(Some(p)) => {
                if let Some(log) = &mut self.log {
                    log.push((SPR_KEY + p, self.spr[p..p + len].to_vec()));
                }
                Some(&mut self.spr[p..p + len])
            }
            Err(None) => None,
        }
    }

    /// The `len` bytes at a `log` key.
    pub(crate) fn logged(&self, key: usize, len: usize) -> &[u8] {
        if key >= SPR_KEY { &self.spr[key - SPR_KEY..key - SPR_KEY + len] } else { &self.ram[key..key + len] }
    }

    /// Puts bytes back at a `log` key (no logging, no undecoding: what it
    /// restores is what was there).
    pub(crate) fn restore(&mut self, key: usize, bytes: &[u8]) {
        let to = if key >= SPR_KEY { &mut self.spr[key - SPR_KEY..key - SPR_KEY + bytes.len()] } else { &mut self.ram[key..key + bytes.len()] };
        to.copy_from_slice(bytes);
    }

    /// The words of `len` bytes of RAM from `p` are to be decoded again.
    #[cold]
    fn undecode(&mut self, p: usize, len: usize) {
        let first = p.saturating_sub(self.code_at) / 4;
        let end = ((p + len).saturating_sub(self.code_at)).div_ceil(4).min(self.code.len());
        for d in &mut self.code[first.min(end)..end] {
            d.op = Op::Undecoded;
        }
        for (at, n, _) in &mut self.natives {
            // RAM below 0x20000000 is mapped as is
            let (lo, hi) = (*at as usize & 0x1FFF_FFFF, (*at as usize & 0x1FFF_FFFF) + *n as usize);
            if *at != u32::MAX && p < hi && p + len > lo {
                let entry = (lo.wrapping_sub(self.code_at)) / 4;
                if let Some(d) = self.code.get_mut(entry) {
                    d.op = Op::Undecoded;
                }
                *at = u32::MAX;
            }
        }
    }

    /// Words from `va` (in RAM) for `len` bytes run decoded (`Cpu::run`).
    pub fn watch_code(&mut self, va: u32, len: u32) {
        if let Ok(p) = Self::map(va, len as usize) {
            self.code_at = p;
            self.code = vec![Insn::UNDECODED; len as usize / 4];
        }
    }

    #[inline(always)]
    pub fn r8(&self, va: u32) -> Option<u8> {
        self.slice(va, 1).map(|b| b[0])
    }
    #[inline(always)]
    pub fn r16(&self, va: u32) -> Option<u16> {
        self.slice(va, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    #[inline(always)]
    pub fn r32(&self, va: u32) -> Option<u32> {
        self.slice(va, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    #[inline(always)]
    pub fn r64(&self, va: u32) -> Option<u64> {
        self.slice(va, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    #[inline(always)]
    pub fn w8(&mut self, va: u32, v: u8) -> Option<()> {
        self.slice_mut(va, 1).map(|b| b[0] = v)
    }
    #[inline(always)]
    pub fn w16(&mut self, va: u32, v: u16) -> Option<()> {
        self.slice_mut(va, 2).map(|b| b.copy_from_slice(&v.to_le_bytes()))
    }
    #[inline(always)]
    pub fn w32(&mut self, va: u32, v: u32) -> Option<()> {
        self.slice_mut(va, 4).map(|b| b.copy_from_slice(&v.to_le_bytes()))
    }
    #[inline(always)]
    pub fn w64(&mut self, va: u32, v: u64) -> Option<()> {
        self.slice_mut(va, 8).map(|b| b.copy_from_slice(&v.to_le_bytes()))
    }
}

#[inline(always)]
fn sx32(v: u64) -> u64 {
    v as u32 as i32 as i64 as u64
}

/// The register file. `gpr` holds each register's low 64 bits, `gpu` the
/// high 64 that only `lq`/`sq` and the MMI touch (PCSX2 writes `UD[0]` for
/// every other instruction, so the high half survives them).
#[derive(Clone)]
pub struct Cpu {
    pub gpr: [u64; 32],
    pub gpu: [u64; 32],
    pub hi: u64,
    pub lo: u64,
    pub hi1: u64,
    pub lo1: u64,
    pub sa: u64,
    pub pc: u32,
    pub npc: u32,
    pub fpr: [u32; 32],
    pub fcr31: u32,
    pub facc: u32,
    /// Instructions retired; COP0 `Count` reads it.
    pub insns: u64,
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu {
            gpr: [0; 32],
            gpu: [0; 32],
            hi: 0,
            lo: 0,
            hi1: 0,
            lo1: 0,
            sa: 0,
            pc: 0,
            npc: 4,
            fpr: [0; 32],
            fcr31: 0,
            facc: 0,
            insns: 0,
        }
    }
}

pub const SP: usize = 29;
pub const RA: usize = 31;
pub const V0: usize = 2;
pub const V1: usize = 3;
pub const A0: usize = 4;

impl Cpu {
    #[inline(always)]
    fn set(&mut self, r: usize, v: u64) {
        if r != 0 {
            self.gpr[r] = v;
        }
    }

    /// Jumps to `pc` with no delay slot pending.
    pub fn jump(&mut self, pc: u32) {
        self.pc = pc;
        self.npc = pc.wrapping_add(4);
    }

    #[inline(always)]
    fn branch(&mut self, taken: bool, cur: u32, imm: u64) {
        if taken {
            self.npc = cur.wrapping_add(4).wrapping_add((imm << 2) as u32);
        }
    }

    /// A branch-likely: when not taken its delay slot is skipped.
    #[inline(always)]
    fn branch_likely(&mut self, taken: bool, cur: u32, imm: u64) {
        if taken {
            self.npc = cur.wrapping_add(4).wrapping_add((imm << 2) as u32);
        } else {
            self.pc = self.npc;
            self.npc = self.pc.wrapping_add(4);
        }
    }

    /// Executes the instruction at `pc`.
    #[inline(always)]
    pub fn step(&mut self, m: &mut Mem) -> Result<(), Trap> {
        let cur = self.pc;
        let insn = m.r32(cur).ok_or(Trap::Fault { pc: cur, va: cur })?;
        self.pc = self.npc;
        self.npc = self.npc.wrapping_add(4);
        self.insns += 1;
        self.exec(m, insn, cur)
    }

    fn exec(&mut self, m: &mut Mem, insn: u32, cur: u32) -> Result<(), Trap> {
        let op = insn >> 26;
        let rs = ((insn >> 21) & 31) as usize;
        let rt = ((insn >> 16) & 31) as usize;
        let rd = ((insn >> 11) & 31) as usize;
        let sa = (insn >> 6) & 31;
        let imm = insn as u16 as i16 as i64 as u64;
        let uimm = (insn & 0xFFFF) as u64;
        let unimpl = Trap::Unimplemented { pc: cur, insn };
        let fault = |va: u32| Trap::Fault { pc: cur, va };
        let ea = || (self.gpr[rs] as u32).wrapping_add(imm as u32);
        match op {
            0x00 => return self.special(insn, cur, rs, rt, rd, sa),
            0x01 => {
                let v = self.gpr[rs] as i64;
                match rt {
                    0x00 => self.branch(v < 0, cur, imm),
                    0x01 => self.branch(v >= 0, cur, imm),
                    0x02 => self.branch_likely(v < 0, cur, imm),
                    0x03 => self.branch_likely(v >= 0, cur, imm),
                    0x10 => {
                        self.gpr[RA] = cur.wrapping_add(8) as u64;
                        self.branch(v < 0, cur, imm)
                    }
                    0x11 => {
                        self.gpr[RA] = cur.wrapping_add(8) as u64;
                        self.branch(v >= 0, cur, imm)
                    }
                    0x12 => {
                        self.gpr[RA] = cur.wrapping_add(8) as u64;
                        self.branch_likely(v < 0, cur, imm)
                    }
                    0x13 => {
                        self.gpr[RA] = cur.wrapping_add(8) as u64;
                        self.branch_likely(v >= 0, cur, imm)
                    }
                    0x18 => self.sa = ((self.gpr[rs] as u32 ^ uimm as u32) & 15) as u64 * 8, // MTSAB
                    0x19 => self.sa = ((self.gpr[rs] as u32 ^ uimm as u32) & 7) as u64 * 16, // MTSAH
                    _ => return Err(unimpl),
                }
            }
            0x02 => self.npc = (cur.wrapping_add(4) & 0xF000_0000) | ((insn & 0x03FF_FFFF) << 2),
            0x03 => {
                self.gpr[RA] = cur.wrapping_add(8) as u64;
                self.npc = (cur.wrapping_add(4) & 0xF000_0000) | ((insn & 0x03FF_FFFF) << 2);
            }
            0x04 => self.branch(self.gpr[rs] == self.gpr[rt], cur, imm),
            0x05 => self.branch(self.gpr[rs] != self.gpr[rt], cur, imm),
            0x06 => self.branch(self.gpr[rs] as i64 <= 0, cur, imm),
            0x07 => self.branch(self.gpr[rs] as i64 > 0, cur, imm),
            0x08 | 0x09 => self.set(rt, sx32(self.gpr[rs].wrapping_add(imm))),
            0x0A => self.set(rt, ((self.gpr[rs] as i64) < imm as i64) as u64),
            0x0B => self.set(rt, (self.gpr[rs] < imm) as u64),
            0x0C => self.set(rt, self.gpr[rs] & uimm),
            0x0D => self.set(rt, self.gpr[rs] | uimm),
            0x0E => self.set(rt, self.gpr[rs] ^ uimm),
            0x0F => self.set(rt, sx32(uimm << 16)),
            0x10 => return self.cop0(insn, cur, rs, rt, rd),
            0x11 => return self.cop1(m, insn, cur, rs, rt, rd, imm),
            0x14 => self.branch_likely(self.gpr[rs] == self.gpr[rt], cur, imm),
            0x15 => self.branch_likely(self.gpr[rs] != self.gpr[rt], cur, imm),
            0x16 => self.branch_likely(self.gpr[rs] as i64 <= 0, cur, imm),
            0x17 => self.branch_likely(self.gpr[rs] as i64 > 0, cur, imm),
            0x18 | 0x19 => self.set(rt, self.gpr[rs].wrapping_add(imm)),
            0x1A => {
                // LDL
                let a = ea();
                let shift = (a & 7) as usize;
                let mem = m.r64(a & !7).ok_or(fault(a))?;
                const MASK: [u64; 8] = [
                    0x00ff_ffff_ffff_ffff,
                    0x0000_ffff_ffff_ffff,
                    0x0000_00ff_ffff_ffff,
                    0x0000_0000_ffff_ffff,
                    0x0000_0000_00ff_ffff,
                    0x0000_0000_0000_ffff,
                    0x0000_0000_0000_00ff,
                    0,
                ];
                const SHIFT: [u32; 8] = [56, 48, 40, 32, 24, 16, 8, 0];
                self.set(rt, (self.gpr[rt] & MASK[shift]) | (mem << SHIFT[shift]));
            }
            0x1B => {
                // LDR
                let a = ea();
                let shift = (a & 7) as usize;
                let mem = m.r64(a & !7).ok_or(fault(a))?;
                const MASK: [u64; 8] = [
                    0,
                    0xff00_0000_0000_0000,
                    0xffff_0000_0000_0000,
                    0xffff_ff00_0000_0000,
                    0xffff_ffff_0000_0000,
                    0xffff_ffff_ff00_0000,
                    0xffff_ffff_ffff_0000,
                    0xffff_ffff_ffff_ff00,
                ];
                const SHIFT: [u32; 8] = [0, 8, 16, 24, 32, 40, 48, 56];
                self.set(rt, (self.gpr[rt] & MASK[shift]) | (mem >> SHIFT[shift]));
            }
            0x1C => return self.mmi(insn, cur, rs, rt, rd, sa),
            0x1E => {
                // LQ
                let a = ea() & !15;
                let lo = m.r64(a).ok_or(fault(a))?;
                let hi = m.r64(a + 8).ok_or(fault(a))?;
                if rt != 0 {
                    self.gpr[rt] = lo;
                    self.gpu[rt] = hi;
                }
            }
            0x1F => {
                // SQ
                let a = ea() & !15;
                let (lo, hi) = (self.gpr[rt], self.gpu[rt]);
                m.w64(a, lo).ok_or(fault(a))?;
                m.w64(a + 8, hi).ok_or(fault(a))?;
            }
            0x20 => {
                let a = ea();
                let v = m.r8(a).ok_or(fault(a))?;
                self.set(rt, v as i8 as i64 as u64);
            }
            0x21 => {
                let a = ea();
                let v = m.r16(a).ok_or(fault(a))?;
                self.set(rt, v as i16 as i64 as u64);
            }
            0x22 => {
                // LWL
                let a = ea();
                let shift = (a & 3) as usize;
                let mem = m.r32(a & !3).ok_or(fault(a))?;
                const MASK: [u32; 4] = [0x00ff_ffff, 0x0000_ffff, 0x0000_00ff, 0];
                const SHIFT: [u32; 4] = [24, 16, 8, 0];
                let v = (self.gpr[rt] as u32 & MASK[shift]) | (mem << SHIFT[shift]);
                self.set(rt, v as i32 as i64 as u64);
            }
            0x23 => {
                let a = ea();
                let v = m.r32(a).ok_or(fault(a))?;
                self.set(rt, v as i32 as i64 as u64);
            }
            0x24 => {
                let a = ea();
                let v = m.r8(a).ok_or(fault(a))?;
                self.set(rt, v as u64);
            }
            0x25 => {
                let a = ea();
                let v = m.r16(a).ok_or(fault(a))?;
                self.set(rt, v as u64);
            }
            0x26 => {
                // LWR
                let a = ea();
                let shift = (a & 3) as usize;
                let mem = m.r32(a & !3).ok_or(fault(a))?;
                const MASK: [u32; 4] = [0, 0xff00_0000, 0xffff_0000, 0xffff_ff00];
                const SHIFT: [u32; 4] = [0, 8, 16, 24];
                let v = (self.gpr[rt] as u32 & MASK[shift]) | (mem >> SHIFT[shift]);
                if rt != 0 {
                    if shift == 0 {
                        self.gpr[rt] = v as i32 as i64 as u64;
                    } else {
                        self.gpr[rt] = (self.gpr[rt] & !0xFFFF_FFFF) | v as u64;
                    }
                }
            }
            0x27 => {
                let a = ea();
                let v = m.r32(a).ok_or(fault(a))?;
                self.set(rt, v as u64);
            }
            0x28 => {
                let a = ea();
                m.w8(a, self.gpr[rt] as u8).ok_or(fault(a))?;
            }
            0x29 => {
                let a = ea();
                m.w16(a, self.gpr[rt] as u16).ok_or(fault(a))?;
            }
            0x2A => {
                // SWL
                let a = ea();
                let shift = (a & 3) as usize;
                let mem = m.r32(a & !3).ok_or(fault(a))?;
                const MASK: [u32; 4] = [0xffff_ff00, 0xffff_0000, 0xff00_0000, 0];
                const SHIFT: [u32; 4] = [24, 16, 8, 0];
                m.w32(a & !3, ((self.gpr[rt] as u32) >> SHIFT[shift]) | (mem & MASK[shift])).ok_or(fault(a))?;
            }
            0x2B => {
                let a = ea();
                m.w32(a, self.gpr[rt] as u32).ok_or(fault(a))?;
            }
            0x2C => {
                // SDL
                let a = ea();
                let shift = (a & 7) as usize;
                let mem = m.r64(a & !7).ok_or(fault(a))?;
                const MASK: [u64; 8] = [
                    0xffff_ffff_ffff_ff00,
                    0xffff_ffff_ffff_0000,
                    0xffff_ffff_ff00_0000,
                    0xffff_ffff_0000_0000,
                    0xffff_ff00_0000_0000,
                    0xffff_0000_0000_0000,
                    0xff00_0000_0000_0000,
                    0,
                ];
                const SHIFT: [u32; 8] = [56, 48, 40, 32, 24, 16, 8, 0];
                m.w64(a & !7, (self.gpr[rt] >> SHIFT[shift]) | (mem & MASK[shift])).ok_or(fault(a))?;
            }
            0x2D => {
                // SDR
                let a = ea();
                let shift = (a & 7) as usize;
                let mem = m.r64(a & !7).ok_or(fault(a))?;
                const MASK: [u64; 8] = [
                    0,
                    0x0000_0000_0000_00ff,
                    0x0000_0000_0000_ffff,
                    0x0000_0000_00ff_ffff,
                    0x0000_0000_ffff_ffff,
                    0x0000_00ff_ffff_ffff,
                    0x0000_ffff_ffff_ffff,
                    0x00ff_ffff_ffff_ffff,
                ];
                const SHIFT: [u32; 8] = [0, 8, 16, 24, 32, 40, 48, 56];
                m.w64(a & !7, (self.gpr[rt] << SHIFT[shift]) | (mem & MASK[shift])).ok_or(fault(a))?;
            }
            0x2E => {
                // SWR
                let a = ea();
                let shift = (a & 3) as usize;
                let mem = m.r32(a & !3).ok_or(fault(a))?;
                const MASK: [u32; 4] = [0, 0x0000_00ff, 0x0000_ffff, 0x00ff_ffff];
                const SHIFT: [u32; 4] = [0, 8, 16, 24];
                m.w32(a & !3, ((self.gpr[rt] as u32) << SHIFT[shift]) | (mem & MASK[shift])).ok_or(fault(a))?;
            }
            0x2F | 0x33 => {} // CACHE, PREF
            0x31 => {
                // LWC1
                let a = ea();
                self.fpr[rt] = m.r32(a).ok_or(fault(a))?;
            }
            0x37 => {
                let a = ea();
                let v = m.r64(a).ok_or(fault(a))?;
                self.set(rt, v);
            }
            0x39 => {
                // SWC1
                let a = ea();
                m.w32(a, self.fpr[rt]).ok_or(fault(a))?;
            }
            0x3F => {
                let a = ea();
                m.w64(a, self.gpr[rt]).ok_or(fault(a))?;
            }
            _ => return Err(unimpl),
        }
        Ok(())
    }

    fn special(&mut self, insn: u32, cur: u32, rs: usize, rt: usize, rd: usize, sa: u32) -> Result<(), Trap> {
        let s = self.gpr[rs];
        let t = self.gpr[rt];
        match insn & 0x3F {
            0x00 => self.set(rd, sx32(((t as u32) << sa) as u64)),
            0x02 => self.set(rd, sx32(((t as u32) >> sa) as u64)),
            0x03 => self.set(rd, ((t as u32 as i32) >> sa) as i64 as u64),
            0x04 => self.set(rd, sx32(((t as u32) << (s & 31)) as u64)),
            0x06 => self.set(rd, sx32(((t as u32) >> (s & 31)) as u64)),
            0x07 => self.set(rd, ((t as u32 as i32) >> (s & 31)) as i64 as u64),
            0x08 => self.npc = s as u32,
            0x09 => {
                self.set(rd, cur.wrapping_add(8) as u64);
                self.npc = s as u32;
            }
            0x0A => {
                if t == 0 {
                    self.set(rd, s)
                }
            }
            0x0B => {
                if t != 0 {
                    self.set(rd, s)
                }
            }
            0x0C => return Err(Trap::Syscall { pc: cur, insn }),
            0x0D => return Err(Trap::Break { pc: cur, insn }),
            0x0F => {} // SYNC
            0x10 => self.set(rd, self.hi),
            0x11 => self.hi = s,
            0x12 => self.set(rd, self.lo),
            0x13 => self.lo = s,
            0x14 => self.set(rd, t << (s & 63)),
            0x16 => self.set(rd, t >> (s & 63)),
            0x17 => self.set(rd, ((t as i64) >> (s & 63)) as u64),
            0x18 => {
                let r = (s as u32 as i32 as i64).wrapping_mul(t as u32 as i32 as i64);
                self.lo = sx32(r as u64);
                self.hi = sx32((r >> 32) as u64);
                self.set(rd, self.lo);
            }
            0x19 => {
                let r = (s as u32 as u64).wrapping_mul(t as u32 as u64);
                self.lo = sx32(r);
                self.hi = sx32(r >> 32);
                self.set(rd, self.lo);
            }
            0x1A => {
                let (n, d) = (s as u32 as i32, t as u32 as i32);
                (self.lo, self.hi) = div32(n, d);
            }
            0x1B => {
                let (n, d) = (s as u32, t as u32);
                (self.lo, self.hi) = divu32(n, d);
            }
            0x20 | 0x21 => self.set(rd, sx32(s.wrapping_add(t))),
            0x22 | 0x23 => self.set(rd, sx32(s.wrapping_sub(t))),
            0x24 => self.set(rd, s & t),
            0x25 => self.set(rd, s | t),
            0x26 => self.set(rd, s ^ t),
            0x27 => self.set(rd, !(s | t)),
            0x28 => self.set(rd, self.sa),
            0x29 => self.sa = s,
            0x2A => self.set(rd, ((s as i64) < (t as i64)) as u64),
            0x2B => self.set(rd, (s < t) as u64),
            0x2C | 0x2D => self.set(rd, s.wrapping_add(t)),
            0x2E | 0x2F => self.set(rd, s.wrapping_sub(t)),
            0x38 => self.set(rd, t << sa),
            0x3A => self.set(rd, t >> sa),
            0x3B => self.set(rd, ((t as i64) >> sa) as u64),
            0x3C => self.set(rd, t << (sa + 32)),
            0x3E => self.set(rd, t >> (sa + 32)),
            0x3F => self.set(rd, ((t as i64) >> (sa + 32)) as u64),
            _ => return Err(Trap::Unimplemented { pc: cur, insn }),
        }
        Ok(())
    }

    fn cop0(&mut self, insn: u32, cur: u32, rs: usize, rt: usize, rd: usize) -> Result<(), Trap> {
        match rs {
            0x00 => {
                // MFC0: Count is the only register anything here reads
                let v = if rd == 9 { self.insns as u32 } else { 0 };
                self.set(rt, v as i32 as i64 as u64);
            }
            0x04 => {} // MTC0
            0x10 if matches!(insn & 0x3F, 0x38 | 0x39) => {} // EI, DI
            _ => return Err(Trap::Unimplemented { pc: cur, insn }),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn cop1(&mut self, m: &mut Mem, insn: u32, cur: u32, rs: usize, rt: usize, rd: usize, imm: u64) -> Result<(), Trap> {
        let _ = m;
        let fs = rd;
        match rs {
            0x00 => self.set(rt, self.fpr[fs] as i32 as i64 as u64),
            0x02 => self.set(rt, if fs == 31 { self.fcr31 } else if fs == 0 { 0x2E30 } else { 0 } as i32 as i64 as u64),
            0x04 => self.fpr[fs] = self.gpr[rt] as u32,
            0x06 => {
                if fs == 31 {
                    self.fcr31 = self.gpr[rt] as u32
                }
            }
            0x08 => {
                let c = self.fcr31 & (1 << 23) != 0;
                match rt {
                    0 => self.branch(!c, cur, imm),
                    1 => self.branch(c, cur, imm),
                    2 => self.branch_likely(!c, cur, imm),
                    3 => self.branch_likely(c, cur, imm),
                    _ => return Err(Trap::Unimplemented { pc: cur, insn }),
                }
            }
            0x10 => {
                let ft = rt;
                let fd = ((insn >> 6) & 31) as usize;
                match insn & 0x3F {
                    0x06 => self.fpr[fd] = self.fpr[fs], // MOV.S
                    0x07 => self.fpr[fd] = self.fpr[fs] ^ 0x8000_0000,
                    0x05 => self.fpr[fd] = self.fpr[fs] & 0x7FFF_FFFF,
                    0x30 => self.cmp(false),
                    0x32 => {
                        let eq = f32::from_bits(self.fpr[fs]) == f32::from_bits(self.fpr[ft]);
                        self.cmp(eq)
                    }
                    0x34 => {
                        let lt = f32::from_bits(self.fpr[fs]) < f32::from_bits(self.fpr[ft]);
                        self.cmp(lt)
                    }
                    0x36 => {
                        let le = f32::from_bits(self.fpr[fs]) <= f32::from_bits(self.fpr[ft]);
                        self.cmp(le)
                    }
                    _ => return Err(Trap::Unimplemented { pc: cur, insn }),
                }
            }
            _ => return Err(Trap::Unimplemented { pc: cur, insn }),
        }
        Ok(())
    }

    fn cmp(&mut self, c: bool) {
        if c {
            self.fcr31 |= 1 << 23;
        } else {
            self.fcr31 &= !(1 << 23);
        }
    }

    fn mmi(&mut self, insn: u32, cur: u32, rs: usize, rt: usize, rd: usize, sa: u32) -> Result<(), Trap> {
        let unimpl = Trap::Unimplemented { pc: cur, insn };
        match insn & 0x3F {
            0x00 | 0x01 | 0x20 | 0x21 => {
                // MADD, MADDU, MADD1, MADDU1
                let one = insn & 0x20 != 0;
                let (hi, lo) = if one { (self.hi1, self.lo1) } else { (self.hi, self.lo) };
                let acc = ((hi as u32 as u64) << 32) | lo as u32 as u64;
                let r = if insn & 1 == 0 {
                    (acc as i64).wrapping_add((self.gpr[rs] as u32 as i32 as i64).wrapping_mul(self.gpr[rt] as u32 as i32 as i64)) as u64
                } else {
                    acc.wrapping_add((self.gpr[rs] as u32 as u64).wrapping_mul(self.gpr[rt] as u32 as u64))
                };
                let (nlo, nhi) = (sx32(r), sx32(r >> 32));
                if one {
                    (self.lo1, self.hi1) = (nlo, nhi);
                } else {
                    (self.lo, self.hi) = (nlo, nhi);
                }
                self.set(rd, nlo);
            }
            0x10 => self.set(rd, self.hi1),
            0x11 => self.hi1 = self.gpr[rs],
            0x12 => self.set(rd, self.lo1),
            0x13 => self.lo1 = self.gpr[rs],
            0x18 => {
                let r = (self.gpr[rs] as u32 as i32 as i64).wrapping_mul(self.gpr[rt] as u32 as i32 as i64);
                self.lo1 = sx32(r as u64);
                self.hi1 = sx32((r >> 32) as u64);
                self.set(rd, self.lo1);
            }
            0x19 => {
                let r = (self.gpr[rs] as u32 as u64).wrapping_mul(self.gpr[rt] as u32 as u64);
                self.lo1 = sx32(r);
                self.hi1 = sx32(r >> 32);
                self.set(rd, self.lo1);
            }
            0x1A => (self.lo1, self.hi1) = div32(self.gpr[rs] as u32 as i32, self.gpr[rt] as u32 as i32),
            0x1B => (self.lo1, self.hi1) = divu32(self.gpr[rs] as u32, self.gpr[rt] as u32),
            0x08 => {
                // MMI0
                match sa {
                    0x12 => {
                        // PEXTLW
                        let (s, t) = (self.gpr[rs], self.gpr[rt]);
                        let lo = (t & 0xFFFF_FFFF) | ((s & 0xFFFF_FFFF) << 32);
                        let hi = (t >> 32) | (s & 0xFFFF_FFFF_0000_0000);
                        self.set128(rd, lo, hi);
                    }
                    _ => return Err(unimpl),
                }
            }
            0x09 => {
                // MMI2
                match sa {
                    0x0E => {
                        // PCPYLD
                        let (lo, hi) = (self.gpr[rt], self.gpr[rs]);
                        self.set128(rd, lo, hi);
                    }
                    0x12 => {
                        // PAND
                        let (lo, hi) = (self.gpr[rs] & self.gpr[rt], self.gpu[rs] & self.gpu[rt]);
                        self.set128(rd, lo, hi);
                    }
                    0x13 => {
                        // PXOR
                        let (lo, hi) = (self.gpr[rs] ^ self.gpr[rt], self.gpu[rs] ^ self.gpu[rt]);
                        self.set128(rd, lo, hi);
                    }
                    _ => return Err(unimpl),
                }
            }
            0x29 => {
                // MMI3
                match sa {
                    0x0E => {
                        // PCPYUD
                        let (lo, hi) = (self.gpu[rs], self.gpu[rt]);
                        self.set128(rd, lo, hi);
                    }
                    0x12 => {
                        // POR
                        let (lo, hi) = (self.gpr[rs] | self.gpr[rt], self.gpu[rs] | self.gpu[rt]);
                        self.set128(rd, lo, hi);
                    }
                    0x13 => {
                        // PNOR
                        let (lo, hi) = (!(self.gpr[rs] | self.gpr[rt]), !(self.gpu[rs] | self.gpu[rt]));
                        self.set128(rd, lo, hi);
                    }
                    _ => return Err(unimpl),
                }
            }
            _ => return Err(unimpl),
        }
        Ok(())
    }

    #[inline(always)]
    fn set128(&mut self, r: usize, lo: u64, hi: u64) {
        if r != 0 {
            self.gpr[r] = lo;
            self.gpu[r] = hi;
        }
    }
}

// ---- decoded execution --------------------------------------------------------
//
// The same instructions as `exec`, each word decoded once (`Mem::watch_code`):
// its fields taken out, a branch's or jump's target worked out from its own
// address, and one arm a word. What has no arm here goes to `exec`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Op {
    Undecoded,
    /// `exec` runs it
    Exec,
    /// the entry of a function computed natively (`imm`: its index in
    /// `Mem::natives`)
    Native,
    Sll,
    Srl,
    Sra,
    Sllv,
    Srlv,
    Srav,
    Jr,
    Jalr,
    Movz,
    Movn,
    Mfhi,
    Mthi,
    Mflo,
    Mtlo,
    Dsllv,
    Dsrlv,
    Dsrav,
    Mult,
    Multu,
    Div,
    Divu,
    Addu,
    Subu,
    And,
    Or,
    Xor,
    Nor,
    Slt,
    Sltu,
    Daddu,
    Dsubu,
    Dsll,
    Dsrl,
    Dsra,
    Dsll32,
    Dsrl32,
    Dsra32,
    Bltz,
    Bgez,
    Bltzl,
    Bgezl,
    J,
    Jal,
    Beq,
    Bne,
    Blez,
    Bgtz,
    Beql,
    Bnel,
    Blezl,
    Bgtzl,
    Addiu,
    Slti,
    Sltiu,
    Andi,
    Ori,
    Xori,
    Lui,
    Daddiu,
    Lb,
    Lh,
    Lw,
    Lbu,
    Lhu,
    Lwu,
    Ld,
    Sb,
    Sh,
    Sw,
    Sd,
}

/// One word, decoded. `imm` is what the op takes: the immediate sign- or
/// zero-extended as the instruction does, `lui`'s shifted, or a branch's or
/// jump's target; `word` is the instruction, for `exec`.
#[derive(Clone, Copy, Debug)]
pub struct Insn {
    op: Op,
    rs: u8,
    rt: u8,
    rd: u8,
    sa: u8,
    imm: u32,
    word: u32,
}

impl Insn {
    const UNDECODED: Insn = Insn { op: Op::Undecoded, rs: 0, rt: 0, rd: 0, sa: 0, imm: 0, word: 0 };
}

/// The word at `pc`, decoded.
fn decode(word: u32, pc: u32) -> Insn {
    let rs = ((word >> 21) & 31) as u8;
    let rt = ((word >> 16) & 31) as u8;
    let rd = ((word >> 11) & 31) as u8;
    let sa = ((word >> 6) & 31) as u8;
    let simm = word as u16 as i16 as i32 as u32;
    let zimm = word & 0xFFFF;
    let target = pc.wrapping_add(4).wrapping_add(simm << 2);
    let jump = (pc.wrapping_add(4) & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2);
    let (op, imm) = match word >> 26 {
        0x00 => (
            match word & 0x3F {
                0x00 => Op::Sll,
                0x02 => Op::Srl,
                0x03 => Op::Sra,
                0x04 => Op::Sllv,
                0x06 => Op::Srlv,
                0x07 => Op::Srav,
                0x08 => Op::Jr,
                0x09 => Op::Jalr,
                0x0A => Op::Movz,
                0x0B => Op::Movn,
                0x10 => Op::Mfhi,
                0x11 => Op::Mthi,
                0x12 => Op::Mflo,
                0x13 => Op::Mtlo,
                0x14 => Op::Dsllv,
                0x16 => Op::Dsrlv,
                0x17 => Op::Dsrav,
                0x18 => Op::Mult,
                0x19 => Op::Multu,
                0x1A => Op::Div,
                0x1B => Op::Divu,
                0x20 | 0x21 => Op::Addu,
                0x22 | 0x23 => Op::Subu,
                0x24 => Op::And,
                0x25 => Op::Or,
                0x26 => Op::Xor,
                0x27 => Op::Nor,
                0x2A => Op::Slt,
                0x2B => Op::Sltu,
                0x2C | 0x2D => Op::Daddu,
                0x2E | 0x2F => Op::Dsubu,
                0x38 => Op::Dsll,
                0x3A => Op::Dsrl,
                0x3B => Op::Dsra,
                0x3C => Op::Dsll32,
                0x3E => Op::Dsrl32,
                0x3F => Op::Dsra32,
                _ => Op::Exec,
            },
            0,
        ),
        0x01 => match rt {
            0x00 => (Op::Bltz, target),
            0x01 => (Op::Bgez, target),
            0x02 => (Op::Bltzl, target),
            0x03 => (Op::Bgezl, target),
            _ => (Op::Exec, 0),
        },
        0x02 => (Op::J, jump),
        0x03 => (Op::Jal, jump),
        0x04 => (Op::Beq, target),
        0x05 => (Op::Bne, target),
        0x06 => (Op::Blez, target),
        0x07 => (Op::Bgtz, target),
        0x08 | 0x09 => (Op::Addiu, simm),
        0x0A => (Op::Slti, simm),
        0x0B => (Op::Sltiu, simm),
        0x0C => (Op::Andi, zimm),
        0x0D => (Op::Ori, zimm),
        0x0E => (Op::Xori, zimm),
        0x0F => (Op::Lui, zimm << 16),
        0x14 => (Op::Beql, target),
        0x15 => (Op::Bnel, target),
        0x16 => (Op::Blezl, target),
        0x17 => (Op::Bgtzl, target),
        0x18 | 0x19 => (Op::Daddiu, simm),
        0x20 => (Op::Lb, simm),
        0x21 => (Op::Lh, simm),
        0x23 => (Op::Lw, simm),
        0x24 => (Op::Lbu, simm),
        0x25 => (Op::Lhu, simm),
        0x27 => (Op::Lwu, simm),
        0x37 => (Op::Ld, simm),
        0x28 => (Op::Sb, simm),
        0x29 => (Op::Sh, simm),
        0x2B => (Op::Sw, simm),
        0x3F => (Op::Sd, simm),
        _ => (Op::Exec, 0),
    };
    Insn { op, rs, rt, rd, sa, imm, word }
}

/// Why `Cpu::run` returned.
pub enum Stop {
    /// `pc` left the decoded stretch, or the instruction budget ran out
    Left,
    /// `pc` is at the entry of `Mem::natives[k]`, not yet run
    Native(usize),
}

impl Cpu {
    /// Runs decoded words while `pc` stays in `Mem::watch_code`'s stretch, at
    /// most until `insns` reaches `end`; returns at the first `pc` outside it
    /// (a stand-in, the caller's return address), which the caller steps, or
    /// at a native function's entry.
    pub fn run(&mut self, m: &mut Mem, end: u64) -> Result<Stop, Trap> {
        let base = m.code_at as u32;
        while self.insns < end {
            let cur = self.pc;
            let i = (cur.wrapping_sub(base) >> 2) as usize;
            if cur & 3 != 0 || i >= m.code.len() {
                return Ok(Stop::Left);
            }
            let mut d = m.code[i];
            if d.op == Op::Undecoded {
                let word = m.r32(cur).ok_or(Trap::Fault { pc: cur, va: cur })?;
                d = match m.natives.iter().position(|&(a, _, _)| a == cur) {
                    Some(k) => Insn { op: Op::Native, imm: k as u32, word, ..Insn::UNDECODED },
                    None => decode(word, cur),
                };
                m.code[i] = d;
            }
            if d.op == Op::Native {
                return Ok(Stop::Native(d.imm as usize));
            }
            self.pc = self.npc;
            self.npc = self.npc.wrapping_add(4);
            self.insns += 1;
            self.exec_decoded(m, d, cur)?;
        }
        Ok(Stop::Left)
    }

    #[inline(always)]
    fn exec_decoded(&mut self, m: &mut Mem, d: Insn, cur: u32) -> Result<(), Trap> {
        let (rs, rt, rd) = (d.rs as usize, d.rt as usize, d.rd as usize);
        let s = self.gpr[rs];
        let t = self.gpr[rt];
        let simm = d.imm as i32 as i64 as u64;
        let ea = (s as u32).wrapping_add(d.imm);
        let fault = |va: u32| Trap::Fault { pc: cur, va };
        let sa = d.sa as u32;
        match d.op {
            Op::Undecoded | Op::Native => unreachable!(),
            Op::Exec => return self.exec(m, d.word, cur),
            Op::Sll => self.set(rd, sx32(((t as u32) << sa) as u64)),
            Op::Srl => self.set(rd, sx32(((t as u32) >> sa) as u64)),
            Op::Sra => self.set(rd, ((t as u32 as i32) >> sa) as i64 as u64),
            Op::Sllv => self.set(rd, sx32(((t as u32) << (s & 31)) as u64)),
            Op::Srlv => self.set(rd, sx32(((t as u32) >> (s & 31)) as u64)),
            Op::Srav => self.set(rd, ((t as u32 as i32) >> (s & 31)) as i64 as u64),
            Op::Jr => self.npc = s as u32,
            Op::Jalr => {
                self.set(rd, cur.wrapping_add(8) as u64);
                self.npc = s as u32;
            }
            Op::Movz => {
                if t == 0 {
                    self.set(rd, s)
                }
            }
            Op::Movn => {
                if t != 0 {
                    self.set(rd, s)
                }
            }
            Op::Mfhi => self.set(rd, self.hi),
            Op::Mthi => self.hi = s,
            Op::Mflo => self.set(rd, self.lo),
            Op::Mtlo => self.lo = s,
            Op::Dsllv => self.set(rd, t << (s & 63)),
            Op::Dsrlv => self.set(rd, t >> (s & 63)),
            Op::Dsrav => self.set(rd, ((t as i64) >> (s & 63)) as u64),
            Op::Mult => {
                let r = (s as u32 as i32 as i64).wrapping_mul(t as u32 as i32 as i64);
                self.lo = sx32(r as u64);
                self.hi = sx32((r >> 32) as u64);
                self.set(rd, self.lo);
            }
            Op::Multu => {
                let r = (s as u32 as u64).wrapping_mul(t as u32 as u64);
                self.lo = sx32(r);
                self.hi = sx32(r >> 32);
                self.set(rd, self.lo);
            }
            Op::Div => (self.lo, self.hi) = div32(s as u32 as i32, t as u32 as i32),
            Op::Divu => (self.lo, self.hi) = divu32(s as u32, t as u32),
            Op::Addu => self.set(rd, sx32(s.wrapping_add(t))),
            Op::Subu => self.set(rd, sx32(s.wrapping_sub(t))),
            Op::And => self.set(rd, s & t),
            Op::Or => self.set(rd, s | t),
            Op::Xor => self.set(rd, s ^ t),
            Op::Nor => self.set(rd, !(s | t)),
            Op::Slt => self.set(rd, ((s as i64) < (t as i64)) as u64),
            Op::Sltu => self.set(rd, (s < t) as u64),
            Op::Daddu => self.set(rd, s.wrapping_add(t)),
            Op::Dsubu => self.set(rd, s.wrapping_sub(t)),
            Op::Dsll => self.set(rd, t << sa),
            Op::Dsrl => self.set(rd, t >> sa),
            Op::Dsra => self.set(rd, ((t as i64) >> sa) as u64),
            Op::Dsll32 => self.set(rd, t << (sa + 32)),
            Op::Dsrl32 => self.set(rd, t >> (sa + 32)),
            Op::Dsra32 => self.set(rd, ((t as i64) >> (sa + 32)) as u64),
            Op::Bltz => self.take((s as i64) < 0, d.imm),
            Op::Bgez => self.take((s as i64) >= 0, d.imm),
            Op::Bltzl => self.take_likely((s as i64) < 0, d.imm),
            Op::Bgezl => self.take_likely((s as i64) >= 0, d.imm),
            Op::J => self.npc = d.imm,
            Op::Jal => {
                self.gpr[RA] = cur.wrapping_add(8) as u64;
                self.npc = d.imm;
            }
            Op::Beq => self.take(s == t, d.imm),
            Op::Bne => self.take(s != t, d.imm),
            Op::Blez => self.take(s as i64 <= 0, d.imm),
            Op::Bgtz => self.take(s as i64 > 0, d.imm),
            Op::Beql => self.take_likely(s == t, d.imm),
            Op::Bnel => self.take_likely(s != t, d.imm),
            Op::Blezl => self.take_likely(s as i64 <= 0, d.imm),
            Op::Bgtzl => self.take_likely(s as i64 > 0, d.imm),
            Op::Addiu => self.set(rt, sx32(s.wrapping_add(simm))),
            Op::Slti => self.set(rt, ((s as i64) < simm as i64) as u64),
            Op::Sltiu => self.set(rt, (s < simm) as u64),
            Op::Andi => self.set(rt, s & d.imm as u64),
            Op::Ori => self.set(rt, s | d.imm as u64),
            Op::Xori => self.set(rt, s ^ d.imm as u64),
            Op::Lui => self.set(rt, sx32(d.imm as u64)),
            Op::Daddiu => self.set(rt, s.wrapping_add(simm)),
            Op::Lb => {
                let v = m.r8(ea).ok_or(fault(ea))?;
                self.set(rt, v as i8 as i64 as u64);
            }
            Op::Lh => {
                let v = m.r16(ea).ok_or(fault(ea))?;
                self.set(rt, v as i16 as i64 as u64);
            }
            Op::Lw => {
                let v = m.r32(ea).ok_or(fault(ea))?;
                self.set(rt, v as i32 as i64 as u64);
            }
            Op::Lbu => {
                let v = m.r8(ea).ok_or(fault(ea))?;
                self.set(rt, v as u64);
            }
            Op::Lhu => {
                let v = m.r16(ea).ok_or(fault(ea))?;
                self.set(rt, v as u64);
            }
            Op::Lwu => {
                let v = m.r32(ea).ok_or(fault(ea))?;
                self.set(rt, v as u64);
            }
            Op::Ld => {
                let v = m.r64(ea).ok_or(fault(ea))?;
                self.set(rt, v);
            }
            Op::Sb => m.w8(ea, t as u8).ok_or(fault(ea))?,
            Op::Sh => m.w16(ea, t as u16).ok_or(fault(ea))?,
            Op::Sw => m.w32(ea, t as u32).ok_or(fault(ea))?,
            Op::Sd => m.w64(ea, t).ok_or(fault(ea))?,
        }
        Ok(())
    }

    /// A branch to `target` when `taken`, after its delay slot.
    #[inline(always)]
    fn take(&mut self, taken: bool, target: u32) {
        if taken {
            self.npc = target;
        }
    }

    /// A branch-likely: when not taken its delay slot is skipped.
    #[inline(always)]
    fn take_likely(&mut self, taken: bool, target: u32) {
        if taken {
            self.npc = target;
        } else {
            self.pc = self.npc;
            self.npc = self.pc.wrapping_add(4);
        }
    }
}

/// `DIV` as PCSX2 does it, the overflow and divide-by-zero cases included.
fn div32(n: i32, d: i32) -> (u64, u64) {
    if n == i32::MIN && d == -1 {
        (i32::MIN as i64 as u64, 0)
    } else if d != 0 {
        ((n / d) as i64 as u64, (n % d) as i64 as u64)
    } else {
        (if n < 0 { 1 } else { -1i64 as u64 }, n as i64 as u64)
    }
}

fn divu32(n: u32, d: u32) -> (u64, u64) {
    match (n.checked_div(d), n.checked_rem(d)) {
        (Some(q), Some(r)) => (sx32(q as u64), sx32(r as u64)),
        _ => (-1i64 as u64, n as i32 as i64 as u64),
    }
}
