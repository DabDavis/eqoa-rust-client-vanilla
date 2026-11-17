//! The zone actors round a place, each with its triangles in world coordinates
//! (`eqoa actormesh`): what go-eqoa-server's `tools/decor_fit.py` fits a string
//! of decoration lights to (the owner, 2026-11-16: "we also need to make the
//! christmas lights able to be put on any model. in freeport we can decorate put
//! lights on the fence/low wall near freeport coach, the well, etc."). The fit
//! is the tool's; this reads the disc the operator has, so no art leaves it.

use std::fmt::Write;

/// Every actor of `world` (a world file of the disc at `iso`) whose place is
/// within `r` of (`x`, `z`) and, with `only`, whose sprite is that DictID, as
/// JSON: `{"actors": [{"zone", "sprite", "pos", "rot", "scale", "tris": [[x,y,z]
/// x3, ...]}]}`, its triangles placed by `esf::zone::Actor::matrix`.
pub fn actor_meshes(iso: &str, world: &str, x: f32, z: f32, r: f32, only: Option<i32>) -> Result<String, Box<dyn std::error::Error>> {
    let disc = iso::Iso::open(std::path::Path::new(iso))?;
    let e = disc.find(world).ok_or("no such world on the disc")?.clone();
    let mut w = esf::Window::new(iso::open_file(std::path::Path::new(iso))?, e.lba as u64 * iso::SECTOR, e.size as u64);
    let def = esf::Container::open_deferred(&mut w)?;
    let mut out = String::from("{\"actors\":[");
    let mut first = true;
    for (zi, &zn) in def.zones().iter().enumerate() {
        let Some((off, len)) = def.node(zn).deferred else { continue };
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut w, off, &mut b)?;
        let c = esf::Container::parse_records(b);
        let Some(&root) = c.roots.first() else { continue };
        for a in c.actors(root) {
            let (dx, dz) = (a.pos[0] - x, a.pos[2] - z);
            if dx * dx + dz * dz > r * r || only.is_some_and(|o| o != a.sprite) {
                continue;
            }
            let Some(&n) = c.dict.get(&a.sprite) else { continue };
            let Some(m) = c.build_model(n) else { continue };
            let mx = a.matrix();
            let place = |p: [f32; 3]| [p[0] * mx[0][0] + p[1] * mx[1][0] + p[2] * mx[2][0] + mx[3][0], p[0] * mx[0][1] + p[1] * mx[1][1] + p[2] * mx[2][1] + mx[3][1], p[0] * mx[0][2] + p[1] * mx[1][2] + p[2] * mx[2][2] + mx[3][2]];
            if !first {
                out.push(',');
            }
            first = false;
            // each batch's texture (its base layer's DictID) and how many triangles it has
            let tex: Vec<String> = m.batches.iter().map(|bt| format!("[\"{:#010X}\",{}]", bt.texture() as u32, bt.indices.len() / 3)).collect();
            write!(out, "{{\"zone\":{zi},\"sprite\":\"{:#010X}\",\"pos\":[{},{},{}],\"rot\":{},\"scale\":{},\"textures\":[{}],\"tris\":[", a.sprite as u32, a.pos[0], a.pos[1], a.pos[2], a.rot[0], a.scale, tex.join(","))?;
            let mut t1 = true;
            for bt in &m.batches {
                for tri in bt.indices.chunks(3) {
                    if tri.len() < 3 {
                        continue;
                    }
                    if !t1 {
                        out.push(',');
                    }
                    t1 = false;
                    let v: Vec<[f32; 3]> = tri.iter().map(|&i| place(m.pos[i as usize])).collect();
                    write!(out, "[[{:.3},{:.3},{:.3}],[{:.3},{:.3},{:.3}],[{:.3},{:.3},{:.3}]]", v[0][0], v[0][1], v[0][2], v[1][0], v[1][1], v[1][2], v[2][0], v[2][1], v[2][2])?;
                }
            }
            out.push_str("]}");
        }
    }
    out.push_str("]}\n");
    Ok(out)
}

/// Which zones of `world` hold each of `ids` (a texture's or a sprite's
/// DictID), as JSON `{"0xID": [zone, ...]}`: whether art seen in one place is
/// in the zones of another (a tent's cloth of Odus in Tunaria's).
pub fn dict_zones(iso: &str, world: &str, ids: &[i32]) -> Result<String, Box<dyn std::error::Error>> {
    let disc = iso::Iso::open(std::path::Path::new(iso))?;
    let e = disc.find(world).ok_or("no such world on the disc")?.clone();
    let mut w = esf::Window::new(iso::open_file(std::path::Path::new(iso))?, e.lba as u64 * iso::SECTOR, e.size as u64);
    let def = esf::Container::open_deferred(&mut w)?;
    let mut found: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    // the world file's own records (its resident art) as zone -1
    let mut resident = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        if def.dict.contains_key(id) {
            resident.push(i);
        }
    }
    for (zi, &zn) in def.zones().iter().enumerate() {
        let Some((off, len)) = def.node(zn).deferred else { continue };
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut w, off, &mut b)?;
        let c = esf::Container::parse_records(b);
        for (i, id) in ids.iter().enumerate() {
            if c.dict.contains_key(id) {
                found[i].push(zi);
            }
        }
    }
    let mut out = String::from("{");
    for (i, id) in ids.iter().enumerate() {
        let zones: Vec<String> = resident.contains(&i).then(|| "-1".to_string()).into_iter().chain(found[i].iter().map(|z| z.to_string())).collect();
        write!(out, "{}\"{:#010X}\":[{}]", if i > 0 { "," } else { "" }, *id as u32, zones.join(","))?;
    }
    out.push_str("}\n");
    Ok(out)
}

/// Texture `id` of zone `zone` of `world` (-1: the file's resident records)
/// as a PNG at `out`, its first level: a contact sheet's picture of it.
pub fn texture_png(iso: &str, world: &str, zone: i64, id: i32, out: &str) -> Result<String, Box<dyn std::error::Error>> {
    let disc = iso::Iso::open(std::path::Path::new(iso))?;
    let e = disc.find(world).ok_or("no such world on the disc")?.clone();
    let mut w = esf::Window::new(iso::open_file(std::path::Path::new(iso))?, e.lba as u64 * iso::SECTOR, e.size as u64);
    let def = esf::Container::open_deferred(&mut w)?;
    let c = if zone == -2 {
        // a resource file (CHAR.ESF) read whole
        let mut b = vec![0u8; e.size as usize];
        esf::ReadAt::read_at(&mut w, 0, &mut b)?;
        esf::Container::open(b)?
    } else if zone < 0 {
        def
    } else {
        let zn = *def.zones().get(zone as usize).ok_or("no such zone")?;
        let (off, len) = def.node(zn).deferred.ok_or("zone not deferred")?;
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut w, off, &mut b)?;
        esf::Container::parse_records(b)
    };
    let s = c.surface(id).ok_or("no such texture there")?;
    let img = s.decode(&c.data).ok_or("texture does not decode")?;
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&img.rgba)?;
    }
    std::fs::write(out, png_bytes)?;
    // its mean colour, for a tint's gain
    let n = (img.rgba.len() / 4).max(1) as f64;
    let mean: Vec<f64> = (0..3).map(|k| img.rgba.chunks(4).map(|p| p[k] as f64).sum::<f64>() / n).collect();
    Ok(format!("{out}: {}x{} mean {:.0} {:.0} {:.0}", img.width, img.height, mean[0], mean[1], mean[2]))
}

#[cfg(test)]
mod tests {
    /// Freeport's well (0x5D88E0B5, zone 84 at 25296.7, 15750.1), off the disc
    /// when one is at hand: its triangles placed round its spot, at its foot's
    /// height and up to its roof.
    #[test]
    fn the_well_by_freeports_boards() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !p.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let json = super::actor_meshes(p.to_str().unwrap(), "TUNARIA.ESF", 25296.7, 15750.1, 2.0, Some(0x5D88_E0B5)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let actors = v["actors"].as_array().unwrap();
        assert_eq!(actors.len(), 1, "{json:.200}");
        assert_eq!(actors[0]["zone"], 84);
        let tris = actors[0]["tris"].as_array().unwrap();
        assert!(tris.len() > 100);
        let ys: Vec<f64> = tris.iter().flat_map(|t| t.as_array().unwrap().iter().map(|p| p[1].as_f64().unwrap())).collect();
        let (lo, hi) = ys.iter().fold((f64::MAX, f64::MIN), |(a, b), &y| (a.min(y), b.max(y)));
        assert!((lo - 54.0).abs() < 0.5 && (hi - 62.5).abs() < 0.6, "{lo} .. {hi}");
        let xs: Vec<f64> = tris.iter().flat_map(|t| t.as_array().unwrap().iter().map(|p| p[0].as_f64().unwrap())).collect();
        assert!(xs.iter().all(|&x| (x - 25296.7).abs() < 6.0));
    }
}
