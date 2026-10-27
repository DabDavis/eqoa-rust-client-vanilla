//! The page's app icons, made by `web/build.sh` into each build: the
//! program's own drawn disc (`crates/eqoa/src/icon_art.rs`, its one drawing,
//! which `crates/eqoa/build.rs` takes the same way for the Windows program's
//! .ico) as the PNGs a browser installs the page with (`web/manifest.json`):
//! the disc alone at 192 and 512 (purpose `any`), the disc on the page's
//! background inside the circle every launcher's mask keeps (`maskable`), and
//! iOS's Home Screen icon (`apple-touch-icon.png`), opaque, as iOS draws a
//! clear corner black. No game art.
//!
//!     cargo run --release -p webicons -- <directory>

use std::path::Path;

#[allow(dead_code)]
mod icon_art {
    include!("../../../crates/eqoa/src/icon_art.rs");
}

/// The page's background (`web/index.html`'s `--bg`).
const BG: [u8; 3] = [0x0b, 0x0d, 0x10];

/// A maskable icon's safe zone: the circle of 80% of its width in its middle
/// is all a launcher's mask is sure to keep.
const SAFE: f32 = 0.8;

/// Each icon: its file, its size, the disc's share of it across, and what is
/// under the disc (none: clear).
const ICONS: [(&str, u32, f32, Option<[u8; 3]>); 4] = [
    ("icon-192.png", 192, 1.0, None),
    ("icon-512.png", 512, 1.0, None),
    // a tenth inside the safe zone
    ("icon-maskable-512.png", 512, SAFE * 0.9, Some(BG)),
    // iOS rounds the square's corners off: the disc well inside them
    ("apple-touch-icon.png", 180, 0.84, Some(BG)),
];

/// `size` square, RGBA with straight alpha: the disc `share` of it across in
/// its middle, over `under` (opaque), or over nothing.
fn tile(size: u32, share: f32, under: Option<[u8; 3]>) -> Vec<u8> {
    let d = ((size as f32 * share).round() as u32).clamp(1, size);
    let disc = icon_art::disc_rgba(d);
    let at = (size - d) / 2;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let o = ((y * size + x) * 4) as usize;
            let mut p = [0u8; 4];
            if (at..at + d).contains(&x) && (at..at + d).contains(&y) {
                let i = (((y - at) * d + (x - at)) * 4) as usize;
                p.copy_from_slice(&disc[i..i + 4]);
            }
            match under {
                None => out[o..o + 4].copy_from_slice(&p),
                Some(bg) => {
                    let a = p[3] as f32 / 255.0;
                    for k in 0..3 {
                        out[o + k] = (p[k] as f32 * a + bg[k] as f32 * (1.0 - a)).round() as u8;
                    }
                    out[o + 3] = 255;
                }
            }
        }
    }
    out
}

fn write(path: &Path, size: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size, size);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(rgba).map_err(|e| e.to_string())
}

fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: webicons <directory>");
        std::process::exit(2);
    };
    for (name, size, share, under) in ICONS {
        if let Err(e) = write(&Path::new(&dir).join(name), size, &tile(size, share, under)) {
            eprintln!("webicons: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether every pixel of `rgba` (`size` square) outside the circle of
    /// `share` of its width is the page's background.
    fn background_outside(rgba: &[u8], size: u32, share: f32) -> bool {
        let half = size as f32 / 2.0;
        let r = share * half;
        (0..size).all(|y| {
            (0..size).all(|x| {
                let (dx, dy) = (x as f32 + 0.5 - half, y as f32 + 0.5 - half);
                let o = ((y * size + x) * 4) as usize;
                dx * dx + dy * dy <= r * r || rgba[o..o + 4] == [BG[0], BG[1], BG[2], 255]
            })
        })
    }

    /// The `any` icons are the program's disc itself, its corners and its
    /// hole clear (control: the opaque ones are clear nowhere).
    #[test]
    fn the_any_icons_are_the_disc() {
        for size in [192, 512] {
            let t = tile(size, 1.0, None);
            assert_eq!(t, icon_art::disc_rgba(size));
            let middle = (((size / 2) * size + size / 2) * 4 + 3) as usize;
            assert_eq!((t[3], t[middle]), (0, 0), "a corner and the hole clear");
        }
        let opaque = tile(180, 0.84, Some(BG));
        assert!(opaque.chunks(4).all(|p| p[3] == 255), "control: an icon on the background is opaque");
    }

    /// The maskable icon's disc is inside the safe zone, so a round or a
    /// squircle mask cuts only background (control: a disc 0.9 across
    /// reaches out of it).
    #[test]
    fn the_maskable_icon_keeps_to_the_safe_zone() {
        let (_, size, share, under) = ICONS[2];
        assert!(background_outside(&tile(size, share, under), size, SAFE));
        assert!(!background_outside(&tile(size, 0.9, under), size, SAFE), "control");
    }
}
