// The armour trim's aura (trimglow.rs): a plate piece's batches again, skinned
// as the body and pushed out a little along the smoothed normals, lit where
// the trim's mask (recolor.rs's trim_mask: r the trim, g its halo, b the
// gold's brightness, a the texel's alpha) says. A glowing trim adds its colour
// (the trim bright, its halo fainter, stronger at the silhouette); a dark one
// (params.z) darkens under itself (alpha) with a faint sheen (rgb,
// premultiplied: shell.rs's DARK_BLEND). The shell's slow shimmer and pulse,
// faded by the fog.

struct Frame {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    sun_dir: vec4<f32>,
    sun: vec4<f32>,      // w: time in seconds
    ambient: vec4<f32>,
    fog_color: vec4<f32>,  // w: on
    fog: vec4<f32>,        // near, far
    point: vec4<f32>,
    point_color: vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame: Frame;

@group(1) @binding(0) var mask: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Trim {
    color: vec4<f32>,   // rgb the aura, a its strength
    params: vec4<f32>,  // x: inflation, y: shimmer frequency, z: 1 dark
};
@group(1) @binding(2) var<uniform> trim: Trim;

struct Palette {
    m: array<mat4x4<f32>, 72>,
};
@group(2) @binding(0) var<uniform> palette: Palette;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) bones: vec4<u32>,
    @location(9) weights: vec4<u32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) rel: vec3<f32>,
    @location(2) body: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) fog: f32,
};

@vertex
fn vs_trim(v: VsIn) -> VsOut {
    var o: VsOut;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var pos = vec4<f32>(v.pos + v.normal * trim.params.x, 1.0);
    var nrm = vec4<f32>(v.normal, 0.0);
    let wsum = f32(v.weights.x + v.weights.y + v.weights.z + v.weights.w);
    if (wsum > 0.0) {
        var sp = vec4<f32>(0.0);
        var sn = vec4<f32>(0.0);
        for (var k = 0u; k < 4u; k++) {
            let b = v.bones[k];
            if (b < 72u && v.weights[k] > 0u) {
                let w = f32(v.weights[k]) / wsum;
                sp += palette.m[b] * pos * w;
                sn += palette.m[b] * nrm * w;
            }
        }
        pos = vec4<f32>(sp.xyz, 1.0);
        nrm = vec4<f32>(sn.xyz, 0.0);
    }
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    o.fog = 1.0;
    if (frame.fog_color.w > 0.5) {
        o.fog = clamp((frame.fog.y - o.clip.w) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    }
    o.normal = (model * nrm).xyz;
    o.rel = rel;
    o.body = v.pos;
    o.uv = v.uv;
    return o;
}

fn hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash(i), hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x), mix(hash(i + vec3<f32>(0.0, 1.0, 0.0)), hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash(i + vec3<f32>(0.0, 0.0, 1.0)), hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x), mix(hash(i + vec3<f32>(0.0, 1.0, 1.0)), hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

@fragment
fn fs_trim(i: VsOut) -> @location(0) vec4<f32> {
    let m = textureSample(mask, samp, i.uv);
    let core = m.r * m.a;
    let halo = m.g * m.a;
    let n = normalize(i.normal);
    let v = normalize(-i.rel);
    let facing = abs(dot(n, v));
    let rim = pow(1.0 - facing, 1.5);
    let t = frame.sun.w - 600.0 * floor(frame.sun.w / 600.0);
    let p = i.body * trim.params.y;
    let shimmer = noise(p * 1.7 + vec3<f32>(0.0, -0.7 * t, 0.3 * t));
    let pulse = 0.85 + 0.15 * sin(t * 2.2);
    if (trim.params.z > 0.5) {
        // dark: the trim taken toward black (a glint of sheen where it turns
        // from the eye), smoke off it darker at the silhouette
        let smoke = halo * (0.35 + 0.55 * rim) * (0.6 + 0.8 * shimmer);
        let a = clamp((core * 0.95 + smoke * (1.0 - core) * pulse) * trim.color.a * i.fog, 0.0, 0.95);
        let hi = max(trim.color.r, max(trim.color.g, trim.color.b));
        var sheen = vec3<f32>(0.16, 0.12, 0.24);
        if (hi >= 0.01) {
            sheen = trim.color.rgb / hi * 0.22;
        }
        let glint = core * (0.18 + 0.82 * pow(1.0 - facing, 2.0)) * (0.6 + 0.6 * shimmer) * 0.55;
        return vec4<f32>(sheen * (glint + 0.4 * smoke * rim) * i.fog, a);
    }
    // glowing: the trim lit at its gold's own brightness and more, its halo
    // fainter, brighter at the silhouette
    let lit = core * (0.1 + 0.65 * m.b) * (0.8 + 0.4 * shimmer);
    let glow = (halo - core * 0.5) * (0.3 + 0.7 * rim) * (0.6 + 0.8 * shimmer);
    let a = clamp((lit + max(glow, 0.0)) * pulse * trim.color.a * i.fog, 0.0, 1.0);
    return vec4<f32>(trim.color.rgb, a);
}
