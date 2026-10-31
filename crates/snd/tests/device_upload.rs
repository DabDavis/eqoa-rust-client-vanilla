//! The device's uploads where they part from the console (F3 20261025-190120,
//! sounds garbled until a restart): a sound let go while its upload is in
//! flight, and a wave that finds no SPU RAM. Synthetic waves, no disc.

mod common;

use snd::device::{Kind, Link, SoundDevice};
use snd::Driver;
use vi::fpu::Ee;

struct Local {
    drv: Driver,
    pcm: Vec<i16>,
}

impl Link for Local {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        self.drv.call(fno, packet, &mut self.pcm)
    }
    fn dma(&mut self, addr: u32, data: &[u8]) {
        self.drv.io.dma(addr, data);
    }
}

fn device() -> (SoundDevice<Ee>, Local) {
    let mut l = Local { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new() };
    let mut d = SoundDevice::new(1);
    assert_eq!(d.init(&mut l), 0);
    (d, l)
}

/// A game frame: `Process`, then 1,600 samples.
fn frame(d: &mut SoundDevice<Ee>, l: &mut Local) {
    d.process(l);
    l.drv.run(1600, &mut l.pcm);
    l.pcm.clear();
}

/// A mono wave of `blocks` ADPCM blocks, its data bytes `fill`, the last
/// block flagged as the end.
fn wave(blocks: usize, fill: u8) -> Vec<u8> {
    let mut v = vec![fill; blocks * 16];
    for b in 0..blocks {
        v[b * 16] = 0;
        v[b * 16 + 1] = 0;
    }
    v[(blocks - 1) * 16 + 1] = 1;
    v
}

fn make(d: &mut SoundDevice<Ee>, adpcm: &[u8]) -> i32 {
    d.create_wave(1, (adpcm.len() / 16) as i32, 22050, 1.0, 0.0, 0, adpcm)
}

/// Whether sound `i` is uploaded, and whether its SPU RAM holds `want`.
fn held(d: &SoundDevice<Ee>, ram: &[u8], i: i32, want: &[u8]) -> (bool, bool) {
    let Some(Some(s)) = d.sounds.get(i as usize) else { panic!("sound {i} gone") };
    let Kind::Wave(w) = &s.kind else { panic!("sound {i} is not a wave") };
    let at = w.spu_addr as usize;
    (w.cached != 0, ram[at..at + want.len()] == *want)
}

/// Wave A (128 KB, two DMAs) is let go once its upload has begun and wave B
/// made at once: B takes A's slot and A's place. A's upload still finishes,
/// but must not mark B uploaded: B is uploaded after it and holds its own
/// bytes. The control keeps A, so B takes another slot. Before the fix the
/// race left B "uploaded" with A's bytes in its place (it played A at B's
/// rate, and longer, until it was let go).
#[test]
fn a_sound_let_go_mid_upload_does_not_mark_the_next() {
    for let_go in [false, true] {
        let (mut d, mut l) = device();
        let (a, b) = (wave(8192, 0xaa), wave(1024, 0x55));
        let ia = make(&mut d, &a);
        frame(&mut d, &mut l);
        frame(&mut d, &mut l);
        if let_go {
            d.release_sound(ia);
        }
        let ib = make(&mut d, &b);
        assert_eq!(ib == ia, let_go, "B takes A's slot only when A was let go");
        for _ in 0..30 {
            frame(&mut d, &mut l);
        }
        assert_eq!(held(&d, l.drv.io.spu.ram(), ib, &b), (true, true), "let A go {let_go}: B uploaded, its own bytes in place");
        if !let_go {
            assert_eq!(held(&d, l.drv.io.spu.ram(), ia, &a), (true, true), "control: A uploaded too");
        }
    }
}

/// A wave that finds no SPU RAM goes again: no slot is kept for it and
/// nothing is uploaded to address 0. The pool (1.73 MB past the BGM ring)
/// takes six 256 KB waves; the seventh fails. Before the fix it kept its
/// slot, at SPU address 0, and the upload sweep sent 256 KB of zeros there,
/// over the SPU2's own area and the BGM ring.
#[test]
fn a_wave_with_no_ram_keeps_no_slot_and_writes_nothing() {
    let (mut d, mut l) = device();
    let mark = vec![0x5a; 0x6000];
    l.drv.io.spu.write(0, &mark);
    let w = wave(16384, 0x11);
    for k in 0..6 {
        assert!(make(&mut d, &w) >= 0, "wave {k} fits");
    }
    let held_before = d.sounds_held();
    assert_eq!(make(&mut d, &w), -1, "the seventh does not fit");
    assert_eq!(d.sounds_held(), held_before, "and keeps no slot");
    assert_eq!(d.failed_creates, [0, 1]);
    d.flush(&mut l);
    assert!(l.drv.io.spu.ram()[..0x6000] == mark[..], "nothing written at SPU address 0");
    let (free, largest) = d.spu_free();
    assert_eq!((free, largest), (snd::device::SPU_POOL - 0x40000 - 6 * 0x40000, free), "the pool's accounting");
}

/// The output's link (`AudioLink`) as the device meets it while the IOP
/// is busy: the device keeps its packet (`busy`), and a DMA lands at once.
struct Held {
    drv: Driver,
    pcm: Vec<i16>,
    busy: bool,
}

impl Link for Held {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        assert!(!self.busy, "a packet sent over a busy IOP");
        self.drv.call(fno, packet, &mut self.pcm)
    }
    fn dma(&mut self, addr: u32, data: &[u8]) {
        self.drv.io.dma(addr, data);
    }
    fn busy(&mut self) -> bool {
        self.busy
    }
}

/// A game frame over `Held`: the game's own commands first (a sound started
/// and stopped, as a fight's are), then `Process`, then 1,600 samples.
fn held_frame(d: &mut SoundDevice<Ee>, l: &mut Held, other: i32) {
    let h = d.play(other, 0, 0);
    assert!(h >= 0, "the other sound plays");
    d.stop(h);
    d.process(l);
    l.drv.run(1600, &mut l.pcm);
    l.pcm.clear();
}

/// F3 20261025-225337 (crunchy again with the first race fixed): a 128 KB
/// wave goes as two 64 KB blocks through the IOP's one buffer, each DMA'd in
/// then copied out by a command in the next packet. While the IOP was busy
/// (a BGM stop or start waiting for its vblank) two packets were kept, the
/// game's commands ahead of the copy, and the console's guard (a copy that
/// OPENS the packet) let the second block into the buffer before the first
/// one's copy was sent: the copy then put the second block's bytes in the
/// first block's place, and the wave was marked uploaded. Now the upload
/// waits for the copy to be sent. The control: the IOP never busy, the
/// same frames, the same commands.
#[test]
fn a_copy_kept_in_the_packet_is_not_overwritten() {
    for busy in [false, true] {
        let mut l = Held { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new(), busy: false };
        let mut d = SoundDevice::new(1);
        assert_eq!(d.init(&mut l), 0);
        // the other sound, uploaded before
        let other = make(&mut d, &wave(64, 0x33));
        d.flush(&mut l);
        // A: its first block 0xaa, its second 0xbb
        let mut a = wave(8192, 0xaa);
        for b in 4096..8192 {
            for k in 2..16 {
                a[b * 16 + k] = 0xbb;
            }
        }
        let ia = make(&mut d, &a);
        held_frame(&mut d, &mut l, other); // the upload begins
        held_frame(&mut d, &mut l, other); // the first block into the IOP's buffer
        l.busy = busy;
        held_frame(&mut d, &mut l, other); // its copy queued, the packet kept
        held_frame(&mut d, &mut l, other); // the console sent the second block in now
        l.busy = false;
        for _ in 0..8 {
            held_frame(&mut d, &mut l, other);
        }
        assert_eq!(held(&d, l.drv.io.spu.ram(), ia, &a), (true, true), "busy {busy}: A uploaded, each block in its place");
        assert_eq!(d.check_ram(l.drv.io.spu.ram(), false).map(|c| c.differ), Some(vec![]), "busy {busy}: the report's check agrees");
        assert_eq!(d.flushes_refused > 0, busy, "busy {busy}: packets kept back");
        assert_eq!(d.uploads_held > 0, busy, "busy {busy}: the second block held back");
    }
}

/// The report's check of every uploaded sound's RAM (`check_ram`): all as
/// uploaded after a flush; a byte of one wave's RAM overwritten, that wave
/// alone named; an XM's sample bank compared too; and nothing compared while
/// a copy is still to be sent (a busy IOP), when a sound marked uploaded may
/// not be in place yet. The race above, unfixed, shows as a difference here.
#[test]
fn the_ram_check_names_a_sound_overwritten() {
    let (mut d, mut l) = device();
    let (a, b) = (wave(256, 0x21), wave(512, 0x43));
    let (ia, ib) = (make(&mut d, &a), make(&mut d, &b));
    let ix = d.create_xm(1.0, 0.0, vec![0u8; 0x400], vec![0x65u8; 0x3000]);
    assert!(ix >= 0);
    d.flush(&mut l);
    let ram = |l: &Local| l.drv.io.spu.ram().to_vec();
    let c = d.check_ram(&ram(&l), false).unwrap();
    assert_eq!((c.checked, c.differ.clone()), (3, vec![]), "every sound as uploaded");
    // one byte of B overwritten
    let Some(Some(snd::device::Sound { kind: Kind::Wave(w), .. })) = d.sounds.get(ib as usize) else { panic!() };
    let at = w.spu_addr + 100;
    l.drv.io.spu.write(at, &[0xee]);
    let c = d.check_ram(&ram(&l), false).unwrap();
    assert_eq!(c.differ, vec![ib as usize], "B alone");
    assert!(!c.differ.contains(&(ia as usize)), "control: A untouched");
    // the XM's bank
    let Some(Some(snd::device::Sound { kind: Kind::Xm(x), .. })) = d.sounds.get(ix as usize) else { panic!() };
    l.drv.io.spu.write(x.spu_addr + 0x2fff, &[0x00]);
    assert_eq!(d.check_ram(&ram(&l), false).unwrap().differ, vec![ib as usize, ix as usize]);
    assert_eq!(d.check_ram(&ram(&l), true), None, "not while the IOP is busy");
}

/// A packet kept back while the IOP is busy fills (a busy fight's sounds
/// started and stopped), and the console drops a command that does not fit.
/// The copy of an upload's last block was one of them: the wave was marked
/// uploaded and played whatever its RAM held. Now what does not fit goes in
/// the packets after it. The control: the IOP never busy, the same commands.
#[test]
fn a_full_packet_does_not_lose_a_copy() {
    for busy in [false, true] {
        let mut l = Held { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new(), busy: false };
        let mut d = SoundDevice::new(1);
        assert_eq!(d.init(&mut l), 0);
        let other = make(&mut d, &wave(64, 0x33));
        d.flush(&mut l);
        let a = wave(1024, 0x7e);
        let ia = make(&mut d, &a);
        held_frame(&mut d, &mut l, other); // the upload begins
        held_frame(&mut d, &mut l, other); // its one block into the IOP's buffer
        l.busy = busy;
        // a frame's worth of sounds, more than a packet holds: thirteen
        // starts fit (121 words), and a pitch (5) leaves no room for a copy
        for _ in 0..20 {
            let h = d.play(other, 0, 0);
            d.stop(h);
        }
        let h = d.play(other, 0, 0);
        d.set_pitch(h, 4096);
        held_frame(&mut d, &mut l, other); // the copy would be queued now
        d.stop(h);
        l.busy = false;
        for _ in 0..6 {
            held_frame(&mut d, &mut l, other);
        }
        assert_eq!(held(&d, l.drv.io.spu.ram(), ia, &a), (true, true), "busy {busy}: A uploaded, its bytes in place");
        assert_eq!(d.check_ram(l.drv.io.spu.ram(), false).map(|c| c.differ), Some(vec![]));
        assert!(d.most_words >= 121 && d.cmds_later > 0, "the packet filled: {} words, {} sent later", d.most_words, d.cmds_later);
        assert_eq!(d.cmds_dropped, 0, "nothing lost");
    }
}

/// A stop that finds the packet full (no stop in it yet to merge into) was
/// dropped, as the console drops it: the device let its channel go and the
/// voice played on with no sound to stop it, a looped one for good, over
/// whatever its RAM came to hold. It now goes in the packet after it. The
/// control: the same stop with room for it.
#[test]
fn a_stop_that_finds_the_packet_full_still_stops() {
    for full in [false, true] {
        let (mut d, mut l) = device();
        // a looped wave: its first block starts the loop, its last repeats it
        let mut lp = wave(64, 0x5d);
        lp[1] = 4;
        lp[63 * 16 + 1] = 3;
        let looped = d.create_wave(1, 64, 22050, 1.0, 0.0, 1, &lp);
        let other = make(&mut d, &wave(64, 0x33));
        d.flush(&mut l);
        let h = d.play(looped, 0, 0);
        assert!(h >= 0);
        frame(&mut d, &mut l);
        frame(&mut d, &mut l);
        assert_ne!(d.status[4] & 1, 0, "the loop plays on channel 0");
        if full {
            // thirteen starts (121 words) and a pitch (5): no room for a stop
            for _ in 0..13 {
                d.play(other, 0, 0);
            }
            let p = d.play(other, 0, 0);
            d.set_pitch(p, 4096);
        }
        d.stop(h);
        for _ in 0..6 {
            frame(&mut d, &mut l);
        }
        assert_eq!(d.status[4] & 1, 0, "full {full}: the loop stopped");
        assert_eq!(d.later_by_id[4] > 0, full, "full {full}: the stop found no room");
    }
}

/// A volume and a pitch that find the packet full were dropped, as the
/// console drops them: a sound just started then played its whole life at
/// the full volume and the pitch it started with. They now go in the packet
/// after it, in the same flush: the voice's volume and pitch are the ones
/// set. The control: the same sets with room for them.
#[test]
fn a_set_that_finds_the_packet_full_goes_in_the_next() {
    // the left volume and the pitch of voice `ch` on the first core
    let regs = |l: &Local, ch: u16| (l.drv.io.sd_get_param(ch << 1), l.drv.io.sd_get_param((ch << 1) | 0x200));
    for full in [false, true] {
        let (mut d, mut l) = device();
        let w = make(&mut d, &wave(64, 0x33));
        d.flush(&mut l);
        if full {
            // thirteen starts (117 words): the fourteenth fits, its sets do not
            for _ in 0..13 {
                d.play(w, 0, 0);
            }
        }
        let p = d.play(w, 0, 0);
        assert!(p >= 0);
        d.set_volume(p, 0.25);
        d.set_pitch(p, 2048);
        let ch = if full { 13 } else { 0 };
        assert_eq!(d.later_by_id[5] + d.later_by_id[6] > 0, full, "full {full}: the sets found no room");
        frame(&mut d, &mut l);
        frame(&mut d, &mut l);
        let base = ((22050i32 << 12) / 48000) as u16;
        assert_eq!(regs(&l, ch), ((0.25f32 * 16383.0) as u16, base / 2), "full {full}: the voice has the volume and pitch set");
    }
}

/// An ambient track's stop that finds the packet full was dropped, as the
/// console drops it: SNDDRV's player went on with the module, repeating, on
/// the channels the device had let go and gave the next sounds, its notes
/// keyed over theirs for good. It now goes in the packet after it, and the
/// player stops. The control: the same stop with room for it. AMBTRACK.ESF's
/// first module, off the disc.
#[test]
fn an_ambient_tracks_stop_that_finds_the_packet_full_still_stops() {
    let Some(mut disc) = common::disc() else { return };
    let amb = esf::Container::open(disc.read("DATA/AMBTRACK.ESF").unwrap()).unwrap();
    let (_, xms) = esf::sound::all(&amb);
    let x = xms.into_iter().next().expect("a module");
    let playing = |d: &SoundDevice<Ee>| (0..8).any(|j| d.status[8 + 2 * j] != 0 || d.status[9 + 2 * j] != 0);
    for full in [false, true] {
        let (mut d, mut l) = device();
        let ix = d.create_xm(x.volume, x.pan, x.module.clone(), x.samples.clone());
        d.set_repeat(ix, 1);
        let other = make(&mut d, &wave(64, 0x33));
        d.flush(&mut l);
        let h = d.play(ix, 0, 0);
        assert!(h >= 0, "the track starts: {h}");
        for _ in 0..4 {
            frame(&mut d, &mut l);
        }
        assert!(playing(&d), "the player has the module");
        if full {
            // thirteen starts and a fourteenth (126 words): no room for its stop
            for _ in 0..14 {
                d.play(other, 0, 0);
            }
        }
        d.stop(h);
        for _ in 0..6 {
            frame(&mut d, &mut l);
        }
        assert!(!playing(&d), "full {full}: the player stopped the module");
        assert_eq!(d.later_by_id[9] > 0, full, "full {full}: the stop found no room");
    }
}
