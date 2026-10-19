//! Every decoded surface against `go-eqoa-esf-tools-v2`'s fixtures
//! (`esfviewer/expected.js`: DictID, size, CRC32 of the RGBA, alpha flag) —
//! the same check the browser tool and the Blender importer run. The
//! containers are read straight off the disc.
//!
//! Inputs: `tests/common/mod.rs`.

mod common;

#[test]
fn surfaces_match_reference_parser() {
    let Some((mut disc, expect)) = common::inputs("expected.js") else {
        return;
    };
    for (name, rows) in &expect {
        let c = common::container(&mut disc, name);
        let mut got: Vec<(i64, String, String, bool)> = c
            .surfaces
            .iter()
            .filter_map(|s| {
                let img = s.decode(&c.data)?;
                Some((
                    s.dict_id as i64,
                    format!("{}x{}", s.width, s.height),
                    format!("{:08x}", crc32fast::hash(&img.rgba)),
                    img.alpha,
                ))
            })
            .collect();
        got.sort_by(|a, b| (a.0, &a.2).cmp(&(b.0, &b.2)));
        let want: Vec<(i64, String, String, bool)> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r[0].as_i64().unwrap(),
                    r[1].as_str().unwrap().into(),
                    r[2].as_str().unwrap().into(),
                    r[3].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(got.len(), want.len(), "{name}: surface count");
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "{name}[{i}]");
        }
        eprintln!("{name}: {} surfaces, {} records, all CRCs match", got.len(), c.nodes.len());
    }
}
