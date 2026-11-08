// Dynamic Lighting (the options' Extended > Lighting): the strip programs'
// light evaluated per fragment instead of per vertex, and more point lights
// than the PS2's one. This file is appended to shader.wgsl's text (the
// module `lit.rs` makes), so its structs and bindings are this one's; the
// PS2's pipelines are made of shader.wgsl alone, which this does not touch.
//
// The same light as the console's, each term where it is evaluated:
//
//   per vertex:   base = colour + ambient + emissive + static.rgb
//                 sunvis = colour.a · static.a  (the sun visibility a bake keeps)
//   per fragment: light = clamp(base + sun · max(−N·S, 0) · sunvis
//                         + Σ point · (1 − min(|L|/range, 1)) · max(L·N/|L|, 0), 0, 1)
//
// with N the interpolated normal made a unit again, and the points the
// personal light (the frame's `point`) and the `lights` list. A light marked
// fixed (a fire's, a placed light's) is left out on a baked instance
// (`fs_lit_baked`), whose colour buffers were baked with the scene's fixed
// lights: lit by them again it would be lit twice. The rest (the personal
// light, a spell's, a glowing weapon's) light everything, as the console's
// one point light lights baked geometry too.
//
// A radial flora plant (a layer's flags.w) is `VIRStrip6`'s still: no normal
// and no vertex colour, so its point lights are their falloff times 0.15 at
// the distance in the plant's own space, as vs_main lights it.

// One point light: where it is, relative to the eye, and its range; its
// colour, and w = 1 for a fixed light.
struct Light {
    at: vec4<f32>,
    color: vec4<f32>,
};

struct Lights {
    // x: how many of `l` are in use
    count: vec4<u32>,
    l: array<Light, 16>,
};
@group(3) @binding(0) var<uniform> lights: Lights;

struct LitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // rgb: the light that does not depend on a direction (colour + ambient +
    // emissive + static.rgb, a plant's ambient + emissive + sun × 0.15);
    // w: the sun's visibility (colour.a × static.a)
    @location(1) base: vec4<f32>,
    // x: 1 none, 0 all fog; y: the depth before the camera, the level's
    // (1/Q); z: the vertex alpha a flora plant's fade is (1 for the rest);
    // w: a plant's 1/scale (its own space's distances)
    @location(2) misc: vec4<f32>,
    // the position relative to the eye, and the normal in the world
    @location(3) rel: vec3<f32>,
    @location(4) nrm: vec3<f32>,
};

@vertex
fn vs_lit(v: VsIn) -> LitOut {
    var o: LitOut;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
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
    // relative to the eye first, as vs_main
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    let fog = clamp((frame.fog.y - o.clip.w) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    let s = layer.uv1.zw * frame.sun.w;
    let scroll = s - trunc(s);
    o.uv = vec2<f32>(
        v.uv.x * layer.uv0.x + v.uv.y * layer.uv0.z + layer.uv1.x + scroll.x,
        v.uv.x * layer.uv0.y + v.uv.y * layer.uv0.w + layer.uv1.y + scroll.y,
    );
    o.rel = rel;
    o.nrm = (model * nrm).xyz;
    let ambient = frame.ambient.rgb + layer.emissive.rgb;
    o.base = vec4<f32>(v.color.rgb + ambient + v.static_light.rgb, v.color.a * v.static_light.a);
    o.misc = vec4<f32>(fog, o.clip.w, 1.0, 1.0);
    if (layer.flags.w > 0.5) {
        o.base = vec4<f32>(ambient + frame.sun.rgb * 0.15, 0.0);
        o.misc.z = v.static_light.a;
        o.misc.w = 1.0 / length(v.m0.xyz);
    }
    return o;
}

// The point term of the strip programs at `l` (the light less the point)
// for a light of `range`: the falloff times the cosine.
fn point_term(l: vec3<f32>, range: f32, n: vec3<f32>) -> f32 {
    let d = length(l);
    if (d <= 0.0 || d >= range) {
        return 0.0;
    }
    return (1.0 - d / range) * max(dot(l, n) / d, 0.0);
}

fn shade(i: LitOut, baked: bool) -> vec3<f32> {
    var light = i.base.rgb;
    if (layer.flags.w > 0.5) {
        // VIRStrip6: the falloff alone, times 0.15, in the plant's space
        let k = 0.15;
        if (frame.point.w > 0.0) {
            let d = length((frame.point.xyz - frame.eye.xyz) - i.rel) * i.misc.w;
            light += frame.point_color.rgb * ((1.0 - min(d / frame.point.w, 1.0)) * k);
        }
        for (var j = 0u; j < min(lights.count.x, 16u); j++) {
            let p = lights.l[j];
            let d = length(p.at.xyz - i.rel) * i.misc.w;
            light += p.color.rgb * ((1.0 - min(d / p.at.w, 1.0)) * k);
        }
        return min(light, vec3<f32>(1.0));
    }
    let len = length(i.nrm);
    var n = vec3<f32>(0.0);
    if (len > 0.0) {
        n = i.nrm / len;
    }
    light += frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * i.base.w;
    if (frame.point.w > 0.0) {
        light += frame.point_color.rgb * point_term((frame.point.xyz - frame.eye.xyz) - i.rel, frame.point.w, n);
    }
    for (var j = 0u; j < min(lights.count.x, 16u); j++) {
        let p = lights.l[j];
        if (baked && p.color.w > 0.5) {
            continue;
        }
        let l = p.at.xyz - i.rel;
        // out of reach: nothing (most fragments, most lights)
        if (dot(l, l) >= p.at.w * p.at.w) {
            continue;
        }
        light += p.color.rgb * point_term(l, p.at.w, n);
    }
    return clamp(light, vec3<f32>(0.0), vec3<f32>(1.0));
}

// fs_main's texel, alpha test and fog, over the light `shade` makes.
fn lit_fragment(i: LitOut, baked: bool) -> vec4<f32> {
    var texel = vec4<f32>(1.0);
    if (layer.flags.x > 0.5) {
        var lod = -16.0;
        if (frame.eye.w > 0.5) {
            lod = log2(max(i.misc.y, 1e-6)) + layer.flags.z;
        }
        texel = textureSampleLevel(tex, samp, i.uv, lod);
    }
    let alpha = texel.a * layer.tint.a * i.misc.z;
    if (layer.flags.y < 0.5 && alpha < 0.5) {
        discard;
    }
    var rgb = texel.rgb * layer.tint.rgb * shade(i, baked);
    if (frame.fog_color.w > 0.5) {
        rgb = mix(frame.fog_color.rgb, rgb, i.misc.x);
    }
    return vec4<f32>(rgb, alpha);
}

@fragment
fn fs_lit(i: LitOut) -> @location(0) vec4<f32> {
    return lit_fragment(i, false);
}

@fragment
fn fs_lit_baked(i: LitOut) -> @location(0) vec4<f32> {
    return lit_fragment(i, true);
}
