//! What the client keeps between runs. The game keeps `VIGameOptions` in
//! `game.opt` on the memory card (SUPPORT's `vigameoptions.cpp`): loaded
//! before the login (`VIWndInitFromMC`, which sets the two sound classes'
//! volumes from it), written by the Options window's Save Changes
//! (`VIWndOptions::SaveOptions`) and at the server select
//! (`VIWndServerSelect::Connect`, for the home shard, which the client does
//! not keep: it has no server select). The client does the same with the
//! options it shows, and keeps its own beside them in `client.opt`: the
//! Options window's Extended page (Level of Detail, Brightness, Frame Rate,
//! Camera Distance, Camera Turn Speed, and each window's Controller Focus by its place among the
//! client's windows open, `multibox`), which the game does not have; and in `keys.opt` the
//! Controls page's keys and buttons (`binds`), per device as the card keeps
//! every option, not per character. `server.opt` holds the title's Network
//! Setup's server, and natively `disc.opt` the disc image the player picked
//! (`discs`).
//!
//! Each window of several keeps its own options by its place (`multibox`; F3
//! 20261023, the user's choice): window 1 in `game.opt` and `client.opt` as the
//! client always kept them, window n in `game-<n>.opt` and `client-<n>.opt`
//! (`own_file`), so one window may play with sound and another without. A
//! window that has kept none of its own yet starts from window 1's: its
//! options and home shard, not its saved account (two windows cannot play one
//! account at once). `client.opt` also holds what the windows share: every
//! window's Controller Focus and Capture Controller, which a later window's
//! save writes into it, window 1's own lines left as they are. The keys and
//! buttons are the computer's (one keyboard), and a save writes them only when
//! this window's Controls page changed them, so it never puts back what an
//! older window still holds over another window's.
//!
//! Where: natively `$EQOA_CONFIG`, else `eqoa-rs` in the platform's config
//! directory (`$XDG_CONFIG_HOME` or `~/.config`, `~/Library/Application
//! Support` on a Mac, `%APPDATA%` on Windows), for the game (`eqoa`, `eqoa
//! boot`, `eqoa play`: `keep_in_user_dir`); the other commands (`shot`, `playshot`) keep and read
//! nothing unless `$EQOA_CONFIG` names a directory, so a player's settings
//! never change a test's picture. In a page, the origin's localStorage
//! (`eqoa.game.opt`, `eqoa.client.opt` and `eqoa.keys.opt`, as hex; a later
//! window's `eqoa.game-<n>.opt` and `eqoa.client-<n>.opt`), where every access may
//! fail (a private window, storage turned off): the page then runs on the
//! defaults and keeps nothing.
//!
//! `game.opt` is the game's 1,024 bytes, little-endian as the EE stores them,
//! with the home shard (+0x98) always zero: the client never stores it. The
//! login (+0x10 the username, +0x50 the password, +0x90 Save Account) is
//! STATION's: kept when its Save Account is ticked, as the card kept it, the
//! password in plain text (`save_login`); empty otherwise.
//!
//! The frame cap the Extended page's Frame Rate sets is kept here too
//! (`frame_due`, `frame_wait`): a page's animation frames and the native
//! window's redraws both go through it.

use crate::options::GameOptions;
use std::cell::Cell;
use vi::fpu::{Ee, F};

/// `sizeof(VIGameOptions)`, all of which `Load` wants.
pub const CARD_SIZE: usize = 1024;
/// `VIGameOptions::m_magic` (+0x3FC), which `Load` checks.
const MAGIC: u32 = 0x0123_4567;
/// Window 1's game.opt, and the one a later window starts from.
const GAME_FILE: &str = "game.opt";
/// Window 1's client.opt, which also holds what the windows share.
const CLIENT_FILE: &str = "client.opt";
const KEYS_FILE: &str = "keys.opt";

/// This window's place among the client's windows open (`multibox`), from 0.
fn place() -> usize {
    crate::multibox::place()
}

/// The file a window keeps its own `base` options in: window 1's
/// `<base>.opt`, as the client always kept them; window n's `<base>-<n>.opt`.
fn own_file(base: &str, place: usize) -> String {
    if place == 0 { format!("{base}.opt") } else { format!("{base}-{}.opt", place + 1) }
}

/// game.opt as this window keeps it, the whole record, when the game would
/// load it (`from_card`): its own file; a later window that has kept none yet
/// reads window 1's, its saved account left out (`0x10..0x91`: two windows
/// cannot play one account at once).
fn game_card() -> Option<Vec<u8>> {
    let me = place();
    let loads = |b: Vec<u8>| GameOptions::from_card(&b).is_some().then_some(b);
    match store::read(&own_file("game", me)) {
        Some(b) => loads(b),
        None if me > 0 => {
            let mut b = store::read(GAME_FILE).and_then(loads)?;
            b[0x10..0x91].fill(0);
            Some(b)
        }
        None => None,
    }
}

/// Writes this window's game.opt.
fn write_game(card: &[u8]) -> Result<(), String> {
    store::write(&own_file("game", place()), card)
}

impl GameOptions {
    /// `VIGameOptions` as `Save` writes it: `m_flags` 0, the settings, the
    /// magic at +0x3FC; the login, `m_x90`, `m_region` (the disc's, set by
    /// `Load`), the home shard and the padding zero.
    pub fn to_card(self) -> [u8; CARD_SIZE] {
        let mut b = [0u8; CARD_SIZE];
        b[0x04] = self.battle;
        b[0x05] = self.stereo;
        b[0x06] = self.follow;
        b[0x07] = self.inverted;
        b[0x08..0x0C].copy_from_slice(&self.fx_volume.0.to_le_bytes());
        b[0x0C..0x10].copy_from_slice(&self.music_volume.0.to_le_bytes());
        b[0x91] = self.compact;
        b[0x92] = self.no_damage;
        b[0x93] = self.no_faction;
        b[0x94] = self.no_loot;
        b[0x95] = self.auto_attack;
        b[0x97] = self.vibration;
        b[0x3FC..].copy_from_slice(&MAGIC.to_le_bytes());
        b
    }

    /// `VIGameOptions::Load`'s test of what the card held: shorter than the
    /// record, `m_flags` set or the magic wrong is `None` (the defaults).
    /// The login is never read. A volume outside 0..1 (the game's own are
    /// never) reads as 1: the client's guard against a damaged file, which
    /// the game does not need.
    pub fn from_card(b: &[u8]) -> Option<GameOptions> {
        if b.len() < CARD_SIZE {
            return None;
        }
        let word = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        if word(0) != 0 || word(0x3FC) != MAGIC {
            return None;
        }
        let volume = |o: usize| {
            let v = f32::from_bits(word(o));
            F::<Ee>::new(if (0.0..=1.0).contains(&v) { v } else { 1.0 })
        };
        Some(GameOptions {
            battle: b[0x04],
            stereo: b[0x05],
            follow: b[0x06],
            inverted: b[0x07],
            fx_volume: volume(0x08),
            music_volume: volume(0x0C),
            compact: b[0x91],
            no_damage: b[0x92],
            no_faction: b[0x93],
            no_loot: b[0x94],
            auto_attack: b[0x95],
            vibration: b[0x97],
        })
    }
}

/// The options as `VIWndInitFromMC` loads them before the world: the
/// card's, else `SetDefaults`' with the client's own Soft Keyboard default
/// (Compact: a keyboard is at hand), which `EQOA_SOFT_KEYBOARD` turns off
/// whatever was kept (a test's).
pub fn game_options() -> GameOptions {
    let mut o = game_card().and_then(|b| GameOptions::from_card(&b)).unwrap_or(GameOptions { compact: 1, ..Default::default() });
    if std::env::var_os("EQOA_SOFT_KEYBOARD").is_some() {
        o.compact = 0;
    }
    o
}

/// Save Changes (`SaveOptions`: `VIGetGameOptions()->Save()`), with the
/// client's own settings beside the game's: this window's (`own_file`), and
/// the keys and buttons when its Controls page changed them. Whether all were
/// kept; a failure is said and play goes on (the game's `m_saveFailed`).
pub fn save(o: &GameOptions) -> bool {
    let mut card = with_login(o.to_card());
    // the home shard the server list keeps there stays (`set_home_shard`)
    if let Some(b) = game_card() {
        card[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN].copy_from_slice(&b[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN]);
    }
    let game = write_game(&card);
    let client = save_client();
    let keys = save_binds();
    for (what, r) in [("the options", &game), ("the client's options", &client), ("the keys and buttons", &keys)] {
        if let Err(e) = r {
            crate::say(&format!("{what} were not kept: {e}"));
        }
    }
    game.is_ok() && client.is_ok() && keys.is_ok()
}

/// The login STATION's Save Account kept (`VIGameOptions::m_username`,
/// `m_password`, `m_x90`), from the card's whole record (`Load`): empty and
/// off when there is none.
pub fn saved_login() -> (String, String, bool) {
    let Some(b) = game_card() else {
        return (String::new(), String::new(), false);
    };
    let text = |r: std::ops::Range<usize>| b[r].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
    (text(0x10..0x50), text(0x50..0x90), b[0x90] != 0)
}

/// `card` with the login kept on the card put back (`to_card` writes none).
fn with_login(mut card: [u8; CARD_SIZE]) -> [u8; CARD_SIZE] {
    if let Some(b) = game_card() {
        card[0x10..0x91].copy_from_slice(&b[0x10..0x91]);
    }
    card
}

/// STATION's `SaveAccount` and the `Save()` three frames on: the login
/// (`SetUsername`, `SetPassword`, 63 characters and the terminator each) and
/// the flag into the options, the rest of the record as it was (the
/// defaults' when there was none). Whether it was kept.
pub fn save_login(user: &str, password: &str, on: bool) -> bool {
    let mut card = game_card().and_then(|b| <[u8; CARD_SIZE]>::try_from(&b[..CARD_SIZE]).ok()).unwrap_or_else(|| game_options().to_card());
    for (at, s) in [(0x10, user), (0x50, password)] {
        card[at..at + 0x40].fill(0);
        for (k, c) in s.chars().take(0x3F).enumerate() {
            card[at + k] = if (c as u32) < 256 { c as u8 } else { b'?' };
        }
    }
    card[0x90] = on as u8;
    match write_game(&card) {
        Ok(()) => true,
        Err(e) => {
            crate::say(&format!("the saved account was not kept: {e}"));
            false
        }
    }
}

// ---- game.opt's home shard (`VIGameOptions::m_homeShard`) ---------------------------

/// `m_homeShard[64]` at +0x98: the server list's last pick, UTF-16.
const HOME_SHARD: usize = 0x98;
const HOME_SHARD_LEN: usize = 128;

/// `HomeShardName()`: the shard the server list picked last, as game.opt
/// keeps it (none when game.opt is not there or not the game's).
pub fn home_shard() -> Option<String> {
    let b = game_card()?;
    let w: Vec<u16> = b[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
    (!w.is_empty()).then(|| String::from_utf16_lossy(&w))
}

/// `SetHomeShardName(name)` and `Save()` (`VIWndServerSelect::Connect`):
/// the name into game.opt (63 characters at most), the rest of it as kept,
/// or the options as they stand when there is none.
pub fn set_home_shard(name: &str) -> Result<(), String> {
    let mut b = game_card().unwrap_or_else(|| game_options().to_card().to_vec());
    let w: Vec<u16> = name.encode_utf16().take(HOME_SHARD_LEN / 2 - 1).collect();
    b[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN].fill(0);
    for (k, c) in w.iter().enumerate() {
        b[HOME_SHARD + 2 * k..HOME_SHARD + 2 * k + 2].copy_from_slice(&c.to_le_bytes());
    }
    write_game(&b)
}

// ---- the client's own: the server, as the title's Network Setup keeps it ---------

const SERVER_FILE: &str = "server.opt";

/// The server the title's Network Setup keeps (`server.opt`: the address
/// alone, a host name or IPv4 address), if one was set.
pub fn server() -> Option<String> {
    store::read(SERVER_FILE).map(|b| String::from_utf8_lossy(&b).trim().to_string()).filter(|s| !s.is_empty())
}

/// Keeps `s` as the server (blank: none).
pub fn set_server(s: &str) -> Result<(), String> {
    store::write(SERVER_FILE, s.trim().as_bytes())
}

// ---- the client's own: the disc image, as the player picked it (natively) --------

const DISC_FILE: &str = "disc.opt";

/// The disc image the player picked last (`disc.opt`: its path alone, on a
/// line of its own), if one was (`discs`).
pub fn disc() -> Option<String> {
    store::read(DISC_FILE).map(|b| String::from_utf8_lossy(&b).trim_end_matches(['\r', '\n']).to_string()).filter(|s| !s.is_empty())
}

/// Keeps `path` as the disc image.
pub fn set_disc(path: &str) -> Result<(), String> {
    store::write(DISC_FILE, format!("{path}\n").as_bytes())
}

// ---- the client's own: the game's save icon off the player's disc (natively) -----

const ICON_FILE: &str = "saveicon.png";
const ICON_KEY_FILE: &str = "saveicon.key";

/// The game's save icon as `icon` drew it off a disc (`saveicon.png`), if the
/// disc it came from (`saveicon.key`) is `disc`.
#[cfg(not(target_arch = "wasm32"))]
pub fn kept_icon(disc: &str) -> Option<Vec<u8>> {
    let key = store::read(ICON_KEY_FILE)?;
    if String::from_utf8_lossy(&key).trim_end_matches(['\r', '\n']) != disc {
        return None;
    }
    store::read(ICON_FILE)
}

/// Keeps the save icon drawn off `disc` (a PNG).
#[cfg(not(target_arch = "wasm32"))]
pub fn keep_icon(disc: &str, png: &[u8]) -> Result<(), String> {
    store::write(ICON_FILE, png)?;
    store::write(ICON_KEY_FILE, format!("{disc}\n").as_bytes())
}

// ---- the client's own: the 2002 disc (natively), for F9's art (`era`) ------------

const DISC_2002_FILE: &str = "disc2002.opt";

/// The 2002 disc image the player picked for its art (`disc2002.opt`: its
/// path alone, on a line of its own), if one was (`era`).
pub fn disc_2002() -> Option<String> {
    store::read(DISC_2002_FILE).map(|b| String::from_utf8_lossy(&b).trim_end_matches(['\r', '\n']).to_string()).filter(|s| !s.is_empty())
}

/// Keeps `path` as the 2002 disc image.
pub fn set_disc_2002(path: &str) -> Result<(), String> {
    store::write(DISC_2002_FILE, format!("{path}\n").as_bytes())
}

// ---- the client's own: the Controls page ----------------------------------------

thread_local! {
    /// The keys and buttons now (`binds`): read from the store the first
    /// time they are asked for.
    static BINDS: std::cell::RefCell<Option<crate::binds::Binds>> = const { std::cell::RefCell::new(None) };
    /// The keys and buttons as keys.opt had them when this window last read
    /// or wrote it (`save_binds`).
    static BINDS_KEPT: std::cell::RefCell<Option<crate::binds::Binds>> = const { std::cell::RefCell::new(None) };
}

/// `f` over the keys and buttons now: what was kept (`keys.opt`), as the
/// Controls page changed them since.
pub fn with_binds<R>(f: impl FnOnce(&crate::binds::Binds) -> R) -> R {
    BINDS.with(|c| {
        let mut c = c.borrow_mut();
        let b = c.get_or_insert_with(|| {
            let b = store::read(KEYS_FILE).map_or_else(Default::default, |b| crate::binds::Binds::from_text(&String::from_utf8_lossy(&b)));
            if !b.is_default() {
                crate::say(&format!("keys and buttons: {}", b.describe()));
            }
            BINDS_KEPT.with(|k| *k.borrow_mut() = Some(b.clone()));
            b
        });
        f(b)
    })
}

/// Save Changes' half for keys.opt: written only when this window's keys and
/// buttons are not what it last read or wrote there, so a window saving its
/// sound never puts back the keys it read at its start over the ones another
/// window saved since.
fn save_binds() -> Result<(), String> {
    let now = binds();
    if BINDS_KEPT.with(|k| k.borrow().as_ref() == Some(&now)) {
        return Ok(());
    }
    store::write(KEYS_FILE, now.to_text().as_bytes())?;
    BINDS_KEPT.with(|k| *k.borrow_mut() = Some(now));
    Ok(())
}

/// The keys and buttons now, a copy.
pub fn binds() -> crate::binds::Binds {
    with_binds(Clone::clone)
}

/// Changes them (the Controls page, as it moves them); kept only by Save
/// Changes (`save`).
pub fn set_binds(b: crate::binds::Binds) {
    BINDS.with(|c| *c.borrow_mut() = Some(b));
}

// ---- the client's own: the Extended page ---------------------------------------

/// The Extended page's Brightness: steps 0 to 10, 5 leaving the picture as
/// it is drawn.
pub const BRIGHTNESS_STEPS: u8 = 10;
pub const BRIGHTNESS_AS_DRAWN: u8 = 5;

/// The Extended page's Camera Distance: the follow orbit's zoom-out ceiling
/// (`camstyle`), from the game's 8 to 50 in steps of 3 (15 places on the
/// slider, the client's 11 among them, and its default).
pub const CAMERA_MIN: u8 = 8;
pub const CAMERA_MAX: u8 = 50;
pub const CAMERA_STEP: u8 = 3;

/// The Extended page's Camera Turn Speed, in percent of the game's own
/// (`camstyle::turn_speed`): from half to three times it in quarters (11
/// places on the slider), the game's own the default.
pub const TURN_MIN: u16 = 50;
pub const TURN_MAX: u16 = 300;
pub const TURN_STEP: u16 = 25;
pub const TURN_GAME: u16 = 100;

/// The Extended page's Frame Rate: a cap on the frames drawn, or none (the
/// display's rate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameRate {
    Thirty,
    Sixty,
    Display,
}

impl FrameRate {
    /// The row's order, which left and right step through (and round).
    pub const ALL: [FrameRate; 3] = [FrameRate::Thirty, FrameRate::Sixty, FrameRate::Display];

    /// The time between two frames it keeps, in ms (0: every frame the
    /// display shows). 30 is the game's own frame (`frames::FRAME_MS`), so
    /// that under it every frame drawn is one of the game's: at the 33 ms a
    /// page on WebGL2 kept from 2026-10-20 the frames drawn crept ahead of the
    /// game's and every few seconds one was not, the HUD then holding a frame
    /// (F3 20261026-030902: a list's pointer pulsing unevenly).
    pub fn gap_ms(self) -> f64 {
        match self {
            FrameRate::Thirty => crate::frames::FRAME_MS,
            FrameRate::Sixty => 1000.0 / 60.0,
            FrameRate::Display => 0.0,
        }
    }

    fn key(self) -> &'static str {
        match self {
            FrameRate::Thirty => "30",
            FrameRate::Sixty => "60",
            FrameRate::Display => "display",
        }
    }
}

/// The Extended page's Controller Focus: which controllers a window of the
/// client reads (`input::Gamepads`), kept for each window by the place it
/// takes among the client's windows open on the computer (`multibox`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    /// Alone, every controller, as the client always read them; with other
    /// windows of the client open (multiboxing), one each: the first
    /// window's the first controller, the second's the next, and so on,
    /// each read whichever window is in front.
    #[default]
    Auto,
    /// Every controller, while this window is the one in front: one
    /// controller for several windows, following the front one.
    Window,
    /// That controller alone (1 to 4, a group's size), in front or not;
    /// several windows on the same one are all driven by it.
    Pad(u8),
    /// None: the keyboard, the mouse and the touch screen only.
    Off,
}

impl Focus {
    /// The row's order, which left and right step through (and round).
    pub const ALL: [Focus; 7] = [Focus::Auto, Focus::Window, Focus::Pad(1), Focus::Pad(2), Focus::Pad(3), Focus::Pad(4), Focus::Off];

    /// As `client.opt` spells it.
    pub fn key(self) -> String {
        match self {
            Focus::Auto => "auto".into(),
            Focus::Window => "window".into(),
            Focus::Pad(n) => format!("pad{n}"),
            Focus::Off => "off".into(),
        }
    }

    pub fn from_key(s: &str) -> Option<Focus> {
        Focus::ALL.into_iter().find(|f| f.key() == s)
    }
}

/// The most bytes of an output device's name Output Device keeps.
pub const DEVICE_NAME_MAX: usize = 200;

/// An output device as the system names it (Output Device, `audio`), held in
/// place so the settings stay `Copy`: at most `DEVICE_NAME_MAX` bytes, a
/// longer name cut at a character, and every name compared by its cut
/// (`same`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceName {
    len: u8,
    bytes: [u8; DEVICE_NAME_MAX],
}

impl DeviceName {
    pub fn new(name: &str) -> DeviceName {
        let mut end = name.len().min(DEVICE_NAME_MAX);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        let mut bytes = [0; DEVICE_NAME_MAX];
        bytes[..end].copy_from_slice(&name.as_bytes()[..end]);
        DeviceName { len: end as u8, bytes }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    /// Whether the system's `name` is this device (by the cut both keep).
    pub fn same(&self, name: &str) -> bool {
        *self == DeviceName::new(name)
    }
}

impl std::fmt::Debug for DeviceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

/// The client's own settings: the Options window's Extended page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientOptions {
    /// Level of Detail: the game's levels (`VILODSprite::Raster`) while set;
    /// off, every LOD body draws its near level out to its off distance
    pub lod: bool,
    /// Brightness, 0 to `BRIGHTNESS_STEPS` (`curve`)
    pub brightness: u8,
    /// Fog, 0 (off) to `fog::STEPS`, the game's at `fog::GAME` (`fog`)
    pub fog: u8,
    /// Frame Rate as chosen; `None` until it is, the platform's
    /// (`set_platform_rate`: 30 on a page's WebGL2, else the display's)
    pub frame_rate: Option<FrameRate>,
    /// Camera Distance: the follow orbit's ceiling, `CAMERA_MIN` to
    /// `CAMERA_MAX`
    pub camera_distance: u8,
    /// Camera Turn Speed: percent of the game's, `TURN_MIN` to `TURN_MAX`
    pub camera_turn: u16,
    /// Screen Shake: the camera shaken as the game shakes it (the spell
    /// effects' shake events, `camstyle::screen_shake`), or never
    pub screen_shake: bool,
    /// Controller Focus, each window's by its place (`focus`)
    pub controller: [Focus; crate::multibox::PLACES],
    /// Capture Controller, one setting for every window: a Pad that several
    /// windows open share is read only by the one in front (`input::captured`)
    pub capture: bool,
    /// Sound in Background, one setting for every window: on, a window
    /// behind others keeps its sound; off, a window not in front is heard
    /// not at all (`audio::set_heard`), its game's sound running on
    pub background_sound: bool,
    /// Ability Bars: the HUD's ability bars and the spell book's slots, the
    /// game's two (`ABILITY_BARS_GAME`) or up to `hudmenu::MAX_BARS`, five
    /// slots each (`hudmenu`)
    pub ability_bars: u8,
    /// NPC/PC Display: how many NPCs and players (the player's own among
    /// them) the client keeps in view, the game's 24 (`ENTITY_SLOTS_GAME`)
    /// up to `ENTITY_SLOTS_MAX`, on a server that offers more (`net::entity`)
    pub entity_slots: u8,
    /// NPC/PC Distance: how far NPCs and players are drawn, the game's
    /// 100 (`ENTITY_VIEW_GAME`: a player race's body to 100, any other to
    /// its own off distance) up to `ENTITY_VIEW_MAX`, each at least as far as
    /// the game draws it (`lod::view_dist`); a server that offers more looks
    /// for them that far
    pub entity_view: u16,
    /// Name Distance: how far from where the game's camera could be
    /// (`lod::eye`) a character's name plate is drawn, the game's 22
    /// (`NAME_DISTANCE_GAME`: `VINameSprite::Raster`) up to
    /// `NAME_DISTANCE_MAX` (`play::labels`)
    pub name_distance: u8,
    /// Overlapping Names (F3 20261025-025611: a crowd within Name Distance
    /// showed five names of twenty-four): false, Hide, the game's
    /// `VINameSprite::RasterDeferred` (a plate whose rect meets a nearer
    /// one's is dropped); true, Show, every plate kept, the nearest drawn
    /// over the rest (`play::labels`)
    pub overlapping_names: bool,
    /// Art: Frontiers' (the disc the game runs off), or the 2002 disc's
    /// (`era`, F9)
    pub art: esf::Era,
    /// VSync, natively (F3 20261024-065802: tearing): on, a frame is shown
    /// only at the display's refresh (`wgpu::PresentMode::Fifo`), so none
    /// tears; off, at once (`Immediate` where the surface has it, else
    /// `Mailbox`), and with Frame Rate the display's the loop keeps to the
    /// display's refresh itself (`frame_wait`). A page's frames are the
    /// browser's to pace: there it is always on.
    pub vsync: bool,
    /// Shouts heard: the game's Restore Shouts (true) and Ignore Shouts
    /// (false), the quick chat's (DoAction 0x0B030003, `SendFlag`), kept by
    /// the client as soon as they are chosen (`set_shouts`; the game keeps
    /// none, and the server forgets at the next login): ignored, the server
    /// is told again at each login (`Play::tell_shouts`) and any shout it
    /// still sends is dropped (`chat::is_shout`). Each window its own.
    pub shouts: bool,
    /// Update Notices, one setting for every window: on, the game asks in
    /// its own window to restart into a newer build once one is in place
    /// (`update::Offer`); off, it never asks, and the newer build starts the
    /// next time the game does (`update`'s check before the title, as ever).
    /// A page's Reload notice the same (`web/main.js`).
    pub update_notices: bool,
    /// Mipmaps: on, the textures' mip levels sampled as the GS samples them
    /// (`render::gs_lod_k`: the level by depth, blended), which is how the
    /// game is drawn; off, every texel off the base level, sharper and
    /// shimmering in the distance (`render::set_mipmaps`).
    pub mipmaps: bool,
    /// Flora: on, the radial flora the game grows round the camera out of
    /// each zone's tables (`vi::flora`, `flora`), as it is drawn; off, none.
    pub flora: bool,
    /// Shadows: on, the characters' blob shadows as the game draws them
    /// (`vi::shadow`, `shadows`: within 50 of the eye, on the ground and the
    /// props under each); off, none.
    pub shadows: bool,
    /// Environment: on, the effect volumes placed in the world (ground fog,
    /// dust, falling leaves, light shafts, the flocks of birds: `fx`), as the
    /// game draws them; off, none.
    pub environment: bool,
    /// Loading Screens: Longer, a long trip (Return Home, a coach, a port of
    /// 200 or more, another world) keeps the loading screen up at least a
    /// second, as a PS2's disc read would (`loading::FAR_HOLD_MS`); Instant,
    /// it comes down as soon as the place is in (the user, 2026-10-25: "so
    /// if players want the instant or the longer loading screens").
    pub loading_longer: bool,
    /// Text: how the game's font is drawn scaled to the window, wherever it
    /// is drawn (`render::ui::TextFilter`): Sharp, crisp, or Smooth, the
    /// PS2's own filtering (F3 20261024-200725: "smooth out the text as per
    /// live").
    pub text: render::ui::TextFilter,
    /// Render Scale (`render::scale`; the user, 2026-10-27: "render scale
    /// x2, x4"): the whole frame, the world and the 2D layer over it, drawn 1
    /// (as ever), 2 or 4 times the window's size each way and averaged down to
    /// it, which smooths every edge, cut-out and thin thing and changes
    /// nothing else; at less where the window is too large for it
    /// (`render::scale::fit`).
    pub render_scale: u8,
    /// HUD: how the 2D layer's textures other than the fonts are drawn scaled
    /// to the window (`render::ui::HudFilter`): Nearest, as the GS samples
    /// them, by default (the user, 2026-10-28: "make nearest the default ...
    /// since there is a cost": the pregame's screens are this art from edge to
    /// edge, F3 20261027-213115); or Sharp, every texel as wide as every other,
    /// as Text's Sharp draws a font (the user, 2026-10-27: "add the sharp
    /// textures fix like sharp font").
    pub hud: render::ui::HudFilter,
    /// Output Device, natively (F3 20261024-201433: the game played into
    /// the system's default output, a television, while the player listened
    /// elsewhere): none, the system's default, followed when it changes; or
    /// a device by its name, the default while it is not connected
    /// (`audio::retarget`). Each window its own.
    pub output: Option<DeviceName>,
}

/// Render Scale's steps, the first the game's picture as drawn.
pub const RENDER_SCALES: [u8; 3] = [1, 2, 4];

/// The game's ability bars (with the expansion's second).
pub const ABILITY_BARS_GAME: u8 = 2;
/// The game's entity slots (CLIENT's `m_entities[24]`), the most a state
/// channel can carry, and NPC/PC Display's step.
pub const ENTITY_SLOTS_GAME: u8 = net::entity::RETAIL_SLOTS as u8;
pub const ENTITY_SLOTS_MAX: u8 = net::entity::MAX_SLOTS as u8;
pub const ENTITY_SLOTS_STEP: u8 = 8;
/// The game's character draw distance (a player race's LOD body's last
/// level), the farthest NPC/PC Distance goes (the client reads the
/// world's zones to 1050 round the player), and its step.
pub const ENTITY_VIEW_GAME: u16 = 100;
pub const ENTITY_VIEW_MAX: u16 = 1000;
pub const ENTITY_VIEW_STEP: u16 = 50;
/// The game's name plate distance (`VINameSprite::Raster`: a plate within 22
/// of the camera), the farthest Name Distance goes (twice the game's
/// character draw distance: past it a name is a few pixels at any zoom), and
/// its step past the game's (30, 40 .. 200).
pub const NAME_DISTANCE_GAME: u8 = 22;
pub const NAME_DISTANCE_MAX: u8 = 200;
pub const NAME_DISTANCE_STEP: u8 = 10;

/// Name Distance moved a step from `v`: right (`d` > 0) on to the next ten
/// past it (the game's 22 to 30), left to the ten before it, or the game's
/// 22 from 30 (a number between them, one client.opt held, to the one past
/// it); None at either end.
pub fn step_name_distance(v: u8, d: i32) -> Option<u8> {
    let (lo, hi, by) = (NAME_DISTANCE_GAME as i32, NAME_DISTANCE_MAX as i32, NAME_DISTANCE_STEP as i32);
    let v = v as i32;
    if (d < 0 && v <= lo) || (d > 0 && v >= hi) || d == 0 {
        return None;
    }
    let to = if d > 0 { (v / by + 1) * by } else { (v - 1).div_euclid(by) * by };
    Some(to.clamp(lo, hi) as u8)
}

impl Default for ClientOptions {
    fn default() -> Self {
        ClientOptions {
            lod: true,
            brightness: BRIGHTNESS_AS_DRAWN,
            fog: crate::fog::GAME,
            frame_rate: None,
            camera_distance: crate::camstyle::FOLLOW_MAX_DISTANCE as u8,
            camera_turn: TURN_GAME,
            screen_shake: true,
            controller: [Focus::Auto; crate::multibox::PLACES],
            capture: false,
            background_sound: true,
            ability_bars: ABILITY_BARS_GAME,
            entity_slots: ENTITY_SLOTS_GAME,
            entity_view: ENTITY_VIEW_GAME,
            name_distance: NAME_DISTANCE_GAME,
            overlapping_names: false,
            art: esf::Era::Frontiers,
            vsync: true,
            shouts: true,
            update_notices: true,
            text: render::ui::TextFilter::Sharp,
            mipmaps: true,
            flora: true,
            shadows: true,
            environment: true,
            loading_longer: true,
            render_scale: 1,
            hud: render::ui::HudFilter::Nearest,
            output: None,
        }
    }
}

impl ClientOptions {
    /// `client.opt`: a line a setting, `name=value`; Camera Turn Speed and
    /// Fog only when they are not the game's, Screen Shake only when off, a
    /// window's Controller Focus as `controller.<window>` (from 1) only when
    /// it is not Auto, Capture Controller only when on, Sound in Background
    /// only when off, Ability Bars, NPC/PC Display, NPC/PC Distance and
    /// Name Distance only when not the game's, Overlapping Names only when
    /// Show, Art only when it is the 2002 disc's, VSync, Shouts and Update
    /// Notices only when off, Text only when Smooth, Render Scale only above
    /// 1, HUD only when Nearest, and Output Device only when a device is
    /// chosen (by its name).
    pub fn to_text(self) -> String {
        let mut s = format!("lod={}\nbrightness={}\ncamera_distance={}\n", if self.lod { "on" } else { "off" }, self.brightness, self.camera_distance);
        if let Some(r) = self.frame_rate {
            s.push_str(&format!("frame_rate={}\n", r.key()));
        }
        if self.camera_turn != TURN_GAME {
            s.push_str(&format!("camera_turn={}\n", self.camera_turn));
        }
        if self.fog != crate::fog::GAME {
            s.push_str(&format!("fog={}\n", self.fog));
        }
        if !self.screen_shake {
            s.push_str("screen_shake=off\n");
        }
        for (k, f) in self.controller.iter().enumerate().filter(|(_, f)| **f != Focus::Auto) {
            s.push_str(&format!("controller.{}={}\n", k + 1, f.key()));
        }
        if self.capture {
            s.push_str("capture_controller=on\n");
        }
        if !self.background_sound {
            s.push_str("background_sound=off\n");
        }
        if self.ability_bars != ABILITY_BARS_GAME {
            s.push_str(&format!("ability_bars={}\n", self.ability_bars));
        }
        if self.entity_slots != ENTITY_SLOTS_GAME {
            s.push_str(&format!("entity_slots={}\n", self.entity_slots));
        }
        if self.entity_view != ENTITY_VIEW_GAME {
            s.push_str(&format!("entity_view={}\n", self.entity_view));
        }
        if self.name_distance != NAME_DISTANCE_GAME {
            s.push_str(&format!("name_distance={}\n", self.name_distance));
        }
        if self.overlapping_names {
            s.push_str("overlapping_names=show\n");
        }
        if self.art == esf::Era::Y2002 {
            s.push_str("art=2002\n");
        }
        if !self.vsync {
            s.push_str("vsync=off\n");
        }
        if !self.shouts {
            s.push_str("shouts=off\n");
        }
        if !self.update_notices {
            s.push_str("update_notices=off\n");
        }
        if self.text == render::ui::TextFilter::Smooth {
            s.push_str("text=smooth\n");
        }
        // Mipmaps only when off
        if !self.mipmaps {
            s.push_str("mipmaps=off\n");
        }
        // Flora and Environment only when off
        if !self.flora {
            s.push_str("flora=off\n");
        }
        // Shadows only when off
        if !self.shadows {
            s.push_str("shadows=off\n");
        }
        if !self.environment {
            s.push_str("environment=off\n");
        }
        // Loading Screens only when Instant
        if !self.loading_longer {
            s.push_str("loading=instant\n");
        }
        // Render Scale only above 1, HUD only when Sharp
        if self.render_scale != 1 {
            s.push_str(&format!("render_scale={}\n", self.render_scale));
        }
        if self.hud == render::ui::HudFilter::Sharp {
            s.push_str("hud=sharp\n");
        }
        if let Some(d) = self.output {
            s.push_str(&format!("output={}\n", d.as_str()));
        }
        s
    }

    /// The settings `to_text` wrote; a line not understood, or a value out
    /// of range, leaves that setting at its default.
    pub fn from_text(s: &str) -> ClientOptions {
        let mut c = ClientOptions::default();
        for line in s.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match (k.trim(), v.trim()) {
                ("lod", "on") => c.lod = true,
                ("lod", "off") => c.lod = false,
                ("brightness", v) => {
                    if let Some(b) = v.parse::<u8>().ok().filter(|&b| b <= BRIGHTNESS_STEPS) {
                        c.brightness = b;
                    }
                }
                ("frame_rate", v) => c.frame_rate = FrameRate::ALL.into_iter().find(|r| r.key() == v).or(c.frame_rate),
                ("camera_distance", v) => {
                    if let Some(d) = v.parse::<u8>().ok().filter(|d| (CAMERA_MIN..=CAMERA_MAX).contains(d)) {
                        c.camera_distance = d;
                    }
                }
                ("camera_turn", v) => {
                    if let Some(t) = v.parse::<u16>().ok().filter(|t| (TURN_MIN..=TURN_MAX).contains(t)) {
                        c.camera_turn = t;
                    }
                }
                ("fog", v) => {
                    if let Some(f) = v.parse::<u8>().ok().filter(|&f| f <= crate::fog::STEPS) {
                        c.fog = f;
                    }
                }
                ("screen_shake", "on") => c.screen_shake = true,
                ("screen_shake", "off") => c.screen_shake = false,
                ("capture_controller", "on") => c.capture = true,
                ("capture_controller", "off") => c.capture = false,
                ("art", "2002") => c.art = esf::Era::Y2002,
                ("art", "frontiers") => c.art = esf::Era::Frontiers,
                ("background_sound", "on") => c.background_sound = true,
                ("background_sound", "off") => c.background_sound = false,
                ("vsync", "on") => c.vsync = true,
                ("vsync", "off") => c.vsync = false,
                ("shouts", "on") => c.shouts = true,
                ("shouts", "off") => c.shouts = false,
                ("update_notices", "on") => c.update_notices = true,
                ("update_notices", "off") => c.update_notices = false,
                ("overlapping_names", "show") => c.overlapping_names = true,
                ("overlapping_names", "hide") => c.overlapping_names = false,
                ("text", "sharp") => c.text = render::ui::TextFilter::Sharp,
                ("text", "smooth") => c.text = render::ui::TextFilter::Smooth,
                ("mipmaps", "on") => c.mipmaps = true,
                ("mipmaps", "off") => c.mipmaps = false,
                ("flora", "on") => c.flora = true,
                ("flora", "off") => c.flora = false,
                ("shadows", "on") => c.shadows = true,
                ("shadows", "off") => c.shadows = false,
                ("environment", "on") => c.environment = true,
                ("environment", "off") => c.environment = false,
                ("loading", "longer") => c.loading_longer = true,
                ("loading", "instant") => c.loading_longer = false,
                ("render_scale", v) => {
                    if let Some(k) = v.parse::<u8>().ok().filter(|k| RENDER_SCALES.contains(k)) {
                        c.render_scale = k;
                    }
                }
                ("hud", "nearest") => c.hud = render::ui::HudFilter::Nearest,
                ("hud", "sharp") => c.hud = render::ui::HudFilter::Sharp,
                ("output", v) if !v.is_empty() => c.output = Some(DeviceName::new(v)),
                ("ability_bars", v) => {
                    if let Some(n) = v.parse::<u8>().ok().filter(|&n| (ABILITY_BARS_GAME as i32..=crate::hudmenu::MAX_BARS).contains(&(n as i32))) {
                        c.ability_bars = n;
                    }
                }
                ("entity_slots", v) => {
                    if let Some(n) = v.parse::<u8>().ok().filter(|n| (ENTITY_SLOTS_GAME..=ENTITY_SLOTS_MAX).contains(n)) {
                        c.entity_slots = n;
                    }
                }
                ("entity_view", v) => {
                    if let Some(d) = v.parse::<u16>().ok().filter(|d| (ENTITY_VIEW_GAME..=ENTITY_VIEW_MAX).contains(d)) {
                        c.entity_view = d;
                    }
                }
                ("name_distance", v) => {
                    if let Some(d) = v.parse::<u8>().ok().filter(|d| (NAME_DISTANCE_GAME..=NAME_DISTANCE_MAX).contains(d)) {
                        c.name_distance = d;
                    }
                }
                (k, v) if k.starts_with("controller.") => {
                    let window = k["controller.".len()..].parse::<usize>().ok().filter(|w| (1..=crate::multibox::PLACES).contains(w));
                    if let (Some(w), Some(f)) = (window, Focus::from_key(v)) {
                        c.controller[w - 1] = f;
                    }
                }
                _ => {}
            }
        }
        c
    }
}

impl ClientOptions {
    /// A later window's own file (`client-<n>.opt`): the Extended page's rows,
    /// the lines the windows share (Controller Focus, Capture Controller,
    /// Sound in Background, Update Notices) left to client.opt.
    fn rows_text(self) -> String {
        ClientOptions { controller: [Focus::Auto; crate::multibox::PLACES], capture: false, background_sound: true, update_notices: true, ..self }.to_text()
    }
}

/// client.opt as it stands: window 1's rows, and what the windows share.
fn read_shared() -> ClientOptions {
    store::read(CLIENT_FILE).map_or_else(ClientOptions::default, |b| ClientOptions::from_text(&String::from_utf8_lossy(&b)))
}

/// The client's settings as this window keeps them: client.opt, with a later
/// window's rows out of its own file when it has kept one (else window 1's,
/// which it starts from).
fn read_client() -> ClientOptions {
    let shared = read_shared();
    let me = place();
    match store::read(&own_file("client", me)).filter(|_| me > 0) {
        Some(b) => ClientOptions { controller: shared.controller, capture: shared.capture, background_sound: shared.background_sound, update_notices: shared.update_notices, ..ClientOptions::from_text(&String::from_utf8_lossy(&b)) },
        None => shared,
    }
}

/// Save Changes' half for the client's settings: window 1 writes client.opt
/// whole (its rows, every window's Controller Focus, Capture Controller); a
/// later window its rows to its own file, and its Controller Focus and
/// Capture Controller into client.opt, window 1's rows and the other windows'
/// own as the file has them now (another window may have saved its own since
/// this one read them).
fn save_client() -> Result<(), String> {
    let me = place();
    let mine = client();
    let mut shared = read_shared();
    shared.controller[me] = mine.controller[me];
    shared.capture = mine.capture;
    shared.background_sound = mine.background_sound;
    shared.update_notices = mine.update_notices;
    if me == 0 {
        store::write(CLIENT_FILE, ClientOptions { controller: shared.controller, ..mine }.to_text().as_bytes())?;
    } else {
        store::write(&own_file("client", me), mine.rows_text().as_bytes())?;
        store::write(CLIENT_FILE, shared.to_text().as_bytes())?;
    }
    FILE_SHARED.with(|f| f.set(Some((shared.capture, shared.background_sound, shared.update_notices))));
    Ok(())
}

thread_local! {
    /// The client's settings now (`client`) and the place they are this
    /// window's for: read from the store the first time they are asked for,
    /// and again if the place changed (a page learns its place a moment after
    /// it starts).
    static CLIENT: Cell<Option<(usize, ClientOptions)>> = const { Cell::new(None) };
    /// The frame rate when none was chosen (`set_platform_rate`).
    static PLATFORM_RATE: Cell<FrameRate> = const { Cell::new(FrameRate::Display) };
    /// When the next capped frame is due (`now_ms`'s clock).
    static FRAME_DUE: Cell<f64> = const { Cell::new(0.0) };
    /// The display's refresh, in ms (`set_display_refresh`).
    static DISPLAY_GAP: Cell<f64> = const { Cell::new(1000.0 / 60.0) };
    /// What the windows share as `client.opt` last had it (`look_shared`):
    /// Capture Controller, Sound in Background and Update Notices.
    static FILE_SHARED: Cell<Option<(bool, bool, bool)>> = const { Cell::new(None) };
}

/// The client's settings now: what this window kept (`read_client`), as the
/// page changed them since.
pub fn client() -> ClientOptions {
    let me = place();
    CLIENT.with(|c| match c.get() {
        Some((p, v)) if p == me => v,
        _ => {
            let v = read_client();
            c.set(Some((me, v)));
            FILE_SHARED.with(|f| f.set(Some((v.capture, v.background_sound, v.update_notices))));
            v
        }
    })
}

/// Changes them (the Extended page, as its rows move); kept only by Save
/// Changes (`save`).
pub fn set_client(v: ClientOptions) {
    let me = place();
    CLIENT.with(|c| c.set(Some((me, v))));
}

/// The frame rate when the player chose none: a page on WebGL2 keeps 30
/// (`web::gfx`), everything else the display's.
#[cfg(any(target_arch = "wasm32", test))]
pub fn set_platform_rate(r: FrameRate) {
    PLATFORM_RATE.with(|p| p.set(r));
}

/// The follow orbit's zoom-out ceiling now (Camera Distance).
pub fn camera_distance() -> f32 {
    client().camera_distance as f32
}

/// The camera's turn now, in percent of the game's (Camera Turn Speed).
pub fn camera_turn() -> u16 {
    client().camera_turn
}

/// Fog now, 0 (off) to `fog::STEPS`, the game's at `fog::GAME`.
pub fn fog() -> u8 {
    client().fog
}

/// Whether the camera shakes as the game shakes it (Screen Shake).
pub fn screen_shake() -> bool {
    client().screen_shake
}

/// VSync now: the player's natively, always on in a page (the browser
/// paces its frames).
pub fn vsync() -> bool {
    cfg!(target_arch = "wasm32") || client().vsync
}

/// The display's refresh, in ms (`set_display_refresh`; a page's is taken
/// as 60 Hz).
pub fn display_gap() -> f64 {
    DISPLAY_GAP.with(|g| g.get())
}

/// The display's refresh as the window's monitor reports it (mHz; 60 Hz when
/// it reports none), the gap the loop keeps with VSync off and Frame Rate the
/// display's.
pub fn set_display_refresh(millihertz: Option<u32>) {
    DISPLAY_GAP.with(|g| g.set(millihertz.filter(|&m| m >= 1000).map_or(1000.0 / 60.0, |m| 1_000_000.0 / m as f64)));
}

/// Capture Controller now: one setting for every window of the client.
pub fn capture() -> bool {
    client().capture
}

/// Sound in Background now: one setting for every window of the client.
pub fn background_sound() -> bool {
    client().background_sound
}

/// Ability Bars now: how many the HUD and the spell book show, with the
/// expansion's second bar (`Play::ability_bars`).
pub fn ability_bars() -> u8 {
    client().ability_bars
}

/// NPC/PC Display now: how many entity slots to keep where the server offers
/// more than the game's (`Play::agree_entity_view`).
pub fn entity_slots() -> usize {
    client().entity_slots as usize
}

/// NPC/PC Distance now: how far NPCs and players are drawn (`lod`), and
/// looked for where the server offers more (`Play::agree_entity_view`).
pub fn entity_view() -> u16 {
    client().entity_view
}

/// Name Distance now: how far from where the game's camera could be a name
/// plate is drawn (`play::PlateView`).
pub fn name_distance() -> f32 {
    client().name_distance as f32
}

/// Overlapping Names now: Show (true), every name plate kept, or Hide, the
/// game's rule (`play::PlateView`).
pub fn overlapping_names() -> bool {
    client().overlapping_names
}

/// What the windows share as another window saved it (Capture Controller,
/// Sound in Background, Update Notices): `client.opt`'s, each taken when it changed since
/// this window last looked (`multibox::seat`'s look, twice a second), so a
/// change saved in one window reaches the others while they run; this
/// window's own change, not saved yet, stands until then. The other settings
/// are each window's as it read them at its start.
pub fn look_shared() {
    let Some(b) = store::read(CLIENT_FILE) else { return };
    let file = ClientOptions::from_text(&String::from_utf8_lossy(&b));
    let kept = (file.capture, file.background_sound, file.update_notices);
    let _ = client();
    let Some(last) = FILE_SHARED.with(|f| f.replace(Some(kept))) else { return };
    let mut c = client();
    if last.0 != kept.0 {
        c.capture = kept.0;
        crate::say(&format!("controllers: Capture Controller {} (saved in another window)", if kept.0 { "on" } else { "off" }));
    }
    if last.1 != kept.1 {
        c.background_sound = kept.1;
        crate::say(&format!("sound: Sound in Background {} (saved in another window)", if kept.1 { "on" } else { "off" }));
    }
    if last.2 != kept.2 {
        c.update_notices = kept.2;
        crate::say(&format!("update: Update Notices {} (saved in another window)", if kept.2 { "on" } else { "off" }));
    }
    if last != kept {
        set_client(c);
    }
}

/// Art now: Frontiers' or the 2002 disc's (`era`).
pub fn art() -> esf::Era {
    client().art
}

/// Changes it and keeps it at once (F9 is a key, not a page's row that Save
/// Changes keeps): this window's client options written as they stand.
pub fn set_art(art: esf::Era) -> Result<(), String> {
    let mut c = client();
    c.art = art;
    set_client(c);
    save_client()
}

/// Whether shouts are heard: Restore Shouts, or Ignore Shouts kept
/// (`ClientOptions::shouts`).
pub fn shouts() -> bool {
    client().shouts
}

/// Ignore Shouts (false) or Restore Shouts (true), kept at once, as F9's
/// Art is (the quick chat's choice is no page's row that Save Changes
/// keeps): this window's client options written as they stand.
pub fn set_shouts(heard: bool) -> Result<(), String> {
    let mut c = client();
    c.shouts = heard;
    set_client(c);
    save_client()
}

/// Whether the game asks to restart into a newer build (Update Notices).
pub fn update_notices() -> bool {
    client().update_notices
}

/// Whether a long trip keeps the loading screen up a while (Loading
/// Screens: Longer), or it comes down at once (Instant).
pub fn loading_longer() -> bool {
    client().loading_longer
}

/// This window's Controller Focus: its place's (`multibox::place`).
pub fn focus() -> Focus {
    client().controller[place()]
}

/// Changes it (the Extended page, as its row moves); kept only by Save
/// Changes (`save`), beside the other windows' own.
pub fn set_focus(f: Focus) {
    let mut c = client();
    c.controller[place()] = f;
    set_client(c);
}

/// The Frame Rate in effect: the player's, else the platform's.
pub fn frame_rate() -> FrameRate {
    client().frame_rate.unwrap_or_else(|| PLATFORM_RATE.with(|p| p.get()))
}

/// The Brightness step's curve over a colour's 256 levels,
/// `(level / 255) ^ 2^((5 - step) / 5)`: `None` at 5 (the picture as drawn,
/// no pass at all), a square root at 10 (the darks lifted most, white kept
/// white) and a square at 0.
pub fn curve(step: u8) -> Option<[u8; 256]> {
    let step = step.min(BRIGHTNESS_STEPS);
    if step == BRIGHTNESS_AS_DRAWN {
        return None;
    }
    let e = 2f64.powf((BRIGHTNESS_AS_DRAWN as f64 - step as f64) / 5.0);
    let mut t = [0u8; 256];
    for (i, v) in t.iter_mut().enumerate() {
        *v = ((i as f64 / 255.0).powf(e) * 255.0).round() as u8;
    }
    Some(t)
}

/// The Extended page's settings in a line, and where they are kept (a
/// report's).
pub fn describe() -> String {
    let c = client();
    let rate = match frame_rate() {
        FrameRate::Thirty => "30",
        FrameRate::Sixty => "60",
        FrameRate::Display => "the display's",
    };
    format!(
        "level of detail {}, brightness {} of {BRIGHTNESS_STEPS}, fog {} of {}{}, frame rate {rate}{}, vsync {}, render scale x{}{}, camera distance {}, camera turn speed {}%, screen shake {}, controller focus {}, capture controller {}, sound in background {}, output device {}, ability bars {}, npc/pc display {}, npc/pc view distance {}, name distance {}, overlapping names {}, art {}, update notices {}, shouts {}, text {}, hud {}, mipmaps {}, flora {}, shadows {}, environment {}, loading screens {}; {}",
        if c.lod { "on" } else { "off" },
        c.brightness,
        c.fog,
        crate::fog::STEPS,
        if c.fog == crate::fog::GAME { " (the game's)" } else if c.fog == 0 { " (off)" } else { "" },
        if c.frame_rate.is_none() { " (not chosen)" } else { "" },
        if cfg!(target_arch = "wasm32") { "the browser's" } else if c.vsync { "on" } else { "off" },
        c.render_scale,
        match render::scale::would_draw(c.render_scale as u32) {
            k if k != c.render_scale as u32 => format!(" (drawn at x{k})"),
            _ => String::new(),
        },
        c.camera_distance,
        c.camera_turn,
        if c.screen_shake { "on" } else { "off" },
        focus().key(),
        if c.capture { "on" } else { "off" },
        if c.background_sound { "on" } else { "off" },
        c.output.map_or("the system default".to_string(), |d| format!("{:?}", d.as_str())),
        c.ability_bars,
        c.entity_slots,
        c.entity_view,
        c.name_distance,
        if c.overlapping_names { "show" } else { "hide" },
        match c.art {
            esf::Era::Frontiers => "frontiers",
            esf::Era::Y2002 => "2002",
        },
        if c.update_notices { "on" } else { "off" },
        if c.shouts { "heard" } else { "ignored (kept)" },
        match c.text {
            render::ui::TextFilter::Sharp => "sharp",
            render::ui::TextFilter::Smooth => "smooth",
        },
        match c.hud {
            render::ui::HudFilter::Nearest => "nearest",
            render::ui::HudFilter::Sharp => "sharp",
        },
        if c.mipmaps { "on" } else { "off" },
        if c.flora { "on" } else { "off" },
        if c.shadows { "on" } else { "off" },
        if c.environment { "on" } else { "off" },
        if c.loading_longer { "longer" } else { "instant" },
        kept_where()
    )
}

/// Where the settings are kept, for `describe`: a page's storage, or
/// natively the config directory (the home directory a report takes out),
/// and whose they are when this window is not the first.
fn kept_where() -> String {
    #[cfg(any(target_arch = "wasm32", test))]
    let at = (if store::available() { "kept in the page's storage" } else { "the page's storage refused: nothing is kept" }).to_string();
    #[cfg(all(not(target_arch = "wasm32"), not(test)))]
    let at = store::dir().map_or_else(|| "not kept: no config directory".to_string(), |d| format!("kept in {}", d.display()));
    match place() {
        0 => at,
        me => format!("{at}, as window {}'s own ({}, {})", me + 1, own_file("game", me), own_file("client", me)),
    }
}

// ---- the frame cap ---------------------------------------------------------------

/// Milliseconds on one clock since the first asked (`performance.now()`'s
/// in a page, through `web_time`).
fn now_ms() -> f64 {
    thread_local! {
        static START: web_time::Instant = web_time::Instant::now();
    }
    START.with(|s| s.elapsed().as_secs_f64() * 1000.0)
}

/// How early a frame may come and still count as on time: a frame's start
/// jitters, and the due times are lined up on the first frame drawn, which
/// may itself have come late (the 4 ms the 30 cap has always allowed).
const EARLY: f64 = 4.0;

/// The cap's rule: for a frame at `now` with the next due at `due` and the
/// gap `gap`, `None` if it is not drawn, else the due time after it. A frame
/// up to `EARLY` before its time is on time; due times step by the gap from
/// the last, so a 60 Hz display draws every other frame at 30 and a 144 Hz
/// one about every fifth, and on average a gap apart however the display's
/// frames fall; after a stall the next is a gap from now.
fn take(due: f64, now: f64, gap: f64) -> Option<f64> {
    if now + EARLY < due {
        return None;
    }
    Some(if now - due > gap { now + gap } else { due + gap })
}

/// The gap the frames keep, in ms (0: none): the Frame Rate's; with the
/// display's and VSync off, the display's refresh, so that a loop no present
/// paces never spins through thousands of frames a second.
fn frame_gap() -> f64 {
    match frame_rate().gap_ms() {
        0.0 if !vsync() => DISPLAY_GAP.with(|g| g.get()),
        g => g,
    }
}

/// Whether this frame is drawn under the Frame Rate: always without a cap;
/// with one, a frame before the next is due is skipped whole (a page skips
/// the animation frame, the game's clocks then seeing longer frames rather
/// than fewer ticks).
pub fn frame_due() -> bool {
    let gap = frame_gap();
    if gap == 0.0 {
        return true;
    }
    let now = now_ms();
    FRAME_DUE.with(|d| match take(d.get(), now, gap) {
        Some(next) => {
            d.set(next);
            true
        }
        None => false,
    })
}

/// Natively: how long the window's loop may sleep before the next capped
/// frame is due (`None`: it is due, or there is no cap), so a capped window
/// waits instead of asking for frames it would skip.
pub fn frame_wait() -> Option<std::time::Duration> {
    let gap = frame_gap();
    if gap == 0.0 {
        return None;
    }
    let wait = FRAME_DUE.with(|d| d.get()) - EARLY - now_ms();
    (wait > 0.0).then(|| std::time::Duration::from_secs_f64(wait / 1000.0))
}

// ---- where they are kept ---------------------------------------------------------

/// Natively, the directory the settings are kept in, if there is one
/// (`multibox` keeps the windows' places there too).
#[cfg(not(target_arch = "wasm32"))]
pub fn config_dir() -> Option<std::path::PathBuf> {
    store::dir()
}

/// Natively, the player's own config directory (`eqoa-rs` in the
/// platform's), whether this run keeps its settings there or not.
#[cfg(not(target_arch = "wasm32"))]
pub fn user_dir() -> Option<std::path::PathBuf> {
    store::user_dir()
}

/// `eqoa play` keeps its settings in the player's config directory; without
/// this (the test commands) only `$EQOA_CONFIG` is used. A page has only its
/// storage.
pub fn keep_in_user_dir() {
    #[cfg(not(target_arch = "wasm32"))]
    store::USER_DIR.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// The unit tests' store: in memory, a thread's own.
#[cfg(test)]
mod store {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        pub static FILES: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
        /// a store that refuses (a private window)
        pub static REFUSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    pub static USER_DIR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// The tests keep nothing on disk.
    pub fn dir() -> Option<std::path::PathBuf> {
        None
    }

    pub fn user_dir() -> Option<std::path::PathBuf> {
        None
    }

    pub fn read(name: &str) -> Option<Vec<u8>> {
        if REFUSE.with(|r| r.get()) {
            return None;
        }
        FILES.with(|f| f.borrow().get(name).cloned())
    }

    pub fn write(name: &str, b: &[u8]) -> Result<(), String> {
        if REFUSE.with(|r| r.get()) {
            return Err("refused".into());
        }
        FILES.with(|f| f.borrow_mut().insert(name.to_string(), b.to_vec()));
        Ok(())
    }

    pub fn available() -> bool {
        !REFUSE.with(|r| r.get())
    }
}

/// Natively: files in the config directory.
#[cfg(all(not(test), not(target_arch = "wasm32")))]
mod store {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub static USER_DIR: AtomicBool = AtomicBool::new(false);

    /// `$EQOA_CONFIG`, else (`keep_in_user_dir`) `eqoa-rs` in the
    /// platform's config directory.
    pub fn dir() -> Option<PathBuf> {
        if let Some(d) = std::env::var_os("EQOA_CONFIG") {
            return Some(PathBuf::from(d));
        }
        if !USER_DIR.load(Ordering::Relaxed) {
            return None;
        }
        user_dir()
    }

    /// `eqoa-rs` in the platform's config directory.
    pub fn user_dir() -> Option<PathBuf> {
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home().map(|h| h.join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".config")))
        };
        base.map(|b| b.join("eqoa-rs"))
    }

    pub fn read(name: &str) -> Option<Vec<u8>> {
        std::fs::read(dir()?.join(name)).ok()
    }

    /// Written whole beside the file, then moved over it: a run cut off
    /// mid-write leaves the old one.
    pub fn write(name: &str, b: &[u8]) -> Result<(), String> {
        let Some(d) = dir() else { return Ok(()) };
        let go = || -> std::io::Result<()> {
            std::fs::create_dir_all(&d)?;
            let tmp = d.join(format!("{name}.new"));
            std::fs::write(&tmp, b)?;
            std::fs::rename(&tmp, d.join(name))
        };
        go().map_err(|e| format!("{}: {e}", d.join(name).display()))
    }
}

/// In a page: the origin's localStorage, every access allowed to fail.
#[cfg(all(not(test), target_arch = "wasm32"))]
mod store {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    pub fn available() -> bool {
        storage().is_some()
    }

    pub fn read(name: &str) -> Option<Vec<u8>> {
        let s = storage()?.get_item(&format!("eqoa.{name}")).ok().flatten()?;
        let b = s.as_bytes();
        (b.len() % 2 == 0).then_some(())?;
        b.chunks(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok()).collect()
    }

    pub fn write(name: &str, b: &[u8]) -> Result<(), String> {
        let s = storage().ok_or("the page has no storage")?;
        let hex: String = b.iter().map(|v| format!("{v:02x}")).collect();
        s.set_item(&format!("eqoa.{name}"), &hex).map_err(|e| format!("{e:?}"))
    }
}

#[cfg(test)]
pub mod testing {
    /// Empties this thread's store and settings, a store that refuses or not.
    pub fn reset(refuse: bool) {
        super::store::FILES.with(|f| f.borrow_mut().clear());
        super::store::REFUSE.with(|r| r.set(refuse));
        super::CLIENT.with(|c| c.set(None));
        super::BINDS.with(|c| *c.borrow_mut() = None);
        super::BINDS_KEPT.with(|c| *c.borrow_mut() = None);
        crate::multibox::testing::set_place(None);
        super::PLATFORM_RATE.with(|p| p.set(super::FrameRate::Display));
        super::FRAME_DUE.with(|d| d.set(0.0));
        super::DISPLAY_GAP.with(|g| g.set(1000.0 / 60.0));
        super::FILE_SHARED.with(|f| f.set(None));
    }

    /// Writes `name` in the store as another window's save would.
    pub fn put(name: &str, b: &[u8]) {
        super::store::FILES.with(|f| f.borrow_mut().insert(name.to_string(), b.to_vec()));
    }

    /// What the store holds under `name`.
    pub fn kept(name: &str) -> Option<Vec<u8>> {
        super::store::FILES.with(|f| f.borrow().get(name).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed() -> GameOptions {
        GameOptions {
            battle: 3,
            stereo: 0,
            follow: 1,
            inverted: 1,
            fx_volume: F::new(0.3),
            music_volume: F::new(0.7),
            compact: 0,
            no_damage: 1,
            no_faction: 1,
            no_loot: 1,
            auto_attack: 1,
            vibration: 0,
        }
    }

    /// The record is the game's: each setting at its offset in vi.h's
    /// `VIGameOptions`, `m_flags` 0, the magic last, and nothing in the
    /// login or the home shard; it reads back the same.
    #[test]
    fn the_card_record_is_the_games() {
        let o = changed();
        let b = o.to_card();
        assert_eq!(b.len(), 1024);
        assert_eq!(&b[0x3FC..], &[0x67, 0x45, 0x23, 0x01], "the magic, little-endian");
        assert_eq!(&b[0..4], &[0; 4], "m_flags");
        assert_eq!(&b[4..8], &[3, 0, 1, 1], "battle music, output, camera, pitch axis");
        assert_eq!(f32::from_le_bytes(b[8..12].try_into().unwrap()), 0.3, "m_musicVolume: the Fx Volume");
        assert_eq!(f32::from_le_bytes(b[12..16].try_into().unwrap()), 0.7, "m_effectVolume: the Music Volume");
        assert!(b[0x10..0x90].iter().all(|&c| c == 0), "the username and password are never kept");
        assert_eq!(&b[0x91..0x98], &[0, 1, 1, 1, 1, 0, 0], "keyboard, damage, faction, loot, auto attack, region, vibration");
        assert!(b[0x98..0x3FC].iter().all(|&c| c == 0), "the home shard and the padding");
        assert_eq!(GameOptions::from_card(&b), Some(o));
        // the control: a setting a byte off does not read back
        let mut moved = b;
        moved[0x95] = 0;
        moved[0x96] = 1;
        assert_ne!(GameOptions::from_card(&moved), Some(o));
    }

    /// `Load`: a record cut short, with `m_flags` set or without the magic
    /// is the defaults; one the PS2 wrote, login and region included, reads
    /// (the login is not taken).
    #[test]
    fn load_takes_only_a_whole_record() {
        let b = changed().to_card();
        assert_eq!(GameOptions::from_card(&b[..1023]), None);
        let mut flags = b;
        flags[0] = 1;
        assert_eq!(GameOptions::from_card(&flags), None);
        let mut magic = b;
        magic[0x3FC] = 0x66;
        assert_eq!(GameOptions::from_card(&magic), None);
        let mut ps2 = b;
        ps2[0x10..0x13].copy_from_slice(b"dab");
        ps2[0x50..0x53].copy_from_slice(b"pwd");
        ps2[0x96] = 1;
        assert_eq!(GameOptions::from_card(&ps2), Some(changed()));
        // a volume no game writes
        let mut loud = b;
        loud[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(GameOptions::from_card(&loud).unwrap().fx_volume, F::ONE);
    }

    /// Saved, then loaded as the next run loads them: every setting as it
    /// was; with nothing kept, the defaults (the keyboard Compact); with the
    /// store refusing, the defaults and a save that says it failed.
    #[test]
    fn options_kept_between_runs() {
        testing::reset(false);
        let none = game_options();
        assert_eq!(none, GameOptions { compact: 1, ..Default::default() });
        let o = changed();
        let mine = ClientOptions { lod: false, brightness: 8, frame_rate: Some(FrameRate::Sixty), camera_distance: 50, camera_turn: 175, ..Default::default() };
        set_client(mine);
        assert!(save(&o));
        CLIENT.with(|c| c.set(None));
        assert_eq!(game_options(), o);
        assert_eq!(client(), mine);
        let card = testing::kept(GAME_FILE).unwrap();
        assert!(card[0x10..0x90].iter().all(|&c| c == 0));
        testing::reset(true);
        assert!(!save(&o));
        assert_eq!(game_options(), none);
        assert_eq!(client(), ClientOptions::default());
        testing::reset(false);
    }

    /// The keys and buttons: kept by Save Changes in keys.opt and read back
    /// as the next run reads them; nothing kept, the default layout; with the
    /// store refusing, the defaults and a save that says it failed.
    #[test]
    fn keys_kept_between_runs() {
        testing::reset(false);
        assert!(binds().is_default());
        let mut b = crate::binds::Binds::default();
        b.bind_key(crate::binds::row_named("use").unwrap(), "q");
        set_binds(b.clone());
        assert!(save(&GameOptions::default()));
        BINDS.with(|c| *c.borrow_mut() = None);
        assert_eq!(binds(), b, "read back");
        assert_eq!(testing::kept(KEYS_FILE).as_deref(), Some(&b"key.talk=e\nkey.use=q\n"[..]));
        testing::reset(true);
        assert!(binds().is_default());
        assert!(!save(&GameOptions::default()));
        testing::reset(false);
    }

    #[test]
    fn client_text_round_trips_and_forgives() {
        for c in [
            ClientOptions::default(),
            ClientOptions { lod: false, brightness: 0, frame_rate: Some(FrameRate::Thirty), camera_distance: 8, ..Default::default() },
            ClientOptions { lod: true, brightness: 10, frame_rate: Some(FrameRate::Display), camera_distance: 50, ..Default::default() },
            ClientOptions { camera_turn: TURN_MIN, ..Default::default() },
            ClientOptions { camera_turn: TURN_MAX, ..Default::default() },
            ClientOptions { controller: std::array::from_fn(|k| Focus::ALL[k % Focus::ALL.len()]), ..Default::default() },
            ClientOptions { screen_shake: false, capture: true, ..Default::default() },
            ClientOptions { fog: 0, ..Default::default() },
            ClientOptions { fog: crate::fog::STEPS, ..Default::default() },
            ClientOptions { background_sound: false, ..Default::default() },
            ClientOptions { ability_bars: 3, ..Default::default() },
            ClientOptions { ability_bars: 4, ..Default::default() },
            ClientOptions { entity_slots: 128, ..Default::default() },
            ClientOptions { entity_slots: ENTITY_SLOTS_MAX, ..Default::default() },
            ClientOptions { entity_view: 450, ..Default::default() },
            ClientOptions { entity_view: ENTITY_VIEW_MAX, ..Default::default() },
            ClientOptions { art: esf::Era::Y2002, ..Default::default() },
            ClientOptions { text: render::ui::TextFilter::Smooth, ..Default::default() },
            ClientOptions { output: Some(DeviceName::new("Speakers (Realtek(R) Audio)")), ..Default::default() },
            ClientOptions { output: Some(DeviceName::new("Kopfhörer (2- Arctis 7 Game) = Ü")), ..Default::default() },
        ] {
            assert_eq!(ClientOptions::from_text(&c.to_text()), c);
        }
        assert_eq!(ClientOptions { text: render::ui::TextFilter::Smooth, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\ntext=smooth\n", "Text only when Smooth");
        // Output Device: only when one is chosen, by its name; an empty name
        // is the system default; a name past the most kept is cut at a
        // character, and the system's own name still matches it
        let tv = DeviceName::new("LG TV SSCR2 (NVIDIA High Definition Audio)");
        assert_eq!(ClientOptions { output: Some(tv), ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\noutput=LG TV SSCR2 (NVIDIA High Definition Audio)\n", "Output Device only when chosen");
        assert_eq!(ClientOptions::from_text("output=").output, None, "the system default");
        assert!(tv.same("LG TV SSCR2 (NVIDIA High Definition Audio)") && !tv.same("LG TV SSCR2"), "the whole name");
        let long = "é".repeat(150);
        let cut = DeviceName::new(&long);
        assert_eq!(cut.as_str().len(), DEVICE_NAME_MAX, "cut at a character: 100 two-byte é");
        assert!(cut.same(&long) && ClientOptions::from_text(&format!("output={long}")).output == Some(cut), "matched by its cut");
        let odd = DeviceName::new(&format!("a{}", "é".repeat(150)));
        assert_eq!(odd.as_str().len(), DEVICE_NAME_MAX - 1, "the character across the end left out");
        assert_eq!(ClientOptions::from_text("text=sharp").text, render::ui::TextFilter::Sharp);
        for bad in ["text=blurry", "text=", "text=1"] {
            assert_eq!(ClientOptions::from_text(bad).text, render::ui::TextFilter::Sharp, "{bad}: Sharp, the default");
        }
        assert_eq!(ClientOptions { entity_slots: 128, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nentity_slots=128\n", "NPC/PC Display only when not the game's");
        assert_eq!(ClientOptions::from_text("entity_slots=100").entity_slots, 100, "any number in range");
        for bad in ["entity_slots=23", "entity_slots=205", "entity_slots=999", "entity_slots=lots"] {
            assert_eq!(ClientOptions::from_text(bad).entity_slots, ENTITY_SLOTS_GAME, "{bad}: the game's 24");
        }
        assert_eq!(ClientOptions { entity_view: 600, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nentity_view=600\n", "NPC/PC Distance only when not the game's");
        for bad in ["entity_view=99", "entity_view=1001", "entity_view=far"] {
            assert_eq!(ClientOptions::from_text(bad).entity_view, ENTITY_VIEW_GAME, "{bad}: the game's 100");
        }
        assert_eq!(ClientOptions { name_distance: 60, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nname_distance=60\n", "Name Distance only when not the game's");
        assert_eq!(ClientOptions::from_text("name_distance=60").name_distance, 60, "read back");
        assert_eq!(ClientOptions::from_text("name_distance=25").name_distance, 25, "a number between the tens, kept");
        for bad in ["name_distance=21", "name_distance=201", "name_distance=far"] {
            assert_eq!(ClientOptions::from_text(bad).name_distance, NAME_DISTANCE_GAME, "{bad}: the game's 22");
        }
        assert_eq!(ClientOptions { art: esf::Era::Y2002, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nart=2002\n", "Art only when it is the 2002 disc's");
        assert_eq!(ClientOptions::from_text("art=frontiers").art, esf::Era::Frontiers);
        for bad in ["art=1999", "art=2003", "art="] {
            assert_eq!(ClientOptions::from_text(bad).art, esf::Era::Frontiers, "{bad}: Frontiers', the default");
        }
        assert_eq!(ClientOptions { ability_bars: 4, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nability_bars=4\n", "Ability Bars only when not the game's");
        for bad in ["ability_bars=1", "ability_bars=5", "ability_bars=three"] {
            assert_eq!(ClientOptions::from_text(bad).ability_bars, ABILITY_BARS_GAME, "{bad}: the game's two");
        }
        assert_eq!(ClientOptions { background_sound: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nbackground_sound=off\n", "Sound in Background only when off");
        for bad in ["background_sound=no", "background_sound=0"] {
            assert!(ClientOptions::from_text(bad).background_sound, "{bad}: on, the default");
        }
        assert_eq!(ClientOptions { fog: 0, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nfog=0\n", "Fog only when not the game's");
        assert_eq!(ClientOptions { vsync: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nvsync=off\n", "VSync only when off");
        assert!(!ClientOptions::from_text("vsync=off").vsync && ClientOptions::from_text("vsync=on").vsync);
        for bad in ["vsync=no", "vsync=0", "vsync=adaptive"] {
            assert!(ClientOptions::from_text(bad).vsync, "{bad}: on, the default");
        }
        assert_eq!(ClientOptions { shouts: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nshouts=off\n", "Shouts only when ignored");
        assert!(!ClientOptions::from_text("shouts=off").shouts && ClientOptions::from_text("shouts=on").shouts);
        assert_eq!(ClientOptions { update_notices: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nupdate_notices=off\n", "Update Notices only when off");
        assert!(!ClientOptions::from_text("update_notices=off").update_notices && ClientOptions::from_text("update_notices=on").update_notices);
        for bad in ["shouts=no\nupdate_notices=0", "shouts=ignored\nupdate_notices=never"] {
            let c = ClientOptions::from_text(bad);
            assert!(c.shouts && c.update_notices, "{bad}: on, the defaults");
        }
        assert_eq!(ClientOptions { overlapping_names: true, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\noverlapping_names=show\n", "Overlapping Names only when Show");
        assert!(ClientOptions::from_text("overlapping_names=show").overlapping_names && !ClientOptions::from_text("overlapping_names=hide").overlapping_names);
        for bad in ["overlapping_names=on", "overlapping_names=1", "overlapping_names=all"] {
            assert!(!ClientOptions::from_text(bad).overlapping_names, "{bad}: Hide, the default");
        }
        assert_eq!(ClientOptions { loading_longer: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nloading=instant\n", "Loading Screens only when Instant");
        assert!(!ClientOptions::from_text("loading=instant").loading_longer && ClientOptions::from_text("loading=longer").loading_longer);
        for bad in ["loading=off", "loading=0", "loading=fast"] {
            assert!(ClientOptions::from_text(bad).loading_longer, "{bad}: Longer, the default");
        }
        assert_eq!(ClientOptions::default().to_text(), "lod=on\nbrightness=5\ncamera_distance=11\n", "no Frame Rate until one is chosen, no game's turn, no window's Auto");
        assert_eq!(ClientOptions { camera_turn: 250, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\ncamera_turn=250\n");
        assert_eq!(
            ClientOptions { screen_shake: false, capture: true, ..Default::default() }.to_text(),
            "lod=on\nbrightness=5\ncamera_distance=11\nscreen_shake=off\ncapture_controller=on\n",
            "Screen Shake only off, Capture Controller only on"
        );
        testing::reset(false);
        assert_eq!(describe(), "level of detail on, brightness 5 of 10, fog 5 of 10 (the game's), frame rate the display's (not chosen), vsync on, render scale x1, camera distance 11, camera turn speed 100%, screen shake on, controller focus auto, capture controller off, sound in background on, output device the system default, ability bars 2, npc/pc display 24, npc/pc view distance 100, name distance 22, overlapping names hide, art frontiers, update notices on, shouts heard, text sharp, hud nearest, mipmaps on, flora on, shadows on, environment on, loading screens longer; kept in the page's storage");
        testing::reset(true);
        assert!(describe().ends_with("refused: nothing is kept"));
        testing::reset(false);
        let odd = ClientOptions::from_text("brightness=11\nlod=maybe\nframe_rate=120\ncamera_distance=51\nwhat=ever\nnoise\ncontroller.0=pad1\ncontroller.17=pad1\ncontroller.2=pad5\ncontroller.x=off");
        assert_eq!(odd, ClientOptions::default());
        assert_eq!(ClientOptions::from_text("camera_distance=7").camera_distance, 11);
        for bad in ["camera_turn=49", "camera_turn=301", "camera_turn=fast", "camera_turn=-100"] {
            assert_eq!(ClientOptions::from_text(bad).camera_turn, TURN_GAME, "{bad}: the game's");
        }
        assert_eq!(ClientOptions::from_text("camera_turn=300").camera_turn, 300);
        let two = ClientOptions::from_text("controller.2=pad3\ncontroller.16=window");
        assert_eq!((two.controller[1], two.controller[15], two.controller[0]), (Focus::Pad(3), Focus::Window, Focus::Auto));
        for bad in ["fog=11", "fog=-1", "fog=thick"] {
            assert_eq!(ClientOptions::from_text(bad).fog, crate::fog::GAME, "{bad}: the game's");
        }
        for bad in ["screen_shake=no", "capture_controller=1", "capture_controller=yes"] {
            let c = ClientOptions::from_text(bad);
            assert!(c.screen_shake && !c.capture, "{bad}: the defaults");
        }
    }

    /// Mipmaps, on by default, kept only when off; anything but on or off
    /// leaves it on.
    #[test]
    fn mipmaps_kept_only_when_off() {
        assert!(ClientOptions::default().mipmaps, "on, the game's");
        assert_eq!(ClientOptions { mipmaps: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nmipmaps=off\n", "Mipmaps only when off");
        assert!(!ClientOptions::from_text("mipmaps=off").mipmaps && ClientOptions::from_text("mipmaps=on").mipmaps);
        for bad in ["mipmaps=0", "mipmaps=no", "mipmaps="] {
            assert!(ClientOptions::from_text(bad).mipmaps, "{bad}: on, the default");
        }
    }

    /// Shadows, on by default (the game's), kept only when off; anything
    /// but on or off leaves it on.
    #[test]
    fn shadows_kept_only_when_off() {
        let d = ClientOptions::default();
        assert!(d.shadows, "on, the game's");
        assert_eq!(ClientOptions { shadows: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nshadows=off\n");
        assert!(!d.to_text().contains("shadows"), "the default not written");
        let c = ClientOptions::from_text("shadows=off");
        assert!(!c.shadows && c.flora && c.environment, "Shadows alone");
        assert!(ClientOptions::from_text("shadows=on") == d);
        for bad in ["shadows=0", "shadows=", "shadow=off"] {
            assert!(ClientOptions::from_text(bad).shadows, "{bad}: on, the default");
        }
        // the control: Flora off is not Shadows off
        assert!(ClientOptions::from_text("flora=off").shadows);
    }

    /// Flora and Environment, on by default, each kept only when off;
    /// anything but on or off leaves it on, and one never moves the other.
    #[test]
    fn flora_and_environment_kept_only_when_off() {
        let d = ClientOptions::default();
        assert!(d.flora && d.environment, "on, the game's");
        assert_eq!(ClientOptions { flora: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nflora=off\n");
        assert_eq!(ClientOptions { environment: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nenvironment=off\n");
        let c = ClientOptions::from_text("flora=off");
        assert!(!c.flora && c.environment, "Flora alone");
        let c = ClientOptions::from_text("environment=off");
        assert!(c.flora && !c.environment, "Environment alone");
        assert!(ClientOptions::from_text("flora=on\nenvironment=on") == d);
        for bad in ["flora=0", "environment=no", "flora="] {
            let c = ClientOptions::from_text(bad);
            assert!(c.flora && c.environment, "{bad}: on, the default");
        }
    }

    /// Render Scale, x1 by default, kept only above 1 and only as one of its
    /// steps; HUD, Nearest by default (since 2026-10-28; Sharp the day
    /// before), kept only when Sharp, and a file from the builds that wrote
    /// `hud=nearest` (Sharp their default) reads the same; neither moves the
    /// other, and anything else leaves each at its default.
    #[test]
    fn render_scale_and_hud_kept_only_off_their_defaults() {
        use render::ui::HudFilter;
        let d = ClientOptions::default();
        assert_eq!((d.render_scale, d.hud, d.text), (1, HudFilter::Nearest, render::ui::TextFilter::Sharp), "x1; the HUD Nearest, the game's");
        let head = "lod=on\nbrightness=5\ncamera_distance=11\n";
        assert_eq!(d.to_text(), head, "none written at their defaults");
        assert_eq!(ClientOptions { render_scale: 2, ..d }.to_text(), format!("{head}render_scale=2\n"));
        assert_eq!(ClientOptions { render_scale: 4, ..d }.to_text(), format!("{head}render_scale=4\n"));
        assert_eq!(ClientOptions { hud: HudFilter::Sharp, ..d }.to_text(), format!("{head}hud=sharp\n"));
        for k in [1, 2, 4] {
            assert_eq!(ClientOptions::from_text(&format!("render_scale={k}")).render_scale, k);
        }
        let c = ClientOptions::from_text("render_scale=4\nhud=sharp\n");
        assert_eq!((c.render_scale, c.hud), (4, HudFilter::Sharp));
        assert_eq!(ClientOptions::from_text(&c.to_text()), c, "round trip");
        let c = ClientOptions::from_text("hud=sharp");
        assert_eq!((c.render_scale, c.hud), (1, HudFilter::Sharp), "HUD alone");
        assert_eq!(ClientOptions::from_text("hud=nearest\nrender_scale=1"), d, "the earlier builds' hud=nearest: the default");
        for bad in ["render_scale=3", "render_scale=8", "render_scale=0", "render_scale=x2", "hud=smooth", "hud="] {
            assert_eq!(ClientOptions::from_text(bad), d, "{bad}: the defaults");
        }
    }

    /// The options kept by the build with Textures' Crisp (gone the same day:
    /// the user, "world textures cant render proper without smooth. so only
    /// use for UI, Text") still load: the line is left out and written back
    /// no more. The control: the lines beside it are read.
    #[test]
    fn options_with_textures_crisp_still_load() {
        let c = ClientOptions::from_text("textures=crisp\nhud=sharp\nrender_scale=2\n");
        assert_eq!((c.render_scale, c.hud), (2, render::ui::HudFilter::Sharp), "the lines beside it read");
        let text = c.to_text();
        assert!(!text.contains("textures"), "{text}");
        assert!(text.contains("render_scale=2\n") && text.contains("hud=sharp\n"), "{text}");
    }

    /// The options kept by a build with voice chat (voice chat was only a
    /// test, and is gone) still load: its lines are left out and written
    /// back no more. The control: the lines beside them are read.
    #[test]
    fn options_with_the_voice_lines_still_load() {
        let c = ClientOptions::from_text("voice=ptt\nvoice_demo=on\nvoice_volume=15\nflora=off\nloading=instant\n");
        assert!(!c.flora && !c.loading_longer, "the lines beside them read");
        let text = c.to_text();
        assert!(!text.contains("voice"), "{text}");
        assert!(text.contains("flora=off\n") && text.contains("loading=instant\n"), "{text}");
    }

    /// The options kept by a build with the map, the minimap, the Stats Box,
    /// others' mana bars, Target of Target and the quests' marks and tracker
    /// (all gone) still load: their lines are left out and written back no
    /// more. The control: the lines beside them are read.
    #[test]
    fn options_with_the_lines_of_rows_gone_still_load() {
        let old = "target_of_target=on\nmana_bars=off\nminimap=off\nstats_box=on\nquest_markers=off\nquest_tracker=off\nmap_markers=off\nmap_towns=off\nflora=off\nloading=instant\n";
        let c = ClientOptions::from_text(old);
        assert_eq!(c, ClientOptions { flora: false, loading_longer: false, ..Default::default() }, "the lines beside them read");
        let text = c.to_text();
        for gone in ["target_of_target", "mana_bars", "minimap", "stats_box", "quest_", "map_"] {
            assert!(!text.contains(gone), "{gone}: {text}");
        }
        assert!(text.contains("flora=off\n") && text.contains("loading=instant\n"), "{text}");
    }

    /// Capture Controller is one setting for every window: another window's
    /// save reaches this one at its next look (`look_shared`), while its own
    /// change not saved yet stands until the file changes; the control: the
    /// other settings stay this window's as it read them.
    #[test]
    fn capture_controller_reaches_every_window() {
        testing::reset(false);
        assert!(!capture());
        look_shared();
        assert!(!capture(), "nothing kept: still off");
        // another window saves it on, and a brightness of its own
        testing::put(CLIENT_FILE, b"lod=on\nbrightness=9\ncamera_distance=11\ncapture_controller=on\n");
        look_shared();
        assert!(capture(), "taken at the next look");
        assert_eq!(client().brightness, 5, "the control: this window's own brightness stays");
        // this window turns it off, not saved: the file has not changed since
        let mut c = client();
        c.capture = false;
        set_client(c);
        look_shared();
        assert!(!capture(), "its own change stands");
        // saved here, then the other window's off: each reaches the other
        assert!(save(&GameOptions::default()));
        assert!(!String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap().contains("capture_controller"));
        testing::put(CLIENT_FILE, b"lod=on\nbrightness=9\ncamera_distance=11\ncapture_controller=on\n");
        look_shared();
        assert!(capture());
        testing::reset(false);
    }

    /// Sound in Background is one setting for every window, as Capture
    /// Controller is: another window's save of it reaches this one at its
    /// next look, and a later window's own file never carries it; the
    /// control: this window's own brightness stays as it read it.
    #[test]
    fn sound_in_background_reaches_every_window() {
        testing::reset(false);
        assert!(background_sound(), "on until turned off");
        look_shared();
        testing::put(CLIENT_FILE, b"lod=on\nbrightness=9\ncamera_distance=11\nbackground_sound=off\n");
        look_shared();
        assert!(!background_sound(), "taken at the next look");
        assert_eq!(client().brightness, 5, "the control: this window's own brightness stays");
        assert!(!ClientOptions { background_sound: false, ..Default::default() }.rows_text().contains("background_sound"), "a window's own file: the shared line left to client.opt");
        testing::reset(false);
    }

    /// Update Notices is one setting for every window, as Sound in Background
    /// is: another window's save reaches this one at its next look, Save
    /// Changes in a later window puts it in client.opt (never in its own
    /// file), and window 1 reads it back. The control: this window's own
    /// brightness stays as it read it.
    #[test]
    fn update_notices_reach_every_window() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        assert!(update_notices(), "on until turned off");
        look_shared();
        testing::put(CLIENT_FILE, b"lod=on\nbrightness=9\ncamera_distance=11\nupdate_notices=off\n");
        look_shared();
        assert!(!update_notices(), "taken at the next look");
        assert_eq!(client().brightness, 5, "the control: this window's own brightness stays");
        assert!(!ClientOptions { update_notices: false, ..Default::default() }.rows_text().contains("update_notices"), "a window's own file: the shared line left to client.opt");
        // a later window turns it on again and saves: client.opt has it
        testing::reset(false);
        testing::put(CLIENT_FILE, b"lod=on\nbrightness=5\ncamera_distance=11\nupdate_notices=off\n");
        set_place(Some(1));
        assert!(!update_notices(), "window 2 reads the shared line");
        set_client(ClientOptions { update_notices: true, brightness: 3, ..client() });
        assert!(save(&GameOptions::default()));
        assert!(!String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap().contains("update_notices"), "on again in client.opt");
        assert!(!String::from_utf8(testing::kept("client-2.opt").unwrap()).unwrap().contains("update_notices"), "never in a window's own file");
        set_place(Some(0));
        CLIENT.with(|c| c.set(None));
        assert!(update_notices(), "window 1 reads it back on");
        testing::reset(false);
    }

    /// Ignore Shouts is kept as soon as it is chosen (no Save Changes: the
    /// quick chat's choice is no page's row), each window its own: window 2's
    /// goes to client-2.opt and leaves window 1's file byte for byte, and
    /// Restore Shouts takes the line out again. The control: window 1 still
    /// hears them.
    #[test]
    fn ignore_shouts_is_kept_at_once_for_its_window() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        set_place(Some(0));
        assert!(shouts(), "heard until ignored");
        set_shouts(false).unwrap();
        assert_eq!(String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap(), "lod=on\nbrightness=5\ncamera_distance=11\nshouts=off\n", "kept at once");
        CLIENT.with(|c| c.set(None));
        assert!(!shouts(), "read back at the next start");
        set_shouts(true).unwrap();
        assert_eq!(String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap(), "lod=on\nbrightness=5\ncamera_distance=11\n", "Restore Shouts takes the line out");
        // window 2 ignores them: its own file, window 1's untouched
        let one = testing::kept(CLIENT_FILE).unwrap();
        set_place(Some(1));
        set_shouts(false).unwrap();
        assert_eq!(String::from_utf8(testing::kept("client-2.opt").unwrap()).unwrap(), "lod=on\nbrightness=5\ncamera_distance=11\nshouts=off\n");
        assert_eq!(testing::kept(CLIENT_FILE).unwrap(), one, "window 1's file byte for byte");
        set_place(Some(0));
        CLIENT.with(|c| c.set(None));
        assert!(shouts(), "the control: window 1 still hears them");
        testing::reset(false);
    }

    /// Controller Focus is each window's: Save Changes writes this window's
    /// (here the first: no place taken) and keeps another window's as the
    /// file has it, though this one read the file before that window saved
    /// (its own copy says Auto there: written as it is, it would be lost).
    #[test]
    fn each_window_keeps_its_own_controller_focus() {
        testing::reset(false);
        assert_eq!(focus(), Focus::Auto);
        set_focus(Focus::Pad(2));
        assert_eq!(client().controller[..2], [Focus::Pad(2), Focus::Auto]);
        // another window saves its own meanwhile
        store::write(CLIENT_FILE, b"lod=on\nbrightness=5\ncamera_distance=11\ncontroller.2=window\n").unwrap();
        assert!(save(&GameOptions::default()));
        let kept = String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap();
        assert_eq!(kept, "lod=on\nbrightness=5\ncamera_distance=11\ncontroller.1=pad2\ncontroller.2=window\n");
        // and this window's own goes back to Auto: the line goes
        set_focus(Focus::Auto);
        assert!(save(&GameOptions::default()));
        let kept = String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap();
        assert_eq!(kept, "lod=on\nbrightness=5\ncamera_distance=11\ncontroller.2=window\n");
        testing::reset(false);
    }

    /// Each window keeps its own options (F3 20261023-082517: sound in one
    /// window, none in the other): window 1 in game.opt and client.opt as
    /// ever, window 2 in game-2.opt and client-2.opt; a window 2 that has kept
    /// none yet starts from window 1's. The control: window 2's save leaves
    /// window 1's files byte for byte, and window 1 reads its own back.
    #[test]
    fn each_window_keeps_its_own_options() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        // window 1 saves its sound and a brightness
        set_place(Some(0));
        let one = changed();
        set_client(ClientOptions { brightness: 8, ..Default::default() });
        assert!(save(&one));
        let (game1, client1) = (testing::kept("game.opt").unwrap(), testing::kept("client.opt").unwrap());
        // window 2 has kept nothing: it starts from window 1's
        set_place(Some(1));
        assert_eq!(game_options(), one, "window 1's options to start from");
        assert_eq!(client().brightness, 8, "and its Extended rows");
        // window 2 turns its sound off and its brightness down, and saves
        let mut two = one;
        two.fx_volume = F::new(0.0);
        two.music_volume = F::new(0.0);
        set_client(ClientOptions { brightness: 2, ..client() });
        assert!(save(&two));
        assert_eq!(GameOptions::from_card(&testing::kept("game-2.opt").unwrap()), Some(two), "its own file");
        assert_eq!(String::from_utf8(testing::kept("client-2.opt").unwrap()).unwrap(), "lod=on\nbrightness=2\ncamera_distance=11\n");
        assert_eq!(testing::kept("game.opt").unwrap(), game1, "the control: window 1's game.opt untouched");
        assert_eq!(testing::kept("client.opt").unwrap(), client1, "and its client.opt");
        // each reads its own back, as each would at its next start
        set_place(Some(0));
        CLIENT.with(|c| c.set(None));
        assert_eq!((game_options(), client().brightness), (one, 8), "window 1: sound, its brightness");
        set_place(Some(1));
        CLIENT.with(|c| c.set(None));
        assert_eq!((game_options(), client().brightness), (two, 2), "window 2: no sound, its brightness");
        assert!(describe().ends_with("kept in the page's storage, as window 2's own (game-2.opt, client-2.opt)"));
        testing::reset(false);
    }

    /// A later window starting from window 1's options does not take its
    /// saved account (two windows cannot play one account at once), but does
    /// its home shard; its own Save Account and home shard go to its own file.
    /// The control: window 1 keeps its account and shard.
    #[test]
    fn a_later_window_saves_its_own_account() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        set_place(Some(0));
        assert!(save_login("alice", "secret1", true));
        set_home_shard("Marr's Fist").unwrap();
        set_place(Some(1));
        assert_eq!(saved_login(), (String::new(), String::new(), false), "not window 1's account");
        assert_eq!(home_shard().as_deref(), Some("Marr's Fist"), "window 1's shard to start from");
        assert!(save_login("bob", "secret2", true));
        set_home_shard("Hodstock").unwrap();
        assert_eq!(saved_login(), ("bob".to_string(), "secret2".to_string(), true));
        assert_eq!(home_shard().as_deref(), Some("Hodstock"));
        assert!(testing::kept("game-2.opt").is_some());
        set_place(Some(0));
        assert_eq!(saved_login(), ("alice".to_string(), "secret1".to_string(), true), "the control: window 1's own");
        assert_eq!(home_shard().as_deref(), Some("Marr's Fist"));
        testing::reset(false);
    }

    /// What the windows share stays in client.opt: a later window's save puts
    /// its Controller Focus and Capture Controller there, window 1's rows and
    /// the other windows' focus as the file has them, and its own file holds
    /// only its rows; window 1 sees the capture, and its save keeps window 2's
    /// focus. The control: window 1's rows in client.opt are unchanged.
    #[test]
    fn what_the_windows_share_stays_in_client_opt() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        set_place(Some(0));
        set_client(ClientOptions { brightness: 8, ..Default::default() });
        assert!(save(&GameOptions::default()));
        // window 3 saved its focus meanwhile
        let three = "lod=on\nbrightness=8\ncamera_distance=11\ncontroller.3=pad1\n";
        testing::put("client.opt", three.as_bytes());
        set_place(Some(1));
        set_focus(Focus::Pad(2));
        let mut c = client();
        c.capture = true;
        c.camera_distance = 50;
        set_client(c);
        assert!(save(&GameOptions::default()));
        assert_eq!(String::from_utf8(testing::kept("client.opt").unwrap()).unwrap(), "lod=on\nbrightness=8\ncamera_distance=11\ncontroller.2=pad2\ncontroller.3=pad1\ncapture_controller=on\n", "window 1's rows as they were, the shared lines");
        assert_eq!(String::from_utf8(testing::kept("client-2.opt").unwrap()).unwrap(), "lod=on\nbrightness=8\ncamera_distance=50\n", "its rows alone");
        // window 1: the capture reaches it, its own rows stay, and its save
        // keeps window 2's and window 3's focus
        set_place(Some(0));
        look_shared();
        assert!(capture());
        assert_eq!((client().camera_distance, focus()), (11, Focus::Auto));
        assert!(save(&GameOptions::default()));
        let kept = String::from_utf8(testing::kept("client.opt").unwrap()).unwrap();
        assert!(kept.contains("controller.2=pad2\ncontroller.3=pad1\ncapture_controller=on\n"), "{kept}");
        // window 2 again: its focus and capture out of client.opt, its rows its own
        set_place(Some(1));
        CLIENT.with(|c| c.set(None));
        assert_eq!((focus(), capture(), client().camera_distance), (Focus::Pad(2), true, 50));
        testing::reset(false);
    }

    /// A page learns its place a moment after it starts: settings read before
    /// it are window 1's, and read again once the place is known.
    #[test]
    fn a_place_learned_late_reads_its_own() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        testing::put("client.opt", b"lod=on\nbrightness=8\ncamera_distance=11\n");
        testing::put("client-2.opt", b"lod=off\nbrightness=3\ncamera_distance=11\n");
        set_place(Some(0));
        assert_eq!(client().brightness, 8, "before the place: window 1's");
        set_place(Some(1));
        assert_eq!((client().brightness, client().lod), (3, false), "its own once it is window 2");
        testing::reset(false);
    }

    /// keys.opt is the computer's: a save writes it only when this window's
    /// Controls page changed the keys, so a window saving its sound does not
    /// put back the keys it read at its start over another window's newer
    /// ones. The control: a change of this window's own is written.
    #[test]
    fn a_save_keeps_another_windows_keys() {
        testing::reset(false);
        assert!(binds().is_default(), "read at the start: nothing moved");
        // another window moves Use onto Q and saves
        let mut theirs = crate::binds::Binds::default();
        theirs.bind_key(crate::binds::row_named("use").unwrap(), "q");
        testing::put(KEYS_FILE, theirs.to_text().as_bytes());
        // this window saves its sound, its keys untouched
        assert!(save(&GameOptions::default()));
        assert_eq!(testing::kept(KEYS_FILE).as_deref(), Some(theirs.to_text().as_bytes()), "the other window's keys stay");
        // this window moves a key of its own: then its keys are written
        let mut mine = crate::binds::Binds::default();
        mine.bind_key(crate::binds::row_named("use").unwrap(), "x");
        set_binds(mine.clone());
        assert!(save(&GameOptions::default()));
        assert_eq!(testing::kept(KEYS_FILE).as_deref(), Some(mine.to_text().as_bytes()), "the control: its own change is kept");
        testing::reset(false);
    }

    /// The curve: none at 5; brighter above, darker below, black and white
    /// kept; 10 is the square root.
    #[test]
    fn brightness_curves() {
        assert_eq!(curve(BRIGHTNESS_AS_DRAWN), None);
        for s in (0..=10).filter(|&s| s != 5) {
            let t = curve(s).unwrap();
            assert_eq!((t[0], t[255]), (0, 255), "step {s}");
            assert!(t.windows(2).all(|w| w[0] <= w[1]), "step {s} rises");
            assert!(if s > 5 { t[64] > 64 } else { t[64] < 64 }, "step {s}");
        }
        assert_eq!(curve(10).unwrap()[64], 128, "sqrt(64/255) * 255 = 127.75");
        assert_eq!(curve(0).unwrap()[128], 64, "(128/255)^2 * 255 = 64.25");
        assert!(curve(6).unwrap()[64] < curve(7).unwrap()[64]);
    }

    /// Under the 30 cap every frame drawn is one of the game's (`frames.rs`), so
    /// what the game counts in frames (the HUD's windows, a list's pulsing
    /// pointer) steps once a frame drawn, at even times (F3 20261026-030902: the
    /// who list's pointer pulsed unevenly at Frame Rate 30). Natively the loop
    /// wakes when the cap's frame is due (`frame_wait`, a millisecond or so late
    /// at most) whatever the display, and in a page on each animation frame of a
    /// 60 to 165 Hz display; the cap's gap was 33.0 ms against the game's
    /// 33.33, so the frames drawn crept 0.33 ms a frame ahead of the game's and
    /// about every hundredth (every 3.3 s) came over `frames::EARLY_MS` early:
    /// no step, a frame of the HUD shown twice (66 ms), at a different point of
    /// each pulse. The control: that gap again, which skips.
    #[test]
    fn under_the_30_cap_every_frame_drawn_is_the_games() {
        // (frames drawn, those of the game's, the longest gap between the game's)
        let run = |gap: f64, wake: &dyn Fn(f64, f64) -> f64| {
            let (mut due, mut frames, mut t, mut drawn, mut games, mut last, mut longest) =
                (0.0, crate::frames::Frames::default(), 1000.0, 0, 0, f64::NAN, 0.0f64);
            let mut seed = 0x9E37_79B9u32;
            while t < 61_000.0 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let jitter = (seed % 1000) as f64 / 1000.0;
                if let Some(next) = take(due, t, gap) {
                    due = next;
                    drawn += 1;
                    if frames.due(t) {
                        games += 1;
                        if !last.is_nan() {
                            longest = longest.max(t - last);
                        }
                        last = t;
                    }
                }
                t = wake(t, due).max(t + 0.5) + jitter;
            }
            (drawn, games, longest)
        };
        // natively: asleep until the cap's frame is due (less `EARLY`)
        let native = |_: f64, due: f64| due - EARLY;
        let thirty = FrameRate::Thirty.gap_ms();
        let (drawn, games, longest) = run(thirty, &native);
        assert!((1795..=1805).contains(&drawn), "natively: {drawn} frames drawn in a minute");
        assert_eq!(games, drawn, "natively: every frame drawn is the game's");
        assert!(longest < crate::frames::FRAME_MS + 1.5, "natively: the game's frames at most {longest} ms apart");
        // a page: an animation frame of the display each time
        for hz in [60.0, 75.0, 120.0, 144.0, 165.0] {
            let page = move |t: f64, _: f64| ((t / (1000.0 / hz)).floor() + 1.0) * (1000.0 / hz);
            let (drawn, games, longest) = run(thirty, &page);
            assert!(drawn > 1700, "{hz} Hz: {drawn} frames drawn in a minute");
            assert_eq!(games, drawn, "{hz} Hz: every frame drawn is the game's");
            assert!(longest < crate::frames::FRAME_MS + 1000.0 / hz + 1.5, "{hz} Hz: the game's frames at most {longest} ms apart");
        }
        // the control: the cap's old 33.0 ms
        let (drawn, games, longest) = run(33.0, &native);
        assert!(drawn - games >= 10, "33.0 ms: {} frames drawn of the game's skipped", drawn - games);
        assert!(longest > 2.0 * crate::frames::FRAME_MS - 1.0, "33.0 ms: a frame of the HUD shown twice ({longest} ms)");
    }

    /// The 30 cap as a page on WebGL2 has kept it: on a 60 Hz display every
    /// other frame, on 165 Hz 30 a second and never two within 29 ms; 60 on
    /// 60 Hz every frame, the first frame drawn 3 ms late or not (the due
    /// times line up on it); on 75 to 165 Hz 60 a second, never two within
    /// 11 ms. The control: the display's every frame, uncapped.
    #[test]
    fn the_cap_holds_at_every_display_rate() {
        let run = |hz: f64, gap: f64, late: f64, early: f64| {
            let (mut due, mut drawn, mut last, mut min_gap) = (0.0, 0, f64::MIN, f64::MAX);
            for k in 0..(hz as usize * 10) {
                // the first frame late, then each a little late every third
                let now = 1000.0 + k as f64 * 1000.0 / hz + if k == 0 { late } else if k % 3 == 1 { 0.5 } else { 0.0 };
                let next = if now + early < due { None } else { Some(if now - due > gap { now + gap } else { due + gap }) };
                if early == EARLY {
                    assert_eq!(next, take(due, now, gap), "the rule written out");
                }
                if let Some(next) = next {
                    due = next;
                    drawn += 1;
                    min_gap = min_gap.min(now - last);
                    last = now;
                }
            }
            (drawn as f64 / 10.0, min_gap)
        };
        let thirty = FrameRate::Thirty.gap_ms();
        let sixty = FrameRate::Sixty.gap_ms();
        assert_eq!(thirty, crate::frames::FRAME_MS, "30: the game's own frame");
        let (fps, _) = run(60.0, thirty, 0.0, EARLY);
        assert!((29.5..=31.0).contains(&fps), "30 on 60 Hz: {fps}");
        let (fps, min) = run(165.0, thirty, 0.0, EARLY);
        assert!((29.5..=31.0).contains(&fps) && min > 29.0, "30 on 165 Hz: {fps}, {min}");
        for late in [0.0, 3.0] {
            assert_eq!(run(60.0, sixty, late, EARLY).0, 60.0, "60 on 60 Hz, first frame {late} ms late: every frame");
        }
        for hz in [75.0, 120.0, 144.0, 165.0] {
            let (fps, min) = run(hz, sixty, 0.0, EARLY);
            assert!((59.0..=61.0).contains(&fps) && min > 11.0, "60 on {hz} Hz: {fps}, {min}");
        }
        assert_eq!(run(165.0, 0.0, 0.0, f64::INFINITY).0, 165.0, "the control");
    }

    /// The rate in effect: the platform's until the player chooses one.
    #[test]
    fn the_players_rate_over_the_platforms() {
        testing::reset(false);
        assert_eq!(frame_rate(), FrameRate::Display);
        set_platform_rate(FrameRate::Thirty);
        assert_eq!(frame_rate(), FrameRate::Thirty);
        set_client(ClientOptions { frame_rate: Some(FrameRate::Sixty), ..client() });
        assert_eq!(frame_rate(), FrameRate::Sixty);
        assert!(frame_due(), "the first frame is due");
        assert!(!frame_due(), "the next comes a gap later");
        assert!(frame_wait().is_some());
        set_client(ClientOptions { frame_rate: Some(FrameRate::Display), ..client() });
        assert!(frame_due() && frame_due() && frame_wait().is_none(), "no cap: every frame");
        testing::reset(false);
    }

    /// VSync off with Frame Rate the display's: the loop keeps to the
    /// display's refresh (144 Hz here, 60 when the monitor reports none),
    /// as no present paces it. The control: VSync on, no cap (the present
    /// waits for the display); and a chosen 30 keeps its own gap either way.
    #[test]
    fn vsync_off_keeps_to_the_displays_refresh() {
        testing::reset(false);
        set_client(ClientOptions { frame_rate: Some(FrameRate::Display), ..client() });
        assert_eq!(frame_gap(), 0.0, "the control: VSync on, the present paces it");
        set_client(ClientOptions { vsync: false, ..client() });
        set_display_refresh(Some(144_000));
        assert!((frame_gap() - 1000.0 / 144.0).abs() < 1e-9, "144 Hz: {}", frame_gap());
        assert!(frame_due(), "the first frame is due");
        assert!(!frame_due(), "the next a refresh later");
        assert!(frame_wait().is_some(), "the loop sleeps meanwhile");
        set_display_refresh(None);
        assert!((frame_gap() - 1000.0 / 60.0).abs() < 1e-9, "no refresh reported: 60");
        set_client(ClientOptions { frame_rate: Some(FrameRate::Thirty), ..client() });
        assert_eq!(frame_gap(), crate::frames::FRAME_MS, "a chosen cap is its own");
        set_client(ClientOptions { frame_rate: Some(FrameRate::Thirty), vsync: true, ..client() });
        assert_eq!(frame_gap(), crate::frames::FRAME_MS);
        set_display_refresh(None);
        testing::reset(false);
    }

    /// The server list's home shard: kept at game.opt's +0x98 as UTF-16,
    /// the options beside it untouched, and an options save keeps it.
    #[test]
    fn the_home_shard_lives_in_game_opt() {
        testing::reset(false);
        assert_eq!(home_shard(), None);
        set_home_shard("Marr's Fist").unwrap();
        assert_eq!(home_shard().as_deref(), Some("Marr's Fist"));
        let b = testing::kept(GAME_FILE).unwrap();
        assert_eq!(&b[0x98..0x9C], &[b'M', 0, b'a', 0]);
        assert!(GameOptions::from_card(&b).is_some(), "still the game's record");
        // Save Changes keeps it
        assert!(save(&GameOptions { battle: 3, ..game_options() }));
        assert_eq!(home_shard().as_deref(), Some("Marr's Fist"));
        assert_eq!(game_options().battle, 3);
        // STATION's Save Account beside it: each keeps the other
        assert!(save_login("player002", "pw", true));
        assert_eq!(home_shard().as_deref(), Some("Marr's Fist"));
        set_home_shard("Hodstock").unwrap();
        assert_eq!(saved_login(), ("player002".to_string(), "pw".to_string(), true));
        set_home_shard("Marr's Fist").unwrap();
        // a longer name is cut to 63 characters, the terminator kept
        set_home_shard(&"x".repeat(80)).unwrap();
        assert_eq!(home_shard().map(|s| s.len()), Some(63));
        // the control: a damaged game.opt has no home shard
        let mut bad = testing::kept(GAME_FILE).unwrap();
        bad[0x3FC] ^= 1;
        store::write(GAME_FILE, &bad).unwrap();
        assert_eq!(home_shard(), None);
        testing::reset(false);
    }

    /// STATION's Save Account: the login at +0x10 and +0x50 (63 characters
    /// and the terminator) and the flag at +0x90, the rest of the record as it
    /// was; read back as `Load` reads it; kept through the Options window's
    /// Save Changes; an untick writes them empty. The controls: nothing kept
    /// is empty and off, and a store that refuses keeps nothing.
    #[test]
    fn save_account_keeps_the_login_where_the_card_did() {
        testing::reset(false);
        assert_eq!(saved_login(), (String::new(), String::new(), false));
        let o = changed();
        assert!(save(&o));
        let long = "p".repeat(70);
        assert!(save_login("player0001", &long, true));
        let card = testing::kept(GAME_FILE).unwrap();
        assert_eq!(&card[0x10..0x1A], b"player0001");
        assert!(card[0x1A..0x50].iter().all(|&c| c == 0));
        assert_eq!(&card[0x50..0x8F], "p".repeat(63).as_bytes(), "63 and the terminator");
        assert_eq!(card[0x8F], 0);
        assert_eq!(card[0x90], 1, "m_x90");
        assert_eq!(GameOptions::from_card(&card), Some(o), "the settings as they were");
        assert_eq!(saved_login(), ("player0001".to_string(), "p".repeat(63), true));
        // Save Changes keeps the login
        let mut quiet = o;
        quiet.battle = 0;
        assert!(save(&quiet));
        assert_eq!(saved_login(), ("player0001".to_string(), "p".repeat(63), true));
        assert_eq!(game_options(), quiet);
        // unticked: empty
        assert!(save_login("", "", false));
        let card = testing::kept(GAME_FILE).unwrap();
        assert!(card[0x10..0x91].iter().all(|&c| c == 0));
        assert_eq!(saved_login(), (String::new(), String::new(), false));
        testing::reset(true);
        assert!(!save_login("player0001", "pw", true), "the control: a refusing store");
        testing::reset(false);
        assert_eq!(saved_login(), (String::new(), String::new(), false));
    }

    /// Network Setup's server: kept trimmed, a blank one is none, and a
    /// store that refuses keeps nothing.
    #[test]
    fn the_server_is_kept_trimmed_and_blank_is_none() {
        testing::reset(false);
        assert_eq!(server(), None);
        set_server("  eqoa.example.com \n").unwrap();
        assert_eq!(server().as_deref(), Some("eqoa.example.com"));
        set_server("").unwrap();
        assert_eq!(server(), None, "blank is none");
        testing::reset(true);
        assert!(set_server("192.0.2.1").is_err(), "the control: a refusing store");
        assert_eq!(server(), None);
        testing::reset(false);
    }

    /// The picked disc image: its path kept whole (spaces and all), read
    /// back the same after an editor's line end, none when empty, and
    /// nothing kept by a store that refuses.
    #[test]
    fn the_disc_image_is_kept_whole() {
        testing::reset(false);
        assert_eq!(disc(), None);
        set_disc(r"C:\Games\EQOA Frontiers (USA).iso").unwrap();
        assert_eq!(testing::kept(DISC_FILE).unwrap(), b"C:\\Games\\EQOA Frontiers (USA).iso\n");
        assert_eq!(disc().as_deref(), Some(r"C:\Games\EQOA Frontiers (USA).iso"));
        store::write(DISC_FILE, b" /home/player/my disc.iso \r\n").unwrap();
        assert_eq!(disc().as_deref(), Some(" /home/player/my disc.iso "), "only the line end goes");
        store::write(DISC_FILE, b"\n").unwrap();
        assert_eq!(disc(), None, "empty is none");
        testing::reset(true);
        assert!(set_disc("/x.iso").is_err(), "the control: a refusing store");
        assert_eq!(disc(), None);
        testing::reset(false);
    }

    /// Name Distance's steps: from the game's 22 on to 30, then by tens to
    /// 200, and back to 22 from 30; a number between the tens (one
    /// client.opt held) to the ten past it either way; none past either end.
    /// The control: tens counted from the game's 22 (32, 42 ..) miss 30.
    #[test]
    fn name_distance_steps() {
        let walk = |mut v: u8, d: i32| {
            let mut seen = vec![v];
            while let Some(n) = step_name_distance(v, d) {
                v = n;
                seen.push(v);
            }
            seen
        };
        let up = walk(NAME_DISTANCE_GAME, 1);
        assert_eq!(up.len(), 19, "22, then 30 to 200: {up:?}");
        assert_eq!((up[1], up[2], up[18]), (30, 40, 200));
        assert_eq!(walk(NAME_DISTANCE_MAX, -1), up.iter().rev().copied().collect::<Vec<_>>(), "and back down");
        assert_eq!((step_name_distance(25, 1), step_name_distance(25, -1)), (Some(30), Some(22)));
        assert_eq!((step_name_distance(195, 1), step_name_distance(195, -1)), (Some(200), Some(190)));
        assert_eq!((step_name_distance(22, -1), step_name_distance(200, 1)), (None, None), "its ends");
        let from_22 = |v: u8| v + NAME_DISTANCE_STEP;
        assert_ne!(from_22(NAME_DISTANCE_GAME), up[1], "the control must fail");
    }
}
