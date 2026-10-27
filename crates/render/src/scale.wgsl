// The options' Render Scale: the frame drawn k times larger each way, each
// target pixel the average of the k by k block under it (scale.rs). Each
// channel's level is read as the byte the texture stores, the k * k bytes
// summed and divided rounding half up, so the result is the same on every
// GPU. Alpha is 1, as the frame's is (its passes write colour only).

@group(0) @binding(0) var frame: texture_2d<f32>;
// x: k
@group(0) @binding(1) var<uniform> scale: vec4<u32>;

// one triangle over the whole target
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let k = scale.x;
    let at = vec2<u32>(p.xy) * k;
    var sum = vec3<u32>(0u);
    for (var y = 0u; y < k; y++) {
        for (var x = 0u; x < k; x++) {
            let c = textureLoad(frame, vec2<i32>(at + vec2<u32>(x, y)), 0);
            sum += vec3<u32>(round(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0));
        }
    }
    let n = k * k;
    return vec4<f32>(vec3<f32>((sum + vec3<u32>(n / 2u)) / n) / 255.0, 1.0);
}
