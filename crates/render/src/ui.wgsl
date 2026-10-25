// The 2D layer: quads in clip space, texture times colour. Mode 0 is alpha
// tested at 128 (fs_tested); the others blend by the pipeline's state.

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    // the quad's texture rectangle: least uv, greatest uv
    @location(3) rect: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) rect: vec4<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = vec4<f32>(v.pos, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.rect = v.rect;
    return o;
}

// The texel a fragment takes, kept to its quad's own: a pixel whose centre is
// on a mirrored quad's leading edge is at that quad's far uv, the texel past
// its rectangle (an atlas neighbour's); half a texel in, the nearest texel is
// always the quad's, and one inside it is the one it was. A rectangle of no
// width (a solid fill's single uv) keeps the texel at its uv.
fn own(i: VsOut) -> vec2<f32> {
    let half = vec2<f32>(0.5) / vec2<f32>(textureDimensions(tex));
    let lo = i.rect.xy + half;
    return clamp(i.uv, lo, max(i.rect.zw - half, lo));
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, own(i)) * i.color;
}

@fragment
fn fs_tested(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, own(i)) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return c;
}

// A font's quad through the filtering sampler under the options' Text
// (`TextFilter`). Sharp: within a texel its own colour, and only round its
// edge a mix with the next, over the band of `w` target pixels the edge
// falls in ("sharp bilinear"). From two pixels a texel the band is a whole
// pixel and every texel is drawn as wide as every other; it narrows as a
// texel nears one pixel, to the nearest texel, which is near even there
// and which a whole pixel's mix would blur. A pixel's centre that is not in
// a band takes its texel's centre: at a whole scale none is, the nearest
// texel. What a mix reaches past a glyph's edge is the clear texel round it
// (`pack`).
fn sharp(i: VsOut) -> vec2<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    let t = i.uv * size;
    // the target pixels a texel spans, across and down
    let k = 1.0 / max(vec2<f32>(abs(dpdx(t.x)) + abs(dpdy(t.x)), abs(dpdx(t.y)) + abs(dpdy(t.y))), vec2<f32>(1.0 / 64.0));
    let w = clamp(k - 1.0, vec2<f32>(1.0 / 256.0), vec2<f32>(1.0));
    let c = fract(t) - 0.5;
    let r = vec2<f32>(0.5) - 0.5 * w / k;
    return (floor(t) + 0.5 + (c - clamp(c, -r, r)) * k / w) / size;
}

@fragment
fn fs_sharp(i: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, sharp(i)) * i.color;
}

@fragment
fn fs_sharp_tested(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, sharp(i)) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return c;
}

// Smooth: the texels mixed across their whole width, as the GS magnifies a
// font's texture.
@fragment
fn fs_smooth(i: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, i.uv) * i.color;
}

@fragment
fn fs_smooth_tested(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return c;
}
