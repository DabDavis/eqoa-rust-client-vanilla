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
    return keep(i.uv, i.rect);
}

fn keep(uv: vec2<f32>, rect: vec4<f32>) -> vec2<f32> {
    let half = vec2<f32>(0.5) / vec2<f32>(textureDimensions(tex));
    let lo = rect.xy + half;
    return clamp(uv, lo, max(rect.zw - half, lo));
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

// The other textures' quad under the options' HUD Sharp (`HudFilter`): the
// fonts' Sharp, kept to the quad's own texels (`keep`: an atlas packs its
// cells edge to edge, and a mix past a cell's edge would take its
// neighbour's). Along an axis the quad's rectangle is less than a texel
// across (a solid fill's single uv), the texel the nearest takes there,
// whole.
fn art_sharp(i: VsOut) -> vec2<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    let s = keep(sharp(i), i.rect);
    let thin = (i.rect.zw - i.rect.xy) * size < vec2<f32>(1.0);
    return select(s, (floor(s * size) + 0.5) / size, thin);
}

// The column and row `p` (whole numbers) wrapped into a texture `n` texels
// across and down, as the layer's repeat wraps them: `p - n * floor(p / n)`,
// a quotient the division's rounding left a step short or over put back. In
// floats, once a pixel: an integer `%` is no one instruction of a GPU's but
// dozens (more in a page, whose compiler guards each), and the four texels
// took sixteen a pixel; the pregame's screens are this filter's from edge to
// edge, and the Render Scale draws them four or sixteen times over (F3
// 20261027-213115: character select at x2 fell to 26 fps in a page; at x4
// on an RX 580 its frame took 19 ms with them and 10 without, Freeport's 9).
// GL's `%` (a page's WebGL2) is undefined below 0, too.
fn wrap(p: vec2<f32>, n: vec2<f32>) -> vec2<f32> {
    let w = p - n * floor(p / n);
    return select(select(w, w - n, w >= n), w + n, w < vec2<f32>(0.0));
}

// The texels round `uv` mixed as the filter mixes them, each one's colour
// weighted by its alpha: a clear texel gives an edge's mix its clearness and
// none of its colour. The game's art keeps whatever colour its clear texels
// have (a font's are its ink's, `ui::pack`), and a clear black one mixed in
// as it is darkened the edge beside it. Where all four are clear, their
// colours mixed as they are (only an added blend reads them).
fn mix_clear(uv: vec2<f32>) -> vec4<f32> {
    let n = vec2<f32>(textureDimensions(tex));
    let t = uv * n - 0.5;
    let f = t - floor(t);
    // the four texels' columns and rows, the second past the first, wrapped
    let p0 = wrap(floor(t), n);
    let p1 = select(p0 + 1.0, vec2<f32>(0.0), p0 + 1.0 >= n);
    let i0 = vec2<i32>(p0);
    let i1 = vec2<i32>(p1);
    let a = textureLoad(tex, i0, 0);
    let b = textureLoad(tex, vec2<i32>(i1.x, i0.y), 0);
    let c = textureLoad(tex, vec2<i32>(i0.x, i1.y), 0);
    let d = textureLoad(tex, i1, 0);
    let pre = mix(mix(vec4<f32>(a.rgb * a.a, a.a), vec4<f32>(b.rgb * b.a, b.a), f.x), mix(vec4<f32>(c.rgb * c.a, c.a), vec4<f32>(d.rgb * d.a, d.a), f.x), f.y);
    if (pre.a <= 0.0) {
        return vec4<f32>(mix(mix(a.rgb, b.rgb, f.x), mix(c.rgb, d.rgb, f.x), f.y), 0.0);
    }
    return vec4<f32>(pre.rgb / pre.a, pre.a);
}

@fragment
fn fs_art_sharp(i: VsOut) -> @location(0) vec4<f32> {
    return mix_clear(art_sharp(i)) * i.color;
}

@fragment
fn fs_art_sharp_tested(i: VsOut) -> @location(0) vec4<f32> {
    let c = mix_clear(art_sharp(i)) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return c;
}
