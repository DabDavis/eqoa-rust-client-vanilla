// Shading after the VU1 strip programs (claude-notes/vu1-microcode-
// disassembly-20261017.md, verified there against PCSX2):
//
//   light = clamp(colour + ambient + sun · max(−N·S, 0) · colour.a, 0, 1)
//   pixel = texture × layer colour (0x80 = 1.0, the GS's modulate) × light
//
// where S is the direction the sun's light travels and, on packed geometry,
// the vertex alpha is a sun-visibility factor. Not yet ported: the point
// light, the static-lighting term, fog, and every layer after the first.

struct Frame {
    view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,
    sun: vec4<f32>,
    ambient: vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame: Frame;

struct Material {
    tint: vec4<f32>,
    flags: vec4<f32>, // x: 1 when textured
};
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> material: Material;

// The skin palette, `VIRStrip4`'s: four node matrices per vertex, weighted.
// Unskinned models carry identities and bone 255 (unused).
struct Palette {
    m: array<mat4x4<f32>, 64>,
};
@group(2) @binding(0) var<uniform> palette: Palette;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) bones: vec4<u32>,
    @location(9) weights: vec4<u32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var pos = vec4<f32>(v.pos, 1.0);
    var nrm = vec4<f32>(v.normal, 0.0);
    let wsum = f32(v.weights.x + v.weights.y + v.weights.z + v.weights.w);
    if (wsum > 0.0) {
        var sp = vec4<f32>(0.0);
        var sn = vec4<f32>(0.0);
        for (var k = 0u; k < 4u; k++) {
            let b = v.bones[k];
            if (b < 64u && v.weights[k] > 0u) {
                let w = f32(v.weights[k]) / wsum;
                sp += palette.m[b] * pos * w;
                sn += palette.m[b] * nrm * w;
            }
        }
        pos = vec4<f32>(sp.xyz, 1.0);
        nrm = vec4<f32>(sn.xyz, 0.0);
    }
    o.clip = frame.view_proj * model * pos;
    o.uv = v.uv;
    // Placements scale uniformly, so the upper 3×3 is fine for normals.
    o.normal = (model * nrm).xyz;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    var texel = vec4<f32>(1.0);
    if (material.flags.x > 0.5) {
        texel = textureSample(tex, samp, i.uv);
    }
    // The base layer's alpha test: TEST 0x3080B, A >= 128 of 255.
    if (texel.a < 0.5) {
        discard;
    }
    let n = normalize(i.normal);
    let sun = frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * i.color.a;
    let light = clamp(i.color.rgb + frame.ambient.rgb + sun, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(texel.rgb * material.tint.rgb * light, 1.0);
}
