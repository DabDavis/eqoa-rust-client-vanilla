//! The client binary: the game from its boot, and the disc and container
//! inspection commands the first slice is built on:
//!
//! ```text
//! eqoa [boot [server]]          the game: loading screens, licence, title, Station's login,
//!                               the server list, character select, the world (`pregame`)
//! eqoa <image>                  the game on that disc image (one dropped on the program), kept
//! eqoa play <server> <account>  straight into the world (EQOA_PASSWORD; EQOA_CHAR picks)
//! eqoa ls   [iso]               every file on the disc
//! eqoa tree [iso] <file>        a container's record tree
//! eqoa zones <world.esf>        the zones in a world file
//! eqoa volumes <world.esf>      every effect volume it places (fog, dust, shafts, leaves, flocks)
//! eqoa view <file> [model]       a model (wgpu + winit)
//! eqoa view <world> zone <n>     a zone: terrain and every placed actor
//! eqoa view <world> walk <n>     the character, walking in a zone
//! eqoa shot <file> [model | zone <n>] <out.png> [camera]   one view, offscreen
//! eqoa listen <world> <x> <z> <seconds> <out.wav> [hour] [walk]   the world's sound there, offline
//! eqoa mapshot <world> <out.png> [x z [heading [scale [secs]]]]   the map (M) built as in play, drawn
//! eqoa fxview [visual [secs]]   a human casting a spell visual at another, again and again, as
//!                               0x00B1 plays it (the GM panel's preview; `fxview`); with an
//!                               out.png after the visual, `[ms,...] [secs]`: drawn at those times
//! eqoa rumble                   each controller the game would read rumbles a moment, as the game
//!                               rumbles one (the large motor, then the small)
//!                               (walk: the human walks forward, its footsteps with it;
//!                                cues: the respawn cue at 1 s and the level-up sound at 2 s;
//!                                fight: in combat from 2 s to 8 s, the battle music)
//! ```
//!
//! A model is a list index or a DictID in hex (`0x…`). A `shot` camera is
//! `yaw pitch zoom` for a model, `x y z yaw pitch` for a zone, and for a walk
//! a script, `ms [fwd strafe orbit pitch]`, held for that much game time.
//!
//! `iso` defaults to `$EQOA_ISO`, then `run/eqoa.iso`. The game (`eqoa`,
//! `boot`, `play`) also takes the image the player picked last, and with none
//! at hand `eqoa` asks for it in the desktop's file dialog (`discs`).

// The browser build runs none of the commands (the page calls `web`).
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]
// On Windows no console window of its own: double-clicked, the game had a
// terminal beside it. Run from a terminal it writes there (`attach_console`).
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod audio;
mod era;
#[cfg(target_os = "macos")]
mod macos;
mod chat;
mod dialog;
mod attackclock;
mod auction;
mod auctionwnd;
mod bank;
mod camreach;
mod camstyle;
mod client_image;
mod playersearch;
mod searchwnd;
mod s2;
mod controls;
mod arrival;
mod craft;
mod craftwnd;
mod death;
mod discs;
// the program's icon: the drawn disc it carries, then the game's own save
// icon off the player's disc (natively; a page has its favicon)
#[cfg(not(target_arch = "wasm32"))]
mod icon;
#[cfg(not(target_arch = "wasm32"))]
mod icon_art;
#[cfg(not(target_arch = "wasm32"))]
mod saveicon;
mod editbox;
mod fog;
mod frames;
mod group;
mod guild;
// the music off a disc read asynchronously: a page's (natively, its tests)
#[cfg(any(target_arch = "wasm32", test))]
mod bgm;
mod bindpage;
mod binds;
mod blacksmith;
mod gear;
mod gram;
mod hideui;
mod hud;
mod damage;
mod display;
mod hudmenu;
mod influence;
mod input;
// the page's gamepads (the browser build), checked natively by its tests
#[cfg(any(test, target_arch = "wasm32"))]
mod padmap;
mod spellbook;
mod infotext;
mod inventory;
mod itemmenu;
mod classmastery;
mod cs;
mod faction;
mod loading;
mod lod;
mod loot;
mod map;
// the memory a report and the log carry (F3 crash 20261026-220031)
mod mem;
mod merchant;
mod modules;
mod mouse;
mod multibox;
mod quantity;
mod questguide;
mod questlog;
mod statbox;
mod tracker;
mod quickchat;
mod alarm;
mod startmenu;
mod textlist;
mod trade;
mod trainer;
mod trails;
mod options;
mod settings;
mod social;
mod wholist;
mod zones;
mod flora;
mod fx;
mod fxview;
mod particles;
mod patcher;
mod play;
mod pointer;
mod pregame;
mod rings;
mod scene;
mod servers;
mod sky;
mod spelling;
mod touch;
mod sound;
mod spells;
mod stream;
#[cfg(not(target_arch = "wasm32"))]
mod gpulost;
#[cfg(not(target_arch = "wasm32"))]
mod gpustart;
#[cfg(not(target_arch = "wasm32"))]
mod nothrottle;
mod report;
mod ui;
mod update;
#[cfg(not(target_arch = "wasm32"))]
mod updatewnd;
mod viewer;
mod wndlist;
mod walker;
mod weather;
mod winplace;
// Windows' Xbox controllers, read in the background (its mapping checked
// natively by its tests)
#[cfg(any(windows, test))]
mod xinput;
// Windows' PlayStation controllers, read over HID in the background, and
// their motors there and on a Mac (`hidpad::mac`, IOKit; its reports checked
// natively by its tests)
#[cfg(any(windows, target_os = "macos", test))]
mod hidpad;

#[cfg(target_arch = "wasm32")]
mod web;

use std::process::ExitCode;

/// In a browser the page drives it (`web::view`, `web::play`).
#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    #[cfg(windows)]
    attach_console();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eqoa: {e}");
            // the line into the log, and the log closed after it: a run that
            // ends on an error (no disc image chosen) is not offered as a
            // crash at the next start
            report::clean_exit();
            ExitCode::FAILURE
        }
    }
}

/// Run from a terminal on Windows, the program writes to it: it has no
/// console of its own (`windows_subsystem`), so it takes its parent's, and
/// what was not redirected goes there. Double-clicked there is none, and
/// nothing changes.
#[cfg(windows)]
fn attach_console() {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle};
    // SAFETY: constants, and handles this process owns from here on
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(which);
            if (h.is_null() || h == INVALID_HANDLE_VALUE)
                && let Ok(f) = std::fs::OpenOptions::new().write(true).open("CONOUT$")
            {
                SetStdHandle(which, f.into_raw_handle());
            }
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// What a test that makes a GPU device holds for its whole run: with two at
/// once, this machine's Vulkan loader crashed (`vkSetDebugUtilsObjectNameEXT`)
/// as one test's device went while the other's named a buffer.
#[cfg(test)]
pub static GPU_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A line for whoever runs the client: the console in a page, stderr natively.
pub fn say(s: &str) {
    #[cfg(target_arch = "wasm32")]
    web::log(s);
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{s}");
}

/// The disc image's path: the argument, the one the game took
/// (`discs::taken`), `$EQOA_ISO`, or `run/eqoa.iso`.
pub fn disc_path(arg: Option<&String>) -> String {
    arg.cloned().or_else(discs::taken).or_else(|| std::env::var("EQOA_ISO").ok()).unwrap_or_else(|| discs::CHECKOUT.into())
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
    let f = lib.add(file, stream::open_file(std::path::Path::new(&disc_path(None)), file)?);
    lib.add("CHAR.ESF", esf::Container::open(disc(None)?.read("CHAR.ESF")?)?);
    lib.add(sky::FILE, esf::Container::open(disc(None)?.read(sky::FILE)?)?);
    // the zones round the spot, off the disc (a world read whole has them)
    if let Some(mut src) = stream::source_for(&lib, f) {
        stream::fill(&mut lib, f, x, z, src.as_mut());
    }
    let world = vi::world::World::<Ee>::load(&lib, f).ok_or("no world in the file")?;
    let mut walker = walker::Walker::new(world, x, z, 0.0);
    let human = lib.lookup(viewer::HUMAN).ok_or("no human in CHAR.ESF")?;
    walker.set_character(&lib, human);
    // the UI sounds' volumes are SUPPORT's: waited for here
    modules::load_now(modules::Module::Support, std::path::Path::new(&disc_path(None)));
    let mut sound = sound::Sound::offline(std::path::Path::new(&disc_path(None))).ok_or("no sound device")?;
    // the options' Fx and Music Volume (`EQOA_FX_VOLUME`, `EQOA_MUSIC_VOLUME`,
    // 0 to 1): the two sound classes' volumes, as the options set them
    for (cls, var) in [(0, "EQOA_FX_VOLUME"), (1, "EQOA_MUSIC_VOLUME")] {
        if let Some(v) = std::env::var(var).ok().and_then(|v| v.parse::<f32>().ok()) {
            sound.dev.set_sound_class_volume(cls, v);
        }
    }
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

/// `eqoa volumes`: every effect volume a world places, zone by zone (each
/// zone read off the disc on its own and let go).
fn volumes(file: &str) -> Result<()> {
    use std::io::Write;
    let iso = disc_path(None);
    let mut lib = esf::Library::new();
    let w = lib.add(file, stream::open_file(std::path::Path::new(&iso), file)?);
    let mut src = stream::FileZones::open(std::path::Path::new(&iso), file);
    let mut out = std::io::stdout().lock();
    let mut count = std::collections::BTreeMap::<&str, usize>::new();
    for zi in 0..lib.zone_count(w) {
        let list = match (lib.zone(w, zi), lib.zone_range(w, zi), src.as_mut()) {
            (Some((f, z)), _, _) => fx::placements(lib.file(f), z, zi),
            (None, Some((off, len)), Some(s)) => {
                use stream::ZoneSource;
                s.request(zi, off, len);
                let Some((_, Ok(b))) = s.poll() else { continue };
                let c = esf::Container::parse_records(b);
                let Some(&z) = c.roots.first() else { continue };
                fx::placements(&c, z, zi)
            }
            _ => continue,
        };
        for p in list {
            let (a, r) = (&p.actor, &p.record);
            let size = [r.bbox[3] - r.bbox[0], r.bbox[4] - r.bbox[1], r.bbox[5] - r.bbox[2]];
            match r.params {
                Some(q) => {
                    *count.entry(fx::type_name(q.ty)).or_default() += 1;
                    writeln!(
                        out,
                        "zone {:3} at ({:8.1}, {:6.1}, {:8.1}) yaw {:5.2} scale {:4.2}  {:6} box {:5.1}x{:5.1}x{:5.1}  hours {:5.2}-{:5.2}  seen {:5.1}  strength {:4.2}  density {:4.2}  flags {}  surface {:#010x}{}  sprite {:#010x}{}",
                        p.zone, a.pos[0], a.pos[1], a.pos[2], a.rot[0], a.scale, fx::type_name(q.ty), size[0], size[1], size[2], q.p34, q.p38, q.p30, q.p28, q.density, q.flags,
                        q.surface as u32, if p.surface_found { "" } else if q.surface == -1 || q.surface == 0 { " (none)" } else { " (missing)" },
                        q.sprite as u32, if p.sprite_found { "" } else if q.sprite == -1 || q.sprite == 0 { " (none)" } else { " (missing)" },
                    )?;
                }
                None => {
                    *count.entry("no params").or_default() += 1;
                    writeln!(out, "zone {:3} at ({:8.1}, {:6.1}, {:8.1})  no 0xC330", p.zone, a.pos[0], a.pos[1], a.pos[2])?;
                }
            }
        }
    }
    eprintln!("{file}: {count:?}");
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        // `eqoa` alone, or `eqoa boot [server]`: the game from the disc's
        // boot (`boot`)
        None | Some("boot") => boot(None, args.get(1).cloned())?,
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
        Some("volumes") => {
            let file = args.get(1).ok_or("usage: eqoa volumes <world.esf>")?;
            volumes(file)?;
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some("rumble") => input::check_motors(10_000)?,
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
        // `eqoa icon <size> <out.png>`: the program's own icon, the drawn disc
        // (a desktop entry's picture); `eqoa icon <size> <out.png> save`: the
        // game's save icon off the disc's SUPPORT, as the window takes it once
        // the disc is known (the player's own: never shared)
        #[cfg(not(target_arch = "wasm32"))]
        Some("icon") => {
            let usage = "usage: eqoa icon <size> <out.png> [save]";
            let size: u32 = args.get(1).ok_or(usage)?.parse()?;
            let out = args.get(2).ok_or(usage)?;
            if !(1..=1024).contains(&size) {
                return Err(usage.into());
            }
            let rgba = match args.get(3).map(String::as_str) {
                None => icon_art::disc_rgba(size),
                Some("save") => {
                    let support = modules::load_now(modules::Module::Support, std::path::Path::new(&disc_path(None))).ok_or("no SUPPORT: no disc image, or not the Frontiers one")?;
                    saveicon::read(&support).ok_or("no save icon in SUPPORT")?.render(size)
                }
                Some(_) => return Err(usage.into()),
            };
            std::fs::write(out, icon::encode(&rgba, size)?)?;
            say(&format!("{out}: {size}x{size}"));
        }
        // `eqoa fxview [visual] [cast seconds]`: the GM panel's spell-visual
        // preview in a window; `eqoa fxview <visual> <out.png> [ms,...]
        // [cast seconds]` drawn at those times
        #[cfg(not(target_arch = "wasm32"))]
        Some("fxview") => {
            let usage = "usage: eqoa fxview [visual [cast seconds]] | eqoa fxview <visual> <out.png> [ms,ms,...] [cast seconds]";
            let rest = &args[1..];
            let effect = match rest.first() {
                Some(v) => Some(fxview::parse_visual(v).ok_or(usage)?),
                None => None,
            };
            let secs = |s: Option<&String>| -> Result<i32> {
                let s: f32 = match s {
                    Some(s) => s.parse()?,
                    None => fxview::CAST_MS as f32 / 1000.0,
                };
                Ok((s * 1000.0) as i32)
            };
            let lib = fxview::library_off(&mut disc(None)?)?;
            // `EQOA_FX_AIM=caster|human|dummy|both`: what the cast lands on (a
            // dummy; `both` a second human, and a third casting at itself)
            let aim = std::env::var("EQOA_FX_AIM").ok().and_then(|a| fxview::Aim::parse(&a));
            match rest.get(1).filter(|a| a.ends_with(".png")) {
                Some(out) => {
                    let times: Vec<i64> = match rest.get(2) {
                        Some(t) => t.split(',').map(|t| t.trim().parse()).collect::<std::result::Result<_, _>>()?,
                        None => vec![1500],
                    };
                    let camera = std::env::var("EQOA_FX_CAMERA").ok().and_then(|v| {
                        let n: Vec<f32> = v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                        (n.len() == 3).then(|| fxview::Orbit { yaw: n[0], pitch: n[1], zoom: n[2] })
                    });
                    fxview::shots(lib, effect, secs(rest.get(3))?, aim, &times, out, camera)?;
                }
                None => fxview::run(lib, effect, secs(rest.get(1))?, aim, None)?,
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
        Some("mapshot") => {
            // `eqoa mapshot <world> <out.png> [x z [heading [scale [secs]]]]`: the
            // world's map built as in play and drawn round (x, z)
            let usage = "usage: eqoa mapshot <world> <out.png> [x z [heading [scale [secs]]]]";
            let world = args.get(1).ok_or(usage)?.to_uppercase();
            let out = args.get(2).ok_or(usage)?;
            let num = |i: usize| args.get(i).map(|v| v.parse::<f32>()).transpose();
            let size = std::env::var("EQOA_SHOT_SIZE").ok().and_then(|v| {
                let (w, h) = v.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            });
            let budget = std::env::var("EQOA_MAP_BUDGET").ok().and_then(|v| v.parse().ok());
            let shot = map::tool::Shot {
                world,
                at: [num(3)?.unwrap_or(25300.0), num(4)?.unwrap_or(15200.0)],
                heading: num(5)?.unwrap_or(0.0),
                scale: num(6)?,
                size: size.unwrap_or((1280, 720)),
                secs: num(7)?.unwrap_or(120.0),
                budget_ms: budget,
            };
            map::tool::shot(&shot, out)?;
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
            // taken out of a report, as Station's login is
            report::secret(user, &pass);
            // the options kept in the player's config directory (a playshot
            // keeps them only in `$EQOA_CONFIG`: `settings`)
            if args[0] == "play" {
                settings::keep_in_user_dir();
            }
            // the disc: `$EQOA_ISO`, the one picked last, `run/eqoa.iso`
            discs::boot_disc(None).ok_or("no disc image: start eqoa once to choose it, or set EQOA_ISO")?;
            // CLIENT and SUPPORT (the start menu, the HUD): the player's
            // images, else off the disc, on threads of their own meanwhile.
            modules::load(std::path::Path::new(&disc_path(None)));
            // the server's patch as its folder holds it (the pregame's
            // patcher fills it; `s2`)
            s2::for_server(host);
            let (session, player, dump) = login(host, user, &pass)?;
            let (mut lib, src) = world_library(&player)?;
            let mut play = play::Play::new(session, player, dump);
            play_tables(&mut play)?;
            let play = Some(play);
            if args[0] == "playshot" {
                let out = args.get(3).ok_or("usage: eqoa playshot <host> <user> <out.png> [ms]")?;
                let ms: i64 = args.get(4).map(|m| m.parse()).transpose()?.unwrap_or(3000);
                let script = viewer::Script { ms, controls: walker::Controls::default() };
                // `EQOA_SHOT_SIZE=WxH`: the picture's size (960x540 without)
                let size = std::env::var("EQOA_SHOT_SIZE").ok().and_then(|v| {
                    let (w, h) = v.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                });
                viewer::shot_with(&mut lib, &src, None, Some(script), size.unwrap_or((960, 540)), out, play)?;
            } else {
                viewer::run_with(lib, src, play)?;
            }
        }
        // `eqoa <image>`: the game on that disc image (a file dropped on the
        // program), checked as a pick is and kept for the next time
        Some(image) if std::path::Path::new(image).is_file() => boot(Some(image), None)?,
        _ => return Err("usage: eqoa [boot [server]] | eqoa <image> | ls|tree|zones|view|shot|login (see main.rs)".into()),
    }
    Ok(())
}

/// The game from the disc's boot, the loading screens, the licence and the
/// title, Station's login, the server list and character select, into the
/// world (`pregame`). The server is the one the title's Network Setup keeps,
/// or `server`. The disc is the image `image` names, else the one at hand
/// (`discs::boot_disc`), else the one the player picks as the game starts
/// (`discs::settle`, in the window's event loop).
fn boot(image: Option<&str>, server: Option<String>) -> Result<()> {
    settings::keep_in_user_dir();
    // the log a report carries, and the crash reports (`report`); the
    // program as it starts, for a start again after a lost graphics device
    #[cfg(not(target_arch = "wasm32"))]
    {
        report::start();
        gpulost::begin();
        // a window in the back kept at its pace by the system too (multiboxing)
        nothrottle::keep_pace();
    }
    // a newer build on the release, asked for while the boot's pictures show
    #[cfg(not(target_arch = "wasm32"))]
    update::start();
    if let Some(iso) = discs::boot_disc(image) {
        disc(None)?;
        // CLIENT and SUPPORT (the UI's atlas, the start menu), off the
        // disc on threads of their own while the boot's pictures show
        modules::load(std::path::Path::new(&iso));
    }
    viewer::run_boot(server)?;
    // the run closed (its log says so); the window closed for the release's
    // build, or for a graphics device lost: the game starts in this one's
    // place (a device lost again soon after a start: a crash report instead)
    #[cfg(not(target_arch = "wasm32"))]
    {
        if gpulost::lost() && !gpulost::settled() {
            report::crash(&format!("The graphics device was lost ({}) within 30 s of the game's start: the game did not start again, as it would only lose it again.", gpulost::why()));
        }
        report::clean_exit();
        update::relaunch_if_requested();
        if gpulost::lost() && gpulost::settled() {
            gpulost::relaunch();
        }
    }
    Ok(())
}

/// The world the MemoryDump names, as `EnterGameLoad` loads it: the world
/// file (its zones left on the disc, the player's own read now), the
/// character art every scene keeps (`SHARED`), then CHAR.ESF and the rest,
/// in this order (the dictionary's first registration wins); and the walk at
/// the player's place.
pub fn world_library(player: &net::session::Player) -> Result<(esf::Library, viewer::Source)> {
    // the server's patch (`s2`) before the world's files are read
    s2::begin_session();
    // The world the dump names: the world table's file for its id
    // (`LoadWorld`), or its path for a world not on the disc.
    let file = play::login_world_file(player);
    let (mut lib, src) = source(&file, &[])?;
    let main_file = 0;
    lib.add("CHAR.ESF", esf::Container::open(disc(None)?.read("CHAR.ESF")?)?);
    // a server's patch's own names for CHAR.ESF's art (`s2`: Sandstorm's set 9)
    s2::get().register_art(&mut lib);
    // What creatures hold: weapons and shields.
    lib.add("ITEM.ESF", esf::Container::open(disc(None)?.read("ITEM.ESF")?)?);
    // The name plates' font ("Arial").
    lib.add("ARIAL.CSF", esf::Container::open(disc(None)?.read("ARIAL.CSF")?)?);
    // The HUD's art and font ("ArialUI").
    lib.add("UI.CSF", esf::Container::open(disc(None)?.read("UI.CSF")?)?);
    lib.add("ARIALUI.CSF", esf::Container::open(disc(None)?.read("ARIALUI.CSF")?)?);
    // The abilities' and items' icons (the HUD's quick menus).
    lib.add("ITEMICON.ESF", esf::Container::open(disc(None)?.read("ITEMICON.ESF")?)?);
    // The spell effects, and the lightning's texture.
    lib.add("SPELLFX.CSF", esf::Container::open(disc(None)?.read("SPELLFX.CSF")?)?);
    lib.add("FX.CSF", esf::Container::open(disc(None)?.read("FX.CSF")?)?);
    // EnterGameLoad's last file (the card's copy when there is one):
    // the compass's patch art.
    lib.add("ADDART.CSF", esf::Container::open(disc(None)?.read("ADDART.CSF")?)?);
    // A server's patch's own art (`s2`: where CLIENT reads the card's added
    // art, Sandstorm's S2 reads its ss-art.csf, robes 4 and 5), out of the
    // server's folder.
    if let Some(path) = s2::get().art_path() {
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| esf::Container::open(b).map_err(|e| format!("{e:?}"))) {
            Ok(c) => {
                lib.add(path.file_name().map_or_else(String::new, |n| n.to_string_lossy().to_uppercase()), c);
            }
            Err(e) => eprintln!("s2: {}: {e}", path.display()),
        }
    }
    // the 2002 disc's art while it is the art drawn (`era`, F9): its files in
    // place of their namesakes, read with the world's
    #[cfg(not(target_arch = "wasm32"))]
    if era::current() == esf::Era::Y2002 && era::available() {
        match era::read_now(&era::names_of(&lib)) {
            Ok(art) => {
                let n = era::install(&mut lib, art);
                lib.set_view(esf::Era::Y2002);
                say(&format!("art: {n} of the 2002 disc's files stand in for Frontiers'"));
            }
            Err(e) => say(&format!("art: the 2002 disc could not be read: {e}")),
        }
    }
    let [x, _, z] = player.pos;
    let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(main_file), x, z).ok_or("no zone at the player's position")?;
    // the player's zone off the disc; the walker reads the rest round it
    add_twin(&mut lib, main_file, &file);
    stream::load_zone_now(&mut lib, main_file, index)?;
    let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
    drop(src);
    // Msg000D: SetLocation at the dump's place, its height the server's
    let spawn = walker::Spawn { pos: player.pos, heading: player.hpr[0], placing: walker::Placing::AsGiven };
    Ok((lib, viewer::Source::Walk { file: main_file, index, character, spawn: Some(spawn) }))
}

/// What `Play` reads off the disc beside the library: the game's words, for
/// the lines the client writes itself, and the HUD's texture names (its atlas
/// is SUPPORT's, and the start menu CLIENT's, taken up as they come,
/// `modules`).
pub fn play_tables(play: &mut play::Play) -> Result<()> {
    // the server's folder's copy first (`s2`: the patcher's), else the disc's
    play.words = s2::get().words().or_else(|| disc(None).ok()?.read("WORDS.TXT").ok().and_then(|b| crate::spelling::words(&b)));
    play.ui_textures = disc(None)?.read("TEXTURES.TXT").ok().and_then(|b| vi::strtab::StringTable::parse(&b));
    if play.ui_textures.is_none() {
        eprintln!("no HUD: TEXTURES.TXT is not on the disc");
    }
    Ok(())
}

/// The Station login and the game session up to world entry, printing
/// every event: the first shard and the first character are taken.
fn login(host: &str, user: &str, pass: &str) -> Result<(net::session::Session, net::session::Player, Box<net::dump::MemoryDump>)> {
    use net::session::{Event, Session};
    // `EQOA_AUTH_PORT` / `EQOA_GAME_PORT`: a server on other ports (a test one)
    let port = |v: &str, d: u16| std::env::var(v).ok().and_then(|p| p.parse().ok()).unwrap_or(d);
    let l = net::auth::login((host, port("EQOA_AUTH_PORT", 9735)), user, pass)?;
    eprintln!("auth: account {} status {} features {:#x}", l.account, l.status, l.game_features);
    let server = std::net::ToSocketAddrs::to_socket_addrs(&(host, port("EQOA_GAME_PORT", 10070)))?.next().ok_or("no address")?;
    let mut s = Session::connect(server, user, l.token)?;
    let t = std::time::Instant::now();
    while t.elapsed() < std::time::Duration::from_secs(30) {
        let events = s.pump();
        for (k, e) in events.iter().enumerate() {
            match e {
                Event::ShardList(list) => {
                    eprintln!("shards: {list:?}");
                    if let Some(sh) = list.first() {
                        // EQOA_GAME_PORT also moves the shard's port (a test
                        // server's list still names the default one)
                        let mut ep = sh.endpoint;
                        if std::env::var_os("EQOA_GAME_PORT").is_some() {
                            ep = (ep & !0xFFFF_0000) | (port("EQOA_GAME_PORT", 10070) as u64) << 16;
                        }
                        s.connect_shard(ep, crate::s2::get().shard_hello().as_deref());
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
                    // what came with it (the ignore and buddy lists) is the
                    // game's to read
                    s.held = events[k + 1..].to_vec();
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

/// The twin of the world in file `main` (`esf::Library::set_twin`), when the
/// disc it came off wants one (`stream::wants_twin`): the 2002 disc's Tunaria
/// gets Frontiers', for its zones' effect volumes and streamed audio. Added
/// after the library's other files, so no index moves.
fn add_twin(lib: &mut esf::Library, main: usize, file: &str) {
    let era = lib.era_of(main);
    if !stream::wants_twin(era) {
        return;
    }
    match stream::open_file(std::path::Path::new(&disc_path(None)), file) {
        Ok(t) => {
            lib.set_twin(main, Some(t), era::other(era));
            say(&format!("art: {file} off the 2002 disc, with Frontiers' effect volumes and streamed audio"));
        }
        Err(e) => say(&format!("art: no Frontiers {file} beside the 2002 disc's ({e}): no effect volumes or streamed audio")),
    }
}

/// The library and what to show: `[model]` or `zone <n>`.
fn source(file: &str, rest: &[String]) -> Result<(esf::Library, viewer::Source)> {
    let mut iso = disc(None)?;
    let mut lib = esf::Library::new();
    let t = std::time::Instant::now();
    // a world with its zones left on the disc (stream::open_file), off the
    // disc of the art drawn when that disc has it (`era`: Tunaria off the
    // 2002 disc)
    let era = era::world_era(file);
    #[cfg(not(target_arch = "wasm32"))]
    let image = era::image_of(era).unwrap_or_else(|| disc_path(None));
    #[cfg(target_arch = "wasm32")]
    let image = disc_path(None);
    // the 2002 disc's world that does not open is Frontiers' (the game plays on)
    let (era, world) = match stream::open_file(std::path::Path::new(&image), file) {
        Ok(c) => (era, c),
        Err(e) if era == esf::Era::Y2002 => {
            say(&format!("art: the 2002 disc's {file} did not open ({e}): Frontiers' world"));
            (esf::Era::Frontiers, stream::open_file(std::path::Path::new(&disc_path(None)), file)?)
        }
        Err(e) => return Err(e),
    };
    let main = lib.add_from(file, world, era);
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
            add_twin(&mut lib, main, file);
            stream::load_zone_now(&mut lib, main, index)?;
            let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
            Ok((lib, viewer::Source::Walk { file: main, index, character, spawn: None }))
        }
        [z, n] if z == "zone" => {
            let index: usize = n.parse()?;
            lib.file(main).zones().get(index).ok_or("no such zone")?;
            add_twin(&mut lib, main, file);
            stream::load_zone_now(&mut lib, main, index)?;
            let (file, zone) = lib.zone(main, index).ok_or("no such zone")?;
            Ok((lib, viewer::Source::Zone { file, zone, index }))
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
