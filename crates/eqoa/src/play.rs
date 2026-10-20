//! Playing on a server: the game session (crate `net`) pumped every frame
//! beside the walker, the player's state sent on channel 0x40 every
//! `gEntityUpdateRate` (100 ms; the library sends nothing when it has not
//! changed), and the entities on channels 0x00-0x17 drawn where
//! `VIClient::Extrapolate` puts them, each as its CSprite.
//!
//! Not yet: the creatures' own walk physics between updates
//! (`VICreatureEntity::Walk`/`ExtrapolateMove`/`ExtrapolateGround`), their
//! animation from it, items, armour and names.

#[path = "npcw.rs"]
mod npcw;
#[path = "bankplay.rs"]
mod bankplay;
#[path = "mail.rs"]
mod mail;
#[path = "support.rs"]
mod support;

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
    /// Slot 4's place this frame (`CalcSlotWorldTransform(4)`), which a
    /// damage number floats from.
    slot4_at: Option<[f32; 3]>,
    /// The actor's transform and the centre transform it was drawn with
    /// this frame (what an effect's node transform is under).
    actor_m: Matrix44<Ee>,
    centre: Matrix44<Ee>,
    /// The three hands' weapon ends (the item's node and its two point
    /// sprites) and the weapon trail.
    weapons: [Option<(i32, crate::trails::Points)>; 3],
    trail: crate::trails::State,
    /// `m_p2A8`: not yet seen a frame (a corpse seen first lies at once).
    first: bool,
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
    /// `m_abilities`, by id (the map's key, `m_p00`), and the list's token
    /// (`m_p2BB60`) that 0x0032 echoes.
    pub abilities: std::collections::BTreeMap<i64, net::dump::Ability>,
    ability_token: u32,
    /// The HUD's three quick menus.
    pub hud_menus: crate::hudmenu::HudMenus,
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
    /// The active effects (state channel 0x43), `VIWndInfluence`'s icons.
    pub effects: net::dump::Effects,
    /// The floating damage numbers (`m_p4FD28`).
    damage: crate::damage::Damage,
    /// The player's actor position and its slot 4 this frame, while it is
    /// drawn.
    self_at: Option<([f32; 3], Option<[f32; 3]>)>,
    /// The game's words (WORDS.TXT), for the lines the client writes itself.
    pub words: Option<vi::strtab::StringTable>,
    /// What the HUD is built from, before there is a device: the atlas
    /// (built in) and TEXTURES.TXT.
    pub ui_data: Option<(Vec<vi::ui::UiTexEntry>, vi::strtab::StringTable)>,
    /// The HUD, once the viewer has a device (`attach_ui`): `VIUI`, the
    /// layer's assets, `VIWndGame`.
    pub hud: Option<(crate::ui::Vui, crate::ui::UiAssets, crate::hud::Hud)>,
    /// `m_targetInfoValid` / `m_targetDisposition`, from 0x0010.
    pub target_info: Option<u8>,
    /// 0x0010's title and guild (`m_p4FCA8`, `m_p4FC28`).
    target_title: String,
    target_guild: String,
    /// `m_p678`: the target's info window wanted (Q held on a player).
    pub info_up: bool,
    /// `m_petId` and `m_petName` (filled from the entity once it is seen).
    pet_id: u32,
    pet_name: String,
    info_window: vi::ui::Window,
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
    /// `VIWndGame::m_mode`: 0 playing, 2 the edit box, 7 a dialog's
    /// answers, 9 a dialog showing, 13 the start menu, 14 a message box,
    /// 12 the trainer, 22 the quest journal, 40 Class Mastery, 41 factions, 35 the who list.
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
    /// The group and the invitations.
    pub group: crate::group::Group,
    /// `VIWndMBox` (mode 14).
    mbox: crate::dialog::MBox,
    /// The player's death camera (`m_pC1B0`) and the screen's fade; the
    /// actors whose spells a death ends, for the next spell frame.
    death: Option<crate::death::DeathCam>,
    fade: crate::death::Fade,
    dead_casters: Vec<i32>,
    death_ms: i64,
    /// The camera: the race cameras (CLIENT's, when its image was read), the
    /// preset the start menu's framing put aside (`m_camPreset`) and the
    /// style's (`m_camSaved`), whether the world was entered (`m_p2BD50`),
    /// and whether a preset was saved, and what the start menu last asked
    /// of it (`OpenStartMenu`'s ApplyRaceCamera: true; `CloseStartMenu`'s
    /// ApplyDefaultCamera: false), done with the next frame.
    pub race_cams: Option<[Vec<crate::camstyle::RaceCamera>; 2]>,
    /// The personal light (`SetPersonalLight(actor, 20, (1, 0.69, 0.36))`).
    pub light: vi::tod::PersonalLight<vi::fpu::Ee>,
    cam_preset: crate::camstyle::Preset,
    cam_saved: crate::camstyle::Preset,
    cam_entered: bool,
    race_framed: bool,
    race_frame_due: Option<bool>,
    /// The weapon trails (`VITrailFx`'s pool) and this frame's strips.
    trails: vi::trail::Trails<Ee>,
    pub trail_strips: Vec<render::billboard::Strips>,
    /// The start menu (mode 13), when CLIENT's image gave its menus.
    pub start: Option<crate::startmenu::StartData>,
    /// `m_p2EC28`: Quit answered yes; the viewer leaves.
    pub quit: bool,
    /// Start-menu work waiting for the HUD's raster (the font): an open
    /// (with its reset) and keys (code, down).
    start_open: Option<bool>,
    /// `VIWndSpellBook` (mode 5): the book, whether it waits to be opened
    /// (with the HUD's raster), and its keys.
    pub book: crate::spellbook::SpellBook,
    book_open: bool,
    book_keys: Vec<(i32, bool)>,
    start_keys: Vec<(i32, bool)>,
    /// The start menu's item menu (`VIWndItemMenu`).
    items: crate::itemmenu::ItemMenu,
    /// `m_p13B90`: a destroy (0x003C) is waiting for the server's answer.
    waiting: bool,
    /// An emote's action for the player's own walker.
    emote: Option<i32>,
    /// The who list (mode 35): its data, its window, keys waiting for the
    /// raster, and a tell's addressee while the edit box is up for it.
    who: crate::wholist::Who,
    who_wnd: crate::wholist::WhoList,
    who_open: bool,
    who_keys: Vec<(i32, bool)>,
    /// Tradeskills (`VIClientPart3834C`) and their windows: the recipe book
    /// (mode 6) and the experiment (mode 37); the keys and what the part said
    /// for the windows, handled with the next HUD frame.
    craft: crate::craft::Craft,
    recipe_wnd: crate::craftwnd::RecipeWnd,
    exp_wnd: crate::craftwnd::ExperimentWnd,
    craft_keys: Vec<(i32, bool)>,
    craft_outs: Vec<crate::craft::CraftOut>,
    /// The quests (`m_questTitle`) and the journal (mode 22).
    quests: crate::questlog::Quests,
    quest_log: crate::questlog::QuestLog,
    quest_open: bool,
    quest_keys: Vec<(i32, bool)>,
    /// The stat trainer (mode 12): opened, 0x001D's refresh, keys.
    trainer: crate::trainer::Trainer,
    trainer_open: bool,
    trainer_refresh: bool,
    trainer_keys: Vec<(i32, bool)>,
    /// Class Mastery (`VIClientPart4EC58`) and its window (mode 40): opened,
    /// 0x1402s and keys, handled with the raster.
    cm: crate::classmastery::Part,
    cm_wnd: crate::classmastery::ClassMastery,
    cm_open: bool,
    cm_msgs: Vec<Vec<u8>>,
    cm_keys: Vec<(i32, bool)>,
    /// The faction standings (`VIClientPart4F078`) and their window
    /// (mode 41).
    factions: crate::faction::Factions,
    faction_wnd: crate::faction::FactionWnd,
    faction_open: bool,
    faction_keys: Vec<(i32, bool)>,
    who_tell: Option<String>,
    /// `VIGameOptions` and the Options window (mode 34); what it asked of
    /// the sound device, done in the next frame; a camera style to apply
    /// (`ApplyOptions`); the faction and loot filters as last sent
    /// (`m_p502D4`, `m_p502D8`), none since the world entry.
    pub options: crate::options::GameOptions,
    options_wnd: crate::options::WndOptions,
    sound_ops: Vec<crate::options::OptionsUp>,
    apply_camera: bool,
    filters_sent: Option<(bool, bool)>,
    /// The buddy list (VIClient's map, `VIWndBuddy`: mode 18, a request
    /// mode 17) and the ignore list (`VIClientPart38300`,
    /// `VIWndIgnoreList`: mode 26); a panel to open with the next HUD
    /// frame, and keys for whichever is up.
    buddies: crate::social::Buddies,
    buddy_wnd: crate::social::BuddyList,
    ignore: crate::social::Ignore,
    ignore_wnd: crate::social::IgnoreList,
    panel_open: bool,
    panel_keys: Vec<(i32, bool)>,
    /// The guild (`m_guild`), its commands and invitation (`m_p2EF24`), and
    /// `VIWndGuild` (modes 20, the invitation, and 21, the roster) with the
    /// mode it is to open in.
    pub guild: crate::guild::Guild,
    guild_cmds: crate::guild::Cmds,
    guild_wnd: crate::guild::WndGuild,
    guild_open: Option<i32>,
    guild_keys: Vec<(i32, bool)>,
    /// `VIWndEdit` (mode 2), and keys for it waiting for the raster.
    edit: crate::editbox::EditBox,
    edit_keys: Vec<(i32, bool)>,
    /// `EQOA_EDIT_TEXT`'s progress.
    pub edit_typed: usize,
    /// The spell effects (`VIScene::m_spellEffects`), loaded with the first
    /// frame; the casts heard this frame; each slot's entity as the effects
    /// last saw it; the player's own clock for its actions.
    pub spells: Option<crate::spells::Spells>,
    casts: Vec<crate::spells::Cast>,
    spell_ids: Vec<Option<u32>>,
    spell_rand: Rand,
    /// The corpse, merchant and trade windows (modes 1, 10, 15, 16).
    npcw: npcw::NpcWindows,
    /// `VIBankControl` and the bank window (`bankplay`)
    bank: bankplay::BankPlay,
    /// the gram client and the mail and tell windows (`mail.rs`)
    mail: mail::MailWindows,
    /// the blacksmith and the customer-service windows (`support`)
    support: support::SupportWindows,
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
            14 => {
                let mut cues = Vec::new();
                let answer = self.mbox.key(key, &mut cues);
                self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
                // VIWndGame::OnMessage 268 / 260: the invitation's answer
                match answer {
                    Some(crate::dialog::BoxAnswer::Yes(0, 0x7020000)) => self.accept_invite(),
                    Some(crate::dialog::BoxAnswer::No(0, 0x7020000)) => self.decline_invite(),
                    Some(crate::dialog::BoxAnswer::Yes(0, 0x50E0000)) => self.session.send_op_byte(0x0041, 5),
                    Some(crate::dialog::BoxAnswer::Yes(0, 0xA010000)) => self.do_action(0xA010000, 0),
                    Some(crate::dialog::BoxAnswer::Yes(0, 0xB051007)) => self.session.send(&crate::guild::Cmds::bare(2)),
                    Some(crate::dialog::BoxAnswer::Yes(0, 0xB051008)) => self.session.send(&crate::guild::Cmds::bare(3)),
                    Some(crate::dialog::BoxAnswer::Yes(0, 0xB0B1002)) => {
                        let to = String::from_utf16_lossy(&self.edit.text);
                        self.edit.title.clear();
                        self.send_report(&to);
                    }
                    _ => {}
                }
                if answer.is_some() {
                    self.mode = 0;
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
            self.info_up = true;
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
                    let mut weapons = [None; 3];
                    if let (Some(cs), Some(c)) = (&sprite, &e.creature) {
                        #[allow(clippy::needless_range_loop)]
                        for i in 0..3 {
                            let id = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
                            let node = cs.item_nodes[i];
                            if id == 0 || node < 0 {
                                continue;
                            }
                            weapons[i] = crate::trails::weapon_points(lib, id).map(|w| (node, w));
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
                    Some(Puppet { model: e.model, look, sprite, anims, rand, gpu, attached, name_at: None, slot4_at: None, actor_m: Matrix44::identity(), centre: Matrix44::identity(), weapons, trail: Default::default(), first: true })
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

/// `CalcSlotWorldTransform(slot)`'s translation: the slot's node under the
/// centre transform `m`; None when the sprite lacks the slot (the game then
/// takes the actor's own transform, no offset).
fn slot_point(cs: &CSprite<Ee>, slot: usize, m: &Matrix44<Ee>) -> Option<[f32; 3]> {
    let node = *cs.node_ids.get(slot)?;
    let n = cs.h.nodes.get(usize::try_from(node).ok()?)?;
    Some(Matrix44::mul(&n.world, m).mul_point(&Vect3::of(0.0, 0.0, 0.0)).to_array())
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
        let mut p = Play { session, player, dump, entities: Entities::default(), last_send: 0, puppets: (0..SLOTS).map(|_| None).collect(), cues: vec![crate::sound::Cue::Named("respawn", 0.4)], weather: Vec::new(), abilities: Default::default(), ability_token: 0, hud_menus: Default::default(), walks: (0..SLOTS).map(|_| None).collect(), self_model: None, self_puppet: None, chat: Default::default(), typing: None, target: u32::MAX, auto_attack: false, attack_due: 0, swung: 0, stats: Default::default(), effects: Default::default(), damage: Default::default(), self_at: None, words: None, ui_data: None, hud: None, target_info: None, target_title: String::new(), target_guild: String::new(), info_up: false, pet_id: u32::MAX, pet_name: String::new(), info_window: Default::default(), chat_pending: Vec::new(), wnd_chat: None, hud_last: 0, channel: 0, reply_name: String::new(), mode: 0, dialogs: Default::default(), priv_chat: Default::default(), response: Default::default(), pending_box: None, world_change: None, group: Default::default(), mbox: Default::default(), trails: Default::default(), trail_strips: Vec::new(), race_cams: None, light: Default::default(), cam_preset: Default::default(), cam_saved: Default::default(), cam_entered: false, race_framed: false, race_frame_due: None, death: None, fade: Default::default(), dead_casters: Vec::new(), death_ms: 0, start: None, quit: false, start_open: None, start_keys: Vec::new(), emote: None, edit: Default::default(), edit_keys: Vec::new(), who: Default::default(), who_wnd: Default::default(), who_open: false, who_keys: Vec::new(), who_tell: None, edit_typed: 0, spells: None, casts: Vec::new(), spell_ids: vec![None; SLOTS], spell_rand: Rand(1), book: Default::default(), book_open: false, book_keys: Vec::new(), items: Default::default(), waiting: false, quests: Default::default(), quest_log: Default::default(), quest_open: false, quest_keys: Vec::new(), trainer: Default::default(), trainer_open: false, trainer_refresh: false, trainer_keys: Vec::new(), cm: Default::default(), cm_wnd: Default::default(), cm_open: false, cm_msgs: Vec::new(), cm_keys: Vec::new(), factions: Default::default(), faction_wnd: Default::default(), faction_open: false, faction_keys: Vec::new(), npcw: Default::default(), options: crate::options::GameOptions { compact: std::env::var_os("EQOA_SOFT_KEYBOARD").is_none() as u8, ..Default::default() }, options_wnd: Default::default(), sound_ops: Vec::new(), apply_camera: false, filters_sent: None, buddies: Default::default(), buddy_wnd: crate::social::BuddyList::new(0), ignore: Default::default(), ignore_wnd: crate::social::IgnoreList::new(0), panel_open: false, panel_keys: Vec::new(), guild: Default::default(), guild_cmds: Default::default(), guild_wnd: Default::default(), guild_open: None, guild_keys: Vec::new(), craft: Default::default(), recipe_wnd: Default::default(), exp_wnd: Default::default(), craft_keys: Vec::new(), craft_outs: Vec::new(), mail: Default::default(), support: Default::default(), bank: Default::default() };
        p.take_abilities();
        p.npcw_world_entry();
        p
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
                Event::TargetInfo { disposition, title, guild, .. } => {
                    self.target_info = Some(disposition);
                    self.target_title = title;
                    self.target_guild = guild;
                }
                Event::ReplyName(n) => self.reply_name = n,
                Event::Dialog { text, .. } => self.show_dialog(&text),
                Event::DialogBox { id, text, options } => self.dialog_box(id, &text, &options),
                Event::Opcode { op: 0x005E, .. } => self.msg_005e(walker),
                Event::Teleport { world, pos, hpr, .. } => {
                    // Msg00B2: back to play, then Respawn.
                    self.mode = 0;
                    self.respawned(walker);
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
                    self.npcw_respawn();
                    self.bank_reset();
                }
                Event::LevelExp { level, exp } => {
                    if self.dump.player.level < level {
                        self.cues.push(crate::sound::Cue::Ui(16));
                    }
                    self.dump.player.level = level;
                    self.dump.player.exp = exp;
                }
                Event::GroupRoster { ids, names } => {
                    let me = self.self_id32();
                    self.group.roster(&ids, &names, me);
                }
                Event::GroupInvite { id, name, .. } => {
                    // Msg0773: refused at once while in a group
                    if self.group.in_group {
                        self.session.send_msg(0x0624, id, 2);
                    } else {
                        self.group.invites.push(crate::group::Invite { id, name: name.clone() });
                        if self.mode == 0 {
                            self.show_invite(&name);
                        }
                    }
                }
                Event::GroupInviteGone(id) => {
                    // Msg0774
                    if let Some(k) = self.group.invites.iter().position(|i| i.id == id) {
                        self.group.invites.remove(k);
                        self.show_word(635);
                    }
                }
                Event::GroupRemoved(sub) => {
                    // 0x069A (5 builds a line and shows nothing, as shipped)
                    if self.group.in_group {
                        match sub {
                            1..=4 => self.show_word(629 + sub as i32),
                            7 => self.show_word(631),
                            _ => {}
                        }
                        self.left_group();
                    }
                }
                Event::GroupDisbanded => {
                    if self.group.in_group {
                        self.show_word(634);
                        self.left_group();
                    }
                }
                Event::GroupNotice { sub, name } => {
                    // 0x069C
                    match sub {
                        3 => self.show_word(627),
                        1 => self.show_word_name(558, &name),
                        2 => self.show_word_name(628, &name),
                        4 => self.show_word_name(642, &name),
                        5 => self.show_word_name(1022, &name),
                        _ => {}
                    }
                }
                Event::State { channel: 0x40, data } => self.group.channel40(&data),
                Event::InvAdd(item) => {
                    // AddInvItem
                    let limit = item.item.max_stack;
                    crate::inventory::add_item(&mut self.dump.inventory, &item, limit, crate::inventory::CAPACITY);
                    self.npcw_item_added();
                }
                Event::InvTook { slot, sound, n } => {
                    // Msg003C
                    crate::inventory::took(&mut self.dump.inventory, slot as usize, n as i64);
                    self.npcw_item_took(sound);
                }
                Event::InvMove { from, to } => crate::inventory::op003d(&mut self.dump.inventory, from as usize, to as usize),
                Event::InvUse { slot, charge } => {
                    if let Some(c) = crate::inventory::use_charge(&mut self.dump.inventory, slot as usize, charge != 0) {
                        self.cues.push(crate::sound::Cue::Ui(c));
                    }
                }
                Event::InvEquip { i, slot } => {
                    if let Some(c) = crate::inventory::equip(&mut self.dump.inventory, i as usize, slot as i64) {
                        self.cues.push(crate::sound::Cue::Ui(c));
                    }
                }
                Event::InvUnequip(slot) => {
                    if let Some(c) = crate::inventory::take_off(&mut self.dump.inventory, slot as i64) {
                        self.cues.push(crate::sound::Cue::Ui(c));
                    }
                }
                Event::QuickSlot { slot, pos } => crate::inventory::quick_slot(&mut self.dump.inventory, slot as usize, pos),
                Event::ItemHp { id, hp } => {
                    // Msg0060: the hit points and sound 1
                    if let Some(k) = crate::inventory::find(&self.dump.inventory, id) {
                        crate::inventory::set_hp(&mut self.dump.inventory, k, hp as i64);
                    }
                    self.cues.push(crate::sound::Cue::Ui(1));
                }
                Event::Opcode { op: 0x0E02, data } => self.who.recv(&data),
                Event::Opcode { op: 0x007C, data } => self.quests.recv_add(&data),
                // VIClientPart3834C::Recv1102
                Event::Opcode { op: 0x1102, data } => {
                    let modes = crate::craft::Modes { recipe: self.mode == 6, experiment: self.mode == 37 };
                    let words = self.words.clone().unwrap_or_default();
                    let out = self.craft.recv(&data, modes, &words, self.session.expansion & net::session::EXPANSION_FLAGS);
                    self.craft_outs.extend(out);
                }
                Event::Opcode { op: 0x0D41, data } => self.mail_recv(&data),
                Event::Opcode { op: 0x1402, data } => self.cm_msgs.push(data),
                Event::Opcode { op: 0x0468, data } => {
                    if let Some(o) = self.guild.recv_0468(&data) {
                        self.guild_out(o);
                    }
                }
                Event::Opcode { op: 0x0579, data } => {
                    for o in crate::guild::Cmds::recv_0579(&data) {
                        self.guild_out(o);
                    }
                }
                Event::Opcode { op: 0x057A, data } => {
                    let idle = self.mode == 0;
                    let (inv, o) = self.guild_cmds.recv_057a(&data, &self.guild, idle);
                    match inv {
                        Some(crate::guild::Invited::Busy(m)) => {
                            let name = self.guild_cmds.name.clone();
                            self.show_word_name(650, &name);
                            self.session.send(&m);
                        }
                        Some(crate::guild::Invited::InGuild(m)) => self.session.send(&m),
                        // OpenGuild: mode 20, the box
                        Some(crate::guild::Invited::Show) => self.open_guild(20),
                        None => {}
                    }
                    if let Some(o) = o {
                        self.guild_out(o);
                    }
                }
                Event::Opcode { op: 0x07E2, data } => self.factions.recv(&data),
                // Msg001D: the points spent come off, and the trainer refreshes
                Event::Opcode { op: 0x001D, data } => {
                    let mut r = net::stream::Reader::new(&data);
                    let z = r.signed();
                    if !r.error {
                        self.dump.player.p40 -= z;
                        self.trainer_refresh = true;
                    }
                }
                Event::Opcode { op: 0x007D, data } => self.quests.recv_remove(&data),
                Event::Opcode { op: op @ 0x1003..=0x1006, data } => {
                    self.ignore.recv(op, &data);
                }
                Event::Opcode { op: op @ (0x03D5 | 0x03D6), data } => {
                    self.buddies.recv(op, &data);
                }
                // ShowMsg(590): the other's buddy list is full
                Event::Opcode { op: 0x03D7, .. } => self.show_word(590),
                Event::Opcode { op: 0x076F, data } => self.buddy_request(&data),
                Event::Opcode { op: 0x0770, data } => {
                    // Msg0770: word 557 with the name
                    let name = String::from_utf8_lossy(&net::stream::Reader::new(&data).string(24)).trim_end_matches('\0').to_string();
                    self.show_word_name(557, &name);
                }
                Event::Opcode { op: 0x0771, data } => {
                    // Msg0771: why an invitation went nowhere, by type
                    let mut r = net::stream::Reader::new(&data);
                    let kind = r.u8();
                    let name = String::from_utf8_lossy(&r.string(24)).trim_end_matches('\0').to_string();
                    let word = match kind {
                        2 => Some(558),
                        1 => Some(559),
                        5 => Some(560),
                        6 => Some(587),
                        7 => Some(589),
                        8 => Some(586),
                        9 => Some(592),
                        10 => Some(642),
                        // the switch leaves the word unset: stack garbage
                        _ => None,
                    };
                    if let Some(w) = word {
                        self.show_word_name(w, &name);
                    }
                }
                Event::Pet(id) => {
                    self.pet_id = id.unwrap_or(u32::MAX);
                    if id.is_none() {
                        self.pet_name.clear();
                    }
                }
                Event::AbilityAdd { token, flag, ability } => {
                    // Msg00B3
                    self.ability_token = token;
                    self.abilities.insert(ability.id, *ability);
                    if flag != 0 {
                        self.show_word(721);
                    }
                }
                Event::AbilitySlot { token, index, slot } => {
                    self.ability_token = token;
                    self.ability_slot(index as i64, slot as i64);
                }
                Event::AbilityMoveSlot { token, from, to } => {
                    // Msg00B5
                    self.ability_token = token;
                    let (from, mut to) = (from as i64, to as i64);
                    let mut found = None;
                    for (k, a) in self.abilities.iter_mut() {
                        if a.valid != 0 {
                            if a.slot == from {
                                found = Some(*k);
                            }
                            if from < a.slot {
                                a.slot -= 1;
                            }
                        }
                    }
                    if from < to {
                        to -= 1;
                    }
                    for a in self.abilities.values_mut() {
                        if a.valid != 0 && a.slot >= to {
                            a.slot += 1;
                        }
                    }
                    if let Some(a) = found.and_then(|k| self.abilities.get_mut(&k)) {
                        a.slot = to;
                    }
                }
                Event::AbilityMoveIndex { token, from, to } => {
                    // Msg00B6
                    self.ability_token = token;
                    let (from, to) = (from as i64, to as i64);
                    let mut found = None;
                    for (k, a) in self.abilities.iter_mut() {
                        if a.display_index == from {
                            found = Some(*k);
                        }
                        if from < a.display_index && a.display_index < 252 {
                            a.display_index -= 1;
                        }
                    }
                    for a in self.abilities.values_mut() {
                        if a.display_index >= to && a.display_index < 252 {
                            a.display_index += 1;
                        }
                    }
                    if let Some(a) = found.and_then(|k| self.abilities.get_mut(&k)) {
                        a.display_index = to;
                    }
                }
                Event::AbilityRemove { token, index } => {
                    // Msg00B8
                    self.ability_token = token;
                    if let Some(k) = self.abilities.iter().find(|(_, a)| a.display_index == index as i64).map(|(k, _)| *k) {
                        self.abilities.remove(&k);
                    }
                }
                Event::AbilityClearBar { token } => {
                    // Msg00FF
                    self.ability_token = token;
                    for a in self.abilities.values_mut().filter(|a| a.display_index < 252) {
                        a.valid = 0;
                        a.slot = -1;
                    }
                }
                Event::AbilityUsable { token, id, p14 } => {
                    // Msg00FD
                    self.ability_token = token;
                    if let Some(a) = self.abilities.get_mut(&(id as i64)) {
                        a.p14 = p14 as i64;
                        if p14 == 0 {
                            a.valid = 0;
                            a.slot = -1;
                        }
                    }
                }
                Event::AbilityLockout { index, token, lockout } => {
                    // Msg00CD
                    self.ability_token = token;
                    if let Some(a) = self.abilities.values_mut().find(|a| a.display_index == index as i64) {
                        a.lockout = lockout as i64;
                    }
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
                    // the same m_player.m_p30 the dump set (the breath gauge's)
                    self.dump.player.p30 = self.stats.p30;
                }
                Event::State { channel: 0x43, data } => net::dump::read_effects(&mut self.effects, &data),
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
                // CastingCombat (0x00B1) and Msg0776: an effect to play; one
                // the player cast itself (0x0776) it has already played.
                Event::Opcode { op, data } => {
                    if self.bank_opcode(op, &data) || self.npcw_opcode(op, &data) || self.support_opcode(op, &data) {
                    } else if op == 0x00DB {
                        self.damage_indication(&data, walker.cam.camera_mode);
                    } else if let Some(c) = crate::spells::cast_of(op, &data)
                        && !(op == 0x0776 && Some(c.caster) == self.self_id32())
                    {
                        self.casts.push(c);
                    }
                    }
                Event::Weather { weather, instant } => {
                    eprintln!("play: weather {weather}{}", if instant { " (instant)" } else { "" });
                    self.weather.push((weather, instant));
                }
                Event::WorldEntry { self_id, player, dump } => {
                    eprintln!("play: world entry again as {self_id:#x} at {:?}", player.pos);
                    self.filters_sent = None;
                    self.player = player;
                    self.dump = dump;
                    self.take_abilities();
                    self.respawned(walker);
                    if self.cam_entered {
                        let rc = self.race_camera();
                        crate::camstyle::set_style(&mut walker.cam, self.cam_saved.mode, rc, &mut walker.scene);
                    }
                    self.npcw_world_entry();
                    self.bank_reset();
                    self.mail_connect();
                    self.support_connect();
                    self.pet_id = u32::MAX;
                    self.pet_name.clear();
                    // Msg000D's PlayCue
                    self.cues.push(crate::sound::Cue::Named("respawn", 0.4));
                }
                _ => {}
            }
        }
        if self.has_self() {
            self.auto_attack_tick(walker, now);
            self.options_tick();
        }
        // what the Options window asked of the sound device and the camera
        for op in std::mem::take(&mut self.sound_ops) {
            let Some(s) = sound.as_deref_mut() else { continue };
            match op {
                crate::options::OptionsUp::Volume(cls, v) => s.dev.set_sound_class_volume(cls, v.0),
                crate::options::OptionsUp::Output(stereo) => s.dev.set_output_mode(stereo as i32),
                crate::options::OptionsUp::Preview(t) => s.preview = Some(t),
                crate::options::OptionsUp::PreviewEnd => s.end_preview(),
                crate::options::OptionsUp::Apply => {}
            }
        }
        if let Some(s) = sound.as_deref_mut() {
            s.battle_option = self.options.battle as i32;
        }
        // the options saved: ToggleCameraStyle when the follow option and the
        // style disagree
        if std::mem::take(&mut self.apply_camera) {
            let follow = self.options.follow != 0;
            let mode = walker.cam.camera_mode;
            if (mode == 2 && !follow) || (mode == 3 && follow) {
                let rc = self.race_camera();
                self.cam_saved = crate::camstyle::toggle(&mut walker.cam, follow, rc, &mut walker.scene);
            }
        }
        // PlayEmote's action on the player's own sprite
        if let Some(a) = self.emote.take() {
            walker.set_action(a);
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
        } else if self.mode == 0
            && let Some(i) = self.group.invites.first()
        {
            let name = i.name.clone();
            self.show_invite(&name);
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
                            // UpdateCreatures: state 14 is death (laid down at
                            // once for a corpse seen first), its spells ended
                            _ if c.state == 14 => {
                                if !cs.dead {
                                    cs.set_death(true, p.first, &p.anims, &mut p.rand, None);
                                    self.dead_casters.push(slot as i32);
                                }
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
                    p.first = false;
                    let m = cs.h.calc_trans_about_center(&actor, &p.anims);
                    crate::trails::process(&mut self.trails, &mut p.trail, cs, &p.weapons, &m, game_ms);
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
                    p.slot4_at = slot_point(cs, 4, &m);
                    (m.to_cols(), cs.h.blend.iter().map(|m| m.to_cols()).collect::<Vec<_>>())
                }
                None => (actor.to_cols(), Vec::new()),
            };
            p.actor_m = actor;
            p.centre = glam_to_matrix(matrix);
            g.renderer.set_instances(g.queue, &p.gpu, &[matrix]);
            if !palette.is_empty() {
                g.renderer.set_palette(g.queue, &p.gpu, &palette);
            }
        }
        self.spells_frame(walker, lib, game_ms, sound);
        // the player's own swing, on its walker's pose
        if let (Some(p), Some(cs)) = (self.self_puppet.as_mut(), walker.sprite.as_mut()) {
            let actor = Matrix44::transform(&Vect3::new(walker.cam.walk.phys.angles.x, F::ZERO, F::ZERO), F::ONE, &walker.cam.walk.phys.smooth_location());
            let centre = cs.h.calc_trans_about_center(&actor, &walker.anims);
            crate::trails::process(&mut self.trails, &mut p.trail, cs, &p.weapons, &centre, game_ms);
            self.self_at = Some((walker.cam.walk.phys.smooth_location().to_array(), slot_point(cs, 4, &centre)));
        } else {
            self.self_at = None;
        }
        self.trail_strips = crate::trails::render(&mut self.trails, game_ms);
        // The camera style: the first entry's ToggleCameraStyle; the start
        // menu's ApplyRaceCamera while it is up, ApplyDefaultCamera after
        let rc = self.race_camera();
        if !self.cam_entered && self.has_self() {
            self.cam_entered = true;
            self.cam_saved = crate::camstyle::toggle(&mut walker.cam, self.options.follow != 0, rc, &mut walker.scene);
        }
        // Every OpenStartMenu frames (a return from the spell book or the
        // trainer too) and every CloseStartMenu restores, an entry's window
        // included (its message closes the menu first), so the windows open
        // from it are drawn over the play camera. A preset is applied only
        // once one was saved (no race table, none).
        match self.race_frame_due.take() {
            Some(true) => {
                if let Some(p) = crate::camstyle::apply_race(&mut walker.cam, rc, &mut walker.scene) {
                    self.cam_preset = p;
                    self.race_framed = true;
                }
            }
            Some(false) if self.race_framed => crate::camstyle::apply(&mut walker.cam, &self.cam_preset),
            _ => {}
        }
        // DeathCamera: the eye swung round the body, the fade at 4 s
        if let Some(d) = self.death.as_mut() {
            let dt = ((game_ms - self.death_ms).clamp(0, 1000)) as f32 * 0.001;
            let body = walker.cam.walk.phys.smooth_location().to_array();
            let (eye, dir, fade) = d.step(body, dt);
            walker.eye.position = eye;
            walker.eye.direction = dir;
            if fade {
                self.fade.set(1.0, 4.0);
            }
        }
        self.death_ms = game_ms;
        new_model
    }

    /// `VIScene`'s spell effects, a frame: the creatures as their actors (the
    /// player as `SELF`), an entity gone told to them, this frame's casts
    /// played with their entities named as `CastingCombat` names them, then
    /// every playback run (`VISpellEffectSystem::Process`).
    fn spells_frame(&mut self, walker: &mut Walker, lib: &Library, game_ms: i64, sound: Option<&mut crate::sound::Sound>) {
        use crate::spells::{Body, SELF, Spells};
        let spells = self.spells.get_or_insert_with(|| Spells::load(lib, game_ms));
        let me = self.session.self_id.map(|s| s as u32);
        let mut bodies: Vec<Body> = Vec::new();
        for (slot, (e, p)) in self.entities.slots.iter().zip(self.puppets.iter_mut()).enumerate() {
            let Some(e) = e.as_ref().filter(|e| Some(e.id) != me) else { continue };
            match p {
                Some(p) => bodies.push(Body { actor: slot as i32, sprite: p.sprite.as_mut().map(|cs| (cs, &p.anims, &mut p.rand)), transform: p.actor_m, centre: p.centre, attack: [0, 0] }),
                None => {
                    let m = Matrix44::transform(&v3(e.hpr), F::ONE, &v3(e.pos));
                    bodies.push(Body { actor: slot as i32, sprite: None, transform: m, centre: m, attack: [0, 0] });
                }
            }
        }
        // the player: its walker's body and the attack types in its hands
        let mut attack = [0, 0];
        for it in &self.dump.inventory.items {
            match it.equip_slot {
                14 => attack[0] = it.item.attack_type as i32,
                15 => attack[1] = it.item.attack_type as i32,
                _ => {}
            }
        }
        let actor = Matrix44::transform(&Vect3::new(walker.cam.walk.phys.angles.x, F::ZERO, F::ZERO), F::ONE, &walker.cam.walk.phys.smooth_location());
        let has_self = me.is_some_and(|me| self.entities.slots.iter().flatten().any(|e| e.id == me));
        if has_self {
            let anims = &walker.anims;
            let (sprite, centre) = match walker.sprite.as_mut() {
                Some(cs) => {
                    let c = cs.h.calc_trans_about_center(&actor, anims);
                    (Some((cs, anims, &mut self.spell_rand)), c)
                }
                None => (None, actor),
            };
            bodies.push(Body { actor: SELF, sprite, transform: actor, centre, attack });
        }
        // a death ends the spells its actor was casting
        for a in std::mem::take(&mut self.dead_casters) {
            spells.stop_caster(a, &mut bodies, game_ms);
        }
        // NotifyActorWasDestroyed: a slot's entity gone or replaced
        for slot in 0..SLOTS {
            let now = self.entities.slots[slot].as_ref().map(|e| e.id).filter(|&id| Some(id) != me);
            if self.spell_ids[slot].is_some() && self.spell_ids[slot] != now {
                spells.actor_gone(slot as i32, &mut bodies, game_ms);
            }
            self.spell_ids[slot] = now;
        }
        let actor_of = |id: u32| -> i32 {
            if id == u32::MAX {
                -1
            } else if Some(id) == me {
                if has_self { SELF } else { -1 }
            } else {
                self.entities.slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id)).map_or(-1, |s| s as i32)
            }
        };
        let casts: Vec<_> = std::mem::take(&mut self.casts)
            .into_iter()
            .map(|c| {
                let (a, t) = (actor_of(c.caster), actor_of(c.target));
                let ts: Vec<i32> = c.targets.iter().map(|&id| actor_of(id)).filter(|&a| a != -1).take(8).collect();
                (c, a, t, ts)
            })
            .collect();
        spells.frame(&mut bodies, &casts, if has_self { SELF } else { -1 }, Some(&mut walker.cam.effect), game_ms, &walker.eye, lib, sound);
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
    /// As 0x007E would: the creature named `name` becomes the player's pet
    /// (a test hook; no server here sends 0x007E).
    pub fn pet_by_name(&mut self, name: &str) -> bool {
        let Some(id) = self.entities.slots.iter().flatten().find(|e| e.creature.as_ref().is_some_and(|c| c.name.trim() == name.trim())).map(|e| e.id) else {
            return false;
        };
        self.pet_id = id;
        self.pet_name.clear();
        true
    }

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
        let toggle = self.options.auto_attack != 0;
        if (!toggle || !self.auto_attack) && (self.attack_due == 0 || (self.attack_due.wrapping_sub(now) as i32) <= 0) {
            self.attack_due = now;
            let mut w = net::stream::Writer::op(0x000F);
            w.u32(self.target);
            self.session.send(&w.data);
        }
        // the Auto Attack option makes the button a toggle
        self.auto_attack = !toggle || !self.auto_attack;
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
        if std::env::var_os("EQOA_CHAT_LOG").is_some() {
            eprintln!("chat: {text}");
        }
        self.chat_pending.push((text.clone(), color, self.session.now()));
        self.chat.push_back((text, color));
        while self.chat.len() > 8 {
            self.chat.pop_front();
        }
    }

    /// `Msg005E`: the player died. The death pose, its spells ended, the
    /// cue, back to play with auto-attack off, every invitation refused (3),
    /// and `ResetCamera`: the death camera from where the camera is.
    fn msg_005e(&mut self, walker: &mut Walker) {
        if let Some(cs) = walker.sprite.as_mut() {
            cs.set_death(true, false, &walker.anims, &mut self.spell_rand, None);
            self.dead_casters.push(crate::spells::SELF);
        }
        self.mode = 0;
        self.cues.push(crate::sound::Cue::Named("death", 0.4));
        self.auto_attack = false;
        for i in std::mem::take(&mut self.group.invites) {
            self.session.send_msg(0x0624, i.id, 3);
        }
        let body = walker.cam.walk.phys.smooth_location().to_array();
        self.death = Some(crate::death::DeathCam::new(walker.eye.position.to_array(), walker.eye.direction.to_array(), body));
    }

    /// A respawn (0x00B2, or the dump again): the death camera off, the body
    /// up, the cue and the fade back in over a second.
    fn respawned(&mut self, walker: &mut Walker) {
        if self.death.take().is_some() {
            if let Some(cs) = walker.sprite.as_mut() {
                cs.dead = false;
                cs.set_locomotion(0, &walker.anims, &mut self.spell_rand);
            }
            self.cues.push(crate::sound::Cue::Named("respawn", 0.4));
            self.fade.set(1.0, 0.0);
            self.fade.set(0.0, 1.0);
        }
    }

    /// `GetRaceCamera(GetRace(), ExpansionFlags())`.
    fn race_camera(&self) -> Option<crate::camstyle::RaceCamera> {
        let t = self.race_cams.as_ref()?;
        crate::camstyle::race_camera(t, self.dump.player.race as i32, self.session.expansion & net::session::EXPANSION_FLAGS)
    }

    /// The frame's point light: the personal light 3 above the player
    /// (`SetPersonalLight(int actor, ...)`: the actor's up axis × 3), its
    /// radius stepped (`SetupTimeOfDayLights`) at `hour` under the weather.
    pub fn point_light(&mut self, walker: &Walker, now: i64, hour: f32, weather: Option<&crate::weather::Weather>) -> Option<(glam::Vec3, f32, glam::Vec3)> {
        use vi::fpu::F;
        let p = walker.cam.walk.phys.position;
        self.light.at = vi::vect::Vect3::new(p.x, p.y + F::new(3.0), p.z);
        let (rain, o, out) = weather.map_or((F::ZERO, F::ZERO, F::ONE), |w| (w.atm.intensity, w.atm.overcast_interp(), w.outdoors.intensity));
        let (at, r, c) = self.light.step(now, F::new(hour), rain, out, o)?;
        Some((glam::Vec3::new(at.x.0, at.y.0, at.z.0), r.0, glam::Vec3::new(c[0].0, c[1].0, c[2].0)))
    }

    /// The P key in play (`InitCameraStyle`): first person and back.
    pub fn camera_style_key(&mut self, walker: &mut Walker) {
        if self.mode == 0 {
            let rc = self.race_camera();
            self.cam_saved = crate::camstyle::cycle(&mut walker.cam, self.options.follow != 0, rc, &mut walker.scene);
        }
    }

    /// Dead: the walk takes no controls (`UpdateControls` is not run).
    pub fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    /// The dump's abilities, by id, and its token; its quests.
    fn take_abilities(&mut self) {
        self.abilities = self.dump.abilities.iter().map(|a| (a.id, a.clone())).collect();
        self.ability_token = self.dump.p2bb60;
        // the dump's crafting token and slots (SetP78C0, ReadSlots)
        self.craft.token = self.dump.craft_p78c0;
        self.craft.slots = self.dump.craft_slots;
        // the dump's quests (m_questCount, m_questTitle) and their stamp
        // ReadState: the MemoryDump's Class Mastery
        self.cm.m = self.dump.mastery.clone();
        self.quests = crate::questlog::Quests { titles: self.dump.quests.iter().map(|q| crate::ui::wide(q)).collect(), stamp: self.dump.p2bb0c };
    }

    /// A word with a name in its `%s` to the chat log.
    fn show_word_name(&mut self, id: i32, name: &str) {
        let fmt = self.words.as_ref().map_or_else(|| format!("(word {id}) %s"), |w| w.get(id, 0).to_string());
        self.add_chat(fmt.replacen("%s", name, 1).replacen("%S", name, 1), [1.0; 4]);
    }

    /// `VIWndGame::ShowInvite`: word 636 with the name, as a Yes/No prompt
    /// whose answer is 0x7020000.
    fn show_invite(&mut self, name: &str) {
        let fmt = self.words.as_ref().map_or_else(|| "%s".to_string(), |w| w.get(636, 0).to_string());
        let text = crate::ui::wide(&fmt.replacen("%s", name, 1).replacen("%S", name, 1));
        let now = self.session.now() as i64;
        self.mbox.prompt(&text, 1, 0, 0x7020000, now);
        self.mode = 14;
    }

    /// `LeaveGroup`/`DisbandGroup`'s and 0x069A's common tail: out of the
    /// group, and plain text back to say if it went to the group
    /// (`GroupChanged(0)`).
    fn left_group(&mut self) {
        self.group.clear();
        if self.channel == 2 {
            self.group_changed(0);
        }
    }

    /// `GroupChanged`: the channel plain text goes to (0x000E), and its word.
    fn group_changed(&mut self, state: u8) {
        self.channel = state;
        self.session.send_op_byte(0x000E, state);
        if state <= 4 {
            self.show_word(595 + state as i32);
        }
    }

    /// `Reset`: every other invitation refused (0x0624, 2).
    fn refuse_invites(&mut self) {
        for i in std::mem::take(&mut self.group.invites) {
            self.session.send_msg(0x0624, i.id, 2);
        }
    }

    /// `AcceptInvite`: the first invitation, word 622, 0x0622; in the group,
    /// not leading it, the others refused.
    fn accept_invite(&mut self) {
        let Some(i) = self.group.invites.first().cloned() else {
            self.show_word(624);
            return;
        };
        self.show_word_name(622, &i.name);
        self.group.drop_invites(i.id);
        self.session.send_op_int(0x0622, i.id);
        self.group.in_group = true;
        self.group.leader = false;
        self.refuse_invites();
    }

    /// `DeclineInvite`: the first invitation, word 623, 0x0624 with 1.
    fn decline_invite(&mut self) {
        let Some(i) = self.group.invites.first().cloned() else {
            self.show_word(624);
            return;
        };
        self.show_word_name(623, &i.name);
        self.group.drop_invites(i.id);
        self.session.send_msg(0x0624, i.id, 1);
    }

    /// What the start menu's stats column and `UpdateLocation` read.
    fn start_view(&self, walker: &Walker) -> (crate::startmenu::Stats, crate::startmenu::Location) {
        let st = crate::startmenu::Stats {
            name: self.dump.player.name.clone(),
            class: self.dump.player.class as i32,
            level: self.dump.player.level as i32,
            hp: self.stats.stat[7],
            max_hp: self.stats.stat[8],
            power: self.stats.stat[9],
            max_power: self.stats.stat[10],
            ac: self.stats.stat[14],
            tunar: self.dump.player.p34 as i32,
            effects: (0..self.effects.count.max(0) as usize).map(|i| self.effects.name[i].clone()).collect(),
            expansion: net::session::EXPANSION_FLAGS,
        };
        let g = walker.scene.grid.as_ref();
        let p = walker.cam.walk.committed.position;
        let world = self.dump.player.world as i32;
        let loc = crate::startmenu::Location {
            world,
            pos: [p.x.0, p.y.0, p.z.0],
            cols: g.map_or(1, |g| g.cols),
            rows: g.map_or(1, |g| g.rows),
            cell: g.map_or(1.0, |g| g.cell_size.0),
            origin: g.map_or([0.0; 3], |g| [g.origin.x.0, g.origin.y.0, g.origin.z.0]),
            bounds: self.start.as_ref().and_then(|s| s.bounds.get(world as usize).copied().flatten()),
        };
        (st, loc)
    }

    /// `OpenStartMenu(a)` (done with the next HUD frame): the table for the
    /// bank and expansion, mode 13, auto-attack off, the main menu shown
    /// (`a` resets it).
    pub fn open_start_menu(&mut self, reset: bool) {
        if self.start.is_some() {
            self.start_open = Some(reset);
            self.mode = 13;
            self.auto_attack = false;
            self.race_frame_due = Some(true);
        }
    }

    /// A key for the start menu (the game's codes, `startmenu::keys`), down
    /// or up, handled with the next HUD frame.
    pub fn start_key(&mut self, key: i32, down: bool) {
        self.start_keys.push((key, down));
    }

    /// `UseItem(i, b)`: 0x003E (62) with the item's inventory id, the target
    /// and `b`.
    fn use_item(&mut self, i: i32, b: u8) {
        let Some(it) = self.dump.inventory.items.get(i.max(0) as usize).filter(|_| i >= 0) else { return };
        let mut w = net::stream::Writer::op(62);
        w.u32(it.inv_index).u32(self.target).u8(b);
        self.session.send(&w.data);
    }

    /// What the item menu asked for: `UseItemOn` (0x003C, one at a time),
    /// `SwapItems` (61), `RequestQuickSlot` (`SendMsg(10, ...)`).
    fn item_outs(&mut self, outs: Vec<crate::itemmenu::ItemOut>) {
        use crate::itemmenu::ItemOut;
        let id = |p: &Play, i: i32| p.dump.inventory.items.get(i.max(0) as usize).filter(|_| i >= 0).map(|it| it.inv_index);
        for o in outs {
            match o {
                ItemOut::UseItem(i, b) => self.use_item(i, b),
                ItemOut::UseItemOn(i, n) => {
                    if let (false, Some(slot)) = (self.waiting, id(self, i)) {
                        self.waiting = true;
                        let mut w = net::stream::Writer::op(0x3C);
                        w.u32(slot).u32(n as u32);
                        self.session.send(&w.data);
                    }
                }
                ItemOut::Swap(a, b) => {
                    if let (Some(a), Some(b)) = (id(self, a), id(self, b)) {
                        let mut w = net::stream::Writer::op(61);
                        w.u32(a).u32(b);
                        self.session.send(&w.data);
                    }
                }
                ItemOut::QuickSlot(i, pos) => {
                    if let Some(slot) = id(self, i) {
                        self.session.send_msg(10, slot, pos as u8);
                    }
                }
                ItemOut::Back => {}
            }
        }
    }

    /// `CloseStartMenu(mode)`.
    fn close_start_menu(&mut self, mode: i32) {
        self.race_frame_due = Some(false);
        self.mode = mode;
        if let Some(s) = self.start.as_mut() {
            s.menu.visible = false;
        }
    }

    /// The start menu's waiting work, with the raster: the open, then each
    /// key to the focused menu; 260 from it closes it, 261 is
    /// `VIWndGame::OnMessage`'s mode-13 path.
    fn run_start_menu(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, loc: &crate::startmenu::Location) -> Vec<(u32, i32)> {
        let now = self.session.now() as i64;
        let expansion = self.session.expansion;
        // OpenStartMenu: the bank's tables when the bank has the auction
        // (GetBank()->GetP78CC())
        let bank_tables = self.dump.bank.p78cc;
        let mut actions = Vec::new();
        let Some(s) = self.start.as_mut() else { return actions };
        let attack_words = s.attack_words.clone();
        // UpdateQuickBars' item menus, every frame
        let inv = &self.dump.inventory;
        let pl = &self.dump.player;
        let flags = net::session::EXPANSION_FLAGS;
        self.items.set_built(crate::itemmenu::build(
            inv,
            s.item_counts,
            &|k| crate::inventory::item_icon(&inv.items[k], flags) as i32,
            &|k| crate::inventory::can_use(&inv.items[k], pl.class, pl.race, pl.level),
        ));
        let info = crate::infotext::Info {
            words: &vui.words,
            inv,
            hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
            base: &self.stats.base,
            class: pl.class,
            level: pl.level,
            attack_words: &attack_words,
        };
        let mut outs = Vec::new();
        if let Some(reset) = self.start_open.take() {
            s.pick(bank_tables, expansion);
            s.menu.visible = true;
            s.menu.show_main(vui, r, reset, now, loc);
        }
        let mut cues = Vec::new();
        for (key, down) in std::mem::take(&mut self.start_keys) {
            if !s.menu.visible {
                continue;
            }
            if self.items.visible {
                let mut o = Vec::new();
                self.items.key(vui, r, key, down, inv, &info, self.waiting, &mut cues, &mut o);
                if o.contains(&crate::itemmenu::ItemOut::Back) {
                    // VIWndStartMenu::OnMessage 260 from the items: ShowMain(0)
                    self.items.visible = false;
                    s.menu.show_main(vui, r, false, now, loc);
                }
                outs.extend(o);
                continue;
            }
            match s.menu.key(vui, r, &s.menus, key, down, now, loc, &mut cues) {
                crate::startmenu::Up::Items => {
                    // ShowItems
                    self.items.open(vui, r, now);
                    self.waiting = false;
                }
                crate::startmenu::Up::Back => {
                    s.menu.visible = false;
                    actions.push((0, 0));
                }
                crate::startmenu::Up::Action(msg, id) => {
                    s.menu.visible = false;
                    actions.push((msg, id));
                }
                _ => {}
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        self.item_outs(outs);
        actions
    }

    /// A key for the spell book (the game's codes), down or up.
    pub fn book_key(&mut self, key: i32, down: bool) {
        self.book_keys.push((key, down));
    }

    /// What the spell book asked for.
    fn book_actions(&mut self, out: Vec<crate::spellbook::Out>) {
        use crate::spellbook::Out;
        for o in out {
            match o {
                Out::Sound(n) => self.cues.push(crate::sound::Cue::Ui(n)),
                Out::Send0033(index, slot) => {
                    let mut w = net::stream::Writer::op(0x0033);
                    w.u32(self.ability_token).u8(index as u8).u8(slot as u8);
                    self.session.send(&w.data);
                }
                Out::Send003A(from, to) => {
                    // only to a display index the list has
                    if self.abilities.values().any(|a| a.display_index == to) {
                        let mut w = net::stream::Writer::op(0x003A);
                        w.u32(self.ability_token).u8(from as u8).u8(to as u8);
                        self.session.send(&w.data);
                    }
                }
                Out::Send00FE => {
                    let mut w = net::stream::Writer::op(0x00FE);
                    w.u32(self.ability_token);
                    self.session.send(&w.data);
                }
                Out::Word(id) => self.show_word(id),
                Out::Red(id) => self.warn(id),
                Out::Close => {
                    // VIWndGame::OnMessage 260 from the book: the start menu again
                    self.book.visible = false;
                    self.mode = 0;
                    self.open_start_menu(false);
                }
            }
        }
    }

    /// What came out of the start menu: (0, 0) closed it, else an entry's
    /// message.
    fn start_actions(&mut self, actions: Vec<(u32, i32)>) {
        let now = self.session.now() as i64;
        for (msg, id) in actions {
            self.close_start_menu(0);
            match msg {
                0 => {}
                0x0801_0000 => {
                    // the spell book (mode 5)
                    self.mode = 5;
                    self.book_open = true;
                }
                0x0A01_0000 => {
                    // Quit: word 722 as a Yes/No prompt
                    let text = self.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(722, 0)));
                    self.mbox.prompt(&text, 1, 0, 0xA010000, now);
                    self.mode = 14;
                }
                // the edit box: a name (the target's) or a line
                // OpenPanel34 (the battle music's Unload is the sound's own),
                // OpenPanel18 and m_p38300.OpenPanel: the windows
                0x0B0D_0002 => self.open_panel(34),
                0x0B05_0000 => self.open_panel(18),
                0x0B05_3000 => self.open_panel(26),
                0x0B05_3001 => self.open_edit(msg, 24, 67, true),
                0x0B05_3002 => self.open_edit(msg, 24, 613, true),
                0x0B04_0000 => self.open_edit(msg, 24, 86, true),
                0x0B0B_1002 => self.open_edit(msg, 24, 1470, true),
                0x0301_0000 => {
                    self.open_edit(0xB0F0000, 128, -1, false);
                    self.edit.title = self.chat_title();
                }
                // OpenPanel40: the list asked for, the window (mode 40)
                0x0208_0000 => {
                    if let Some(m) = self.cm.open() {
                        self.session.send(&m);
                    }
                    self.cm_open = true;
                    self.mode = 40;
                }
                // VIClientPart4F078::Command -> OpenFaction: page 0, mode 41
                0xAA00_0001 => {
                    self.factions.reset();
                    let m = self.factions.request(0);
                    self.session.send(&m);
                    self.faction_open = true;
                    self.mode = 41;
                }
                // OpenTrainer: mode 12
                0x0302_0000 => {
                    self.trainer_open = true;
                    self.mode = 12;
                }
                // VIClientPart3834C::Use: the recipe book of a skill (its
                // list asked for) or the experiment
                m if crate::craft::Craft::is_mine(m) => {
                    let inv = self.dump.inventory.clone();
                    let out = self.craft.use_id(m, id, &inv);
                    self.craft_outs.extend(out);
                }
                // OpenQuestLog: mode 22
                0x0B0A_0000 => {
                    self.quest_open = true;
                    self.mode = 22;
                }
                0x0B05_1000..=0x0B05_100C if msg != 0x0B05_1001 => self.guild_action(msg, id),
                // Mail > Compose and Inbox, Tell and Reply
                0x0B0B_0000 => self.open_mail(23),
                0x0B0B_0002 => self.open_mail(24),
                0x0B0D_0000 => self.open_mail(28),
                0x0B0D_0001 => self.open_mail(29),
                // Support: the CS request form (31) and inbox (32)
                0x0B0B_1000 => self.open_support(31),
                0x0B0B_1001 => self.open_support(32),
                _ => self.do_action(msg, id),
            }
        }
    }

    /// 0x00DB: the attacker, the amount (zigzag) and the defender.
    /// `DamageFromSelf` when the player hit: over the defender; else
    /// `DamageFromOther`: over the player, only from a walk camera not in
    /// first person (`m_cameraMode` 0) and no manual camera (mode 38,
    /// `IsModal`). Both off under the options' `m_x92`.
    fn damage_indication(&mut self, data: &[u8], camera_mode: i32) {
        let mut r = net::stream::Reader::new(data);
        let (attacker, amount, defender) = (r.u32(), r.signed() as i32, r.u32());
        if r.error || self.options.no_damage != 0 {
            return;
        }
        let me = self.self_id32();
        if Some(attacker) == me {
            self.damage.add(defender, amount, 0);
        } else if camera_mode != 0 && self.mode != 38
            && let Some(me) = me
        {
            self.damage.add(me, amount, 1);
        }
    }

    /// A 0x00DB as if the server had sent it (`EQOA_DAMAGE` in a playshot):
    /// `self` and `target` name the player and its target, anything else is
    /// an id in hex.
    pub fn fake_damage(&mut self, attacker: &str, amount: i64, defender: &str, camera_mode: i32) {
        let id = |s: &str| match s {
            "self" => self.self_id32().unwrap_or(u32::MAX),
            "target" => self.target,
            _ => u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(u32::MAX),
        };
        let mut w = net::stream::Writer::default();
        w.u32(id(attacker)).signed(amount).u32(id(defender));
        self.damage_indication(&w.data, camera_mode);
    }

    /// Effects as if state channel 0x43 had brought them
    /// (`EQOA_EFFECTS=<icon>:<name>|...` in a playshot, the icon a DictID in
    /// hex): the channel's value built and read.
    pub fn fake_effects(&mut self, spec: &str) {
        let parts: Vec<(u32, &str)> = spec.split('|').filter_map(|p| p.split_once(':')).filter_map(|(i, n)| Some((u32::from_str_radix(i.trim_start_matches("0x"), 16).ok()?, n))).collect();
        let mut v = (parts.len() as u32).to_le_bytes().to_vec();
        for (icon, name) in parts {
            let mut r = icon.to_le_bytes().to_vec();
            let w: Vec<u16> = name.encode_utf16().collect();
            r.extend((w.len() as u32).to_le_bytes());
            r.extend(w.iter().flat_map(|c| c.to_le_bytes()));
            r.resize(132, 0);
            v.extend(r);
        }
        net::dump::read_effects(&mut self.effects, &v);
    }

    /// Quests as if 0x007C had brought them (`EQOA_QUESTS=a|b|...` in a
    /// playshot).
    pub fn fake_quests(&mut self, spec: &str) {
        for q in spec.split('|') {
            self.quests.add(crate::ui::wide(q));
        }
    }

    /// A character window is up (the trainer, mode 12; the quest journal,
    /// mode 22).
    pub fn char_window(&self) -> bool {
        matches!(self.mode, 6 | 12 | 22 | 37 | 40 | 41)
    }

    /// A key for the character window up, handled with the next HUD frame.
    pub fn char_key(&mut self, key: i32, down: bool) {
        match self.mode {
            12 => self.trainer_keys.push((key, down)),
            40 => self.cm_keys.push((key, down)),
            41 => self.faction_keys.push((key, down)),
            22 => self.quest_keys.push((key, down)),
            6 | 37 => self.craft_keys.push((key, down)),
            _ => {}
        }
    }

    /// Class Mastery's replies, the window's open, keys and drawing, and
    /// what it asks.
    fn run_class_mastery(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        let mut cues = Vec::new();
        if std::mem::take(&mut self.cm_open) {
            self.cm_wnd.open(vui, r, &self.cm);
        }
        for data in std::mem::take(&mut self.cm_msgs) {
            match self.cm.recv(&data) {
                // Recv03: ShowNotice, sound 7
                Some(crate::classmastery::PartUp::Points(d)) => {
                    let text = self.words.as_ref().map_or_else(String::new, |w| if d > 1 { w.get(1627, 0).replace("%d", &d.to_string()) } else { w.get(1628, 0).to_string() });
                    self.add_chat(text, [1.0; 4]);
                    cues.push(7);
                }
                // ShowNode < 0: the node cleared
                Some(crate::classmastery::PartUp::Node) if !self.cm_wnd.show_node(vui, r, &self.cm, self.mode == 40) => self.cm.node = None,
                _ => {}
            }
        }
        let mut ups = Vec::new();
        for (k, down) in std::mem::take(&mut self.cm_keys) {
            if self.mode != 40 {
                break;
            }
            ups.extend(self.cm_wnd.key(vui, r, &mut self.cm, k, down, now, &mut cues));
        }
        if self.mode == 40 {
            self.cm_wnd.draw(vui, r, &self.cm, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                // ClosePanel6C0: Close, then CloseClassMastery
                crate::classmastery::CmUp::Close => {
                    self.cm.close();
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                crate::classmastery::CmUp::Send(m) => self.session.send(&m),
            }
        }
    }

    /// The faction window's open, keys, drawing and what it asks.
    fn run_factions(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        if std::mem::take(&mut self.faction_open) {
            self.faction_wnd.open(vui, r);
        }
        let mut ups = Vec::new();
        let mut cues = Vec::new();
        for (k, down) in std::mem::take(&mut self.faction_keys) {
            if self.mode != 41 {
                break;
            }
            ups.extend(self.faction_wnd.key(&mut self.factions, k, down, &mut cues));
        }
        if self.mode == 41 {
            self.faction_wnd.draw(vui, r, &self.factions, now, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                // CloseFaction: reset, back to the start menu
                crate::faction::FactionUp::Close => {
                    self.factions.reset();
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                crate::faction::FactionUp::Send(m) => self.session.send(&m),
            }
        }
    }

    /// The trainer's open, refresh, keys, drawing and what it asks.
    fn run_trainer(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        let empty = Vec::new();
        let slot_words = self.start.as_ref().map_or(&empty, |s| &s.slot_words);
        let pl = &self.dump.player;
        let d = crate::trainer::Data {
            name: &pl.name,
            class: pl.class as i32,
            race: pl.race as i32,
            level: pl.level as i32,
            base: &self.stats.base,
            cur: &self.stats.stat,
            p40: pl.p40 as i32,
            p44: pl.p44 as i32,
            mastery: &self.cm.m.stats,
            bonuses: &self.cm.m.bonuses,
            inv: &self.dump.inventory,
            slot_words,
        };
        if std::mem::take(&mut self.trainer_open) {
            self.trainer.open(vui, r, &d, now);
        }
        if std::mem::take(&mut self.trainer_refresh) {
            self.trainer.refresh(&d);
        }
        let mut ups = Vec::new();
        let mut cues = Vec::new();
        for (k, down) in std::mem::take(&mut self.trainer_keys) {
            if self.mode != 12 {
                break;
            }
            ups.extend(self.trainer.key(&d, k, down, now, &mut cues));
        }
        if self.mode == 12 {
            self.trainer.draw(vui, r, &d, now, &mut cues);
        }
        let p40 = d.p40;
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                // 260: CloseTrainer, back to the start menu
                crate::trainer::TrainUp::Close => {
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                crate::trainer::TrainUp::Train(points) => {
                    if let Some(m) = crate::trainer::train_stats(&points, p40) {
                        self.session.send(&m);
                    }
                }
            }
        }
    }

    /// The tradeskill windows: what the part said (requests, lines, windows
    /// to open, boxes), then the open window's keys, drawing and asks.
    fn run_craft(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        use crate::craft::CraftOut;
        use crate::craftwnd::CraftUp;
        let now = self.session.now() as i64;
        let flags = self.session.expansion & net::session::EXPANSION_FLAGS;
        let mut cues = Vec::new();
        let mut ups: Vec<CraftUp> = std::mem::take(&mut self.craft_outs).into_iter().map(CraftUp::Out).collect();
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let info = crate::infotext::Info {
            words: &vui.words,
            inv: &self.dump.inventory,
            hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
            base: &self.stats.base,
            class: pl.class,
            level: pl.level,
            attack_words: &attack_words,
        };
        let items = self.craft.items.clone();
        let icon = |i: usize| items.items.get(i).map_or(-1, |it| crate::inventory::item_icon(it, flags) as i32);
        let mut k = 0;
        // the part's outs, applied in turn (a window opened first)
        while k < ups.len() {
            if let CraftUp::Out(o) = ups[k].clone() {
                match o {
                    CraftOut::OpenRecipe(skill) => {
                        // VIWndGame::OpenRecipe: SetSkill, mode 6
                        self.recipe_wnd.skill = skill;
                        self.recipe_wnd.open(vui, r);
                        self.mode = 6;
                    }
                    CraftOut::OpenExperiment => {
                        self.exp_wnd.open(vui, r, &self.craft, &icon, now);
                        self.mode = 37;
                    }
                    CraftOut::RecipeMessage(text, _) => self.recipe_wnd.message(&text, now),
                    CraftOut::RecipeInspect => self.recipe_wnd.inspect(vui, r, &self.craft),
                    CraftOut::Notify(text, _) => self.exp_wnd.notify(&text, now),
                    CraftOut::Ask(text, p1, p2, _) => self.exp_wnd.ask(&text, p1, p2, now),
                    _ => {}
                }
            }
            k += 1;
        }
        for (key, down) in std::mem::take(&mut self.craft_keys) {
            match self.mode {
                6 => ups.extend(self.recipe_wnd.key(vui, &mut self.craft, key, down, now, &mut cues)),
                37 => ups.extend(self.exp_wnd.key(vui, r, &mut self.craft, &info, key, down, now, &mut cues)),
                _ => break,
            }
        }
        let name = self.dump.player.name.clone();
        match self.mode {
            6 => self.recipe_wnd.draw(vui, r, &self.craft, &name, now, &mut cues),
            37 => self.exp_wnd.draw(vui, r, &self.craft, &info, &icon, now, &mut cues),
            _ => {}
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                // ClosePanel708 / 70C: CloseRecipe / CloseExperiment, back to
                // the start menu
                CraftUp::Close => {
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                CraftUp::Out(CraftOut::Send(m)) => self.session.send(&m),
                CraftUp::Out(CraftOut::ShowWord(w)) => self.show_word(w),
                CraftUp::Out(CraftOut::Chat(t, c)) => self.add_chat(t, c),
                CraftUp::Out(_) => {}
            }
        }
    }

    /// The journal's open, keys, drawing and what it asks.
    fn run_quests(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        if std::mem::take(&mut self.quest_open) {
            self.quest_log.open(vui, r);
        }
        let mut ups = Vec::new();
        let mut cues = Vec::new();
        for (k, down) in std::mem::take(&mut self.quest_keys) {
            if self.mode != 22 {
                break;
            }
            ups.extend(self.quest_log.key(vui, &self.quests, k, down, now, &mut cues));
        }
        if self.mode == 22 {
            self.quest_log.draw(vui, r, &self.quests, now, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                // ClosePanel6DC: CloseQuestLog, back to the start menu
                crate::questlog::QuestUp::Close => {
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                crate::questlog::QuestUp::Drop(i) => {
                    let m = self.quests.send_remove(i);
                    self.session.send(&m);
                }
            }
        }
    }

    /// A key for the who list, handled with the next HUD frame.
    pub fn who_key(&mut self, key: i32, down: bool) {
        self.who_keys.push((key, down));
    }

    /// The who list's open, keys, drawing and what it asks.
    fn run_who(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        if std::mem::take(&mut self.who_open) {
            self.who_wnd.open(vui, r);
        }
        let mut ups = Vec::new();
        let mut cues = Vec::new();
        for (k, down) in std::mem::take(&mut self.who_keys) {
            if self.mode != 35 {
                break;
            }
            ups.extend(self.who_wnd.key(vui, r, &self.who, k, down, now, &mut cues));
        }
        if self.mode == 35 {
            self.who_wnd.draw(vui, r, &self.who, now, true, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                crate::wholist::WhoUp::Close => {
                    // ClosePanel704: CloseWhoList, back to the start menu
                    self.who.close();
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                crate::wholist::WhoUp::Refresh => {
                    let req = self.who.open();
                    self.session.send(&req);
                }
                crate::wholist::WhoUp::Tell(name) => {
                    // Tell: one row, 128 long, centred on the list
                    self.open_edit(0xB0F0000, 128, -1, false);
                    self.who_tell = Some(name);
                }
                crate::wholist::WhoUp::Invite(name, id) => self.invite_to_group(id, &name),
                crate::wholist::WhoUp::Mail(name) => eprintln!("play: mail to {name}: the gram window is not ported"),
            }
        }
    }

    /// `InviteToGroup(rec)`: as `InviteEntity`, by the record's id and name.
    fn invite_to_group(&mut self, id: u32, name: &str) {
        if Some(id) == self.self_id32() {
            self.show_word(679);
            return;
        }
        if self.group.in_group && !self.group.leader {
            self.show_word(640);
            return;
        }
        if self.group.members.iter().take(self.group.count()).any(|m| m.id == id) {
            self.show_word_name(1023, name);
            return;
        }
        if !self.group.in_group {
            self.group.leader = true;
            self.group.in_group = true;
        }
        self.show_word_name(637, name);
        let mut w = net::stream::Writer::op(0x0625);
        w.u32(id).string(name.as_bytes());
        self.session.send(&w.data);
        self.refuse_invites();
    }

    /// The edit box for `purpose`: `max` long, titled with a word (-1 none),
    /// holding the target's name when `name` and the target is a creature.
    fn open_edit(&mut self, purpose: u32, max: usize, title: i32, name: bool) {
        let target = if name { self.target_name().unwrap_or_default() } else { String::new() };
        let e = &mut self.edit;
        e.clear();
        e.set_max_length(max);
        e.set_text(&crate::ui::wide(&target));
        e.title = if title >= 0 { self.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(title, 0))) } else { Vec::new() };
        e.purpose = purpose;
        // gKeyboardAttached: the host has one, so the option decides
        e.pad_only = self.options.compact != 0;
        e.focus(self.session.now() as i64);
        self.mode = 2;
    }

    /// `SetEditTitle(1)`: the channel's word and "message".
    fn chat_title(&self) -> Vec<u16> {
        let Some(w) = self.words.as_ref() else { return Vec::new() };
        let word = match self.channel {
            0 => 106,
            1 => 109,
            2 => 107,
            3 => 108,
            4 => 512,
            _ => return Vec::new(),
        };
        crate::ui::wide(&format!("{} {}", w.get(word, 0), w.get(522, 0)))
    }

    /// The edit box shows the compact legend (no soft keyboard).
    pub fn edit_pad_only(&self) -> bool {
        self.edit.pad_only
    }

    /// A key for the edit box (the game's codes), handled with the next HUD
    /// frame.
    pub fn edit_key(&mut self, key: i32, down: bool) {
        self.edit_keys.push((key, down));
    }

    /// The edit box's keys, with the raster; what is done with its text
    /// (`VIWndGame::OnMessage` 259), or its cancel (260).
    fn run_edit(&mut self, vui: &vi::ui::Vui, r: &dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        let mut cues = Vec::new();
        for (key, down) in std::mem::take(&mut self.edit_keys) {
            if self.mode != 2 {
                break;
            }
            if !down {
                self.edit.key_up(key);
                continue;
            }
            match self.edit.key(vui, r, key, now, &mut cues) {
                crate::editbox::EditUp::Done(purpose) => self.edit_done(purpose),
                crate::editbox::EditUp::Cancel => {
                    self.mode = if self.who_tell.take().is_some() { 35 } else { 0 };
                }
                _ => {}
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
    }

    /// 259: the edit box's text to where it was for.
    fn edit_done(&mut self, purpose: u32) {
        let text = String::from_utf16_lossy(&self.edit.text);
        // VIWndWhoList::SendTell, back to the list
        if let Some(name) = self.who_tell.take() {
            if !text.is_empty() {
                self.chat_line(&format!("/t {name} {text}"));
            }
            self.mode = 35;
            return;
        }
        let mut keep = false;
        if !text.is_empty() {
            match purpose {
                0x0B0F_0000 => self.chat_line(&text),
                0x0B04_0000 => match self.buddies.add(&self.dump.player.name, &text) {
                    Ok(req) => self.session.send(&req),
                    Err((word, name)) => {
                        let fmt = self.words.as_ref().map_or_else(|| format!("(word {word})"), |w| w.get(word, 0).to_string());
                        self.add_chat(fmt.replacen("%s", &name, 1), [1.0, 0.25, 0.25, 1.0]);
                    }
                },
                // the guild's: Create, SetMotd, Say, Invite
                0x0B05_1009 => self.session.send(&crate::guild::Cmds::create(&text)),
                0x0B05_100B => self.session.send(&crate::guild::Cmds::text(11, &text)),
                0x0B05_1006 => self.session.send(&crate::guild::Cmds::text(12, &text)),
                0x0B05_1000 => match crate::guild::Cmds::invite(&self.dump.player.name, &text, &self.guild) {
                    Ok(m) => self.session.send(&m),
                    Err(word) => {
                        let w = self.words.as_ref().map_or_else(|| format!("(word {word})"), |w| w.get(word, 0).to_string());
                        self.add_chat(w, [1.0, 0.25, 0.25, 1.0]);
                    }
                },
                // VIClientPart38300::Add and Remove (by name, any case)
                0x0B05_3001 => self.session.send(&crate::social::Ignore::add(&text)),
                0x0B05_3002 => {
                    if let Some(req) = self.ignore.remove(&text) {
                        self.session.send(&req);
                    }
                }
                0x0B0B_1002 => {
                    // the report asks first (word 1472)
                    let w = self.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(1472, 0)));
                    self.mbox.prompt(&w, 1, 0, 0xB0B1002, self.session.now() as i64);
                    self.mode = 14;
                    keep = true;
                }
                _ => {}
            }
        }
        if !keep {
            self.edit.title.clear();
            self.mode = 0;
        }
    }

    /// `SendReport(to)` (0x00F0): the player, who, and the chat log as
    /// "Text=" and the lines '|'-separated; not oneself (word 1471).
    fn send_report(&mut self, to: &str) {
        if to.eq_ignore_ascii_case(&self.dump.player.name) {
            self.warn(1471);
            return;
        }
        let mut text = String::from("Text=");
        for (line, _) in &self.chat {
            text.push_str(line);
            text.push('|');
        }
        let mut w = net::stream::Writer::op(240);
        w.string(self.dump.player.name.as_bytes()).string(to.as_bytes()).string(text.as_bytes());
        self.session.send(&w.data);
    }

    /// `VIClient::DoAction`: what an entry's message does (the menus'
    /// commands; those that open a window not ported say so).
    pub fn do_action(&mut self, id: u32, arg: i32) {
        let target = self.target;
        match id {
            // VIBankControl::IsCommand / Do: the auction search
            0xA801_0000 | 0xA802_0000 | 0xA803_0000 => self.open_auction_search(id),
            0x0701_0000 | 0x0704_0000 | 0x0705_0000 | 0x0706_0000 => self.group_action(id),
            0x0B01_0000 => self.group_changed(0),
            0x0B02_0000 => self.group_changed(1),
            0x0B03_0000 => {
                if self.group.in_group && self.group.count() >= 2 {
                    self.group_changed(2);
                } else {
                    self.show_word(629);
                }
            }
            0x0B03_0001 => self.group_changed(3),
            // OpenPanel35: the who list asked for, and shown
            0x0B03_0004 => {
                let req = self.who.open();
                self.session.send(&req);
                self.who_open = true;
                self.mode = 35;
            }
            0x0B03_0002 => self.show_word(570),
            0x0B06_0000 => self.request_trade(),
            0x0508_0000 => {
                if target != u32::MAX {
                    self.session.send_op_int(0x0004, target);
                }
            }
            0x0503_0000 | 0x050A_0000 | 0x050B_0000 | 0x050C_0000 | 0x050D_0000 | 0x050E_0000 | 0x050F_0000 | 0x050F_1000 => self.pet_command(id),
            0x0601_0000 | 0x0602_0000 | 0x0603_0000 | 0x0604_0000 => {
                // PlayEmote: the opcode; the player's own action follows
                let k = ((id >> 16) & 0xF) as usize - 1;
                self.session.send_opcode([30, 66, 67, 68][k]);
                self.emote = Some([56, 57, 60, 61][k]);
            }
            0x0A01_0000 => self.quit = true,
            0x0B05_2000 => self.session.send_opcode(69),
            // the guild roster: ShowInvite (OpenGuildMenu, mode 21), or word 570
            0x0B05_1001 => {
                if self.guild.in_guild {
                    self.open_guild(21);
                } else {
                    self.show_word(570);
                }
            }
            0x0B09_0000 => self.session.send_op_int(123, target),
            0x0B0E_0000 => self.session.send_opcode(198),
            0x0B0E_0001 => self.session.send_opcode(222),
            0x0B0E_0003 => self.session.send_opcode(200),
            0x0B0E_0006 => self.session.send_op_int(203, target),
            0x0905_0000 | 0x0509_1004 | 0x0B0E_0002 => {}
            0x0509_1005 => {
                let w = self.words.as_ref().map_or_else(String::new, |w| w.get(836, 0).to_string());
                self.add_chat(format!("{w} 1.108 Protocol=18"), [1.0; 4]);
            }
            0x0902_0000 => {
                let p = self.player.pos;
                self.add_chat(format!("{:.1} {:.1} {:.1}", p[0], p[1], p[2]), [1.0; 4]);
            }
            _ => eprintln!("play: DoAction {id:#x} ({arg}) not ported"),
        }
    }

    /// `PetCommand`: op 0x41 with the command, with a pet (else word 1021);
    /// dismissing asks first (word 830).
    fn pet_command(&mut self, id: u32) {
        if self.pet_id == u32::MAX {
            self.show_word(1021);
            return;
        }
        match id {
            0x0503_0000 => self.session.send_op_byte(0x41, 0),
            0x050A_0000 => self.session.send_op_byte(0x41, 1),
            0x050B_0000 => self.session.send_op_byte(0x41, 2),
            0x050C_0000 => self.session.send_op_byte(0x41, 3),
            0x050D_0000 => self.session.send_op_byte(0x41, 4),
            0x050E_0000 => {
                let w = self.words.as_ref().map_or_else(String::new, |w| w.get(830, 0).to_string());
                let now = self.session.now() as i64;
                self.mbox.prompt(&crate::ui::wide(&format!("{w}?")), 1, 0, 0x050E0000, now);
                self.mode = 14;
            }
            0x050F_0000 => self.session.send_op_byte(0x41, 7),
            0x050F_1000 => {
                let mut w = net::stream::Writer::op(0x41);
                w.u8(8).u32(self.target);
                self.session.send(&w.data);
            }
            _ => {}
        }
    }

    /// The start menu's group actions (`DoAction`): 0x07010000 invite the
    /// target (`InviteEntity`), 0x07040000 remove it (0x0626), 0x07050000
    /// disband (0x0627), 0x07060000 leave (0x0623).
    pub fn group_action(&mut self, action: u32) {
        match action {
            0x0701_0000 => self.invite_entity(self.target),
            0x0704_0000 => {
                if !self.group.in_group {
                    self.show_word(629);
                } else if self.target != u32::MAX {
                    if self.group.leader {
                        self.session.send_op_int(0x0626, self.target);
                    } else {
                        self.show_word(638);
                    }
                }
            }
            0x0705_0000 => {
                if !self.group.in_group {
                    self.show_word(629);
                } else if self.group.leader {
                    self.session.send_opcode(0x0627);
                    self.left_group();
                } else {
                    self.show_word(639);
                }
            }
            0x0706_0000 => {
                if self.group.in_group {
                    self.session.send_opcode(0x0623);
                    self.left_group();
                } else {
                    self.show_word(629);
                }
            }
            _ => {}
        }
    }

    /// `InviteEntity`: a player, not yet in the group, invited (0x0625 with
    /// the id and name); the inviter forms the group and leads it.
    fn invite_entity(&mut self, id: u32) {
        let Some(e) = self.entities.slots.iter().flatten().find(|e| e.id == id) else {
            self.show_word(678);
            return;
        };
        let Some(c) = e.creature.clone() else {
            self.show_word(588);
            return;
        };
        if c.p256 != 2 {
            self.show_word(1015);
            return;
        }
        if c.p264 == 0 {
            self.show_word(1014);
            return;
        }
        if Some(id) == self.self_id32() {
            self.show_word(679);
            return;
        }
        if self.group.in_group && !self.group.leader {
            self.show_word(640);
            return;
        }
        if self.group.members.iter().take(self.group.count()).any(|m| m.id == id) {
            self.show_word_name(1023, &c.name);
            return;
        }
        if !self.group.in_group {
            self.group.leader = true;
            self.group.in_group = true;
        }
        self.show_word_name(637, &c.name);
        let mut w = net::stream::Writer::op(0x0625);
        w.u32(id).string(c.name.as_bytes());
        self.session.send(&w.data);
        self.refuse_invites();
    }

    /// `ShowText(word)`: a word to the chat log.
    fn show_word(&mut self, id: i32) {
        let text = self.words.as_ref().map_or_else(|| format!("(word {id})"), |w| w.get(id, 0).to_string());
        self.add_chat(text, [1.0; 4]);
    }

    /// `Msg00B4`: the ability at display index `index` goes onto `slot` or
    /// off it (it toggles), and any other on that slot comes off; word 824
    /// (on) or 825 (off) with its name, and sound 6.
    fn ability_slot(&mut self, index: i64, slot: i64) {
        let Some(found) = self.abilities.iter().find(|(_, a)| a.display_index == index).map(|(k, _)| *k) else { return };
        for a in self.abilities.values_mut() {
            if a.slot == slot && a.display_index != index {
                a.valid = 0;
                a.slot = -1;
            }
        }
        let a = self.abilities.get_mut(&found).unwrap();
        a.valid = (a.valid == 0) as i64;
        a.slot = slot;
        let (valid, name) = (a.valid != 0, a.name.clone());
        let fmt = self.words.as_ref().map_or_else(|| "%S".to_string(), |w| w.get(if valid { 824 } else { 825 }, 0).to_string());
        self.add_chat(fmt.replacen("%S", &name, 1).replacen("%s", &name, 1), [1.0; 4]);
        self.cues.push(crate::sound::Cue::Ui(6));
    }

    /// `Send0032`: use an ability, by its slot, on the target.
    fn send_0032(&mut self, id: i64) {
        let Some(slot) = self.abilities.get(&id).map(|a| a.slot) else { return };
        let mut w = net::stream::Writer::op(0x0032);
        w.u32(self.ability_token).u8(slot as u8).u32(self.target);
        self.session.send(&w.data);
    }

    /// A pad key for the HUD's quick menus in play (`VIWndHUDMenu::OnKeyDown`
    /// and `VIWndGame`'s left and right): using an ability sends 0x0032.
    pub fn hud_key(&mut self, key: crate::hudmenu::Key) {
        if self.mode != 0 {
            return;
        }
        let now = self.session.now() as i64;
        // the expansion mask's bit 1: the second ability bar (menu 1)
        let second = (self.session.expansion >> 1) & 1 != 0;
        let mut cues = Vec::new();
        let used = self.hud_menus.key(key, now, second, &mut cues);
        self.cues.extend(cues);
        if let Some((msg, id)) = used {
            match msg {
                0x8060000 => self.send_0032(id),
                0x2030000 => self.use_item(id as i32, 1),
                _ => {}
            }
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
    /// the target in reach its spell effect plays, the player on the target
    /// (and the time is kept); otherwise the swing's animation plays and
    /// 0x00D8 goes out, and the next is 2 s off.
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
            let effect = vi::swing::swing_effect(swing);
            let me = self.self_id32();
            let found = |id: Option<u32>| self.entities.slots.iter().flatten().any(|e| Some(e.id) == id);
            if self.spells.as_ref().is_some_and(|s| s.has(effect)) && found(me) && found(Some(self.target)) {
                self.casts.push(crate::spells::Cast { effect, time: 0, caster: me.unwrap(), target: self.target, targets: Vec::new() });
                self.swung = now;
            }
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

    /// `MaybeStopAutoAttack`: the button released, unless the Auto Attack
    /// option is on.
    pub fn attack_release(&mut self) {
        if self.options.auto_attack == 0 {
            self.auto_attack = false;
        }
    }

    /// `VIClient::Process`'s part of the options: with Auto Attack on, a
    /// target gone, not to be fought or oneself stops it; the faction and
    /// loot filters sent when they change (201, 202: off, then 1), and once
    /// after the world entry (then 0).
    fn options_tick(&mut self) {
        if self.options.auto_attack != 0 && self.target != u32::MAX && self.auto_attack {
            let alive = self.entities.slots.iter().flatten().find(|e| e.id == self.target).is_some_and(|e| e.p19 != 0);
            if !alive || Some(self.target) == self.self_id32() {
                self.auto_attack = false;
            }
        }
        let on = (self.options.no_faction == 0, self.options.no_loot == 0);
        let sent = self.filters_sent;
        let mut send = |op: u16, on: bool, b: u8| {
            let mut w = net::stream::Writer::op(op);
            w.u8(!on as u8).u8(b);
            self.session.send(&w.data);
        };
        match sent {
            None => {
                send(201, on.0, 0);
                send(202, on.1, 0);
            }
            Some(s) => {
                if s.0 != on.0 {
                    send(201, on.0, 1);
                }
                if s.1 != on.1 {
                    send(202, on.1, 1);
                }
            }
        }
        self.filters_sent = Some(on);
    }

    /// `Msg076F`: a buddy request. Busy (any window up) refuses it with
    /// word 649, a full list (32) with reason 8; else `ShowBuddyRequest`
    /// (mode 17).
    fn buddy_request(&mut self, data: &[u8]) {
        self.buddies.recv_request(data);
        if self.mode != 0 {
            let name = self.buddies.request_name.clone();
            eprintln!("play: a buddy request from {name} while busy (mode {})", self.mode);
            self.show_word_name(649, &name);
            let req = self.buddies.refuse(1);
            self.session.send(&req);
        } else if self.buddies.count() >= 32 {
            let req = self.buddies.refuse(8);
            self.session.send(&req);
        } else {
            let now = self.hud_last;
            let words = self.words.clone();
            let w = |id: i32| words.as_ref().map_or_else(String::new, |w| w.get(id, 0).to_string());
            let name = self.buddies.request_name.clone();
            self.buddy_wnd.show_request(&w, &name, now);
            self.mode = 17;
        }
    }

    /// `OpenOptions` (34), `OpenBuddyList` (18), `OpenIgnoreList` (26):
    /// the mode, the window set up with the next HUD frame.
    /// A guild text: `ShowMsg`/`ShowText` to the chat, a chat line in the
    /// guild colour.
    fn guild_out(&mut self, o: crate::guild::Out) {
        match o {
            crate::guild::Out::Msg(w) => self.show_word(w),
            crate::guild::Out::Word(w, args) => {
                let mut t = self.words.as_ref().map_or_else(|| format!("(word {w})"), |x| x.get(w, 0).to_string());
                for a in args {
                    // VIString::snprintf16: %s, %S and %u each take the next
                    if let Some(i) = ["%s", "%S", "%u", "%d"].iter().filter_map(|f| t.find(f)).min() {
                        t.replace_range(i..i + 2, &a);
                    }
                }
                self.add_chat(t, [1.0; 4]);
            }
            crate::guild::Out::Chat(t) => self.add_chat(t, crate::guild::CHAT_COLOR),
        }
    }

    /// `OpenGuild` (20, the invitation) and `OpenGuildMenu` (21, the
    /// roster): `SetMode(0)`, then the window with the next HUD frame.
    fn open_guild(&mut self, mode: i32) {
        self.mode = mode;
        self.guild_open = Some(mode);
        self.guild_keys.clear();
    }

    /// The start menu's guild entries (`VIWndGame::OnMessage` 261): each
    /// asks its `Can*` first.
    fn guild_action(&mut self, msg: u32, id: i32) {
        use crate::guild::Cmds;
        let me = self.dump.player.id as i32;
        let g = &self.guild;
        let now = self.session.now() as i64;
        let word_box = |p: &mut Self, word: i32, kind: i32, purpose: i32| {
            let t = p.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(word, 0)));
            p.mbox.prompt(&t, kind, 0, purpose, now);
            p.mode = 14;
        };
        let err = match msg {
            0x0B05_1000 => Cmds::can_invite(g, me),
            0x0B05_1006 | 0x0B05_1008 | 0x0B05_100A => Cmds::can_member(g),
            0x0B05_1009 => Cmds::can_create(g, self.dump.player.p34, self.group.in_group && self.group.count() == 4, self.group.leader),
            0x0B05_1007 | 0x0B05_100B => Cmds::can_lead(g, me),
            0x0B05_100C => {
                self.session.send(&Cmds::set_chat(id != 0));
                return;
            }
            _ => return,
        };
        match (msg, err) {
            (0x0B05_1000, 0) => self.open_edit(msg, 24, 86, true),
            (0x0B05_1000, 3 | 11 | 5) => self.show_word([570, 584, 546][[3, 11, 5].iter().position(|&e| e == err).unwrap()]),
            (0x0B05_1006, 0) => self.open_edit(msg, 128, -1, false),
            (0x0B05_1009, 0) => self.open_edit(msg, 64, 1027, false),
            (0x0B05_1009, 4) => self.show_word(569),
            (0x0B05_1009, 6) => self.guild_out(crate::guild::Out::Word(567, vec![crate::guild::COST.to_string()])),
            (0x0B05_1009, 7) => self.show_word(568),
            (0x0B05_1007, 0) => word_box(self, 543, 1, 0xB051007),
            (0x0B05_1008, 0) => word_box(self, 542, 1, 0xB051008),
            // List: Prompt(motd), an OK box
            (0x0B05_100A, 0) => {
                let t = crate::ui::wide(&self.guild.motd);
                self.mbox.prompt(&t, 0, 0, 0, now);
                self.mode = 14;
            }
            (0x0B05_100B, 0) => self.open_edit(msg, 64, 1028, false),
            (_, 3) => self.show_word(570),
            (_, 5) => self.show_word(546),
            _ => {}
        }
    }

    /// `VIWndGuild` in modes 20 and 21: its opening, keys, drawing and what
    /// it asks.
    fn run_guild(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        if !matches!(self.mode, 20 | 21) {
            self.guild_keys.clear();
            return;
        }
        let Some(start) = self.start.as_ref() else { return };
        let menus = &start.menus;
        let now = vui.time;
        let words = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut cues = Vec::new();
        let mut ups = Vec::new();
        match self.guild_open.take() {
            // UpdateMode 20: the invitation box
            Some(20) => self.guild_wnd.show_invite(&words, &self.guild_cmds, now),
            // UpdateMode 21: Close, the roster
            Some(21) => self.guild_wnd.close(vui, r, now),
            _ => {}
        }
        let me = self.dump.player.id as i32;
        for (k, down) in std::mem::take(&mut self.guild_keys) {
            ups.extend(self.guild_wnd.key(menus, &self.guild, me, &words, k, down, now, &mut cues));
        }
        ups.extend(self.guild_wnd.draw(vui, r, menus, &self.guild, now, &mut cues));
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        use crate::guild::Up;
        for up in ups {
            match up {
                // HideInvite: CloseGuildMenu (back to the start menu), the
                // invitation forgotten
                Up::Hide => self.close_guild_menu(),
                Up::NotInGuild => {
                    self.show_word(570);
                    self.close_guild_menu();
                }
                // 305: CloseGuildMenu, the edit box holding "/t name "
                Up::Tell(name) => {
                    self.close_guild_menu();
                    self.close_start_menu(2);
                    self.open_edit(0xB0F0000, 128, -1, false);
                    self.edit.set_text(&crate::ui::wide(&format!("/t {name} ")));
                }
                Up::Member(sub, id) => self.session.send(&crate::guild::Cmds::member_op(sub, id)),
                Up::Text(w) => self.show_word(w),
                // the answer: CloseGuild (mode 0), then 9 (join) or 10 (refuse)
                Up::Join => {
                    self.guild_wnd.mbox_visible = false;
                    self.mode = 0;
                    let m = self.guild_cmds.join();
                    self.session.send(&m);
                }
                Up::Refuse => {
                    self.guild_wnd.mbox_visible = false;
                    self.mode = 0;
                    let m = self.guild_cmds.refuse(1);
                    self.session.send(&m);
                }
            }
        }
    }

    /// A test's guild command (`EQOA_GUILD`): `create <name>` or
    /// `invite <name>`, sent as the client's senders send them.
    pub fn guild_test(&mut self, cmd: &str) {
        match cmd.split_once(' ') {
            Some(("create", name)) => self.session.send(&crate::guild::Cmds::create(name)),
            Some(("invite", name)) => {
                if let Ok(m) = crate::guild::Cmds::invite(&self.dump.player.name, name, &self.guild) {
                    self.session.send(&m);
                }
            }
            _ => eprintln!("play: EQOA_GUILD: {cmd:?} is not create or invite"),
        }
    }

    /// `CloseGuildMenu` (with `HideInvite`'s forgetting the inviter): the
    /// window reset and hidden, the start menu again.
    fn close_guild_menu(&mut self) {
        self.guild_wnd.reset();
        self.guild_cmds.inviter = u32::MAX;
        self.mode = 0;
        self.open_start_menu(false);
    }

    fn open_panel(&mut self, mode: i32) {
        self.mode = mode;
        self.panel_open = true;
        self.panel_keys.clear();
    }

    /// The modes whose window takes the start menu's keys.
    pub fn panel_up(&self) -> bool {
        matches!(self.mode, 17 | 18 | 20 | 21 | 26 | 34)
    }

    /// A key for the window of a panel mode, handled with the next HUD
    /// frame.
    pub fn panel_key(&mut self, key: i32, down: bool) {
        if matches!(self.mode, 20 | 21) {
            self.guild_keys.push((key, down));
            return;
        }
        self.panel_keys.push((key, down));
    }

    /// The panels' opening, keys, drawing and what they ask.
    fn run_panels(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let Some(start) = self.start.as_ref() else { return };
        let menus = &start.menus;
        self.buddy_wnd.menu.menu = start.buddy_menu;
        self.ignore_wnd.menu.menu = start.ignore_menu;
        // the HUD's clock, which the windows' tick pulses are drawn on
        let now = vui.time;
        let words = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut cues = Vec::new();
        let (mut buddy_ups, mut ignore_ups, mut option_ups) = (Vec::new(), Vec::new(), Vec::new());
        if std::mem::take(&mut self.panel_open) {
            match self.mode {
                18 => self.buddy_wnd.open(vui, r),
                26 => self.ignore_wnd.open(vui, r),
                34 => self.options_wnd.open(vui, r, &self.options),
                _ => {}
            }
        }
        for (k, down) in std::mem::take(&mut self.panel_keys) {
            match self.mode {
                17 | 18 => buddy_ups.extend(self.buddy_wnd.key(menus, &self.buddies, &words, k, down, now, &mut cues)),
                26 => ignore_ups.extend(self.ignore_wnd.key(menus, &self.ignore, &words, k, down, now, &mut cues)),
                34 if down => self.options_wnd.key(vui, r, &mut self.options, k, now, &mut cues, &mut option_ups),
                _ => {}
            }
        }
        match self.mode {
            17 | 18 => buddy_ups.extend(self.buddy_wnd.draw(vui, r, menus, &self.buddies, now, &mut cues)),
            26 => self.ignore_wnd.draw(vui, r, menus, &self.ignore, now, &mut cues),
            34 => self.options_wnd.draw(vui, r, &self.options, &mut option_ups),
            _ => {}
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        use crate::social::{BuddyUp, IgnoreUp};
        for up in buddy_ups {
            match up {
                BuddyUp::Close => {
                    // ClosePanel6D0: CloseBuddyList, back to the start menu
                    self.buddy_wnd.hide();
                    self.buddies.request = u32::MAX;
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                BuddyUp::Tell(name) => {
                    // 304: CloseBuddyList, the edit box holding "/t name "
                    self.buddy_wnd.hide();
                    self.open_edit(0xB0F0000, 128, -1, false);
                    self.edit.set_text(&crate::ui::wide(&format!("/t {name} ")));
                }
                BuddyUp::Mail(name) => self.buddy_mail(name),
                BuddyUp::Remove(n) => {
                    if let Some(req) = self.buddies.remove(n) {
                        self.session.send(&req);
                    }
                }
                BuddyUp::Accept => {
                    let req = self.buddies.accept();
                    self.session.send(&req);
                    // HideBuddyRequest
                    self.buddy_wnd.hide();
                    self.mode = 0;
                }
                BuddyUp::Reject => {
                    let req = self.buddies.refuse(2);
                    self.session.send(&req);
                    self.buddy_wnd.hide();
                    self.mode = 0;
                }
            }
        }
        for up in ignore_ups {
            match up {
                IgnoreUp::Close => {
                    // CloseIgnoreList
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                IgnoreUp::Remove(id) => self.session.send(&crate::social::Ignore::remove_id(id)),
            }
        }
        for up in option_ups {
            if up == crate::options::OptionsUp::Apply {
                // ApplyOptions: CloseOptions, the battle music, the camera
                self.mode = 0;
                self.apply_camera = true;
                self.sound_ops.push(crate::options::OptionsUp::PreviewEnd);
            } else {
                self.sound_ops.push(up);
            }
        }
    }

    /// Builds the HUD on the viewer's device: the 2D layer, UI.CSF's and
    /// ITEMICON.ESF's surfaces (the item icons the client's icon cache
    /// would stream), ADDART.CSF's (the compass's patch), FX.CSF's damage
    /// font and "ArialUI" out of the loaded files.
    pub fn attach_ui(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, lib: &Library) {
        let Some((atlas, textures)) = self.ui_data.take() else { return };
        let mut ui = render::ui::Ui::new(device, format, render::DEPTH_FORMAT);
        let words = self.words.clone().unwrap_or_default();
        let vui = crate::ui::Vui::new(words, textures, atlas);
        let mut assets = crate::ui::UiAssets::default();
        assets.add_white(&mut ui, device, queue);
        for (name, c) in &lib.files {
            if name.eq_ignore_ascii_case("UI.CSF") || name.eq_ignore_ascii_case("ITEMICON.ESF") || name.eq_ignore_ascii_case("ADDART.CSF") {
                assets.add_surfaces(&mut ui, device, queue, c);
            }
            if name.eq_ignore_ascii_case("FX.CSF") {
                assets.add_surface(&mut ui, device, queue, c, vi::hash::hash_resource_id("damage_font.tga"));
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
    pub fn draw_hud(&mut self, ui: &mut render::ui::Ui, walker: &Walker, time_ms: i64, view_proj: glam::Mat4, size: (u32, u32)) {
        ui.quads.clear();
        let under_liquid = walker.cam.walk.is_under_liquid();
        let (start_stats, start_loc) = self.start_view(walker);
        let info_text = if self.info_up || self.info_window.open > 0.0 { self.target_info_text() } else { None };
        let me = self.self_id32();
        let find = |id: Option<u32>| self.entities.slots.iter().flatten().find(|e| Some(e.id) == id);
        let me_e = find(me);
        let target = find(Some(self.target));
        // VIClient's frame: a pet with no name takes its creature's
        let pet_e = find(Some(self.pet_id)).filter(|_| self.pet_id != u32::MAX);
        if self.pet_id != u32::MAX && self.pet_name.is_empty() {
            self.pet_name = pet_e.and_then(|e| e.creature.as_ref()).map(|c| c.name.chars().take(23).collect()).unwrap_or_default();
        }
        let pet = (self.pet_id != u32::MAX).then(|| crate::hud::PetState {
            name: self.pet_name.clone(),
            is_target: self.target == self.pet_id,
            creature: pet_e.and_then(|e| e.creature.as_ref().map(|c| (e.p19 as i32, c.flags262 & 4 != 0, c.flags262 & 8 != 0))),
        });
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
            pet,
            group_count: self.group.count() as i32,
            group: self.group.members[..self.group.count()]
                .iter()
                .filter(|m| Some(m.id) != me && m.active)
                .map(|m| {
                    let flags = find(Some(m.id)).and_then(|e| e.creature.as_ref()).map_or(0, |c| c.flags262);
                    crate::hud::MemberState { name: m.name.clone(), is_target: m.id == self.target, hp: (m.hp_known != 0).then_some((m.hp, flags & 4 != 0, flags & 8 != 0)) }
                })
                .collect(),
            patches: self.group.members[..self.group.count()]
                .iter()
                .enumerate()
                .map(|(index, m)| {
                    // the entity's actor while it has one, else channel 0x40's place in this world
                    let actor = if Some(m.id) == me { self.self_at.map(|a| a.0) } else { self.actor_pos(m.id) };
                    let pos = actor.or_else(|| (m.x == self.dump.player.world as i32).then_some(m.pos));
                    crate::hud::PatchMember { index, id: m.id, pos }
                })
                .collect(),
            self_id: me,
            target: self.target,
            camera: walker.eye.position.to_array(),
            time_ms,
        };
        let now = self.session.now();
        let dt = ((time_ms - self.hud_last).clamp(0, 250)) as f32 / 1000.0;
        self.hud_last = time_ms;
        let mut actions = Vec::new();
        if let Some((mut vui_o, assets, mut hud_o)) = self.hud.take() {
            let (vui, hud) = (&mut vui_o, &mut hud_o);
            vui.time = time_ms;
            // VIClient's m_p4FD28.Process: after the scene, before the windows
            let font = assets.surface(vi::hash::hash_resource_id("damage_font.tga"));
            let (slots, puppets, self_at, me) = (&self.entities.slots, &self.puppets, self.self_at, self.self_id32());
            self.damage.process(ui, font, time_ms, view_proj, size, |id| {
                let k = slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id))?;
                if Some(id) == me {
                    return self_at.map(|(pos, slot4)| crate::damage::Target { pos, slot4 });
                }
                let p = puppets.get(k)?.as_ref()?;
                Some(crate::damage::Target { pos: slots[k].as_ref()?.actor_pos, slot4: p.slot4_at })
            });
            let mut r = assets.raster(ui);
            hud.draw(vui, &mut r, &state);
            if crate::influence::shown(self.mode) {
                crate::influence::draw(vui, &mut r, &self.effects);
            }
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
            if self.mode == 14 {
                self.mbox.draw(vui, &mut r, now as i64);
            }
            actions = self.run_start_menu(vui, &mut r, &start_loc);
            // DrawFade: black over the whole screen at the fade's alpha (the
            // game's spans the wide edges; here past the 4:3 frame's sides)
            let a = self.fade.step(dt);
            if a > 0.0
                && let Some(tex) = assets.white
            {
                let (x0, y0, x1, y1) = (-2000.0, -1.0, 2640.0, 449.0);
                r.ui.push_whole(render::ui::Quad { tex, blend: 3, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[0.5, 0.5]; 4], color: [0.0, 0.0, 0.0, a], clip: None });
            }
            self.run_who(vui, &mut r);
            self.run_quests(vui, &mut r);
            self.run_craft(vui, &mut r);
            self.run_trainer(vui, &mut r);
            self.run_class_mastery(vui, &mut r);
            self.run_factions(vui, &mut r);
            self.run_npcw(vui, &mut r);
            self.run_bank(vui, &mut r);
            self.run_panels(vui, &mut r);
            self.run_guild(vui, &mut r);
            self.run_mail(vui, &mut r);
            self.run_support(vui, &mut r);
            self.run_edit(vui, &r);
            if self.mode == 2 {
                let mut cues = Vec::new();
                self.edit.draw(vui, &mut r, now as i64, &mut cues);
                self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
            }
            // the spell book: opened, its keys, drawn; what it asks for
            let now_ms = now as i64;
            let expansion = self.session.expansion;
            if std::mem::take(&mut self.book_open) {
                self.book.open(vui, &r, expansion, now_ms);
            }
            let mut book_out = Vec::new();
            for (k, down) in std::mem::take(&mut self.book_keys) {
                if self.mode == 5 {
                    book_out.extend(self.book.key(&self.abilities, expansion, k, down, now_ms));
                }
            }
            if self.mode == 5 {
                book_out.extend(self.book.draw(vui, &mut r, &self.abilities, self.dump.player.class as i32, expansion, now_ms));
            }
            self.book_actions(book_out);
            if let Some(s) = self.start.as_mut() {
                let mut cues = Vec::new();
                s.menu.draw(vui, &mut r, &s.menus, now as i64, &start_stats, &mut cues);
                if s.menu.visible && self.items.visible {
                    let pl = &self.dump.player;
                    let info = crate::infotext::Info {
                        words: &vui.words,
                        inv: &self.dump.inventory,
                        hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
                        base: &self.stats.base,
                        class: pl.class,
                        level: pl.level,
                        attack_words: &s.attack_words,
                    };
                    self.items.draw(vui, &mut r, &info, &mut cues);
                }
                self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
            }
            // DrawStatus: under water, the breath gauge
            if under_liquid && (self.mode == 0 || self.mode == 36 || self.mode == 38) {
                let at = [430, 50];
                vui.draw_word_ex(&mut r, 911, at[0], at[1], [1.0; 4], true);
                crate::hud::draw_gauge(vui, &mut r, 9, 136, [at[0], at[1] + 19], true, self.dump.player.p30 as f32 / 255.0);
            }
            // the target's info window
            if self.info_up {
                self.info_window.open(false);
            } else {
                self.info_window.close(false);
            }
            if self.info_window.is_open() || self.info_window.is_opening() || self.info_window.is_closing() {
                let w = &mut self.info_window;
                match info_text {
                    None => w.close(true),
                    Some(text) => {
                        let text = crate::ui::wide(&text);
                        let lines = w.count_lines(vui, &mut r, &text, 155);
                        w.init(vui, &r, [500, 235], 195, lines, 1);
                        if text.is_empty() {
                            w.close(true);
                        }
                        w.draw(vui, &mut r);
                        let c = [1.0, 1.0, 1.0, w.open];
                        w.text_box(vui, &mut r, &text, 20, 0, w.size[0] - 40, lines, c, 3);
                    }
                }
            }
            // UpdateQuickBars, then the menu up (the HUD's, in play)
            if self.mode == 0 {
                self.hud_menus.build(&self.abilities, &self.dump.inventory.items);
                let abilities = &self.abilities;
                let inventory = &self.dump.inventory.items;
                let name = |msg: u32, id: i64| -> Vec<u16> {
                    match msg {
                        0x08060000 => abilities.get(&id).map(|a| crate::ui::wide(&a.name)).unwrap_or_default(),
                        0x02030000 => inventory.get(id as usize).map(|it| crate::ui::wide(&it.item.name)).unwrap_or_default(),
                        _ => Vec::new(),
                    }
                };
                self.hud_menus.draw(vui, &mut r, now as i64, &name);
            }
            self.hud = Some((vui_o, assets, hud_o));
        }
        self.start_actions(actions);
    }

    /// An entity's actor position, while it is drawn.
    fn actor_pos(&self, id: u32) -> Option<[f32; 3]> {
        let k = self.entities.slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id))?;
        self.puppets.get(k)?.as_ref()?;
        Some(self.entities.slots[k].as_ref()?.actor_pos)
    }

    /// `DrawTargetInfo`'s text: a player's name, guild, race, title or
    /// class and level (the guild and title with 0x0010's answer), a
    /// creature's name and level; `None` closes the window.
    fn target_info_text(&self) -> Option<String> {
        let e = self.entities.slots.iter().flatten().find(|e| e.id == self.target)?;
        let c = e.creature.as_ref()?;
        let word = |id: i32| self.words.as_ref().map_or(String::new(), |w| w.get(id, 0).to_string());
        // GetRace / GetClass: out of range is entry 0; races 10-20 are empty
        const RACE_WORDS: [i32; 10] = [183, 185, 186, 187, 188, 189, 191, 192, 193, 190];
        let race = if (c.race as usize) < 21 { RACE_WORDS.get(c.race as usize).copied().unwrap_or(-1) } else { 183 };
        let race = word(race);
        let class = word(if c.class < 15 { 165 + c.class as i32 } else { 165 });
        let level = format!("{} {}", word(503), c.level);
        match c.p256 {
            2 => {
                let (guild, title) = if self.target_info.is_some() { (self.target_guild.as_str(), self.target_title.as_str()) } else { ("", "") };
                let rank = if title.is_empty() { class.as_str() } else { title };
                Some(if guild.is_empty() { format!("{}\n{race}\n{rank}\n{level}", c.name) } else { format!("{}\n{guild}\n{race}\n{rank}\n{level}", c.name) })
            }
            3 => Some(format!("{}\n{level}", c.name)),
            _ => None,
        }
    }

    /// The target's name, for the screen.
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
        if std::env::var_os("EQOA_INV").is_some() {
            for (k, it) in self.dump.inventory.items.iter().enumerate() {
                eprintln!("inv {k}: {} wear {} equip {} quick {} p4c {} p54 {} qty {} icon {:#x}/{:#x}", it.item.name, it.item.wear_slot, it.equip_slot, it.quick_slot, it.item.p4c, it.item.p54, it.quantity, it.item.icon, it.item.icon2);
            }
        }
        let n = self.entities.slots.iter().flatten().count();
        let on_bar = self.abilities.values().filter(|a| a.valid != 0).count();
        let fx = self.spells.as_ref().map_or_else(String::new, |s| format!(", {}", s.describe()));
        format!("{n} entities, {} items, {} abilities ({on_bar} on a bar), {} packets sent{fx}", self.dump.inventory.items.len(), self.abilities.len(), self.session.packets_sent)
    }
}
