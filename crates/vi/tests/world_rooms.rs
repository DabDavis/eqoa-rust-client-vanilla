//! The rooms' portals and `VIScene::Render`'s walk as far as `RasterActor`
//! (`World::raster_actors`), on the disc's TUNARIA. The portals: every room's
//! list as long as its record's count, every portal's corners on its plane
//! (Newell's, `CalcPortalPlane`), every portal into a room of its zone; the
//! control, a portal's corners against another portal's plane, are not. The
//! walk (the decomp's line by line; `VIClipStack` is held to the oracle in
//! `oracle_clip`): from inside closed rooms (Freeport's building by the docks,
//! zone 84 room 27, and the house of rooms in zone 83) looking every way, each
//! actor rastered is in view, within the view distance, and in a closed room
//! the portals reach, or outdoors once a portal to the outside is reached
//! (`RenderByFrustum`'s whole world); and the control, the frustum alone (what
//! chose the shadows' casters before), takes actors behind the walls the walk
//! leaves out. From outdoors on the docks, every actor of the camera's own
//! room in view and within the distance is rastered. Twice over, the same
//! list (the stamps let go).

use vi::clipstack::{Camera, Frustum};
use vi::fpu::{F, Ieee};
use vi::geom::Sphere;
use vi::vect::Vect3;
use vi::world::World;

type M = Ieee;

fn world() -> Option<World<M>> {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return None;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    World::<M>::load(&lib, f)
}

/// A camera like the game's at `eye` looking along (yaw, pitch): the view 55
/// degrees high at 4:3, near 0.3, far `far`, its corners in `CalcFrustum`'s
/// order as the game's world is drawn (looking along +z, +x on the left).
fn camera(eye: [f32; 3], yaw: f32, pitch: f32, far: f32) -> Camera<M> {
    let f = [yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()];
    let l = (f[2] * f[2] + f[0] * f[0]).sqrt().max(1e-6);
    let r = [-f[2] / l, 0.0, f[0] / l];
    let u = [r[1] * f[2] - r[2] * f[1], r[2] * f[0] - r[0] * f[2], r[0] * f[1] - r[1] * f[0]];
    let th = (55.0f32.to_radians() * 0.5).tan();
    let mut c = [Vect3::default(); 8];
    for (k, &(sx, sy, d)) in [(-1.0, 1.0, 0.3), (1.0, 1.0, 0.3), (1.0, -1.0, 0.3), (-1.0, -1.0, 0.3), (-1.0, 1.0, far), (1.0, 1.0, far), (1.0, -1.0, far), (-1.0, -1.0, far)].iter().enumerate() {
        let (hw, hh) = (d * th * 4.0 / 3.0, d * th);
        let p = [0, 1, 2].map(|i| eye[i] + f[i] * d + r[i] * (sx * hw) + u[i] * (sy * hh));
        c[k] = Vect3::of(p[0], p[1], p[2]);
    }
    Camera { frustum: Frustum::of_corners(c), position: Vect3::of(eye[0], eye[1], eye[2]), dir: Vect3::of(f[0], f[1], f[2]), far: F::new(far) }
}

/// Inside the pushed frustum (planes 1..5, the near one dropped) by
/// `CullByTop`'s sphere test.
fn in_view(cam: &Camera<M>, b: &Sphere<M>) -> bool {
    !cam.frustum.planes[1..].iter().any(|p| b.r < p.distance(&b.c))
}

/// The rooms each actor of zone `z` occupies.
fn rooms_of(w: &World<M>, z: usize) -> std::collections::HashMap<usize, Vec<usize>> {
    let mut m: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for (r, room) in w.zones[z].rooms.iter().enumerate() {
        for &a in room.actors.iter().chain(room.moving.iter()) {
            m.entry(a).or_default().push(r);
        }
    }
    m
}

#[test]
fn every_portal_as_its_record_has_it() {
    let Some(w) = world() else { return };
    let mut portals = 0;
    for (z, zone) in w.zones.iter().enumerate() {
        for (r, room) in zone.rooms.iter().enumerate() {
            assert_eq!(room.portal_list.len() as i32, room.portals, "zone {z} room {r}: the count");
            for (i, p) in room.portal_list.iter().enumerate() {
                portals += 1;
                assert!((p.dest as usize) < zone.rooms.len(), "zone {z} room {r} portal {i}: into room {}", p.dest);
                assert!(p.verts.len() >= 3, "zone {z} room {r} portal {i}: {} corners", p.verts.len());
                let n = p.plane.n;
                assert!(((n.x * n.x + n.y * n.y + n.z * n.z).0 - 1.0).abs() < 1e-4, "zone {z} room {r} portal {i}: a unit normal");
                for v in &p.verts {
                    assert!(p.plane.distance(v).0.abs() < 0.01, "zone {z} room {r} portal {i}: a corner off its plane");
                }
            }
        }
    }
    assert_eq!(portals, 2662, "Tunaria's portals");
    // the control: the building's west door's corners against its floor
    // opening's plane are not on it
    let room = &w.zones[84].rooms[27];
    let (door, floor) = (&room.portal_list[0], &room.portal_list[3]);
    assert!(door.verts.iter().any(|v| floor.plane.distance(v).0.abs() > 1.0));
}

#[test]
fn the_walk_from_inside_closed_rooms() {
    let Some(mut w) = world() else { return };
    let all = |_: usize| Some(F::<M>::new(500.0));
    let mut behind_walls = 0;
    let mut closed_only = 0;
    // (zone, room, eye): Freeport's building by the docks, and zone 83's house
    for (z, room, eye) in [(84usize, 27usize, [25488.0f32, 62.0, 15629.0]), (83, 99, [23189.0, 75.0, 15699.0]), (83, 74, [23190.0, 50.0, 15900.0])] {
        assert_eq!(w.find_leaf(&Vect3::of(eye[0], eye[1], eye[2])).map(|(zz, l)| (zz, w.zones[zz].leaves[l].0 as usize)), Some((z, room)), "the eye in zone {z} room {room}");
        let occupied = rooms_of(&w, z);
        // the closed rooms the portals reach from it, and whether one of them
        // opens on the outside
        let mut reach = vec![room];
        let mut k = 0;
        let mut outside = false;
        while k < reach.len() {
            for p in &w.zones[z].rooms[reach[k]].portal_list {
                let d = p.dest as usize;
                if w.zones[z].rooms[d].flags & 1 != 0 {
                    outside = true;
                } else if !reach.contains(&d) {
                    reach.push(d);
                }
            }
            k += 1;
        }
        for step in 0..48 {
            let yaw = step as f32 * std::f32::consts::TAU / 16.0;
            let pitch = [-0.7f32, 0.0, 0.6][step / 16];
            let cam = camera(eye, yaw, pitch, 300.0);
            let got = w.raster_actors(&cam, &cam.position, &all);
            let again = w.raster_actors(&cam, &cam.position, &all);
            assert_eq!(got, again, "zone {z} room {room} step {step}: the same twice");
            let mut seen = std::collections::HashSet::new();
            let mut reached_outside = false;
            for &a in &got {
                assert!(seen.insert(a), "rastered twice");
                let ac = &w.actors[a];
                assert!(in_view(&cam, &ac.bound), "zone {z} room {room} step {step}: actor {a} out of view");
                let d = ac.location.distance(&cam.position).0;
                assert!(d < 500.0, "zone {z} room {room} step {step}: actor {a} at {d}");
                let in_reach = occupied.get(&a).is_some_and(|rs| rs.iter().any(|r| reach.contains(r)));
                if !in_reach {
                    reached_outside = true;
                    assert!(outside, "zone {z} room {room} step {step}: actor {a} beyond the reach of the portals, with no way out");
                }
            }
            if !reached_outside {
                closed_only += 1;
                // the control: the frustum alone takes actors of rooms the
                // portals do not reach (behind the walls)
                let frustum_only = w.actors.iter().enumerate().filter(|(i, ac)| ac.flags & 5 == 0 && ac.sprite.is_some() && in_view(&cam, &ac.bound) && ac.location.distance(&cam.position).0 < 300.0 && !got.contains(i)).count();
                if frustum_only > 0 {
                    behind_walls += 1;
                }
            }
        }
    }
    eprintln!("{closed_only} of 144 views stayed inside; in {behind_walls} the frustum alone took actors behind the walls");
    assert!(closed_only >= 20, "views that stayed inside: {closed_only}");
    assert!(behind_walls >= 20, "views where the walls hid what the frustum alone takes: {behind_walls}");
}

#[test]
fn the_walk_from_outdoors() {
    let Some(mut w) = world() else { return };
    let all = |_: usize| Some(F::<M>::new(500.0));
    let eye = [25330.0f32, 56.0, 15721.0];
    let (z, leaf) = w.find_leaf(&Vect3::of(eye[0], eye[1], eye[2])).unwrap();
    let room = w.zones[z].leaves[leaf].0 as usize;
    assert_ne!(w.zones[z].rooms[room].flags & 1, 0, "the docks are outdoors");
    let own: Vec<usize> = w.zones[z].rooms[room].actors.clone();
    let mut checked = 0;
    for step in 0..16 {
        let cam = camera(eye, step as f32 * std::f32::consts::TAU / 16.0, -0.1, 400.0);
        let got = w.raster_actors(&cam, &cam.position, &all);
        for &a in &got {
            assert!(in_view(&cam, &w.actors[a].bound), "step {step}: actor {a} out of view");
        }
        for &a in &own {
            let ac = &w.actors[a];
            if ac.flags & 5 == 0 && ac.sprite.is_some() && in_view(&cam, &ac.bound) && ac.location.distance(&cam.position).0 < 400.0 {
                assert!(got.contains(&a), "step {step}: the camera's own room's actor {a} in view, not rastered");
                checked += 1;
            }
        }
    }
    eprintln!("{checked} of the camera's own room's actors in view, every one rastered");
    assert!(checked > 100, "own-room actors checked: {checked}");
}
