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
#[path = "voiceplay.rs"]
mod voiceplay;
#[path = "voicewndplay.rs"]
mod voicewndplay;

use crate::walker::{Placing, Spawn, Walker};
use esf::Library;
use net::entity::{Entities, MAX_SLOTS, RETAIL_SLOTS};
use net::session::{Event, Player, Session};
use render::ui::{Anchor, Vertical};
use render::{GpuModel, Renderer, wgpu};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;
use vi::camera::WalkCamera;
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
    /// The items in hand as they were attached (`held_items`).
    held: Held,
    /// The two hands' projectiles as they were attached
    /// (`spells::hand_projectiles`).
    projectile: [Option<u32>; 2],
    /// The name plate's anchor this frame: 0.6 above the name node (3).
    name_at: Option<[f32; 3]>,
    /// Slot 4's place this frame (`CalcSlotWorldTransform(4)`), which a
    /// damage number floats from.
    slot4_at: Option<[f32; 3]>,
    /// The actor's transform and the centre transform it was drawn with
    /// this frame (what an effect's node transform is under).
    actor_m: Matrix44<Ee>,
    centre: Matrix44<Ee>,
    /// The three hands' weapon ends and the weapon trail.
    weapons: Weapons,
    trail: crate::trails::State,
    /// `m_p2A8`: not yet seen a frame (a corpse seen first lies at once).
    first: bool,
    /// Rastered this frame: within its off distance (`VIScene::RasterActor`),
    /// invisible or not, which is when its own emitters run.
    shown: bool,
    /// The art it was built in (`esf::Library::view_for` its model): a model
    /// the 2002 disc has is built whole as that disc has it, F9 rebuilds it.
    era: esf::Era,
}

/// An entity's actor in the walker's collision world (`VIEntity::m_actor`):
/// whose it is, the actor, and the model (in its art) its sprite was made
/// from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EntityActor {
    id: u32,
    actor: usize,
    model: u32,
    era: esf::Era,
}

/// What an entity's actor is to be this frame (`place_actor`).
struct ActorAt {
    id: u32,
    model: u32,
    era: esf::Era,
    at: Vect3<Ee>,
    hpr: Vect3<Ee>,
    scale: F<Ee>,
    dead: bool,
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
    /// The entity slots and view distance last told the server
    /// (`agree_entity_view`).
    view_sent: Option<(usize, u16)>,
    last_send: u32,
    puppets: Vec<Option<Puppet>>,
    /// The sounds the client asked for this frame (`PlaySound`, `PlayCue`).
    pub cues: Vec<crate::sound::Cue>,
    /// `m_abilities`, by id (the map's key, `m_p00`), and the list's token
    /// (`m_p2BB60`) that 0x0032 echoes.
    pub abilities: std::collections::BTreeMap<i64, net::dump::Ability>,
    ability_token: u32,
    /// S2's record of the pad, which 0x0032 carries on a server whose patch
    /// has it (`s2::Presses`: Sandstorm's live S2)
    presses: crate::s2::Presses,
    /// The HUD's quick menus.
    pub hud_menus: crate::hudmenu::HudMenus,
    /// A 0x0033 for a slot past the game's ten (the Extended page's Ability
    /// Bars) waiting for its 0x00B4: (display index, slot, the session's
    /// clock when it was sent, in ms). A server that keeps only the game's slots
    /// (Sandstorm, measured 2026-10-23: slots 10 and 13 dropped, slot 3
    /// answered) never answers it, and the player is told (`slot_unanswered`).
    slot_asked: Option<(i64, i64, i64)>,
    /// The live client's weapon sets, kept by a server that sends them
    /// (0x0021; `gear`): with them the HUD and the item menu are SOE's final
    /// layout.
    pub gear: crate::gear::Gear,
    /// What the server's messages asked of the atmosphere this frame, in
    /// their order (`Atmos`), for `weather::Weather::take`.
    pub weather: Vec<Atmos>,
    /// Each creature's `m_walk`, made with it (`CreateEntity`: `Init` at the
    /// origin), keyed by the entity's id.
    walks: Vec<Option<(u32, WalkControl<Ee>)>>,
    /// Each entity's actor in the walker's collision world (`m_actor`), by
    /// slot (`place_actors`), and the player's own (`place_self_actor`): what
    /// a walk stops at. The scene they are in (`World::serial`), and the
    /// ignore list changed since the last frame (`Msg1003Done`).
    actors: Vec<Option<EntityActor>>,
    self_actor: Option<EntityActor>,
    actors_in: u64,
    refresh_collision: bool,
    /// The player's model as the server last named it on its channel.
    pub self_model: Option<u32>,
    /// The player dressed as its own channel says (look, items, helm),
    /// placed with the walker's pose.
    self_puppet: Option<Puppet>,
    /// `UpdateGame`'s `SetActorVisibility` for the player's own actor, false
    /// in first person (`self_visibility`).
    self_shown: bool,
    /// `m_p00088`: the follow camera is inside the player's head.
    pub cam_inside: bool,
    /// A world change's camera for the walker built again in the new world
    /// (`world_entered`): the style, and for a warp the preset it had (the
    /// game's one walk camera keeps its mode, pitch and distance through
    /// `Respawn`; `Msg000D` sets the saved style again).
    cam_carry: Option<(i32, Option<crate::camstyle::Preset>)>,
    /// The chat log, newest last (`VIWndGame::AddText`'s lines).
    pub chat: std::collections::VecDeque<(String, [f32; 4])>,
    /// A line being typed, when chat is open.
    pub typing: Option<String>,
    /// `m_target`: the selected entity's id, or u32::MAX.
    pub target: u32,
    /// `m_autoAttack`: held while the attack button is (the 0x40 carries it).
    pub auto_attack: bool,
    /// `m_p071E8`: when the next 0x000F may go out, and the next swing;
    /// after a swap it follows the server's rounds (`attackclock`).
    attack: crate::attackclock::AttackClock,
    /// `m_p071F0`: when the last swing landed in reach.
    pub swung: u32,
    /// `m_p00074`: autorun (`ToggleP74`: L3, or X on the PS2's keyboard).
    pub autorun: bool,
    /// `m_p00078`: following (`ToggleFollow`), the group member `m_p0007C`.
    following: bool,
    follow_id: u32,
    /// `m_p2BB40` .. `m_p2BB48`: the player's own cast (`CastingCombat`),
    /// on the session's clock; the walk walks through it.
    cast_window: (u32, u32),
    /// `m_p06DC8`: 0x00FC's byte, the player held in place.
    rooted: bool,
    /// `m_p06DC0` and `m_p06DC4`: movement and the camera allowed
    /// (`ApplyRaceCamera` takes both away while the start menu frames the
    /// character, `ApplyDefaultCamera` gives them back; the manual camera's
    /// `UpdateMode` 38 too, `Refresh` gives them back).
    locks: (bool, bool),
    /// `VIWndGame`'s `m_p03C` and `m_p040`: L1 or R1 pressed in play, and
    /// when (the session's clock); still held 300 ms on with nothing else
    /// up, the manual camera begins (`manual_frame`).
    aim_hold: bool,
    aim_at: u32,
    /// The manual camera begun and not ended by `ResetView` (the client's:
    /// `manual_frame` ends what a window's `SetMode` left behind).
    manual_on: bool,
    /// `DrawReticle`'s static `s_reticleAlpha`: 1 as the target changes,
    /// then 0.05 less a frame of the game's, down to 0.5.
    reticle_alpha: F<Ee>,
    /// `m_p50318`: the pad's mode last frame (1 a DualShock read).
    pad_mode: i32,
    /// `CycleTarget`'s static `lastGroup`.
    last_group: bool,
    /// `gKeyboardAttached`, as the host can know it: the last key came from a
    /// keyboard, not a pad or the touch screen. With the options' Soft
    /// Keyboard on (`compact`) it gives the edit boxes their compact face.
    pub keyboard: bool,
    /// The pad's motors as `Rumble` asked (motor, level, ms), for the
    /// viewer's pad to take.
    pub rumbles: Vec<(u8, u8, i32)>,
    /// The stat channel (0x42).
    pub stats: net::dump::Stats,
    /// The active effects (state channel 0x43), `VIWndInfluence`'s icons.
    pub effects: net::dump::Effects,
    /// The floating damage numbers (`m_p4FD28`).
    damage: crate::damage::Damage,
    /// The selection rings round the target (`TrackTarget`).
    rings: crate::rings::Rings,
    /// The player's actor position and its slot 4 this frame, while it is
    /// drawn.
    self_at: Option<([f32; 3], Option<[f32; 3]>)>,
    /// The game's words (WORDS.TXT), for the lines the client writes itself.
    pub words: Option<vi::strtab::StringTable>,
    /// TEXTURES.TXT, the HUD's texture names, until SUPPORT's atlas comes
    /// (`take_modules`).
    pub ui_textures: Option<vi::strtab::StringTable>,
    /// What the HUD is built from once the viewer has a device
    /// (`attach_ui`): SUPPORT's atlas and TEXTURES.TXT.
    ui_data: Option<(Vec<vi::ui::UiTexEntry>, vi::strtab::StringTable)>,
    /// The HUD's surfaces and fonts, uploaded while the atlas is awaited.
    ui_assets: Option<crate::ui::UiAssets>,
    /// The art the HUD's layer was made in (`attach_ui`).
    ui_art: Option<esf::Era>,
    /// CLIENT's and SUPPORT's images taken up (`take_modules`).
    modules_taken: [bool; 2],
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
    /// A move into another world, for the viewer to load: the world file,
    /// and where and how the walk begins there.
    world_change: Option<(String, Spawn)>,
    /// 0x00B2's tokens not answered yet, oldest first: the move's `Respawn`
    /// is not done (another world or the zones round the arrival still to
    /// come), and no 0x40 goes out meanwhile (`arrived`).
    arrival: Vec<u32>,
    /// The group and the invitations.
    pub group: crate::group::Group,
    /// `VIWndMBox` (mode 14).
    mbox: crate::dialog::MBox,
    /// The player's death camera (`m_pC1B0`) and the screen's fade; the
    /// actors whose spells a death ends, for the next spell frame.
    death: Option<crate::death::DeathCam>,
    fade: crate::death::Fade,
    dead_casters: Vec<i32>,
    /// When the death camera last stepped, and the eye it left, which the
    /// frames between the game's keep (`DeathCamera` swings by a step a
    /// frame of the game's).
    death_ms: i64,
    death_eye: Option<(Vect3<Ee>, Vect3<Ee>)>,
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
    /// Entered from the game's own boot (`pregame`): a logout or a lost
    /// connection goes back to it in the window (the server list, or the
    /// disconnect's notice), not to a page's login card.
    pub booted: bool,
    /// `m_p4F0E8`: the server ended the connection, with CLIENT's reason
    /// (`net::session::disconnect_word`); the viewer leaves, and a page
    /// shows the reason on its login screen.
    pub disconnected: Option<i32>,
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
    /// the who window shows a server's player search (`playersearch`), when
    /// the patch's own window cannot be read
    search_mode: bool,
    /// a server's player search in the patch's own window (`searchwnd`),
    /// over play
    search: crate::searchwnd::Search,
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
    /// the journal was opened from the quest tracker (`Pick::Quests`): closed,
    /// it goes back to the world, not to the start menu
    quest_from_hud: bool,
    quest_keys: Vec<(i32, bool)>,
    /// The quest guide (`questguide`): the client's own quest marks and
    /// tracker where the server sends none; whether this server offered its
    /// own in the session (then the guide stays off for good); when the guide
    /// may begin, on the session's clock (`GUIDE_WAIT_MS` into the world, so
    /// a server's offer comes first); and whether a scripted run turned it on
    /// whatever the server offers (`EQOA_QUEST_GUIDE`).
    guide: crate::questguide::Guide,
    server_quests: bool,
    guide_from: Option<u32>,
    guide_forced: bool,
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
    /// Ignore Shouts told the server this login (`tell_shouts`), and a shout
    /// it sent dropped since (said once in the log).
    shouts_told: bool,
    shouts_dropped: bool,
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
    /// the Options window to open on its Controls page (F1 from play)
    controls_open: bool,
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
    /// The projectiles in flight as drawn (`draw_projectiles`): per DictID
    /// its model and a GPU model for each one flying at once, kept for the
    /// next.
    projectiles: std::collections::BTreeMap<u32, (Option<esf::Model>, Vec<GpuModel>)>,
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
    /// The button glyphs the last HUD frame drew, where (`pointer`).
    glyphs: crate::pointer::Glyphs,
    /// The wheel's lines not yet turned into a window's up or down.
    wheel: f32,
    /// A pointer's keys for the window up, and the one held (`press`).
    key_queue: std::collections::VecDeque<i32>,
    held: Option<i32>,
    /// The quick-chat menus L2 opens (`VIWndMenu4`, mode 30) and the dump's
    /// sets they show.
    quick: crate::quickchat::QuickChat,
    /// A mouse press on a quick-chat choice: its direction's key, held until
    /// the button is let go (`point_press`).
    quick_held: Option<i32>,
    /// `m_alarm`: the play-time alarm.
    alarm: crate::alarm::Alarm,
    /// `EQOA_QUICK_TEXT`'s progress: the piece, the character.
    quick_typed: (usize, usize),
    /// The map (M; the client's own, `map.rs`).
    pub map: crate::map::Map,
    /// The loading screen (`VIClient::LoadingProgress`, `loading.rs`): up
    /// from a move or world entry until the zones round the arrival are in.
    loading: crate::loading::Screen,
    /// `m_speed`: the run speed the server gives (the dump's, 0x00F8's and
    /// Class Mastery's), which `UpdateGame` hands the walk every frame
    /// (`control_client`).
    speed: f32,
    /// The walk's walk speed as `SetSpeed` leaves it: 3, or 0 with a run
    /// speed of 0 (InitParms' 3 until then).
    walk_speed: f32,
    /// Frames drawn, for the far characters' turns at a pose
    /// (`far_pose_every`), and how many were far the last frame.
    drawn: u32,
    far_last: u32,
    /// Voice chat (`voice`, `voiceplay`): the server's, or the demo's.
    pub voice: crate::voice::PlayVoice,
    /// The voice window (`voicewnd`, `voicewndplay`), over play.
    voice_wnd: crate::voicewnd::VoiceWnd,
}

/// Past the game's own distance (drawn only for NPC/PC Distance) a
/// character is posed every frame while no more than this many are there;
/// with more, each takes its turn (its slot's) one frame drawn in as many as
/// keeps the poses to about this many a frame, and its place every frame:
/// there it is a few pixels tall, and with two hundred in view their poses,
/// on the EE's floats, were most of a frame (measured: 16 of 25 ms). Posed
/// one frame in four however few they were, a character walking there
/// stepped at a quarter of the frame rate (F3 20261024-065253: Desert
/// Madmen, a human's LOD body, at 110 in the dawn fog).
const FAR_POSE_BUDGET: u32 = 50;

/// How long into the world the quest guide waits for a server's own quest
/// marks and tracker before making its own (`Play::guide_on`): go-eqoa-server
/// offers them at once, with the world entry's first replies.
const GUIDE_WAIT_MS: u32 = 3000;

/// The quest marks for the map and the minimap: the guide's while it is on
/// (`guide_on`), else the server's while it offers them, none of either with
/// Quest Markers off; `None` (no Quests button) with neither. Of the fields,
/// not of `Play`, so the map borrows beside it.
fn quest_marks_of<'a>(session: &'a Session, guide: &'a crate::questguide::Guide, guide_on: bool) -> Option<&'a [net::session::QuestMark]> {
    let on = crate::settings::client().quest_markers;
    if guide_on {
        Some(if on { &guide.estimate.marks[..] } else { &[] })
    } else {
        session.quest_marks_offer.then_some(if on { &session.quest_marks[..] } else { &[] })
    }
}

/// How many frames drawn a far character waits between poses with `far` of
/// them in view: every frame up to `FAR_POSE_BUDGET`, then about that many
/// poses a frame.
/// Movement and the camera as `VIWndGame::UpdateMode` leaves them in
/// `mode` (`m_p06DC0`, `m_p06DC4`): it holds both first (`SetP06DC0(0, 0)`)
/// and lets them go only in play (0), the edit box (2), a tell (28) and a
/// reply (29), and the chat box's history (33: R2's `SetP06DC0(1, 1)` after
/// it); fishing (36) lets the camera go (`SetP06DC0(0, 1)`). The quick chat
/// (30) and the start menu (13) are opened from play without an
/// `UpdateMode`, so they keep play's (the start menu's framing holds both
/// itself: `ApplyRaceCamera`). Every other window (an NPC's words, a dialog,
/// a prompt, the corpse, a merchant, the bank, the trainer, what the start
/// menu opens, the manual camera) holds the player and the camera where they
/// are until it closes (F3 20261025-045748).
pub fn mode_locks(mode: i32) -> (bool, bool) {
    match mode {
        0 | 2 | 28 | 29 | 33 | 30 | 13 => (true, true),
        36 => (false, true),
        _ => (false, false),
    }
}

fn far_pose_every(far: u32) -> u32 {
    far.div_ceil(FAR_POSE_BUDGET).max(1)
}

/// `gWorldInfo`'s paths on the Frontiers disc, by world index (CLIENT's
/// table names six; a server's S2 can add more, which are not read here).
/// What a message asks of the atmosphere, in the order the messages came: a
/// weather byte (0x00C5 faded, 0x00C7 at once), or the sky set at once for
/// where the player now stands, as `Respawn` (0x00B2) and `Msg000D` end with
/// `SetSky(m_camera, 1)`. A sky that only fades in after a port held every
/// weather change behind its 30 s (`VIAtmosphere::SetWeather` waits on a
/// change under way), and Sandstorm sends the weather just before each port.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Atmos {
    Weather(u8, bool),
    SkyNow,
}

pub const WORLD_FILES: [&str; 6] = ["TUNARIA.ESF", "RATHE.ESF", "ODUS.ESF", "LAVASTM.ESF", "PLANESKY.ESF", "SECRETS.ESF"];

/// The world file a login loads, as `Msg000D`'s `LoadWorld(m_player.m_world)`
/// picks it: the world table's file for the dump's world id (the server's
/// patch's table, else CLIENT's six: `s2::Rules::world_file`), unless the
/// dump says the world is not on the disc (`m_p0662C`), when its path is
/// loaded; the path too for an id past the table. A file name in capitals.
/// The login took the path alone, so a vampire standing in Sandstorm's
/// Sanctuary (world 7, SECRETS.ESF) logged in to Tunaria's hills (F3
/// 20261025-055704); a teleport there already went by the id.
pub fn login_world_file(player: &net::session::Player) -> String {
    let path = || player.world_path.rsplit(['\\', '/']).next().unwrap_or("tunaria.esf").to_uppercase();
    if player.world_not_on_disc {
        return path();
    }
    crate::s2::get().world_file(player.world).unwrap_or_else(path)
}

#[cfg(test)]
mod login_world {
    use super::*;

    fn player(world: i64, path: &str, not_on_disc: bool) -> net::session::Player {
        net::session::Player { world, world_path: path.into(), world_not_on_disc: not_on_disc, ..Default::default() }
    }

    /// A login goes to the world the dump's id names in the world table,
    /// whatever its path says: Sandstorm's patch's seventh is Secrets
    /// again. The dump's flag, or an id past the table, loads the path. The
    /// control: the path alone, as the login had it, goes to Tunaria.
    #[test]
    fn a_login_loads_its_world_by_the_id() {
        let world = |esf: &str| crate::s2::World { name: String::new(), esf: esf.into(), name_id: 0, bounds: [0; 4] };
        let mut eight: Vec<_> = WORLD_FILES.iter().map(|f| world(&format!("data\\{f}"))).collect();
        eight.push(world("data3\\Underfoot.esf"));
        eight.push(world("data3\\Secrets.esf"));
        let s2 = crate::s2::Rules { worlds: Some(eight), ..crate::s2::Rules::retail() };
        let cases = [
            (crate::s2::Rules::retail(), player(2, "data\\tunaria.esf", false), "ODUS.ESF"),
            (crate::s2::Rules::retail(), player(9, "data3\\Underfoot.esf", false), "UNDERFOOT.ESF"),
            (crate::s2::Rules::retail(), player(0, "data2\\odus.esf", true), "ODUS.ESF"),
            (s2.clone(), player(7, "data\\tunaria.esf", false), "SECRETS.ESF"),
            (s2.clone(), player(6, "data\\tunaria.esf", false), "UNDERFOOT.ESF"),
            (s2, player(0, "data\\tunaria.esf", false), "TUNARIA.ESF"),
        ];
        let by_path = |p: &net::session::Player| p.world_path.rsplit(['\\', '/']).next().unwrap_or("tunaria.esf").to_uppercase();
        let mut control_fails = false;
        for (rules, p, want) in cases {
            assert_eq!(crate::s2::with_rules(rules, || login_world_file(&p)), want, "world {} by {:?}", p.world, p.world_path);
            control_fails |= by_path(&p) != want;
        }
        assert!(control_fails, "the path alone must miss one");
    }
}

impl Play {
    /// `ShowDialog` as 0x0046 calls it (`b` 1): expanded (`%p` and the icon
    /// codes), shown now when nothing else is up, else queued.
    fn show_dialog(&mut self, text: &[u16]) {
        self.show_dialog_b(text, true);
    }

    /// `ShowDialog(id, text, a, b)`: `b` is `OpenDialog`'s sound 15 when the
    /// dialog opens at once; a queued one opens with it whatever `b` was.
    fn show_dialog_b(&mut self, text: &[u16], b: bool) {
        let ctx = vi::ui::ExpandContext { player_name: self.dump.player.name.clone(), ..Default::default() };
        let buf = vi::ui::expand(text, 218, true, &ctx);
        if self.mode == 0 {
            self.open_dialog(&buf, b);
        } else if self.dialogs.len() < 11 {
            self.dialogs.push_back(buf);
        }
    }

    /// Case 0x0036: a byte, then the player's training points
    /// (`m_player.m_p40`, `ReadSigned` into an int); when they change,
    /// `QuestStage(byte, change)`: sound 7 with the byte, word 625 ("You
    /// have received more training points!") in a dialog (`b` 0), and the
    /// trainer refreshed (`VIWndGame::Msg001D`); before the dialog, the
    /// manual camera (mode 38) ended (`ResetView`).
    fn msg_0036<S: vi::scene::Scene<Ee>>(&mut self, data: &[u8], cam: &mut WalkCamera<Ee>, scene: &mut S) {
        let mut r = net::stream::Reader::new(data);
        let flag = r.u8();
        let points = r.signed() as i32 as i64;
        if r.error {
            return;
        }
        let old = self.dump.player.p40;
        self.dump.player.p40 = points;
        if points != old {
            eprintln!("play: training points {old} -> {points} (0x0036)");
            if flag != 0 {
                self.cues.push(crate::sound::Cue::Ui(7));
            }
            if self.mode == 38 {
                self.reset_view_on(cam, scene);
            }
            let text = self.words.as_ref().map_or_else(|| "(word 625)".to_string(), |w| w.get(625, 0).to_string());
            self.show_dialog_b(&vi::ui::wide(&text), false);
            self.trainer_refresh = true;
        }
    }

    /// Case 0x001C: the stat cap the trainer holds a stat under
    /// (`m_player.m_p44`, `GetP44Total`), four raw bytes.
    fn msg_001c(&mut self, data: &[u8]) {
        let mut r = net::stream::Reader::new(data);
        let cap = r.u32() as i32;
        if !r.error {
            self.dump.player.p44 = cap as i64;
        }
    }

    /// Case 0x077A (no payload): the server refusing a fight it judges the
    /// player outmatched in. `AddWord(1196)` in red ("You are outmatched.
    /// Attacking again is not advised."), `StopAutoAttack`, and
    /// `SendOpcode(1915)`: the 0x077B the server waits for.
    fn msg_077a(&mut self) {
        let text = self.words.as_ref().map_or_else(|| "(word 1196)".to_string(), |w| w.get(1196, 0).to_string());
        self.add_chat(text, [1.0, 0.25, 0.25, 1.0]);
        self.auto_attack = false;
        self.session.send_opcode(0x077B);
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
            // VIWndPrivChat::OnKeyDown: Enter and ✕ (13, 134) send 279, Escape
            // and △ (27, 132) send 260; VIWndGame::OnMessage hides the box on
            // either (F3 20261021-024123: △ did not close it)
            9 => {
                if matches!(key, crate::dialog::Key::Enter | crate::dialog::Key::Escape) {
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
                    // SendD4On / SendD4Off: the summons or resurrection taken or refused
                    Some(crate::dialog::BoxAnswer::Yes(2, _)) => self.session.send_op_byte(0x00D4, 1),
                    Some(crate::dialog::BoxAnswer::No(2, _)) => self.session.send_op_byte(0x00D4, 0),
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

    /// `VIWndGame::OpenDialog(text, b)`: sound 15 with `b` (0x0046's and
    /// the queue's; 0x0036's word 625 opens without).
    fn open_dialog(&mut self, text: &[u16], b: bool) {
        if b {
            self.cues.push(crate::sound::Cue::Ui(15));
        }
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


/// What a creature holds as the game dresses it: `ApplyItems` hands each
/// item slot its item (the override if any) and `Apply` attaches it
/// (`AttachItem`) on the node the sprite names for that slot, if the sprite
/// holds items at all (`CSprite::item_node`: a player turned into a creature
/// keeps its weapons on its channel, and the creature shows none). Per slot,
/// the item's DictID and its node.
fn held_items(cs: &CSprite<Ee>, c: &net::entity::Creature) -> Held {
    std::array::from_fn(|i| {
        let id = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
        cs.item_node(i).filter(|_| id != 0).map(|node| (id, node))
    })
}

/// A creature's model as `Apply` dresses it: the look on its text slots, the
/// items and helm on their nodes.
/// Built in the art its model is drawn in (`esf::Library::view_for`): the
/// 2002 disc's, whole, for a model that disc has while its art is drawn.
fn build_puppet(lib: &Library, g: &mut Gpu, e: &net::entity::Entity, look: Option<vi::cust::Look>, slot: usize, game_ms: i64) -> Option<Puppet> {
    let era = lib.view_for(e.model as i32);
    lib.with_view(era, || build_puppet_in(lib, g, e, look, slot, game_ms, era))
}

fn build_puppet_in(lib: &Library, g: &mut Gpu, e: &net::entity::Entity, look: Option<vi::cust::Look>, slot: usize, game_ms: i64, era: esf::Era) -> Option<Puppet> {
    let textures = render::library_textures(lib);
    lib.lookup(e.model as i32).and_then(|r| {
                    let mut model = lib.build_model(r)?;
                    let mut anims = AnimScene { time: game_ms, ..Default::default() };
                    let mut rand = Rand(slot as u64 + 1);
                    let sprite = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand);
                    let held = held_by(sprite.as_ref(), e.creature.as_ref());
                    let (attached, weapons, helm_worn) = match (&sprite, &e.creature) {
                        (Some(cs), Some(c)) => attach(lib, g, cs, c, held),
                        _ => (Vec::new(), [None; 3], false),
                    };
                    let projectile = projectiles_of(sprite.as_ref(), e.creature.as_ref());
                    // ApplyLook: the look's textures and tints on the layers the
                    // sprite's text slots name.
                    if let (Some(cs), Some(look)) = (&sprite, &look) {
                        dress(&mut model, &vi::cust::apply_look_with(cs, look, helm_worn, |id| lib.surface(id as i32).is_some(), &crate::s2::get().cust()));
                    }
                    let gpu = g.renderer.upload(g.device, g.queue, &model, &[render::IDENTITY], &textures);
                    Some(Puppet { model: e.model, look, sprite, anims, rand, gpu, attached, held, projectile, name_at: None, slot4_at: None, actor_m: Matrix44::identity(), centre: Matrix44::identity(), weapons, trail: Default::default(), first: true, shown: false, era })
                })
}

/// Per item slot, the item's DictID and its node (`held_items`).
type Held = [Option<(u32, i32)>; 3];

/// The HUD's 2D layer and what it holds (`Play::attach_ui`): UI.CSF's and
/// ITEMICON.ESF's surfaces, ADDART.CSF's, FX.CSF's damage font and "ArialUI",
/// out of the files in the order the library's view registers them, so a
/// 2002 file's surface goes up ahead of its Frontiers namesake's (`era`).
fn ui_layer(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, lib: &Library) -> (render::ui::Ui, crate::ui::UiAssets) {
    let mut ui = render::ui::Ui::new(device, format, render::DEPTH_FORMAT);
    let mut assets = crate::ui::UiAssets::default();
    assets.add_white(&mut ui, device, queue);
    let files = lib.in_view();
    for &f in &files {
        let (name, c) = &lib.files[f];
        if name.eq_ignore_ascii_case("UI.CSF") || name.eq_ignore_ascii_case("ITEMICON.ESF") || name.eq_ignore_ascii_case("ADDART.CSF") {
            assets.add_surfaces_of(&mut ui, device, queue, lib, f);
        }
        if name.eq_ignore_ascii_case("FX.CSF") {
            assets.add_surface(&mut ui, device, queue, c, vi::hash::hash_resource_id("damage_font.tga"));
        }
    }
    let id = vi::ui::font_id() as i32;
    let font = files.iter().map(|&f| lib.file(f)).find_map(|c| (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(c, i).filter(|f| f.id == id)));
    if let Some(f) = font {
        assets.font = Some(ui.add_font(device, queue, &f));
    }
    (ui, assets)
}

/// The three hands' weapon ends: the item's node and its two point sprites.
type Weapons = [Option<(i32, crate::trails::Points)>; 3];

/// `held_items` of a puppet's sprite: none without a sprite or a creature.
fn held_by(sprite: Option<&CSprite<Ee>>, c: Option<&net::entity::Creature>) -> Held {
    sprite.zip(c).map_or([None; 3], |(cs, c)| held_items(cs, c))
}

/// The hands' projectiles `Apply` leaves a puppet's sprite
/// (`spells::hand_projectiles`): none without a sprite or a creature (no
/// `AttachItem` then).
fn projectiles_of(sprite: Option<&CSprite<Ee>>, c: Option<&net::entity::Creature>) -> [Option<u32>; 2] {
    sprite.zip(c).map_or([None; 2], |(cs, c)| crate::spells::hand_projectiles(c, cs.holds_items))
}

/// What `Apply` attaches to a creature's sprite: the items in hand (`held`,
/// as `held_items` gives them) and the helm (SetHelm, at node 8, in the
/// tint), each on its node. With them, the hands' weapon ends and whether a
/// helm is worn.
fn attach(lib: &Library, g: &mut Gpu, cs: &CSprite<Ee>, c: &net::entity::Creature, held: Held) -> (Vec<(i32, GpuModel)>, Weapons, bool) {
    let textures = render::library_textures(lib);
    let mut attached = Vec::new();
    let mut weapons = [None; 3];
    for (i, held) in held.into_iter().enumerate() {
        let Some((id, node)) = held else { continue };
        weapons[i] = crate::trails::weapon_points(lib, id).map(|w| (node, w));
        if let Some(m) = lib.lookup(id as i32).and_then(|r| lib.build_model(r)) {
            attached.push((node, g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
        }
    }
    let mut helm_worn = false;
    // the server's patch's tables (`s2`), retail's without one
    let t = crate::s2::get().cust();
    if cs.node_ids[8] >= 0
        && let Some(h) = vi::cust::helm_with(c.armor[5], &t)
        && let Some(mut m) = lib.lookup(h as i32).and_then(|r| lib.build_model(r))
    {
        let tint = t.tint_at_most(c.armor_tint[5]);
        for b in &mut m.batches {
            for l in &mut b.layers {
                l.color = tint;
            }
        }
        helm_worn = true;
        attached.push((cs.node_ids[8], g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
    }
    (attached, weapons, helm_worn)
}

/// What `Apply` does to a creature's sprite when its entity is applied again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Redo {
    Keep,
    /// A new model or look: the creature made anew (`SetActorSprite`, then
    /// every item and the whole look).
    Build,
    /// The same sprite, a hand's item changed: the items attached again
    /// (`DetachItem`, `AttachItem`), the sprite kept mid-animation.
    Items,
}

/// `Apply`'s tests, from what a puppet was made with (its model and look)
/// and holds (`held_items`), and what its entity is now. An item is attached
/// again when its model changes (`SetItem` and `Apply` test the item's
/// DictID), not when only its type does.
fn redo(was: (u32, Option<vi::cust::Look>, Held), now: (u32, Option<vi::cust::Look>, Held)) -> Redo {
    if (was.0, was.1) != (now.0, now.1) {
        Redo::Build
    } else if was.2 != now.2 {
        Redo::Items
    } else {
        Redo::Keep
    }
}

/// `Apply` on a creature's puppet as `redo` decides it, from the entity as
/// it is now, dressed in `look`. The puppet it replaced, when it was made
/// anew.
fn apply(lib: &Library, g: &mut Gpu, p: &mut Option<Puppet>, e: &net::entity::Entity, look: Option<vi::cust::Look>, slot: usize, game_ms: i64) -> Option<Option<Puppet>> {
    let r = p.as_ref().map_or(Redo::Build, |p| redo((p.model, p.look, p.held), (e.model, look, held_by(p.sprite.as_ref(), e.creature.as_ref()))));
    // the art swapped (F9) since it was built: made anew in the art now
    let r = if p.as_ref().is_some_and(|p| p.era != lib.view_for(e.model as i32)) { Redo::Build } else { r };
    match r {
        Redo::Keep => None,
        Redo::Build => Some(std::mem::replace(p, build_puppet(lib, g, e, look, slot, game_ms))),
        Redo::Items => {
            if let Some(p) = p.as_mut()
                && let (Some(cs), Some(c)) = (&p.sprite, &e.creature)
            {
                p.held = held_items(cs, c);
                p.projectile = crate::spells::hand_projectiles(c, cs.holds_items);
                (p.attached, p.weapons, _) = lib.with_view(p.era, || attach(lib, g, cs, c, p.held));
            }
            None
        }
    }
}

/// A creature's puppet goes or is made anew: its continuous sound stopped
/// and its sounds let go, as `VICSprite::Release` does (`Sound::let_go`).
fn let_go(s: &mut crate::sound::Sound, p: &Option<Puppet>) {
    let Some(cs) = p.as_ref().and_then(|p| p.sprite.as_ref()) else { return };
    s.let_go(&cs.sounds(), cs.snd.channel);
}

/// `ApplyLook`'s `SetVisible`: a creature whose `Flag1` is up (state byte
/// 154's bit 1, an invisible one: Sandstorm sets it on the caster's own
/// channel) has its skins hidden (`VIClientPart06D00::Apply`'s
/// `SetSkinsVisible`), the body and all it wears; what it holds, its helm and
/// its name stay. No creature (none sent yet) is shown.
fn skins_shown(c: Option<&net::entity::Creature>) -> bool {
    c.is_none_or(|c| c.flags262 & 1 == 0)
}

/// The walker's body as the game rasters the player's actor: hidden past its
/// off distance from the camera (`VIScene::RasterActor`), and of an LOD body
/// the level its centre's distance picks (`VILODSprite::Raster`), both from
/// no farther than the game's camera could be (`lod::eye`). `matrix` is the
/// centre transform `Walker::pose` returns. Whether it is shown.
pub fn place_body(body: &mut GpuModel, walker: &Walker, matrix: [[f32; 4]; 4]) -> bool {
    let at = walker.cam.walk.phys.smooth_location();
    let eye = crate::lod::eye(&walker.cam, &walker.eye.position);
    let shown = crate::lod::within(&at, &eye, crate::lod::off_dist(&body.lod_dists));
    crate::lod::place(body, &glam_to_matrix(matrix), &eye);
    body.visible = shown;
    shown
}

/// Puts layer changes onto the batches of the palette entries they name.
pub(crate) fn dress(model: &mut esf::Model, changes: &[vi::cust::LayerChange]) {
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

/// `VINameSprite::Raster`'s plates kept, at most this many: its 24, one for
/// each of the game's entity slots.
pub const NAME_PLATES_GAME: usize = 24;

/// How this frame's name plates are kept and sized (`labels`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlateView {
    /// where a plate's distance is measured from: no farther than the
    /// game's camera could be (`lod::eye`), so zooming out keeps the names
    /// the console showed round the player
    pub eye: glam::Vec3,
    /// how far from it a plate is kept: `VINameSprite::Raster`'s 22, or the
    /// options' Name Distance
    pub distance: f32,
    /// every plate's size against the game's (`lod::plate_scale`)
    pub scale: f32,
    /// at most this many: the game's 24, or as many as NPC/PC Display keeps
    /// in view
    pub most: usize,
    /// Overlapping Names Show: every plate kept, a plate that meets a
    /// nearer one's included (the game's `RasterDeferred` drops it: Hide)
    pub overlap: bool,
}

impl PlateView {
    /// The game's, from `eye`: within 22, at its size, at most 24, a plate
    /// that meets a nearer one's dropped.
    #[cfg(test)]
    pub fn game(eye: glam::Vec3) -> PlateView {
        PlateView { eye, distance: crate::settings::NAME_DISTANCE_GAME as f32, scale: 1.0, most: NAME_PLATES_GAME, overlap: false }
    }

    /// This frame's for the walker's camera `cam` at `eye` in a window
    /// `size`: from `lod::eye`, to Name Distance, at the zoom's size
    /// (`lod::plate_scale`), at most the entity slots NPC/PC Display keeps,
    /// overlapping or not as Overlapping Names says.
    pub fn of(cam: &vi::camera::WalkCamera<Ee>, eye: &Vect3<Ee>, size: (u32, u32)) -> PlateView {
        let k = size.1 as f32 / render::text::SCREEN_H;
        PlateView {
            eye: glam::Vec3::from(crate::lod::eye(cam, eye).to_array()),
            distance: crate::settings::name_distance(),
            scale: crate::lod::plate_scale(cam, eye, k),
            most: crate::settings::entity_slots().max(NAME_PLATES_GAME),
            overlap: crate::settings::overlapping_names(),
        }
    }
}

/// `VINameSprite::Raster` and `RasterDeferred` for a frame seen through
/// `view_proj`, kept as `view` says: plates within its distance of its eye
/// (the squared view-space length plus w² under the distance squared: the
/// game's 22, from the game's camera), at most its `most`, nearest first,
/// each dropped if its rect on the PS2's 640x448 screen (at the view's
/// scale) meets one already kept (`VIRect::Intersects`, inclusive); with
/// Overlapping Names Show (`overlap`) none is dropped, and they come
/// farthest first, so the nearest is drawn over the rest (a plate tests the
/// scene's depth but writes none: the order decides between plates).
pub fn labels(plates: &[Plate], view: &PlateView, view_proj: glam::Mat4, text: &render::text::Text) -> Vec<render::text::Label> {
    labels_by(plates, view, view_proj, |s| text.width(s), text.height())
}

/// `labels` with the font's measures: a string's `width` and the `height`,
/// in the PS2's pixels.
fn labels_by(plates: &[Plate], view: &PlateView, view_proj: glam::Mat4, width: impl Fn(&str) -> u32, height: u32) -> Vec<render::text::Label> {
    let mut queued = Vec::new();
    let reach = view.distance * view.distance;
    for p in plates {
        let at = glam::Vec3::from(p.at);
        if reach <= (at - view.eye).length_squared() + 1.0 || queued.len() >= view.most {
            continue;
        }
        // in doubles, as the models' view-projection is made eye-relative
        // (`Renderer::draw_in`): at Freeport's x 25,300 a float steps 2 mm,
        // and a plate's depth projected in floats came out up to that far
        // from the models' it is tested against, by a different amount each
        // frame, so where a head reached a name's depth its edge flickered
        // (F3 20261025-025540)
        let clip = (view_proj.as_dmat4() * at.as_dvec3().extend(1.0)).as_vec4();
        if clip.w <= 0.0 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        let (qx, qy) = ((ndc.x * 320.0 + 320.0) as i32, (-ndc.y * 224.0 + 224.0) as i32);
        let w = (width(&p.name) as f32 * view.scale).round() as i32;
        let h = (height as f32 * view.scale).round() as i32;
        let min = (qx - (w >> 1), qy - (h >> 1));
        queued.push((clip.w, min, (min.0 + w, min.1 + h), ndc, p));
    }
    queued.sort_by(|a, b| a.0.total_cmp(&b.0));
    /// A kept plate: its rect's corners, where it is drawn, what.
    type Kept<'a> = ((i32, i32), (i32, i32), glam::Vec3, &'a Plate);
    let mut kept: Vec<Kept> = Vec::new();
    for (_, min, max, ndc, p) in queued {
        let meets = !view.overlap && kept.iter().any(|k| !(min.0 > k.1.0 || max.0 < k.0.0 || min.1 > k.1.1 || max.1 < k.0.1));
        if !meets {
            kept.push((min, max, ndc, p));
        }
    }
    if view.overlap {
        kept.reverse();
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

/// How a creature moves as its channel last said and as it is drawn now (a
/// problem report's entity lines): its locomotion state (what it plays,
/// `SetLocomotion`, when the server does not mark it walking, byte 141:
/// Sandstorm never does), its velocity (what `ExtrapolateMove` carries it
/// along between updates), how long ago its last update came, and how far
/// from that place `Extrapolate`'s glide still draws it. A player pushing
/// against a wall or another character keeps a running velocity while it
/// stands (`VIWalkControl::Move` clears it only off the floor), so every
/// watcher carries it on and pulls it back at each update: a slide this line
/// tells from a late update.
fn move_line(e: &net::entity::Entity, c: &net::entity::Creature, now: u32) -> String {
    let ago = now.wrapping_sub(e.stamp.wrapping_sub(2 * UPDATE_RATE)) as i32;
    let off = (0..3).map(|i| (e.actor_pos[i] - e.pos[i]).powi(2)).sum::<f32>().sqrt();
    format!(
        "move: state {} walking {}, velocity [{:.2}, {:.2}, {:.2}], updated {ago} ms ago, drawn {off:.2} from it",
        c.state, c.p255, c.velocity[0], c.velocity[1], c.velocity[2]
    )
}

fn look_of(c: &net::entity::Creature) -> vi::cust::Look {
    vi::cust::Look { armor: c.armor, armor_tint: c.armor_tint, hair: c.hair, face: c.face, robe: c.robe as i8, robe_tint: c.robe_tint }
}

/// A creature's look and hands as its channel sent them, raw (a problem
/// report's entity lines): what `ApplyLook` and `ApplyItems` dress it in, so
/// a report shows whether the server sent a robe (byte 109: 0-127 is one,
/// 4 and 5 Sandstorm's own, 128-255 none) rather than leaving it to a picture.
fn look_line(c: &net::entity::Creature) -> String {
    let hex = |ids: &[u32; 3]| ids.map(|id| format!("{id:#x}")).join(", ");
    format!(
        "look: robe {} tint {}, armor {:?} tints {:?}, hair {:?}, face {}; items [{}] over [{}] types {:?}",
        c.robe,
        c.robe_tint,
        c.armor,
        c.armor_tint,
        c.hair,
        c.face,
        hex(&c.items),
        hex(&c.items_over),
        c.item_types
    )
}

fn v3(a: [f32; 3]) -> Vect3<Ee> {
    Vect3::of(a[0], a[1], a[2])
}

/// Where the walk stands now (`self.player.pos` changes only on a move).
fn walker_at(walker: &Walker) -> [f32; 3] {
    let p = walker.cam.walk.phys.position;
    [p.x.0, p.y.0, p.z.0]
}

/// The scale an entity's actor has: the channel's, as it is drawn (1 for
/// none).
fn actor_scale(s: f32) -> F<Ee> {
    F::new(if s > 0.0 { s } else { 1.0 })
}

/// One entity's actor kept as `want` says: let go when its entity went or
/// another took the slot (`ReleaseActor`), made with its model's sprite
/// (`CreateActor`), its sprite made again when the model or its art changed
/// (`SetActorSprite`), placed (`SetActorTransform`, when it moved: the rooms
/// it is in are those it was in), its copy dead or alive as its sprite. The
/// actor kept, and whether it was made now.
fn place_actor(w: &mut vi::world::World<Ee>, lib: &Library, had: Option<EntityActor>, want: Option<ActorAt>) -> (Option<EntityActor>, bool) {
    let had = match (had, &want) {
        (Some(a), Some(n)) if a.id == n.id => Some(a),
        (Some(a), _) => {
            w.release_actor(a.actor);
            None
        }
        (None, _) => None,
    };
    let Some(n) = want else { return (None, false) };
    let sprite = |w: &mut vi::world::World<Ee>| lib.with_view(n.era, || lib.lookup(n.model as i32)).and_then(|r| w.find_sprite(lib, r));
    let (a, made) = match had {
        None => {
            let Some(s) = sprite(w) else { return (None, false) };
            (EntityActor { id: n.id, actor: w.create_actor(n.at, n.hpr, n.scale, s), model: n.model, era: n.era }, true)
        }
        Some(mut a) => {
            if a.model != n.model || a.era != n.era {
                let Some(s) = sprite(w) else {
                    w.release_actor(a.actor);
                    return (None, false);
                };
                w.set_actor_sprite(a.actor, s);
                (a.model, a.era) = (n.model, n.era);
            }
            let now = &w.actors[a.actor];
            if now.location.to_array() != n.at.to_array() || now.hpr.to_array() != n.hpr.to_array() || now.scale.bits() != n.scale.bits() {
                w.set_actor_transform(a.actor, n.hpr, n.scale, n.at);
            }
            (a, false)
        }
    };
    w.set_actor_dead(a.actor, n.dead);
    (Some(a), made)
}

#[cfg(test)]
mod entity_actors {
    use super::*;
    use vi::coll::CollSphere;
    use vi::scene::{Filter, FilterKind};
    use vi::world::World;

    const SIGNPOST: u32 = 0x6A82352C;
    const HUMAN: u32 = 0x70D898C6;

    /// TUNARIA's zones round Freeport's open ground and CHAR.ESF, and the
    /// collision world made of them; None without the disc.
    fn freeport(x: f32, z: f32) -> Option<(Library, World<Ee>)> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        lib.add("CHAR.ESF", crate::stream::open_file(&iso, "CHAR.ESF").unwrap());
        let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
        crate::stream::fill(&mut lib, w, x, z, &mut src);
        let world = World::load(&lib, w)?;
        Some((lib, world))
    }

    /// What a walk's sweep across (x, z) at body height meets: the actor, or
    /// -2 for nothing.
    fn meets(w: &mut World<Ee>, x: f32, z: f32) -> i32 {
        let y = crate::walker::ground(w, x, z).unwrap();
        let mut s = CollSphere::<Ee>::default();
        s.init(Vect3::of(x - 4.0, y + 1.2, z), Vect3::of(x + 4.0, y + 1.2, z), F::new(0.6));
        let mut f = Filter::new(FilterKind::Scene, -1);
        if w.collide_sphere(&mut s, &mut f) { f.hit_actor } else { -2 }
    }

    /// `place_actor` over an entity's time in a slot: made with its first
    /// frame, kept (the same actor) as it moves, its sprite made again when
    /// its model changes, nothing while its sprite is dead, let go for another
    /// entity in the slot (made in its place) and when the slot empties; a
    /// model not on the disc makes none. Each step is read back off the
    /// collision world: what a walk's sweep where the entity stands meets.
    /// The control: open ground, before and after.
    #[test]
    fn an_entity_has_an_actor_while_it_is_drawn() {
        let (x, z) = (25275.115f32, 15766.627f32);
        let Some((lib, mut w)) = freeport(x, z) else { return };
        let (x2, z2) = (x - 8.0, z);
        assert_eq!((meets(&mut w, x, z), meets(&mut w, x2, z2)), (-2, -2), "the control: open ground");
        let at = |w: &mut World<Ee>, x: f32, z: f32| Vect3::of(x, crate::walker::ground(w, x, z).unwrap(), z);
        let want = |w: &mut World<Ee>, id: u32, model: u32, (x, z): (f32, f32), dead: bool| {
            let p = at(w, x, z);
            Some(ActorAt { id, model, era: lib.view(), at: p, hpr: Vect3::default(), scale: F::ONE, dead })
        };

        let n = want(&mut w, 7, SIGNPOST, (x, z), false);
        let (a, made) = place_actor(&mut w, &lib, None, n);
        let a = a.expect("made");
        assert!(made && a.id == 7 && a.model == SIGNPOST);
        assert_eq!(meets(&mut w, x, z), a.actor as i32);
        let n = want(&mut w, 7, SIGNPOST, (x, z), false);
        let (b, made) = place_actor(&mut w, &lib, Some(a), n);
        assert_eq!((b.map(|b| b.actor), made), (Some(a.actor), false), "kept");

        // moved: the same actor where it is now
        let n = want(&mut w, 7, SIGNPOST, (x2, z2), false);
        let (b, _) = place_actor(&mut w, &lib, b, n);
        assert_eq!(b.map(|b| b.actor), Some(a.actor));
        assert_eq!((meets(&mut w, x, z), meets(&mut w, x2, z2)), (-2, a.actor as i32), "moved");

        // another model: its sprite made again
        let human = w.find_sprite(&lib, lib.lookup(HUMAN as i32).unwrap()).unwrap();
        let n = want(&mut w, 7, HUMAN, (x2, z2), false);
        let (b, _) = place_actor(&mut w, &lib, b, n);
        assert_eq!(b.map(|b| b.model), Some(HUMAN));
        assert_eq!(w.actors[a.actor].sprite, Some(human));
        assert_eq!(meets(&mut w, x2, z2), a.actor as i32);

        // dead, then alive (a dump)
        let n = want(&mut w, 7, HUMAN, (x2, z2), true);
        let (b, _) = place_actor(&mut w, &lib, b, n);
        assert_eq!(meets(&mut w, x2, z2), -2, "a corpse");
        let n = want(&mut w, 7, HUMAN, (x2, z2), false);
        let (b, _) = place_actor(&mut w, &lib, b, n);
        assert_eq!(meets(&mut w, x2, z2), a.actor as i32);

        // another entity in the slot: the first let go, the second made
        let n = want(&mut w, 8, SIGNPOST, (x, z), false);
        let (c, made) = place_actor(&mut w, &lib, b, n);
        let c = c.unwrap();
        assert!(made && c.id == 8);
        assert_eq!((meets(&mut w, x, z), meets(&mut w, x2, z2)), (c.actor as i32, -2), "the first gone");

        // the slot empty: let go
        assert_eq!(place_actor(&mut w, &lib, Some(c), None).0.map(|c| c.id), None);
        assert_eq!((meets(&mut w, x, z), meets(&mut w, x2, z2)), (-2, -2), "the control: open ground again");

        // a model the disc does not have: no actor
        let count = w.actors.len();
        let n = want(&mut w, 9, 0x1234_5678, (x, z), false);
        assert_eq!(place_actor(&mut w, &lib, None, n), (None, false));
        assert_eq!(w.actors.len(), count);
    }
}

/// `CycleTarget`'s reach: a creature this far from the camera, squared
/// (200), or farther is never targeted.
const CYCLE_REACH2: f32 = 40000.0;

/// What `TargetCentre` reads of a creature with an actor: its id, where it is
/// measured (a character's slot 4, else its place), its +0x18 bit 0 (the
/// selectable flag), whether it is of a player's side (`m_p256` 3) and how
/// far its place is from the camera, squared.
#[derive(Clone, Copy, Debug)]
struct Seen {
    id: u32,
    at: [f32; 3],
    selectable: bool,
    side: bool,
    d2: f32,
}

/// `TargetCentre`'s choice: of the creatures `seen`, not the player `me`,
/// not the pet, none of a player's side, the selectable ones in view
/// (`view_proj`'s frustum: `m_frustum.Cull(pos, 0)`) within `CycleTarget`'s
/// reach the one whose place lies nearest the middle of the picture through
/// `game` (x and y after the divide), the first of equals; none when there is
/// none. The reach is not the console's: its `TargetCentre` takes any
/// distance, the 24 creatures a PS2 keeps reaching 300 (and vanilla's NPC/PC
/// View Distance 1,000), so the crosshair took what Tab could not, out of
/// sight too (the user, 2026-10-29: "holding r1 with the crosshair sight lets
/// players target normally untargetable npcs", then Tab's reach).
fn centre_choice(seen: &[Seen], me: Option<u32>, pet: u32, game: glam::Mat4, view_proj: glam::Mat4) -> Option<u32> {
    let (mut best, mut best_d) = (None, 999999.0f32);
    for s in seen {
        if Some(s.id) == me || s.id == pet || s.side || !s.selectable || s.d2 >= CYCLE_REACH2 {
            continue;
        }
        let pos = glam::Vec3::from(s.at).extend(1.0);
        let h = view_proj * pos;
        if !(h.w > 0.0 && (h.x / h.w).abs() <= 1.0 && (h.y / h.w).abs() <= 1.0 && h.z / h.w >= 0.0 && h.z / h.w <= 1.0) {
            continue;
        }
        let g = game * pos;
        let inv = 1.0 / g.w;
        let (x, y) = (g.x * inv, g.y * inv);
        let d = (x * x + y * y).sqrt();
        if d < best_d {
            best_d = d;
            best = Some(s.id);
        }
    }
    best
}

/// `DrawReticle`'s alpha: on a new target (`flash`) full, else what it was;
/// the next frame's 0.05 less, down to 0.5. (drawn, next)
fn reticle_step(alpha: F<Ee>, flash: bool) -> (F<Ee>, F<Ee>) {
    let a = if flash { F::ONE } else { alpha };
    let next = a - F::new(0.05);
    (a, if next < F::new(0.5) { F::new(0.5) } else { next })
}

/// `UpdateGame`'s `SetActorVisibility` for the player's own actor, from
/// whether it was shown, the camera mode, whether the player is dead, the
/// distance from the camera to the head and the creature's npc flags:
/// whether it is shown now, and `m_p00088` (the follow camera is inside the
/// head).
fn own_actor_shown(was: bool, mode: i32, dead: bool, head_to_camera: F<Ee>, npc_flags: u16, manual_in: bool) -> (bool, bool) {
    let (shown, inside) = match mode {
        // first person, or the manual camera all the way in
        _ if manual_in => (was && dead, false),
        0 => (was && dead, false),
        2 if head_to_camera < F::new(0.8) => (false, true),
        _ => (true, false),
    };
    (shown && npc_flags & 0x200 == 0, inside)
}

#[cfg(test)]
mod name_plates {
    use super::*;
    use glam::{Mat4, Vec3};

    /// One-glyph-wide names, the font's 7 pixels a character, 12 tall.
    fn width(s: &str) -> u32 {
        s.chars().count() as u32 * 7
    }

    /// A plate at `at` (the view is the identity, so its x and y are where
    /// it is on the screen, -1 to 1).
    fn plate(at: [f32; 3], name: &str) -> Plate {
        Plate { at, color: [1.0; 4], name: name.into() }
    }

    fn names(view: &PlateView, plates: &[Plate]) -> Vec<String> {
        labels_by(plates, view, Mat4::IDENTITY, width, 12).into_iter().map(|l| l.text).collect()
    }

    /// Zoomed out to 50 the plates round the player stay, measured from the
    /// eye drawn in to the game's 8 (`PlateView::of` takes `lod::eye`'s):
    /// one 18 past it is kept. The control: from the camera itself, 60 off,
    /// none is.
    #[test]
    fn zoomed_out_the_names_round_the_player_stay() {
        let near = Vec3::new(0.0, 0.0, -8.0);
        let camera = Vec3::new(0.0, 0.0, -50.0);
        let p = [plate([0.0, 0.0, 10.0], "Ab")];
        assert_eq!(names(&PlateView::game(near), &p), ["Ab"]);
        assert!(names(&PlateView::game(camera), &p).is_empty(), "the control must fail");
    }

    /// The walker's view of them (`PlateView::of`): in the follow orbit 50
    /// back, from the eye drawn in to 8, to Name Distance, at the floor's
    /// size in a 1080-line window, as many as NPC/PC Display keeps (the
    /// game's 24 at least); at the game's camera, the game's. The control:
    /// the camera itself is 50 off.
    #[test]
    fn the_walkers_view_of_the_plates() {
        crate::settings::testing::reset(false);
        let mut cam = vi::camera::WalkCamera::<Ee> { camera_mode: 3, target_height: F::new(1.68), ..Default::default() };
        cam.walk.phys.position = Vect3::of(1000.0, 50.0, 2000.0);
        let back = |d: f32| Vect3::of(1000.0, 51.68, 2000.0 - d);
        let v = PlateView::of(&cam, &back(50.0), (1920, 1080));
        assert!((v.eye - Vec3::new(1000.0, 51.68, 1992.0)).length() < 1e-3, "{:?}", v.eye);
        assert_eq!((v.distance, v.most), (22.0, NAME_PLATES_GAME));
        assert!((v.scale - crate::lod::PLATE_FLOOR).abs() < 1e-3);
        assert_eq!(PlateView::of(&cam, &back(8.0), (1920, 1080)), PlateView::game(Vec3::new(1000.0, 51.68, 1992.0)), "the game's camera: the game's");
        crate::settings::set_client(crate::settings::ClientOptions { name_distance: 90, entity_slots: 64, ..Default::default() });
        let v = PlateView::of(&cam, &back(50.0), (1920, 1080));
        assert_eq!((v.distance, v.most, v.overlap), (90.0, 64, false));
        crate::settings::set_client(crate::settings::ClientOptions { overlapping_names: true, ..Default::default() });
        assert!(PlateView::of(&cam, &back(50.0), (1920, 1080)).overlap, "Overlapping Names Show");
        assert!((v.eye - Vec3::new(1000.0, 51.68, 1950.0)).length() > 40.0, "the control: the camera itself");
        crate::settings::testing::reset(false);
    }

    /// Name Distance keeps plates the game's 22 does not: one 30 off at 40,
    /// none at the game's (the control), and one past the distance at
    /// neither.
    #[test]
    fn name_distance_keeps_farther_plates() {
        let eye = Vec3::ZERO;
        let p = [plate([0.0, 0.0, 30.0], "Ab"), plate([0.5, 0.5, 45.0], "Cd")];
        assert_eq!(names(&PlateView { distance: 40.0, ..PlateView::game(eye) }, &p), ["Ab"]);
        assert!(names(&PlateView::game(eye), &p).is_empty(), "the control: the game's 22");
    }

    /// The zoom's size sizes the rects that may not meet: two names 9.6
    /// pixels apart, 14 wide, meet at the game's size (the farther dropped)
    /// and not at half of it, where both are kept. The control is the game's
    /// size.
    #[test]
    fn smaller_names_crowd_less() {
        let eye = Vec3::ZERO;
        let p = [plate([0.0, 0.0, 10.0], "Ab"), plate([0.03, 0.0, 12.0], "Cd")];
        assert_eq!(names(&PlateView { scale: 0.5, ..PlateView::game(eye) }, &p), ["Ab", "Cd"]);
        assert_eq!(names(&PlateView::game(eye), &p), ["Ab"], "the control: the game's size");
    }

    /// At most the view's `most`: thirty names apart on the screen, all kept
    /// when NPC/PC Display keeps 64 in view, the first 24 at the game's.
    #[test]
    fn as_many_names_as_entities_in_view() {
        let eye = Vec3::ZERO;
        let p: Vec<Plate> = (0..30).map(|i| plate([-0.9 + (i % 6) as f32 * 0.35, -0.8 + (i / 6) as f32 * 0.4, 10.0], "A")).collect();
        assert_eq!(names(&PlateView { most: 64, ..PlateView::game(eye) }, &p).len(), 30);
        assert_eq!(names(&PlateView::game(eye), &p).len(), NAME_PLATES_GAME, "the control: the game's 24");
    }

    /// A plate's depth at Freeport's coordinates (x 25,300, where a float
    /// steps 2 mm) is the depth the models are drawn at there: their
    /// view-projection is made relative to the eye in doubles
    /// (`Renderer::draw_in`) and their places taken from the eye, so the
    /// reference is that, in doubles. Forty names 8 to 30 away in front of a
    /// camera in the square, through the game's projection (near 0.3), each
    /// projected alone: within a float step or two of a depth near 1. The
    /// control, the projection in floats (as before), is off by thousands of
    /// times that, up to 0.28 of depth along the view, a different amount as
    /// the camera moves: the flicker where a head reached a name's depth (F3
    /// 20261025-025540).
    #[test]
    fn a_plates_depth_is_the_models_far_from_the_origin() {
        let eye = Vec3::new(25272.58, 56.2, 15727.70);
        // the walker's (`Walker::view`): the game's projection to a fog's far
        // plane, looking along the camera's direction
        let proj = crate::walker::projection_to(16.0 / 9.0, 300.0);
        let view_proj = proj * glam::camera::rh::view::look_to_mat4(eye, Vec3::new(20.0, -1.5, 3.0).normalize(), Vec3::Y);
        let rel = view_proj.as_dmat4() * glam::DMat4::from_translation(eye.as_dvec3());
        let view = PlateView { distance: 200.0, most: 64, overlap: true, ..PlateView::game(eye) };
        let (mut ours, mut floats, mut world) = (0f64, 0f64, 0f64);
        for k in 0..40 {
            let (d, a) = (8.0 + (k % 8) as f32 * 3.1, -0.5 + (k / 8) as f32 * 0.25);
            let at = eye + Vec3::new(d * a.cos(), -1.0 + (k % 3) as f32 * 0.7, d * a.sin());
            let c = rel * (at.as_dvec3() - eye.as_dvec3()).extend(1.0);
            let want = c.z / c.w;
            let got = labels_by(&[plate(at.to_array(), "Ab")], &view, view_proj, width, 12);
            assert_eq!(got.len(), 1, "{at:?} in view");
            let f = view_proj * at.extend(1.0);
            ours = ours.max((got[0].at[2] as f64 - want).abs());
            let off = ((f.z / f.w) as f64 - want).abs();
            floats = floats.max(off);
            // the depth that is, along the view: d(ndc)/dz = near·far/((far-near)·z²)
            let z = c.w;
            world = world.max(off * z * z * (300.0 - 0.3) / (0.3 * 300.0));
        }
        eprintln!("the plates' depth off the models' by at most {ours:e} (projected in floats: {floats:e}, as much as {world:.3} of depth)");
        assert!(ours < 2.5e-7, "a float step or two: {ours:e}");
        assert!(floats > 1e-5, "the control: in floats {floats:e}");
    }

    /// Overlapping Names (F3 20261025-025611: five names of a crowd of
    /// twenty-four): three names on one spot of the screen, 10, 12 and 14
    /// away, through a perspective (the plates' depths differ). Show keeps
    /// all three, the farthest first, so the nearest is drawn last, over
    /// them; Hide, the game's `RasterDeferred`, keeps the nearest alone (the
    /// control: the game's rule fails Show's).
    #[test]
    fn overlapping_names_show_keeps_every_plate_the_nearest_on_top() {
        let eye = Vec3::ZERO;
        // w = z: a perspective's depth, x and y over it
        let proj = Mat4::from_cols(glam::Vec4::X, glam::Vec4::Y, glam::Vec4::new(0.0, 0.0, 1.0, 1.0), glam::Vec4::ZERO);
        let p = [plate([0.0, 0.0, 12.0], "Cd"), plate([0.0, 0.0, 10.0], "Ab"), plate([0.0, 0.0, 14.0], "Ef")];
        let shown = |view: &PlateView| -> Vec<String> { labels_by(&p, view, proj, width, 12).into_iter().map(|l| l.text).collect() };
        assert_eq!(shown(&PlateView { overlap: true, ..PlateView::game(eye) }), ["Ef", "Cd", "Ab"]);
        assert_eq!(shown(&PlateView::game(eye)), ["Ab"], "the control: the game's keeps the nearest alone");
    }
}

#[cfg(test)]
mod own_actor {
    use super::*;

    /// Each of `UpdateGame`'s arms: first person hides it (a dead player's
    /// keeps what it had), the follow camera hides it inside the head only,
    /// the others show it, and npc flag 0x200 hides it whatever the camera.
    /// The control: a rule that always shows it (as before) fails.
    #[test]
    fn first_person_and_a_camera_in_the_head_hide_the_player() {
        let near = F::new(0.5);
        let far = F::new(3.0);
        let cases = [
            ((true, 0, false, far, 0), (false, false)),
            ((true, 0, true, far, 0), (true, false)),
            ((false, 0, true, far, 0), (false, false)),
            ((true, 2, false, near, 0), (false, true)),
            ((false, 2, false, far, 0), (true, false)),
            ((true, 2, false, F::new(0.8), 0), (true, false)),
            ((false, 1, false, near, 0), (true, false)),
            ((false, 3, false, near, 0), (true, false)),
            ((true, 3, false, far, 0x200), (false, false)),
            ((true, 3, false, far, 0x1FF), (true, false)),
        ];
        for ((was, mode, dead, d, flags), want) in cases {
            assert_eq!(own_actor_shown(was, mode, dead, d, flags, false), want, "{:?}", (was, mode, dead, d, flags));
        }
        let always = |_: bool, _: i32, _: bool, _: F<Ee>, _: u16| (true, false);
        assert!(cases.iter().any(|&((w, m, de, d, f), want)| always(w, m, de, d, f) != want), "the control must fail");
        // the manual camera all the way in hides it as first person does,
        // whatever the camera (a dead player's keeps what it had); the
        // control: the same cases without it show it
        for mode in [1, 2, 3] {
            assert_eq!(own_actor_shown(true, mode, false, far, 0, true), (false, false), "mode {mode}");
            assert_eq!(own_actor_shown(true, mode, true, far, 0, true), (true, false), "mode {mode}, dead");
            assert_eq!(own_actor_shown(true, mode, false, far, 0, false), (true, false), "mode {mode}: the control");
        }
    }
}

/// What `UpdateCreatures` does with a creature's sprite each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreatureLoco {
    /// `SetDeath` (once)
    Death,
    /// its walk control's `UpdateSprite`
    Walk,
    /// `SetLocomotion(state)`
    State,
}

/// `EQOA_WATCH=<name>`: a line a frame on the creature of that name (where
/// it is, where it is drawn, its state, its walk's, its animation), and,
/// when it is the player's own, one for each 0x40 sent (where, how fast,
/// its locomotion): how another player looks on the move, from both ends.
fn watching(name: &str) -> bool {
    static WATCH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    WATCH.get_or_init(|| std::env::var("EQOA_WATCH").ok()).as_deref().is_some_and(|w| w.eq_ignore_ascii_case(name))
}

/// `UpdateCreatures`' order: state 14 is death first, walking (`m_p255`)
/// or not; then a walking creature follows its walk control, and any other
/// takes its state as its locomotion.
fn creature_loco(state: u8, walks: bool) -> CreatureLoco {
    if state == 14 {
        CreatureLoco::Death
    } else if walks {
        CreatureLoco::Walk
    } else {
        CreatureLoco::State
    }
}

#[cfg(test)]
mod creature_sprite {
    use super::*;

    /// A creature that dies on the move dies: the death comes before the
    /// walk. The control: the walk first, as the client had it, leaves it
    /// walking.
    #[test]
    fn a_walking_creature_that_dies_dies() {
        let cases = [((14, true), CreatureLoco::Death), ((14, false), CreatureLoco::Death), ((1, true), CreatureLoco::Walk), ((3, false), CreatureLoco::State), ((0, false), CreatureLoco::State)];
        for ((state, walks), want) in cases {
            assert_eq!(creature_loco(state, walks), want, "state {state}, walks {walks}");
        }
        let walk_first = |state: u8, walks: bool| if walks { CreatureLoco::Walk } else { creature_loco(state, false) };
        assert!(cases.iter().any(|&((s, w), want)| walk_first(s, w) != want), "the control must fail");
    }
}

#[cfg(test)]
mod far_poses {
    use super::*;

    /// How many far characters `frame` poses on each of `frames` frames
    /// drawn with `far` of them in view, each in its slot's turn, through
    /// the turns `every` gives.
    fn posed(far: u32, frames: u32, every: impl Fn(u32) -> u32) -> Vec<u32> {
        (0..frames).map(|d| (0..far).filter(|&slot| d.wrapping_add(slot).is_multiple_of(every(far))).count() as u32).collect()
    }

    /// A few far characters are posed every frame, as near ones are; a
    /// crowd takes turns, about `FAR_POSE_BUDGET` a frame. The control: one
    /// frame in four however few (the client's before) poses a quarter of
    /// Sandstorm's 24.
    #[test]
    fn far_characters_are_posed_every_frame_unless_a_crowd() {
        for few in [1, 3, 24, FAR_POSE_BUDGET] {
            assert!(posed(few, 8, far_pose_every).iter().all(|&n| n == few), "{few} far: every one, every frame");
        }
        assert!(posed(200, 8, far_pose_every).iter().all(|&n| n == 50), "200 far: fifty a frame");
        assert!(posed(204, 10, far_pose_every).iter().all(|&n| (40..=41).contains(&n)), "204 far: about forty a frame");
        assert_eq!(far_pose_every(0), 1);
        assert!(posed(24, 8, |_| 4).iter().all(|&n| n == 6), "the control: a quarter of 24");
    }
}

#[cfg(test)]
mod held {
    use super::*;

    /// A player turned into a creature keeps its weapons on its channel
    /// (Sandstorm's bard in Mouse Form, `pcaps/ps2_main_capture.pcap`: both
    /// hands 0xF4DD991A, type 5): the creature's sprite holds no items, so
    /// none is attached and both hands swing hand to hand. The same items on
    /// a sprite that holds them hang on its nodes, the override first. The
    /// control: the nodes alone, as the client went by before, attach both
    /// weapons to the creature.
    #[test]
    fn a_form_shows_no_weapons() {
        let mut c = net::entity::Creature { item_types: [5, 5], ..Default::default() };
        c.items = [0xF4DD991A, 0xF4DD991A, 0];
        let mut form = CSprite::<Ee>::new(vi::hsprite::HSprite::new(0), 2, 9, 0, F::ONE, false);
        form.item_nodes[..2].copy_from_slice(&[6, 10]);
        assert_eq!(held_items(&form, &c), [None; 3]);
        assert_eq!(crate::spells::hand_attack_types(&c, form.holds_items), [11, 11]);
        let by_node: Vec<usize> = (0..3).filter(|&i| c.items[i] != 0 && form.item_nodes[i] >= 0).collect();
        assert_eq!(by_node, [0, 1], "the control: by the nodes alone both weapons hang on the creature");

        let mut human = CSprite::<Ee>::new(vi::hsprite::HSprite::new(0), 1, 0, 0, F::ONE, true);
        human.item_nodes[..3].copy_from_slice(&[18, 27, 28]);
        c.items_over[1] = 0x1234;
        assert_eq!(held_items(&human, &c), [Some((0xF4DD991A, 18)), Some((0x1234, 27)), None]);
        assert_eq!(crate::spells::hand_attack_types(&c, human.holds_items), [5, 5]);
    }

    /// A weapon swapped with nothing else changed is attached again
    /// (`Apply` tests each hand's item), on the sprite as it is; so is an
    /// override taking a hand. A new model or look makes the creature anew,
    /// and an item's type alone changes nothing, as `Apply` tests only the
    /// item's model. A creature that holds no items has nothing to attach.
    /// The control: going by the model and look alone, as the client did
    /// before, keeps the old weapon in the hand.
    #[test]
    fn a_swapped_weapon_is_attached_again() {
        let mut human = CSprite::<Ee>::new(vi::hsprite::HSprite::new(0), 1, 0, 0, F::ONE, true);
        human.item_nodes[..3].copy_from_slice(&[18, 27, 28]);
        let mut c = net::entity::Creature { item_types: [1, 11], ..Default::default() };
        c.items = [0xF4DD991A, 0, 0];
        let state = |cs: &CSprite<Ee>, c: &net::entity::Creature, model: u32| (model, Some(look_of(c)), held_items(cs, c));
        let was = state(&human, &c, 0xC698D870);
        assert_eq!(redo(was, was), Redo::Keep);

        let mut swapped = c.clone();
        swapped.items[0] = 0x845FECDD;
        assert_eq!(redo(was, state(&human, &swapped, 0xC698D870)), Redo::Items);
        let mut over = c.clone();
        over.items_over[1] = 0x1234;
        assert_eq!(redo(was, state(&human, &over, 0xC698D870)), Redo::Items);
        let mut retyped = c.clone();
        retyped.item_types[0] = 5;
        assert_eq!(redo(was, state(&human, &retyped, 0xC698D870)), Redo::Keep);
        let mut dyed = swapped.clone();
        dyed.armor_tint[0] = 3;
        assert_eq!(redo(was, state(&human, &dyed, 0xC698D870)), Redo::Build);
        assert_eq!(redo(was, state(&human, &swapped, 0x9BBCEEB9)), Redo::Build);

        let mut form = CSprite::<Ee>::new(vi::hsprite::HSprite::new(0), 2, 9, 0, F::ONE, false);
        form.item_nodes[..2].copy_from_slice(&[6, 10]);
        assert_eq!(redo(state(&form, &c, 0x8150E6AF), state(&form, &swapped, 0x8150E6AF)), Redo::Keep);

        let by_model_and_look = |a: (u32, Option<vi::cust::Look>), b: (u32, Option<vi::cust::Look>)| if a != b { Redo::Build } else { Redo::Keep };
        let now = state(&human, &swapped, 0xC698D870);
        assert_eq!(by_model_and_look((was.0, was.1), (now.0, now.1)), Redo::Keep, "the control: by the model and look alone the old weapon stays");
    }
}

pub struct Gpu<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

impl Play {
    pub fn new(session: Session, player: Player, dump: Box<net::dump::MemoryDump>) -> Self {
        // the server's patch (`s2`): as the patcher left its folder, or
        // `EQOA_S2_DIR`'s
        crate::s2::begin_session();
        // a world entry shows what Hide UI hid (`hideui`)
        crate::hideui::show();
        // Msg000D reads `m_speed` out of the dump
        let speed = dump.speed;
        let mut p = Play { session, player, dump, entities: Entities::default(), view_sent: None, last_send: 0, puppets: (0..MAX_SLOTS).map(|_| None).collect(), cues: vec![crate::sound::Cue::Named("respawn", 0.4)], weather: Vec::new(), abilities: Default::default(), ability_token: 0, presses: Default::default(), hud_menus: Default::default(), slot_asked: None, gear: Default::default(), walks: (0..MAX_SLOTS).map(|_| None).collect(), actors: vec![None; MAX_SLOTS], self_actor: None, actors_in: 0, refresh_collision: false, self_model: None, self_puppet: None, self_shown: true, cam_inside: false, cam_carry: None, chat: Default::default(), typing: None, target: u32::MAX, auto_attack: false, attack: Default::default(), swung: 0, stats: Default::default(), effects: Default::default(), damage: Default::default(), rings: Default::default(), self_at: None, words: None, ui_textures: None, ui_data: None, ui_assets: None, ui_art: None, modules_taken: [false; 2], hud: None, target_info: None, target_title: String::new(), target_guild: String::new(), info_up: false, pet_id: u32::MAX, pet_name: String::new(), info_window: Default::default(), chat_pending: Vec::new(), wnd_chat: None, hud_last: 0, channel: 0, reply_name: String::new(), mode: 0, dialogs: Default::default(), priv_chat: Default::default(), response: Default::default(), pending_box: None, world_change: None, arrival: Vec::new(), group: Default::default(), mbox: Default::default(), trails: Default::default(), trail_strips: Vec::new(), race_cams: None, light: Default::default(), cam_preset: Default::default(), cam_saved: Default::default(), cam_entered: false, race_framed: false, race_frame_due: None, death: None, fade: Default::default(), dead_casters: Vec::new(), death_ms: 0, death_eye: None, start: None, quit: false, booted: false, disconnected: None, start_open: None, start_keys: Vec::new(), emote: None, edit: Default::default(), edit_keys: Vec::new(), who: Default::default(), who_wnd: Default::default(), who_open: false, search_mode: false, search: Default::default(), who_keys: Vec::new(), who_tell: None, edit_typed: 0, spells: None, projectiles: Default::default(), casts: Vec::new(), spell_ids: vec![None; MAX_SLOTS], spell_rand: Rand(1), book: Default::default(), book_open: false, book_keys: Vec::new(), items: Default::default(), waiting: false, quests: Default::default(), quest_log: Default::default(), quest_open: false, quest_from_hud: false, quest_keys: Vec::new(), guide: Default::default(), server_quests: false, guide_from: None, guide_forced: std::env::var_os("EQOA_QUEST_GUIDE").is_some(), trainer: Default::default(), trainer_open: false, trainer_refresh: false, trainer_keys: Vec::new(), cm: Default::default(), cm_wnd: Default::default(), cm_open: false, cm_msgs: Vec::new(), cm_keys: Vec::new(), factions: Default::default(), faction_wnd: Default::default(), faction_open: false, faction_keys: Vec::new(), npcw: Default::default(), options: crate::options::GameOptions { compact: std::env::var_os("EQOA_SOFT_KEYBOARD").is_none() as u8, ..Default::default() }, options_wnd: Default::default(), sound_ops: Vec::new(), apply_camera: false, filters_sent: None, shouts_told: false, shouts_dropped: false, buddies: Default::default(), buddy_wnd: crate::social::BuddyList::new(0), ignore: Default::default(), ignore_wnd: crate::social::IgnoreList::new(0), panel_open: false, panel_keys: Vec::new(), controls_open: false, guild: Default::default(), guild_cmds: Default::default(), guild_wnd: Default::default(), guild_open: None, guild_keys: Vec::new(), craft: Default::default(), recipe_wnd: Default::default(), exp_wnd: Default::default(), craft_keys: Vec::new(), craft_outs: Vec::new(), mail: Default::default(), support: Default::default(), bank: Default::default(), autorun: false, following: false, follow_id: u32::MAX, cast_window: (0, 0), rooted: false, locks: (true, true), aim_hold: false, aim_at: 0, manual_on: false, reticle_alpha: F::new(0.5), pad_mode: 0, last_group: false, keyboard: true, rumbles: Vec::new(), glyphs: Default::default(), wheel: 0.0, key_queue: Default::default(), held: None, quick: Default::default(), quick_held: None, alarm: Default::default(), quick_typed: (0, 0), map: Default::default(), loading: Default::default(), speed, walk_speed: 3.0, drawn: 0, far_last: 0, voice: crate::voice::PlayVoice::for_platform(), voice_wnd: Default::default() };
        p.take_abilities();
        p.npcw_world_entry();
        // world entry: LoadModels and StreamFillCache under the loading screen
        let now = p.session.now() as i64;
        p.loading.begin(p.player.pos, p.player.hpr[0], false, false, now);
        // the options kept (`settings`; `VIWndInitFromMC`'s Load), and the
        // two sound classes' volumes set from them as it sets them
        p.options = crate::settings::game_options();
        p.sound_ops.push(crate::options::OptionsUp::Volume(0, p.options.fx_volume));
        p.sound_ops.push(crate::options::OptionsUp::Volume(1, p.options.music_volume));
        p
    }

    fn self_id32(&self) -> Option<u32> {
        self.session.self_id.map(|s| s as u32)
    }

    /// What play reads out of CLIENT's image: the start menu (its menus and
    /// `gWorldInfo`'s bounds) and the race cameras.
    pub fn client_data(&mut self, img: &crate::client_image::ClientImage) {
        self.start = Some(crate::startmenu::StartData::from_image(img));
        self.race_cams = Some(crate::camstyle::race_cameras(img));
        self.map.client_data(img);
    }

    /// CLIENT's and SUPPORT's data, taken up as each image comes
    /// (`modules`): the start menu and the race cameras; the HUD's atlas
    /// (the viewer builds the HUD once it is here, `attach_ui`) and the UI
    /// sounds' volumes.
    fn take_modules(&mut self, sound: Option<&mut crate::sound::Sound>) {
        use crate::modules::{Module, get};
        if !self.modules_taken[0]
            && let Some(img) = get(Module::Client)
        {
            self.modules_taken[0] = true;
            // CLIENT's data as the server's patch leaves it (`s2`)
            if let Some(c) = crate::client_image::ClientImage::from_bytes(img.to_vec()) {
                self.client_data(&c.patched(crate::s2::get().image.clone()));
            }
        }
        if !self.modules_taken[1]
            && let Some(img) = get(Module::Support)
        {
            self.modules_taken[1] = true;
            if let (Some(atlas), Some(textures)) = (vi::ui::read_atlas(&img), self.ui_textures.take()) {
                self.ui_data = Some((atlas, textures));
            }
        }
        if let Some(s) = sound {
            s.take_support();
        }
    }

    /// The HUD's UI once it is built (its fonts and atlas, the layer's
    /// textures): what the report's dialog draws with.
    pub fn ui_parts(&self) -> Option<(&vi::ui::Vui, &crate::ui::UiAssets)> {
        self.hud.as_ref().map(|h| (&h.0, &h.1))
    }

    /// What a report says of play (`report`): the character, where it is
    /// (`at`, the walker's), the entities round it and what has come of
    /// CLIENT and SUPPORT.
    pub fn report(&self, at: [f32; 3]) -> String {
        use std::fmt::Write;
        let mut o = String::new();
        let p = &self.player;
        let _ = writeln!(o, "character: {} (server id {}), level {}, race {}, class {}", p.name, p.server_id, self.dump.player.level, p.race, p.class);
        // the world's file as the disc's table names it (the dump's path is
        // the login's, and a teleport into another world does not change it)
        let file = crate::s2::get().world_file(p.world).unwrap_or_else(|| p.world_path.clone());
        let _ = writeln!(o, "world {} ({file}), at {at:?}", p.world);
        let _ = writeln!(o, "server's patch: {}", crate::s2::get().describe());
        let target = if self.target == u32::MAX { "none".to_string() } else { format!("{:#x}", self.target) };
        let _ = writeln!(o, "mode {}, target {target}, dead {}, autorun {}, auto-attack {}", self.mode, self.is_dead(), self.autorun, self.auto_attack);
        let start = match (&self.start, self.modules_taken[0]) {
            (Some(_), _) => "in",
            (None, true) => "CLIENT came but was not read",
            (None, false) => "waiting for CLIENT",
        };
        let hud = match (&self.hud, &self.ui_data, self.modules_taken[1]) {
            (Some(_), _, _) => "built",
            (None, Some(_), _) => "its atlas is in, not built yet",
            (None, None, true) => "SUPPORT came, but no atlas or no TEXTURES.TXT",
            (None, None, false) => "waiting for SUPPORT",
        };
        let _ = writeln!(o, "start menu: {start}; HUD: {hud}");
        if self.search.open || !self.search.list().is_empty() {
            let [race, class, min, max] = self.search.filter();
            let _ = writeln!(o, "player search: {}, race {race} class {class} levels {min}-{max}, {} found", if self.search.open { "open" } else { "closed" }, self.search.list().len());
        }
        if self.voice_wnd.open {
            let _ = writeln!(o, "{}", self.voice_wnd.describe());
        }
        let _ = writeln!(o, "{}", self.map.describe());
        if let Some(w) = self.loading.wait {
            let _ = writeln!(o, "loading screen: up {} ms, for the zones round {:?}{}", self.session.now() as i64 - w.since, w.at, if w.entering { ", the world's file first" } else { "" });
        }
        let _ = writeln!(o, "entities: {} of {}, drawn to {} (the server's most {:?})", self.entities.slots.iter().flatten().count(), self.entities.limit, crate::settings::entity_view(), self.session.entity_view_offer);
        let now = self.session.now();
        for e in self.entities.slots.iter().flatten() {
            let name = e.creature.as_ref().map_or("", |c| c.name.as_str());
            let _ = writeln!(o, "  {:#x} kind {} model {:#x} {name:?} at {:?}", e.id, e.kind, e.model, e.pos);
            if let Some(c) = &e.creature {
                let _ = writeln!(o, "    {}", look_line(c));
                let _ = writeln!(o, "    {}", move_line(e, c, now));
            }
        }
        o
    }

    /// One frame. Returns the player's model when the server has just named
    /// a different one (the caller re-dresses the walker). `game_frame`: one
    /// of the game's frames (`frames.rs`), which what it counts in frames
    /// steps on.
    pub fn frame(&mut self, walker: &mut Walker, lib: &Library, mut g: Gpu, game_ms: i64, game_frame: bool, mut sound: Option<&mut crate::sound::Sound>) -> Option<u32> {
        self.take_modules(sound.as_deref_mut());
        self.drawn = self.drawn.wrapping_add(1);
        let now = self.session.now();
        // the loading screen's gears, turning by the clock (`SetTime(now)`)
        self.loading.frame(lib, g.renderer, g.device, g.queue, now as i64);
        let mut new_model = None;
        self.actors_scene(walker);
        for e in self.session.pump() {
            match e {
                Event::TargetInfo { disposition, b, title, guild, .. } => {
                    self.target_info = Some(disposition);
                    self.rings.con = Some(b);
                    self.target_title = title;
                    self.target_guild = guild;
                }
                Event::ReplyName(n) => self.reply_name = n,
                // Msg0045: "Profanity Filter: On" or "Off" (550, 551, 552)
                Event::ProfanityFilter(on) => {
                    let w = |n: i32| self.words.as_ref().map_or_else(|| format!("(word {n})"), |w| w.get(n, 0).to_string());
                    let line = format!("{}: {}", w(550), w(if on { 551 } else { 552 }));
                    self.add_chat(line, [1.0; 4]);
                }
                Event::Dialog { text, .. } => {
                    if crate::questguide::is_finished(&text) {
                        self.guide_finished(now);
                    }
                    self.show_dialog(&text);
                }
                Event::DialogBox { id, text, options } => self.dialog_box(id, &text, &options),
                Event::Opcode { op: 0x005E, .. } => self.msg_005e(walker),
                Event::Teleport { world, pos, hpr, token } => self.msg_00b2(walker, world, pos, hpr, token, now as i64),
                // 0x00F8: `m_speed`, then SetSpeed (a speed buff, a snare)
                Event::Speed(v) => self.set_speed(v),
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
                Event::SummonPrompt { name, kind } => self.summon_prompt(&name, kind),
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
                Event::QuickSlot { slot, pos } => crate::inventory::quick_slot(&mut self.dump.inventory, slot as usize, pos, self.gear.on),
                Event::GearSet { slot, name, main, off } => {
                    eprintln!("play: weapon set {slot} {name:?}: items {} and {} (0x0021)", main as i32, off as i32);
                    self.gear.set(slot, name, main, off);
                }
                Event::ItemHp { id, hp } => {
                    // Msg0060: the hit points and sound 1
                    if let Some(k) = crate::inventory::find(&self.dump.inventory, id) {
                        crate::inventory::set_hp(&mut self.dump.inventory, k, hp as i64);
                    }
                    self.cues.push(crate::sound::Cue::Ui(1));
                }
                Event::Opcode { op: 0x0E02, data } => self.who.recv(&data),
                // a server's player search (`playersearch`, `s2`): 0x1501
                // empties the list, a 0x1500 is a player found. The patch's
                // window keeps them whether it is up or not, as its handler
                // does; the who window too while it shows the search
                Event::Opcode { op: 0x1501, .. } if crate::s2::get().player_search => {
                    self.search.clear();
                    if self.search_mode {
                        self.who.list.clear();
                        self.who.pending = false;
                    }
                }
                Event::Opcode { op: 0x1500, data } if crate::s2::get().player_search => {
                    self.search.found(&data);
                    if self.search_mode
                        && let Some(f) = crate::playersearch::Found::parse(&data)
                    {
                        self.who.list.push(f.row());
                        self.who.pending = false;
                    }
                }
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
                // the training points as they now are (a level's, a refund's)
                Event::Opcode { op: 0x0036, data } => self.msg_0036(&data, &mut walker.cam, &mut walker.scene),
                Event::Opcode { op: 0x001C, data } => self.msg_001c(&data),
                // OutMatched: the fight refused, said in red, auto-attack off
                Event::Opcode { op: 0x077A, .. } => self.msg_077a(),
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
                    // Msg1003Done (not for a rename): every creature's
                    // collision refreshed against the list
                    if op != 0x1006 {
                        self.refresh_collision = true;
                    }
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
                Event::Assist(id) => self.msg_007b(id),
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
                    if self.slot_asked.is_some_and(|(i, _, _)| i == index as i64) {
                        self.slot_asked = None;
                    }
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
                Event::State { channel, data } if let Some(slot) = net::entity::slot_of_channel(channel) => {
                    let self_id = self.session.self_id;
                    let skel = self.puppets[slot].as_ref().and_then(|p| p.sprite.as_ref()).map_or(0, |s| s.skel_type);
                    let scene = &mut walker.scene;
                    let walk = &mut self.walks[slot];
                    let had = self.actors[slot];
                    self.entities.channel(slot, &data, self_id, now, UPDATE_RATE, |e| {
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
                            w.parm64 = had.filter(|a| a.id == e.id).map_or(-1, |a| a.actor as i32);
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
                Event::Disconnected(r) => {
                    let text = net::session::disconnect_word(r)
                        .and_then(|w| self.words.as_ref().map(|t| t.get(w, 0).to_string()))
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| format!("Disconnected (reason {r})."));
                    eprintln!("play: disconnected (reason {r}): {text}");
                    #[cfg(target_arch = "wasm32")]
                    if !self.booted {
                        crate::web::disconnected(&text);
                    }
                    self.disconnected = Some(r);
                }
                Event::Chat(text, color) => self.server_line(text, color),
                // CastingCombat (0x00B1) and Msg0776: an effect to play; one
                // the player cast itself (0x0776) it has already played.
                Event::Opcode { op, data } => {
                    if self.bank_opcode(op, &data) || self.npcw_opcode(op, &data) || self.support_opcode(op, &data) {
                    } else if op == 0x00DB {
                        self.damage_indication(&data, walker.cam.camera_mode);
                    } else if op == 0x00FC {
                        // m_p06DC8: held in place
                        self.rooted = data.first().is_some_and(|&b| b != 0);
                    } else if let Some(c) = crate::spells::cast_of(op, &data) {
                        // CastingCombat: a cast of the player's own stamps the
                        // cast window, through which the walk walks
                        if op == 0x00B1 && Some(c.caster) == self.self_id32() {
                            let now = self.session.now();
                            let mut end = now.wrapping_add(c.time as u32);
                            if (end.wrapping_sub(self.cast_window.1) as i32) < 0 {
                                end = self.cast_window.1;
                            }
                            self.cast_window = (now, end);
                        }
                        if !(op == 0x0776 && Some(c.caster) == self.self_id32()) {
                            self.casts.push(c);
                        }
                    }
                    }
                Event::Weather { weather, instant } => {
                    // stamped with the server's hour, which every window shares
                    let hour = self.session.time_of_day().map_or(String::new(), |h| format!(" at hour {:.4}", h.0));
                    eprintln!("play: weather {weather}{}{hour}", if instant { " (instant)" } else { "" });
                    self.weather.push(Atmos::Weather(weather, instant));
                }
                Event::WorldEntry { self_id, player, dump } => {
                    eprintln!("play: world entry again as {self_id:#x} at {:?}", player.pos);
                    // Msg000D in play: EnterWorld, then SetLocation and
                    // SetOrbitAngleBehind at the dump's place (a respawn is
                    // a dump at the bind point), another world's file read
                    // first, black until it is in (`world_entered`)
                    let mut elsewhere = false;
                    let (arrival, heading, here) = (player.pos, player.hpr[0], player.world == self.player.world);
                    let far = !here || crate::loading::far(walker_at(walker), arrival);
                    if player.world != self.player.world {
                        match crate::s2::get().world_file(player.world) {
                            Some(f) => {
                                eprintln!("play: the dump's world {} ({f}) at {:?}", player.world, player.pos);
                                // SetLocation at the dump's place, as it is
                                self.world_change = Some((f.to_string(), Spawn { pos: player.pos, heading: player.hpr[0], placing: Placing::AsGiven }));
                                elsewhere = true;
                            }
                            None => eprintln!("play: the dump's world {}, which the disc's world table does not have", player.world),
                        }
                    } else {
                        walker.place(player.pos, player.hpr[0]);
                    }
                    self.filters_sent = None;
                    self.player = player;
                    // Msg000D reads `m_speed` out of the dump (no SetSpeed)
                    self.speed = dump.speed;
                    self.dump = dump;
                    self.take_abilities();
                    self.respawned(walker);
                    if elsewhere {
                        self.fade.set(1.0, 0.0);
                        // the walker is built again there (`world_entered`)
                        self.cam_carry = self.cam_entered.then_some((self.cam_saved.mode, None));
                    }
                    if here || elsewhere {
                        self.loading.begin(arrival, heading, elsewhere, false, now as i64);
                        if far {
                            self.loading.hold_far();
                        }
                    }
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
                    // and its SetSky(m_camera, 1): the sky where it stands, at once
                    self.weather.push(Atmos::SkyNow);
                }
                _ => {}
            }
        }
        self.agree_entity_view();
        // Mana Bars: a server that offers others' power told whether to send it
        self.session.agree_power_bars(crate::settings::client().mana_bars);
        // Quest Markers: a server that offers quest marks told whether to send them
        // (none while a scripted run has the guide make them)
        self.session.agree_quest_marks(crate::settings::client().quest_markers && !self.guide_forced);
        self.session.agree_quest_progress(crate::settings::client().quest_tracker && !self.guide_forced);
        self.guide_wait(now);
        // Voice Chat: the server's voice, or the demo's (`voiceplay`)
        self.voice_frame(now as i64);
        self.drop_dead_target_info();
        if self.has_self() {
            self.auto_attack_tick(walker, now);
            self.self_visibility(walker);
            self.options_tick();
            self.tell_shouts();
        }
        // what the Options window asked of the sound device and the camera
        // (kept until there is a device: the volumes loaded with the options)
        for op in if sound.is_some() { std::mem::take(&mut self.sound_ops) } else { Vec::new() } {
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
        // UpdateGame: back in play, the next waiting dialog (OpenDialog's
        // b 1).
        if self.mode == 0
            && let Some(d) = self.dialogs.pop_front()
        {
            self.open_dialog(&d, true);
        } else if self.mode == 0
            && let Some(d) = self.pending_box.take()
        {
            self.show_dialog_box(d);
        } else if self.mode == 0
            && let Some(i) = self.group.invites.first()
        {
            let name = i.name.clone();
            self.show_invite(&name);
        } else if self.mode == 0 && self.alarm.on {
            let words = self.words.clone();
            let w = move |n: i32| words.as_ref().map_or_else(|| format!("(word {n})"), |t| t.get(n, 0).to_string());
            let now = self.session.now() as i64;
            if let Some(o) = self.alarm.tick(now, &w) {
                self.alarm_outs(vec![o]);
            }
        }
        let self_id = self.session.self_id;
        // UpdateGame: the player's own actor where its walk has taken it,
        // before the other creatures walk
        self.place_self_actor(walker, lib);
        // UpdateCreatures / VICreatureEntity::Walk: a creature with its own
        // walk physics (a player, +0x255) is stepped by it (it stops at the
        // other actors, its own passed over); any other is carried along its
        // velocity since it was last moved (ExtrapolateMove).
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
                w.parm64 = self.actors[slot].filter(|a| a.id == e.id).map_or(-1, |a| a.actor as i32);
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
        self.position_due(&walker.cam.walk.committed, now);
        let me = self.self_id32();
        // the far characters' turns, by how many were far the last frame
        let every = far_pose_every(self.far_last);
        let mut far = 0;
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
                apply(lib, &mut g, &mut self.self_puppet, e, look, slot, game_ms);
            }
            // a creature's sounds are shared while it is loaded
            let sounds_of = |p: &Option<Puppet>| p.as_ref().and_then(|p| p.sprite.as_ref()).map(|cs| cs.sounds()).unwrap_or_default();
            let Some(e) = want else {
                if let Some(s) = sound.as_deref_mut() {
                    let_go(s, &self.puppets[slot]);
                }
                self.puppets[slot] = None;
                continue;
            };
            let look = e.creature.as_ref().map(look_of);
            if let Some(old) = apply(lib, &mut g, &mut self.puppets[slot], e, look, slot, game_ms)
                && let Some(s) = sound.as_deref_mut()
            {
                s.share_sounds(lib, &sounds_of(&self.puppets[slot]));
                let_go(s, &old);
            }
            let Some(p) = self.puppets[slot].as_mut() else { continue };
            let f = F::<Ee>::new;
            let actor = Matrix44::transform(
                &Vect3::new(f(e.actor_hpr[0]), f(e.actor_hpr[1]), F::ZERO),
                f(if e.scale > 0.0 { e.scale } else { 1.0 }),
                &Vect3::new(f(e.actor_pos[0]), f(e.actor_pos[1]), f(e.actor_pos[2])),
            );
            // As the game rasters it: past its off distance not at all
            // (`VIScene::RasterActor`, at the actor's place), and of an LOD
            // body the one level its distance picks (`VILODSprite::Raster`),
            // from no farther than the game's camera could be (`lod::eye`);
            // out to NPC/PC Distance where that is farther (`lod`), and
            // there posed in its turn (`far_pose_every`), past it not at all.
            let at = Vect3::new(f(e.actor_pos[0]), f(e.actor_pos[1]), f(e.actor_pos[2]));
            let eye = crate::lod::eye(&walker.cam, &walker.eye.position);
            let lod_on = crate::settings::client().lod;
            let shown = crate::lod::within(&at, &eye, crate::lod::view_dist(&p.gpu.lod_dists));
            let near = crate::lod::within(&at, &eye, crate::lod::off_dist(&p.gpu.lod_dists));
            far += (shown && !near) as u32;
            let pose = p.first || near || (shown && self.drawn.wrapping_add(slot as u32).is_multiple_of(every));
            let (matrix, palette) = match p.sprite.as_mut() {
                Some(cs) => {
                    p.anims.time = game_ms;
                    // UpdateCreatures: a dead creature's sprite dies, a walking
                    // one's follows its walk control, any other takes
                    // SetLocomotion(state) (`creature_loco`).
                    if let Some(c) = &e.creature {
                        let walk = self.walks[slot].as_ref().map(|(_, w)| w).filter(|_| c.p255 != 0);
                        match (creature_loco(c.state, walk.is_some()), walk) {
                            // laid down at once for a corpse seen first, its
                            // spells ended
                            (CreatureLoco::Death, _) => {
                                if !cs.dead {
                                    cs.set_death(true, p.first, &p.anims, &mut p.rand, None);
                                    self.dead_casters.push(slot as i32);
                                }
                            }
                            (CreatureLoco::Walk, Some(w)) => {
                                w.update_sprite(cs, &p.anims, &mut p.rand);
                            }
                            _ => cs.set_locomotion(c.state as i32, &p.anims, &mut p.rand),
                        }
                        if watching(&c.name) {
                            let w = self.walks[slot].as_ref().map(|(_, w)| w);
                            eprintln!(
                                "[watch] {game_ms} {} pos {:.2?} drawn {:.2?} state {} p255 {} walk {:?} vel {:.2?} anim {:?} stamp {} pose {} frame {}",
                                c.name,
                                e.pos,
                                e.actor_pos,
                                c.state,
                                c.p255,
                                w.map(|w| (w.committed.loco_state, w.phys.flags)),
                                c.velocity,
                                [cs.cur[0].anim, cs.cur[1].anim],
                                e.stamp,
                                pose,
                                if cs.cur[0].play >= 0 { cs.h.plays.get(cs.cur[0].play).frame_a } else { -1 }
                            );
                        }
                    }
                    // VICSprite::Process: the sounds its triggers fired
                    if let Some(s) = sound.as_deref_mut() {
                        cs.process_sounds(&walker.eye.position, &walker.eye.direction, &at, Some(&mut s.sink()));
                    }
                    // VIHSprite::Raster's Process at the skeleton's level for
                    // its distance, from the eye the levels go by
                    // (`lod::anim_level`; every node with Level of Detail off)
                    if pose {
                        cs.h.process(crate::lod::anim_level(&cs.h, &actor, &eye, lod_on), &p.anims);
                    }
                    p.first = false;
                    let m = cs.h.calc_trans_about_center(&actor, &p.anims);
                    crate::trails::process(&mut self.trails, &mut p.trail, cs, &p.weapons, &m, game_ms);
                    if pose {
                        cs.h.set_blend_matrices();
                    }
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
                    (m.to_cols(), if pose { cs.h.blend.iter().map(|m| m.to_cols()).collect::<Vec<_>>() } else { Vec::new() })
                }
                None => (actor.to_cols(), Vec::new()),
            };
            p.actor_m = actor;
            p.centre = glam_to_matrix(matrix);
            g.renderer.set_instances(g.queue, &p.gpu, &[matrix]);
            if !palette.is_empty() {
                g.renderer.set_palette(g.queue, &p.gpu, &palette);
            }
            crate::lod::place(&mut p.gpu, &p.centre, &eye);
            p.gpu.visible = shown && skins_shown(e.creature.as_ref());
            p.shown = shown;
            for (_, a) in &mut p.attached {
                a.visible = shown;
            }
        }
        self.far_last = far;
        // Extrapolate: each actor where the entity is drawn
        self.place_actors(walker, lib);
        self.spells_frame(walker, lib, game_ms, sound);
        self.draw_projectiles(walker, lib, &mut g);
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
        // the options' Camera Distance as it moves: the follow orbit's ceiling
        if walker.cam.camera_mode == 3 {
            walker.cam.max_orbit_distance = F::new(crate::settings::camera_distance());
        }
        // and its Camera Turn Speed: the view's turn by the stick and the keys
        crate::camstyle::turn_speed(&mut walker.cam, crate::settings::camera_turn());
        // and its Screen Shake: off, the spell effects' shake ended before
        // the camera applies it
        crate::camstyle::screen_shake(&mut walker.cam, crate::settings::screen_shake());
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
                    self.locks = (false, false);
                }
            }
            Some(false) if self.race_framed => {
                crate::camstyle::apply(&mut walker.cam, &self.cam_preset);
                self.locks = (true, true);
            }
            _ => {}
        }
        // DeathCamera: the eye swung round the body, the fade at 4 s; a step
        // a frame of the game's (it rises and turns by a constant a frame),
        // the eye it left kept between them
        match self.death.as_mut() {
            Some(d) => {
                if game_frame {
                    let dt = ((game_ms - self.death_ms).clamp(0, 1000)) as f32 * 0.001;
                    let body = walker.cam.walk.phys.smooth_location().to_array();
                    let (eye, dir, fade) = d.step(body, dt);
                    self.death_eye = Some((eye, dir));
                    if fade {
                        self.fade.set(1.0, 4.0);
                    }
                    self.death_ms = game_ms;
                }
                if let Some((eye, dir)) = self.death_eye {
                    walker.eye.position = eye;
                    walker.eye.direction = dir;
                }
            }
            None => {
                self.death_ms = game_ms;
                self.death_eye = None;
            }
        }
        self.manual_frame(walker, game_frame);
        // S2's pad read at the end of `VIClient::Process`: a frame counted
        if game_frame {
            self.presses.frame_end();
        }
        new_model
    }

    /// The actors made in an earlier scene are nothing in this one (a world
    /// change, or F9's art, made the walker's world again): forgotten, to be
    /// made again in it.
    fn actors_scene(&mut self, walker: &Walker) {
        if walker.scene.serial != self.actors_in {
            self.actors_in = walker.scene.serial;
            self.actors.fill(None);
            self.self_actor = None;
        }
    }

    /// `UpdateGame`'s `self->SetPos`/`SetHpr`: the player's own actor (its
    /// model's sprite) at the walk's committed smooth location and heading,
    /// its copy dead with the player's sprite. Its walk and camera pass over
    /// it (`UpdateControls`: `m_walkCam.m_parm64`), as every other walk does
    /// over its own; the other players' walks stop at it.
    fn place_self_actor(&mut self, walker: &mut Walker, lib: &Library) {
        self.actors_scene(walker);
        let me = self.self_id32();
        let c = &walker.cam.walk.committed;
        let (at, hpr) = (c.smooth_location(), Vect3::new(c.angles.x, F::ZERO, F::ZERO));
        let dead = walker.sprite.as_ref().is_some_and(|cs| cs.dead);
        let want = self.entities.slots.iter().flatten().find(|e| Some(e.id) == me && e.model != 0).map(|e| ActorAt {
            id: e.id,
            model: e.model,
            era: lib.view_for(e.model as i32),
            at,
            hpr,
            scale: actor_scale(e.scale),
            dead,
        });
        self.self_actor = place_actor(&mut walker.scene, lib, self.self_actor, want).0;
        walker.cam.walk.parm64 = self.self_actor.map_or(-1, |a| a.actor as i32);
    }

    /// `VIClient::Extrapolate`'s `SetActorPos`/`SetActorHpr` (with
    /// `VIEntity::CreateActor`/`ReleaseActor`) for every entity but the
    /// player: each one drawn has an actor of its model's sprite at its
    /// actor's place, turn (a creature with its own walk kept level) and
    /// scale, its copy dead with its sprite; a creature's is a capsule the
    /// player's walk stops at. `RefreshCollision`: a creature new, or every
    /// one after the ignore list changed, collideable unless the list names
    /// it.
    fn place_actors(&mut self, walker: &mut Walker, lib: &Library) {
        self.actors_scene(walker);
        let me = self.self_id32();
        let refresh = std::mem::take(&mut self.refresh_collision);
        for slot in 0..MAX_SLOTS {
            let e = self.entities.slots[slot].as_ref().filter(|e| Some(e.id) != me);
            let want = match (e, self.puppets[slot].as_ref()) {
                (Some(e), Some(p)) => {
                    let level = e.creature.as_ref().is_some_and(|c| c.p255 != 0);
                    let hpr = if level { [e.actor_hpr[0], 0.0, 0.0] } else { e.actor_hpr };
                    let dead = p.sprite.as_ref().is_some_and(|cs| cs.dead);
                    Some(ActorAt { id: e.id, model: p.model, era: p.era, at: v3(e.actor_pos), hpr: v3(hpr), scale: actor_scale(e.scale), dead })
                }
                _ => None,
            };
            let (a, made) = place_actor(&mut walker.scene, lib, self.actors[slot], want);
            self.actors[slot] = a;
            if let (Some(a), Some(c)) = (a, e.and_then(|e| e.creature.as_ref()))
                && (made || refresh)
            {
                let ignored = self.ignore.list.iter().any(|r| r.name == c.name);
                walker.scene.set_actor_collideable(a.actor, !ignored);
            }
        }
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
        // each creature's hands as its sprite holds them (`SetAttackAction`'s
        // types: an empty hand is 11, hand to hand, and so is every hand of a
        // sprite that holds no items)
        let hands = |e: &net::entity::Entity, p: &Option<Puppet>| {
            let holds = p.as_ref().and_then(|p| p.sprite.as_ref()).is_none_or(|cs| cs.holds_items);
            e.creature.as_ref().map_or([11, 11], |c| crate::spells::hand_attack_types(c, holds))
        };
        for (slot, (e, p)) in self.entities.slots.iter().zip(self.puppets.iter_mut()).enumerate() {
            let Some(e) = e.as_ref().filter(|e| Some(e.id) != me) else { continue };
            let attack = hands(e, p);
            match p {
                Some(p) => bodies.push(Body { actor: slot as i32, sprite: p.sprite.as_mut().map(|cs| (cs, &p.anims, &mut p.rand)), transform: p.actor_m, centre: p.centre, attack, projectile: p.projectile }),
                None => {
                    let m = Matrix44::transform(&v3(e.hpr), F::ONE, &v3(e.pos));
                    bodies.push(Body { actor: slot as i32, sprite: None, transform: m, centre: m, attack, projectile: [None; 2] });
                }
            }
        }
        // the player: its walker's body, and the attack types of what its
        // hands hold. The game dresses its own sprite off its channel as any
        // creature's (`hands`); go-eqoa-server leaves the channel's types 0,
        // so they come from what the dump says is worn, put in the hands as
        // `ApplyItems` puts them (the first: the primary or two-handed
        // weapon; the second: the secondary or the ranged one), an empty
        // hand 11, and both 11 while its sprite holds no items (a form)
        let mut attack = [11, 11];
        for it in &self.dump.inventory.items {
            match it.equip_slot {
                14 | 16 => attack[0] = it.item.attack_type as i32,
                15 | 18 => attack[1] = it.item.attack_type as i32,
                _ => {}
            }
        }
        if walker.sprite.as_ref().is_some_and(|cs| !cs.holds_items) {
            attack = [11, 11];
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
            // its projectiles as its own channel's items were attached (the
            // projectile is the item's model or a fixed one, which the dump
            // does not carry)
            let projectile = self.self_puppet.as_ref().map_or([None; 2], |p| p.projectile);
            bodies.push(Body { actor: SELF, sprite, transform: actor, centre, attack, projectile });
        }
        // a death ends the spells its actor was casting
        for a in std::mem::take(&mut self.dead_casters) {
            spells.stop_caster(a, &mut bodies, game_ms);
        }
        // NotifyActorWasDestroyed: a slot's entity gone or replaced
        for slot in 0..MAX_SLOTS {
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
        // `EQOA_RUMBLE_TEST`: a cast of the player's own, to test the motors
        if has_self && let Some(c) = me.and_then(crate::input::rumble_test) {
            self.casts.push(c);
        }
        let casts: Vec<_> = std::mem::take(&mut self.casts)
            .into_iter()
            .map(|c| {
                let (a, t) = (actor_of(c.caster), actor_of(c.target));
                let ts: Vec<i32> = c.targets.iter().map(|&id| actor_of(id)).filter(|&a| a != -1).take(8).collect();
                (c, a, t, ts)
            })
            .collect();
        spells.frame(&mut bodies, &casts, if has_self { SELF } else { -1 }, Some(&mut walker.cam.effect), game_ms, &walker.eye, lib, sound);
        self.rumbles.append(&mut spells.rumbles);
    }

    /// The projectiles in flight (`Spells::flying`), each drawn as
    /// `VIScene::RasterActor` draws an actor: within its sprite's off distance
    /// (a GroupSprite reads none: `VISprite`'s 500), at the actor's transform.
    /// One GPU model for each flying at once, kept for the next (as the
    /// boids', `fx.rs`); those not flying are not drawn.
    fn draw_projectiles(&mut self, walker: &Walker, lib: &Library, g: &mut Gpu) {
        let flying = self.spells.as_ref().map_or(&[][..], |s| &s.flying[..]);
        let eye = crate::lod::eye(&walker.cam, &walker.eye.position);
        let textures = render::library_textures(lib);
        let mut used: std::collections::BTreeMap<u32, usize> = Default::default();
        for (id, m) in flying {
            if !crate::lod::within(&m.m[3].xyz(), &eye, crate::lod::OFF_DIST) {
                continue;
            }
            let (model, pool) = self.projectiles.entry(*id).or_insert_with(|| (lib.lookup(*id as i32).and_then(|r| lib.build_model(r)), Vec::new()));
            let Some(model) = model.as_ref() else { continue };
            let n = used.entry(*id).or_default();
            if *n == pool.len() {
                pool.push(g.renderer.upload(g.device, g.queue, model, &[m.to_cols()], &textures));
            }
            let gpu = &mut pool[*n];
            g.renderer.set_instances(g.queue, gpu, &[m.to_cols()]);
            gpu.visible = true;
            *n += 1;
        }
        for (id, (_, pool)) in &mut self.projectiles {
            for gpu in pool.iter_mut().skip(used.get(id).copied().unwrap_or(0)) {
                gpu.visible = false;
            }
        }
    }

    /// `UpdateGame`'s `TrackTarget`: the selection rings round the target
    /// (`rings.rs`), drawn over the scene (`Renderer::set_overlay`). The
    /// target's actor is the player's own (its walker's position and slot 4)
    /// or the creature's in its slot, at slot 4 when its sprite is a
    /// character's; none (no target, or one with no actor) resets them, as
    /// the HUD turned off would (here it is always up). `step`: one of the
    /// game's frames, which the rings' clock and colour step on; between
    /// them the rings are placed again for the view as they are.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rings(&mut self, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, walker: &Walker, view_proj: glam::Mat4, lib: &Library, step: bool) {
        // no scene under the loading screen
        if self.loading.up() {
            r.set_overlay(device, queue, view_proj, &[], &render::library_textures(lib));
            return;
        }
        let me = self.self_id32();
        let target = (self.target != u32::MAX).then_some(self.target);
        let aim = target.and_then(|id| {
            if Some(id) == me {
                return self.self_at.map(|(pos, slot4)| crate::rings::Aim { id, pos, slot4 });
            }
            let k = self.entities.slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id))?;
            let e = self.entities.slots[k].as_ref().filter(|e| self.in_scene(e))?;
            Some(crate::rings::Aim { id, pos: e.actor_pos, slot4: self.slot4_of(k) })
        });
        // UpdateGame: the target info stops counting while the target's
        // +0x19 is 0
        let valid = self.target_info.is_some() && !self.entities.slots.iter().flatten().any(|e| Some(e.id) == target && e.p19 == 0);
        let tris = self.rings.frame(lib, walker, aim, me, valid, self.cam_inside, self.mode == 38, view_proj, step);
        // Hide UI (`hideui`): the rings go on easing, none drawn
        let tris = if crate::hideui::hidden() { Vec::new() } else { tris };
        r.set_overlay(device, queue, view_proj, &tris, &render::library_textures(lib));
    }

    /// `SetName`'s plates for every creature drawn: its name, in
    /// `NameColors[ApplyLook's pick]`, at its name node.
    pub fn plates(&self) -> Vec<Plate> {
        let mut out = Vec::new();
        if self.loading.up() {
            return out;
        }
        for (slot, e) in self.entities.slots.iter().enumerate() {
            let (Some(e), Some(Some(p))) = (e, self.puppets.get(slot)) else { continue };
            let (Some(c), Some(at)) = (&e.creature, p.name_at) else { continue };
            // a character not drawn (past its distance, `VIScene::
            // RasterActor`) rasters no name either
            if c.name.is_empty() || !p.shown {
                continue;
            }
            let color = if c.p256 < 2 { 0 } else if c.p299 & 1 != 0 { 2 } else { 1 };
            out.push(Plate { at, color: NAME_COLORS[color], name: c.name.clone() });
        }
        out
    }

    /// Mode 33 (R2 held): the chat box has the keys (`VIWndChat::OnKeyDown`
    /// and `OnKeyUp`), and what it passes up goes to `VIWndGame`: R2 let go
    /// ends the scroll, L3 toggles autorun (33 is a chat mode), and the
    /// attack and the target's details are let go as in play.
    fn chat_scroll_key(&mut self, key: i32, down: bool) {
        let took = self.wnd_chat.as_mut().is_some_and(|c| if down { c.key_down(key) } else { c.key_up(key) });
        if took {
            return;
        }
        if down {
            if key == 146 {
                self.toggle_autorun();
            }
            return;
        }
        match key {
            // R2, or the keyboard's C (`OnKeyUp` 67, 99, 143)
            143 | 67 | 99 => {
                if let Some(c) = self.wnd_chat.as_mut() {
                    c.set_scroll(0);
                }
                self.mode = 0;
            }
            134 => self.attack_release(),
            133 => self.info_up = false,
            _ => {}
        }
    }

    /// `CycleTarget(group)`. Not `group` (R1, Tab): the next creature in
    /// view, by `CompareY` over the game's score (mostly how far off the
    /// screen's centre line, plus distance; a corpse 2 further), from the
    /// camera at `eye` looking along `dir`, the group, the pet and the player
    /// left out. `group` (L1): the player, the group's other members, the
    /// pet, in turn. Changing between the two starts each from its first;
    /// then 0x0010 with the token and the choice.
    pub fn cycle_target(&mut self, group: bool, eye: glam::Vec3, dir: glam::Vec3, view_proj: glam::Mat4) {
        let me = self.self_id32();
        let members: Vec<u32> = self.group.members[..self.group.count()].iter().map(|m| m.id).collect();
        let mut cand: Vec<(f32, u32)> = Vec::new();
        if group {
            cand.extend(me.map(|id| (0.0, id)));
            cand.extend(members.iter().filter(|&&id| Some(id) != me).map(|&id| (0.0, id)));
            if self.pet_id != u32::MAX {
                cand.push((0.0, self.pet_id));
            }
        }
        for e in &self.entities.slots {
            let Some(e) = e else { continue };
            if group || !self.in_scene(e) || Some(e.id) == me || members.contains(&e.id) || e.id == self.pet_id {
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
            if e.p18 & 1 == 0 || d2 >= CYCLE_REACH2 {
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
        if !group {
            cand.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        let n = cand.len();
        // the one after k; k None is "not found": the first. Into the group's
        // cycle from the other, the one after the last (the player); out of
        // it, the first.
        let k = if self.last_group != group {
            if group { n.checked_sub(1) } else { None }
        } else {
            cand.iter().position(|c| c.1 == self.target)
        };
        self.auto_attack = false;
        self.target = match (n, k) {
            (0, _) => u32::MAX,
            (_, None) => cand[0].1,
            (_, Some(k)) => cand[(k + 1) % n].1,
        };
        self.target_info = None;
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(self.target);
        self.session.send(&w.data);
        self.last_group = group;
    }

    /// `VIWndGame::OnKeyDown`'s L1 and R1 in play (` and Tab on the
    /// keyboard): the cycle (`CycleTarget`), and the hold begun (`m_p03C`,
    /// `m_p040`): still held 300 ms on, the manual camera (`manual_frame`).
    pub fn target_key(&mut self, group: bool, eye: glam::Vec3, dir: glam::Vec3, view_proj: glam::Mat4) {
        self.cycle_target(group, eye, dir, view_proj);
        self.aim_hold = true;
        self.aim_at = self.session.now();
    }

    /// `VIWndGame::OnKeyUp`'s L1 and R1: the manual camera ended
    /// (`ResetView`), or in play the hold given up.
    pub fn target_key_up(&mut self, walker: &mut Walker) {
        self.target_key_up_on(&mut walker.cam, &mut walker.scene);
    }

    fn target_key_up_on<S: vi::scene::Scene<Ee>>(&mut self, cam: &mut WalkCamera<Ee>, scene: &mut S) {
        if self.mode == 38 {
            self.reset_view_on(cam, scene);
        } else if self.mode == 0 {
            self.aim_hold = false;
        }
    }

    /// The manual camera is up (mode 38, `IsModal`).
    pub fn aiming(&self) -> bool {
        self.mode == 38
    }

    /// A window is up: neither play nor the manual camera, which keeps play's
    /// HUD (and the touch screen's whole pad).
    pub fn window_up(&self) -> bool {
        self.mode != 0 && self.mode != 38
    }

    /// What brings back what Hide UI hid (`hideui::frame`): a window up, the
    /// map, a server's player search, the voice window, the target's details
    /// (Q held), a line being typed, or the player dead.
    pub fn wants_ui(&mut self) -> bool {
        self.window_up() || self.map.open || self.search.open || self.voice_wnd.open || self.info_up || self.info_window.open > 0.0 || self.keyboard_typing() || self.is_dead()
    }

    /// The loading screen is up (drawn on the 2D layer, never hidden).
    pub fn loading_up(&self) -> bool {
        self.loading.up()
    }

    /// The Controls page's "Target: the target's target" (F, the client's
    /// own key) and a click on the Target of Target bar: the game's Assist
    /// Target (`DoAction(0x0B0E0006)`, 0x00CB), the server answering whom to
    /// target (`Msg007B`). The server's answer, not the target's channel:
    /// an object id there is the target's own session's on a server that
    /// counts them per session (go-eqoa-server), and the server knows a
    /// creature's enemy, which the channel does not carry.
    pub fn assist_target(&mut self) {
        self.do_action(0x0B0E_0006, 0);
    }

    /// Target of Target (the Extended page's, off by default): the name of
    /// whom the target's channel says it targets (`Creature::target`, its
    /// state's bytes 58-61, which CLIENT reads into `m_p74` and never looks
    /// at), when that is an entity in view other than the target. Sandstorm
    /// fills it for players (their own target; the player's own channel too)
    /// and leaves every NPC's 0 (its captures of 2026-02: no creature's moved
    /// in a fight), so a creature shows none; go-eqoa-server counts object
    /// ids per session, and there a player's is its own session's id.
    pub fn target_of_target(&self) -> Option<String> {
        let t = self.target_of_target_id()?;
        self.entities.slots.iter().flatten().find(|e| e.id == t)?.creature.as_ref().map(|c| c.name.clone()).filter(|n| !n.trim().is_empty())
    }

    /// Whom the target targets, as its channel says (Target of Target's
    /// entity): none with the option off, without a target, or naming none
    /// or itself.
    fn target_of_target_id(&self) -> Option<u32> {
        if !crate::settings::client().target_of_target || self.target == u32::MAX {
            return None;
        }
        let t = self.entities.slots.iter().flatten().find(|e| e.id == self.target)?.creature.as_ref()?.target;
        (t != 0 && t != u32::MAX && t != self.target).then_some(t)
    }

    /// Mana Bars (the Extended page's, the client's own): entity `id`'s power
    /// of 255 for a bar under its health, the player's own off the stat
    /// channel and anyone else's as the server last sent it
    /// (`net::session::OP_POWER_BARS`: none from a server that sends none,
    /// Sandstorm); none with the option off.
    fn bar_power(&self, id: u32) -> Option<u8> {
        if !crate::settings::client().mana_bars {
            return None;
        }
        if Some(id) == self.self_id32() {
            let (p, top) = (self.stats.stat[9] as i64, self.stats.stat[10] as i64);
            return (top > 0).then(|| (p.clamp(0, top) * 255 / top) as u8);
        }
        self.session.power_of(id)
    }

    /// `FindEntity(m_selfId)`: the player's own entity is in.
    fn self_entity(&self) -> bool {
        let me = self.self_id32();
        me.is_some() && self.entities.slots.iter().flatten().any(|e| Some(e.id) == me)
    }

    /// The player's eye as the manual camera holds it: the actor's place
    /// (`GetActorPos`: where the walker drew it), the race camera's `m_p34`
    /// above it.
    fn manual_eye(&self) -> Option<Vect3<Ee>> {
        let (at, _) = self.self_at.filter(|_| self.self_entity())?;
        let mut pos = v3(at);
        pos.y += F::new(self.race_camera().map_or(1.68, |r| r.p34));
        Some(pos)
    }

    /// `VIClient::BeginManualCamera` (L1 or R1 held): the eye held at the
    /// player's own (`manual_eye`), looking the walk's way in first person
    /// and the fixed follow, the view's (`dir`, the camera's) in the orbit
    /// camera, read back as euler angles off its basis under world up; the
    /// pitch short of straight up or down; blended in over 0.2 s, at once
    /// from first person; nothing in the free orbit (mode 1). Then
    /// `ManualCameraBegun`: mode 38, and `UpdateMode` (the attack as the Auto
    /// Attack option keeps it, movement and the camera locked).
    fn begin_manual_camera(&mut self, cam: &mut WalkCamera<Ee>, dir: Vect3<Ee>) {
        let Some(pos) = self.manual_eye() else { return };
        let mut hpr = match cam.camera_mode {
            0 | 2 => cam.walk.committed.angles,
            3 => {
                // `m_camera.m_dir`: unit, as VICamera::CalcAxes keeps it
                let mut dir = dir;
                dir.normalize();
                let mut up = Vect3::of(0.0, 1.0, 0.0);
                let mut right = Vect3::cross(&up, &dir);
                up = Vect3::cross(&dir, &right);
                right.normalize();
                up.normalize();
                let mut m = Matrix44::<Ee>::identity();
                (m.m[0].x, m.m[0].y, m.m[0].z) = (right.x, right.y, right.z);
                (m.m[1].x, m.m[1].y, m.m[1].z) = (up.x, up.y, up.z);
                (m.m[2].x, m.m[2].y, m.m[2].z) = (dir.x, dir.y, dir.z);
                m.euler_zxy()
            }
            _ => return,
        };
        hpr.y = vi::fpu::clamp(hpr.y, F::new(-1.5607964), F::new(1.5607964));
        let time = if cam.camera_mode != 0 { F::new(0.2) } else { F::ZERO };
        cam.effect.begin_manual(pos, hpr, time);
        eprintln!("play: the manual camera (L1 or R1 held)");
        self.manual_on = true;
        self.mode = 38;
        self.attack_release();
        self.locks = (false, false);
    }

    /// `VIClient::ResetView` (L1 or R1 let go; the chat line opened over the
    /// manual camera; a quest stage's dialog): out of the manual camera. Out
    /// of the orbit camera the walk is set where it stands facing the look's
    /// heading (the player turned to where it looked), and the follow
    /// camera put behind it; the orbit camera is put behind the look. Blended
    /// back out over 0.2 s, at once in first person; then `VIWndGame::Refresh`:
    /// play again (`UpdateMode` 0: movement and the camera free).
    pub fn reset_view(&mut self, walker: &mut Walker) {
        self.reset_view_on(&mut walker.cam, &mut walker.scene);
    }

    fn reset_view_on<S: vi::scene::Scene<Ee>>(&mut self, cam: &mut WalkCamera<Ee>, scene: &mut S) {
        if !self.self_entity() {
            return;
        }
        eprintln!("play: the manual camera ended");
        self.manual_on = false;
        let pi = F::<Ee>::new(std::f32::consts::PI);
        if cam.camera_mode != 3 {
            let mut p = cam.walk.committed;
            p.angles.x = cam.effect.manual_hpr.x;
            // (and the entity's SetHpr: the walker is the player's entity)
            cam.walk.set_location(p.position, p.angles, scene);
            if cam.camera_mode != 0 {
                let a = cam.walk.committed.angles.x + pi;
                cam.choose_orbit_angle(a, scene);
                cam.effect.end_manual(F::new(0.2));
            } else {
                cam.effect.end_manual(F::ZERO);
            }
        } else {
            let a = cam.effect.manual_hpr.x + pi;
            cam.choose_orbit_angle(a, scene);
            cam.effect.end_manual(F::new(0.2));
        }
        self.mode = 0;
        self.attack_release();
        self.locks = (true, true);
    }

    /// `UpdateGame`'s manual camera, a frame drawn: the eye kept at the
    /// player's own as the body moves (`SetManualXyz`); and a frame of the
    /// game's, `VIWndGame::OnDrawSelf`'s hold: L1 or R1 held 300 ms on in play
    /// begins it, and any window up (the map and the search too) gives the
    /// hold up. The client's own: a manual camera that a window's `SetMode`
    /// took mode 38 from (a teleport, a respawn) ends at once, where the PS2
    /// holds the eye where it was until the next look; and one whose walker
    /// was built again under it (a world change) leaves mode 38.
    fn manual_frame(&mut self, walker: &mut Walker, game_frame: bool) {
        let dir = walker.eye.direction;
        self.manual_frame_on(&mut walker.cam, dir, game_frame);
    }

    fn manual_frame_on(&mut self, cam: &mut WalkCamera<Ee>, dir: Vect3<Ee>, game_frame: bool) {
        if self.mode == 38 {
            if let Some(pos) = self.manual_eye() {
                cam.effect.manual_xyz = pos;
            }
            if cam.effect.manual_terp == F::ZERO && cam.effect.manual_terp_rate == F::ZERO {
                self.manual_on = false;
                self.mode = 0;
                self.locks = (true, true);
            }
        } else if self.manual_on {
            cam.effect.end_manual(F::ZERO);
            self.manual_on = false;
        }
        if game_frame {
            let now = self.session.now();
            let busy = self.mode != 0 || self.map.open || self.search.open || self.voice_wnd.open || self.typing.is_some();
            if self.aim_hold && now.wrapping_sub(self.aim_at) as i32 > 300 && !busy {
                self.begin_manual_camera(cam, dir);
                self.aim_hold = false;
            }
            if self.mode != 0 || busy {
                self.aim_at = now;
                self.aim_hold = false;
            }
        }
    }

    /// `VIClient::TargetCentre` (the manual camera all the way in, a frame of
    /// the game's), measured through the game's camera (`walker.view(4/3)`:
    /// the frame's 10/7 across a 4:3 picture, as the console measures) and in
    /// view as drawn (`view_proj`: a wider window's sides count). True when
    /// the target changed (`DrawReticle(1)`).
    fn target_centre(&mut self, walker: &Walker, view_proj: glam::Mat4) -> bool {
        let (proj, view) = walker.view(4.0 / 3.0);
        let eye = walker.eye_position();
        let seen: Vec<Seen> = self
            .entities
            .slots
            .iter()
            .enumerate()
            .filter_map(|(k, e)| {
                let e = e.as_ref().filter(|e| self.in_scene(e))?;
                let at = self.slot4_of(k).unwrap_or(e.actor_pos);
                let d2 = (glam::Vec3::from(e.actor_pos) - eye).length_squared();
                Some(Seen { id: e.id, at, selectable: e.p18 & 1 != 0, side: e.creature.as_ref().is_some_and(|c| c.p256 == 3), d2 })
            })
            .collect();
        let choice = centre_choice(&seen, self.self_id32(), self.pet_id, proj * view, view_proj);
        self.centre_target(choice)
    }

    /// `TargetCentre`'s choice made the target: a change stops the attack,
    /// drops the target's info and asks the server (0x0010, the token 0),
    /// even for none. True when it changed.
    fn centre_target(&mut self, choice: Option<u32>) -> bool {
        let old = self.target;
        self.target = choice.unwrap_or(u32::MAX);
        if old == self.target {
            return false;
        }
        self.auto_attack = false;
        self.target_info = None;
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(self.target);
        self.session.send(&w.data);
        true
    }

    /// A world change for the viewer to carry out (`EnterWorld` -> `LoadWorld`).
    pub fn take_world_change(&mut self) -> Option<(String, Spawn)> {
        self.world_change.take()
    }

    /// The viewer has carried the world change out (or given it up): the
    /// screen black since the teleport shows again.
    /// The new world is in, its walker built again: the screen back, the
    /// walker dressed again on the next frame (the server's model on a new
    /// walker) and its camera as the change left it (`cam_carry`), behind
    /// the player.
    pub fn world_entered(&mut self, walker: &mut Walker) {
        self.fade.set(0.0, 0.0);
        self.loading.entered();
        self.self_model = None;
        if let Some((mode, preset)) = self.cam_carry.take() {
            let rc = self.race_camera();
            crate::camstyle::set_style(&mut walker.cam, mode, rc, &mut walker.scene);
            if let Some(p) = preset {
                crate::camstyle::apply(&mut walker.cam, &p);
            }
            walker.cam.set_orbit_angle_behind();
            walker.eye = walker.cam.process(&mut walker.scene);
        }
    }

    /// `Msg00B2`: a move of the server's (a port, a coach, Return Home):
    /// `Respawn` there, then `SendOpSigned(73, token)`. The PS2 does both in
    /// one go, its frame stopped in `Respawn`'s `EnterWorld` until the world
    /// and the zones round the arrival are in, so the answer and its first
    /// 0x40 from the arrival leave together (Sandstorm's captures,
    /// `claude-notes/world-change-state-resync.md`); here the move is done
    /// when the loading screen comes down (`loading_step`), and the token
    /// waits for it (`arrival`). With no self (`m_selfId` -1) nothing
    /// moves, and a move into a world the table lacks cannot be made: both
    /// are answered at once, as the PS2 answers whatever happens.
    fn msg_00b2(&mut self, walker: &mut Walker, world: u8, pos: [f32; 3], hpr: [f32; 3], token: u32, now: i64) {
        if self.session.self_id.is_some() && self.teleport(walker, world, pos, hpr, now) {
            self.arrival.push(token);
        } else {
            self.session.answer_teleport(token);
        }
    }

    /// `Respawn` done (the loading screen down): each move's token answered,
    /// oldest first, then the 0x40 from where the player now stands, in the
    /// same packet as on a PS2.
    fn arrived(&mut self, walker: &Walker) {
        if self.arrival.is_empty() {
            return;
        }
        for token in std::mem::take(&mut self.arrival) {
            self.session.answer_teleport(token);
        }
        let now = self.session.now();
        self.send_position(&walker.cam.walk.committed, now);
    }

    /// `SendPosition`, at the update rate (`m_p07578`, 100 ms), and none
    /// while a move of the server's is not done (`arrival`): the PS2's
    /// frame stops in `Respawn` until it stands at the arrival, where this
    /// client went on walking the world it was leaving, and sent the new
    /// world's number with the old world's place (Sandstorm answered that
    /// with 0x07D2, "This character has been modified by a customer service
    /// representative").
    fn position_due(&mut self, walk: &vi::walk::WalkPhysics<Ee>, now: u32) {
        if self.session.self_id.is_some() && self.arrival.is_empty() && now.wrapping_sub(self.last_send) >= UPDATE_RATE {
            self.send_position(walk, now);
        }
    }

    /// The frame's end: what it sent leaves now, as `VIClient::Process` ends
    /// its frame with `drdp_flush` after `SendPosition` and the windows.
    /// The session's own flush comes after what has arrived (`pump`), so a
    /// 0x40 queued after it waited for the next frame's arrivals and left
    /// with them: one made before a port rode in the datagram that
    /// acknowledged the port, the old place given as the move's answer.
    pub fn flush(&mut self) {
        self.session.flush();
    }

    /// `SendPosition`'s 0x40: the committed walk state, the attack held and
    /// the target.
    fn send_position(&mut self, walk: &vi::walk::WalkPhysics<Ee>, now: u32) {
        self.last_send = now;
        if watching(&self.player.name) {
            eprintln!("[sent] {now} pos {:.2?} vel {:.2?} heading {:.3} loco {} flags {:#x} mode {}", walk.position.to_array(), walk.velocity.to_array(), walk.angles.x.0, walk.loco_state, walk.flags, self.mode);
        }
        let msg = vi::selfstate::encode(self.player.world as u8, walk, self.auto_attack as u8, self.target);
        self.session.send_state(0x40, &msg);
    }

    /// Msg00B2's `Respawn`: back to play, then Respawn at `pos` in `world`. A
    /// dead player stays dead (EnterWorld leaves m_pC1B0): only the dump
    /// (Msg000D) ends a death. Respawn ends autorun and the follow. Its floor
    /// is cast where the server put it (`Walker::relocate`, a new world's
    /// walk's `Placing::Respawn`), and again once the zones there are in
    /// (`loading_step`), as `EnterWorld` streams them before the cast.
    /// Whether the move is under way (the loading screen up until it is
    /// done): not into a world the table lacks.
    pub fn teleport(&mut self, walker: &mut Walker, world: u8, pos: [f32; 3], hpr: [f32; 3], now: i64) -> bool {
        self.mode = 0;
        self.autorun = false;
        self.following = false;
        let moving = if world as i64 != self.player.world {
            // the world table: the disc's six, or the server's patch's
            // (`s2`: Sandstorm's eight)
            match crate::s2::get().world_file(world as i64) {
                Some(f) => {
                    eprintln!("play: teleport to world {world} ({f}) at {pos:?}");
                    // Respawn's casts at the arrival
                    self.world_change = Some((f.to_string(), Spawn { pos, heading: hpr[0], placing: Placing::Respawn }));
                    self.loading.begin(pos, hpr[0], true, true, now);
                    self.loading.hold_far();
                    // black until the new world is entered (`world_entered`),
                    // the old one not shown while it loads
                    self.fade.set(1.0, 0.0);
                    self.player.world = world as i64;
                    self.player.pos = pos;
                    // the walker is built again there (`world_entered`)
                    self.cam_carry = Some((walker.cam.camera_mode, Some(crate::camstyle::save(&walker.cam))));
                    true
                }
                None => {
                    eprintln!("play: teleport to world {world}, which the disc's world table does not have");
                    false
                }
            }
        } else {
            eprintln!("play: teleport to {pos:?}");
            let far = crate::loading::far(walker_at(walker), pos);
            walker.relocate(pos, hpr[0]);
            self.player.pos = pos;
            // EnterWorld: StreamFillCache there under the loading screen
            self.loading.begin(pos, hpr[0], false, true, now);
            if far {
                self.loading.hold_far();
            }
            true
        };
        // Respawn ends with SetSky(m_camera, 1): the sky where it stands, at
        // once (another world's once it is entered: `world_pending`)
        self.weather.push(Atmos::SkyNow);
        self.npcw_respawn();
        self.bank_reset();
        moving
    }

    /// Whether another world is still to be entered (its file read): the
    /// atmosphere's orders from its move on wait for it (`Weather::take`).
    pub fn world_pending(&self) -> bool {
        self.world_change.is_some() || self.loading.wait.is_some_and(|w| w.entering)
    }

    /// The loading screen while it is up: its view and the gears, which the
    /// frame draws in place of the scene (`loading.rs`).
    pub fn loading_scene(&self, size: (u32, u32)) -> Option<(render::View, Vec<&GpuModel>)> {
        self.loading.up().then(|| (crate::loading::view(size), self.loading.models().collect()))
    }

    pub fn loading(&self) -> bool {
        self.loading.up()
    }

    /// Where the loading screen waits for the zones: none while it is down
    /// or another world's file is still being read.
    pub fn loading_at(&self) -> Option<glam::Vec3> {
        self.loading.waits_at().map(glam::Vec3::from)
    }

    /// A frame under the loading screen: once the zones round the arrival
    /// are in and drawn (`settled`), or at the cap, the screen down; until
    /// then the walk held where the move put it (the game's frame stops in
    /// `StreamFillCache`), to go on from there on the ground that came. A
    /// 0x00B2 move's floor is cast again as it comes down (`Respawn` casts
    /// once `EnterWorld` has the zones in; a far move's first cast here met
    /// nothing and kept the server's height, which a little under a floor fell
    /// through it).
    pub fn loading_step(&mut self, walker: &mut Walker, settled: bool) {
        let now = self.session.now() as i64;
        if let Some(w) = self.loading.end(settled, now) {
            crate::say(&format!("loading screen down: the zones round {:?} in after {} ms{}", w.at.map(|v| v.round()), now - w.since, if settled { "" } else { " (the cap)" }));
            if w.respawn {
                walker.relocate(w.at, w.heading);
            }
            // a move of the server's done: its answer, and the 0x40 from here
            self.arrived(walker);
            return;
        }
        let p = walker.cam.walk.phys.position;
        if let Some((at, heading)) = self.loading.hold_at([p.x.0, p.y.0, p.z.0], walker.cam.walk.phys.angles.x.0) {
            walker.hold(at, heading);
        }
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
        self.target_id(id);
        true
    }

    /// `Msg007B`, an assist's answer: -1 is word 838 ("There is no target
    /// for assisting."), else `id` targeted and asked about (0x0010 with the
    /// token), the attack and the target's details left as they are.
    fn msg_007b(&mut self, id: u32) {
        if id == u32::MAX {
            self.show_word(838);
            return;
        }
        self.target = id;
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(id);
        self.session.send(&w.data);
    }

    /// `id` targeted, as `CycleTarget` leaves a choice: the attack off, the
    /// target's details asked again, 0x0010 with the token.
    pub fn target_id(&mut self, id: u32) {
        self.target = id;
        self.auto_attack = false;
        self.target_info = None;
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(id);
        self.session.send(&w.data);
    }

    /// The creature drawn under window point `at` of a `size` window seen
    /// through `view_proj` (the player left out): of those whose upright,
    /// from the feet to the name plate's anchor, passes within a quarter of
    /// its height on screen (12 pixels at least), the nearest the camera.
    pub fn creature_at(&self, at: (f64, f64), size: (u32, u32), view_proj: glam::Mat4) -> Option<u32> {
        let (w, h) = (size.0 as f32, size.1 as f32);
        let px = |p: glam::Vec3| {
            let c = view_proj * p.extend(1.0);
            (c.w > 0.0).then(|| (glam::Vec2::new((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h), c.w))
        };
        let cursor = glam::Vec2::new(at.0 as f32, at.1 as f32);
        let me = self.self_id32();
        let mut best: Option<(u32, f32)> = None;
        for (slot, e) in self.entities.slots.iter().enumerate() {
            // any with an actor, a body or none (its upright two high)
            let Some(e) = e.as_ref().filter(|e| self.in_scene(e)) else { continue };
            if Some(e.id) == me {
                continue;
            }
            let foot = glam::Vec3::from(e.actor_pos);
            let name_at = self.puppets.get(slot).and_then(|p| p.as_ref()).and_then(|p| p.name_at);
            let head = name_at.map_or(foot + glam::Vec3::Y * 2.0, glam::Vec3::from);
            let (Some((a, depth)), Some((b, _))) = (px(foot), px(head)) else { continue };
            let reach = ((b - a).length() * 0.25).max(12.0);
            if crate::mouse::to_segment(cursor, a, b) <= reach && best.is_none_or(|(_, d)| depth < d) {
                best = Some((e.id, depth));
            }
        }
        best.map(|b| b.0)
    }

    /// A click or a tap `c` on a `size` window, on the game's windows
    /// (`pointer`; `finger` for a touch screen's, which may miss a
    /// little): in play the HUD's quick menus take it (a row used,
    /// the title bar the next menu); with a window up a right click is △, a
    /// button glyph drawn there that button, else what the window has there
    /// (its rows, tabs and fields) as the keys it takes. True when a window
    /// took it (one up takes every one), so it targets nothing.
    pub fn point(&mut self, c: crate::mouse::Click, size: (u32, u32), finger: bool) -> bool {
        use crate::mouse::Click;
        // a mouse means a keyboard beside it (the edit boxes' compact face,
        // with the options' Soft Keyboard on); a finger, none
        self.keyboard = !finger;
        let (Click::Target(x, y) | Click::Interact(x, y) | Click::Right(x, y)) = c;
        let f = render::ui::frame_from_window(size, [x as f32, y as f32]);
        // the map, while it is up: its buttons and glyphs; a right click is △;
        // a click on the map itself puts the waypoint there or takes it away
        if self.map.open {
            let keys = if matches!(c, Click::Right(..)) { vec![crate::pointer::key::TRIANGLE] } else { self.map.point(f, finger) };
            if keys.is_empty() && matches!(c, Click::Target(..)) {
                self.map.click([x as f32, y as f32], finger);
            }
            self.press(&keys);
            return true;
        }
        // in play; a server's player search or the voice window over it is
        // a window
        if self.mode == 0 && !self.search.open && !self.voice_wnd.open {
            // Hide UI (`hideui`): none of the HUD's buttons is there to press
            if matches!(c, Click::Right(..)) || crate::hideui::hidden() {
                return false;
            }
            // the quick menus and the chat box are drawn out at a wide
            // window's right, the compass at its left; in a tall window the
            // compass and the quick menus up at its top, the chat box down at
            // its bottom (each answers in its own place)
            let (side, up) = (render::ui::side(size), render::ui::top(size));
            // the compass opens the map (the touch screen's way to it)
            if crate::map::on_compass([f[0] + side, f[1] + up]) && self.hud.is_some() {
                self.map.show();
                return true;
            }
            if let Some(k) = self.hud_menus.point([f[0] - side, f[1] + up]) {
                // the pad key it stands for in S2's record, let go at once
                self.note_hud_key(k, true);
                self.hud_key(k);
                self.note_hud_key(k, false);
                return true;
            }
            // the chat box: the chat line, as R3 opens it
            if self.wnd_chat.as_ref().is_some_and(|w| w.contains([f[0] - side, f[1] - up])) {
                self.open_edit(0xB0F0000, 128, -1, false);
                self.edit.title = self.chat_title();
                return true;
            }
            // the player's frame, a member's bar, the pet's: targeted, as
            // L1's cycle targets each (noted where they were placed)
            let pick = self.hud.as_ref().and_then(|h| h.2.pick(f));
            let id = match pick {
                // the bug report (`report`)
                Some(crate::hud::Pick::Report) => {
                    crate::report::open();
                    return true;
                }
                // the Target of Target bar: the game's Assist Target
                Some(crate::hud::Pick::TargetsTarget) => {
                    self.assist_target();
                    return true;
                }
                // the minimap: the map opened, as the compass opens it
                Some(crate::hud::Pick::Minimap) => {
                    self.map.show();
                    return true;
                }
                // the quest tracker: the quest journal (OpenQuestLog's mode
                // 22), closed back to the world
                Some(crate::hud::Pick::Quests) => {
                    self.quest_open = true;
                    self.quest_from_hud = true;
                    self.mode = 22;
                    return true;
                }
                // the mic badge: the voice window
                Some(crate::hud::Pick::Voice) => {
                    self.open_voice_window();
                    return true;
                }
                // a voice mark: that one muted, or heard again (`voiceplay`)
                Some(crate::hud::Pick::VoiceMark(peer)) => {
                    self.voice_toggle_mute(peer);
                    return true;
                }
                Some(crate::hud::Pick::Me) => self.self_id32(),
                Some(crate::hud::Pick::Member(id)) => Some(id),
                Some(crate::hud::Pick::Pet) => (self.pet_id != u32::MAX).then_some(self.pet_id),
                None => return false,
            };
            if let Some(id) = id.filter(|&id| id != self.target) {
                self.target_id(id);
            }
            return true;
        }
        let keys = if matches!(c, Click::Right(..)) {
            vec![crate::pointer::key::TRIANGLE]
        } else {
            match self.glyphs.at(f, if finger { crate::pointer::FINGER } else { 0.0 }) {
                Some(k) => vec![k],
                None => self.window_point(f),
            }
        };
        self.press(&keys);
        true
    }

    /// The wheel's `lines` (up positive; a finger's drag down) on the window
    /// up: the d-pad's up or down for each whole line (a few at most
    /// waiting). False in play.
    pub fn window_wheel(&mut self, lines: f32) -> bool {
        use crate::pointer::key;
        if ((self.mode == 0 || self.mode == 38) && !self.search.open && !self.voice_wnd.open) || self.quick_dpad() {
            self.wheel = 0.0;
            return self.mode != 0 && self.mode != 38;
        }
        self.wheel += lines;
        while self.wheel.abs() >= 1.0 {
            let k = if self.wheel > 0.0 { key::UP } else { key::DOWN };
            self.wheel -= self.wheel.signum();
            if self.key_queue.len() < 3 {
                self.press(&[k]);
            }
        }
        true
    }

    /// What the window up has at frame point `f`, found as `window_key`
    /// finds the window: the window's selection moved there, and the keys
    /// to press (`pointer`).
    fn window_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        use crate::pointer::key;
        let one = |k: Option<i32>| k.into_iter().collect::<Vec<i32>>();
        if self.search.open {
            return self.search.point(f);
        }
        if self.mode == 30 {
            return self.quick.point(f);
        }
        if self.mode == 2 {
            return one(self.edit.point(f).flatten());
        }
        if self.voice_window_owns() {
            return self.voice_wnd.point(f);
        }
        if self.bank_owns() {
            return self.bank_point(f);
        }
        if self.npcw_owns() {
            return self.npcw_point(f);
        }
        if self.support_owns() {
            return self.support_point(f);
        }
        if self.mail_owns() {
            return self.mail_point(f);
        }
        if let Some(s) = self.start.as_mut() {
            match self.mode {
                13 if self.items.visible => return self.items.point(f),
                13 => return one(s.menu.point(f, &s.menus).then_some(key::CROSS)),
                5 => return self.book.point(f),
                35 => return self.who_wnd.point(f),
                _ if self.char_window() => return self.char_point(f),
                _ if self.panel_up() => return self.panel_point(f),
                _ => {}
            }
        }
        one(match self.mode {
            7 => self.response.point(f).then_some(key::CROSS),
            // the NPC's words: a click goes on, as Enter does
            9 => Some(key::CROSS),
            14 => self.mbox.point(f).then_some(key::CROSS),
            _ => None,
        })
    }

    /// `window_point` for the character windows.
    fn char_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        match self.mode {
            12 => self.trainer.point(f),
            40 => self.cm_wnd.point(f),
            41 => self.faction_wnd.point(f),
            22 => self.quest_log.point(f),
            6 => self.recipe_wnd.point(f),
            37 => self.exp_wnd.point(f),
            _ => Vec::new(),
        }
    }

    /// `window_point` for the panels.
    fn panel_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        match self.mode {
            17 | 18 => self.buddy_wnd.point(f),
            26 => self.ignore_wnd.point(f),
            34 => self.options_wnd.point(f),
            20 | 21 => self.guild_wnd.point(f),
            _ => Vec::new(),
        }
    }

    /// Where creature `name` is drawn in a `size` window seen through
    /// `view_proj`, the middle of its upright, when it is on screen (for
    /// scripted clicks); `*` is the first creature that is (not the player).
    pub fn screen_of(&self, name: &str, size: (u32, u32), view_proj: glam::Mat4) -> Option<(f64, f64)> {
        let me = self.self_id32();
        let (w, h) = (size.0 as f32, size.1 as f32);
        self.entities.slots.iter().enumerate().find_map(|(slot, e)| {
            let e = e.as_ref()?;
            let c = e.creature.as_ref()?;
            if (name.trim() != "*" && c.name.trim() != name.trim()) || Some(e.id) == me {
                return None;
            }
            if !self.in_scene(e) {
                return None;
            }
            let name_at = self.puppets.get(slot).and_then(|p| p.as_ref()).and_then(|p| p.name_at);
            let foot = glam::Vec3::from(e.actor_pos);
            let head = name_at.map_or(foot + glam::Vec3::Y * 2.0, glam::Vec3::from);
            let c = view_proj * ((foot + head) * 0.5).extend(1.0);
            let (x, y) = ((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h);
            (c.w > 0.0 && (0.0..w).contains(&x) && (0.0..h).contains(&y)).then_some((x as f64, y as f64))
        })
    }

    /// A click on a creature (or on nothing, which keeps the target): it is
    /// targeted, and with `interact` talked to, or its corpse opened (the
    /// attack button's loot request on a corpse).
    pub fn click(&mut self, id: Option<u32>, interact: bool) {
        let Some(id) = id else { return };
        if self.mode != 0 {
            return;
        }
        if id != self.target {
            self.target_id(id);
        }
        if interact {
            let corpse = self.entities.slots.iter().flatten().find(|e| e.id == id).is_some_and(|e| e.p18 & 2 != 0);
            if corpse {
                self.attack_press();
                self.attack_release();
            } else {
                self.interact_with_target();
            }
        }
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
        if (!toggle || !self.auto_attack) && self.attack.may_press(now) {
            self.attack.press(now);
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
    /// tell. A command it does not know goes to the server as typed, where
    /// the game answers it with word 826 itself: the client's own, as
    /// Sandstorm's S2 forwards them, so the server's commands (a GM's
    /// `/goto`, go-eqoa-server's dev console) can be typed with a slash.
    /// Plain text goes to the current channel. Not here: the crafting
    /// commands.
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
            "/alarm" => self.alarm_command(&rest),
            // the client's own, never sent: the voice lobby (`voiceplay`)
            // alone, the voice window (`voicewndplay`)
            "/voice" if rest.trim().is_empty() => self.open_voice_window(),
            "/voice" => self.voice_command(&rest),
            "/time" => self.do_action(0x0B0E_0008, 0),
            "/follow" => self.do_action(0x0B0E_0007, 0),
            "/r" | "/reply" => {
                if !self.reply_name.is_empty() {
                    let line = format!("/t {} {}", self.reply_name, rest);
                    self.session.say(&line, target);
                }
            }
            _ => self.session.say(text, target),
        }
    }

    /// The art swapped (F9, `era`): each creature is made anew in it as it is
    /// next applied (`Puppet::era`), the player's own body with them (its
    /// model told the window again), the hands' projectiles, the spell
    /// effects, the rings, the gears and the HUD's art are made again, and
    /// the chat says which art is drawn.
    pub fn art_changed(&mut self, era: esf::Era) {
        self.self_model = None;
        self.projectiles.clear();
        // the spell effects, the target's rings, the loading screen's gears
        // and the HUD's art (`attach_ui`) made again in the art now
        self.spells = None;
        self.rings.art_changed();
        self.loading.art_changed();
        self.add_chat(crate::era::line(era).to_string(), [1.0; 4]);
    }

    /// F9 or the Extended page's Art with no 2002 disc image at hand, in the
    /// world: where to choose it (natively a file dialog is not opened in the
    /// world, `viewer`'s `ask_disc`).
    pub fn no_2002_disc(&mut self) {
        self.add_chat("The 2002 art needs your EverQuest Online Adventures (USA) disc image: choose it with F9 at the title or at character select.".to_string(), [1.0; 4]);
    }

    /// A line the server sent (0x0A7A, 0x0A7B, 0x069E, 0x0758, 0x00CE) for
    /// the chat box, logged; but someone else's shout while Ignore Shouts is
    /// kept is dropped (a server that does not listen to 0x0C01's flag still
    /// sends them), the first said in the log.
    fn server_line(&mut self, text: String, color: [f32; 4]) {
        if !crate::settings::shouts() && crate::chat::is_shout(&text, color, &self.player.name) {
            if !self.shouts_dropped {
                eprintln!("chat: shouts ignored (Ignore Shouts, kept): the server's shouts are not shown");
                self.shouts_dropped = true;
            }
            return;
        }
        eprintln!("chat: {text}");
        self.add_chat(text, color);
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
    /// cue, back to play with autorun, the follow and auto-attack off, every
    /// invitation refused (3), and `ResetCamera`: the death camera from where
    /// the camera is.
    fn msg_005e(&mut self, walker: &mut Walker) {
        if let Some(cs) = walker.sprite.as_mut() {
            cs.set_death(true, false, &walker.anims, &mut self.spell_rand, None);
            self.dead_casters.push(crate::spells::SELF);
        }
        self.mode = 0;
        self.cues.push(crate::sound::Cue::Named("death", 0.4));
        self.autorun = false;
        self.following = false;
        self.auto_attack = false;
        for i in std::mem::take(&mut self.group.invites) {
            self.session.send_msg(0x0624, i.id, 3);
        }
        // ResetCamera: first person becomes the follow camera (style 2),
        // processed once, so the death camera starts behind the body
        if walker.cam.camera_mode == 0 {
            let rc = self.race_camera();
            crate::camstyle::set_style(&mut walker.cam, 2, rc, &mut walker.scene);
            walker.eye = walker.cam.process(&mut walker.scene);
        }
        let body = walker.cam.walk.phys.smooth_location().to_array();
        self.death = Some(crate::death::DeathCam::new(walker.eye.position.to_array(), walker.eye.direction.to_array(), body));
        self.death_eye = None;
    }

    /// The dump again, Msg000D's end: `ClearPC1B0` (the death camera off,
    /// the body up), then `SetFade(1, 0)` and `SetFade(0, 1)`, the screen
    /// black and back over a second. A warp (0x00B2) does not end a death.
    /// `VIClient::SetSpeed` (0x00F8, Class Mastery's 5 and 6): `m_speed`,
    /// the walk's run speed, and its walk speed 3, or 0 when the run speed
    /// is 0. The walk takes them on its next frame (`control_client`).
    fn set_speed(&mut self, v: f32) {
        self.speed = v;
        self.walk_speed = if F::<Ee>::new(v) == F::ZERO { 0.0 } else { 3.0 };
    }

    fn respawned(&mut self, walker: &mut Walker) {
        if self.death.take().is_some()
            && let Some(cs) = walker.sprite.as_mut()
        {
            cs.dead = false;
            cs.set_locomotion(0, &walker.anims, &mut self.spell_rand);
        }
        self.fade.set(1.0, 0.0);
        self.fade.set(0.0, 1.0);
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

    /// The mouse wheel past the closest zoom, or out of first person (the
    /// client's own: `mouse::Zoom`), in play: into first person as P's
    /// `InitCameraStyle` goes there, saved as it saves, the camera it left
    /// kept in `back`; out to that camera's style, distance and tilt (or,
    /// first person having come by P, P's way out).
    pub fn wheel_style(&mut self, walker: &mut Walker, zoom: crate::mouse::Zoom, back: &mut Option<crate::camstyle::ZoomBack>) {
        use crate::mouse::Zoom;
        if self.mode != 0 || zoom == Zoom::None {
            return;
        }
        let rc = self.race_camera();
        match (zoom, back.take()) {
            (Zoom::In, _) => *back = Some(crate::camstyle::zoom_in(&mut walker.cam, rc, &mut walker.scene)),
            (_, Some(b)) => crate::camstyle::zoom_out(&mut walker.cam, &b, rc, &mut walker.scene),
            (_, None) => {
                self.camera_style_key(walker);
                return;
            }
        }
        self.cam_saved = crate::camstyle::save(&walker.cam);
    }

    /// Quit answered yes (`m_p2EC28`, `Process`'s LOGOUT): 0x09B0 to the
    /// server, sent now, then `Disconnect(9, 1)`: in a page its login screen
    /// again, saying word 820 ("Logged out."), as the console goes back to
    /// the server selection word 722 promised; natively the client leaves.
    fn logout(&mut self) {
        self.session.send(&net::stream::Writer::op(0x09B0).data);
        self.session.flush();
        let text = net::session::disconnect_word(9)
            .and_then(|w| self.words.as_ref().map(|t| t.get(w, 0).to_string()))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Logged out.".to_string());
        eprintln!("play: logged out: {text}");
        #[cfg(target_arch = "wasm32")]
        if !self.booted {
            crate::web::disconnected(&text);
        }
        self.quit = true;
    }

    /// Dead: the walk takes no controls (`UpdateControls` is not run).
    pub fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    /// Play alone on the screen (no window, box or map up, nothing typed
    /// into, no world change under way) and the player alive: where a box of
    /// the client's own may stay (`update`'s question).
    pub fn plain(&mut self) -> bool {
        self.mode == 0 && !self.map.open && !self.keyboard_typing() && !self.is_dead() && self.world_change.is_none()
    }

    /// `plain`, and no fight on: where such a box may come up.
    pub fn quiet(&mut self) -> bool {
        self.plain() && !self.auto_attack && self.self_fighting() != Some(true)
    }

    /// Out of the world as the start menu's Quit leaves it (`logout`): for
    /// the update's restart.
    pub fn log_out(&mut self) {
        if !self.quit {
            self.logout();
        }
    }

    /// `UpdateGame`'s own actor, after `AutoAttackTick`: hidden in first
    /// person (`m_cameraMode` 0) while alive (a death leaves it as it was,
    /// `ResetCamera` having left first person); in the follow camera (2)
    /// hidden while the camera is within 0.8 of the head, `m_targetHeight`
    /// above where the actor last stood (`m_p00088`); shown otherwise; and
    /// hidden whatever the camera while its creature carries npc flag 0x200;
    /// and hidden, as first person hides it, while the manual camera (mode
    /// 38) is all the way in (`ManualTerp` 1).
    fn self_visibility(&mut self, walker: &Walker) {
        let mut d = self.self_at.map_or_else(|| walker.cam.walk.phys.smooth_location(), |(at, _)| v3(at));
        d.y += walker.cam.target_height;
        d -= walker.eye.position;
        let me = self.self_id32();
        let flags = self.entities.slots.iter().flatten().find(|e| Some(e.id) == me).and_then(|e| e.creature.as_ref()).map_or(0, |c| c.npc_flags);
        let manual_in = self.mode == 38 && walker.cam.effect.manual_terp == F::ONE;
        (self.self_shown, self.cam_inside) = own_actor_shown(self.self_shown, walker.cam.camera_mode, self.death.is_some(), d.magnitude(), flags, manual_in);
    }

    /// `ToggleP74`: autorun on or off, and the line saying so ("%S: %S",
    /// words 720 and 551 or 552).
    pub fn toggle_autorun(&mut self) {
        self.autorun = !self.autorun;
        let w = |id: i32| self.words.as_ref().map_or_else(|| format!("(word {id})"), |w| w.get(id, 0).to_string());
        let text = format!("{}: {}", w(720), w(if self.autorun { 551 } else { 552 }));
        self.add_chat(text, [1.0; 4]);
    }

    /// `ToggleFollow` (`/follow`, the quick command): on, the target when it
    /// is another of the group's members, else word 1195 ("You must target a
    /// group member to follow.") and off; on, off. `UpdateControls` steers it.
    fn toggle_follow(&mut self) {
        self.following = !self.following;
        if self.following {
            let target = self.target;
            if Some(target) != self.self_id32() && self.group.members[..self.group.count()].iter().any(|m| m.id == target) {
                self.follow_id = target;
            } else {
                self.show_word(1195);
                self.following = false;
            }
        }
    }

    /// What `UpdateControls` changed (`Walker::frame_with`): autorun off
    /// (`ToggleP74`, its line), the follow ended.
    pub fn controls_changed(&mut self, c: crate::controls::Changed) {
        if c.autorun {
            self.toggle_autorun();
        }
        if c.follow {
            self.following = false;
        }
    }

    /// `ClearTarget` (triangle in play): the target let go; nothing is sent.
    pub fn clear_target(&mut self) {
        self.target_info = None;
        self.target = u32::MAX;
    }

    /// `UpdateControls`' opening: the pad lost (its mode from 1 to 0) says
    /// so in red (word 1805) and stops the auto-attack and the autorun.
    pub fn pad_mode(&mut self, mode: i32) {
        if mode == 0 && self.pad_mode == 1 {
            self.warn(1805);
            self.auto_attack = false;
            if self.autorun {
                self.toggle_autorun();
            }
        }
        self.pad_mode = mode;
    }

    /// Whether a pointer may look round: in play (no window up) and with the
    /// camera free (the start menu's framing takes it, `m_p06DC4`).
    pub fn look_free(&self) -> bool {
        self.mode == 0 && self.locks.1 && !self.map.open
    }

    /// What `UpdateControls` reads of the client this frame.
    pub fn control_client(&self) -> crate::controls::Client {
        let now = self.session.now();
        let me = self.self_id32();
        let target = if self.target != u32::MAX && Some(self.target) != me {
            self.actor_pos(self.target).zip(self.self_at.map(|s| s.0))
        } else {
            None
        };
        let (a, b) = self.cast_window;
        // the member followed, as UpdateControls finds it: still in the
        // group and the scene (`FindEntity`), with an actor, as the player is
        let follow = if !self.following {
            crate::controls::Follow::Off
        } else {
            let id = self.follow_id;
            let member = self.group.members[..self.group.count()].iter().any(|m| m.id == id) && self.entities.slots.iter().flatten().any(|e| e.id == id);
            match (member && id != u32::MAX && Some(id) != me).then(|| self.actor_pos(id).zip(self.self_at.map(|s| s.0))).flatten() {
                Some((member_at, self_at)) => crate::controls::Follow::At(member_at, self_at),
                None => crate::controls::Follow::Lost,
            }
        };
        crate::controls::Client {
            follow,
            // UpdateGame: `m_speed`, 1.05 times it while following
            run_speed: if self.following { (F::<Ee>::new(self.speed) * F::new(1.05)).0 } else { self.speed },
            walk_speed: self.walk_speed,
            has_chat: matches!(self.mode, 2 | 28 | 29 | 30 | 33),
            inverted: self.options.inverted != 0,
            auto_attack: self.auto_attack,
            swung: now.wrapping_sub(self.swung) < 1000,
            target,
            autorun: self.autorun,
            casting: now.wrapping_sub(a) as i32 >= 0 && (now.wrapping_sub(b) as i32) < 0,
            // the window's (`mode_locks`) and the client's other holds (the
            // start menu's framing, the manual camera, a server's search)
            move_ok: self.locks.0 && mode_locks(self.mode).0,
            look_ok: self.locks.1 && mode_locks(self.mode).1,
            rooted: self.rooted,
            // IsTimedOut (8 s with nothing arriving) is not applied:
            // go-eqoa-server sends nothing while nothing changes, so it would
            // stop a player standing still (the Sandstorm capture's longest
            // silence is 2.6 s)
            timed_out: false,
            // the keyboard's held axes: the viewer's (`game_keys`)
            keys: [0.0; 8],
            // the mouse steering the body: the viewer's (`Mouse::steer`)
            steer: false,
            // `IsModal`: the manual camera
            modal: self.mode == 38,
            // a frame of the game's: the walker's (`frame_with`)
            step: true,
        }
    }

    /// `SetKey`'s part for a window that has the keys: the editor, the NPC,
    /// bank, support and mail windows, the start menu and what it opened,
    /// the chat box's history, a dialog. False in play (mode 0), where
    /// `VIWndGame` takes them.
    fn window_key(&mut self, key: i32, down: bool) -> bool {
        // the map, while it is up, over everything (`map.rs`)
        if self.map.key(key, down) {
            return true;
        }
        // a server's player search, while it is up: every key (S2 puts its
        // handler in `VIWndGame`'s and the HUD's menus' `OnKeyDown`)
        if self.search.open {
            self.search_key(key, down);
            return true;
        }
        if self.mode == 30 {
            self.quick.key(key, down);
            return true;
        }
        if self.mode == 2 {
            self.edit_key(key, down);
            return true;
        }
        // the voice window, while it is up over play: every key
        if self.voice_window_owns() {
            self.voice_window_key(key, down);
            return true;
        }
        if self.bank_owns() {
            self.bank_key(key, down);
            return true;
        }
        if self.npcw_owns() {
            self.npcw_key(key, down);
            return true;
        }
        if self.support_owns() {
            self.support_key(key, down);
            return true;
        }
        if self.mail_owns() {
            self.mail_key(key, down);
            return true;
        }
        if self.start.is_some() {
            match self.mode {
                13 => self.start_key(key, down),
                5 => self.book_key(key, down),
                35 => self.who_key(key, down),
                _ if self.char_window() => self.char_key(key, down),
                _ if self.panel_up() => self.panel_key(key, down),
                _ => return self.other_window_key(key, down),
            }
            return true;
        }
        self.other_window_key(key, down)
    }

    /// The chat box's history (mode 33) and the dialogs: any mode but play.
    fn other_window_key(&mut self, key: i32, down: bool) -> bool {
        use crate::dialog::Key as D;
        if self.mode == 33 {
            self.chat_scroll_key(key, down);
            return true;
        }
        // play, and the manual camera (the game window's own keys)
        if self.mode == 0 || self.mode == 38 {
            return false;
        }
        let k = match key {
            138 => Some(D::Up),
            139 => Some(D::Down),
            136 => Some(D::Left),
            137 => Some(D::Right),
            134 => Some(D::Enter),
            132 => Some(D::Escape),
            _ => None,
        };
        if down && let Some(k) = k {
            self.dialog_key(k);
        }
        // a key's release still stops the attack (`OnKeyUp` 134)
        if !down && key == 134 {
            self.attack_release();
        }
        true
    }

    /// Keys for the window up from a pointer (`point`, `window_wheel`), one
    /// a frame: each held until the HUD's next frame has taken it, as a
    /// pad's press is (the windows' `KeyFlash` acts on a key still down
    /// when it draws), then let go (`pump_keys`). `pointer::MOVED` is the
    /// move's sound, at once.
    fn press(&mut self, keys: &[i32]) {
        for &k in keys {
            if k == crate::pointer::MOVED {
                self.cues.push(crate::sound::Cue::Ui(11));
            } else {
                self.key_queue.push_back(k);
            }
        }
        if self.held.is_none() {
            self.pump_keys();
        }
    }

    /// After a HUD frame: the pointer's key held through it let go, and the
    /// next one pressed.
    fn pump_keys(&mut self) {
        if let Some(k) = self.held.take() {
            self.window_key(k, false);
        }
        if let Some(k) = self.key_queue.pop_front() {
            self.window_key(k, true);
            self.held = Some(k);
        }
    }

    /// `VIClient::SetKey` for the pad's keys (128 + a `VIConsolePad` field,
    /// `PadKeys`'): to the window that has the keys (the editor, the NPC,
    /// bank, support and mail windows, the start menu and what it opened,
    /// a dialog), else, in play, `VIWndGame::OnKeyDown`/`OnKeyUp` and the
    /// HUD's quick menus. The camera's style and the target's cycle need the
    /// walker and the window's shape.
    pub fn set_key(&mut self, key: i32, down: bool, walker: &mut Walker, aspect: f32) {
        use crate::hudmenu::Key as H;
        // S2's record of a pad key (its hook in SetKey, past the dead test)
        if !self.is_dead() {
            self.presses.key(key, down, self.session.timer() as u32, crate::input::from_device());
        }
        self.keyboard = false;
        if self.typing.is_some() || self.window_key(key, down) {
            return;
        }
        if !down {
            match key {
                134 => self.attack_release(),
                133 => self.info_up = false,
                // L1, R1: the manual camera ended, or the hold given up
                140 | 142 => self.target_key_up(walker),
                _ => {}
            }
            return;
        }
        // The manual camera (mode 38): the HUD's quick menu has the keys, as
        // in play (`UpdateMode`), its up, down and use; left and right light
        // it and switch nothing (`Interact` and `Attack` are play's alone);
        // R3 ends it and opens the chat line. Every other key of the game
        // window's is play's (mode 0) and does nothing here.
        if self.mode == 38 {
            match key {
                136 => self.hud_key(H::Left),
                137 => self.hud_key(H::Right),
                138 => self.hud_key(H::Up),
                139 => self.hud_key(H::Down),
                135 => self.hud_key(H::Use),
                147 => {
                    self.reset_view(walker);
                    self.open_edit(0xB0F0000, 128, -1, false);
                    self.edit.title = self.chat_title();
                }
                _ => {}
            }
            return;
        }
        match key {
            144 => self.open_start_menu(true),
            145 => self.camera_style_key(walker),
            134 => self.attack_press(),
            133 => self.interact_with_target(),
            132 => self.clear_target(),
            140 | 142 => {
                let (proj, v) = walker.view(aspect);
                let dir = glam::Vec3::from(walker.eye.direction.to_array());
                self.target_key(key == 140, walker.eye_position(), dir, proj * v);
            }
            136 => self.hud_key(H::Left),
            137 => self.hud_key(H::Right),
            138 => self.hud_key(H::Up),
            139 => self.hud_key(H::Down),
            135 => self.hud_key(H::Use),
            // R3: the chat line, the edit box with the channel's title
            147 => {
                self.open_edit(0xB0F0000, 128, -1, false);
                self.edit.title = self.chat_title();
            }
            146 => self.toggle_autorun(),
            // R2 held: the chat box's history (mode 33), movement and the
            // camera allowed (`SetP06DC0(1, 1)`)
            143 => {
                self.mode = 33;
                self.locks = (true, true);
                if let Some(c) = self.wnd_chat.as_mut() {
                    c.set_scroll(1);
                }
            }
            // L2: the quick-chat menus (mode 30, `VIWndMenu4`)
            141 => self.open_quick_chat(),
            _ => {}
        }
    }

    /// The dump's abilities, by id, and its token; its quests; its quick-chat
    /// sets (`LoadTextSets`).
    fn take_abilities(&mut self) {
        self.quick.load(&self.dump.text_sets);
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

    /// `Msg00D3`: a summons (word 610) or a resurrection (611) offered by
    /// `name`, put up as `VIWndGame::Prompt(text, 1, 2, 0)` puts it, over
    /// whatever was open (`SetMode(0)`); the answer is 0x00D4
    /// (`dialog_key`). The space's buffer holds only its terminator, so the
    /// name runs into the word, as on the console.
    fn summon_prompt(&mut self, name: &str, kind: u8) {
        if self.mode == 13 {
            self.close_start_menu(0);
        }
        let word = if kind == 0 { 610 } else { 611 };
        let mut text = crate::ui::wide(name);
        text.extend(self.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(word, 0))));
        // text[128]
        text.truncate(127);
        let now = self.session.now() as i64;
        self.mbox.prompt(&text, 1, 2, 0, now);
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

    /// `VIClient::m_worldId`: the world the client is in now, which
    /// `UpdateLocation`'s `ZoneName` and the compass's group patches read. A
    /// teleport into another world (`Msg00B2`) sets it and the next dump
    /// does; the dump kept from the login does not (F3 20261023-053742: in
    /// Rathe the start menu named Tunaria's Anu Village).
    fn world_id(&self) -> i32 {
        self.player.world as i32
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
        let world = self.world_id();
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
                // the live client's RequestGearSet: 0x0021, sets 0-3
                ItemOut::GearSet(set, name) => {
                    if (0..crate::gear::SETS as i32).contains(&set) {
                        eprintln!("play: weapon set {set} prepared as {name:?} (0x0021)");
                        let mut w = net::stream::Writer::op(0x0021);
                        w.u8(set as u8).wide(&name);
                        self.session.send(&w.data);
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

    /// `VIWndGame::SetMode(0)`, for the one window this client keeps apart
    /// from the mode: the start menu (its inventory in it) draws and takes
    /// keys while it is visible, whatever the mode, so it is closed as
    /// `SetMode(0)` closes it (`CloseStartMenu(0)`); every other window runs
    /// in its own mode only and goes with the mode put to 0.
    fn set_mode_zero(&mut self) {
        if self.mode == 13 || self.start.as_ref().is_some_and(|s| s.menu.visible) {
            self.close_start_menu(0);
        } else {
            self.mode = 0;
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
        self.items.set_gear(self.gear.view(inv, &vui.words, vui.language));
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
            // ShowMain: the items put away (m_items->SetVisible(false)), so
            // an inventory the menu was closed over never opens with it
            self.items.visible = false;
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
                    self.items.open(vui, r);
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
                    let name = self.abilities.values().find(|a| a.display_index == index).map_or_else(String::new, |a| a.name.clone());
                    eprintln!("play: memorize {name:?} (display index {index}) onto slot {slot}");
                    if slot >= 10 {
                        self.slot_asked = Some((index, slot, self.session.now() as i64));
                    }
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
        if r.error {
            return;
        }
        // a server's patch (`s2`, Sandstorm's S2: its control panel's
        // colours): three more bytes, when there, are the number's colour
        let rest = &r.data[r.pos..];
        let rgb = (crate::s2::get().damage_rgb && rest.len() >= 3).then(|| [rest[0], rest[1], rest[2]]);
        let me = self.self_id32();
        // the player's or its pet's hit: the guide's kill when it dies
        if amount > 0 && (Some(attacker) == me || (attacker == self.pet_id && self.pet_id != u32::MAX)) {
            self.guide.hit(defender, self.session.now());
        }
        // the player's own hit: after a swap the swing timer follows the
        // server's rounds (`attackclock`), whatever the damage numbers show
        if Some(attacker) == me {
            let delay = self.attack.kind().map_or(3500, |k| if vi::swing::swing_effect(k) == 0 { 3500 } else { vi::swing::swing_delay(k) });
            self.attack.own_hit(self.session.now(), delay as u32);
        }
        if self.options.no_damage != 0 {
            return;
        }
        if Some(attacker) == me {
            self.damage.add_colored(defender, amount, 0, rgb);
        } else if camera_mode != 0 && self.mode != 38
            && let Some(me) = me
        {
            self.damage.add_colored(me, amount, 1, rgb);
        }
    }

    /// A 0x00DB as if the server had sent it (`EQOA_DAMAGE` in a playshot):
    /// `self` and `target` name the player and its target, anything else is
    /// an id in hex.
    /// `EQOA_DIE`: the player's death as 0x005E brings it (a test without
    /// a fight).
    pub fn fake_death(&mut self, walker: &mut Walker) {
        self.msg_005e(walker);
    }

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
                // Recv03: `ShowNotice(text, 7)`: sound 7, then the text in
                // a dialog opened without its own sound (`ShowDialog(0,
                // text, 0, 0)`), queued behind one already up
                Some(crate::classmastery::PartUp::Points(d)) => {
                    let text = self.words.as_ref().map_or_else(String::new, |w| if d > 1 { w.get(1627, 0).replace("%d", &d.to_string()) } else { w.get(1628, 0).to_string() });
                    cues.push(7);
                    self.show_dialog_b(&vi::ui::wide(&text), false);
                }
                // ShowNode < 0: the node cleared
                Some(crate::classmastery::PartUp::Node) if !self.cm_wnd.show_node(vui, r, &self.cm, self.mode == 40) => self.cm.node = None,
                // Recv05, Recv06: `m_owner->SetSpeed(m_speed)`
                Some(crate::classmastery::PartUp::Speed(v)) => self.set_speed(v),
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
                // ClosePanel6DC: CloseQuestLog, back to the start menu (to
                // the world when the quest tracker opened it)
                crate::questlog::QuestUp::Close => {
                    self.mode = 0;
                    if !std::mem::take(&mut self.quest_from_hud) {
                        self.open_start_menu(false);
                    }
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
                    self.search_mode = false;
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                // the player search asks again (`playersearch`), the who list its own
                crate::wholist::WhoUp::Refresh if self.search_mode => {
                    self.who.pending = true;
                    self.session.send(&crate::playersearch::search_levels(self.dump.player.level as i32));
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
        // the face of the voice window's password left on no other line
        e.password = false;
        e.set_max_length(max);
        e.set_text(&crate::ui::wide(&target));
        e.title = if title >= 0 { self.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(title, 0))) } else { Vec::new() };
        e.purpose = purpose;
        e.focus(self.session.now() as i64);
        self.mode = 2;
    }

    /// `VIWndGame::OnKeyDown`'s keyboard chat keys, the edit box R3 opens
    /// (`0xB0F0000`, 128 long, a line for `ChatLine`): Enter (13, as R3) and
    /// `/` (47, the line begun with it) titled with the channel
    /// (`SetEditTitle(1)`), R (82, "/reply ") and T (84, "/tell ") untitled.
    pub fn chat_key(&mut self, text: &str, titled: bool) {
        self.open_edit(0xB0F0000, 128, -1, false);
        if titled {
            self.edit.title = self.chat_title();
        }
        self.edit.set_text(&crate::ui::wide(text));
    }

    /// Whether a server's player search is up (`searchwnd`): it has the
    /// keys and the pointer.
    pub fn search_open(&self) -> bool {
        self.search.open
    }

    /// A key for a server's player search while it is up (S2's FD4B8,
    /// which takes `OnKeyDown`s only): what it asks, done at once.
    pub fn search_key(&mut self, key: i32, down: bool) {
        use crate::searchwnd::Out;
        if !down || !self.search.open {
            return;
        }
        match self.search.key(key) {
            Some(Out::Close) => self.close_search(),
            Some(Out::Send(m)) => {
                self.session.send(&m);
                // `EQOA_SEARCH_FOUND=<n>`: a test's answer, at once, from a
                // server that has none (0x1501, then n players)
                if let Some(n) = std::env::var("EQOA_SEARCH_FOUND").ok().and_then(|v| v.parse::<usize>().ok()) {
                    self.search.clear();
                    for b in crate::searchwnd::test_players(n) {
                        self.search.found(&b);
                    }
                }
            }
            // FAA40: the window closed, then `OnKeyDown(84)`'s chat line,
            // its text the tell's
            Some(Out::Tell(text)) => {
                self.close_search();
                self.chat_key(&String::from_utf16_lossy(&text), false);
            }
            Some(Out::Invite(id, name)) => {
                self.invite_to_group(id, &name);
                self.close_search();
            }
            None => {}
        }
    }

    /// FD178: the search's window down and its list let go, the player and
    /// the camera let go (its FD13C(1)); the HUD's menu is drawn again.
    fn close_search(&mut self) {
        self.search.close();
        self.locks = (true, true);
    }

    /// The HUD is up (SUPPORT's atlas has come): its windows can be drawn.
    pub fn hud_ready(&self) -> bool {
        self.hud.is_some()
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
        crate::editbox::pad_only()
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
                    if !self.voice_edit_done(self.edit.purpose, false) {
                        self.mode = if self.who_tell.take().is_some() { 35 } else { 0 };
                    }
                }
                _ => {}
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
    }

    /// 259: the edit box's text to where it was for.
    fn edit_done(&mut self, purpose: u32) {
        // the voice window's fields (`voicewndplay`)
        if self.voice_edit_done(purpose, true) {
            return;
        }
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
                0x0B0E_000A => self.alarm_command(&text),
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
                self.search_mode = false;
                self.who_open = true;
                self.mode = 35;
            }
            // a server's patch's Group menu (`s2`, Sandstorm's player
            // search): Search, in the patch's window (`searchwnd`) on the
            // player's levels, or in the who window when the patch's window
            // is not this shape; Register, Unregister
            0xFF00_0001 if crate::s2::get().player_search => {
                let level = self.dump.player.level as i32;
                match crate::searchwnd::Art::current() {
                    Some(art) => self.search.open(level, art),
                    None => {
                        self.who.close();
                        self.who.pending = true;
                        self.session.send(&crate::playersearch::search_levels(level));
                        self.search_mode = true;
                        self.who_open = true;
                        self.mode = 35;
                    }
                }
            }
            0xFF00_0002 if crate::s2::get().player_search => self.session.send(&crate::playersearch::register()),
            0xFF00_0003 if crate::s2::get().player_search => self.session.send(&crate::playersearch::unregister()),
            // m_p2EF24.CanChat: in a guild its channel (word 599), else
            // word 570; any other answer "Guild error: n" (word 835)
            0x0B03_0002 => match crate::guild::Cmds::can_member(&self.guild) {
                0 => self.group_changed(4),
                3 => self.show_word(570),
                r => {
                    let w = self.words.as_ref().map_or_else(String::new, |w| w.get(835, 0).to_string());
                    self.add_chat(format!("{w}: {r}"), [1.0; 4]);
                }
            },
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
            0x0A01_0000 => self.logout(),
            0x0B05_2000 => self.session.send_opcode(69),
            // the guild roster: ShowInvite (OpenGuildMenu, mode 21), or word 570
            0x0B05_1001 => {
                if self.guild.in_guild {
                    self.open_guild(21);
                } else {
                    self.show_word(570);
                }
            }
            // Assist Leader (123, 0x007B) and Assist Target (203, 0x00CB):
            // the server answers with whom to target (Msg007B)
            0x0B09_0000 => self.session.send_op_int(123, target),
            0x0B0E_0000 => self.session.send_opcode(198),
            0x0B0E_0001 => self.session.send_opcode(222),
            0x0B0E_0003 => self.session.send_opcode(200),
            0x0B0E_0006 => self.session.send_op_int(203, target),
            0x0B0E_0007 => self.toggle_follow(),
            0x0905_0000 | 0x0509_1004 | 0x0B0E_0002 => {}
            0x0509_1005 => {
                let w = self.words.as_ref().map_or_else(String::new, |w| w.get(836, 0).to_string());
                self.add_chat(format!("{w} 1.108 Protocol=18"), [1.0; 4]);
            }
            0x0902_0000 => {
                let p = self.player.pos;
                self.add_chat(format!("{:.1} {:.1} {:.1}", p[0], p[1], p[2]), [1.0; 4]);
            }
            // VIWndGame::OnMessage 261 in play, where DoAction hands these:
            // the chat line with the channel's title, Tell, Reply
            0x0301_0000 => {
                self.open_edit(0xB0F0000, 128, -1, false);
                self.edit.title = self.chat_title();
            }
            0x0B0D_0000 => self.open_mail(28),
            0x0B0D_0001 => self.open_mail(29),
            // m_p38300.OpenPanel: the ignore list; ShowTargetPlayerName:
            // the edit box with the target's name, "Ignore Target" (995)
            0x0B05_3000 => self.open_panel(26),
            0x0B05_3001 => self.open_edit(id, 24, 995, true),
            // Stop Ignoring: DoAction has no case for it (the start menu's
            // entry asks for a name on its own path)
            0x0B05_3002 => {}
            // VIClientPart2EFCC::SendFlag: shouts heard or not (0x0C01, 1);
            // the client keeps the choice (the game keeps none: the server
            // forgets it at the next login), told again at each login
            // (`tell_shouts`), and drops what shouts still come meanwhile
            0x0B03_0003 => {
                self.send_shout_flag(arg != 0);
                self.shouts_told = true;
                match crate::settings::set_shouts(arg != 0) {
                    Ok(()) => eprintln!("chat: shouts {} (kept)", if arg != 0 { "heard" } else { "ignored" }),
                    Err(e) => eprintln!("chat: shouts {}, not kept: {e}", if arg != 0 { "heard" } else { "ignored" }),
                }
            }
            // m_p2EF24.SetChat: the guild's chat heard or not
            0x0B05_100C => self.session.send(&crate::guild::Cmds::set_chat(arg != 0)),
            // DefaultTextSets: the built-in quick-chat menus, sent
            0x0B08_0000 => {
                for m in crate::quickchat::default_text_sets(&mut self.quick.sets) {
                    self.session.send(&m);
                }
            }
            // VITimeOfDay::GetLocalTime, to the chat box
            0x0B0E_0008 => {
                let t = crate::alarm::local_time_text(crate::alarm::local_time());
                self.add_chat(t, [1.0; 4]);
            }
            // the alarm: off, or the edit box for its time (ShowNameClear,
            // word 1199, 256 long)
            0x0B0E_0009 => {
                if self.alarm.on {
                    let words = self.words.clone();
                    let w = move |n: i32| words.as_ref().map_or_else(|| format!("(word {n})"), |t| t.get(n, 0).to_string());
                    let now = self.session.now() as i64;
                    let outs = self.alarm.set(false, 0, 0, now, &w);
                    self.alarm_outs(outs);
                } else {
                    self.open_edit(0x0B0E_000A, 256, 1199, false);
                }
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
        eprintln!("play: {name:?} {} slot {slot} (0x00B4)", if valid { "onto" } else { "off" });
        let fmt = self.words.as_ref().map_or_else(|| "%S".to_string(), |w| w.get(if valid { 824 } else { 825 }, 0).to_string());
        self.add_chat(fmt.replacen("%S", &name, 1).replacen("%s", &name, 1), [1.0; 4]);
        self.cues.push(crate::sound::Cue::Ui(6));
    }

    /// `Send0032`: use an ability, by its slot, on the target; on a server
    /// whose patch has it, with S2's record of the pad after (the live S2's
    /// cave before SendRaw: 11 bytes).
    fn send_0032(&mut self, id: i64) {
        let Some(slot) = self.abilities.get(&id).map(|a| a.slot) else { return };
        eprintln!("play: use slot {slot} ({:?})", self.abilities.get(&id).map_or("", |a| a.name.as_str()));
        let mut w = net::stream::Writer::op(0x0032);
        w.u32(self.ability_token).u8(slot as u8).u32(self.target);
        if crate::s2::get().use_skill_record {
            w.bytes(&self.presses.tail());
        }
        self.session.send(&w.data);
    }

    /// A keyboard's or a pointer's key for the HUD's quick menus (the
    /// arrows, E, a click), into S2's record as the pad key it stands for
    /// (on a PS2 each is that key through SetKey: the d-pad, circle).
    pub fn note_hud_key(&mut self, key: crate::hudmenu::Key, down: bool) {
        use crate::hudmenu::Key as H;
        let code = match key {
            H::Up => 138,
            H::Down => 139,
            H::Left => 136,
            H::Right => 137,
            H::Use => 135,
        };
        if !self.is_dead() {
            self.presses.key(code, down, self.session.timer() as u32, crate::input::from_device());
        }
    }

    /// The keyboard's C (99) held in play: the chat box's history, as R2
    /// (mode 33, movement and the camera allowed, `SetP06DC0(1, 1)`); in it,
    /// the keys to the chat box (its up and down 19 and 20), C let go back
    /// to play (`chat_scroll_key`).
    pub fn chat_history_key(&mut self, key: i32, down: bool) {
        match self.mode {
            0 if down && key == 99 => {
                self.mode = 33;
                self.locks = (true, true);
                if let Some(c) = self.wnd_chat.as_mut() {
                    c.set_scroll(1);
                }
            }
            33 => self.chat_scroll_key(key, down),
            _ => {}
        }
    }

    /// `VIWndGame::OnKeyDown`'s L2 (the keyboard's Z) in play: the
    /// quick-chat menus over the root's set, with the keys (mode 30; the
    /// effects' bar goes while they are up). In mode 30 the key reaches the
    /// menus, which pass it up to close them.
    pub fn open_quick_chat(&mut self) {
        if self.mode == 0 && self.quick.open() {
            self.mode = 30;
        }
    }

    /// A key for the quick-chat menus (the game's codes; the keyboard's
    /// cursor keys 17-20 are directions as the d-pad's are).
    pub fn quick_key(&mut self, key: i32, down: bool) {
        if self.mode == 30 {
            self.quick.key(key, down);
        }
    }

    /// The quick-chat editor's edit box has the keys: the keyboard types.
    pub fn quick_typing(&mut self) -> bool {
        self.mode == 30 && self.quick.typing()
    }

    /// The start menu's item menu has a weapon set's name editor up (the
    /// live client's): the keyboard types into it.
    pub fn items_typing(&self) -> bool {
        self.mode == 13 && self.items.typing()
    }

    /// A quick-chat menu (not its editor) has the keys: the touch screen's
    /// pad keeps its d-pad, to hold a direction.
    pub fn quick_dpad(&mut self) -> bool {
        self.mode == 30 && self.quick.menu_has_keys()
    }

    /// `EQOA_QUICK_TEXT=a|b`: each piece typed into the quick-chat editor's
    /// edit box as it opens, a character a frame, then Enter.
    pub fn quick_script_step(&mut self, text: &str) {
        if !self.quick_typing() {
            if self.quick_typed.1 > 0 {
                self.quick_typed = (self.quick_typed.0 + 1, 0);
            }
            return;
        }
        let (piece, k) = self.quick_typed;
        let Some(t) = text.split('|').nth(piece) else { return };
        match t.chars().nth(k) {
            Some(c) => self.quick.key(c as i32, true),
            None if k == t.chars().count() => self.quick.key(13, true),
            None => return,
        }
        self.quick_typed.1 += 1;
    }

    /// The quick-chat menus' keys and drawing, on one of the game's frames
    /// (each window there counts frames); what they asked, for after it.
    fn run_quick(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) -> Vec<crate::quickchat::Out> {
        if self.mode != 30 {
            if self.quick.visible() {
                self.quick.close();
            }
            return Vec::new();
        }
        let (mut cues, mut outs) = (Vec::new(), Vec::new());
        let mut ctx = crate::quickchat::Ctx { vui, now: self.session.now() as i64, cues: &mut cues, outs: &mut outs };
        self.quick.run(r, &mut ctx);
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        outs
    }

    /// What the quick-chat menus asked: a line said (`VIClientText::Expand`
    /// first, `ChatLine`), 258 (closed, then the command's `DoAction`), the
    /// server's messages, and the keys they passed up to `VIWndGame`: in
    /// mode 30 Escape, △, L2 and Z close them, Enter and R3 open the chat
    /// line in their place; ✕ and □ let go stop the attack and the target's
    /// details.
    fn quick_outs(&mut self, outs: Vec<crate::quickchat::Out>) {
        use crate::quickchat::Out;
        for o in outs {
            match o {
                Out::Say(line) => {
                    let text = vi::ui::expand(&line, 256, false, &self.expand_context());
                    self.chat_line(&String::from_utf16_lossy(&text));
                }
                Out::Done(msg, arg) => {
                    if self.mode == 30 {
                        self.mode = 0;
                        if msg != -1 {
                            self.do_action(msg as u32, arg);
                        }
                    }
                }
                Out::Send(m) => self.session.send(&m),
                Out::Key(k, true) => match k {
                    27 | 132 | 141 | 90 | 122 if self.mode == 30 => self.mode = 0,
                    13 | 147 => {
                        self.open_edit(0xB0F0000, 128, -1, false);
                        self.edit.title = self.chat_title();
                    }
                    _ => {}
                },
                Out::Key(k, false) => match k {
                    134 => self.attack_release(),
                    133 => self.info_up = false,
                    _ => {}
                },
            }
        }
    }

    /// What `VIClientText::Expand` reads off the client: the player's name,
    /// level and class (WORDS.TXT 165 + the class), the target's name.
    fn expand_context(&self) -> vi::ui::ExpandContext {
        let cls = self.dump.player.class;
        let class_name = if (0..=14).contains(&cls) { self.words.as_ref().map_or_else(String::new, |w| w.get(165 + cls as i32, 0).to_string()) } else { String::new() };
        vi::ui::ExpandContext { player_name: self.dump.player.name.clone(), level: self.dump.player.level as i32, class_name, target_name: self.target_name().unwrap_or_default() }
    }

    /// A mouse button pressed at window pixel `at`: on a quick-chat choice it
    /// holds that direction until it is let go (`point_release`), as the
    /// d-pad would: a click is a tap, a press of half a second the editor.
    /// True when it took the press (its click is then not a click).
    pub fn point_press(&mut self, at: (f64, f64), size: (u32, u32)) -> bool {
        if self.mode != 30 || self.quick_held.is_some() {
            return false;
        }
        let f = render::ui::frame_from_window(size, [at.0 as f32, at.1 as f32]);
        let Some(k) = self.quick.choice_key(f) else { return false };
        self.keyboard = true;
        self.quick.key(k, true);
        self.quick_held = Some(k);
        true
    }

    /// The button let go: a held choice's direction let go. True when the
    /// press was one (its click is dropped).
    pub fn point_release(&mut self) -> bool {
        let Some(k) = self.quick_held.take() else { return false };
        if self.mode == 30 {
            self.quick.key(k, false);
        }
        true
    }

    /// The alarm's lines and its ring (`VIAlarm`'s `ShowMsg`, `ShowText`,
    /// `PromptText`).
    fn alarm_outs(&mut self, outs: Vec<crate::alarm::AlarmOut>) {
        use crate::alarm::AlarmOut;
        for o in outs {
            match o {
                AlarmOut::Word(w) => self.show_word(w),
                AlarmOut::Text(t) => self.add_chat(t, [1.0; 4]),
                AlarmOut::Prompt(t) => {
                    let now = self.session.now() as i64;
                    self.mbox.prompt(&crate::ui::wide(&t), 0, 0, 0, now);
                    self.mode = 14;
                }
            }
        }
    }

    /// `VIAlarm::Command`: `/alarm`'s and the edit box's text.
    fn alarm_command(&mut self, arg: &str) {
        let words = self.words.clone();
        let w = move |n: i32| words.as_ref().map_or_else(|| format!("(word {n})"), |t| t.get(n, 0).to_string());
        let off = w(552);
        let now = self.session.now() as i64;
        let outs = self.alarm.command(arg, &off, now, &w);
        self.alarm_outs(outs);
    }

    /// `SelectTarget(which)`, the keyboard's 1 to 6 in play: 1 the player,
    /// 2 to 5 the group's others in roster order (a place holding the player
    /// gives the next), 6 the pet; past the group, or anything else, none.
    /// The auto-attack stops; 0x0010 with the token and the choice.
    pub fn select_target(&mut self, which: i32) {
        let me = self.self_id32();
        let ids: Vec<u32> = self.group.members[..self.group.count()].iter().map(|m| m.id).collect();
        let n = ids.len() as i32;
        self.target = match which {
            1 => me.unwrap_or(u32::MAX),
            2..=5 if which <= n => {
                let k = (which - 2) as usize;
                if Some(ids[k]) == me { ids.get(k + 1).copied().unwrap_or(u32::MAX) } else { ids[k] }
            }
            6 => self.pet_id,
            _ => u32::MAX,
        };
        self.auto_attack = false;
        self.target_info = None;
        let mut w = net::stream::Writer::op(0x0010);
        w.u32(0).u32(self.target);
        self.session.send(&w.data);
    }

    /// The ability bars the HUD and the spell book show: one, or with the
    /// expansion mask's bit 1 the game's two, or as many as the Extended
    /// page's Ability Bars asks (their slots the server's to keep).
    pub fn ability_bars(&self) -> i32 {
        if (self.session.expansion >> 1) & 1 != 0 { (crate::settings::ability_bars() as i32).clamp(2, crate::hudmenu::MAX_BARS) } else { 1 }
    }

    /// What the HUD's carousel holds: the ability bars (`ability_bars`) and
    /// the weapon sets (a server that keeps them).
    pub fn hud_bars(&self) -> crate::hudmenu::Bars {
        crate::hudmenu::Bars { abilities: self.ability_bars(), gear: self.gear.on }
    }

    /// The live client's `UseGearSet` (0x012C3418): 0x0022 with the set
    /// (4 bare hands).
    fn use_gear_set(&mut self, set: i64) {
        if (0..=crate::gear::BARE_HANDS).contains(&set) {
            eprintln!("play: weapon set {set} taken up (0x0022)");
            let mut w = net::stream::Writer::op(0x0022);
            w.u8(set as u8);
            self.session.send(&w.data);
        }
    }

    /// A pad key for the HUD's quick menus in play (`VIWndHUDMenu::OnKeyDown`
    /// and `VIWndGame`'s left and right): using an ability sends 0x0032.
    pub fn hud_key(&mut self, key: crate::hudmenu::Key) {
        // in play, and in the manual camera (`UpdateMode` 38 gives the menu
        // the keys as 0 does), where left and right switch nothing
        if self.mode != 0 && self.mode != 38 {
            return;
        }
        let now = self.session.now() as i64;
        let bars = self.hud_bars();
        let mut cues = Vec::new();
        let used = self.hud_menus.key(key, now, bars, self.mode == 0, &mut cues);
        self.cues.extend(cues);
        if let Some((msg, id)) = used {
            match msg {
                0x8060000 => self.send_0032(id),
                0x2030000 => self.use_item(id as i32, 1),
                crate::gear::MSG => self.use_gear_set(id),
                _ => {}
            }
        }
    }

    /// A memorize past the game's ten slots that the server left unanswered
    /// for three seconds (`slot_asked`): said in red, the client's words, as
    /// the server did not keep it.
    fn slot_unanswered(&mut self) {
        let Some((_, slot, at)) = self.slot_asked else { return };
        if (self.session.now() as i64) - at < 3000 {
            return;
        }
        self.slot_asked = None;
        eprintln!("play: no 0x00B4 for slot {slot}: the server keeps the game's ten slots only");
        self.add_chat(format!("This server did not keep the ability on bar {}: it keeps abilities on the game's two bars only.", slot / 5 + 1), [1.0, 0.25, 0.25, 1.0]);
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
        if !self.auto_attack || !self.attack.is_due(now) {
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
        // a swing other than the last tick's (a swap) has the timer follow
        // the server's rounds for the rest of the fight
        self.attack.swing(swing);
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
        self.attack.add(delay as u32);
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

    /// `VIClientPart2EFCC::SendFlag(b)`: 0x0C01, 1, whether shouts are heard.
    fn send_shout_flag(&mut self, heard: bool) {
        let mut w = net::stream::Writer::op(0x0C01);
        w.u8(1).u8(heard as u8);
        self.session.send(&w.data);
    }

    /// Ignore Shouts kept (`settings::shouts`): the server told once a login,
    /// as the player in the world, as if Ignore Shouts were chosen again. A
    /// login and not each dump: the server keeps the flag while the session
    /// stands (the PS2 sends it only when chosen), and what a server answers
    /// to it is in no capture, so a reply line at every respawn and world
    /// change could follow; a server that forgets meanwhile has its shouts
    /// dropped (`Event::Chat`).
    fn tell_shouts(&mut self) {
        if self.shouts_told {
            return;
        }
        self.shouts_told = true;
        if !crate::settings::shouts() {
            eprintln!("chat: shouts ignored (kept): the server told again");
            self.send_shout_flag(false);
        }
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

    /// F1 in play, or the HUD's "F1 Controls": the Options window on its
    /// Controls page (the client's own; F1 makes no key code in the game's
    /// keyboard table, `VIKeyboard::Read`). Not without the start menu's
    /// CLIENT, nor over another window.
    pub fn open_controls(&mut self) {
        if self.start.is_some() && self.hud_ready() && self.mode == 0 && !self.is_dead() {
            self.open_panel(34);
            self.controls_open = true;
        }
    }

    /// macOS's Settings… (⌘,): the Options window on its menu, as the start
    /// menu's Options entry opens it; as F1's Controls page, not over another
    /// window, the start menu included (its Options are there), nor dead.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn open_options(&mut self) {
        if self.start.is_some() && self.hud_ready() && self.mode == 0 && !self.is_dead() {
            self.open_panel(34);
        }
    }

    /// What the Controls page waits for, if anything: a key pressed then is
    /// its (`capture_key`), a controller's button too (`capture_button`).
    pub fn capturing(&self) -> Option<crate::bindpage::Column> {
        if self.mode == 34 { self.options_wnd.capturing() } else { None }
    }

    /// A key pressed while the Controls page waits, by its name.
    pub fn capture_key(&mut self, name: &str) {
        let mut cues = Vec::new();
        if let Some(h) = self.hud.as_ref() {
            self.options_wnd.capture_key(&h.0, name, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
    }

    /// A controller's button pressed while the Controls page waits (its bit
    /// index as the controller has it).
    pub fn capture_button(&mut self, phys: u8) {
        let mut cues = Vec::new();
        if let Some(h) = self.hud.as_ref() {
            self.options_wnd.capture_button(&h.0, phys, &mut cues);
        }
        self.keyboard = false;
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
    }

    /// Whether the keyboard is typing into an edit box (the edit box, the
    /// quick chat's, the bank's, the NPC windows', support's and mail's
    /// editors, the plain line before the HUD): its keys are never remapped
    /// (`binds`).
    pub fn keyboard_typing(&mut self) -> bool {
        self.typing.is_some()
            || self.mode == 2
            || self.quick_typing()
            || (self.bank_owns() && self.bank_typing())
            || (self.npcw_owns() && self.npcw_typing())
            || (self.support_owns() && self.support_typing())
            || (self.mail_owns() && self.mail_typing())
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
                34 if std::mem::take(&mut self.controls_open) => self.options_wnd.open_controls(vui, r, &self.options),
                34 => self.options_wnd.open(vui, r, &self.options),
                _ => {}
            }
        }
        for (k, down) in std::mem::take(&mut self.panel_keys) {
            match self.mode {
                17 | 18 => buddy_ups.extend(self.buddy_wnd.key(menus, &self.buddies, &words, k, down, now, &mut cues)),
                26 => ignore_ups.extend(self.ignore_wnd.key(menus, &self.ignore, &words, k, down, now, &mut cues)),
                34 if down => self.options_wnd.key(vui, r, &mut self.options, k, now, &mut cues, &mut option_ups),
                34 => self.options_wnd.release(k),
                _ => {}
            }
        }
        match self.mode {
            17 | 18 => buddy_ups.extend(self.buddy_wnd.draw(vui, r, menus, &self.buddies, now, &mut cues)),
            26 => self.ignore_wnd.draw(vui, r, menus, &self.ignore, now, &mut cues),
            34 => {
                self.options_wnd.draw(vui, r, &self.options, &mut option_ups);
                cues.extend(self.options_wnd.take_cues());
            }
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

    /// The map's frame (`map.rs`): its build behind the game, and while it
    /// is up its view and quads, the player and the group on it. It closes
    /// when a window comes up (the server's, a dialog) or the player dies.
    #[allow(clippy::too_many_arguments)]
    pub fn map_frame(&mut self, walker: &Walker, lib: &Library, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, size: (u32, u32), dt: f32, sticks: [f32; 4]) {
        if self.map.open && (self.mode != 0 || self.is_dead()) {
            self.map.close();
        }
        let now = self.session.now();
        self.guide_frame(walker, now);
        let guide_on = self.guide_on(now);
        let p = &walker.cam.walk.phys;
        let me = self.self_id32();
        let group = self.group.members[..self.group.count()]
            .iter()
            .enumerate()
            .filter(|(_, m)| Some(m.id) != me)
            .filter_map(|(index, m)| {
                // the entity's actor while it has one, else channel 0x40's place in this world
                let pos = self.actor_pos(m.id).or_else(|| (m.x == self.player.world as i32).then_some(m.pos))?;
                Some((pos, index))
            })
            .collect();
        let origin = walker.scene.grid.as_ref().map_or([1000.0, 1000.0], |g| [g.origin.x.0, g.origin.z.0]);
        let cx = crate::map::Context {
            lib,
            device,
            queue,
            format,
            size,
            dt,
            player: Some(([p.position.x.0, p.position.y.0, p.position.z.0], p.angles.x.0)),
            world: self.player.world as i32,
            origin,
            group,
            sticks,
            keyboard: self.keyboard,
            words: self.words.as_ref(),
            vui: self.hud.as_ref().map(|h| &h.0),
            quest_marks: quest_marks_of(&self.session, &self.guide, guide_on),
        };
        self.map.frame(&cx);
        // the minimap where the quick menus are, not under Hide UI
        if self.minimap_up() && !crate::hideui::hidden() {
            let mv = self.mini_view();
            self.map.mini_frame(&cx, &mv);
        } else {
            self.map.mini_off();
        }
    }

    /// Whether the minimap is up (`map::mini`): Extended's Minimap on, in
    /// play where the HUD's quick menus are (no window up, no player
    /// search), the HUD made, the map not up and no loading screen.
    pub fn minimap_up(&self) -> bool {
        crate::settings::client().minimap && self.right_hud_up()
    }

    /// Whether the HUD's right side under the quick menus is up: the game's
    /// own play (mode 0, the quick chat's 30, the manual camera's 38), no
    /// search, voice window, map or loading screen over it, the HUD made.
    fn right_hud_up(&self) -> bool {
        (self.mode == 0 || self.mode == 30 || self.mode == 38) && !self.search.open && !self.voice_wnd.open && !self.map.open && !self.loading.up() && self.hud.is_some()
    }

    /// The quest tracker's quests while it is up (`tracker`): Quest Tracker
    /// on, where the minimap could be.
    fn tracker_quests(&self) -> Vec<net::session::TrackedQuest> {
        if !crate::settings::client().quest_tracker || !self.right_hud_up() {
            Vec::new()
        } else if self.guide_on(self.session.now()) {
            self.guide.estimate.tracker.clone()
        } else {
            self.session.quest_progress.clone()
        }
    }

    /// The quest marks for the map and the minimap now (`quest_marks_of`).
    fn quest_marks(&self, now: u32) -> Option<&[net::session::QuestMark]> {
        quest_marks_of(&self.session, &self.guide, self.guide_on(now))
    }

    /// The guide off for good once this server offers its own quest marks or
    /// tracker; its wait begun once the player is in the world.
    fn guide_wait(&mut self, now: u32) {
        self.server_quests |= !self.guide_forced && (self.session.quest_marks_offer || self.session.quest_progress_offer);
        if self.guide_from.is_none() && self.self_id32().is_some() && !self.loading.up() {
            self.guide_from = Some(now.wrapping_add(GUIDE_WAIT_MS));
        }
    }

    /// Whether the quest guide makes the marks and the tracker: this server
    /// never offered its own in the session (a scripted run may say it did
    /// not), and the wait into the world is over.
    fn guide_on(&self, now: u32) -> bool {
        !self.server_quests && self.guide_from.is_some_and(|t| now.wrapping_sub(t) as i32 >= 0)
    }

    /// The guide's frame while it is on: the journal, the player and its
    /// items as they are, and the creatures the player hit that lie dead.
    fn guide_frame(&mut self, walker: &Walker, now: u32) {
        if !self.guide_on(now) {
            return;
        }
        self.guide.character(self.player.server_id, &self.dump.player.name);
        let p = &walker.cam.walk.phys.position;
        let journal: Vec<String> = self.quests.titles.iter().map(|t| String::from_utf16_lossy(t)).collect();
        let input = crate::questguide::Input {
            journal: &journal,
            class: self.dump.player.class,
            race: self.dump.player.race,
            level: self.dump.player.level,
            world: self.player.world as i32,
            at: [p.x.0, p.z.0],
            items: &self.dump.inventory.items,
            tunar: self.dump.player.p34,
        };
        let entities = &self.entities;
        let dead = |id: u32| entities.slots.iter().flatten().find(|e| e.id == id && e.p18 & 2 != 0).and_then(|e| e.creature.as_ref()).map(|c| c.name.clone());
        self.guide.frame(&input, &dead, now);
    }

    /// The game's "You have finished a quest!" (0x0046): the guide's quest
    /// whose line left the journal is finished.
    fn guide_finished(&mut self, now: u32) {
        if !self.guide_on(now) {
            return;
        }
        let journal: Vec<String> = self.quests.titles.iter().map(|t| String::from_utf16_lossy(t)).collect();
        let at = self.dump.player.pos;
        let input = crate::questguide::Input {
            journal: &journal,
            class: self.dump.player.class,
            race: self.dump.player.race,
            level: self.dump.player.level,
            world: self.player.world as i32,
            at: [at[0], at[2]],
            items: &self.dump.inventory.items,
            tunar: self.dump.player.p34,
        };
        self.guide.finished_notice(&input, now);
    }

    /// What the minimap shows of the entities: every creature and player in
    /// the scene but the player and the group (the minimap draws those as
    /// the map does) and corpses, in its name's colour (`plates`), and the
    /// target.
    fn mini_view(&self) -> crate::map::mini::MiniView {
        let me = self.self_id32();
        let group: Vec<u32> = self.group.members[..self.group.count()].iter().map(|m| m.id).collect();
        let dots = self
            .entities
            .slots
            .iter()
            .flatten()
            .filter(|e| Some(e.id) != me && !group.contains(&e.id) && e.p18 & 2 == 0 && self.in_scene(e))
            .filter_map(|e| {
                let c = e.creature.as_ref()?;
                let color = if c.p256 < 2 { 0 } else if c.p299 & 1 != 0 { 2 } else { 1 };
                Some(([e.actor_pos[0], e.actor_pos[2]], NAME_COLORS[color]))
            })
            .collect();
        let target = self.actor_pos(self.target).filter(|_| Some(self.target) != me).map(|p| [p[0], p[2]]);
        crate::map::mini::MiniView { dots, target, marks: self.mini_marks() }
    }

    /// The marks on the minimap (`map::mini::Mark`: a kind, a world, a place
    /// and a tooltip), every source's: the map's XP camps and coach stops,
    /// the quests' to take and steps to do. Each source appends its own here;
    /// the minimap draws what it is given (the quests' over the rest, on its
    /// rim while farther).
    fn mini_marks(&self) -> Vec<crate::map::mini::Mark> {
        let mut out = Vec::new();
        // the quests' (`map::quests`, the server's or the guide's), while
        // Quest Markers is on
        if let Some(m) = self.quest_marks(self.session.now()) {
            out.extend(crate::map::quests::mini_marks(m));
        }
        out
    }

    /// Builds the HUD on the viewer's device: the 2D layer, UI.CSF's and
    /// ITEMICON.ESF's surfaces (the item icons the client's icon cache
    /// would stream), ADDART.CSF's (the compass's patch), FX.CSF's damage
    /// font and "ArialUI" out of the loaded files, uploaded the first time
    /// (as the viewer gets its device); then `VIUI` over them, once
    /// SUPPORT's atlas is here. The viewer calls it every frame.
    pub fn attach_ui(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, lib: &Library) {
        // the art swapped since the layer was made (F9, `era`): the layer and
        // its surfaces made again in the art now, the HUD's windows kept
        if self.ui_art.is_some_and(|a| a != lib.view()) {
            self.ui_art = Some(lib.view());
            if let Some(hud) = self.hud.as_mut() {
                let (ui, mut assets) = ui_layer(device, queue, format, lib);
                assets.glyphs = Some(crate::pointer::GlyphTable::new(&hud.0));
                hud.1 = assets;
                renderer.ui = Some(ui);
            } else if self.ui_assets.take().is_some() {
                renderer.ui = None;
            }
        }
        if self.hud.is_none() && self.ui_assets.is_none() && (self.ui_textures.is_some() || self.ui_data.is_some()) {
            let (ui, assets) = ui_layer(device, queue, format, lib);
            renderer.ui = Some(ui);
            self.ui_assets = Some(assets);
            self.ui_art = Some(lib.view());
        }
        if self.hud.is_none()
            && self.ui_data.is_some()
            && let Some(assets) = self.ui_assets.take()
        {
            let (atlas, textures) = self.ui_data.take().unwrap();
            let words = self.words.clone().unwrap_or_default();
            let vui = crate::ui::Vui::new(words, textures, atlas);
            let mut assets = assets;
            assets.glyphs = Some(crate::pointer::GlyphTable::new(&vui));
            self.hud = Some((vui, assets, crate::hud::Hud::new()));
            self.wnd_chat = Some(crate::chat::WndChat::new());
        }
    }

    /// The touch screen's pad over the HUD (`touch.rs`): its discs, and the
    /// atlas's glyphs once SUPPORT's atlas is here.
    pub fn draw_touch(&self, ui: &mut render::ui::Ui, touch: &crate::touch::TouchPad, size: (u32, u32)) {
        let mut glyphs: Vec<(i32, crate::touch::Glyph)> = Vec::new();
        if let Some((vui, assets, _)) = self.hud.as_ref() {
            for b in crate::touch::layout(size).buttons {
                let e = vui.entry(b.glyph);
                let Some(tex) = assets.surface(vui.entry_id(&e)) else { continue };
                let (w, h) = ui.texture_size(tex);
                let (w, h) = (w as f32, h as f32);
                let uv = [e.uv0[0] as f32 / w, e.uv0[1] as f32 / h, (e.uv0[0] + e.uv1[0]) as f32 / w, (e.uv0[1] + e.uv1[1]) as f32 / h];
                glyphs.push((b.glyph, (tex, uv, [e.uv1[0] as f32, e.uv1[1] as f32])));
            }
        }
        touch.draw(ui, size, self.window_up(), &|id| glyphs.iter().find(|g| g.0 == id).map(|g| g.1));
    }

    /// The HUD on a frame drawn: on one of the game's (`step`, `frames.rs`)
    /// made again and kept, else the kept quads drawn again; and either way
    /// the damage numbers under it, placed through this frame's view.
    pub fn hud(&mut self, ui: &mut render::ui::Ui, walker: &Walker, time_ms: i64, view_proj: glam::Mat4, size: (u32, u32), step: bool) {
        // the loading screen up or down since the quads kept: made again now
        if step || self.loading.kept != self.loading.up() {
            self.loading.kept = self.loading.up();
            self.draw_hud(ui, walker, time_ms, view_proj, size);
            ui.keep();
        } else {
            ui.again();
        }
        if !self.loading.up() {
            let under = self.damage.place(view_proj, size, |id| self.drawn_at(id));
            ui.quads.splice(0..0, under);
        }
    }

    /// The HUD's quads for one of the game's frames (`VIWndGame::OnDrawSelf`),
    /// its windows stepped (their pulses, animations and scrolls count
    /// frames).
    fn draw_hud(&mut self, ui: &mut render::ui::Ui, walker: &Walker, time_ms: i64, view_proj: glam::Mat4, size: (u32, u32)) {
        ui.begin(size);
        // LoadingProgress in place of the game's frame: no HUD, no windows
        if self.loading.up() {
            let hud = self.hud.as_ref();
            if let Some(assets) = hud.map(|h| &h.1).or(self.ui_assets.as_ref()) {
                let mut r = assets.raster(ui);
                // the Frontiers layout, its logo too; the original game's in
                // the 2002 art (`era`)
                crate::loading::draw_2d(hud.map(|h| &h.0), &mut r, net::session::EXPANSION_FLAGS & 2 != 0 && self.ui_art != Some(esf::Era::Y2002));
            }
            ui.end();
            return;
        }
        // UpdateGame's manual camera all the way in: `TargetCentre`, its
        // crosshair drawn below (`DrawReticle`)
        let reticle = (self.mode == 38 && walker.cam.effect.manual_terp == F::ONE).then(|| self.target_centre(walker, view_proj));
        // VIWndEdit::OnDrawSelf's `sPadOnly`: a keyboard in use and the
        // options' Soft Keyboard on show the compact face
        crate::editbox::set_pad_only(self.keyboard && self.options.compact != 0);
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
            target_of_target: self.target_of_target(),
            target_power: target.and_then(|e| self.bar_power(e.id)),
            tot_hp: self.target_of_target_id().and_then(|t| find(Some(t))).map(|e| e.p19),
            tot_power: self.target_of_target_id().and_then(|t| self.bar_power(t)),
            pet,
            group_count: self.group.count() as i32,
            group: self.group.members[..self.group.count()]
                .iter()
                .filter(|m| Some(m.id) != me && m.active)
                .map(|m| {
                    let flags = find(Some(m.id)).and_then(|e| e.creature.as_ref()).map_or(0, |c| c.flags262);
                    crate::hud::MemberState { id: m.id, name: m.name.clone(), is_target: m.id == self.target, hp: (m.hp_known != 0).then_some((m.hp, flags & 4 != 0, flags & 8 != 0)), power: self.bar_power(m.id) }
                })
                .collect(),
            patches: self.group.members[..self.group.count()]
                .iter()
                .enumerate()
                .map(|(index, m)| {
                    // the entity's actor while it has one, else channel 0x40's place in this world
                    let actor = if Some(m.id) == me { self.self_at.map(|a| a.0) } else { self.actor_pos(m.id) };
                    let pos = actor.or_else(|| (m.x == self.world_id()).then_some(m.pos));
                    crate::hud::PatchMember { index, id: m.id, pos }
                })
                .collect(),
            self_id: me,
            target: self.target,
            camera: walker.eye.position.to_array(),
            time_ms,
            waypoint: self.map.waypoint(self.player.world as i32),
            minimap: self.minimap_up(),
            tracker: self.tracker_quests(),
            voice: self.voice_badge(),
            voice_marks: self.voice.voice.marks(),
        };
        let now = self.session.now();
        let dt = ((time_ms - self.hud_last).clamp(0, 250)) as f32 / 1000.0;
        self.hud_last = time_ms;
        let mut actions = Vec::new();
        let mut quick_outs = Vec::new();
        if let Some((mut vui_o, assets, mut hud_o)) = self.hud.take() {
            let (vui, hud) = (&mut vui_o, &mut hud_o);
            vui.time = time_ms;
            // VIClient's m_p4FD28.Process: after the scene, before the windows
            let font = assets.surface(vi::hash::hash_resource_id("damage_font.tga"));
            let (slots, puppets, self_at, me, world) = (&self.entities.slots, &self.puppets, self.self_at, self.self_id32(), self.player.world);
            self.damage.frame(font, time_ms, view_proj, |id| {
                let k = slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id))?;
                if Some(id) == me {
                    return self_at.map(|(pos, slot4)| crate::damage::Target { pos, slot4 });
                }
                // an actor (`in_scene`), slot 4 when its body has one
                let e = slots[k].as_ref().filter(|e| e.world as i64 == world)?;
                Some(crate::damage::Target { pos: e.actor_pos, slot4: puppets.get(k).and_then(|p| p.as_ref()).and_then(|p| p.slot4_at) })
            });
            let mut r = assets.raster(ui);
            // A window wider than 4:3 has the HUD at its sides (`Anchor`):
            // the frame, the gauges, the group, the effects and the target's
            // bar at the left (`Hud::draw`); the chat box, the quick menus,
            // the breath gauge and the target's details at the right; the
            // boxes and the menus in the middle, with the frame, and a menu
            // takes back under it what it covers on a 4:3 screen (`end`). A
            // window narrower than 4:3 has them at its top and bottom
            // (`Vertical`): the frame, the gauges, the group, the effects, the
            // quick menus and the breath gauge up with the top; the target's
            // bar, the report's button and the chat box down with the bottom;
            // the target's details, the boxes and the menus in the middle.
            // DrawReticle, before the HUD: ADDART.CSF's AT_Reticle.tga in the
            // middle of the frame (blend 3, no dither), at full on a new
            // target, then 0.05 less a frame of the game's down to half (the
            // damage numbers, placed again every frame drawn, go under it
            // here; the PS2 draws them after it)
            if let Some(flash) = reticle {
                use vi::ui::Raster;
                let id = vi::ui::icon_id("AT_Reticle");
                if r.has(id) {
                    let (a, next) = reticle_step(self.reticle_alpha, flash);
                    r.ui.anchor = Anchor::Middle;
                    r.blit_full(id, 4, [320 - 16, 224 - 16], 3, a.0);
                    self.reticle_alpha = next;
                }
            }
            // UpdateGame, before the HUD: while dead, "You have died." on a
            // bar in the middle, coming up over its first two seconds
            if let Some(d) = self.death.as_ref() {
                r.ui.anchor = Anchor::Middle;
                let a = (d.time() * 0.5).min(1.0);
                let text = crate::ui::wide(vui.words.get(501, vui.language));
                let w = vui.string_width(&r, &text);
                vui.blit_bar(&mut r, &text, [0.8, 0.8, 0.8, a], [320 - ((w + 50) >> 1), 229], 33, 34, w + 50, 3, a, 0, true);
            }
            hud.draw(vui, &mut r, &state);
            // a server's player search (`searchwnd`), drawn where S2 draws
            // it, in `VIWndGame::OnDrawSelf` before the windows it owns (the
            // effects, the chat box); while it is up the player and the
            // camera are held, every frame (its FD13C)
            if self.search.open {
                self.locks = (false, false);
                r.ui.anchor = Anchor::Middle;
                self.search.draw(vui, &mut r);
            }
            r.ui.anchor = Anchor::Left;
            r.ui.vertical = Vertical::Top;
            if crate::influence::shown(self.mode) {
                crate::influence::draw(vui, &mut r, &self.effects);
            }
            r.ui.anchor = Anchor::Right;
            r.ui.vertical = Vertical::Bottom;
            if let Some(c) = self.wnd_chat.as_mut() {
                for (text, color, at) in self.chat_pending.drain(..) {
                    let rr = &r;
                    c.text.add_text(&crate::ui::wide(&text), color, at, |s| vi::ui::Raster::string_width(rr, s));
                }
                c.draw(vui, &mut r, now, dt);
                self.cues.extend(c.cues.drain(..).map(crate::sound::Cue::Ui));
            }
            r.ui.anchor = Anchor::Middle;
            r.ui.vertical = Vertical::Middle;
            if self.mode == 9 {
                self.priv_chat.draw(vui, &mut r);
            }
            if self.mode == 7 {
                self.response.draw(vui, &mut r);
            }
            if self.mode == 14 {
                self.mbox.draw(vui, &mut r, now as i64);
            }
            quick_outs = self.run_quick(vui, &mut r);
            actions = self.run_start_menu(vui, &mut r, &start_loc);
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
            // the voice window over play's windows, under the edit box that
            // types its fields
            r.ui.anchor = Anchor::Middle;
            r.ui.vertical = Vertical::Middle;
            self.draw_voice_window(vui, &mut r);
            self.run_edit(vui, &r);
            if self.mode == 2 {
                let mut cues = Vec::new();
                self.edit.draw(vui, &mut r, now as i64, &mut cues);
                self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
            }
            // the spell book: opened, its keys, drawn; what it asks for
            let now_ms = now as i64;
            let expansion = self.session.expansion;
            let bars = self.ability_bars();
            if std::mem::take(&mut self.book_open) {
                self.book.open(vui, &r, expansion, now_ms);
            }
            let mut book_out = Vec::new();
            for (k, down) in std::mem::take(&mut self.book_keys) {
                if self.mode == 5 {
                    book_out.extend(self.book.key(&self.abilities, bars, k, down, now_ms));
                }
            }
            if self.mode == 5 {
                book_out.extend(self.book.draw(vui, &mut r, &self.abilities, self.dump.player.class as i32, bars, now_ms));
            }
            self.book_actions(book_out);
            self.slot_unanswered();
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
            r.ui.anchor = Anchor::Right;
            r.ui.vertical = Vertical::Top;
            // DrawStatus: under water, the breath gauge
            if under_liquid && (self.mode == 0 || self.mode == 36 || self.mode == 38) {
                let at = [430, 50];
                vui.draw_word_ex(&mut r, 911, at[0], at[1], [1.0; 4], true);
                crate::hud::draw_gauge(vui, &mut r, 9, 136, [at[0], at[1] + 19], true, self.dump.player.p30 as f32 / 255.0);
            }
            // the target's info window, in the middle of a tall window's right
            r.ui.vertical = Vertical::Middle;
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
            r.ui.vertical = Vertical::Top;
            // UpdateQuickBars, then the menu up (the HUD's, in play; L2's
            // menus open over it without `UpdateMode`, so it stays up there;
            // a server's player search hides them while it is up)
            if (self.mode == 0 || self.mode == 30 || self.mode == 38) && !self.search.open && !self.voice_wnd.open {
                let bars = self.hud_bars();
                self.hud_menus.build(&self.abilities, &self.dump.inventory, &self.gear, bars);
                let abilities = &self.abilities;
                let inventory = &self.dump.inventory.items;
                let gear = &self.gear;
                let name = |msg: u32, id: i64| -> Vec<u16> {
                    match msg {
                        0x08060000 => abilities.get(&id).map(|a| crate::ui::wide(&a.name)).unwrap_or_default(),
                        0x02030000 => inventory.get(id as usize).map(|it| crate::ui::wide(&it.item.name)).unwrap_or_default(),
                        crate::gear::MSG => crate::ui::wide(&gear.name(id, &vui.words, vui.language)),
                        _ => Vec::new(),
                    }
                };
                self.hud_menus.draw(vui, &mut r, now as i64, &name);
            }
            // `EQOA_GLYPHS=1`: the button glyphs drawn, whenever they change
            if std::env::var_os("EQOA_GLYPHS").is_some() && r.glyphs.list != self.glyphs.list {
                let list: Vec<String> = r.glyphs.list.iter().map(|g| format!("{} {:?}", crate::input::key_name(g.key), g.rect.map(|v| v as i32))).collect();
                eprintln!("glyphs (mode {}): {}", self.mode, list.join(", "));
            }
            self.glyphs = std::mem::take(&mut r.glyphs);
            // DrawFade, last, as `Process` draws it after every window: black
            // over the whole screen at the fade's alpha (the game's spans the
            // wide edges; here past the 4:3 frame's sides)
            let a = self.fade.step(dt);
            if a > 0.0
                && let Some(tex) = assets.white
            {
                let (x0, y0, x1, y1) = (-2000.0, -1.0, 2640.0, 449.0);
                r.ui.push_whole(render::ui::Quad { tex, blend: 3, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[0.5, 0.5]; 4], color: [0.0, 0.0, 0.0, a], clip: None });
            }
            r.ui.end();
            self.hud = Some((vui_o, assets, hud_o));
        }
        self.start_actions(actions);
        self.quick_outs(quick_outs);
        self.pump_keys();
    }

    /// Where the creature `id` is drawn this frame, which its damage numbers
    /// float from (`damage::place`): the player's walk (`self_at`), another's
    /// actor (`actor_pos`), as `Process`'s `GetActorPos` takes them.
    fn drawn_at(&self, id: u32) -> Option<[f32; 3]> {
        if Some(id) == self.self_id32() {
            return self.self_at.map(|s| s.0);
        }
        self.actor_pos(id)
    }

    /// An entity's actor position, while it has an actor (`in_scene`).
    fn actor_pos(&self, id: u32) -> Option<[f32; 3]> {
        self.entities.slots.iter().flatten().find(|e| e.id == id).filter(|e| self.in_scene(e)).map(|e| e.actor_pos)
    }

    /// Whether the entity has an actor in the scene, as every one in the
    /// scene's dimension has on a PS2 (`VIEntity::OnWorldLoaded`: the
    /// world it is in, the channel's byte, is the one loaded): one with no
    /// model too, whose actor holds the null sprite (`VIClientPart06D00::
    /// Create`), so `CycleTarget` and `TargetCentre` take it, the rings go
    /// round it and its numbers float over it, where no body is drawn (F3
    /// 20261024-203920: Sandstorm's Ice Statue in Geyvax Cave, model 0,
    /// targetable only on a PS2).
    fn in_scene(&self, e: &net::entity::Entity) -> bool {
        e.world as i64 == self.player.world
    }

    /// The slot's character's slot 4 (`CalcSlotWorldTransform(4)`), when its
    /// body is a character's: none for a creature with no body.
    fn slot4_of(&self, slot: usize) -> Option<[f32; 3]> {
        self.puppets.get(slot)?.as_ref().and_then(|p| p.sprite.as_ref().and(p.slot4_at))
    }

    /// NPC/PC Display and Distance against the server's offer
    /// (`net::session::OP_ENTITY_VIEW`): the slots read are the player's
    /// number within the server's most, the game's 24 from a server that
    /// offered none (Sandstorm, eqoa.live), and a server that offered is told
    /// both whenever either changes (it fills that many from that far from
    /// its next pass; what was past a lowered number is dropped here at once,
    /// and its puppets with it). The view distance draws farther on any
    /// server (`lod::view_dist`); only one that offered sends from farther.
    fn agree_entity_view(&mut self) {
        let offer = self.session.entity_view_offer;
        let n = offer.map_or(RETAIL_SLOTS, |(most, _)| crate::settings::entity_slots().min(most as usize)).clamp(RETAIL_SLOTS, MAX_SLOTS);
        let view = crate::settings::entity_view();
        if n != self.entities.limit {
            eprintln!("play: {n} entity slots (NPC/PC Display {}, the server's most {offer:?})", crate::settings::entity_slots());
            self.entities.set_limit(n);
        }
        if offer.is_some() && self.view_sent != Some((n, view)) {
            self.session.send_entity_view(n as u32, view as u32);
            self.view_sent = Some((n, view));
        }
        self.options_wnd.server_note = Some(match offer {
            Some((slots, view)) => format!("This server: {slots} within {view}"),
            None => format!("This server: {RETAIL_SLOTS} (the game's)"),
        });
    }

    /// `VIClient::Process`: the target info goes (`m_targetInfoValid` 0)
    /// while the target is in view with its +0x19 at 0, so a dead one's bar
    /// shows no face beside it (`DrawTarget` draws it only with the info; F3
    /// 20261023-075853) and its rings stop counting; it stays gone until a
    /// 0x0010 sets it again.
    /// The session ends (a logout, a lost connection): every creature's
    /// puppet goes, its loop stopped and its sounds let go (`let_go`), as
    /// the scene releases its sprites. Dropped without it, the device kept
    /// their waves and played their loops on into the game's screens.
    pub fn let_go_sounds(&mut self, s: &mut crate::sound::Sound) {
        for p in &mut self.puppets {
            let_go(s, p);
            *p = None;
        }
    }

    fn drop_dead_target_info(&mut self) {
        if self.target != u32::MAX && self.entities.slots.iter().flatten().any(|e| e.id == self.target && e.p19 == 0) {
            self.target_info = None;
        }
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

    /// Every emitter a creature carries this frame, the creatures' as `frame`
    /// posed them, the player's own on its walker's pose (`matrix`, the
    /// centre transform `place_self` takes): each particle sprite in its hands
    /// (each held item's `particles::item_emitters`) under its hand's node,
    /// updated only while its item is drawn (`VICSprite::Raster` rasters what
    /// a drawn sprite holds; an invisible creature's items are still drawn);
    /// and its sprite's own emitters (`particles::part_emitters`) under their
    /// nodes, run while the creature is rastered (`UpdateParticleEmitters`,
    /// from `VICSprite::Raster`), a player-only one on the player alone
    /// (`SetIsPlayer`).
    pub fn in_hand(&self, lib: &Library, walker: &Walker, matrix: [[f32; 4]; 4]) -> Vec<crate::particles::InHand> {
        use crate::particles::{How, InHand, OWN};
        let mut out = Vec::new();
        let mut add = |holder: i32, p: &Puppet, cs: &CSprite<Ee>, centre: &Matrix44<Ee>, shown: Option<bool>| {
            for (hand, h) in p.held.iter().enumerate() {
                let Some((item, node)) = *h else { continue };
                let Some(n) = cs.h.nodes.get(node as usize) else { continue };
                let drawn = shown.unwrap_or_else(|| p.attached.iter().any(|a| a.0 == node && a.1.visible));
                let at = Matrix44::mul(&n.world, centre);
                for (k, e) in lib.with_view(p.era, || crate::particles::item_emitters(lib, item)).into_iter().enumerate() {
                    out.push(InHand { key: (holder, hand, item, k), world: Matrix44::mul(&e.member, &at), def: e.def, how: How::Sprite(e.flags), drawn });
                }
            }
            let player = holder == crate::spells::SELF;
            for (k, e) in lib.with_view(p.era, || crate::particles::part_emitters(lib, p.model)).into_iter().enumerate() {
                let Some(n) = cs.h.nodes.get(e.node.max(0) as usize).filter(|_| e.node >= 0) else { continue };
                if e.player_only && !player {
                    continue;
                }
                let how = How::Part { use_location: e.use_location, once: e.once };
                out.push(InHand { key: (holder, OWN, p.model, k), world: Matrix44::mul(&n.world, centre), def: e.def, how, drawn: shown.unwrap_or(p.shown) });
            }
        };
        for (slot, p) in self.puppets.iter().enumerate() {
            if let Some(p) = p
                && let Some(cs) = &p.sprite
            {
                add(slot as i32, p, cs, &p.centre, None);
            }
        }
        if let (Some(p), Some(cs)) = (&self.self_puppet, walker.sprite.as_ref()) {
            add(crate::spells::SELF, p, cs, &glam_to_matrix(matrix), Some(self.self_shown));
        }
        out
    }

    pub fn has_self(&self) -> bool {
        self.self_puppet.is_some()
    }

    /// Places the dressed player with the walker's pose: the body at its
    /// model matrix and palette (its LOD level and off distance as
    /// `place_body` takes them), the attachments on the walker's nodes.
    pub fn place_self(&mut self, walker: &Walker, matrix: [[f32; 4]; 4], palette: &[[[f32; 4]; 4]], r: &Renderer, queue: &wgpu::Queue) {
        let me = self.self_id32();
        let skins = skins_shown(self.entities.slots.iter().flatten().find(|e| Some(e.id) == me).and_then(|e| e.creature.as_ref()));
        let Some(p) = &mut self.self_puppet else { return };
        r.set_instances(queue, &p.gpu, &[matrix]);
        if !palette.is_empty() {
            r.set_palette(queue, &p.gpu, palette);
        }
        let shown = place_body(&mut p.gpu, walker, matrix);
        p.gpu.visible &= skins;
        for (_, a) in &mut p.attached {
            a.visible = shown;
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
        let me = self.self_puppet.as_ref().filter(|_| self.self_shown);
        let flying = self.projectiles.values().flat_map(|(_, pool)| pool);
        self.puppets.iter().flatten().chain(me).flat_map(|p| std::iter::once(&p.gpu).chain(p.attached.iter().map(|a| &a.1))).chain(flying)
    }

    pub fn describe(&self) -> String {
        if std::env::var_os("EQOA_INV").is_some() {
            for (k, it) in self.dump.inventory.items.iter().enumerate() {
                eprintln!("inv {k}: {} wear {} equip {} quick {} p4c {} p54 {} qty {} icon {:#x}/{:#x}", it.item.name, it.item.wear_slot, it.equip_slot, it.quick_slot, it.item.p4c, it.item.p54, it.quantity, it.item.icon, it.item.icon2);
            }
        }
        let n = self.entities.slots.iter().flatten().count();
        // the characters within their distance this frame (NPC/PC Distance's)
        let drawn = self.puppets.iter().flatten().filter(|p| p.shown).count();
        let on_bar = self.abilities.values().filter(|a| a.valid != 0).count();
        let fx = self.spells.as_ref().map_or_else(String::new, |s| format!(", {}", s.describe()));
        format!("{n} entities ({drawn} drawn), {} items, {} abilities ({on_bar} on a bar), {} packets sent{fx}, {}", self.dump.inventory.items.len(), self.abilities.len(), self.session.packets_sent, self.rings.describe())
    }
}

#[cfg(test)]
mod commands {
    use super::*;
    use crate::controls::Follow;

    /// Play on a session nothing answers, the player 0x100, no words (a
    /// word is said as "(word n)").
    fn play() -> Play {
        play_with(Box::default())
    }

    /// `play` entered on `dump`.
    fn play_with(dump: Box<net::dump::MemoryDump>) -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        Play::new(s, Player::default(), dump)
    }

    /// The quest guide speaks only where the server does not (the user's
    /// "so sandstorm can benefit without server additions"; no mixing): its
    /// marks once the wait into the world is over on a server that offered
    /// none; the server's alone, the guide off for good, once it offers its
    /// own, even after a world change takes the offer back; a scripted run's
    /// `EQOA_QUEST_GUIDE` the guide's whatever the server offers.
    #[test]
    fn the_quest_guide_speaks_only_where_the_server_does_not() {
        crate::settings::testing::reset(false);
        let mark = |npc: &str| net::session::QuestMark { kind: 1, world: 0, x: 0.0, z: 0.0, npc: npc.into(), quests: Vec::new() };
        let npcs = |m: Option<&[net::session::QuestMark]>| m.map(|m| m.iter().map(|m| m.npc.clone()).collect::<Vec<_>>());
        let mut p = play();
        p.guide.estimate.marks = vec![mark("Commander Nothard (guide)")];
        p.guide_wait(500);
        assert_eq!((p.guide_from, npcs(p.quest_marks(500))), (None, None), "under the loading screen: no wait yet, no Quests button");
        p.loading.wait = None;
        p.guide_wait(1000);
        assert_eq!(p.guide_from, Some(1000 + GUIDE_WAIT_MS));
        assert!(!p.guide_on(1000 + GUIDE_WAIT_MS - 1), "the wait");
        assert_eq!(npcs(p.quest_marks(1000 + GUIDE_WAIT_MS)), Some(vec!["Commander Nothard (guide)".to_string()]), "Sandstorm: the guide's");
        // a server's own: its alone, for good
        p.session.quest_marks_offer = true;
        p.session.quest_marks = vec![mark("Commander Nothard")];
        p.guide_wait(9000);
        assert!(!p.guide_on(9000));
        assert_eq!(npcs(p.quest_marks(9000)), Some(vec!["Commander Nothard".to_string()]), "the server's, none of the guide's");
        p.session.quest_marks_offer = false;
        p.guide_wait(9100);
        assert_eq!(npcs(p.quest_marks(9100)), None, "a world change: still not the guide's");
        // Quest Markers off: none of either
        let mut c = crate::settings::client();
        c.quest_markers = false;
        crate::settings::set_client(c);
        let mut q = play();
        q.guide.estimate.marks = vec![mark("Commander Nothard (guide)")];
        q.loading.wait = None;
        q.guide_wait(0);
        assert_eq!(npcs(q.quest_marks(GUIDE_WAIT_MS)), Some(Vec::new()), "off: the button, no marks");
        // a scripted run's: the guide's, the server's offer ignored
        let mut r = play();
        r.guide_forced = true;
        r.session.quest_marks_offer = true;
        r.loading.wait = None;
        r.guide_wait(0);
        assert!(r.guide_on(GUIDE_WAIT_MS));
    }

    /// A click on the map while it is up, on no button of its, puts the
    /// waypoint under it (`Map::click`) and is the map's; a right click is
    /// △ (it closes the map) and puts none. The control: the map not up,
    /// the same click puts none.
    #[test]
    fn a_click_on_the_map_puts_the_waypoint() {
        use crate::mouse::Click;
        crate::settings::testing::reset(false);
        let size = (1280, 720);
        let mut p = play();
        p.point(Click::Target(900.0, 300.0), size, false);
        assert_eq!(p.map.waypoint(0), None, "the control: the map not up");
        p.map = crate::map::testing::up(size);
        assert!(p.point(Click::Target(900.0, 300.0), size, false), "the map's");
        let at = p.map.waypoint(0).expect("put");
        assert!(p.point(Click::Right(200.0, 200.0), size, false));
        assert_eq!(p.map.waypoint(0), Some(at), "a right click puts none");
        assert!(!p.map.open, "a right click is △: the map closed");
        p.map.show();
        assert!(p.point(Click::Target(400.0, 500.0), size, true), "a finger's tap");
        assert_ne!(p.map.waypoint(0), Some(at), "moved by the tap");
    }

    fn last_line(p: &Play) -> &str {
        p.chat.back().map_or("", |l| l.0.as_str())
    }

    /// `UpdateMode`'s holds (F3 20261025-045748: "When the NPC dialogue
    /// window is opened, the player should stop running and not be able to
    /// move until they clear out the dialogues"): an NPC's words (0x0046's
    /// `ShowDialog`, mode 9) hold the walk and the camera until they are
    /// answered, as every window does but the chat line, a tell or a reply,
    /// the quick chat and the chat box's history; fishing holds the walk
    /// only. The controls: play lets both go, and the client's own holds
    /// (`locks`) still count there.
    #[test]
    fn a_window_holds_the_walk_and_the_camera_as_update_mode_does() {
        let held = |p: &Play| {
            let c = p.control_client();
            (c.move_ok, c.look_ok)
        };
        let mut p = play();
        assert_eq!(held(&p), (true, true), "in play");
        p.show_dialog(&"Hail, traveller".encode_utf16().collect::<Vec<u16>>());
        assert_eq!((p.mode, held(&p)), (9, (false, false)), "held while the NPC's words are up");
        p.close_dialog();
        assert_eq!((p.mode, held(&p)), (0, (true, true)), "answered: let go");
        let modes = [(7, (false, false)), (14, (false, false)), (1, (false, false)), (10, (false, false)), (11, (false, false)), (5, (false, false)), (38, (false, false)), (2, (true, true)), (28, (true, true)), (29, (true, true)), (30, (true, true)), (33, (true, true)), (36, (false, true))];
        for (mode, want) in modes {
            p.mode = mode;
            assert_eq!(held(&p), want, "mode {mode}");
        }
        p.mode = 0;
        p.locks = (false, true);
        assert_eq!(held(&p), (false, true), "the client's own hold");
    }

    /// A far 0x00B2 move in the same world (Freeport's open ground to the
    /// Highpass road, 3,600 away, 2 under the ground there): its zones are
    /// not in when it lands, so its floor cast meets nothing and the server's
    /// height stands (the control: where the client left it, under the
    /// ground, to fall through it); once they are in and the screen comes
    /// down the floor is cast again and it stands on the ground, as the game
    /// casts after `EnterWorld` has streamed them. A dump's arrival (`Msg000D`'s
    /// `SetLocation`) stays as given.
    #[test]
    fn a_far_teleport_lands_on_the_floor_once_its_zones_are_in() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let (a, b) = ((25275.115f32, 15766.627f32), (23533.45f32, 18890.77f32));
        let mut lib = Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
        crate::stream::fill(&mut lib, w, a.0, a.1, &mut src);
        let at_a = || Walker::arrive(vi::world::World::load(&lib, w).unwrap(), Spawn::ground(a.0, a.1, 0.0));
        let mut walker = at_a();
        let mut also = at_a();
        crate::stream::fill(&mut lib, w, b.0, b.1, &mut src);
        let with_b = || vi::world::World::load(&lib, w).unwrap();
        let floor = crate::walker::ground(&mut with_b(), b.0, b.1).unwrap();
        let pos = [b.0, floor - 2.0, b.1];
        let mut p = play();
        // the world entry's own screen down first, as in play
        p.loading_step(&mut walker, true);
        assert!(!p.loading());
        // made FAR_HOLD_MS ago: the far move's hold is past
        let ago = p.session.now() as i64 - crate::loading::FAR_HOLD_MS;
        p.teleport(&mut walker, 0, pos, [0.5, 0.0, 0.0], ago);
        assert!(p.loading());
        assert_eq!(walker.cam.walk.phys.position.y.0, floor - 2.0, "the control: no floor met, the server's height");
        // the zones round the arrival come in (the streamer's), and the screen comes down
        walker.scene = with_b();
        p.loading_step(&mut walker, true);
        assert!(!p.loading());
        let y = walker.cam.walk.phys.position.y.0;
        assert!((y - floor).abs() < 0.3, "on the ground at {floor}: {y}");
        // made now, the far move's hold keeps the screen up a second though
        // its zones are in (F3 20261025-171303); a short hop's does not
        let mut held = play();
        let mut w2 = Walker::arrive(with_b(), Spawn::ground(a.0, a.1, 0.0));
        held.loading_step(&mut w2, true);
        let now = held.session.now() as i64;
        held.teleport(&mut w2, 0, pos, [0.5, 0.0, 0.0], now);
        held.loading_step(&mut w2, true);
        assert!(held.loading(), "a far port: held");
        let mut hop = play();
        let mut w3 = Walker::arrive(with_b(), Spawn::ground(b.0, b.1, 0.0));
        let near = [b.0 + 50.0, floor, b.1];
        hop.loading_step(&mut w3, true);
        let now = hop.session.now() as i64;
        hop.teleport(&mut w3, 0, near, [0.5, 0.0, 0.0], now);
        hop.loading_step(&mut w3, true);
        assert!(!hop.loading(), "the control: a 50-unit hop comes down at once");
        // Instant (Extended > UI > Loading Screens): the far port's screen
        // comes down as its zones are in
        let mut c = crate::settings::client();
        c.loading_longer = false;
        crate::settings::set_client(c);
        let mut quick = play();
        let mut w4 = Walker::arrive(with_b(), Spawn::ground(a.0, a.1, 0.0));
        quick.loading_step(&mut w4, true);
        let now = quick.session.now() as i64;
        quick.teleport(&mut w4, 0, pos, [0.5, 0.0, 0.0], now);
        quick.loading_step(&mut w4, true);
        assert!(!quick.loading(), "Instant: a far port's screen down at once");
        c.loading_longer = true;
        crate::settings::set_client(c);
        // a dump's arrival there: as given
        let mut d = play();
        d.loading.begin(pos, 0.5, false, false, 0);
        also.place(pos, 0.5);
        also.scene = with_b();
        d.loading_step(&mut also, true);
        assert_eq!(also.cam.walk.phys.position.y.0, floor - 2.0, "a dump's SetLocation as it is");
    }

    /// `Msg00B2` answers its token once `Respawn` is done, and no 0x40 goes
    /// out until then: within a world (a far port) and across worlds
    /// (Tunaria's Freeport to Odus), the answer is handed on with the first
    /// 0x40 from the arrival and nothing between them, so the two leave in
    /// one packet, as Sandstorm's captures show a PS2 sending them. Sent at
    /// once on the 0x00B2, with the old world's place sent as the new
    /// world's meanwhile, is what Sandstorm kicked with 0x07D2. Answered at
    /// once: a world the table lacks (no move), and no self. Controls: the
    /// 0x40 goes out every update before the move and after it, so the
    /// silence between is the move's; the 0x40 at the arrival is the walk's
    /// own state there.
    #[test]
    fn a_move_is_answered_with_its_first_position_from_the_arrival() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        crate::s2::with_rules(crate::s2::Rules::retail(), || {
            let mut p = play();
            p.session.record = Some(Vec::new());
            let sent = |p: &mut Play| std::mem::take(p.session.record.as_mut().unwrap());
            let answer = |token: i64| {
                let mut w = net::stream::Writer::op(0x0049);
                w.signed(token);
                (drdp::TYPE_GUARANTEED, w.data)
            };
            let positions = |d: &[(u8, Vec<u8>)]| d.iter().filter(|m| m.0 == 0x40).count();
            let answers = |d: &[(u8, Vec<u8>)]| d.iter().filter(|m| m.0 == drdp::TYPE_GUARANTEED && m.1.starts_with(&[0x49, 0x00])).cloned().collect::<Vec<_>>();

            let (a, b) = ((25275.115f32, 15766.627f32), (23533.45f32, 18890.77f32));
            let mut lib = Library::new();
            let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
            let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
            crate::stream::fill(&mut lib, w, a.0, a.1, &mut src);
            crate::stream::fill(&mut lib, w, b.0, b.1, &mut src);
            let tunaria = || vi::world::World::load(&lib, w).unwrap();
            let mut walker = Walker::arrive(tunaria(), Spawn::ground(a.0, a.1, 0.0));
            let mut now = 1000u32;
            let mut tick = |p: &mut Play, walker: &Walker, n: u32| {
                for _ in 0..n {
                    now += UPDATE_RATE;
                    p.position_due(&walker.cam.walk.committed, now);
                }
            };

            // the control: in play, a 0x40 every update
            tick(&mut p, &walker, 3);
            assert_eq!(sent(&mut p).iter().map(|m| m.0).collect::<Vec<_>>(), [0x40; 3], "the control: a position every update in play");

            // a far port within the world: nothing until the screen comes down
            let floor = crate::walker::ground(&mut tunaria(), b.0, b.1).unwrap();
            // made FAR_HOLD_MS ago (the far move's hold past)
            let ago = |p: &Play| p.session.now() as i64 - crate::loading::FAR_HOLD_MS;
            p.msg_00b2(&mut walker, 0, [b.0, floor + 1.0, b.1], [0.5, 0.0, 0.0], 7, ago(&p));
            assert!(p.loading());
            tick(&mut p, &walker, 5);
            assert_eq!(sent(&mut p), [], "no answer and no 0x40 before the arrival");
            p.loading_step(&mut walker, true);
            assert!(!p.loading());
            let at = vi::selfstate::encode(0, &walker.cam.walk.committed, 0, p.target);
            assert_eq!(sent(&mut p), [answer(7), (0x40, at)], "the answer, then the 0x40 from the arrival");
            let pos = walker.cam.walk.committed.position;
            assert!((pos.x.0 - b.0).abs() < 0.01 && (pos.z.0 - b.1).abs() < 0.01 && (pos.y.0 - floor).abs() < 0.3, "{pos:?} on the floor at {floor}");
            // then in play again
            tick(&mut p, &walker, 2);
            assert_eq!(positions(&sent(&mut p)), 2, "positions again after the arrival");

            // into Odus (world 2): the old world's walk goes on meanwhile,
            // and none of it is sent; the answer and the 0x40 come from the
            // new world's walk at the arrival
            let odus = [7338.18f32, 54.21, 11124.25];
            p.msg_00b2(&mut walker, 2, odus, [1.0, 0.0, 0.0], 0x1234, ago(&p));
            assert!(p.world_pending());
            let (file, spawn) = p.take_world_change().expect("a world change");
            assert_eq!(file, "ODUS.ESF");
            tick(&mut p, &walker, 5);
            assert_eq!(sent(&mut p), [], "nothing while Odus is read");
            // the viewer's world entry: a walk begun there (Tunaria's
            // geometry stands in for Odus's: the messages are the test)
            let mut there = Walker::arrive(tunaria(), spawn);
            p.world_entered(&mut there);
            tick(&mut p, &there, 2);
            assert_eq!(sent(&mut p), [], "nothing until the zones round the arrival are in");
            p.loading_step(&mut there, true);
            let at = vi::selfstate::encode(2, &there.cam.walk.committed, 0, p.target);
            assert_eq!(sent(&mut p), [answer(0x1234), (0x40, at)], "world 2, from the arrival");
            let pos = there.cam.walk.committed.position;
            assert!((pos.x.0 - odus[0]).abs() < 0.01 && (pos.z.0 - odus[2]).abs() < 0.01);

            // two moves before the first is done: both answered, in order
            p.msg_00b2(&mut there, 2, [odus[0] + 50.0, odus[1], odus[2]], [0.0; 3], 1, 0);
            p.msg_00b2(&mut there, 2, [odus[0] + 90.0, odus[1], odus[2]], [0.0; 3], 2, 0);
            tick(&mut p, &there, 2);
            assert_eq!(sent(&mut p), []);
            p.loading_step(&mut there, true);
            let d = sent(&mut p);
            assert_eq!(answers(&d), [answer(1), answer(2)]);
            assert_eq!(d.last().map(|m| m.0), Some(0x40));
            assert!((there.cam.walk.committed.position.x.0 - (odus[0] + 90.0)).abs() < 0.01, "at the second's arrival");

            // a world the table lacks: no move, answered at once, play goes on
            p.msg_00b2(&mut there, 9, odus, [0.0; 3], 3, 0);
            assert!(!p.loading());
            assert_eq!(sent(&mut p), [answer(3)]);
            tick(&mut p, &there, 1);
            assert_eq!(positions(&sent(&mut p)), 1);

            // no self (`m_selfId` -1): nothing moves, answered at once
            p.session.self_id = None;
            let before = there.cam.walk.committed.position;
            p.msg_00b2(&mut there, 2, [odus[0] + 300.0, odus[1], odus[2]], [0.0; 3], 4, 0);
            assert!(!p.loading());
            assert_eq!(there.cam.walk.committed.position, before);
            assert_eq!(sent(&mut p), [answer(4)]);
        });
    }

    /// A creature with no body (model 0: Sandstorm's Ice Statue in Geyvax
    /// Cave, 0x101076) is targeted as on a PS2, where its actor holds the
    /// null sprite and `CycleTarget` asks only for an actor: R1 picks it, the
    /// mouse finds it (an upright two high where its body would stand), and
    /// its place is its actor's for the rings and the numbers, with no slot
    /// 4. The control: one in another world has no actor here and is left
    /// out (the statue alone is cycled to); the old rule, a body drawn, left
    /// the statue out too (R1 found nothing).
    #[test]
    fn a_creature_with_no_body_is_targeted() {
        let mut p = play();
        let creature = |id: u32, world: i8, at: [f32; 3], name: &str| net::entity::Entity {
            id,
            kind: 130,
            world,
            p18: 1,
            p19: 1,
            pos: at,
            actor_pos: at,
            creature: Some(net::entity::Creature { name: name.into(), ..Default::default() }),
            ..Default::default()
        };
        p.entities.slots[0] = Some(creature(0x0020_0001, 2, [1.0, 0.0, -10.0], "Elsewhere"));
        p.entities.slots[1] = Some(creature(0x0010_1076, 0, [0.0, 0.0, -10.0], "Ice Statue"));
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0);
        let vp = proj * glam::camera::rh::view::look_to_mat4(glam::Vec3::ZERO, glam::Vec3::NEG_Z, glam::Vec3::Y);
        p.cycle_target(false, glam::Vec3::ZERO, glam::Vec3::NEG_Z, vp);
        assert_eq!(p.target, 0x0010_1076, "R1: the statue");
        p.cycle_target(false, glam::Vec3::ZERO, glam::Vec3::NEG_Z, vp);
        assert_eq!(p.target, 0x0010_1076, "R1 again: still the only one with an actor");
        // a click a unit up its upright
        let c = vp * glam::Vec4::new(0.0, 1.0, -10.0, 1.0);
        let at = (((c.x / c.w + 1.0) * 0.5 * 640.0) as f64, ((1.0 - c.y / c.w) * 0.5 * 480.0) as f64);
        assert_eq!(p.creature_at(at, (640, 480), vp), Some(0x0010_1076));
        assert_eq!(p.actor_pos(0x0010_1076), Some([0.0, 0.0, -10.0]));
        assert_eq!(p.slot4_of(1), None, "no body, no slot 4");
        assert_eq!(p.actor_pos(0x0020_0001), None, "the control: another world's has no actor here");
    }

    fn grouped(p: &mut Play, ids: &[u32]) {
        p.group.in_group = true;
        p.group.members = ids.iter().map(|&id| crate::group::Member { id, ..Default::default() }).collect();
    }

    /// 0x0036 (case 0x0036, `QuestStage`): the training points as the server
    /// now has them, a refund's as a level's. A change sets them, says word
    /// The player (0x100) in, drawn at (10, 0, 10), and a walk camera on the
    /// test room there facing 0.5, in camera mode `mode`.
    fn aim_rig(mode: i32) -> (Play, WalkCamera<Ee>, crate::camreach::testing::Room) {
        let mut p = play();
        p.entities.slots[0] = Some(net::entity::Entity { id: 0x100, kind: 130, ..Default::default() });
        p.self_at = Some(([10.0, 0.0, 10.0], None));
        let mut room = crate::camreach::testing::Room { t: 0, wall: f32::INFINITY };
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(10.0, 0.0, 10.0), Vect3::of(0.5, 0.0, 0.0), &mut room);
        cam.set_camera_mode(mode);
        (p, cam, room)
    }

    /// `VIWndGame::OnDrawSelf`'s hold: L1 or R1 pressed in play and still
    /// held more than 300 ms on (a frame of the game's) begins the manual
    /// camera (`BeginManualCamera`): mode 38, movement and the camera locked,
    /// the eye at the player's (the race camera's 1.68 without CLIENT's
    /// table), looking the walk's way in first person and the fixed follow,
    /// at once from first person and over 0.2 s otherwise; the orbit camera
    /// looks along the view, read back off its basis (here straight down
    /// +z, heading 0, and a view 30 degrees down: pitch 30). The controls:
    /// 200 ms is not yet, a frame not the game's waits, a window up gives the
    /// hold up, the free orbit (mode 1) and no player begin nothing.
    #[test]
    fn a_hold_of_300_ms_begins_the_manual_camera() {
        let along_z = Vect3::of(0.0, 0.0, 1.0);
        for mode in [0, 2] {
            let (mut p, mut cam, _) = aim_rig(mode);
            p.aim_hold = true;
            p.aim_at = p.session.now().wrapping_sub(200);
            p.manual_frame_on(&mut cam, along_z, true);
            assert_eq!((p.mode, p.aim_hold), (0, true), "mode {mode}: 200 ms, not yet");
            p.aim_at = p.session.now().wrapping_sub(301);
            p.manual_frame_on(&mut cam, along_z, false);
            assert_eq!(p.mode, 0, "mode {mode}: not a frame of the game's");
            p.manual_frame_on(&mut cam, along_z, true);
            assert_eq!((p.mode, p.locks, p.aim_hold), (38, (false, false), false), "mode {mode}");
            let e = cam.effect;
            assert_eq!(e.manual_xyz.to_array(), [10.0, 1.68, 10.0]);
            assert_eq!(e.manual_hpr.x.0, 0.5, "mode {mode}: the walk's heading");
            let (terp, rate) = if mode == 0 { (1.0, 0.0) } else { (0.0, (F::<Ee>::ONE / F::new(0.2)).0) };
            assert_eq!((e.manual_terp.0, e.manual_terp_rate.0), (terp, rate), "mode {mode}");
        }
        // the orbit camera: the view's way, its pitch read off the basis
        let (mut p, mut cam, _) = aim_rig(3);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(301);
        let down = Vect3::of(0.0, -(30f32.to_radians().sin()), 30f32.to_radians().cos());
        p.manual_frame_on(&mut cam, down, true);
        assert_eq!(p.mode, 38);
        let h = cam.effect.manual_hpr;
        assert!(h.x.0.abs() < 1e-6 && (h.y.0 - 30f32.to_radians()).abs() < 1e-5, "{h:?}");
        // the pitch held short of straight down
        let (mut p, mut cam, _) = aim_rig(3);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(301);
        p.manual_frame_on(&mut cam, Vect3::of(0.0, -1.0, 1e-4), true);
        assert_eq!(cam.effect.manual_hpr.y.0, 1.5607964);
        // a window up: the hold given up
        let (mut p, mut cam, _) = aim_rig(0);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(400);
        p.mode = 12;
        p.manual_frame_on(&mut cam, along_z, true);
        assert_eq!((p.mode, p.aim_hold), (12, false));
        // the free orbit, and no player: nothing
        let (mut p, mut cam, _) = aim_rig(1);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(400);
        p.manual_frame_on(&mut cam, along_z, true);
        assert_eq!((p.mode, p.aim_hold, cam.effect.manual_terp.0), (0, false, 0.0));
        let (mut p, mut cam, _) = aim_rig(0);
        p.entities.slots[0] = None;
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(400);
        p.manual_frame_on(&mut cam, along_z, true);
        assert_eq!(p.mode, 0);
    }

    /// `ResetView`, L1 or R1 let go: out of the orbit camera the walk faces
    /// the look's heading and the follow camera is put behind it, blended out
    /// over 0.2 s (first person at once); the orbit camera is put behind the
    /// look and its body keeps its heading; play again with movement and the
    /// camera free. Let go in play, the hold is given up. The control:
    /// without the player nothing changes.
    #[test]
    fn letting_go_ends_the_manual_camera_facing_the_look() {
        let along_z = Vect3::of(0.0, 0.0, 1.0);
        let begun = |mode: i32| {
            let (mut p, mut cam, room) = aim_rig(mode);
            p.aim_hold = true;
            p.aim_at = p.session.now().wrapping_sub(301);
            p.manual_frame_on(&mut cam, along_z, true);
            assert_eq!(p.mode, 38, "mode {mode}");
            cam.effect.manual_terp = F::ONE;
            cam.effect.manual_terp_rate = F::ZERO;
            cam.effect.manual_hpr.x = F::new(1.25);
            (p, cam, room)
        };
        for mode in [0, 2, 3] {
            let (mut p, mut cam, mut room) = begun(mode);
            p.target_key_up_on(&mut cam, &mut room);
            assert_eq!((p.mode, p.locks, p.manual_on), (0, (true, true), false), "mode {mode}");
            let heading = cam.walk.committed.angles.x.0;
            assert_eq!(heading, if mode == 3 { 0.5 } else { 1.25 }, "mode {mode}: the body's heading");
            let rate = cam.effect.manual_terp_rate.0;
            if mode == 0 {
                assert_eq!((cam.effect.manual_terp.0, rate), (0.0, 0.0), "first person: at once");
            } else {
                assert_eq!((cam.effect.manual_terp.0, rate), (1.0, (F::<Ee>::new(-1.0) / F::new(0.2)).0), "mode {mode}: blended out");
                // the camera behind: the body's heading in 2, the look's in 3
                let behind = (F::<Ee>::new(if mode == 3 { 1.25 } else { heading }) + F::new(std::f32::consts::PI)).fmodf(F::new(std::f32::consts::TAU));
                assert!((cam.cur.orbit.x.0 - behind.0).abs() < 1e-5, "mode {mode}: orbit {} behind {}", cam.cur.orbit.x.0, behind.0);
            }
        }
        // let go in play: the hold given up
        let (mut p, mut cam, mut room) = aim_rig(0);
        p.aim_hold = true;
        p.target_key_up_on(&mut cam, &mut room);
        assert_eq!((p.mode, p.aim_hold), (0, false));
        // the control: no player
        let (mut p, mut cam, mut room) = begun(0);
        p.entities.slots[0] = None;
        p.target_key_up_on(&mut cam, &mut room);
        assert_eq!((p.mode, cam.walk.committed.angles.x.0), (38, 0.5));
    }

    /// The manual camera a frame drawn: the eye follows the player's place
    /// (`SetManualXyz`); a window's `SetMode` taking mode 38 from it ends it
    /// at once (the client's), but a blend out after `ResetView` goes on
    /// (the control); a camera built again under it leaves mode 38.
    #[test]
    fn the_manual_eye_follows_the_player_and_is_never_left_behind() {
        let along_z = Vect3::of(0.0, 0.0, 1.0);
        let (mut p, mut cam, mut room) = aim_rig(2);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(301);
        p.manual_frame_on(&mut cam, along_z, true);
        p.self_at = Some(([12.0, 1.0, 9.0], None));
        p.manual_frame_on(&mut cam, along_z, false);
        assert_eq!(cam.effect.manual_xyz.to_array(), [12.0, (F::<Ee>::new(1.0) + F::new(1.68)).0, 9.0]);
        // a window's SetMode
        cam.effect.manual_terp = F::ONE;
        cam.effect.manual_terp_rate = F::ZERO;
        p.mode = 0;
        p.manual_frame_on(&mut cam, along_z, false);
        assert_eq!((cam.effect.manual_terp.0, p.manual_on), (0.0, false), "ended at once");
        // the control: ResetView's blend out goes on
        let (mut p, mut cam, _) = aim_rig(2);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(301);
        p.manual_frame_on(&mut cam, along_z, true);
        cam.effect.manual_terp = F::ONE;
        p.reset_view_on(&mut cam, &mut room);
        p.manual_frame_on(&mut cam, along_z, false);
        assert_eq!((cam.effect.manual_terp.0, cam.effect.manual_terp_rate.0), (1.0, (F::<Ee>::new(-1.0) / F::new(0.2)).0), "still blending out");
        // a camera built again (a world change): mode 38 left
        let (mut p, mut cam, _) = aim_rig(2);
        p.aim_hold = true;
        p.aim_at = p.session.now().wrapping_sub(301);
        p.manual_frame_on(&mut cam, along_z, true);
        cam.effect = Default::default();
        p.manual_frame_on(&mut cam, along_z, false);
        assert_eq!((p.mode, p.locks), (0, (true, true)));
    }

    /// `TargetCentre`'s choice, from a camera at the origin looking along +z
    /// (the game's 55 degrees and 10/7; a 16:9 window drawn Hor+): the
    /// selectable creature in view nearest the picture's middle, measured in
    /// the game's picture: B, 0.3 up at 10, beats A, 0.5 to the side (in the
    /// 4:3 picture's units, 0.058 against 0.067); the player, the pet, one of
    /// a player's side, one not selectable and one behind the camera never;
    /// one at the wide window's edge only when nothing else is in view, and
    /// none at all is none. The controls: measured through the 16:9 view as
    /// drawn, A would be nearer. Nothing 200 or more from the camera, Tab's
    /// reach, though nearer the middle (just inside it is the control).
    #[test]
    fn the_target_is_the_creature_nearest_the_middle() {
        let view = glam::camera::rh::view::look_to_mat4(glam::Vec3::ZERO, glam::Vec3::Z, glam::Vec3::Y);
        let game = crate::walker::projection_to(4.0 / 3.0, 6000.0) * view;
        let drawn = crate::walker::projection_to(16.0 / 9.0, 6000.0) * view;
        let seen = |id: u32, at: [f32; 3]| Seen { id, at, selectable: true, side: false, d2: glam::Vec3::from(at).length_squared() };
        let a = seen(1, [0.5, 0.0, 10.0]);
        let b = seen(2, [0.0, 0.3, 10.0]);
        let others = [
            seen(0x100, [0.0, 0.0, 10.0]),
            seen(7, [0.0, 0.0, 10.0]),
            Seen { side: true, ..seen(3, [0.0, 0.0, 10.0]) },
            Seen { selectable: false, ..seen(4, [0.0, 0.0, 10.0]) },
            seen(5, [0.0, 0.0, -10.0]),
        ];
        let mut all = vec![a, b];
        all.extend(others);
        assert_eq!(centre_choice(&all, Some(0x100), 7, game, drawn), Some(2));
        // the wide window's edge: past the 4:3 picture's side, in the 16:9 one
        let edge = seen(6, [8.5, 0.0, 10.0]);
        assert_eq!(centre_choice(&[edge, a], Some(0x100), 7, game, drawn), Some(1));
        assert_eq!(centre_choice(&[edge], Some(0x100), 7, game, drawn), Some(6));
        assert_eq!(centre_choice(&others, Some(0x100), 7, game, drawn), None);
        // the control: measured as drawn
        assert_eq!(centre_choice(&[a, b], Some(0x100), 7, drawn, drawn), Some(1));
        // Tab's reach: dead ahead at 250 is passed over for one off to the
        // side at 20; just inside 200 it is taken; alone at 250, nothing
        let far = seen(8, [0.0, 0.0, 250.0]);
        let side = seen(9, [3.0, 0.0, 20.0]);
        assert_eq!(centre_choice(&[far, side], Some(0x100), 7, game, drawn), Some(9));
        assert_eq!(centre_choice(&[seen(8, [0.0, 0.0, 199.0]), side], Some(0x100), 7, game, drawn), Some(8));
        assert_eq!(centre_choice(&[far], Some(0x100), 7, game, drawn), None);
    }

    /// The choice made the target: a change stops the attack and drops the
    /// target's info (and asks the server, 0x0010), none too; the same
    /// again changes nothing.
    #[test]
    fn a_new_centre_target_stops_the_attack() {
        let mut p = play();
        p.target = 5;
        p.auto_attack = true;
        p.target_info = Some(3);
        assert!(!p.centre_target(Some(5)), "the same: nothing");
        assert_eq!((p.auto_attack, p.target_info), (true, Some(3)));
        assert!(p.centre_target(Some(6)));
        assert_eq!((p.target, p.auto_attack, p.target_info), (6, false, None));
        assert!(p.centre_target(None), "none is a change too");
        assert_eq!(p.target, u32::MAX);
    }

    /// `DrawReticle`'s alpha, a frame of the game's: full on a new target,
    /// then 0.05 less a frame (the EE's arithmetic), never under half; ten
    /// frames from full to half, at 30 and at 165 frames a second drawn (it
    /// steps with the HUD, `frames.rs`). The control: stepped every frame
    /// drawn, 165 Hz reaches half in a sixth of the time.
    #[test]
    fn the_crosshair_flashes_and_fades_to_half_a_frame_of_the_games() {
        let (a, next) = reticle_step(F::new(0.5), true);
        assert_eq!((a.0, next.0), (1.0, (F::<Ee>::ONE - F::new(0.05)).0));
        assert_eq!(reticle_step(F::new(0.5), false), (F::new(0.5), F::new(0.5)), "held at half");
        let mut alpha = F::<Ee>::ONE;
        let mut steps = 0;
        while alpha > F::new(0.5) {
            alpha = reticle_step(alpha, false).1;
            steps += 1;
        }
        assert_eq!(steps, 10);
        let half_at = |hz: f64, every_frame: bool| {
            let mut clock = crate::frames::Frames::default();
            let mut alpha = F::<Ee>::ONE;
            for i in 0..hz as usize * 2 {
                let t = i as f64 * 1000.0 / hz;
                if (clock.due(t) || every_frame) && alpha > F::new(0.5) {
                    alpha = reticle_step(alpha, false).1;
                    if alpha == F::new(0.5) {
                        return t;
                    }
                }
            }
            f64::INFINITY
        };
        for hz in [30.0, 165.0] {
            assert!((half_at(hz, false) - 300.0).abs() < 40.0, "{hz} Hz: {}", half_at(hz, false));
        }
        assert!(half_at(165.0, true) < 70.0, "the control");
    }

    /// 625 in a dialog opened without sound 15 (`b` 0), plays sound 7 when the
    /// byte asks, and refreshes the trainer; queued behind an open window, the
    /// dialog waits its turn. The controls: the same points again change
    /// nothing, a byte of 0 plays no sound 7, and a message cut short is
    /// read as nothing.
    #[test]
    fn training_points_come_from_0x0036() {
        let msg = |flag: u8, points: i64| {
            let mut w = net::stream::Writer::op(0x0036);
            w.u8(flag).signed(points);
            w.data[2..].to_vec()
        };
        let sevens = |p: &Play| p.cues.iter().filter(|c| matches!(c, crate::sound::Cue::Ui(7))).count();
        let fifteens = |p: &Play| p.cues.iter().filter(|c| matches!(c, crate::sound::Cue::Ui(15))).count();
        let mut p = play();
        let (mut cam, mut room) = (WalkCamera::<Ee>::default(), crate::camreach::testing::Room { t: 0, wall: f32::INFINITY });
        p.dump.player.p40 = 3;
        p.msg_0036(&msg(1, 10), &mut cam, &mut room);
        assert_eq!(p.dump.player.p40, 10, "the refund's points");
        assert_eq!((sevens(&p), fifteens(&p)), (1, 0), "sound 7 for the byte, no sound 15 (b 0)");
        assert_eq!(p.mode, 9, "word 625 in the dialog, in play at once");
        assert!(std::mem::take(&mut p.trainer_refresh), "the trainer refreshed");
        // the same points again: QuestStage only on a change
        p.mode = 0;
        p.cues.clear();
        p.msg_0036(&msg(1, 10), &mut cam, &mut room);
        assert_eq!((p.dump.player.p40, sevens(&p), p.mode, p.trainer_refresh), (10, 0, 0, false));
        // a byte of 0: the dialog, no sound 7; behind the trainer it waits
        p.mode = 12;
        p.msg_0036(&msg(0, 4), &mut cam, &mut room);
        assert_eq!((p.dump.player.p40, sevens(&p), p.mode), (4, 0, 12));
        let want = vi::ui::expand(&vi::ui::wide("(word 625)"), 218, true, &vi::ui::ExpandContext::default());
        assert_eq!(p.dialogs.back(), Some(&want), "queued");
        // cut short: nothing read
        p.msg_0036(&[1], &mut cam, &mut room);
        assert_eq!(p.dump.player.p40, 4);
        // go-eqoa-server's buildTrainingPoints(true, 25) after its opcode:
        // the byte 1, then zigzag 25
        p.msg_0036(&[0x01, 50], &mut cam, &mut room);
        assert_eq!(p.dump.player.p40, 25);
    }

    /// 0x001C: the trainer's stat cap (`m_p44`), four raw bytes; cut short,
    /// nothing (the control).
    /// The guaranteed messages in the client's datagrams, each starting
    /// with its opcode (read as `net`'s own tests read them; a state
    /// message ends the datagram's reading, none goes out here).
    fn guaranteed(datagrams: &[Vec<u8>]) -> Vec<Vec<u8>> {
        use drdp::buffer::Buffer;
        let mut out = Vec::new();
        for d in datagrams {
            let mut b = Buffer::reader(&d[..d.len().saturating_sub(4)]);
            b.u16();
            b.u16();
            while b.remaining() > 0 && !b.overflow {
                let h = drdp::read_segment_header(&mut b, 0, 0);
                let mut s = Buffer::reader(&b.rest()[..(h.size as usize).min(b.rest().len())]);
                b.skip(h.size as usize);
                if h.flags & 0x30000 != 0 || s.max == 0 {
                    continue;
                }
                let f = s.u8();
                if f & 0x40 != 0 {
                    s.u32();
                }
                s.u16();
                if f & 1 != 0 {
                    s.u16();
                    if f & 4 != 0 {
                        s.packed();
                    }
                }
                if f & 2 != 0 {
                    s.u16();
                    if f & 8 != 0 {
                        s.packed();
                    }
                }
                if f & 0x10 != 0 {
                    while s.u8() != 248 && !s.overflow {
                        s.u16();
                    }
                }
                while s.remaining() > 0 && !s.overflow {
                    let ty = s.u8();
                    let size = s.size() as usize;
                    if ty.wrapping_add(7) < 3 {
                        s.u16();
                        if ty == drdp::TYPE_GUARANTEED {
                            out.push(s.rest()[..size.min(s.rest().len())].to_vec());
                        }
                        s.skip(size);
                    } else if ty == 252 {
                        s.skip(size);
                    } else {
                        break;
                    }
                }
            }
        }
        out
    }

    /// NPC/PC Display and Distance against the server: one that offered
    /// nothing (the control: Sandstorm's, eqoa.live's) is told nothing and the
    /// game's 24 are read; an offer is answered once with the player's numbers
    /// (the slots within it), again when either changes; a lowered number
    /// drops what was past it.
    #[test]
    fn npc_pc_display_agrees_with_the_server() {
        struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
        impl net::session::Datagrams for Rec {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                self.0.borrow_mut().push(d.to_vec());
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        let mut p = Play::new(s, Player::default(), Box::default());
        let asked = |p: &mut Play| -> Vec<Vec<u8>> {
            p.session.flush();
            let out = guaranteed(&sent.borrow()).into_iter().filter(|m| m.starts_with(&[0x00, 0x7E])).collect();
            sent.borrow_mut().clear();
            out
        };
        asked(&mut p);
        crate::settings::set_client(crate::settings::ClientOptions { entity_slots: 128, ..Default::default() });

        p.agree_entity_view();
        assert!(asked(&mut p).is_empty(), "the control: no offer, nothing sent");
        assert_eq!(p.entities.limit, 24);
        assert_eq!(p.options_wnd.server_note.as_deref(), Some("This server: 24 (the game's)"));

        p.session.entity_view_offer = Some((204, 1000));
        p.agree_entity_view();
        assert_eq!(asked(&mut p), [vec![0x00, 0x7E, 128, 0, 0, 0, 100, 0, 0, 0]], "128 within the game's 100");
        assert_eq!(p.entities.limit, 128);
        assert_eq!(p.options_wnd.server_note.as_deref(), Some("This server: 204 within 1000"));
        p.agree_entity_view();
        assert!(asked(&mut p).is_empty(), "told once");
        crate::settings::set_client(crate::settings::ClientOptions { entity_slots: 128, entity_view: 600, ..Default::default() });
        p.agree_entity_view();
        assert_eq!(asked(&mut p), [vec![0x00, 0x7E, 128, 0, 0, 0, 0x58, 0x02, 0, 0]], "the distance changed: told again");

        p.entities.slots[100] = Some(net::entity::Entity { id: 0x4242, kind: 130, ..Default::default() });
        p.entities.slots[50] = Some(net::entity::Entity { id: 0x4343, kind: 130, ..Default::default() });
        crate::settings::set_client(crate::settings::ClientOptions { entity_slots: 64, entity_view: 600, ..Default::default() });
        p.agree_entity_view();
        assert_eq!(asked(&mut p), [vec![0x00, 0x7E, 64, 0, 0, 0, 0x58, 0x02, 0, 0]]);
        assert!(p.entities.slots[100].is_none() && p.entities.slots[50].is_some(), "past 64 dropped, within kept");

        p.session.entity_view_offer = Some((40, 500));
        p.agree_entity_view();
        assert_eq!(asked(&mut p), [vec![0x00, 0x7E, 40, 0, 0, 0, 0x58, 0x02, 0, 0]], "the server's most slots; the distance is the server's to cap");
        assert_eq!(p.entities.limit, 40);
    }

    /// Case 0x077A, the server's OutMatched: word 1196 ("You are
    /// outmatched...") in red, auto-attack off, and 0x077B (1915) sent
    /// back, as CLIENT's case does. The control: the same flush with
    /// nothing received sends no 0x077B and leaves the attack on.
    #[test]
    fn outmatched_says_so_stops_the_attack_and_answers() {
        struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
        impl net::session::Datagrams for Rec {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                self.0.borrow_mut().push(d.to_vec());
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        let mut p = Play::new(s, Player::default(), Box::default());
        p.session.flush();
        assert!(guaranteed(&sent.borrow()).iter().any(|m| m.starts_with(&[0x04, 0x09])), "the reader reads the login");
        sent.borrow_mut().clear();
        p.auto_attack = true;
        // the control: nothing received, nothing answered, the attack on
        p.session.flush();
        assert!(!guaranteed(&sent.borrow()).iter().any(|m| m[..] == [0x7B, 0x07]));
        assert!(p.auto_attack);
        p.msg_077a();
        assert_eq!(p.chat.back(), Some(&("(word 1196)".to_string(), [1.0, 0.25, 0.25, 1.0])), "word 1196 in red");
        assert!(!p.auto_attack, "auto-attack off");
        p.session.flush();
        let msgs = guaranteed(&sent.borrow());
        assert!(msgs.iter().any(|m| m[..] == [0x7B, 0x07]), "0x077B answered: {msgs:02x?}");
    }

    /// 0x0032 on the rules of a patch with S2's record of the pad carries its
    /// 11 bytes after the target: the press E made (circle's low bits, the
    /// pad read, pressed this frame). The control: retail's rules send the
    /// game's 0x0032 alone, the same bytes up to the target.
    #[test]
    fn use_skill_carries_s2s_record_of_the_pad() {
        struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
        impl net::session::Datagrams for Rec {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                self.0.borrow_mut().push(d.to_vec());
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        let mut p = Play::new(s, Player::default(), Box::default());
        p.abilities.insert(7, net::dump::Ability { slot: 1, name: "Sanctuary".into(), ..Default::default() });
        p.ability_token = 0x55;
        p.target = 0x4242;
        let used = |p: &mut Play, r: crate::s2::Rules| -> Vec<u8> {
            crate::s2::with_rules(r, || {
                p.session.flush();
                sent.borrow_mut().clear();
                p.note_hud_key(crate::hudmenu::Key::Use, true);
                p.send_0032(7);
                p.note_hud_key(crate::hudmenu::Key::Use, false);
                p.session.flush();
            });
            guaranteed(&sent.borrow()).into_iter().find(|m| m.starts_with(&[0x32, 0x00])).expect("0x0032 sent")
        };
        let retail = used(&mut p, crate::s2::Rules::retail());
        assert_eq!(retail, [0x32, 0x00, 0x55, 0, 0, 0, 1, 0x42, 0x42, 0, 0], "the control: the game's");
        let live = used(&mut p, crate::s2::Rules { use_skill_record: true, ..crate::s2::Rules::retail() });
        assert_eq!(live.len(), 22);
        assert_eq!(live[..11], retail[..]);
        assert_eq!(live[21] & !1, 0x76, "circle, the pad read, pressed this frame");
    }

    /// ConnectShard's hello: the patch's bytes after the opcode when it has
    /// them, before 0x0001 (as the session records what it sends). The
    /// control: none sends retail's DiscVersion 18.
    #[test]
    fn the_shard_hello_is_the_patch_s() {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        // two shards: the one just closed is not opened again at once
        let hello = |s: &mut Session, drdp_port: u16, h: Option<&[u8]>| -> Vec<Vec<u8>> {
            s.record = Some(Vec::new());
            s.connect_shard(drdp::address_to_endpoint([127, 0, 0, 1], 10070, drdp_port), h);
            s.record.take().unwrap().into_iter().filter(|(t, _)| *t == drdp::TYPE_GUARANTEED).map(|(_, m)| m).collect()
        };
        let retail = hello(&mut s, 1, None);
        assert_eq!(retail[0], [0, 0, 18, 0, 0, 0], "the control: retail's");
        assert!(retail[1].starts_with(&[0x01, 0x00]), "then 0x0001");
        let patch = [21, 0, 0, 0, 3, 0, 0, 0, 0x31, 0, 0, 0, 9, 9];
        let live = hello(&mut s, 2, Some(&patch));
        assert_eq!(live[0][..2], [0, 0]);
        assert_eq!(live[0][2..], patch);
        assert!(live[1].starts_with(&[0x01, 0x00]));
        assert_eq!(live.len(), 2);
    }

    /// Ignore Shouts (the quick chat's |10, DoAction 0x0B030003 with 0) sends
    /// the game's 0x0C01 [1, 0] and is kept; the next login tells the server
    /// again, once, and a shout the server still sends is dropped while the
    /// player's own and a say are not; Restore Shouts (1) sends [1, 1] and
    /// brings them back. The control: a login with shouts heard tells the
    /// server nothing.
    #[test]
    fn ignore_shouts_stays_until_restored() {
        struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
        impl net::session::Datagrams for Rec {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                self.0.borrow_mut().push(d.to_vec());
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        crate::settings::testing::reset(false);
        let login = |sent: &std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>| {
            let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
            s.self_id = Some(0x100);
            let mut p = Play::new(s, Player { name: "Gnolie".into(), ..Default::default() }, Box::default());
            p.session.flush();
            sent.borrow_mut().clear();
            p
        };
        let flags = |p: &mut Play, sent: &std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>| -> Vec<Vec<u8>> {
            p.session.flush();
            let out = guaranteed(&sent.borrow()).into_iter().filter(|m| m.starts_with(&[0x01, 0x0C, 0x01])).collect();
            sent.borrow_mut().clear();
            out
        };
        let green = [63.0 / 255.0, 1.0, 63.0 / 255.0, 1.0];
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        // the control: shouts heard, a login tells nothing, a shout is shown
        let mut p = login(&sent);
        p.tell_shouts();
        assert!(flags(&mut p, &sent).is_empty(), "the control: nothing told");
        p.server_line("Powerbrick shouts: yrs?".into(), green);
        assert_eq!(last_line(&p), "Powerbrick shouts: yrs?");
        // Ignore Shouts: sent and kept
        p.do_action(0x0B03_0003, 0);
        assert_eq!(flags(&mut p, &sent), [vec![0x01, 0x0C, 0x01, 0x00]], "the game's 0x0C01 [1, 0]");
        assert!(!crate::settings::shouts(), "kept");
        // the next login: told once, shouts dropped, own and says kept
        let mut p = login(&sent);
        p.tell_shouts();
        p.tell_shouts();
        assert_eq!(flags(&mut p, &sent), [vec![0x01, 0x0C, 0x01, 0x00]], "told again, once");
        p.server_line("Powerbrick shouts: yrs?".into(), green);
        assert_eq!(last_line(&p), "", "someone else's shout dropped");
        p.server_line("Gnolie shouts: lfg".into(), green);
        assert_eq!(last_line(&p), "Gnolie shouts: lfg", "the player's own kept");
        p.server_line("Powerbrick: no shouts: here".into(), [1.0; 4]);
        assert_eq!(last_line(&p), "Powerbrick: no shouts: here", "a say kept");
        // Restore Shouts: sent, kept, heard again
        p.do_action(0x0B03_0003, 1);
        assert_eq!(flags(&mut p, &sent), [vec![0x01, 0x0C, 0x01, 0x01]], "the game's 0x0C01 [1, 1]");
        assert!(crate::settings::shouts());
        p.server_line("Powerbrick shouts: back".into(), green);
        assert_eq!(last_line(&p), "Powerbrick shouts: back");
        let mut p = login(&sent);
        p.tell_shouts();
        assert!(flags(&mut p, &sent).is_empty(), "restored: the next login tells nothing");
        crate::settings::testing::reset(false);
    }

    #[test]
    fn the_stat_cap_comes_from_0x001c() {
        let mut p = play();
        p.dump.player.p44 = 11;
        p.msg_001c(&40u32.to_le_bytes());
        assert_eq!(p.dump.player.p44, 40);
        p.msg_001c(&[7, 0]);
        assert_eq!(p.dump.player.p44, 40);
    }

    /// The player's own hit (0x00DB, the player the attacker) reaches the
    /// swing timer, with the damage numbers turned off too: after a swap
    /// (the bow's swing, then a melee weapon's) the round's first hit puts
    /// the next swing a fresh press's 660 ms before it, one melee delay on.
    /// The control: another attacker's hit moves nothing.
    #[test]
    fn an_own_hit_reaches_the_swing_timer() {
        let hit = |attacker: u32| {
            let mut w = net::stream::Writer::default();
            w.u32(attacker).signed(-30).u32(0x200);
            w.data
        };
        let mut p = play();
        p.options.no_damage = 1;
        let me = p.self_id32().expect("the player's id");
        p.attack.press(0);
        p.attack.swing(9);
        p.attack.add(5000);
        p.attack.swing(4);
        let due = p.attack.due;
        p.damage_indication(&hit(me ^ 1), 0);
        assert_eq!(p.attack.due, due, "another's hit");
        let now = p.session.now();
        p.damage_indication(&hit(me), 0);
        assert_eq!(p.attack.due, now.wrapping_sub(crate::attackclock::PRESS_TO_FIRST_HIT).wrapping_add(4000), "the player's own");
    }

    /// A server's patch's Group menu (`s2`, Sandstorm's player search):
    /// Search opens the who window on the search's results (mode 35, the
    /// request out), and the who list proper takes the window back; without
    /// the patch (the control) its message does nothing, as no retail menu
    /// carries it.
    #[test]
    fn a_patchs_search_opens_the_who_window() {
        let search = crate::s2::Rules { player_search: true, ..crate::s2::Rules::retail() };
        crate::s2::with_rules(search, || {
            let mut p = play();
            p.do_action(0xFF00_0001, 0);
            assert_eq!((p.mode, p.search_mode, p.who_open, p.who.pending), (35, true, true, true));
            p.do_action(0x0B03_0004, 0);
            assert!(!p.search_mode);
        });
        let mut p = play();
        p.do_action(0xFF00_0001, 0);
        assert_eq!((p.mode, p.search_mode, p.who_open), (0, false, false));
    }

    /// Sandstorm's live S2 (`searchwnd`): its Search opens the patch's own
    /// window over play, on the player's own levels, with nothing asked
    /// yet (the who window stays shut); while it is up it has the keys, its
    /// Search asks with its filter, and △ closes it and lets the player
    /// and the camera go. The control: a patch whose window is not that
    /// shape (its search flag alone) shows the search in the who window,
    /// asked for the player's levels too.
    #[test]
    fn the_live_patchs_search_window() {
        struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
        impl net::session::Datagrams for Rec {
            fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
                self.0.borrow_mut().push(d.to_vec());
            }
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let Some(patch) = crate::searchwnd::testing::live() else { return };
        crate::s2::with_rules(crate::s2::Rules::from_patch(&patch), || {
            let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
            s.self_id = Some(0x100);
            let mut p = Play::new(s, Player::default(), Box::default());
            p.dump.player.level = 60;
            p.do_action(0xFF00_0001, 0);
            assert!(p.search_open());
            assert_eq!((p.mode, p.who_open, p.search_mode), (0, false, false));
            let searches = |sent: &std::cell::RefCell<Vec<Vec<u8>>>| guaranteed(&sent.borrow()).into_iter().filter(|m| m.starts_with(&[0x02, 0x15])).collect::<Vec<_>>();
            p.session.flush();
            assert!(searches(&sent).is_empty(), "nothing asked on opening");
            // down to Search, ✕
            for k in [139, 139, 139, 139, 134] {
                assert!(p.window_key(k, true), "the window has the key");
                p.window_key(k, false);
            }
            p.session.flush();
            assert_eq!(searches(&sent), vec![vec![0x02, 0x15, 0xFF, 0xFF, 45, 60]]);
            // a frame drawn holds the player; △ lets go
            p.locks = (false, false);
            p.window_key(132, true);
            assert!(!p.search_open());
            assert_eq!(p.locks, (true, true));
            assert!(!p.window_key(134, true), "closed: play has the keys");
        });
        let flag = crate::s2::Rules { player_search: true, ..crate::s2::Rules::retail() };
        crate::s2::with_rules(flag, || {
            let mut p = play();
            p.dump.player.level = 60;
            p.do_action(0xFF00_0001, 0);
            assert!(!p.search_open());
            assert_eq!((p.mode, p.search_mode), (35, true));
        });
    }

    /// A server's patch that colours damage (`s2`: Sandstorm's S2) has the
    /// three bytes after 0x00DB's body colour the number, the player's own
    /// hit and one taken alike; without it (the control) they are passed
    /// over and the number is the game's white.
    #[test]
    fn a_patchs_damage_colour() {
        let hit = |attacker: u32, defender: u32| {
            let mut w = net::stream::Writer::default();
            w.u32(attacker).signed(-30).u32(defender).bytes(&[255, 128, 0]);
            w.data
        };
        let shown = |p: &mut Play, id: u32| {
            p.damage.step(33, |e| (e == id).then_some(crate::damage::Target { pos: [0.0; 3], slot4: None }))
        };
        let colored = crate::s2::Rules { damage_rgb: true, ..crate::s2::Rules::retail() };
        let (s, me) = crate::s2::with_rules(colored, || {
            let mut p = play();
            let me = p.self_id32().expect("the player's id");
            p.damage_indication(&hit(me, 0x200), 0);
            (shown(&mut p, 0x200), me)
        });
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
        let mut p = play();
        p.damage_indication(&hit(me, 0x200), 0);
        let s = shown(&mut p, 0x200);
        assert_eq!((s.len(), s[0].color), (1, [1.0; 4]));
    }

    /// 0x1402 sub-op 3 (`VIClientPart4EC58::Recv03`): a rise in Class
    /// Mastery points is `ShowNotice(text, 7)`: sound 7, then word 1628
    /// for one point or 1627 for several (`%d` the rise) in a dialog opened
    /// without sound 15 (`ShowDialog(0, text, 0, 0)`), queued behind a
    /// window already up (F3 20261023-082704: it went to the chat box, no
    /// dialog). The controls: the same points again, and fewer, say
    /// nothing.
    #[test]
    fn a_class_mastery_point_is_a_dialog() {
        let msg = |points: u32| {
            let mut v = vec![3u8];
            v.extend(points.to_le_bytes());
            v.extend(0u32.to_le_bytes());
            v
        };
        let sevens = |p: &Play| p.cues.iter().filter(|c| matches!(c, crate::sound::Cue::Ui(7))).count();
        let fifteens = |p: &Play| p.cues.iter().filter(|c| matches!(c, crate::sound::Cue::Ui(15))).count();
        let mut lines = vec![String::new(); 1628];
        lines[1626] = "You have earned %d mastery points.".into();
        lines[1627] = "You have earned 1 mastery point.".into();
        let mut p = play();
        p.words = vi::strtab::StringTable::parse(lines.join("\n").as_bytes());
        let vui = crate::pointer::testing::vui();
        let mut r = crate::pointer::testing::NullRaster;
        let said = |s: &str| vi::ui::expand(&vi::ui::wide(s), 218, true, &vi::ui::ExpandContext::default());
        // one point, nothing up: the dialog at once, sound 7, no sound 15
        p.cm_msgs.push(msg(1));
        p.run_class_mastery(&vui, &mut r);
        assert_eq!((sevens(&p), fifteens(&p), p.mode), (1, 0, 9), "word 1628 in the dialog");
        assert!(p.chat_pending.is_empty(), "not in the chat box");
        // the same points again: no rise, nothing said
        p.mode = 0;
        p.cues.clear();
        p.cm_msgs.push(msg(1));
        p.run_class_mastery(&vui, &mut r);
        assert_eq!((sevens(&p), p.mode, p.dialogs.len()), (0, 0, 0));
        // three more behind the trainer: word 1627 with the rise, queued
        p.mode = 12;
        p.cm_msgs.push(msg(4));
        p.run_class_mastery(&vui, &mut r);
        assert_eq!((sevens(&p), p.mode), (1, 12));
        assert_eq!(p.dialogs.back(), Some(&said("You have earned 3 mastery points.")));
        // fewer (a point spent): nothing
        p.cues.clear();
        p.cm_msgs.push(msg(2));
        p.run_class_mastery(&vui, &mut r);
        assert_eq!((sevens(&p), p.dialogs.len()), (0, 1));
    }

    /// A problem report's entity lines carry each creature's look and hands
    /// as its channel sent them: here Coachman Quicksteed as Sandstorm sent
    /// him in `pcaps/ps2_sandstorm.pcap` (no robe: byte 109 is 255), so a
    /// report of a robe on someone shows what the server sent. An entity
    /// with no creature part has no look line (the control).
    /// The start menu's Location and the compass's group patches go by the
    /// world the client is in (`m_worldId`): after a teleport into Rathe
    /// (`Msg00B2` sets the player's world, the dump keeps the login's) a
    /// cell is named from Rathe's grid, and a report says RATHE.ESF. The
    /// control: the dump's world, Tunaria, which named a Rathe cell "Anu
    /// Village" (F3 20261023-053742).
    #[test]
    fn the_location_is_the_world_the_client_is_in() {
        let mut p = play();
        p.dump.player.world = 0;
        p.player.world_path = "data\\tunaria.esf".into();
        // what Msg00B2 into world 1 sets
        p.player.world = 1;
        assert_eq!(p.world_id(), 1);
        let here = crate::zones::zone(p.world_id(), 1, 1);
        assert_eq!(here, Some(1178), "Rathe's grid");
        assert_ne!(crate::zones::zone(p.dump.player.world as i32, 1, 1), here, "the control: Tunaria's grid names it otherwise");
        let r = p.report([0.0; 3]);
        assert!(r.contains("world 1 (RATHE.ESF)"), "{r}");
        assert!(!r.contains("tunaria.esf"), "the login's path is not the world now");
    }

    /// A dead target's bar shows no face: `Process` drops the target info
    /// while the target's +0x19 is 0, and it stays dropped until a 0x0010
    /// (F3 20261023-075853). The control: a living target keeps it.
    #[test]
    fn a_dead_targets_info_goes() {
        let mut p = play();
        p.target = 0x13B875E;
        p.target_info = Some(1);
        p.entities.slots[4] = Some(net::entity::Entity { id: 0x13B875E, kind: 130, p19: 40, ..Default::default() });
        p.drop_dead_target_info();
        assert_eq!(p.target_info, Some(1), "the control: alive, the face stays");
        p.entities.slots[4].as_mut().unwrap().p19 = 0;
        p.drop_dead_target_info();
        assert_eq!(p.target_info, None, "dead: no face");
        p.entities.slots[4].as_mut().unwrap().p19 = 40;
        p.drop_dead_target_info();
        assert_eq!(p.target_info, None, "and not back before a 0x0010");
        // another creature at 0 is not the target's death
        p.target_info = Some(1);
        p.entities.slots[5] = Some(net::entity::Entity { id: 0x42, kind: 130, p19: 0, ..Default::default() });
        p.drop_dead_target_info();
        assert_eq!(p.target_info, Some(1));
    }

    #[test]
    fn a_report_shows_each_creatures_look() {
        let mut p = play();
        let mut c = net::entity::Creature { name: "Coachman Quicksteed".into(), robe: 255, hair: [2, 3, 1], face: 3, item_types: [1, 0], ..Default::default() };
        c.armor = [0, 0, 0, 0, 2, 0];
        c.armor_tint = [255; 6];
        c.items = [0xBF7E7DCB, 0, 0];
        p.entities.slots[15] = Some(net::entity::Entity { id: 0x2B1DE, kind: 130, model: 0x70D898C6, creature: Some(c), ..Default::default() });
        p.entities.slots[3] = Some(net::entity::Entity { id: 0x42, kind: 129, model: 0x1234, ..Default::default() });
        let r = p.report([0.0; 3]);
        let lines: Vec<&str> = r.lines().collect();
        let at = lines.iter().position(|l| l.contains("\"Coachman Quicksteed\"")).expect("the coachman's line");
        assert_eq!(
            lines.get(at + 1).copied(),
            Some("    look: robe 255 tint 0, armor [0, 0, 0, 0, 2, 0] tints [255, 255, 255, 255, 255, 255], hair [2, 3, 1], face 3; items [0xbf7e7dcb, 0x0, 0x0] over [0x0, 0x0, 0x0] types [1, 0]")
        );
        assert!(lines.get(at + 2).is_some_and(|l| l.starts_with("    move: state 0 walking 0, velocity [0.00, 0.00, 0.00], updated ")), "then how it moves: {:?}", lines.get(at + 2));
        let plain = lines.iter().position(|l| l.starts_with("  0x42 kind 129")).expect("the plain entity's line");
        assert!(!lines.get(plain + 1).is_some_and(|l| l.contains("look:") || l.contains("move:")), "an entity with no creature part has no look or move line");
    }

    /// A report's move line: the state and velocity the channel last sent,
    /// the time since that update (its stamp is the update's time plus the
    /// glide's two update periods) and how far the glide still draws it from
    /// where it is. A player shoved against a wall: standing (state 0) with
    /// a running velocity, drawn 1.5 past where it stands.
    #[test]
    fn a_report_says_how_each_creature_moves() {
        let c = net::entity::Creature { state: 0, velocity: [8.0, 0.0, -3.0], ..Default::default() };
        let mut e = net::entity::Entity { id: 7, kind: 130, pos: [10.0, 0.0, 20.0], actor_pos: [11.2, 0.0, 20.9], ..Default::default() };
        e.stamp = 5_000 + 2 * UPDATE_RATE;
        assert_eq!(move_line(&e, &c, 5_350), "move: state 0 walking 0, velocity [8.00, 0.00, -3.00], updated 350 ms ago, drawn 1.50 from it");
        // the clock wraps as the session's does
        e.stamp = 50u32.wrapping_sub(100).wrapping_add(2 * UPDATE_RATE);
        assert!(move_line(&e, &c, 50).contains("updated 100 ms ago"), "{}", move_line(&e, &c, 50));
    }

    /// The chat mode's Guild (`|17`, the start menu's): the guild's channel
    /// when in one (word 599), else word 570 and the channel as it was.
    #[test]
    fn guild_chat_is_the_guilds_channel_in_a_guild_only() {
        let mut p = play();
        p.do_action(0x0B03_0002, 0);
        assert_eq!((p.channel, last_line(&p)), (0, "(word 570)"));
        p.guild.in_guild = true;
        p.do_action(0x0B03_0002, 0);
        assert_eq!((p.channel, last_line(&p)), (4, "(word 599)"));
        p.do_action(0x0B01_0000, 0);
        assert_eq!((p.channel, last_line(&p)), (0, "(word 595)"), "Say takes it back");
    }

    /// The run speed is the server's, as `UpdateGame` hands `m_speed` to
    /// the walk each frame: the dump's at world entry (Sandstorm's
    /// 8.833333), then 0x00F8's `SetSpeed` (Spirit of the Wolf's 11.483333
    /// and a snare's 7.243333 in a Sandstorm capture, the walk speed 3 with
    /// either; a stop, 0, stops the walk too), and a dump's again (Msg000D:
    /// no SetSpeed, the walk speed kept). The control is the walk's own
    /// speed, the client's until 2026-10-23, which no buff moved (the user:
    /// "sow and invis seems to have no effect on toons").
    #[test]
    fn the_run_speed_is_the_servers() {
        let at = |p: &Play| (p.control_client().run_speed.to_bits(), p.control_client().walk_speed);
        let mut p = play_with(Box::new(net::dump::MemoryDump { speed: f32::from_bits(0x410D5555), ..Default::default() }));
        assert_eq!(at(&p), (0x410D5555, 3.0));
        p.set_speed(f32::from_bits(0x4137BBBB));
        assert_eq!(at(&p), (0x4137BBBB, 3.0));
        assert_ne!(p.control_client().run_speed, vi::walk::RUN_SPEED, "the control: the walk's own speed");
        p.set_speed(f32::from_bits(0x40E7C963));
        assert_eq!(at(&p), (0x40E7C963, 3.0));
        p.set_speed(0.0);
        assert_eq!(at(&p), (0, 0.0));
        // a denormal is 0 to the EE's compare
        p.set_speed(f32::from_bits(1));
        assert_eq!(at(&p).1, 0.0);
        p.speed = f32::from_bits(0x410D5555);
        assert_eq!(at(&p), (0x410D5555, 0.0), "a dump's speed leaves the walk speed as SetSpeed left it");
    }

    /// `ApplyLook`'s `Flag1` (state byte 154, bit 1): an invisible creature's
    /// skins are hidden. The pet bits beside it (4 and 8) hide nothing; no
    /// creature yet is shown. The control is the client until 2026-10-23,
    /// which drew every body whatever the byte said.
    #[test]
    fn an_invisible_creature_has_its_skins_hidden() {
        let c = |flags262: u8| net::entity::Creature { flags262, ..Default::default() };
        assert!(!skins_shown(Some(&c(1))));
        assert!(!skins_shown(Some(&c(0x1D))));
        assert!(skins_shown(Some(&c(0))) && skins_shown(Some(&c(4))) && skins_shown(Some(&c(8))));
        assert!(skins_shown(None));
    }

    /// `ToggleFollow`: a targeted member of the group is followed; the
    /// player itself, someone outside the group or nothing is word 1195.
    /// Again, off.
    #[test]
    fn follow_takes_a_targeted_group_member() {
        let mut p = play();
        grouped(&mut p, &[0x100, 0x200]);
        for t in [u32::MAX, 0x100, 0x300] {
            p.target = t;
            p.chat.clear();
            p.do_action(0x0B0E_0007, 0);
            assert!(!p.following && last_line(&p) == "(word 1195)", "target {t:#x}");
        }
        p.target = 0x200;
        p.chat.clear();
        p.do_action(0x0B0E_0007, 0);
        assert!(p.following && p.follow_id == 0x200 && p.chat.is_empty());
        p.do_action(0x0B0E_0007, 0);
        assert!(!p.following && p.chat.is_empty(), "again: off, nothing said");
        // and `/follow` is the same command
        p.chat_line("/follow");
        assert!(p.following);
    }

    /// What `UpdateControls` is given: off; the member gone from the group
    /// or not in the scene, lost; and while following the run is 1.05 times
    /// the speed the server gave (the dump's, Sandstorm's 8.833333).
    #[test]
    fn the_follow_handed_to_the_controls() {
        let speed = f32::from_bits(0x410D5555);
        let mut p = play_with(Box::new(net::dump::MemoryDump { speed, ..Default::default() }));
        assert_eq!((p.control_client().follow, p.control_client().run_speed), (Follow::Off, speed));
        grouped(&mut p, &[0x100, 0x200]);
        p.target = 0x200;
        p.do_action(0x0B0E_0007, 0);
        let c = p.control_client();
        // no entity 0x200 in the scene: lost, for UpdateControls to end it
        assert_eq!(c.follow, Follow::Lost);
        assert_eq!(c.run_speed, (F::<Ee>::new(speed) * F::new(1.05)).0);
        assert!(c.run_speed > speed);
        p.controls_changed(crate::controls::Changed { autorun: false, follow: true });
        assert_eq!(p.control_client().follow, Follow::Off);
        // a follow that ends autorun says so, as the stick's does
        p.autorun = true;
        p.controls_changed(crate::controls::Changed { autorun: true, follow: false });
        assert!(!p.autorun && last_line(&p) == "(word 720): (word 552)");
    }

    /// `Msg007B`: the id targeted, the attack left on; -1 is word 838.
    #[test]
    fn an_assist_answer_targets_its_id() {
        let mut p = play();
        p.target = 0x200;
        p.auto_attack = true;
        p.msg_007b(0x300);
        assert_eq!((p.target, p.auto_attack), (0x300, true));
        p.msg_007b(u32::MAX);
        assert_eq!((p.target, last_line(&p)), (0x300, "(word 838)"));
    }

    /// Hide UI (`hideui`): a click on the compass opens the map only while
    /// the HUD is shown; hidden, no HUD button is there and the click is the
    /// world's. The control: shown, the same click opens the map.
    #[test]
    fn hidden_the_huds_buttons_take_no_click() {
        let mut p = play();
        p.hud = Some((crate::ui::Vui::new(Default::default(), Default::default(), Vec::new()), crate::ui::UiAssets::default(), crate::hud::Hud::new()));
        let compass = crate::mouse::Click::Target(39.0, 54.0);
        crate::hideui::toggle();
        assert!(crate::hideui::hidden());
        assert!(!p.point(compass, (640, 448), false), "hidden: the world's click");
        assert!(!p.map.open);
        crate::hideui::toggle();
        assert!(p.point(compass, (640, 448), false), "the control: shown, the HUD's");
        assert!(p.map.open, "and the compass opens the map");
    }

    /// What shows the UI again (`Play::wants_ui`): a mode of the game's but
    /// the manual camera's, the map, the target's details, a line typed; and
    /// a world entry. The control: play alone wants none.
    #[test]
    fn a_window_up_shows_the_ui_again() {
        let mut p = play();
        assert!(!p.wants_ui(), "the control: play alone");
        p.mode = 13;
        assert!(p.wants_ui(), "the start menu");
        p.mode = 38;
        assert!(!p.wants_ui(), "the manual camera is play's");
        p.mode = 0;
        p.map.open = true;
        assert!(p.wants_ui(), "the map");
        p.map.open = false;
        p.info_up = true;
        assert!(p.wants_ui(), "the target's details");
        p.info_up = false;
        p.typing = Some(String::new());
        assert!(p.wants_ui(), "a line typed");
        p.typing = None;
        crate::hideui::toggle();
        assert!(crate::hideui::frame(false), "hidden, play alone");
        p.mode = 1;
        assert!(!crate::hideui::frame(p.wants_ui()), "a corpse's window: shown again");
        crate::hideui::toggle();
        let _ = play();
        assert!(!crate::hideui::hidden(), "a world entry shows it");
    }

    /// Target of Target (the Extended page's): the name of whom the target's
    /// channel says it targets, among the entities in view, the player's own
    /// included; none with the option off (the control), no target, a
    /// target's target of 0 or -1 (Sandstorm's NPCs), itself, or one not in
    /// view.
    #[test]
    fn the_target_of_target_is_the_targets_channels() {
        use net::entity::{Creature, Entity};
        crate::settings::testing::reset(false);
        let mut p = play();
        let put = |p: &mut Play, k: usize, id: u32, name: &str, target: u32| {
            p.entities.slots[k] = Some(Entity { id, kind: 130, creature: Some(Creature { name: name.into(), target, ..Default::default() }), ..Default::default() });
        };
        put(&mut p, 0, 0x100, "Lanelook", 0x200);
        put(&mut p, 1, 0x200, "Casil", 0x300);
        put(&mut p, 2, 0x300, "Luck", u32::MAX);
        p.target = 0x200;
        assert_eq!(p.target_of_target(), None, "the control: off, the game's bar alone");
        crate::settings::set_client(crate::settings::ClientOptions { target_of_target: true, ..Default::default() });
        assert_eq!(p.target_of_target().as_deref(), Some("Luck"));
        put(&mut p, 1, 0x200, "Casil", 0x100);
        assert_eq!(p.target_of_target().as_deref(), Some("Lanelook"), "the player's own");
        for (t, why) in [(0, "0: an NPC on Sandstorm"), (u32::MAX, "-1"), (0x200, "itself"), (0x999, "not in view")] {
            put(&mut p, 1, 0x200, "Casil", t);
            assert_eq!(p.target_of_target(), None, "{why}");
        }
        p.target = 0x300;
        assert_eq!(p.target_of_target(), None, "Luck targets none");
        p.target = u32::MAX;
        assert_eq!(p.target_of_target(), None, "no target");
        crate::settings::testing::reset(false);
    }
}
