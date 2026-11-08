// Ambient Occlusion (ao.rs). From the depth the opaque half of the frame
// wrote: a pyramid of it, four levels from a small size down, side by side
// in one texture (fs_z); how much of the space round each pixel is filled,
// at the small size (fs_ao); that blurred across and down without crossing a
// depth edge (fs_blur_h, fs_blur_v); and multiplied into the frame at its
// own size (fs_apply). The small size is the frame's over `1 << mode.y`:
// half the window's, or a quarter past 1,440 lines (and the options' Render
// Scale's frame as much smaller again), so the cost does not grow with a
// display's pixels. Its texture holds the factor in r and the view depth in
// g and b (16 bits over `KEY_RANGE`), which the blur and the upsampling
// weigh by.
//
// A sample's depth comes from the pyramid's level for its distance (twice
// as coarse for each doubling past `NEAR_TEXELS`), as Scalable Ambient
// Obscurance takes it (McGuire, Mara and Luebke 2012): read straight off the
// frame's depth, the samples of neighbouring pixels land far apart and the
// reads are most of the cost (2.5 of 4 ms at 3840x2160 on an RX 580).
// A level holds the depth of the frame's pixel at its texel's corner, as
// the float's own bits in four bytes, which every GPU (WebGL2's too) can
// draw into and gives back exactly.
//
// Positions are relative to the eye: `inverse` undoes the frame's own
// eye-relative view-projection (shader.wgsl), so they are small near the
// camera wherever the camera is (a world position at Freeport, x 25,300,
// steps 2 mm and its neighbours' differences would be noise).
//
// The frame's depth is read through a nearest sampler, not textureLoad:
// wgpu's GL backend leaves a depth texture's own filters linear, which
// GLES 3.0 counts as incomplete for a texelFetch; a bound sampler's count.

struct Ao {
    inverse: mat4x4<f32>,  // clip -> relative to the eye
    size: vec4<f32>,       // the frame's width and height, then the small size's
    params: vec4<f32>,     // radius, intensity, bias, pixels a unit at view depth 1
    fog: vec4<f32>,        // near, far, 1 when on
    mode: vec4<u32>,       // x: 1 on, 2 the factor, 3 view depth, 4 normals; y: the small size's shift
    levels: array<vec4<i32>, 4>,  // the pyramid's: where each starts, its size
};
@group(0) @binding(0) var depth: texture_2d<f32>;
@group(0) @binding(1) var small: texture_2d<f32>;
@group(0) @binding(2) var<uniform> u: Ao;
@group(0) @binding(3) var nearest: sampler;
@group(0) @binding(4) var pyramid: texture_2d<f32>;

const TAU: f32 = 6.2831853;
// samples on a spiral of `TURNS` turns round the pixel, `STEP` apart
const SAMPLES: i32 = 12;
const TURNS: f32 = 5.0;
const STEP: f32 = 2.6179939; // TURNS * TAU / SAMPLES
// the farthest a sample lands, as a share of the frame's height (96 pixels
// at 1,080 lines), so the picture is the same at any size
const REACH: f32 = 0.0889;
// samples nearer than this (small texels) read the pyramid's level 0
const NEAR_TEXELS: f32 = 8.0;
// the view depths the stored key spans
const KEY_RANGE: f32 = 512.0;
// view depths the factor fades out over (the depth's steps grow with it)
const FADE_FROM: f32 = 150.0;
const FADE_TO: f32 = 250.0;

// one triangle over the whole frame
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn depth_at(p: vec2<i32>) -> f32 {
    let q = clamp(p, vec2<i32>(0), vec2<i32>(u.size.xy) - vec2<i32>(1));
    return textureSampleLevel(depth, nearest, (vec2<f32>(q) + 0.5) / u.size.xy, 0.0).r;
}

// `inverse` times the frame's pixel `p` at depth `d`: its position relative
// to the eye over w, and one over its view depth in w.
fn unproject(p: vec2<i32>, d: f32) -> vec4<f32> {
    let uv = (vec2<f32>(p) + 0.5) / u.size.xy;
    return u.inverse * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
}

fn position(p: vec2<i32>, d: f32) -> vec3<f32> {
    let h = unproject(p, d);
    return h.xyz / h.w;
}

fn view_depth(p: vec2<i32>, d: f32) -> f32 {
    return 1.0 / unproject(p, d).w;
}

// A depth as its float's bits in four bytes, and back.
fn pack_depth(d: f32) -> vec4<f32> {
    let b = bitcast<u32>(d);
    return vec4<f32>(vec4<u32>(b & 255u, (b >> 8u) & 255u, (b >> 16u) & 255u, b >> 24u)) / 255.0;
}

fn unpack_depth(c: vec4<f32>) -> f32 {
    let b = vec4<u32>(round(c * 255.0));
    return bitcast<f32>(b.x | (b.y << 8u) | (b.z << 16u) | (b.w << 24u));
}

// The pyramid's level `l` at the small texel `h` (clamped to it): the
// frame's pixel it holds the depth of, and that depth.
fn level_at(l: i32, h: vec2<i32>) -> vec3<f32> {
    let level = u.levels[l];
    let t = clamp(h >> vec2<u32>(u32(l)), vec2<i32>(0), level.zw - vec2<i32>(1));
    let d = unpack_depth(textureLoad(pyramid, level.xy + t, 0));
    return vec3<f32>(vec2<f32>(t << vec2<u32>(u32(l) + u.mode.y)), d);
}

// The pyramid: each level's texel holds the depth of the frame's pixel at
// its corner (level 0 one a small texel, each level after one in four).
@fragment
fn fs_z(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let a = vec2<i32>(at.xy);
    for (var l = 0; l < 4; l++) {
        let level = u.levels[l];
        let t = a - level.xy;
        if all(t >= vec2<i32>(0)) && all(t < level.zw) {
            return pack_depth(depth_at(t << vec2<u32>(u32(l) + u.mode.y)));
        }
    }
    return vec4<f32>(0.0);
}

// The surface's normal at `c` facing the eye, from the positions either side
// of it across (`l`, `r`) and down (`t`, `b`): on each axis the nearer one,
// so it is not taken across an edge (and not one a border clamps onto `c`).
fn normal_from(c: vec3<f32>, l: vec3<f32>, r: vec3<f32>, t: vec3<f32>, b: vec3<f32>) -> vec3<f32> {
    let dl = c - l;
    let dr = r - c;
    let dt = c - t;
    let db = b - c;
    let dx = select(dr, dl, dot(dl, dl) > 0.0 && (dot(dl, dl) < dot(dr, dr) || dot(dr, dr) == 0.0));
    let dy = select(db, dt, dot(dt, dt) > 0.0 && (dot(dt, dt) < dot(db, db) || dot(db, db) == 0.0));
    var n = cross(dx, dy);
    let len2 = dot(n, n);
    if len2 < 1e-20 {
        return normalize(-c);
    }
    n = n * inverseSqrt(len2);
    return select(n, -n, dot(n, c) > 0.0);
}

fn level0_position(h: vec2<i32>) -> vec3<f32> {
    let s = level_at(0, h);
    return position(vec2<i32>(s.xy), s.z);
}

// A key in [0, 1] as two bytes' worth, and back.
fn pack_key(x: f32) -> vec2<f32> {
    let y = clamp(x, 0.0, 1.0) * 255.0;
    let hi = floor(y);
    return vec2<f32>(hi / 255.0, y - hi);
}

fn unpack_key(k: vec2<f32>) -> f32 {
    return (round(k.x * 255.0) + k.y) / 255.0;
}

// A turn of the spiral per pixel, so neighbours sample differently and the
// blur averages them (interleaved gradient noise).
fn spin(h: vec2<i32>) -> f32 {
    let f = vec2<f32>(h);
    return TAU * fract(52.9829189 * fract(dot(f, vec2<f32>(0.06711056, 0.00583715))));
}

// The small pass: its pixel `h` stands for the frame's pixel at its
// coordinates times the scale (the pyramid's level 0). A sample counts by how far above
// the pixel's plane it is (a sine) and fades to nothing at the radius, so a
// wall a step away darkens the floor at its foot and a wall across the room
// does not.
@fragment
fn fs_ao(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let h = vec2<i32>(at.xy);
    let s = level_at(0, h);
    let p = vec2<i32>(s.xy);
    let d = s.z;
    if d >= 1.0 {
        return vec4<f32>(1.0, pack_key(1.0), 1.0);
    }
    let hc = unproject(p, d);
    let c = hc.xyz / hc.w;
    let w = 1.0 / hc.w;
    let one = vec2<i32>(1, 0);
    let n = normal_from(c, level0_position(h - one), level0_position(h + one), level0_position(h - one.yx), level0_position(h + one.yx));
    let radius = u.params.x;
    // the radius in small texels here
    let reach = min(radius * u.params.w / max(w, 1e-3), REACH * u.size.y) / f32(1u << u.mode.y);
    var a = 1.0;
    if reach >= 1.0 {
        // the spiral's direction turned a step a sample (one cosine and sine
        // a pixel, not one a sample)
        let turn = spin(h) + 0.5 * STEP;
        var dir = vec2<f32>(cos(turn), sin(turn));
        let step = vec2<f32>(cos(STEP), sin(STEP));
        let r2 = radius * radius;
        let last = vec2<i32>(u.size.zw) - vec2<i32>(1);
        var sum = 0.0;
        for (var i = 0; i < SAMPLES; i++) {
            let out = (f32(i) + 0.5) / f32(SAMPLES) * reach;
            // kept on the frame: off it, the border's depth at the sample's
            // own place would be a point that is not there (above the ground
            // at the top edge, which then reads as filled)
            let q = clamp(h + vec2<i32>(round(dir * out)), vec2<i32>(0), last);
            dir = vec2<f32>(dir.x * step.x - dir.y * step.y, dir.x * step.y + dir.y * step.x);
            let sq = level_at(clamp(i32(floor(log2(max(out / NEAR_TEXELS, 1.0)))), 0, 3), q);
            if sq.z >= 1.0 {
                continue;
            }
            let v = position(vec2<i32>(sq.xy), sq.z) - c;
            let vv = dot(v, v);
            let rise = max(dot(v, n) * inverseSqrt(vv + 1e-8) - u.params.z, 0.0);
            sum += rise * max(1.0 - vv / r2, 0.0);
        }
        a = max(1.0 - u.params.y * 2.0 * sum / f32(SAMPLES), 0.0);
    }
    return vec4<f32>(a, pack_key(w / KEY_RANGE), 1.0);
}

// Nine taps along `dir`, each weighed by its distance and by how near its
// depth is to the middle one's (none across an edge).
fn blur(at: vec4<f32>, dir: vec2<i32>) -> vec4<f32> {
    let h = vec2<i32>(at.xy);
    let last = vec2<i32>(u.size.zw) - vec2<i32>(1);
    let mid = textureLoad(small, h, 0);
    let w0 = unpack_key(mid.yz) * KEY_RANGE;
    var weights = array<f32, 5>(1.0, 0.88, 0.61, 0.32, 0.14);
    let tolerance = 0.03 * w0 + 0.05;
    var sum = mid.x;
    var total = 1.0;
    for (var k = 1; k <= 4; k++) {
        for (var side = -1; side <= 1; side += 2) {
            let s = textureLoad(small, clamp(h + dir * (k * side), vec2<i32>(0), last), 0);
            let g = weights[k] * max(1.0 - abs(unpack_key(s.yz) * KEY_RANGE - w0) / tolerance, 0.0);
            sum += s.x * g;
            total += g;
        }
    }
    return vec4<f32>(sum / total, mid.yz, 1.0);
}

@fragment
fn fs_blur_h(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(at, vec2<i32>(1, 0));
}

@fragment
fn fs_blur_v(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(at, vec2<i32>(0, 1));
}

// The frame's pixel: the four nearest small factors, each by its
// distance and by how near its depth is to this pixel's, faded with distance
// and with the fog (which the strip programs mix in after the light), and
// multiplied in (the pipeline's blend). The diagnostic views replace it.
// A small texel stands for the frame's pixel at its corner (fs_ao, level 0),
// so the pixel `p` is at `p` over the scale in small texels.
@fragment
fn fs_apply(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(at.xy);
    let d = depth_at(p);
    if d >= 1.0 {
        return vec4<f32>(1.0);
    }
    let w = view_depth(p, d);
    let mode = u.mode.x;
    if mode == 3u {
        return vec4<f32>(vec3<f32>(clamp(w / 100.0, 0.0, 1.0)), 1.0);
    }
    if mode == 4u {
        let one = vec2<i32>(1, 0);
        let n = normal_from(position(p, d), position(p - one, depth_at(p - one)), position(p + one, depth_at(p + one)), position(p - one.yx, depth_at(p - one.yx)), position(p + one.yx, depth_at(p + one.yx)));
        return vec4<f32>(n * 0.5 + 0.5, 1.0);
    }
    let f = vec2<f32>(p) / f32(1u << u.mode.y);
    let b = floor(f);
    let t = f - b;
    let last = vec2<i32>(u.size.zw) - vec2<i32>(1);
    let tolerance = 0.02 * w + 0.05;
    var sum = 0.0;
    var total = 0.0;
    for (var k = 0; k < 4; k++) {
        let o = vec2<i32>(k & 1, k >> 1u);
        let s = textureLoad(small, clamp(vec2<i32>(b) + o, vec2<i32>(0), last), 0);
        let near = select(1.0 - t, t, o == vec2<i32>(1));
        let e = (unpack_key(s.yz) * KEY_RANGE - w) / tolerance;
        let g = (near.x * near.y + 1e-3) / (1.0 + 16.0 * e * e);
        sum += s.x * g;
        total += g;
    }
    var a = sum / total;
    var shown = 1.0 - smoothstep(FADE_FROM, FADE_TO, w);
    if u.fog.z > 0.5 {
        shown *= clamp((u.fog.y - w) / (u.fog.y - u.fog.x), 0.0, 1.0);
    }
    a = mix(1.0, a, shown);
    return vec4<f32>(a, a, a, 1.0);
}
