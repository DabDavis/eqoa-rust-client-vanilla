//! The game's code modules the client reads data out of: CLIENT (the start
//! menu's definitions, `gWorldInfo`, the race cameras) and SUPPORT (the
//! HUD's atlas, the UI sounds' volumes), which the disc holds only
//! DNAS-encrypted. Natively each is the player's decrypted image when there
//! is one (`$EQOA_CLIENT`, else `run/client.bin`; `$EQOA_SUPPORT`, else
//! `run/support.bin`), else it is unpacked off the disc by `dnas` on a thread
//! of its own and kept for the next run (`cache_dir`); in a page, the page's
//! worker unpacks it (`web::unpack_modules`). Either way an image comes when
//! it comes: `get` answers `None` until then, and what reads one takes it up
//! once it is there (`Play::take_modules`, `Sound::take_support`).
//!
//! An image is kept only when it is the Frontiers build's (`check`), since
//! the addresses read out of it are that build's.

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    Client,
    Support,
}

impl Module {
    pub const ALL: [Module; 2] = [Module::Client, Module::Support];

    /// Its name on the disc (`<name>.AUT`).
    pub fn name(self) -> &'static str {
        match self {
            Module::Client => "CLIENT",
            Module::Support => "SUPPORT",
        }
    }

    /// What the client has from it.
    pub fn gives(self) -> &'static str {
        match self {
            Module::Client => "the start menu",
            Module::Support => "the HUD",
        }
    }
}

static IMAGES: Mutex<[Option<Arc<Vec<u8>>>; 2]> = Mutex::new([None, None]);

/// The module's image, once it has come.
pub fn get(m: Module) -> Option<Arc<Vec<u8>>> {
    IMAGES.lock().unwrap()[m as usize].clone()
}

/// Keeps an image that passed `check`.
pub fn put(m: Module, image: Vec<u8>) {
    IMAGES.lock().unwrap()[m as usize] = Some(Arc::new(image));
}

/// Why an image cannot serve, if it cannot: not an SNR1 module, or not the
/// Frontiers build's (CLIENT: `ClientImage::is_frontiers`; SUPPORT: its
/// atlas, `vi::ui::read_atlas`).
pub fn check(m: Module, image: &[u8]) -> Result<(), &'static str> {
    if image.get(0..4) != Some(b"SNR1") {
        return Err("not an SNR1 module");
    }
    let frontiers = match m {
        Module::Client => crate::client_image::ClientImage::from_bytes(image.to_vec()).is_some_and(|c| c.is_frontiers()),
        Module::Support => vi::ui::read_atlas(image).is_some(),
    };
    if frontiers { Ok(()) } else { Err("not the Frontiers build's") }
}

/// Natively, as a session starts: each module out of the player's file, else
/// off the disc (`iso_path`) on a thread of its own, so it is there by the
/// time the world is, or soon after.
pub fn load(iso_path: &std::path::Path) {
    for m in Module::ALL {
        if get(m).is_some() {
            continue;
        }
        if let Some(image) = from_file(m) {
            put(m, image);
            continue;
        }
        let iso = iso_path.to_path_buf();
        std::thread::spawn(move || {
            if let Some(image) = from_disc(m, &iso) {
                put(m, image);
            }
        });
    }
}

/// One module, waited for (`eqoa listen` has no session to wait in).
pub fn load_now(m: Module, iso_path: &std::path::Path) -> Option<Arc<Vec<u8>>> {
    if get(m).is_none()
        && let Some(image) = from_file(m).or_else(|| from_disc(m, iso_path))
    {
        put(m, image);
    }
    get(m)
}

/// The player's decrypted image: `$EQOA_<NAME>`, else `run/<name>.bin`.
fn from_file(m: Module) -> Option<Vec<u8>> {
    let path = std::env::var(format!("EQOA_{}", m.name())).unwrap_or_else(|_| format!("run/{}.bin", m.name().to_lowercase()));
    let image = std::fs::read(&path).ok()?;
    match check(m, &image) {
        Ok(()) => Some(image),
        Err(e) => {
            eprintln!("{path}: {e}; {} comes off the disc instead", m.name());
            None
        }
    }
}

/// Off the disc: the cache's copy when it has one, else unpacked by the
/// disc's own DNAS library (`dnas`) and kept in the cache.
fn from_disc(m: Module, iso_path: &std::path::Path) -> Option<Vec<u8>> {
    let files = (|| {
        let mut disc = iso::Iso::open(iso_path).map_err(|e| e.to_string())?;
        let util = disc.read("UTIL.REL").map_err(|e| e.to_string())?;
        let aut = disc.read(&format!("{}.AUT", m.name())).map_err(|e| e.to_string())?;
        Ok::<_, String>((util, aut))
    })();
    let (util, aut) = match files {
        Ok(f) => f,
        Err(e) => {
            eprintln!("no {}: {} is not on the disc ({e})", m.gives(), m.name());
            return None;
        }
    };
    let cached = cache_dir().map(|d| d.join(cache_name(m, &util, &aut)));
    if let Some(p) = &cached
        && let Ok(image) = std::fs::read(p)
        && check(m, &image).is_ok()
    {
        return Some(image);
    }
    let t = std::time::Instant::now();
    let image = match dnas::unpack(&util, &aut).map_err(|e| e.to_string()).and_then(|i| check(m, &i).map(|()| i).map_err(String::from)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("no {}: {} off the disc: {e}", m.gives(), m.name());
            return None;
        }
    };
    eprintln!("{} unpacked off the disc in {:.1} s", m.name(), t.elapsed().as_secs_f64());
    if let Some(p) = cached {
        // written whole, then named: a run cut short leaves no half module
        let tmp = p.with_extension("tmp");
        let kept = p.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&tmp, &image)).and_then(|()| std::fs::rename(&tmp, &p));
        if let Err(e) = kept {
            eprintln!("{}: not kept in {} ({e})", m.name(), p.display());
        }
    }
    Some(image)
}

/// Where unpacked modules are kept between runs: `$EQOA_CACHE`, else
/// `eqoa-rs/dnas` in the platform's cache directory (`$XDG_CACHE_HOME` or
/// `~/.cache`, `~/Library/Caches` on a Mac, `%LOCALAPPDATA%` on Windows).
fn cache_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(d) = std::env::var_os("EQOA_CACHE") {
        return Some(PathBuf::from(d));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("eqoa-rs").join("dnas"))
}

/// The cache's name for a module: its name and a hash (FNV-1a, 64 bits) of
/// what it was unpacked from, so another disc's never answers for it.
fn cache_name(m: Module, util: &[u8], aut: &[u8]) -> String {
    let h = util.iter().chain(aut).fold(0xCBF2_9CE4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3));
    format!("{}-{h:016x}.snr1", m.name())
}
