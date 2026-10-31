//! The program's log, natively (in a page the browser's console is the log):
//! what the program writes to stderr goes through a pipe (`start`) to the
//! terminal it had and to `eqoa.log` in the config directory (the run before
//! kept as `eqoa.prev.log`; a second window open beside the first,
//! multiboxing, keeps `eqoa-2.log`, and so on). A panic's message and
//! backtrace go there too.

use std::fs::File;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `eqoa.log` stops growing here.
const LOG_FILE_MAX: u64 = 32 << 20;
/// The line a run that closes writes last.
const CLOSED: &str = "[eqoa] closed";
/// The lines waiting for the terminal's thread (`tee`), at most: past them a
/// line reaches the log only.
const TERMINAL_QUEUE: usize = 1024;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Log {
    file: Option<File>,
    bytes: u64,
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

static LOG: Mutex<Log> = Mutex::new(Log { file: None, bytes: 0, orig: None, drained: None });
/// Where stderr went before the pipe, to write to (a descriptor, a
/// handle), -1 without one: what must reach it before the program may end.
static TERMINAL: AtomicIsize = AtomicIsize::new(-1);
/// Whether the terminal's thread is behind: its queue full, so a write
/// to where stderr went blocks (a pipe nothing reads) and the panic
/// hook writes there no more (`to_terminal`).
static TERMINAL_BEHIND: AtomicBool = AtomicBool::new(false);
/// This window, from 1 (`multibox`'s place): the files of its own.
static WINDOW: AtomicUsize = AtomicUsize::new(1);

/// One of this window's files: `eqoa.log` the first window's,
/// `eqoa-2.log` the second's.
fn own(base: &str, ext: &str) -> String {
    match WINDOW.load(Ordering::Relaxed) {
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
}

/// The program's log for the game as players start it (main's `boot`):
/// stderr through a pipe to where it went and to `eqoa.log`; the panic hook.
pub fn start() {
    let dir = crate::settings::config_dir();
    // a window open beside others (multiboxing) has files of its own:
    // its place's, taken now (`multibox`)
    let window = crate::multibox::place() + 1;
    WINDOW.store(window, Ordering::Relaxed);
    let stamp = stamp(SystemTime::now());
    let run = if window == 1 { stamp } else { format!("{stamp}-w{window}") };
    if let Some(d) = &dir {
        let _ = std::fs::create_dir_all(d);
        let (log, prev) = (d.join(own("eqoa", "log")), d.join(own("eqoa", "prev.log")));
        if log.exists() {
            let _ = std::fs::rename(&log, &prev);
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
        // one `caught` steps over: nothing in the log, nothing on the terminal
        if CAUGHT.with(|c| c.get()) > 0 {
            return;
        }
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        // after the graphics device was lost (an upload in the frame that
        // lost it): the game starts again in this one's place (`gpulost`)
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
        let text = format!("thread '{thread}' {info}");
        if let Ok(mut l) = LOG.try_lock() {
            for line in text.lines().chain(std::iter::once("stack backtrace:")) {
                push_line(&mut l, line);
            }
            for line in bt.to_string().lines() {
                push_line(&mut l, line);
            }
            if let Some(f) = l.file.as_mut() {
                let _ = f.flush();
            }
        }
        let shown = std::env::var("RUST_BACKTRACE").is_ok_and(|v| v != "0");
        to_terminal(&if shown { format!("{text}\nstack backtrace:\n{bt}\n") } else { format!("{text}\n") });
    }));
}

thread_local! {
    /// How deep this thread is in `caught`: a panic there is stepped over.
    static CAUGHT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// `f` with a panic in it caught and handed back as its message, said
/// nowhere: for a library's own fault the game can step over (gilrs's
/// lookup of a controller it holds no state for yet, `input.rs`).
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
fn terminal_thread<W: Write + Send + 'static>(mut to: W) -> Option<(mpsc::SyncSender<Vec<u8>>, mpsc::Receiver<()>)> {
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

/// The log's reader: every line of `reader` to `sink` (the log) as it
/// comes, and queued for the terminal's thread, never waited
/// for: while its queue is full a line goes to `sink` only, and
/// `TERMINAL_BEHIND` says so (the panic hook's message too). Until the
/// pipe's end closes.
fn read_lines(mut r: impl BufRead, mut terminal: Option<mpsc::SyncSender<Vec<u8>>>, mut sink: impl FnMut(&str)) {
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

/// The run closes: stderr to where it went, what the pipe holds read into
/// the log (half a second at most), and the log's last line says so.
pub fn close() {
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

fn version() -> String {
    crate::update::COMMIT.map_or_else(|| "local".to_string(), |c| format!("dev-{}", &c[..c.len().min(7)]))
}

fn channel() -> &'static str {
    crate::update::CHANNEL.unwrap_or("local")
}

fn stamp(t: SystemTime) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `caught` hands a panic back as its message (gilrs's index panic is a
    /// formatted String, a literal a &str) and a value through as it is, its
    /// depth back to none after each, a nested one's too; the control: the
    /// same panic outside `caught` unwinds to `catch_unwind`, uncaught by it.
    #[test]
    fn a_caught_panic_comes_back_as_its_message() {
        let v: Vec<u8> = vec![1, 2];
        let i = v.len();
        assert_eq!(caught(|| v[i]), Err("index out of bounds: the len is 2 but the index is 2".to_string()));
        assert_eq!(caught(|| -> u8 { panic!("a literal") }), Err("a literal".to_string()));
        assert_eq!(caught(|| 7), Ok(7));
        assert_eq!(caught(|| caught(|| -> u8 { panic!("inner") }).map_err(|e| e + " (nested)")), Ok(Err("inner (nested)".to_string())));
        assert!(std::panic::catch_unwind(|| v[i]).is_err(), "the control: it panics");
        assert_eq!(caught(|| v[0]), Ok(1), "and after all that, a plain call");
    }
/// A terminal that reads nothing (a launcher's pipe never drained: a
/// Steam Deck's game mode) does not stop the log: 20,000 lines, far past
/// the terminal's queue, all reach it while the terminal's thread is stuck
/// in its first write, and the panic hook is told to keep off it. The
/// control: the stuck writer itself, written as the log's reader used to
/// write (itself, in the loop), never returns.
#[test]
fn a_terminal_that_reads_nothing_does_not_stop_the_log() {
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
}
