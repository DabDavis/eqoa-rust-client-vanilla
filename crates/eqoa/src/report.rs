//! Problem reports and crash reports: what a player sends when something goes
//! wrong, to the report server (reportd on eqoaclassic, `/report` and `/crash`;
//! its public board, eqoaclassic.com/eqoa/reports/board, shows a report's note
//! and picture and never the rest), as eqoa-rs sent them and the Go client
//! before it (Dab's Mod's F3 and crash reports). Nothing is sent unless the
//! player says so, and the account name and the password the game sent
//! Station are taken out of all of it (`secret`), as is the home directory's
//! path.
//!
//! In a page they are web/report.js's (its dialog, the canvas's picture, the
//! page's own log and what it catches); the client hands it its state, the
//! frame drawn, the secrets and the HUD's button through `crate::web`.
//!
//! Natively they are this module's (`native`):
//! - the log: what the program writes to stderr goes through a pipe (`start`)
//!   to the terminal it had, to `eqoa.log` in the config directory (the run
//!   before kept as `eqoa.prev.log`; a second window open beside the first,
//!   multiboxing, keeps `eqoa-2.log`, and so on) and to the last lines a
//!   report carries;
//! - F3, or the HUD's Report a bug, takes a picture of the next frame and
//!   opens a dialog over the game, the pregame's screens too (`Dialog`): what
//!   happened, a name to be credited by on the board, Send or Close;
//! - a panic writes a crash report into `crashes/` at once (its message and
//!   backtrace, the client's state, the log), and a run that ended without
//!   closing (killed, or stopped by a fault no panic caught) is found by the
//!   next start in the log it left; the next start offers what is waiting
//!   (its first window only), every report in one offer, saying how many:
//!   Send (each its own, newest first), Not now or Delete.

/// The account name and the password the game sent Station (its API's
/// logins), taken out of every report.
pub fn secret(account: &str, password: &str) {
    #[cfg(target_arch = "wasm32")]
    crate::web::secret(account, password);
    #[cfg(not(target_arch = "wasm32"))]
    native::secret(account, password);
}

/// The client's state for a report, noted once a second at most: `state`
/// runs only then.
pub fn note_state(state: impl FnOnce() -> String) {
    #[cfg(target_arch = "wasm32")]
    crate::web::note_state(state);
    #[cfg(not(target_arch = "wasm32"))]
    native::note_state(state);
}

/// A frame is drawn: a page takes its picture now if it asked (natively the
/// frame's texture is read before it is presented, `native::take_shot`).
pub fn frame_drawn() {
    #[cfg(target_arch = "wasm32")]
    crate::web::frame_drawn();
}

/// F3 or the HUD's Report a bug: the report's dialog.
pub fn open() {
    #[cfg(target_arch = "wasm32")]
    crate::web::open_report();
    #[cfg(not(target_arch = "wasm32"))]
    native::open();
}

/// The report server: `EQOA_REPORTS=<base>` names another (a test's).
pub const SERVER: &str = "https://eqoaclassic.com/eqoa/reports";

/// A string as a JSON string, quotes and all.
pub fn json_string(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Standard base64, padded.
pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}

/// `text` with every password and every account name (as a word, case
/// kept: a character named after the account keeps its name) taken out,
/// as web/report.js's `scrub`; a password under four characters is as
/// likely to be part of a word and is left.
pub fn scrub(text: &str, accounts: &[String], passwords: &[String]) -> String {
    let mut t = text.to_string();
    for p in passwords.iter().filter(|p| p.len() >= 4) {
        t = t.replace(p.as_str(), "<password>");
    }
    for a in accounts.iter().filter(|a| !a.trim().is_empty()) {
        let a = a.trim();
        let mut out = String::with_capacity(t.len());
        let mut rest = t.as_str();
        while let Some(i) = rest.find(a) {
            let before = rest[..i].chars().next_back();
            let after = rest[i + a.len()..].chars().next();
            let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            out.push_str(&rest[..i]);
            out.push_str(if word(before) || word(after) { a } else { "<account>" });
            rest = &rest[i + a.len()..];
        }
        out.push_str(rest);
        t = out;
    }
    t
}

/// `text` cut to `max` bytes (at a character), saying so.
pub fn fit(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max.saturating_sub(40);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n(cut to fit the report server)\n", &text[..end])
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{Key, caught, clean_exit, click, crash, draw, is_open, is_report_key, key, key_of, restore_stderr, set_gpu, set_shot, start, wants_shot};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{SERVER, base64, fit, json_string, scrub};
    use std::collections::VecDeque;
    use std::fs::File;
    use std::io::{BufRead, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
    use std::sync::{Mutex, MutexGuard, mpsc};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use vi::ui::{Raster, Vui, wide};

    /// The log lines a report carries, and how long each may be.
    const LOG_LINES: usize = 200;
    const LOG_LINE_MAX: usize = 400;
    /// `eqoa.log` stops growing here (the ring goes on).
    const LOG_FILE_MAX: u64 = 32 << 20;
    const NOTE_MAX: usize = 1000;
    const NAME_MAX: usize = 32;
    /// the server keeps 512 KiB of a report and 256 KiB of a crash
    const REPORT_MAX: usize = 500 * 1024;
    const CRASH_MAX: usize = 240 * 1024;
    const SHOT_WIDTH: u32 = 1280;
    /// The crash reports kept waiting, at most.
    const CRASHES_KEPT: usize = 5;
    /// The line a run that closes writes last: a log without it is a run
    /// that did not close.
    const CLOSED: &str = "[eqoa] closed";
    /// The lines waiting for the terminal's thread (`tee`), at most: past
    /// them a line reaches the log and the report's ring only.
    const TERMINAL_QUEUE: usize = 1024;

    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- the log ------------------------------------------------------------

    struct Log {
        file: Option<File>,
        bytes: u64,
        ring: VecDeque<String>,
        /// the program's stderr before the pipe (`restore_stderr`), and on
        /// Windows the pipe's end that stands in for it (closed then)
        #[cfg(unix)]
        orig: Option<i32>,
        #[cfg(windows)]
        orig: Option<(usize, usize)>,
        #[cfg(not(any(unix, windows)))]
        orig: Option<()>,
        /// what the pipe's reader says as it reaches the end
        drained: Option<mpsc::Receiver<()>>,
    }

    static LOG: Mutex<Log> = Mutex::new(Log { file: None, bytes: 0, ring: VecDeque::new(), orig: None, drained: None });
    /// Where stderr went before the pipe, to write to (a descriptor, a
    /// handle), -1 without one: what must reach it before the program may end.
    static TERMINAL: AtomicIsize = AtomicIsize::new(-1);
    /// Whether the terminal's thread is behind: its queue full, so a write
    /// to where stderr went blocks (a pipe nothing reads) and the panic
    /// hook writes there no more (`to_terminal`).
    pub(super) static TERMINAL_BEHIND: AtomicBool = AtomicBool::new(false);
    /// This run's name: when it started, and which window it is past the
    /// first (`crash-<run>.txt` is its crash).
    static RUN: Mutex<String> = Mutex::new(String::new());
    static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
    /// This window, from 1 (`multibox`'s place): the files of its own.
    static WINDOW: AtomicUsize = AtomicUsize::new(1);

    fn dir() -> Option<PathBuf> {
        lock(&DIR).clone()
    }

    /// One of this window's files: `eqoa.log` the first window's,
    /// `eqoa-2.log` the second's.
    fn own(base: &str, ext: &str) -> String {
        window_file(base, ext, WINDOW.load(Ordering::Relaxed))
    }

    /// A window's file by its number (from 1).
    pub(super) fn window_file(base: &str, ext: &str, window: usize) -> String {
        match window {
            0 | 1 => format!("{base}.{ext}"),
            n => format!("{base}-{n}.{ext}"),
        }
    }

    fn push_line(l: &mut Log, line: &str) {
        if let Some(f) = l.file.as_mut()
            && l.bytes < LOG_FILE_MAX
            && f.write_all(line.as_bytes()).is_ok()
            && f.write_all(b"\n").is_ok()
        {
            l.bytes += line.len() as u64 + 1;
        }
        let mut short: String = line.chars().take(LOG_LINE_MAX).collect();
        if short.len() < line.len() {
            short.push('…');
        }
        l.ring.push_back(short);
        while l.ring.len() > LOG_LINES {
            l.ring.pop_front();
        }
    }

    /// The program's log for the game as players start it (main's `boot`):
    /// stderr through a pipe to where it went, `eqoa.log` and the ring; the
    /// run before checked for a crash it left; the panic hook.
    pub fn start() {
        let dir = crate::settings::config_dir();
        // a window open beside others (multiboxing) has files of its own:
        // its place's, taken now (`multibox`)
        let window = crate::multibox::place() + 1;
        WINDOW.store(window, Ordering::Relaxed);
        let stamp = stamp(SystemTime::now());
        let run = if window == 1 { stamp } else { format!("{stamp}-w{window}") };
        *lock(&RUN) = run.clone();
        *lock(&DIR) = dir.clone();
        if let Some(d) = &dir {
            let _ = std::fs::create_dir_all(d);
            let (log, prev) = (d.join(own("eqoa", "log")), d.join(own("eqoa", "prev.log")));
            if log.exists() {
                let _ = std::fs::rename(&log, &prev);
            }
            take_last_run(d, &prev);
            // the newest kept (one is offered a start)
            let old = waiting();
            for f in &old[..old.len().saturating_sub(CRASHES_KEPT)] {
                let _ = std::fs::remove_file(f);
            }
            if let Ok(f) = File::create(&log) {
                lock(&LOG).file = Some(f);
            }
        }
        let mut l = lock(&LOG);
        let head = format!("[eqoa] run {run}, {} ({}), {} {}, window {window}", version(), channel(), std::env::consts::OS, std::env::consts::ARCH);
        push_line(&mut l, &head);
        drop(l);
        tee();
        // the panic's message written here, not by the hook before (into the
        // pipe): the pipe's reader is a thread the program does not wait for
        // as a panic ends it, and the terminal would miss the message
        std::panic::set_hook(Box::new(move |info| {
            // one `caught` steps over: no crash, nothing on the terminal
            if CAUGHT.with(|c| c.get()) > 0 {
                return;
            }
            let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
            // after the graphics device was lost (an upload in the frame that
            // lost it): the game starts again in this one's place, its log
            // closed so the next start offers no crash (`gpulost`)
            if crate::gpulost::lost() && crate::gpulost::settled() {
                if let Ok(mut l) = LOG.try_lock() {
                    push_line(&mut l, &format!("graphics: thread '{thread}' {info}"));
                    push_line(&mut l, "graphics: the device was lost: the game starts again");
                    push_line(&mut l, CLOSED);
                    if let Some(f) = l.file.as_mut() {
                        let _ = f.flush();
                    }
                    if let Some(orig) = l.orig.take() {
                        restore(orig);
                    }
                }
                crate::gpulost::relaunch();
            }
            let bt = std::backtrace::Backtrace::force_capture();
            write_crash(&format!("A panic on the {thread} thread: {info}\n\nbacktrace:\n{bt}"), None, SystemTime::now());
            let text = format!("thread '{thread}' {info}");
            if let Ok(mut l) = LOG.try_lock() {
                for line in text.lines() {
                    push_line(&mut l, line);
                }
            }
            let shown = std::env::var("RUST_BACKTRACE").is_ok_and(|v| v != "0");
            to_terminal(&if shown { format!("{text}\nstack backtrace:\n{bt}\n") } else { format!("{text}\n") });
        }));
    }

    thread_local! {
        /// How deep this thread is in `caught`: a panic there is no crash.
        static CAUGHT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    /// `f` with a panic in it caught and handed back as its message, written
    /// as no crash report and said nowhere: for a library's own fault the game
    /// can step over (gilrs's lookup of a controller it holds no state for
    /// yet, `input.rs`).
    pub fn caught<T>(f: impl FnOnce() -> T) -> Result<T, String> {
        CAUGHT.with(|c| c.set(c.get() + 1));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        CAUGHT.with(|c| c.set(c.get() - 1));
        r.map_err(|e| e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".to_string()))
    }

    /// `text` to where stderr went before the pipe (stderr itself without
    /// one).
    fn to_terminal(text: &str) {
        let t = TERMINAL.load(Ordering::Relaxed);
        if t < 0 {
            eprint!("{text}");
            return;
        }
        // where stderr went reads nothing: a write there would never return
        if TERMINAL_BEHIND.load(Ordering::Relaxed) {
            return;
        }
        // SAFETY: the copy `tee` made of the old stderr, never closed
        #[cfg(unix)]
        let f = unsafe { <File as std::os::fd::FromRawFd>::from_raw_fd(t as i32) };
        #[cfg(windows)]
        let f = unsafe { <File as std::os::windows::io::FromRawHandle>::from_raw_handle(t as _) };
        #[cfg(any(unix, windows))]
        {
            let mut f = std::mem::ManuallyDrop::new(f);
            let _ = f.write_all(text.as_bytes());
        }
    }

    /// stderr into a pipe whose other end a thread reads, line by line, into
    /// the log and back out to where stderr went. The thread never stops
    /// reading (a full pipe would stop every writer, the game with it), so
    /// where stderr went is written by a thread of its own, which may block
    /// there: a launcher's pipe that nothing reads (a Steam Deck in Steam's
    /// game mode, "seems to hang from steam") once filled stopped the reader
    /// in its write, then every `eprintln!`, the game's frames with them.
    fn tee() {
        let Ok((reader, writer)) = std::io::pipe() else { return };
        #[cfg(unix)]
        let to = {
            use std::os::fd::{AsRawFd, FromRawFd};
            // SAFETY: duplicating and replacing the process's own fd 2
            let orig = unsafe { libc::dup(2) };
            if orig < 0 || unsafe { libc::dup2(writer.as_raw_fd(), 2) } < 0 {
                return;
            }
            lock(&LOG).orig = Some(orig);
            drop(writer);
            // SAFETY: a copy of the old stderr of its own (`orig` stays for
            // `restore_stderr`), never closed
            let copy = unsafe { libc::dup(orig) };
            if copy >= 0 {
                TERMINAL.store(copy as isize, Ordering::Relaxed);
            }
            (copy >= 0).then(|| unsafe { File::from_raw_fd(copy) })
        };
        #[cfg(windows)]
        let to = {
            use std::os::windows::io::{FromRawHandle, IntoRawHandle};
            use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, SetStdHandle};
            // SAFETY: the process's own standard handles
            let orig = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
            let w = writer.into_raw_handle();
            if unsafe { SetStdHandle(STD_ERROR_HANDLE, w as _) } == 0 {
                return;
            }
            let valid = !orig.is_null() && orig as isize != -1;
            lock(&LOG).orig = Some((orig as usize, w as usize));
            if valid {
                TERMINAL.store(orig as isize, Ordering::Relaxed);
            }
            // SAFETY: the old stderr, a console or a file the program was
            // started with; never closed (ManuallyDrop below)
            valid.then(|| unsafe { File::from_raw_handle(orig as _) })
        };
        #[cfg(not(any(unix, windows)))]
        let to: Option<File> = {
            drop(writer);
            None
        };
        let (done, drained) = mpsc::channel();
        lock(&LOG).drained = Some(drained);
        let terminal = to.and_then(|t| terminal_thread(Kept(std::mem::ManuallyDrop::new(t))));
        let _ = std::thread::Builder::new().name("log".into()).spawn(move || {
            let (queue, written) = terminal.unzip();
            read_lines(std::io::BufReader::new(reader), queue, |text| push_line(&mut lock(&LOG), text));
            // what the terminal's thread still holds, a moment at most
            if let Some(w) = written {
                let _ = w.recv_timeout(Duration::from_millis(300));
            }
            let _ = done.send(());
        });
    }

    /// Where stderr went, written to and never closed (`restore_stderr` puts
    /// it back as fd 2).
    struct Kept(std::mem::ManuallyDrop<File>);

    impl Write for Kept {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }

    /// A thread writing what it is handed to `to` (where stderr went), and
    /// the queue to hand it lines by; the second channel says it is done
    /// (the queue let go and emptied, or a write failed).
    pub(super) fn terminal_thread<W: Write + Send + 'static>(mut to: W) -> Option<(mpsc::SyncSender<Vec<u8>>, mpsc::Receiver<()>)> {
        let (queue, lines) = mpsc::sync_channel::<Vec<u8>>(TERMINAL_QUEUE);
        let (written, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("terminal".into())
            .spawn(move || {
                for line in lines {
                    if to.write_all(&line).is_err() {
                        break;
                    }
                }
                let _ = written.send(());
            })
            .ok()?;
        Some((queue, done))
    }

    /// The log's reader: every line of `reader` to `sink` (the log and the
    /// ring) as it comes, and queued for the terminal's thread, never waited
    /// for: while its queue is full a line goes to `sink` only, and
    /// `TERMINAL_BEHIND` says so (the panic hook's message too). Until the
    /// pipe's end closes.
    pub(super) fn read_lines(mut r: impl BufRead, mut terminal: Option<mpsc::SyncSender<Vec<u8>>>, mut sink: impl FnMut(&str)) {
        let mut line = Vec::new();
        loop {
            line.clear();
            match r.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if let Some(t) = terminal.as_ref() {
                match t.try_send(line.clone()) {
                    Ok(()) => TERMINAL_BEHIND.store(false, Ordering::Relaxed),
                    Err(mpsc::TrySendError::Full(_)) => TERMINAL_BEHIND.store(true, Ordering::Relaxed),
                    Err(mpsc::TrySendError::Disconnected(_)) => terminal = None,
                }
            }
            let text = String::from_utf8_lossy(&line);
            sink(text.trim_end_matches(['\n', '\r']));
        }
    }

    /// stderr back where it went before `start` (a program started in this
    /// one's place inherits it, and would find a pipe nothing reads), and the
    /// pipe's end closed: its reader reads what is left and stops.
    pub fn restore_stderr() {
        let Some(orig) = lock(&LOG).orig.take() else { return };
        restore(orig);
    }

    #[cfg(unix)]
    type Orig = i32;
    #[cfg(windows)]
    type Orig = (usize, usize);
    #[cfg(not(any(unix, windows)))]
    type Orig = ();

    fn restore(orig: Orig) {
        #[cfg(unix)]
        // SAFETY: the copy of the old stderr `tee` kept; fd 2 was the pipe's
        // only end to write to
        unsafe {
            libc::dup2(orig, 2);
        }
        #[cfg(windows)]
        // SAFETY: the old stderr handle `tee` kept, and the pipe's end it
        // stood in for, this module's alone
        unsafe {
            windows_sys::Win32::System::Console::SetStdHandle(windows_sys::Win32::System::Console::STD_ERROR_HANDLE, orig.0 as _);
            windows_sys::Win32::Foundation::CloseHandle(orig.1 as _);
        }
        #[cfg(not(any(unix, windows)))]
        let _ = orig;
    }

    /// A crash report for the next start that no panic wrote: `detail`, with
    /// the state and the log as they are now.
    pub fn crash(detail: &str) {
        write_crash(detail, None, SystemTime::now());
    }

    /// The run closes: stderr to where it went, what the pipe holds read
    /// into the log (half a second at most), and the log's last line says so.
    pub fn clean_exit() {
        restore_stderr();
        let drained = lock(&LOG).drained.take();
        if let Some(d) = drained {
            let _ = d.recv_timeout(Duration::from_millis(500));
        }
        let mut l = lock(&LOG);
        push_line(&mut l, CLOSED);
        if let Some(f) = l.file.as_mut() {
            let _ = f.flush();
        }
    }

    fn tail(n: usize) -> Vec<String> {
        let l = lock(&LOG);
        l.ring.iter().skip(l.ring.len().saturating_sub(n)).cloned().collect()
    }

    // ---- the state, the secrets ---------------------------------------------

    struct Noted {
        state: String,
        at: Option<Instant>,
        saved: Option<Instant>,
        accounts: Vec<String>,
        passwords: Vec<String>,
    }

    static NOTED: Mutex<Noted> = Mutex::new(Noted { state: String::new(), at: None, saved: None, accounts: Vec::new(), passwords: Vec::new() });

    pub(super) fn secret(account: &str, password: &str) {
        let mut n = lock(&NOTED);
        if !account.trim().is_empty() && !n.accounts.iter().any(|a| a == account.trim()) {
            n.accounts.push(account.trim().to_string());
        }
        if password.len() >= 4 && !n.passwords.iter().any(|p| p == password) {
            n.passwords.push(password.to_string());
        }
    }

    pub(super) fn note_state(state: impl FnOnce() -> String) {
        let mut n = lock(&NOTED);
        if n.at.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        n.state = state();
        n.at = Some(Instant::now());
        // kept on disk now and then, scrubbed: what a run that does not
        // close leaves the next start
        if n.saved.is_none_or(|t| t.elapsed() >= Duration::from_secs(10))
            && let Some(d) = dir()
        {
            n.saved = Some(Instant::now());
            let text = clean(&n.state, &n);
            let _ = std::fs::write(d.join(own("state", "txt")), text);
        }
    }

    /// Scrubbed: the secrets, and the home directory as `~`.
    fn clean(text: &str, n: &Noted) -> String {
        let mut t = scrub(text, &n.accounts, &n.passwords);
        for home in ["HOME", "USERPROFILE"].iter().filter_map(std::env::var_os) {
            let h = home.to_string_lossy().into_owned();
            if h.len() > 1 {
                t = t.replace(&h, "~");
            }
        }
        t
    }

    fn clean_now(text: &str) -> String {
        match NOTED.try_lock() {
            Ok(n) => clean(text, &n),
            Err(_) => clean(text, &Noted { state: String::new(), at: None, saved: None, accounts: Vec::new(), passwords: Vec::new() }),
        }
    }

    fn version() -> String {
        crate::update::COMMIT.map_or_else(|| "local".to_string(), |c| format!("dev-{}", &c[..c.len().min(7)]))
    }

    fn channel() -> &'static str {
        crate::update::CHANNEL.unwrap_or("local")
    }

    fn platform() -> String {
        let os = match std::env::consts::OS {
            "linux" => "Linux",
            "windows" => "Windows",
            "macos" => "macOS",
            o => o,
        };
        let gpu = lock(&GPU).clone();
        format!("{os} {}{}", std::env::consts::ARCH, if gpu.is_empty() { String::new() } else { format!(" {gpu}") }).chars().take(64).collect()
    }

    /// The graphics adapter, for a report's platform.
    static GPU: Mutex<String> = Mutex::new(String::new());

    /// The graphics adapter's name and API (the viewer's, as it opens one).
    pub fn set_gpu(s: String) {
        *lock(&GPU) = s;
    }

    // ---- crashes ------------------------------------------------------------

    pub(super) fn stamp(t: SystemTime) -> String {
        let s = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let (days, secs) = (s / 86400, s % 86400);
        // the civil date of a day count (Howard Hinnant's)
        let z = days as i64 + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + (m <= 2) as i64;
        format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    }

    fn crashes() -> Option<PathBuf> {
        Some(dir()?.join("crashes"))
    }

    /// A crash report on disk now (`crashes/crash-<run>.txt`): `detail`,
    /// the state last noted, the log's last lines; scrubbed. `log` stands
    /// in for this run's log (a run before's). `at` is when the game
    /// stopped (its `when:`). Locks are only tried: a panic may come while
    /// one is held.
    fn write_crash(detail: &str, log: Option<(&str, &[String], &str)>, at: SystemTime) {
        let Some(d) = crashes() else { return };
        write_crash_in(&d, detail, log, at, None);
    }

    /// `write_crash` into the crash directory `d`; `of` the version that
    /// stopped as its log's first line names it ("dev-79f115e (dev)"), this
    /// program's when it is this run's.
    fn write_crash_in(d: &Path, detail: &str, log: Option<(&str, &[String], &str)>, at: SystemTime, of: Option<&str>) {
        let _ = std::fs::create_dir_all(d);
        let run = RUN.try_lock().map(|r| r.clone()).unwrap_or_default();
        let (name, state, lines) = match log {
            Some((run, lines, state)) => (format!("crash-{run}.txt"), state.to_string(), lines.to_vec()),
            None => {
                let state = NOTED.try_lock().map(|n| n.state.clone()).unwrap_or_default();
                let lines = LOG.try_lock().map(|l| l.ring.iter().cloned().collect()).unwrap_or_default();
                (format!("crash-{run}.txt"), state, lines)
            }
        };
        let text = format!(
            "EQOA native client crash report\nversion: {}\nwhen: {}\nplatform: {}\n\n--- crash ---\n{}\n\n[client, as last noted]\n{}\n\n[log] last {} lines\n{}\n",
            of.map_or_else(|| format!("{} ({})", version(), channel()), str::to_string),
            stamp(at),
            platform(),
            detail.trim_end(),
            if state.trim().is_empty() { "-" } else { state.trim_end() },
            lines.len(),
            lines.join("\n")
        );
        let _ = std::fs::write(d.join(name), clean_now(&text));
    }

    /// The run before's leavings, at a start: its crash report if it did not
    /// close (`ended_without_closing`), then the state it noted put away, so
    /// a run that stops before noting any (a graphics driver ending the
    /// program as its device is made) is reported with none, never with an
    /// earlier run's (F3 crash 20261026-130834: a run that stopped at its
    /// window came with the state of a two-and-a-half-hour run before it).
    pub(super) fn take_last_run(dir: &Path, prev: &Path) {
        ended_without_closing(dir, prev);
        let _ = std::fs::remove_file(dir.join(own("state", "txt")));
    }

    #[cfg(test)]
    pub(super) fn ended_without_closing_for_test(dir: &Path, prev: &Path) {
        ended_without_closing(dir, prev);
    }

    /// The run before (this window's), if it did not close (no `CLOSED` in
    /// its log) and wrote no crash report of its own: one written for it,
    /// from the log and the state it left.
    fn ended_without_closing(dir: &Path, prev: &Path) {
        let Ok(text) = std::fs::read_to_string(prev) else { return };
        let lines: Vec<&str> = text.lines().collect();
        let Some(first) = lines.first() else { return };
        if lines.contains(&CLOSED) {
            return;
        }
        let Some(run) = first.strip_prefix("[eqoa] run ").and_then(|r| r.split(',').next()) else { return };
        // "[eqoa] run <run>, <version> (<channel>), <os> <arch>, window <n>"
        let of = first.split(", ").nth(1).filter(|v| v.ends_with(')'));
        if dir.join("crashes").join(format!("crash-{run}.txt")).exists() {
            return;
        }
        let state = std::fs::read_to_string(dir.join(own("state", "txt"))).unwrap_or_default();
        let tail: Vec<String> = lines[lines.len().saturating_sub(LOG_LINES)..].iter().map(|l| l.chars().take(LOG_LINE_MAX).collect()).collect();
        // it stopped about when its log was last written, not now
        let at = std::fs::metadata(prev).and_then(|m| m.modified()).unwrap_or_else(|_| SystemTime::now());
        write_crash_in(&dir.join("crashes"), &unclosed_detail(run, &lines), Some((run, &tail, &state)), at, of);
    }

    /// What a crash report says of a run that did not close: out of memory
    /// when its log ends with Rust's abort for an allocation refused (F3
    /// crash 20261026-220031, eleven hours in: "memory allocation of 229376
    /// bytes failed"), the memory its state last noted then the one to read.
    pub(super) fn unclosed_detail(run: &str, lines: &[&str]) -> String {
        let last = lines.iter().rev().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("");
        if last.starts_with("memory allocation of ") && last.ends_with(" failed") {
            return format!("The run of {run} ran out of memory: \"{last}\", Rust's abort when the system refuses an allocation (the machine's memory and page file full, or the program grown past them: the memory below, and over the run, tell which).");
        }
        format!("The run of {run} ended without closing: killed, or stopped by a fault no panic caught (its log's last line is the last it wrote).")
    }

    /// The crash reports waiting, oldest first.
    fn waiting() -> Vec<PathBuf> {
        crashes().map_or_else(Vec::new, |d| waiting_in(&d))
    }

    /// The crash reports in the directory `d`, oldest first (their names are
    /// their runs' start times).
    pub(super) fn waiting_in(d: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "txt")).collect();
        v.sort();
        v
    }

    // ---- sending ------------------------------------------------------------

    fn server() -> String {
        std::env::var("EQOA_REPORTS").unwrap_or_else(|_| SERVER.to_string()).trim_end_matches('/').to_string()
    }

    /// A POST of `body` (JSON) to the server's `route`: the report's id.
    pub(super) fn post(route: &str, body: String) -> Result<String, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .build()
            .into();
        let url = format!("{}/{route}", server());
        let mut r = agent.post(&url).header("Content-Type", "application/json").send(body).map_err(|e| format!("the report server could not be reached ({e})"))?;
        let status = r.status().as_u16();
        let text = r.body_mut().with_config().limit(64 * 1024).read_to_string().unwrap_or_default();
        let field = |k: &str| -> Option<String> {
            let at = text.find(&format!("\"{k}\""))? + k.len() + 2;
            let rest = text[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_string())
        };
        match field("id") {
            Some(id) if (200..300).contains(&status) && text.contains("\"ok\"") && !text.contains("\"ok\":false") => Ok(id),
            _ => Err(field("error").unwrap_or_else(|| format!("the report server answered {status}"))),
        }
    }

    fn problem_body(note: &str, name: &str, shot: Option<&[u8]>) -> String {
        let n = lock(&NOTED);
        let state = n.state.clone();
        drop(n);
        let log = tail(LOG_LINES);
        let report = fit(
            &clean_now(&format!(
                "EQOA native client problem report\nversion: {}\nchannel: {}\nname: {}\nnote: {}\n\n--- trace ---\neqoa native client trace {}\nplatform: {}\n{}\n\n[client]\n{}\n\n[controllers]\n{}\n\n[other windows]\n{}\n\n[log] last {} lines\n{}\n",
                version(),
                channel(),
                if name.is_empty() { "-" } else { name },
                if note.is_empty() { "-".to_string() } else { note.replace('\n', " ") },
                stamp(SystemTime::now()),
                platform(),
                crate::update::describe(),
                state.trim_end(),
                controllers(),
                other_windows(),
                log.len(),
                log.join("\n")
            )),
            REPORT_MAX,
        );
        format!(
            "{{\"report\":{},\"note\":{},\"name\":{},\"version\":{},\"platform\":{},\"channel\":{},\"screenshot\":{}}}",
            json_string(&report),
            json_string(&clean_now(note)),
            json_string(name),
            json_string(&version()),
            json_string(&platform()),
            json_string(channel()),
            json_string(&shot.map(base64).unwrap_or_default())
        )
    }

    /// The controllers connected and who reads them (`input`): the log says
    /// what a window reads only as it changes, long gone by a report.
    fn controllers() -> String {
        let l = crate::input::describe_controllers();
        if l.is_empty() { "not read yet".into() } else { l.join("\n") }
    }

    /// The lines of another window's log a report carries (`other_windows`).
    const OTHER_LINES: usize = 12;
    /// How much of the end of another window's log is read for them.
    const OTHER_TAIL: u64 = 256 * 1024;

    /// The other windows open, each with the last lines of its own log on
    /// its arrivals at the front, its controllers and its stalls (`focus:`,
    /// `controllers:`, `gamepad:`, `multibox:`, `stall:`; nothing a player
    /// typed or was sent): a report carries only its own window's log, and
    /// the delay of F3 20261026-091519 was the second window's, reported
    /// from the first; F3 20261029-025532's freezes were both windows' at
    /// once, and a stall's line says when, in UTC, to read them together.
    fn other_windows() -> String {
        let Some(d) = dir() else { return "none read (no config directory)".into() };
        let me = WINDOW.load(Ordering::Relaxed).max(1);
        let windows: Vec<usize> = crate::multibox::seat().others.iter().map(|o| o.place + 1).collect();
        other_windows_in(&d, me, &windows)
    }

    pub(super) fn other_windows_in(dir: &Path, me: usize, windows: &[usize]) -> String {
        use std::io::{Read, Seek, SeekFrom};
        let mut out = Vec::new();
        for &n in windows.iter().filter(|&&n| n != me) {
            let name = window_file("eqoa", "log", n);
            let mut text = String::new();
            let read = File::open(dir.join(&name)).and_then(|mut f| {
                let len = f.metadata()?.len();
                f.seek(SeekFrom::Start(len.saturating_sub(OTHER_TAIL)))?;
                let mut b = Vec::new();
                f.read_to_end(&mut b)?;
                text = String::from_utf8_lossy(&b).into_owned();
                Ok(())
            });
            if read.is_err() {
                out.push(format!("window {n}: no {name}"));
                continue;
            }
            let lines: Vec<&str> = text.lines().filter(|l| ["focus:", "controllers:", "gamepad:", "multibox:", "stall:"].iter().any(|p| l.starts_with(p))).collect();
            out.push(format!("window {n} ({name}), its last {} of these lines:", lines.len().min(OTHER_LINES)));
            out.extend(lines.iter().skip(lines.len().saturating_sub(OTHER_LINES)).map(|l| l.chars().take(LOG_LINE_MAX).collect::<String>()));
        }
        if out.is_empty() { "none open".into() } else { out.join("\n") }
    }

    /// A crash report as sent: the file the run before wrote, the note, and
    /// the program sending it (its version and updater: the crash may be of a
    /// version long gone, and a player who stays on one says why).
    fn crash_body(text: &str, note: &str) -> String {
        let sender = format!("[the program sending it]\nversion: {} ({})\n{}", version(), channel(), crate::update::describe());
        let report = fit(&clean_now(&format!("{}\nnote: {}\n\n{sender}\n", text.trim_end(), if note.is_empty() { "-" } else { note })), CRASH_MAX);
        format!(
            "{{\"report\":{},\"note\":{},\"version\":{},\"platform\":{},\"channel\":{}}}",
            json_string(&report),
            json_string(&clean_now(note)),
            json_string(&version()),
            json_string(&platform()),
            json_string(channel())
        )
    }

    // ---- the dialog -----------------------------------------------------------

    #[derive(Clone, Debug, PartialEq)]
    pub(super) enum Kind {
        /// F3: what happened and a name, with the frame's picture
        Problem,
        /// the crash reports waiting (their files), newest first: one offer
        /// for them all, as many as there are (a player whose game stopped
        /// several times was asked once a start, start after start, F3 crash
        /// 20261028-235209: "this window wont go away. Idk why. Everytime i
        /// log in this happens")
        Crash(Vec<PathBuf>),
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum Field {
        Note,
        Name,
        /// whether the picture goes with it (a problem's, when there is one)
        Picture,
    }

    /// What the dialog is doing.
    #[derive(Clone, Debug, PartialEq)]
    pub(super) enum Status {
        Editing,
        Sending,
        /// the ids the server gave, one a report
        Sent(Vec<String>),
        /// the ids of those sent before one failed (crash reports, newest
        /// first), and why it failed: the rest wait
        Failed(Vec<String>, String),
    }

    /// What a send came to: each report sent (a crash report's file, none
    /// for a problem's) with its id, and the error that stopped it, if one
    /// did.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub(super) struct Sent {
        pub(super) sent: Vec<(Option<PathBuf>, String)>,
        /// crash reports gone before their turn (deleted meanwhile): nothing
        /// to send, and not waiting any more
        pub(super) gone: Vec<PathBuf>,
        pub(super) failed: Option<String>,
    }

    pub(super) struct Dialog {
        pub(super) kind: Kind,
        pub(super) note: String,
        pub(super) name: String,
        pub(super) field: Field,
        pub(super) status: Status,
        /// the picture, a PNG (a problem's)
        pub(super) shot: Option<Vec<u8>>,
        /// whether the picture goes with it: the player's to turn off (F3
        /// 20261023-083108, Liav: "Toggle button for sending a screenshot")
        pub(super) send_shot: bool,
        /// waiting for the frame's picture (shown once it came, or after a
        /// second without one)
        pub(super) shot_wanted: Option<Instant>,
        pub(super) result: std::sync::Arc<Mutex<Option<Sent>>>,
        /// the buttons, as last drawn (frame pixels): what a click finds
        pub(super) buttons: Vec<([i32; 4], Button)>,
        /// each crash report's own header, in `Kind::Crash`'s order: when
        /// the game stopped, in which version (read once, as the dialog
        /// opens)
        pub(super) stopped: Vec<Stopped>,
    }

    /// What a crash report waiting says of itself: when the game stopped
    /// (its `when:`) and the version it was (its `version:`), when it says.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub(super) struct Stopped {
        pub(super) at: Option<SystemTime>,
        pub(super) version: Option<String>,
    }

    pub(super) fn stopped(f: &Path) -> Stopped {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        let field = |k: &str| text.lines().take(8).find_map(|l| l.strip_prefix(k)).map(str::trim);
        Stopped {
            at: field("when: ").and_then(parse_stamp),
            version: field("version: ").and_then(|v| v.split_whitespace().next()).map(str::to_string),
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum Button {
        Send,
        Close,
        Delete,
        /// the picture's row: a click turns it on or off
        Picture,
    }

    static DIALOG: Mutex<Option<Dialog>> = Mutex::new(None);
    /// The crash reports were looked for this run (offered once).
    static OFFERED: Mutex<bool> = Mutex::new(false);

    fn name_kept() -> String {
        dir().and_then(|d| std::fs::read_to_string(d.join("report-name.txt")).ok()).map(|s| s.trim().to_string()).unwrap_or_default()
    }

    pub(super) fn open() {
        let mut d = lock(&DIALOG);
        if d.is_some() {
            return;
        }
        *d = Some(Dialog::new(Kind::Problem));
    }

    impl Dialog {
        pub(super) fn new(kind: Kind) -> Dialog {
            let problem = kind == Kind::Problem;
            let stopped = match &kind {
                Kind::Crash(files) => files.iter().map(|f| stopped(f)).collect(),
                Kind::Problem => Vec::new(),
            };
            Dialog {
                kind,
                stopped,
                note: String::new(),
                name: if problem { name_kept() } else { String::new() },
                field: Field::Note,
                status: Status::Editing,
                shot: None,
                send_shot: true,
                shot_wanted: problem.then(Instant::now),
                result: Default::default(),
                buttons: Vec::new(),
            }
        }

        fn shown(&self) -> bool {
            self.shot_wanted.is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
        }

        /// A key: true when it is the dialog's (it is open: every key is).
        pub(super) fn key(&mut self, k: Key) -> Action {
            if matches!(self.status, Status::Sending) {
                return Action::Stay;
            }
            if let Status::Sent(_) = self.status {
                return match k {
                    Key::Enter | Key::Escape => Action::Close,
                    _ => Action::Stay,
                };
            }
            match k {
                Key::Escape => Action::Close,
                Key::Enter => Action::Send,
                Key::Delete if matches!(self.kind, Kind::Crash(_)) => Action::Delete,
                // the note, the name, then the picture's row when there is one
                Key::Tab if self.kind == Kind::Problem => {
                    self.field = match self.field {
                        Field::Note => Field::Name,
                        Field::Name if self.shot.is_some() => Field::Picture,
                        _ => Field::Note,
                    };
                    Action::Stay
                }
                // on the picture's row Space turns it on or off; nothing is typed
                Key::Text(t) if self.field == Field::Picture => {
                    if t.contains(' ') {
                        self.send_shot = !self.send_shot;
                    }
                    Action::Stay
                }
                Key::Backspace if self.field == Field::Picture => Action::Stay,
                Key::Backspace => {
                    self.text().pop();
                    Action::Stay
                }
                Key::Text(t) => {
                    let max = if self.field == Field::Note { NOTE_MAX } else { NAME_MAX };
                    let field = self.field;
                    let s = self.text();
                    for c in t.chars().filter(|c| !c.is_control()) {
                        if s.chars().count() < max && (field == Field::Note || c != '\n') {
                            s.push(c);
                        }
                    }
                    Action::Stay
                }
                _ => Action::Stay,
            }
        }

        fn text(&mut self) -> &mut String {
            match self.field {
                Field::Note | Field::Picture => &mut self.note,
                Field::Name => &mut self.name,
            }
        }

        /// The picture that goes with a problem report: none when there is
        /// none, or the player turned it off.
        pub(super) fn shot_to_send(&self) -> Option<&[u8]> {
            self.shot.as_deref().filter(|_| self.send_shot)
        }

        /// A click at frame point `p`, on the buttons as last drawn: what it
        /// does (the picture's row turns the picture on or off at once).
        pub(super) fn click_at(&mut self, p: [f32; 2]) -> Action {
            let hit = self.buttons.iter().find(|(r, _)| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32).map(|b| b.1);
            match (hit, &self.status) {
                (_, Status::Sending) => Action::Stay,
                (Some(Button::Close), _) => Action::Close,
                (Some(Button::Send), Status::Sent(_)) => Action::Close,
                (Some(Button::Send), _) => Action::Send,
                (Some(Button::Delete), _) => Action::Delete,
                (Some(Button::Picture), Status::Sent(_)) => Action::Stay,
                (Some(Button::Picture), _) => {
                    self.send_shot = !self.send_shot;
                    self.field = Field::Picture;
                    Action::Stay
                }
                (None, _) => Action::Stay,
            }
        }

        fn send(&mut self) {
            self.status = Status::Sending;
            let (note, name) = (self.note.trim().to_string(), self.name.replace(['\r', '\n', '\t'], " ").trim().to_string());
            let result = self.result.clone();
            match &self.kind {
                Kind::Problem => {
                    if let Some(d) = dir() {
                        let _ = std::fs::write(d.join("report-name.txt"), &name);
                    }
                    let body = problem_body(&note, &name, self.shot_to_send());
                    let _ = std::thread::Builder::new().name("report".into()).spawn(move || {
                        let r = post("report", body);
                        crate::say(&match &r {
                            Ok(id) => format!("report: sent as {id}"),
                            Err(e) => format!("report: not sent: {e}"),
                        });
                        *lock(&result) = Some(match r {
                            Ok(id) => Sent { sent: vec![(None, id)], ..Default::default() },
                            Err(e) => Sent { failed: Some(e), ..Default::default() },
                        });
                    });
                }
                Kind::Crash(files) => {
                    let files = files.clone();
                    let _ = std::thread::Builder::new().name("report".into()).spawn(move || {
                        *lock(&result) = Some(send_crashes(&files, &note, post));
                    });
                }
            }
        }

        /// The send's answer, once it came: the crash reports it sent (or
        /// found gone) no longer waiting, so a Send after a failure sends
        /// only the rest.
        pub(super) fn poll(&mut self) {
            let Some(s) = lock(&self.result).take() else { return };
            if let Kind::Crash(files) = &mut self.kind {
                let done: Vec<&PathBuf> = s.sent.iter().filter_map(|(f, _)| f.as_ref()).chain(&s.gone).collect();
                let mut k = 0;
                while k < files.len() {
                    if done.contains(&&files[k]) {
                        files.remove(k);
                        self.stopped.remove(k);
                    } else {
                        k += 1;
                    }
                }
            }
            let ids = s.sent.into_iter().map(|(_, id)| id).collect();
            self.status = match s.failed {
                None => Status::Sent(ids),
                Some(e) => Status::Failed(ids, e),
            };
        }
    }

    /// The crash reports `files` sent, newest first (as `Kind::Crash` holds
    /// them), each its own POST through `post` (the server's route and the
    /// body, its id back) with the player's `note`, each one sent deleted.
    /// The first that fails stops it: it and those after it are kept, for a
    /// Send again or the next start. One gone before its turn is skipped.
    pub(super) fn send_crashes(files: &[PathBuf], note: &str, post: impl Fn(&str, String) -> Result<String, String>) -> Sent {
        let mut out = Sent::default();
        for f in files {
            let Ok(text) = std::fs::read_to_string(f) else {
                out.gone.push(f.clone());
                continue;
            };
            match post("crash", crash_body(&text, note)) {
                Ok(id) => {
                    let _ = std::fs::remove_file(f);
                    crate::say(&format!("report: sent as {id}"));
                    out.sent.push((Some(f.clone()), id));
                }
                Err(e) => {
                    crate::say(&format!("report: not sent: {e}"));
                    out.failed = Some(e);
                    break;
                }
            }
        }
        out
    }

    /// A key as the dialog takes it (the viewer's, from the window's).
    #[derive(Clone, Debug, PartialEq)]
    pub enum Key {
        Enter,
        Escape,
        Backspace,
        Tab,
        Delete,
        Text(String),
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum Action {
        Stay,
        Send,
        Close,
        Delete,
    }

    /// F3, the report's key (`binds::reserved` keeps it from the game).
    pub fn is_report_key(k: &winit::keyboard::Key) -> bool {
        *k == winit::keyboard::Key::Named(winit::keyboard::NamedKey::F3)
    }

    /// A key of the window's as the dialog takes it (the keyboard's own
    /// table is `viewer.rs`'s and `binds`'s: these keys are the dialog's
    /// alone, while it is open).
    pub fn key_of(event: &winit::event::KeyEvent) -> Option<Key> {
        use winit::keyboard::{Key as W, NamedKey as N};
        Some(match &event.logical_key {
            W::Named(N::Enter) => Key::Enter,
            W::Named(N::Escape) => Key::Escape,
            W::Named(N::Backspace) => Key::Backspace,
            W::Named(N::Tab) => Key::Tab,
            W::Named(N::Delete) => Key::Delete,
            W::Named(N::Space) => Key::Text(" ".into()),
            _ => Key::Text(event.text.as_ref()?.to_string()),
        })
    }

    /// Whether the dialog is open: it has the keyboard.
    pub fn is_open() -> bool {
        lock(&DIALOG).is_some()
    }

    pub(super) fn act(d: &mut Option<Dialog>, a: Action) {
        let Some(dl) = d.as_mut() else { return };
        match a {
            Action::Stay => {}
            Action::Send => dl.send(),
            Action::Close => *d = None,
            // every crash report the offer holds (Not now keeps them all)
            Action::Delete => {
                if let Kind::Crash(files) = &dl.kind {
                    for f in files {
                        let _ = std::fs::remove_file(f);
                    }
                }
                *d = None;
            }
        }
    }

    /// A key for the open dialog.
    pub fn key(k: Key) {
        let mut d = lock(&DIALOG);
        let a = match d.as_mut() {
            Some(dl) if dl.shown() => dl.key(k),
            _ => return,
        };
        act(&mut d, a);
    }

    /// A click at frame point `p`: true when the dialog took it (it is
    /// open: every click is).
    pub fn click(p: [f32; 2]) -> bool {
        let mut d = lock(&DIALOG);
        let Some(dl) = d.as_mut() else { return false };
        let a = dl.click_at(p);
        act(&mut d, a);
        true
    }

    /// Whether the dialog wants a picture of this frame (read before it is
    /// presented: the viewer's).
    pub fn wants_shot() -> bool {
        lock(&DIALOG).as_ref().is_some_and(|d| d.shot_wanted.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) && d.shot.is_none())
    }

    /// The frame's picture: RGBA rows, `w` by `h`, made at most `SHOT_WIDTH`
    /// wide and kept as a PNG for the report.
    pub fn set_shot(w: u32, h: u32, rgba: &[u8]) {
        let k = (SHOT_WIDTH as f32 / w as f32).min(1.0);
        let (ow, oh) = (((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
        let mut out = Vec::with_capacity((ow * oh * 4) as usize);
        for y in 0..oh {
            let sy = (y as f32 / k) as u32;
            for x in 0..ow {
                let s = ((sy.min(h - 1) * w + ((x as f32 / k) as u32).min(w - 1)) * 4) as usize;
                out.extend_from_slice(&rgba[s..s + 4]);
                // the frame's alpha is the GS's, not the picture's
                *out.last_mut().unwrap() = 255;
            }
        }
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, ow, oh);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            if let Ok(mut wr) = enc.write_header()
                && wr.write_image_data(&out).is_ok()
            {
                drop(wr);
            } else {
                png.clear();
            }
        }
        if let Some(d) = lock(&DIALOG).as_mut() {
            d.shot = (!png.is_empty()).then_some(png);
            d.shot_wanted = None;
        }
    }

    /// The offer of the crash reports waiting in `crashes` (the config
    /// directory's): one dialog for them all, newest first; none waiting,
    /// none.
    pub(super) fn offer_in(crashes: &Path) -> Option<Dialog> {
        let mut files = waiting_in(crashes);
        files.reverse();
        (!files.is_empty()).then(|| Dialog::new(Kind::Crash(files)))
    }

    /// The dialog over the frame, when it is open (and the crash reports
    /// waiting offered, all in one, once, the first time there is a UI to
    /// offer them on, by the first window: two would offer them twice): the
    /// 2D layer's `r`, the game's `vui`.
    pub fn draw(vui: &Vui, r: &mut dyn Raster) {
        let mut d = lock(&DIALOG);
        if d.is_none() {
            let mut offered = lock(&OFFERED);
            if !*offered {
                *offered = true;
                if WINDOW.load(Ordering::Relaxed) == 1
                    && let Some(c) = crashes()
                {
                    *d = offer_in(&c);
                }
            }
        }
        let Some(dl) = d.as_mut() else { return };
        dl.poll();
        if !dl.shown() {
            return;
        }
        draw_dialog(dl, vui, r);
    }

    /// `draw`'s panel: its lines, and its buttons noted where they were
    /// drawn for a click (`Dialog::click_at`).
    pub(super) fn draw_dialog(dl: &mut Dialog, vui: &Vui, r: &mut dyn Raster) {
        let white = [1.0, 1.0, 1.0, 1.0];
        let dim = [0.72, 0.72, 0.72, 1.0];
        let gold = [1.0, 0.85, 0.45, 1.0];
        let h = vui.font_height(r).max(10);
        // the panel across the frame's middle; its height is what it holds
        let (x0, x1) = (40, 600);
        let (tx, tw) = (x0 + 16, x1 - x0 - 32);
        let wrap = |r: &dyn Raster, s: &str, w: i32| -> Vec<String> {
            let mut out = Vec::new();
            for para in s.split('\n') {
                let mut cur = String::new();
                for word in para.split(' ') {
                    let t = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                    if vui.string_width(r, &wide(&t)) > w && !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        cur = word.to_string();
                    } else {
                        cur = t;
                    }
                }
                out.push(cur);
            }
            out
        };
        let caret = |on: bool| if on && blink() { "_" } else { "" };
        let editing = dl.status == Status::Editing;
        let (title, intro, label, fine) = match &dl.kind {
            Kind::Problem => (
                "Report a problem".to_string(),
                "What went wrong, and what were you doing?".to_string(),
                "What happened",
                "The note and the picture go on the public board (eqoaclassic.com/eqoa/reports/board), the rest to the developer only. Your account and password are never sent.",
            ),
            Kind::Crash(files) => {
                let (title, intro) = crash_words(&dl.stopped, &version(), SystemTime::now());
                (
                    title,
                    intro,
                    "What you were doing (optional)",
                    if files.len() > 1 {
                        "They go to the developer and are not shown anywhere. Your account and password are never sent."
                    } else {
                        "It goes to the developer and is not shown anywhere. Your account and password are never sent."
                    },
                )
            }
        };
        let several = matches!(&dl.kind, Kind::Crash(files) if files.len() > 1);
        // what goes where, top to bottom: (x, y, text, colour)
        let mut lines: Vec<(i32, i32, String, [f32; 4])> = Vec::new();
        let mut y = 0;
        lines.push((tx, y, title, gold));
        y += h + 6;
        for l in wrap(r, &intro, tw) {
            lines.push((tx, y, l, dim));
            y += h;
        }
        y += 6;
        lines.push((tx, y, label.to_string(), white));
        y += h + 2;
        let rows = 3;
        let note_box = (y, rows * h + 8);
        let note = wrap(r, &format!("{}{}", dl.note, caret(editing && dl.field == Field::Note)), tw - 12);
        for (k, l) in note[note.len().saturating_sub(rows as usize)..].iter().enumerate() {
            lines.push((tx + 6, y + 4 + k as i32 * h, l.clone(), white));
        }
        y += note_box.1 + 8;
        // the picture's row, a button: where it was drawn
        let mut pic_y = None;
        if dl.kind == Kind::Problem {
            let n = format!("Your name on the board (Tab): {}{}", dl.name, caret(editing && dl.field == Field::Name));
            lines.push((tx, y, n, if dl.field == Field::Name { white } else { dim }));
            y += h + 2;
            if dl.shot.is_some() {
                let pic = format!("Send the picture of the screen: {} (Tab, Space or click)", if dl.send_shot { "Yes" } else { "No" });
                lines.push((tx, y, pic, if dl.field == Field::Picture { white } else { dim }));
                pic_y = Some(y);
                y += h + 2;
            }
        }
        let left = match &dl.kind {
            Kind::Crash(files) => files.len(),
            Kind::Problem => 0,
        };
        let status = match &dl.status {
            Status::Editing => String::new(),
            Status::Sending => "Sending…".to_string(),
            Status::Sent(ids) if ids.len() > 1 => format!("Sent all {} as {}. Thank you.", ids.len(), and_list(ids)),
            Status::Sent(ids) => format!("Sent as {}. Thank you.", and_list(ids)),
            Status::Failed(ids, e) if ids.is_empty() => format!("Not sent: {e}. Try again in a moment."),
            Status::Failed(ids, e) => format!("Sent {} as {}; {left} not sent: {e}. Try again in a moment.", ids.len(), and_list(ids)),
        };
        if !status.is_empty() {
            let c = if matches!(dl.status, Status::Failed(..)) { [1.0, 0.45, 0.45, 1.0] } else { [0.55, 1.0, 0.55, 1.0] };
            for l in wrap(r, &status, tw) {
                lines.push((tx, y, l, c));
                y += h;
            }
        }
        y += 6;
        let sent = matches!(dl.status, Status::Sent(_));
        let labels: Vec<(String, Button)> = match (&dl.kind, sent) {
            (_, true) => vec![("Close (Enter)".into(), Button::Send)],
            (Kind::Problem, false) => vec![("Send (Enter)".into(), Button::Send), ("Close (Esc)".into(), Button::Close)],
            (Kind::Crash(_), false) if several => vec![("Send all (Enter)".into(), Button::Send), ("Not now (Esc)".into(), Button::Close), ("Delete all (Del)".into(), Button::Delete)],
            (Kind::Crash(_), false) => vec![("Send (Enter)".into(), Button::Send), ("Not now (Esc)".into(), Button::Close), ("Delete (Del)".into(), Button::Delete)],
        };
        let buttons_y = y;
        y += h + 10 + 8;
        let fine_y = y;
        let fine = wrap(r, fine, tw);
        y += fine.len() as i32 * h;
        // the panel round it, the frame's middle
        let height = y + 20;
        let y0 = ((448 - height) / 2).max(4);
        vui.fill_rectangle(r, [[x0, y0], [x1, y0 + height]], [0.02, 0.02, 0.05, 0.94]);
        vui.fill_rectangle(r, [[x0, y0], [x1, y0 + 2]], gold);
        let top = y0 + 10;
        vui.fill_rectangle(r, [[tx, top + note_box.0], [x1 - 16, top + note_box.0 + note_box.1]], [0.13, 0.13, 0.17, 1.0]);
        for (x, ly, text, c) in &lines {
            vui.draw_string_ex(r, &wide(text), *x, top + ly, *c, false);
        }
        dl.buttons.clear();
        if let Some(py) = pic_y {
            dl.buttons.push(([tx, top + py, x1 - 16, top + py + h], Button::Picture));
        }
        let mut bx = tx;
        for (label, b) in labels {
            let w = vui.string_width(r, &wide(&label)) + 24;
            let rect = [bx, top + buttons_y, bx + w, top + buttons_y + h + 10];
            vui.fill_rectangle(r, [[rect[0], rect[1]], [rect[2], rect[3]]], if b == Button::Send { [0.4, 0.31, 0.08, 1.0] } else { [0.2, 0.2, 0.25, 1.0] });
            vui.draw_string_ex(r, &wide(&label), bx + 12, rect[1] + 5, white, false);
            dl.buttons.push((rect, b));
            bx += w + 12;
        }
        for (k, l) in fine.iter().enumerate() {
            vui.draw_string_ex(r, &wide(l), tx, top + fine_y + k as i32 * h, [0.55, 0.55, 0.55, 1.0], false);
        }
    }

    /// The caret's half-second blink.
    fn blink() -> bool {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        (START.get_or_init(Instant::now).elapsed().as_millis() / 500).is_multiple_of(2)
    }

    /// `stamp`'s time again, off its text (`20261023-010203`, UTC).
    pub(super) fn parse_stamp(s: &str) -> Option<SystemTime> {
        let b = s.as_bytes();
        if b.len() != 15 || b[8] != b'-' || !b[..8].iter().chain(&b[9..]).all(u8::is_ascii_digit) {
            return None;
        }
        let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().unwrap_or(0);
        let (y, m, d) = (n(0..4), n(4..6), n(6..8));
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        // the day count of a civil date (Howard Hinnant's), `stamp`'s the other way
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146097 + doe - 719468;
        let secs = days * 86400 + n(9..11) * 3600 + n(11..13) * 60 + n(13..15);
        Some(UNIX_EPOCH + Duration::from_secs(u64::try_from(secs).ok()?))
    }

    /// How long before `now` the time `at` was, in a player's words.
    pub(super) fn ago(at: SystemTime, now: SystemTime) -> String {
        let s = now.duration_since(at).map_or(0, |d| d.as_secs());
        let n = |k: u64, one: &str, many: &str| if k == 1 { one.to_string() } else { format!("{k} {many}") };
        match s {
            0..60 => "less than a minute ago".to_string(),
            60..3600 => format!("{} ago", n(s / 60, "a minute", "minutes")),
            3600..172800 => format!("{} ago", n(s / 3600, "an hour", "hours")),
            _ => format!("{} days ago", s / 86400),
        }
    }

    /// A crash report's title and first words: when the game stopped, and
    /// that it was another version when it was not `this` one. A report
    /// waits until it is sent or deleted, so the one offered after an
    /// update's restart is from before it (F3 crash 20261023-225944, sent
    /// with "no crash, just notified about update": the title said "last
    /// time" and gave the hour its run had started, in UTC).
    ///
    /// Several waiting (newest first) are offered as one: how many times it
    /// stopped and when the last time was, and which of them were in another
    /// version, by its versions (newest first, each once).
    pub(super) fn crash_words(all: &[Stopped], this: &str, now: SystemTime) -> (String, String) {
        let newest = all.first().cloned().unwrap_or_default();
        if all.len() <= 1 {
            let title = match newest.at {
                Some(at) => format!("The game stopped {}", ago(at, now)),
                None => "The game stopped earlier".to_string(),
            };
            let then = "It wrote a crash report then: sending it helps it get fixed.";
            let intro = match &newest.version {
                Some(v) if v != this => format!("That was in an earlier version of the game ({v}). {then}"),
                _ => then.to_string(),
            };
            return (title, intro);
        }
        let times = if all.len() == 2 { "twice".to_string() } else { format!("{} times", all.len()) };
        let title = match newest.at {
            Some(at) => format!("The game stopped {times}, the last {}", ago(at, now)),
            None => format!("The game stopped {times}"),
        };
        let other: Vec<&Stopped> = all.iter().filter(|s| s.version.as_deref().is_some_and(|v| v != this)).collect();
        let mut versions: Vec<String> = Vec::new();
        for v in other.iter().filter_map(|s| s.version.clone()) {
            if !versions.contains(&v) {
                versions.push(v);
            }
        }
        let which = if versions.len() > 1 { "earlier versions" } else { "an earlier version" };
        let each = "It wrote a crash report each time: sending them helps it get fixed.";
        let intro = match other.len() {
            0 => each.to_string(),
            n if n == all.len() => format!("Those were in {which} of the game ({}). {each}", versions.join(", ")),
            1 => format!("One of them was in {which} of the game ({}). {each}", versions.join(", ")),
            n => format!("{n} of them were in {which} of the game ({}). {each}", versions.join(", ")),
        };
        (title, intro)
    }

    /// "a", "a and b", "a, b and c".
    pub(super) fn and_list(items: &[String]) -> String {
        match items {
            [] => String::new(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// `caught` hands a panic back as its message (gilrs's index panic is a
    /// formatted String, a literal a &str) and a value through as it is, its
    /// depth back to none after each, a nested one's too; the control: the
    /// same panic outside `caught` unwinds to `catch_unwind`, uncaught by it.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_caught_panic_comes_back_as_its_message() {
        use native::caught;
        let v: Vec<u8> = vec![1, 2];
        let i = v.len();
        assert_eq!(caught(|| v[i]), Err("index out of bounds: the len is 2 but the index is 2".to_string()));
        assert_eq!(caught(|| -> u8 { panic!("a literal") }), Err("a literal".to_string()));
        assert_eq!(caught(|| 7), Ok(7));
        assert_eq!(caught(|| caught(|| -> u8 { panic!("inner") }).map_err(|e| e + " (nested)")), Ok(Err("inner (nested)".to_string())));
        assert!(std::panic::catch_unwind(|| v[i]).is_err(), "the control: it panics");
        assert_eq!(caught(|| v[0]), Ok(1), "and after all that, a plain call");
    }

    /// A start takes the run before's leavings: a run that closed leaves no
    /// crash, and its state is put away; the next run, stopped at its
    /// window before noting any (F3 crash 20261026-130834), is reported with
    /// no state and as the version its own log names, not the state of the
    /// run before it nor the version reading it. The control: the same
    /// unclosed log with the old state left in place (`ended_without_closing`
    /// alone, as a start did) reports that state.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_crash_carries_its_own_runs_state_and_version() {
        use native::take_last_run;
        let dir = std::env::temp_dir().join(format!("eqoa-last-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (prev, state) = (dir.join("eqoa.prev.log"), dir.join("state.txt"));
        let crash = |run: &str| std::fs::read_to_string(dir.join("crashes").join(format!("crash-{run}.txt")));
        std::fs::write(&prev, "[eqoa] run 20261025-180304, dev-da994c8 (dev), windows x86_64, window 1\nplay: in the world\n[eqoa] closed\n").unwrap();
        std::fs::write(&state, "frames 536108, clock 9020.2 s\n").unwrap();
        take_last_run(&dir, &prev);
        assert!(crash("20261025-180304").is_err(), "a run that closed is no crash");
        assert!(!state.exists(), "its state put away");
        let stopped = "[eqoa] run 20261025-205606, dev-79f115e (dev), windows x86_64, window 1\nwindow: as kept (at 952,0, 958x991), on \\\\.\\DISPLAY1 of 1 monitor(s)\n";
        std::fs::write(&prev, stopped).unwrap();
        take_last_run(&dir, &prev);
        let text = crash("20261025-205606").unwrap();
        assert!(text.contains("\nversion: dev-79f115e (dev)\n"), "{text}");
        assert!(text.contains("[client, as last noted]\n-\n"), "{text}");
        assert!(text.contains("[log] last 2 lines\n[eqoa] run 20261025-205606"), "{text}");
        // the control
        std::fs::remove_dir_all(dir.join("crashes")).unwrap();
        std::fs::write(&state, "frames 536108, clock 9020.2 s\n").unwrap();
        native::ended_without_closing_for_test(&dir, &prev);
        assert!(crash("20261025-205606").unwrap().contains("[client, as last noted]\nframes 536108"), "the old state, left in place, is taken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A run whose log ends with Rust's abort for an allocation refused is
    /// said to have run out of memory (F3 crash 20261026-220031); the
    /// controls: the same line before others, and a log that just stops, are
    /// a run that ended without closing.
    #[test]
    fn a_run_out_of_memory_says_so() {
        use native::unclosed_detail;
        let oom = ["[eqoa] run 20261026-100442, dev-83bb416 (dev), windows x86_64, window 1", "chat: Mrboojanlezx: lfg", "memory allocation of 229376 bytes failed", ""];
        let d = unclosed_detail("20261026-100442", &oom);
        assert!(d.starts_with("The run of 20261026-100442 ran out of memory: \"memory allocation of 229376 bytes failed\""), "{d}");
        let before = ["[eqoa] run 1", "memory allocation of 229376 bytes failed", "chat: after it"];
        assert!(unclosed_detail("1", &before).contains("ended without closing"), "the control: not the last line");
        assert!(unclosed_detail("1", &oom[..2]).contains("ended without closing"), "the control: a log that just stops");
    }

    /// A report carries the other windows' arrivals, controllers and stalls
    /// out of their own logs, the last few of those lines only; the controls: this
    /// window's own log is not read again, a line of anything else (chat) is
    /// left out, and a window with no log says so.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_report_carries_the_other_windows_arrivals() {
        use native::{other_windows_in, window_file};
        let dir = std::env::temp_dir().join(format!("eqoa-other-windows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!((window_file("eqoa", "log", 1), window_file("eqoa", "log", 2)), ("eqoa.log".to_string(), "eqoa-2.log".to_string()));
        let mut two = String::new();
        for k in 0..20 {
            two += &format!("focus: in front; its last frame {k} ms before, its first frame 412 ms after; the first press 450 ms after: the controller's cross\n");
        }
        two += "chat: Someone tells the group: my account is hunter2\n";
        two += "stall: 8412 ms from one frame to the next, to 02:41:07 UTC (2:13:05 into the run): the frame itself took 8395 ms (sound 8355 ms)\n";
        two += "controllers: window 2 of 2, Pad 1: controller 1 (DualSense) while this window is in front (Capture Controller)\n";
        std::fs::write(dir.join("eqoa-2.log"), &two).unwrap();
        std::fs::write(dir.join("eqoa.log"), "focus: this window's own line\n").unwrap();
        let text = other_windows_in(&dir, 1, &[2, 3]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "window 2 (eqoa-2.log), its last 12 of these lines:");
        assert_eq!(lines.len(), 1 + 12 + 1, "{text}");
        assert!(lines[1].contains("its last frame 10 ms before"), "the last twelve, oldest first: {text}");
        assert!(lines[11].starts_with("stall: 8412 ms from one frame to the next, to 02:41:07 UTC"), "its stalls too: {text}");
        assert!(lines[12].starts_with("controllers: window 2 of 2"));
        assert_eq!(lines[13], "window 3: no eqoa-3.log");
        assert!(!text.contains("hunter2") && !text.contains("this window's own line"), "the controls: {text}");
        assert_eq!(other_windows_in(&dir, 2, &[2]), "none open", "the control: its own window is not another");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Strings as JSON strings: quotes, backslashes, line ends and control
    /// characters escaped, the rest as it is.
    #[test]
    fn strings_go_out_as_json() {
        assert_eq!(json_string("a \"b\" \\ c\nd\te\u{1}é"), "\"a \\\"b\\\" \\\\ c\\nd\\te\\u0001é\"");
    }

    /// Base64 as the standard has it, padding and all.
    #[test]
    fn base64_is_the_standard_one() {
        for (b, s) in [(&b""[..], ""), (b"f", "Zg=="), (b"fo", "Zm8="), (b"foo", "Zm9v"), (b"foobar", "Zm9vYmFy"), (&[0xff, 0xfe, 0x00][..], "//4A")] {
            assert_eq!(base64(b), s);
        }
    }

    /// The password out wherever it is, the account as a word only: a
    /// character named after it keeps its name; a short password stays.
    #[test]
    fn secrets_come_out_of_a_report() {
        let t = "login zed with Tulip-7781; character Zedra; zed_x; (zed) said abc";
        let s = scrub(t, &["zed".into()], &["Tulip-7781".into(), "abc".into()]);
        assert_eq!(s, "login <account> with <password>; character Zedra; zed_x; (<account>) said abc");
        assert_eq!(scrub(t, &[], &[]), t, "the control: nothing to take out");
    }

    /// The dialog's keys: Tab moves between the note and the name (a
    /// problem's), Delete deletes a crash report (and nothing else), text
    /// takes no control characters, and once sent only Enter and Escape do
    /// anything.
    #[test]
    fn the_dialogs_keys() {
        use native::{Action, Dialog, Field, Key, Kind, Status};
        let mut d = Dialog::new(Kind::Problem);
        d.shot_wanted = None;
        assert_eq!(d.key(Key::Text("it fell over".into())), Action::Stay);
        assert_eq!(d.key(Key::Tab), Action::Stay);
        assert_eq!(d.field, Field::Name);
        d.key(Key::Text("Zed\nra".into()));
        d.key(Key::Backspace);
        assert_eq!((d.note.as_str(), d.name.as_str()), ("it fell over", "Zedr"));
        assert_eq!(d.key(Key::Delete), Action::Stay, "the control: a problem report has nothing to delete");
        assert_eq!(d.key(Key::Enter), Action::Send);
        let mut c = Dialog::new(Kind::Crash(vec!["crash-20261023-010203.txt".into()]));
        c.key(Key::Tab);
        assert_eq!(c.field, Field::Note, "a crash report has no name");
        assert_eq!(c.key(Key::Delete), Action::Delete);
        assert_eq!(c.key(Key::Escape), Action::Close);
        c.status = Status::Sending;
        assert_eq!(c.key(Key::Escape), Action::Stay, "nothing while it is sent");
        c.status = Status::Sent(vec!["x".into()]);
        assert_eq!((c.key(Key::Delete), c.key(Key::Text("a".into())), c.key(Key::Enter)), (Action::Stay, Action::Stay, Action::Close));
    }

    /// The picture can be left out (F3 20261023-083108): Tab goes from the
    /// name to the picture's row when there is a picture, Space there turns
    /// it off and on (typing nothing), and a click on the row as drawn does
    /// the same; off, the report goes without it. The controls: the picture
    /// goes by default, and with no picture there is no row (Tab goes back
    /// to the note, nothing to click).
    #[test]
    fn the_picture_can_be_left_out() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        use native::{Action, Button, Dialog, Field, Key, Kind, draw_dialog};
        let vui = vui();
        let mut d = Dialog::new(Kind::Problem);
        d.shot_wanted = None;
        d.shot = Some(vec![1, 2, 3]);
        assert_eq!(d.shot_to_send(), Some(&[1u8, 2, 3][..]), "it goes by default");
        let name = d.name.clone();
        d.key(Key::Tab);
        d.key(Key::Tab);
        assert_eq!(d.field, Field::Picture);
        assert_eq!(d.key(Key::Text(" ".into())), Action::Stay);
        assert_eq!(d.shot_to_send(), None, "turned off");
        assert_eq!(d.name, name, "nothing typed into the name");
        d.key(Key::Text(" ".into()));
        assert!(d.shot_to_send().is_some(), "and on again");
        d.key(Key::Tab);
        assert_eq!(d.field, Field::Note, "round to the note");
        draw_dialog(&mut d, &vui, &mut NullRaster);
        let row = d.buttons.iter().find(|b| b.1 == Button::Picture).map(|b| b.0).expect("the row is a button");
        assert_eq!(d.click_at(mid(row)), Action::Stay);
        assert_eq!(d.shot_to_send(), None, "clicked off");
        let mut e = Dialog::new(Kind::Problem);
        e.shot_wanted = None;
        e.key(Key::Tab);
        e.key(Key::Tab);
        assert_eq!(e.field, Field::Note, "no picture: Tab goes from the name back to the note");
        draw_dialog(&mut e, &vui, &mut NullRaster);
        assert!(e.buttons.iter().all(|b| b.1 != Button::Picture), "no picture: no row");
    }

    /// The strings a raster was asked to draw.
    #[derive(Default)]
    struct Words(Vec<String>);

    impl vi::ui::Raster for Words {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16]) {
            self.0.push(String::from_utf16_lossy(s));
        }
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
            self.0.push(String::from_utf16_lossy(s));
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// A crash report waiting says how long ago the game stopped (its
    /// `when:`, `stamp`'s text read back) and, when it was another version,
    /// that it was an earlier one (F3 crash 20261023-225944: offered after
    /// an update's restart, it said "last time" with the hour its run had
    /// started, and its player wrote "no crash, just notified about
    /// update"). The controls: this version's own report names no version,
    /// a report with no header says no time, and a malformed stamp is none.
    #[test]
    fn a_crash_says_when_it_stopped_and_in_which_version() {
        use crate::pointer::testing::vui;
        use native::{Dialog, Kind, Stopped, ago, crash_words, draw_dialog, parse_stamp, stamp};
        use std::time::{Duration, UNIX_EPOCH};
        let at = |s: u64| UNIX_EPOCH + Duration::from_secs(s);
        // to text and back: a leap day, the end of February in a century's
        // year that is no leap year, the first second
        assert_eq!(stamp(at(951_782_400)), "20000229-000000");
        for s in [0, 951_782_400, 1_792_755_679, 4_107_542_399, 4_107_542_400] {
            assert_eq!(parse_stamp(&stamp(at(s))), Some(at(s)), "{}", stamp(at(s)));
        }
        assert_eq!(parse_stamp("20261023-114119"), Some(at(1_792_755_679)));
        for bad in ["20261023-1141", "2026102x-114119", "20261323-114119", "20261023 114119", ""] {
            assert_eq!(parse_stamp(bad), None, "{bad:?}");
        }
        let now = at(1_800_000_000);
        let before = |s: u64| now - Duration::from_secs(s);
        assert_eq!(ago(before(30), now), "less than a minute ago");
        assert_eq!(ago(before(60), now), "a minute ago");
        assert_eq!(ago(before(150), now), "2 minutes ago");
        assert_eq!(ago(before(3600), now), "an hour ago");
        assert_eq!(ago(before(11 * 3600 + 18 * 60), now), "11 hours ago");
        assert_eq!(ago(before(3 * 86400), now), "3 days ago");
        assert_eq!(ago(now + Duration::from_secs(5), now), "less than a minute ago", "a clock put back");
        // the report as it waited: written at 11:41 by e768e44, offered at
        // 22:59 by 3ff30da
        let dir = std::env::temp_dir().join(format!("eqoa-crash-words-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("crash-20261023-093148.txt");
        std::fs::write(&f, "EQOA native client crash report\nversion: dev-e768e44 (dev)\nwhen: 20261023-114119\nplatform: Windows x86_64\n\n--- crash ---\nA panic\n").unwrap();
        let mut d = Dialog::new(Kind::Crash(vec![f]));
        assert_eq!(d.stopped, [Stopped { at: Some(at(1_792_755_679)), version: Some("dev-e768e44".into()) }]);
        let offered = at(1_792_755_679 + 11 * 3600 + 18 * 60 + 25);
        let (title, intro) = crash_words(&d.stopped, "dev-3ff30da", offered);
        assert_eq!(title, "The game stopped 11 hours ago");
        assert_eq!(intro, "That was in an earlier version of the game (dev-e768e44). It wrote a crash report then: sending it helps it get fixed.");
        assert_eq!(crash_words(&d.stopped, "dev-e768e44", offered).1, "It wrote a crash report then: sending it helps it get fixed.", "the control: this version's own");
        // drawn: the dialog asks the clock, so its age is today's
        let mut w = Words::default();
        d.shot_wanted = None;
        draw_dialog(&mut d, &vui(), &mut w);
        assert!(w.0.iter().any(|l| l.starts_with("The game stopped ") && l.ends_with(" ago")), "{:?}", w.0);
        assert!(w.0.iter().all(|l| !l.contains("last time")), "{:?}", w.0);
        let none = Dialog::new(Kind::Crash(vec![dir.join("crash-missing.txt")]));
        assert_eq!(none.stopped, [Stopped::default()]);
        assert_eq!(crash_words(&none.stopped, "dev-3ff30da", offered), ("The game stopped earlier".to_string(), "It wrote a crash report then: sending it helps it get fixed.".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every crash report waiting is one offer (F3 crash 20261028-235209,
    /// "this window wont go away. Idk why. Everytime i log in this happens":
    /// the game had stopped several times that morning, and each start
    /// offered the newest of the reports kept, start after start): the offer
    /// holds them all, newest first, and says how many; Not now keeps them
    /// all and Delete deletes them all; Send sends each in turn, newest
    /// first, its own POST, and the one that fails is kept with those after
    /// it, which a Send again sends, and only those. The planted control: the
    /// oldest report, which an offer of the newest alone never reached, is
    /// gone after Delete (one report's offer, the old one, leaves it and the
    /// middle one). The controls: the file beside them that is no crash
    /// report stays through everything, a problem report's dialog deletes
    /// nothing, and a report waiting alone reads and asks as it did.
    #[test]
    fn every_crash_report_waiting_is_one_offer() {
        use crate::pointer::testing::vui;
        use native::{Action, Dialog, Kind, Status, act, crash_words, draw_dialog, offer_in, send_crashes};
        use std::cell::RefCell;
        use std::time::{Duration, UNIX_EPOCH};
        let dir = std::env::temp_dir().join(format!("eqoa-crash-offer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let plant = || {
            let crash = |run: &str, when: &str, version: &str| {
                let f = dir.join(format!("crash-{run}.txt"));
                std::fs::write(&f, format!("EQOA native client crash report\nversion: {version} (dev)\nwhen: {when}\nplatform: Windows x86_64\n\n--- crash ---\nA panic in {run}\n")).unwrap();
                f
            };
            (crash("20261028-005857", "20261028-005950", "dev-8e8987d"), crash("20261028-063000", "20261028-063205", "dev-8e8987d"), crash("20261028-120000", "20261028-120130", "dev-6504760"))
        };
        let other = dir.join("crash-shot.png");
        std::fs::write(&other, b"not a crash report").unwrap();
        let (a, b, c) = plant();
        let all = |f: &[&std::path::PathBuf]| f.iter().all(|f| f.exists());
        let d = offer_in(&dir).expect("an offer");
        assert_eq!(d.kind, Kind::Crash(vec![c.clone(), b.clone(), a.clone()]), "all three, newest first");
        // its words: how many, the last one's time, the versions not this one
        let at = |s: u64| UNIX_EPOCH + Duration::from_secs(s);
        let newest = d.stopped[0].at.expect("its time");
        let (title, intro) = crash_words(&d.stopped, "dev-8e8987d", newest + Duration::from_secs(2 * 3600));
        assert_eq!(title, "The game stopped 3 times, the last 2 hours ago");
        assert_eq!(intro, "One of them was in an earlier version of the game (dev-6504760). It wrote a crash report each time: sending them helps it get fixed.");
        let (_, intro) = crash_words(&d.stopped, "dev-b5ef31c", at(1_900_000_000));
        assert_eq!(intro, "Those were in earlier versions of the game (dev-6504760, dev-8e8987d). It wrote a crash report each time: sending them helps it get fixed.");
        assert_eq!(crash_words(&d.stopped[..2], "dev-6504760", newest).0, "The game stopped twice, the last less than a minute ago");
        let mut d = d;
        let mut w = Words::default();
        draw_dialog(&mut d, &vui(), &mut w);
        assert!(w.0.iter().any(|l| l.starts_with("The game stopped 3 times, the last ")), "{:?}", w.0);
        assert!(w.0.iter().any(|l| l == "Send all (Enter)") && w.0.iter().any(|l| l == "Delete all (Del)"), "{:?}", w.0);
        // Not now: all kept
        act(&mut Some(d), Action::Close);
        assert!(all(&[&a, &b, &c]), "Not now keeps them all");
        // Send, the server failing the second: the first gone, the rest kept
        let posted = RefCell::new(Vec::new());
        let failing = |route: &str, body: String| {
            assert_eq!(route, "crash");
            let mut p = posted.borrow_mut();
            p.push(["005857", "063000", "120000"].into_iter().find(|r| body.contains(&format!("A panic in 20261028-{r}"))).unwrap_or("?"));
            assert!(body.contains("note: it stopped as I logged in"), "the note with each");
            if p.len() == 2 { Err("the report server could not be reached (timed out)".to_string()) } else { Ok(format!("id{}", p.len())) }
        };
        let mut d = offer_in(&dir).unwrap();
        let Kind::Crash(files) = d.kind.clone() else { unreachable!() };
        let s = send_crashes(&files, "it stopped as I logged in", failing);
        assert_eq!(*posted.borrow(), ["120000", "063000"], "newest first, stopping at the failure");
        assert!(!c.exists() && all(&[&b, &a]), "the one sent deleted, the one that failed and the one after it kept");
        *d.result.lock().unwrap() = Some(s);
        d.poll();
        assert_eq!(d.kind, Kind::Crash(vec![b.clone(), a.clone()]), "what is left waits for a Send again");
        assert_eq!((d.stopped.len(), &d.status), (2, &Status::Failed(vec!["id1".into()], "the report server could not be reached (timed out)".into())));
        let mut w = Words::default();
        draw_dialog(&mut d, &vui(), &mut w);
        assert!(w.0.iter().any(|l| l.starts_with("Sent 1 as id1; 2 not sent: the report server")), "{:?}", w.0);
        // Send again: only those, newest first
        posted.borrow_mut().clear();
        let working = |_: &str, body: String| {
            let mut p = posted.borrow_mut();
            p.push(["005857", "063000", "120000"].into_iter().find(|r| body.contains(&format!("A panic in 20261028-{r}"))).unwrap_or("?"));
            Ok(format!("id{}", p.len() + 1))
        };
        let Kind::Crash(files) = d.kind.clone() else { unreachable!() };
        *d.result.lock().unwrap() = Some(send_crashes(&files, "", working));
        d.poll();
        assert_eq!(*posted.borrow(), ["063000", "005857"]);
        assert_eq!((&d.kind, &d.status), (&Kind::Crash(Vec::new()), &Status::Sent(vec!["id2".into(), "id3".into()])));
        assert!(!a.exists() && !b.exists() && other.exists(), "all sent and gone; the control kept");
        let mut w = Words::default();
        draw_dialog(&mut d, &vui(), &mut w);
        assert!(w.0.iter().any(|l| l.starts_with("Sent all 2 as id2 and id3.")), "{:?}", w.0);
        // Delete: every one, the planted oldest too
        let (a, b, c) = plant();
        let mut problem = Some(Dialog::new(Kind::Problem));
        act(&mut problem, Action::Delete);
        assert!(all(&[&a, &b, &c]), "the control: a problem report's dialog deletes nothing");
        act(&mut offer_in(&dir), Action::Delete);
        assert!(!c.exists() && !b.exists(), "the newest and the middle one deleted");
        assert!(!a.exists(), "the planted control: the oldest, which the offer of one never reached, deleted too");
        assert!(other.exists(), "the control: no crash report, not deleted");
        assert!(offer_in(&dir).is_none(), "nothing waits: nothing offered");
        // the control: one waiting alone reads and asks as before
        let one = dir.join("crash-20261029-011105.txt");
        std::fs::write(&one, "EQOA native client crash report\nversion: dev-6504760 (dev)\nwhen: 20261029-011105\n").unwrap();
        let mut d = offer_in(&dir).unwrap();
        assert_eq!(d.kind, Kind::Crash(vec![one.clone()]));
        let mut w = Words::default();
        draw_dialog(&mut d, &vui(), &mut w);
        assert!(w.0.iter().any(|l| l == "Send (Enter)") && w.0.iter().any(|l| l == "Delete (Del)") && !w.0.iter().any(|l| l.contains(" times")), "{:?}", w.0);
        assert_eq!(crash_words(&d.stopped, "dev-6504760", at(1_900_000_000)).1, "It wrote a crash report then: sending it helps it get fixed.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A terminal that reads nothing (a launcher's pipe never drained: a
    /// Steam Deck's game mode) does not stop the log: 20,000 lines, far past
    /// the terminal's queue, all reach it while the terminal's thread is stuck
    /// in its first write, and the panic hook is told to keep off it. The
    /// control: the stuck writer itself, written as the log's reader used to
    /// write (itself, in the loop), never returns.
    #[test]
    fn a_terminal_that_reads_nothing_does_not_stop_the_log() {
        use native::{TERMINAL_BEHIND, read_lines, terminal_thread};
        use std::io::Write;
        use std::sync::atomic::Ordering;
        use std::sync::mpsc;
        use std::time::Duration;
        /// A write that waits until the test lets it go (its sender dropped).
        struct Stuck(mpsc::Receiver<()>);
        impl Write for Stuck {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                let _ = self.0.recv();
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (hold, stuck) = mpsc::channel::<()>();
        let (queue, written) = terminal_thread(Stuck(stuck)).unwrap();
        let text: String = (0..20_000).map(|i| format!("line {i}\n")).collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut n = 0;
            let mut last = String::new();
            read_lines(std::io::Cursor::new(text.into_bytes()), Some(queue), |l| {
                n += 1;
                last = l.to_string();
            });
            let _ = tx.send((n, last));
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(20)), Ok((20_000, "line 19999".to_string())));
        assert!(TERMINAL_BEHIND.load(Ordering::Relaxed), "the panic hook keeps off a terminal that reads nothing");
        // let go: the terminal's thread ends and says so
        drop(hold);
        assert_eq!(written.recv_timeout(Duration::from_secs(5)), Ok(()));
        TERMINAL_BEHIND.store(false, Ordering::Relaxed);
        // the control: writing to it in the loop, as the reader did, stops there
        let (hold2, stuck2) = mpsc::channel::<()>();
        let (tx2, rx2) = mpsc::channel();
        std::thread::spawn(move || {
            let mut t = Stuck(stuck2);
            for i in 0..3 {
                let _ = t.write_all(format!("line {i}\n").as_bytes());
                let _ = tx2.send(i);
            }
        });
        assert_eq!(rx2.recv_timeout(Duration::from_millis(300)), Err(mpsc::RecvTimeoutError::Timeout));
        drop(hold2);
    }

    /// A report too long for the server is cut, at a character, saying so.
    #[test]
    fn a_long_report_is_cut_to_fit() {
        assert_eq!(fit("short", 100), "short");
        let long = "é".repeat(100);
        let f = fit(&long, 60);
        assert!(f.len() < 60 && f.ends_with("(cut to fit the report server)\n"));
    }
}
