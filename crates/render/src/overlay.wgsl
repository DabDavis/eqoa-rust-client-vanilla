// Triangles over the finished scene with the Z test off (overlay.rs): each
// corner already through the frame's view-projection, its colour the layer
// colour times the strip program's light, as the GS gets it; the texture
// modulated by it, alpha as stored times the colour's; no fog (the caller's
// raster has fog off). Mode 0 is alpha tested at 128 (fs_tested); the others
// blend by the pipeline's state.

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) clip: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    // the GS's K (levels: `effects_lod_k`)
    @location(3) k: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // the depth before the camera, the texel's level (1/Q: shader.wgsl's)
    @location(2) depth: f32,
    @location(3) k: f32,
};

fn texel(i: VsOut) -> vec4<f32> {
    return textureSampleLevel(tex, samp, i.uv, log2(max(i.depth, 1e-6)) + i.k);
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = v.clip;
    o.uv = v.uv;
    o.color = v.color;
    o.depth = v.clip.w;
    o.k = v.k;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    return texel(i) * i.color;
}

@fragment
fn fs_tested(i: VsOut) -> @location(0) vec4<f32> {
    let c = texel(i) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return c;
}
