// Light Shadows' casters (`lightshadow.rs`): what stands between a surface
// and a point light, drawn into one face of its map. Appended to
// shader.wgsl's text (the module `lightshadow::Points` makes), so its
// structs and bindings are that file's: the frame (group 0: the camera, for
// a layer's level and scroll), a layer (group 1), the skin palette (group 2)
// and the face (group 3, at its offset).
//
// As Sun Shadows' casters (suncast.wgsl): every opaque layer of a material
// whose first layer is opaque, alpha tested as fs_main tests it, at the
// level the camera sees the texture at, both faces, a vertex whose normal is
// turned from the light put back along its ray from the light (which moves
// it nowhere in the face, only farther from the light); a layer no texel of
// which can fail the alpha test as depth alone. Positions are taken relative
// to the eye as vs_main takes them, then less the light: the face's matrix
// is the light's.

struct Face {
    // a position less the light to the face's clip space
    m: mat4x4<f32>,
    // the light less the eye; w: how far a vertex turned from it is put back
    origin: vec4<f32>,
    // x: the face's layer in the maps (the cache's, which lightcopy.wgsl reads)
    layer: vec4<u32>,
};
@group(3) @binding(0) var<uniform> face: Face;

struct LightCastOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // the depth before the camera (the texture's level, as fs_main takes it)
    @location(1) depth: f32,
};

// The vertex as vs_main skins and places it, less the eye, and its normal.
struct Placed {
    rel: vec3<f32>,
    n: vec3<f32>,
};

fn light_cast_place(v: VsIn) -> Placed {
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
    var p: Placed;
    p.rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    p.n = v.m0.xyz * nrm.x + v.m1.xyz * nrm.y + v.m2.xyz * nrm.z;
    return p;
}

// The vertex in the face's clip space: less the light, put back along its
// ray when its normal is turned from the light.
fn light_cast_clip(p: Placed) -> vec4<f32> {
    var l = p.rel - face.origin.xyz;
    let d = length(l);
    if (dot(p.n, l) > 0.0 && d > 0.0) {
        l *= 1.0 + face.origin.w / d;
    }
    return face.m * vec4<f32>(l, 1.0);
}

@vertex
fn vs_light_cast(v: VsIn) -> LightCastOut {
    var o: LightCastOut;
    let p = light_cast_place(v);
    o.clip = light_cast_clip(p);
    o.depth = (frame.view_proj * vec4<f32>(p.rel, 1.0)).w;
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
fn fs_light_cast(i: LightCastOut) {
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

// A layer no texel of which fails the alpha test casts as depth alone.
@vertex
fn vs_light_cast_solid(v: VsIn) -> @builtin(position) vec4<f32> {
    return light_cast_clip(light_cast_place(v));
}
