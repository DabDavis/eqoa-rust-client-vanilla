//! Offline real-disc interaction coverage for PC modal-window geometry.
//! This test never connects to a server and is ignored without the user's disc.
use super::*;

fn frame(p: &mut Play, r: &mut render::Renderer, w: &crate::walker::Walker, size: (u32, u32), now: &mut i64) {
    *now += 100;
    p.hud(r.ui.as_mut().expect("disc UI"), w, *now, glam::Mat4::IDENTITY, size, true);
}

fn open(p: &mut Play, r: &mut render::Renderer, w: &mut crate::walker::Walker, size: (u32, u32), now: &mut i64, msg: u32) {
    use crate::startmenu::keys;
    // The viewer's keyboard Escape branch calls this normal entry point;
    // the PC controller's Start instead opens Settings.
    p.open_start_menu(true);
    frame(p, r, w, size, now);
    assert_eq!(p.mode, 13, "Start opens the real menu");
    let start = p.start.as_ref().expect("CLIENT start menu");
    let menu = start.menu.main.menu;
    let n = start.menus.list[menu].entries.iter().position(|e| e.msg == msg).expect("disc menu entry");
    for _ in 0..n {
        p.start_key(keys::DOWN, true);
        frame(p, r, w, size, now);
        p.start_key(keys::DOWN, false);
        frame(p, r, w, size, now);
    }
    p.start_key(keys::CROSS, true);
    frame(p, r, w, size, now);
    p.start_key(keys::CROSS, false);
    for _ in 0..8 { frame(p, r, w, size, now); }
}

#[test]
#[ignore = "manual offline GPU interaction test: requires local disc"]
fn inventory_and_book_move_resize_zoom_and_restore() {
    let (mut lib, source, output) = offline_assets();
    crate::settings::testing::reset(false);
    crate::settings::set_ui_mode(true).unwrap();
    let size = (960, 540);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&render::packed::device_descriptor(&adapter))).unwrap();
    let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let mut play = fixture_play();
    for (i, item) in play.dump.inventory.items.iter_mut().enumerate() {
        item.item.name = format!("Fixture item {}", i + 1);
        item.inv_index = i as u32 + 1;
        item.quantity = 1;
    }
    play.loading.wait = None;
    play.take_modules(None);
    play.attach_ui(&mut renderer, &device, &queue, render::TARGET_FORMAT, &lib);
    let scene = source.scene(&lib);
    let (mut walker, _, _) = source.walker(&mut lib, &scene).expect("offline world walker");
    let mut now = 1000i64;
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let px = |p| render::ui::window_from_frame(size, p);
    let press = |p: &mut Play, at: [f32; 2]| {
        assert!(p.hud_press(px(at), size), "window control at {at:?}");
        assert!(p.hud_release());
    };
    let drag = |p: &mut Play, a: [f32; 2], b: [f32; 2]| {
        assert!(p.hud_press(px(a), size), "window drag start {a:?}");
        assert!(p.hud_drag(px(b), size), "window drag move {b:?}");
        assert!(p.hud_release());
    };
    let check_controls = |p: &mut Play, r: &mut render::Renderer, now: &mut i64, mode| {
        for (command, visible) in [("", false), ("/hud", true), ("/hud lock", false)] {
            if !command.is_empty() { p.chat_line(command); }
            frame(p, r, &walker, size, now);
            assert_eq!(p.pc_windows.geometry().expect("PC modal").0, mode);
            let button_drawn = r.ui.as_ref().unwrap().quads.iter().any(|q| q.color == [0.26, 0.23, 0.16, 1.0]);
            assert_eq!(button_drawn, visible, "window {mode} edit buttons");
        }
    };
    play.open_dialog(&crate::ui::wide("Welcome, adventurer."), false);
    check_controls(&mut play, &mut renderer, &mut now, 9);
    play.close_dialog();
    play.open_start_menu(true);
    check_controls(&mut play, &mut renderer, &mut now, 13);
    play.start_key(144, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    play.open_options();
    check_controls(&mut play, &mut renderer, &mut now, 34);
    play.panel_key(27, true);
    for _ in 0..8 { frame(&mut play, &mut renderer, &walker, size, &mut now); }
    assert_eq!(play.mode, 0, "Options closes back to play");

    play.chat_line("/hud");
    // Open via the normal Start button, then select the actual disc menu row.
    open(&mut play, &mut renderer, &mut walker, size, &mut now, 0x0200_0000);
    assert!(play.items.visible, "ordinary Start menu opened Inventory");
    let (key, before, _, zoom) = play.pc_windows.geometry().expect("Inventory frame");
    assert_eq!(key, 1013);
    assert_eq!(play.pc_windows.scrollbars(), [false; 2]);
    let header = [before[0] + 25.0, before[1] + 9.0];
    drag(&mut play, header, [header[0] + 35.0, header[1] + 20.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let (_, moved, _, _) = play.pc_windows.geometry().unwrap();
    assert_ne!(moved, before, "Inventory moved");
    let grip = [moved[2] - 5.0, moved[3] - 5.0];
    drag(&mut play, grip, [grip[0] + 30.0, grip[1] + 20.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let (_, resized, _, _) = play.pc_windows.geometry().unwrap();
    assert!(resized[2] - resized[0] > moved[2] - moved[0] || resized[3] - resized[1] > moved[3] - moved[1], "Inventory resized");
    press(&mut play, [resized[2] - 71.0, resized[1] + 9.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let (_, inventory_saved, _, inventory_zoom) = play.pc_windows.geometry().unwrap();
    assert!(inventory_zoom > zoom, "Inventory zoom increased");
    eprintln!("Inventory frame: before={before:?}, saved={inventory_saved:?}, zoom={inventory_zoom}%");
    press(&mut play, [inventory_saved[2] - 45.0, inventory_saved[1] + 9.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let pinned = play.pc_windows.geometry().unwrap().1;
    let header = [pinned[0] + 25.0, pinned[1] + 9.0];
    drag(&mut play, header, [header[0] - 20.0, header[1] - 10.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    assert_eq!(play.pc_windows.geometry().unwrap().1, pinned, "pin prevents moving Inventory");
    press(&mut play, [pinned[2] - 45.0, pinned[1] + 9.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    // Close by Start and reopen Ability through the same real menu.
    play.start_key(27, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    play.start_key(144, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    assert_eq!(play.mode, 0, "Start closes Inventory");
    open(&mut play, &mut renderer, &mut walker, size, &mut now, 0x0801_0000);
    assert_eq!(play.mode, 5, "ordinary Start menu opened Spell Book");
    let (key, book_before, source_rect, book_zoom) = play.pc_windows.geometry().expect("Spell Book frame");
    assert_eq!(key, 5);
    let capture_book = |lib: &mut esf::Library, name: &str| {
        let mut snapshot = fixture_play();
        snapshot.loading.wait = None;
        snapshot.take_modules(None);
        snapshot.mode = 5;
        snapshot.book_open = true;
        let path = output.join(name);
        let script = crate::viewer::Script { ms: 1000, controls: Default::default() };
        crate::viewer::shot_with(lib, &source, None, Some(script), size, path.to_str().unwrap(), Some(snapshot)).expect("Book PNG");
    };
    assert_eq!(play.pc_windows.scrollbars(), [false; 2], "ordinary Book open has no scrollbars");
    capture_book(&mut lib, "offline-pc-window-book.png");
    frame(&mut play, &mut renderer, &walker, (640, 480), &mut now);
    assert_eq!(play.pc_windows.scrollbars(), [false; 2], "an untouched Book fits the smaller display");
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    for _ in 0..7 {
        assert!(play.window_wheel(-1.0));
        frame(&mut play, &mut renderer, &walker, size, &mut now);
    }
    assert!(play.book.sel >= 6, "wheel reaches beyond the first six spells without a permanent track");
    for _ in 0..7 {
        assert!(play.window_wheel(1.0));
        frame(&mut play, &mut renderer, &walker, size, &mut now);
    }
    for (delta, bars, file) in [
        ([-76., 0.], [true, false], "offline-pc-window-book-narrow.png"),
        ([0., -60.], [false, true], "offline-pc-window-book-short.png"),
    ] {
        let corner = [book_before[2] - 5., book_before[3] - 5.];
        drag(&mut play, corner, [corner[0] + delta[0], corner[1] + delta[1]]);
        frame(&mut play, &mut renderer, &walker, size, &mut now);
        assert_eq!(play.pc_windows.scrollbars(), bars);
        capture_book(&mut lib, file);
        let bounds = play.pc_windows.geometry().unwrap().1;
        let corner = [bounds[2] - 5., bounds[3] - 5.];
        drag(&mut play, corner, [corner[0] - delta[0], corner[1] - delta[1]]);
        frame(&mut play, &mut renderer, &walker, size, &mut now);
        assert_eq!(play.pc_windows.scrollbars(), [false; 2], "enlarging the viewport clears its track");
        let bounds = play.pc_windows.geometry().unwrap().1;
        press(&mut play, [bounds[2] - 15., bounds[1] + 9.]);
        frame(&mut play, &mut renderer, &walker, size, &mut now);
    }
    let h = [book_before[0] + 25.0, book_before[1] + 9.0];
    drag(&mut play, h, [h[0] - 24.0, h[1] + 12.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let (_, moved, _, _) = play.pc_windows.geometry().unwrap();
    assert_ne!(moved, book_before, "Spell Book moved independently");
    let grip = [moved[2] - 5.0, moved[3] - 5.0];
    drag(&mut play, grip, [grip[0] - 25.0, grip[1] - 15.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let (_, resized, _, _) = play.pc_windows.geometry().unwrap();
    assert_ne!([resized[2]-resized[0], resized[3]-resized[1]], [moved[2]-moved[0], moved[3]-moved[1]], "Spell Book resized");
    press(&mut play, [resized[2] - 71.0, resized[1] + 9.0]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    let (_, book_saved, _, zoom) = play.pc_windows.geometry().unwrap();
    assert!(zoom > book_zoom, "Spell Book zoom increased");
    assert_eq!(play.pc_windows.scrollbars(), [true; 2]);
    eprintln!("Spell Book frame: before={book_before:?}, saved={book_saved:?}, zoom={zoom}%");
    play.chat_line("/hud lock");
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    assert_eq!(play.pc_windows.scrollbars(), [true; 2], "locking keeps needed scrollbars");
    assert_eq!(play.pc_windows.geometry().unwrap().3, zoom, "locking keeps the saved zoom");
    // Find a real book row via its existing hit test, then send its projected
    // location through Play's normal transformed mouse path.
    let original = play.book.sel;
    let mut hit = None;
    'outer: for y in (source_rect[1] as i32..source_rect[3] as i32).step_by(3) {
        for x in (source_rect[0] as i32..source_rect[2] as i32).step_by(3) {
            let point = [x as f32, y as f32];
            let projected = play.pc_windows.project(point);
            if projected[0] < book_saved[0] || projected[0] >= book_saved[2]-10.0 || projected[1] < book_saved[1]+19.0 || projected[1] >= book_saved[3]-10.0 { continue; }
            play.book.point(point);
            if play.book.sel != original { hit = Some((point, play.book.sel)); break 'outer; }
        }
    }
    let (native, expected) = hit.expect("visible selectable spell row");
    play.book.sel = original;
    let click = px(play.pc_windows.project(native));
    assert!(play.point(crate::mouse::Click::Target(click[0] as f64, click[1] as f64), size, false));
    assert_eq!(play.book.sel, expected, "transformed click selects the intended spell");
    eprintln!("Spell Book mapped selection: native={native:?}, window={click:?}, selected={expected}");
    play.book_key(27, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    assert_eq!(play.mode, 13, "Escape returns from Spell Book to Start");
    play.start_key(144, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    assert_eq!(play.mode, 0, "Start closes the menu");
    // A new window manager must reload both layouts from the saved store,
    // rather than merely retaining the first manager's in-memory state.
    play.chat_line("/hud");
    play.pc_windows = Default::default();
    open(&mut play, &mut renderer, &mut walker, size, &mut now, 0x0200_0000);
    let (key, restored, _, restored_zoom) = play.pc_windows.geometry().unwrap();
    assert_eq!(key, 1013);
    assert_eq!((restored, restored_zoom), (inventory_saved, inventory_zoom), "Inventory placement and zoom survive reopening after editing Book");
    play.start_key(27, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    play.start_key(144, true);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    open(&mut play, &mut renderer, &mut walker, size, &mut now, 0x0801_0000);
    let (key, restored, _, restored_zoom) = play.pc_windows.geometry().unwrap();
    assert_eq!(key, 5);
    assert_eq!((restored, restored_zoom), (book_saved, zoom), "Spell Book placement and zoom survive reopening");
    assert_eq!(play.pc_windows.scrollbars(), [true; 2], "a deliberately smaller saved Book retains its viewport");
    capture_book(&mut lib, "offline-pc-window-book-small.png");
    press(&mut play, [restored[2] - 15., restored[1] + 9.]);
    frame(&mut play, &mut renderer, &walker, size, &mut now);
    assert_eq!(play.pc_windows.scrollbars(), [false; 2], "Reset returns to auto fit");
    capture_book(&mut lib, "offline-pc-window-book-restored.png");
    crate::settings::set_ui_mode(false).unwrap();
    capture_book(&mut lib, "offline-controller-window-book.png");
    crate::settings::set_ui_mode(true).unwrap();
}
