// The fans a PS2 frame keeps: ground fog blended as the GS blends it into
// the game's 16-bit frame (`fog16.rs`). `VIRaster::Init` makes the frame
// PSMCT16S and sets DTHE in every draw environment; a material without the
// dither flag (an effect volume's) is drawn under the DIMX matrix
// 0x2031312020313120, whose entries are 0 to 3. So each fan's blend is
//
//   C = clamp(C + ((S - C) * As >> 7) + d, 0, 255) & ~7
//
// with C a multiple of 8 (the frame's five bits), As the fan's alpha (0x80
// one) and d the pixel's matrix entry: a lift of four levels or less never
// shows, a darkening of four or less shows twice over, and the fog stops a
// good way short of its own colour. That needs the frame as each fan finds
// it, which a blend cannot read; so the fans add up what fell on each pixel
// (`fans.wgsl` `fs_sums`: how many, their alphas, alphas squared, alphas
// times colour and times colour squared), and here the GS's own steps run
// over as many fans, a standard deviation's alpha above the mean and one
// below, and a standard deviation's colour above the mean and one below, in
// turn (the count and every sum kept), from the frame before the fog (the
// copy), under the pixel's own entry d. What it changes the copy by is what
// the fog changes the frame by.

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var sums: texture_2d<f32>;
@group(0) @binding(2) var squares: texture_2d<f32>;
@group(0) @binding(3) var colours: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

// The pixel's entry of DIMX 0x2031312020313120: bits 4 * (4 * (y & 3) +
// (x & 3)), the two halves the same (rows 2 and 3 repeat rows 0 and 1).
fn dither(p: vec2<i32>) -> i32 {
    return i32((0x20313120u >> (4u * u32(4 * (p.y & 1) + (p.x & 3)))) & 7u);
}

// One fan's blend into a 16-bit frame, as the GS does it: ALPHA_1's
// (Cs - Cd) * As >> 7 + Cd, the dither's d, COLCLAMP, the low three bits cut.
fn gs_step(c: i32, s: i32, a: i32, d: i32) -> i32 {
    return clamp(c + (((s - c) * a) >> 7u) + d, 0, 255) & ~7;
}

// The frame's level after `n` fans from `c0` under the matrix entry `d`,
// four kinds in turn: the colours `light` and `dark` each at the alphas `hi`
// and `lo`. Once a round of the four leaves it where it was, every round
// after does too: only the fans left over are stepped.
fn gs_fans(c0: i32, light: i32, dark: i32, hi: i32, lo: i32, n: i32, d: i32) -> i32 {
    var c = c0;
    var k = 0;
    loop {
        if (k >= n) {
            break;
        }
        let was = c;
        c = gs_step(c, light, hi, d);
        if (k + 1 < n) {
            c = gs_step(c, dark, lo, d);
        }
        if (k + 2 < n) {
            c = gs_step(c, dark, hi, d);
        }
        if (k + 3 < n) {
            c = gs_step(c, light, lo, d);
        }
        k += 4;
        if (c == was) {
            let left = select(0, n - k, k < n) % 4;
            if (left > 0) {
                c = gs_step(c, light, hi, d);
            }
            if (left > 1) {
                c = gs_step(c, dark, lo, d);
            }
            if (left > 2) {
                c = gs_step(c, dark, hi, d);
            }
            break;
        }
    }
    return c;
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(at.xy);
    let w = textureLoad(sums, p, 0);
    if (w.a <= 0.0) {
        discard;
    }
    let cd = textureLoad(scene, p, 0).rgb;
    let q = textureLoad(squares, p, 0).rg;
    // the fans: as many, their alphas a standard deviation above and below
    // the mean in turn, which keeps the count and both sums; where the
    // alphas lean so far that the one below would be under 0 (a few strong
    // fans among faint ones), as many equal fans as keep both sums (the rest
    // are fans of nothing). The two meet where the mean is the deviation. A
    // GPU divides by a reciprocal a bit off (0.375 / 1.5 comes out under
    // 0.25), so an As a whole number is taken to within a thousandth.
    let count = max(q.y, 1.0);
    let mean = w.a / count;
    let sd = sqrt(max(q.x / count - mean * mean, 0.0));
    var fans = count;
    var strong = mean + sd;
    var faint = mean - sd;
    if (mean < sd && q.x > 0.0) {
        fans = w.a * w.a / q.x;
        strong = q.x / w.a;
        faint = strong;
    }
    let n = i32(min(floor(fans + 0.5), 1024.0));
    let hi = i32(floor(min(strong, 2.0) * 128.0 + 1e-3));
    let lo = i32(floor(max(faint, 0.0) * 128.0 + 1e-3));
    // the colours: a standard deviation above and below the mean, by alpha
    let mean_c = w.rgb / w.a;
    let sd_c = sqrt(max(textureLoad(colours, p, 0).rgb / w.a, vec3<f32>(0.0)));
    let light = vec3<i32>(clamp(floor((mean_c + sd_c) * 255.0 + 0.5), vec3<f32>(0.0), vec3<f32>(255.0)));
    let dark = vec3<i32>(clamp(floor((mean_c - sd_c) * 255.0 + 0.5), vec3<f32>(0.0), vec3<f32>(255.0)));
    // the frame before the fog as the PS2's holds it: five bits a channel,
    // the nearest (halves up)
    let c0 = clamp(vec3<i32>(floor(cd * 255.0 / 8.0 + 0.5)) * 8, vec3<i32>(0), vec3<i32>(248));
    // under the pixel's own matrix entry, as every fan there is on the PS2
    let d = dither(p);
    let c = vec3<i32>(gs_fans(c0.x, light.x, dark.x, hi, lo, n, d), gs_fans(c0.y, light.y, dark.y, hi, lo, n, d), gs_fans(c0.z, light.z, dark.z, hi, lo, n, d));
    return vec4<f32>(clamp(cd + vec3<f32>(c - c0) / 255.0, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// One fan alone (the sequential reference, `Renderer::set_ps2_sequential`):
// its step under the pixel's own matrix entry, the GS's arithmetic exactly.
@fragment
fn fs_one(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(at.xy);
    let w = textureLoad(sums, p, 0);
    if (w.a <= 0.0) {
        discard;
    }
    let cd = textureLoad(scene, p, 0).rgb;
    let a = i32(floor(min(w.a, 2.0) * 128.0 + 1e-3));
    let s = vec3<i32>(clamp(floor(w.rgb / w.a * 255.0 + 0.5), vec3<f32>(0.0), vec3<f32>(255.0)));
    let c0 = clamp(vec3<i32>(floor(cd * 255.0 / 8.0 + 0.5)) * 8, vec3<i32>(0), vec3<i32>(248));
    let d = dither(p);
    let c = vec3<i32>(gs_step(c0.x, s.x, a, d), gs_step(c0.y, s.y, a, d), gs_step(c0.z, s.z, a, d));
    return vec4<f32>(clamp(cd + vec3<f32>(c - c0) / 255.0, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
