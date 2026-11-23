//! Manual, offline-only visual fixture. Uses the player's disc read-only and a
//! session whose transport never sends or receives a datagram. Run this ignored
//! test alone with EQOA_ISO, EQOA_OFFLINE_HUD_OUT and an isolated EQOA_CACHE.

use super::*;

struct Nowhere;
impl net::session::Datagrams for Nowhere {
    fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> { None }
}

fn fixture_play(icon_ids: &[i64]) -> Play {
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

#[test]
#[ignore = "manual offline GPU fixture: requires local disc and output directory"]
fn offline_pc_and_controller_hud_png() {
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
        if narrow {
            eprintln!("offline HUD screen {:?}, pieces {:?}", crate::hudlayout::Beyond::of(size).screen(), crate::hudlayout::drawn_all());
        }
    }
}
