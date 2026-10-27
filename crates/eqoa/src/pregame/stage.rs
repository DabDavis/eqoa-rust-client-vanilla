//! The character screens' 3D: CLIENT's pregame scene as `VIClient::LoadWorld`
//! ("PreGame Load") makes it. ARENA.CSF is a world of its own, one zone of
//! twelve small sets, a race's home each (the lava of Neriak, Erudin's blue
//! marble, a halfling's burrow...): `VIWndCharacterSelect::ShowCharacter` and
//! the creation pages stand the character in its race's set (`VIRaceDef`'s
//! select place) and aim the walk camera at it (`VIWalkCamera::Init`,
//! `SetCameraMode(1)`, the race camera's review framing, `OrbitRight`), the
//! scene lit at 8:00 (`SetTimeOfDay(8.0)`), and render it into the window's
//! viewport (`Viewport(358, 48, 620, 396)`) through `m_camera`: 55 degrees,
//! the viewport's own shape (0.75287354), 0.1 to 900.
//!
//! Here: the arena's zone uploaded once (terrain and the 239 placed actors
//! with their baked lighting, as `scene::zone` builds a zone, but room by
//! room), its collision world for the camera (`VIScene::Collide`), the sky
//! `SetSky` gives the scene
//! (SKY.CSF's dome, sun, corona and moon, as the atmosphere sets them at
//! 8:00 under a clear sky: made once, as nothing in it moves at a fixed
//! hour), and the view a window draws its figure and the arena through
//! (`pregame::Scene`).
//!
//! What of the arena is drawn is `VIScene::Render`'s: `VIWorld::RenderByPortals`
//! finds the camera's room, and a room closed to the sky (flag bit 0 clear) is
//! drawn alone, with what its portals show (`VIZone::RenderRoom`): first its own
//! sprite (`m_p28`, resolved into `m_p1C`), which is one of the zone's terrain
//! patches (drawn through the view's rotation alone because a patch is placed
//! by the pre-translations, relative to the camera), then its actors; a room
//! open to the sky draws the whole world by the frustum. Each race's set is a
//! closed room of its own with no portals, its floor and walls its patch (seven
//! open rooms lie between the sets, with nothing in them), so the camera in a
//! set shows that set. Drawn whole, the other eleven sets showed past the
//! character, all 239 actors and twelve patches a frame (the user, 2026-10-27:
//! "the draw distance is too far in the background of the preview of
//! character, shows the whole arena.esf also FPS really suffers here").

use crate::play::Gpu;
use esf::Library;
use render::GpuModel;
use vi::camera::{Eye, WalkCamera};
use vi::fpu::{Ee, F};
use vi::vect::Vect3;
use vi::world::World;

/// `m_camera` in the character scenes: `LoadWorld`'s second `Init`.
pub const FOV: f32 = 55.0;
pub const ASPECT: f32 = 0.752_873_54;
pub const NEAR: f32 = 0.1;
pub const FAR: f32 = 900.0;
/// `SetTimeOfDay(8.0)`.
pub const HOUR: f32 = 8.0;
/// The viewport the select and creation pages render into.
pub const VIEWPORT: [i32; 4] = [358, 48, 620, 396];

/// The pixels of a `size` target that the viewport `v` (inclusive frame
/// pixels) takes, as `[x, y, w, h]`: exactly those a 2D quad laid over the
/// same frame rectangle covers (its edges `render::ui::SAMPLE_BIAS` up and
/// left, as `render::ui` draws every quad; a pixel covered when its centre
/// is inside), so a piece of the 2D layer drawn over the viewport hides all
/// of the 3D under it. Truncating the edges instead gave the 3D a column (and
/// a row) the 2D did not cover in a window scaled by a fraction: character
/// select's plate over the view showed a line of the arena at its left side
/// (1062x780, F3 20261023-142644): the frame is drawn 4:3 at any window
/// shape, so its pixels are almost never whole window pixels.
pub fn viewport_pixels(size: (u32, u32), v: [i32; 4]) -> [u32; 4] {
    let b = render::ui::SAMPLE_BIAS;
    let lo = render::ui::window_from_frame(size, [v[0] as f32 - b, v[1] as f32 - b]);
    let hi = render::ui::window_from_frame(size, [(v[2] + 1) as f32 - b, (v[3] + 1) as f32 - b]);
    let px = |e: f32, n: u32| (e - 0.5).ceil().clamp(0.0, n as f32) as u32;
    let (x, y) = (px(lo[0], size.0), px(lo[1], size.1));
    [x, y, px(hi[0], size.0).saturating_sub(x), px(hi[1], size.1).saturating_sub(y)]
}

pub struct Stage {
    /// the arena's collision (the camera's) and its clock
    pub world: World<Ee>,
    /// `m_walkCam`
    pub cam: WalkCamera<Ee>,
    pub eye: Eye<Ee>,
    /// the arena's zone on the GPU (`Arena`)
    arena: Arena,
    /// the sky's models, drawn through the view's rotation alone
    sky: Vec<GpuModel>,
}

/// The arena on the GPU, room by room, and the terrain patches no room owns.
#[derive(Default)]
struct Arena {
    rooms: Vec<RoomModels>,
    unowned: Vec<GpuModel>,
}

/// A room's own sprite, its terrain patch, when its DictID names one
/// (`VIScene::ReplaceProxyRooms`), and its actors.
#[derive(Default)]
struct RoomModels {
    own: Vec<GpuModel>,
    actors: Vec<GpuModel>,
}

impl Stage {
    /// The arena out of the pregame's library (ARENA.CSF, read with
    /// CLIENT's files). None when it is not there.
    pub fn new(lib: &Library, g: &mut Gpu) -> Option<Stage> {
        let f = lib.file_named("ARENA.CSF")?;
        let mut world = World::<Ee>::load(lib, f)?;
        let arena = arena(lib, f, &world, g)?;
        let mut cam = WalkCamera::<Ee>::default();
        let eye = cam.init(Vect3::of(0.0, 0.0, 0.0), Vect3::of(0.0, 0.0, 0.0), &mut world);
        let sky = sky(lib, &mut world, &eye, g).unwrap_or_default();
        Some(Stage { world, cam, eye, arena, sky })
    }

    /// The arena and the sky uploaded again in the art drawn now (F9: the
    /// 2002 disc's sky), the camera and the collision kept where they are.
    pub fn remake(&mut self, lib: &Library, g: &mut Gpu) {
        if let Some(arena) = lib.file_named("ARENA.CSF").and_then(|f| arena(lib, f, &self.world, g)) {
            self.arena = arena;
        }
        let eye = self.eye;
        self.sky = sky(lib, &mut self.world, &eye, g).unwrap_or_default();
    }

    /// `m_walkCam->Init(at, hpr, camera, scene)` and `SetCameraMode(1)`:
    /// the camera's subject at `at`, facing `hpr`.
    pub fn aim(&mut self, at: [f32; 3], hpr: [f32; 3]) {
        self.eye = self.cam.init(Vect3::of(at[0], at[1], at[2]), Vect3::of(hpr[0], hpr[1], hpr[2]), &mut self.world);
        self.cam.set_camera_mode(1);
    }

    /// `VIWalkCamera::Process`, after whatever the window set (the race
    /// camera's framing, `OrbitRight`): the eye this frame.
    pub fn process(&mut self, now: i64) {
        self.world.time = now;
        self.eye = self.cam.process(&mut self.world);
    }

    /// The view the window's viewport shows: `m_camera` (55 degrees, the
    /// viewport's shape) at the walk camera's eye, the 8:00 lights.
    pub fn view(&self) -> render::View {
        let eye = glam::Vec3::from(self.eye.position.to_array());
        let dir = glam::Vec3::from(self.eye.direction.to_array());
        let sky_proj = glam::camera::rh::proj::directx::perspective(FOV.to_radians(), ASPECT, NEAR, FAR);
        // `SetFarPlane`: the terrain profiles' far at the eye, where the
        // arena has a profile grid
        let far = self.world.grid.as_ref().map_or(FAR, |g| g.far_plane(&self.eye.position).0);
        let proj = crate::fog::with_far(sky_proj, far);
        let view = glam::camera::rh::view::look_to_mat4(eye, dir.normalize_or(glam::Vec3::Z), glam::Vec3::Y);
        let tod = vi::tod::TimeOfDay::<Ee>::new();
        let hour = F::new(HOUR);
        let (amb, sun, dir) = tod.lights(hour, F::ZERO);
        let rgb = |c: [F<Ee>; 3]| glam::Vec3::new(c[0].0, c[1].0, c[2].0);
        let mut v = render::View {
            view_proj: proj * view,
            eye,
            sun_dir: glam::Vec3::from(dir.to_array()),
            sun: rgb(sun),
            ambient: rgb(amb),
            fog: None,
            point: None,
            clear: glam::Vec3::ZERO,
            sky_view_proj: None,
        };
        // the fog and the sky's colour as the terrain profiles under the eye
        // blend them, where the arena has a profile grid
        if let Some(grid) = self.world.grid.as_ref() {
            let b = grid.profile_blend(&self.eye.position);
            let fog = tod.fog(hour, F::ZERO, &b, self.world.env_flags(&self.eye.position), &sun);
            v.fog = Some((fog.near.0, fog.far.0, glam::Vec3::new(fog.color[0].0, fog.color[1].0, fog.color[2].0)));
            let c = tod.sky(hour, F::ZERO, &b);
            v.clear = glam::Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0;
        }
        if !self.sky.is_empty() {
            let mut rot = view;
            rot.w_axis = glam::Vec4::W;
            v.sky_view_proj = Some(sky_proj * rot);
        }
        v
    }

    /// `room_alone` at the camera's eye.
    pub fn room_alone(&self) -> Option<usize> {
        room_alone(&self.world, &self.eye.position)
    }

    /// What is drawn of the stage this frame: the sky, then the camera's
    /// room, its patch and its actors, when it is drawn alone (`room_alone`),
    /// else every room's and the patches no room owns.
    pub fn models(&self) -> std::vec::IntoIter<&GpuModel> {
        let mut out: Vec<&GpuModel> = self.sky.iter().collect();
        match self.room_alone().and_then(|r| self.arena.rooms.get(r)) {
            Some(r) => out.extend(r.own.iter().chain(&r.actors)),
            None => {
                out.extend(&self.arena.unowned);
                out.extend(self.arena.rooms.iter().flat_map(|r| r.own.iter().chain(&r.actors)));
            }
        }
        out.into_iter()
    }
}

/// The room `VIWorld::RenderByPortals` draws alone with the camera at `at`:
/// its room, when it is closed to the sky and has no portals to draw through
/// (the arena's sets have none); None, the whole arena by the frustum.
fn room_alone(world: &World<Ee>, at: &Vect3<Ee>) -> Option<usize> {
    let (z, leaf) = world.find_leaf(at)?;
    let zone = world.zones.get(z)?;
    let room = usize::try_from(zone.leaves.get(leaf)?.0).ok()?;
    let r = zone.rooms.get(room)?;
    (r.flags & 1 == 0 && r.portals == 0).then_some(room)
}

/// The arena's zone (file `f`) on the GPU, with its baked lighting: each of
/// `world`'s rooms (the zone's, in order) with its own patch and its actors,
/// and the patches no room owns.
fn arena(lib: &Library, f: usize, world: &World<Ee>, g: &mut Gpu) -> Option<Arena> {
    let zone = *lib.file(f).zones().first()?;
    let textures = render::library_textures(lib);
    let mut upload = |s: &crate::scene::Scene| -> Vec<GpuModel> {
        s.parts
            .iter()
            .zip(&s.statics)
            .map(|((m, mats), st)| {
                let gm = g.renderer.upload(g.device, g.queue, m, mats, &textures);
                g.renderer.set_static_lights(g.queue, &gm, st);
                gm
            })
            .collect()
    };
    let rooms = world.zones.first().map(|z| z.rooms.as_slice()).unwrap_or_default();
    let owned: Vec<i32> = rooms.iter().map(|r| r.sprite_id as i32).filter(|&id| id != 0).collect();
    let rooms = rooms
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let id = r.sprite_id as i32;
            let own = if id != 0 { upload(&crate::scene::patches(lib, f, zone, &|p| p == id)) } else { Vec::new() };
            RoomModels { own, actors: upload(&crate::scene::room(lib, f, zone, k as i32)) }
        })
        .collect();
    let unowned = upload(&crate::scene::patches(lib, f, zone, &|p| !owned.contains(&p)));
    Some(Arena { rooms, unowned })
}

/// The scene's sky at 8:00 (`VIScene::SetSky`, the atmosphere's `SetSky`,
/// `Update` and `SetupTimeOfDaySky` once): SKY.CSF's sprites on the GPU with
/// their palettes and placements as `sky::update` sets them in play. SKY.CSF
/// and FX.CSF are CLIENT's pregame files; their bytes are parsed again into
/// a library of the sky's own (the atmosphere takes one to own).
fn sky(lib: &Library, world: &mut World<Ee>, eye: &Eye<Ee>, g: &mut Gpu) -> Option<Vec<GpuModel>> {
    // in the art drawn: the 2002 disc's copies where they stand in
    let parse = |name: &str| lib.file_named(name).and_then(|f| esf::Container::parse(lib.file(f).data.clone()).ok());
    let mut l = Library::new();
    l.add(crate::sky::FILE, parse("SKY.CSF")?);
    if let Some(fx) = parse("FX.CSF") {
        l.add("DATA2/FX.CSF", fx);
    }
    let mut wx = crate::weather::Weather::new(&mut l, 0);
    wx.frame(world, None, eye, 0, HOUR);
    if let Some(grid) = world.grid.as_ref() {
        let b = grid.profile_blend(&eye.position);
        let c = vi::tod::TimeOfDay::<Ee>::new().sky(F::new(HOUR), F::ZERO, &b);
        wx.sky_colour(c, vi::tod::lavastorm_weight(&b));
    }
    let a = &wx.atm;
    let frame = crate::sky::SkyFrame {
        plane: a.p618,
        lights: a.p618 == 0 && a.overcast != 0,
        rot: vi::atmosphere::SkyRotations { sun: a.sky_rot, corona: a.corona_rot, moon: a.moon_rot },
        pals: (0..5).map(|k| a.pals.get(&k).cloned().unwrap_or_default()).collect(),
    };
    let mut models = Vec::new();
    let at = crate::sky::upload(g.renderer, g.device, g.queue, &l, &mut models)?;
    let textures = render::library_textures(&l);
    crate::sky::update(g.renderer, g.device, g.queue, &mut models, at, &frame, &textures);
    Some(models)
}

#[cfg(test)]
mod viewport_tests {
    use super::*;

    /// The viewport takes the pixels a 2D quad over the same frame rectangle
    /// covers, no more and no fewer, at whole and fractional scales alike:
    /// each pixel's centre against the quad's edges as `render::ui` places
    /// them (`(p - SAMPLE_BIAS) * scale` past the frame's corner). The
    /// control: the truncated edges the client had leave the 3D a column the
    /// quad does not cover at 1062x780 (Bird's window).
    #[test]
    fn the_viewport_is_what_a_quad_over_it_covers() {
        let span = |size: (u32, u32), axis: usize| {
            let b = render::ui::SAMPLE_BIAS;
            let lo = render::ui::window_from_frame(size, [VIEWPORT[0] as f32 - b, VIEWPORT[1] as f32 - b])[axis];
            let hi = render::ui::window_from_frame(size, [(VIEWPORT[2] + 1) as f32 - b, (VIEWPORT[3] + 1) as f32 - b])[axis];
            (lo, hi)
        };
        let agrees = |size: (u32, u32), vp: [u32; 4]| {
            (0..2).all(|axis| {
                let (lo, hi) = span(size, axis);
                let n = if axis == 0 { size.0 } else { size.1 };
                (0..n).all(|c| {
                    let quad = (c as f32 + 0.5) >= lo && (c as f32 + 0.5) < hi;
                    let view = c >= vp[axis] && c < vp[axis] + vp[axis + 2];
                    quad == view
                })
            })
        };
        for size in [(640, 448), (1280, 896), (1062, 780), (1280, 720), (960, 720), (1737, 1010), (2560, 1417), (1918, 1078)] {
            assert!(agrees(size, viewport_pixels(size, VIEWPORT)), "{size:?}");
        }
        // the control: the edges truncated, unbiased
        let truncated = |size: (u32, u32)| {
            let a = render::ui::window_from_frame(size, [VIEWPORT[0] as f32, VIEWPORT[1] as f32]);
            let b = render::ui::window_from_frame(size, [(VIEWPORT[2] + 1) as f32, (VIEWPORT[3] + 1) as f32]);
            let (x, y) = (a[0] as u32, a[1] as u32);
            [x, y, b[0] as u32 - x, b[1] as u32 - y]
        };
        assert!(!agrees((1062, 780), truncated((1062, 780))), "truncation leaves the 3D a line the quad misses");
    }
}

#[cfg(test)]
mod room_tests {
    use super::*;

    /// ARENA.CSF off the disc, and its world; None without `run/eqoa.iso`.
    fn arena() -> Option<(Library, usize, World<Ee>)> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = Library::new();
        let f = lib.add("ARENA.CSF", esf::Container::open(iso::Iso::open(&iso).ok()?.read("DATA2/ARENA.CSF").ok()?).ok()?);
        let w = World::<Ee>::load(&lib, f)?;
        Some((lib, f, w))
    }

    /// The arena's rooms as `ParseZoneRoom` reads them: a closed room with no
    /// portals for each of the twelve sets, its own sprite a terrain patch of
    /// the zone and its floor; seven open rooms between them with nothing in
    /// them.
    #[test]
    fn the_arena_is_twelve_closed_sets() {
        let Some((lib, f, w)) = arena() else { return };
        let rooms = &w.zones[0].rooms;
        assert_eq!(rooms.len(), 19);
        let closed: Vec<usize> = (0..19).filter(|&k| rooms[k].flags & 1 == 0).collect();
        assert_eq!(closed, [0, 2, 3, 5, 6, 9, 10, 12, 13, 14, 15, 17]);
        assert!(rooms.iter().all(|r| r.portals == 0));
        let zone = *lib.file(f).zones().first().unwrap();
        for (k, r) in rooms.iter().enumerate() {
            let own = crate::scene::patches(&lib, f, zone, &|p| p == r.sprite_id as i32);
            assert_eq!(own.parts.len(), closed.contains(&k) as usize, "room {k}: its own patch");
            let open = !closed.contains(&k);
            assert_eq!(crate::scene::room(&lib, f, zone, k as i32).parts.is_empty(), open || k == 10 || k == 12 || k == 17, "room {k}: its actors");
        }
        // every patch is a set's: none is left for the frustum alone
        let owned: Vec<i32> = rooms.iter().map(|r| r.sprite_id as i32).filter(|&i| i != 0).collect();
        assert!(crate::scene::patches(&lib, f, zone, &|p| !owned.contains(&p)).parts.is_empty());
        assert_eq!(crate::scene::patches(&lib, f, zone, &|_| true).parts.len(), 1, "the control: all twelve, one model");
    }

    /// The camera in a set (the dark elf's, where character select puts it)
    /// draws that room alone; in an open room between the sets, or off the
    /// arena, everything (the control: the old stage drew everything always).
    #[test]
    fn the_camera_in_a_set_draws_that_set_alone() {
        let Some((_, _, w)) = arena() else { return };
        assert_eq!(room_alone(&w, &Vect3::of(-14.0, 1.63, 1.8)), Some(9), "Neriak's set");
        // room 1 is open (bit 0): its box spans x 0..19, z 6..19 with room 0's
        // closed one inside it, x 7..19
        let open = (0..20).map(|i| Vect3::of(0.5 + i as f32 * 0.3, 1.0, 12.0)).find(|v| w.find_leaf(v).is_some_and(|(z, l)| w.zones[z].rooms[w.zones[z].leaves[l].0 as usize].flags & 1 == 1));
        let open = open.expect("a spot in an open room");
        assert_eq!(room_alone(&w, &open), None);
        assert_eq!(room_alone(&w, &Vect3::of(5000.0, 1.0, 5000.0)), None, "off the arena");
    }
}
