//! The world's sound, as `VIScene` and `VIClient` make it: the ambient tracker
//! music of the terrain under the camera (`VIScene::SetSoundTrack`), the
//! streamed music of the region the camera stands in (the first half of
//! `VIScene::UpdateSoundEmitters`), and `VISoundDevice::Process` once a frame,
//! all over `snd` on the audio thread (`audio.rs`).
//!
//! The scene's dictionary is kept for its sounds only (`dict`): a DictID to
//! the device's sound. The loader is synchronous: a track `Load`ed is made
//! from AMBTRACK.ESF at once and is complete at the next call; its upload to
//! the IOP then takes the device's frames, a block a frame, as on the console.
//! Not ported: the outdoors probe (`CalcOutdoorsIntensity`, so `m_p30` stays
//! 1), the weather's intensity (0), and the sound sprites' half of
//! `UpdateSoundEmitters` (no world places one; SPELLFX does).

use crate::audio::{Audio, AudioLink};
use snd::device::SoundDevice;
use std::collections::HashMap;
use vi::camera::Eye;
use vi::fpu::{Ee, F};
use vi::world::{SpriteKind, World};

/// `VISceneSoundTrack::SetTracks`: a (day, night) pair of XM DictIDs for each
/// terrain profile's music index; index 0 is silence.
const TRACKS: [[u32; 2]; 11] = [
    [0, 0],
    [0x9A5DBACC, 0xA5C60038],
    [0xEBF2EEC3, 0x909E78E7],
    [0x92C9A8CE, 0xA0DBE44A],
    [0xB9422F46, 0x2FEB20F5],
    [0x864A0682, 0x656F64E9],
    [0xDB5AA9FB, 0x2143B5DF],
    [0x42E6CF3A, 0x72F08216],
    [0x1ADCC066, 0x68B480A2],
    [0x9E562B5A, 0xCEF71F36],
    [0x0CE4AB35, 0xFD6FEEE9],
];

/// `VISceneSoundTrackState`
#[derive(Clone, Copy, Debug)]
struct TrackState {
    /// 0 idle, 2 fading in, 1 fading out, 3 faded out
    p00: i32,
    /// when the fade started
    p08: i64,
    /// the volume the fade started at, and the volume now
    p10: F<Ee>,
    p14: F<Ee>,
    /// the loader's handle, -1
    p18: i32,
    /// the playback, -1
    p1c: i32,
    /// the track's DictID
    p20: u32,
    /// the device's sound, -1
    p24: i32,
}

impl TrackState {
    fn cleared() -> Self {
        TrackState { p00: 0, p08: 0, p10: F::ZERO, p14: F::ZERO, p18: -1, p1c: -1, p20: 0, p24: -1 }
    }
}

/// `VISceneSoundTrack`
struct SoundTrack {
    /// 1 until the first track has started: it starts at full volume
    p00: i32,
    /// the volume a fade in reaches
    p04: F<Ee>,
    /// the fade time, ms
    p10: i64,
    /// when the track last changed
    p18: i64,
    /// the underwater ambience (the sound), and its playback
    p20: i32,
    p24: i32,
    /// the outdoors intensity
    p30: F<Ee>,
    state: [TrackState; 2],
}

impl SoundTrack {
    fn cleared() -> Self {
        SoundTrack { p00: 1, p04: F::ONE, p10: 15000, p18: 0, p20: -1, p24: -1, p30: F::ONE, state: [TrackState::cleared(); 2] }
    }
}

pub struct Sound {
    pub dev: SoundDevice<Ee>,
    link: AudioLink,
    audio: Audio,
    ambtrack: Option<esf::Container>,
    /// DictID -> the device's sound
    dict: HashMap<u32, i32>,
    /// the loader's jobs: handle -> the DictID it loaded
    loads: HashMap<i32, u32>,
    next_load: i32,
    track: SoundTrack,
    /// `m_p63C`, the streamed-music region playing
    stream_best: i32,
}

impl Sound {
    /// The device on a fresh audio output, with AMBTRACK.ESF for the tracks.
    pub fn new(iso_path: &std::path::Path) -> Option<Sound> {
        let disc = iso::Iso::open(iso_path).ok()?;
        let audio = Audio::open(Box::new(crate::audio::IsoDisc(disc)));
        eprintln!("{}", audio.describe);
        Self::with(iso_path, audio)
    }

    /// The same with no output: `audio().render` runs the driver.
    pub fn offline(iso_path: &std::path::Path) -> Option<Sound> {
        let disc = iso::Iso::open(iso_path).ok()?;
        Self::with(iso_path, Audio::offline(Box::new(crate::audio::IsoDisc(disc))))
    }

    pub fn audio(&self) -> &Audio {
        &self.audio
    }

    /// What plays, in a line: the BGM request and track, the XM slots.
    pub fn describe(&self) -> String {
        let st = self.audio.status();
        let t = &self.track;
        format!(
            "region music {} (bgm {}), ambient track {:08x}/{:08x} (states {}/{}), voices {:06x} {:06x}",
            self.stream_best,
            if st[6] != 0 { "streaming" } else { "off" },
            t.state[0].p20,
            t.state[1].p20,
            t.state[0].p00,
            t.state[1].p00,
            st[4],
            st[5]
        )
    }

    fn with(iso_path: &std::path::Path, audio: Audio) -> Option<Sound> {
        let mut link = audio.link();
        // sBgmVersion is 1 in every live game (set outside the decompiled code)
        let mut dev = SoundDevice::new(1);
        if dev.init(&mut link) < 0 {
            eprintln!("sound: VISoundDevice::Init failed");
            return None;
        }
        let ambtrack = iso::Iso::open(iso_path).ok().and_then(|mut d| d.read("DATA/AMBTRACK.ESF").ok()).and_then(|b| esf::Container::open(b).ok());
        Some(Sound { dev, link, audio, ambtrack, dict: HashMap::new(), loads: HashMap::new(), next_load: 1, track: SoundTrack::cleared(), stream_best: -1 })
    }

    // ---- the scene's dictionary and the loader --------------------------------------

    /// `VISoundDevice::ReleaseSound` and the dictionary entry with it.
    fn release(&mut self, i: i32) {
        self.dev.release_sound(i);
        if self.dev.sounds.get(i as usize).is_none_or(|s| s.is_none()) {
            self.dict.retain(|_, v| *v != i);
        }
    }

    fn available(&self, id: u32) -> bool {
        id != 0 && self.ambtrack.as_ref().is_some_and(|c| c.lookup(id as i32).is_some_and(|n| c.node(n).ty == esf::sound::XM))
    }

    /// `VILoader::Load`: the record parsed into the device and the dictionary.
    fn load(&mut self, id: u32) -> i32 {
        let Some(c) = &self.ambtrack else { return -1 };
        let Some(x) = c.lookup(id as i32).and_then(|n| esf::sound::xm(c, n)) else { return -1 };
        let i = self.dev.create_xm(x.volume, x.pan, x.module, x.samples);
        if i < 0 {
            return -1;
        }
        self.dict.insert(id, i);
        let h = self.next_load;
        self.next_load += 1;
        self.loads.insert(h, id);
        h
    }

    /// `VILoader::Release`: the loader's own reference goes.
    fn loader_release(&mut self, h: i32) {
        if let Some(id) = self.loads.remove(&h)
            && let Some(&i) = self.dict.get(&id)
        {
            self.release(i);
        }
    }

    // ---- a frame ----------------------------------------------------------------

    /// The world changed: `VIScene::StopSoundTrack`, and the region's music
    /// stops with the scene it was in.
    pub fn world_changed(&mut self) {
        self.stop_sound_track();
        self.stream_best = -1;
        self.dev.stop_bgm(1);
    }

    /// One frame of the game's sound, for the camera `eye` at `hour` and the
    /// scene time `time` (ms).
    pub fn frame(&mut self, world: &mut World<Ee>, eye: &Eye<Ee>, hour: f32, time: i64) {
        self.update_stream_emitters(world, eye);
        self.set_sound_track(world, eye, hour, time);
        self.dev.process(&mut self.link);
    }

    /// The streamed music: of the streamed-audio regions at the camera, the
    /// highest priority one plays on BGM slot 1 (restarted when it changes,
    /// or when it loops), and with none the slot stops.
    fn update_stream_emitters(&mut self, world: &mut World<Ee>, eye: &Eye<Ee>) {
        let (view, _) = vi::emit::camera_view(&eye.position, &eye.direction);
        let Some(q) = world.query_emitters_at_location(&eye.position, &view, 12, 32) else { return };
        let priority = |s: usize| match &world.sprites[s].kind {
            SpriteKind::Stream { priority, .. } => *priority,
            _ => 0,
        };
        let mut best = -1i32;
        let mut pri = 0;
        for e in &q {
            if priority(e.sprite) >= pri {
                best = e.sprite as i32;
                pri = priority(e.sprite);
            }
        }
        if best >= 0 {
            let SpriteKind::Stream { path, looping, .. } = &world.sprites[best as usize].kind else { return };
            if self.stream_best != best || *looping {
                self.stream_best = best;
                self.dev.play_bgm(1, &format!("Music\\{path}"));
            }
        } else {
            self.stream_best = -1;
            self.dev.stop_bgm(1);
        }
    }

    fn stop_sound_track(&mut self) {
        for k in 0..2 {
            let s = self.track.state[k];
            if s.p1c != -1 {
                self.dev.stop(s.p1c);
            }
            if s.p18 != -1 {
                self.loader_release(s.p18);
            }
            if s.p24 != -1 {
                self.release(s.p24);
            }
            self.track.state[k] = TrackState::cleared();
        }
        if self.track.p24 != -1 {
            self.dev.stop(self.track.p24);
            self.track.p24 = -1;
        }
        self.track.p00 = 1;
    }

    /// `VIScene::SetSoundTrack`: the terrain profile with the heaviest weight
    /// at the camera names a day and a night track. A change of track (at most
    /// once per 15 s, or at once the first time) moves the playing state to
    /// the second slot to fade out and loads the new one; each slot fades over
    /// 15 s, a slot faded out is retired, and a loaded one starts. Under water
    /// the music is muted and the underwater ambience plays instead.
    fn set_sound_track(&mut self, world: &World<Ee>, eye: &Eye<Ee>, hour: f32, time: i64) {
        let Some(grid) = &world.grid else { return };
        let b = grid.profile_blend(&eye.position);
        let (mut best, mut k) = (F::<Ee>::ZERO, 0);
        for j in 0..4 {
            if j == 0 || best < b.weight[j] {
                best = b.weight[j];
                k = j;
            }
        }
        let env = world.env_flags(&eye.position);
        let prof = vi::tod::PROFILES.get(b.index[k] as usize).map_or(0, |p| p.music);
        let hour = F::<Ee>::new(hour);
        let night = (hour < F::new(5.0) || F::new(20.0) < hour) as usize;
        let track = TRACKS.get(prof as usize).map_or(0, |t| t[night]);
        let t = &mut self.track;

        if (t.p10 < time - t.p18 || t.p00 != 0)
            && track != t.state[0].p20 {
                let s1 = t.state[1];
                if s1.p1c != -1 {
                    self.dev.stop(s1.p1c);
                }
                if s1.p18 != -1 {
                    self.loader_release(s1.p18);
                }
                if s1.p24 != -1 {
                    self.release(s1.p24);
                }
                let t = &mut self.track;
                if t.state[0].p24 != -1 {
                    t.state[0].p00 = 1;
                    t.state[0].p08 = time;
                    t.state[0].p10 = t.state[0].p14;
                }
                t.state[1] = t.state[0];
                t.state[0] = TrackState::cleared();
                t.state[0].p20 = track;
                t.p18 = time;
            }

        if self.track.state[0].p18 != -1 {
            // the loader's job is done the call after it began
            let Some(&idx) = self.dict.get(&track) else { return };
            self.dev.share_sound(idx);
            self.dev.set_repeat(idx, 1);
            self.track.state[0].p24 = idx;
            self.loader_release(self.track.state[0].p18);
            self.track.state[0].p18 = -1;
        } else if self.track.state[0].p24 == -1 {
            if let Some(&idx) = self.dict.get(&track) {
                self.dev.share_sound(idx);
                self.dev.set_repeat(idx, 1);
                self.track.state[0].p24 = idx;
            } else if self.available(track) {
                self.track.state[0].p18 = self.load(track);
                if self.track.state[0].p18 < 0 {
                    return;
                }
            }
        }

        // the weather's intensity is 0: the overcast does not dim it
        let q = F::<Ee>::ONE - F::ZERO;
        for j in 0..2 {
            let t = &mut self.track;
            if t.state[j].p1c == -1 {
                continue;
            }
            let s = &mut t.state[j];
            if s.p00 < 3 && s.p00 > 0 {
                let mut x = F::<Ee>::new((time - s.p08) as f32) / F::new(t.p10 as f32);
                x = if F::ZERO > x { F::ZERO } else { x };
                x = if F::ONE < x { F::ONE } else { x };
                let mut v = F::ZERO;
                if s.p00 != 1 {
                    v = t.p04;
                }
                v -= s.p10;
                v *= x;
                v += s.p10;
                s.p14 = v;
                if x == F::ONE {
                    if s.p00 == 1 {
                        s.p00 = 3;
                    } else if s.p00 == 2 {
                        s.p00 = 0;
                    }
                }
            }
            let mut v = s.p14;
            v *= q;
            v *= q;
            v *= q;
            v *= t.p30;
            if env & 0xF != 0 {
                v = F::ZERO;
            }
            let h = s.p1c;
            self.dev.set_volume(h, v.0);
        }

        for j in 0..2 {
            let s = self.track.state[j];
            if s.p00 == 3 {
                if s.p1c != -1 {
                    self.dev.stop(s.p1c);
                }
                if s.p18 != -1 {
                    self.loader_release(s.p18);
                }
                if s.p24 != -1 {
                    self.release(s.p24);
                }
                self.track.state[j] = TrackState::cleared();
            }
        }

        for j in 0..2 {
            let s = self.track.state[j];
            if s.p1c != -1 || s.p24 == -1 {
                continue;
            }
            let idx = s.p24;
            if !self.dev.sounds.get(idx as usize).and_then(|s| s.as_ref()).is_some_and(|s| s.is_cached()) {
                continue;
            }
            let h = self.dev.play(idx, 0, 0);
            let t = &mut self.track;
            t.state[j].p1c = h;
            if t.p00 != 0 {
                t.state[j].p14 = F::ONE;
                t.p00 = 0;
            } else {
                t.state[j].p14 = F::ZERO;
            }
            t.state[j].p00 = 2;
            t.state[j].p08 = time;
            t.state[j].p10 = t.state[j].p14;
            let mut v = t.state[j].p10 * q;
            v *= q;
            v *= q;
            v *= t.p30;
            if env & 0xF != 0 {
                v = F::ZERO;
            }
            self.dev.set_volume(h, v.0);
        }

        if env & 0xF != 0 {
            if self.track.p24 == -1 && self.track.p20 != -1 {
                self.track.p24 = self.dev.play(self.track.p20, 0, 0);
                self.dev.set_volume(self.track.p24, 0.7);
            }
        } else if self.track.p24 != -1 {
            self.dev.stop(self.track.p24);
            self.track.p24 = -1;
        }
    }
}
