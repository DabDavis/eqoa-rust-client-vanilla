//! The spell effects against the decomp, call for call: eqoa-port's
//! `build/oracle_spell` runs SUPPORT's spell-effect, particle and lightning
//! TUs over a fake world (scene, character sprites, camera effect, recording
//! raster) with newlib's rand(), and `vi::spell` runs here under `Ieee` over a
//! [`Fake`] that keeps the same rules. Every call the effects make on the
//! world (actions, triggers, created actors and where they are put, sounds,
//! camera shakes, vibration), every lightning strip vertex and every particle
//! billboard is compared, frame by frame, bit for bit.
//!
//! The game's own effects drive it: all 238 of SPELLFX.CSF's 0xC200 records
//! (first a set covering every event type the disc uses), played by and on CSprites and a
//! simple sprite, with area targets, stopped, outlived by their actors and by
//! their casters. Negative controls: a planted record, an effect's wait made longer
//! on our side only, and one rule of the fake world changed on
//! our side only.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port) or the disc.

mod common;
use common::Oracle;
use std::collections::HashMap;
use vi::csprite::Rand;
use vi::fpu::{F, Ieee};
use vi::geom::{Matrix44, Vect4};
use vi::particle::{Color, Definition, System};
use vi::spell::{SpellEffect, SpellEvent, SpellScene, SpellSystem};
use vi::vect::Vect3;

type M = Ieee;
type Rec = (u32, Vec<u32>);
/// id, sprite type, where, yaw, the hands' projectile sprites, triggers
type Placed = (i32, i32, [f32; 3], f32, [i32; 2], i32);

#[derive(Clone, Copy, PartialEq)]
enum Mutation {
    None,
    Planted,
    Effect,
    Rule,
}

#[derive(Clone)]
struct Actor {
    ty: i32,
    m: Matrix44<M>,
    proj: [i32; 2],
    triggers: i32,
    seq: i32,
    last: i32,
    action_time: i64,
    reset: [i64; 4],
}

/// The oracle's fake world, rule for rule (see `oracle_spell.cpp`).
struct Fake {
    actors: HashMap<i32, Actor>,
    sprite_type: HashMap<i32, i32>,
    next_actor: i32,
    next_sprite: i32,
    time: i64,
    rec: Vec<Rec>,
    trip: i64,
}

fn fb(f: F<M>) -> u32 {
    f.0.to_bits()
}

fn vb(v: &Vect3<M>) -> [u32; 3] {
    [fb(v.x), fb(v.y), fb(v.z)]
}

fn cb(c: &Color<M>) -> [u32; 4] {
    [fb(c.r), fb(c.g), fb(c.b), fb(c.a)]
}

impl Fake {
    fn put(&mut self, k: u32, w: &[u32]) {
        self.rec.push((k, w.to_vec()));
    }
    fn act(&mut self, a: i32) -> i32 {
        let t = self.time;
        let x = self.actors.get_mut(&a).unwrap();
        let h = x.seq;
        x.seq += 1;
        x.last = h;
        x.action_time = t;
        h
    }
}

impl SpellScene<M> for Fake {
    fn time(&self) -> i64 {
        self.time
    }
    fn sprite_type(&self, actor: i32) -> Option<i32> {
        self.actors.get(&actor).map(|a| a.ty).filter(|&t| t != 0)
    }
    fn actor_transform(&self, actor: i32) -> Matrix44<M> {
        self.actors[&actor].m
    }
    fn actor_location(&self, actor: i32) -> Vect3<M> {
        self.actors[&actor].m.m[3].xyz()
    }
    fn slot_world_transform(&self, actor: i32, node: i32) -> Matrix44<M> {
        let mut m = self.actors[&actor].m;
        m.m[3].x += F::new(0.1);
        m.m[3].y += F::new(0.25) * F::new(node as f32);
        m
    }
    fn set_action(&mut self, actor: i32, id: i32) -> i32 {
        let h = self.act(actor);
        self.put(1, &[actor as u32, id as u32, h as u32]);
        h
    }
    fn set_attack_action(&mut self, actor: i32, slot: i32, alt: bool) -> i32 {
        let h = self.act(actor);
        self.put(2, &[actor as u32, slot as u32, alt as u32, h as u32]);
        h
    }
    fn set_weapon_trail(&mut self, actor: i32, id: i32, color: Color<M>) {
        let mut w = vec![actor as u32, id as u32];
        w.extend(cb(&color));
        self.put(3, &w);
    }
    fn find_trigger(&self, actor: i32, hash: u32) -> Option<i32> {
        let n = self.actors.get(&actor)?.triggers.min(4);
        (0..n).find(|&k| hash == vi::hash::hash_resource_id(&format!("Spell FX {}", k + 1)))
    }
    fn reset_trigger(&mut self, actor: i32, i: i32) {
        let t = self.time;
        self.actors.get_mut(&actor).unwrap().reset[i as usize] = t;
        self.put(4, &[actor as u32, i as u32]);
    }
    fn has_tripped(&self, actor: i32, i: i32) -> bool {
        (self.time - self.actors[&actor].reset[i as usize]) as i32 >= self.trip as i32 * (i + 1)
    }
    fn is_action_complete(&self, actor: i32, action: i32) -> bool {
        let x = &self.actors[&actor];
        action != x.last || (self.time - x.action_time) as i32 >= 700
    }
    fn item_projectile(&self, actor: i32, slot: usize) -> i32 {
        self.actors[&actor].proj[slot]
    }
    fn create_actor(&mut self, at: Vect3<M>, hpr: Vect3<M>, scale: F<M>, sprite: i32) -> i32 {
        let id = self.next_actor;
        self.next_actor += 1;
        let ty = if (0..1024).contains(&sprite) { self.sprite_type.get(&sprite).copied().unwrap_or(0) } else { 0 };
        let mut m = Matrix44::identity();
        m.m[3] = Vect4::new(at.x, at.y, at.z, F::ONE);
        self.actors.insert(id, Actor { ty, m, proj: [0, 0], triggers: 0, seq: 0, last: 0, action_time: 0, reset: [0; 4] });
        let mut w = vec![id as u32];
        w.extend(vb(&at));
        w.extend(vb(&hpr));
        w.push(fb(scale));
        w.push(sprite as u32);
        self.put(5, &w);
        id
    }
    fn release_actor(&mut self, actor: i32) {
        self.put(6, &[actor as u32]);
    }
    fn set_actor_collideable(&mut self, actor: i32, on: bool) {
        self.put(7, &[actor as u32, on as u32]);
    }
    fn set_actor_location(&mut self, actor: i32, at: Vect3<M>) {
        let mut w = vec![actor as u32];
        w.extend(vb(&at));
        self.put(8, &w);
    }
    fn set_actor_hpr(&mut self, actor: i32, hpr: Vect3<M>) {
        let mut w = vec![actor as u32];
        w.extend(vb(&hpr));
        self.put(9, &w);
    }
    fn set_actor_transform(&mut self, actor: i32, hpr: Vect3<M>, scale: F<M>, at: Vect3<M>) {
        let mut w = vec![actor as u32];
        w.extend(vb(&hpr));
        w.push(fb(scale));
        w.extend(vb(&at));
        self.put(10, &w);
    }
    fn sprite_kind(&self, sprite: i32) -> i32 {
        self.sprite_type.get(&sprite).copied().unwrap_or(0)
    }
    fn copy_sprite(&mut self, sprite: i32) -> i32 {
        let k = self.next_sprite;
        self.next_sprite += 1;
        let t = self.sprite_kind(sprite);
        self.sprite_type.insert(k, t);
        self.put(11, &[sprite as u32, k as u32]);
        k
    }
    fn restart_hsprite(&mut self, sprite: i32, mode: i32) {
        self.put(12, &[sprite as u32, mode as u32]);
    }
    fn sound_settings(&mut self, actor: i32, pan: bool, attenuation: i32) {
        self.put(13, &[actor as u32, pan as u32, attenuation as u32]);
    }
    fn fade_begin(&mut self, _actor: i32) -> Option<u8> {
        None
    }
    fn fade_set(&mut self, _actor: i32, _alpha: u8) {}
    fn shake(&mut self, speed: i32, amp: F<M>) {
        self.put(14, &[speed as u32, fb(amp)]);
    }
    fn shake_terp(&mut self, mag: F<M>, time: F<M>) {
        self.put(15, &[fb(mag), fb(time)]);
    }
    fn camera_reset(&mut self) {
        self.put(16, &[]);
    }
    fn vibrate(&mut self, power: u8, time: u8, mode: i32) {
        self.put(17, &[power as u32, time as u32, mode as u32]);
    }
}

/// Words after each record kind (`oracle_spell.cpp`'s table).
fn words(k: u32) -> usize {
    match k {
        1 => 3,
        2 => 4,
        3 => 6,
        4 => 2,
        5 => 9,
        6 => 1,
        7 => 2,
        8 | 9 => 4,
        10 => 8,
        11..=12 => 2,
        13 => 3,
        14 | 15 => 2,
        16 => 0,
        17 => 3,
        18 => 4,
        19 | 21 => 0,
        20 => 5,
        22 => 10,
        _ => panic!("record kind {k}"),
    }
}

fn records(b: &[u8]) -> Vec<Rec> {
    let w: Vec<u32> = b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < w.len() {
        let k = w[i];
        let n = words(k);
        out.push((k, w[i + 1..i + 1 + n].to_vec()));
        i += 1 + n;
    }
    out
}

/// Records split three ways: the world's, the lightning's, the particles'.
fn split(r: Vec<Rec>) -> [Vec<Rec>; 3] {
    let mut s: [Vec<Rec>; 3] = Default::default();
    for x in r {
        let k = match x.0 {
            1..=17 => 0,
            18..=21 => 1,
            _ => 2,
        };
        s[k].push(x);
    }
    s
}

fn event_bytes(ev: &SpellEvent<M>) -> Vec<u8> {
    let mut p = Vec::new();
    for v in [ev.ty, ev.p04, ev.id, ev.p0c, ev.p10] {
        p.extend(v.to_le_bytes());
    }
    for f in ev.f {
        p.extend(f.0.to_le_bytes());
    }
    p
}

fn i32_at(b: &[u8]) -> i32 {
    i32::from_le_bytes(b[..4].try_into().unwrap())
}

const PROJECTILE: i32 = 900;

fn matrix(at: [f32; 3], yaw: f32) -> Matrix44<M> {
    let (s, c) = yaw.sin_cos();
    let mut m = Matrix44::<M>::identity();
    m.m[0] = Vect4::new(F::new(c), F::ZERO, F::new(-s), F::ZERO);
    m.m[2] = Vect4::new(F::new(s), F::ZERO, F::new(c), F::ZERO);
    m.m[3] = Vect4::new(F::new(at[0]), F::new(at[1]), F::new(at[2]), F::ONE);
    m
}

/// Returns (frames, world records, strip records, billboards) compared, and
/// how many world records of each kind.
fn scenario(mutation: Mutation) -> Option<([usize; 4], std::collections::BTreeMap<u32, usize>)> {
    let mut o = Oracle::open_at("oracle_spell")?;
    let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let Ok(mut disc) = iso::Iso::open(&iso) else {
        eprintln!("SKIP: no disc");
        return None;
    };
    let fx = esf::Container::open(disc.read("DATA2/SPELLFX.CSF").unwrap()).unwrap();

    let t0 = 1000i64;
    let mut ps = System::<M>::new(t0, 2000);
    let mut spells = SpellSystem::<M>::default();
    let mut rand = Rand(31337);
    let mut fake = Fake { actors: HashMap::new(), sprite_type: HashMap::new(), next_actor: 100, next_sprite: 500, time: t0, rec: Vec::new(), trip: if mutation == Mutation::Rule { 151 } else { 150 } };
    let mut p = 2000i32.to_le_bytes().to_vec();
    p.extend(t0.to_le_bytes());
    o.call(1200, &p);
    o.call(1201, &31337u32.to_le_bytes());

    // the world: a CSprite caster with a projectile in its off hand, a CSprite
    // target, two area targets (a CSprite and a simple sprite), a simple caster
    let cast: [Placed; 5] = [(1, 5, [10.0, 0.0, 5.0], 0.3, [-1, PROJECTILE], 4), (2, 5, [22.0, 1.0, -4.0], 2.0, [-1, -1], 2), (3, 5, [25.0, 0.0, 3.0], 1.0, [PROJECTILE, -1], 1), (4, 1, [18.0, 0.5, -9.0], 0.0, [-1, -1], 0), (5, 1, [4.0, 2.0, 12.0], -1.2, [-1, -1], 0)];
    for &(id, ty, at, yaw, proj, triggers) in &cast {
        let m = matrix(at, yaw);
        let mut p = vec![];
        for v in [id, ty] {
            p.extend(v.to_le_bytes());
        }
        for r in &m.m {
            for f in [r.x, r.y, r.z, r.w] {
                p.extend(f.0.to_le_bytes());
            }
        }
        for v in [proj[0], proj[1], triggers] {
            p.extend(v.to_le_bytes());
        }
        o.call(1203, &p);
        fake.actors.insert(id, Actor { ty, m, proj, triggers, seq: 0, last: -1, action_time: 0, reset: [0; 4] });
    }
    let sprite = |o: &mut Oracle, fake: &mut Fake, id: i32, ty: i32| {
        let mut p = id.to_le_bytes().to_vec();
        p.extend(ty.to_le_bytes());
        o.call(1204, &p);
        fake.sprite_type.insert(id, ty);
    };
    sprite(&mut o, &mut fake, PROJECTILE, 1);
    o.call(1210, &[1i32.to_le_bytes(), 1i32.to_le_bytes(), 1i32.to_le_bytes()].concat());
    spells.player_actor = 1;
    spells.vibrate = true;
    spells.camera = true;

    // the effects: parsed with their particle definitions and sprites numbered
    // as they are met
    let mut defs: HashMap<u32, usize> = HashMap::new();
    let mut sprites: HashMap<u32, i32> = HashMap::new();
    let mut effects = Vec::new();
    for n in (0..fx.nodes.len()).filter(|&i| fx.nodes[i].ty == 0xC200) {
        let mut want_defs = Vec::new();
        let mut want_sprites = Vec::new();
        let e = SpellEffect::<M>::parse(&fx, n, &mut |k, id| {
            let Some(node) = fx.lookup(id as i32) else { return -1 };
            if k == 3 {
                let next = defs.len();
                *defs.entry(id).or_insert_with(|| {
                    want_defs.push(node);
                    next
                }) as i32
            } else {
                let next = 10 + sprites.len() as i32;
                *sprites.entry(id).or_insert_with(|| {
                    want_sprites.push((next, fx.node(node).ty));
                    next
                })
            }
        });
        for node in want_defs {
            let d = fx.child(node, 0xC020).unwrap();
            let (v, body) = (fx.node(d).version as i32, fx.body(d));
            let mut p = v.to_le_bytes().to_vec();
            p.extend(body);
            let k = i32_at(&o.call(1202, &p));
            assert_eq!(k as usize, ps.add_definition(Definition::parse(body, v).unwrap()));
        }
        for (id, ty) in want_sprites {
            sprite(&mut o, &mut fake, id, match ty {
                0xB100 => 9,
                0x2200 => 4,
                _ => 1,
            });
        }
        effects.push(e.unwrap());
    }
    assert_eq!(effects.len(), 238);
    // a set covering every event type the disc uses first, then all the rest
    let mut chosen: Vec<usize> = Vec::new();
    for ty in 1..=17 {
        if let Some(k) = effects.iter().position(|e| e.lists.iter().flatten().any(|ev| ev.ty == ty))
            && !chosen.contains(&k)
        {
            chosen.push(k);
        }
    }
    for k in 0..effects.len() {
        if !chosen.contains(&k) {
            chosen.push(k);
        }
    }
    for e in &effects {
        let mut p = Vec::new();
        for l in &e.lists {
            p.extend((l.len() as i32).to_le_bytes());
            for ev in l {
                p.extend(event_bytes(ev));
            }
        }
        let k = i32_at(&o.call(1205, &p));
        assert_eq!(k as usize, spells.effects.len());
        spells.effects.push(e.clone());
    }
    if mutation == Mutation::Effect {
        // one wait twice as long (and past a frame step), ours only
        let k = chosen[0];
        let ev = spells.effects[k].lists.iter_mut().flatten().find(|ev| ev.ty == 1 && ev.p04 == 1).unwrap();
        ev.id = ev.id * 2 + 120;
    }

    let mut t = t0;
    let mut n = [0usize; 4];
    let mut kinds = std::collections::BTreeMap::new();
    let plays: [(i32, i32, &[i32]); 4] = [(1, 2, &[3, 4]), (5, 3, &[]), (1, 4, &[2]), (2, 1, &[3, 5])];
    for (b, batch) in chosen.chunks(3).enumerate() {
        let mut handles = Vec::new();
        for (j, &k) in batch.iter().enumerate() {
            let (caster, target, sec) = plays[(b + j) % 4];
            let mut p = Vec::new();
            for v in [k as i32, 900 + 100 * j as i32, caster, target, sec.len() as i32] {
                p.extend(v.to_le_bytes());
            }
            for s in sec {
                p.extend(s.to_le_bytes());
            }
            let h = i32_at(&o.call(1206, &p));
            assert_eq!(h, spells.play(k, 900 + 100 * j as i32, caster, target, sec, &mut fake), "batch {b}: play {k}'s handle");
            handles.push(h);
        }
        for frame in 0..200 {
            t += match frame {
                90 => 400,
                // long frames, at the 0.1 s cap, so a projectile overshoots
                f if f % 25 == 12 => 100 + (f % 4) as i64,
                f => 16 + ((f * 7 + b * 3) % 37) as i64,
            };
            fake.time = t;
            let r = o.call(1207, &t.to_le_bytes());
            spells.process(&mut fake, &mut ps, &mut rand);
            let mut ours = std::mem::take(&mut fake.rec);
            for d in &spells.bolts {
                ours.push((18, cb(&d.color).to_vec()));
                for s in &d.strips {
                    ours.push((19, vec![]));
                    for (uv, v) in s {
                        let mut w = vec![fb(uv[0]), fb(uv[1])];
                        w.extend(vb(v));
                        ours.push((20, w));
                    }
                    ours.push((21, vec![]));
                }
            }
            for (_, bbs) in ps.render(t) {
                for bb in bbs {
                    let mut w = vb(&bb.location).to_vec();
                    w.extend([bb.frame as u32, fb(bb.width), fb(bb.height)]);
                    w.extend(cb(&bb.color));
                    ours.push((22, w));
                }
            }
            if mutation == Mutation::Planted && b == 1 && frame == 40 && !ours.is_empty() {
                let last = ours.len() - 1;
                if let Some(w) = ours[last].1.first_mut() {
                    *w ^= 1;
                } else {
                    ours[last].0 = 99;
                }
            }
            let want = split(records(&r));
            let got = split(ours);
            for s in 0..3 {
                for (i, (a, g)) in want[s].iter().zip(&got[s]).enumerate() {
                    assert_eq!(a, g, "batch {b} frame {frame}: stream {s} record {i}");
                }
                assert_eq!(want[s].len(), got[s].len(), "batch {b} frame {frame}: stream {s}'s length");
                n[s + 1] += want[s].len();
            }
            for r in &want[0] {
                *kinds.entry(r.0).or_default() += 1;
            }
            n[0] += 1;
            // mid-batch: an actor goes, a playback is stopped, a caster's are
            if frame == 60 && b % 3 == 1 {
                let r = o.call(1209, &4i32.to_le_bytes());
                spells.notify_actor_destroyed(4, &mut fake);
                assert_eq!(split(records(&r))[0], split(std::mem::take(&mut fake.rec))[0], "batch {b}: destroyed");
            }
            if frame == 120 && b % 2 == 0 {
                let r = o.call(1208, &handles[0].to_le_bytes());
                spells.stop(handles[0], &mut fake, &mut ps);
                assert_eq!(records(&r), std::mem::take(&mut fake.rec), "batch {b}: stop");
            }
            if frame == 150 && b % 4 == 3 {
                let r = o.call(1211, &1i32.to_le_bytes());
                spells.stop_if_this_was_caster(1, &mut fake, &mut ps);
                assert_eq!(records(&r), std::mem::take(&mut fake.rec), "batch {b}: stop if caster");
            }
        }
        for &h in &handles {
            let r = o.call(1208, &h.to_le_bytes());
            spells.stop(h, &mut fake, &mut ps);
            assert_eq!(records(&r), std::mem::take(&mut fake.rec), "batch {b}: final stop");
        }
    }
    Some((n, kinds))
}

#[test]
fn spell_effects_match_the_decomp() {
    let Some((n, kinds)) = scenario(Mutation::None) else { return };
    eprintln!("{} frames: {} world calls, {} strip records, {} billboards equal; world calls by kind {kinds:?}", n[0], n[1], n[2], n[3]);
    assert!(n[1] > 1000 && n[2] > 1000 && n[3] > 1000, "every stream is exercised: {n:?}");
    // actions, attacks, trails, triggers, actors made, moved and released,
    // sounds, shakes, vibration
    for k in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 13, 14, 15, 17] {
        assert!(kinds.contains_key(&k), "world call kind {k} exercised: {kinds:?}");
    }
}

fn must_fail(m: Mutation) {
    match std::panic::catch_unwind(|| scenario(m)) {
        Ok(None) => {}
        Ok(Some(_)) => panic!("a mutation passed"),
        Err(_) => {}
    }
}

#[test]
fn a_planted_record_fails() {
    must_fail(Mutation::Planted);
}

#[test]
fn an_effect_with_a_longer_wait_fails() {
    must_fail(Mutation::Effect);
}

#[test]
fn a_changed_world_rule_fails() {
    must_fail(Mutation::Rule);
}
