//! The 2D layer `VIRaster` draws between `Begin2D` and `End2D`: textured
//! quads in the PS2's 640x448 pixels (`Blit`, `StretchBlit`, a font's
//! glyphs, the HUD's own fans and strips), each with a blend mode and a
//! colour, drawn in the order they were made, scaled to the target. Blend
//! modes are the material layer's (0 alpha tested at 128, 1 add, 2
//! subtract, 3 alpha, 4 multiply), with the same GS arithmetic as the 3D
//! path: texture times colour, alpha as stored.
//!
//! A font is packed into one texture (`add_font`) and laid out as
//! `VIRaster::DrawString` lays it out: each glyph its width wide and the
//! font's height tall, one pixel apart.
//!
//! The frame keeps its 4:3 shape in a wider target, in the middle, and a
//! quad made under an `Anchor` other than the middle's moves out with the
//! side it belongs to: the HUD reaches the window's edges.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 448.0;

/// A quad's clip that is the whole target (`Ui::push_whole`).
const WHOLE: [i32; 4] = [i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4];

/// Where the frame sits in a `size` target: its offset and its scale. The
/// frame keeps its 4:3 shape (a PS2 shows it on a 4:3 picture): the full
/// height, in the middle of a wider target (eqoa-port's widescreen does the
/// same with the HUD).
fn frame(size: (u32, u32)) -> (f32, f32, f32, f32) {
    let (tw, th) = (size.0 as f32, size.1 as f32);
    let fw = (th * 4.0 / 3.0).min(tw);
    let fh = if fw < th * 4.0 / 3.0 { fw * 3.0 / 4.0 } else { th };
    ((tw - fw) / 2.0, (th - fh) / 2.0, fw / SCREEN_W, fh / SCREEN_H)
}

/// A point of a `size` target, in normalised device coordinates (y up), as
/// a point of the frame: off its sides in a wider target.
pub fn frame_from_ndc(size: (u32, u32), ndc: [f32; 2]) -> [f32; 2] {
    let (tw, th) = (size.0 as f32, size.1 as f32);
    let (ox, oy, sx, sy) = frame(size);
    [((ndc[0] + 1.0) * 0.5 * tw - ox) / sx, ((1.0 - ndc[1]) * 0.5 * th - oy) / sy]
}

/// The whole frame pixels a `size` target shows beyond each side of the
/// frame: none when it is 4:3 or narrower.
pub fn side(size: (u32, u32)) -> f32 {
    let (ox, _, sx, _) = frame(size);
    (ox / sx).floor()
}

/// Where the quads made now go across a target wider than the frame
/// (`Ui::anchor`). The game lays its HUD out for a 4:3 picture; in a wider
/// one each piece keeps its distance from its own side of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Anchor {
    /// With the frame, in the middle (the menus and the boxes).
    #[default]
    Middle,
    /// With the target's left side.
    Left,
    /// With the target's right side.
    Right,
    /// Frame x `.0` with the left side and `.1` with the right, stretched
    /// between: a backdrop that joins a left piece to a right one.
    Span(i32, i32),
}

impl Anchor {
    /// Where frame x `x` goes, `side` frame pixels showing beyond each side
    /// of the frame.
    pub fn place(self, x: f32, side: f32) -> f32 {
        match self {
            Anchor::Middle => x,
            Anchor::Left => x - side,
            Anchor::Right => x + side,
            Anchor::Span(a, b) => x - side + 2.0 * side * (x - a as f32) / (b - a).max(1) as f32,
        }
    }
}

/// `Ui::push`'s placing: relative to the viewport's corner and cut to it
/// (the GS's offset and scissor), then across the target by `anchor`, the
/// cut too (the whole frame's, without a viewport).
fn place(q: &mut Quad, viewport: Option<[i32; 4]>, anchor: Anchor, side: f32) {
    if let Some([x1, y1, x2, y2]) = viewport {
        for p in &mut q.pos {
            p[0] += x1 as f32;
            p[1] += y1 as f32;
        }
        q.clip = Some([x1, y1, x2 + 1, y2 + 1]);
    } else {
        q.clip = None;
    }
    if side > 0.0 && anchor != Anchor::Middle {
        for p in &mut q.pos {
            p[0] = anchor.place(p[0], side);
        }
        let [x0, y0, x1, y1] = q.clip.unwrap_or([0, 0, SCREEN_W as i32, SCREEN_H as i32]);
        let x = |v: i32| anchor.place(v as f32, side).round() as i32;
        q.clip = Some([x(x0), y0, x(x1), y1]);
    }
}

/// The frame rectangle an opaque quad covers (its bounds within its cut):
/// alpha tested, or blended at full alpha.
fn opaque_rect(q: &Quad) -> Option<[f32; 4]> {
    if !(q.blend == 0 || (q.blend == 3 && q.color[3] >= 1.0)) {
        return None;
    }
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in &q.pos {
        r = [r[0].min(p[0]), r[1].min(p[1]), r[2].max(p[0]), r[3].max(p[1])];
    }
    if let Some([x0, y0, x1, y1]) = q.clip {
        r = [r[0].max(x0 as f32), r[1].max(y0 as f32), r[2].min(x1 as f32), r[3].min(y1 as f32)];
    }
    Some(r)
}

/// What `Ui::push` notes of quad `i` for `end`: how far it went to a side,
/// or, placed with the frame, what it covers.
fn note(i: usize, q: &Quad, anchor: Anchor, side: f32, sided: &mut Vec<(usize, f32)>, opaque: &mut Vec<(usize, [f32; 4])>) {
    match anchor {
        Anchor::Left | Anchor::Right if side > 0.0 => sided.push((i, anchor.place(0.0, side))),
        Anchor::Middle => opaque.extend(opaque_rect(q).map(|r| (i, r))),
        _ => {}
    }
}

/// `Ui::end`: each quad moved `dx` to a side goes back with the frame when an
/// opaque quad made after it (with the frame) covers its middle, as that
/// window covers it on a 4:3 screen.
fn settle(quads: &mut [Quad], sided: &[(usize, f32)], opaque: &[(usize, [f32; 4])]) {
    for &(i, dx) in sided {
        let q = &mut quads[i];
        let c = [q.pos.iter().map(|p| p[0]).sum::<f32>() / 4.0 - dx, q.pos.iter().map(|p| p[1]).sum::<f32>() / 4.0];
        if opaque.iter().any(|&(j, r)| j > i && c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3]) {
            for p in &mut q.pos {
                p[0] -= dx;
            }
            if let Some(c) = &mut q.clip {
                c[0] -= dx as i32;
                c[2] -= dx as i32;
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// One quad: its corners in screen pixels and texture coordinates, in fan
/// order (the first, second, third and fourth vertex the game sends).
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub tex: usize,
    pub blend: u8,
    pub pos: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
    /// The viewport it was drawn under (`VIRaster::Viewport`), screen
    /// pixels x0, y0, x1, y1 (exclusive); None for the whole screen.
    pub clip: Option<[i32; 4]>,
}

struct Texture {
    bg: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// A font packed into one of the layer's textures.
pub struct Font {
    pub tex: usize,
    pub height: u32,
    glyphs: HashMap<u16, (u32, [f32; 4])>,
}

impl Font {
    /// `FindChar`, with `'?'` for a character the font lacks.
    fn find(&self, code: u16) -> Option<&(u32, [f32; 4])> {
        self.glyphs.get(&code).or_else(|| self.glyphs.get(&(b'?' as u16)))
    }

    /// `VIRFont::GetStringWidth(s, 1)`: each glyph's width plus one; -1
    /// when even `'?'` is missing.
    pub fn string_width(&self, s: &[u16]) -> i32 {
        let mut w = 0;
        for &c in s {
            let Some(g) = self.find(c) else { return -1 };
            w += g.0 as i32 + 1;
        }
        w
    }
}

pub struct Ui {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipelines: Vec<wgpu::RenderPipeline>,
    textures: Vec<Texture>,
    pub fonts: Vec<Font>,
    pub quads: Vec<Quad>,
    /// `VIRaster::Viewport(x1, y1, x2, y2)` (inclusive) as quads are made:
    /// they are placed relative to (x1, y1) and cut to the rectangle, as the
    /// GS's offset and scissor do. None is the whole screen.
    pub viewport: Option<[i32; 4]>,
    /// Where the quads made now go across a wider target.
    pub anchor: Anchor,
    /// The target's frame pixels beyond each side of the frame (`begin`).
    side: f32,
    /// The quads moved to a side, and how far; the opaque quads placed
    /// with the frame, and what they cover (`end`).
    sided: Vec<(usize, f32)>,
    opaque: Vec<(usize, [f32; 4])>,
}

impl Ui {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, depth: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // Texels to pixels one for one at 640x448, so the nearest texel;
        // a rotated quad (the compass) the GS also samples point. Repeating,
        // as the blit material's layer does (`InitSolidFill`'s flags 1, so
        // CLAMP_1 is 0): a blit bigger than its surface tiles it, which is
        // how the menus' panels and frame edges are drawn (every UI surface
        // is a power of two, as the GS's wrap needs).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipelines = (0..5)
            .map(|mode| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ui"),
                    layout: Some(&pl),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                        })],
                    },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: depth,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::Always),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(if mode == 0 { "fs_tested" } else { "fs_main" }),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(mode), write_mask: wgpu::ColorWrites::COLOR })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        Ui { layout, sampler, pipelines, textures: Vec::new(), fonts: Vec::new(), quads: Vec::new(), viewport: None, anchor: Anchor::Middle, side: 0.0, sided: Vec::new(), opaque: Vec::new() }
    }

    /// A new frame's quads, for a `size` target: none yet, and those made
    /// next go with the frame (`anchor`).
    pub fn begin(&mut self, size: (u32, u32)) {
        self.quads.clear();
        self.sided.clear();
        self.opaque.clear();
        self.side = side(size);
        self.anchor = Anchor::Middle;
    }

    /// The frame's quads are made: a piece moved to a side that a window
    /// made after it covers on a 4:3 screen goes back under that window, so
    /// a menu hides what it hides there (and the piece is not cut by its
    /// edge); what no window covers stays at the side.
    pub fn end(&mut self) {
        settle(&mut self.quads, &self.sided, &self.opaque);
        self.sided.clear();
        self.opaque.clear();
    }

    /// A texture for the layer: RGBA8, top row first. Its index.
    pub fn add_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) -> usize {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(tex.as_image_copy(), rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) }, size);
        let view = tex.create_view(&Default::default());
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.textures.push(Texture { bg, width, height });
        self.textures.len() - 1
    }

    /// A texture's size.
    pub fn texture_size(&self, tex: usize) -> (u32, u32) {
        let t = &self.textures[tex];
        (t.width, t.height)
    }

    /// A font, packed into rows of a 2048-wide texture. Its index.
    pub fn add_font(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, font: &esf::font::Font) -> usize {
        let (w, h) = (2048u32, font.height);
        let (mut x, mut row) = (0u32, 0u32);
        let mut places = Vec::new();
        for g in &font.glyphs {
            if x + g.width as u32 > w {
                x = 0;
                row += 1;
            }
            places.push((x, row));
            x += g.width as u32 + 1;
        }
        let ah = (row + 1) * (h + 1);
        let mut atlas = vec![0u8; (w * ah * 4) as usize];
        let mut glyphs = HashMap::new();
        for (g, &(gx, gr)) in font.glyphs.iter().zip(&places) {
            let gy = gr * (h + 1);
            for y in 0..h {
                for xx in 0..g.width as u32 {
                    let s = ((y * g.width as u32 + xx) * 4) as usize;
                    let d = (((gy + y) * w + gx + xx) * 4) as usize;
                    atlas[d..d + 4].copy_from_slice(&g.rgba[s..s + 4]);
                }
            }
            let uv = [gx as f32 / w as f32, gy as f32 / ah as f32, (gx + g.width as u32) as f32 / w as f32, (gy + h) as f32 / ah as f32];
            glyphs.entry(g.code).or_insert((g.width as u32, uv));
        }
        let tex = self.add_texture(device, queue, w, ah, &atlas);
        self.fonts.push(Font { tex, height: h, glyphs });
        self.fonts.len() - 1
    }

    /// `VIRaster::DrawString`: the glyphs from (x, y), each its width, one
    /// pixel apart, alpha-blended in `color` (`'?'` for a character the
    /// font lacks; nothing when it lacks that too).
    pub fn text(&mut self, font: usize, x: i32, y: i32, color: [f32; 4], s: &[u16]) {
        let f = &self.fonts[font];
        let (tex, h) = (f.tex, f.height as f32);
        let mut x0 = x as f32;
        let y0 = y as f32;
        let mut quads = Vec::new();
        for &c in s {
            let Some(&(w, uv)) = f.find(c) else { return };
            let x1 = x0 + w as f32;
            quads.push(Quad {
                tex,
                blend: 3,
                pos: [[x0, y0], [x1, y0], [x1, y0 + h], [x0, y0 + h]],
                uv: [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]],
                color,
                clip: None,
            });
            x0 = x1 + 1.0;
        }
        for q in quads {
            self.push(q);
        }
    }

    /// A quad cut only to the target, not to the frame or a viewport: for
    /// what floats over the world, which reaches past the frame's sides in a
    /// wider window. Its place is its own (no `anchor`).
    pub fn push_whole(&mut self, mut q: Quad) {
        q.clip = Some(WHOLE);
        self.quads.push(q);
    }

    /// A quad, placed in and cut to the current viewport, then moved across
    /// a wider target by `anchor`.
    pub fn push(&mut self, mut q: Quad) {
        place(&mut q, self.viewport, self.anchor, self.side);
        note(self.quads.len(), &q, self.anchor, self.side, &mut self.sided, &mut self.opaque);
        self.quads.push(q);
    }

    /// Draws the frame's quads into `pass` (a `size` target) and keeps them
    /// (the caller clears `quads` when it rebuilds them).
    pub fn draw<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32)) {
        if self.quads.is_empty() {
            return;
        }
        let (tw, th) = (size.0 as f32, size.1 as f32);
        let (ox, oy, sx, sy) = frame(size);
        let mut v = Vec::with_capacity(self.quads.len() * 6);
        for q in &self.quads {
            let p = |i: usize| Vertex {
                pos: [(ox + q.pos[i][0] * sx) / tw * 2.0 - 1.0, 1.0 - (oy + q.pos[i][1] * sy) / th * 2.0],
                uv: q.uv[i],
                color: q.color,
            };
            v.extend([p(0), p(1), p(2), p(0), p(2), p(3)]);
        }
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("ui"), contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX });
        pass.set_vertex_buffer(0, buf.slice(..));
        // Runs of one texture, blend mode and viewport, in order.
        let mut i = 0;
        while i < self.quads.len() {
            let (tex, blend, clip) = (self.quads[i].tex, self.quads[i].blend, self.quads[i].clip);
            let mut j = i;
            while j < self.quads.len() && self.quads[j].tex == tex && self.quads[j].blend == blend && self.quads[j].clip == clip {
                j += 1;
            }
            let [x0, y0, x1, y1] = clip.unwrap_or([0, 0, SCREEN_W as i32, SCREEN_H as i32]);
            let px = |v: i32, o: f32, k: f32, max: u32| (o + v as f32 * k).clamp(0.0, max as f32) as u32;
            let (a, b) = (px(x0, ox, sx, size.0), px(y0, oy, sy, size.1));
            let (c, d) = (px(x1, ox, sx, size.0), px(y1, oy, sy, size.1));
            if c <= a || d <= b {
                i = j;
                continue;
            }
            pass.set_scissor_rect(a, b, c - a, d - b);
            pass.set_pipeline(&self.pipelines[blend.min(4) as usize]);
            pass.set_bind_group(0, &self.textures[tex].bg, &[]);
            pass.draw((i * 6) as u32..(j * 6) as u32, 0..1);
            i = j;
        }
        pass.set_scissor_rect(0, 0, size.0, size.1);
    }
}

#[cfg(test)]
mod tests {
    use super::{Anchor, Quad, note, place, settle, side};

    fn quad(x0: f32, x1: f32) -> Quad {
        Quad { tex: 0, blend: 0, pos: [[x0, 10.0], [x1, 10.0], [x1, 20.0], [x0, 20.0]], uv: [[0.0; 2]; 4], color: [1.0; 4], clip: None }
    }

    #[test]
    fn a_target_shows_whole_frame_pixels_beyond_the_frame_only_when_wider() {
        assert_eq!(side((640, 480)), 0.0);
        assert_eq!(side((1280, 960)), 0.0);
        assert_eq!(side((540, 960)), 0.0);
        // 16:9: the frame is 720 of 960 at 9/8 a pixel, 120 / 1.125 beyond each side
        assert_eq!(side((960, 540)), 106.0);
        assert_eq!(side((1920, 1080)), 106.0);
        assert_eq!(side((2560, 1080)), 248.0);
    }

    #[test]
    fn a_four_by_three_target_places_every_anchor_as_the_frame() {
        for a in [Anchor::Middle, Anchor::Left, Anchor::Right, Anchor::Span(-1, 202)] {
            let (mut q, mut v) = (quad(5.0, 25.0), quad(5.0, 25.0));
            place(&mut q, None, a, 0.0);
            assert_eq!((q.pos, q.clip), (quad(5.0, 25.0).pos, None), "{a:?}");
            place(&mut v, Some([100, 200, 299, 259]), a, 0.0);
            assert_eq!(v.pos[0], [105.0, 210.0], "{a:?}");
            assert_eq!(v.clip, Some([100, 200, 300, 260]), "{a:?}");
        }
    }

    #[test]
    fn an_anchored_quad_keeps_its_distance_from_its_side() {
        let s = 106.0;
        let mut l = quad(10.0, 69.0);
        place(&mut l, None, Anchor::Left, s);
        assert_eq!([l.pos[0][0], l.pos[1][0]], [10.0 - s, 69.0 - s]);
        // cut to the frame moved with it: the target's left side
        assert_eq!(l.clip, Some([-106, 0, 534, 448]));
        let mut r = quad(430.0, 592.0);
        place(&mut r, Some([400, 20, 599, 59]), Anchor::Right, s);
        assert_eq!([r.pos[0][0], r.pos[1][0]], [830.0 + s, 992.0 + s]);
        assert_eq!(r.clip, Some([506, 20, 706, 60]));
        // the middle stays the frame's
        let mut m = quad(300.0, 340.0);
        place(&mut m, None, Anchor::Middle, s);
        assert_eq!((m.pos[0][0], m.clip), (300.0, None));
    }

    #[test]
    fn a_span_is_one_stretch_so_tiles_stay_edge_to_edge_and_reach_the_right_piece() {
        // The chat backdrop: the HUD's piece at -1..202 (tiled as BlitRect
        // tiles 64-wide art), the chat's at 200..642 with the right side.
        let s = 213.0;
        let a = Anchor::Span(-1, 202);
        let edges = [-1.0, 63.0, 127.0, 191.0, 202.0];
        let mut tiles: Vec<Quad> = edges.windows(2).map(|e| quad(e[0], e[1])).collect();
        for t in &mut tiles {
            place(t, None, a, s);
        }
        for w in tiles.windows(2) {
            assert_eq!(w[0].pos[1][0], w[1].pos[0][0]);
        }
        assert_eq!(tiles[0].pos[0][0], -1.0 - s);
        let mut chat = quad(200.0, 642.0);
        place(&mut chat, None, Anchor::Right, s);
        let end = tiles.last().unwrap().pos[1][0];
        assert!(end >= chat.pos[0][0] && end - chat.pos[0][0] == 2.0, "{end} vs {}", chat.pos[0][0]);
        // each tile stretched on its own would leave the strip in pieces
        let mut alone = quad(63.0, 127.0);
        place(&mut alone, None, Anchor::Span(63, 127), s);
        assert_ne!(alone.pos[0][0], tiles[1].pos[0][0]);
    }

    /// Quads made as `Ui::push` makes them (each `(x0, x1, y0, y1, blend,
    /// alpha, anchor)`), then `Ui::end`'s settling; their left edges.
    fn layer(side: f32, made: &[(f32, f32, f32, f32, u8, f32, Anchor)]) -> Vec<Quad> {
        let (mut quads, mut sided, mut opaque) = (Vec::new(), Vec::new(), Vec::new());
        for &(x0, x1, y0, y1, blend, alpha, a) in made {
            let mut q = Quad { tex: 0, blend, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[0.0; 2]; 4], color: [1.0, 1.0, 1.0, alpha], clip: None };
            place(&mut q, None, a, side);
            note(quads.len(), &q, a, side, &mut sided, &mut opaque);
            quads.push(q);
        }
        settle(&mut quads, &sided, &opaque);
        quads
    }

    #[test]
    fn a_menu_takes_back_under_it_the_pieces_it_covers_on_a_four_by_three_screen() {
        let s = 106.0;
        // the compass, then a full-screen window's backdrop (642x332 at -1,-1)
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        let backdrop = (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle);
        let q = layer(s, &[compass, backdrop]);
        assert_eq!(q[0].pos[0], [10.0, 15.0]);
        assert_eq!(q[0].clip, Some([0, 0, 640, 448]));
        // the target's bar below it stays at the side, and so does the chat box at the right
        let target = (20.0, 194.0, 373.0, 399.0, 3, 1.0, Anchor::Left);
        let chat = (191.0, 653.0, 380.0, 449.0, 3, 1.0, Anchor::Right);
        let q = layer(s, &[target, chat, backdrop]);
        assert_eq!((q[0].pos[0][0], q[1].pos[0][0]), (20.0 - s, 191.0 + s));
        // nothing moves on a 4:3 screen
        let q = layer(0.0, &[compass, backdrop]);
        assert_eq!((q[0].pos[0][0], q[0].clip), (10.0, None));
    }

    #[test]
    fn only_an_opaque_window_made_after_a_piece_covers_it() {
        let s = 106.0;
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        // a box that leaves the corner alone (the options)
        let boxed = layer(s, &[compass, (120.0, 520.0, 100.0, 330.0, 0, 1.0, Anchor::Middle)]);
        assert_eq!(boxed[0].pos[0][0], 10.0 - s);
        // a backdrop drawn before the piece is under it, not over it
        let before = layer(s, &[(-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle), compass]);
        assert_eq!(before[1].pos[0][0], 10.0 - s);
        // a translucent box does not hide it; one blended at full alpha does
        let dim = layer(s, &[compass, (-1.0, 641.0, -1.0, 331.0, 3, 0.75, Anchor::Middle)]);
        assert_eq!(dim[0].pos[0][0], 10.0 - s);
        let full = layer(s, &[compass, (-1.0, 641.0, -1.0, 331.0, 3, 1.0, Anchor::Middle)]);
        assert_eq!(full[0].pos[0][0], 10.0);
        // a piece of another side is no window
        let sided = layer(s, &[compass, (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Right)]);
        assert_eq!(sided[0].pos[0][0], 10.0 - s);
    }
}
