//! Heroic Nagafen's Cinder Brand. The encounter sends the final position after
//! moving it off the valves, along with the same radius it uses to burn players.
use net::stream::Reader;
use render::billboard::Strips;
use vi::coll::{CollRay, EnvRay};
use vi::fpu::Ee;
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::world::World;
use web_time::{Duration, Instant};

pub const OPCODE: u16 = 0x7EC4;
const SEGMENTS: usize = 96;
const DARK: [f32; 4] = [0.012, 0.023, 0.055, 1.0];
const BRIGHT: [f32; 4] = [0.56, 1.0, 1.0, 1.0];

pub struct Pool {
    world: u8,
    instance: u32,
    center: [f32; 3],
    radius: f32,
    until: Instant,
    mesh: Option<Strips>,
}

impl Pool {
    pub fn receive(current: &mut Option<Self>, data: &[u8], now: Instant) {
        let mut r = Reader::new(data);
        let (world, instance, left) = (r.u8(), r.u32(), r.u32());
        if r.error || instance == 0 {
            return;
        }
        if left == 0 {
            if r.pos == data.len() && current.as_ref().is_some_and(|p| (p.world, p.instance) == (world, instance)) {
                *current = None;
            }
            return;
        }
        let center = [r.f32(), r.f32(), r.f32()];
        let radius = r.f32();
        if r.error || r.pos != data.len() || !center.iter().all(|v| v.is_finite()) || !radius.is_finite() || radius <= 0.0 {
            return;
        }
        *current = Some(Self { world, instance, center, radius, until: now + Duration::from_millis(left.into()), mesh: None });
    }

    fn visible(current: &mut Option<Self>, world: i64, now: Instant) -> Option<&mut Self> {
        if current.as_ref().is_some_and(|p| now >= p.until) {
            *current = None;
        }
        current.as_mut().filter(|p| i64::from(p.world) == world)
    }

    pub fn draw(current: &mut Option<Self>, scene: &mut World<Ee>, world: i64, strips: &mut Vec<Strips>) {
        let Some(pool) = Self::visible(current, world, Instant::now()) else { return };
        let [x, y, z] = pool.center;
        if pool.mesh.is_none() && scene.find_leaf(&Vect3::of(x, y, z)).is_none() {
            return; // the island's collision has not streamed in yet
        }
        let mesh = pool.mesh.get_or_insert_with(|| boundary(pool.center, pool.radius, |x, z| surface(scene, x, pool.center[1], z)));
        strips.push(mesh.clone());
    }
}

// Near the pool's elevation, use the higher of solid ground and the lava skin.
// WalkFloor ignores characters. A rock ledge overhead must not pull the ring to the roof.
fn surface(scene: &mut World<Ee>, x: f32, y: f32, z: f32) -> f32 {
    let from = Vect3::of(x, y + 4.0, z);
    let to = Vect3::of(x, y - 4.0, z);
    let mut solid = CollRay::default();
    solid.init(from, to);
    let mut filter = Filter::new(FilterKind::WalkFloor, -1);
    let floor = scene.collide_ray(&mut solid, &mut filter).then(|| solid.hit_point().y.0);
    let mut lava = EnvRay::default();
    lava.init(from, to, 4);
    let liquid = scene.collide_env(&mut lava).then(|| lava.hit_point().y.0);
    match (floor, liquid) {
        (Some(a), Some(b)) => a.max(b),
        (Some(h), None) | (None, Some(h)) => h,
        _ => y,
    }
}

// All three bands are on the dangerous side. The outer vertices sit on the
// server's circle. Lift them slightly to avoid z-fighting on shallow slopes.
fn boundary(center: [f32; 3], radius: f32, mut height: impl FnMut(f32, f32) -> f32) -> Strips {
    let width = 0.85_f32.min(radius * 0.25);
    let mut bands = vec![Vec::with_capacity((SEGMENTS + 1) * 2); 3];
    for step in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * (step % SEGMENTS) as f32 / SEGMENTS as f32;
        let (s, c) = angle.sin_cos();
        let mut points = [[0.0; 3]; 4];
        for (p, inset) in points.iter_mut().zip([1.0, 0.76, 0.24, 0.0]) {
            let r = radius - inset * width;
            let (x, z) = (center[0] + r * c, center[2] + r * s);
            *p = [x, height(x, z) + 0.12, z];
        }
        for (band, pair) in bands.iter_mut().zip(points.windows(2)) {
            band.extend([([0.0; 2], pair[0]), ([0.0; 2], pair[1])]);
        }
    }
    let colors = [DARK, BRIGHT, DARK].into_iter().map(|color| vec![color; (SEGMENTS + 1) * 2]).collect();
    Strips { texture: 0, blend: 0, ztest: true, zwrite: false, color: [1.0; 4], strips: bands, colors }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(instance: u32, left: u32) -> Vec<u8> {
        let mut data = vec![18];
        data.extend(instance.to_le_bytes());
        data.extend(left.to_le_bytes());
        if left > 0 {
            for value in [4816.0_f32, -28.64, 4370.0, 10.0] {
                data.extend(value.to_le_bytes());
            }
        }
        data
    }

    #[test]
    fn boundary_lifetime_and_instance() {
        let now = Instant::now();
        let mut pool = None;
        Pool::receive(&mut pool, &message(7, 7000), now);
        let marker = pool.as_ref().unwrap();
        assert_eq!((marker.world, marker.instance), (18, 7));
        assert_eq!((marker.center, marker.radius), ([4816.0, -28.64, 4370.0], 10.0));
        assert_eq!(marker.until, now + Duration::from_secs(7));
        Pool::receive(&mut pool, &message(6, 0), now); // an old instance finishing
        assert!(Pool::visible(&mut pool, 0, now).is_none());
        assert!(Pool::visible(&mut pool, 18, now + Duration::from_millis(6999)).is_some());
        Pool::receive(&mut pool, &message(7, 0), now);
        assert!(pool.is_none());
        Pool::receive(&mut pool, &message(7, 1000), now - Duration::from_secs(2));
        assert!(Pool::visible(&mut pool, 18, now).is_none());
        assert!(pool.is_none());
        let mut invalid = message(7, 12000);
        invalid[21..25].copy_from_slice(&f32::NAN.to_le_bytes());
        Pool::receive(&mut pool, &invalid, now);
        Pool::receive(&mut pool, &invalid[..17], now);
        assert!(pool.is_none());
    }

    #[test]
    fn cinder_rim_matches_the_advertised_circle_and_follows_the_floor() {
        for r in [5.0, 10.0, 12.5] {
            let mesh = boundary([4816.0, -28.64, 4370.0], r, |x, z| -28.64 + 0.1*(x-4816.0) + 0.2*(z-4370.0));
            assert!(mesh.ztest && !mesh.zwrite);
            assert_eq!([mesh.colors[0][0], mesh.colors[1][0], mesh.colors[2][0]], [DARK, BRIGHT, DARK]);
            for band in &mesh.strips {
                assert_eq!(band[..2], band[band.len()-2..]);
                for (_, [x, y, z]) in band {
                    assert!((x-4816.0).hypot(z-4370.0) <= r+0.001);
                    assert!((y - (-28.52 + 0.1*(x-4816.0) + 0.2*(z-4370.0))).abs() < 0.001);
                }
            }
            for (_, [x, _, z]) in mesh.strips[2].iter().skip(1).step_by(2) {
                assert!(((x-4816.0).hypot(z-4370.0) - r).abs() < 0.001);
            }
        }
    }
}
