//! The client binary. For now, the disc and container inspection commands the
//! first slice is built on:
//!
//! ```text
//! eqoa ls   [iso]               every file on the disc
//! eqoa tree [iso] <file>        a container's record tree
//! eqoa zones <world.esf>        the zones in a world file
//! eqoa view <file> [model]       a model (wgpu + winit)
//! eqoa view <world> zone <n>     a zone: terrain and every placed actor
//! eqoa view <world> walk <n>     the character, walking in a zone
//! eqoa shot <file> [model | zone <n>] <out.png> [camera]   one view, offscreen
//! ```
//!
//! A model is a list index or a DictID in hex (`0x…`). A `shot` camera is
//! `yaw pitch zoom` for a model, `x y z yaw pitch` for a zone, and for a walk
//! a script, `ms [fwd strafe orbit pitch]`, held for that much game time.
//!
//! `iso` defaults to `$EQOA_ISO`, then `run/eqoa.iso`.

mod scene;
mod sky;
mod stream;
mod viewer;
mod walker;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eqoa: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn disc(arg: Option<&String>) -> Result<iso::Iso> {
    let path = arg
        .cloned()
        .or_else(|| std::env::var("EQOA_ISO").ok())
        .unwrap_or_else(|| "run/eqoa.iso".into());
    Ok(iso::Iso::open(&path).map_err(|e| format!("{path}: {e}"))?)
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("ls") => {
            for e in disc(args.get(1))?.entries() {
                println!("{:>10}  {:>7}  {}", e.size, e.lba, e.path);
            }
        }
        Some("tree") => {
            let (iso_arg, file) = match args.len() {
                2 => (None, &args[1]),
                3 => (Some(&args[1]), &args[2]),
                _ => return Err("usage: eqoa tree [iso] <file>".into()),
            };
            let c = esf::Container::open(disc(iso_arg)?.read(file)?)?;
            for &r in &c.roots {
                print_tree(&c, r, 0);
            }
            eprintln!("{} records, {} DictIDs, {} surfaces", c.nodes.len(), c.dict.len(), c.surfaces.len());
        }
        Some("zones") => {
            let file = args.get(1).ok_or("usage: eqoa zones <world.esf>")?;
            let t = std::time::Instant::now();
            let bytes = disc(None)?.read(file)?;
            let read = t.elapsed();
            let c = esf::Container::open(bytes)?;
            eprintln!("{file}: read {read:.2?}, parsed {:.2?}, {} records", t.elapsed() - read, c.nodes.len());
            for (i, &z) in c.zones().iter().enumerate() {
                let pre = c.pre_translations(z);
                let lo = pre.iter().fold([f32::MAX; 3], |a, p| [a[0].min(p[0]), a[1].min(p[1]), a[2].min(p[2])]);
                println!(
                    "{i:4}  {:4} patches  {:5} actors  {:3} islands  first island ({:.0}, {:.0}, {:.0})",
                    c.terrain_patches(z).len(),
                    c.actors(z).len(),
                    pre.len(),
                    lo[0],
                    lo[1],
                    lo[2]
                );
            }
        }
        Some("view") | Some("shot") => {
            let shot = args[0] == "shot";
            let usage = "usage: eqoa view <file> [model] | eqoa view <world> zone <n> | eqoa shot <same> <out.png> [camera...]";
            let file = args.get(1).ok_or(usage)?;
            let mut rest = &args[2..];
            if shot {
                // The PNG is the first argument ending in .png.
                let at = rest.iter().position(|a| a.ends_with(".png")).ok_or(usage)?;
                let (out, nums) = (&rest[at], &rest[at + 1..]);
                rest = &rest[..at];
                let (lib, src) = source(file, rest)?;
                let (cam, script) = match src {
                    viewer::Source::Walk { .. } => (None, script_args(nums)?),
                    _ => (camera_args(&src, nums)?, None),
                };
                viewer::shot(&lib, &src, cam, script, (960, 540), out)?;
            } else {
                let (lib, src) = source(file, rest)?;
                viewer::run(lib, src)?;
            }
        }
        _ => return Err("usage: eqoa ls|tree|zones|view|shot (see main.rs)".into()),
    }
    Ok(())
}

/// Files every scene loads before its own, in this order (the dictionary's
/// first-registration-wins rule makes the order matter). The character art the
/// game keeps resident through the session.
const SHARED: &[&str] = &["CHARCUST.CSF", "CHARFACE.CSF", sky::FILE];

/// The library and what to show: `[model]` or `zone <n>`.
fn source(file: &str, rest: &[String]) -> Result<(esf::Library, viewer::Source)> {
    let mut iso = disc(None)?;
    let mut lib = esf::Library::new();
    let t = std::time::Instant::now();
    let main = lib.add(file, esf::Container::open(iso.read(file)?)?);
    for s in SHARED {
        if !s.eq_ignore_ascii_case(file) {
            lib.add(*s, esf::Container::open(iso.read(s)?)?);
        }
    }
    eprintln!("loaded {} files in {:.2?}", lib.files.len(), t.elapsed());
    match rest {
        [z, n] if z == "walk" => {
            // The character's art: CHAR.ESF, after everything else.
            lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF")?)?);
            let index: usize = n.parse()?;
            lib.file(main).zones().get(index).ok_or("no such zone")?;
            let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
            Ok((lib, viewer::Source::Walk { file: main, index, character }))
        }
        [z, n] if z == "zone" => {
            let index: usize = n.parse()?;
            let zone = *lib.file(main).zones().get(index).ok_or("no such zone")?;
            Ok((lib, viewer::Source::Zone { file: main, zone, index }))
        }
        [] | [_] => {
            let list = scene::list_models(&lib, main);
            let index = match rest.first() {
                None => 0,
                Some(m) => match m.strip_prefix("0x") {
                    Some(hex) => {
                        let id = u32::from_str_radix(hex, 16)? as i32;
                        list.iter().position(|(d, _)| *d == id).ok_or("no model with that DictID")?
                    }
                    None => m.parse::<usize>().ok().filter(|&i| i < list.len()).ok_or("no such model")?,
                },
            };
            Ok((lib, viewer::Source::Models { list, index }))
        }
        _ => Err("expected [model] or zone <n>".into()),
    }
}

/// `shot` for a walk: `ms [fwd strafe orbit pitch]`, held throughout.
fn script_args(n: &[String]) -> Result<Option<viewer::Script>> {
    let v: Vec<f32> = n.iter().map(|s| s.parse()).collect::<std::result::Result<_, _>>()?;
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    Ok(Some(viewer::Script {
        ms: g(0) as i64,
        controls: walker::Controls { fwd: g(1), strafe: g(2), orbit: g(3), pitch: g(4) },
    }))
}

/// `shot` camera numbers: orbit `yaw pitch zoom`, or fly `x y z yaw pitch`.
fn camera_args(src: &viewer::Source, n: &[String]) -> Result<Option<viewer::Camera>> {
    let n: Vec<f32> = n.iter().map(|s| s.parse()).collect::<std::result::Result<_, _>>()?;
    Ok(match (src, n.as_slice()) {
        (_, []) => None,
        (viewer::Source::Models { .. }, [yaw, pitch, zoom]) => Some(viewer::Camera::Orbit { yaw: *yaw, pitch: *pitch, zoom: *zoom }),
        (viewer::Source::Zone { .. }, [x, y, z, yaw, pitch]) => {
            Some(viewer::Camera::Fly { pos: glam::Vec3::new(*x, *y, *z), yaw: *yaw, pitch: *pitch })
        }
        _ => return Err("camera: orbit `yaw pitch zoom`, fly `x y z yaw pitch`".into()),
    })
}

fn print_tree(c: &esf::Container, id: esf::NodeId, depth: usize) {
    let n = c.node(id);
    println!("{:indent$}{:04X} v{} {} bytes", "", n.ty, n.version, n.size, indent = depth * 2);
    for &k in &n.children {
        print_tree(c, k, depth + 1);
    }
}
