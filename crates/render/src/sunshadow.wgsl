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
// slice of the view, snapped to its texels in the world so it does not
// shimmer as the camera moves (`sunshadow::fit`). A fragment takes the
// cascade its depth before the camera falls in, looked up from a point moved
// off the surface along its normal (more as the sun grazes it) and a little
// toward the sun, four bilinear comparisons a texel apart; past
// `params.y` the shadow fades out, gone by `params.z`, the cascades' end.
// Over the last part of each of the first three cascades' slices the shadow
// is blended into the next cascade's, so the seams do not show as the camera
// moves. The last cascade is drawn every other frame, a frame old between,
// a margin wider than its slice for what the camera did meanwhile
// (`sunshadow::Sun::draw`).

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
// The last part of each of the first three cascades' slices blended into the
// next's (`sunshadow::SEAM`).
const SUN_SEAM: f32 = 0.15;

// The sun's visibility at the fragment from cascade `c`: four bilinear
// comparisons half a texel either way, from the point moved off the surface
// (`sinl` the sine of its angle to the sun); and 1 if the cascade's box
// holds that point, else 0.
fn sun_cascade(i: LitOut, n: vec3<f32>, sinl: f32, c: u32) -> vec2<f32> {
    let t = sun.texel[c];
    let p = sun.m[c] * vec4<f32>(i.rel + n * (t * SUN_NORMAL_OFFSET * sinl), 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    let z = p.z - t * SUN_DEPTH_BIAS * sun.inv_depth[c];
    let e = 0.5 * sun.params.w;
    var v = textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(-e, -e), c, z);
    v += textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(e, -e), c, z);
    v += textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(-e, e), c, z);
    v += textureSampleCompareLevel(sun_map, sun_cmp, uv + vec2<f32>(e, e), c, z);
    let held = all(abs(p.xy) <= vec2<f32>(1.0)) && p.z >= 0.0 && p.z <= 1.0;
    return vec2<f32>(v * 0.25, select(0.0, 1.0, held));
}

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
    let cosl = clamp(-dot(n, frame.sun_dir.xyz), 0.0, 1.0);
    let sinl = sqrt(1.0 - cosl * cosl);
    var v = sun_cascade(i, n, sinl, c).x;
    // over the last part of its slice, blended into the next cascade's
    // (whose box, round the next slice, holds it in a window of the game's
    // shape: else the cascade's own), so the seam where the texels grow and
    // the offsets with them moves with the camera unseen
    if (c < 3u) {
        var start = 0.0;
        if (c > 0u) {
            start = sun.ends[c - 1u];
        }
        let b = smoothstep(mix(start, sun.ends[c], 1.0 - SUN_SEAM), sun.ends[c], d);
        if (b > 0.0) {
            let next = sun_cascade(i, n, sinl, c + 1u);
            v = mix(v, next.x, b * next.y);
        }
    }
    let fade = smoothstep(sun.params.y, sun.params.z, d);
    return mix(1.0, v, sun.params.x * (1.0 - fade));
}
