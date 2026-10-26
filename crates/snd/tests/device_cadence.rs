//! The device's packets at the client's frame rates against the console's
//! (F3 20261026-075325, crunchy at Frame Rate 60): the console runs a sound's
//! callers once a frame of the game's, 30 a second, and sends one packet a
//! frame; the client draws more frames than that and ran the callers that set
//! a playing sound's volume and pan (a creature's loop, an effect's sound
//! sprites, the rain) on every one of them, into the one packet. Between two
//! of the game's frames (`set_between`) those sets are held for the game's
//! frame. Synthetic waves through SNDDRV, no disc.

use snd::device::{Link, SoundDevice};
use snd::Driver;
use vi::fpu::Ee;

/// SNDDRV here, every packet it was sent kept.
struct Local {
    drv: Driver,
    pcm: Vec<i16>,
    sent: Vec<Vec<u32>>,
}

impl Link for Local {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        self.sent.push(packet.to_vec());
        self.drv.call(fno, packet, &mut self.pcm)
    }
    fn dma(&mut self, addr: u32, data: &[u8]) {
        self.drv.io.dma(addr, data);
    }
}

fn device() -> (SoundDevice<Ee>, Local) {
    let mut l = Local { drv: Driver::new(Box::new(snd::NoDisc)), pcm: Vec::new(), sent: Vec::new() };
    let mut d = SoundDevice::new(1);
    assert_eq!(d.init(&mut l), 0);
    (d, l)
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

/// What a frame of a fight asks of the device, as the game's callers ask it
/// each time they run: `loops` sounds playing all along whose volume and pan
/// are set each time (a creature's loop, an effect's sound sprites, the
/// rain's two voices), and, once in some of the game's frames (an event: in
/// one of the frames drawn), a one-shot started (a swing, a hit) with its
/// volume, pan and pitch set at once, as `VICSprite::ProcessSounds` starts
/// one, and a loop stopped and another started. Every value goes by the
/// game's frame `g`, so a frame drawn between two of the game's asks what the
/// game's frame asks.
struct Fight {
    loops: Vec<i32>,
    waves: Vec<i32>,
}

impl Fight {
    fn new(d: &mut SoundDevice<Ee>, l: &mut Local, loops: usize) -> Fight {
        let waves: Vec<i32> = (0..4).map(|k| d.create_wave(1, 64, 22050, 0.8, 0.0, 1, &wave(64, 0x31 + k as u8))).collect();
        d.flush(l);
        assert!(waves.iter().all(|&w| w >= 0), "the waves made");
        let loops = (0..loops).map(|k| d.play(waves[k % 2], 0, 0)).collect::<Vec<_>>();
        assert!(loops.iter().all(|&h| h >= 0), "the loops start");
        // their first packet, at the console's cadence in every run
        d.process(l);
        l.drv.run(1600, &mut l.pcm);
        Fight { loops, waves }
    }

    /// One run of the callers in the game's frame `g`, its events with it
    /// when `events`.
    fn ask(&mut self, d: &mut SoundDevice<Ee>, g: usize, events: bool) {
        for (k, &h) in self.loops.iter().enumerate() {
            let v = 0.2 + 0.05 * ((g + k) % 10) as f32;
            d.set_volume(h, v);
            d.set_pan(h, -0.5 + 0.1 * ((g * 3 + k) % 10) as f32);
        }
        if events && g % 3 == 0 {
            let h = d.play(self.waves[2 + g % 2], 0, 0);
            if h >= 0 {
                d.set_volume(h, 0.1 + 0.02 * (g % 7) as f32);
                d.set_pan(h, 0.3);
                d.set_pitch(h, 3800 + (g % 5) as i32 * 100);
            }
        }
        if events && g % 20 == 10 {
            let k = (g / 20) % self.loops.len();
            d.stop(self.loops[k]);
            self.loops[k] = d.play(self.waves[k % 2], 0, 0);
        }
    }
}

/// `games` of the game's frames of a fight with `loops` loops, `drawn`
/// frames drawn in each (1: the console's 30 a second; 2: Frame Rate 60; 5:
/// about 165), the callers run on every one, the game's frame's events in the
/// frame drawn `event_at` (0 the first; `drawn - 1` the game's frame itself);
/// between two of the game's frames the device is told so when `hold`. The
/// packets sent, the output, and the commands dropped.
fn run(loops: usize, games: usize, drawn: usize, event_at: usize, hold: bool) -> (Vec<Vec<u32>>, Vec<i16>, u32, [u32; 16]) {
    let (mut d, mut l) = device();
    let mut f = Fight::new(&mut d, &mut l, loops);
    l.sent.clear();
    l.pcm.clear();
    for g in 0..games {
        for k in 0..drawn {
            let game_frame = k + 1 == drawn;
            if hold {
                d.set_between(!game_frame);
            }
            f.ask(&mut d, g, k == event_at.min(drawn - 1));
        }
        d.process(&mut l);
        l.drv.run(1600, &mut l.pcm);
    }
    (l.sent, l.pcm, d.cmds_dropped, d.dropped_by_id)
}

/// Six loops (the report had seven voices playing, 000f13) fit the packet at
/// the console's cadence and fill it at Frame Rate 60 and above, where a
/// full packet drops what comes after: here the events of the game's frame,
/// a one-shot's start or its volume, pan and pitch (it then played as it
/// started, at full volume), and a loop's stop and start. With the hold the
/// packets are the console's at every rate, word for word, and so is the
/// sound, sample for sample. The control is the same run without it.
#[test]
fn frame_rate_60_sends_the_consoles_packets() {
    let (console, console_pcm, console_drops, _) = run(6, 90, 1, 0, true);
    assert_eq!(console_drops, 0, "the console's cadence fits the packet");
    for drawn in [2, 5] {
        let (sent, pcm, drops, by_id) = run(6, 90, drawn, drawn - 1, false);
        assert!(drops > 0, "without the hold, {drawn} frames drawn a game frame fill the packet");
        assert!(by_id[2] + by_id[5] + by_id[6] > 0, "and drop the frame's events: {by_id:?}");
        assert_ne!(sent, console, "the control: packets not the console's");
        assert!(pcm != console_pcm, "nor its sound");
        let (sent, pcm, drops, _) = run(6, 90, drawn, drawn - 1, true);
        assert_eq!(drops, 0, "with the hold nothing is dropped at {drawn} frames drawn");
        assert_eq!(sent.len(), console.len());
        assert!(sent == console, "with the hold the packets are the console's at {drawn} frames drawn");
        assert!(pcm == console_pcm, "and the sound is, sample for sample");
    }
}

/// The events of a game's frame in an earlier frame drawn (a trigger tripped
/// between two of the game's frames): with the hold nothing is dropped and the
/// sound is the console's, sample for sample (the commands of different voices
/// in another order in the packet, which SNDDRV runs whole before it mixes).
/// The control: without the hold the same run drops commands.
#[test]
fn events_between_frames_sound_as_the_consoles() {
    let (_, console_pcm, _, _) = run(6, 90, 1, 0, true);
    for drawn in [2, 5] {
        let (_, _, drops, _) = run(6, 90, drawn, 0, false);
        assert!(drops > 0, "the control: without the hold {drawn} frames drawn drop commands");
        let (_, pcm, drops, _) = run(6, 90, drawn, 0, true);
        assert_eq!(drops, 0, "with the hold none at {drawn}");
        assert!(pcm == console_pcm, "and the console's sound at {drawn}");
    }
}

/// A set between two of the game's frames that the game's frame does not
/// repeat (a pitch changed once) is not lost: it goes in the game's frame's
/// packet. The control: told nothing, the device sends it at once, into the
/// same packet.
#[test]
fn a_set_held_between_frames_still_goes() {
    for hold in [false, true] {
        let (mut d, mut l) = device();
        let w = d.create_wave(1, 64, 22050, 0.8, 0.0, 1, &wave(64, 0x42));
        d.flush(&mut l);
        let h = d.play(w, 0, 0);
        d.process(&mut l);
        l.sent.clear();
        if hold {
            d.set_between(true);
        }
        d.set_pitch(h, 5000);
        assert_eq!(d.held_sets, u32::from(hold), "held only when told");
        d.process(&mut l);
        let p = &l.sent[0];
        let pitch = find(p, 5).expect("the pitch is in the frame's packet");
        assert_eq!(p[pitch + 2] as i32, ((22050i32 << 12) / 48000) * 5000 / 4096, "at its value");
    }
}

/// A stop sends what was held for its voices before itself, so a voice's
/// commands keep their order: the packet is the one the device sends told
/// nothing (the control).
#[test]
fn a_stop_sends_what_was_held_first() {
    let mut packets = Vec::new();
    for hold in [false, true] {
        let (mut d, mut l) = device();
        let w = d.create_wave(1, 64, 22050, 0.8, 0.0, 1, &wave(64, 0x42));
        d.flush(&mut l);
        let h = d.play(w, 0, 0);
        d.process(&mut l);
        l.sent.clear();
        if hold {
            d.set_between(true);
        }
        d.set_volume(h, 0.25);
        d.set_pitch(h, 3000);
        d.stop(h);
        d.process(&mut l);
        let p = l.sent[0].clone();
        let (vol, pitch, stop) = (find(&p, 6).unwrap(), find(&p, 5).unwrap(), find(&p, 4).unwrap());
        assert!(vol < stop && pitch < stop, "the sets before the stop ({hold})");
        packets.push(p);
    }
    assert_eq!(packets[0], packets[1], "as the console sends them");
}

/// A sound started between two of the game's frames takes its sets at once,
/// as its start's (on the console they are the same frame's): held, it would
/// start at full volume. The control: one started in an earlier packet is
/// held.
#[test]
fn a_sound_started_between_frames_is_set_at_once() {
    let (mut d, mut l) = device();
    let w = d.create_wave(1, 64, 22050, 0.8, 0.0, 1, &wave(64, 0x42));
    d.flush(&mut l);
    let old = d.play(w, 0, 0);
    d.process(&mut l);
    l.sent.clear();
    d.set_between(true);
    let new = d.play(w, 0, 0);
    d.set_volume(new, 0.3);
    assert_eq!(d.held_sets, 0, "a new sound's set is sent");
    d.set_volume(old, 0.3);
    assert_eq!(d.held_sets, 1, "the control: an old one's is held");
    d.set_between(false);
    d.process(&mut l);
    let p = &l.sent[0];
    let (play, vol) = (find(p, 2).unwrap(), find(p, 6).unwrap());
    assert!(play < vol, "the new one's set after its start");
}

/// Where command `id`'s arguments start in packet `p`.
fn find(p: &[u32], id: u32) -> Option<usize> {
    let mut at = 1;
    for _ in 0..p[0] {
        let (cmd, argc) = (p[at], p[at + 1] as usize);
        if cmd == id {
            return Some(at + 2);
        }
        at += 2 + argc;
    }
    None
}
