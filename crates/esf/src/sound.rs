//! Sound records, as `VIESFParse::ParseAdpcmObj` and `ParseXmObj` read them.
//!
//! A wave (0xB000) is a 0xB010 header (DictID, channels, 16-byte ADPCM blocks
//! per channel, sample rate, volume, pan, loop) and 0xB020, the ADPCM: for a
//! stereo wave, the left channel's blocks then the right's. An XM (0xB030) is
//! 0xB040, the DictID, volume and pan then the packed module SNDDRV plays in
//! place, and 0xB060, its samples' ADPCM bank.

use crate::{Container, NodeId, f32le, i32le};

pub const WAVE: u16 = 0xB000;
pub const XM: u16 = 0xB030;

#[derive(Clone, Debug)]
pub struct Wave {
    pub id: u32,
    pub channels: i32,
    pub blocks: i32,
    pub rate: i32,
    pub volume: f32,
    pub pan: f32,
    /// 1 when the wave loops: one more block, which `VIWave::Allocate` counts
    pub looped: i32,
    /// `blocks * channels * 16` bytes
    pub adpcm: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Xm {
    pub id: u32,
    pub volume: f32,
    pub pan: f32,
    /// the module, its first word 0 (the parser rejects any other)
    pub module: Vec<u8>,
    pub samples: Vec<u8>,
}

/// The wave a 0xB000 record holds.
pub fn wave(c: &Container, n: NodeId) -> Option<Wave> {
    let head = c.child(n, 0xB010)?;
    let b = c.body(head);
    if b.len() < 28 {
        return None;
    }
    let channels = i32le(b, 4);
    let blocks = i32le(b, 8);
    let size = (blocks.max(0) as usize) * ((channels.max(0) as usize) << 4);
    let data = c.body(c.child(n, 0xB020)?);
    Some(Wave {
        id: i32le(b, 0) as u32,
        channels,
        blocks,
        rate: i32le(b, 12),
        volume: f32le(b, 16),
        pan: f32le(b, 20),
        looped: i32le(b, 24),
        adpcm: data[..size.min(data.len())].to_vec(),
    })
}

/// The XM a 0xB030 record holds.
pub fn xm(c: &Container, n: NodeId) -> Option<Xm> {
    let head = c.child(n, 0xB040)?;
    let node = c.node(head);
    let b = c.body(head);
    let (volume, pan, at) = if node.version != 0 { (f32le(b, 4), f32le(b, 8), 12) } else { (1.0, 0.0, 4) };
    let len = b.len().checked_sub(12)?;
    let module = b.get(at..at + len)?.to_vec();
    if module.len() < 4 || i32le(&module, 0) != 0 {
        return None;
    }
    let samples = c.body(c.child(n, 0xB060)?).to_vec();
    Some(Xm { id: i32le(b, 0) as u32, volume, pan, module, samples })
}

/// Every sound record in the container, in file order.
pub fn all(c: &Container) -> (Vec<Wave>, Vec<Xm>) {
    let (mut waves, mut xms) = (Vec::new(), Vec::new());
    for (i, node) in c.nodes.iter().enumerate() {
        match node.ty {
            WAVE => waves.extend(wave(c, i)),
            XM => xms.extend(xm(c, i)),
            _ => {}
        }
    }
    (waves, xms)
}
