//! Offline real-disc PNGs of the item menu (Heirloom petition 3118): the
//! Retail List with Compare Gear, and the Bags grid in the PC and the
//! Controller interfaces. Never connects to a server; ignored without the
//! player's disc. Run alone with EQOA_ISO, EQOA_OFFLINE_HUD_OUT and an
//! isolated EQOA_CACHE.
use super::*;

fn frame(p: &mut Play, r: &mut render::Renderer, w: &crate::walker::Walker, size: (u32, u32), now: &mut i64) {
    *now += 100;
    p.hud(r.ui.as_mut().expect("disc UI"), w, *now, glam::Mat4::IDENTITY, size, true);
}

fn tap(p: &mut Play, r: &mut render::Renderer, w: &crate::walker::Walker, size: (u32, u32), now: &mut i64, key: i32) {
    p.start_key(key, true);
    frame(p, r, w, size, now);
    p.start_key(key, false);
    frame(p, r, w, size, now);
}

/// A pack of real icons (ITEMICON.ESF's wide ones) and mixed gear: worn
/// pieces, better and worse candidates, stacks, food.
fn stock(p: &mut Play, icons: &[i32]) {
    use net::dump::{Item, ItemStat, NetItem};
    let mut icon = icons.iter().copied().cycle();
    p.dump.inventory.items.clear();
    let mut add = |name: &str, wear: i64, equip: i64, damage: i64, qty: i64, p4c: i64, stats: &[(i64, i64)]| {
        let k = p.dump.inventory.items.len();
        p.dump.inventory.items.push(NetItem {
            quantity: qty,
            equip_slot: equip,
            quick_slot: -1,
            inv_index: k as u32 + 1,
            cur_hp: 100,
            charges: 0,
            item: Item {
                id: 5000 + k as i64,
                name: name.into(),
                wear_slot: wear,
                damage,
                attack_type: if damage > 0 { 1 } else { 0 },
                class_mask: -1,
                race_mask: -1,
                max_hp: if wear != -1 { 100 } else { 0 },
                durability: if wear != -1 { 100 } else { 0 },
                max_stack: if qty > 1 { 20 } else { 1 },
                p4c,
                icon: icon.next().unwrap() as i64,
                stats: stats.iter().map(|&(id, value)| ItemStat { id, value }).collect(),
                ..Default::default()
            },
        });
    };
    // worn: helm, chest, primary, shield
    add("Bronze Cap", 1, 0, 0, 1, 0, &[(0, 2), (12, 8)]);
    add("Ringmail Tunic", 4, 4, 0, 1, 0, &[(2, 3), (12, 15)]);
    add("Rusty Longsword", 12, 14, 6, 1, 0, &[(0, 1)]);
    add("Round Shield", 13, 17, 0, 1, 0, &[(12, 10), (2, 2)]);
    // candidates
    add("Fine Steel Helm", 1, -1, 0, 1, 0, &[(0, 5), (2, 4), (12, 6)]);
    add("Gnollhide Tunic", 4, -1, 0, 1, 0, &[(1, 4), (12, 9)]);
    add("Greatsword of Ages", 15, -1, 14, 1, 0, &[(0, 6), (12, 12)]);
    add("Jade Ring", 6, -1, 0, 1, 0, &[(3, 3), (4, 3)]);
    add("Ration", -1, -1, 0, 12, 2, &[]);
    add("Water Flask", -1, -1, 0, 8, 2, &[]);
    add("Bone Chips", -1, -1, 0, 17, 0, &[]);
    add("Spider Silk", -1, -1, 0, 5, 0, &[]);
    add("Minor Healing Potion", -1, -1, 0, 3, 7, &[]);
    add("Wolf Pelt", -1, -1, 0, 1, 0, &[]);
    add("Copper Bracelet", 8, -1, 0, 1, 0, &[(5, 2)]);
    add("Leather Boots", 11, -1, 0, 1, 0, &[(12, 4)]);
}

fn icons() -> Vec<i32> {
    let mut disc = crate::disc(None).unwrap();
    let c = esf::Container::open(disc.read("ITEMICON.ESF").unwrap()).unwrap();
    let wide: Vec<i32> = c.surfaces.iter().filter(|s| s.width >= s.height * 2).map(|s| s.dict_id).collect();
    eprintln!("ITEMICON.ESF: {} surfaces, {} wide; first sizes {:?}", c.surfaces.len(), wide.len(), c.surfaces.iter().take(6).map(|s| (s.width, s.height)).collect::<Vec<_>>());
    wide.into_iter().step_by(7).take(16).collect()
}

#[test]
#[ignore = "manual offline GPU fixture: requires local disc and output directory"]
fn offline_inventory_bags_png() {
    let (mut lib, source, output) = offline_assets();
    let icons = icons();
    let size = (960, 540);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&render::packed::device_descriptor(&adapter))).unwrap();
    let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
    if let Some(f) = crate::viewer::arial(&lib) {
        renderer.set_font(&device, &queue, &f);
    }
    let scene = source.scene(&lib);
    let (mut walker, model, mut stream) = source.walker(&mut lib, &scene).expect("offline world walker");
    let at = glam::Vec3::from(walker.cam.walk.phys.position.to_array());
    stream.update(at, &mut walker.scene, &lib, crate::stream::Gpu { renderer: &mut renderer, device: &device, queue: &queue }, None);
    let (matrix, palette) = walker.pose();
    let mut body = renderer.upload(&device, &queue, &model, &[matrix], &render::library_textures(&lib));
    renderer.set_palette(&queue, &body, &palette);
    crate::play::place_body(&mut body, &walker, matrix);
    let mut view = source.camera(scene.bounds).view(scene.bounds, 16. / 9.);
    let shots = [
        ("offline-inventory-list-compare.png", true, false, 4),
        ("offline-inventory-bags-pc.png", true, true, 4),
        ("offline-inventory-bags-controller.png", false, true, 6),
    ];
    for (name, pc, bags, select) in shots {
        crate::settings::testing::reset(false);
        crate::settings::set_ui_mode(pc).unwrap();
        let mut o = crate::settings::client();
        o.inventory_bags = bags;
        o.compare_gear = crate::settings::CompareGear::Always;
        crate::settings::set_client(o);
        let mut p = fixture_play();
        p.dump.player.class = 0;
        p.dump.player.level = 20;
        stock(&mut p, &icons);
        p.loading.wait = None;
        p.take_modules(None);
        p.attach_ui(&mut renderer, &device, &queue, render::TARGET_FORMAT, &lib);
        let mut now = 1000i64;
        frame(&mut p, &mut renderer, &walker, size, &mut now);
        p.open_start_menu(true);
        frame(&mut p, &mut renderer, &walker, size, &mut now);
        let start = p.start.as_ref().expect("CLIENT start menu");
        let n = start.menus.list[start.menu.main.menu].entries.iter().position(|e| e.msg == 0x0200_0000).expect("Inventory entry");
        for _ in 0..n {
            tap(&mut p, &mut renderer, &walker, size, &mut now, 20);
        }
        tap(&mut p, &mut renderer, &walker, size, &mut now, 13);
        assert!(p.items.visible, "Inventory opened");
        if bags {
            // a candidate selected: the d-pad over the grid (Pack Order)
            for _ in 0..select {
                tap(&mut p, &mut renderer, &walker, size, &mut now, 18);
            }
        } else {
            // the Equipment tab, its fifth row (the steel helm)
            tap(&mut p, &mut renderer, &walker, size, &mut now, 18);
            for _ in 0..select {
                tap(&mut p, &mut renderer, &walker, size, &mut now, 20);
            }
        }
        for _ in 0..12 {
            frame(&mut p, &mut renderer, &walker, size, &mut now);
        }
        let (projection, v) = walker.view(size.0 as f32 / size.1 as f32);
        view.view_proj = projection * v;
        view.eye = walker.eye_position();
        p.hud(renderer.ui.as_mut().unwrap(), &walker, now, view.view_proj, size, true);
        let items: Vec<_> = stream.draws().chain(std::iter::once(render::Draw::Model(&body))).collect();
        let px = render::capture_around(&device, &queue, &mut renderer, size.0, size.1, &view, &items, None, None, |_, _, _, _| {});
        let path = output.join(name);
        crate::viewer::write_png(path.to_str().unwrap(), size.0, size.1, &px).expect("png");
        eprintln!("{name}: pc={pc} bags={bags}");
    }
}
