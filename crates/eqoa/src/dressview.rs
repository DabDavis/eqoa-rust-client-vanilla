//! `eqoa dress <body> <out.png> [look...]`: one body off the disc dressed in
//! a look as the game dresses a creature (`play::dress_look`: the text slots,
//! and `slotless` on a body with none), drawn standing (its idle, as
//! `SetLocomotion(0)` leaves it) from the front, the front a little to its
//! left, its left and behind, side by side, under the GM preview's daylight
//! (the sun over the camera's shoulder), with no window, framed on the body
//! (not its wings or tail). For judging armour on bodies by eye.
//!
//! The look, as words: `set=N` (the five worn slots in armour set N),
//! `armor=a,b,c,d,e` (chest, bracer, gloves, legs, boots), `tint=T` (every
//! slot's tint), `robe=R`, `rtint=T`, `dye=RRGGBB` (the live game's colour on
//! every slot and the robe), `hair=c,l,s`, `face=F`, `at=ms` (the idle's
//! time), `size=WxH` (one view's). None of them: the body bare. Held items:
//! `main=ID`, `off=ID`, `shield=ID` (ITEM.ESF DictIDs in hex, the three item
//! slots), each hung as `AttachItem` hangs it, on the node the body names for
//! its slot when the body holds items; `race=1` draws the body as a player's
//! in a body one of a server's races wears (`racebody`: it holds them
//! whatever its header says). `action=N` plays the sprite's action N from the
//! start (23: the first hand's attack) instead of standing. `tilt=DEG` looks
//! from that far above (negative: below) the body's middle instead of the
//! usual 14 degrees: into a hollow armour at the collar, say. `cape=N` puts
//! cape style N on it (`cape`, as a player's character wears one on a server
//! with capes), `capecolor=RRGGBB` in that colour.
//! `opts=R:a,b,c` dresses it in the steps a, b, c of race R's option rows
//! (`raceopts`), out of the list go-eqoa-server sends (`$EQOA_RACE_OPTIONS`, a
//! file of 0x7E23's body): the body row's step is the body drawn, the others
//! its parts' colours and textures; `sex=1` a female's steps.
//! `helm=N` puts on armour set N's helm (`SetHelm`, on the node and seated
//! as a player's character wears it: `slotless::helm_on`), in the helm's
//! tint (`tint=T`); `helmfit=0` seats it as the PS2 does, without this
//! client's fit to a skeleton's skull (`slotless::helm_fit`), to judge the fit
//! by. `era=2002` draws the body, its helm and what it holds in the 2002
//! disc's art (`$EQOA_2002_ISO`), as the world draws them with F9.
//! `frame=head` frames the views on the head (the node the body names for
//! it and the nodes under it) instead of the body.
//! `helmseat=A,sx,sy,sz,ox,oy,oz` (or `A,s,ox,oy,oz`) seats the helm on node
//! `A` (`named`: the body's node of the human's helm node's name; `head`: its
//! head's; or a node's number) stretched and moved as given in that node's
//! frame, in place of the seat the body has (`slotless::HEAD_SEATS`): to
//! judge a seat by before writing it down.

use esf::Library;
use glam::{Mat4, Vec3};
use render::{Renderer, View, wgpu};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::Rand;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;

/// The files a body and what it holds need, in the order the game registers
/// them.
pub const FILES: &[&str] = &["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF", "ITEM.ESF"];

/// What a list of words names.
#[derive(Debug)]
pub struct Words {
    pub look: vi::cust::Look,
    /// The items in the item slots (main hand, off hand, shield): DictIDs, 0
    /// none.
    pub items: [u32; 3],
    /// The body as a player's, one of a server's races wears (`racebody`).
    pub race: bool,
    /// The action played from the start, else standing.
    pub action: Option<i32>,
    /// The idle's (or the action's) time, ms.
    pub at: i64,
    /// One view's size.
    pub size: (u32, u32),
    /// The views' height above the middle, in degrees (None: the usual).
    pub tilt: Option<f32>,
    /// The cape worn (`cape`), style 0 none.
    pub cape: net::cape::Cape,
    /// A race's option rows' steps (`opts=R:a,b,c`), and the sex they are a
    /// sex's of.
    pub opts: Option<(i32, Vec<usize>)>,
    pub sex: i32,
    /// The helm seated with this client's fit (`slotless::helm_fit`).
    pub helm_fit: bool,
    /// The 2002 disc's art.
    pub y2002: bool,
    /// Framed on the head alone.
    pub head: bool,
    /// A helm seat tried in place of the body's (`helmseat=`): the node (by
    /// `slotless::Anchor`, or a node's number) and the fit.
    pub seat: Option<(SeatNode, crate::slotless::HelmFit)>,
}

/// The node a tried helm seat hangs on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeatNode {
    Anchor(crate::slotless::Anchor),
    Node(i32),
}

/// What a list of words names: the look, the items held, the idle's time and
/// one view's size.
pub fn parse(words: &[String]) -> Result<Words, String> {
    let mut look = vi::cust::Look { robe: -1, ..Default::default() };
    let (mut at, mut size) = (1500i64, (360u32, 540u32));
    let (mut items, mut race, mut action, mut tilt) = ([0u32; 3], false, None, None);
    let mut cape = net::cape::Cape::default();
    let (mut opts, mut sex) = (None, 0);
    let (mut helm_fit, mut y2002, mut head) = (true, false, false);
    let mut seat = None;
    let nums = |v: &str| v.split(',').map(|n| n.trim().parse::<u8>().map_err(|e| format!("{v}: {e}"))).collect::<Result<Vec<u8>, String>>();
    let hex = |w: &str, v: &str| u32::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16).map_err(|e| format!("{w}: {e}"));
    for w in words {
        let (k, v) = w.split_once('=').ok_or_else(|| format!("{w}: a look is words like set=3, tint=8, robe=1"))?;
        match k {
            "main" => items[0] = hex(w, v)?,
            "off" => items[1] = hex(w, v)?,
            "shield" => items[2] = hex(w, v)?,
            "race" => race = v != "0",
            "cape" => cape.style = v.parse().map_err(|e| format!("{w}: {e}"))?,
            "capecolor" => cape.color = (u32::from_str_radix(v.trim_start_matches('#'), 16).map_err(|e| format!("{w}: {e}"))? << 8) | 0xFF,
            "action" => action = Some(v.parse().map_err(|e| format!("{w}: {e}"))?),
            "set" => {
                let n = nums(v)?[0];
                look.armor[..5].fill(n);
            }
            "armor" => {
                for (i, n) in nums(v)?.into_iter().take(6).enumerate() {
                    look.armor[i] = n;
                }
            }
            "tint" => look.armor_tint.fill(nums(v)?[0]),
            "robe" => look.robe = v.parse::<i8>().map_err(|e| format!("{w}: {e}"))?,
            "rtint" => look.robe_tint = nums(v)?[0],
            "dye" => {
                let c = u32::from_str_radix(v.trim_start_matches('#'), 16).map_err(|e| format!("{w}: {e}"))?;
                let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
                look.armor_color = Some([rgba; 6]);
                look.robe_color = Some(rgba);
            }
            "hair" => {
                for (i, n) in nums(v)?.into_iter().take(3).enumerate() {
                    look.hair[i] = n;
                }
            }
            "face" => look.face = nums(v)?[0],
            "opts" => {
                let (r, steps) = v.split_once(':').ok_or_else(|| format!("{w}: opts=race:step,step"))?;
                let race = r.parse().map_err(|e| format!("{w}: {e}"))?;
                let steps = steps.split(',').filter(|s| !s.is_empty()).map(|s| s.trim().parse::<usize>().map_err(|e| format!("{w}: {e}"))).collect::<Result<Vec<_>, _>>()?;
                opts = Some((race, steps));
            }
            "sex" => sex = v.parse().map_err(|e| format!("{w}: {e}"))?,
            "helm" => look.armor[5] = nums(v)?[0],
            "helmfit" => helm_fit = v != "0",
            "helmseat" => {
                let parts: Vec<&str> = v.split(',').collect();
                let node = match parts[0] {
                    "named" => SeatNode::Anchor(crate::slotless::Anchor::Named),
                    "head" => SeatNode::Anchor(crate::slotless::Anchor::Head),
                    n => SeatNode::Node(n.parse().map_err(|e| format!("{w}: {e}"))?),
                };
                let f: Vec<f32> = parts[1..].iter().map(|n| n.trim().parse::<f32>().map_err(|e| format!("{w}: {e}"))).collect::<Result<_, _>>()?;
                let (scale, offset) = match f.len() {
                    4 => (glam::Vec3::splat(f[0]), glam::Vec3::new(f[1], f[2], f[3])),
                    6 => (glam::Vec3::new(f[0], f[1], f[2]), glam::Vec3::new(f[3], f[4], f[5])),
                    _ => return Err(format!("{w}: helmseat=A,s,ox,oy,oz or A,sx,sy,sz,ox,oy,oz")),
                };
                seat = Some((node, crate::slotless::HelmFit { scale, offset }));
            }
            "frame" => match v {
                "head" => head = true,
                "body" => head = false,
                _ => return Err(format!("{w}: frame=head or frame=body")),
            },
            "era" => match v {
                "2002" => y2002 = true,
                "frontiers" => y2002 = false,
                _ => return Err(format!("{w}: era=2002 or era=frontiers")),
            },
            "at" => at = v.parse().map_err(|e| format!("{w}: {e}"))?,
            "tilt" => tilt = Some(v.parse::<f32>().map_err(|e| format!("{w}: {e}"))?),
            "size" => {
                let (a, b) = v.split_once('x').ok_or_else(|| format!("{w}: WxH"))?;
                size = (a.parse().map_err(|e| format!("{w}: {e}"))?, b.parse().map_err(|e| format!("{w}: {e}"))?);
            }
            _ => return Err(format!("{w}: not a word of a look")),
        }
    }
    Ok(Words { look, items, race, action, at, size, tilt, cape, opts, sex, helm_fit, y2002, head, seat })
}

/// The body drawn in the look, holding what `words` say, from four sides
/// into `out`.
#[cfg(not(target_arch = "wasm32"))]
pub fn shot(lib: &Library, model: i32, words: &Words, out: &str) -> Result<(), String> {
    let (look, at, (w, h)) = (&words.look, words.at, words.size);
    // a race's options: the body its body row chose, the look its other rows
    // make; out of `$EQOA_RACE_OPTIONS`, else the thread's (a test's)
    let opts = match &words.opts {
        Some((race, steps)) => {
            if let Ok(path) = std::env::var("EQOA_RACE_OPTIONS") {
                let data = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
                let o = net::raceopts::read_options(&data).ok_or_else(|| format!("{path}: not the race options"))?;
                crate::raceopts::set_options(Some(&o));
            }
            let o = crate::raceopts::options().ok_or("opts= needs EQOA_RACE_OPTIONS (a file of 0x7E23's body)")?;
            Some((o.rows(*race).to_vec(), steps.clone()))
        }
        None => None,
    };
    let model = opts.as_ref().and_then(|(rows, steps)| crate::raceopts::body_of_steps(rows, words.sex, steps)).map_or(model, |b| b as i32);
    // built whole in the art the body is drawn in (`esf::Library::view_for`)
    if words.y2002 {
        lib.set_view(esf::Era::Y2002);
        lib.set_view(lib.view_for(model));
    }
    let r = lib.lookup(model).ok_or_else(|| format!("no body {:#010x} in CHAR.ESF", model as u32))?;
    let mut anims = AnimScene::<Ee>::default();
    let mut rand = Rand(1);
    let mut cs = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand).ok_or("not a character")?;
    if words.race {
        let races = net::creation::Creation {
            all_classes: false,
            races: vec![net::creation::Race { id: 11, like: 0, city: 0, bonus: [0; 7], class_mask: 0, body: [model as u32, 0], name: String::new(), short: String::new(), desc: String::new() }],
        };
        crate::racebody::grant(&mut cs, model as u32, true, Some(&races));
    }
    // `AttachItem`'s test: a node in the slot, on a sprite that holds items
    let held: Vec<(i32, u32)> = (0..3).filter_map(|i| cs.item_node(i).filter(|_| words.items[i] != 0).map(|n| (n, words.items[i]))).collect();
    let mut mesh = lib.build_model(r).ok_or("no mesh")?;
    let before = mesh.batches.len();
    // the helm, on its node and seated as a player's character wears it
    let t = vi::cust::Tables::retail();
    let mut helm = crate::slotless::helm_on(lib, model as u32, &cs, true, look.armor[5], look.helm_color(&t), &t, words.helm_fit);
    // a seat tried in place of the body's (`helmseat=`)
    if let Some((at, fit)) = words.seat {
        let node = match at {
            SeatNode::Anchor(crate::slotless::Anchor::Named) => crate::slotless::named_helm_node(lib, model as u32, cs.sex != 0).unwrap_or(-1),
            SeatNode::Anchor(crate::slotless::Anchor::Head) => cs.node_ids[7],
            SeatNode::Node(n) => n,
        };
        helm = (node >= 0)
            .then(|| vi::cust::helm_with(look.armor[5], &t))
            .flatten()
            .and_then(|h| lib.lookup(h as i32))
            .and_then(|r| lib.build_model(r))
            .map(|mut m| {
                let tint = look.helm_color(&t);
                m.batches.iter_mut().flat_map(|b| b.layers.iter_mut()).for_each(|l| l.color = tint);
                fit.apply(&mut m);
                (node, m)
            });
    }
    if look.armor[5] != 0 {
        match &helm {
            Some((node, _)) => eprintln!(
                "helm {} on node {node}{}",
                look.armor[5],
                if words.seat.is_some() {
                    " (seat tried)"
                } else if !words.helm_fit {
                    " (no fit)"
                } else if crate::slotless::helm_fit(lib, model as u32, true).or_else(|| crate::slotless::head_fit(model as u32, &cs, true)).is_some() {
                    " (fitted)"
                } else {
                    ""
                }
            ),
            None => eprintln!("helm {}: none on this body", look.armor[5]),
        }
    }
    crate::play::dress_look(lib, model, &mut mesh, &cs, look, helm.is_some(), &t, true);
    // a Wisp's (`wisp`): its armour alone, the wisp hung on the helm's node as an item is
    let mut wisp = None;
    if let Some((rows, steps)) = &opts {
        let rl = crate::raceopts::look_of_steps(rows, words.sex, steps, model as u32);
        crate::raceopts::dress(lib, model as u32, &mut mesh, &rl);
        crate::wisp::dress(lib, model as u32, &mut mesh, &cs, look, Some(&rl), &t);
        wisp = crate::wisp::attachment(lib, model as u32, &cs, Some(&rl));
    }
    let tris = |b: &[esf::Batch]| b.iter().map(|b| b.indices.len() / 3).sum::<usize>();
    // `EQOA_DRESS_DEBUG=slots`: the made-up batches flat, a colour a slot
    // (chest red, bracer yellow, gloves green, legs blue, boots magenta);
    // `parts`: the body's own triangles a colour by where they are fitted
    // (body red, arms yellow and orange, legs blue and cyan, head grey, off
    // the spine light grey, wearing nothing black, a part the body keeps its
    // own look on purple); `nodes`: each held item's node as it is drawn (its
    // axes and place) and the item's bounds, printed (an item's length runs
    // along its node's z)
    let debug = std::env::var("EQOA_DRESS_DEBUG").unwrap_or_default();
    let flat = |l: &mut esf::Layer, c: [u8; 3]| {
        l.texture = 0;
        l.color = [c[0], c[1], c[2], 255];
    };
    if debug == "slots" {
        const C: [[u8; 3]; 5] = [[220, 40, 40], [230, 210, 40], [40, 200, 60], [50, 90, 230], [210, 50, 210]];
        let from = before.min(mesh.batches.len());
        for b in mesh.batches[from..].iter_mut().filter(|b| b.material >= crate::slotless::ENTRY) {
            let k = (b.material - crate::slotless::ENTRY) as usize;
            b.layers.iter_mut().for_each(|l| flat(l, C[k]));
        }
        // a body's own slots alike
        for b in mesh.batches.iter_mut().take(before) {
            if let Some(k) = (0..5).find(|&i| cs.text_slots[i].0 == b.material) {
                b.layers.iter_mut().for_each(|l| flat(l, C[k]));
            }
        }
    }
    if debug == "parts"
        && let Some(groups) = crate::slotless::debug_groups(lib, model, &mesh, &cs)
    {
        const C: [[u8; 3]; 9] = [[220, 40, 40], [230, 210, 40], [240, 130, 20], [50, 90, 230], [40, 200, 220], [130, 130, 130], [10, 10, 10], [190, 190, 190], [150, 60, 200]];
        let mut split = Vec::new();
        for (b, g) in mesh.batches.iter().take(before).zip(&groups) {
            for code in 0..9u8 {
                let indices: Vec<u32> = b.indices.chunks_exact(3).zip(g).filter(|(_, c)| **c == code).flat_map(|(t, _)| t.to_vec()).collect();
                if !indices.is_empty() {
                    let mut layer = b.layers.first().cloned().unwrap_or(esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] });
                    flat(&mut layer, C[code as usize]);
                    layer.blend = 0;
                    split.push(esf::Batch { material: b.material, layers: vec![layer], indices, lod: b.lod });
                }
            }
        }
        mesh.batches = split;
    }
    // a Wisp's body is stripped (`wisp`): fewer batches than it was dressed with
    let kept = before.min(mesh.batches.len());
    eprintln!(
        "{:#010x}: {} text slots, {} batches dressed, {} made-up batches ({} triangles of {})",
        model as u32,
        cs.text_slots.iter().filter(|s| s.0 != -1).count(),
        before,
        mesh.batches.len() - kept,
        tris(&mesh.batches[kept..]),
        tris(&mesh.batches[..kept])
    );
    // the cape, fitted to the body as it is built (`cape::model`)
    let cape = (words.cape.style != 0).then(|| crate::cape::model(lib, words.cape, model as u32, &cs)).flatten();
    if words.cape.style != 0 {
        match &cape {
            Some(m) => eprintln!("cape {} ({}): {} triangles", words.cape.style, crate::cape::style(words.cape.style).map_or("", |s| s.what), tris(&m.batches)),
            None => eprintln!("cape {}: none on this body", words.cape.style),
        }
    }
    if words.items.iter().any(|&i| i != 0) {
        eprintln!("holds items: {} (item nodes {:?}); hung: {:?}", cs.holds_items, &cs.item_nodes[..3], held.iter().map(|(n, i)| format!("{i:#010x} on node {n}")).collect::<Vec<_>>());
    }
    // standing, at `at` ms of its idle (or of the action asked for)
    let mut t = 0;
    while t <= at {
        anims.time = t;
        match words.action {
            Some(a) if t == 0 => {
                cs.set_action(a, &anims, &mut rand);
            }
            Some(_) => {}
            None => {
                cs.set_locomotion(0, &anims, &mut rand);
            }
        }
        cs.h.process(0, &anims);
        t += 16;
    }
    let centre = cs.h.calc_trans_about_center(&Matrix44::identity(), &anims);
    cs.h.set_blend_matrices();
    // framed on the posed body's bounds: its torso, limbs and head where it
    // has them (not wings or a tail), else everything
    let palette: Vec<[[f32; 4]; 4]> = cs.h.blend.iter().map(|m| m.to_cols()).collect();
    let world = Mat4::from_cols_array_2d(&centre.to_cols());
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut framed = vec![true; mesh.pos.len()];
    if let Some(groups) = crate::slotless::debug_groups(lib, model, &mesh, &cs) {
        framed.fill(false);
        for (b, g) in mesh.batches.iter().zip(&groups) {
            for (t, &code) in b.indices.chunks_exact(3).zip(g) {
                if code <= 5 || code == 8 {
                    t.iter().for_each(|&v| framed[v as usize] = true);
                }
            }
        }
        if !framed.contains(&true) {
            framed.fill(true);
        }
    }
    // `frame=head`: on the head alone (the vertices on the node the body
    // names for its head, 7, and the nodes under it), to judge a helm by
    let head = cs.node_ids[7];
    if words.head && head >= 0 {
        let under = |mut n: i32| {
            while n >= 0 && (n as usize) < cs.h.nodes.len() {
                if n == head {
                    return true;
                }
                n = cs.h.nodes[n as usize].parent;
            }
            false
        };
        for (i, f) in framed.iter_mut().enumerate() {
            let (b, wt) = (mesh.bones[i], mesh.weights[i]);
            let k = (0..4).max_by_key(|&k| wt[k]).unwrap_or(0);
            *f = b[k] != 255 && wt[k] > 0 && under(b[k] as i32);
        }
        if !framed.contains(&true) {
            framed.fill(true);
        }
    }
    for (i, p) in mesh.pos.iter().enumerate().filter(|(i, _)| framed[*i]) {
        let (b, wt) = (mesh.bones[i], mesh.weights[i]);
        let mut q = Vec3::ZERO;
        let mut sum = 0.0;
        for k in 0..4 {
            if b[k] != 255 && (b[k] as usize) < palette.len() && wt[k] > 0 {
                q += Mat4::from_cols_array_2d(&palette[b[k] as usize]).transform_point3(Vec3::from(*p)) * wt[k] as f32;
                sum += wt[k] as f32;
            }
        }
        let q = world.transform_point3(if sum > 0.0 { q / sum } else { Vec3::from(*p) });
        lo = lo.min(q);
        hi = hi.max(q);
    }
    // the head framed with the helm on it
    if words.head
        && let Some((node, m)) = &helm
        && let Some(n) = cs.h.nodes.get(*node as usize)
    {
        let at = Mat4::from_cols_array_2d(&Matrix44::mul(&n.world, &centre).to_cols());
        for p in &m.pos {
            let q = at.transform_point3(Vec3::from(*p));
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    let mid = (lo + hi) * 0.5;
    let fov = 30f32.to_radians();
    let reach = ((hi.y - lo.y) * 0.5 / 0.86).max((hi.x - lo.x).max(hi.z - lo.z) * 0.5 / 0.7 / (w as f32 / h as f32)) / (fov * 0.5).tan();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).map_err(|e| e.to_string())?;
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let textures = render::library_textures(lib);
    let mut gpu = r.upload(&device, &queue, &mesh, &[render::IDENTITY], &textures);
    r.set_instances(&queue, &gpu, &[centre.to_cols()]);
    if !palette.is_empty() {
        r.set_palette(&queue, &gpu, &palette);
    }
    // the cape on the body's own palette and place
    let mut cape = cape.map(|m| {
        let g = r.upload(&device, &queue, &m, &[render::IDENTITY], &textures);
        r.set_instances(&queue, &g, &[centre.to_cols()]);
        if !palette.is_empty() {
            r.set_palette(&queue, &g, &palette);
        }
        g
    });
    // each item (and the helm) on its node's animated place under the centre
    // transform, as the game draws an attached sprite
    let mut items = Vec::new();
    let hung = held.iter().map(|&(node, id)| (node, id, lib.lookup(id as i32).and_then(|r| lib.build_model(r)))).chain(helm.map(|(node, m)| (node, 0, Some(m))));
    for (node, id, m) in hung {
        let (Some(n), Some(m)) = (cs.h.nodes.get(node as usize), m) else {
            eprintln!("{id:#010x}: no such item");
            continue;
        };
        let at = Matrix44::mul(&n.world, &centre);
        if debug == "nodes" {
            let row = |i: usize| format!("({:6.3} {:6.3} {:6.3})", at.m[i].x.0, at.m[i].y.0, at.m[i].z.0);
            eprintln!("node {node}: x{} y{} z{} at{}", row(0), row(1), row(2), row(3));
            let (lo, hi) = m.pos.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| (std::array::from_fn(|k| lo[k].min(p[k])), std::array::from_fn(|k| hi[k].max(p[k]))));
            eprintln!("  item {id:#010x} bounds {lo:?} .. {hi:?}");
        }
        let g = r.upload(&device, &queue, &m, &[render::IDENTITY], &textures);
        r.set_instances(&queue, &g, &[at.to_cols()]);
        items.push((g, at));
    }
    if let Some((node, m)) = &wisp
        && let Some(n) = cs.h.nodes.get(*node as usize)
    {
        let at = Matrix44::mul(&n.world, &centre);
        let g = r.upload(&device, &queue, m, &[render::IDENTITY], &textures);
        r.set_instances(&queue, &g, &[at.to_cols()]);
        items.push((g, at));
    }
    // from the front, from the front a little to its left, from its left,
    // from behind
    let yaws = [0.0f32, 0.6, std::f32::consts::FRAC_PI_2, std::f32::consts::PI];
    let mut sheet = vec![0u8; (w * yaws.len() as u32 * h * 4) as usize];
    for (k, yaw) in yaws.iter().enumerate() {
        let dir = match words.tilt {
            Some(t) => {
                let (s, c) = t.to_radians().sin_cos();
                Vec3::new(yaw.sin() * c, s, yaw.cos() * c)
            }
            None => Vec3::new(yaw.sin() * 0.97, 0.24, yaw.cos() * 0.97).normalize(),
        };
        let eye = mid + dir * reach;
        let v = glam::camera::rh::view::look_at_mat4(eye, mid, Vec3::Y);
        let p = glam::camera::rh::proj::directx::perspective(fov, w as f32 / h as f32, reach * 0.05, reach * 4.0);
        let mut view = View::daylight(p * v);
        view.eye = eye;
        // the sun from over the camera's shoulder, so each side is seen lit
        view.sun_dir = (Vec3::new(-dir.x, 0.0, -dir.z) * 0.6 + Vec3::new(dir.z, 0.0, -dir.x) * 0.25 + Vec3::new(0.0, -0.75, 0.0)).normalize();
        let at_eye = Vect3::new(F::new(eye.x), F::new(eye.y), F::new(eye.z));
        crate::lod::place(&mut gpu, &centre, &at_eye);
        for (g, at) in &mut items {
            crate::lod::place(g, at, &at_eye);
        }
        if let Some(g) = cape.as_mut() {
            crate::lod::place(g, &centre, &at_eye);
        }
        let draws: Vec<render::Draw> = std::iter::once(render::Draw::Model(&gpu)).chain(items.iter().map(|(g, _)| render::Draw::Model(g))).chain(cape.iter().map(render::Draw::Model)).collect();
        let rgba = render::capture(&device, &queue, &mut r, w, h, &view, &draws, None);
        for y in 0..h as usize {
            let src = &rgba[y * w as usize * 4..(y + 1) * w as usize * 4];
            let o = (y * (w as usize * yaws.len()) + k * w as usize) * 4;
            sheet[o..o + w as usize * 4].copy_from_slice(src);
        }
    }
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| format!("{out}: {e}"))?), w * yaws.len() as u32, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&sheet)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words make the look they say, and the items, race and action;
    /// a stray word is refused (the control).
    #[test]
    fn looks_from_words() {
        let words = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let w = parse(&words("set=3 tint=8 robe=1 rtint=2 at=900 size=200x300")).unwrap();
        let l = w.look;
        assert_eq!((l.armor, l.armor_tint, l.robe, l.robe_tint, w.at, w.size), ([3, 3, 3, 3, 3, 0], [8; 6], 1, 2, 900, (200, 300)));
        let l = parse(&words("armor=1,2,3,4,5 dye=c03020")).unwrap().look;
        assert_eq!(l.armor, [1, 2, 3, 4, 5, 0]);
        assert_eq!(l.armor_color, Some([[0xC0, 0x30, 0x20, 255]; 6]));
        let bare = parse(&[]).unwrap();
        assert_eq!((bare.look.armor, bare.look.robe, bare.items, bare.race, bare.action), ([0; 6], -1, [0; 3], false, None));
        let held = parse(&words("main=17F9870B off=0xB0A9CBD2 shield=2486a8d7 race=1 action=23")).unwrap();
        assert_eq!((held.items, held.race, held.action), ([0x17F9_870B, 0xB0A9_CBD2, 0x2486_A8D7], true, Some(23)));
        assert!(!parse(&words("race=0")).unwrap().race);
        assert_eq!((bare.tilt, parse(&words("tilt=60")).unwrap().tilt, parse(&words("tilt=-30")).unwrap().tilt), (None, Some(60.0), Some(-30.0)));
        let caped = parse(&words("cape=3 capecolor=c03020")).unwrap();
        assert_eq!(caped.cape, net::cape::Cape { style: 3, color: 0xC030_20FF });
        assert_eq!(bare.cape, net::cape::Cape::default(), "no cape unless asked");
        let helmed = parse(&words("set=3 helm=4 helmfit=0 era=2002 frame=head")).unwrap();
        assert_eq!((helmed.look.armor, helmed.helm_fit, helmed.y2002, helmed.head), ([3, 3, 3, 3, 3, 4], false, true, true));
        assert_eq!((bare.look.armor[5], bare.helm_fit, bare.y2002, bare.head), (0, true, false, false), "no helm, the fit on, Frontiers' art and the body framed unless asked");
        assert!(parse(&words("frame=feet")).is_err());
        let seated = parse(&words("helm=4 helmseat=head,1.4,0,0.36,0.1")).unwrap();
        assert_eq!(seated.seat, Some((SeatNode::Anchor(crate::slotless::Anchor::Head), crate::slotless::HelmFit { scale: glam::Vec3::splat(1.4), offset: glam::Vec3::new(0.0, 0.36, 0.1) })));
        let stretched = parse(&words("helmseat=6,2.4,1.6,2.0,0,0.48,0.12")).unwrap();
        assert_eq!(stretched.seat, Some((SeatNode::Node(6), crate::slotless::HelmFit { scale: glam::Vec3::new(2.4, 1.6, 2.0), offset: glam::Vec3::new(0.0, 0.48, 0.12) })));
        assert_eq!(parse(&words("helmseat=named,1,0,0,0")).unwrap().seat.map(|s| s.0), Some(SeatNode::Anchor(crate::slotless::Anchor::Named)));
        assert_eq!(bare.seat, None, "the body's own seat unless asked");
        assert!(parse(&words("helmseat=head,1,2")).is_err());
        assert!(parse(&words("helmseat=chin,1,0,0,0")).is_err());
        assert!(parse(&words("sett=3")).is_err());
        assert!(parse(&words("main=sword")).is_err());
        assert!(parse(&words("era=1999")).is_err());
    }
}
