//! The options' volumes on the device's sound classes hold for every sound,
//! including one started after they were set (the owner, 2026-11-17:
//! "players turn off sound or turn it down and the sounds or music comes
//! back"): a class at 0 is silent however its sounds are started and
//! however their own volume is set after; 0.3 is quieter than 1. Synthetic
//! looped waves through SNDDRV, no disc.

use snd::Driver;
use snd::device::{Link, SoundDevice};
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

/// A looped mono wave of `blocks` ADPCM blocks, its data bytes `fill`.
fn wave(blocks: usize, fill: u8) -> Vec<u8> {
    let mut v = vec![fill; blocks * 16];
    for b in 0..blocks {
        v[b * 16] = 0x24;
        v[b * 16 + 1] = 0;
    }
    v[1] = 4;
    v[(blocks - 1) * 16 + 1] = 3;
    v
}

/// The loudest sample over `frames` game frames of a wave on class `cls`
/// started after the class's volume was set to `class_volume`, its own volume
/// set to full each frame after (as a creature's loop or the rain sets it).
fn loudest(cls: i32, class_volume: f32, set_after: bool) -> i32 {
    let mut l = Local { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new() };
    let mut d: SoundDevice<Ee> = SoundDevice::new(1);
    assert_eq!(d.init(&mut l), 0);
    let w = d.create_wave(1, 64, 22050, 0.8, 0.0, 1, &wave(64, 0x31));
    d.flush(&mut l);
    assert!(w >= 0);
    d.set_sound_class_volume(cls, class_volume);
    let h = d.play(w, 0, cls);
    assert!(h >= 0, "it starts");
    l.pcm.clear();
    for _ in 0..30 {
        if set_after {
            d.set_volume(h, 1.0);
        }
        d.process(&mut l);
        l.drv.run(1600, &mut l.pcm);
    }
    assert!(d.is_playing(h));
    l.pcm.iter().map(|&s| (s as i32).abs()).max().unwrap_or(0)
}

#[test]
fn a_class_at_zero_is_silent_for_sounds_started_after() {
    for cls in [0, 1] {
        let full = loudest(cls, 1.0, false);
        assert!(full > 0, "the control: class {cls} at 1 is heard");
        assert_eq!(loudest(cls, 0.0, false), 0, "class {cls} at 0: a sound started after it");
        assert_eq!(loudest(cls, 0.0, true), 0, "class {cls} at 0: its own volume set after");
        let low = loudest(cls, 0.3, true);
        assert!(low > 0 && low < full, "class {cls} at 0.3: quieter ({low} of {full})");
    }
}

/// The class's volume set while its sound plays: silent from then, and the
/// other class untouched.
#[test]
fn a_class_turned_down_while_playing() {
    let mut l = Local { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new() };
    let mut d: SoundDevice<Ee> = SoundDevice::new(1);
    assert_eq!(d.init(&mut l), 0);
    let w = d.create_wave(1, 64, 22050, 0.8, 0.0, 1, &wave(64, 0x31));
    d.flush(&mut l);
    let (fx, music) = (d.play(w, 0, 0), d.play(w, 0, 1));
    assert!(fx >= 0 && music >= 0);
    for _ in 0..5 {
        d.process(&mut l);
        l.drv.run(1600, &mut l.pcm);
    }
    d.set_sound_class_volume(0, 0.0);
    d.set_sound_class_volume(1, 0.0);
    for _ in 0..5 {
        d.process(&mut l);
        l.drv.run(1600, &mut l.pcm);
    }
    l.pcm.clear();
    for _ in 0..30 {
        d.set_volume(fx, 1.0);
        d.set_volume(music, 1.0);
        d.process(&mut l);
        l.drv.run(1600, &mut l.pcm);
    }
    assert!(l.pcm.iter().all(|&s| s == 0), "both classes at 0: silent");
    assert_eq!((d.sound_class_volume(0), d.sound_class_volume(1)), (0.0, 0.0));
}
