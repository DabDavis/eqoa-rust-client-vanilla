// Sun Shadows' casters (`sunshadow.rs`): what stands between a surface and
// the sun, drawn into one cascade of the shadow map. Appended to
// shader.wgsl's text (the module `sunshadow::Sun` makes), so its structs and
// bindings are that file's: the frame (group 0: the camera, for a layer's
// level and scroll), a layer (group 1, the material's own bind group), the
// skin palette (group 2), and the cascade (group 3).
//
// Every opaque layer of a material whose first layer is opaque is drawn,
// alpha tested as fs_main tests it (a leaf's cut-out casts the leaf's
// shape), at the level the camera sees the texture at, both its faces: a
// vertex whose normal is turned from the sun put back by the cascade's
// `push` (a ceiling or a cave's roof, seen from above, casts; the underside
// of a thin thing, an awning's patches or a roof's planks a little over its
// own top, does not shadow that top). Along the light alone, so what a
// caster covers in the map is not moved, only how near the sun it is; put
// back by the vertex, not the fragment, the depth test still runs before
// the fragment does (written in the fragment it cost 0.6 ms of an RX 580's
// frame at 2,048 a side). Nothing is written but depth; a layer no texel of
// which can fail the alpha test runs no fragment at all (vs_cast_solid).
// Positions are taken relative to the eye as vs_main takes them: the
// cascade's matrix is the eye's too (`sunshadow::fit`).

struct Cascade {
    // a position less the eye to the cascade's clip space
    m: mat4x4<f32>,
    // x: how far a face turned from the sun is put back, in clip z
    push: vec4<f32>,
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
    var nrm = vec4<f32>(v.normal, 0.0);
    let wsum = f32(v.weights.x + v.weights.y + v.weights.z + v.weights.w);
    if (wsum > 0.0) {
        var sp = vec4<f32>(0.0);
        var sn = vec4<f32>(0.0);
        for (var k = 0u; k < 4u; k++) {
            let b = v.bones[k];
            if (b < 72u && v.weights[k] > 0u) {
                let w = f32(v.weights[k]) / wsum;
                sp += palette.m[b] * pos * w;
                sn += palette.m[b] * nrm * w;
            }
        }
        pos = vec4<f32>(sp.xyz, 1.0);
        nrm = vec4<f32>(sn.xyz, 0.0);
    }
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    o.clip = cascade.m * vec4<f32>(rel, 1.0);
    // turned from the sun: put back along the light (the map's z)
    let n = v.m0.xyz * nrm.x + v.m1.xyz * nrm.y + v.m2.xyz * nrm.z;
    if (dot(n, frame.sun_dir.xyz) > 0.0) {
        o.clip.z = min(o.clip.z + cascade.push.x, 1.0);
    }
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

// A layer no texel of which fails the alpha test (`solid` in lib.rs) casts
// as depth alone: the vertex as vs_cast puts it, and no fragment to run (an
// alpha-tested fragment that ran for every caster cost 0.5 ms of an RX 580's
// 1080p frame at four cascades of 2,048).
@vertex
fn vs_cast_solid(v: VsIn) -> @builtin(position) vec4<f32> {
    var pos = vec4<f32>(v.pos, 1.0);
    var nrm = vec4<f32>(v.normal, 0.0);
    let wsum = f32(v.weights.x + v.weights.y + v.weights.z + v.weights.w);
    if (wsum > 0.0) {
        var sp = vec4<f32>(0.0);
        var sn = vec4<f32>(0.0);
        for (var k = 0u; k < 4u; k++) {
            let b = v.bones[k];
            if (b < 72u && v.weights[k] > 0u) {
                let w = f32(v.weights[k]) / wsum;
                sp += palette.m[b] * pos * w;
                sn += palette.m[b] * nrm * w;
            }
        }
        pos = vec4<f32>(sp.xyz, 1.0);
        nrm = vec4<f32>(sn.xyz, 0.0);
    }
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    var clip = cascade.m * vec4<f32>(rel, 1.0);
    let n = v.m0.xyz * nrm.x + v.m1.xyz * nrm.y + v.m2.xyz * nrm.z;
    if (dot(n, frame.sun_dir.xyz) > 0.0) {
        clip.z = min(clip.z + cascade.push.x, 1.0);
    }
    return clip;
}
