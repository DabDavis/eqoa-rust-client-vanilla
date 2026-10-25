// Text: screen-space glyph quads, as VIRaster::DrawCenteredString draws
// them (texture times colour, alpha-blended), at the label's depth.

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

// Under the options' Text (`ui::TextFilter`), through the filtering sampler,
// as `ui.wgsl` draws a font: Sharp, each texel its own colour but across the
// pixel its edge crosses (so a moving name keeps its shape); Smooth, the
// texels mixed across their whole width, as the GS magnifies a font.
fn sharp(i: VsOut) -> vec2<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    let t = i.uv * size;
    let k = 1.0 / max(vec2<f32>(abs(dpdx(t.x)) + abs(dpdy(t.x)), abs(dpdx(t.y)) + abs(dpdy(t.y))), vec2<f32>(1.0 / 64.0));
    let w = clamp(k - 1.0, vec2<f32>(1.0 / 256.0), vec2<f32>(1.0));
    let c = fract(t) - 0.5;
    let r = vec2<f32>(0.5) - 0.5 * w / k;
    return (floor(t) + 0.5 + (c - clamp(c, -r, r)) * k / w) / size;
}

@fragment
fn fs_sharp(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, sharp(i));
    if (t.a * i.color.a < 0.02) {
        discard;
    }
    return t * i.color;
}

@fragment
fn fs_smooth(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a * i.color.a < 0.02) {
        discard;
    }
    return t * i.color;
}
