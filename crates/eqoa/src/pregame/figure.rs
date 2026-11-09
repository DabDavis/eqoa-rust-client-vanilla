//! A character in the pregame's 3D: what character select's `ShowCharacter`
//! and the creation pages' `CreateModel` put in the scene. The model's
//! CSprite by its DictID (out of CHARSEL1-4.CSF, which CLIENT's pregame
//! load reads, or a server's race's out of CHAR.ESF, `res::Res::want_bodies`;
//! until that is read its `like` race's, else `HumanMale` when the id is not
//! there, as `ShowCharacter` falls back), dressed by `vi::cust::apply_look` (`SetArmorSlot` x6,
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
    /// a server's race's look on its parts (`raceopts`), drawn when the body
    /// is the one it was chosen on
    pub race_look: Option<net::raceopts::Look>,
    /// the actor's scale: 1, or a body a race's options offer at its size
    /// against the race's own (`raceopts::body_size`), as the world draws it
    pub sprite: Option<CSprite<Ee>>,
    pub anims: AnimScene<Ee>,
    pub rand: Rand,
    pub gpu: GpuModel,
    /// items (and the helm) on their nodes
    pub attached: Vec<(i32, GpuModel)>,
    pub scale: f32,
    /// the items asked for (DictIDs, 0 none), in the item slots 0..3
    pub items: [u32; 3],
    /// the actor's place and heading, pitch and roll (`CreateActor(at, hpr, 1.0)`)
    pub at: [f32; 3],
    pub hpr: [f32; 3],
    /// the centre transform `pose` last placed it under
    centre: Matrix44<Ee>,
    /// the particles of the items in its hands, made at the first frame
    held: Option<crate::particles::Held>,
    /// their billboards this frame (`particles_frame`)
    pub fx: Vec<render::billboard::Batch>,
    /// the body it was built of: the model, or what stood in for it
    body: u32,
    /// the cape it wears (`cape`, `wear_cape`) and its model, none where no
    /// art or body fits; None while it wears none
    cape: Option<(net::cape::Cape, Option<GpuModel>)>,
}

/// `CreateActor`'s sprite: a copy of the parsed one (`VICSprite::Copy`,
/// every CSprite's instance type being 1). The copy keeps the plays
/// `SetDefaults` started and the layers' entries, but not its timers: its
/// `m_animTime` and `m_idleTime` are the constructor's 0, so its first
/// `SetAction(0)` plays the special idle (15) at once, the game's fidget, and
/// puts the next one 20 to 40 s on. The parsed sprite as it is would only
/// breathe until something else was asked of it: an unarmed figure at
/// character select stood still until its first flourish, five seconds in
/// (F3 20261023-142741); an armed one swings at once (`ShowCharacter`).
pub fn actor_sprite(lib: &Library, r: esf::Ref, anims: &mut AnimScene<Ee>, rand: &mut Rand) -> Option<CSprite<Ee>> {
    let mut cs = load_csprite(lib, r, anims, &mut AnimCache::new(), rand)?;
    cs.anim_time = 0;
    cs.idle_time = 0;
    Some(cs)
}

thread_local! {
    /// `sprite_height`'s answers, by model.
    static HEIGHTS: std::cell::RefCell<std::collections::HashMap<u32, Option<f32>>> = Default::default();
}

/// How tall a model stands at its first pose: its skeleton's highest node over
/// its root, times the sprite's default scale (`VIHSprite::m_defaultScale`, the
/// CSprite header's: every player race's skeleton stands about 1.8, a gnome's
/// drawn at 0.67 of it), in world units (what the camera's framing of a race is
/// made for). None when the library has no such model or it has no skeleton.
/// Kept per model: a body's own never changes.
pub fn sprite_height(lib: &Library, model: u32) -> Option<f32> {
    if let Some(h) = HEIGHTS.with(|c| c.borrow().get(&model).copied()) {
        return h;
    }
    let h = lib.lookup(model as i32).and_then(|r| {
        let mut anims = AnimScene { time: 0, ..Default::default() };
        let mut cs = actor_sprite(lib, r, &mut anims, &mut Rand(1))?;
        cs.h.process(0, &anims);
        let top = cs.h.nodes.iter().map(|n| n.world.m[3].y.0).filter(|y| y.is_finite()).reduce(f32::max)?;
        Some(top * cs.h.default_scale.0).filter(|&y| y > 0.0)
    });
    // a body not read yet asks again once it is
    if h.is_some() {
        HEIGHTS.with(|c| c.borrow_mut().insert(model, h));
    }
    h
}

impl Figure {
    /// `CreateActor` with the model's sprite (or `HumanMale`'s), dressed in
    /// `look` (a helm on node 8 when its set has one), `items` (DictIDs, 0
    /// none) in the item slots 0..3. None when neither model is there. Built
    /// whole in the art its model is drawn in (`esf::Library::view_for`):
    /// the 2002 disc's while its art is drawn and that disc has the model.
    pub fn new(lib: &Library, g: &mut Gpu, model: u32, look: vi::cust::Look, items: [u32; 3], at: [f32; 3], hpr: [f32; 3], now: i64) -> Option<Figure> {
        Figure::new_dressed(lib, g, model, look, None, items, at, hpr, now)
    }

    /// `new` in a server's race's look too (`raceopts`: its parts' colours and
    /// textures, laid only on the body they were chosen on).
    #[allow(clippy::too_many_arguments)]
    pub fn new_dressed(lib: &Library, g: &mut Gpu, model: u32, look: vi::cust::Look, race_look: Option<net::raceopts::Look>, items: [u32; 3], at: [f32; 3], hpr: [f32; 3], now: i64) -> Option<Figure> {
        lib.with_view(lib.view_for(model as i32), || Figure::new_in_view(lib, g, model, look, race_look, items, at, hpr, now))
    }

    /// Made again as it stands, in the art drawn now (F9): its model, look and
    /// items at its place; its animation starts over (the 2002 disc's bodies
    /// are other skeletons).
    pub fn remake(&mut self, lib: &Library, g: &mut Gpu, now: i64) {
        let cape = self.cape();
        if let Some(mut f) = Figure::new_dressed(lib, g, self.model, self.look, self.race_look.clone(), self.items, self.at, self.hpr, now) {
            f.wear_cape(lib, g, cape);
            *self = f;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new_in_view(lib: &Library, g: &mut Gpu, model: u32, look: vi::cust::Look, race_look: Option<net::raceopts::Look>, items: [u32; 3], at: [f32; 3], hpr: [f32; 3], now: i64) -> Option<Figure> {
        // a server's race's body not read yet (`res::Res::want_bodies`): its
        // `like` race's of the body's sex stands in, where the game draws a human
        let id = if lib.lookup(model as i32).is_some() {
            model
        } else {
            crate::pregame::client::create::tables::stand_in(model).filter(|&m| lib.lookup(m as i32).is_some()).unwrap_or_else(|| vi::hash::hash_resource_id("HumanMale"))
        };
        let r = lib.lookup(id as i32)?;
        let textures = render::library_textures(lib);
        let mut built = lib.build_model(r)?;
        let mut anims = AnimScene { time: now, ..Default::default() };
        let mut rand = Rand(1);
        let mut sprite = actor_sprite(lib, r, &mut anims, &mut rand);
        // a player's character: in a body one of the server's races wears it
        // holds items as a race's body does (`racebody`)
        if let Some(cs) = sprite.as_mut() {
            crate::racebody::grant(cs, id, true, Some(&crate::pregame::client::create::tables::offer()));
        }
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
            // the server's patch's tables (`s2`), retail's without one
            let t = crate::s2::get().cust();
            // on the node the world puts it on, seated as there (`play::attach`)
            if let Some((node, m)) = crate::slotless::helm_on(lib, id, cs, true, look.armor[5], look.helm_color(&t), &t, true) {
                helm_worn = true;
                attached.push((node, g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
            }
            crate::play::dress_look(lib, id as i32, &mut built, cs, &look, helm_worn, &t, true);
            // a server's race's own look, on the body it was chosen on
            if let Some(rl) = &race_look {
                crate::raceopts::dress(lib, id, &mut built, rl);
            }
        }
        let gpu = g.renderer.upload(g.device, g.queue, &built, &[render::IDENTITY], &textures);
        let scale = crate::raceopts::body_size(id);
        Some(Figure { model, look, race_look, scale, sprite, anims, rand, gpu, attached, items, at, hpr, centre: Matrix44::identity(), held: None, fx: Vec::new(), body: id, cape: None })
    }

    /// Puts on the cape `want` (`cape`: a character's on a server with capes),
    /// or takes it off, when it changes: fitted to the figure's skeleton, in
    /// Frontiers' art, drawn on its palette. A cape no art or body fits is kept
    /// as asked for, with no model.
    pub fn wear_cape(&mut self, lib: &Library, g: &mut Gpu, want: Option<net::cape::Cape>) {
        if self.cape.as_ref().map(|(c, _)| *c) == want {
            return;
        }
        self.cape = want.map(|c| {
            let m = self.sprite.as_ref().and_then(|cs| crate::cape::model(lib, c, self.body, cs));
            let gpu = m.map(|m| lib.with_view(esf::Era::Frontiers, || g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &render::library_textures(lib))));
            (c, gpu)
        });
    }

    /// The cape it wears as asked for (none: none).
    pub fn cape(&self) -> Option<net::cape::Cape> {
        self.cape.as_ref().map(|(c, _)| *c)
    }

    /// `VICSprite::SetAction(id)` (23 the select screen's flourish, 0 back
    /// to standing).
    pub fn set_action(&mut self, id: i32) {
        if let Some(cs) = self.sprite.as_mut() {
            cs.set_action(id, &self.anims, &mut self.rand);
        }
    }

    /// The actor's matrix (`CreateActor`'s place, heading and scale 1; a
    /// body a race's options offer at its size, `scale`).
    pub fn actor(&self) -> Matrix44<Ee> {
        let f = F::<Ee>::new;
        let scale = if self.scale == 1.0 { F::ONE } else { f(self.scale) };
        Matrix44::transform(&Vect3::new(f(self.hpr[0]), f(self.hpr[1]), f(self.hpr[2])), scale, &Vect3::new(f(self.at[0]), f(self.at[1]), f(self.at[2])))
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
                self.centre = m;
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
        if let Some((_, Some(cape))) = &self.cape {
            g.renderer.set_instances(g.queue, cape, &[matrix]);
            if !palette.is_empty() {
                g.renderer.set_palette(g.queue, cape, &palette);
            }
        }
    }

    /// The particles of the items in its hands one frame at `now`, for a
    /// camera at `eye`: each item's particle sprites under its node, as the
    /// scene rasters an attached item (`VIParticleSprite::Raster`, the copy
    /// `AttachItem` makes having its own emitters), and the particle system
    /// rendered. An epic weapon's glow at character select.
    pub fn particles_frame(&mut self, lib: &Library, eye: &Vect3<Ee>, now: i64) {
        use crate::particles::{How, InHand};
        let Some(cs) = self.sprite.as_ref() else { return };
        let mut hands = Vec::new();
        lib.with_view(lib.view_for(self.model as i32), || {
            for (slot, &item) in self.items.iter().enumerate() {
                let Some(n) = cs.item_node(slot).filter(|_| item != 0).and_then(|node| cs.h.nodes.get(node as usize)) else { continue };
                let at = Matrix44::mul(&n.world, &self.centre);
                for (k, e) in crate::particles::item_emitters(lib, item).into_iter().enumerate() {
                    hands.push(InHand { key: (0, slot, item, k), world: Matrix44::mul(&e.member, &at), def: e.def, how: How::Sprite(e.flags), drawn: true });
                }
            }
        });
        let held = self.held.get_or_insert_with(|| crate::particles::Held::new(now));
        self.fx = held.frame(lib, &hands, eye, now);
    }

    /// What to draw: the body, what it carries and its cape.
    pub fn models(&self) -> impl Iterator<Item = &GpuModel> {
        std::iter::once(&self.gpu).chain(self.attached.iter().map(|(_, m)| m)).chain(self.cape.iter().filter_map(|(_, m)| m.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The figures' sprite is `CreateActor`'s copy: its first `SetAction(0)`
    /// (character select's every frame, `OnDrawSelf`) plays the special idle
    /// on both layers at once, as the game's copy does. The control: the
    /// parsed sprite as it is (`SetDefaults`' timers) keeps the plain idle,
    /// barely moving until the flourish five seconds in. Needs `run/eqoa.iso`
    /// (the pregame's CHARSEL files).
    #[test]
    fn a_figure_fidgets_from_the_start() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !path.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let mut iso = iso::Iso::open(&path).unwrap();
        let mut lib = Library::new();
        for f in crate::pregame::client::FILES {
            if let Some(c) = iso.read(f).ok().and_then(|b| esf::Container::open(b).ok()) {
                lib.add(*f, c);
            }
        }
        // a human, as an unarmed level-1 character shows at character select
        let r = lib.lookup(vi::hash::hash_resource_id("HumanMale") as i32).expect("HumanMale in the CHARSEL files");
        let now = 64_250;
        let first = |cs: &mut CSprite<Ee>, anims: &mut AnimScene<Ee>, rand: &mut Rand| {
            anims.time = now + 33;
            cs.set_action(0, anims, rand);
            [cs.cur[0].anim, cs.cur[1].anim]
        };
        let mut anims: AnimScene<Ee> = AnimScene { time: now, ..Default::default() };
        let mut rand = Rand(1);
        let mut cs = actor_sprite(&lib, r, &mut anims, &mut rand).expect("the sprite");
        assert_eq!(first(&mut cs, &mut anims, &mut rand), [15, 15], "the copy's first SetAction(0) is the special idle");
        // the control: the parsed sprite, its timers SetDefaults'
        let mut anims: AnimScene<Ee> = AnimScene { time: now, ..Default::default() };
        let mut rand = Rand(1);
        let mut parsed = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).expect("the sprite");
        assert_eq!(first(&mut parsed, &mut anims, &mut rand), [0, 0], "the parsed sprite only idles");
    }

    /// The items in a figure's hands carry their particles at character
    /// select as in the world (`particles_frame`): the Sword of Virtue's
    /// glow, round the hand that holds it. The control: the same figure with
    /// nothing in its hands has none. Needs `run/eqoa.iso` (the pregame's
    /// files, ITEM.CSF among them) and a graphics adapter.
    #[test]
    fn a_figures_epic_weapon_glows_at_character_select() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let instance = render::wgpu::Instance::new(render::wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&render::wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let mut lib = Library::new();
        for f in crate::pregame::client::FILES {
            if let Some(c) = iso.read(f).ok().and_then(|b| esf::Container::open(b).ok()) {
                lib.add(*f, c);
            }
        }
        let mut g = Gpu { renderer: &mut renderer, device: &device, queue: &queue };
        let human = vi::hash::hash_resource_id("HumanMale");
        let look = vi::cust::Look { hair: [1, 2, 3], face: 1, robe: -1, ..Default::default() };
        let eye = Vect3::of(0.0, 1.0, 4.0);
        let glow = |items: [u32; 3], g: &mut Gpu| {
            let mut fig = Figure::new(&lib, g, human, look, items, [0.0; 3], [0.0; 3], 0).unwrap();
            let mut hand = [0.0; 3];
            for t in (33..1500).step_by(33) {
                fig.pose(g, t);
                fig.particles_frame(&lib, &eye, t);
                let cs = fig.sprite.as_ref().unwrap();
                let n = &cs.h.nodes[cs.item_node(0).unwrap() as usize];
                let w = Matrix44::mul(&n.world, &fig.centre);
                hand = [w.m[3].x.0, w.m[3].y.0, w.m[3].z.0];
            }
            let quads: Vec<[f32; 3]> = fig.fx.iter().flat_map(|b| b.quads.iter().map(|q| q.center)).collect();
            (quads, hand)
        };
        let (quads, hand) = glow([0x88EC_092D, 0, 0], &mut g);
        assert!(!quads.is_empty(), "the sword's glow");
        let near = |c: &[f32; 3]| (0..3).map(|k| (c[k] - hand[k]).powi(2)).sum::<f32>().sqrt() < 3.0;
        assert!(quads.iter().all(near), "round the hand at {hand:?}: {:?}", quads.iter().find(|c| !near(c)));
        assert!(glow([0; 3], &mut g).0.is_empty(), "the control: empty hands, no particles");
    }

    /// A figure in a body one of the server's races wears holds its items as
    /// a race's does (`racebody`): the gargoyles' body, which holds nothing by
    /// its header, hangs the sword on its right hand and the shield on its
    /// left while the server's offer lists it; the control: with no offer (a
    /// server with no races of its own, and a PS2's character select) it holds
    /// nothing. Needs `run/eqoa.iso` and a graphics adapter.
    #[test]
    fn a_gargoyle_figure_holds_its_items() {
        const GARGOYLE: u32 = 0xB4D2_D132;
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let instance = render::wgpu::Instance::new(render::wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&render::wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let mut g = Gpu { renderer: &mut renderer, device: &device, queue: &queue };
        let mut lib = Library::new();
        for f in ["CHAR.ESF", "ITEM.ESF"] {
            lib.add(f, esf::Container::open(iso.read(f).unwrap()).unwrap());
        }
        let look = vi::cust::Look { robe: -1, ..Default::default() };
        // the heirloom sword and shield
        let items = [0x47D9_81E0, 0, 0xD21E_C0C5];
        let nodes = |g: &mut Gpu| {
            let fig = Figure::new(&lib, g, GARGOYLE, look, items, [0.0; 3], [0.0; 3], 0).expect("the figure");
            let mut v: Vec<i32> = fig.attached.iter().map(|a| a.0).collect();
            v.sort_unstable();
            v
        };
        use crate::pregame::client::create::tables;
        tables::set_offer(None);
        assert_eq!(nodes(&mut g), Vec::<i32>::new(), "the control: no offer, nothing held");
        tables::set_offer(Some(&net::creation::Creation {
            all_classes: true,
            races: vec![net::creation::Race { id: 29, like: 9, city: 0, bonus: [0; 7], class_mask: 0x7FFF, body: [GARGOYLE, GARGOYLE], name: "Gargoyle".into(), short: "GAR".into(), desc: String::new() }],
        }));
        assert_eq!(nodes(&mut g), [11, 20], "the sword on node 11, the shield on 20");
        tables::set_offer(None);
    }

    /// A figure at character select wears its character's cape (`cape`) out
    /// of the pregame's own files and the cape's body cut out of CHAR.ESF, as
    /// `res::Res::want_bodies` reads it: one model more, on the body's palette,
    /// none for a style with no art; taken off, the figure draws as it did.
    /// The control: a figure asked for no cape draws exactly its body and what
    /// it carries. Needs `run/eqoa.iso` and a graphics adapter.
    #[test]
    fn a_figure_wears_its_cape_and_none_adds_nothing() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let instance = render::wgpu::Instance::new(render::wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&render::wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let mut g = Gpu { renderer: &mut renderer, device: &device, queue: &queue };
        let mut lib = Library::new();
        for f in crate::pregame::client::FILES {
            if let Some(c) = iso.read(f).ok().and_then(|b| esf::Container::open(b).ok()) {
                lib.add(*f, c);
            }
        }
        let cape = net::cape::Cape { style: 1, color: 0 };
        let bodies = crate::cape::bodies([&cape]);
        assert_eq!(bodies, [0xD130_0BD3], "the knight's body");
        lib.add(crate::pregame::res::BODIES, crate::pregame::res::cut_bodies(iso.read("CHAR.ESF").unwrap(), &bodies).expect("the knight's body"));
        let human = vi::hash::hash_resource_id("HumanMale");
        let look = vi::cust::Look { hair: [1, 2, 3], face: 1, robe: -1, ..Default::default() };
        let mut fig = Figure::new(&lib, &mut g, human, look, [0; 3], [0.0; 3], [0.0; 3], 0).unwrap();
        let bare = fig.models().count();
        fig.wear_cape(&lib, &mut g, None);
        assert_eq!(fig.models().count(), bare, "the control: no cape asked, none drawn");
        fig.wear_cape(&lib, &mut g, Some(cape));
        assert_eq!((fig.models().count(), fig.cape()), (bare + 1, Some(cape)), "the cape drawn with the body");
        fig.pose(&g, 33);
        fig.wear_cape(&lib, &mut g, Some(net::cape::Cape { style: 200, color: 0 }));
        assert_eq!(fig.models().count(), bare, "a style with no art: none drawn");
        fig.wear_cape(&lib, &mut g, None);
        assert_eq!((fig.models().count(), fig.cape()), (bare, None), "taken off");
    }

    /// A figure is its model as the art drawn has it (`era`): with the 2002
    /// disc's CHARSEL files standing in, the human of 2002 (another
    /// skeleton), made again as Frontiers' when the art goes back, its look
    /// and items kept. The control is the Frontiers view's own figure. Needs
    /// both discs (`run/eqoa.iso`, `$EQOA_2002_ISO` or the USA disc under
    /// `../decomp-builds`) and a graphics adapter.
    #[test]
    fn a_figure_is_its_model_in_the_art_drawn() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
            std::fs::read_dir(root.join("../decomp-builds/discs/eqoa-usa")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        });
        let (Ok(mut f), Some(Ok(mut y))) = (iso::Iso::open(root.join("run/eqoa.iso")), y2002.map(iso::Iso::open)) else {
            eprintln!("SKIP: no Frontiers or no 2002 disc");
            return;
        };
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let instance = render::wgpu::Instance::new(render::wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&render::wgpu::RequestAdapterOptions::default())) else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let mut lib = Library::new();
        let mut art = Vec::new();
        for path in crate::pregame::client::FILES {
            let name = crate::pregame::res::name_of(path).to_ascii_uppercase();
            if let Some(c) = f.read(path).ok().and_then(|b| esf::Container::open(b).ok()) {
                lib.add(name.clone(), c);
            }
            if let Some(c) = y.read(crate::pregame::res::name_of(path)).ok().and_then(|b| esf::Container::open(b).ok()) {
                art.push((name, c));
            }
        }
        assert!(crate::era::install(&mut lib, art) >= 4, "the CHARSEL files are other art");
        let human = vi::hash::hash_resource_id("HumanMale");
        let look = vi::cust::Look { hair: [1, 2, 3], face: 1, robe: -1, ..Default::default() };
        let nodes = |fig: &Figure| fig.sprite.as_ref().map_or(0, |cs| cs.h.nodes.len());
        let mut g = Gpu { renderer: &mut renderer, device: &device, queue: &queue };
        let frontiers = Figure::new(&lib, &mut g, human, look, [0; 3], [1.0, 2.0, 3.0], [0.5, 0.0, 0.0], 0).unwrap();
        lib.set_view(esf::Era::Y2002);
        let mut fig = Figure::new(&lib, &mut g, human, look, [0; 3], [1.0, 2.0, 3.0], [0.5, 0.0, 0.0], 0).unwrap();
        assert_ne!(nodes(&fig), nodes(&frontiers), "the 2002 human is another skeleton");
        let y_nodes = nodes(&fig);
        // the art back to Frontiers': made again as Frontiers' where it stood
        lib.set_view(esf::Era::Frontiers);
        fig.remake(&lib, &mut g, 100);
        assert_eq!(nodes(&fig), nodes(&frontiers));
        assert_eq!((fig.model, fig.look, fig.at, fig.hpr, fig.items), (human, look, [1.0, 2.0, 3.0], [0.5, 0.0, 0.0], [0; 3]));
        lib.set_view(esf::Era::Y2002);
        fig.remake(&lib, &mut g, 200);
        assert_eq!(nodes(&fig), y_nodes);
        eprintln!("HumanMale: Frontiers {} nodes, 2002 {y_nodes}", nodes(&frontiers));
    }
}

#[cfg(test)]
mod heights {
    /// A body's height is its skeleton's times its sprite's default scale: every
    /// player race's skeleton stands about 1.8, the gnome's drawn at 0.67 of it,
    /// so a gnome is the shorter and an ogre the taller; a hill giant (CHAR.ESF's,
    /// cut as the pregame cuts it) stands about 1.8 ogres. The control: the
    /// skeleton alone puts the gnome over the human. Needs `run/eqoa.iso`.
    #[test]
    fn a_bodys_height_takes_its_sprites_scale() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut d) = iso::Iso::open(iso.to_str().unwrap()) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        for f in ["CHARSEL1.CSF", "CHARSEL3.CSF"] {
            lib.add(f, esf::Container::open(d.read(f).unwrap()).unwrap());
        }
        lib.add(crate::pregame::res::BODIES, crate::pregame::res::cut_bodies(d.read("CHAR.ESF").unwrap(), &[0xFFB1_EA2C]).unwrap());
        let race = |r: i32| crate::pregame::client::races::get_race(r).model_id(0);
        let h = |m: u32| super::sprite_height(&lib, m).unwrap();
        let (human, gnome, ogre, giant) = (h(race(0)), h(race(3)), h(race(9)), h(0xFFB1_EA2C));
        assert!((1.7..1.85).contains(&human) && (1.25..1.4).contains(&gnome) && (2.5..2.8).contains(&ogre), "{human} {gnome} {ogre}");
        assert!((1.7..1.9).contains(&(giant / ogre)), "the giant: {giant} against {ogre}");
        // the control: the gnome's skeleton alone stands over the human's
        let r = lib.lookup(race(3) as i32).unwrap();
        let mut anims = vi::hsprite::AnimScene { time: 0, ..Default::default() };
        let mut cs = super::actor_sprite(&lib, r, &mut anims, &mut vi::csprite::Rand(1)).unwrap();
        cs.h.process(0, &anims);
        let bare = cs.h.nodes.iter().map(|n| n.world.m[3].y.0).reduce(f32::max).unwrap();
        assert!(bare > human, "the gnome's bare skeleton {bare} against the human's {human}");
    }
}
