// Sun Shadows' casters (`sunshadow.rs`): what stands between a surface and
// the sun, drawn into one cascade of the shadow map. Appended to
// shader.wgsl's text (the module `sunshadow::Sun` makes), so its structs and
// bindings are that file's: the frame (group 0: the camera, for a layer's
// level and scroll), a layer (group 1, the material's own bind group), the
// skin palette (group 2), and the cascade (group 3).
//
// Every opaque layer of a material whose first layer is opaque is drawn,
// alpha tested as fs_main tests it (a leaf's cut-out casts the leaf's
// shape), at the level the camera sees the texture at, its faces turned to
// the sun alone (the pipeline culls back faces, as for a camera); nothing
// is written but depth. Positions are taken relative to the eye as vs_main
// takes them: the cascade's matrix is the eye's too (`sunshadow::fit`).

struct Cascade {
    // a position less the eye to the cascade's clip space
    m: mat4x4<f32>,
};
@group(3) @binding(0) var<uniform> cascade: Cascade;

struct CastOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // the depth before the camera (the texture's level, as fs_main takes it)
    @location(1) depth: f32,
};

@vertex
fn vs_cast(v: VsIn) -> CastOut {
    var o: CastOut;
    var pos = vec4<f32>(v.pos, 1.0);
    let wsum = f32(v.weights.x + v.weights.y + v.weights.z + v.weights.w);
    if (wsum > 0.0) {
        var sp = vec4<f32>(0.0);
        for (var k = 0u; k < 4u; k++) {
            let b = v.bones[k];
            if (b < 72u && v.weights[k] > 0u) {
                let w = f32(v.weights[k]) / wsum;
                sp += palette.m[b] * pos * w;
            }
        }
        pos = vec4<f32>(sp.xyz, 1.0);
    }
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    o.clip = cascade.m * vec4<f32>(rel, 1.0);
    o.depth = (frame.view_proj * vec4<f32>(rel, 1.0)).w;
    let s = layer.uv1.zw * frame.sun.w;
    let scroll = s - trunc(s);
    o.uv = vec2<f32>(
        v.uv.x * layer.uv0.x + v.uv.y * layer.uv0.z + layer.uv1.x + scroll.x,
        v.uv.x * layer.uv0.y + v.uv.y * layer.uv0.w + layer.uv1.y + scroll.y,
    );
    return o;
}

// fs_main's alpha test, and nothing else: what it lets through casts.
@fragment
fn fs_cast(i: CastOut) {
    var alpha = layer.tint.a;
    if (layer.flags.x > 0.5) {
        var lod = -16.0;
        if (frame.eye.w > 0.5) {
            lod = log2(max(i.depth, 1e-6)) + layer.flags.z;
        }
        alpha *= textureSampleLevel(tex, samp, i.uv, lod).a;
    }
    if (alpha < 0.5) {
        discard;
    }
}
