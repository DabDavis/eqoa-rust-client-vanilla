// Light Shadows' copy (`lightshadow.rs`): a face of a map's cache (what
// does not move, drawn once) written into its live face, before the frame's
// movers are drawn over it; a pass, not a texture copy, as wgpu's GL backend
// copies a texture through a colour attachment, which a depth texture
// cannot be. The cache is read through a nearest sampler as an unfilterable
// float (as ao.wgsl reads the frame's depth: GLES 3.0 counts a depth texture
// with linear filters of its own incomplete for a texelFetch).

// lightcast.wgsl's `Face`: the copy reads `layer`
struct Face {
    m: mat4x4<f32>,
    origin: vec4<f32>,
    layer: vec4<u32>,
};
@group(0) @binding(0) var<uniform> face: Face;
@group(1) @binding(0) var cache: texture_2d_array<f32>;
@group(1) @binding(1) var nearest: sampler;

// one triangle over the whole face
@vertex
fn vs_copy(@builtin(vertex_index) k: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(k & 1u) * 4 - 1);
    let y = f32(i32(k >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_copy(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {
    let size = vec2<f32>(textureDimensions(cache));
    return textureSampleLevel(cache, nearest, p.xy / size, i32(face.layer.x), 0.0).r;
}
