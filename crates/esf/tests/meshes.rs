//! Every model's geometry against `go-eqoa-esf-tools-v2`'s `cmd/meshcrc`
//! (`esfviewer/expected-mesh.js`): DictID, vertex, triangle and part counts,
//! the bounding box as raw f32 bits, and the CRC32 of the positions. meshcrc
//! lists SimpleSprite, GroupSprite, CSprite and LODSprite only, so HSprites are
//! held out (and counted).

mod common;

#[test]
fn meshes_match_reference_parser() {
    let Some((mut disc, expect)) = common::inputs("expected-mesh.js") else {
        return;
    };
    for (name, rows) in &expect {
        let c = common::container(&mut disc, name);
        let mut held_out = 0;
        let mut got: Vec<String> = Vec::new();
        for r in c.models() {
            let Some(m) = c.build_model(r.node) else { continue };
            if r.ty == esf::types::HSPRITE {
                held_out += 1;
                continue;
            }
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            let mut h = crc32fast::Hasher::new();
            for p in &m.pos {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                    h.update(&p[k].to_le_bytes());
                }
            }
            let bbox: Vec<String> = lo.iter().chain(&hi).map(|v| format!("{:08x}", v.to_bits())).collect();
            got.push(format!(
                "{} {} {} {} {} {:08x}",
                r.dict_id,
                m.pos.len(),
                m.triangles,
                m.parts,
                bbox.join(","),
                h.finalize()
            ));
        }
        let key = |s: &String| {
            let f: Vec<&str> = s.split(' ').collect();
            (f[0].parse::<i64>().unwrap(), f[5].to_string())
        };
        got.sort_by_key(key);
        let want: Vec<String> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let a = r.as_array().unwrap();
                format!("{} {} {} {} {} {}", a[0], a[1], a[2], a[3], a[4].as_str().unwrap(), a[5].as_str().unwrap())
            })
            .collect();
        let bad: Vec<String> = got
            .iter()
            .zip(&want)
            .filter(|(g, w)| g != w)
            .take(5)
            .map(|(g, w)| format!("got  {g}\nwant {w}"))
            .collect();
        assert!(bad.is_empty() && got.len() == want.len(), "{name}: {} vs {} models\n{}", got.len(), want.len(), bad.join("\n"));
        eprintln!("{name}: {} models match ({held_out} HSprites held out)", got.len());
    }
}
