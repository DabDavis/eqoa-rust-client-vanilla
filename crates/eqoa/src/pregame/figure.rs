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
    /// the items asked for (DictIDs, 0 none), in the item slots 0..3
    pub items: [u32; 3],
    /// the actor's place and heading, pitch and roll (`CreateActor(at, hpr, 1.0)`)
    pub at: [f32; 3],
    pub hpr: [f32; 3],
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

impl Figure {
    /// `CreateActor` with the model's sprite (or `HumanMale`'s), dressed in
    /// `look` (a helm on node 8 when its set has one), `items` (DictIDs, 0
    /// none) in the item slots 0..3. None when neither model is there. Built
    /// whole in the art its model is drawn in (`esf::Library::view_for`):
    /// the 2002 disc's while its art is drawn and that disc has the model.
    pub fn new(lib: &Library, g: &mut Gpu, model: u32, look: vi::cust::Look, items: [u32; 3], at: [f32; 3], hpr: [f32; 3], now: i64) -> Option<Figure> {
        lib.with_view(lib.view_for(model as i32), || Figure::new_in_view(lib, g, model, look, items, at, hpr, now))
    }

    /// Made again as it stands, in the art drawn now (F9): its model, look and
    /// items at its place; its animation starts over (the 2002 disc's bodies
    /// are other skeletons).
    pub fn remake(&mut self, lib: &Library, g: &mut Gpu, now: i64) {
        if let Some(f) = Figure::new(lib, g, self.model, self.look, self.items, self.at, self.hpr, now) {
            *self = f;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new_in_view(lib: &Library, g: &mut Gpu, model: u32, look: vi::cust::Look, items: [u32; 3], at: [f32; 3], hpr: [f32; 3], now: i64) -> Option<Figure> {
        let r = lib.lookup(model as i32).or_else(|| lib.lookup(vi::hash::hash_resource_id("HumanMale") as i32))?;
        let textures = render::library_textures(lib);
        let mut built = lib.build_model(r)?;
        let mut anims = AnimScene { time: now, ..Default::default() };
        let mut rand = Rand(1);
        let sprite = actor_sprite(lib, r, &mut anims, &mut rand);
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
            if cs.node_ids[8] >= 0
                && let Some(h) = vi::cust::helm_with(look.armor[5], &t)
                && let Some(mut m) = lib.lookup(h as i32).and_then(|r| lib.build_model(r))
            {
                let tint = look.helm_color(&t);
                for b in &mut m.batches {
                    for l in &mut b.layers {
                        l.color = tint;
                    }
                }
                helm_worn = true;
                attached.push((cs.node_ids[8], g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
            }
            for ch in vi::cust::apply_look_with(cs, &look, helm_worn, |id| lib.surface(id as i32).is_some(), &t) {
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
        Some(Figure { model, look, sprite, anims, rand, gpu, attached, items, at, hpr })
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
