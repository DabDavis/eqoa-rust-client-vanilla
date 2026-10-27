//! `eqoa mapshot`: a world's map built and drawn offline, natively, as the
//! client builds it in play (the same `Map`, frame by frame at 60 a second
//! with its budget, the cache and all), then a picture of it. For looking at
//! the map without a server, and for measuring the build.

use super::{Context, Map};
use render::wgpu;
use web_time::{Duration, Instant};

/// What `mapshot` is asked: the world, where the player stands and faces,
/// the view's scale (world units a pixel; none: the opening one), the
/// picture's size, how long the build may run.
pub struct Shot {
    pub world: String,
    pub at: [f32; 2],
    pub heading: f32,
    pub scale: Option<f32>,
    pub size: (u32, u32),
    pub secs: f32,
    pub budget_ms: Option<u64>,
}

/// The process's peak memory (Linux's `VmHWM`), MB.
fn peak_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f64 = s.lines().find(|l| l.starts_with("VmHWM:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

pub fn shot(s: &Shot, out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let iso = crate::disc_path(None);
    let mut disc = iso::Iso::open(&iso)?;
    let mut lib = esf::Library::new();
    lib.add(&s.world, crate::stream::open_file(std::path::Path::new(&iso), &s.world)?);
    for f in ["UI.CSF", "ARIALUI.CSF"] {
        lib.add(f, esf::Container::open(disc.read(f)?)?);
    }
    let words = disc.read("WORDS.TXT").ok().and_then(|b| crate::spelling::words(&b));
    let world = crate::play::WORLD_FILES.iter().position(|f| f.eq_ignore_ascii_case(&s.world)).unwrap_or(0) as i32;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    eprintln!("mapshot on {:?}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let mut map = Map::default();
    // the community's markers only from a list named (`EQOA_MAPMARKS`): a
    // tool's run asks no server of its own accord
    map.community.set_base(std::env::var("EQOA_MAPMARKS").ok().filter(|b| !b.trim().is_empty()));
    crate::modules::load_now(crate::modules::Module::Client, std::path::Path::new(&iso));
    if let Some(img) = crate::modules::get(crate::modules::Module::Client).and_then(|i| crate::client_image::ClientImage::from_bytes(i.to_vec())) {
        map.client_data(&img);
    }
    let budget = s.budget_ms.map(Duration::from_millis);
    let t = Instant::now();
    let mut frames = 0u32;
    map.show();
    // `EQOA_MAP_HOVER=<x>,<z>`: that world point taken as the mouse's, its
    // marker's card shown (`markers`)
    map.hover_world = std::env::var("EQOA_MAP_HOVER").ok().and_then(|v| {
        let (x, z) = v.split_once(',')?;
        Some([x.trim().parse().ok()?, z.trim().parse().ok()?])
    });
    let player = Some(([s.at[0], 0.0, s.at[1]], s.heading));
    loop {
        let cx = Context { lib: &lib, device: &device, queue: &queue, format: render::TARGET_FORMAT, size: s.size, dt: 1.0 / 60.0, player, world, origin: [1000.0, 1000.0], group: Vec::new(), in_group: false, target: None, sticks: [0.0; 4], keyboard: true, words: words.as_ref(), vui: None, quest_marks: None };
        map.budget = budget;
        map.frame(&cx);
        frames += 1;
        let done = map.world.as_ref().is_some_and(|w| w.builder.done() && w.builder.tiles_out.is_empty());
        if done && frames > 2 || t.elapsed().as_secs_f32() > s.secs {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    if let (Some(sc), Some(v)) = (s.scale, map.view.as_mut()) {
        v.scale = sc;
        map.follow = true;
    }
    // a few more frames: the view's tiles asked of the cache and shown
    for _ in 0..30 {
        let cx = Context { lib: &lib, device: &device, queue: &queue, format: render::TARGET_FORMAT, size: s.size, dt: 1.0 / 60.0, player, world, origin: [1000.0, 1000.0], group: Vec::new(), in_group: false, target: None, sticks: [0.0; 4], keyboard: true, words: words.as_ref(), vui: None, quest_marks: None };
        map.frame(&cx);
    }
    if let Some(w) = &map.world {
        let st = w.builder.stats;
        eprintln!(
            "{} after {frames} frames, {:.1} s: busy {:.2} s, the longest frame {:.1} ms; peak memory {:.0} MB",
            w.builder.describe(),
            t.elapsed().as_secs_f64(),
            st.busy.as_secs_f64(),
            st.worst.as_secs_f64() * 1e3,
            peak_mb().unwrap_or(0.0)
        );
    }
    let (w, h) = s.size;
    let px = capture(&map, &device, &queue, s.size)?;
    let mut e = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out)?), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()?.write_image_data(&px)?;
    eprintln!("{out}: the map of {} at ({}, {}), scale {:?}", s.world, s.at[0], s.at[1], map.view.map(|v| v.scale));
    Ok(())
}

/// The map up, drawn alone into a `size` picture (over black, as it covers
/// the frame) and read back: RGBA rows.
pub fn capture(map: &Map, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32)) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (w, h) = size;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("map capture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = render::depth_view(device, w, h);
    let row = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("map capture"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&Default::default());
    let tv = target.create_view(&Default::default());
    {
        let _ = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &tv, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    map.draw(device, &mut enc, &tv, &depth, size);
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = buf.slice(..).get_mapped_range()?;
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        px.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
    }
    Ok(px)
}
