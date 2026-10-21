// The options' Brightness: the finished scene's colour levels through a
// table of 256 (curve.rs). Each texel is read where it lies and each channel's
// level (a byte, as the target stores it) looked up, so the result is the
// table's byte exactly on every GPU. Alpha is kept.

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var table: texture_2d<f32>;

// one triangle over the whole target
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn level(c: f32) -> f32 {
    let i = i32(round(clamp(c, 0.0, 1.0) * 255.0));
    return textureLoad(table, vec2<i32>(i, 0), 0).r;
}

@fragment
fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(scene, vec2<i32>(p.xy), 0);
    return vec4<f32>(level(c.r), level(c.g), level(c.b), c.a);
}
