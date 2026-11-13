// A glowing creature's aura shell (shell.rs): the body's skinned mesh again,
// each vertex pushed out along its smoothed normal by the inflation before the
// skin palette (so the shell bends as the body does), drawn additively in the
// glow's colour. Its alpha is a rim, strongest where the surface turns away
// from the eye (1 − |N·V|)^1.6, eased to nothing at the very edge), over a faint fill, times a shimmer
// (value noise in the body's own space drifting up with time) and a slow
// pulse, faded by the fog. A dark part (params.z) is a shadow instead
// (`dark_shell`), drawn premultiplied over (shell.rs's DARK_BLEND).

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

struct Shell {
    color: vec4<f32>,   // rgb the glow, a its strength
    params: vec4<f32>,  // x: inflation, y: shimmer frequency, z: 1 dark
};
@group(1) @binding(0) var<uniform> shell: Shell;

struct Palette {
    m: array<mat4x4<f32>, 72>,
};
@group(2) @binding(0) var<uniform> palette: Palette;

struct VsIn {
    @location(0) pos: vec3<f32>,
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
    // the vertex less the eye, in the world
    @location(1) rel: vec3<f32>,
    // the vertex in the body's own (bind) space, where the shimmer lives
    @location(2) body: vec3<f32>,
    @location(3) fog: f32,
};

@vertex
fn vs_shell(v: VsIn) -> VsOut {
    var o: VsOut;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var pos = vec4<f32>(v.pos + v.normal * shell.params.x, 1.0);
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
    // relative to the eye first, as shader.wgsl's vs_main
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    o.fog = 1.0;
    if (frame.fog_color.w > 0.5) {
        o.fog = clamp((frame.fog.y - o.clip.w) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    }
    o.normal = (model * nrm).xyz;
    o.rel = rel;
    o.body = v.pos;
    return o;
}

fn hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

// trilinear value noise, 0..1
fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash(i), hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x), mix(hash(i + vec3<f32>(0.0, 1.0, 0.0)), hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash(i + vec3<f32>(0.0, 0.0, 1.0)), hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x), mix(hash(i + vec3<f32>(0.0, 1.0, 1.0)), hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

@fragment
fn fs_shell(i: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let v = normalize(-i.rel);
    let facing = abs(dot(n, v));
    // strongest just inside the silhouette, softened to nothing at its very
    // edge so the shell has no hard outline
    let rim = pow(1.0 - facing, 1.6) * smoothstep(0.0, 0.3, facing);
    // the time wraps every ten minutes, so it never grows past the noise's precision
    let t = frame.sun.w - 600.0 * floor(frame.sun.w / 600.0);
    let p = i.body * shell.params.y;
    let shimmer = 0.55 + 0.9 * noise(p + vec3<f32>(0.0, -0.6 * t, 0.25 * t)) * noise(p * 2.1 + vec3<f32>(0.4 * t, 0.0, -0.3 * t));
    let pulse = 0.85 + 0.15 * sin(t * 2.2);
    if (shell.params.z > 0.5) {
        return dark_shell(facing, rim, shimmer, pulse, i.fog);
    }
    let a = (0.08 + 0.92 * rim) * shimmer * pulse * shell.color.a * i.fog;
    return vec4<f32>(shell.color.rgb, clamp(a, 0.0, 1.0));
}

// A dark aura's sheen: its colour's hue lifted to a faint light, or for black
// (no hue) a cold violet; what shows of it against a night.
fn dark_sheen(c: vec3<f32>) -> vec3<f32> {
    let hi = max(c.r, max(c.g, c.b));
    if (hi < 0.01) {
        return vec3<f32>(0.16, 0.12, 0.24);
    }
    return c / hi * 0.22;
}

// A dark part: smoke that darkens what is behind it (alpha: a fill, the rim
// stronger, the shimmer in deeper contrast), a sheen on its silhouette (rgb,
// premultiplied).
fn dark_shell(facing: f32, rim: f32, shimmer: f32, pulse: f32, fog: f32) -> vec4<f32> {
    let smoke = clamp((shimmer - 0.55) / 0.9, 0.0, 1.0);
    let a = (0.45 + 0.55 * rim) * (0.6 + 0.6 * smoke) * pulse * shell.color.a * fog;
    let edge = pow(1.0 - facing, 3.0) * smoothstep(0.0, 0.2, facing);
    let sheen = dark_sheen(shell.color.rgb) * edge * (0.6 + 0.6 * smoke) * fog;
    return vec4<f32>(sheen, clamp(a, 0.0, 0.9));
}
