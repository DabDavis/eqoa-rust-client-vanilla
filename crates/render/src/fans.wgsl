// Immediate-mode fans as the VU1 program VIRFan draws them (decomp/vu:
// VIRStrip's loop with the hub pinned): each vertex lit as VIRStrip lights it,
//
//   light = min(colour + ambient + sun · max(−N·S, 0)
//               + point · (1 − min(|L|/range, 1)) · max(L·N/|L|, 0), 1)
//
// with the vertex alpha the colour's own; fogged by view depth as the strip
// programs fog; textured by the material's one layer (MODULATE: texel ×
// light, texel alpha × vertex alpha) and blended as its GS state says.
//
// A fan is drawn as its front face, two triangles over its first four
// vertices: the effect volumes' fans are both faces of a quad (v0 v1 v2 v3
// v2 v1), and VIRFan's fixed winding parity culls exactly one of them.

struct Frame {
    // the frame's view-projection with the camera at the origin: corners
    // come relative to the eye
    view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,
    sun: vec4<f32>,
    ambient: vec4<f32>,
    fog_color: vec4<f32>,  // w: on
    fog: vec4<f32>,        // near, far, the GS's K (levels: `effects_lod_k`)
    point: vec4<f32>,      // the point light relative to the eye, range (0: none)
    point_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Quad {
    @location(0) c0: vec3<f32>,
    @location(1) c1: vec3<f32>,
    @location(2) c2: vec3<f32>,
    @location(3) c3: vec3<f32>,
    @location(4) uv01: vec4<f32>,
    @location(5) uv23: vec4<f32>,
    @location(6) color: vec4<f32>,
    @location(7) normal: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) fog: f32,
    // the depth before the camera, the texel's level (1/Q: shader.wgsl's)
    @location(3) depth: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, q: Quad) -> VsOut {
    // the fan's triangles (0, 1, 2) and (0, 2, 3)
    var k = vi;
    if (vi == 3u) {
        k = 0u;
    } else if (vi == 4u) {
        k = 2u;
    } else if (vi == 5u) {
        k = 3u;
    }
    var p = q.c0;
    var uv = q.uv01.xy;
    if (k == 1u) {
        p = q.c1;
        uv = q.uv01.zw;
    } else if (k == 2u) {
        p = q.c2;
        uv = q.uv23.xy;
    } else if (k == 3u) {
        p = q.c3;
        uv = q.uv23.zw;
    }
    var o: VsOut;
    o.clip = frame.view_proj * vec4<f32>(p, 1.0);
    o.fog = clamp((frame.fog.y - o.clip.w) / (frame.fog.y - frame.fog.x), 0.0, 1.0);
    o.depth = o.clip.w;
    o.uv = uv;
    let n = q.normal;
    let sun = frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0);
    var point = vec3<f32>(0.0);
    if (frame.point.w > 0.0) {
        let l = frame.point.xyz - p;
        let d = length(l);
        if (d > 0.0) {
            point = frame.point_color.rgb * (1.0 - min(d / frame.point.w, 1.0)) * max(dot(l, n) / d, 0.0);
        }
    }
    o.color = vec4<f32>(clamp(q.color.rgb + frame.ambient.rgb + sun + point, vec3<f32>(0.0), vec3<f32>(1.0)), q.color.a);
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSampleLevel(tex, samp, i.uv, log2(max(i.depth, 1e-6)) + frame.fog.z);
    var rgb = t.rgb * i.color.rgb;
    if (frame.fog_color.w > 0.5) {
        rgb = mix(frame.fog_color.rgb, rgb, i.fog);
    }
    return vec4<f32>(rgb, t.a * i.color.a);
}
