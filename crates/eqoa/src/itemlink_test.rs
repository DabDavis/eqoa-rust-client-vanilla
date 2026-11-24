//! Offline real-disc pointer coverage for two item-comparison frames.
use super::*;
use crate::play::offline_hud_test::{fixture_play, offline_assets};
use render::wgpu;

#[test]
#[ignore = "manual offline GPU interaction test: requires local disc"]
fn two_inspections_move_zoom_pin_and_close_independently() {
    let (mut lib, source, output) = offline_assets();
    crate::settings::testing::reset(false);
    crate::settings::set_ui_mode(true).unwrap();
    let size = (960, 540);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&render::packed::device_descriptor(&adapter))).unwrap();
    let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let mut p = fixture_play();
    p.loading.wait = None;
    p.take_modules(None);
    p.attach_ui(&mut renderer, &device, &queue, render::TARGET_FORMAT, &lib);
    let scene = source.scene(&lib);
    let (walker, _, _) = source.walker(&mut lib, &scene).unwrap();
    for (id, name) in [(1, "First sword"), (2, "Second shield")] {
        p.open_link(id, name);
        let mut item = net::dump::NetItem::default();
        item.item.name = name.into();
        p.links.record(id, Some(item));
    }
    let mut now = 1000;
    let frame = |p: &mut Play, renderer: &mut render::Renderer, now: &mut i64| {
        *now += 100;
        p.hud(renderer.ui.as_mut().unwrap(), &walker, *now, glam::Mat4::IDENTITY, size, true);
    };
    let px = |at| render::ui::window_from_frame(size, at);
    let drag = |p: &mut Play, a, b| {
        assert!(p.hud_press(px(a), size));
        assert!(p.hud_drag(px(b), size));
        assert!(p.hud_release());
    };
    let press = |p: &mut Play, at| {
        assert!(p.hud_press(px(at), size));
        assert!(p.hud_release());
    };
    p.chat_line("/hud");
    frame(&mut p, &mut renderer, &mut now);
    let a = p.links.frame_geometry(0).unwrap();
    let b = p.links.frame_geometry(1).unwrap();
    assert_ne!(a.0, b.0, "distinct stable comparison keys");
    assert!(p.close_link_window());
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(0).unwrap().2, a.2, "survivor source does not jump when other slot closes");
    p.open_link(2, "Second shield");
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(0).unwrap().2, a.2, "survivor source does not jump when other slot reopens");
    now += 100;
    p.hud(renderer.ui.as_mut().unwrap(), &walker, now, glam::Mat4::IDENTITY, (640, 480), true);
    assert_eq!(p.links.frame_geometry(0).unwrap().2, a.2, "source stays stable when the outer window changes aspect");
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(1).unwrap().2, b.2, "the other source stays stable too");
    let head = [a.1[0] + 15.0, a.1[1] + 8.0];
    drag(&mut p, head, [head[0] - 35.0, head[1] + 25.0]);
    frame(&mut p, &mut renderer, &mut now);
    let moved = p.links.frame_geometry(0).unwrap();
    assert_ne!(moved.1, a.1);
    assert_eq!(p.links.frame_geometry(1).unwrap().1, b.1, "other comparison stays put");
    press(&mut p, [b.1[2] - 71.0, b.1[1] + 9.0]);
    frame(&mut p, &mut renderer, &mut now);
    let zoomed = p.links.frame_geometry(1).unwrap();
    assert!(zoomed.3 > b.3, "second comparison zooms independently");
    assert_eq!(p.links.frame_geometry(0).unwrap().3, a.3);
    // Zoom alone expands the frame. Narrow it until it needs a horizontal
    // scrollbar, then pan that viewport; its fixed X must stay available.
    let corner = [zoomed.1[2] - 5., zoomed.1[3] - 5.];
    drag(&mut p, corner, [corner[0] - 48., corner[1]]);
    frame(&mut p, &mut renderer, &mut now);
    let narrow = p.links.frame_geometry(1).unwrap().1;
    press(&mut p, [narrow[2] - 20.0, narrow[3] - 5.0]);
    frame(&mut p, &mut renderer, &mut now);
    assert!(p.links.slot_covers(1, p.links.close_position(1).unwrap()));
    press(&mut p, [narrow[2] - 45.0, narrow[1] + 9.0]);
    frame(&mut p, &mut renderer, &mut now);
    let pinned = p.links.frame_geometry(1).unwrap().1;
    let h = [pinned[0] + 15.0, pinned[1] + 8.0];
    drag(&mut p, h, [h[0] - 25.0, h[1] + 10.0]);
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(1).unwrap().1, pinned, "pin holds second comparison");
    press(&mut p, [pinned[2] - 45.0, pinned[1] + 9.0]);
    frame(&mut p, &mut renderer, &mut now);
    // Put the top comparison over the lower one's X. Its frame must consume
    // the click rather than closing the covered lower window.
    let under_x = p.links.close_position(0).unwrap();
    let top = p.links.frame_geometry(1).unwrap().1;
    let h = [top[0] + 15.0, top[1] + 8.0];
    drag(&mut p, h, [h[0] + under_x[0] - 100.0 - top[0], h[1] + under_x[1] - 40.0 - top[1]]);
    frame(&mut p, &mut renderer, &mut now);
    assert!(p.links.slot_covers(1, under_x), "top frame covers lower X");
    let at = px(under_x);
    assert!(p.point(crate::mouse::Click::Target(at[0] as f64, at[1] as f64), size, false));
    assert_eq!(p.links.windows.len(), 2, "covered X did not close the lower comparison");
    p.chat_line("/hud lock");
    frame(&mut p, &mut renderer, &mut now);
    let top_x = px(p.links.close_position(1).unwrap());
    assert!(p.point(crate::mouse::Click::Target(top_x[0] as f64, top_x[1] as f64), size, false));
    assert_eq!(p.links.windows.len(), 1);
    assert_eq!(p.links.windows[0].id, 1);
    p.chat_line("/hud");
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(0).unwrap().1, moved.1, "survivor retains placement after neighbor closes");
    let survivor_x = px(p.links.close_position(0).unwrap());
    assert!(p.point(crate::mouse::Click::Target(survivor_x[0] as f64, survivor_x[1] as f64), size, false));
    assert!(p.links.windows.is_empty(), "survivor remains clickable and closable");
    p.open_link(1, "First sword");
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(0).unwrap().1, moved.1, "reopening restores the first comparison slot");
    p.open_link(3, "Third item");
    frame(&mut p, &mut renderer, &mut now);
    assert_eq!(p.links.frame_geometry(0).unwrap().1, moved.1, "survivor retains its own placement");
    assert_eq!(p.links.frame_geometry(1).unwrap().3, zoomed.3, "replacement reuses the second comparison slot");
    eprintln!("comparison slots moved, zoomed, pinned, overlapped and closed independently; saved {}", crate::settings::pc_window_layout());
    let mut capture = fixture_play();
    for (id, name) in [(1, "First sword"), (2, "Second shield")] {
        capture.open_link(id, name);
        let mut item = net::dump::NetItem::default();
        item.item.name = name.into();
        capture.links.record(id, Some(item));
    }
    let script = crate::viewer::Script { ms: 1000, controls: Default::default() };
    let path = output.join("offline-item-comparison.png");
    crate::viewer::shot_with(&mut lib, &source, None, Some(script), size, path.to_str().unwrap(), Some(capture)).unwrap();
}
