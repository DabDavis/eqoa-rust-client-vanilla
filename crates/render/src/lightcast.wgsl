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
    return light_cast_vertex(v);
}

fn light_cast_vertex(v: VsIn) -> LightCastOut {
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
    if (light_cast_alpha(i.uv, i.depth) < 0.5) {
        discard;
    }
}

fn light_cast_alpha(uv: vec2<f32>, depth: f32) -> f32 {
    var alpha = layer.tint.a;
    if (layer.flags.x > 0.5) {
        var lod = -16.0;
        if (frame.eye.w > 0.5) {
            lod = log2(max(depth, 1e-6)) + layer.flags.z;
        }
        alpha *= textureSampleLevel(tex, samp, uv, lod).a;
    }
    return alpha;
}

// A layer no texel of which fails the alpha test casts as depth alone.
@vertex
fn vs_light_cast_solid(v: VsIn) -> @builtin(position) vec4<f32> {
    return light_cast_clip(light_cast_place(v));
}

// A mover (a character, what it wears and holds, a prop that moves) as the
// casters above, but never cut by the face's near plane: a vertex nearer the
// light than NEAR is held on the plane (its depth the nearest), so a helm
// floating up within NEAR of the lantern over its wearer still casts its
// whole shadow (the GM Jibmonk's petition 1591: the Jal'Raeth's shadow lost a
// square from its middle near the top of their float, where the near plane
// cut the part of it nearest the light). The near plane stays for what does
// not move: a torch's own stick, a lamp's housing.
fn light_cast_clip_held(p: Placed) -> vec4<f32> {
    var c = light_cast_clip(p);
    c.z = max(c.z, 0.0);
    return c;
}

@vertex
fn vs_light_cast_mover(v: VsIn) -> LightCastOut {
    var o = light_cast_vertex(v);
    o.clip.z = max(o.clip.z, 0.0);
    return o;
}

@vertex
fn vs_light_cast_solid_mover(v: VsIn) -> @builtin(position) vec4<f32> {
    return light_cast_clip_held(light_cast_place(v));
}

// The player's own body under the personal light (`lightshadow::under_personal`),
// drawn only into the face looking down: what of it is at least OWN_CLEAR
// under the light casts, cut there per fragment, so a body round its
// lantern (a hovering gargoyle's wings, 3 over its feet as the lantern is)
// shadows the ground under it as a body under a lamp does instead of
// covering the face from beside the light. Not held on the near plane.
const OWN_CLEAR: f32 = 0.75;

struct LightCastOwn {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) depth: f32,
    // how far under the light
    @location(2) below: f32,
};

@vertex
fn vs_light_cast_own(v: VsIn) -> LightCastOwn {
    let c = light_cast_vertex(v);
    var o: LightCastOwn;
    o.clip = c.clip;
    o.uv = c.uv;
    o.depth = c.depth;
    o.below = face.origin.y - light_cast_place(v).rel.y;
    return o;
}

@fragment
fn fs_light_cast_own(i: LightCastOwn) {
    if (i.below < OWN_CLEAR || light_cast_alpha(i.uv, i.depth) < 0.5) {
        discard;
    }
}
