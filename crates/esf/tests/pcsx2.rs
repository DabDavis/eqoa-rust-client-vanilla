//! PCSX2's names (`esf::pcsx2`) against PCSX2's own dumps of the game: a
//! dumps folder PCSX2 wrote with Dump Textures on (`$EQOA_PCSX2_DUMPS`, its
//! `textures/SLUS-20744/dumps`), every file in it looked for among the names
//! made for every surface on the disc, and the pixels of each one found held
//! to the surface's own (`GsTexture::rgba`), level for level. A dump no
//! surface names is listed with what it is (the fonts' glyph caches are
//! filled as the game runs). Without the folder it says so and passes.
//!
//! Inputs: the disc (`tests/common/mod.rs`'s `$EQOA_ISO`), the dumps.

use std::collections::{BTreeMap, HashMap};

/// What a name was made for: the file, the DictID, the level the hashed
/// chain starts at, and how many levels it holds.
#[derive(Clone, Debug)]
struct Made {
    file: String,
    dict_id: i32,
    from: usize,
    count: usize,
    tex: std::rc::Rc<esf::pcsx2::GsTexture>,
}

fn disc() -> Option<iso::Iso> {
    let p = std::env::var_os("EQOA_ISO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso"));
    if !p.exists() {
        eprintln!("SKIP: no disc (set EQOA_ISO)");
        return None;
    }
    Some(iso::Iso::open(p).unwrap())
}

/// Every name PCSX2 can give every surface on the disc (`GsTexture::names`)
/// whose palette is one of `cluts` (a dump's CLUT hash: the others cannot
/// name one, and hashing every level of 42,000 surfaces is slow).
fn every_name(disc: &mut iso::Iso, cluts: &std::collections::HashSet<u64>) -> HashMap<String, Made> {
    let files: Vec<String> = disc
        .entries()
        .iter()
        .map(|e| e.path.clone())
        .filter(|n| n.to_uppercase().ends_with(".ESF") || n.to_uppercase().ends_with(".CSF"))
        .collect();
    let mut out = HashMap::new();
    for f in &files {
        let Ok(c) = esf::Container::open(disc.read(f).unwrap()) else { continue };
        for s in &c.surfaces {
            if !s.gs_clut_hash(&c.data).is_some_and(|h| cluts.contains(&h)) {
                continue;
            }
            let Some(t) = s.gs(&c.data) else { continue };
            let t = std::rc::Rc::new(t);
            let top = t.top_level();
            for (n, from) in t.names() {
                let count = if n == t.name(0, 0) && from == 0 { 1 } else { top - from + 1 };
                out.entry(n.file_name(0)).or_insert(Made { file: f.clone(), dict_id: s.dict_id, from, count, tex: t.clone() });
            }
        }
    }
    out
}

/// A dump's texture's file name and the level it is: a dumped mip level is
/// its texture's name with `-mip<n>` before the extension.
fn texture_of(d: &str) -> (String, usize) {
    match d.rfind("-mip") {
        Some(i) => (format!("{}.png", &d[..i]), d[i + 4..d.len() - 4].parse::<usize>().unwrap()),
        None => (d.to_string(), 0),
    }
}

fn read_png(p: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let mut d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()));
    d.set_transformations(png::Transformations::EXPAND);
    let mut r = d.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba, "{}: PCSX2 dumps RGBA", p.display());
    (info.width, info.height, buf)
}

#[test]
fn names_match_pcsx2_dumps() {
    let Some(dir) = std::env::var_os("EQOA_PCSX2_DUMPS").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: set EQOA_PCSX2_DUMPS to a folder of PCSX2's dumps of the game");
        return;
    };
    let Some(mut disc) = disc() else { return };
    let mut dumps: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".png")).collect();
    dumps.sort();
    let cluts = dumps.iter().filter_map(|d| esf::pcsx2::Name::parse(&texture_of(d).0)).map(|n| n.clut_hash).collect();
    let names = every_name(&mut disc, &cluts);
    let (mut matched, mut pixels_equal, mut by_kind) = (0, 0, BTreeMap::<String, usize>::new());
    let mut unmatched: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in &dumps {
        let (stem, mip) = texture_of(d);
        let name = esf::pcsx2::Name::parse(&stem).unwrap_or_else(|| panic!("{d}: not a name PCSX2 reads"));
        let what = format!("psm {} {}x{}", name.psm(), name.width(), name.height());
        let Some(m) = names.get(&stem) else {
            unmatched.entry(what).or_default().push(d.clone());
            continue;
        };
        matched += 1;
        let kind = if m.count == 1 && m.from == 0 { "the base alone".to_string() } else { format!("levels {} to {}", m.from, m.from + m.count - 1) };
        if std::env::var_os("EQOA_PCSX2_LIST").is_some() {
            eprintln!("  {d}: {} {:08x} psm {} {}x{} {kind}", m.file, m.dict_id as u32, m.tex.psm, m.tex.levels[0].width, m.tex.levels[0].height);
        }
        *by_kind.entry(if mip > 0 { format!("{kind}, a dumped level") } else { kind }).or_default() += 1;
        let (w, h, rgba) = read_png(&dir.join(d));
        let level = m.from + mip;
        let l = &m.tex.levels[level];
        assert_eq!((w, h), (l.width, l.height), "{d} ({} {:08x} level {level}): size", m.file, m.dict_id);
        if rgba == m.tex.rgba(level) {
            pixels_equal += 1;
        } else {
            panic!("{d} ({} {:08x} level {level}): its pixels are not the surface's", m.file, m.dict_id);
        }
    }
    eprintln!("{} dumps: {matched} named by a surface, every one's pixels the surface's ({pixels_equal})", dumps.len());
    for (k, n) in &by_kind {
        eprintln!("  {k}: {n}");
    }
    for (what, ds) in &unmatched {
        eprintln!("  not a surface's: {what}: {} (e.g. {})", ds.len(), ds[0]);
    }
    assert!(matched > 0, "no dump is named by a surface");
}

/// The names of a planted surface against the hash worked by hand, and
/// what PCSX2's name parsing takes and leaves.
#[test]
fn file_names_as_pcsx2_reads_them() {
    use esf::pcsx2::Name;
    let n = Name { tex0_hash: 0x0438_4101_795a_b542, clut_hash: 0xf9c1_4f4d_49b9_6086, region: (0, 0), bits: 0x2653 };
    assert_eq!(n.file_name(0), "4384101795ab542-f9c14f4d49b96086-00002653.png", "hashes unpadded, bits eight digits");
    assert_eq!(n.file_name(2), "4384101795ab542-f9c14f4d49b96086-00002653-mip2.png");
    assert_eq!((n.psm(), n.width(), n.height()), (19, 512, 512));
    assert_eq!(Name::parse(&n.file_name(0)), Some(n));
    assert_eq!(Name::parse("4384101795ab542-f9c14f4d49b96086-00002653.dds"), Some(n), "any extension after the dot");
    assert_eq!(Name::parse(&n.file_name(2)), None, "a dumped level is not a replacement");
    assert_eq!(Name::parse("4384101795ab542-f9c14f4d49b96086-00002653"), None, "no dot");
    // no palette: two fields; a region; bit 14 (once TCC) dropped
    let direct = Name { tex0_hash: 0xabc, clut_hash: 0, region: (0, 0), bits: 0x1580 };
    assert_eq!(direct.file_name(0), "abc-00001580.png");
    assert_eq!(Name::parse("abc-00001580.png"), Some(direct));
    assert_eq!(Name::parse("abc-00005580.png"), Some(direct), "the old TCC bit is not part of a name");
    let r = Name { tex0_hash: 1, clut_hash: 2, region: (100, 64), bits: 0x1593 };
    assert_eq!(r.file_name(0), "1-2-r100x64-00001593.png");
    assert_eq!(Name::parse(&r.file_name(0)), Some(r));
    assert_eq!(Name::parse("1-r100x64-00001580.png"), Some(Name { tex0_hash: 1, clut_hash: 0, region: (100, 64), bits: 0x1580 }));
    // the old region form: its bits are min x, max x, min y, max y (16 each)
    let old = (5u64) | (105 << 16) | (2 << 32) | (66 << 48);
    assert_eq!(Name::parse(&format!("1-2-r{old:x}-00001593.png")).map(|n| n.region), Some((100, 64)));
}
