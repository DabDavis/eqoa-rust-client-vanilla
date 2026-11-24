//! Real-disc, offline interaction and framebuffer coverage for the PC HUD.
use super::*;
use crate::hudlayout::{self, Beyond, Piece};
use std::path::Path;
use std::time::Duration;

type Size = (u32, u32);
type Rect = [f32; 4];

// Keep one static world, camera and GPU for every input and timed observation.
// This also makes the terrain behind each chat pane a stable pixel reference.
struct World {
    lib: esf::Library,
    walker: crate::walker::Walker,
    stream: crate::stream::Streamer,
    body: render::GpuModel,
    renderer: render::Renderer,
    device: wgpu::Device,
    queue: wgpu::Queue,
    view: render::View,
}

impl World {
    fn new(mut lib: esf::Library, source: &crate::viewer::Source, p: &mut Play) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&render::packed::device_descriptor(&adapter))).unwrap();
        let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        p.loading.wait = None;
        p.take_modules(None);
        p.attach_ui(&mut renderer, &device, &queue, render::TARGET_FORMAT, &lib);
        let scene = source.scene(&lib);
        let view = source.camera(scene.bounds).view(scene.bounds, 16. / 9.);
        let (mut walker, model, mut stream) = source.walker(&mut lib, &scene).unwrap();
        let at = glam::Vec3::from(walker.cam.walk.phys.position.to_array());
        stream.update(at, &mut walker.scene, &lib, crate::stream::Gpu { renderer: &mut renderer, device: &device, queue: &queue }, None);
        let (matrix, palette) = walker.pose();
        let mut body = renderer.upload(&device, &queue, &model, &[matrix], &render::library_textures(&lib));
        renderer.set_palette(&queue, &body, &palette);
        crate::play::place_body(&mut body, &walker, matrix);
        Self { lib, walker, stream, body, renderer, device, queue, view }
    }

    fn frame(&mut self, p: &mut Play, size: Size) {
        let (projection, view) = self.walker.view(size.0 as f32 / size.1 as f32);
        self.view.view_proj = projection * view;
        self.view.eye = self.walker.eye_position();
        p.map_frame(&self.walker, &self.lib, &self.device, &self.queue, render::TARGET_FORMAT, size, 1./30., [0.; 4]);
        p.hud(self.renderer.ui.as_mut().unwrap(), &self.walker, i64::from(p.session.now()), self.view.view_proj, size, true);
    }

    fn pixels(&mut self, p: &Play, size: Size) -> Vec<u8> {
        let items: Vec<_> = self.stream.draws().chain(std::iter::once(render::Draw::Model(&self.body))).collect();
        render::capture_around(&self.device, &self.queue, &mut self.renderer, size.0, size.1, &self.view, &items, None, None,
            |encoder, color, depth, render_size| p.map.draw_mini(&self.device, encoder, color, depth, size, render_size))
    }
}

fn rect(piece: Piece) -> Rect { hudlayout::drawn(piece).unwrap_or_else(|| panic!("{piece:?} missing")) }
fn size_of(r: Rect) -> [f32; 2] { [r[2] - r[0], r[3] - r[1]] }
fn in_rect(r: Rect, x: f32, y: f32) -> [f32; 2] { [r[0] + x * (r[2]-r[0]), r[1] + y * (r[3]-r[1])] }
fn pixel(size: Size, at: [f32; 2]) -> [f32; 2] { render::ui::window_from_frame(size, at) }
fn mouse(size: Size, at: [f32; 2]) -> (f64, f64) { let [x, y] = pixel(size, at); (x.into(), y.into()) }

fn resize(world: &mut World, p: &mut Play, size: Size, piece: Piece) {
    let before = rect(piece);
    let previous = hudlayout::piece_scale_percent(piece);
    let dims = size_of(before);
    let screen = Beyond::of(size).screen();
    // Use a rendered handle whose direction has room for a real pointer.
    let (from, to, sign) = hudlayout::scale_grips(piece).into_iter().find_map(|(r, sign)| {
        let from = in_rect(r, 0.5, 0.5);
        let to = [from[0] + dims[0] * 0.18 * sign[0], from[1] + dims[1] * 0.18 * sign[1]];
        (to[0] >= screen[0] && to[1] >= screen[1] && to[0] < screen[2] && to[1] < screen[3]).then_some((from, to, sign))
    }).unwrap_or_else(|| panic!("no on-screen scaling handle for {piece:?}: {before:?}"));
    assert!(p.hud_press(pixel(size, from), size), "{piece:?} size handle");
    assert_eq!(hudlayout::dragged(), Some(piece), "handle must belong to its own piece");
    assert!(p.hud_drag(pixel(size, to), size));
    assert!(p.hud_release());
    world.frame(p, size);
    let painted = rect(piece);
    let after = size_of(painted);
    let next = hudlayout::piece_scale_percent(piece);
    assert!(next > previous && after[0] > dims[0] * 1.04 && after[1] > dims[1] * 1.04, "{piece:?}: {dims:?}/{previous}% → {after:?}/{next}%");
    let fixed = |r: Rect| [r[if sign[0] > 0. { 0 } else { 2 }], r[if sign[1] > 0. { 1 } else { 3 }]];
    let anchor = fixed(before);
    let fits = (0..2).all(|axis| {
        let outward = anchor[axis] + sign[axis] * after[axis];
        outward >= screen[axis] && outward <= screen[axis + 2]
    });
    if fits {
        let a = pixel(size, anchor);
        let b = pixel(size, fixed(painted));
        assert!((a[0]-b[0]).abs() <= 1.5 && (a[1]-b[1]).abs() <= 1.5, "{piece:?} changed its opposite painted corner: {a:?} → {b:?}, handle {sign:?}");
    }
    eprintln!("HUD handle {piece:?}: {previous}% → {next}%, {before:?} → {painted:?}, fixed corner {anchor:?} checked={fits}");
}

fn hit(p: &mut Play, size: Size, at: [f32;2]) -> bool {
    let (x, y) = mouse(size, at);
    p.point(crate::mouse::Click::Target(x, y), size, false)
}

fn cast_slots(p: &Play) -> Vec<u8> {
    p.session.record.as_ref().unwrap().iter().filter_map(|(channel, data)|
        (*channel == drdp::TYPE_GUARANTEED && data.starts_with(&[0x32, 0])).then(|| data[6])).collect()
}

fn added_area(current: Rect, original: Rect) -> [f32; 2] {
    for y in [0.67, 0.88, 0.12, 0.94, 0.06] { for x in [0.08, 0.92, 0.5] {
        let q = in_rect(current, x, y);
        if q[0] < original[0] - 1. || q[0] > original[2] + 1. || q[1] < original[1] - 1. || q[1] > original[3] + 1. {
            return q;
        }
    }}
    panic!("no newly occupied area in {current:?} compared with {original:?}");
}

fn check_hits(world: &mut World, p: &mut Play, size: Size, original: [Rect;2]) {
    let mini = rect(Piece::Minimap);
    let rim = added_area(mini, original[0]);
    assert!(p.map.mini_contains(pixel(size, rim)), "map layer's wheel/hover rectangle at {size:?}");
    assert!(!p.map.mini_contains(pixel(size, [mini[0] - 4., rim[1]])), "beyond the drawn left roller");
    assert!(hit(p, size, rim) && p.map.open, "clicking the resized minimap opens the map");
    p.map.close();
    assert!(hit(p, size, in_rect(rect(Piece::Player), 0.91, 0.52)));
    assert_eq!(p.target, 0x100, "clicking the scaled player's status bars targets self");
    p.target = 0x200;
    world.frame(p, size);
    let spells = rect(Piece::SpellBars);
    p.session.record = Some(Vec::new());
    assert!(hit(p, size, in_rect(spells, 0.15, 0.875)), "bar one, second gem");
    assert!(hit(p, size, in_rect(spells, 0.95, 0.125)), "bar four, tenth gem");
    let mut edge = crate::binds::KeyEdge::default();
    edge.pc_mode = true;
    let Some(crate::binds::Sent::Slot(bar, gem)) = edge.key("2", true, false, false, &crate::settings::binds()) else { panic!("2 must resolve to an equipped gem"); };
    p.keyboard_slot_edge(bar, gem, true, false);
    p.keyboard_slot_edge(bar, gem, false, false);
    assert_eq!(cast_slots(p), [1, 39, 1], "rendered cells and keyboard dispatch on {size:?}");
    let pane = added_area(rect(Piece::Chat1), original[1]);
    assert!(p.chat_wheel(mouse(size, pane), size, 1.), "detached pane takes wheel in its resized bounds");
    assert!(p.chat_wheel(mouse(size, pane), size, -1.));
    eprintln!("{size:?}: minimap hit/wheel area, player selection, cast slots [1,39,1], detached chat wheel verified");
}

// Convert each pane's bounds into the physical framebuffer for independent
// measurements of color changes and the seam beneath the tabs.
fn physical(size: Size, r: Rect) -> [usize; 4] {
    let a = pixel(size, [r[0], r[1]]);
    let b = pixel(size, [r[2], r[3]]);
    [a[0].ceil().max(0.) as usize, a[1].ceil().max(0.) as usize,
        b[0].floor().min(size.0 as f32) as usize, b[1].floor().min(size.1 as f32) as usize]
}

fn difference(size: Size, a: &[u8], b: &[u8], r: [usize; 4]) -> usize {
    let mut count = 0;
    for y in r[1]..r[3] { for x in r[0]..r[2] {
        let i = 4 * (y * size.0 as usize + x);
        if (0..3).any(|c| a[i+c].abs_diff(b[i+c]) > 8) { count += 1; }
    }}
    count
}

fn wait_until(p: &Play, start: u32, millis: u32) {
    let elapsed = p.session.now().wrapping_sub(start);
    std::thread::sleep(Duration::from_millis(u64::from(millis.saturating_sub(elapsed))));
}

fn fade(world: &mut World, p: &mut Play, size: Size, out: &Path) {
    let mut options = crate::settings::client();
    assert!(!options.chat_wake_on_message, "incoming traffic is passive in the default PC profile");
    options.chat_fade_seconds = 1;
    options.chat_wake_on_message = true;
    crate::settings::set_client(options);
    p.chat_moved((size.0 as f64 / 2., size.1 as f64 / 2.), size);
    let start = p.session.now();
    p.add_chat("Friend: The gate is open.".into(), [1.; 4]);
    p.add_chat("Tester tells you, 'Keep watch.'".into(), [1.; 4]);
    let capture = |world: &mut World, p: &mut Play| {
        world.frame(p, size);
        world.pixels(p, size)
    };
    let active = capture(world, p);
    let main = physical(size, rect(Piece::Chat));
    let tells = physical(size, rect(Piece::Chat1));
    wait_until(p, start, 1500);
    let mid_time = p.session.now().wrapping_sub(start);
    let half = capture(world, p);
    wait_until(p, start, 2120);
    let idle = capture(world, p);
    for r in [main, tells] {
        let full = difference(size, &active, &idle, r);
        let fading = difference(size, &half, &idle, r);
        assert!(full > 400 && fading > 100 && difference(size, &half, &active, r) > 100, "progressive fade at {mid_time}ms: {r:?}, full={full}, half={fading}");
        // This strip crosses the tab's bottom and the panel's top. A gap
        // exposes the same terrain as the fully faded frame for an entire row.
        let left = r[0] + 12;
        for y in r[1] + 4..(r[1] + 36).min(r[3]) {
            let count = difference(size, &active, &idle, [left, y, left + 18, y + 1]);
            assert!(count >= 3, "daylight between chat header and panel at physical row {y} of {r:?}");
        }
    }
    let mut options = crate::settings::client();
    options.chat_wake_on_message = false;
    crate::settings::set_client(options);
    p.add_chat("You hit a training dummy for 24 damage.".into(), [1.;4]);
    p.add_chat("Tester tells you, 'Checking the idle feed.'".into(), [1.;4]);
    let passive = capture(world, p);
    assert!(difference(size, &idle, &passive, main) < 30 && difference(size, &idle, &passive, tells) < 30,
        "passive incoming combat and tells must leave idle panes faded");
    let mut options = crate::settings::client();
    options.chat_wake_on_message = true;
    crate::settings::set_client(options);
    // The hidden panes retain their actual pointer targets.
    p.chat_moved(mouse(size, in_rect(rect(Piece::Chat1), 0.6, 0.6)), size);
    let hover = capture(world, p);
    assert!(difference(size, &idle, &hover, tells) > 400 && difference(size, &idle, &hover, main) < 30);
    p.add_chat("Friend: The group is ready.".into(), [1.; 4]);
    let incoming = capture(world, p);
    assert!(difference(size, &idle, &incoming, main) > 400 && difference(size, &idle, &incoming, tells) > 400);
    p.chat_moved((size.0 as f64 / 2., size.1 as f64 / 2.), size);
    let reset = p.session.now();
    wait_until(p, reset, 2120);
    let idle_again = capture(world, p);
    assert!(difference(size, &idle, &idle_again, main) < 30 && difference(size, &idle, &idle_again, tells) < 30);
    p.add_chat("Tester tells you, 'Only the Tells pane should wake.'".into(), [1.; 4]);
    let routed = capture(world, p);
    assert!(difference(size, &idle, &routed, tells) > 400 && difference(size, &idle, &routed, main) < 30);
    let mut options = crate::settings::client();
    options.chat_fade_seconds = 0;
    crate::settings::set_client(options);
    let off = capture(world, p);
    assert!(difference(size, &idle, &off, main) > 400);
    // Stack the Main and Tells crops for easy comparison across frames,
    // even when their actual positions lie on opposite sides of the screen.
    let width = (main[2]-main[0]).max(tells[2]-tells[0]);
    let height = main[3]-main[1] + 3 + tells[3]-tells[1];
    for (name, pixels) in [("active", active), ("mid", half), ("idle", idle), ("hover-tells", hover), ("incoming", incoming), ("routed-tell", routed)] {
        let mut rows = vec![0; 4 * width * height];
        for (r, offset) in [(main, 0), (tells, main[3] - main[1] + 3)] {
            for y in r[1]..r[3] {
                let from = 4 * (y * size.0 as usize + r[0]);
                let to = 4 * (offset + y - r[1]) * width;
                let length = 4 * (r[2] - r[0]);
                rows[to..to+length].copy_from_slice(&pixels[from..from+length]);
            }
        }
        crate::texpack::write_png(&out.join(format!("offline-pc-chat-{name}.png")), width as u32, height as u32, &rows).unwrap();
    }
    eprintln!("chat elapsed {mid_time}ms midfade, 2120ms fully faded; passive combat and tells stay hidden, optional incoming/hover wake and delay Off restore the relevant panes; no exposed terrain between the tabs and panels at {main:?}, {tells:?}");
}

#[test]
#[ignore = "manual real-disc offline GPU, 960x540 and 640x480"]
fn offline_resized_hud_inputs_and_timed_chat() {
    let (lib, source, out) = offline_assets();
    crate::settings::testing::reset(false);
    hudlayout::testing_reset();
    crate::settings::set_ui_mode(true).unwrap();
    crate::settings::set_chat_detached(1, true).unwrap();
    let mut p = fixture_play();
    for i in 0..15 { p.add_chat(format!("Tester tells you, 'Line {i}.'"), [1.;4]); }
    let mut world = World::new(lib, &source, &mut p);
    let wide = (960, 540);
    for _ in 0..8 { world.frame(&mut p, wide); }
    let sizes = [wide, (640, 480)];
    let original = sizes.map(|size| {
        world.frame(&mut p, size);
        world.frame(&mut p, size);
        [rect(Piece::Minimap), rect(Piece::Chat1)]
    });
    hudlayout::set_unlocked(true);
    world.frame(&mut p, wide);
    world.frame(&mut p, wide);
    for (piece, _) in hudlayout::drawn_all() {
        assert!(!hudlayout::scale_grips(piece).is_empty(), "visible unlocked {piece:?} should expose a size handle");
    }
    let pieces = [Piece::Target, Piece::Player, Piece::Minimap, Piece::SpellBars, Piece::Chat1];
    for piece in pieces { resize(&mut world, &mut p, wide, piece); }
    // Move the detached pane a short distance inside its current column.
    // Either horizontal or vertical separation allows independent sampling.
    let tells = rect(Piece::Chat1);
    let main = rect(Piece::Chat);
    let sideways = tells[0] >= main[2] || main[0] >= tells[2];
    let delta = if sideways { [-14., 14.] } else { [0., -(tells[3] - main[1] + 16.).max(20.)] };
    let at = in_rect(tells, 0.55, 0.55);
    assert!(p.hud_press(pixel(wide, at), wide));
    assert_eq!(hudlayout::dragged(), Some(Piece::Chat1));
    assert!(p.hud_drag(pixel(wide, [at[0]+delta[0], at[1]+delta[1]]), wide) && p.hud_release());
    world.frame(&mut p, wide);
    let moved = rect(Piece::Chat1);
    assert_ne!(moved, tells);
    assert!(moved[2] <= main[0] || main[2] <= moved[0] || moved[3] <= main[1] || main[3] <= moved[1], "expected independent panes: Main={main:?}, Tells={moved:?}");
    let target = rect(Piece::Target);
    let pin = [target[2] - 6., target[1] + 5.];
    assert!(p.hud_press(pixel(wide, pin), wide) && p.hud_release());
    assert!(hudlayout::layout().to_text().contains("pin target\n"), "the displayed pin controls the target");
    world.frame(&mut p, wide);
    assert!(hudlayout::scale_grips(Piece::Target).is_empty(), "pinned target has no active resize control");
    let center = in_rect(target, 0.5, 0.5);
    assert!(p.hud_press(pixel(wide, center), wide));
    assert!(!p.hud_drag(pixel(wide, [center[0]+20., center[1]+10.]), wide));
    assert!(p.hud_release());
    world.frame(&mut p, wide);
    crate::texpack::write_png(&out.join("offline-pc-move-hud.png"), wide.0, wide.1, &world.pixels(&p, wide)).unwrap();
    let saved = hudlayout::layout();
    hudlayout::set_unlocked(false);
    hudlayout::testing_reset();
    crate::settings::set_ui_mode(false).unwrap();
    assert!(pieces.into_iter().all(|piece| hudlayout::piece_scale_percent(piece) == 100), "Controller uses its own native HUD sizes");
    crate::settings::set_ui_mode(true).unwrap();
    assert!(hudlayout::layout().to_text().contains("pin target\n"));
    assert_eq!(hudlayout::layout(), saved, "saved sizes, pins and placements survive the reload");
    for (size, original) in sizes.into_iter().zip(original) {
        world.frame(&mut p, size);
        world.frame(&mut p, size);
        check_hits(&mut world, &mut p, size, original);
        crate::texpack::write_png(&out.join(format!("offline-resized-hud-{}x{}.png", size.0, size.1)), size.0, size.1, &world.pixels(&p, size)).unwrap();
    }
    world.frame(&mut p, wide);
    world.frame(&mut p, wide);
    fade(&mut world, &mut p, wide, &out);
}
