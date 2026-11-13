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
    let mut o = account_read("game").or_else(game_card).and_then(|b| GameOptions::from_card(&b)).unwrap_or(GameOptions { compact: 1, ..Default::default() });
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
    let game = write_game(&card).and(account_write("game", &o.to_card()));
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

// ---- the map's waypoints (`map::Map::click`) ------------------------------------------

/// The map's waypoints as this window keeps them (`waypoints.opt`, window n's
/// `waypoints-<n>.opt`): a line a world, `<world> <x> <z>`, its `gWorldInfo`
/// index and the point on its ground. The client's own (the game has no
/// map), kept the moment one is set or cleared and never sent anywhere; a
/// line that does not read is left out.
pub fn waypoints() -> Vec<(i32, [f32; 2])> {
    let Some(b) = store::read(&own_file("waypoints", place())) else { return Vec::new() };
    String::from_utf8_lossy(&b)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let world = it.next()?.parse().ok()?;
            let (x, z): (f32, f32) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?);
            (it.next().is_none() && x.is_finite() && z.is_finite()).then_some((world, [x, z]))
        })
        .collect()
}

/// Keeps this window's waypoints (`waypoints`): whether they were kept.
pub fn save_waypoints(w: &[(i32, [f32; 2])]) -> bool {
    let text: String = w.iter().map(|(world, p)| format!("{world} {} {}\n", p[0], p[1])).collect();
    match store::write(&own_file("waypoints", place()), text.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            crate::say(&format!("the map's waypoint was not kept: {e}"));
            false
        }
    }
}

// ---- the HUD's pieces where the player put them (`hudlayout`) ---------------------------

/// The HUD's layout as this window keeps it (`hud.opt`, window n's
/// `hud-<n>.opt`): `hudlayout::Layout::to_text`'s lines, empty when nothing
/// was moved. The client's own, kept the moment a drag ends or Reset HUD is
/// chosen, never sent anywhere.
pub fn hud_layout() -> String {
    account_read("hud").or_else(|| store::read(&own_file("hud", place()))).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// Keeps this window's HUD layout (`hud_layout`): whether it was kept.
pub fn save_hud_layout(text: &str) -> bool {
    match store::write(&own_file("hud", place()), text.as_bytes()).and(account_write("hud", text.as_bytes())) {
        Ok(()) => true,
        Err(e) => {
            crate::say(&format!("the HUD's layout was not kept: {e}"));
            false
        }
    }
}

// ---- the quest guide's record (`questguide`) --------------------------------------------

/// The quest guide's file: what it keeps of each character (the quests it
/// finished, its home city, the kills seen), one line a character
/// (`questguide::Kept`). One file for every window, as a character is the same
/// in any of them: read again before each write, so another window's lines
/// stay.
const QUEST_GUIDE_FILE: &str = "questguide.opt";

/// The quest guide's file as text (empty when there is none).
pub fn quest_guide() -> String {
    store::read(QUEST_GUIDE_FILE).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// Keeps the quest guide's file: whether it was kept.
pub fn save_quest_guide(text: &str) -> bool {
    match store::write(QUEST_GUIDE_FILE, text.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            crate::say(&format!("the quest guide's record was not kept: {e}"));
            false
        }
    }
}

// ---- the auto slots (`autocast`) --------------------------------------------------

/// The auto slots' file as text (empty when there is none): a line a
/// character, one file for every window, read again before each write (as
/// the quest guide's).
pub fn auto_cast_file() -> String {
    store::read(crate::autocast::FILE).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// Keeps the auto slots' file: whether it was kept.
pub fn save_auto_cast_file(text: &str) -> bool {
    match store::write(crate::autocast::FILE, text.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            crate::say(&format!("the auto slots were not kept: {e}"));
            false
        }
    }
}

// ---- the account's options (petition 1621) --------------------------------------------

/// The options are the ACCOUNT's (the GM Jibrish's petition 1621, "account
/// bound, not just character bound"): once a login names one
/// (`set_account`: the server and the login name, the pregame's STATION),
/// the Options window's settings (game.opt's), the Extended rows (client.opt's
/// own rows) and the HUD's layout are read from and kept in the account's
/// files, `acct_<server>+<login>.game.opt`, `.client.opt` and `.hud.opt`, so
/// every character of the account, in any window, plays with the same.
/// The window's own files are written as well (the title before a login,
/// and a build before this one, read them). What stays as it was: the keys,
/// buttons and macros (`keys.opt`, the computer's: every account's), the
/// saved login and home shard (game.opt's, the window's), each window's
/// Controller Focus and the settings every window shares (client.opt), the
/// window's place and size, the map's waypoints (the window's), and what
/// is a character's own: its quick bars (the server's), its auto slots and
/// its quest guide record (a line a character).
///
/// A window remembers the account it last logged into (`account.opt`,
/// window n's `account-<n>.opt`), so the title shows and keeps that
/// account's options too. The account's first login on a client of this
/// build makes its files from the window's set most recently kept (`migrate`).
fn account_key() -> Option<String> {
    account_now().or_else(|| store::read(&own_file("account", place())).map(|b| String::from_utf8_lossy(&b).trim().to_string()).filter(|k| !k.is_empty()))
}

/// The account's key, a store name's part: the server and the login,
/// lower case, anything but letters, digits, `.`, `-` and `_` as `_`.
pub fn account_name(server: &str, user: &str) -> String {
    let clean = |t: &str| t.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect::<String>();
    format!("{}+{}", clean(server), clean(user))
}

/// The account's file `what` (`game`, `client`, `hud`).
fn account_file(key: &str, what: &str) -> String {
    format!("acct_{key}.{what}.opt")
}

/// The account's file `what`, when an account is known and it was kept.
fn account_read(what: &str) -> Option<Vec<u8>> {
    store::read(&account_file(&account_key()?, what))
}

/// Keeps the account's file `what` (nothing with no account known).
fn account_write(what: &str, b: &[u8]) -> Result<(), String> {
    match account_key() {
        Some(k) => store::write(&account_file(&k, what), b),
        None => Ok(()),
    }
}

/// A login (`server`, the login name): from now the options are this
/// account's, the account's files made first if it has none (`migrate`);
/// the options read again (`client`'s cache, the game's options at the
/// world's entry, the HUD's layout).
pub fn set_account(server: &str, user: &str) {
    if user.trim().is_empty() {
        return;
    }
    let key = account_name(server, user);
    if account_now().as_deref() == Some(key.as_str()) {
        return;
    }
    let had = ["game", "client", "hud"].iter().any(|w| store::read(&account_file(&key, w)).is_some());
    set_account_now(Some(key.clone()));
    let _ = store::write(&own_file("account", place()), format!("{key}\n").as_bytes());
    if !had {
        migrate(&key);
    }
    bump_generation();
}

/// The account's files made from the window's set kept most recently (the
/// newest of each window's game, client and HUD files; in a page, whose
/// storage keeps no times, this window's own): the settings, never the
/// login or the home shard.
fn migrate(key: &str) {
    let files = |p: usize| ["game", "client", "hud"].map(|w| own_file(w, p));
    let newest = (0..crate::multibox::PLACES).filter_map(|p| files(p).iter().filter_map(|f| store::modified(f)).max().map(|t| (t, p))).max().map(|(_, p)| p);
    let from = newest.unwrap_or_else(place);
    if let Some(o) = store::read(&own_file("game", from)).and_then(|b| GameOptions::from_card(&b)) {
        let _ = store::write(&account_file(key, "game"), &o.to_card());
    }
    if let Some(b) = store::read(&own_file("client", from)) {
        let _ = store::write(&account_file(key, "client"), ClientOptions::from_text(&String::from_utf8_lossy(&b)).rows_text().as_bytes());
    }
    if let Some(b) = store::read(&own_file("hud", from)) {
        let _ = store::write(&account_file(key, "hud"), &b);
    }
    crate::say(&format!("options: kept for the account from now on (from window {}'s)", from + 1));
}

/// The notice the Save Changes box carries (petition 1621: "warn them this
/// will now save for all characters on the account") until the player has
/// said Yes to it once (`account_notice_seen`), when an account is known.
pub const ACCOUNT_NOTICE: &str = "Options now save for every character on this account.";
const NOTICE_FILE: &str = "account-notice.opt";

pub fn account_notice_due() -> bool {
    account_key().is_some() && !noticed() && store::read(NOTICE_FILE).is_none()
}

/// The notice seen: kept, and not shown again this run even if keeping failed.
pub fn account_notice_seen() {
    set_noticed();
    let _ = store::write(NOTICE_FILE, b"seen\n");
}

/// `client`'s cache key: the window's place and the account's generation
/// (a login reads the options again, on every thread that cached them).
fn cache_key() -> usize {
    place() + crate::multibox::PLACES * generation()
}

#[cfg(not(test))]
mod account_state {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    static ACCOUNT: Mutex<Option<String>> = Mutex::new(None);
    static GENERATION: AtomicUsize = AtomicUsize::new(0);
    static NOTICED: AtomicBool = AtomicBool::new(false);
    pub fn account_now() -> Option<String> {
        ACCOUNT.lock().map(|a| a.clone()).unwrap_or(None)
    }
    pub fn set_account_now(k: Option<String>) {
        if let Ok(mut a) = ACCOUNT.lock() {
            *a = k;
        }
    }
    pub fn generation() -> usize {
        GENERATION.load(Ordering::Relaxed)
    }
    pub fn bump_generation() {
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
    pub fn noticed() -> bool {
        NOTICED.load(Ordering::Relaxed)
    }
    pub fn set_noticed() {
        NOTICED.store(true, Ordering::Relaxed);
    }
}

/// The tests': each thread's own, as their store is.
#[cfg(test)]
mod account_state {
    use std::cell::{Cell, RefCell};
    thread_local! {
        pub static ACCOUNT: RefCell<Option<String>> = const { RefCell::new(None) };
        pub static GENERATION: Cell<usize> = const { Cell::new(0) };
        pub static NOTICED: Cell<bool> = const { Cell::new(false) };
    }
    pub fn account_now() -> Option<String> {
        ACCOUNT.with(|a| a.borrow().clone())
    }
    pub fn set_account_now(k: Option<String>) {
        ACCOUNT.with(|a| *a.borrow_mut() = k);
    }
    pub fn generation() -> usize {
        GENERATION.with(|g| g.get())
    }
    pub fn bump_generation() {
        GENERATION.with(|g| g.set(g.get() + 1));
    }
    pub fn noticed() -> bool {
        NOTICED.with(|n| n.get())
    }
    pub fn set_noticed() {
        NOTICED.with(|n| n.set(true));
    }
}
use account_state::{account_now, bump_generation, generation, noticed, set_account_now, set_noticed};

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

/// The server a client that never kept one connects to (the user, 2026-11-08:
/// "lets make eqoaclassic.com the default network config for the vanilla
/// client. apk users reporting they cannot login"): natively alone, as a page
/// is told its own (`web`'s `server` option) and another site's page must not
/// be sent here.
pub const DEFAULT_SERVER: &str = "eqoaclassic.com";

/// The server the title's Network Setup keeps (`server.opt`: the address
/// alone, a host name or IPv4 address), if one was set; a blank one kept is
/// none. With no `server.opt` at all (a new install, a phone's) it is
/// [`DEFAULT_SERVER`], natively.
pub fn server() -> Option<String> {
    match store::read(SERVER_FILE) {
        Some(b) => Some(String::from_utf8_lossy(&b).trim().to_string()).filter(|s| !s.is_empty()),
        None => default_server(),
    }
}

/// [`DEFAULT_SERVER`] natively; none in a page.
fn default_server() -> Option<String> {
    (!cfg!(target_arch = "wasm32")).then(|| DEFAULT_SERVER.to_string())
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

/// The Profanity Filter's tier (Options > Interface; the GM Jibrish's
/// petition 1625): the server's filter of what the player hears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profanity {
    /// Nothing masked.
    Off,
    /// Only racial slurs masked, no swears ("retarded" not either).
    Medium,
    /// Every profane word and slur masked: the game's at every login, and
    /// what On was before the tiers.
    #[default]
    High,
}

impl Profanity {
    /// The row's order, which left and right step through (and round).
    pub const ALL: [Profanity; 3] = [Profanity::Off, Profanity::Medium, Profanity::High];

    /// As `client.opt` spells it; "on" (before the tiers) reads as High.
    pub fn key(self) -> &'static str {
        match self {
            Profanity::Off => "off",
            Profanity::Medium => "medium",
            Profanity::High => "high",
        }
    }

    pub fn from_key(s: &str) -> Option<Profanity> {
        if s == "on" {
            return Some(Profanity::High);
        }
        Profanity::ALL.into_iter().find(|p| p.key() == s)
    }

    /// The row's value, the client's words.
    pub fn label(self) -> &'static str {
        match self {
            Profanity::Off => "Off",
            Profanity::Medium => "Medium",
            Profanity::High => "High",
        }
    }

    /// As go-eqoa-server's 0x7E60 takes it (`net::session::PROFANITY_*`).
    pub fn wire(self) -> u8 {
        match self {
            Profanity::Off => net::session::PROFANITY_OFF,
            Profanity::Medium => net::session::PROFANITY_MEDIUM,
            Profanity::High => net::session::PROFANITY_HIGH,
        }
    }

    /// The next tier along the row, `d` < 0 back, rounding at the ends.
    pub fn step(self, d: i32) -> Profanity {
        let n = Profanity::ALL.len();
        let at = Profanity::ALL.iter().position(|&p| p == self).unwrap_or(n - 1);
        Profanity::ALL[(at + if d < 0 { n - 1 } else { 1 }) % n]
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
    /// Slow When Not Focused (the GM Jibrish's petition 1634), each window's
    /// by its place as Controller Focus is: on, a window the system reports
    /// covered or minimised takes its frames at the game's 30 a second
    /// (`viewer`'s `HIDDEN_GAP`); off, the default, at its Frame Rate as in
    /// front, so a box in the back keeps its pace
    pub slow_unfocused: [bool; crate::multibox::PLACES],
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
    /// The Raid List (`raidlist`): a raid group's every name and health on the
    /// HUD where the server sends them, in place of the group's bars, which
    /// hold three. On by default (the user, 2026-11-07).
    pub raid_list: bool,
    /// Pet Effects (`hud`'s pet bar; the user, 2026-11-09: "put the buff icons
    /// under the pet name plate"): the effects on the player's pet in a row
    /// under its bar, where the server sends them (`Session::agree_pet_effects`);
    /// off, none, as the game shows them nowhere. On by default.
    pub pet_effects: bool,
    /// Buff Window (`buffwnd`; the GM Jibrish's petition 1761): on a server that
    /// offers it (`Session::agree_buffs`), the player's effects in two rows and
    /// its pet's in a third, each with its name and time left and a beneficial
    /// one taken off by a click or the pad; off, or on any other server, the
    /// game's strip (`influence`). On by default.
    pub buff_window: bool,
    /// Quest Markers over the heads (Extended > UI > Name Plates; the GM
    /// Jibrish's petition 1813): on a server that sends them
    /// (`Session::agree_head_marks`), a yellow "!" over an NPC with a quest
    /// the player may take now and a "?" over one where a quest can be moved
    /// on now, above its name; off, or on any other server, none. On by
    /// default. (The map's quest marks are `quest_markers`, the Quest
    /// Helper's.)
    pub head_marks: bool,
    /// Power Bars (Extended > UI > HUD; the server owner's of 2026-11-12, "lets
    /// add power bars to group/raid display"): a thin blue bar of each member's
    /// power under its health in the group's bars (a server's others' power,
    /// `Session::agree_power_bars`: go-eqoa-server's 0x7E01/0x7E02, players and
    /// mercs, and the pet's under its bar) and in the raid list's rows (0x7E1A's
    /// power, which the roster has always carried). Off, or on a server that
    /// sends none, the game's bars. On by default.
    pub power_bars: bool,
    /// Raid Mercenaries (Extended > UI > Group & Raid; the owner, 2026-11-13:
    /// "lets hide mercs from raid group display"): false, Hidden (the
    /// default), the raid list draws its players' rows alone; true, Shown, the
    /// raid's mercs' rows too, each with its coin (`raidlist::row_role`), as
    /// go-eqoa-server's roster sends them (0x7E1A's kind byte). Either way a
    /// merc is targeted in the world and by L1's group cycle.
    pub raid_mercs: bool,
    /// Loading Tips (`tips`; the GM Jibrish's petition 1778): a tip
    /// line on every loading screen. On by default.
    pub loading_tips: bool,
    /// Number Colors (`numcolor`; the server owner's of 2026-11-09: "we can add
    /// the number colors to extended menu"): each row's colour for the floating
    /// numbers of the player's own figures and its pet's, by what made them, on a
    /// server that says so (go-eqoa-server's tags), in `numcolor::ROWS`' order:
    /// 0 Default (the game's own colour), else `numcolor::PALETTE[n - 1]`. Every
    /// row Default by default but the merc's two (Merc Colors, petition 1613:
    /// `numcolor::DEFAULTS`), so only a merc's numbers change until one is
    /// chosen.
    pub number_colors: [crate::numcolor::Choice; crate::numcolor::ROWS],
    /// DPS Meter (`dpsmeter`; the server owner's of 2026-11-10: "lets add dps
    /// meters, will work solo, group, raid"): the HUD's damage meter of the
    /// player's group, where the server keeps one (go-eqoa-server's offer,
    /// `Session::agree_dps_meter`); off, none, and the server asked for none, as
    /// the game has none. Off by default: a panel over the world's right side.
    pub dps_meter: bool,
    /// DPS Meter Shows: the fight (under way, or the last) or Overall, every
    /// fight since the login; a click on the panel's title or `/dps` turns it.
    pub dps_view: crate::dpsmeter::View,
    /// Item Links (`itemlink`; the GM Jibrish's petition 1463: "This should be
    /// off by default for ps2 players and a toggle option. If toggled off for
    /// players, they should just see a plain text item name if linked"): links
    /// made from the inventory, drawn and opened in the chat box, on a server
    /// that offers them (`Session::agree_item_links`); off, none, and the server
    /// sends the plain names a PS2 is sent. On by default.
    pub item_links: bool,
    /// Auto-Cast (`autocast`; the server owner's of 2026-11-11: "lets add
    /// auto-cast to abilities to fire off cooldown/timer/buff gone/pet's die
    /// similar to the mercs"): the quick bars' slots marked auto used by the
    /// client itself; off, none is, whatever is marked. On by default: no slot
    /// is auto until the player marks one.
    pub auto_cast: bool,
    /// Auto Target (`autocast`, the same day's "also auto targeting"): with no
    /// target or a dead one, the nearest creature fighting the player (else
    /// its pet or group) targeted. Off by default.
    pub auto_target: bool,
    /// Chat Always Shown (Jibwiz's ticket 1914: "allows your chat box to stay
    /// fully visible without auto collapse"): the chat box open at its five
    /// rows with the last lines in it, never sliding away (`chat::WndChat::
    /// always`). Off by default, the game's box.
    pub chat_always: bool,
    /// Stacked Spell Bars (Jibwiz's ticket 1913): on a computer, the first two
    /// ability bars drawn as two rows of five stacked above the chat box at
    /// 95%, with no name bar, each slot a key of its own on the Controls page
    /// (`hudmenu::Stacked`, `binds`' bar rows). Off by default, the game's
    /// carousel; a controller or a touch screen keeps the carousel.
    pub stacked_bars: bool,
    /// Spell Bar Opacity (Jibwiz's ticket 1953: "the horizontal spell bars need
    /// opacity as a setting for the non spell icon portion of the bar"): the
    /// stacked bars' cell frames drawn at this percent (0 to 100 in tens), the
    /// icons always whole (`hudmenu::HudMenus::draw_stacked`). 100 by default,
    /// the frames as before.
    pub spell_bar_opacity: u8,
    /// Chat Tabs (Jibwiz's ticket 1951): which of the chat box's tabs each
    /// kind of line goes to (`chat::Kind`, by its index): 0 the box's own,
    /// Main; 1 to 3 a tab of its own, which exists while a kind goes to it and
    /// whose lines Main no longer shows. Every kind Main's by default: no tabs.
    pub chat_tabs: [u8; crate::chat::KINDS],
    /// The tabs' names (`/tabname`), eight characters at most, NUL past the
    /// end; none, the tab is called after the first kind that goes to it.
    pub chat_tab_names: [[char; crate::chat::TAB_NAME_MOST]; crate::chat::TABS],
    /// Pets Pass Through (Thorinoak's ticket 1977, "can we make pets auto
    /// ignored, the clipping on them is terrible"): a pet's or a merc's
    /// (a player's side's: the plate colour 3) capsule collides with no one, as
    /// an ignored player's (`Play::place_actors`). On by default; off, the
    /// game's: every creature is solid.
    pub pets_pass: bool,
    /// Same Shortcuts on Every Character (Piedpiper's ticket 1987): the
    /// quick-chat menus (L2's, `quickchat`) kept for the account, every
    /// character of it given them at its world entry, an edit on any kept
    /// for all (`Play::sync_shortcuts`); turned on, the character in the
    /// world gives the account its menus once. Off by default, the game's:
    /// each character its own, as the server keeps them.
    pub shared_shortcuts: bool,
    /// Auto Save Options (Waldorf's ticket 2002: "A toggle that autosaves
    /// changes to your options instead of having to go back to the 'Save
    /// Changes'"): every step back in the Options window keeps what was
    /// changed as Save Changes keeps it, and Triangle on its menu saves
    /// instead of putting the settings back (`options::WndOptions::keep`).
    /// Off by default, the game's: only Save Changes keeps them.
    pub auto_save: bool,
    /// Profanity Filter (the GM Jibrish's petition 1490: "Under options then
    /// interface add an option to turn the profanity filter on or off"): the
    /// server's filter of what this player hears, told to a server that asks
    /// for it (`Session::agree_profanity_filter`, go-eqoa-server's 0x7E60);
    /// High by default, as the game has it at every login. Options >
    /// Interface. Three tiers since the GM Jibrish's petition 1625: Off,
    /// Medium (only racial slurs) and High (every word, the old On).
    pub profanity_filter: Profanity,
    /// Map (the user, 2026-11-09: "also add the map and quest helper we had
    /// in a former update"; on by default): the client's own map (M, the
    /// compass's click, `map`), the minimap, the waypoints on the compass and
    /// the group's waypoint said in its chat; off, none of them, as the game
    /// has none (M does nothing, the compass is the game's, a group line is a
    /// chat line, nothing is built or kept, the community's marks are not
    /// asked for). Its own parts under it: Minimap (N turns it too), Map
    /// Markers (the map's X or ○) and the town names (the map's T or Select).
    pub map: bool,
    /// Minimap (on by default): the map's pictures round the player under
    /// the quick menus' name plate (`map::mini`); off, none.
    pub minimap: bool,
    /// Minimap Scroll Frame (the owner, 2026-11-13: "make the minimap have
    /// the borders like the spell bar the scroll, with the rollers on top and
    /// bottom"; on by default): the minimap framed as the quick menu's
    /// scroll, its parchment round the pictures and a roller across its top
    /// and its bottom (`map::mini::draw_scroll`); off, the window's frame it
    /// had before.
    pub minimap_scroll: bool,
    /// Map Pointer (petition 1848; the owner: "add as a toggle"): what the
    /// player's dart on the minimap and the map turns with, Facing (the way
    /// the character faces, as it always was; the default) or Camera (the
    /// way the camera looks, as the compass turns): `map::Pointer`.
    pub map_pointer: crate::map::Pointer,
    /// Map Markers: on, the map (M) marks EQOA-Data's XP camps and every
    /// coach (`map::markers`), each with a card of what it is under the
    /// pointer (the user, F3 pass 18: "mark popular xp camps on map with an XP
    /// icon", "also add all coaches to the map"); off, the map shows neither.
    /// The map's own key (X, ○) turns it too.
    pub map_markers: bool,
    /// Town names on the map (the user, F3 pass 18: "add town names to the
    /// map too", `map::markers`): the map's own key (T, Select) turns them,
    /// kept as Map Markers is; on by default.
    pub map_towns: bool,
    /// Quest Helper (the user, 2026-11-09; on by default): the quest marks on
    /// the map and the minimap (a "!" at each giver of a quest the player may
    /// take, a "?" where a step is to be done) and the quest tracker on the
    /// HUD, from a server that sends them (`net::session::OP_QUEST_MARKERS`,
    /// `OP_QUEST_PROGRESS`: go-eqoa-server), else the client's own guide's
    /// (`questguide`: Sandstorm sends no such thing); off, none, the server
    /// asked for none and the guide not run, as the game has none.
    pub quest_helper: bool,
    /// Quest Markers (under the Quest Helper, on by default): the map's and
    /// the minimap's quest marks; the map's own key (J, R3) turns it.
    pub quest_markers: bool,
    /// Quest Tracker (under the Quest Helper, on by default): the HUD's list
    /// of the quests under way under the minimap, each step's objectives
    /// counted ("Bat Eye 0/3"); off, none.
    pub quest_tracker: bool,
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
    /// Ground Fog: PS2, the effect volumes' fog blended as the GS blends it
    /// into the PS2's 16-bit frame, dithered and cut to five bits a channel
    /// (`render::fog16`: a low, dim band, as the game shows it); Exact, blended
    /// at eight bits a channel (a brighter cloud). PS2 by default, the
    /// game's look (the user, 2026-10-30, named no default: flip it here).
    pub ps2_fog: bool,
    /// Particles: PS2, the particles of blend mode 3 (the Red Swarming
    /// Death's swarm, the flies and stench round the rotting, smoke) blended
    /// as the GS blends them into the PS2's 16-bit frame, dithered and cut to
    /// five bits a channel (`render::Renderer::set_ps2_particles`: many faint
    /// puffs stop short of their colour, each its own, as the game shows
    /// them); Exact, blended at eight bits a channel (they pile up into a
    /// darker mass). PS2 by default, the game's look, as Ground Fog's.
    pub ps2_particles: bool,
    /// Ambient Occlusion (the user's of 2026-11-08, "lets add ambient
    /// occlusion"): off, the picture as the game draws it; on, the opaque
    /// scene's creases, corners and contacts darkened per pixel
    /// (`render::ao`). Off by default.
    pub ambient_occlusion: bool,
    /// Sun Shadows (Extended > Lighting, 2026-11-08): on, and with Dynamic
    /// Lighting on, the sun's light stopped by what stands between a surface
    /// and the sun, walls, roofs, trees, characters and hills casting
    /// shadows that move with the time of day (`render::sunshadow`); off, the
    /// sun lights through everything, as the PS2's. Off by default.
    pub sun_shadows: bool,
    /// Light Shadows (Extended > Lighting, 2026-11-09, the user's "sun
    /// shadows lets make all light sources act similar"): on, and with
    /// Dynamic Lighting on, the fires', the personal light's and the spells'
    /// light stopped by what stands between a surface and the light, as Sun
    /// Shadows stop the sun's (`render::lightshadow`); off, those lights
    /// light through everything. Off by default.
    pub light_shadows: bool,
    /// Shadow Distance (Extended > Lighting, 2026-11-09, the user's "the light
    /// shadows need a slider to raise to a greater distance"): how far out
    /// Sun Shadows and Light Shadows reach, from the game's
    /// `SHADOW_DISTANCE_GAME` (Sun Shadows' 250 before the camera, 8 lights
    /// shadowed, 4 in a page and on Android) to `SHADOW_DISTANCE_MAX` (1000,
    /// every light Dynamic Lighting draws) in `SHADOW_DISTANCE_STEP`s
    /// (`render::Renderer::set_shadow_distance`).
    pub shadow_distance: u16,
    /// Dynamic Lighting (the user's of 2026-11-08, phase 1): off, the
    /// PS2's, the strip programs' light per vertex with the personal light
    /// the one point light; on, the light per fragment and the scene's fires,
    /// placed lights, spells and glowing weapons lighting what is round them
    /// (`lights`, `render::lit`). Off by default.
    pub dynamic_lighting: bool,
    /// Loading Screens: Longer, a long trip (Return Home, a coach, a port of
    /// 200 or more with zones a PS2 would read, another world) keeps the
    /// loading screen up at least a second, as a PS2's disc read would
    /// (`loading::FAR_HOLD_MS`, `loading::needs_read`); Instant,
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
    /// Touch Stick Size and Touch Button Size: the touch screen's pad's
    /// sticks and buttons (`touch::layout`), each percent of the pad as it
    /// was, `TOUCH_SIZE_MIN` to `TOUCH_SIZE_MAX` in `TOUCH_SIZE_STEP`s, drawn
    /// larger than it was only as far as the window has room (a player,
    /// 2026-11-02: "Is there a way to make the on screen controller size
    /// adjustable?"; Jibrish's petition 1386, 2026-11-09: "Mobile clients and
    /// mobile browser need the ability to adjust the on screen controler
    /// size"). They took over Touch Pad Size, the whole pad's 50 to 100,
    /// which an older client.opt's `touch_size` gives both.
    pub touch_stick: u8,
    pub touch_buttons: u8,
    /// Touch Opacity: the pad's alphas, percent of its own (`touch`'s
    /// `TouchPad::draw`), `TOUCH_OPACITY_MIN` to `TOUCH_OPACITY_MAX` in
    /// `TOUCH_OPACITY_STEP`s.
    pub touch_opacity: u8,
    /// Touch Edge: how far in from the window's left and right edges the
    /// pad keeps, percent of the window's shorter side, 0 to
    /// `TOUCH_EDGE_MAX` (`touch::layout`).
    pub touch_edge: u8,
    /// Texture Packs, natively (the user, 2026-11-08: PCSX2's replacement
    /// packs read as they are, "an Extended page row turns packs on and
    /// off"): on, a texture a pack in `textures/SLUS-20744/replacements`
    /// names (the `textures` beside the program, `texpack::place`) is drawn
    /// as the pack has it (`texpack`); off, every texture the game's own.
    /// Off by default, as PCSX2's Load Textures is.
    pub texture_packs: bool,
    /// Dump Textures, natively: on, each texture made is written to
    /// `textures/SLUS-20744/dumps` under the names PCSX2 gives it
    /// (`texpack`), for a pack to be made from. Off by default.
    pub dump_textures: bool,
}

/// Render Scale's steps, the first the game's picture as drawn.
pub const RENDER_SCALES: [u8; 3] = [1, 2, 4];

/// Touch Stick Size's and Touch Button Size's range (percent of the pad as
/// it was, `TOUCH_SIZE_WAS`: `touch::layout`) and their step.
pub const TOUCH_SIZE_MIN: u8 = (crate::touch::SMALLEST * 100.0) as u8;
pub const TOUCH_SIZE_WAS: u8 = 100;
pub const TOUCH_SIZE_MAX: u8 = (crate::touch::LARGEST * 100.0) as u8;
pub const TOUCH_SIZE_STEP: u8 = 5;
/// Touch Opacity's range (percent of the pad's own alphas, `TOUCH_OPACITY_WAS`)
/// and its step.
pub const TOUCH_OPACITY_MIN: u8 = (crate::touch::FAINTEST * 100.0 + 0.5) as u8;
pub const TOUCH_OPACITY_WAS: u8 = 100;
pub const TOUCH_OPACITY_MAX: u8 = (crate::touch::BRIGHTEST * 100.0) as u8;
pub const TOUCH_OPACITY_STEP: u8 = 10;
/// Touch Edge's most (percent of the window's shorter side) and its step.
pub const TOUCH_EDGE_MAX: u8 = (crate::touch::EDGE_MOST * 100.0 + 0.5) as u8;
pub const TOUCH_EDGE_STEP: u8 = 1;

/// Shadow Distance's ends and step (`ClientOptions::shadow_distance`).
pub const SHADOW_DISTANCE_GAME: u16 = 250;
pub const SHADOW_DISTANCE_MAX: u16 = 1000;
pub const SHADOW_DISTANCE_STEP: u16 = 125;

/// Shadow Distance as the renderer takes it: times the game's.
pub fn shadow_scale() -> f32 {
    client().shadow_distance as f32 / SHADOW_DISTANCE_GAME as f32
}

/// How the touch screen's pad is drawn now (`touch::layout`, `TouchPad::draw`):
/// the Extended page's Touch Stick Size, Touch Button Size, Touch Opacity and
/// Touch Edge.
pub fn touch_options() -> crate::touch::Opts {
    client().touch_opts()
}

impl ClientOptions {
    /// Its touch rows as the pad takes them (`touch::Opts`).
    pub fn touch_opts(&self) -> crate::touch::Opts {
        let share = |v: u8| v as f32 / 100.0;
        crate::touch::Opts { stick: share(self.touch_stick), buttons: share(self.touch_buttons), opacity: share(self.touch_opacity), edge: share(self.touch_edge) }
    }
}

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
            slow_unfocused: [false; crate::multibox::PLACES],
            capture: false,
            background_sound: true,
            ability_bars: ABILITY_BARS_GAME,
            entity_slots: ENTITY_SLOTS_GAME,
            entity_view: ENTITY_VIEW_GAME,
            name_distance: NAME_DISTANCE_GAME,
            overlapping_names: false,
            raid_list: true,
            pet_effects: true,
            buff_window: true,
            head_marks: true,
            power_bars: true,
            raid_mercs: false,
            loading_tips: true,
            number_colors: crate::numcolor::DEFAULTS,
            dps_meter: false,
            dps_view: crate::dpsmeter::View::Fight,
            item_links: true,
            auto_cast: true,
            auto_target: false,
            chat_always: false,
            stacked_bars: false,
            spell_bar_opacity: 100,
            chat_tabs: [0; crate::chat::KINDS],
            chat_tab_names: [['\0'; crate::chat::TAB_NAME_MOST]; crate::chat::TABS],
            pets_pass: true,
            shared_shortcuts: false,
            auto_save: false,
            profanity_filter: Profanity::High,
            map: true,
            minimap: true,
            minimap_scroll: true,
            map_pointer: crate::map::Pointer::Facing,
            map_markers: true,
            map_towns: true,
            quest_helper: true,
            quest_markers: true,
            quest_tracker: true,
            art: esf::Era::Frontiers,
            vsync: true,
            shouts: true,
            update_notices: true,
            text: render::ui::TextFilter::Sharp,
            mipmaps: true,
            flora: true,
            shadows: true,
            environment: true,
            ps2_fog: true,
            ps2_particles: true,
            ambient_occlusion: false,
            sun_shadows: false,
            light_shadows: false,
            shadow_distance: SHADOW_DISTANCE_GAME,
            dynamic_lighting: false,
            loading_longer: true,
            render_scale: 1,
            hud: render::ui::HudFilter::Nearest,
            output: None,
            touch_stick: TOUCH_SIZE_WAS,
            touch_buttons: TOUCH_SIZE_WAS,
            touch_opacity: TOUCH_OPACITY_WAS,
            touch_edge: 0,
            texture_packs: false,
            dump_textures: false,
        }
    }
}

impl ClientOptions {
    /// `client.opt`: a line a setting, `name=value`; Camera Turn Speed and
    /// Fog only when they are not the game's, Screen Shake only when off, a
    /// window's Controller Focus as `controller.<window>` (from 1) only when
    /// it is not Auto, a window's Slow When Not Focused as
    /// `slow_unfocused.<window>=on` only when on, Capture Controller only when on, Sound in Background
    /// only when off, Ability Bars, NPC/PC Display, NPC/PC Distance and
    /// Name Distance only when not the game's, Overlapping Names only when
    /// Show, each Number Colors row only when not Default
    /// (`number_color.<row>=<colour>`), the DPS Meter only when on and what it
    /// shows only when Overall, the Map, Minimap, Map Markers, the map's town names, the Quest
    /// Helper, Quest Markers and Quest Tracker only when off, Art only when it
    /// is the 2002 disc's, VSync, Shouts and Update
    /// Notices only when off, Text only when Smooth, Render Scale only above
    /// 1, HUD only when Nearest, Output Device only when a device is
    /// chosen (by its name), Touch Stick Size, Touch Button Size, Touch
    /// Opacity and Touch Edge only when not the pad as it was (no Touch Pad
    /// Size, which they took over), and Texture Packs and Dump Textures only
    /// when on (Ambient Occlusion and Dynamic Lighting the same).
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
        for (k, _) in self.slow_unfocused.iter().enumerate().filter(|(_, on)| **on) {
            s.push_str(&format!("slow_unfocused.{}=on\n", k + 1));
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
        if !self.raid_list {
            s.push_str("raid_list=off\n");
        }
        if !self.pet_effects {
            s.push_str("pet_effects=off\n");
        }
        if !self.buff_window {
            s.push_str("buff_window=off\n");
        }
        if !self.head_marks {
            s.push_str("head_marks=off\n");
        }
        if !self.power_bars {
            s.push_str("power_bars=off\n");
        }
        // Raid Mercenaries only when Shown
        if self.raid_mercs {
            s.push_str("raid_mercs=shown\n");
        }
        if !self.loading_tips {
            s.push_str("loading_tips=off\n");
        }
        // Number Colors only where a row is not as it came (Default, or a
        // merc row's colour: `numcolor::DEFAULTS`), a row chosen Default as
        // `default`
        for ((key, &c), d) in crate::numcolor::KEYS.iter().zip(&self.number_colors).zip(crate::numcolor::DEFAULTS) {
            if c != d {
                s.push_str(&format!("number_color.{key}={}\n", crate::numcolor::paint(c).map_or("default", |p| p.key)));
            }
        }
        // the DPS Meter only when on, what it shows only when Overall
        if self.dps_meter {
            s.push_str("dps_meter=on\n");
        }
        if self.dps_view != crate::dpsmeter::View::Fight {
            s.push_str(&format!("dps_meter_shows={}\n", self.dps_view.key()));
        }
        // Item Links only when off
        // Auto-Cast only when off, Auto Target only when on
        if !self.auto_cast {
            s.push_str("auto_cast=off\n");
        }
        if self.auto_target {
            s.push_str("auto_target=on\n");
        }
        // Chat Always Shown and Stacked Spell Bars only when on
        if self.chat_always {
            s.push_str("chat_always=on\n");
        }
        if self.stacked_bars {
            s.push_str("stacked_bars=on\n");
        }
        if self.spell_bar_opacity != 100 {
            s.push_str(&format!("spell_bar_opacity={}\n", self.spell_bar_opacity));
        }
        for (k, &t) in self.chat_tabs.iter().enumerate() {
            if t != 0 {
                s.push_str(&format!("chat_tab.{}={t}\n", crate::chat::Kind::ALL[k].key()));
            }
        }
        for (n, name) in self.chat_tab_names.iter().enumerate() {
            let name: String = name.iter().take_while(|&&c| c != '\0').collect();
            if !name.is_empty() {
                s.push_str(&format!("chat_tab_name.{}={name}\n", n + 1));
            }
        }
        if !self.pets_pass {
            s.push_str("pets_pass=off\n");
        }
        if self.shared_shortcuts {
            s.push_str("shared_shortcuts=on\n");
        }
        // Auto Save Options only when on
        if self.auto_save {
            s.push_str("auto_save_options=on\n");
        }
        if !self.item_links {
            s.push_str("item_links=off\n");
        }
        // the Profanity Filter only when not High
        if self.profanity_filter != Profanity::High {
            s.push_str(&format!("profanity_filter={}\n", self.profanity_filter.key()));
        }
        // the map's and the quests' only when off
        for (on, key) in [
            (self.map, "map"),
            (self.minimap, "minimap"),
            (self.minimap_scroll, "minimap_scroll"),
            (self.map_markers, "map_markers"),
            (self.map_towns, "map_towns"),
            (self.quest_helper, "quest_helper"),
            (self.quest_markers, "quest_markers"),
            (self.quest_tracker, "quest_tracker"),
        ] {
            if !on {
                s.push_str(&format!("{key}=off\n"));
            }
        }
        // Map Pointer only when Camera
        if self.map_pointer != crate::map::Pointer::Facing {
            s.push_str(&format!("map_pointer={}\n", self.map_pointer.key()));
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
        // Ground Fog only when Exact
        if !self.ps2_fog {
            s.push_str("ground_fog=exact\n");
        }
        // Particles only when Exact
        if !self.ps2_particles {
            s.push_str("particles=exact\n");
        }
        // Ambient Occlusion only when on
        if self.ambient_occlusion {
            s.push_str("ambient_occlusion=on\n");
        }
        // Sun Shadows only when on
        if self.sun_shadows {
            s.push_str("sun_shadows=on\n");
        }
        // Light Shadows only when on
        if self.light_shadows {
            s.push_str("light_shadows=on\n");
        }
        // Shadow Distance only when not the game's
        if self.shadow_distance != SHADOW_DISTANCE_GAME {
            s.push_str(&format!("shadow_distance={}\n", self.shadow_distance));
        }
        // Dynamic Lighting only when on
        if self.dynamic_lighting {
            s.push_str("dynamic_lighting=on\n");
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
        // the touch rows only when not the pad as it was
        for (v, was, key) in [
            (self.touch_stick, TOUCH_SIZE_WAS, "touch_stick"),
            (self.touch_buttons, TOUCH_SIZE_WAS, "touch_buttons"),
            (self.touch_opacity, TOUCH_OPACITY_WAS, "touch_opacity"),
            (self.touch_edge, 0, "touch_edge"),
        ] {
            if v != was {
                s.push_str(&format!("{key}={v}\n"));
            }
        }
        // Texture Packs and Dump Textures only when on
        if self.texture_packs {
            s.push_str("texture_packs=on\n");
        }
        if self.dump_textures {
            s.push_str("dump_textures=on\n");
        }
        s
    }

    /// The settings `to_text` wrote; a line not understood, or a value out
    /// of range, leaves that setting at its default. An older file's Touch
    /// Pad Size (`touch_size`, the whole pad's 50 to 100) is both touch sizes
    /// where the file gives neither of its own.
    pub fn from_text(s: &str) -> ClientOptions {
        let mut c = ClientOptions::default();
        let (mut old_size, mut stick, mut buttons) = (None, None, None);
        let ranged = |v: &str, lo: u8, hi: u8| v.parse::<u8>().ok().filter(|t| (lo..=hi).contains(t));
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
                ("raid_list", "on") => c.raid_list = true,
                ("raid_list", "off") => c.raid_list = false,
                ("pet_effects", "on") => c.pet_effects = true,
                ("pet_effects", "off") => c.pet_effects = false,
                ("buff_window", "on") => c.buff_window = true,
                ("buff_window", "off") => c.buff_window = false,
                ("head_marks", "on") => c.head_marks = true,
                ("head_marks", "off") => c.head_marks = false,
                ("power_bars", "on") => c.power_bars = true,
                ("power_bars", "off") => c.power_bars = false,
                ("raid_mercs", "shown") => c.raid_mercs = true,
                ("raid_mercs", "hidden") => c.raid_mercs = false,
                ("loading_tips", "on") => c.loading_tips = true,
                ("loading_tips", "off") => c.loading_tips = false,
                ("dps_meter", "on") => c.dps_meter = true,
                ("dps_meter", "off") => c.dps_meter = false,
                ("dps_meter_shows", "fight") => c.dps_view = crate::dpsmeter::View::Fight,
                ("dps_meter_shows", "overall") => c.dps_view = crate::dpsmeter::View::Overall,
                ("item_links", "on") => c.item_links = true,
                ("item_links", "off") => c.item_links = false,
                ("auto_cast", "on") => c.auto_cast = true,
                ("auto_cast", "off") => c.auto_cast = false,
                ("auto_target", "on") => c.auto_target = true,
                ("auto_target", "off") => c.auto_target = false,
                ("chat_always", "on") => c.chat_always = true,
                ("chat_always", "off") => c.chat_always = false,
                ("stacked_bars", "on") => c.stacked_bars = true,
                ("stacked_bars", "off") => c.stacked_bars = false,
                ("spell_bar_opacity", v) => {
                    if let Some(o) = v.parse::<u8>().ok().filter(|o| *o <= 100 && o % 10 == 0) {
                        c.spell_bar_opacity = o;
                    }
                }
                ("pets_pass", "on") => c.pets_pass = true,
                ("pets_pass", "off") => c.pets_pass = false,
                ("shared_shortcuts", "on") => c.shared_shortcuts = true,
                ("shared_shortcuts", "off") => c.shared_shortcuts = false,
                ("auto_save_options", "on") => c.auto_save = true,
                ("auto_save_options", "off") => c.auto_save = false,
                (k, v) if k.starts_with("chat_tab.") => {
                    let kind = crate::chat::Kind::ALL.iter().position(|x| Some(x.key()) == k.strip_prefix("chat_tab."));
                    if let (Some(kind), Some(t)) = (kind, v.parse::<u8>().ok().filter(|t| (1..=crate::chat::TABS as u8).contains(t))) {
                        c.chat_tabs[kind] = t;
                    }
                }
                (k, v) if k.starts_with("chat_tab_name.") => {
                    if let Some(n) = k.strip_prefix("chat_tab_name.").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=crate::chat::TABS).contains(n)) {
                        c.chat_tab_names[n - 1] = crate::chat::tab_name(v);
                    }
                }
                ("profanity_filter", v) => {
                    if let Some(p) = Profanity::from_key(v) {
                        c.profanity_filter = p;
                    }
                }
                ("map", "on") => c.map = true,
                ("map", "off") => c.map = false,
                ("minimap", "on") => c.minimap = true,
                ("minimap", "off") => c.minimap = false,
                ("minimap_scroll", "on") => c.minimap_scroll = true,
                ("minimap_scroll", "off") => c.minimap_scroll = false,
                ("map_pointer", v) => {
                    if let Some(p) = crate::map::Pointer::from_key(v) {
                        c.map_pointer = p;
                    }
                }
                ("map_markers", "on") => c.map_markers = true,
                ("map_markers", "off") => c.map_markers = false,
                ("map_towns", "on") => c.map_towns = true,
                ("map_towns", "off") => c.map_towns = false,
                ("quest_helper", "on") => c.quest_helper = true,
                ("quest_helper", "off") => c.quest_helper = false,
                ("quest_markers", "on") => c.quest_markers = true,
                ("quest_markers", "off") => c.quest_markers = false,
                ("quest_tracker", "on") => c.quest_tracker = true,
                ("quest_tracker", "off") => c.quest_tracker = false,
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
                ("ground_fog", "ps2") => c.ps2_fog = true,
                ("ground_fog", "exact") => c.ps2_fog = false,
                ("particles", "ps2") => c.ps2_particles = true,
                ("particles", "exact") => c.ps2_particles = false,
                ("ambient_occlusion", "on") => c.ambient_occlusion = true,
                ("ambient_occlusion", "off") => c.ambient_occlusion = false,
                ("sun_shadows", "on") => c.sun_shadows = true,
                ("sun_shadows", "off") => c.sun_shadows = false,
                ("light_shadows", "on") => c.light_shadows = true,
                ("light_shadows", "off") => c.light_shadows = false,
                ("shadow_distance", v) => {
                    if let Some(d) = v.parse::<u16>().ok().filter(|d| (SHADOW_DISTANCE_GAME..=SHADOW_DISTANCE_MAX).contains(d)) {
                        c.shadow_distance = d;
                    }
                }
                ("dynamic_lighting", "on") => c.dynamic_lighting = true,
                ("dynamic_lighting", "off") => c.dynamic_lighting = false,
                ("loading", "longer") => c.loading_longer = true,
                ("loading", "instant") => c.loading_longer = false,
                ("render_scale", v) => {
                    if let Some(k) = v.parse::<u8>().ok().filter(|k| RENDER_SCALES.contains(k)) {
                        c.render_scale = k;
                    }
                }
                ("texture_packs", "on") => c.texture_packs = true,
                ("texture_packs", "off") => c.texture_packs = false,
                ("dump_textures", "on") => c.dump_textures = true,
                ("dump_textures", "off") => c.dump_textures = false,
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
                ("touch_size", v) => old_size = ranged(v, TOUCH_SIZE_MIN, TOUCH_SIZE_WAS).or(old_size),
                ("touch_stick", v) => stick = ranged(v, TOUCH_SIZE_MIN, TOUCH_SIZE_MAX).or(stick),
                ("touch_buttons", v) => buttons = ranged(v, TOUCH_SIZE_MIN, TOUCH_SIZE_MAX).or(buttons),
                ("touch_opacity", v) => {
                    if let Some(t) = ranged(v, TOUCH_OPACITY_MIN, TOUCH_OPACITY_MAX) {
                        c.touch_opacity = t;
                    }
                }
                ("touch_edge", v) => {
                    if let Some(t) = ranged(v, 0, TOUCH_EDGE_MAX) {
                        c.touch_edge = t;
                    }
                }
                (k, v) if k.starts_with("number_color.") => {
                    let row = crate::numcolor::KEYS.iter().position(|&r| r == &k["number_color.".len()..]);
                    if let (Some(row), Some(choice)) = (row, crate::numcolor::from_key(v)) {
                        c.number_colors[row] = choice;
                    }
                }
                (k, v) if k.starts_with("controller.") => {
                    let window = k["controller.".len()..].parse::<usize>().ok().filter(|w| (1..=crate::multibox::PLACES).contains(w));
                    if let (Some(w), Some(f)) = (window, Focus::from_key(v)) {
                        c.controller[w - 1] = f;
                    }
                }
                (k, v) if k.starts_with("slow_unfocused.") => {
                    let window = k["slow_unfocused.".len()..].parse::<usize>().ok().filter(|w| (1..=crate::multibox::PLACES).contains(w));
                    if let (Some(w), "on" | "off") = (window, v) {
                        c.slow_unfocused[w - 1] = v == "on";
                    }
                }
                _ => {}
            }
        }
        c.touch_stick = stick.or(old_size).unwrap_or(c.touch_stick);
        c.touch_buttons = buttons.or(old_size).unwrap_or(c.touch_buttons);
        c
    }
}

impl ClientOptions {
    /// A later window's own file (`client-<n>.opt`): the Extended page's rows,
    /// the lines the windows share (Controller Focus, Capture Controller,
    /// Sound in Background, Update Notices) left to client.opt.
    fn rows_text(self) -> String {
        ClientOptions { controller: [Focus::Auto; crate::multibox::PLACES], slow_unfocused: [false; crate::multibox::PLACES], capture: false, background_sound: true, update_notices: true, ..self }.to_text()
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
    match account_read("client").or_else(|| store::read(&own_file("client", me)).filter(|_| me > 0)) {
        Some(b) => ClientOptions { controller: shared.controller, slow_unfocused: shared.slow_unfocused, capture: shared.capture, background_sound: shared.background_sound, update_notices: shared.update_notices, ..ClientOptions::from_text(&String::from_utf8_lossy(&b)) },
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
    shared.slow_unfocused[me] = mine.slow_unfocused[me];
    shared.capture = mine.capture;
    shared.background_sound = mine.background_sound;
    shared.update_notices = mine.update_notices;
    if me == 0 {
        store::write(CLIENT_FILE, ClientOptions { controller: shared.controller, slow_unfocused: shared.slow_unfocused, ..mine }.to_text().as_bytes())?;
    } else {
        store::write(&own_file("client", me), mine.rows_text().as_bytes())?;
        store::write(CLIENT_FILE, shared.to_text().as_bytes())?;
    }
    FILE_SHARED.with(|f| f.set(Some((shared.capture, shared.background_sound, shared.update_notices))));
    account_write("client", mine.rows_text().as_bytes())
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
    let me = cache_key();
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
    let me = cache_key();
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

/// The Raid List (`raidlist`).
pub fn raid_list() -> bool {
    client().raid_list
}

/// Pet Effects: the pet's effects under its bar (`hud`).
pub fn pet_effects() -> bool {
    client().pet_effects
}

/// Buff Window: the client's own effects window (`buffwnd`).
pub fn buff_window() -> bool {
    client().buff_window
}

/// Quest Markers over the heads: the "!" and "?" over quest NPCs' names
/// (`Play::plates`).
pub fn head_marks() -> bool {
    client().head_marks
}

/// Power Bars: others' power under their health in the group's bars, the
/// pet's bar and the raid list (`hud`, `raidlist`).
pub fn power_bars() -> bool {
    client().power_bars
}

/// Raid Mercenaries: the raid list draws the raid's mercs' rows (Shown) or
/// its players' alone (Hidden, the default).
pub fn raid_mercs() -> bool {
    client().raid_mercs
}

/// Loading Tips: a tip on every loading screen (`tips`).
pub fn loading_tips() -> bool {
    client().loading_tips
}

/// Number Colors now: each row's choice (`numcolor`).
pub fn number_colors() -> [crate::numcolor::Choice; crate::numcolor::ROWS] {
    client().number_colors
}

/// The DPS Meter (`dpsmeter`).
pub fn dps_meter() -> bool {
    client().dps_meter
}

/// Item Links (`itemlink`).
pub fn item_links() -> bool {
    client().item_links
}

/// Auto-Cast (`autocast`).
pub fn auto_cast() -> bool {
    client().auto_cast
}

/// Auto Target (`autocast`).
pub fn auto_target() -> bool {
    client().auto_target
}

/// Chat Always Shown (Extended > UI > Chat; ticket 1914).
pub fn chat_always() -> bool {
    client().chat_always
}

/// Auto Save Options (Extended > Other > General; ticket 2002).
pub fn auto_save() -> bool {
    client().auto_save
}

/// The account's quick-chat menus as `quickchat::shared_text` keeps them (Same
/// Shortcuts on Every Character, ticket 1987): None with no account known or
/// none kept.
pub fn account_shortcuts() -> Option<Vec<u8>> {
    account_read("shortcuts")
}

/// Keeps the account's quick-chat menus (nothing with no account known).
pub fn set_account_shortcuts(b: &[u8]) -> Result<(), String> {
    account_write("shortcuts", b)
}

/// Whether an account is known, whose files `account_shortcuts` reads.
pub fn account_known() -> bool {
    account_key().is_some()
}

/// Stacked Spell Bars (Extended > UI > HUD; ticket 1913).
pub fn stacked_bars() -> bool {
    client().stacked_bars
}

/// Profanity Filter (Options > Interface).
pub fn profanity_filter() -> Profanity {
    client().profanity_filter
}

/// Map Pointer: what the player's dart on the minimap and the map turns
/// with (`map::Pointer`).
pub fn map_pointer() -> crate::map::Pointer {
    client().map_pointer
}

/// What the DPS Meter shows (`dpsmeter::View`).
pub fn dps_view() -> crate::dpsmeter::View {
    client().dps_view
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

/// Chat tab `n`'s name (1 to 3; `/tabname`), kept at once as F9's Art is (a
/// typed command, no page's row that Save Changes keeps).
pub fn set_chat_tab_name(n: usize, name: &str) -> Result<(), String> {
    let mut c = client();
    if let Some(slot) = n.checked_sub(1).and_then(|i| c.chat_tab_names.get_mut(i)) {
        *slot = crate::chat::tab_name(name);
    }
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

/// Whether the map is on (Extended's Map): the map, the minimap, the
/// waypoints and the group's waypoint; off, none of them.
pub fn map() -> bool {
    client().map
}

/// Whether the minimap is up where it could be: the Map on, and its own
/// Minimap.
pub fn minimap() -> bool {
    let c = client();
    c.map && c.minimap
}

/// Whether the minimap is framed as the quick menu's scroll (Minimap Scroll
/// Frame), else the window's frame.
pub fn minimap_scroll() -> bool {
    client().minimap_scroll
}

/// Whether the map marks the XP camps and the coaches (Map Markers).
pub fn map_markers() -> bool {
    client().map_markers
}

/// Map Markers turned (the map's own key), and kept at once, as the waypoints
/// are.
pub fn set_map_markers(on: bool) -> Result<(), String> {
    let mut c = client();
    c.map_markers = on;
    set_client(c);
    save_client()
}

/// Whether the map names the towns.
pub fn map_towns() -> bool {
    client().map_towns
}

/// The map's town names turned (its own key), and kept at once.
pub fn set_map_towns(on: bool) -> Result<(), String> {
    let mut c = client();
    c.map_towns = on;
    set_client(c);
    save_client()
}

/// Whether the Quest Helper is on (Extended's): the quest marks, the tracker
/// and the guide; off, none of them, and no server asked for them.
pub fn quest_helper() -> bool {
    client().quest_helper
}

/// Whether the map and the minimap show quest marks: the Quest Helper on,
/// and its own Quest Markers (the map's J).
pub fn quest_markers() -> bool {
    let c = client();
    c.quest_helper && c.quest_markers
}

/// Quest Markers turned (the map's own key), and kept at once, as Map
/// Markers is.
pub fn set_quest_markers(on: bool) -> Result<(), String> {
    let mut c = client();
    c.quest_markers = on;
    set_client(c);
    save_client()
}

/// Whether the HUD shows the quest tracker where it could: the Quest Helper
/// on, and its own Quest Tracker.
pub fn quest_tracker() -> bool {
    let c = client();
    c.quest_helper && c.quest_tracker
}

/// Minimap turned (its key, N: `binds`' "minimap"), and kept at once, as
/// the map's own keys keep theirs.
pub fn set_minimap(on: bool) -> Result<(), String> {
    let mut c = client();
    c.minimap = on;
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

/// This window's Slow When Not Focused (petition 1634): its place's, off
/// unless chosen.
pub fn slow_unfocused() -> bool {
    client().slow_unfocused[place()]
}

/// Changes it (the Extended page); kept only by Save Changes, beside the
/// other windows' own.
pub fn set_slow_unfocused(on: bool) {
    let mut c = client();
    c.slow_unfocused[place()] = on;
    set_client(c);
}

/// The gap a covered or minimised window's frames keep with Slow When Not
/// Focused off: its Frame Rate's, else the display's refresh, so a window
/// with nothing to draw on never spins.
pub fn awake_gap() -> std::time::Duration {
    let g = frame_gap();
    std::time::Duration::from_secs_f64(if g > 0.0 { g } else { DISPLAY_GAP.with(|d| d.get()) } / 1000.0)
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
        /// each write's turn, for `modified`
        pub static CLOCK: std::cell::Cell<u128> = const { std::cell::Cell::new(0) };
        pub static STAMPS: RefCell<HashMap<String, u128>> = RefCell::new(HashMap::new());
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
        let t = CLOCK.with(|c| {
            c.set(c.get() + 1);
            c.get()
        });
        STAMPS.with(|s| s.borrow_mut().insert(name.to_string(), t));
        Ok(())
    }

    /// When `name` was last written (the writes' order).
    pub fn modified(name: &str) -> Option<u128> {
        STAMPS.with(|s| s.borrow().get(name).copied())
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

    /// When `name` was last written (ns since 1970).
    pub fn modified(name: &str) -> Option<u128> {
        let t = std::fs::metadata(dir()?.join(name)).ok()?.modified().ok()?;
        Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos())
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

    /// localStorage keeps no times: `migrate` takes this window's set.
    pub fn modified(_name: &str) -> Option<u128> {
        None
    }
}

#[cfg(test)]
pub mod testing {
    /// Empties this thread's store and settings, a store that refuses or not.
    pub fn reset(refuse: bool) {
        super::store::FILES.with(|f| f.borrow_mut().clear());
        super::store::REFUSE.with(|r| r.set(refuse));
        super::CLIENT.with(|c| c.set(None));
        super::account_state::ACCOUNT.with(|a| *a.borrow_mut() = None);
        super::account_state::GENERATION.with(|g| g.set(0));
        super::account_state::NOTICED.with(|n| n.set(false));
        super::store::CLOCK.with(|c| c.set(0));
        super::store::STAMPS.with(|f| f.borrow_mut().clear());
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
        let t = super::store::CLOCK.with(|c| {
            c.set(c.get() + 1);
            c.get()
        });
        super::store::STAMPS.with(|s| s.borrow_mut().insert(name.to_string(), t));
    }

    /// Drops `name` from the store, so a test sees whether it is written again.
    pub fn forget(name: &str) {
        super::store::FILES.with(|f| f.borrow_mut().remove(name));
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

    /// Texture Packs and Dump Textures, off by default (every texture the
    /// game's own, as PCSX2's Load Textures and Dump Textures start), kept
    /// only when on; anything but on or off leaves each off.
    #[test]
    fn texture_packs_and_dumps_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.texture_packs && !d.dump_textures, "off, the game's own textures");
        assert_eq!(d.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\n", "nothing kept while off");
        let both = ClientOptions { texture_packs: true, dump_textures: true, ..Default::default() };
        assert_eq!(both.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\ntexture_packs=on\ndump_textures=on\n");
        assert_eq!(ClientOptions::from_text(&both.to_text()), both);
        assert!(!ClientOptions::from_text("texture_packs=off\ndump_textures=off").texture_packs);
        for bad in ["texture_packs=1", "texture_packs=yes", "dump_textures=", "dump_textures=true"] {
            let c = ClientOptions::from_text(bad);
            assert!(!c.texture_packs && !c.dump_textures, "{bad}: off, the default");
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

    /// Ambient Occlusion, off by default (the picture as the game draws
    /// it), kept only when on; anything but on or off leaves it off. The
    /// control: Dynamic Lighting on is not it.
    #[test]
    fn ambient_occlusion_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.ambient_occlusion, "off, the game's picture");
        assert!(!d.to_text().contains("ambient_occlusion"), "the default not written");
        let on = ClientOptions { ambient_occlusion: true, ..Default::default() };
        assert_eq!(on.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nambient_occlusion=on\n");
        assert_eq!(ClientOptions::from_text(&on.to_text()), on);
        assert!(ClientOptions::from_text("ambient_occlusion=off") == d);
        for bad in ["ambient_occlusion=1", "ambient_occlusion=", "ambient_occlusion=On", "ao=on", "dynamic_lighting=on"] {
            assert!(!ClientOptions::from_text(bad).ambient_occlusion, "{bad}: off, the default");
        }
    }

    /// Sun Shadows, off by default, kept only when on, and apart from
    /// Dynamic Lighting (either may be on without the other); anything but
    /// on or off leaves them off.
    #[test]
    fn sun_shadows_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.sun_shadows, "off by default");
        assert!(!d.to_text().contains("sun_shadows"), "the default not written");
        let on = ClientOptions { sun_shadows: true, ..Default::default() };
        assert_eq!(on.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nsun_shadows=on\n");
        assert_eq!(ClientOptions::from_text(&on.to_text()), on);
        let both = ClientOptions { sun_shadows: true, dynamic_lighting: true, ..Default::default() };
        assert_eq!(ClientOptions::from_text(&both.to_text()), both);
        assert!(!ClientOptions::from_text("sun_shadows=on").dynamic_lighting, "Dynamic Lighting not turned on with them");
        assert!(ClientOptions::from_text("sun_shadows=off") == d);
        for bad in ["sun_shadows=1", "sun_shadows=", "sun_shadows=On", "shadows=on"] {
            assert!(!ClientOptions::from_text(bad).sun_shadows, "{bad}: off, the default");
        }
    }

    /// Light Shadows, off by default, kept only when on, and apart from
    /// Dynamic Lighting and Sun Shadows (each may be on without the others);
    /// anything but on or off leaves them off.
    #[test]
    fn light_shadows_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.light_shadows, "off by default");
        assert!(!d.to_text().contains("light_shadows"), "the default not written");
        let on = ClientOptions { light_shadows: true, ..Default::default() };
        assert_eq!(on.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nlight_shadows=on\n");
        assert_eq!(ClientOptions::from_text(&on.to_text()), on);
        let all = ClientOptions { light_shadows: true, sun_shadows: true, dynamic_lighting: true, ..Default::default() };
        assert_eq!(ClientOptions::from_text(&all.to_text()), all);
        let them = ClientOptions::from_text("light_shadows=on");
        assert!(!them.dynamic_lighting && !them.sun_shadows, "Dynamic Lighting and Sun Shadows not turned on with them");
        assert!(ClientOptions::from_text("light_shadows=off") == d);
        for bad in ["light_shadows=1", "light_shadows=", "light_shadows=On", "shadows=on", "sun_shadows=on"] {
            assert!(!ClientOptions::from_text(bad).light_shadows, "{bad}: off, the default");
        }
    }

    /// Shadow Distance, the game's 250 by default and not written then, kept
    /// between it and 1000; anything else leaves the game's.
    #[test]
    fn shadow_distance_kept_when_raised() {
        let d = ClientOptions::default();
        assert_eq!(d.shadow_distance, 250, "the game's");
        assert!(!d.to_text().contains("shadow_distance"), "the default not written");
        let far = ClientOptions { shadow_distance: 750, ..Default::default() };
        assert_eq!(far.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nshadow_distance=750\n");
        assert_eq!(ClientOptions::from_text(&far.to_text()), far);
        for bad in ["shadow_distance=1001", "shadow_distance=249", "shadow_distance=", "shadow_distance=far", "shadow_distance=-500"] {
            assert_eq!(ClientOptions::from_text(bad).shadow_distance, 250, "{bad}: the game's");
        }
        assert_eq!(ClientOptions::from_text("shadow_distance=1000").shadow_distance, 1000);
    }

    /// Dynamic Lighting, off by default (the PS2's light), kept only when
    /// on; anything but on or off leaves it off.
    #[test]
    fn dynamic_lighting_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.dynamic_lighting, "off, the PS2's");
        assert!(!d.to_text().contains("dynamic_lighting"), "the default not written");
        let on = ClientOptions { dynamic_lighting: true, ..Default::default() };
        assert_eq!(on.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\ndynamic_lighting=on\n");
        assert_eq!(ClientOptions::from_text(&on.to_text()), on);
        assert!(ClientOptions::from_text("dynamic_lighting=off") == d);
        for bad in ["dynamic_lighting=1", "dynamic_lighting=", "dynamic_lighting=On", "lighting=on"] {
            assert!(!ClientOptions::from_text(bad).dynamic_lighting, "{bad}: off, the default");
        }
    }

    /// Particles, PS2 by default (the game's look), kept only when Exact;
    /// anything but ps2 or exact leaves it PS2. The control: Ground Fog's
    /// Exact is not the particles'.
    #[test]
    fn particles_kept_only_when_exact() {
        let d = ClientOptions::default();
        assert!(d.ps2_particles, "PS2, the game's");
        assert_eq!(ClientOptions { ps2_particles: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nparticles=exact\n");
        assert!(!d.to_text().contains("particles"), "the default not written");
        let c = ClientOptions::from_text("particles=exact");
        assert!(!c.ps2_particles && c.ps2_fog && c.environment, "Particles alone");
        assert_eq!(ClientOptions::from_text(&c.to_text()), c);
        assert!(ClientOptions::from_text("particles=ps2") == d);
        for bad in ["particles=off", "particles=", "particle=exact", "particles=PS2"] {
            assert!(ClientOptions::from_text(bad).ps2_particles, "{bad}: PS2, the default");
        }
        assert!(ClientOptions::from_text("ground_fog=exact").ps2_particles, "Ground Fog's Exact is not the particles'");
    }

    /// Ground Fog, PS2 by default (the game's look), kept only when Exact;
    /// anything but ps2 or exact leaves it PS2. The control: Environment off
    /// is not Ground Fog's Exact.
    #[test]
    fn ground_fog_kept_only_when_exact() {
        let d = ClientOptions::default();
        assert!(d.ps2_fog, "PS2, the game's");
        assert_eq!(ClientOptions { ps2_fog: false, ..Default::default() }.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\nground_fog=exact\n");
        assert!(!d.to_text().contains("ground_fog"), "the default not written");
        let c = ClientOptions::from_text("ground_fog=exact");
        assert!(!c.ps2_fog && c.environment && c.flora, "Ground Fog alone");
        assert!(ClientOptions::from_text("ground_fog=ps2") == d);
        for bad in ["ground_fog=off", "ground_fog=", "groundfog=exact", "ground_fog=PS2"] {
            assert!(ClientOptions::from_text(bad).ps2_fog, "{bad}: PS2, the default");
        }
        assert!(ClientOptions::from_text("environment=off").ps2_fog);
    }

    /// The touch rows: the pad as it was unless chosen (both sizes 100, its
    /// own opacity, no edge), each written only when not, read back in its
    /// range; anything else leaves that row as it was. An older file's Touch
    /// Pad Size is both sizes, unless the file has a size of its own (in
    /// either order); a save then writes the two, not it. The controls: out
    /// of its 50 to 100 the old line is nothing, and the default pad's
    /// options are `touch::Opts`' default.
    #[test]
    fn touch_rows_kept_only_when_not_the_pad_as_it_was() {
        let d = ClientOptions::default();
        assert_eq!((d.touch_stick, d.touch_buttons, d.touch_opacity, d.touch_edge), (100, 100, 100, 0));
        assert_eq!(d.touch_opts(), crate::touch::Opts::default(), "the pad as it was");
        assert_eq!((TOUCH_SIZE_MIN, TOUCH_SIZE_MAX, TOUCH_OPACITY_MIN, TOUCH_OPACITY_MAX, TOUCH_EDGE_MAX), (50, 200, 10, 200, 10));
        assert!(!d.to_text().contains("touch"), "the defaults not written");
        let c = ClientOptions { touch_stick: 150, touch_buttons: 55, touch_opacity: 40, touch_edge: 6, ..Default::default() };
        assert_eq!(c.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\ntouch_stick=150\ntouch_buttons=55\ntouch_opacity=40\ntouch_edge=6\n");
        assert!(ClientOptions::from_text(&c.to_text()) == c);
        assert_eq!(c.touch_opts(), crate::touch::Opts { stick: 1.5, buttons: 0.55, opacity: 0.4, edge: 0.06 });
        for (line, want) in [("touch_stick=200", (200, 100, 100, 0)), ("touch_buttons=50", (100, 50, 100, 0)), ("touch_opacity=10", (100, 100, 10, 0)), ("touch_edge=10", (100, 100, 100, 10))] {
            let c = ClientOptions::from_text(line);
            assert_eq!((c.touch_stick, c.touch_buttons, c.touch_opacity, c.touch_edge), want, "{line}");
        }
        for bad in ["touch_stick=49", "touch_stick=201", "touch_buttons=", "touch_buttons=x", "touch_opacity=9", "touch_opacity=201", "touch_edge=11", "touch_edge=-1", "touchstick=150"] {
            let c = ClientOptions::from_text(bad);
            assert_eq!((c.touch_stick, c.touch_buttons, c.touch_opacity, c.touch_edge), (100, 100, 100, 0), "{bad}: as it was");
        }
        // an older client.opt's Touch Pad Size: both sizes
        let old = ClientOptions::from_text("lod=on\ntouch_size=70\n");
        assert_eq!((old.touch_stick, old.touch_buttons), (70, 70));
        assert_eq!(old.to_text(), "lod=on\nbrightness=5\ncamera_distance=11\ntouch_stick=70\ntouch_buttons=70\n", "saved as the two");
        assert_eq!(old.touch_opts(), crate::touch::Opts::sized(0.7), "drawn as Touch Pad Size 70 was");
        assert_eq!(ClientOptions::from_text("touch_size=50").touch_buttons, 50);
        for (text, want) in [("touch_size=70\ntouch_stick=120", (120, 70)), ("touch_stick=120\ntouch_size=70", (120, 70)), ("touch_buttons=80\ntouch_size=60", (60, 80))] {
            let c = ClientOptions::from_text(text);
            assert_eq!((c.touch_stick, c.touch_buttons), want, "{text:?}: a size of its own wins");
        }
        for bad in ["touch_size=49", "touch_size=101", "touch_size=150", "touch_size=", "touch_size=x"] {
            let c = ClientOptions::from_text(bad);
            assert_eq!((c.touch_stick, c.touch_buttons), (100, 100), "{bad}: the control, as it was");
        }
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

    /// The options kept by a build with the Stats Box, others' mana bars
    /// and Target of Target (all gone) still load: their lines are left out
    /// and written back no more; the map's and the quests' beside them (gone
    /// on 2026-10-27, back on 2026-11-09) are read again. The control: the
    /// lines beside them are read.
    #[test]
    fn options_with_the_lines_of_rows_gone_still_load() {
        let old = "target_of_target=on\nmana_bars=off\nminimap=off\nstats_box=on\nquest_markers=off\nquest_tracker=off\nmap_markers=off\nmap_towns=off\nflora=off\nloading=instant\n";
        let c = ClientOptions::from_text(old);
        let want = ClientOptions { minimap: false, quest_markers: false, quest_tracker: false, map_markers: false, map_towns: false, flora: false, loading_longer: false, ..Default::default() };
        assert_eq!(c, want, "the lines beside them read");
        let text = c.to_text();
        for gone in ["target_of_target", "mana_bars", "stats_box"] {
            assert!(!text.contains(gone), "{gone}: {text}");
        }
        for kept in ["minimap=off\n", "quest_markers=off\n", "quest_tracker=off\n", "map_markers=off\n", "map_towns=off\n", "flora=off\n", "loading=instant\n"] {
            assert!(text.contains(kept), "{kept}: {text}");
        }
        assert!(c.map && c.quest_helper, "the Map and the Quest Helper on, their defaults");
    }

    /// The Map, the Quest Helper and the parts under them: on by default,
    /// each kept only when off and read back off; anything but on or off
    /// leaves it on. A part off leaves its master on and the other parts
    /// (the controls: one line moves one setting); a master off turns off
    /// what reads through it (`minimap`, `quest_markers`, `quest_tracker`)
    /// while the parts' own settings stay as they were.
    #[test]
    fn the_map_and_the_quest_helper_kept_only_when_off() {
        let d = ClientOptions::default();
        assert!(d.map && d.minimap && d.minimap_scroll && d.map_markers && d.map_towns && d.quest_helper && d.quest_markers && d.quest_tracker, "on by default");
        for key in ["map", "minimap", "minimap_scroll", "map_markers", "map_towns", "quest_helper", "quest_markers", "quest_tracker"] {
            let off = ClientOptions::from_text(&format!("{key}=off"));
            assert_ne!(off, d, "{key}=off read");
            assert_eq!(off.to_text(), format!("lod=on\nbrightness=5\ncamera_distance=11\n{key}=off\n"), "{key}: only when off");
            assert_eq!(ClientOptions::from_text(&off.to_text()), off, "{key}: read back off");
            assert_eq!(ClientOptions::from_text(&format!("{key}=on")), d, "{key}=on");
            for bad in ["0", "no", ""] {
                assert_eq!(ClientOptions::from_text(&format!("{key}={bad}")), d, "{key}={bad}: on, the default");
            }
        }
        testing::reset(false);
        assert!(minimap() && quest_markers() && quest_tracker() && map() && quest_helper());
        set_client(ClientOptions { map: false, quest_helper: false, ..Default::default() });
        assert!(!minimap() && !quest_markers() && !quest_tracker() && !map() && !quest_helper(), "the masters off: their parts off");
        assert!(client().minimap && client().quest_markers && client().quest_tracker, "the parts' own settings kept");
        set_client(ClientOptions { minimap: false, ..Default::default() });
        assert!(!minimap() && map() && quest_markers(), "the control: a part off alone");
        testing::reset(false);
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
    fn buff_window_and_loading_tips_are_kept_only_when_off() {
        let d = ClientOptions::default();
        assert!(d.buff_window && d.loading_tips, "on by default");
        assert!(!d.to_text().contains("buff_window") && !d.to_text().contains("loading_tips"));
        let off = ClientOptions { buff_window: false, loading_tips: false, ..Default::default() }.to_text();
        assert!(off.contains("buff_window=off\n") && off.contains("loading_tips=off\n"));
        let back = ClientOptions::from_text(&off);
        assert!(!back.buff_window && !back.loading_tips);
        assert!(ClientOptions::from_text("buff_window=maybe").buff_window, "the default");
    }

    #[test]
    fn raid_mercs_are_kept_only_when_shown() {
        assert!(!ClientOptions::default().raid_mercs, "Hidden by default");
        assert!(!ClientOptions::default().to_text().contains("raid_mercs"), "the default writes nothing");
        let t = ClientOptions { raid_mercs: true, ..Default::default() }.to_text();
        assert!(t.contains("raid_mercs=shown\n"), "{t}");
        assert!(ClientOptions::from_text(&t).raid_mercs, "read back");
        assert!(!ClientOptions::from_text("raid_mercs=hidden").raid_mercs);
        assert!(!ClientOptions::from_text("raid_mercs=maybe").raid_mercs, "anything else: the default");
    }

    #[test]
    fn power_bars_are_kept_only_when_off() {
        assert!(ClientOptions::default().power_bars, "on by default");
        assert!(!ClientOptions::default().to_text().contains("power_bars"));
        assert!(ClientOptions { power_bars: false, ..Default::default() }.to_text().contains("power_bars=off\n"));
        assert!(!ClientOptions::from_text("power_bars=off").power_bars);
        assert!(ClientOptions::from_text("power_bars=on").power_bars);
        assert!(ClientOptions::from_text("power_bars=maybe").power_bars, "the default");
        assert!(ClientOptions::from_text("mana_bars=off").power_bars, "the control: the old Mana Bars' key is not this");
    }

    #[test]
    fn head_marks_are_kept_only_when_off() {
        assert!(ClientOptions::default().head_marks, "on by default");
        assert!(!ClientOptions::default().to_text().contains("head_marks"));
        assert!(ClientOptions { head_marks: false, ..Default::default() }.to_text().contains("head_marks=off\n"));
        let back = ClientOptions::from_text("head_marks=off\nquest_markers=on");
        assert!(!back.head_marks && back.quest_markers, "the map's Quest Markers its own");
        assert!(ClientOptions::from_text("head_marks=on").head_marks);
        assert!(ClientOptions::from_text("head_marks=maybe").head_marks, "the default");
        assert!(ClientOptions::from_text("quest_markers=off").head_marks, "the control: the map's row is not this");
    }

    #[test]
    fn pet_effects_are_kept_only_when_off() {
        assert!(ClientOptions::default().pet_effects, "on by default");
        assert!(!ClientOptions::default().to_text().contains("pet_effects"));
        assert!(ClientOptions { pet_effects: false, ..Default::default() }.to_text().contains("pet_effects=off\n"));
        assert!(!ClientOptions::from_text("pet_effects=off").pet_effects);
        assert!(ClientOptions::from_text("pet_effects=on").pet_effects);
        assert!(ClientOptions::from_text("pet_effects=maybe").pet_effects, "the default");
    }

    /// Profanity Filter: on by default (the game's at every login) and
    /// written only when off.
    #[test]
    fn the_profanity_filter_is_kept_only_when_off() {
        assert_eq!(ClientOptions::default().profanity_filter, Profanity::High, "High by default");
        assert!(!ClientOptions::default().to_text().contains("profanity"));
        assert!(ClientOptions { profanity_filter: Profanity::Off, ..Default::default() }.to_text().contains("profanity_filter=off\n"));
        assert!(ClientOptions { profanity_filter: Profanity::Medium, ..Default::default() }.to_text().contains("profanity_filter=medium\n"));
        assert_eq!(ClientOptions::from_text("profanity_filter=off").profanity_filter, Profanity::Off);
        assert_eq!(ClientOptions::from_text("profanity_filter=medium").profanity_filter, Profanity::Medium);
        assert_eq!(ClientOptions::from_text("profanity_filter=high").profanity_filter, Profanity::High);
        assert_eq!(ClientOptions::from_text("profanity_filter=on").profanity_filter, Profanity::High, "the old On");
        assert_eq!(ClientOptions::from_text("profanity_filter=maybe").profanity_filter, Profanity::High, "the default");
        // the row's steps, round at both ends
        assert_eq!(Profanity::Off.step(1), Profanity::Medium);
        assert_eq!(Profanity::High.step(1), Profanity::Off);
        assert_eq!(Profanity::Off.step(-1), Profanity::High);
    }

    /// Auto Save Options (ticket 2002): off by default and written only when
    /// on.
    #[test]
    fn auto_save_options_is_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.auto_save, "off by default");
        assert!(!d.to_text().contains("auto_save"));
        let t = ClientOptions { auto_save: true, ..Default::default() }.to_text();
        assert!(t.contains("auto_save_options=on\n"), "{t}");
        assert!(ClientOptions::from_text(&t).auto_save, "round trip");
        assert!(!ClientOptions::from_text("auto_save_options=off").auto_save);
        assert!(!ClientOptions::from_text("auto_save_options=maybe").auto_save, "the default");
    }

    /// The rows of tickets 1951, 1953, 1977 and 1987: kept only when changed,
    /// read back as kept, a value out of range left at its default.
    #[test]
    fn the_tickets_rows_of_2026_11_13_are_kept_only_when_changed() {
        let d = ClientOptions::default();
        assert!(d.pets_pass && !d.shared_shortcuts && d.spell_bar_opacity == 100 && d.chat_tabs == [0; crate::chat::KINDS], "the defaults");
        let t = d.to_text();
        for k in ["pets_pass", "shared_shortcuts", "spell_bar_opacity", "chat_tab"] {
            assert!(!t.contains(k), "{k} not kept at its default: {t}");
        }
        let mut c = ClientOptions { pets_pass: false, shared_shortcuts: true, spell_bar_opacity: 40, ..d };
        c.chat_tabs[crate::chat::Kind::Loot.index()] = 3;
        c.chat_tabs[crate::chat::Kind::Tells.index()] = 1;
        c.chat_tab_names[2] = crate::chat::tab_name("Loot");
        let t = c.to_text();
        assert!(t.contains("chat_tab.loot=3\n") && t.contains("chat_tab_name.3=Loot\n") && t.contains("spell_bar_opacity=40\n"), "{t}");
        assert_eq!(ClientOptions::from_text(&t), c, "round trip");
        // out of range: left at the default
        let bad = ClientOptions::from_text("spell_bar_opacity=45\nchat_tab.loot=4\nchat_tab.nonsense=1\nchat_tab_name.4=x");
        assert_eq!(bad, d);
    }

    /// Item Links: on by default and written only when off.
    #[test]
    fn chat_always_and_stacked_bars_are_kept_only_when_on() {
        let d = ClientOptions::default();
        assert!(!d.chat_always && !d.stacked_bars, "off by default");
        assert!(!d.to_text().contains("chat_always") && !d.to_text().contains("stacked_bars"));
        let t = ClientOptions { chat_always: true, stacked_bars: true, ..Default::default() }.to_text();
        assert!(t.contains("chat_always=on\n") && t.contains("stacked_bars=on\n"), "{t}");
        let back = ClientOptions::from_text(&t);
        assert!(back.chat_always && back.stacked_bars, "round trip");
        assert!(!ClientOptions::from_text("chat_always=off").chat_always);
        assert!(!ClientOptions::from_text("stacked_bars=maybe").stacked_bars, "the default");
    }

    #[test]
    fn item_links_are_kept_only_when_off() {
        assert!(ClientOptions::default().item_links, "on by default");
        assert!(!ClientOptions::default().to_text().contains("item_links"));
        assert!(ClientOptions { item_links: false, ..Default::default() }.to_text().contains("item_links=off\n"));
        assert!(!ClientOptions::from_text("item_links=off").item_links);
        assert!(ClientOptions::from_text("item_links=on").item_links);
        assert!(ClientOptions::from_text("item_links=maybe").item_links, "the default");
    }

    /// Number Colors: every row Default by default and written not at all; a
    /// chosen row is one line by its row's and its colour's keys, and reads
    /// back to itself, the others Default; a row or colour not understood is
    /// Default (the control: an unknown key changes nothing).
    #[test]
    fn number_colors_are_kept_only_when_chosen() {
        let d = ClientOptions::default();
        assert_eq!(d.number_colors, [0, 0, 0, 0, 0, 0, 8, 6], "Default by default, the merc's Light Blue and Purple");
        assert!(!d.to_text().contains("number_color"), "nothing written as they came");
        let mut c = ClientOptions::default();
        c.number_colors[1] = crate::numcolor::from_key("red").unwrap();
        c.number_colors[5] = crate::numcolor::from_key("light_blue").unwrap();
        let text = c.to_text();
        assert!(text.contains("number_color.procs=red\n") && text.contains("number_color.pet_heals=light_blue\n"), "{text}");
        assert_eq!(text.matches("number_color.").count(), 2, "{text}");
        assert_eq!(ClientOptions::from_text(&text).number_colors, c.number_colors, "read back");
        assert_eq!(ClientOptions::from_text(&text), c, "and nothing else changed");
        let odd = ClientOptions::from_text("number_color.procs=mauve\nnumber_color.elbows=red\nnumber_color.heals=default");
        assert_eq!(odd.number_colors, crate::numcolor::DEFAULTS, "not understood: as they came");
        assert_eq!(ClientOptions::from_text("number_color.pet_damage=orange").number_colors, [0, 0, 0, 3, 0, 0, 8, 6]);
        // a merc row chosen Default is kept as `default`, another colour as its key
        let mut m = ClientOptions::default();
        m.number_colors[6] = 0;
        m.number_colors[7] = crate::numcolor::from_key("green").unwrap();
        let text = m.to_text();
        assert!(text.contains("number_color.merc_damage=default\n") && text.contains("number_color.merc_heals=green\n"), "{text}");
        assert_eq!(ClientOptions::from_text(&text).number_colors, m.number_colors, "read back");
    }

    /// Map Pointer: Facing by default and then written nothing; Camera
    /// kept and read back; anything else the default.
    #[test]
    fn the_map_pointer_is_kept_only_when_camera() {
        use crate::map::Pointer;
        let d = ClientOptions::default();
        assert_eq!(d.map_pointer, Pointer::Facing, "Facing by default");
        assert!(!d.to_text().contains("map_pointer"), "the default writes nothing");
        let c = ClientOptions { map_pointer: Pointer::Camera, ..Default::default() };
        let t = c.to_text();
        assert!(t.contains("map_pointer=camera\n"), "{t}");
        assert_eq!(ClientOptions::from_text(&t), c, "round trip");
        assert_eq!(ClientOptions::from_text("map_pointer=facing"), d);
        assert_eq!(ClientOptions::from_text("map_pointer=sideways"), d, "anything else: the default");
    }

    #[test]
    fn the_dps_meter_is_kept_only_when_on_and_overall() {
        use crate::dpsmeter::View;
        let d = ClientOptions::default();
        assert!(!d.dps_meter && d.dps_view == View::Fight, "off by default, on the fight");
        assert!(!d.to_text().contains("dps_meter"), "the defaults write nothing");
        let c = ClientOptions { dps_meter: true, dps_view: View::Overall, ..Default::default() };
        let t = c.to_text();
        assert!(t.contains("dps_meter=on\n") && t.contains("dps_meter_shows=overall\n"), "{t}");
        let back = ClientOptions::from_text(&t);
        assert!(back.dps_meter && back.dps_view == View::Overall, "round trip");
        assert!(!ClientOptions::from_text("dps_meter=off").dps_meter);
        assert_eq!(ClientOptions::from_text("dps_meter_shows=fight").dps_view, View::Fight);
        let odd = ClientOptions::from_text("dps_meter=maybe\ndps_meter_shows=sideways");
        assert!(!odd.dps_meter && odd.dps_view == View::Fight, "anything else: the defaults");
    }

    #[test]
    fn the_raid_list_is_kept_only_when_off() {
        assert!(ClientOptions::default().raid_list, "on by default");
        assert!(!ClientOptions::default().to_text().contains("raid_list"));
        assert!(ClientOptions { raid_list: false, ..Default::default() }.to_text().contains("raid_list=off\n"));
        assert!(!ClientOptions::from_text("raid_list=off").raid_list);
        assert!(ClientOptions::from_text("raid_list=on").raid_list);
        assert!(ClientOptions::from_text("raid_list=maybe").raid_list, "the default");
    }

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
    /// Slow When Not Focused (petition 1634) is each window's, as Controller
    /// Focus is: off by default and no line, `slow_unfocused.<window>=on` in
    /// client.opt beside the other windows' own (one saved meanwhile kept),
    /// never in a window's own file or the account's (the control: a
    /// window's rows are). A line that is not on or off reads as off.
    #[test]
    fn each_window_keeps_its_own_slow_when_not_focused() {
        testing::reset(false);
        assert!(!slow_unfocused(), "off by default");
        assert!(!ClientOptions::default().to_text().contains("slow_unfocused"), "no line while off");
        set_slow_unfocused(true);
        store::write(CLIENT_FILE, b"lod=on\nbrightness=5\ncamera_distance=11\nslow_unfocused.3=on\n").unwrap();
        assert!(save(&GameOptions::default()));
        let kept = String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap();
        assert_eq!(kept, "lod=on\nbrightness=5\ncamera_distance=11\nslow_unfocused.1=on\nslow_unfocused.3=on\n");
        let both = ClientOptions::from_text(&kept);
        assert_eq!((both.slow_unfocused[0], both.slow_unfocused[1], both.slow_unfocused[2]), (true, false, true));
        assert!(!both.rows_text().contains("slow_unfocused"), "a window's own file and the account's: never");
        assert!(both.rows_text().contains("brightness=5"), "the control: its rows are");
        assert!(!ClientOptions::from_text("slow_unfocused.1=yes\nslow_unfocused.0=on\nslow_unfocused.99=on").slow_unfocused.iter().any(|&on| on), "odd lines: off");
        set_slow_unfocused(false);
        assert!(save(&GameOptions::default()));
        let kept = String::from_utf8(testing::kept(CLIENT_FILE).unwrap()).unwrap();
        assert_eq!(kept, "lod=on\nbrightness=5\ncamera_distance=11\nslow_unfocused.3=on\n", "this window's line gone, the other's kept");
        testing::reset(false);
    }

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

    /// Network Setup's server: eqoaclassic.com until one is kept, kept
    /// trimmed, a blank one is none (not the default again), and a store that
    /// refuses keeps nothing.
    #[test]
    fn the_server_is_kept_trimmed_and_blank_is_none() {
        testing::reset(false);
        assert_eq!(server().as_deref(), Some(DEFAULT_SERVER), "none kept yet: the default");
        set_server("  eqoa.example.com \n").unwrap();
        assert_eq!(server().as_deref(), Some("eqoa.example.com"));
        set_server("").unwrap();
        assert_eq!(server(), None, "blank is none");
        testing::reset(true);
        assert!(set_server("192.0.2.1").is_err(), "the control: a refusing store");
        assert_eq!(server().as_deref(), Some(DEFAULT_SERVER), "nothing kept: the default");
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

    /// Petition 1621: a login makes the options the account's. Its first
    /// login takes the window's set kept most recently (here window 2's,
    /// written after window 1's), never the saved login; a save keeps them
    /// for the account, so another window (a character of the same account)
    /// plays with them, and another account keeps its own. What the windows
    /// share stays client.opt's. The control: before any login, window 1's.
    #[test]
    fn options_are_the_accounts() {
        use crate::multibox::testing::set_place;
        testing::reset(false);
        let mut one = GameOptions { battle: 1, ..changed() };
        let mut card = one.to_card();
        card[0x10..0x13].copy_from_slice(b"dab");
        testing::put(GAME_FILE, &card);
        testing::put(CLIENT_FILE, b"brightness=3\ncapture_controller=on\n");
        let two = GameOptions { battle: 2, ..changed() };
        testing::put("game-2.opt", &two.to_card());
        testing::put("client-2.opt", b"brightness=7\n");
        set_place(Some(0));
        assert_eq!((game_options().battle, client().brightness), (1, 3), "the control: no login, window 1's");
        assert!(!account_notice_due(), "no account, no notice");
        set_account("eqoaclassic.com", "Dab");
        assert_eq!(game_options().battle, 2, "window 2's, the newest set");
        assert_eq!(client().brightness, 7);
        assert!(client().capture, "what the windows share: client.opt's");
        let acct = account_file("eqoaclassic.com+dab", "game");
        assert!(testing::kept(&acct).unwrap()[0x10..0x91].iter().all(|&b| b == 0), "never the login");
        assert_eq!(saved_login().0, "dab", "the window's saved login stays");
        // a change saved: the account's
        one.battle = 5;
        let mut c = client();
        c.brightness = 9;
        set_client(c);
        assert!(save(&one));
        // another window, the same account (a second character)
        set_place(Some(3));
        set_account_now(None);
        set_account("eqoaclassic.com", "DAB");
        assert_eq!((game_options().battle, client().brightness), (5, 9), "the account's, in any window");
        // another account: its own, from the newest set (the save just made)
        set_account("eqoaclassic.com", "other");
        assert_eq!(client().brightness, 9);
        let mut c = client();
        c.brightness = 1;
        set_client(c);
        assert!(save(&GameOptions { battle: 6, ..one }));
        set_account("eqoaclassic.com", "dab");
        assert_eq!((game_options().battle, client().brightness), (5, 9), "the first account's untouched");
        // a new run of window 4: the title reads the account it last logged into
        set_account_now(None);
        bump_generation();
        assert_eq!((game_options().battle, client().brightness), (5, 9));
        testing::reset(false);
    }

    /// The notice: due once an account is known, until a Yes; the Yes is kept.
    #[test]
    fn the_account_notice_until_seen() {
        testing::reset(false);
        crate::multibox::testing::set_place(Some(0));
        assert!(!account_notice_due());
        set_account("s", "u");
        assert!(account_notice_due());
        account_notice_seen();
        assert!(!account_notice_due());
        account_state::NOTICED.with(|n| n.set(false));
        assert!(!account_notice_due(), "kept for later runs");
        testing::reset(false);
    }

    /// An account's name is a safe store name, case folded.
    #[test]
    fn account_names() {
        assert_eq!(account_name("EQOAclassic.com:9735", " Dab "), "eqoaclassic.com_9735+dab");
        assert_eq!(account_name("a/b", "..\\x"), "a_b+.._x");
    }
}
