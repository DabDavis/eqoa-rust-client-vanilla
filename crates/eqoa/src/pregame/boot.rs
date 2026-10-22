//! The launcher's loading screens: SLUS's `VIDisplayLoadingScreen(bool)`
//! (0x00102380) shows one of four pictures, `DATA3\LOADING0.16` to
//! `LOADING3.16`, through `VIDisplayScreen16(name, n, reset)` (0x00102058),
//! which reads the file's 280 sectors and sends them to the frame buffer as
//! they are: 640x448 at 16 bits a pixel (PSMCT16S, which DISPFB then shows;
//! red in the low five bits, then green and blue, the top bit unused).
//!
//! Called with `false` it picks one at random (`rand() & 3`) and resets the
//! GS first; with `true` it shows the one after the last, round the four.
//! SLUS calls it in `VILauncher::Init` (false, then true once UTIL.REL is
//! in), in `_root` before SETUP is loaded (true), SETUP's `Init` calls it
//! while its CSFs load (true), and STATION's `Init` (false). The client
//! steps through the same calls as its own loading goes ([`Step`]), each
//! picture up at least [`HOLD_MS`] (the console's took seconds each; ours
//! would otherwise flash by).

/// The frame the pictures fill.
pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 448;

/// The shortest time a picture stays up.
pub const HOLD_MS: i64 = 600;

/// `VIDisplayScreen16`'s file: `\DATA3\%s%d.16;1`, upper-cased.
pub fn path(n: i32) -> String {
    format!("DATA3/LOADING{n}.16")
}

/// A picture's bytes as RGBA8, top row first; None for a file not the size
/// the call reads (280 sectors).
pub fn decode(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < WIDTH * HEIGHT * 2 {
        return None;
    }
    let mut out = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for p in b[..WIDTH * HEIGHT * 2].chunks_exact(2) {
        let v = u16::from_le_bytes([p[0], p[1]]);
        // five bits widened as the GS's PSMCT16 read does: shifted up three
        let c = |s: u16| (((v >> s) & 31) << 3) as u8;
        out.extend([c(0), c(5), c(10), 255]);
    }
    Some(out)
}

/// `VIDisplayLoadingScreen`'s picture counter (SLUS `0x001478D8`) and its
/// `rand()`: newlib's, unseeded, as the launcher calls it before anything
/// else draws from it.
#[derive(Clone, Debug)]
pub struct Rotation {
    pub current: i32,
    rand: vi::csprite::Rand,
}

impl Default for Rotation {
    fn default() -> Self {
        Rotation { current: 0, rand: vi::csprite::Rand(1) }
    }
}

impl Rotation {
    /// `VIDisplayLoadingScreen(b)`: the picture shown now.
    pub fn show(&mut self, next: bool) -> i32 {
        self.current = if next { (self.current + 1).rem_euclid(4) } else { self.rand.rand() & 3 };
        self.current
    }
}

/// The calls the launcher and the modules make, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// `VILauncher::Init`, at once: `false`
    Launcher,
    /// once UTIL.REL is in (the disc's modules found): `true`
    Util,
    /// `_root`, SUPPORT in, before SETUP: `true`
    Support,
    /// SETUP's `Init`, its CSFs loading: `true`
    Setup,
    /// STATION's `Init`: `false`
    Station,
}

impl Step {
    pub fn next(self) -> bool {
        !matches!(self, Step::Launcher | Step::Station)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_widens_five_bits_red_lowest() {
        let mut b = vec![0u8; WIDTH * HEIGHT * 2];
        // red 31, green 1, blue 16, the top bit set
        let v: u16 = 31 | (1 << 5) | (16 << 10) | 0x8000;
        b[..2].copy_from_slice(&v.to_le_bytes());
        let rgba = decode(&b).unwrap();
        assert_eq!(&rgba[..4], &[248, 8, 128, 255]);
        assert_eq!(rgba.len(), WIDTH * HEIGHT * 4);
        // the control: a file a sector short is not a picture
        assert!(decode(&b[..WIDTH * HEIGHT * 2 - 2048]).is_none());
    }

    #[test]
    fn the_first_picture_is_newlibs_first_rand_and_then_they_go_round() {
        let mut r = Rotation::default();
        // newlib's first rand() from seed 1 is 0x5851F42D: & 3 is 1
        assert_eq!(r.show(false), 1);
        assert_eq!([r.show(true), r.show(true), r.show(true)], [2, 3, 0]);
        // the control: a second draw is not the first
        let mut s = Rotation::default();
        s.show(false);
        assert_ne!(s.rand.rand() & 3, 1, "the second rand() would pick the same");
    }

    #[test]
    fn the_paths_are_the_launchers() {
        assert_eq!(path(0), "DATA3/LOADING0.16");
        assert_eq!(path(3), "DATA3/LOADING3.16");
        assert!(Step::Util.next() && Step::Setup.next() && !Step::Station.next() && !Step::Launcher.next());
    }
}
