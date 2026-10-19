//! Inputs for the fixture tests: the disc at `$EQOA_ISO` (default
//! `run/eqoa.iso`) and the fixture directory at `$EQOA_EXPECT` (default
//! `../esfviewer`). Without them a test says so and passes, so a fresh clone
//! still builds.

use std::path::PathBuf;

pub type Expect = serde_json::Map<String, serde_json::Value>;

fn path(var: &str, default: &str) -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| root.join(default));
    if p.exists() {
        Some(p)
    } else {
        eprintln!("SKIP: {} not found (set {var})", p.display());
        None
    }
}

/// The disc, and `window.X = {...}` out of the named fixture file.
pub fn inputs(fixture: &str) -> Option<(iso::Iso, Expect)> {
    let iso_path = path("EQOA_ISO", "run/eqoa.iso")?;
    let dir = path("EQOA_EXPECT", "../esfviewer")?;
    let text = std::fs::read_to_string(dir.join(fixture)).ok()?;
    let json = &text[text.find('=')? + 1..=text.rfind('}')?];
    Some((iso::Iso::open(iso_path).unwrap(), serde_json::from_str(json).unwrap()))
}

pub fn container(disc: &mut iso::Iso, name: &str) -> esf::Container {
    let bytes = disc.read(name).unwrap_or_else(|e| panic!("{name}: {e}"));
    esf::Container::open(bytes).unwrap()
}
