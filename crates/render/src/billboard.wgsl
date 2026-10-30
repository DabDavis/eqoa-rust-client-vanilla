// The billboard program's maths (decomp/vu: VIBbRender), on the GPU: each
// particle's centre in view space, its corners (±width, ±height) about it at
// its own depth, so the quad faces the screen; the UVs from the seven
// sprite-sheet layouts; the colour the gradient's times 128, modulating the
// texture as the GS's MODULATE does; no fog (the billboard PRIM has none).
//
// The alpha is the texel's times the particle's, whether the layer tests or
// blends. The program sends a × 128 when the layer tests and a × 64 when it
// blends (`BeginBillboards`' alpha scale), and MODULATE makes As = At × that
// / 128, At the texel's 0..255: the test compares As with 128, so a pixel
// passes at At × a ≥ 128 (texel × particle ≥ 0.5 here), and the blend takes
// As / 128 (0x80 is one) as its factor, At × a / 256 (texel × particle). The
// 64 is what keeps a texel of 255 from blending twice over, not a half.

struct Frame {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    // x: 1 to alpha-test at 128 (an unblended layer's TEST), y: 1 for the
    // sky's (at the far plane), z: the GS's K (levels: `effects_lod_k`)
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) center: vec3<f32>,
    @location(1) corner: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // the depth before the camera, the texel's level (1/Q: shader.wgsl's)
    @location(2) depth: f32,
};

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    let c = frame.view * vec4<f32>(i.center, 1.0);
    o.pos = frame.proj * vec4<f32>(c.xy + i.corner, c.z, 1.0);
    if (frame.flags.y > 0.5) {
        o.pos.z = o.pos.w;
    }
    o.uv = i.uv;
    o.color = i.color;
    o.depth = o.pos.w;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSampleLevel(tex, samp, i.uv, log2(max(i.depth, 1e-6)) + frame.flags.z);
    let a = t.a * i.color.a;
    if (frame.flags.x > 0.5 && a < 0.5) {
        discard;
    }
    return vec4<f32>(min(t.rgb * i.color.rgb, vec3<f32>(1.0)), a);
}
