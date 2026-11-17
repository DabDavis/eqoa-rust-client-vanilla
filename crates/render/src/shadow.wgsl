// The characters' blob shadows (`VIScene::RenderShadow`, `vi::shadow`) as
// the VU1 program VIRFan draws its fans: each a triangle in the world, its
// vertex white with the shadow's alpha, lit as VIRStrip lights a vertex,
//
//   light = min(colour + ambient + sun + point, 1)
//
// which is 1 for a white vertex whatever the lights; fogged by view depth as
// the strip programs fog; textured by shadow.tga (MODULATE: the texel, its
// alpha times the vertex's) and blended (s, 1 - s). The Z bias
// (`VIRaster::SetZBias(15)`: 15 x 166 added to the GS's screen Z after the
// divide) is a constant k in the reciprocal of the depth for the game's
// camera; the frame gives k, how far this eye is behind the game's camera
// and the projection's w_axis.z, and the vertex is drawn at the depth the
// game's camera pulls it to (`render::shadow::Bias::drawn_at`).

struct Frame {
    // the frame's view-projection with the camera at the origin: the
    // vertices come relative to the eye
    view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,  // w: on
    fog: vec4<f32>,        // near, far, the GS's K (`gs_lod_k(1.0)`), w: 1 a storm's
    bias: vec4<f32>,       // the projection's w_axis.z, k, how far behind the game's camera
};

@group(0) @binding(0) var<uniform> frame: Frame;

// A storm's fog (`View::storm_fog`, fog.w 1): by the distance from the eye,
// per fragment. A storm fogs everything within a few tens of units (heavy
// whiteout 20, sand 40), so a ground triangle reaches from behind the camera
// to past the fog: per vertex, its corner behind the camera was clear (w < 0)
// and its far corner all fog, and the clamped values between them made the
// ground near the eye clear or fogged by which corners the view had behind
// it, so the storm came and went as the camera turned (the owner, 2026-11-16:
// "when players rotate camera the fog will not be seen so looks clear"). The
// distance from the eye, unlike the depth, is the same whichever way the
// camera faces. The game's own fog (fog.w 0) stays the strip programs' value.
fn storm_fog(per_vertex: f32, rel: vec3<f32>) -> f32 {
    if (frame.fog.w > 0.5) {
        return clamp((frame.fog.y - length(rel)) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    }
    return per_vertex;
}
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Vert {
    @location(0) at: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) alpha: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) fog: f32,
    // the depth before the camera, the texel's level (1/Q: shader.wgsl's)
    @location(3) depth: f32,
    // the vertex less the eye (storm_fog)
    @location(4) rel: vec3<f32>,
};

@vertex
fn vs_main(v: Vert) -> VsOut {
    var o: VsOut;
    let c = frame.view_proj * vec4<f32>(v.at, 1.0);
    var z = c.z;
    if (c.w > 1e-4) {
        // Bias::drawn_at: w less the game's camera's pull at its depth
        let wg = max(c.w - frame.bias.z, 0.0);
        let w2 = max(c.w - frame.bias.y * wg * wg / (1.0 + frame.bias.y * wg), 1e-4);
        z = c.z + frame.bias.x * (1.0 / w2 - 1.0 / c.w) * c.w;
    }
    o.clip = vec4<f32>(c.x, c.y, z, c.w);
    o.fog = clamp((frame.fog.y - c.w) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    o.depth = c.w;
    o.rel = v.at;
    o.uv = v.uv;
    o.alpha = v.alpha;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSampleLevel(tex, samp, i.uv, log2(max(i.depth, 1e-6)) + frame.fog.z);
    var rgb = t.rgb;
    if (frame.fog_color.w > 0.5) {
        rgb = mix(frame.fog_color.rgb, rgb, storm_fog(i.fog, i.rel));
    }
    return vec4<f32>(rgb, t.a * i.alpha);
}
