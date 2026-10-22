//! A character in the pregame's 3D: what character select's `ShowCharacter`
//! and the creation pages' `CreateModel` put in the scene. The model's
//! CSprite by its DictID (out of CHARSEL1-4.CSF, which CLIENT's pregame
//! load reads; `HumanMale` when the id is not there, as `ShowCharacter`
//! falls back), dressed by `vi::cust::apply_look` (`SetArmorSlot` x6,
//! `SetHair`, `SetFace`, `SetRobe`), its items on their nodes
//! (`AttachItem`), posed once a frame of the game's (`VIHSprite::Process`)
//! at an actor's place and heading, on the GPU.

use crate::play::Gpu;
use esf::Library;
use render::GpuModel;
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;

pub struct Figure {
    /// the model's DictID, as asked for
    pub model: u32,
    pub look: vi::cust::Look,
    pub sprite: Option<CSprite<Ee>>,
    pub anims: AnimScene<Ee>,
    pub rand: Rand,
    pub gpu: GpuModel,
    /// items (and the helm) on their nodes
    pub attached: Vec<(i32, GpuModel)>,
    /// the actor's place and heading, pitch and roll (`CreateActor(at, hpr, 1.0)`)
    pub at: [f32; 3],
    pub hpr: [f32; 3],
}

impl Figure {
    /// `CreateActor` with the model's sprite (or `HumanMale`'s), dressed in
    /// `look` (a helm on node 8 when its set has one), `items` (DictIDs, 0
    /// none) in the item slots 0..3. None when neither model is there.
    pub fn new(lib: &Library, g: &mut Gpu, model: u32, look: vi::cust::Look, items: [u32; 3], at: [f32; 3], hpr: [f32; 3], now: i64) -> Option<Figure> {
        let r = lib.lookup(model as i32).or_else(|| lib.lookup(vi::hash::hash_resource_id("HumanMale") as i32))?;
        let textures = render::library_textures(lib);
        let mut built = lib.build_model(r)?;
        let mut anims = AnimScene { time: now, ..Default::default() };
        let mut rand = Rand(1);
        let sprite = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand);
        let mut attached = Vec::new();
        let mut helm_worn = false;
        if let Some(cs) = &sprite {
            for (i, &id) in items.iter().enumerate() {
                // `AttachItem`'s test: a node in the slot, on a sprite that
                // holds items at all (`CSprite::item_node`)
                let Some(node) = cs.item_node(i).filter(|_| id != 0) else { continue };
                // `AttachItem`: the item, or `!Item` when it is not there
                let m = lib.lookup(id as i32).or_else(|| lib.lookup(vi::hash::hash_resource_id("!Item") as i32)).and_then(|r| lib.build_model(r));
                if let Some(m) = m {
                    attached.push((node, g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
                }
            }
            if cs.node_ids[8] >= 0
                && let Some(h) = vi::cust::helm(look.armor[5])
                && let Some(mut m) = lib.lookup(h as i32).and_then(|r| lib.build_model(r))
            {
                let tint = vi::cust::TINTS[(look.armor_tint[5] as usize).min(14)];
                for b in &mut m.batches {
                    for l in &mut b.layers {
                        l.color = tint;
                    }
                }
                helm_worn = true;
                attached.push((cs.node_ids[8], g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
            }
            for ch in vi::cust::apply_look(cs, &look, helm_worn, |id| lib.surface(id as i32).is_some()) {
                for b in built.batches.iter_mut().filter(|b| b.material == ch.entry) {
                    if let Some(l) = b.layers.get_mut(ch.layer as usize) {
                        if let Some(t) = ch.texture {
                            l.texture = t as i32;
                        }
                        if let Some(c) = ch.color {
                            l.color = c;
                        }
                    }
                }
            }
        }
        let gpu = g.renderer.upload(g.device, g.queue, &built, &[render::IDENTITY], &textures);
        Some(Figure { model, look, sprite, anims, rand, gpu, attached, at, hpr })
    }

    /// `VICSprite::SetAction(id)` (23 the select screen's flourish, 0 back
    /// to standing).
    pub fn set_action(&mut self, id: i32) {
        if let Some(cs) = self.sprite.as_mut() {
            cs.set_action(id, &self.anims, &mut self.rand);
        }
    }

    /// The actor's matrix (`CreateActor`'s place, heading and scale 1).
    pub fn actor(&self) -> Matrix44<Ee> {
        let f = F::<Ee>::new;
        Matrix44::transform(&Vect3::new(f(self.hpr[0]), f(self.hpr[1]), f(self.hpr[2])), F::ONE, &Vect3::new(f(self.at[0]), f(self.at[1]), f(self.at[2])))
    }

    /// One frame at `now` (the scene's clock, ms): the sprite processed, its
    /// skin palette and the attached items' places to the GPU.
    pub fn pose(&mut self, g: &Gpu, now: i64) {
        let actor = self.actor();
        let (matrix, palette) = match self.sprite.as_mut() {
            Some(cs) => {
                self.anims.time = now;
                cs.h.process(0, &self.anims);
                let m = cs.h.calc_trans_about_center(&actor, &self.anims);
                cs.h.set_blend_matrices();
                for (node, gpu) in &self.attached {
                    if let Some(n) = cs.h.nodes.get(*node as usize) {
                        g.renderer.set_instances(g.queue, gpu, &[Matrix44::mul(&n.world, &m).to_cols()]);
                    }
                }
                (m.to_cols(), cs.h.blend.iter().map(|m| m.to_cols()).collect::<Vec<_>>())
            }
            None => (actor.to_cols(), Vec::new()),
        };
        g.renderer.set_instances(g.queue, &self.gpu, &[matrix]);
        if !palette.is_empty() {
            g.renderer.set_palette(g.queue, &self.gpu, &palette);
        }
    }

    /// What to draw: the body and what it carries.
    pub fn models(&self) -> impl Iterator<Item = &GpuModel> {
        std::iter::once(&self.gpu).chain(self.attached.iter().map(|(_, m)| m))
    }
}
