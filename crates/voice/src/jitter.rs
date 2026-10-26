//! One slot's audio on its way to the speakers: the Opus packets as they come (any order, some
//! lost), decoded in sequence when the output wants samples, at the volume of the peer on it.
//!
//! The buffer holds packets by their RTP sequence number (unwrapped past 65535; the server
//! rewrites them on a slot's switch, so a slot is one stream). Playing starts once `PREBUFFER`
//! are in (60 ms), and again after it ran dry (every pause in talking); a packet that is not
//! there when its turn comes is rebuilt from the next one's forward error correction when that
//! one is in, else concealed by the decoder (Opus's PLC); one that comes after its turn is
//! dropped. More than `MAX_HELD` waiting (300 ms, a burst) skips ahead to the newest few; more
//! than `HIGH` (120 ms) for a second on end (a sender's clock a little fast) drops one packet,
//! so the delay never builds up.

use std::collections::{BTreeMap, VecDeque};

/// The output's rate: Opus's own, and the game's mixer's.
pub(crate) const RATE: u32 = 48_000;
/// One packet's samples: 20 ms.
pub(crate) const FRAME: usize = 960;
/// Packets held before playing starts: 60 ms.
const PREBUFFER: usize = 3;
/// Packets held at most before skipping ahead: 300 ms.
const MAX_HELD: u64 = 15;
/// Packets held, above which a steady backlog is worked off: 120 ms.
const HIGH: u64 = 6;
/// Decodes in a row above `HIGH` before one packet is dropped: a second.
const DRIFT: u32 = 50;
/// Packets kept waiting at most (late or far ahead ones beyond are dropped).
const MAX_KEPT: usize = 64;
/// The longest an Opus packet decodes to at 48 kHz (120 ms).
const MAX_DECODED: usize = 5760;

pub(crate) struct Playback {
    decoder: opus::Decoder,
    packets: BTreeMap<u64, Vec<u8>>,
    /// The newest sequence number seen, unwrapped.
    newest: Option<u64>,
    /// The next one to play, once anything played (an older one coming is too late).
    next: Option<u64>,
    /// Playing; else filling (at the start, or after running dry).
    playing: bool,
    /// Decodes in a row with more than `HIGH` waiting.
    high: u32,
    pcm: VecDeque<f32>,
    scratch: Vec<f32>,
    /// 0 silent, 1 as sent, up to 2.
    pub volume: f32,
    /// What it played lately, smoothed (before the volume: whether the member is talking).
    level: f32,
}

impl Playback {
    pub fn new() -> Playback {
        Playback {
            decoder: opus::Decoder::new(RATE, opus::Channels::Mono).expect("an Opus decoder"),
            packets: BTreeMap::new(),
            newest: None,
            next: None,
            playing: false,
            high: 0,
            pcm: VecDeque::new(),
            scratch: vec![0.0; MAX_DECODED],
            volume: 1.0,
            level: 0.0,
        }
    }

    /// The level the speaking marks show, 0..1 (the RMS of what played lately).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Everything dropped, as new (the engine stopped).
    pub fn reset(&mut self) {
        let volume = self.volume;
        *self = Playback::new();
        self.volume = volume;
    }

    /// `seq` unwrapped past 65535, as the nearest to the newest seen.
    fn unwrap(&self, seq: u16) -> u64 {
        let Some(newest) = self.newest else { return 1 << 32 | seq as u64 };
        let base = newest & !0xFFFF;
        let mut best = base | seq as u64;
        for cand in [best.wrapping_add(1 << 16), best.wrapping_sub(1 << 16)] {
            if cand.abs_diff(newest) < best.abs_diff(newest) {
                best = cand;
            }
        }
        best
    }

    /// A packet off the network.
    pub fn push(&mut self, seq: u16, payload: &[u8]) {
        let ext = self.unwrap(seq);
        if self.next.is_some_and(|n| ext < n) || payload.is_empty() {
            return; // after its turn
        }
        if self.newest.is_none_or(|n| ext > n) {
            self.newest = Some(ext);
        }
        self.packets.insert(ext, payload.to_vec());
        while self.packets.len() > MAX_KEPT {
            self.packets.pop_first();
        }
    }

    fn decode(&mut self, packet: &[u8], fec: bool) {
        let out = if fec || packet.is_empty() { &mut self.scratch[..FRAME] } else { &mut self.scratch[..] };
        match self.decoder.decode_float(packet, out, fec) {
            Ok(n) => self.pcm.extend(&self.scratch[..n]),
            Err(_) => self.pcm.extend(std::iter::repeat_n(0.0, FRAME)),
        }
    }

    /// Decoded samples enough for `frames`, as far as the packets go.
    fn fill(&mut self, frames: usize) {
        while self.pcm.len() < frames {
            if !self.playing {
                if self.packets.len() < PREBUFFER {
                    return;
                }
                self.next = self.packets.keys().next().copied();
                self.playing = true;
                self.high = 0;
                continue;
            }
            let n = self.next.unwrap_or(0);
            if let Some(&last) = self.packets.keys().next_back() {
                let held = last + 1 - n; // every packet kept is at or after `n`
                if held > MAX_HELD {
                    let to = last + 1 - PREBUFFER as u64;
                    self.packets = self.packets.split_off(&to);
                    self.next = Some(to);
                    self.high = 0;
                    continue;
                }
                if held > HIGH {
                    self.high += 1;
                    if self.high >= DRIFT {
                        self.packets.remove(&n);
                        self.next = Some(n + 1);
                        self.high = 0;
                        continue;
                    }
                } else {
                    self.high = 0;
                }
            }
            if let Some(packet) = self.packets.remove(&n) {
                self.decode(&packet, false);
            } else if self.packets.is_empty() {
                self.playing = false; // ran dry: fill again
                return;
            } else if let Some(after) = self.packets.get(&(n + 1)).cloned() {
                self.decode(&after, true);
            } else {
                self.decode(&[], false);
            }
            self.next = Some(n + 1);
        }
    }

    /// `out` (48 kHz, `channels` interleaved) with this member's audio added at its volume.
    pub fn mix_into(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        self.fill(frames);
        let take = frames.min(self.pcm.len());
        let mut sum = 0.0f32;
        for k in 0..take {
            let s = self.pcm.pop_front().unwrap_or(0.0);
            sum += s * s;
            let v = s * self.volume;
            for c in 0..channels {
                out[k * channels + c] += v;
            }
        }
        let rms = if frames > 0 { (sum / frames as f32).sqrt() } else { 0.0 };
        self.level = if rms > self.level { rms } else { self.level * 0.85 + rms * 0.15 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 20 ms packets of a tone at `freq`, Opus-coded as the engine codes them.
    fn tone_packets(freq: f32, n: usize) -> Vec<Vec<u8>> {
        let mut enc = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip).unwrap();
        enc.set_bitrate(opus::Bitrate::Bits(32_000)).unwrap();
        enc.set_inband_fec(true).unwrap();
        enc.set_packet_loss_perc(10).unwrap();
        let mut t = 0usize;
        (0..n)
            .map(|_| {
                let frame: Vec<f32> = (0..FRAME)
                    .map(|_| {
                        t += 1;
                        0.3 * (2.0 * std::f32::consts::PI * freq * t as f32 / RATE as f32).sin()
                    })
                    .collect();
                enc.encode_vec_float(&frame, 1500).unwrap()
            })
            .collect()
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    /// Pulls `frames` the way an output callback would, in blocks of 480.
    fn pull(p: &mut Playback, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames];
        for chunk in out.chunks_mut(480) {
            p.mix_into(chunk, 1);
        }
        out
    }

    /// Nothing plays until 60 ms are in; then the packets play in sequence though they came
    /// out of order, all of them (the control: in order, the same length).
    #[test]
    fn it_waits_for_three_and_plays_in_sequence() {
        let packets = tone_packets(440.0, 10);
        let mut p = Playback::new();
        p.push(0, &packets[0]);
        p.push(1, &packets[1]);
        assert_eq!(rms(&pull(&mut p, 960)), 0.0, "two packets are not enough to start");
        for i in [3usize, 2, 5, 4, 7, 6, 9, 8] {
            p.push(i as u16, &packets[i]);
        }
        let out = pull(&mut p, 10 * FRAME);
        assert!(rms(&out[2 * FRAME..8 * FRAME]) > 0.1, "the tone plays");
        assert_eq!(p.packets.len(), 0, "every packet was played");
        let mut q = Playback::new();
        for (i, pk) in packets.iter().enumerate() {
            q.push(i as u16, pk);
        }
        let ordered = pull(&mut q, 10 * FRAME);
        assert_eq!(ordered.len(), out.len());
    }

    /// A lost packet is rebuilt from the next one's FEC (or concealed): the tone carries on
    /// through it instead of a 20 ms hole; the control, a stream that ran dry, is silent.
    #[test]
    fn a_lost_packet_is_concealed() {
        let packets = tone_packets(440.0, 12);
        let mut p = Playback::new();
        for i in (0..12).filter(|&i| i != 6) {
            p.push(i as u16, &packets[i]);
        }
        let out = pull(&mut p, 12 * FRAME);
        let hole = &out[6 * FRAME..7 * FRAME];
        assert!(rms(hole) > 0.05, "the missing 20 ms is filled: rms {}", rms(hole));
        let after = pull(&mut p, 4 * FRAME);
        assert_eq!(rms(&after), 0.0, "nothing more came: silence");
    }

    /// The volume scales what is added (0 silent, 2 twice), and a late packet (after its turn)
    /// is dropped, not played out of place.
    #[test]
    fn the_volume_scales_and_late_packets_are_dropped() {
        let packets = tone_packets(440.0, 12);
        let run = |volume: f32| {
            let mut p = Playback::new();
            p.volume = volume;
            for (i, pk) in packets.iter().enumerate() {
                p.push(i as u16, pk);
            }
            rms(&pull(&mut p, 12 * FRAME)[3 * FRAME..11 * FRAME])
        };
        let (zero, one, two) = (run(0.0), run(1.0), run(2.0));
        assert_eq!(zero, 0.0);
        assert!((two / one - 2.0).abs() < 0.01, "2.0 doubles: {one} -> {two}");
        let mut p = Playback::new();
        for i in 3..8u16 {
            p.push(i, &packets[i as usize]);
        }
        pull(&mut p, 4 * FRAME);
        let held = p.packets.len();
        p.push(2, &packets[2]);
        assert_eq!(p.packets.len(), held, "a packet before the one playing is dropped");
    }

    /// A backlog past 300 ms skips ahead to the newest, so the delay stays short; the
    /// sequence numbers unwrap past 65535.
    #[test]
    fn a_backlog_skips_ahead_and_sequences_wrap() {
        let packets = tone_packets(440.0, 40);
        let mut p = Playback::new();
        for (i, pk) in packets.iter().enumerate() {
            p.push((65530u32 + i as u32) as u16, pk);
        }
        assert_eq!(p.packets.len(), 40, "40 packets across the wrap");
        pull(&mut p, FRAME);
        assert!(p.packets.len() < MAX_HELD as usize, "skipped ahead: {} left", p.packets.len());
        let keys: Vec<u64> = p.packets.keys().copied().collect();
        assert!(keys.windows(2).all(|w| w[1] == w[0] + 1), "in order across the wrap: {keys:?}");
    }

    /// A sender a little ahead (eight packets waiting, one more coming each one played) is
    /// worked down to `HIGH` a packet a second; the control, five waiting, loses none.
    #[test]
    fn a_steady_backlog_is_worked_off_slowly() {
        let packets = tone_packets(440.0, 400);
        let run = |ahead: usize| {
            let mut p = Playback::new();
            for (i, packet) in packets.iter().enumerate().take(ahead) {
                p.push(i as u16, packet);
            }
            let mut played = 0;
            for (i, packet) in packets.iter().enumerate().skip(ahead) {
                pull(&mut p, FRAME);
                played += 1;
                p.push(i as u16, packet);
            }
            (p.packets.len(), packets.len() - p.packets.len() - played)
        };
        let (held, dropped) = run(8);
        assert!(held as u64 <= HIGH, "worked down: {held} waiting");
        assert!((1..=3).contains(&dropped), "a packet a second at most: {dropped} dropped");
        let (held, dropped) = run(5);
        assert_eq!((held, dropped), (5, 0), "five waiting is not a backlog");
    }
}
