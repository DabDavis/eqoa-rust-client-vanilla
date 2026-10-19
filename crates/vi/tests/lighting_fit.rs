//! Each lit actor's colour buffers against its simple sprites, in the order
//! the pick hands them out (GetNextColorBuffer): a buffer's colour count must
//! equal that sprite's vertex count. The control: each actor's sprites
//! against the NEXT lit actor's buffers must mostly not fit. (Every lit
//! actor in TUNARIA has one buffer and one sprite.)

use vi::fpu::Ieee;
use vi::world::{SpriteKind, World};

fn order(w: &World<Ieee>, s: usize, out: &mut Vec<usize>) {
    match &w.sprites[s].kind {
        SpriteKind::Simple(_) => out.push(s),
        SpriteKind::Group(m) => m.iter().for_each(|&(ms, ..)| order(w, ms, out)),
        SpriteKind::Lod(l) => {
            if let Some(&(f, _)) = l.first() {
                order(w, f, out)
            }
        }
        _ => {}
    }
}

#[test]
fn colour_buffers_fit_their_sprites() {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let w = World::<Ieee>::load(&lib, f).unwrap();
    let (mut fit, mut miss, mut nobuf) = (0, 0, 0);
    let (mut rfit, mut rmiss) = (0, 0);
    let mut prev: Option<Vec<usize>> = None;
    let mut shown = 0;
    for a in &w.actors {
        let (Some(l), Some(s)) = (a.lighting, a.sprite) else { continue };
        let mut sprites = Vec::new();
        order(&w, s, &mut sprites);
        let bufs = &w.lightings[l];
        if let Some(p) = prev.take() {
            for (k, &sp) in p.iter().enumerate() {
                let v = w.sprites[sp].pick.map_or(0, |p| w.picks[p].strips.iter().map(|s| s.verts.len()).sum::<usize>());
                match bufs.get(k) {
                    Some(b) if b.len() == v => rfit += 1,
                    _ => rmiss += 1,
                }
            }
        }
        prev = Some(sprites.clone());
        for (k, &sp) in sprites.iter().enumerate() {
            let verts = w.sprites[sp].pick.map(|p| w.picks[p].strips.iter().map(|s| s.verts.len()).sum::<usize>());
            match (w.lightings[l].get(k), verts) {
                (Some(b), Some(v)) if b.len() == v => fit += 1,
                (Some(b), v) => {
                    miss += 1;
                    if shown < 10 {
                        shown += 1;
                        eprintln!("actor sprite {:08X}: buffer {k} has {} colours, sprite {:?} verts, {} buffers / {} sprites", a.sprite_id, b.len(), v, w.lightings[l].len(), sprites.len());
                    }
                }
                (None, _) => nobuf += 1,
            }
        }
    }
    eprintln!("{fit} fit, {miss} do not, {nobuf} sprites past the buffers; shifted one actor: {rfit} fit, {rmiss} do not");
    assert!(miss * 50 < fit);
    assert!(rfit * 10 < (rfit + rmiss) * 7, "the actor association is not what makes them fit");
}
