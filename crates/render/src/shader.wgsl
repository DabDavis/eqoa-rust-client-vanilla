// Shading after the VU1 strip programs (claude-notes/vu1-microcode-
// disassembly-20261017.md, verified there against PCSX2), one pass per
// material layer, the light per vertex as VU1 computes it (the GS then
// interpolates the colour):
//
//   light = clamp(colour + ambient + sun · max(−N·S, 0) · colour.a · static.a
//                 + static.rgb + point · (1 − min(|L|/range, 1)) · max(L·N/|L|, 0), 0, 1)
//   pixel = texture × layer colour/255 × light
//
// where S is the direction the sun's light travels and, on packed geometry,
// the vertex alpha is a sun-visibility factor. An opaque layer is alpha
// tested at 128 of 255 (TEST 0x3080B); a blended layer's alpha is the
// texture's times the layer colour's, and the pipeline blends it. L runs from
// the vertex to the point light (the personal light; the raster has one).
// The static light is the instance's (VIActor::m_p70, set per actor by
// RasterActor; (0, 0, 0, 1) for terrain).
//
// Fog is the strip programs' clamp(w·scale + offset, 0, 255), with scale and
// offset from VIRaster::SetFog: linear in view depth w, none at the near
// distance and all fog colour at the far one, mixed after texturing.
//
// Positions are taken relative to the eye before they are projected (see
// vs_main): `view_proj` is the frame's with the camera at the origin.
//
// The radial flora's plants (a layer's flags.w) are `VIRStrip6`'s: no normal
// and no vertex colour, so
//
//   light = min(ambient + sun · 0.15 + point · (1 − min(|L|/range, 1)) · 0.15, 1)
//
// with |L| in the plant's own space (the light goes through the inverse model
// view: the world distance over the plant's scale), and the vertex alpha is
// the plant's fade (the instance's static-light alpha), which the GS
// multiplies into the texel's.
//
// A texel comes from the texture's mip levels as the GS takes it (TEX1:
// trilinear, LCM 0): at level log2(w) + K, w the depth before the camera
// (the strip programs' 1/Q, interpolated as the GS does: per pixel) and K
// the layer's (`gs_lod_k`), the base magnified at 0 or under; with the
// options' Mipmaps off (`eye.w` 0), the base always.

struct Frame {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,      // the camera, in the world; w: 1 mip levels on
    sun_dir: vec4<f32>,
    sun: vec4<f32>,      // w: time in seconds
    ambient: vec4<f32>,
    fog_color: vec4<f32>,  // w: on
    fog: vec4<f32>,        // near, far
    point: vec4<f32>,      // the point light: xyz, range (0: none)
    point_color: vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame: Frame;

struct Layer {
    tint: vec4<f32>,
    flags: vec4<f32>,    // x: textured, y: blended, z: the GS's K (levels), w: flora
    uv0: vec4<f32>,      // m0, m1, m3, m4
    uv1: vec4<f32>,      // m6, m7, scroll rate u, v
};
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> layer: Layer;

// The skin palette, `VIRStrip4`'s: four node matrices per vertex, weighted.
// Unskinned models carry identities and bone 255 (unused).
struct Palette {
    m: array<mat4x4<f32>, 64>,
};
@group(2) @binding(0) var<uniform> palette: Palette;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) bones: vec4<u32>,
    @location(9) weights: vec4<u32>,
    @location(10) static_light: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // the vertex's light, as the strip programs compute it per vertex and
    // the GS interpolates it
    @location(1) light: vec3<f32>,
    // 1 none, 0 all fog.
    @location(3) fog: f32,
    // w: the depth before the camera, the level's (1/Q)
    @location(4) depth: f32,
    // the vertex alpha a flora plant's fade is (1 for the rest)
    @location(5) fade: f32,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var pos = vec4<f32>(v.pos, 1.0);
    var nrm = vec4<f32>(v.normal, 0.0);
    let wsum = f32(v.weights.x + v.weights.y + v.weights.z + v.weights.w);
    if (wsum > 0.0) {
        var sp = vec4<f32>(0.0);
        var sn = vec4<f32>(0.0);
        for (var k = 0u; k < 4u; k++) {
            let b = v.bones[k];
            if (b < 64u && v.weights[k] > 0u) {
                let w = f32(v.weights[k]) / wsum;
                sp += palette.m[b] * pos * w;
                sn += palette.m[b] * nrm * w;
            }
        }
        pos = vec4<f32>(sp.xyz, 1.0);
        nrm = vec4<f32>(sn.xyz, 0.0);
    }
    // Relative to the eye first: the instance's translation less the eye,
    // then the rest of the transform added. A world position would be tens of
    // thousands (Freeport is at x 25,300, where a float steps 2 mm) and
    // projecting it would cancel nearly all of that, so two surfaces a few
    // centimetres apart could trade depths. `view_proj * model * pos` kept
    // the world out of it only while the compiler multiplied the matrices
    // first: Mesa's GLSL (GL, and WebGL2 in Firefox on Linux) multiplies
    // `model * pos` first, and the obelisks' glow came through their stone.
    let rel = v.m0.xyz * pos.x + v.m1.xyz * pos.y + v.m2.xyz * pos.z + (v.m3.xyz - frame.eye.xyz);
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    o.fog = clamp((frame.fog.y - o.clip.w) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    o.depth = o.clip.w;
    // The layer's UV transform, and its scroll (VIMaterial::Process: the
    // offset grows at the rate and drops its whole part past ±1).
    let s = layer.uv1.zw * frame.sun.w;
    let scroll = s - trunc(s);
    o.uv = vec2<f32>(
        v.uv.x * layer.uv0.x + v.uv.y * layer.uv0.z + layer.uv1.x + scroll.x,
        v.uv.x * layer.uv0.y + v.uv.y * layer.uv0.w + layer.uv1.y + scroll.y,
    );
    // Placements scale uniformly, so the upper 3×3 is fine for normals.
    let n = normalize((model * nrm).xyz);
    let sun = frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * v.color.a * v.static_light.a;
    var point = vec3<f32>(0.0);
    if (frame.point.w > 0.0) {
        let l = (frame.point.xyz - frame.eye.xyz) - rel;
        let d = length(l);
        if (d > 0.0) {
            point = frame.point_color.rgb * (1.0 - min(d / frame.point.w, 1.0)) * max(dot(l, n) / d, 0.0);
        }
    }
    o.light = clamp(v.color.rgb + frame.ambient.rgb + sun + v.static_light.rgb + point, vec3<f32>(0.0), vec3<f32>(1.0));
    o.fade = 1.0;
    if (layer.flags.w > 0.5) {
        let k = 0.15;
        var pt = vec3<f32>(0.0);
        if (frame.point.w > 0.0) {
            let d = length((frame.point.xyz - frame.eye.xyz) - rel) / length(v.m0.xyz);
            pt = frame.point_color.rgb * ((1.0 - min(d / frame.point.w, 1.0)) * k);
        }
        o.light = min(frame.ambient.rgb + frame.sun.rgb * k + pt, vec3<f32>(1.0));
        o.fade = v.static_light.a;
    }
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    var texel = vec4<f32>(1.0);
    if (layer.flags.x > 0.5) {
        var lod = -16.0;
        if (frame.eye.w > 0.5) {
            lod = log2(max(i.depth, 1e-6)) + layer.flags.z;
        }
        texel = textureSampleLevel(tex, samp, i.uv, lod);
    }
    let alpha = texel.a * layer.tint.a * i.fade;
    if (layer.flags.y < 0.5 && alpha < 0.5) {
        discard;
    }
    var rgb = texel.rgb * layer.tint.rgb * i.light;
    if (frame.fog_color.w > 0.5) {
        rgb = mix(frame.fog_color.rgb, rgb, i.fog);
    }
    return vec4<f32>(rgb, alpha);
}
