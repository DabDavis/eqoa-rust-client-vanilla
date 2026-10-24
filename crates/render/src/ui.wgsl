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
