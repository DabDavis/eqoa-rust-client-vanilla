// A texture pack's stand-in still being read (standin.rs): one level of its
// texture drawn from the surface's own levels, bilinear, out of the level
// of the surface's own nearest the target's size (`level.x`), so that the
// game's own texture shows until the stand-in is written over it.

@group(0) @binding(0) var own: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
// x: the level of `own` to read
@group(0) @binding(2) var<uniform> level: vec4<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// one triangle over the whole level, its first row at the top
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: Out;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs_main(i: Out) -> @location(0) vec4<f32> {
    return textureSampleLevel(own, samp, i.uv, level.x);
}
