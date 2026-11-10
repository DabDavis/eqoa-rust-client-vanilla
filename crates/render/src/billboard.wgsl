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

// The particle's colour and alpha at a pixel, as the GS takes them to its
// blend.
fn shade(i: VsOut) -> vec4<f32> {
    let t = textureSampleLevel(tex, samp, i.uv, log2(max(i.depth, 1e-6)) + frame.flags.z);
    return vec4<f32>(min(t.rgb * i.color.rgb, vec3<f32>(1.0)), t.a * i.color.a);
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let c = shade(i);
    if (frame.flags.x > 0.5 && c.a < 0.5) {
        discard;
    }
    return c;
}

// The particles a PS2 frame keeps (`fog16.wgsl`, as the fans' in
// `fans.wgsl`): each adds its alpha a, a times its colour, a squared and one
// (itself); then, over those sums, a times its colour's distance from their
// mean squared (`fs_spread`), which `fog16.wgsl` makes into the particles it
// blends as the GS does.
struct Sums {
    @location(0) sums: vec4<f32>,
    @location(1) squares: vec4<f32>,
};

@fragment
fn fs_sums(i: VsOut) -> Sums {
    let c = shade(i);
    // a particle whose As is 0 changes nothing on a PS2: not counted
    if (c.a * 128.0 < 1.0) {
        discard;
    }
    var o: Sums;
    o.sums = vec4<f32>(c.rgb * c.a, c.a);
    o.squares = vec4<f32>(c.a * c.a, 1.0, 0.0, 0.0);
    return o;
}

// The sums `fs_sums` made: the mean colour each particle's spread is taken
// from.
@group(2) @binding(0) var sums_made: texture_2d<f32>;

@fragment
fn fs_spread(i: VsOut) -> @location(0) vec4<f32> {
    let c = shade(i);
    if (c.a * 128.0 < 1.0) {
        discard;
    }
    let w = textureLoad(sums_made, vec2<i32>(i.pos.xy), 0);
    let off = c.rgb - w.rgb / max(w.a, 1e-6);
    return vec4<f32>(off * off * c.a, 0.0);
}
