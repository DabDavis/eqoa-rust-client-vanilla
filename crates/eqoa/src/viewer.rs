//! The model viewer: every drawable model in one container, on wgpu + winit.
//!
//! Drag to orbit, scroll to zoom, ←/→ for the previous/next model, Esc quits.
//! `shot` renders the same view offscreen to a PNG, with no window.

use glam::Vec3;
use render::{GpuModel, Renderer, View, wgpu};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub struct Models {
    pub container: esf::Container,
    /// (DictID, model) for every sprite that builds.
    pub list: Vec<(i32, esf::Model)>,
}

impl Models {
    pub fn new(container: esf::Container) -> Self {
        let list = container
            .models()
            .iter()
            .filter_map(|r| container.build_model(r.node).map(|m| (r.dict_id, m)))
            .collect();
        Models { container, list }
    }

    /// A model by list index, or by DictID when written as hex (`0x…`).
    pub fn find(&self, which: &str) -> Option<usize> {
        match which.strip_prefix("0x") {
            Some(hex) => {
                let id = u32::from_str_radix(hex, 16).ok()? as i32;
                self.list.iter().position(|(d, _)| *d == id)
            }
            None => which.parse().ok().filter(|&i| i < self.list.len()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { yaw: 0.6, pitch: 0.3, zoom: 1.0 }
    }
}

/// Frames the model's bounds. Models are Y-up (checked by eye on the human,
/// 0x70D898C6).
pub fn frame_view(bounds: (Vec3, Vec3), orbit: Orbit, aspect: f32) -> View {
    let center = (bounds.0 + bounds.1) * 0.5;
    let radius = ((bounds.1 - bounds.0).length() * 0.5).max(0.01);
    let dist = radius * 2.6 * orbit.zoom;
    let dir = Vec3::new(orbit.yaw.sin() * orbit.pitch.cos(), orbit.pitch.sin(), orbit.yaw.cos() * orbit.pitch.cos());
    let eye = center + dir * dist;
    let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), aspect, dist * 0.01, dist * 10.0);
    View { view_proj: proj * glam::camera::rh::view::look_at_mat4(eye, center, Vec3::Y), light_dir: Vec3::new(-0.4, -0.8, -0.5) }
}

/// Renders one view of a model to a PNG, with no window.
pub fn shot(models: &Models, index: usize, orbit: Orbit, (w, h): (u32, u32), out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb);
    let gpu = renderer.upload(&device, &queue, &models.container, &models.list[index].1);
    let view = frame_view(gpu.bounds, orbit, w as f32 / h as f32);
    let rgba = render::capture(&device, &queue, &renderer, w, h, &view, &[&gpu]);
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    eprintln!("{out}: model {index} ({}), adapter {}", describe(models, index), adapter.get_info().name);
    Ok(())
}

pub fn describe(models: &Models, i: usize) -> String {
    let (id, m) = &models.list[i];
    format!("0x{:08X}, {} verts, {} tris, {} parts{}", *id as u32, m.pos.len(), m.triangles, m.parts, if m.skinned { ", skinned" } else { "" })
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    renderer: Renderer,
    gpu: GpuModel,
}

struct App {
    models: Models,
    index: usize,
    orbit: Orbit,
    drag: Option<(f64, f64)>,
    cursor: (f64, f64),
    gfx: Option<Gfx>,
    display: winit::event_loop::OwnedDisplayHandle,
}

pub fn run(models: Models, index: usize) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let display = event_loop.owned_display_handle();
    let mut app = App { models, index, orbit: Orbit::default(), drag: None, cursor: (0.0, 0.0), gfx: None, display };
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl App {
    fn title(&self) -> String {
        format!("eqoa — model {}/{}: {}", self.index, self.models.list.len(), describe(&self.models, self.index))
    }

    fn select(&mut self, index: usize) {
        self.index = index;
        let title = self.title();
        if let Some(g) = &mut self.gfx {
            g.gpu = g.renderer.upload(&g.device, &g.queue, &self.models.container, &self.models.list[index].1);
            g.window.set_title(&title);
            g.window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let window = Arc::new(el.create_window(Window::default_attributes().with_title(self.title())).expect("window"));
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(self.display.clone())));
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).expect("device");
        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface config");
        config.format = config.format.add_srgb_suffix();
        surface.configure(&device, &config);
        let renderer = Renderer::new(&device, &queue, config.format);
        let gpu = renderer.upload(&device, &queue, &self.models.container, &self.models.list[self.index].1);
        let depth = render::depth_view(&device, config.width, config.height);
        self.gfx = Some(Gfx { window, surface, device, queue, config, depth, renderer, gpu });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(g) = &mut self.gfx else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) if s.width > 0 && s.height > 0 => {
                g.config.width = s.width;
                g.config.height = s.height;
                g.surface.configure(&g.device, &g.config);
                g.depth = render::depth_view(&g.device, s.width, s.height);
                g.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let n = self.models.list.len();
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => el.exit(),
                    Key::Named(NamedKey::ArrowRight) => self.select((self.index + 1) % n),
                    Key::Named(NamedKey::ArrowLeft) => self.select((self.index + n - 1) % n),
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.drag = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if let Some((x, y)) = self.drag {
                    self.orbit.yaw -= (position.x - x) as f32 * 0.01;
                    self.orbit.pitch = (self.orbit.pitch - (position.y - y) as f32 * 0.01).clamp(-1.5, 1.5);
                    self.drag = Some(self.cursor);
                    g.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.orbit.zoom = (self.orbit.zoom * 0.9f32.powf(d)).clamp(0.05, 20.0);
                g.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let frame = match g.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => {
                        g.surface.configure(&g.device, &g.config);
                        return;
                    }
                };
                let view = frame_view(g.gpu.bounds, self.orbit, g.config.width as f32 / g.config.height as f32);
                let target = frame.texture.create_view(&Default::default());
                let mut enc = g.device.create_command_encoder(&Default::default());
                g.renderer.draw(&g.queue, &mut enc, &target, &g.depth, &view, &[&g.gpu]);
                g.queue.submit([enc.finish()]);
                g.queue.present(frame);
            }
            _ => {}
        }
    }
}
