// The billboard program's maths (decomp/vu: VIBbRender), on the GPU: each
// particle's centre in view space, its corners (±width, ±height) about it at
// its own depth, so the quad faces the screen; the UVs from the seven
// sprite-sheet layouts; the colour the gradient's times 128, modulating the
// texture as the GS's MODULATE does; no fog (the billboard PRIM has none).

struct Frame {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    // x: the alpha scale (1 unblended, 0.5 blended: a × 128 or × 64), y: 1
    // to alpha-test at 128 (an unblended layer's TEST), z: 1 for the sky's
    // (at the far plane)
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
};

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    let c = frame.view * vec4<f32>(i.center, 1.0);
    o.pos = frame.proj * vec4<f32>(c.xy + i.corner, c.z, 1.0);
    if (frame.flags.z > 0.5) {
        o.pos.z = o.pos.w;
    }
    o.uv = i.uv;
    o.color = i.color;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    let a = t.a * i.color.a * frame.flags.x;
    if (frame.flags.y > 0.5 && a < 0.5) {
        discard;
    }
    return vec4<f32>(min(t.rgb * i.color.rgb, vec3<f32>(1.0)), a);
}
