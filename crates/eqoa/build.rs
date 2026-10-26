//! The main thread's stack on Windows: 8 MB, as Linux and macOS give it.
//!
//! A Windows program's main thread has the stack its header reserves, 1 MB
//! unless the link says otherwise, and the game's first frame needs more:
//! its frames hold their state by value, and when the first frame opens the
//! sound device the stack holds about 980 KB (measured under cdb on a Windows
//! machine: `eqoa::run` 241 KB, `viewer::run_app` 236, `App::host_frame` 174,
//! `run_boot` 123, `Audio::open` 111, `window_event` 54). Without this the
//! client died there, "thread 'main' has overflowed its stack", before its
//! window had drawn anything: a white window (2026-10-22). `dev.yml` refuses
//! a Windows build whose header reserves less.
//!
//! And the Windows program's own icon (Explorer, a pinned shortcut, the
//! taskbar before the window sets its own): the drawn disc (`src/icon_art.rs`,
//! the same drawing the window takes), written as an .ico and a resource file
//! here and linked in: MSVC's linker takes the .res as it is, GNU's needs it
//! as an object (windres). No game art: the save icon comes off the player's
//! disc at run time (`icon`).

#[allow(dead_code)]
mod icon_art {
    include!("src/icon_art.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:8388608"),
            Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,8388608"),
            _ => {}
        }
        windows_icon();
    }
}

/// The sizes Windows picks from: the title bar, Explorer's views, the taskbar.
const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

fn windows_icon() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let images: Vec<(u32, Vec<u8>)> = SIZES.iter().map(|&s| (s, dib(s, &icon_art::disc_rgba(s)))).collect();
    std::fs::write(out.join("eqoa.ico"), ico(&images)).expect("eqoa.ico");
    let res = out.join("eqoa.res");
    std::fs::write(&res, resources(&images)).expect("eqoa.res");
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo:rustc-link-arg-bins={}", res.display()),
        Ok("gnu") => {
            // `$WINDRES`, else the cross toolchain's, else the host's
            let obj = out.join("eqoa-res.o");
            let tried: Vec<String> = std::env::var("WINDRES").ok().into_iter().chain(["x86_64-w64-mingw32-windres".into(), "windres".into()]).collect();
            let made = tried.iter().any(|w| {
                std::process::Command::new(w).args(["-J", "res", "-O", "coff", "-i"]).arg(&res).arg("-o").arg(&obj).status().is_ok_and(|s| s.success())
            });
            if made {
                println!("cargo:rustc-link-arg-bins={}", obj.display());
            } else {
                println!("cargo:warning=no windres ({}): the program is built without its icon", tried.join(", "));
            }
        }
        _ => {}
    }
}

/// An icon image as a DIB: the header (twice the height, for the mask), the
/// pixels bottom up as BGRA, then the AND mask (set where clear), its rows
/// padded to four bytes.
fn dib(size: u32, rgba: &[u8]) -> Vec<u8> {
    let s = size as usize;
    let mask_row = s.div_ceil(32) * 4;
    let mut d = Vec::with_capacity(40 + s * s * 4 + mask_row * s);
    for v in [40u32, size, size * 2] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&1u16.to_le_bytes()); // planes
    d.extend_from_slice(&32u16.to_le_bytes()); // bits a pixel
    for v in [0u32, (s * s * 4 + mask_row * s) as u32, 0, 0, 0, 0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for y in (0..s).rev() {
        for x in 0..s {
            let p = &rgba[(y * s + x) * 4..][..4];
            d.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    for y in (0..s).rev() {
        let mut row = vec![0u8; mask_row];
        for x in 0..s {
            if rgba[(y * s + x) * 4 + 3] == 0 {
                row[x / 8] |= 0x80 >> (x % 8);
            }
        }
        d.extend_from_slice(&row);
    }
    d
}

/// The directory entry's fields shared by an .ico and a group icon resource.
fn entry_head(size: u32, bytes: usize) -> Vec<u8> {
    let dim = if size >= 256 { 0 } else { size as u8 };
    let mut e = vec![dim, dim, 0, 0];
    e.extend_from_slice(&1u16.to_le_bytes()); // planes
    e.extend_from_slice(&32u16.to_le_bytes()); // bits a pixel
    e.extend_from_slice(&(bytes as u32).to_le_bytes());
    e
}

/// An .ico file of the images.
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut f = Vec::new();
    for v in [0u16, 1, images.len() as u16] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    let mut offset = 6 + 16 * images.len();
    for (size, data) in images {
        f.extend(entry_head(*size, data.len()));
        f.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        f.extend_from_slice(data);
    }
    f
}

/// A resource file (.res): its empty first entry, each image an RT_ICON
/// named by its number, then the RT_GROUP_ICON (number 1) that lists them,
/// which Explorer shows as the program's icon.
fn resources(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    fn resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes()); // the header's size
        for v in [0xFFFFu16, kind, 0xFFFF, name] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // data version
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&0x0409u16.to_le_bytes()); // English (US), as rc's default
        out.extend_from_slice(&0u32.to_le_bytes()); // version
        out.extend_from_slice(&0u32.to_le_bytes()); // characteristics
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    let mut out = Vec::new();
    // the .res file's mark: an entry of nothing
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for v in [0xFFFFu16, 0, 0xFFFF, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0u8; 16]);
    for (i, (_, data)) in images.iter().enumerate() {
        resource(&mut out, RT_ICON, i as u16 + 1, 0x1010, data);
    }
    let mut group = Vec::new();
    for v in [0u16, 1, images.len() as u16] {
        group.extend_from_slice(&v.to_le_bytes());
    }
    for (i, (size, data)) in images.iter().enumerate() {
        group.extend(entry_head(*size, data.len()));
        group.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }
    resource(&mut out, RT_GROUP_ICON, 1, 0x1030, &group);
    out
}
