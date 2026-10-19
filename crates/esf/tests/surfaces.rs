//! Every decoded surface against `go-eqoa-esf-tools-v2`'s fixtures
//! (`esfviewer/expected.js`: DictID, size, CRC32 of the RGBA, alpha flag) —
//! the same check the browser tool and the Blender importer run. The
//! containers are read straight off the disc.
//!
//! Needs the disc at `$EQOA_ISO` (default `run/eqoa.iso`) and the fixture at
//! `$EQOA_EXPECT` (default `../esfviewer/expected.js`); without them it says so
//! and passes, so a fresh clone still builds.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn input(var: &str, default: &str) -> Option<PathBuf> {
    let p = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| root().join(default));
    if p.exists() {
        Some(p)
    } else {
        eprintln!("SKIP: {} not found (set {var})", p.display());
        None
    }
}

#[test]
fn surfaces_match_reference_parser() {
    let (Some(iso_path), Some(expect_path)) =
        (input("EQOA_ISO", "run/eqoa.iso"), input("EQOA_EXPECT", "../esfviewer/expected.js"))
    else {
        return;
    };
    let text = std::fs::read_to_string(expect_path).unwrap();
    let json = &text[text.find('=').unwrap() + 1..=text.rfind('}').unwrap()];
    let expect: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json).unwrap();

    let mut disc = iso::Iso::open(iso_path).unwrap();
    for (name, rows) in &expect {
        let entry = disc
            .entries()
            .iter()
            .find(|e| e.path.rsplit('/').next().unwrap().eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} not on the disc"))
            .clone();
        let c = esf::Container::open(disc.read_entry(&entry).unwrap()).unwrap();
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
