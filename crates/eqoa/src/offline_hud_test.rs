//! Manual, offline-only visual fixture. Uses the player's disc read-only and a
//! session whose transport never sends or receives a datagram. Run this ignored
//! test alone with EQOA_ISO, EQOA_OFFLINE_HUD_OUT and an isolated EQOA_CACHE.

use super::*;

#[path = "pc_window_test.rs"]
mod pc_window_test;

struct Nowhere;
impl net::session::Datagrams for Nowhere {
    fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> { None }
}

pub(super) fn fixture_play(icon_ids: &[i64]) -> Play {
    let mut session = net::session::Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "offline-fixture", [0; 32]).unwrap();
    session.self_id = Some(0x100);
    session.expansion = 2;
    let mut p = Play::new(session, net::session::Player::default(), Box::default());
    p.dump.player.level = 35;
    p.stats.stat[7] = 750;
    p.stats.stat[8] = 1000;
    p.stats.stat[9] = 460;
    p.stats.stat[10] = 700;
    let actor = |id, name: &str, hp| net::entity::Entity {
        id, kind: net::entity::TYPE_CREATURE, p19: hp,
        creature: Some(net::entity::Creature { name: name.into(), ..Default::default() }),
        ..Default::default()
    };
    let mut me = actor(0x100, "Adventurer", 75);
    me.model = crate::viewer::HUMAN as u32;
    me.drawn_model = crate::viewer::HUMAN as u32;
    me.scale = 1.0;
    p.entities.slots[0] = Some(me);
    p.entities.slots[1] = Some(actor(0x200, "Training Dummy", 85));
    p.target = 0x200;
    // Fixture abilities occupy all real 0..39 slots. The front IDs are actual
    // UI.CSF texture DictIDs; names are deliberately synthetic.
    for slot in 0..40i64 {
        let id = slot + 1;
        p.abilities.insert(id, net::dump::Ability { id, valid: 1, slot, name: format!("Fixture {id}"), icon_front: icon_ids[slot as usize % icon_ids.len()], ..Default::default() });
    }
    for (index, name, wear, attack) in [(0, "Sword", 12, 1), (1, "Bow", 14, 7), (2, "Wand", 18, 1), (3, "Pole", 20, 1)] {
        let id = 100 + index;
        p.dump.inventory.items.push(net::dump::NetItem { equip_slot: -1, quick_slot: -1, item: net::dump::Item { id: id as i64, wear_slot: wear, attack_type: attack, ..Default::default() }, ..Default::default() });
        p.gear.set(index as u8, name.into(), id, u32::MAX);
    }
    crate::play_tables(&mut p).expect("disc UI text tables");
    p.add_chat("A friend tells you, 'Welcome to the offline fixture.'".into(), [1.0, 0.5, 1.0, 1.0]);
    p.add_chat("You hit a training dummy for 24 damage.".into(), [1.0; 4]);
    p.add_chat("Welcome to EverQuest Online Adventures.".into(), [1.0; 4]);
    p.add_chat("Friend: Ready for adventure?".into(), [1.0; 4]);
    p
}

pub(super) fn offline_assets() -> (esf::Library, crate::viewer::Source, Vec<i64>, std::path::PathBuf) {
    let iso = std::env::var("EQOA_ISO").expect("absolute EQOA_ISO is required");
    let output = std::path::PathBuf::from(std::env::var_os("EQOA_OFFLINE_HUD_OUT").expect("EQOA_OFFLINE_HUD_OUT is required"));
    assert!(std::path::Path::new(&iso).is_absolute() && output.is_absolute());
    assert!(output.is_dir());
    assert!(std::env::var_os("EQOA_CACHE").is_some(), "use an isolated cache");
    for module in [crate::modules::Module::Client, crate::modules::Module::Support] {
        crate::modules::load_now(module, std::path::Path::new(&iso)).expect("real disc module");
    }
    let (probe, _) = crate::source("TUNARIA.ESF", &["walk".into(), "84".into()]).expect("offline world");
    let zone = vi::world::World::<vi::fpu::Ee>::zone_at(probe.file(0), 4829.0, 17154.0).expect("Qeynos zone");
    drop(probe);
    let (mut lib, src) = crate::source("TUNARIA.ESF", &["walk".into(), zone.to_string()]).expect("Qeynos world");
    let mut disc = crate::disc(None).unwrap();
    let ui = esf::Container::open(disc.read("UI.CSF").unwrap()).unwrap();
    // The first six 32px surfaces are flat tiles; the following fourteen are
    // legible pictograms in the disc's UI image.
    let icon_ids: Vec<i64> = ui.surfaces.iter().filter(|s| s.width == 32 && s.height == 32).skip(6).take(14).map(|s| s.dict_id as u32 as i64).collect();
    assert!(!icon_ids.is_empty(), "disc has 32px UI icons");
    lib.add("UI.CSF", ui);
    for file in ["ITEM.ESF", "ARIAL.CSF", "ARIALUI.CSF", "ITEMICON.ESF", "SPELLFX.CSF", "FX.CSF", "ADDART.CSF"] {
        lib.add(file, esf::Container::open(disc.read(file).unwrap()).unwrap());
    }
    (lib, src, icon_ids, output)
}

#[test]
#[ignore = "manual offline GPU fixture: requires local disc and output directory"]
fn offline_pc_and_controller_hud_png() {
    let (mut lib, src, icon_ids, output) = offline_assets();
    let script = crate::viewer::Script { ms: 1000, controls: Default::default() };
    crate::settings::testing::reset(false);
    let narrow = std::env::var_os("EQOA_HUD_NARROW").is_some();
    for pc in [false, true] {
        if narrow && !pc { continue; }
        crate::settings::set_ui_mode(pc).unwrap();
        if pc {
            crate::settings::set_chat_detached(1, true).unwrap();
            crate::settings::keep_chat_pane_size(1, 70, 5);
        }
        let name = if pc { "offline-pc-hud.png" } else { "offline-controller-hud.png" };
        let path = output.join(name);
        let size = if narrow { (640, 480) } else { (960, 540) };
        crate::viewer::shot_with(&mut lib, &src, None, Some(script), size, path.to_str().unwrap(), Some(fixture_play(&icon_ids))).expect("offline HUD capture");
        eprintln!("{} HUD screen {:?}, pieces {:?}", if pc { "PC" } else { "Controller" },
            crate::hudlayout::Beyond::of(size).screen(), crate::hudlayout::drawn_all());
    }
}

/// Uses actual HUD quads and Move HUD controls, but an in-memory account and a
/// transport that cannot contact a server. Run alone (`--test-threads=1`).
#[test]
#[ignore = "manual offline GPU interaction test: requires local disc"]
fn offline_chat_move_resize_pin_reopen_and_hit_test() {
    use crate::hudlayout::{self, Beyond, Piece};
    let (mut lib, src, icons, output) = offline_assets();
    let size = (960, 540);
    let beyond = Beyond::of(size);
    let path = output.join("offline-chat-interaction.png");
    let script = crate::viewer::Script { ms: 1000, controls: Default::default() };
    crate::settings::testing::reset(false);
    hudlayout::testing_reset();
    crate::settings::set_ui_mode(true).unwrap();
    crate::settings::set_chat_detached(1, true).unwrap();
    hudlayout::set_unlocked(true);
    let mut pointer = fixture_play(&icons);
    pointer.loading.wait = None;
    let render = |lib: &mut esf::Library| {
        crate::viewer::shot_with(lib, &src, None, Some(script), size, path.to_str().unwrap(), Some(fixture_play(&icons))).unwrap();
    };
    let rect = |piece| hudlayout::drawn_all().into_iter().find(|(p, _)| *p == piece).expect("piece drawn").1;
    let px = |p| render::ui::window_from_frame(size, p);
    let drag = |p: &mut Play, from: [f32; 2], to: [f32; 2]| {
        assert!(p.hud_press(px(from), size), "press {from:?}");
        assert!(p.hud_drag(px(to), size), "drag {to:?}");
        assert!(p.hud_release());
    };
    render(&mut lib);
    let main_before = hudlayout::layout().get(Piece::Chat);
    let pane_before = hudlayout::layout().get(Piece::Chat1);
    let r = rect(Piece::Chat);
    let mid = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
    drag(&mut pointer, mid, [mid[0] + 25.0, mid[1]]);
    assert_ne!(hudlayout::layout().get(Piece::Chat), main_before);
    assert_eq!(hudlayout::layout().get(Piece::Chat1), pane_before);
    render(&mut lib);
    let main_moved = hudlayout::layout().get(Piece::Chat);
    let r = rect(Piece::Chat1);
    let mid = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
    drag(&mut pointer, mid, [mid[0] + 20.0, mid[1] - 15.0]);
    assert_ne!(hudlayout::layout().get(Piece::Chat1), pane_before);
    assert_eq!(hudlayout::layout().get(Piece::Chat), main_moved);
    render(&mut lib);
    let main_size = crate::settings::chat_pane_size(0);
    let pane_size = crate::settings::chat_pane_size(1);
    let r = rect(Piece::Chat);
    let grip = [r[0] + 4.0, r[1] + 5.0];
    drag(&mut pointer, grip, [grip[0] - 40.0, grip[1]]);
    let resized_main = crate::settings::chat_pane_size(0);
    assert!(resized_main.0 > main_size.0 && resized_main.1 == main_size.1, "width only");
    assert_eq!(crate::settings::chat_pane_size(1), pane_size);
    render(&mut lib);
    let r = rect(Piece::Chat1);
    let grip = [r[0] + 4.0, r[1] + 5.0];
    drag(&mut pointer, grip, [grip[0], grip[1] - 20.0]);
    let resized_pane = crate::settings::chat_pane_size(1);
    assert!(resized_pane.0 == pane_size.0 && resized_pane.1 > pane_size.1, "height only");
    assert_eq!(crate::settings::chat_pane_size(0), resized_main);
    render(&mut lib);
    let r = rect(Piece::Chat1);
    let pin = [r[2] - 5.0, r[1] + 5.0];
    assert!(pointer.hud_press(px(pin), size));
    assert!(pointer.hud_release());
    assert!(hudlayout::layout().to_text().contains("pin chat1\n"));
    let pane_pinned = hudlayout::layout().get(Piece::Chat1);
    let mid = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
    assert!(pointer.hud_press(px(mid), size));
    assert!(!pointer.hud_drag(px([mid[0] - 20.0, mid[1]]), size));
    assert!(pointer.hud_release());
    assert_eq!(hudlayout::layout().get(Piece::Chat1), pane_pinned);
    // Force the HUD to reload persisted placement, and switch away and back to
    // exercise the saved interface profile as when reopening the client.
    hudlayout::testing_reset();
    assert_eq!(hudlayout::layout().get(Piece::Chat1), pane_pinned);
    assert!(hudlayout::layout().to_text().contains("pin chat1\n"));
    crate::settings::set_ui_mode(false).unwrap();
    crate::settings::set_ui_mode(true).unwrap();
    assert_eq!(crate::settings::chat_pane_size(0), resized_main);
    assert_eq!(crate::settings::chat_pane_size(1), resized_pane);
    assert!(crate::settings::client().chat_detached[0]);
    assert_eq!(hudlayout::layout().get(Piece::Chat1), pane_pinned);
    hudlayout::set_unlocked(true);
    render(&mut lib);
    let main_rect = rect(Piece::Chat);
    let pane_rect = rect(Piece::Chat1);
    assert!(main_rect[2] <= pane_rect[0] || pane_rect[2] <= main_rect[0] || main_rect[3] <= pane_rect[1] || pane_rect[3] <= main_rect[1], "the independently moved panes remain separate: {main_rect:?}, {pane_rect:?}");
    let r = rect(Piece::Chat1);
    assert!(pointer.hud_press(px([r[2] - 5.0, r[1] + 5.0]), size));
    assert!(pointer.hud_release());
    assert!(!hudlayout::layout().to_text().contains("pin chat1\n"));
    hudlayout::set_unlocked(false);
    // Exercise the ordinary pointer and inline-input paths using each pane's
    // saved move and scale. The hit coordinates come from the chat windows
    // themselves; the screen conversion is the same one used by the viewer.
    let vui = crate::pointer::testing::vui();
    let mut raster = crate::pointer::testing::NullRaster;
    let measure = |s: &[u16]| s.len() as i32 * 8;
    let opts = crate::settings::client();
    let screen_width = render::ui::SCREEN_W + 2.0 * beyond.side;
    let mut chat = crate::chat::WndChat::new();
    chat.always = true;
    chat.set_tabs(opts.chat_tabs, opts.chat_tab_names);
    chat.resize(crate::chat::ChatSize::from_dimensions(&opts, screen_width, resized_main), measure);
    for i in 0..15 {
        chat.add_line(&format!("Someone: main line {i}"), [1.0; 4], 0, measure, &[]);
        chat.add_line(&format!("Someone tells you, 'line {i}'"), [1.0; 4], 0, measure, &[]);
    }
    assert!(chat.set_detached(1, true));
    chat.draw(&vui, &mut raster, 0, 0.1);
    let main_input = chat.input_rect(&vui, &raster);
    let main_hit = [(main_input[0] + 8) as f32, (main_input[1] - 10) as f32];
    assert!(chat.contains(main_hit));
    let pane = chat.pane_mut(1).unwrap();
    pane.resize(crate::chat::ChatSize::from_dimensions(&opts, screen_width, resized_pane), measure);
    pane.draw(&vui, &mut raster, 0, 0.1);
    let pane_input = pane.input_rect(&vui, &raster);
    let pane_hit = [(pane_input[0] + 8) as f32, (pane_input[1] - 10) as f32];
    assert!(pane.contains(pane_hit));
    pointer.wnd_chat = Some(chat);
    for (tab, local) in [(0, main_hit), (1, pane_hit)] {
        let frame = hudlayout::frame_point(Play::chat_piece(tab), local, beyond);
        let window = px(frame);
        let at = (window[0] as f64, window[1] as f64);
        let back = Play::chat_pane_point(at, size, tab);
        assert!((back[0] - local[0]).abs() < 0.01 && (back[1] - local[1]).abs() < 0.01);
        assert!(pointer.chat_wheel(at, size, 1.0), "pane {tab} takes wheel after its transform");
        assert!(pointer.chat_press(at, size, true), "pane {tab} takes pointer after its transform");
        assert!(pointer.chat_dragging(), "pane {tab} hit a drawn row");
        let _ = pointer.chat_release();
    }
    pointer.keyboard_chat_key("");
    assert!(pointer.inline_editing(), "PC keyboard uses inline chat after moves");
    let frame = hudlayout::frame_point(Piece::Chat, [(main_input[0] + 4) as f32, (main_input[1] + 4) as f32], beyond);
    let window = px(frame);
    let local = Play::chat_point((window[0] as f64, window[1] as f64), size);
    assert!((local[0] - (main_input[0] + 4) as f32).abs() < 0.01 && (local[1] - (main_input[1] + 4) as f32).abs() < 0.01, "inline row follows the moved Main pane");
    // Dock through the pane's actual control, then detach through Main's
    // actual control. A first detach freezes its inherited dimensions.
    pointer.mode = 0;
    let control = |chat: &crate::chat::WndChat| {
        (0..448).flat_map(|y| (0..640).map(move |x| [x as f32, y as f32]))
            .find(|&p| chat.control_at([p[0] + 2.0, p[1] + 2.0])).map(|p| [p[0] + 2.0, p[1] + 2.0]).expect("drawn chat control")
    };
    let chat = pointer.wnd_chat.as_mut().unwrap();
    let pane = chat.pane_mut(1).unwrap();
    pane.draw_pane_control(&vui, &mut raster, true);
    let at = px(hudlayout::frame_point(Piece::Chat1, control(pane), beyond));
    assert!(pointer.chat_press((at[0] as f64, at[1] as f64), size, true));
    assert!(pointer.wnd_chat.as_ref().unwrap().pane(1).is_none());
    assert!(!crate::settings::client().chat_detached[0]);
    let mut opts = crate::settings::client();
    opts.chat_pane_sizes[0] = None;
    crate::settings::set_client(opts);
    let chat = pointer.wnd_chat.as_mut().unwrap();
    assert!(chat.show_tab(1));
    chat.draw(&vui, &mut raster, 0, 0.1);
    chat.draw_pane_control(&vui, &mut raster, true);
    let at = px(hudlayout::frame_point(Piece::Chat, control(chat), beyond));
    assert!(pointer.chat_press((at[0] as f64, at[1] as f64), size, true));
    assert!(pointer.wnd_chat.as_ref().unwrap().pane(1).is_some());
    assert!(crate::settings::client().chat_detached[0]);
    assert_eq!(crate::settings::client().chat_pane_sizes[0], Some([resized_main.0, resized_main.1]));
    eprintln!("independent chat moves, width/height grips, pin and profile reload verified; main {resized_main:?}, pane {resized_pane:?}; screen {:?}", beyond.screen());
}
