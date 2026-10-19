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
//! eqoa listen <world> <x> <z> <seconds> <out.wav> [hour] [walk]   the world's sound there, offline
//!                               (walk: the human walks forward, its footsteps with it;
//!                                cues: the respawn cue at 1 s and the level-up sound at 2 s;
//!                                fight: in combat from 2 s to 8 s, the battle music)
//! ```
//!
//! A model is a list index or a DictID in hex (`0x…`). A `shot` camera is
//! `yaw pitch zoom` for a model, `x y z yaw pitch` for a zone, and for a walk
//! a script, `ms [fwd strafe orbit pitch]`, held for that much game time.
//!
//! `iso` defaults to `$EQOA_ISO`, then `run/eqoa.iso`.

mod audio;
mod chat;
mod dialog;
mod hud;
mod particles;
mod play;
mod scene;
mod sky;
mod sound;
mod stream;
mod ui;
mod viewer;
mod walker;
mod weather;

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

/// The disc image's path: the argument, `$EQOA_ISO`, or `run/eqoa.iso`.
pub fn disc_path(arg: Option<&String>) -> String {
    arg.cloned().or_else(|| std::env::var("EQOA_ISO").ok()).unwrap_or_else(|| "run/eqoa.iso".into())
}

pub fn disc(arg: Option<&String>) -> Result<iso::Iso> {
    let path = disc_path(arg);
    Ok(iso::Iso::open(&path).map_err(|e| format!("{path}: {e}"))?)
}

/// The world's sound at one spot, offline: the camera stands 3 above the
/// ground at (x, z), the game runs at 30 frames a second, and the mix is
/// written to a WAV.
#[allow(clippy::too_many_arguments)]
fn listen(file: &str, x: f32, z: f32, secs: f32, out: &str, hour: f32, walk: bool, cues: bool, fight: bool) -> Result<()> {
    use vi::fpu::Ee;
    let mut lib = esf::Library::new();
    let f = lib.add(file, esf::Container::open(disc(None)?.read(file)?)?);
    lib.add("CHAR.ESF", esf::Container::open(disc(None)?.read("CHAR.ESF")?)?);
    lib.add(sky::FILE, esf::Container::open(disc(None)?.read(sky::FILE)?)?);
    let world = vi::world::World::<Ee>::load(&lib, f).ok_or("no world in the file")?;
    let mut walker = walker::Walker::new(world, x, z, 0.0);
    let human = lib.lookup(viewer::HUMAN).ok_or("no human in CHAR.ESF")?;
    walker.set_character(&lib, human);
    let mut sound = sound::Sound::offline(std::path::Path::new(&disc_path(None))).ok_or("no sound device")?;
    // the weather (`EQOA_WEATHER`): its rain and thunder, and its dimming of the track
    let mut weather = weather::Weather::new(&mut lib, 0);
    let ids = walker.sprite.as_ref().map(|cs| cs.sounds()).unwrap_or_default();
    sound.share_sounds(&lib, &ids);
    eprintln!("the character's play list names {} sounds", ids.len());
    let frames = (secs * 30.0) as usize;
    let mut pcm: Vec<i16> = Vec::new();
    for k in 0..frames {
        let now = k as i64 * 1000 / 30;
        let c = walker::Controls { fwd: if walk { 1.0 } else { 0.0 }, strafe: 0.0, orbit: 0.0, pitch: 0.0 };
        walker.frame(c, now);
        walker.pose_sounding(Some(&mut sound.sink()));
        if cues && k == 30 {
            sound.cue(&sound::Cue::Named("respawn", 0.4));
        }
        if cues && k == 60 {
            sound.cue(&sound::Cue::Ui(16));
        }
        sound.battle_music(Some(fight && (60..240).contains(&k)));
        let eye = walker.eye;
        sound.frame(&mut walker.scene, &eye, hour, now);
        weather.frame(&mut walker.scene, Some(&mut sound), &eye, now, hour);
        weather.lightning();
        let _ = weather.render(&eye);
        pcm.extend(sound.audio().render(1600));
        if k % 30 == 0 {
            eprintln!("{:5.1} s  {}", k as f32 / 30.0, sound.describe());
            if weather.atm.intensity != vi::fpu::F::ZERO {
                eprintln!("        {}", weather.describe());
            }
        }
    }
    let mut w = Vec::with_capacity(44 + pcm.len() * 2);
    let n = (pcm.len() * 2) as u32;
    w.extend(b"RIFF");
    w.extend((36 + n).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(48000u32.to_le_bytes());
    w.extend((48000u32 * 4).to_le_bytes());
    w.extend(4u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(n.to_le_bytes());
    for v in &pcm {
        w.extend(v.to_le_bytes());
    }
    std::fs::write(out, w)?;
    let loud = pcm.iter().filter(|&&v| v != 0).count();
    let peak = pcm.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    eprintln!("{out}: {:.1} s at 48 kHz, {:.0}% of samples not silent, peak {peak}", pcm.len() as f32 / 96000.0, 100.0 * loud as f32 / pcm.len().max(1) as f32);
    Ok(())
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
                let (mut lib, src) = source(file, rest)?;
                let (cam, script) = match src {
                    viewer::Source::Walk { .. } => (None, script_args(nums)?),
                    _ => (camera_args(&src, nums)?, None),
                };
                viewer::shot(&mut lib, &src, cam, script, (960, 540), out)?;
            } else {
                let (lib, src) = source(file, rest)?;
                viewer::run(lib, src)?;
            }
        }
        Some("listen") => {
            let usage = "usage: eqoa listen <world> <x> <z> <seconds> <out.wav> [hour]";
            let file = args.get(1).ok_or(usage)?;
            let num = |i: usize| -> Result<f32> { Ok(args.get(i).ok_or(usage)?.parse::<f32>()?) };
            let (x, z, secs) = (num(2)?, num(3)?, num(4)?);
            let out = args.get(5).ok_or(usage)?;
            let hour = args.get(6).map(|h| h.parse::<f32>()).transpose()?.unwrap_or(10.0);
            let walk = args[7.min(args.len())..].iter().any(|a| a == "walk");
            let cues = args[7.min(args.len())..].iter().any(|a| a == "cues");
            let fight = args[7.min(args.len())..].iter().any(|a| a == "fight");
            listen(file, x, z, secs, out, hour, walk, cues, fight)?;
        }
        Some("login") => {
            let host = args.get(1).ok_or("usage: eqoa login <host> <user> (password in EQOA_PASSWORD)")?;
            let user = args.get(2).ok_or("usage: eqoa login <host> <user>")?;
            let pass = std::env::var("EQOA_PASSWORD").map_err(|_| "set EQOA_PASSWORD")?;
            login(host, user, &pass)?;
        }
        Some("play") | Some("playshot") => {
            let host = args.get(1).ok_or("usage: eqoa play <host> <user> (password in EQOA_PASSWORD)")?;
            let user = args.get(2).ok_or("usage: eqoa play <host> <user>")?;
            let pass = std::env::var("EQOA_PASSWORD").map_err(|_| "set EQOA_PASSWORD")?;
            let (session, player, dump) = login(host, user, &pass)?;
            // The world the dump names: its file, by the path's last part.
            let file = player.world_path.rsplit(['\\', '/']).next().unwrap_or("tunaria.esf").to_uppercase();
            let (lib, src) = source(&file, &[])?;
            let mut lib = lib;
            let main_file = 0;
            lib.add("CHAR.ESF", esf::Container::open(disc(None)?.read("CHAR.ESF")?)?);
            // What creatures hold: weapons and shields.
            lib.add("ITEM.ESF", esf::Container::open(disc(None)?.read("ITEM.ESF")?)?);
            // The name plates' font ("Arial").
            lib.add("ARIAL.CSF", esf::Container::open(disc(None)?.read("ARIAL.CSF")?)?);
            // The HUD's art and font ("ArialUI").
            lib.add("UI.CSF", esf::Container::open(disc(None)?.read("UI.CSF")?)?);
            lib.add("ARIALUI.CSF", esf::Container::open(disc(None)?.read("ARIALUI.CSF")?)?);
            let [x, _, z] = player.pos;
            let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(main_file), x, z).ok_or("no zone at the player's position")?;
            let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
            drop(src);
            let src = viewer::Source::Walk { file: main_file, index, character, spawn: Some((x, z, player.hpr[0])) };
            let mut play = play::Play::new(session, player, dump);
            // The game's words, for the lines the client writes itself.
            play.words = disc(None)?.read("WORDS.TXT").ok().and_then(|b| vi::strtab::StringTable::parse(&b));
            // The HUD's atlas is SUPPORT's data: out of the player's own
            // decrypted image (EQOA_SUPPORT, else run/support.bin). Without
            // it there is no HUD.
            let support = std::env::var("EQOA_SUPPORT").unwrap_or_else(|_| "run/support.bin".into());
            let atlas = std::fs::read(&support).ok().and_then(|b| vi::ui::read_atlas(&b));
            let textures = disc(None)?.read("TEXTURES.TXT").ok().and_then(|b| vi::strtab::StringTable::parse(&b));
            match (atlas, textures) {
                (Some(a), Some(t)) => play.ui_data = Some((a, t)),
                _ => eprintln!("no HUD: the UI atlas needs a decrypted SUPPORT image at {support} (EQOA_SUPPORT)"),
            }
            let play = Some(play);
            if args[0] == "playshot" {
                let out = args.get(3).ok_or("usage: eqoa playshot <host> <user> <out.png> [ms]")?;
                let ms: i64 = args.get(4).map(|m| m.parse()).transpose()?.unwrap_or(3000);
                let script = viewer::Script { ms, controls: walker::Controls::default() };
                viewer::shot_with(&mut lib, &src, None, Some(script), (960, 540), out, play)?;
            } else {
                viewer::run_with(lib, src, play)?;
            }
        }
        _ => return Err("usage: eqoa ls|tree|zones|view|shot|login (see main.rs)".into()),
    }
    Ok(())
}

/// The Station login and the game session up to world entry, printing
/// every event: the first shard and the first character are taken.
fn login(host: &str, user: &str, pass: &str) -> Result<(net::session::Session, net::session::Player, Box<net::dump::MemoryDump>)> {
    use net::session::{Event, Session};
    let l = net::auth::login((host, 9735), user, pass)?;
    eprintln!("auth: account {} status {} features {:#x}", l.account, l.status, l.game_features);
    let server = std::net::ToSocketAddrs::to_socket_addrs(&(host, 10070))?.next().ok_or("no address")?;
    let mut s = Session::connect(server, user, l.token)?;
    let t = std::time::Instant::now();
    while t.elapsed() < std::time::Duration::from_secs(30) {
        for e in s.pump() {
            match &e {
                Event::ShardList(list) => {
                    eprintln!("shards: {list:?}");
                    if let Some(sh) = list.first() {
                        s.connect_shard(sh.endpoint);
                    }
                }
                Event::Characters(list) => {
                    for c in list {
                        eprintln!("character: {} (id {}, race {}, class {}, level {})", c.name, c.server_id, c.race, c.class, c.level);
                    }
                    // `EQOA_CHAR=<name>` picks one; else the first.
                    let want = std::env::var("EQOA_CHAR").ok();
                    let pick = list.iter().find(|c| want.as_deref() == Some(c.name.as_str())).or(list.first());
                    if let Some(c) = pick {
                        s.select_character(&c.clone());
                    }
                }
                Event::WorldEntry { self_id, player, dump } => {
                    eprintln!("in the world as {self_id:#x}: {player:?} after {:.2?}, {} packets sent", t.elapsed(), s.packets_sent);
                    eprintln!(
                        "  dump: {} quick-chat sets, {} quests, {} items carried ({} worn), bank {} items, {} abilities, speed {}",
                        dump.text_sets.len(),
                        dump.quests.len(),
                        dump.inventory.items.len(),
                        dump.inventory.items.iter().filter(|i| i.equip_slot >= 0).count(),
                        dump.bank.items.items.len(),
                        dump.abilities.len(),
                        dump.speed
                    );
                    let p = player.clone();
                    return Ok((s, p, dump.clone()));
                }
                Event::Disconnected(r) => return Err(format!("disconnected (reason {r})").into()),
                Event::State { channel, data } => eprintln!("state {channel:#04x}: {} bytes", data.len()),
                other => eprintln!("{other:?}"),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    Err("no world entry within 30 s".into())
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
            Ok((lib, viewer::Source::Walk { file: main, index, character, spawn: None }))
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
