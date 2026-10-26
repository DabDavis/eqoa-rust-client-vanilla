//! The program's icon (F3 pass 17, the user: "the program icon is ugly", then
//! "memcard icon i meant once the ISO is chosen, but we ship a custom one
//! also"): the drawn disc the program carries (`icon_art`) until the
//! player's disc is known, then EQOA's own memory-card save icon off it
//! (`saveicon`), rendered once SUPPORT has come and kept in the config
//! directory (`saveicon.png`, with the disc it came from in `saveicon.key`),
//! so a later start with that disc opens its window with it already, before
//! SUPPORT is unpacked again. Another disc, or none, shows the drawn disc.
//!
//! Where it shows: Windows, the window's title bar and taskbar button (and
//! the .exe's own icon, the drawn disc, for Explorer and a pinned shortcut:
//! build.rs); Linux, the window under X11 (`_NET_WM_ICON`; Wayland takes a
//! program's icon from its desktop entry, which a loose binary has none of);
//! macOS, the Dock (`NSApplication`'s icon image: winit sets no window icon
//! there).

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError};
use winit::window::{Icon, Window, WindowAttributes};

/// The picture's own size: the cache's, the Dock's and Windows' taskbar's.
const BIG: u32 = 256;
/// The window's (X11 scales it as it needs).
const WINDOW: u32 = 128;
/// Windows' title bar (it scales it to 16, 20 or 24 by the screen's scale).
#[cfg(windows)]
const SMALL: u32 = 32;

/// The drawn disc, `BIG` square: the program's own.
pub fn disc() -> &'static [u8] {
    static DISC: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    DISC.get_or_init(|| crate::icon_art::disc_rgba(BIG))
}

/// The disc this run plays: the save icon kept is shown only for it.
fn key() -> String {
    crate::disc_path(None)
}

/// The picture a window opens with: the save icon kept for this disc, else
/// the drawn disc.
fn start_picture() -> Vec<u8> {
    kept(&key()).unwrap_or_else(|| disc().to_vec())
}

/// The save icon kept for `disc` (`settings::kept_icon`), if one is and it
/// reads as a picture of the size kept.
fn kept(disc: &str) -> Option<Vec<u8>> {
    decode(&crate::settings::kept_icon(disc)?)
}

/// A kept PNG as `BIG`-square RGBA, if it is one.
fn decode(png_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png_bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    (info.width == BIG && info.height == BIG && info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight).then(|| {
        buf.truncate(info.buffer_size());
        buf
    })
}

/// Keeps a rendered save icon for `disc`, for the next start.
fn keep(disc: &str, rgba: &[u8]) {
    if let Err(e) = encode(rgba, BIG).and_then(|png| crate::settings::keep_icon(disc, &png)) {
        eprintln!("icon: the save icon is not kept ({e})");
    }
}

/// A square RGBA picture as PNG.
pub fn encode(rgba: &[u8], size: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// `rgba` (`from` square) made `to` square by averaging (premultiplied), for
/// sizes that divide it; any other a nearest pick.
pub fn shrink(rgba: &[u8], from: u32, to: u32) -> Vec<u8> {
    let (from, to) = (from as usize, to as usize);
    if from == to {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; to * to * 4];
    if from % to != 0 {
        for y in 0..to {
            for x in 0..to {
                let (sx, sy) = (x * from / to, y * from / to);
                out[(y * to + x) * 4..][..4].copy_from_slice(&rgba[(sy * from + sx) * 4..][..4]);
            }
        }
        return out;
    }
    let k = from / to;
    for y in 0..to {
        for x in 0..to {
            let mut acc = [0f32; 4];
            for j in 0..k {
                for i in 0..k {
                    let p = &rgba[((y * k + j) * from + x * k + i) * 4..][..4];
                    let a = p[3] as f32 / 255.0;
                    for c in 0..3 {
                        acc[c] += p[c] as f32 * a;
                    }
                    acc[3] += a;
                }
            }
            let o = (y * to + x) * 4;
            if acc[3] > 0.0 {
                for c in 0..3 {
                    out[o + c] = (acc[c] / acc[3]).round().clamp(0.0, 255.0) as u8;
                }
            }
            out[o + 3] = (acc[3] / (k * k) as f32 * 255.0).round() as u8;
        }
    }
    out
}

fn icon(rgba: &[u8], size: u32) -> Option<Icon> {
    Icon::from_rgba(shrink(rgba, BIG, size), size, size).ok()
}

/// A window's attributes with the program's icon, as it opens.
pub fn attributes(a: WindowAttributes) -> WindowAttributes {
    let picture = start_picture();
    let a = a.with_window_icon(icon(&picture, WINDOW));
    #[cfg(windows)]
    let a = {
        use winit::platform::windows::WindowAttributesExtWindows;
        a.with_window_icon(icon(&picture, SMALL)).with_taskbar_icon(icon(&picture, BIG))
    };
    a
}

/// The picture on a window that is open (and the Dock's).
fn apply(window: &Window, rgba: &[u8]) {
    window.set_window_icon(icon(rgba, WINDOW));
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowExtWindows;
        window.set_window_icon(icon(rgba, SMALL));
        window.set_taskbar_icon(icon(rgba, BIG));
    }
    #[cfg(target_os = "macos")]
    dock(rgba);
}

/// macOS: the Dock's picture, as the program starts (winit's `resumed`).
#[cfg(target_os = "macos")]
pub fn dock_now() {
    dock(&start_picture());
}

#[cfg(target_os = "macos")]
fn dock(rgba: &[u8]) {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Ok(png) = encode(rgba, BIG) else { return };
    let data = NSData::with_bytes(&png);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else { return };
    // SAFETY: an image, never None (which the setter may not allow).
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&*image)) };
}

/// Where the save icon stands this run.
enum Phase {
    /// SUPPORT not here yet
    Waiting,
    /// being drawn on a thread of its own (a few hundred milliseconds)
    Drawing(Receiver<Option<Vec<u8>>>),
    /// on the window, or there is none to put there
    Done,
}

static PHASE: Mutex<Phase> = Mutex::new(Phase::Waiting);

/// Once a frame: when SUPPORT has come, the game's save icon is drawn off it
/// (on a thread), kept for the next start and put on the window. Nothing
/// after that.
pub fn poll(window: &Window) {
    let mut phase = PHASE.lock().unwrap();
    match &*phase {
        Phase::Done => {}
        Phase::Waiting => {
            if let Some(image) = crate::modules::get(crate::modules::Module::Support) {
                let (tx, rx) = std::sync::mpsc::channel();
                let disc = key();
                std::thread::spawn(move || {
                    let picture = crate::saveicon::read(&image).map(|i| i.render(BIG));
                    match &picture {
                        Some(p) => keep(&disc, p),
                        None => eprintln!("icon: no save icon in SUPPORT; the drawn disc stays"),
                    }
                    let _ = tx.send(picture);
                });
                *phase = Phase::Drawing(rx);
            }
        }
        Phase::Drawing(rx) => match rx.try_recv() {
            Ok(Some(p)) => {
                apply(window, &p);
                *phase = Phase::Done;
            }
            Ok(None) | Err(TryRecvError::Disconnected) => *phase = Phase::Done,
            Err(TryRecvError::Empty) => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disc_is_round_with_a_hole_and_clear_corners() {
        let n = 64usize;
        let d = crate::icon_art::disc_rgba(n as u32);
        assert_eq!(d.len(), n * n * 4);
        let a = |x: usize, y: usize| d[(y * n + x) * 4 + 3];
        // the corners and the hole clear, the ring solid
        for (x, y) in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1), (n / 2, n / 2), (n / 2 - 1, n / 2 - 1)] {
            assert_eq!(a(x, y), 0, "clear at {x},{y}");
        }
        for (x, y) in [(n / 2, 3), (n / 2, n - 4), (3, n / 2), (n - 4, n / 2), (n / 2 + n / 4, n / 2)] {
            assert_eq!(a(x, y), 255, "solid at {x},{y}");
        }
        // round: the alpha is the same at the four quarter turns
        for r in [5usize, 12, 20, 28] {
            let c = n / 2;
            let v = [a(c + r, c), a(c - 1 - r, c), a(c, c + r), a(c, c - 1 - r)];
            assert!(v.iter().all(|&x| x == v[0]), "symmetric at radius {r}: {v:?}");
        }
        // control: a square of the same size would be solid in its corners
        let square = vec![255u8; n * n * 4];
        assert_ne!(square[3], a(0, 0));
    }

    #[test]
    fn small_icons_keep_the_disc() {
        for s in [16u32, 24, 32, 48] {
            let d = crate::icon_art::disc_rgba(s);
            let n = s as usize;
            let cover = d.chunks(4).filter(|p| p[3] > 128).count() as f32 / (n * n) as f32;
            // a disc of radius s/2 - 0.5 less its hole: a bit under pi/4
            assert!(cover > 0.6 && cover < 0.8, "size {s}: coverage {cover}");
        }
    }

    #[test]
    fn shrinking_averages_and_keeps_the_clear() {
        let big = crate::icon_art::disc_rgba(BIG);
        let small = shrink(&big, BIG, 32);
        assert_eq!(small.len(), 32 * 32 * 4);
        assert_eq!(small[3], 0, "a clear corner stays clear");
        assert!(small[(16 * 32 + 2) * 4 + 3] > 200, "the ring stays solid");
        // control: a nearest pick of a size that does not divide still has its size
        assert_eq!(shrink(&big, BIG, 48).len(), 48 * 48 * 4);
    }

    #[test]
    fn a_kept_icon_is_for_its_own_disc() {
        let picture = disc().to_vec();
        keep("a.iso", &picture);
        assert_eq!(kept("a.iso").as_deref(), Some(&picture[..]), "kept for its disc");
        // control: another disc's (or none kept) shows the drawn disc instead
        assert!(kept("b.iso").is_none(), "another disc's is not shown");
        assert!(decode(b"not a png").is_none());
    }
}
