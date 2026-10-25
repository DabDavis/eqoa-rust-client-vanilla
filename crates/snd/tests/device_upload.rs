//! The device's uploads where they part from the console (F3 20261025-190120,
//! sounds garbled until a restart): a sound let go while its upload is in
//! flight, and a wave that finds no SPU RAM. Synthetic waves, no disc.

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
fn held(d: &SoundDevice<Ee>, l: &Local, i: i32, want: &[u8]) -> (bool, bool) {
    let Some(Some(s)) = d.sounds.get(i as usize) else { panic!("sound {i} gone") };
    let Kind::Wave(w) = &s.kind else { panic!("sound {i} is not a wave") };
    let at = w.spu_addr as usize;
    (w.cached != 0, l.drv.io.spu.ram()[at..at + want.len()] == *want)
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
        assert_eq!(held(&d, &l, ib, &b), (true, true), "let A go {let_go}: B uploaded, its own bytes in place");
        if !let_go {
            assert_eq!(held(&d, &l, ia, &a), (true, true), "control: A uploaded too");
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
