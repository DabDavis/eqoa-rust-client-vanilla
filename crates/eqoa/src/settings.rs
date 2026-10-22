//! What the client keeps between runs. The game keeps `VIGameOptions` in
//! `game.opt` on the memory card (SUPPORT's `vigameoptions.cpp`): loaded
//! before the login (`VIWndInitFromMC`, which sets the two sound classes'
//! volumes from it), written by the Options window's Save Changes
//! (`VIWndOptions::SaveOptions`) and at the server select
//! (`VIWndServerSelect::Connect`, for the home shard, which the client does
//! not keep: it has no server select). The client does the same with the
//! options it shows, and keeps its own beside them in `client.opt`: the
//! Options window's Extended page (Level of Detail, Brightness, Frame Rate,
//! Camera Distance), which the game does not have; and in `keys.opt` the
//! Controls page's keys and buttons (`binds`), per device as the card keeps
//! every option, not per character.
//!
//! Where: natively `$EQOA_CONFIG`, else `eqoa-rs` in the platform's config
//! directory (`$XDG_CONFIG_HOME` or `~/.config`, `~/Library/Application
//! Support` on a Mac, `%APPDATA%` on Windows), for `eqoa play`
//! (`keep_in_user_dir`); the other commands (`shot`, `playshot`) keep and read
//! nothing unless `$EQOA_CONFIG` names a directory, so a player's settings
//! never change a test's picture. In a page, the origin's localStorage
//! (`eqoa.game.opt`, `eqoa.client.opt` and `eqoa.keys.opt`, as hex), where every access may
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
const GAME_FILE: &str = "game.opt";
const CLIENT_FILE: &str = "client.opt";
const KEYS_FILE: &str = "keys.opt";

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
    let mut o = store::read(GAME_FILE).and_then(|b| GameOptions::from_card(&b)).unwrap_or(GameOptions { compact: 1, ..Default::default() });
    if std::env::var_os("EQOA_SOFT_KEYBOARD").is_some() {
        o.compact = 0;
    }
    o
}

/// Save Changes (`SaveOptions`: `VIGetGameOptions()->Save()`), with the
/// client's own settings beside the game's. Whether both were kept; a
/// failure is said and play goes on (the game's `m_saveFailed`).
pub fn save(o: &GameOptions) -> bool {
    let mut card = with_login(o.to_card());
    // the home shard the server list keeps there stays (`set_home_shard`)
    if let Some(b) = store::read(GAME_FILE).filter(|b| GameOptions::from_card(b).is_some()) {
        card[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN].copy_from_slice(&b[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN]);
    }
    let game = store::write(GAME_FILE, &card);
    let client = store::write(CLIENT_FILE, client().to_text().as_bytes());
    let keys = store::write(KEYS_FILE, with_binds(|b| b.to_text()).as_bytes());
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
    let Some(b) = store::read(GAME_FILE).filter(|b| GameOptions::from_card(b).is_some()) else {
        return (String::new(), String::new(), false);
    };
    let text = |r: std::ops::Range<usize>| b[r].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
    (text(0x10..0x50), text(0x50..0x90), b[0x90] != 0)
}

/// `card` with the login kept on the card put back (`to_card` writes none).
fn with_login(mut card: [u8; CARD_SIZE]) -> [u8; CARD_SIZE] {
    if let Some(b) = store::read(GAME_FILE).filter(|b| GameOptions::from_card(b).is_some()) {
        card[0x10..0x91].copy_from_slice(&b[0x10..0x91]);
    }
    card
}

/// STATION's `SaveAccount` and the `Save()` three frames on: the login
/// (`SetUsername`, `SetPassword`, 63 characters and the terminator each) and
/// the flag into the options, the rest of the record as it was (the
/// defaults' when there was none). Whether it was kept.
pub fn save_login(user: &str, password: &str, on: bool) -> bool {
    let mut card = store::read(GAME_FILE).filter(|b| GameOptions::from_card(b).is_some()).and_then(|b| <[u8; CARD_SIZE]>::try_from(&b[..CARD_SIZE]).ok()).unwrap_or_else(|| game_options().to_card());
    for (at, s) in [(0x10, user), (0x50, password)] {
        card[at..at + 0x40].fill(0);
        for (k, c) in s.chars().take(0x3F).enumerate() {
            card[at + k] = if (c as u32) < 256 { c as u8 } else { b'?' };
        }
    }
    card[0x90] = on as u8;
    match store::write(GAME_FILE, &card) {
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
    let b = store::read(GAME_FILE).filter(|b| GameOptions::from_card(b).is_some())?;
    let w: Vec<u16> = b[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
    (!w.is_empty()).then(|| String::from_utf16_lossy(&w))
}

/// `SetHomeShardName(name)` and `Save()` (`VIWndServerSelect::Connect`):
/// the name into game.opt (63 characters at most), the rest of it as kept,
/// or the options as they stand when there is none.
pub fn set_home_shard(name: &str) -> Result<(), String> {
    let mut b = store::read(GAME_FILE).filter(|b| GameOptions::from_card(b).is_some()).unwrap_or_else(|| game_options().to_card().to_vec());
    let w: Vec<u16> = name.encode_utf16().take(HOME_SHARD_LEN / 2 - 1).collect();
    b[HOME_SHARD..HOME_SHARD + HOME_SHARD_LEN].fill(0);
    for (k, c) in w.iter().enumerate() {
        b[HOME_SHARD + 2 * k..HOME_SHARD + 2 * k + 2].copy_from_slice(&c.to_le_bytes());
    }
    store::write(GAME_FILE, &b)
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

// ---- the client's own: the Controls page ----------------------------------------

thread_local! {
    /// The keys and buttons now (`binds`): read from the store the first
    /// time they are asked for.
    static BINDS: std::cell::RefCell<Option<crate::binds::Binds>> = const { std::cell::RefCell::new(None) };
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
            b
        });
        f(b)
    })
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
    /// display shows). 30 is the 33 ms a page on WebGL2 has kept since
    /// 2026-10-20.
    pub fn gap_ms(self) -> f64 {
        match self {
            FrameRate::Thirty => 33.0,
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

/// The client's own settings: the Options window's Extended page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientOptions {
    /// Level of Detail: the game's levels (`VILODSprite::Raster`) while set;
    /// off, every LOD body draws its near level out to its off distance
    pub lod: bool,
    /// Brightness, 0 to `BRIGHTNESS_STEPS` (`curve`)
    pub brightness: u8,
    /// Frame Rate as chosen; `None` until it is, the platform's
    /// (`set_platform_rate`: 30 on a page's WebGL2, else the display's)
    pub frame_rate: Option<FrameRate>,
    /// Camera Distance: the follow orbit's ceiling, `CAMERA_MIN` to
    /// `CAMERA_MAX`
    pub camera_distance: u8,
}

impl Default for ClientOptions {
    fn default() -> Self {
        ClientOptions { lod: true, brightness: BRIGHTNESS_AS_DRAWN, frame_rate: None, camera_distance: crate::camstyle::FOLLOW_MAX_DISTANCE as u8 }
    }
}

impl ClientOptions {
    /// `client.opt`: a line a setting, `name=value`.
    pub fn to_text(self) -> String {
        let mut s = format!("lod={}\nbrightness={}\ncamera_distance={}\n", if self.lod { "on" } else { "off" }, self.brightness, self.camera_distance);
        if let Some(r) = self.frame_rate {
            s.push_str(&format!("frame_rate={}\n", r.key()));
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
                _ => {}
            }
        }
        c
    }
}

thread_local! {
    /// The client's settings now (`client`): read from the store the first
    /// time they are asked for.
    static CLIENT: Cell<Option<ClientOptions>> = const { Cell::new(None) };
    /// The frame rate when none was chosen (`set_platform_rate`).
    static PLATFORM_RATE: Cell<FrameRate> = const { Cell::new(FrameRate::Display) };
    /// When the next capped frame is due (`now_ms`'s clock).
    static FRAME_DUE: Cell<f64> = const { Cell::new(0.0) };
}

/// The client's settings now: what was kept (`client.opt`), as the page
/// changed them since.
pub fn client() -> ClientOptions {
    CLIENT.with(|c| {
        c.get().unwrap_or_else(|| {
            let v = store::read(CLIENT_FILE).map_or_else(ClientOptions::default, |b| ClientOptions::from_text(&String::from_utf8_lossy(&b)));
            c.set(Some(v));
            v
        })
    })
}

/// Changes them (the Extended page, as its rows move); kept only by Save
/// Changes (`save`).
pub fn set_client(v: ClientOptions) {
    CLIENT.with(|c| c.set(Some(v)));
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

/// Whether this frame is drawn under the Frame Rate: always without a cap;
/// with one, a frame before the next is due is skipped whole (a page skips
/// the animation frame, the game's clocks then seeing longer frames rather
/// than fewer ticks).
pub fn frame_due() -> bool {
    let gap = frame_rate().gap_ms();
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
    let gap = frame_rate().gap_ms();
    if gap == 0.0 {
        return None;
    }
    let wait = FRAME_DUE.with(|d| d.get()) - EARLY - now_ms();
    (wait > 0.0).then(|| std::time::Duration::from_secs_f64(wait / 1000.0))
}

// ---- where they are kept ---------------------------------------------------------

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
}

/// Natively: files in the config directory.
#[cfg(all(not(test), not(target_arch = "wasm32")))]
mod store {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub static USER_DIR: AtomicBool = AtomicBool::new(false);

    /// `$EQOA_CONFIG`, else (`keep_in_user_dir`) `eqoa-rs` in the
    /// platform's config directory.
    fn dir() -> Option<PathBuf> {
        if let Some(d) = std::env::var_os("EQOA_CONFIG") {
            return Some(PathBuf::from(d));
        }
        if !USER_DIR.load(Ordering::Relaxed) {
            return None;
        }
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
        super::PLATFORM_RATE.with(|p| p.set(super::FrameRate::Display));
        super::FRAME_DUE.with(|d| d.set(0.0));
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
        let mine = ClientOptions { lod: false, brightness: 8, frame_rate: Some(FrameRate::Sixty), camera_distance: 50 };
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
            ClientOptions { lod: false, brightness: 0, frame_rate: Some(FrameRate::Thirty), camera_distance: 8 },
            ClientOptions { lod: true, brightness: 10, frame_rate: Some(FrameRate::Display), camera_distance: 50 },
        ] {
            assert_eq!(ClientOptions::from_text(&c.to_text()), c);
        }
        assert_eq!(ClientOptions::default().to_text(), "lod=on\nbrightness=5\ncamera_distance=11\n", "no Frame Rate until one is chosen");
        let odd = ClientOptions::from_text("brightness=11\nlod=maybe\nframe_rate=120\ncamera_distance=51\nwhat=ever\nnoise");
        assert_eq!(odd, ClientOptions::default());
        assert_eq!(ClientOptions::from_text("camera_distance=7").camera_distance, 11);
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
        assert_eq!(thirty, 33.0, "the page's 30 as it was");
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
}
