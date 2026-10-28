//! `VICSprite`, the character: a skeleton (`HSprite`) plus a table of 66
//! animation entries, the arbiter that picks one per body layer
//! (`SetLocomotion` → `SetAnimation`) and the priorities it ranks them by
//! (decomp `vicsprite.cpp`), and the sounds its animations' triggers fire
//! (`Process`, through `SoundSink`). Not ported: items beyond `AttachItem`'s
//! test (`item_node`; the client attaches them itself), particles, weapon
//! trails, death, faces and armour (the `SetItemAction` call is a no-op).

use crate::fpu::{F, Model};
use crate::hsprite::{AnimScene, HSprite, hash_ref_id};
use crate::vect::Vect3;

/// What `VICSprite::Process` asks of the sound device. A sound is named by
/// its DictID (the parser resolves it to the device's index; the sink does
/// that here), a playback by its handle.
pub trait SoundSink {
    /// `Play(sound, 0, SFX)`: a handle, or below 0
    fn play(&mut self, sound: u32) -> i32;
    fn stop(&mut self, handle: i32);
    fn is_playing(&mut self, handle: i32) -> bool;
    fn set_volume(&mut self, handle: i32, v: f32);
    fn set_pan(&mut self, handle: i32, p: f32);
    fn set_pitch(&mut self, handle: i32, p: i32);
    /// the game's `rand()`
    fn rand(&mut self) -> i32;
}

/// `VICSpriteSoundState`: the continuous sound, its volume and playback.
#[derive(Clone, Copy, Debug)]
pub struct ContSound<M: Model> {
    pub sound: u32,
    pub volume: F<M>,
    pub channel: i32,
}

/// `VICSpriteAnimEntry`.
#[derive(Clone, Copy, Debug)]
pub struct Entry<M: Model> {
    /// The clip per layer (0 lower/whole body, 1 upper), or -1.
    pub animation: [i32; 2],
    /// The play speed `AddPlay` is given.
    pub weight: F<M>,
    /// The playback type: 0 loop, 1 once.
    pub sync: i32,
    pub priority: [i32; 2],
    /// The sound per layer (a DictID, 0 none), its volume scale and its
    /// pitch variation.
    pub sound: [u32; 2],
    pub volume: [F<M>; 2],
    pub pitch_var: [F<M>; 2],
    /// The body layer whose triggers sound it (`SetAnimSoundChannels`).
    pub channel: i32,
}

impl<M: Model> Default for Entry<M> {
    fn default() -> Self {
        Entry { animation: [-1, -1], weight: F::ONE, sync: 0, priority: [0, 0], sound: [0, 0], volume: [F::ONE; 2], pitch_var: [F::ZERO; 2], channel: 0 }
    }
}

/// `VICSpritePlay`: what a layer is playing.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    pub anim: i32,
    pub play: i32,
    pub sync: i32,
    /// `m_flags`: the playback of the layer's last trigger sound, or -1.
    pub flags: i32,
}

impl Default for Layer {
    fn default() -> Self {
        Layer { anim: 0, play: -1, sync: 0, flags: -1 }
    }
}

/// newlib's `rand()`: `next = next * 6364136223846793005 + 1`, the high word.
#[derive(Clone, Debug)]
pub struct Rand(pub u64);

impl Rand {
    pub fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.0 >> 32) & 0x7FFF_FFFF) as i32
    }
}

pub struct CSprite<M: Model> {
    pub h: HSprite<M>,
    /// 1: two layers (lower and upper body).
    pub skel_type: i32,
    pub race: i32,
    pub sex: i32,
    /// `m_slots`: for each of the 19 texture slots (0-4 armour, 7 face, 8-18
    /// hair), the material-palette entry and layer it drives; -1 for none.
    pub text_slots: [(i32, i32); 19],
    /// `m_items[i].m_node` (0x2920): the node each item slot hangs on.
    pub item_nodes: [i32; 6],
    /// `m_flags & 1`, `Init`'s `npc` (the 0x2710 header's last field from
    /// version 4, 1 below): while it is clear `AttachItem` attaches nothing
    /// and leaves the hands' attack types at 11. Clear on the creatures a
    /// form or an illusion makes of a player (Sandstorm's bard in Mouse Form
    /// is 0x8150E6AF), which name item nodes all the same.
    pub holds_items: bool,
    /// `m_node` (0x2915): named nodes; 8 is the helm's.
    pub node_ids: [i32; 10],
    pub dead: bool,
    pub action_seq: i32,
    pub action: i32,
    pub cur: [Layer; 2],
    pub prev: [Layer; 2],
    pub anim_time: i64,
    pub idle_time: i64,
    pub entries: [Entry<M>; 66],
    /// The body skin sprite (`m_body[0]`).
    pub body: i32,
    /// `m_snd`, the continuous sound (0x2940).
    pub snd: ContSound<M>,
}

fn in_range(id: i32, lo: i32, n: u32) -> bool {
    ((id - lo) as u32) < n
}

impl<M: Model> CSprite<M> {
    /// `Init(skel, race, sex, scale, npc)` on a cleared sprite, `npc` as
    /// `holds_items`.
    pub fn new(h: HSprite<M>, skel_type: i32, race: i32, sex: i32, scale: F<M>, holds_items: bool) -> Self {
        let mut h = h;
        h.default_scale = scale;
        CSprite {
            h,
            skel_type,
            race,
            sex,
            text_slots: [(-1, -1); 19],
            item_nodes: [-1; 6],
            holds_items,
            node_ids: [-1; 10],
            dead: false,
            action_seq: 0,
            action: -1,
            cur: [Layer::default(); 2],
            prev: [Layer::default(); 2],
            anim_time: 0,
            idle_time: 0,
            entries: [Entry::default(); 66],
            body: -1,
            snd: ContSound { sound: 0, volume: F::ONE, channel: -1 },
        }
    }

    /// `AttachItem`'s test: the node an item in `slot` hangs on, or None when
    /// the sprite attaches nothing there (`m_flags & 1` clear, or no
    /// `m_items[slot].m_node`).
    pub fn item_node(&self, slot: usize) -> Option<i32> {
        let node = *self.item_nodes.get(slot)?;
        (self.holds_items && node >= 0).then_some(node)
    }

    /// `SetAnimSoundChannels`: a two-layer skeleton sounds its first 17
    /// entries (the locomotion) from the lower body, everything else (and
    /// every entry of a one-layer skeleton) from layer 0.
    pub fn set_anim_sound_channels(&mut self) {
        if self.skel_type != 1 {
            for e in &mut self.entries[..19] {
                e.channel = 0;
            }
        } else {
            for e in &mut self.entries[..17] {
                e.channel = 1;
            }
        }
    }

    /// `CalcVolumeAndPan`: the volume falls off linearly to nothing at 50
    /// units; the pan is the cosine between the listener-to-source direction
    /// and the camera's right, in the ground plane.
    pub fn calc_volume_and_pan(cam_pos: &Vect3<M>, cam_dir: &Vect3<M>, at: &Vect3<M>) -> (F<M>, F<M>) {
        let mut vol = cam_pos.distance(at) * F::new(0.02);
        let x = F::ONE - vol;
        let x = if F::ZERO > x { F::ZERO } else { x };
        vol = if F::ONE < x { F::ONE } else { x };
        if vol > F::<M>::ZERO {
            let mut d = Vect3::new(cam_pos.x - at.x, F::ZERO, cam_pos.z - at.z);
            let mut r = Vect3::new(cam_dir.z, F::ZERO, -cam_dir.x);
            d.normalize();
            r.normalize();
            (vol, d.x * r.x + d.z * r.z)
        } else {
            (vol, F::ZERO)
        }
    }

    /// `VICSprite::Process`'s sounds: each tripped sound trigger plays its
    /// layer's sound on the entry that layer is playing (stopping the last
    /// one), at the distance's volume and pan and a random pitch within the
    /// entry's variation; a particle trigger is only reset; then the
    /// continuous sound is kept playing at the right volume and pan. Without
    /// a device a sound trigger stays tripped.
    pub fn process_sounds(&mut self, cam_pos: &Vect3<M>, cam_dir: &Vect3<M>, at: &Vect3<M>, mut sd: Option<&mut dyn SoundSink>) {
        let id = |n: &str| hash_ref_id(n);
        let (prim, prim_bpu, prim_bpl) = (id("Primary Sound"), id("Primary Sound_BPU"), id("Primary Sound_BPL"));
        let (sec, sec_bpu, sec_bpl) = (id("Secondary Sound"), id("Secondary Sound_BPU"), id("Secondary Sound_BPL"));
        let particles = ["Particle 1", "Particle 1_BPL", "Particle 1_BPU", "Particle 2", "Particle 2_BPL", "Particle 2_BPU"].map(id);
        for i in 0..self.h.triggers.len() {
            if self.h.triggers[i].tripped == 0 {
                continue;
            }
            let t = self.h.triggers[i].id;
            let mut layer = -1;
            let mut sel = 0usize;
            if t == prim || t == prim_bpl || t == prim_bpu {
                layer = 0;
            } else if t == sec || t == sec_bpl || t == sec_bpu {
                layer = 1;
            } else if particles.contains(&t) {
                sel = 1;
            }
            if layer != -1 {
                let Some(sd) = sd.as_deref_mut() else { continue };
                let layer = layer as usize;
                sel = (t == prim_bpl || t == sec_bpl) as usize;
                let e = self.entries[self.cur[sel].anim.clamp(0, 65) as usize];
                if sel as i32 == e.channel && e.sound[layer] != 0 {
                    if self.cur[sel].flags != -1 {
                        sd.stop(self.cur[sel].flags);
                        self.cur[sel].flags = -1;
                    }
                    let (mut vol, pan) = Self::calc_volume_and_pan(cam_pos, cam_dir, at);
                    vol = vol * e.volume[layer];
                    if vol > F::ZERO {
                        let mut pitch = F::<M>::new((sd.rand() & 0x1FFF) as f32) - F::new(4096.0);
                        pitch = pitch * e.pitch_var[layer];
                        let h = sd.play(e.sound[layer]);
                        pitch = pitch + F::new(4096.0);
                        if h >= 0 {
                            sd.set_volume(h, vol.0);
                            sd.set_pan(h, pan.0);
                            sd.set_pitch(h, pitch.0 as i32);
                            self.cur[sel].flags = h;
                        } else {
                            self.cur[sel].flags = -1;
                        }
                    }
                }
                self.h.reset_trigger(i);
            } else if sel != 0 {
                // the matching one-shot particle emitters restart: not ported
                self.h.reset_trigger(i);
            }
        }
        if let Some(sd) = sd
            && self.snd.sound != 0
        {
            let (mut vol, pan) = Self::calc_volume_and_pan(cam_pos, cam_dir, at);
            vol = vol * self.snd.volume;
            if vol > F::ZERO && !self.dead {
                if self.snd.channel == -1 || !sd.is_playing(self.snd.channel) {
                    self.snd.channel = sd.play(self.snd.sound);
                }
                if self.snd.channel >= 0 {
                    sd.set_volume(self.snd.channel, vol.0);
                    sd.set_pan(self.snd.channel, pan.0);
                }
            } else if self.snd.channel != -1 {
                sd.stop(self.snd.channel);
                self.snd.channel = -1;
            }
        }
    }

    /// Every sound the sprite can play: what its loader shares with it.
    pub fn sounds(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.entries.iter().flat_map(|e| e.sound).chain(std::iter::once(self.snd.sound)).filter(|&s| s != 0).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// `SetDefaultEntries`: an entry the file leaves empty borrows idle's,
    /// or a related action's.
    pub fn set_default_entries(&mut self) {
        let e = &mut self.entries;
        for i in 0..66 {
            if e[i].animation[0] != -1 {
                continue;
            }
            match i {
                0..=6 | 8..=18 | 37..=39 | 45 | 46 => e[i] = e[0],
                65 => {
                    if self.race == 9 {
                        e[i] = e[0];
                        e[i].sync = 1;
                    } else if e[41].animation[0] != -1 {
                        e[i] = e[41];
                        e[i].weight *= F::new(0.5);
                    } else {
                        e[i] = e[0];
                        e[i].sync = 1;
                    }
                }
                7 | 34 | 56 | 57 | 59 | 60 | 61 | 63 | 64 => {
                    e[i] = e[0];
                    e[i].sync = 1;
                }
                23 | 25 | 26 | 28 | 29 | 31 | 33 | 35 | 41 | 42 | 43 | 44 | 50 | 51 | 52 | 53 => {
                    if e[23].animation[0] != -1 {
                        e[i] = e[23];
                    } else {
                        e[i] = e[0];
                        e[i].sync = 1;
                    }
                }
                22 | 24 | 27 | 30 | 32 | 36 | 40 => {
                    if e[22].animation[0] != -1 {
                        e[i] = e[22];
                    } else if e[23].animation[0] != -1 {
                        e[i] = e[23];
                    } else {
                        e[i] = e[0];
                        e[i].sync = 1;
                    }
                }
                _ => {}
            }
        }
    }

    /// `SetAnimPriorities`: the ranking table, per layer.
    pub fn set_anim_priorities(&mut self) {
        const P0: [(usize, i32); 56] = [
            (0, 2), (1, 2), (2, 2), (3, 2), (4, 2), (5, 2), (6, 2), (7, 3), (8, 3), (9, 3), (10, 2), (11, 2),
            (12, 2), (13, 2), (14, 7), (15, 0), (16, 1), (17, 2), (18, 6), (22, 5), (23, 5), (24, 5), (25, 5),
            (26, 5), (27, 5), (28, 5), (29, 5), (30, 5), (31, 5), (32, 5), (33, 5), (34, 4), (35, 5), (36, 5),
            (37, 5), (38, 5), (39, 5), (40, 5), (41, 5), (42, 5), (43, 5), (44, 5), (45, 5), (46, 5), (50, 5),
            (51, 5), (52, 5), (53, 5), (56, 4), (57, 4), (59, 4), (60, 4), (61, 4), (63, 4), (64, 4), (65, 4),
        ];
        const P1: [(usize, i32); 56] = [
            (0, 2), (1, 6), (2, 6), (3, 6), (4, 6), (5, 6), (6, 6), (7, 7), (8, 3), (9, 3), (10, 6), (11, 6),
            (12, 6), (13, 6), (14, 8), (15, 0), (16, 1), (17, 1), (18, 7), (22, 5), (23, 5), (24, 5), (25, 5),
            (26, 5), (27, 5), (28, 5), (29, 5), (30, 5), (31, 5), (32, 5), (33, 5), (34, 4), (35, 5), (36, 5),
            (37, 5), (38, 5), (39, 5), (40, 5), (41, 5), (42, 5), (43, 7), (44, 7), (45, 5), (46, 5), (50, 5),
            (51, 5), (52, 6), (53, 6), (56, 4), (57, 4), (59, 4), (60, 4), (61, 4), (63, 4), (64, 4), (65, 4),
        ];
        // Entries 19-21, 47-49, 54, 55, 58 and 62 are never assigned and keep
        // the constructor's 0.
        for (i, p) in P0 {
            self.entries[i].priority[0] = p;
        }
        for (i, p) in P1 {
            self.entries[i].priority[1] = p;
        }
    }

    /// `SetDefaults`: idle on each layer, the body skin, and the first
    /// fidget time (20 to 40 s away).
    pub fn set_defaults(&mut self, sc: &AnimScene<M>, rand: &mut Rand) {
        let e0 = self.entries[0];
        self.cur[0].anim = 0;
        self.cur[0].play = self.h.add_play(e0.animation[0] as usize, e0.sync, e0.weight, sc);
        if self.skel_type == 1 {
            self.cur[1].anim = 0;
            self.cur[1].play = self.h.add_play(e0.animation[1] as usize, e0.sync, e0.weight, sc);
        }
        self.h.attach_skin(self.body);
        self.anim_time = sc.time + (rand.rand() % 20000) as i64 + 20000;
    }

    /// `SetLocomotion`.
    /// `SetDeath(on, erase)`: dead, the death locomotion (14); with `erase`
    /// every play (current and previous, both layers) dropped first, its
    /// trigger sound stopped, and the death animation put at its last frame.
    pub fn set_death(&mut self, on: bool, erase: bool, sc: &AnimScene<M>, rand: &mut Rand, mut sound: Option<&mut dyn SoundSink>) {
        self.dead = on;
        if !on {
            return;
        }
        if erase {
            for i in 0..2 {
                for l in [&mut self.cur[i], &mut self.prev[i]] {
                    if l.play != -1 {
                        self.h.erase_play(l.play);
                        if let Some(sd) = sound.as_deref_mut()
                            && l.flags != -1
                        {
                            sd.stop(l.flags);
                            l.flags = -1;
                        }
                        l.play = -1;
                        l.anim = 0;
                    }
                }
            }
        }
        self.set_locomotion(14, sc, rand);
        if erase {
            for i in 0..2 {
                let p = self.cur[i].play;
                if p == -1 {
                    continue;
                }
                let nf = sc.anims[self.h.plays.get(p).anim].num_frames;
                if nf > 0 {
                    self.h.set_play_frame(p, F::from_int(nf - 1), sc);
                }
            }
        }
    }

    pub fn set_locomotion(&mut self, id: i32, sc: &AnimScene<M>, rand: &mut Rand) {
        if self.skel_type == 1 {
            if self.set_animation(id, 0, sc, rand) != 0 {
                self.action = self.action_seq;
            }
            self.set_animation(id, 1, sc, rand);
        } else if self.set_animation(id, 0, sc, rand) != 0 {
            self.action = self.action_seq;
        }
        self.action_seq += 1;
    }

    /// `SetAction`: `SetLocomotion`'s body, handing back the action's
    /// handle (for `IsActionComplete`). `SetAnimation` cannot fail here (no
    /// play pool runs out), so the decomp's early -1 never arises.
    pub fn set_action(&mut self, id: i32, sc: &AnimScene<M>, rand: &mut Rand) -> i32 {
        let handle = self.action_seq;
        self.set_locomotion(id, sc, rand);
        handle
    }

    pub fn is_action_complete(&self, i: i32) -> bool {
        if self.action != i {
            return true;
        }
        self.cur[0].play != -1 && self.h.is_playback_stopped(self.cur[0].play)
    }

    fn prio(&self, id: i32, layer: usize) -> i32 {
        self.entries[id.clamp(0, 65) as usize].priority[layer]
    }

    /// `SetAnimation(id, layer)`: 1 when a new play started, 0 when the
    /// request was refused or already playing.
    pub fn set_animation(&mut self, id: i32, layer: usize, sc: &AnimScene<M>, rand: &mut Rand) -> i32 {
        let mut id = id;
        if self.anim_time < sc.time {
            if id == 0 {
                id = 15;
            }
            if self.skel_type != 1 || layer == 1 {
                self.anim_time = sc.time + (rand.rand() % 20000) as i64 + 20000;
            }
        }
        if sc.time > 5000 && id == 0 {
            id = if sc.time - self.idle_time > 4999 { 0 } else { 17 };
        }
        let watch = if self.skel_type == 1 { Some(1) } else if layer == 0 { Some(0) } else { None };
        if let Some(w) = watch
            && in_range(id, 22, 32)
            && id != 34
        {
            let a = self.cur[w].anim;
            if in_range(a, 12, 2) || a == 44 {
                id = 44;
            }
        }
        if self.dead {
            id = 14;
        }

        // 0: from `check`, 1: from `special`, 2: on to `rank`.
        let mut route = 0;
        if self.cur[layer].play != -1 && self.prev[layer].play != -1 {
            let mut erase = true;
            if !self.h.is_blend_complete(self.cur[layer].play, sc) {
                let cur = self.cur[layer].anim;
                if self.prio(id, layer) < self.prio(cur, layer) || id == cur {
                    erase = false;
                } else if cur == 15 {
                    erase = false;
                    route = 1;
                } else if id == 0 {
                    erase = false;
                } else {
                    self.h.freeze_playback(self.cur[layer].play, sc);
                }
            }
            if erase {
                self.h.erase_play(self.prev[layer].play);
                self.prev[layer].play = -1;
            }
        }
        if route == 0 {
            // check:
            if self.cur[layer].anim == 15 {
                route = 1;
            } else {
                if self.cur[layer].sync == 1
                    && !self.h.is_playback_stopped(self.cur[layer].play)
                    && self.prio(id, layer) < self.prio(self.cur[layer].anim, layer)
                {
                    return 0;
                }
                route = 2;
            }
        }
        if route == 1 {
            // special:
            if id == 0 && self.cur[layer].sync == 1 && !self.h.is_playback_stopped(self.cur[layer].play) {
                return 0;
            }
        }
        // rank:
        if self.prio(id, layer) < self.prio(self.cur[layer].anim, layer)
            && (self.cur[layer].sync != 1 || !self.h.is_playback_stopped(self.cur[layer].play))
            && (id != 15 || self.cur[layer].anim != 0)
            && (layer == 0 || (id != 0 && id != 17) || self.cur[layer].anim >= 14)
        {
            return 0;
        }
        if (in_range(id, 22, 22) || in_range(id, 50, 4)) && !in_range(id, 37, 3) && !in_range(id, 31, 2) && id != 35 && id != 33 {
            self.idle_time = sc.time;
        }
        if id == self.cur[layer].anim && self.cur[layer].play != -1 {
            return 0;
        }
        if self.prev[layer].play == -1 {
            self.prev[layer] = self.cur[layer];
            let e = self.entries[id.clamp(0, 65) as usize];
            let a_idx = if self.skel_type == 1 && layer == 0 && self.cur[1].anim == id {
                e.animation[0]
            } else {
                e.animation[layer]
            };
            if a_idx >= 0 && (a_idx as usize) < sc.anims.len() {
                if self.cur[layer].play != -1 {
                    self.h.stop_playback(self.cur[layer].play, sc);
                    if self.skel_type == 1 && layer == 0 && self.cur[1].anim == id {
                        self.cur[0].anim = id;
                        self.cur[0].sync = e.sync;
                        self.cur[0].play = self.h.add_interpolated_play_with_sync(a_idx as usize, self.cur[1].play, 250, sc);
                    } else {
                        self.cur[layer].anim = id;
                        self.cur[layer].sync = e.sync;
                        self.cur[layer].play = self.h.add_interpolated_play(a_idx as usize, e.sync, e.weight, 250, sc);
                    }
                } else {
                    self.cur[layer].anim = id;
                    self.cur[layer].sync = e.sync;
                    self.cur[layer].play = self.h.add_play(a_idx as usize, e.sync, e.weight, sc);
                }
            } else {
                if self.cur[layer].play != -1 {
                    self.h.stop_playback(self.cur[layer].play, sc);
                }
                self.cur[layer].anim = id;
                self.cur[layer].sync = e.sync;
                self.cur[layer].play = -1;
            }
        }
        // The layer's sound and particle triggers are re-armed.
        let names: [&str; 4] = match (self.skel_type == 1, layer) {
            (false, _) => ["Primary Sound", "Secondary Sound", "Particle 1", "Particle 2"],
            (true, 0) => ["Primary Sound_BPU", "Secondary Sound_BPU", "Particle 1_BPU", "Particle 2_BPU"],
            _ => ["Primary Sound_BPL", "Secondary Sound_BPL", "Particle 1_BPL", "Particle 2_BPL"],
        };
        for n in names {
            if let Some(i) = self.h.find_trigger(hash_ref_id(n)) {
                self.h.reset_trigger(i);
            }
        }
        // layer 0: SetItemAction(0) -- items are not ported.
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpu::Ee;

    /// `AttachItem` attaches only to a sprite whose `m_flags & 1` is set, and
    /// only where the slot names a node: a sprite with the flag clear holds
    /// nothing however many nodes it names.
    #[test]
    fn attach_item_needs_the_flag_and_a_node() {
        let mut cs = CSprite::<Ee>::new(HSprite::new(0), 2, 9, 0, F::ONE, false);
        cs.item_nodes[..2].copy_from_slice(&[6, 10]);
        assert_eq!((cs.item_node(0), cs.item_node(1), cs.item_node(2)), (None, None, None), "the flag clear: nothing");
        cs.holds_items = true;
        assert_eq!((cs.item_node(0), cs.item_node(1), cs.item_node(2)), (Some(6), Some(10), None), "the flag set: each slot's node");
        assert_eq!(cs.item_node(6), None, "past the slots");
    }
}
