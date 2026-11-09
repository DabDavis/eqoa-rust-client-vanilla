// Sun Shadows (`sunshadow.rs`): appended after lit.wgsl's text, whose sun
// term then takes the sun's visibility as the lesser of the strip programs'
// (colour.a × static.a, what a bake keeps) and the shadow map's:
//
//   per fragment: light = base + sun · max(−N·S, 0) · min(sunvis, shadow) + …
//
// (`sunshadow::receivers_wgsl` makes that change to lit.wgsl's line), so the
// map only takes light away where the console's light reaches, never adds
// any, and a surface the bake already darkened is not darkened again by the
// same wall. A radial flora plant's sun (VIRStrip6's sun × 0.15, no normal)
// takes the map's visibility alone.
//
// The map is four cascades of the view's depth (`sunshadow::ENDS`), each an
// orthographic view along the sun's light round the sphere that holds its
// slice of the view, drawn every frame and snapped to its texels, which stay
// put in the world as the camera moves and as the sun turns, so nothing
// shimmers and a shadow moves with the sun a little every frame
// (`sunshadow::fit`, `Grid`). A fragment takes the cascade its depth before
// the camera falls in, looked up from a point moved off the surface along its
// normal (more as the sun grazes it) and a little toward the sun, four
// bilinear comparisons a texel apart; past `params.y` the shadow fades out,
// gone by `params.z`, the cascades' end.

struct SunShadow {
    // each cascade's matrix: a position less the eye to its clip space
    m: array<mat4x4<f32>, 4>,
    // where each cascade ends, in depth before the camera
    ends: vec4<f32>,
    // the world size of one of each cascade's texels
    texel: vec4<f32>,
    // 1 / the depth each cascade's clip z spans, in world units
    inv_depth: vec4<f32>,
    // x: the shadows' strength (0 none: the sun down), y and z: where they
    // fade out, in depth before the camera; w: 1 / the map's size
    params: vec4<f32>,
};
@group(3) @binding(1) var sun_map: texture_depth_2d_array;
@group(3) @binding(2) var sun_cmp: sampler_comparison;
@group(3) @binding(3) var<uniform> sun: SunShadow;

// How far off the surface a fragment looks the map up, in its cascade's
// texels: along the normal, times the sine of the angle to the sun; and
// toward the sun. Against acne on the lit side with no shadow detached from
// its caster.
const SUN_NORMAL_OFFSET: f32 = 1.5;
const SUN_DEPTH_BIAS: f32 = 1.0;

// The sun's visibility at the fragment, 0 (in shadow) to 1, for a surface
// facing `n` (a unit, or the plant's up).
fn sun_shadow(i: LitOut, n: vec3<f32>) -> f32 {
    let d = i.misc.y;
    if (sun.params.x <= 0.0 || d >= sun.params.z) {
        return 1.0;
    }
    var c = 3u;
    if (d < sun.ends.z) {
        c = 2u;
    }
    if (d < sun.ends.y) {
        c = 1u;
    }
    if (d < sun.ends.x) {
        c = 0u;
    }
    let t = sun.texel[c];
    let cosl = clamp(-dot(n, frame.sun_dir.xyz), 0.0, 1.0);
    let sinl = sqrt(1.0 - cosl * cosl);
    let p = sun.m[c] * vec4<f32>(i.rel + n * (t * SUN_NORMAL_OFFSET * sinl), 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    let z = p.z - t * SUN_DEPTH_BIAS * sun.inv_depth[c];
    let e = 0.5 * sun.params.w;
    var v = textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(-e, -e), c, z);
    v += textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(e, -e), c, z);
    v += textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(-e, e), c, z);
    v += textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(e, e), c, z);
    let fade = smoothstep(sun.params.y, sun.params.z, d);
    return mix(1.0, v * 0.25, sun.params.x * (1.0 - fade));
}
