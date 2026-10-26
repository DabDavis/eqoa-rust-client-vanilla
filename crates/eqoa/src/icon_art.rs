// The program's own picture, drawn here and nowhere else: a disc (F3 pass 17,
// the user: "the program icon is ugly ... we can use a disc icon for the
// program", a drawn disc, no game art). A PS2-era disc is blue-black
// underneath, so the data side is a deep blue with a cyan-to-violet sheen
// running through it from the top left, round a silver hub and its hole, with
// a dark edge that keeps it apart from a light taskbar or a dark one.
//
// Plain std, outer comments only: build.rs `include!`s this file to make the
// Windows program's .ico from the same drawing, and `icon` takes it as the
// window's and the Dock's picture until the player's own disc gives the
// game's save icon (`saveicon`).

/// The disc, `size` × `size`, RGBA with straight alpha: the corners and the
/// hole clear, edges anti-aliased (each pixel the mean of a grid of samples).
pub fn disc_rgba(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let n = if size <= 64 { 8 } else { 4 };
    let half = size as f32 / 2.0;
    // the disc fills the square but for half a pixel at each side
    let radius = half - 0.5;
    let px = 1.0 / radius;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for j in 0..size {
        for i in 0..size {
            let mut acc = [0.0f32; 4];
            for sj in 0..n {
                for si in 0..n {
                    let x = (i as f32 + (si as f32 + 0.5) / n as f32 - half) / radius;
                    let y = (j as f32 + (sj as f32 + 0.5) / n as f32 - half) / radius;
                    let s = disc_sample(x, y, px);
                    for k in 0..4 {
                        acc[k] += s[k];
                    }
                }
            }
            let count = (n * n) as f32;
            let a = acc[3] / count;
            let o = ((j * size + i) * 4) as usize;
            if a > 0.0 {
                for k in 0..3 {
                    out[o + k] = (acc[k] / count / a * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
            out[o + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Where the disc's parts end, as a share of its radius, from the centre out.
pub const DISC_HOLE: f32 = 0.155;
const DISC_LIP: f32 = 0.19;
const DISC_HUB: f32 = 0.295;
const DISC_GROOVE: f32 = 0.315;
const DISC_CLEAR: f32 = 0.385;
const DISC_INNER_EDGE: f32 = 0.405;
const DISC_RIM: f32 = 0.94;

/// One sample, premultiplied RGBA in 0..1; `x`, `y` across the disc in -1..1
/// (y down), `px` one pixel in those units (the edge is never thinner than
/// most of one).
fn disc_sample(x: f32, y: f32, px: f32) -> [f32; 4] {
    let r = (x * x + y * y).sqrt();
    if r > 1.0 || r <= DISC_HOLE {
        return [0.0; 4];
    }
    let edge = 1.0 - (0.8 * px).max(0.022);
    // the light from the top left: a streak through the centre and a faint
    // one across it, as a disc catches a lamp
    let th = y.atan2(x);
    let a = th + 0.75 * std::f32::consts::PI;
    let streak = (2.0 * a).cos().max(0.0).powi(3);
    let cross = (2.0 * a + std::f32::consts::PI).cos().max(0.0).powi(12) * 0.14;
    let (rgb, alpha) = if r > edge {
        (disc_hex(0x0a0f1f), 1.0)
    } else if r > DISC_RIM {
        (disc_mix(disc_hex(0x3a59b0), disc_hex(0xa9c0f2), streak), 1.0)
    } else if r > DISC_INNER_EDGE {
        let t = ((r - DISC_INNER_EDGE) / (DISC_RIM - DISC_INNER_EDGE)).powf(0.8);
        let mut c = disc_mix(disc_hex(0x0c1840), disc_hex(0x22408f), t);
        // the tracks, seen only large
        let tracks = 1.0 + 0.04 * (r * 160.0).sin();
        for v in &mut c {
            *v *= tracks;
        }
        // the sheen: cyan to violet in bands across the radius
        let hue = 0.6 + 0.16 * (r * 7.0 + 0.6).sin();
        let sheen = disc_hsv(hue, 0.5, 1.0);
        let k = 0.7 * streak * (0.3 + 0.7 * t) + cross;
        (disc_mix(c, sheen, k.min(1.0)), 1.0)
    } else if r > DISC_CLEAR {
        (disc_hex(0x08112c), 1.0)
    } else if r > DISC_GROOVE {
        // the clear plastic round the hub
        (disc_mix(disc_hex(0x9aaac2), disc_hex(0xdfe7f2), streak), 0.78)
    } else if r > DISC_HUB {
        (disc_hex(0x66748c), 1.0)
    } else if r > DISC_LIP {
        (disc_mix(disc_hex(0xbfc8d6), disc_hex(0xf2f5fa), streak * 0.9 + cross), 1.0)
    } else {
        (disc_hex(0x56637a), 1.0)
    };
    [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha]
}

fn disc_hex(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

fn disc_mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn disc_hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}
