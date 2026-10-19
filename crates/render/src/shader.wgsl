// Viewer shading. Texture × layer tint (0x80 = 1.0, the GS's modulate) ×
// vertex colour × a half-Lambert light. The game's own lighting is the strip
// programs' (decomp/vu); this is a stand-in until that is ported.

struct Frame {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame: Frame;

struct Material {
    tint: vec4<f32>,
    flags: vec4<f32>, // x: 1 when textured
};
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> material: Material;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = frame.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.normal = v.normal;
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
    let lambert = dot(n, -frame.light_dir.xyz) * 0.5 + 0.5;
    let rgb = texel.rgb * material.tint.rgb * i.color * (0.35 + 0.65 * lambert);
    return vec4<f32>(rgb, 1.0);
}
