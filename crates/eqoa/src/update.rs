//! The updater: a build CI made follows the one rolling `dev` release on
//! GitHub (`.github/workflows/dev.yml`) and puts a newer build there in its
//! own place when the game starts, before the title.
//!
//! CI bakes the channel and the commit in (`EQOA_UPDATE_CHANNEL`,
//! `EQOA_BUILD_COMMIT`); a build made anywhere else has neither and never
//! updates itself. The game as players start it (`eqoa`, `eqoa boot`, `eqoa
//! <image>`; not a scripted command, nor with `EQOA_OFFSCREEN` or
//! `EQOA_NO_UPDATE` set) starts a thread (`start`) that reads the release's
//! `update.txt`: the commit it was built from and, a line a platform, the
//! program's file, its SHA-256 and its size. Another commit's program is
//! downloaded beside this one (`<exe>.new`), held to that size and hash, and
//! put in this one's place by two renames, the running program aside to
//! `<exe>.old` first: a running program is never written to, which Windows
//! refuses, and a failure between the two puts the old one back. The boot's
//! pictures show meanwhile; the boot waits for the thread before the title
//! (`settled`, at most `WAIT`, a line on the picture while it downloads), and
//! with the new program in place it closes the window (`staged`), after which
//! `relaunch_if_requested` starts the new program with the same arguments. The
//! next start removes `<exe>.old`. A failure anywhere leaves the program as it
//! was, says so in the log, and the game goes on.
//!
//! Testing: `EQOA_UPDATE_URL=<base>` reads `<base>update.txt` and the files
//! beside it instead of the release (a server of one's own, `python3 -m
//! http.server`), offscreen too, and a build given `EQOA_UPDATE_CHANNEL=dev
//! EQOA_BUILD_COMMIT=<commit>` follows it.

use std::time::Duration;

/// The channel CI built this for (`dev`); none for any other build.
pub const CHANNEL: Option<&str> = option_env!("EQOA_UPDATE_CHANNEL");
/// The commit CI built this from (its `GITHUB_SHA`).
pub const COMMIT: Option<&str> = option_env!("EQOA_BUILD_COMMIT");

/// This platform's name in `update.txt`: the programs `dev.yml` builds.
pub const PLATFORM: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x86_64")
} else if cfg!(all(windows, target_arch = "x86_64")) {
    Some("windows-x86_64")
} else if cfg!(target_os = "macos") {
    Some("macos")
} else {
    None
};

/// How long the boot waits for the updater before the title, from its
/// start: a check takes a second or two and a download a few more on most
/// lines; past this the game goes on, and a download still coming is put in
/// place for the next start.
pub const WAIT: Duration = Duration::from_secs(30);

/// One platform's program in `update.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub platform: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
}

/// `update.txt`, as `dev.yml` writes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub version: String,
    pub commit: String,
    pub builds: Vec<Build>,
}

impl Manifest {
    /// Its lines: `version <name>`, `commit <sha>` and `build <platform>
    /// <file> <sha256> <size>`; any other line, or a build line that does not
    /// read, is passed over.
    pub fn parse(s: &str) -> Manifest {
        let mut m = Manifest::default();
        for line in s.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w.as_slice() {
                ["version", v] => m.version = v.to_string(),
                ["commit", c] => m.commit = c.to_string(),
                ["build", platform, file, sha, size] if sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()) => {
                    if let Ok(size) = size.parse() {
                        m.builds.push(Build { platform: platform.to_string(), file: file.to_string(), sha256: sha.to_ascii_lowercase(), size });
                    }
                }
                _ => {}
            }
        }
        m
    }

    pub fn build(&self, platform: &str) -> Option<&Build> {
        self.builds.iter().find(|b| b.platform == platform)
    }
}

/// Where the updater stands.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// asking the release
    Checking,
    /// the release's program coming: the bytes so far, of
    Downloading { version: String, done: u64, total: u64 },
    /// nothing to do, or it failed (said in the log): the game goes on
    Done,
    /// the release's program (`version`) is in place: the boot closes the
    /// window for `relaunch_if_requested`
    Staged { version: String },
}

impl State {
    /// Whether the boot may go on to the title, `waited` after the updater
    /// started.
    pub fn settled(&self, waited: Duration) -> bool {
        !matches!(self, State::Checking | State::Downloading { .. }) || waited >= WAIT
    }

    /// The line the boot's picture carries.
    pub fn line(&self) -> Option<String> {
        match self {
            State::Downloading { version, done, total } => Some(format!("Downloading the update ({version}): {}%", done * 100 / (*total).max(1))),
            State::Staged { version } => Some(format!("Starting the update ({version})")),
            _ => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{relaunch_if_requested, start};

/// The boot found the release's program in place and closes the window:
/// `relaunch_if_requested` starts it after (natively; a page updates itself).
pub fn request_relaunch() {
    #[cfg(not(target_arch = "wasm32"))]
    native::request_relaunch();
}

/// Whether the boot may go on to the title: no updater this run, it is
/// done, a program is in place, or it has had `WAIT`.
pub fn settled() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return native::with(|s, waited| s.settled(waited)).unwrap_or(true);
    #[cfg(target_arch = "wasm32")]
    true
}

/// A new program is in place: the boot closes the window, to start it.
pub fn staged() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return native::with(|s, _| matches!(s, State::Staged { .. })).unwrap_or(false);
    #[cfg(target_arch = "wasm32")]
    false
}

/// The line the boot's picture carries while the update downloads.
pub fn line() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return native::with(|s, _| s.line()).flatten();
    #[cfg(target_arch = "wasm32")]
    None
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Build, COMMIT, CHANNEL, Manifest, PLATFORM, State};
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, Instant, SystemTime};

    /// The release this build follows.
    const RELEASE: &str = "https://github.com/DabDavis/eqoa-rust-client-vanilla/releases/download/dev/";

    struct Shared {
        state: State,
        since: Instant,
        /// the program's path as it started (after the swap, Linux names a
        /// running program by its new name, `<exe>.old`)
        exe: PathBuf,
        relaunch: bool,
    }

    static UPDATER: Mutex<Option<Shared>> = Mutex::new(None);

    pub(super) fn with<T>(f: impl FnOnce(&State, Duration) -> T) -> Option<T> {
        UPDATER.lock().ok()?.as_ref().map(|s| f(&s.state, s.since.elapsed()))
    }

    fn set(state: State) {
        if let Ok(mut u) = UPDATER.lock()
            && let Some(s) = u.as_mut()
        {
            s.state = state;
        }
    }

    /// The updater's thread, when this build follows a channel and the run
    /// is a player's (the module's doc).
    pub fn start() {
        clean_up();
        let (Some(_), Some(commit), Some(platform)) = (CHANNEL, COMMIT, PLATFORM) else { return };
        if let Some(from) = std::env::var_os("EQOA_UPDATED") {
            // the program an update just started: it does not ask again
            crate::say(&format!("update: this is the release's build now ({})", from.to_string_lossy()));
            return;
        }
        // (a server of one's own is a test of the updater: it runs offscreen too)
        let test = std::env::var("EQOA_UPDATE_URL").ok();
        if std::env::var_os("EQOA_NO_UPDATE").is_some() || (std::env::var_os("EQOA_OFFSCREEN").is_some() && test.is_none()) {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let base = test.unwrap_or_else(|| RELEASE.to_string());
        *UPDATER.lock().unwrap() = Some(Shared { state: State::Checking, since: Instant::now(), exe: exe.clone(), relaunch: false });
        let spawned = std::thread::Builder::new().name("update".into()).spawn(move || {
            let state = match update(&base, &exe, commit, platform, &set) {
                Ok(Some(version)) => {
                    crate::say(&format!("update: {version} is in place"));
                    State::Staged { version }
                }
                Ok(None) => State::Done,
                Err(e) => {
                    crate::say(&format!("update: {e}"));
                    State::Done
                }
            };
            set(state);
        });
        if spawned.is_err() {
            set(State::Done);
        }
    }

    pub(super) fn request_relaunch() {
        if let Ok(mut u) = UPDATER.lock()
            && let Some(s) = u.as_mut()
        {
            s.relaunch = true;
        }
    }

    /// Once the window and the sound device are closed (main's `boot`): the
    /// program the update put in place, started with this one's arguments,
    /// in its place (on Linux and macOS the same process).
    pub fn relaunch_if_requested() {
        let Some((exe, version)) = UPDATER.lock().ok().and_then(|u| {
            let s = u.as_ref()?;
            let State::Staged { version } = &s.state else { return None };
            s.relaunch.then(|| (s.exe.clone(), version.clone()))
        }) else {
            return;
        };
        // stderr as it was before the log's pipe: the new program would
        // inherit a pipe nothing reads any more; and this window's place
        // (multiboxing) let go for it
        crate::report::restore_stderr();
        crate::multibox::leave();
        let mut c = std::process::Command::new(&exe);
        c.args(std::env::args_os().skip(1)).env("EQOA_UPDATED", &version);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let e = c.exec();
            crate::say(&format!("update: {} did not start: {e}", exe.display()));
        }
        #[cfg(not(unix))]
        if let Err(e) = c.spawn() {
            crate::say(&format!("update: {} did not start: {e}", exe.display()));
        }
    }

    /// The program an update moved aside, removed (on Windows it may still
    /// be running for a moment: the next start removes it then).
    pub fn clean_up() {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::fs::remove_file(sibling(&exe, "old"));
        }
    }

    /// `<exe>.<ext>` beside the program (`eqoa.exe.new`).
    pub(super) fn sibling(exe: &Path, ext: &str) -> PathBuf {
        let mut name = exe.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(ext);
        exe.with_file_name(name)
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(format!("eqoa-vanilla/{}", COMMIT.unwrap_or("local")))
            .build()
            .into()
    }

    /// The check and, when the release is another commit's, its program
    /// downloaded and put in place: the release's version, or None when
    /// there was nothing to do.
    pub(super) fn update(base: &str, exe: &Path, commit: &str, platform: &str, progress: &dyn Fn(State)) -> Result<Option<String>, String> {
        let agent = agent();
        let url = format!("{base}update.txt");
        let text = agent
            .get(&url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))?;
        let m = Manifest::parse(&text);
        if m.commit.is_empty() {
            return Err(format!("{url} names no commit"));
        }
        if m.commit.eq_ignore_ascii_case(commit) {
            return Ok(None);
        }
        let b = m.build(platform).ok_or_else(|| format!("{url} has no {platform} build"))?;
        // the same program under another commit's name: nothing to do
        if sha256_file(exe).is_ok_and(|h| h == b.sha256) {
            return Ok(None);
        }
        let Some(_lock) = Lock::take(exe) else {
            // another window of the game is updating it
            return Ok(None);
        };
        let new = sibling(exe, "new");
        let version = if m.version.is_empty() { m.commit.clone() } else { m.version.clone() };
        progress(State::Downloading { version: version.clone(), done: 0, total: b.size });
        download(&agent, &format!("{base}{}", b.file), &new, b, &|done| progress(State::Downloading { version: version.clone(), done, total: b.size }))?;
        swap(exe, &new)?;
        Ok(Some(version))
    }

    /// The file at `url` into `to`, held to the build's size and SHA-256;
    /// nothing left there when it is not.
    fn download(agent: &ureq::Agent, url: &str, to: &Path, b: &Build, progress: &dyn Fn(u64)) -> Result<(), String> {
        let r = (|| -> Result<(), String> {
            let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
            let mut body = resp.body_mut().with_config().limit(b.size + 1).reader();
            let mut f = std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 64 * 1024];
            let mut done = 0u64;
            let mut shown = 0u64;
            loop {
                let n = body.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                f.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", to.display()))?;
                done += n as u64;
                if done - shown >= 256 * 1024 {
                    progress(done);
                    shown = done;
                }
            }
            f.sync_all().map_err(|e| format!("{}: {e}", to.display()))?;
            if done != b.size {
                return Err(format!("{url}: {done} bytes, where update.txt says {}", b.size));
            }
            if hex(&h.finalize()) != b.sha256 {
                return Err(format!("{url}: its SHA-256 is not the one update.txt names"));
            }
            Ok(())
        })();
        if r.is_err() {
            let _ = std::fs::remove_file(to);
        }
        r
    }

    /// The new program in the old one's place: the old aside to
    /// `<exe>.old` (a running program may be renamed, never written to),
    /// the new one to its name with its permissions; the old one put back
    /// when the second rename fails.
    pub(super) fn swap(exe: &Path, new: &Path) -> Result<(), String> {
        if let Ok(m) = std::fs::metadata(exe) {
            let _ = std::fs::set_permissions(new, m.permissions());
        }
        let old = sibling(exe, "old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).map_err(|e| format!("{}: {e}", exe.display()))?;
        if let Err(e) = std::fs::rename(new, exe) {
            let _ = std::fs::rename(&old, exe);
            let _ = std::fs::remove_file(new);
            return Err(format!("{}: {e}", exe.display()));
        }
        Ok(())
    }

    pub(super) fn sha256_file(path: &Path) -> std::io::Result<String> {
        let mut f = std::fs::File::open(path)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex(&h.finalize()))
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// `<exe>.lock`, held while one window of the game updates the program:
    /// another started with it leaves the update to it. One left by a
    /// program that died is taken after ten minutes.
    struct Lock(PathBuf);

    impl Lock {
        fn take(exe: &Path) -> Option<Lock> {
            let path = sibling(exe, "lock");
            let stale = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age > Duration::from_secs(600));
            if stale {
                let _ = std::fs::remove_file(&path);
            }
            std::fs::OpenOptions::new().write(true).create_new(true).open(&path).ok().map(|_| Lock(path))
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::native::{sha256_file, sibling, swap, update};
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;

    const OLD: &[u8] = b"the program as it is";
    const NEW: &[u8] = b"the release's program, a commit later";

    fn sha(b: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    /// A directory of its own in the temporary directory, emptied.
    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eqoa-update-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A server on a port of its own answering `files` by path (404 for any
    /// other), each request on a connection of its own.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { continue };
                let mut req = Vec::new();
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).is_ok_and(|n| n == 1) {
                    req.push(b[0]);
                }
                let line = String::from_utf8_lossy(&req).lines().next().unwrap_or_default().to_string();
                let path = line.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
                match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = s.write_all(body);
                    }
                    None => {
                        let _ = write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        base
    }

    fn manifest(commit: &str, sha: &str, size: usize) -> Vec<u8> {
        format!(
            "version dev-{}\ncommit {commit}\nbuild linux-x86_64 eqoa-vanilla-linux-x86_64 {sha} {size}\nbuild windows-x86_64 eqoa-vanilla-windows-x86_64.exe {sha} {size}\nbuild macos eqoa-vanilla-macos {sha} {size}\n",
            &commit[..7]
        )
        .into_bytes()
    }

    const MINE: &str = "1111111111111111111111111111111111111111";
    const THEIRS: &str = "2222222222222222222222222222222222222222";

    /// Its lines as dev.yml writes them; a line that does not read, a short
    /// hash and unknown lines passed over; no such platform is none.
    #[test]
    fn the_manifest_reads_as_ci_writes_it() {
        let text = String::from_utf8(manifest(THEIRS, &sha(NEW), NEW.len())).unwrap() + "build linux-x86_64 other abc 12\nbuild ios x zz 1\nsomething else\n";
        let m = Manifest::parse(&text);
        assert_eq!((m.version.as_str(), m.commit.as_str(), m.builds.len()), ("dev-2222222", THEIRS, 3));
        assert_eq!(m.build("windows-x86_64").map(|b| (b.file.as_str(), b.size)), Some(("eqoa-vanilla-windows-x86_64.exe", NEW.len() as u64)));
        assert_eq!(m.build("linux-x86_64").unwrap().sha256, sha(NEW), "the first linux line, not the one with a short hash");
        assert!(m.build("freebsd").is_none());
    }

    /// Another commit's program downloaded and put in place, the old one
    /// aside; the same commit, the same program and a release that does not
    /// answer leave it as it is.
    #[test]
    fn another_commits_program_is_put_in_place() {
        let d = dir("swap");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len())), ("eqoa-vanilla-linux-x86_64", NEW.to_vec())]);
        // the control: this program's own commit
        assert_eq!(update(&base, &exe, THEIRS, "linux-x86_64", &|_| {}), Ok(None));
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        let seen = std::sync::Mutex::new(Vec::new());
        assert_eq!(update(&base, &exe, MINE, "linux-x86_64", &|s| seen.lock().unwrap().push(s)), Ok(Some("dev-2222222".into())));
        assert_eq!(std::fs::read(&exe).unwrap(), NEW);
        assert_eq!(std::fs::read(sibling(&exe, "old")).unwrap(), OLD);
        assert!(!sibling(&exe, "new").exists() && !sibling(&exe, "lock").exists());
        assert_eq!(seen.lock().unwrap().first(), Some(&State::Downloading { version: "dev-2222222".into(), done: 0, total: NEW.len() as u64 }));
        // the program is the release's already (another commit, the same bytes)
        assert_eq!(update(&base, &exe, MINE, "linux-x86_64", &|_| {}), Ok(None));
        // nothing answers
        let gone = "http://127.0.0.1:9/";
        assert!(update(gone, &exe, MINE, "linux-x86_64", &|_| {}).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), NEW);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A download that is not the program update.txt promised (another
    /// hash, another size, no such file) leaves the program as it is and
    /// nothing beside it.
    #[test]
    fn a_download_that_is_not_the_promised_one_changes_nothing() {
        for (name, m, file) in [
            ("hash", manifest(THEIRS, &sha(b"something else"), NEW.len()), NEW.to_vec()),
            ("size", manifest(THEIRS, &sha(NEW), NEW.len() + 1), NEW.to_vec()),
            ("long", manifest(THEIRS, &sha(NEW), NEW.len() - 1), NEW.to_vec()),
        ] {
            let d = dir(name);
            let exe = d.join("eqoa.exe");
            std::fs::write(&exe, OLD).unwrap();
            let base = serve(vec![("update.txt", m), ("eqoa-vanilla-windows-x86_64.exe", file)]);
            assert!(update(&base, &exe, MINE, "windows-x86_64", &|_| {}).is_err(), "{name}");
            assert_eq!(std::fs::read(&exe).unwrap(), OLD, "{name}");
            assert!(!sibling(&exe, "new").exists() && !sibling(&exe, "old").exists(), "{name}");
            let _ = std::fs::remove_dir_all(&d);
        }
        let d = dir("missing");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len()))]);
        assert!(update(&base, &exe, MINE, "macos", &|_| {}).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Another window updating the program (its lock) leaves it alone.
    #[test]
    fn a_second_window_leaves_the_update_to_the_first() {
        let d = dir("lock");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        std::fs::write(sibling(&exe, "lock"), b"").unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len())), ("eqoa-vanilla-linux-x86_64", NEW.to_vec())]);
        assert_eq!(update(&base, &exe, MINE, "linux-x86_64", &|_| {}), Ok(None));
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        std::fs::remove_file(sibling(&exe, "lock")).unwrap();
        assert!(update(&base, &exe, MINE, "linux-x86_64", &|_| {}).unwrap().is_some(), "the control: unlocked, it updates");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The second rename failing puts the old program back.
    #[test]
    fn a_failed_swap_puts_the_program_back() {
        let d = dir("rollback");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        assert!(swap(&exe, &d.join("not-there")).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        assert!(!sibling(&exe, "old").exists());
        assert_eq!(sha256_file(&exe).unwrap(), sha(OLD));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The boot waits while the updater checks or downloads, for `WAIT` at
    /// most; its line says how far the download is.
    #[test]
    fn the_boot_waits_for_the_updater_at_most_its_while() {
        let down = State::Downloading { version: "dev-2222222".into(), done: 512, total: 1024 };
        assert!(!State::Checking.settled(Duration::from_secs(1)) && !down.settled(Duration::from_secs(29)));
        assert!(down.settled(WAIT) && State::Done.settled(Duration::ZERO));
        assert!(State::Staged { version: "v".into() }.settled(Duration::ZERO));
        assert_eq!(down.line().as_deref(), Some("Downloading the update (dev-2222222): 50%"));
        assert_eq!(State::Checking.line(), None);
    }
}
