//! EQOA's own memory-card save icon, out of the player's SUPPORT image: the
//! picture a PS2's browser showed for the game's save (`VIMemCardIconData`,
//! 0x17898 bytes, which `VIMemCard` writes to the card as `static.ico`:
//! decomp/src/vimemcard.cpp). Rendered here on the CPU, front on, as the
//! program's icon once the player's disc is known (`icon`, the user's "memcard
//! icon ... once the ISO is chosen"): it is their own disc's, kept only in
//! their config directory, never committed and never sent.
//!
//! The layout is the PS2 save icon's (as ps2sdk's and mymc's notes describe
//! the `.ico` format), checked against SUPPORT's own copy, whose parts add up
//! to its 0x17898 bytes: a 20-byte header (0x00010000, the animation's shape
//! count, the texture type, a float, the vertex count, three a triangle); per
//! vertex each shape's position (four i16 in 1/4096), a normal (four i16), a
//! UV (two i16 in 1/4096) and a colour (four u8, 0x80 = 1.0); the animation;
//! then the texture, 128 × 128 PSMCT16 (red in the low five bits), the blob's
//! last 32 KiB when the type says it is stored whole (bit 3 clear: Frontiers'
//! is 7). The model stands on y = 0 with up as -y, its face towards -z: the
//! render looks at it from there, shape 0 (the rest of the animation's shapes
//! are its frames), lit from the top left as the browser's lights show it.

/// SUPPORT's image is indexed by `VA - 0x003E3D00` (`vi::ui`); the icon is at
/// `0x004C8C38` in the Frontiers build's.
const ICON_VA: usize = 0x004C_8C38;
const IMAGE_BASE: usize = 0x003E_3D00;
const ICON_LEN: usize = 0x17898;
const TEX: usize = 128;
const TEX_BYTES: usize = TEX * TEX * 2;

/// The icon, shape 0, ready to draw.
pub struct SaveIcon {
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    color: Vec<f32>,
    tex: Vec<[f32; 3]>,
}

/// The icon out of a SUPPORT image (the Frontiers build's: `modules::check`),
/// `None` when what is at its address is not one.
pub fn read(support: &[u8]) -> Option<SaveIcon> {
    let o = ICON_VA - IMAGE_BASE;
    parse(support.get(o..o + ICON_LEN)?)
}

/// The icon out of its own bytes (a `static.ico`).
pub fn parse(d: &[u8]) -> Option<SaveIcon> {
    let w = |p: usize| d.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let h = |p: usize| i16::from_le_bytes([d[p], d[p + 1]]) as f32;
    if w(0)? != 0x0001_0000 {
        return None;
    }
    let shapes = w(4)? as usize;
    let kind = w(8)?;
    let count = w(16)? as usize;
    if !(1..=16).contains(&shapes) || kind & 8 != 0 || count == 0 || count % 3 != 0 {
        return None;
    }
    let stride = 8 * shapes + 16;
    if 20 + count * stride + TEX_BYTES > d.len() {
        return None;
    }
    let mut icon = SaveIcon { pos: Vec::with_capacity(count), normal: Vec::with_capacity(count), uv: Vec::with_capacity(count), color: Vec::with_capacity(count), tex: Vec::with_capacity(TEX * TEX) };
    for v in 0..count {
        let p = 20 + v * stride;
        icon.pos.push([h(p) / 4096.0, h(p + 2) / 4096.0, h(p + 4) / 4096.0]);
        let n = p + 8 * shapes;
        icon.normal.push([h(n) / 4096.0, h(n + 2) / 4096.0, h(n + 4) / 4096.0]);
        icon.uv.push([h(n + 8) / 4096.0, h(n + 10) / 4096.0]);
        // the colour's grey (the browser's 0x80 = 1.0); the icon's are all 0x80
        let c = &d[n + 12..n + 15];
        icon.color.push((c[0] as f32 + c[1] as f32 + c[2] as f32) / 3.0 / 128.0);
    }
    let t = &d[d.len() - TEX_BYTES..];
    for i in 0..TEX * TEX {
        let px = u16::from_le_bytes([t[2 * i], t[2 * i + 1]]);
        let five = |s: u16| {
            let c = ((px >> s) & 31) as f32;
            ((c * 8.0) + (c / 4.0).floor()) / 255.0
        };
        icon.tex.push([five(0), five(5), five(10)]);
    }
    if icon.pos.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    Some(icon)
}

impl SaveIcon {
    /// `size` × `size` RGBA with straight alpha, the background clear: shape 0
    /// front on, fitted with a margin, textured (bilinear), lit from the top
    /// left and front, anti-aliased by drawing larger and averaging.
    pub fn render(&self, size: u32) -> Vec<u8> {
        let size = size.max(1) as usize;
        let ss = if size <= 128 { 4 } else { 3 };
        let s = size * ss;
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &self.pos {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6) * 1.12;
        let (cx, cy) = ((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
        let screen: Vec<[f32; 3]> = self
            .pos
            .iter()
            // up is -y, which is the picture's up as it is; the eye is at -z,
            // so the nearer a point the greater its -z
            .map(|p| [(p[0] - cx) / span * s as f32 + s as f32 / 2.0, (p[1] - cy) / span * s as f32 + s as f32 / 2.0, -p[2]])
            .collect();
        let light = {
            let l = [-0.4f32, -0.6, -0.7];
            let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            [l[0] / n, l[1] / n, l[2] / n]
        };
        let mut depth = vec![f32::MIN; s * s];
        let mut color = vec![[0f32; 3]; s * s];
        let mut cover = vec![false; s * s];
        for t in (0..screen.len()).step_by(3) {
            let [a, b, c] = [screen[t], screen[t + 1], screen[t + 2]];
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if area.abs() < 1e-9 {
                continue;
            }
            let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
            let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(s - 1);
            let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
            let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(s - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / area;
                    let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                    let i = y * s + x;
                    if z <= depth[i] {
                        continue;
                    }
                    let lerp2 = |v: &[[f32; 2]]| [w0 * v[t][0] + w1 * v[t + 1][0] + w2 * v[t + 2][0], w0 * v[t][1] + w1 * v[t + 1][1] + w2 * v[t + 2][1]];
                    let uv = lerp2(&self.uv);
                    let n = [0, 1, 2].map(|k| w0 * self.normal[t][k] + w1 * self.normal[t + 1][k] + w2 * self.normal[t + 2][k]);
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                    let lit = ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len).clamp(0.0, 1.0);
                    let grey = w0 * self.color[t] + w1 * self.color[t + 1] + w2 * self.color[t + 2];
                    let shade = (0.55 + 0.6 * lit) * grey;
                    let tex = self.sample(uv[0], uv[1]);
                    depth[i] = z;
                    color[i] = tex.map(|v| (v * shade).min(1.0));
                    cover[i] = true;
                }
            }
        }
        let mut out = vec![0u8; size * size * 4];
        for y in 0..size {
            for x in 0..size {
                let (mut sum, mut n) = ([0f32; 3], 0usize);
                for sy in 0..ss {
                    for sx in 0..ss {
                        let i = (y * ss + sy) * s + x * ss + sx;
                        if cover[i] {
                            for k in 0..3 {
                                sum[k] += color[i][k];
                            }
                            n += 1;
                        }
                    }
                }
                let o = (y * size + x) * 4;
                if n > 0 {
                    for k in 0..3 {
                        out[o + k] = (sum[k] / n as f32 * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                    out[o + 3] = (n as f32 / (ss * ss) as f32 * 255.0).round() as u8;
                }
            }
        }
        out
    }

    /// The texture at (u, v), bilinear, clamped at its edges.
    fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        let fx = (u * TEX as f32 - 0.5).clamp(0.0, (TEX - 1) as f32);
        let fy = (v * TEX as f32 - 0.5).clamp(0.0, (TEX - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(TEX - 1), (y0 + 1).min(TEX - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let at = |x: usize, y: usize| self.tex[y * TEX + x];
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        [0, 1, 2].map(|k| (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-triangle icon of the format, its texture one colour: what a test
    /// can make without the game's.
    fn synthetic(shapes: u32, texel: u16) -> Vec<u8> {
        let mut d = Vec::new();
        for v in [0x0001_0000u32, shapes, 7, 0x3F80_0000, 3] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        // a triangle facing -z, its UVs over the texture
        let verts = [([-4096i16, 0, 0], [0i16, 0]), ([4096, 0, 0], [4096, 0]), ([0, -8192, 0], [2048, 4096])];
        for (p, uv) in verts {
            for _ in 0..shapes {
                for c in [p[0], p[1], p[2], 0] {
                    d.extend_from_slice(&c.to_le_bytes());
                }
            }
            for c in [0i16, 0, -4096, 0] {
                d.extend_from_slice(&c.to_le_bytes());
            }
            for c in uv {
                d.extend_from_slice(&c.to_le_bytes());
            }
            d.extend_from_slice(&[0x80, 0x80, 0x80, 0x80]);
        }
        // an animation block of no frames (the reader skips to the texture)
        d.extend_from_slice(&[0u8; 20]);
        for _ in 0..TEX * TEX {
            d.extend_from_slice(&texel.to_le_bytes());
        }
        d
    }

    fn coverage(rgba: &[u8]) -> f32 {
        rgba.chunks(4).map(|p| p[3] as f32 / 255.0).sum::<f32>() / (rgba.len() / 4) as f32
    }

    #[test]
    fn a_synthetic_icon_draws_its_triangle_in_its_colour() {
        // pure red in PSMCT16: red is the low five bits
        let icon = parse(&synthetic(2, 31)).expect("parses");
        let rgba = icon.render(64);
        let cov = coverage(&rgba);
        // a triangle is half its box; the box fills the fitted square less the margin
        assert!(cov > 0.25 && cov < 0.55, "coverage {cov}");
        let (r, g, b) = rgba.chunks(4).filter(|p| p[3] == 255).fold((0u64, 0u64, 0u64), |a, p| (a.0 + p[0] as u64, a.1 + p[1] as u64, a.2 + p[2] as u64));
        assert!(r > 10 * (g + b + 1), "red {r} green {g} blue {b}");
        // the apex (up, -y) is at the top of the picture: the top row's middle
        // is drawn before its bottom corners' rows are
        let row = |y: usize| (0..64).any(|x| rgba[(y * 64 + x) * 4 + 3] > 0);
        let top = (0..64).find(|&y| row(y)).unwrap();
        let mid = (top..64).find(|&y| (0..64).filter(|&x| rgba[(y * 64 + x) * 4 + 3] > 0).count() > 30).unwrap();
        assert!(mid > top + 10, "the triangle's point is at the top (top {top}, wide from {mid})");
    }

    #[test]
    fn not_an_icon_is_refused() {
        // control: the same bytes with the header's version, the shape count
        // or the triangle count wrong
        let good = synthetic(1, 31);
        assert!(parse(&good).is_some());
        let mut bad = good.clone();
        bad[0] = 1;
        assert!(parse(&bad).is_none(), "a wrong version");
        let mut bad = good.clone();
        bad[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse(&bad).is_none(), "no shapes");
        let mut bad = good.clone();
        bad[16..20].copy_from_slice(&4u32.to_le_bytes());
        assert!(parse(&bad).is_none(), "not a whole number of triangles");
        let mut bad = good;
        bad[8..12].copy_from_slice(&15u32.to_le_bytes());
        assert!(parse(&bad).is_none(), "a compressed texture is not read");
    }

    /// The player's SUPPORT, when this checkout has one (`run/support.bin`,
    /// `$EQOA_SUPPORT`); the test passes without, saying so.
    fn support() -> Option<Vec<u8>> {
        let path = std::env::var("EQOA_SUPPORT").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../run/support.bin").into());
        std::fs::read(path).ok()
    }

    #[test]
    fn supports_own_icon_is_a_face_not_noise() {
        let Some(img) = support() else {
            eprintln!("no run/support.bin: the save icon's own test skipped");
            return;
        };
        let icon = read(&img).expect("SUPPORT's save icon");
        assert_eq!(icon.pos.len(), 792, "792 vertices, 264 triangles");
        let rgba = icon.render(64);
        let cov = coverage(&rgba);
        assert!(cov > 0.3 && cov < 0.7, "coverage {cov}");
        // a face: warm (more red than blue), neither black nor a flat colour
        let solid: Vec<&[u8]> = rgba.chunks(4).filter(|p| p[3] == 255).collect();
        let mean = |k: usize| solid.iter().map(|p| p[k] as f32).sum::<f32>() / solid.len() as f32;
        let (r, g, b) = (mean(0), mean(1), mean(2));
        assert!(r > b + 20.0 && r > 80.0, "a skin colour: {r} {g} {b}");
        let var = solid.iter().map(|p| (p[0] as f32 - r).powi(2)).sum::<f32>() / solid.len() as f32;
        assert!(var > 100.0, "shaded, not flat: variance {var}");
        // control: the same model on a texture of zeros is black, which the
        // colour test refuses
        let mut dark = img.clone();
        let o = ICON_VA - IMAGE_BASE + ICON_LEN - TEX_BYTES;
        dark[o..o + TEX_BYTES].fill(0);
        let black = read(&dark).unwrap().render(64);
        let lit: Vec<&[u8]> = black.chunks(4).filter(|p| p[3] == 255).collect();
        let red = lit.iter().map(|p| p[0] as f32).sum::<f32>() / lit.len() as f32;
        assert!(red < 5.0, "no texture, no face: {red}");
        // and a SUPPORT that is not the Frontiers build's (the icon moved) is refused
        assert!(read(&img[1..]).is_none());
    }
}
