//! The IOP as SNDDRV sees it: two regions of its memory the EE writes into by
//! SIF DMA (the ADPCM bounce buffer and the XM modules' pool), and libsd over
//! the SPU2.
//!
//! An XM module lives in the pool as the bytes the EE sent, and is read in
//! place through IOP addresses after `XmModule::Relocate` has turned its
//! offsets into pointers, as SNDDRV reads it. The addresses are the module's
//! own: the default bases are SNDDRV's (`gIopBuf` and `gXmPool`) on a module
//! loaded at 0x80000; the oracle test passes the oracle's.

use crate::spu2::Spu2;

pub const IOP_BUF_SIZE: usize = 0x10000;
pub const XM_POOL_SIZE: usize = 0x19000;
pub const DEFAULT_IOP_BUF: u32 = 0x80000 + 0x11298;
pub const DEFAULT_XM_POOL: u32 = 0x80000 + 0x21298;

pub struct Iop {
    pub buf_addr: u32,
    pub buf: Vec<u8>,
    pub pool_addr: u32,
    pub pool: Vec<u8>,
    pub spu: Spu2,
}

impl Iop {
    pub fn new(buf_addr: u32, pool_addr: u32) -> Iop {
        Iop { buf_addr, buf: vec![0; IOP_BUF_SIZE], pool_addr, pool: vec![0; XM_POOL_SIZE], spu: Spu2::default() }
    }

    fn at(&self, addr: u32, n: usize) -> Option<&[u8]> {
        let in_region = |base: u32, mem: &[u8]| {
            let off = addr.wrapping_sub(base) as usize;
            (off < mem.len() && off + n <= mem.len()).then_some(off)
        };
        if let Some(off) = in_region(self.pool_addr, &self.pool) {
            return Some(&self.pool[off..off + n]);
        }
        if let Some(off) = in_region(self.buf_addr, &self.buf) {
            return Some(&self.buf[off..off + n]);
        }
        None
    }

    /// The EE's SIF DMA: bytes into IOP memory at an IOP address. A block
    /// that lies inside one of the two regions (every upload's) is one copy:
    /// natively this runs under the lock the output's callback mixes under.
    pub fn dma(&mut self, addr: u32, data: &[u8]) {
        for (base, mem) in [(self.pool_addr, &mut self.pool), (self.buf_addr, &mut self.buf)] {
            let off = addr.wrapping_sub(base) as usize;
            if off < mem.len() && data.len() <= mem.len() - off {
                mem[off..off + data.len()].copy_from_slice(data);
                return;
            }
        }
        for (k, &b) in data.iter().enumerate() {
            self.w8(addr.wrapping_add(k as u32), b);
        }
    }

    // Memory outside the two regions reads as zero and ignores writes: on the
    // console it is the rest of the IOP, which no well-formed module reaches.
    pub fn r8(&self, addr: u32) -> u8 {
        self.at(addr, 1).map_or(0, |b| b[0])
    }
    pub fn r16(&self, addr: u32) -> u16 {
        self.at(addr, 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn r32(&self, addr: u32) -> u32 {
        self.at(addr, 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn w8(&mut self, addr: u32, v: u8) {
        let off = addr.wrapping_sub(self.pool_addr) as usize;
        if off < self.pool.len() {
            self.pool[off] = v;
            return;
        }
        let off = addr.wrapping_sub(self.buf_addr) as usize;
        if off < self.buf.len() {
            self.buf[off] = v;
        }
    }
    pub fn w32(&mut self, addr: u32, v: u32) {
        for (k, b) in v.to_le_bytes().into_iter().enumerate() {
            self.w8(addr.wrapping_add(k as u32), b);
        }
    }

    // ---- libsd, as SNDDRV imports it --------------------------------------------
    // voice parameter  (param << 8) | (voice << 1) | core      param 0..7
    // core parameter   (param << 8) | core                     param 8..18
    // switch           (switch << 8) | core                    KON 0x15, KOFF 0x16
    // voice address    (addr << 8) | 0x40 | (voice << 1) | core SSA 0x20, LSAX 0x21, NAX 0x22

    pub fn sd_init(&mut self) {
        self.spu.reset();
    }

    pub fn sd_set_param(&mut self, entry: u16, value: u16) {
        let param = (entry >> 8) as i32;
        let core = (entry & 1) as i32;
        let voice = ((entry >> 1) & 0x1f) as i32;
        if param <= 18 {
            self.spu.set_param(core, voice, param, value);
        }
    }

    pub fn sd_get_param(&self, entry: u16) -> u16 {
        self.spu.get_param((entry & 1) as i32, ((entry >> 1) & 0x1f) as i32, (entry >> 8) as i32)
    }

    pub fn sd_set_switch(&mut self, entry: u16, value: u32) {
        let sw = entry >> 8;
        let core = (entry & 1) as i32;
        if sw == 0x15 {
            self.spu.key_on(core, value & 0xffffff);
        } else if sw == 0x16 {
            self.spu.key_off(core, value & 0xffffff);
        }
    }

    pub fn sd_set_addr(&mut self, entry: u16, value: u32) {
        let a = (entry >> 8) as i32 - 0x20;
        if (0..=2).contains(&a) {
            self.spu.set_addr((entry & 1) as i32, ((entry >> 1) & 0x1f) as i32, a, value);
        }
    }

    pub fn sd_get_addr(&self, entry: u16) -> u32 {
        let a = (entry >> 8) as i32 - 0x20;
        if (0..=2).contains(&a) {
            return self.spu.get_addr((entry & 1) as i32, ((entry >> 1) & 0x1f) as i32, a);
        }
        0
    }
}
