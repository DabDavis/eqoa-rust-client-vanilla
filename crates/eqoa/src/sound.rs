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
//! The weather's intensity dims the track (`weather` is the atmosphere's), and
//! so does the outdoors intensity (`m_p30`, `outdoors`, set each frame from
//! the weather's probe); the rain and thunder are the atmosphere's own
//! (`weather.rs`). The track plays on the effects' class (`SetSoundTrack`'s
//! VISOUNDCLASS_SFX), so the options' Fx Volume sets it, not their Music
//! Volume, as on a PS2. The sound sprites' half of `UpdateSoundEmitters` is
//! `spells.rs`'s (no world places one; SPELLFX does).

use crate::audio::{Audio, AudioLink};
use snd::device::SoundDevice;
use std::collections::HashMap;
use vi::camera::Eye;
use vi::fpu::{Ee, F};
use vi::world::{SpriteKind, World};

/// A sound the client asks for: `VIClient::PlaySound(id)` (a row of
/// UISOUND.TXT at its `GetUISound` volume) or `PlayCue(name, volume)`.
#[derive(Clone, Debug)]
pub enum Cue {
    Ui(i32),
    Named(&'static str, f32),
}

/// SUPPORT's `sUISounds`: 17 (id, volume) pairs at this address in the
/// image, which is indexed by `VA - 0x003E3D00`.
const UI_SOUNDS_VA: usize = 0x004E_DA48;
const IMAGE_BASE: usize = 0x003E_3D00;

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
    /// the terrain profile the track was last chosen by, and its music
    /// index (`TRACKS`' row), -1 before: for the sound's line in a report
    profile: i32,
    music: i32,
}

impl SoundTrack {
    fn cleared() -> Self {
        SoundTrack { p00: 1, p04: F::ONE, p10: 15000, p18: 0, p20: -1, p24: -1, p30: F::ONE, state: [TrackState::cleared(); 2], profile: -1, music: -1 }
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
    /// The characters' sounds: DictID -> (the device's sound, the creatures
    /// sharing it), as each creature's loader shares them.
    chars: HashMap<u32, (i32, u32)>,
    /// the game's `rand()`
    rand: vi::csprite::Rand,
    /// `VIUI::m_sounds` (UISOUND.TXT) and `sUISounds`' volumes, once
    /// SUPPORT's image is there to read them from (`take_support`)
    ui: Option<(vi::strtab::StringTable, [f32; 17])>,
    /// UISOUND.TXT until then
    ui_names: Option<vi::strtab::StringTable>,
    /// `VIBattleMusic`'s state and track
    battle: (i32, i32),
    /// `VIGameOptions::m_language`, which is the battle-music option: 0 none,
    /// 1..8 a song, 9 (the default) a random one each fight
    pub battle_option: i32,
    /// the Options window's own VIBattleMusic (`VIWndOptions::m_music`),
    /// previewing a track on the same slot while its Sound page is up
    pub preview: Option<i32>,
    /// the weather's intensity (`m_atmosphere.m_intensity`), which dims the
    /// track: set by `weather.rs` each frame
    pub weather: F<Ee>,
    /// `CalcOutdoorsIntensity`'s value this frame (`m_soundTrack.m_p30`).
    pub outdoors: F<Ee>,
    /// voices playing that no sound holds (`Strays`)
    strays: Strays,
}

/// Voices the SPU2 plays that none of the device's sounds holds, the BGM's
/// pair aside, counted frame by frame (`Sound::frame`): one stopped in the
/// packet just sent still shows for a frame, so a voice counts once it has
/// played on so for `STRAY_FRAMES` (the device's own rule for a sound that
/// went quiet). A voice nothing holds is never stopped by anything, and plays
/// whatever its RAM holds now (the report's sound line; F3 20261025-225337).
#[derive(Clone)]
struct Strays {
    /// each voice's frames playing with no sound (core 0's 24, then core 1's)
    frames: [u8; 48],
    /// how many times a voice has reached `STRAY_FRAMES`
    times: u32,
}

const STRAY_FRAMES: u8 = 3;

impl Strays {
    fn new() -> Strays {
        Strays { frames: [0; 48], times: 0 }
    }

    /// A frame's look: `playing` the status block's voices (core 0, core 1)
    /// and `held` the device's channels.
    fn look(&mut self, playing: [u32; 2], held: u64) {
        let free = [!(held & 0xff_ffff) as u32, !((held >> 24) & 0xff_ffff) as u32 & !0xc0_0000];
        for c in 0..2 {
            for v in 0..24 {
                let k = c * 24 + v;
                if playing[c] & free[c] & (1 << v) != 0 {
                    self.frames[k] = self.frames[k].saturating_add(1);
                    if self.frames[k] == STRAY_FRAMES {
                        self.times += 1;
                        // when, for a report: a stop or a module's stop the
                        // device sent and SNDDRV never had
                        crate::say(&format!("sound: voice {v} of core {c} plays on with no sound to stop it"));
                    }
                } else {
                    self.frames[k] = 0;
                }
            }
        }
    }
}

thread_local! {
    /// A sound made before the world, with the files it starts with read
    /// already (a page's, `web::play`): the viewer takes it (`make`).
    static MADE: std::cell::RefCell<Option<Sound>> = const { std::cell::RefCell::new(None) };
}

/// The sound the viewer starts the game with: one made for it (`give`),
/// else a new one on the disc image at `iso_path`; with the options' Fx and
/// Music Volume on it from the start. The game sets them only once the
/// title's card check has passed (`VIWndSplash::Draw`), the card being where
/// a PS2 keeps them, so the loading pictures, the licence and the agreement
/// played at full volume; this client has them before its first sound (F3
/// 20261027-133042).
pub fn make(iso_path: &std::path::Path) -> Option<Sound> {
    let mut s = MADE.with(|m| m.borrow_mut().take()).or_else(|| Sound::new(iso_path))?;
    s.set_option_volumes(&crate::settings::game_options());
    Some(s)
}

/// Leaves a sound for the viewer (`make`).
#[cfg(target_arch = "wasm32")]
pub fn give(s: Sound) {
    MADE.with(|m| *m.borrow_mut() = Some(s));
}

impl Sound {
    /// The options' Fx and Music Volume on the two sound classes, as the
    /// game sets them (class 0 the effects, class 1 the music).
    pub fn set_option_volumes(&mut self, o: &crate::options::GameOptions) {
        self.dev.set_sound_class_volume(0, o.fx_volume.0);
        self.dev.set_sound_class_volume(1, o.music_volume.0);
    }

    /// The device on a fresh audio output, with AMBTRACK.ESF for the tracks;
    /// natively the music's files read whole on a thread of their own
    /// (`bgm::read_on_a_thread`), so the driver, under the lock the output's
    /// callback and the game's thread take, never waits on the disc.
    pub fn new(iso_path: &std::path::Path) -> Option<Sound> {
        let disc = iso::Iso::open(iso_path).ok()?;
        #[cfg(not(target_arch = "wasm32"))]
        let music: Box<dyn snd::Disc> = {
            let bgm = crate::bgm::Bgm::new(disc.entries());
            if crate::bgm::read_on_a_thread(&bgm, crate::bgm::image_reader(iso_path.to_path_buf())) { Box::new(bgm) } else { Box::new(crate::audio::IsoDisc(disc)) }
        };
        #[cfg(target_arch = "wasm32")]
        let music: Box<dyn snd::Disc> = Box::new(crate::audio::IsoDisc(disc));
        let audio = Audio::open(music);
        eprintln!("{}", audio.describe);
        let mut iso = iso::Iso::open(iso_path).ok()?;
        Self::with(&mut |path| iso.read(path).ok(), audio)
    }

    /// The same with no output: `audio().render` runs the driver.
    pub fn offline(iso_path: &std::path::Path) -> Option<Sound> {
        let disc = iso::Iso::open(iso_path).ok()?;
        let mut iso = iso::Iso::open(iso_path).ok()?;
        Self::with(&mut |path| iso.read(path).ok(), Audio::offline(Box::new(crate::audio::IsoDisc(disc))))
    }

    pub fn audio(&self) -> &Audio {
        &self.audio
    }

    /// What plays, in a line: the BGM request and track, the XM slots, and
    /// what the device holds: its sounds of the 128, the SPU RAM they take of
    /// the pool (the BGM ring's 256 KB among them), and the sounds it could
    /// not make since the start (no slot, no memory). Then, for a crunch
    /// (F3 20261025-225337): every uploaded sound's RAM against what its
    /// upload sent (`check_ram`: a sound whose RAM was overwritten plays
    /// garbled until it is let go), the voices playing that no sound of the
    /// device's holds (the BGM's pair aside), the packets and upload steps
    /// kept back, and the commands a full packet dropped (a busy frame's) and
    /// the stops it sent with the next one instead.
    pub fn describe(&self) -> String {
        let st = self.audio.status();
        let t = &self.track;
        let (free, largest) = self.dev.spu_free();
        let pool = snd::device::SPU_POOL;
        let ram = match self.audio.check_ram(&self.dev) {
            None => "not compared now (a copy still to go)".to_string(),
            Some(c) if c.differ.is_empty() => format!("all {} as uploaded", c.checked),
            Some(c) => format!("{} of {} NOT as uploaded (slots {:?})", c.differ.len(), c.checked, c.differ),
        };
        let lasting = |c: usize| (0..24).filter(|&v| self.strays.frames[c * 24 + v] >= STRAY_FRAMES).fold(0u32, |m, v| m | 1 << v);
        format!(
            "{}; sounds' RAM: {ram}; voices playing on with no sound {:06x} {:06x} ({} times since the start); packets up to {} words of 127, kept back {}, upload steps held {}, commands that found the packet full {}{} (stops sent next {}, volumes and pitches next {}); sets held between the game's frames {}",
            self.describe_play(st, t, free, largest, pool),
            lasting(0),
            lasting(1),
            self.strays.times,
            self.dev.most_words,
            self.dev.flushes_refused,
            self.dev.uploads_held,
            self.dev.cmds_dropped,
            dropped_kinds(&self.dev.dropped_by_id),
            self.dev.stops_dropped,
            self.dev.sets_kept,
            self.dev.held_sets
        )
    }

    fn describe_play(&self, st: [u32; 128], t: &SoundTrack, free: i32, largest: i32, pool: i32) -> String {
        format!(
            "battle {}, region music {} (bgm {} {:?}), ambient track {:08x}/{:08x} (states {}/{}; terrain profile {}, music {}, played as effects), voices {:06x} {:06x}; volumes: effects {:.2}, music {:.2}; sounds {} of 128, sound RAM {} of {} KB (largest free {} KB), failed creates {} (no slot {}, no memory {})",
            self.battle.1,
            self.stream_best,
            if st[6] != 0 { "streaming" } else { "off" },
            self.dev.bgm_playing(),
            t.state[0].p20,
            t.state[1].p20,
            t.state[0].p00,
            t.state[1].p00,
            t.profile,
            t.music,
            st[4],
            st[5],
            self.dev.sound_class_volume(0),
            self.dev.sound_class_volume(1),
            self.dev.sounds_held(),
            (pool - free) / 1024,
            pool / 1024,
            largest / 1024,
            self.dev.failed_creates[0] + self.dev.failed_creates[1],
            self.dev.failed_creates[0],
            self.dev.failed_creates[1]
        )
    }

    /// The device over `audio`, with the files it starts with read by
    /// `read` (a path on the disc, as `iso::Iso::read` takes it): AMBTRACK.ESF,
    /// the resident files' waves and UISOUND.TXT.
    pub fn with(read: &mut dyn FnMut(&str) -> Option<Vec<u8>>, audio: Audio) -> Option<Sound> {
        let mut link = audio.link();
        // sBgmVersion is 1 in every live game (set outside the decompiled code)
        let mut dev = SoundDevice::new(1);
        if dev.init(&mut link) < 0 {
            eprintln!("sound: VISoundDevice::Init failed");
            return None;
        }
        let ambtrack = read("DATA/AMBTRACK.ESF").and_then(|b| esf::Container::open(b).ok());
        Some(Sound {
            dev,
            link,
            audio,
            ambtrack,
            dict: HashMap::new(),
            loads: HashMap::new(),
            next_load: 1,
            track: SoundTrack::cleared(),
            stream_best: -1,
            chars: HashMap::new(),
            rand: vi::csprite::Rand(1),
            ui: None,
            ui_names: None,
            battle: (0, -1),
            battle_option: 9,
            preview: None,
            weather: F::ZERO,
            outdoors: F::ONE,
            strays: Strays::new(),
        })
        .map(|mut s| {
            s.load_resident(read);
            s
        })
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

    /// What is resident from the start: UI.CSF's sounds (the UI's),
    /// FX.CSF's (`respawn`, `death`, `amb_underwater`) and SKY.CSF's (the
    /// rain and the thunder, loaded with the scene's shared files), the UI's
    /// sound names, and their volumes once SUPPORT's image is there
    /// (`modules`).
    fn load_resident(&mut self, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) {
        for f in ["DATA2/UI.CSF", "DATA2/FX.CSF", "DATA2/SKY.CSF"] {
            let Some(c) = read(f).and_then(|b| esf::Container::open(b).ok()) else { continue };
            for w in esf::sound::all(&c).0 {
                let i = self.dev.create_wave(w.channels, w.blocks, w.rate, w.volume, w.pan, w.looped, &w.adpcm);
                if i >= 0 {
                    self.dict.insert(w.id, i);
                }
            }
        }
        // resident before the world is entered, as on the console: the whole
        // upload queue at once (VISoundDevice::Flush)
        self.dev.flush(&mut self.link);
        // VIScene::Init: the underwater ambience by name
        self.track.p20 = self.dict.get(&vi::hash::hash_resource_id("amb_underwater")).copied().unwrap_or(-1);
        self.ui_names = read("DATA/UISOUND.TXT").and_then(|b| vi::strtab::StringTable::parse(&b));
        if self.ui_names.is_none() {
            eprintln!("sound: no UI sounds (UISOUND.TXT is not on the disc)");
        }
        self.take_support();
    }

    /// `sUISounds`' volumes, out of SUPPORT's image once it has come
    /// (`modules`): the UI sounds play from then on.
    pub fn take_support(&mut self) {
        if self.ui.is_some() || self.ui_names.is_none() {
            return;
        }
        let Some(img) = crate::modules::get(crate::modules::Module::Support) else { return };
        let o = UI_SOUNDS_VA - IMAGE_BASE;
        let Some(b) = img.get(o..o + 17 * 8) else { return };
        let volumes = std::array::from_fn(|k| f32::from_le_bytes(b[k * 8 + 4..k * 8 + 8].try_into().unwrap()));
        self.ui = self.ui_names.take().map(|n| (n, volumes));
    }

    /// The device's sound for a resident wave's name, or -1 (what
    /// `VIDictionary::Find` hands `SetResources`).
    pub fn sound_named(&self, name: &str) -> i32 {
        self.dict.get(&vi::hash::hash_resource_id(name)).copied().unwrap_or(-1)
    }

    /// `VIUI::SoundPlay(id)`: the row's name at `GetUISound`'s volume (an
    /// id past the table takes entry 0's).
    pub fn play_ui(&mut self, id: i32) {
        let Some((names, vols)) = &self.ui else { return };
        let name = names.get(id, 0).to_string();
        let vol = vols[if (0..17).contains(&id) { id as usize } else { 0 }];
        self.play_named(&name, vol);
    }

    /// `VIUI::SoundPlay(name, vol)`: the sound by its name's hash, at
    /// priority 127, then its volume.
    pub fn play_named(&mut self, name: &str, vol: f32) {
        if let Some(&i) = self.dict.get(&vi::hash::hash_resource_id(name)) {
            let h = self.dev.play(i, 127, 0);
            if h >= 0 {
                self.dev.set_volume(h, vol);
            }
        }
    }

    pub fn cue(&mut self, c: &Cue) {
        match c {
            Cue::Ui(id) => self.play_ui(*id),
            Cue::Named(n, v) => self.play_named(n, *v),
        }
    }

    // ---- the battle music --------------------------------------------------------
    // gFeatureFlags is 0x300 in every live game, so VIBattleMusic's streamed
    // half: a battleNN.bgm on BGM slot 0, which out-ranks the region's on 1.

    /// `VIClient::Process`'s battle music: `Play(option)` every frame the
    /// player (`LeaderByte263`; groups are not ported) is fighting, else
    /// `Stop`. `None`: no creature of the player's yet (dead or not in the
    /// world): `Stop` too.
    pub fn battle_music(&mut self, fighting: Option<bool>) {
        if let Some(t) = self.preview {
            self.battle_play(t);
        } else if fighting == Some(true) {
            self.battle_play(self.battle_option);
        } else {
            // Stop: the state, and the slot's request
            self.battle.0 = 0;
            self.dev.stop_bgm(0);
        }
    }

    /// The Options window's `m_music.StopNow()`: its slot stopped and
    /// handed back to the fight.
    pub fn end_preview(&mut self) {
        if self.preview.take().is_some() {
            self.battle.0 = 0;
            self.dev.stop_bgm(0);
        }
    }

    /// `VIBattleMusic::Play`: 9 picks one of eight when a fight starts.
    fn battle_play(&mut self, track: i32) {
        if track == 9 {
            if self.battle.0 == 0 {
                self.battle.0 = 3;
                self.battle.1 = self.rand.rand() % 8 + 1;
            }
        } else if track != -1 {
            self.battle.1 = track;
        }
        if self.battle.1 == 0 {
            self.battle.0 = 0;
            self.dev.stop_bgm(0);
            return;
        }
        let path = format!("bgm\\battle{:02}.bgm", self.battle.1);
        self.dev.play_bgm(0, &path);
    }

    // ---- the characters' sounds ---------------------------------------------------

    /// A creature was loaded: the waves its play list names are made (from
    /// whichever loaded file holds them) or shared.
    pub fn share_sounds(&mut self, lib: &esf::Library, ids: &[u32]) {
        for &id in ids {
            if let Some(e) = self.chars.get_mut(&id) {
                self.dev.share_sound(e.0);
                e.1 += 1;
                continue;
            }
            let Some(r) = lib.lookup(id as i32) else { continue };
            let c = lib.file(r.file);
            if c.node(r.node).ty != esf::sound::WAVE {
                continue;
            }
            let Some(w) = esf::sound::wave(c, r.node) else { continue };
            let i = self.dev.create_wave(w.channels, w.blocks, w.rate, w.volume, w.pan, w.looped, &w.adpcm);
            if i >= 0 {
                self.chars.insert(id, (i, 1));
            }
        }
    }

    /// A creature's sprite goes (it left, or is made anew for another model
    /// or look): `VICSprite::Release`'s sound half. Its continuous sound
    /// (`snd.channel`, a loop) is stopped, then its share of each sound let
    /// go. Releasing alone leaves the loop playing for as long as another
    /// creature shares its wave, and on after a logout.
    pub fn let_go(&mut self, ids: &[u32], cont_channel: i32) {
        if cont_channel != -1 {
            self.dev.stop(cont_channel);
        }
        self.release_sounds(ids);
    }

    /// The creature went: its share of each sound with it.
    pub fn release_sounds(&mut self, ids: &[u32]) {
        for id in ids {
            let Some(e) = self.chars.get_mut(id) else { continue };
            self.dev.release_sound(e.0);
            e.1 -= 1;
            if e.1 == 0 {
                self.chars.remove(id);
            }
        }
    }

    /// The device as `VICSprite::Process` sees it.
    pub fn sink(&mut self) -> Sink<'_> {
        Sink(self)
    }

    // ---- a frame ----------------------------------------------------------------

    /// The world changed: `VIScene::StopSoundTrack`, and the region's music
    /// stops with the scene it was in.
    pub fn world_changed(&mut self) {
        self.stop_sound_track();
        self.stream_best = -1;
        self.dev.stop_bgm(1);
    }

    /// The output fed between the game's frames: on every frame drawn that
    /// is not one (`frame`, `frame_alone`), and a page's animation frames
    /// that its cap skips (`web::frame_due`), since feeding there too halves
    /// the time between feeds, which a page's lead is sized by
    /// (`audio::lead_for`). Only mixing (natively nothing: the output's
    /// callback mixes): the game's sound waits for its frame.
    pub fn feed(&mut self) {
        self.audio.feed();
    }

    /// One frame of the game's sound, for the camera `eye` at `hour` and the
    /// scene time `time` (ms): once a frame of the game's, never once a
    /// frame drawn, as `VIClient::Process` runs `VISoundDevice::Process`
    /// (`pregame::host`'s `sound_frame` says what it counts in frames).
    pub fn frame(&mut self, world: &mut World<Ee>, eye: &Eye<Ee>, hour: f32, time: i64) {
        self.update_stream_emitters(world, eye);
        self.set_sound_track(world, eye, hour, time);
        self.dev.process(&mut self.link);
        let st = self.audio.status();
        self.strays.look([st[4], st[5]], self.dev.channels());
        self.audio.feed();
    }

    // ---- before the world (`pregame`) -----------------------------------------

    /// One frame of the device with no world (`VISoundDevice::Process`, as
    /// SETUP's, STATION's and the character screens' loops run it: once a
    /// frame of theirs, `pregame::host`'s `sound_frame`).
    pub fn frame_alone(&mut self) {
        self.dev.process(&mut self.link);
        self.audio.feed();
    }

    /// SETUP's `StartAmbientSoundtrack` and `VIStation::PlayMusic`: an XM out
    /// of a file the pregame read (SETUP.CSF's `splash_screen`), made on the
    /// device and uploaded at once, its repeat set, played at priority
    /// `priority` on class 0 (the music's) at `volume`. The playback's handle
    /// and the device's sound (to release with `stop_xm`), or None. SETUP's
    /// Init uploads what SETUP.CSF made before the title (`m_sound.Flush()`
    /// after its parse), so the title's Play finds the XM resident; made here,
    /// it is flushed here, or Play refuses it while it uploads (-2) and the
    /// title has no music.
    pub fn play_xm_in(&mut self, c: &esf::Container, id: u32, repeat: bool, priority: i32, volume: f32) -> Option<(i32, i32)> {
        let x = c.lookup(id as i32).and_then(|n| esf::sound::xm(c, n))?;
        let i = self.dev.create_xm(x.volume, x.pan, x.module, x.samples);
        if i < 0 {
            return None;
        }
        self.dev.flush(&mut self.link);
        self.dev.set_repeat(i, repeat as i32);
        let h = self.dev.play(i, priority, 0);
        if h < 0 {
            self.dev.release_sound(i);
            return None;
        }
        self.dev.set_volume(h, volume);
        Some((h, i))
    }

    /// `VISoundDevice::Stop(handle)`, and the sound `play_xm_in` made let go.
    pub fn stop_xm(&mut self, (h, i): (i32, i32)) {
        self.dev.stop(h);
        self.dev.release_sound(i);
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
        self.track.p30 = self.outdoors;
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
        (t.profile, t.music) = (b.index[k], prof);

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

        let q = F::<Ee>::ONE - self.weather;
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

/// The commands a full packet dropped, by kind (`SoundDevice::dropped_by_id`),
/// as the sound line says them: none, or ": 12 volume, 3 play".
fn dropped_kinds(by_id: &[u32; 16]) -> String {
    const NAMES: [&str; 16] = ["init", "upload", "play", "frame", "stop", "pitch", "volume", "relocation", "xm play", "xm stop", "xm volume", "xm pitch", "free", "bgm play", "bgm stop", "bgm volume"];
    let mut kinds: Vec<(u32, &str)> = by_id.iter().zip(NAMES).filter(|(n, _)| **n != 0).map(|(&n, k)| (n, k)).collect();
    kinds.sort_by(|a, b| b.0.cmp(&a.0));
    if kinds.is_empty() {
        return String::new();
    }
    format!(": {}", kinds.iter().map(|(n, k)| format!("{n} {k}")).collect::<Vec<_>>().join(", "))
}

pub struct Sink<'a>(&'a mut Sound);

impl vi::csprite::SoundSink for Sink<'_> {
    fn play(&mut self, sound: u32) -> i32 {
        match self.0.chars.get(&sound) {
            Some(&(i, _)) => self.0.dev.play(i, 0, 0),
            None => -1,
        }
    }
    fn stop(&mut self, h: i32) {
        self.0.dev.stop(h);
    }
    fn is_playing(&mut self, h: i32) -> bool {
        self.0.dev.is_playing(h)
    }
    fn set_volume(&mut self, h: i32, v: f32) {
        self.0.dev.set_volume(h, v);
    }
    fn set_pan(&mut self, h: i32, p: f32) {
        self.0.dev.set_pan(h, p);
    }
    fn set_pitch(&mut self, h: i32, p: i32) {
        self.0.dev.set_pitch(h, p);
    }
    fn rand(&mut self) -> i32 {
        self.0.rand.rand()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A voice playing that no sound holds counts once it has played on so
    /// for three frames, once a run; the controls: a voice a sound holds, the
    /// BGM's pair, and one seen with no sound for only two frames (a stop
    /// in the packet just sent shows for a frame) never count.
    #[test]
    fn voices_no_sound_holds_count_after_three_frames() {
        let mut s = Strays::new();
        // channel 2 held (core 0 voice 2); core 0 voice 5 and core 1 voice 1
        // (channel 25) playing with no sound; the BGM pair (core 1 22, 23)
        let held = 1u64 << 2;
        let playing = [(1 << 2) | (1 << 5), (1 << 1) | 0xc0_0000];
        s.look(playing, held);
        s.look(playing, held);
        assert_eq!(s.times, 0, "two frames: a stop not yet heard");
        s.look(playing, held);
        assert_eq!(s.times, 2, "three: core 0 voice 5 and core 1 voice 1");
        assert_eq!((s.frames[5], s.frames[24 + 1], s.frames[2], s.frames[24 + 22]), (3, 3, 0, 0));
        s.look(playing, held);
        assert_eq!(s.times, 2, "once a run");
        s.look([1 << 2, 0], held);
        s.look(playing, held);
        s.look(playing, held);
        assert_eq!(s.times, 2, "a new run starts again from nothing");
        s.look(playing, held);
        assert_eq!(s.times, 4);
    }

    /// The options' volumes go on the classes as the game sets them (Fx on
    /// class 0, the effects; Music on class 1); the control, the device's own
    /// from its `Init`, both full, which is what a sound made before the
    /// title's card check had (F3 20261027-133042).
    #[test]
    fn the_options_volumes_go_on_their_classes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !path.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let mut s = Sound::offline(&path).unwrap();
        assert_eq!((s.dev.sound_class_volume(0), s.dev.sound_class_volume(1)), (1.0, 1.0), "the device's own");
        let o = crate::options::GameOptions { fx_volume: F::new(0.3), music_volume: F::ZERO, ..Default::default() };
        s.set_option_volumes(&o);
        assert_eq!((s.dev.sound_class_volume(0), s.dev.sound_class_volume(1)), (0.3, 0.0));
    }

    /// The ambience at a spot of `world`, three seconds of it at `hour`, with
    /// the two classes' volumes (Fx, Music) as the options set them: the
    /// sound's line once the track plays, and its samples.
    fn ambience_at(path: &std::path::Path, world: &str, x: f32, z: f32, hour: f32, fx: f32, music: f32) -> (String, Vec<i16>) {
        // the world's zones off this disc image (the game's `disc_path` is
        // the player's, which a test does not have)
        let image = path.to_path_buf();
        crate::stream::set_source_factory(Box::new(move |lib, file| crate::stream::FileZones::open(&image, &lib.files[file].0).map(|z| Box::new(z) as Box<dyn crate::stream::ZoneSource>)));
        let mut lib = esf::Library::new();
        let f = lib.add(world, crate::stream::open_file(path, world).unwrap());
        if let Some(mut src) = crate::stream::source_for(&lib, f) {
            crate::stream::fill(&mut lib, f, x, z, src.as_mut());
        }
        let w = vi::world::World::<Ee>::load(&lib, f).unwrap();
        let mut walker = crate::walker::Walker::new(w, x, z, 0.0);
        let mut s = Sound::offline(path).unwrap();
        s.dev.set_sound_class_volume(0, fx);
        s.dev.set_sound_class_volume(1, music);
        let (mut line, mut pcm) = (String::new(), Vec::new());
        for k in 0..90 {
            let now = k * 1000 / 30;
            walker.frame(crate::walker::Controls::default(), now);
            let eye = walker.eye;
            s.frame(&mut walker.scene, &eye, hour, now);
            pcm.extend(s.audio.render(1600));
            if k == 60 {
                line = s.describe();
            }
        }
        (line, pcm)
    }

    /// Bird's spot in Lavastorm (F3 20261023-140759): the terrain's profile
    /// there is 19, the world's own storm profile, whose music is the eighth
    /// (`VIWorld::SetProfiles`), so its day track plays, as on a PS2. The
    /// track is the effects' (`SetSoundTrack` plays it on VISOUNDCLASS_SFX):
    /// the options' Fx Volume at 0 makes it silent, their Music Volume at 0
    /// leaves it as loud as it was. Each half is the other's control: a
    /// track on the music's class fails both.
    #[test]
    fn the_ambience_is_the_effects_class() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !path.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let loud = |pcm: &[i16]| pcm.iter().filter(|v| v.unsigned_abs() > 16).count();
        let (line, full) = ambience_at(&path, "LAVASTM.ESF", 5239.9, 4431.2, 6.49, 1.0, 1.0);
        assert!(line.contains("ambient track 1adcc066/") && line.contains("terrain profile 19, music 8"), "{line}");
        assert!(line.contains("volumes: effects 1.00, music 1.00"), "{line}");
        let n = loud(&full);
        assert!(n > full.len() / 4, "the track plays: {n} of {}", full.len());
        let (_, no_fx) = ambience_at(&path, "LAVASTM.ESF", 5239.9, 4431.2, 6.49, 0.0, 1.0);
        assert_eq!(loud(&no_fx), 0, "Fx Volume 0: the ambience is silent");
        let (line, no_music) = ambience_at(&path, "LAVASTM.ESF", 5239.9, 4431.2, 6.49, 1.0, 0.0);
        assert!(line.contains("volumes: effects 1.00, music 0.00"), "{line}");
        assert_eq!(no_music, full, "Music Volume 0: the ambience as it was");
    }

    /// A creature's continuous sound (a loop) stops when its sprite goes,
    /// as `VICSprite::Release` stops `m_snd.m_channel` before it releases,
    /// even while another creature of its model still shares the wave. The
    /// control only releases, as the client did: the loop plays on.
    #[test]
    fn a_creatures_loop_stops_when_it_goes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !path.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let mut iso = iso::Iso::open(&path).unwrap();
        let ch = esf::Container::open(iso.read("DATA/CHAR.ESF").unwrap()).unwrap();
        // the continuous sound twelve of CHAR.ESF's models loop
        let id = 0x1db6_ca44u32;
        let w = esf::sound::wave(&ch, ch.lookup(id as i32).unwrap()).unwrap();
        assert_eq!(w.looped, 1, "a loop");
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", ch);
        for stop in [false, true] {
            let audio = Audio::offline(Box::new(crate::audio::IsoDisc(iso::Iso::open(&path).unwrap())));
            let mut s = Sound::with(&mut |p| iso.read(p).ok(), audio).unwrap();
            // two creatures of the model come; the first one's loop plays
            s.share_sounds(&lib, &[id]);
            s.share_sounds(&lib, &[id]);
            s.dev.flush(&mut s.link);
            let h = vi::csprite::SoundSink::play(&mut s.sink(), id);
            assert!(h >= 0, "the loop starts: {h}");
            for _ in 0..30 {
                s.frame_alone();
                s.audio.render(1600);
            }
            assert!(s.dev.is_playing(h), "the loop plays");
            // the first creature goes
            if stop {
                s.let_go(&[id], h);
            } else {
                s.release_sounds(&[id]);
            }
            for _ in 0..30 {
                s.frame_alone();
                s.audio.render(1600);
            }
            assert_eq!(s.dev.is_playing(h), !stop, "stopped with its creature: {stop}");
            assert!(s.chars.contains_key(&id), "the other creature still shares the wave");
        }
    }

    /// The title's music plays: SETUP.CSF's `splash_screen` is uploaded
    /// before it is played, as SETUP's Init flushes, and a second of it at
    /// 30 frames a second is not silence. The control: played as soon as it
    /// is made, the device refuses it while it uploads.
    #[test]
    fn the_titles_music_plays() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !path.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let mut iso = iso::Iso::open(&path).unwrap();
        let setup = esf::Container::open(iso.read("DATA2/SETUP.CSF").unwrap()).unwrap();
        let id = vi::hash::hash_resource_id("splash_screen");
        let audio = Audio::offline(Box::new(crate::audio::IsoDisc(iso::Iso::open(&path).unwrap())));
        let mut s = Sound::with(&mut |p| iso.read(p).ok(), audio).unwrap();
        // control: made and played at once
        let x = esf::sound::xm(&setup, setup.lookup(id as i32).unwrap()).unwrap();
        let i = s.dev.create_xm(x.volume, x.pan, x.module, x.samples);
        assert_eq!(s.dev.play(i, 127, 0), -2, "still uploading");
        s.dev.release_sound(i);
        let (h, _) = s.play_xm_in(&setup, id, true, 127, 1.0).expect("the title's music plays");
        let mut pcm = Vec::new();
        for _ in 0..30 {
            s.frame_alone();
            pcm.extend(s.audio.render(1600));
        }
        assert!(s.dev.is_playing(h), "still playing a second on");
        let loud = pcm.iter().filter(|&&v| v.unsigned_abs() > 64).count();
        assert!(loud > pcm.len() / 4, "music, not silence: {loud} of {} samples", pcm.len());
    }
}
