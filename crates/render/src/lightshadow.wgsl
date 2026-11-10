// Light Shadows (`lightshadow.rs`): appended after lit.wgsl's text (and
// sunshadow.wgsl's, with Sun Shadows on), whose point terms then take each
// light's visibility out of its map, where it has one:
//
//   per fragment: light = … + Σ point · (1 − |L|/range) · max(L·N/|L|, 0) · shadow
//
// (`lightshadow::receivers_wgsl` makes that change to lit.wgsl's lines),
// looked up only where the term is not nothing: a fragment out of a light's
// reach, or turned from it, pays nothing for its map.
//
// A map is a cube round its light: six faces along the world's axes, each
// widened by a guard band (`lightshadow::GUARD`) so the filter's taps stay
// inside the face the point's largest axis picks (`lightshadow::face_of`).
// A face's depth is a − b / m for a point m along its axis
// (`lightshadow::depth_map`), the reference the same of the fragment's,
// looked up from a point moved off the surface along its normal (more as
// the light grazes it: as far as the filter reaches, so that no texel of it
// finds the surface itself nearer the light) and a little nearer the light,
// through a tent of the map's texels round it (`lightshadow::REACH` texels
// each way: its shadow's edge soft, and gliding as a light or a caster
// moves where a narrower filter's stepped a texel at a time). A map's
// strength fades its shadow in and out as the budget hands maps between
// lights.

struct LightShadows {
    // each light of `lights.l` its map's place + 1 (0: none), four a vec4
    slot: array<vec4<u32>, 4>,
    // each map's origin less the eye, and w its strength (0 none, 1 whole)
    origin: array<vec4<f32>, 16>,
    // each map's depth mapping: x a, y b
    depth: array<vec4<f32>, 16>,
    // x: the faces' half width (a little over 1), y: 1 / the size, z: a
    // texel's world size per unit of distance along a face's axis, w: the
    // filter's reach in texels (`lightshadow::REACH`)
    params: vec4<f32>,
};
@group(3) @binding(4) var light_maps: texture_depth_2d_array;
@group(3) @binding(5) var light_cmp: sampler_comparison;
@group(3) @binding(6) var<uniform> light_shadows: LightShadows;

// How far off the surface a fragment looks its light's map up, in the
// map's texels there: along the normal, the filter's reach (params.w) times
// the sine of the angle to the light (a surface sloping from the light
// stays clear of its own texels across the tent: half a texel less left
// lines of acne along the map's texels where the tent's last texels
// reached); and toward the light, this many.
const LIGHT_DEPTH_BIAS: f32 = 1.5;

// The face a point less the light falls in: its largest axis's, the
// earlier on a tie (`lightshadow::face_of`), and that face's forward and up.
fn light_face(v: vec3<f32>) -> u32 {
    let a = abs(v);
    if (a.x >= a.y && a.x >= a.z) {
        return select(1u, 0u, v.x >= 0.0);
    }
    if (a.y >= a.z) {
        return select(3u, 2u, v.y >= 0.0);
    }
    return select(5u, 4u, v.z >= 0.0);
}

fn light_face_forward(k: u32) -> vec3<f32> {
    let s = select(-1.0, 1.0, (k & 1u) == 0u);
    if (k < 2u) {
        return vec3<f32>(s, 0.0, 0.0);
    }
    if (k < 4u) {
        return vec3<f32>(0.0, s, 0.0);
    }
    return vec3<f32>(0.0, 0.0, s);
}

fn light_face_up(k: u32) -> vec3<f32> {
    if (k == 2u) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    if (k == 3u) {
        return vec3<f32>(0.0, 0.0, -1.0);
    }
    return vec3<f32>(0.0, 1.0, 0.0);
}

// Light `j`'s visibility at the fragment, 0 (in its shadow) to 1, for a
// surface facing `n` (a unit, or a plant's up): 1 for a light with no map.
fn light_shadow(j: u32, i: LitOut, n: vec3<f32>) -> f32 {
    let s = light_shadows.slot[j / 4u][j % 4u];
    if (s == 0u) {
        return 1.0;
    }
    let k = s - 1u;
    let o = light_shadows.origin[k];
    var v = i.rel - o.xyz;
    let len = length(v);
    if (len <= 0.0) {
        return 1.0;
    }
    // a texel's size here, and the point moved off the surface
    let t = max(max(abs(v.x), abs(v.y)), abs(v.z)) * light_shadows.params.z;
    let cosl = clamp(-dot(n, v) / len, 0.0, 1.0);
    let sinl = sqrt(1.0 - cosl * cosl);
    v += n * (t * light_shadows.params.w * sinl);
    let face = light_face(v);
    let f = light_face_forward(face);
    let u = light_face_up(face);
    let r = cross(f, u);
    let m = dot(v, f);
    let ndc = vec2<f32>(dot(v, r), dot(v, u)) / (m * light_shadows.params.x);
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let dm = light_shadows.depth[k];
    let z = dm.x - dm.y / max(m - t * LIGHT_DEPTH_BIAS, 1e-4);
    let layer = k * 6u + face;
    // the texels round the point, their centres at whole numbers: the
    // tent's weights for each, paired, each pair one bilinear comparison
    // placed between its two texels at their weights' ratio
    let e = light_shadows.params.y;
    let p = uv / e - 0.5;
    let b = floor(p);
    let fr = p - b;
    // the tent of radius 3: texels −2 to 3 from b weigh (1 − |k − fr| / 3) / 3,
    // paired (−2, −1) (0, 1) (2, 3): weights (3 − 2fr, 5, 1 + 2fr) / 9
    let w0 = 3.0 - 2.0 * fr;
    let w2 = 1.0 + 2.0 * fr;
    let a0 = (b - 1.5 + (2.0 - fr) / w0) * e;
    let a1 = (b + 0.5 + (2.0 + fr) / 5.0) * e;
    let a2 = (b + 2.5 + fr / w2) * e;
    var row = w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a0.y), layer, z);
    row += 5.0 * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a0.y), layer, z);
    row += w2.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a2.x, a0.y), layer, z);
    var vis = w0.y * row;
    row = w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a1.y), layer, z);
    row += 5.0 * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a1.y), layer, z);
    row += w2.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a2.x, a1.y), layer, z);
    vis += 5.0 * row;
    row = w0.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a0.x, a2.y), layer, z);
    row += 5.0 * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a1.x, a2.y), layer, z);
    row += w2.x * textureSampleCompareLevel(light_maps, light_cmp, vec2<f32>(a2.x, a2.y), layer, z);
    vis += w2.y * row;
    return mix(1.0, vis / 81.0, o.w);
}
