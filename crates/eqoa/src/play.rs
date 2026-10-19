//! Playing on a server: the game session (crate `net`) pumped every frame
//! beside the walker, the player's state sent on channel 0x40 every
//! `gEntityUpdateRate` (100 ms; the library sends nothing when it has not
//! changed), and the entities on channels 0x00-0x17 drawn where
//! `VIClient::Extrapolate` puts them, each as its CSprite.
//!
//! Not yet: the creatures' own walk physics between updates
//! (`VICreatureEntity::Walk`/`ExtrapolateMove`/`ExtrapolateGround`), their
//! animation from it, items, armour and names.

use crate::walker::Walker;
use esf::Library;
use net::entity::{Entities, SLOTS};
use net::session::{Event, Player, Session};
use render::{GpuModel, Renderer, wgpu};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;
use vi::walk::{WalkControl, WalkPhysics};

/// `gEntityUpdateRate`.
pub const UPDATE_RATE: u32 = 100;

/// One drawn entity: its model as a CSprite, animated on its own clock.
struct Puppet {
    model: u32,
    look: Option<vi::cust::Look>,
    sprite: Option<CSprite<Ee>>,
    anims: AnimScene<Ee>,
    rand: Rand,
    gpu: GpuModel,
    /// Items and the helm, each on its node (`Attach`).
    attached: Vec<(i32, GpuModel)>,
    /// The name plate's anchor this frame: 0.6 above the name node (3).
    name_at: Option<[f32; 3]>,
}

/// `VICSprite::NameColors`.
const NAME_COLORS: [[f32; 4]; 3] = [[212.0 / 255.0, 192.0 / 255.0, 111.0 / 255.0, 1.0], [100.0 / 255.0, 175.0 / 255.0, 220.0 / 255.0, 1.0], [90.0 / 255.0, 1.0, 80.0 / 255.0, 1.0]];

/// A name plate to draw: where, in what colour, what.
pub struct Plate {
    pub at: [f32; 3],
    pub color: [f32; 4],
    pub name: String,
}

pub struct Play {
    pub session: Session,
    pub player: Player,
    /// The MemoryDump's whole content (inventory, abilities, ...).
    pub dump: Box<net::dump::MemoryDump>,
    pub entities: Entities,
    last_send: u32,
    puppets: Vec<Option<Puppet>>,
    /// The sounds the client asked for this frame (`PlaySound`, `PlayCue`).
    pub cues: Vec<crate::sound::Cue>,
    /// The weather the server set this frame (0x00C5, 0x00C7): the byte, and
    /// whether it is instant.
    pub weather: Vec<(u8, bool)>,
    /// Each creature's `m_walk`, made with it (`CreateEntity`: `Init` at the
    /// origin), keyed by the entity's id.
    walks: Vec<Option<(u32, WalkControl<Ee>)>>,
    /// The player's model as the server last named it on its channel.
    pub self_model: Option<u32>,
    /// The player dressed as its own channel says (look, items, helm),
    /// placed with the walker's pose.
    self_puppet: Option<Puppet>,
    /// The chat log, newest last (`VIWndGame::AddText`'s lines).
    pub chat: std::collections::VecDeque<(String, [f32; 4])>,
    /// A line being typed, when chat is open.
    pub typing: Option<String>,
    /// `m_target`: the selected entity's id, or u32::MAX.
    pub target: u32,
    /// `m_autoAttack`: held while the attack button is (the 0x40 carries it).
    pub auto_attack: bool,
    /// `m_p071E8`: when the next 0x000F may go out, and the next swing.
    attack_due: u32,
    /// `m_p071F0`: when the last swing landed in reach.
    pub swung: u32,
    /// The stat channel (0x42).
    pub stats: net::dump::Stats,
    /// The game's words (WORDS.TXT), for the lines the client writes itself.
    pub words: Option<vi::strtab::StringTable>,
    /// What the HUD is built from, before there is a device: the atlas
    /// (out of SUPPORT) and TEXTURES.TXT.
    pub ui_data: Option<(Vec<vi::ui::UiTexEntry>, vi::strtab::StringTable)>,
    /// The HUD, once the viewer has a device (`attach_ui`): `VIUI`, the
    /// layer's assets, `VIWndGame`.
    pub hud: Option<(crate::ui::Vui, crate::ui::UiAssets, crate::hud::Hud)>,
    /// `m_targetInfoValid` / `m_targetDisposition`, from 0x0010.
    pub target_info: Option<u8>,
    /// Chat lines waiting for the chat box, with when they came.
    chat_pending: Vec<(String, [f32; 4], u32)>,
    /// The chat box (`VIWndChat`), with the HUD.
    wnd_chat: Option<crate::chat::WndChat>,
    /// When the HUD was last drawn (its frame time).
    hud_last: i64,
    /// `m_p382E4`: the chat channel plain text goes to (0 say, 2 group,
    /// 3 shout, 4 guild).
    pub channel: u8,
    /// `m_p4F0B8`: who last sent a tell.
    pub reply_name: String,
    /// `VIWndGame::m_mode`: 0 playing, 9 a dialog showing.
    pub mode: i32,
    /// `m_dialogs`: dialogs waiting while another mode is up (at most 11).
    dialogs: std::collections::VecDeque<Vec<u16>>,
    /// `VIWndPrivChat`: the dialog box.
    priv_chat: crate::dialog::PrivChat,
    /// `VIWndResponse`: a dialog's answers.
    response: crate::dialog::Response,
    /// `m_dialog` / `m_p2EBFC`: an option box waiting while another mode is up.
    pending_box: Option<crate::dialog::DialogBox>,
    /// A teleport into another world, for the viewer to load: the world
    /// file, where, the heading.
    world_change: Option<(String, [f32; 3], f32)>,
}

/// `gWorldInfo`'s paths on the Frontiers disc, by world index (CLIENT's
/// table names six; a server's S2 can add more, which are not read here).
pub const WORLD_FILES: [&str; 6] = ["TUNARIA.ESF", "RATHE.ESF", "ODUS.ESF", "LAVASTM.ESF", "PLANESKY.ESF", "SECRETS.ESF"];

impl Play {
    /// `ShowDialog`: expanded (`%p` and the icon codes), shown now when
    /// nothing else is up, else queued.
    fn show_dialog(&mut self, text: &[u16]) {
        let ctx = vi::ui::ExpandContext { player_name: self.dump.player.name.clone(), ..Default::default() };
        let buf = vi::ui::expand(text, 218, true, &ctx);
        if self.mode == 0 {
            self.open_dialog(&buf);
        } else if self.dialogs.len() < 11 {
            self.dialogs.push_back(buf);
        }
    }

    /// `Msg0034`: the text (128) and each option (64) expanded; shown at
    /// once in play, else kept in the one waiting slot (and refused, 0x0035
    /// 255, when that is taken).
    fn dialog_box(&mut self, id: u32, text: &[u16], options: &[Vec<u16>]) {
        let ctx = vi::ui::ExpandContext { player_name: self.dump.player.name.clone(), ..Default::default() };
        let d = crate::dialog::DialogBox {
            id,
            text: vi::ui::expand(text, 128, true, &ctx),
            options: options.iter().take(6).map(|o| vi::ui::expand(o, 64, true, &ctx)).collect(),
        };
        if self.mode == 0 {
            self.show_dialog_box(d);
        } else if self.pending_box.is_none() {
            self.pending_box = Some(d);
        } else {
            self.session.send_msg(0x0035, id, 255);
        }
    }

    /// A 0x0034 dialog as if the server had sent it (`EQOA_BOX` in a
    /// playshot: the text, then the options, `|`-separated).
    pub fn fake_dialog_box(&mut self, spec: &str) {
        let mut parts = spec.split('|').map(|p| p.encode_utf16().collect::<Vec<u16>>());
        let text = parts.next().unwrap_or_default();
        let options: Vec<Vec<u16>> = parts.collect();
        self.dialog_box(0, &text, &options);
    }

    /// `VIWndGame::ShowDialogBox`.
    fn show_dialog_box(&mut self, d: crate::dialog::DialogBox) {
        self.cues.push(crate::sound::Cue::Ui(15));
        let Some((vui, _, _)) = &self.hud else { return };
        self.response.set_dialog(vui, d);
        self.mode = 7;
    }

    /// A key for the dialog windows: true when one took it.
    pub fn dialog_key(&mut self, key: crate::dialog::Key) -> bool {
        match self.mode {
            9 => {
                if matches!(key, crate::dialog::Key::Enter) {
                    self.close_dialog();
                }
                true
            }
            7 => {
                let Some((vui, _, _)) = &self.hud else { return true };
                let time = vui.time;
                let answer = self.response.key(vui, key, time);
                if let Some(c) = self.response.cue.take() {
                    self.cues.push(crate::sound::Cue::Ui(c));
                }
                if let crate::dialog::Answer::Send(id, choice) = answer {
                    self.session.send_msg(0x0035, id, choice);
                    self.mode = 0;
                }
                true
            }
            _ => false,
        }
    }

    /// `VIWndGame::OpenDialog`.
    fn open_dialog(&mut self, text: &[u16]) {
        // every dialog this client opens is ShowDialog's b = 1 (0x0046, and
        // the queue): sound 15 with it
        self.cues.push(crate::sound::Cue::Ui(15));
        self.priv_chat.set_text(text);
        self.mode = 9;
    }

    /// The dialog box answered (279): back to play.
    pub fn close_dialog(&mut self) {
        if self.mode == 9 {
            self.mode = 0;
        }
    }

    /// `InteractWithTarget`: by the target's npc flags, 0x004C (1), 0x124D
    /// (2) or 0x0061 (8); a player or a corpse opens the details box (not
    /// ported); anyone else is talked to (0x0004).
    pub fn interact_with_target(&mut self) {
        let Some(e) = self.entities.slots.iter().flatten().find(|e| e.id == self.target) else { return };
        let Some(c) = &e.creature else { return };
        let f = c.npc_flags;
        let op = if f & 1 != 0 {
            0x004C
        } else if f & 2 != 0 {
            0x124D
        } else if f & 8 != 0 {
            0x0061
        } else if c.p256 == 2 || c.p256 == 3 {
            return;
        } else {
            0x0004
        };
        let mut w = net::stream::Writer::op(op);
        w.u32(self.target);
        self.session.send(&w.data);
    }
}


/// A creature's model as `Apply` dresses it: the look on its text slots, the
/// items and helm on their nodes.
fn build_puppet(lib: &Library, g: &mut Gpu, e: &net::entity::Entity, look: Option<vi::cust::Look>, slot: usize, game_ms: i64) -> Option<Puppet> {
    let textures = render::library_textures(lib);
    lib.lookup(e.model as i32).and_then(|r| {
                    let mut model = lib.build_model(r)?;
                    let mut anims = AnimScene { time: game_ms, ..Default::default() };
                    let mut rand = Rand(slot as u64 + 1);
                    let sprite = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand);
                    // Attach: the items in hand (ApplyItems: the override if
                    // any) and the helm (SetHelm, at node 8, in the tint).
                    let mut attached = Vec::new();
                    let mut helm_worn = false;
                    if let (Some(cs), Some(c)) = (&sprite, &e.creature) {
                        for i in 0..3 {
                            let id = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
                            let node = cs.item_nodes[i];
                            if id == 0 || node < 0 {
                                continue;
                            }
                            if let Some(m) = lib.lookup(id as i32).and_then(|r| lib.build_model(r)) {
                                attached.push((node, g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
                            }
                        }
                        if cs.node_ids[8] >= 0
                            && let Some(h) = vi::cust::helm(c.armor[5])
                            && let Some(mut m) = lib.lookup(h as i32).and_then(|r| lib.build_model(r))
                        {
                            let tint = vi::cust::TINTS[(c.armor_tint[5] as usize).min(14)];
                            for b in &mut m.batches {
                                for l in &mut b.layers {
                                    l.color = tint;
                                }
                            }
                            helm_worn = true;
                            attached.push((cs.node_ids[8], g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
                        }
                    }
                    // ApplyLook: the look's textures and tints on the layers the
                    // sprite's text slots name.
                    if let (Some(cs), Some(look)) = (&sprite, &look) {
                        dress(&mut model, &vi::cust::apply_look(cs, look, helm_worn, |id| lib.surface(id as i32).is_some()));
                    }
                    let gpu = g.renderer.upload(g.device, g.queue, &model, &[render::IDENTITY], &textures);
                    Some(Puppet { model: e.model, look, sprite, anims, rand, gpu, attached, name_at: None })
                })
}

/// Puts layer changes onto the batches of the palette entries they name.
fn dress(model: &mut esf::Model, changes: &[vi::cust::LayerChange]) {
    for ch in changes {
        for b in model.batches.iter_mut().filter(|b| b.material == ch.entry) {
            if let Some(l) = b.layers.get_mut(ch.layer as usize) {
                if let Some(t) = ch.texture {
                    l.texture = t as i32;
                }
                if let Some(c) = ch.color {
                    l.color = c;
                }
            }
        }
    }
}

/// A column-major matrix back into the row-vector `Matrix44`.
fn glam_to_matrix(m: [[f32; 4]; 4]) -> Matrix44<Ee> {
    let mut out = Matrix44::<Ee>::identity();
    for (r, row) in out.m.iter_mut().enumerate() {
        row.x = F::new(m[r][0]);
        row.y = F::new(m[r][1]);
        row.z = F::new(m[r][2]);
        row.w = F::new(m[r][3]);
    }
    out
}

/// The chat log as labels, bottom left, newest lowest, and the line being
/// typed below it. A stand-in for `VIWndGame`'s chat window, which is not
/// ported: the lines, their colours and the font are the game's.
pub fn chat_labels(play: &Play, text: &render::text::Text, size: (u32, u32)) -> Vec<render::text::Label> {
    let k = size.1 as f32 / render::text::SCREEN_H;
    let line = (text.height() as f32 + 2.0) * k;
    let mut out = Vec::new();
    // With the HUD the chat box shows the lines; only the line being typed
    // is drawn here.
    let mut rows: Vec<(String, [f32; 4])> = if play.hud.is_some() { Vec::new() } else { play.chat.iter().cloned().collect() };
    if let Some(t) = &play.typing {
        rows.push((format!("> {t}_"), [1.0, 1.0, 0.6, 1.0]));
    }
    if play.hud.is_none()
        && let Some(name) = play.target_name()
    {
        out.push(render::text::Label { at: [0.0, 1.0 - (24.0 * k) / size.1 as f32 * 2.0, 0.0], color: [1.0, 1.0, 1.0, 1.0], text: name });
    }
    let n = rows.len();
    for (i, (s, c)) in rows.into_iter().enumerate() {
        let w = text.width(&s) as f32 * k;
        let x = 12.0 * k + w / 2.0;
        let y = size.1 as f32 - 12.0 * k - line * (n - i) as f32 + line / 2.0;
        let ndc = [x / size.0 as f32 * 2.0 - 1.0, 1.0 - y / size.1 as f32 * 2.0, 0.0];
        out.push(render::text::Label { at: ndc, color: c, text: s });
    }
    out
}

/// `VINameSprite::Raster` and `RasterDeferred` for a frame seen from `eye`
/// through `view_proj`: plates within 22 of the eye (the squared view-space
/// length plus w² under 22²), at most 24, nearest first, each dropped if its
/// rect on the PS2's 640x448 screen meets one already kept (`VIRect::
/// Intersects`, inclusive).
pub fn labels(plates: &[Plate], eye: glam::Vec3, view_proj: glam::Mat4, text: &render::text::Text) -> Vec<render::text::Label> {
    let mut queued = Vec::new();
    for p in plates {
        let at = glam::Vec3::from(p.at);
        if 22.0 * 22.0 <= (at - eye).length_squared() + 1.0 || queued.len() >= 24 {
            continue;
        }
        let clip = view_proj * at.extend(1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        let (qx, qy) = ((ndc.x * 320.0 + 320.0) as i32, (-ndc.y * 224.0 + 224.0) as i32);
        let w = text.width(&p.name) as i32;
        let h = text.height() as i32;
        let min = (qx - (w >> 1), qy - (h >> 1));
        queued.push((clip.w, min, (min.0 + w, min.1 + h), ndc, p));
    }
    queued.sort_by(|a, b| a.0.total_cmp(&b.0));
    /// A kept plate: its rect's corners, where it is drawn, what.
    type Kept<'a> = ((i32, i32), (i32, i32), glam::Vec3, &'a Plate);
    let mut kept: Vec<Kept> = Vec::new();
    for (_, min, max, ndc, p) in queued {
        let meets = kept.iter().any(|k| !(min.0 > k.1.0 || max.0 < k.0.0 || min.1 > k.1.1 || max.1 < k.0.1));
        if !meets {
            kept.push((min, max, ndc, p));
        }
    }
    kept.into_iter().map(|(_, _, ndc, p)| render::text::Label { at: ndc.to_array(), color: p.color, text: p.name.clone() }).collect()
}

fn look_of(c: &net::entity::Creature) -> vi::cust::Look {
    vi::cust::Look { armor: c.armor, armor_tint: c.armor_tint, hair: c.hair, face: c.face, robe: c.robe as i8, robe_tint: c.robe_tint }
}

fn v3(a: [f32; 3]) -> Vect3<Ee> {
    Vect3::of(a[0], a[1], a[2])
}

pub struct Gpu<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

impl Play {
    pub fn new(session: Session, player: Player, dump: Box<net::dump::MemoryDump>) -> Self {
        Play { session, player, dump, entities: Entities::default(), last_send: 0, puppets: (0..SLOTS).map(|_| None).collect(), cues: vec![crate::sound::Cue::Named("respawn", 0.4)], weather: Vec::new(), walks: (0..SLOTS).map(|_| None).collect(), self_model: None, self_puppet: None, chat: Default::default(), typing: None, target: u32::MAX, auto_attack: false, attack_due: 0, swung: 0, stats: Default::default(), words: None, ui_data: None, hud: None, target_info: None, chat_pending: Vec::new(), wnd_chat: None, hud_last: 0, channel: 0, reply_name: String::new(), mode: 0, dialogs: Default::default(), priv_chat: Default::default(), response: Default::default(), pending_box: None, world_change: None }
    }

    fn self_id32(&self) -> Option<u32> {
        self.session.self_id.map(|s| s as u32)
    }

    /// One frame. Returns the player's model when the server has just named
    /// a different one (the caller re-dresses the walker).
    pub fn frame(&mut self, walker: &mut Walker, lib: &Library, mut g: Gpu, game_ms: i64, mut sound: Option<&mut crate::sound::Sound>) -> Option<u32> {
        let now = self.session.now();
        let mut new_model = None;
        for e in self.session.pump() {
            match e {
                Event::TargetInfo { disposition, .. } => self.target_info = Some(disposition),
                Event::ReplyName(n) => self.reply_name = n,
                Event::Dialog { text, .. } => self.show_dialog(&text),
                Event::DialogBox { id, text, options } => self.dialog_box(id, &text, &options),
                Event::Teleport { world, pos, hpr, .. } => {
                    // Msg00B2: back to play, then Respawn.
                    self.mode = 0;
                    if world as i64 != self.player.world {
                        match WORLD_FILES.get(world as usize) {
                            Some(f) => {
                                eprintln!("play: teleport to world {world} ({f}) at {pos:?}");
                                self.world_change = Some((f.to_string(), pos, hpr[0]));
                                self.player.world = world as i64;
                                self.player.pos = pos;
                                // The walker is rebuilt: dress it again.
                                self.self_model = None;
                            }
                            None => eprintln!("play: teleport to world {world}, which the disc's world table does not have"),
                        }
                    } else {
                        eprintln!("play: teleport to {pos:?}");
                        walker.relocate(pos, hpr[0]);
                        self.player.pos = pos;
                    }
                }
                Event::LevelExp { level, exp } => {
                    if self.dump.player.level < level {
                        self.cues.push(crate::sound::Cue::Ui(16));
                    }
                    self.dump.player.level = level;
                    self.dump.player.exp = exp;
                }
                Event::Action { id, action } => {
                    // Msg 0x0775: the player's own only for action 34.
                    if Some(id) == self.self_id32() {
                        if action == 34 {
                            walker.set_action(34);
                        }
                    } else if let Some(slot) = self.entities.slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id))
                        && let Some(p) = self.puppets[slot].as_mut()
                        && let Some(cs) = p.sprite.as_mut()
                    {
                        cs.set_action(action as i32, &p.anims, &mut p.rand);
                    }
                }
                Event::PlayerP2C(v) => self.dump.player.p2c = v as i64,
                Event::State { channel: 0x42, data } => {
                    self.stats = net::dump::read_stats(&mut net::stream::Reader::new(&data));
                }
                Event::State { channel, data } if (channel as usize) < SLOTS => {
                    let self_id = self.session.self_id;
                    let skel = self.puppets[channel as usize].as_ref().and_then(|p| p.sprite.as_ref()).map_or(0, |s| s.skel_type);
                    let scene = &mut walker.scene;
                    let walk = &mut self.walks[channel as usize];
                    self.entities.channel(channel as usize, &data, self_id, now, UPDATE_RATE, |e| {
                        if walk.as_ref().map(|w| w.0) != Some(e.id) {
                            let mut w = WalkControl::<Ee>::default();
                            w.init(Vect3::default(), Vect3::default(), scene);
                            *walk = Some((e.id, w));
                        }
                        // VICreatureEntity::Sync: the update becomes the walk
                        // control's physics; a creature that does not walk is
                        // put on the ground at once.
                        if let (Some(c), Some((_, w))) = (&e.creature, walk.as_mut()) {
                            let p = WalkPhysics::<Ee> {
                                position: v3(e.pos),
                                angles: v3(e.hpr),
                                velocity: v3(c.velocity),
                                angular_vel: v3(c.angular_vel),
                                force: v3(c.accel),
                                torque: v3(c.angular_accel),
                                floor: F::new(c.floor),
                                loco_state: c.state as i32,
                                target_heading: F::new(c.target_heading),
                                flags: c.walk_flags,
                                ..Default::default()
                            };
                            w.set_update_physics(&p, scene);
                        }
                        if e.creature.as_ref().is_some_and(|c| c.p255 == 0) {
                            let (mut pos, mut hpr) = (v3(e.pos), v3(e.hpr));
                            vi::creature::extrapolate_ground(scene, -1, skel, None, &mut pos, &mut hpr);
                            e.pos = pos.to_array();
                            e.hpr = hpr.to_array();
                        }
                        e.extrapolated = game_ms;
                    });
                }
                Event::Disconnected(r) => eprintln!("play: disconnected (reason {r})"),
                Event::Chat(text, color) => {
                    eprintln!("chat: {text}");
                    self.add_chat(text, color);
                }
                Event::Weather { weather, instant } => {
                    eprintln!("play: weather {weather}{}", if instant { " (instant)" } else { "" });
                    self.weather.push((weather, instant));
                }
                Event::WorldEntry { self_id, player, dump } => {
                    eprintln!("play: world entry again as {self_id:#x} at {:?}", player.pos);
                    self.player = player;
                    self.dump = dump;
                    // Msg000D's PlayCue
                    self.cues.push(crate::sound::Cue::Named("respawn", 0.4));
                }
                _ => {}
            }
        }
        if self.has_self() {
            self.auto_attack_tick(walker, now);
        }
        // UpdateGame: back in play, the next waiting dialog.
        if self.mode == 0
            && let Some(d) = self.dialogs.pop_front()
        {
            self.open_dialog(&d);
        } else if self.mode == 0
            && let Some(d) = self.pending_box.take()
        {
            self.show_dialog_box(d);
        }
        let self_id = self.session.self_id;
        // UpdateCreatures / VICreatureEntity::Walk: a creature with its own
        // walk physics (a player, +0x255) is stepped by it; any other is
        // carried along its velocity since it was last moved (ExtrapolateMove).
        let me = self.self_id32();
        for (slot, e) in self.entities.slots.iter_mut().enumerate() {
            let Some(e) = e else { continue };
            let Some(c) = &e.creature else { continue };
            if Some(e.id) == me {
                continue;
            }
            if c.p255 != 0
                && let Some((_, w)) = self.walks[slot].as_mut()
            {
                w.process(1, &mut walker.scene);
                e.pos = w.committed.position.to_array();
                e.hpr = [w.committed.angles.x.0, 0.0, 0.0];
                continue;
            }
            let skel = self.puppets[slot].as_ref().and_then(|p| p.sprite.as_ref()).map_or(0, |s| s.skel_type);
            let dt = F::<Ee>::new((game_ms - e.extrapolated) as i32 as f32) * F::new(0.001);
            let (mut pos, mut hpr) = (v3(e.pos), v3(e.hpr));
            vi::creature::extrapolate_move(&mut walker.scene, -1, skel, None, &v3(c.velocity), dt, &mut pos, &mut hpr);
            e.pos = pos.to_array();
            e.hpr = hpr.to_array();
            e.extrapolated = game_ms;
        }
        self.entities.extrapolate(self_id, now, UPDATE_RATE);
        // SendPosition, at the update rate.
        if self_id.is_some() && now.wrapping_sub(self.last_send) >= UPDATE_RATE {
            self.last_send = now;
            let msg = vi::selfstate::encode(self.player.world as u8, &walker.cam.walk.committed, self.auto_attack as u8, self.target);
            self.session.send_state(0x40, &msg);
        }
        let me = self.self_id32();
        for (slot, entity) in self.entities.slots.iter().enumerate() {
            let want = entity.as_ref().filter(|e| Some(e.id) != me && e.model != 0);
            if let Some(e) = entity
                && Some(e.id) == me
                && e.model != 0
                && self.self_model != Some(e.model)
            {
                self.self_model = Some(e.model);
                new_model = Some(e.model);
            }
            if let Some(e) = entity
                && Some(e.id) == me
                && e.model != 0
            {
                let look = e.creature.as_ref().map(look_of);
                if self.self_puppet.as_ref().map(|p| (p.model, p.look)) != Some((e.model, look)) {
                    self.self_puppet = build_puppet(lib, &mut g, e, look, slot, game_ms);
                }
            }
            // a creature's sounds are shared while it is loaded
            let sounds_of = |p: &Option<Puppet>| p.as_ref().and_then(|p| p.sprite.as_ref()).map(|cs| cs.sounds()).unwrap_or_default();
            let Some(e) = want else {
                if let Some(s) = sound.as_deref_mut() {
                    s.release_sounds(&sounds_of(&self.puppets[slot]));
                }
                self.puppets[slot] = None;
                continue;
            };
            let look = e.creature.as_ref().map(look_of);
            if self.puppets[slot].as_ref().map(|p| (p.model, p.look)) != Some((e.model, look)) {
                let old = sounds_of(&self.puppets[slot]);
                self.puppets[slot] = build_puppet(lib, &mut g, e, look, slot, game_ms);
                if let Some(s) = sound.as_deref_mut() {
                    s.share_sounds(lib, &sounds_of(&self.puppets[slot]));
                    s.release_sounds(&old);
                }
            }
            let Some(p) = self.puppets[slot].as_mut() else { continue };
            let f = F::<Ee>::new;
            let actor = Matrix44::transform(
                &Vect3::new(f(e.actor_hpr[0]), f(e.actor_hpr[1]), F::ZERO),
                f(if e.scale > 0.0 { e.scale } else { 1.0 }),
                &Vect3::new(f(e.actor_pos[0]), f(e.actor_pos[1]), f(e.actor_pos[2])),
            );
            let (matrix, palette) = match p.sprite.as_mut() {
                Some(cs) => {
                    p.anims.time = game_ms;
                    // UpdateCreatures: a walking creature's sprite follows its
                    // walk control; any other takes SetLocomotion(state).
                    if let Some(c) = &e.creature {
                        match self.walks[slot].as_ref() {
                            Some((_, w)) if c.p255 != 0 => {
                                w.update_sprite(cs, &p.anims, &mut p.rand);
                            }
                            _ => cs.set_locomotion(c.state as i32, &p.anims, &mut p.rand),
                        }
                    }
                    // VICSprite::Process: the sounds its triggers fired
                    if let Some(s) = sound.as_deref_mut() {
                        let at = Vect3::new(f(e.actor_pos[0]), f(e.actor_pos[1]), f(e.actor_pos[2]));
                        cs.process_sounds(&walker.eye.position, &walker.eye.direction, &at, Some(&mut s.sink()));
                    }
                    cs.h.process(0, &p.anims);
                    let m = cs.h.calc_trans_about_center(&actor, &p.anims);
                    cs.h.set_blend_matrices();
                    for (node, gpu) in &p.attached {
                        if let Some(n) = cs.h.nodes.get(*node as usize) {
                            g.renderer.set_instances(g.queue, gpu, &[Matrix44::mul(&n.world, &m).to_cols()]);
                        }
                    }
                    p.name_at = cs.h.nodes.get(cs.node_ids[3].max(0) as usize).filter(|_| cs.node_ids[3] >= 0).map(|n| {
                        let w = Matrix44::mul(&n.world, &m);
                        let a = w.mul_point(&Vect3::of(0.0, 0.6, 0.0));
                        a.to_array()
                    });
                    (m.to_cols(), cs.h.blend.iter().map(|m| m.to_cols()).collect::<Vec<_>>())
                }
                None => (actor.to_cols(), Vec::new()),
            };
            g.renderer.set_instances(g.queue, &p.gpu, &[matrix]);
            if !palette.is_empty() {
                g.renderer.set_palette(g.queue, &p.gpu, &palette);
            }
        }
        new_model
    }

    /// `SetName`'s plates for every creature drawn: its name, in
    /// `NameColors[ApplyLook's pick]`, at its name node.
    pub fn plates(&self) -> Vec<Plate> {
        let mut out = Vec::new();
        for (slot, e) in self.entities.slots.iter().enumerate() {
            let (Some(e), Some(Some(p))) = (e, self.puppets.get(slot)) else { continue };
            let (Some(c), Some(at)) = (&e.creature, p.name_at) else { continue };
            if c.name.is_empty() {
                continue;
            }
            let color = if c.p256 < 2 { 0 } else if c.p299 & 1 != 0 { 2 } else { 1 };
            out.push(Plate { at, color: NAME_COLORS[color], name: c.name.clone() });
        }
        out
    }

    /// `CycleTarget(0)`: the next creature in view, by `CompareY` over the
    /// game's score (mostly how far off the screen's centre line, plus
    /// distance; a corpse 2 further), from the camera at `eye` looking along
    /// `dir`; then 0x0010 with the token and the choice. Not here: the group
    /// and the pet, which are skipped (there are none yet).
    pub fn cycle_target(&mut self, eye: glam::Vec3, dir: glam::Vec3, view_proj: glam::Mat4) {
        let me = self.self_id32();
        let mut cand: Vec<(f32, u32)> = Vec::new();
        for (slot, e) in self.entities.slots.iter().enumerate() {
            let Some(e) = e else { continue };
            if self.puppets.get(slot).is_none_or(|p| p.is_none()) || Some(e.id) == me {
                continue;
            }
            let mut dead = false;
            if let Some(c) = &e.creature {
                if c.p256 == 3 {
                    continue;
                }
                dead = c.state == 14;
            }
            let pos = glam::Vec3::from(e.actor_pos);
            let d2 = (pos - eye).length_squared();
            let mut near = false;
            if d2 < 15.0 {
                let a = glam::Vec3::new(pos.x - eye.x, 0.0, pos.z - eye.z).normalize_or_zero();
                let b = glam::Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
                near = a.dot(b) > 0.65;
            }
            let h = view_proj * pos.extend(1.0);
            let inside = h.w > 0.0 && (h.x / h.w).abs() <= 1.0 && (h.y / h.w).abs() <= 1.0 && h.z / h.w >= 0.0 && h.z / h.w <= 1.0;
            if !near && !inside {
                continue;
            }
            if e.p18 & 1 == 0 || d2 >= 40000.0 {
                continue;
            }
            let x = h.x / h.w;
            let score = if near {
                0.0
            } else if d2 < 25.0 {
                d2.sqrt() / 50.0 * 0.35
            } else {
                d2.sqrt() / 50.0 * 0.35 + x * x
            };
            cand.push((score + if dead { 2.0 } else { 0.0 }, e.id));
        }
        cand.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self.auto_attack = false;
        self.target_info = None;
        let n = cand.len();
        self.target = if n == 0 {
            u32::MAX
        } else {
            match cand.iter().position(|c| c.1 == self.target) {
                None => cand[0].1,
                Some(k) => cand[(k + 1) % n].1,
            }
        };
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(self.target);
        self.session.send(&w.data);
    }

    /// A world change for the viewer to carry out (`EnterWorld` -> `LoadWorld`).
    pub fn take_world_change(&mut self) -> Option<(String, [f32; 3], f32)> {
        self.world_change.take()
    }

    /// The creatures' names, slot order.
    pub fn entity_names(&self) -> Vec<String> {
        self.entities.slots.iter().flatten().filter_map(|e| e.creature.as_ref().map(|c| c.name.trim().to_string())).collect()
    }

    /// Targets the creature called `name` (as a click on it would), 0x0010.
    pub fn target_by_name(&mut self, name: &str) -> bool {
        let Some(id) = self.entities.slots.iter().flatten().find(|e| e.creature.as_ref().is_some_and(|c| c.name.trim() == name.trim())).map(|e| e.id) else {
            return false;
        };
        self.target = id;
        self.auto_attack = false;
        self.target_info = None;
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(id);
        self.session.send(&w.data);
        true
    }

    /// `ToggleAutoAttack`, the button pressed (the game option that makes it
    /// a toggle is off, as by default): a corpse is asked for its loot
    /// (0x0015); a creature not flagged 0x80 is attacked: 0x000F with the
    /// target when the swing timer allows, and auto-attack on while held.
    pub fn attack_press(&mut self) {
        let now = self.session.now();
        let Some(e) = self.entities.slots.iter().flatten().find(|e| e.id == self.target) else { return };
        if e.p18 & 2 != 0 {
            let mut w = net::stream::Writer::op(0x0015);
            w.u32(self.target);
            self.session.send(&w.data);
            return;
        }
        let Some(c) = &e.creature else { return };
        if c.npc_flags & 0x80 != 0 {
            return;
        }
        if self.attack_due == 0 || (self.attack_due.wrapping_sub(now) as i32) <= 0 {
            self.attack_due = now;
            let mut w = net::stream::Writer::op(0x000F);
            w.u32(self.target);
            self.session.send(&w.data);
        }
        self.auto_attack = true;
    }

    /// `ChatLine`: a typed line. A slash command picks its channel for the
    /// one line (`/say`, `/g`, `/gu`, `/s`; 0x000E sets it and puts the
    /// current one back), `/t` goes as typed, `/r` to whoever last sent a
    /// tell; an unknown command is answered with word 826. Plain text goes
    /// to the current channel. Not here: `/alarm`, `/time`, `/follow` and
    /// the crafting commands.
    pub fn chat_line(&mut self, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        if self.session.self_id.is_none() || wide.len() >= 128 {
            return;
        }
        let target = self.target;
        if wide.first() != Some(&(b'/' as u16)) {
            match self.channel {
                4 => self.session.send_op_sub_wide(0x0578, 12, text),
                3 => self.session.send_op_sub_wide(0x0C01, 2, text),
                _ => self.session.say(text, target),
            }
            return;
        }
        // The command: up to ten characters, to a space, A-Z lowered.
        let mut n = 0;
        let mut cmd = String::new();
        loop {
            let c = wide[n] as u8;
            cmd.push(if c.is_ascii_uppercase() { c + 32 } else { c } as char);
            n += 1;
            if n >= wide.len() || wide[n] == b' ' as u16 || n >= 10 {
                break;
            }
        }
        let rest_at = if wide.get(n) == Some(&(b' ' as u16)) { n + 1 } else { n };
        let rest = String::from_utf16_lossy(&wide[rest_at.min(wide.len())..]);
        let back = self.channel;
        match cmd.as_str() {
            "/sa" | "/say" => {
                self.session.send_op_byte(0x000E, 0);
                self.session.say(&rest, target);
                self.session.send_op_byte(0x000E, back);
            }
            "/g" | "/group" => {
                self.session.send_op_byte(0x000E, 2);
                self.session.say(&rest, target);
                self.session.send_op_byte(0x000E, back);
            }
            "/gu" | "/guild" => {
                self.session.send_op_byte(0x000E, 4);
                self.session.send_op_sub_wide(0x0578, 12, &rest);
                self.session.send_op_byte(0x000E, back);
            }
            "/s" | "/shout" => {
                self.session.send_op_byte(0x000E, 3);
                self.session.send_op_sub_wide(0x0C01, 2, &rest);
                self.session.send_op_byte(0x000E, back);
            }
            "/t" | "/tell" => self.session.say(text, target),
            "/r" | "/reply" => {
                if !self.reply_name.is_empty() {
                    let line = format!("/t {} {}", self.reply_name, rest);
                    self.session.say(&line, target);
                }
            }
            _ => {
                let word = self.words.as_ref().map_or_else(|| "(word 826)".to_string(), |w| w.get(826, 0).to_string());
                self.add_chat(format!("{word} \"{cmd}\"."), [1.0; 4]);
            }
        }
    }

    /// `VIWndGame::AddText`: a line for the chat box (queued with its time
    /// until the box, which measures with the font, takes it), and for the
    /// plain log shown when there is no HUD.
    fn add_chat(&mut self, text: String, color: [f32; 4]) {
        self.chat_pending.push((text.clone(), color, self.session.now()));
        self.chat.push_back((text, color));
        while self.chat.len() > 8 {
            self.chat.pop_front();
        }
    }

    /// A line the client writes itself: word `id`, in red as the game's
    /// warnings are.
    fn warn(&mut self, id: i32) {
        let text = self.words.as_ref().map_or_else(|| format!("(word {id})"), |w| w.get(id, 0).to_string());
        self.add_chat(text, [1.0, 0.25, 0.25, 1.0]);
    }

    /// `AutoAttackTick`, each frame while auto-attack is held: when the
    /// swing is due, what is worn picks the swing (`GetSwing`), and with
    /// the target in reach its spell effect would play (`VISpellEffects`
    /// are not ported, so only the time is kept); otherwise the swing's
    /// animation plays and 0x00D8 goes out, and the next is 2 s off.
    fn auto_attack_tick(&mut self, walker: &mut Walker, now: u32) {
        if !self.auto_attack || (now.wrapping_sub(self.attack_due) as i32) < 0 {
            return;
        }
        let (mut b14, mut b15, mut b16, mut b17, mut b18, mut b19, mut b20) = (false, false, false, false, false, false, false);
        let mut slot: i32 = -1;
        let mut anim = 23;
        let mut delay = 3500;
        let mut no_effect = false;
        for (i, it) in self.dump.inventory.items.iter().enumerate() {
            match it.equip_slot {
                14 | 16 | 18 => {
                    match it.equip_slot {
                        14 => b14 = true,
                        16 => b16 = true,
                        _ => b18 = true,
                    }
                    anim = vi::swing::attack_anim(it.item.attack_type as i32, false);
                    if slot == -1 {
                        slot = i as i32;
                    }
                }
                15 => b15 = true,
                17 => b17 = true,
                19 => {
                    no_effect = true;
                    b19 = true;
                }
                20 => b20 = true,
                _ => {}
            }
        }
        let swing = vi::swing::get_swing(self.player.class as i32, self.player.level as i32, b14, b15, b16, b18, b17, b19, b20);
        if vi::swing::swing_effect(swing) == 0 {
            no_effect = true;
        } else {
            delay = vi::swing::swing_delay(swing);
        }
        if self.target_in_range(walker, slot) && !no_effect {
            self.swung = now;
        } else {
            // With no weapon (slot -1) the game reads the slot before the
            // list's first, whatever that holds; taken here as not ranged.
            let ranged = slot >= 0 && self.dump.inventory.items[slot as usize].equip_slot == 18;
            if !ranged && !no_effect {
                walker.set_action(anim);
            }
            self.session.send(&net::stream::Writer::op(0x00D8).data);
            delay = 2000;
        }
        self.attack_due = self.attack_due.wrapping_add(delay as u32);
    }

    /// `TargetInRange(slot)`: melee within 4.5 (plus 3 or 5 for a large
    /// target, npc flags 0x400 / 0x800), a ranged weapon within its range
    /// with the power and the ammunition for it.
    fn target_in_range(&mut self, walker: &Walker, slot: i32) -> bool {
        // `extra` starts from an uninitialised stack float in the game;
        // taken as 0.
        let mut extra = 0.0f32;
        if self.target == u32::MAX {
            return false;
        }
        let me = self.self_id32();
        let Some(t) = self.entities.slots.iter().flatten().find(|e| e.id == self.target) else { return false };
        if t.p19 == 0 || Some(t.id) == me {
            return false;
        }
        let Some(m) = self.entities.slots.iter().flatten().find(|e| Some(e.id) == me) else { return false };
        let (Some(tc), Some(mc)) = (&t.creature, &m.creature) else { return false };
        if mc.p256 == 2 && (tc.p256 == 2 || tc.p256 == 3) {
            return false;
        }
        if tc.npc_flags & 0x400 != 0 {
            extra += 3.0;
        } else if tc.npc_flags & 0x800 != 0 {
            extra += 5.0;
        }
        // The player's entity stands where the walk camera last put it.
        let a = walker.cam.walk.committed.position.to_array();
        let b = t.pos;
        let d = (a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1]) + (a[2] - b[2]) * (a[2] - b[2]);
        let mut max = (extra + 4.5) * (extra + 4.5);
        let mut ranged_type = None;
        if slot >= 0 {
            let (attack_type, range) = {
                let item = &self.dump.inventory.items[slot as usize].item;
                (item.attack_type, item.range)
            };
            match attack_type {
                1..=6 => {}
                7 | 9 | 10 => {
                    let r = range as f32;
                    max = (r + extra) * (r + extra);
                    if !self.check_power() || !self.has_ammo_for(attack_type) {
                        return false;
                    }
                    ranged_type = Some(attack_type);
                }
                _ => return false,
            }
        }
        if 0.0 <= d && d <= max {
            return true;
        }
        if ranged_type.is_some() {
            self.warn(723);
        }
        false
    }

    /// `CheckPower`: a tenth of the power pool at least.
    fn check_power(&mut self) -> bool {
        if self.stats.stat[9] >= (self.stats.stat[10] as f32 / 10.0) as i32 {
            return true;
        }
        self.warn(731);
        false
    }

    /// `HasAmmoFor`: arrows (type 1) for a bow, bolts (2) for a crossbow,
    /// carried and not worn.
    fn has_ammo_for(&mut self, attack_type: i64) -> bool {
        let want = match attack_type {
            7 => 1,
            9 => 2,
            10 => return true,
            _ => -1,
        };
        if want >= 0 && self.dump.inventory.items.iter().any(|i| i.item.p18 == want && i.item.wear_slot == -1) {
            return true;
        }
        self.warn(if attack_type == 7 { 724 } else { 725 });
        false
    }

    /// `MaybeStopAutoAttack`: the button released.
    pub fn attack_release(&mut self) {
        self.auto_attack = false;
    }

    /// Builds the HUD on the viewer's device: the 2D layer, UI.CSF's
    /// surfaces and "ArialUI" out of the loaded files.
    pub fn attach_ui(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, lib: &Library) {
        let Some((atlas, textures)) = self.ui_data.take() else { return };
        let mut ui = render::ui::Ui::new(device, format, render::DEPTH_FORMAT);
        let words = self.words.clone().unwrap_or_default();
        let vui = crate::ui::Vui::new(words, textures, atlas);
        let mut assets = crate::ui::UiAssets::default();
        for (name, c) in &lib.files {
            if name.eq_ignore_ascii_case("UI.CSF") {
                assets.add_surfaces(&mut ui, device, queue, c);
            }
        }
        let id = vi::ui::font_id() as i32;
        let font = lib.files.iter().find_map(|(_, c)| (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(c, i).filter(|f| f.id == id)));
        if let Some(f) = font {
            assets.font = Some(ui.add_font(device, queue, &f));
        }
        renderer.ui = Some(ui);
        self.hud = Some((vui, assets, crate::hud::Hud::new()));
        self.wnd_chat = Some(crate::chat::WndChat::new());
    }

    /// The HUD's quads for this frame (`VIWndGame::OnDrawSelf`).
    pub fn draw_hud(&mut self, ui: &mut render::ui::Ui, walker: &Walker, time_ms: i64) {
        ui.quads.clear();
        let me = self.self_id32();
        let find = |id: Option<u32>| self.entities.slots.iter().flatten().find(|e| Some(e.id) == id);
        let me_e = find(me);
        let target = find(Some(self.target));
        let dir = walker.eye.direction;
        let (heading, _, _) = dir.get_hpr();
        let state = crate::hud::HudState {
            has_self: me_e.is_some(),
            self_hp: me_e.map_or(0, |e| e.p19 as i32),
            power: self.stats.stat[9],
            max_power: self.stats.stat[10],
            level: self.dump.player.level as i32,
            exp: self.dump.player.exp as i32,
            heading: heading.0,
            chat_open: self.wnd_chat.as_ref().is_some_and(|c| c.mode != 0),
            target_name: target.and_then(|e| e.creature.as_ref()).map(|c| c.name.clone()),
            target_hp: target.map_or(0, |e| e.p19 as i32),
            target_info: self.target_info,
            self_targeted_flag: Some(self.target) == me && self.dump.player.p2c > 0,
        };
        let now = self.session.now();
        let dt = ((time_ms - self.hud_last).clamp(0, 250)) as f32 / 1000.0;
        self.hud_last = time_ms;
        if let Some((vui, assets, hud)) = self.hud.as_mut() {
            vui.time = time_ms;
            let mut r = assets.raster(ui);
            hud.draw(vui, &mut r, &state);
            if let Some(c) = self.wnd_chat.as_mut() {
                for (text, color, at) in self.chat_pending.drain(..) {
                    let rr = &r;
                    c.text.add_text(&crate::ui::wide(&text), color, at, |s| vi::ui::Raster::string_width(rr, s));
                }
                c.draw(vui, &mut r, now, dt);
            }
            if self.mode == 9 {
                self.priv_chat.draw(vui, &mut r);
            }
            if self.mode == 7 {
                self.response.draw(vui, &mut r);
            }
        }
    }

    /// The target's name, for the screen (the target window is not ported).
    pub fn target_name(&self) -> Option<String> {
        self.entities.slots.iter().flatten().find(|e| e.id == self.target).and_then(|e| e.creature.as_ref()).map(|c| c.name.clone())
    }

    /// Whether the player is drawn dressed (the caller hides its plain body).
    /// `LeaderByte263` (no group): the player's creature is fighting.
    /// `None` before the player's creature is known (no battle music then).
    pub fn self_fighting(&self) -> Option<bool> {
        let me = self.self_id32()?;
        self.entities.slots.iter().flatten().find(|e| e.id == me)?.creature.as_ref().map(|c| c.p263 != 0)
    }

    pub fn has_self(&self) -> bool {
        self.self_puppet.is_some()
    }

    /// Places the dressed player with the walker's pose: the body at its
    /// model matrix and palette, the attachments on the walker's nodes.
    pub fn place_self(&self, walker: &Walker, matrix: [[f32; 4]; 4], palette: &[[[f32; 4]; 4]], r: &Renderer, queue: &wgpu::Queue) {
        let Some(p) = &self.self_puppet else { return };
        r.set_instances(queue, &p.gpu, &[matrix]);
        if !palette.is_empty() {
            r.set_palette(queue, &p.gpu, palette);
        }
        let Some(cs) = walker.sprite.as_ref() else { return };
        let centre = glam_to_matrix(matrix);
        for (node, gpu) in &p.attached {
            if let Some(n) = cs.h.nodes.get(*node as usize) {
                r.set_instances(queue, gpu, &[Matrix44::mul(&n.world, &centre).to_cols()]);
            }
        }
    }

    pub fn models(&self) -> impl Iterator<Item = &GpuModel> {
        self.puppets.iter().chain(std::iter::once(&self.self_puppet)).flatten().flat_map(|p| std::iter::once(&p.gpu).chain(p.attached.iter().map(|a| &a.1)))
    }

    pub fn describe(&self) -> String {
        let n = self.entities.slots.iter().flatten().count();
        format!("{n} entities, {} packets sent", self.session.packets_sent)
    }
}
