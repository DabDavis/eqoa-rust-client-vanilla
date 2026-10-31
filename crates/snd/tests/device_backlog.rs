//! A frame's commands past one packet (F3 pass 25: "sound cutting out when
//! many mobs on screen"): the console's SoundCmd drops what does not fit its
//! 128-word packet, and a crowd's frame (a pack's swings and hits started at
//! once, each with its volume, pan and pitch) asked for several packets'
//! worth. The device keeps what does not fit in its backlog and sends it in
//! the packets after the first, in order. Synthetic waves through SNDDRV, no
//! disc; the console's rule (`console_drops`) is the control.

use snd::device::{Link, SoundDevice};
use snd::Driver;
use vi::fpu::Ee;

/// SNDDRV here, every packet it was sent kept; busy after each call while
/// `busy_after` is set (a command waiting for its vblank), until cleared.
struct Local {
    drv: Driver,
    pcm: Vec<i16>,
    sent: Vec<Vec<u32>>,
    busy_after: bool,
    busy: bool,
}

impl Link for Local {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        assert!(!self.busy, "a packet sent over a busy IOP");
        self.sent.push(packet.to_vec());
        self.busy = self.busy_after;
        self.drv.call(fno, packet, &mut self.pcm)
    }
    fn dma(&mut self, addr: u32, data: &[u8]) {
        self.drv.io.dma(addr, data);
    }
    fn busy(&mut self) -> bool {
        self.busy
    }
}

/// A looped mono wave of `blocks` ADPCM blocks: it plays until stopped.
fn looped(blocks: usize, fill: u8) -> Vec<u8> {
    let mut v = vec![fill; blocks * 16];
    for b in 0..blocks {
        v[b * 16] = 0x24;
        v[b * 16 + 1] = 0;
    }
    v[1] = 4;
    v[(blocks - 1) * 16 + 1] = 3;
    v
}

fn device(console_drops: bool) -> (SoundDevice<Ee>, Local, i32) {
    let mut l = Local { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new(), sent: Vec::new(), busy_after: false, busy: false };
    let mut d = SoundDevice::new(1);
    d.console_drops = console_drops;
    assert_eq!(d.init(&mut l), 0);
    let w = d.create_wave(1, 64, 22050, 1.0, 0.0, 1, &looped(64, 0x35));
    d.flush(&mut l);
    assert!(w >= 0);
    (d, l, w)
}

/// The voices SNDDRV plays (the status block's two cores), the BGM's pair aside.
fn playing(d: &SoundDevice<Ee>) -> u32 {
    (d.status[4] & 0xff_ffff).count_ones() + (d.status[5] & 0x3f_ffff).count_ones()
}

/// `n` sounds started in one frame, each set as `VICSprite::ProcessSounds`
/// sets a swing's (volume, pan, pitch): 26 words each.
fn crowd(d: &mut SoundDevice<Ee>, w: i32, n: usize) {
    for k in 0..n {
        let h = d.play(w, 0, 0);
        assert!(h >= 0, "sound {k} starts");
        d.set_volume(h, 0.2 + 0.01 * k as f32);
        d.set_pan(h, -0.5 + 0.03 * k as f32);
        d.set_pitch(h, 3800 + k as i32 * 10);
    }
}

/// 30 sounds in one frame (780 words): every one reaches SNDDRV and plays,
/// in seven packets, the first the console's to the word. The control is the
/// console's rule: one packet, the rest dropped, five sounds playing; and
/// four sounds (109 words with the frame's own) fit, the packets the
/// console's either way.
#[test]
fn a_crowds_frame_reaches_snddrv_whole() {
    let run = |console_drops: bool, n: usize| {
        let (mut d, mut l, w) = device(console_drops);
        l.sent.clear();
        crowd(&mut d, w, n);
        d.process(&mut l);
        l.drv.run(1600, &mut l.pcm);
        // a second frame, for the status block of the voices the first started
        d.process(&mut l);
        (l.sent, playing(&d), d.cmds_later, d.cmds_dropped)
    };
    let (sent, voices, later, lost) = run(false, 30);
    assert_eq!((voices, lost), (30, 0), "every sound plays, none lost");
    assert!(later > 0, "the packet filled");
    let (console, console_voices, _, console_lost) = run(true, 30);
    assert!(console_lost > 0 && console_voices < 30, "the control: the console's rule drops ({console_lost} lost, {console_voices} playing)");
    assert_eq!(sent[0], console[0], "the first packet is the console's");
    assert!(sent.len() >= 7, "the rest in the packets after it: {} sent", sent.len());
    let commands = |p: &[Vec<u32>]| p.iter().map(|p| p[0]).sum::<u32>();
    // every command of the frame: 30 x (play, volume, pan, pitch), the BGM's
    // volume and the frame's own; the last packet is the second frame's
    assert_eq!(commands(&sent[..sent.len() - 1]), 30 * 4 + 2, "every command sent, in the frame's packets");
    for p in &sent {
        assert!(p[0] > 0, "no empty packet");
    }
    // a frame that fits: the same packets either way
    let (small, small_voices, small_later, _) = run(false, 4);
    let (small_console, _, _, _) = run(true, 4);
    assert_eq!((small_voices, small_later), (4, 0));
    assert_eq!(small, small_console, "a frame that fits is the console's, packet for packet");
}

/// An IOP busy once a packet has gone (a command waiting for its vblank):
/// what is left goes at the next flush, ahead of the next frame's commands,
/// and every sound plays. The control: the order of the commands SNDDRV ran
/// is the order they were queued in.
#[test]
fn a_backlog_waits_for_a_busy_iop_in_order() {
    let (mut d, mut l, w) = device(false);
    l.sent.clear();
    crowd(&mut d, w, 20);
    l.busy_after = true;
    d.process(&mut l);
    assert_eq!(l.sent.len(), 1, "one packet, then the IOP is busy");
    l.drv.run(1600, &mut l.pcm);
    // the next frame: one more sound, then the IOP free again
    let h = d.play(w, 0, 0);
    assert!(h >= 0);
    d.set_volume(h, 0.9);
    l.busy_after = false;
    l.busy = false;
    d.process(&mut l);
    l.drv.run(1600, &mut l.pcm);
    d.process(&mut l);
    assert_eq!(playing(&d), 21, "every sound plays");
    // the plays' first channels in the order SNDDRV was sent them: 0 to 20
    let mut channels = Vec::new();
    for p in &l.sent {
        let mut at = 1;
        for _ in 0..p[0] {
            let (id, argc) = (p[at], p[at + 1] as usize);
            if id == 2 {
                channels.push(p[at + 2]);
            }
            at += 2 + argc;
        }
    }
    assert_eq!(channels, (0..21).collect::<Vec<u32>>(), "the plays in their order, the frame after's last");
}
