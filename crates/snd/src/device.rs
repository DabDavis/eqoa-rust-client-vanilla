//! `VISoundDevice`, `VIWave` and `VIXm`: the EE half of the sound system,
//! line by line from the decomp (SUPPORT's `visounddevice.cpp`, `viwave.cpp`,
//! `vixm.cpp`).
//!
//! It owns 46 playback records, 128 loaded sounds, a first-fit allocator over
//! SPU RAM and another over the IOP's XM pool, one upload in flight, and the
//! BGM cross-fade, and it says nothing to the IOP directly: every operation is
//! a command appended to a packet that is shipped over SIF RPC (`Link::call`),
//! and every upload a SIF DMA (`Link::dma`). The packet buffer and the
//! argument buffer are kept whole between calls, as the statics they are, so
//! the packets are the console's to the byte.
//!
//! The float arithmetic is generic over `vi::Model`: `Ee` in the client, `Ieee`
//! against eqoa-port's oracle. A float to int conversion truncates on both.
//!
//! Two departures, both where the console misbehaves (F3 20261025-190120,
//! sounds garbled until a restart; `tests/device_upload.rs`): a sound let go
//! while its upload is in flight leaves that upload nothing to mark done
//! (`delete_sound`; the console writes through a pointer into the deleted
//! sound, which the next sound made may occupy), and a wave that finds no SPU
//! RAM is let go (`create_wave`; the console keeps it at address 0 and
//! uploads zeros there).
//!
//! More of the same kind (F3 20261025-225337, crunchy again with the first
//! fixed), where the console loses an upload's command: the packet is 128
//! words and a busy frame's own sounds fill it, and the console drops what
//! does not fit, the copy of a block included, so the sound was marked
//! uploaded with whatever its RAM held; the packet is now sent first to make
//! room (`idle_transfer`). A block is not sent into the IOP's buffer while
//! the packet still holds, unsent, the copy of the last one (the console
//! guards only a copy that opens it, `gSndPacket[1] == 1`), nor a module
//! while a relocation waits. And a stop that finds the packet full heads the
//! next one (`stop_channels`): dropped, its voices played on with no sound to
//! stop them. None of these changes a packet the decomp's device sends where
//! the console loses nothing (`tests/oracle_snddev.rs`).

use std::marker::PhantomData;
use vi::Model;

/// Where the packets and the uploads go: SNDDRV, on this host or another.
pub trait Link {
    /// An RPC: the 512-byte packet, answered with the 512-byte status block.
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128];
    /// A SIF DMA of `data` to IOP address `addr`, complete on return.
    fn dma(&mut self, addr: u32, data: &[u8]);
    /// `sceSifCheckStatRpc`: the last call is still running, so a packet
    /// cannot go yet (it stays, and grows, until one can).
    fn busy(&mut self) -> bool {
        false
    }
    /// Whether the BGM `path` can start streaming now: always on a console
    /// and natively; a page's disc has to fetch the file first
    /// (`Disc::ready`), and the request waits for it.
    fn bgm_ready(&mut self, _path: &str) -> bool {
        true
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Sfx = 0,
    Music = 1,
}

/// A wave: ADPCM, one voice or a stereo pair.
#[derive(Clone, Debug, Default)]
pub struct Wave {
    pub format: i32,
    pub channel_count: i32,
    pub sample_size: i32,
    pub sample_frame_count: i32,
    pub sample_rate: i32,
    pub looped: i32,
    pub cached: i32,
    pub volume: f32,
    pub pan: f32,
    /// the EE copy, until its upload has started
    pub buffer: Option<Vec<u8>>,
    pub size: i32,
    pub spu_addr: u32,
    /// the CRC-32 of the `size` bytes its upload sends (`check_ram`)
    pub crc: u32,
}

/// A tracker module and its sample bank.
#[derive(Clone, Debug, Default)]
pub struct Xm {
    pub spu_addr: u32,
    pub xm_addr: u32,
    pub xm_cached: i32,
    pub sample_cached: i32,
    /// the module; it stays after its upload (`ChannelCount` reads it)
    pub xm: Option<Vec<u8>>,
    pub xm_size: i32,
    /// the sample bank, until its upload has started
    pub sample: Option<Vec<u8>>,
    pub spu_size: i32,
    /// the CRC-32 of the sample bank (`check_ram`)
    pub sample_crc: u32,
    pub repeat: i32,
    pub volume: f32,
    pub pan: f32,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Wave(Wave),
    Xm(Xm),
}

#[derive(Clone, Debug)]
pub struct Sound {
    pub id: u32,
    pub link: i32,
    pub kind: Kind,
}

impl Sound {
    pub fn is_cached(&self) -> bool {
        match &self.kind {
            Kind::Wave(w) => w.cached != 0,
            Kind::Xm(x) => x.sample_cached != 0 && x.xm_cached != 0,
        }
    }
    pub fn volume(&self) -> f32 {
        match &self.kind {
            Kind::Wave(w) => w.volume,
            Kind::Xm(x) => x.volume,
        }
    }
    pub fn pan(&self) -> f32 {
        match &self.kind {
            Kind::Wave(w) => w.pan,
            Kind::Xm(x) => x.pan,
        }
    }
    pub fn set_volume(&mut self, v: f32) {
        match &mut self.kind {
            Kind::Wave(w) => w.volume = v,
            Kind::Xm(x) => x.volume = v,
        }
    }
    pub fn set_pan(&mut self, p: f32) {
        match &mut self.kind {
            Kind::Wave(w) => w.pan = p,
            Kind::Xm(x) => x.pan = p,
        }
    }
    fn channel_count(&self) -> i32 {
        match &self.kind {
            Kind::Wave(w) => w.channel_count,
            // VIXm::ChannelCount reads a short at +8 of the module
            Kind::Xm(x) => x.xm.as_ref().map_or(0, |m| i16::from_le_bytes([m[8], m[9]]) as i32),
        }
    }
}

/// What a command needs of a sound.
#[derive(Clone, Copy)]
enum View {
    Wave { spu_addr: u32, size: i32, rate: i32, volume: f32 },
    Xm { xm_addr: u32, repeat: i32, volume: f32 },
}

impl Sound {
    fn view(&self) -> View {
        match &self.kind {
            Kind::Wave(w) => View::Wave { spu_addr: w.spu_addr, size: w.size, rate: w.sample_rate, volume: w.volume },
            Kind::Xm(x) => View::Xm { xm_addr: x.xm_addr, repeat: x.repeat, volume: x.volume },
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Playback {
    channels: u64,
    priority: i32,
    sound: i32,
    /// slot | (serial << 8)
    handle: i32,
    pitch: i32,
    volume: f32,
    pan: f32,
    /// the device's frame when it started
    frame: i32,
    class: i32,
}

/// A first-fit allocator over a list of free blocks, in list order.
#[derive(Clone, Debug, Default)]
struct Blocks(Vec<(u32, i32)>);

impl Blocks {
    fn alloc(&mut self, n: i32) -> Option<u32> {
        let k = self.0.iter().position(|b| b.1 >= n)?;
        let addr = self.0[k].0;
        self.0[k].1 -= n;
        self.0[k].0 = addr.wrapping_add(n as u32);
        if self.0[k].1 == 0 {
            self.0.remove(k);
        }
        Some(addr)
    }

    /// in before the first block above it, then the merge pass
    fn free(&mut self, addr: u32, n: i32) {
        let k = self.0.iter().position(|b| addr < b.0).unwrap_or(self.0.len());
        self.0.insert(k, (addr, n));
        let mut i = 0;
        while i < self.0.len() {
            if i + 1 < self.0.len() && self.0[i].0.wrapping_add(self.0[i].1 as u32) == self.0[i + 1].0 {
                self.0[i + 1].0 = self.0[i].0;
                self.0[i + 1].1 += self.0[i].1;
                self.0.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

/// The SPU RAM the device allocates from (0x5040 up): all of it but the
/// SPU2's own areas below and its last 4 KB.
pub const SPU_POOL: i32 = 0x1f9fc0;

fn round64(size: i32) -> i32 {
    ((size + 63) / 64) * 64
}

/// What an upload reads from.
enum Source {
    /// a buffer the sound gave up, freed when the upload is done
    Owned(Vec<u8>),
    /// an XM's module, which the sound keeps
    Module(usize),
}

/// What an upload marks done.
#[derive(Clone, Copy)]
enum Flag {
    WaveCached(usize),
    XmSample(usize),
    XmModule(usize),
}

/// `check_ram`'s answer: how many uploaded sounds were compared, and the
/// slots of those whose SPU RAM is not what their upload sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RamCheck {
    pub checked: usize,
    pub differ: Vec<usize>,
}

pub struct SoundDevice<M: Model> {
    channels: u64,
    playbacks: [Playback; 46],
    pub sounds: Vec<Option<Sound>>,
    pub frame: i32,
    slots: [Option<usize>; 46],
    active: Vec<usize>,
    free: Vec<usize>,
    handle_seed: i32,
    spu_mem: Blocks,
    xm_mem: Blocks,
    iop_buf: u32,
    xm_base: u32,
    xm_size: i32,
    xfer_src: Option<Source>,
    xfer_off: usize,
    xfer_remain: i32,
    xfer_spu_addr: i32,
    xfer_xm_addr: i32,
    xfer_xm_spu: i32,
    xfer_sample_spu: i32,
    xfer_last: i32,
    xfer_dma: u32,
    xfer_idle: i32,
    xfer_flag: Option<Flag>,
    class_volume: [f32; 2],
    stereo: i32,
    bgm_request: [String; 2],
    bgm_playing: String,
    bgm_pending: String,
    bgm_fade_dir: i32,
    bgm_fade: f32,
    bgm_volume: f32,
    bgm_addr: u32,
    bgm_version: i32,
    /// the statics: the IOP's last answer, one command's arguments, the packet
    pub status: [u32; 128],
    args: [i32; 16],
    packet: [u32; 128],
    words: i32,
    /// sounds that could not be made since the device was made: no free
    /// slot of the 128, no SPU RAM (or XM pool) for it (the report's sound
    /// line)
    pub failed_creates: [u32; 2],
    /// packets kept back because the IOP was still busy with the last call
    /// (`sound_flush`), and blocks of an upload held back because a copy or
    /// relocation for the last one was among them (`idle_transfer`): the
    /// report's sound line
    pub flushes_refused: u32,
    pub uploads_held: u32,
    /// commands `sound_cmd` dropped because the packet was full, as the
    /// console's SoundCmd drops them (a busy frame's sounds), and the stops
    /// that found it full (`stop_channels`: sent at the head of the next
    /// packet instead, so not among the dropped)
    pub cmds_dropped: u32,
    pub stops_dropped: u32,
    /// stops that found the packet full, for the head of the next one
    stops_waiting: u64,
    _m: PhantomData<M>,
}

impl<M: Model> SoundDevice<M> {
    /// The constructor, and `SetBgmVersion` (a static on the console, set
    /// before the device is made).
    pub fn new(bgm_version: i32) -> Self {
        let mut d = SoundDevice {
            channels: 0,
            playbacks: [Playback::default(); 46],
            sounds: vec![None; 128],
            frame: 0,
            slots: [None; 46],
            active: Vec::new(),
            free: (0..46).collect(),
            handle_seed: 0x16062,
            spu_mem: Blocks::default(),
            xm_mem: Blocks::default(),
            iop_buf: 0,
            xm_base: 0,
            xm_size: 0,
            xfer_src: None,
            xfer_off: 0,
            xfer_remain: 0,
            xfer_spu_addr: 0,
            xfer_xm_addr: 0,
            xfer_xm_spu: 0,
            xfer_sample_spu: 0,
            xfer_last: 0,
            xfer_dma: 0,
            xfer_idle: 1,
            xfer_flag: None,
            class_volume: [0.0; 2],
            stereo: 0,
            bgm_request: Default::default(),
            bgm_playing: String::new(),
            bgm_pending: String::new(),
            bgm_fade_dir: 0,
            bgm_fade: 0.0,
            bgm_volume: 0.0,
            bgm_addr: 0,
            bgm_version,
            status: [0; 128],
            args: [0; 16],
            packet: [0; 128],
            words: 0,
            failed_creates: [0; 2],
            flushes_refused: 0,
            uploads_held: 0,
            cmds_dropped: 0,
            stops_dropped: 0,
            stops_waiting: 0,
            _m: PhantomData,
        };
        d.bgm_version = bgm_version;
        d
    }

    fn reset_state(&mut self) {
        self.active.clear();
        self.free = (0..46).collect();
        self.sounds.fill(None);
        self.slots = [None; 46];
        self.channels = 0;
        self.frame = 0;
        self.xfer_remain = 0;
        self.xfer_dma = 0;
        self.xfer_last = 0;
        self.xfer_idle = 1;
        self.bgm_addr = 0;
        self.bgm_request = Default::default();
        self.bgm_playing.clear();
        self.bgm_pending.clear();
        self.bgm_volume = 1.0;
        self.bgm_fade_dir = 0;
        self.bgm_fade = 0.0;
        self.stereo = 1;
        self.class_volume = [1.0; 2];
    }

    pub fn init(&mut self, link: &mut dyn Link) -> i32 {
        if self.create_device(link) < 0 {
            return -1;
        }
        self.reset_state();
        self.handle_seed = 0x16062;
        self.xfer_src = None;
        self.bgm_addr = self.alloc_spu_mem(self.bgm_buffer_size() * 2);
        if self.bgm_addr != 0 { 0 } else { -1 }
    }

    pub fn reset(&mut self, link: &mut dyn Link) -> i32 {
        self.xfer_src = None;
        let mut k = 0;
        while k < self.active.len() {
            let pb = self.active[k];
            let next = self.active.get(k + 1).copied();
            let s = self.playbacks[pb].sound;
            if matches!(self.sound(s).map(|s| &s.kind), Some(Kind::Xm(_))) {
                self.stop(self.playbacks[pb].handle);
            }
            k = match next {
                Some(n) => self.active.iter().position(|&a| a == n).unwrap_or(self.active.len()),
                None => self.active.len(),
            };
        }
        self.stop_all_bgm();
        self.stop_channels(u64::MAX);
        self.sound_flush(link);
        for i in 0..128 {
            if self.sounds[i].is_some() {
                self.delete_sound(i);
            }
        }
        if self.bgm_addr != 0 {
            self.free_spu_mem(self.bgm_addr, self.bgm_buffer_size() * 2);
            self.bgm_addr = 0;
        }
        self.xm_mem = Blocks::default();
        self.init_xm_mem(self.xm_base, self.xm_size);
        self.init_spu_mem();
        self.reset_state();
        self.bgm_addr = self.alloc_spu_mem(self.bgm_buffer_size() * 2);
        if self.bgm_addr != 0 { 0 } else { -1 }
    }

    pub fn close(&mut self, link: &mut dyn Link) -> i32 {
        self.reset(link);
        self.release_device(link);
        0
    }

    pub fn bgm_buffer_size(&self) -> i32 {
        let mut n = 16384;
        if self.bgm_version == 1 {
            n = 0x10000;
        }
        n * 2
    }

    fn sound(&self, i: i32) -> Option<&Sound> {
        if i < 0 {
            return None;
        }
        self.sounds.get(i as usize).and_then(|s| s.as_ref())
    }

    /// One frame: an upload step, the playbacks whose voices have gone quiet
    /// stopped, the BGM, and the packet shipped.
    pub fn process(&mut self, link: &mut dyn Link) -> i32 {
        let playing = self.status[4] as u64 | ((self.status[5] as u64) << 24);
        if self.xfer_idle == 0 {
            self.idle_transfer(link);
        } else {
            self.start_one_upload();
        }
        let mut k = 0;
        while k < self.active.len() {
            let pb = self.active[k];
            let next = self.active.get(k + 1).copied();
            let p = self.playbacks[pb];
            match self.sound(p.sound).map(|s| &s.kind) {
                Some(Kind::Wave(_)) => {
                    if p.channels & playing == 0 && self.frame.wrapping_sub(p.frame) >= 3 {
                        self.stop(p.handle);
                    }
                }
                Some(Kind::Xm(_)) => {
                    let found = (0..8).any(|j| self.status[8 + 2 * j] as i32 == (p.channels & 0xffffff) as i32 && self.status[9 + 2 * j] as i32 == (p.channels >> 24) as i32);
                    if !found && self.frame.wrapping_sub(p.frame) >= 3 {
                        self.stop(p.handle);
                    }
                }
                None => {}
            }
            k = match next {
                Some(n) => self.active.iter().position(|&a| a == n).unwrap_or(self.active.len()),
                None => self.active.len(),
            };
        }
        if self.status[6] == 0 {
            self.bgm_playing.clear();
        }
        self.stream_bgm(link);
        self.sound_cmd(3, 0);
        self.sound_flush(link);
        if self.stops_waiting != 0 && self.packet[0] == 0 {
            let mask = std::mem::take(&mut self.stops_waiting);
            self.stop_channels(mask);
        }
        self.frame = self.frame.wrapping_add(1);
        0
    }

    /// The 128-entry sweep for the first sound with something still to send:
    /// one upload is begun and the sweep stops there.
    fn start_one_upload(&mut self) -> bool {
        for i in 0..128 {
            let Some(s) = &mut self.sounds[i] else { continue };
            match &mut s.kind {
                Kind::Wave(w) => {
                    if w.cached == 0 {
                        let src = w.buffer.take().unwrap_or_default();
                        let (spu, size) = (w.spu_addr, w.size);
                        self.begin_adpcm_transfer(src, spu, size, Flag::WaveCached(i));
                        return true;
                    }
                }
                Kind::Xm(x) => {
                    if x.sample_cached == 0 {
                        let src = x.sample.take().unwrap_or_default();
                        let (spu, size) = (x.spu_addr, x.spu_size);
                        self.begin_adpcm_transfer(src, spu, size, Flag::XmSample(i));
                        return true;
                    } else if x.xm_cached == 0 {
                        let (xa, spu, size) = (x.xm_addr, x.spu_addr, x.xm_size);
                        self.begin_xm_transfer(i, xa, spu, size);
                        return true;
                    }
                }
            }
        }
        false
    }

    // ---- sounds ------------------------------------------------------------------

    pub fn create_sound(&mut self, kind: Kind, share: bool) -> i32 {
        let Some(i) = self.sounds.iter().position(|s| s.is_none()) else {
            self.failed_creates[0] += 1;
            return -1;
        };
        self.sounds[i] = Some(Sound { id: 0, link: share as i32, kind });
        i as i32
    }

    pub fn share_sound(&mut self, i: i32) -> i32 {
        match self.sounds.get_mut(i as usize).and_then(|s| s.as_mut()) {
            Some(s) => {
                s.link += 1;
                0
            }
            None => -1,
        }
    }

    /// A last release frees the sound and stops what plays it. (The
    /// dictionary's own entry, when it has an id, is the caller's to remove.)
    pub fn release_sound(&mut self, i: i32) -> i32 {
        let Some(s) = self.sounds.get_mut(i as usize).and_then(|s| s.as_mut()) else { return 0 };
        s.link -= 1;
        if s.link <= 0 {
            let mut k = 0;
            while k < self.active.len() {
                let pb = self.active[k];
                let next = self.active.get(k + 1).copied();
                if self.playbacks[pb].sound == i {
                    self.stop(self.playbacks[pb].handle);
                }
                k = match next {
                    Some(n) => self.active.iter().position(|&a| a == n).unwrap_or(self.active.len()),
                    None => self.active.len(),
                };
            }
            self.delete_sound(i as usize);
        }
        0
    }

    /// The destructors: a wave gives back its SPU memory, an XM its XM pool
    /// block and its SPU memory.
    ///
    /// An upload in flight for the sound still finishes (the next upload
    /// always starts after it), but marks nothing done: by then its slot may
    /// hold a new sound, which would be taken as uploaded and play whatever
    /// its SPU RAM held, for as long as it lived (F3 20261025-190120:
    /// garbled sounds until a restart). A departure from the console, whose
    /// `IdleTransfer` writes `*m_xferFlag = 1` through a pointer into the
    /// VIWave or VIXm deleted meanwhile: freed memory, which the next sound
    /// made may well occupy. A module's transfer reads its sound's slot
    /// block by block, so it goes on with nothing (zeros) instead.
    fn delete_sound(&mut self, i: usize) {
        if matches!(self.xfer_flag, Some(Flag::WaveCached(k) | Flag::XmSample(k) | Flag::XmModule(k)) if k == i) {
            self.xfer_flag = None;
            if matches!(self.xfer_src, Some(Source::Module(k)) if k == i) {
                self.xfer_src = Some(Source::Owned(Vec::new()));
            }
        }
        if let Some(s) = self.sounds[i].take() {
            match s.kind {
                Kind::Wave(w) => {
                    if w.spu_addr != 0 {
                        self.free_spu_mem(w.spu_addr, w.size);
                    }
                }
                Kind::Xm(x) => {
                    if x.xm_addr != 0 {
                        self.free_xm_mem(x.xm_addr, x.xm_size);
                    }
                    if x.spu_addr != 0 {
                        self.free_spu_mem(x.spu_addr, x.spu_size);
                    }
                }
            }
        }
    }

    /// `VIESFParse::ParseAdpcmObj`'s wave and `VIWave::Create`: 28 ADPCM
    /// samples per 16-byte block, one block of leader and one more when the
    /// sample loops. Returns the sound, or -1.
    #[allow(clippy::too_many_arguments)]
    pub fn create_wave(&mut self, channels: i32, blocks: i32, rate: i32, volume: f32, pan: f32, looped: i32, adpcm: &[u8]) -> i32 {
        let i = self.create_sound(Kind::Wave(Wave::default()), true);
        if i < 0 {
            return -1;
        }
        let frames = (blocks - 1 - looped) * 28;
        let c = if channels > 0 { channels } else { 1 };
        let channel_count = if c < 3 { c } else { 2 };
        let nblocks = (frames + 27) / 28 + 1 + (looped != 0) as i32;
        let size = nblocks * channel_count * 16;
        let spu = self.alloc_spu_mem(size);
        let Some(Kind::Wave(w)) = self.sounds[i as usize].as_mut().map(|s| &mut s.kind) else { unreachable!() };
        // the parser sets the file's volume and pan, then VIWave::Create
        // sets 1 and 0 over them: every wave plays at its own full volume,
        // centred, whatever its record says
        let _ = (volume, pan);
        w.volume = 1.0;
        w.pan = 0.0;
        w.sample_rate = rate;
        w.channel_count = channel_count;
        w.format = 1;
        w.sample_size = 4;
        w.sample_frame_count = frames;
        w.looped = looped;
        w.size = size;
        w.spu_addr = spu;
        if spu == 0 {
            // No SPU RAM: the sound goes again, as `create_xm`'s does. A
            // departure: on the console `VIWave::Create` fails with the sound
            // left in the table (and in the parser's dictionary, which
            // `ParseAdpcmObj` fills first) at SPU address 0, never uploaded
            // and never freed, so the upload sweep sends its size in zeros
            // (the console: whatever its null buffer reads) to address 0,
            // over the BGM ring's head.
            self.failed_creates[1] += 1;
            self.release_sound(i);
            return -1;
        }
        let mut buf = vec![0u8; size as usize];
        let n = adpcm.len().min(buf.len());
        buf[..n].copy_from_slice(&adpcm[..n]);
        w.crc = crc32fast::hash(&buf);
        w.buffer = Some(buf);
        w.cached = 0;
        i
    }

    /// `VIXm::Create`: SPU memory for the samples, a block of the XM pool for
    /// the module. Returns the sound, or -1 with the sound released. (On the
    /// console a failure is undefined: `Destroy` frees without forgetting, and
    /// the parser then releases sound -1.)
    pub fn create_xm(&mut self, volume: f32, pan: f32, module: Vec<u8>, samples: Vec<u8>) -> i32 {
        let (xm_size, spu_size) = (module.len() as i32, samples.len() as i32);
        let sample_crc = crc32fast::hash(&samples);
        let x = Xm { volume, pan, xm: Some(module), xm_size, sample: Some(samples), spu_size, sample_crc, ..Default::default() };
        let i = self.create_sound(Kind::Xm(x), true);
        if i < 0 {
            return -1;
        }
        let spu = self.alloc_spu_mem(spu_size);
        let xa = if spu != 0 { self.alloc_xm_mem(xm_size) } else { 0 };
        if let Some(Kind::Xm(x)) = self.sounds[i as usize].as_mut().map(|s| &mut s.kind) {
            x.spu_addr = spu;
            x.xm_addr = xa;
        }
        if spu == 0 || xa == 0 {
            self.failed_creates[1] += 1;
            self.release_sound(i);
            return -1;
        }
        i
    }

    pub fn set_repeat(&mut self, i: i32, r: i32) {
        if let Some(Some(Sound { kind: Kind::Xm(x), .. })) = self.sounds.get_mut(i as usize) {
            x.repeat = r;
        }
    }

    // ---- BGM ---------------------------------------------------------------------

    pub fn play_bgm(&mut self, slot: usize, path: &str) -> i32 {
        self.bgm_request[slot] = path.to_string();
        0
    }

    pub fn stop_bgm(&mut self, slot: usize) {
        if !self.bgm_request[slot].is_empty() && self.bgm_playing == self.bgm_request[slot] {
            self.bgm_fade_dir = -1;
        }
        self.bgm_request[slot].clear();
    }

    pub fn stop_all_bgm(&mut self) {
        self.bgm_request[0].clear();
        self.bgm_request[1].clear();
        if !self.bgm_playing.is_empty() {
            self.args[0] = 0;
            self.sound_cmd(14, 1);
            self.bgm_playing.clear();
        }
    }

    /// The BGM streaming now (`m_bgmPlaying`), empty for none.
    pub fn bgm_playing(&self) -> &str {
        &self.bgm_playing
    }

    pub fn is_bgm_playing(&self, _slot: usize) -> bool {
        !self.bgm_playing.is_empty()
    }

    /// The slot is ignored: every caller writes the one master BGM volume.
    pub fn set_bgm_volume(&mut self, _slot: usize, v: f32) {
        self.bgm_volume = v;
    }

    /// Request slot 0 wins over slot 1, and a change of request cross-fades:
    /// the running track is faded out first and the new one only starts once
    /// it is gone.
    fn stream_bgm(&mut self, link: &mut dyn Link) {
        if !self.bgm_playing.is_empty() && self.bgm_fade_dir != 0 {
            match self.bgm_fade_dir {
                -1 => {
                    self.bgm_fade = M::sub(self.bgm_fade, 0.05);
                    if M::le(self.bgm_fade, 0.0) {
                        self.bgm_fade = 0.0;
                        self.bgm_fade_dir = 0;
                        self.args[0] = 0;
                        self.sound_cmd(14, 1);
                        self.bgm_playing.clear();
                        return;
                    }
                }
                1 => {
                    self.bgm_fade = M::add(self.bgm_fade, 0.05);
                    if M::le(1.0, self.bgm_fade) {
                        self.bgm_fade = 1.0;
                        self.bgm_fade_dir = 0;
                    }
                }
                _ => {}
            }
        }
        self.bgm_pending.clear();
        if !self.bgm_request[0].is_empty() {
            self.bgm_pending = self.bgm_request[0].clone();
        } else if !self.bgm_request[1].is_empty() {
            self.bgm_pending = self.bgm_request[1].clone();
        }
        if self.bgm_pending != self.bgm_playing {
            if !self.bgm_playing.is_empty() {
                self.bgm_fade_dir = -1;
            } else if link.bgm_ready(&self.bgm_pending) {
                self.bgm_playing = self.bgm_pending.clone();
                // strcpy into the argument words: the bytes past the NUL keep
                // what the last command left there
                let mut bytes = [0u8; 64];
                for (k, w) in self.args.iter().enumerate() {
                    bytes[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
                }
                let name = self.bgm_playing.as_bytes();
                let n = name.len().min(63);
                bytes[..n].copy_from_slice(&name[..n]);
                bytes[n] = 0;
                for (k, w) in self.args.iter_mut().enumerate() {
                    *w = i32::from_le_bytes(bytes[k * 4..k * 4 + 4].try_into().unwrap());
                }
                self.args[8] = self.bgm_addr as i32;
                self.sound_cmd(13, 9);
                self.sound_flush(link);
                self.bgm_fade_dir = 0;
                self.bgm_fade = 1.0;
            }
        }
        self.args[0] = M::mul(M::mul(M::mul(self.bgm_volume, self.bgm_fade), self.class_volume[1]), 16383.0) as i32;
        self.sound_cmd(15, 1);
    }

    // ---- playing -------------------------------------------------------------------

    /// Start a sound: a handle, -1 when it cannot, -2 while it is still
    /// uploading.
    pub fn play(&mut self, sound: i32, priority: i32, cls: i32) -> i32 {
        if cls >= 2 || sound == -1 {
            return -1;
        }
        let Some(s) = self.sound(sound) else { return -1 };
        if !s.is_cached() {
            return -2;
        }
        let count = s.channel_count();
        let channels = self.allocate_channels(count, priority);
        if channels == 0 {
            return -1;
        }
        let mut handle = -1;
        let mut pb = 0;
        for i in 0..46 {
            if self.slots[i].is_none() {
                handle = i as i32 | (self.handle_seed.wrapping_shl(8) & 0x7fffff00);
                self.handle_seed = self.handle_seed.wrapping_add(1);
                pb = self.new_playback();
                self.slots[i] = Some(pb);
                break;
            }
        }
        if handle == -1 {
            return -1;
        }
        let s = self.sound(sound).unwrap();
        let (view, pan) = (s.view(), s.pan());
        let cv = self.class_volume[cls as usize];
        let p = &mut self.playbacks[pb];
        p.priority = priority;
        p.sound = sound;
        p.channels = channels;
        p.handle = handle;
        p.frame = self.frame;
        p.class = cls;
        p.pitch = 4096;
        p.volume = 1.0;
        p.pan = pan;
        let p = *p;
        match view {
            View::Wave { spu_addr, size, rate, volume } => {
                let first = first_channel(channels);
                let second = next_channel(channels, first);
                let pan = M::mul(p.pan, 16383.0) as i32;
                self.args[0] = first;
                self.args[1] = spu_addr as i32;
                self.args[2] = second;
                self.args[3] = if second != -1 { spu_addr.wrapping_add((size >> 1) as u32) as i32 } else { 0 };
                self.args[4] = (((rate as i64) << 12) / 48000) as i32;
                self.args[5] = M::mul(M::mul(M::mul(p.volume, volume), cv), 16383.0) as u32 as i32;
                self.args[6] = if self.stereo != 0 { pan } else { 0 };
                self.sound_cmd(2, 7);
            }
            View::Xm { xm_addr, repeat, volume } => {
                let mut pan = M::mul(M::add(M::mul(p.pan, 0.5), 0.5), 255.0) as i32;
                self.args[0] = (channels & 0xffffff) as i32;
                self.args[1] = (channels >> 24) as i32;
                self.args[2] = xm_addr as i32;
                self.args[3] = repeat;
                self.args[4] = p.pitch;
                self.args[5] = M::mul(M::mul(M::mul(p.volume, volume), cv), 255.0) as u32 as i32;
                if self.stereo == 0 {
                    pan = 0;
                }
                self.args[6] = pan;
                self.sound_cmd(8, 7);
            }
        }
        handle
    }

    fn playback(&self, handle: i32) -> Option<usize> {
        let pb = self.slots[((handle & 0xff) % 46) as usize]?;
        (self.playbacks[pb].handle == handle).then_some(pb)
    }

    pub fn is_playing(&self, handle: i32) -> bool {
        self.playback(handle).is_some()
    }

    pub fn stop(&mut self, handle: i32) {
        let Some(pb) = self.playback(handle) else { return };
        let p = self.playbacks[pb];
        if let Some(Kind::Xm(_)) = self.sound(p.sound).map(|s| &s.kind) {
            self.args[0] = (p.channels & 0xffffff) as i32;
            self.args[1] = (p.channels >> 24) as i32;
            self.sound_cmd(9, 2);
        }
        self.stop_channels(p.channels);
        self.channels &= !p.channels;
        self.slots[((p.handle & 0xff) % 46) as usize] = None;
        self.delete_playback(pb);
    }

    pub fn pitch(&self, handle: i32) -> i32 {
        self.playback(handle).map_or(4096, |pb| self.playbacks[pb].pitch)
    }

    pub fn set_pitch(&mut self, handle: i32, pitch: i32) {
        let Some(pb) = self.playback(handle) else { return };
        self.playbacks[pb].pitch = pitch;
        let p = self.playbacks[pb];
        match self.sound(p.sound).map(|s| s.view()) {
            Some(View::Wave { rate, .. }) => {
                let rate = (rate << 12) / 48000;
                let scaled = rate.wrapping_mul(p.pitch) / 4096;
                self.args[0] = first_channel(p.channels);
                self.args[1] = next_channel(p.channels, self.args[0]);
                self.args[2] = scaled;
                self.sound_cmd(5, 3);
            }
            Some(View::Xm { .. }) => {
                self.args[0] = (p.channels & 0xffffff) as i32;
                self.args[1] = (p.channels >> 24) as i32;
                self.args[2] = pitch;
                self.sound_cmd(11, 3);
            }
            None => {}
        }
    }

    pub fn volume(&self, handle: i32) -> f32 {
        self.playback(handle).map_or(0.0, |pb| self.playbacks[pb].volume)
    }

    fn send_volume(&mut self, pb: usize) {
        let p = self.playbacks[pb];
        let cv = self.class_volume[p.class as usize];
        match self.sound(p.sound).map(|s| s.view()) {
            Some(View::Wave { volume, .. }) => {
                let g = M::mul(M::mul(volume, p.volume), cv);
                let mut pan = M::mul(p.pan, 16383.0) as i32;
                self.args[0] = first_channel(p.channels);
                self.args[1] = next_channel(p.channels, self.args[0]);
                self.args[2] = M::mul(g, 16383.0) as u32 as i32;
                if self.stereo == 0 {
                    pan = 0;
                }
                self.args[3] = pan;
                self.sound_cmd(6, 4);
            }
            Some(View::Xm { volume, .. }) => {
                let g = M::mul(M::mul(volume, p.volume), cv);
                let mut pan = M::mul(M::add(M::mul(p.pan, 0.5), 0.5), 255.0) as i32;
                self.args[0] = (p.channels & 0xffffff) as i32;
                self.args[1] = (p.channels >> 24) as i32;
                self.args[2] = M::mul(g, 255.0) as u32 as i32;
                if self.stereo == 0 {
                    pan = 0;
                }
                self.args[3] = pan;
                self.sound_cmd(10, 4);
            }
            None => {}
        }
    }

    pub fn set_volume(&mut self, handle: i32, v: f32) {
        let lo = if M::lt(v, 0.0) { 0.0 } else { v };
        let vol = if M::lt(lo, 1.0) { lo } else { 1.0 };
        let Some(pb) = self.playback(handle) else { return };
        self.playbacks[pb].volume = vol;
        self.send_volume(pb);
    }

    pub fn pan(&self, handle: i32) -> f32 {
        self.playback(handle).map_or(0.0, |pb| self.playbacks[pb].pan)
    }

    pub fn set_pan(&mut self, handle: i32, p: f32) {
        let lo = if M::lt(p, -1.0) { -1.0 } else { p };
        let pan = if M::lt(lo, 1.0) { lo } else { 1.0 };
        let Some(pb) = self.playback(handle) else { return };
        self.playbacks[pb].pan = pan;
        self.send_volume(pb);
    }

    /// Runs the whole upload queue to completion, one DMA at a time.
    pub fn flush(&mut self, link: &mut dyn Link) -> i32 {
        let mut done = false;
        while !done {
            if self.xfer_idle == 0 {
                self.idle_transfer(link);
                self.sound_cmd(3, 0);
                self.sound_flush(link);
            } else {
                done = !self.start_one_upload();
            }
        }
        0
    }

    /// A sound class's volume (`m_classVolume`: 0 the effects', 1 the
    /// music's), as the options set it.
    pub fn sound_class_volume(&self, cls: i32) -> f32 {
        self.class_volume.get(cls as usize).copied().unwrap_or(0.0)
    }

    pub fn set_sound_class_volume(&mut self, cls: i32, v: f32) {
        self.class_volume[cls as usize] = v;
        for k in 0..self.active.len() {
            let pb = self.active[k];
            if self.playbacks[pb].class == cls {
                let (h, vol) = (self.playbacks[pb].handle, self.playbacks[pb].volume);
                self.set_volume(h, vol);
            }
        }
        let v = self.bgm_volume;
        self.set_bgm_volume(0, v);
        self.set_bgm_volume(1, v);
    }

    pub fn set_output_mode(&mut self, stereo: i32) {
        self.stereo = stereo;
        for k in 0..self.active.len() {
            let pb = self.active[k];
            let (h, p) = (self.playbacks[pb].handle, self.playbacks[pb].pan);
            self.set_pan(h, p);
        }
    }

    fn create_device(&mut self, link: &mut dyn Link) -> i32 {
        self.init_spu_mem();
        self.words = 0;
        self.packet[0] = 0;
        self.sound_cmd(0, 0);
        self.sound_flush(link);
        self.iop_buf = self.status[0];
        self.xm_base = self.status[2];
        self.xm_size = self.status[3] as i32;
        self.init_xm_mem(self.xm_base, self.xm_size);
        0
    }

    fn release_device(&mut self, link: &mut dyn Link) {
        self.sound_cmd(12, 0);
        self.sound_flush(link);
        self.xm_mem = Blocks::default();
        self.spu_mem = Blocks::default();
    }

    // ---- channels --------------------------------------------------------------------

    /// Two passes over the playback list: the first only counts what could be
    /// stolen, the second stops it, so a request that cannot be met leaves
    /// every running sound alone.
    fn allocate_channels(&mut self, count: i32, priority: i32) -> u64 {
        let free = 46 - channel_count(self.channels);
        if free < count {
            let mut need = count - free;
            for &pb in &self.active {
                if priority >= self.playbacks[pb].priority {
                    need -= channel_count(self.playbacks[pb].channels);
                    if need <= 0 {
                        break;
                    }
                }
            }
            if need > 0 {
                return 0;
            }
            let mut need = count - free;
            let mut k = 0;
            while k < self.active.len() {
                let pb = self.active[k];
                let next = self.active.get(k + 1).copied();
                if priority >= self.playbacks[pb].priority {
                    need -= channel_count(self.playbacks[pb].channels);
                    self.stop(self.playbacks[pb].handle);
                    if need <= 0 {
                        break;
                    }
                }
                k = match next {
                    Some(n) => self.active.iter().position(|&a| a == n).unwrap_or(self.active.len()),
                    None => self.active.len(),
                };
            }
        }
        let mut got = 0u64;
        let mut n = 0;
        for i in 0..46 {
            let bit = 1u64 << i;
            if self.channels & bit == 0 {
                got |= bit;
                n += 1;
                if n == count {
                    break;
                }
            }
        }
        self.channels |= got;
        got
    }

    /// A stop already queued this frame is merged into rather than re-sent.
    fn stop_channels(&mut self, mask: u64) {
        if let Some(k) = self.find_cmd(4) {
            self.packet[k] |= (mask & 0xffffff) as u32;
            self.packet[k + 1] |= (mask >> 24) as u32;
        } else if self.words + 2 + 3 >= 128 {
            // A departure: the packet is full and the console drops the stop,
            // leaving its voices playing with no sound to stop them (a looped
            // one for good, over whatever its RAM comes to hold). It goes at
            // the head of the next packet instead, ahead of anything started
            // there on those channels (`process`). (F3 20261025-225337)
            self.stops_waiting |= mask;
            self.stops_dropped += 1;
        } else {
            self.args[0] = (mask & 0xffffff) as i32;
            self.args[1] = (mask >> 24) as i32;
            self.sound_cmd(4, 2);
        }
    }

    fn new_playback(&mut self) -> usize {
        let pb = self.free.remove(0);
        self.active.push(pb);
        pb
    }

    fn delete_playback(&mut self, pb: usize) {
        self.active.retain(|&a| a != pb);
        self.free.push(pb);
    }

    // ---- the two allocators --------------------------------------------------------

    fn init_spu_mem(&mut self) {
        self.spu_mem = Blocks(vec![(0x5040, SPU_POOL)]);
    }

    /// The SPU RAM the allocator has free: in all, and its largest block.
    pub fn spu_free(&self) -> (i32, i32) {
        (self.spu_mem.0.iter().map(|b| b.1).sum(), self.spu_mem.0.iter().map(|b| b.1).max().unwrap_or(0))
    }

    /// The sounds made and not yet let go, of the 128.
    pub fn sounds_held(&self) -> usize {
        self.sounds.iter().filter(|s| s.is_some()).count()
    }

    /// Every uploaded sound's SPU RAM against the bytes its upload sent: a
    /// wave's, and an XM's sample bank (its module is relocated in place on
    /// the IOP, so it is not compared). Nothing but an upload writes a
    /// sound's RAM, so a difference is RAM that something else overwrote.
    /// `ram` is the SPU2's; None while a copy is still to be sent (`busy`:
    /// the IOP has not answered the last call, so one may wait inside it),
    /// when what is marked uploaded may not be there yet.
    pub fn check_ram(&self, ram: &[u8], busy: bool) -> Option<RamCheck> {
        if busy || (self.packet[0] != 0 && self.find_cmd(1).is_some()) {
            return None;
        }
        let mut c = RamCheck::default();
        for (i, s) in self.sounds.iter().enumerate() {
            let Some(s) = s else { continue };
            let (addr, size, crc) = match &s.kind {
                Kind::Wave(w) if w.cached != 0 && w.spu_addr != 0 => (w.spu_addr as usize, w.size as usize, w.crc),
                Kind::Xm(x) if x.sample_cached != 0 && x.spu_addr != 0 => (x.spu_addr as usize, x.spu_size as usize, x.sample_crc),
                _ => continue,
            };
            let Some(bytes) = ram.get(addr..addr + size) else { continue };
            c.checked += 1;
            if crc32fast::hash(bytes) != crc {
                c.differ.push(i);
            }
        }
        Some(c)
    }

    pub fn alloc_spu_mem(&mut self, size: i32) -> u32 {
        self.spu_mem.alloc(round64(size)).unwrap_or(0)
    }

    fn free_spu_mem(&mut self, addr: u32, size: i32) {
        self.spu_mem.free(addr, round64(size));
    }

    fn init_xm_mem(&mut self, addr: u32, size: i32) {
        let addr = (addr.wrapping_add(63) / 64).wrapping_mul(64);
        let size = ((size - 65) / 64) * 64;
        self.xm_mem = Blocks(vec![(addr, size)]);
    }

    fn xm_block(&self, size: i32) -> i32 {
        let n = round64(size);
        if self.bgm_version == 1 && n < 0xc000 { 0xc000 } else { n }
    }

    fn alloc_xm_mem(&mut self, size: i32) -> u32 {
        let n = self.xm_block(size);
        self.xm_mem.alloc(n).unwrap_or(0)
    }

    fn free_xm_mem(&mut self, addr: u32, size: i32) {
        let n = self.xm_block(size);
        self.xm_mem.free(addr, n);
    }

    // ---- uploads ---------------------------------------------------------------------

    /// The sound gives up its EE copy: it is freed when the last block has
    /// gone across.
    fn begin_adpcm_transfer(&mut self, src: Vec<u8>, spu_addr: u32, size: i32, flag: Flag) {
        self.xfer_remain = size;
        self.xfer_src = Some(Source::Owned(src));
        self.xfer_off = 0;
        self.xfer_spu_addr = spu_addr as i32;
        self.xfer_xm_addr = -1;
        self.xfer_flag = Some(flag);
        self.xfer_idle = 0;
    }

    fn begin_xm_transfer(&mut self, sound: usize, xm_addr: u32, sample_addr: u32, size: i32) {
        self.xfer_remain = size;
        self.xfer_src = Some(Source::Module(sound));
        self.xfer_off = 0;
        self.xfer_spu_addr = -1;
        self.xfer_xm_addr = xm_addr as i32;
        self.xfer_xm_spu = xm_addr as i32;
        self.xfer_sample_spu = sample_addr as i32;
        self.xfer_flag = Some(Flag::XmModule(sound));
        self.xfer_idle = 0;
    }

    /// One step of the upload: a DMA's completion is answered with command 1
    /// (the IOP copies the block on to SPU RAM) or, for a module, command 7;
    /// otherwise the next block of at most 64 KB goes.
    fn idle_transfer(&mut self, link: &mut dyn Link) {
        if self.xfer_dma != 0 {
            // A departure: the packet fills (a busy frame's sounds started,
            // stopped, their volumes and pans set, all queued before this
            // step; or packets kept back while the IOP was busy) and
            // `sound_cmd` drops what does not fit, as the console's SoundCmd
            // does: the copy (or relocation) dropped, the sound was marked
            // uploaded with whatever its RAM held, and played so until it was
            // let go. The packet is sent first to make room, and while the IOP
            // is too busy to take it the step waits. (F3 20261025-225337)
            if self.words + 2 + 3 >= 128 && self.sound_flush(link) < 0 {
                self.uploads_held += 1;
                return;
            }
            self.xfer_dma = 0;
            if self.xfer_spu_addr != -1 {
                self.args[0] = self.xfer_last;
                self.args[1] = self.xfer_spu_addr;
                self.xfer_spu_addr = self.xfer_spu_addr.wrapping_add(self.xfer_last);
                self.sound_cmd(1, 2);
            }
            if self.xfer_remain != 0 {
                return;
            }
            self.xfer_idle = 1;
            if matches!(self.xfer_src, Some(Source::Owned(_))) {
                self.xfer_src = None;
            }
            if self.xfer_xm_addr != -1 {
                self.args[0] = self.xfer_xm_spu;
                self.args[1] = self.xfer_sample_spu;
                self.sound_cmd(7, 2);
            }
            if let Some(flag) = self.xfer_flag {
                match flag {
                    Flag::WaveCached(i) => {
                        if let Some(Kind::Wave(w)) = self.sounds[i].as_mut().map(|s| &mut s.kind) {
                            w.cached = 1;
                        }
                    }
                    Flag::XmSample(i) => {
                        if let Some(Kind::Xm(x)) = self.sounds[i].as_mut().map(|s| &mut s.kind) {
                            x.sample_cached = 1;
                        }
                    }
                    Flag::XmModule(i) => {
                        if let Some(Kind::Xm(x)) = self.sounds[i].as_mut().map(|s| &mut s.kind) {
                            x.xm_cached = 1;
                        }
                    }
                }
            }
            return;
        }
        if self.xfer_remain > 0 {
            if self.packet[1] == 1 {
                return;
            }
            // A departure: the console waits only for a copy that opens the
            // packet. While the IOP is busy (a BGM stop or start waiting for
            // its vblank) the packet is kept and grows (`sound_flush`), the
            // game's commands of the frame ahead of the copy; the next block
            // then went into the IOP's buffer before the copy of the last one
            // was sent, which copied the new block in its place, and the sound
            // was marked uploaded with another part of itself (or the next
            // sound's start) where that part belonged, wrong for as long as it
            // was kept. A relocation waiting would apply to the next module's
            // bytes as well as its own. (F3 20261025-225337)
            if self.packet[0] != 0 && (self.find_cmd(1).is_some() || self.find_cmd(7).is_some()) {
                self.uploads_held += 1;
                return;
            }
            let n = if self.xfer_remain > 65535 { 0x10000 } else { (self.xfer_remain + 63) & -64 };
            // a whole block of n bytes, past the end of the source as the
            // console reads it (zeros here)
            let mut block = vec![0u8; n as usize];
            let src: &[u8] = match &self.xfer_src {
                Some(Source::Owned(v)) => v,
                Some(Source::Module(i)) => match self.sounds[*i].as_ref().map(|s| &s.kind) {
                    Some(Kind::Xm(x)) => x.xm.as_deref().unwrap_or(&[]),
                    _ => &[],
                },
                None => &[],
            };
            let from = self.xfer_off.min(src.len());
            let k = (src.len() - from).min(n as usize);
            block[..k].copy_from_slice(&src[from..from + k]);
            let dest = if self.xfer_spu_addr == -1 { self.xfer_xm_addr as u32 } else { self.iop_buf };
            link.dma(dest, &block);
            self.xfer_dma = 1;
            self.xfer_remain -= n;
            self.xfer_off += n as usize;
            self.xfer_last = n;
            if self.xfer_spu_addr == -1 {
                self.xfer_xm_addr = self.xfer_xm_addr.wrapping_add(n);
            }
        }
        if self.xfer_remain < 0 {
            self.xfer_remain = 0;
        }
    }

    // ---- the packet ------------------------------------------------------------------

    /// Ship the packet if there is anything in it.
    fn sound_flush(&mut self, link: &mut dyn Link) -> i32 {
        if link.busy() {
            self.flushes_refused += 1;
            return -1;
        }
        if self.packet[0] == 0 {
            return -2;
        }
        self.status = link.call(0x80f0, &self.packet);
        self.packet[0] = 0;
        let n = self.words;
        self.words = 0;
        n
    }

    fn sound_cmd(&mut self, id: u32, argc: i32) {
        let n = self.words;
        if n + argc + 3 < 128 {
            let n = n as usize;
            self.packet[n + 1] = id;
            self.packet[n + 2] = argc as u32;
            for i in 0..argc as usize {
                self.packet[n + 3 + i] = self.args[i] as u32;
            }
            self.words += 2 + argc;
            self.packet[0] += 1;
        } else {
            self.cmds_dropped += 1;
            if id == 4 {
                self.stops_dropped += 1;
            }
        }
    }

    /// Where a queued command's arguments start, by its id.
    fn find_cmd(&self, id: u32) -> Option<usize> {
        let n = self.packet[0];
        let mut p = 1usize;
        for _ in 0..n {
            let cmd = *self.packet.get(p)?;
            let argc = *self.packet.get(p + 1)? as usize;
            p += 2;
            if cmd == id {
                return Some(p);
            }
            p += argc;
        }
        None
    }

    pub fn channels(&self) -> u64 {
        self.channels
    }
}

fn channel_count(mask: u64) -> i32 {
    (0..46).map(|i| ((mask >> i) & 1) as i32).sum()
}

fn first_channel(mask: u64) -> i32 {
    (0..46).find(|&i| (mask >> i) & 1 != 0).unwrap_or(-1)
}

fn next_channel(mask: u64, channel: i32) -> i32 {
    ((channel + 1).max(0)..46).find(|&i| (mask >> i) & 1 != 0).unwrap_or(-1)
}
