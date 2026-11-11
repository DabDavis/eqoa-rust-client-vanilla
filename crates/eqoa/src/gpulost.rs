//! The graphics device lost. Windows' drivers lose it across a sleep and
//! wake; a driver reset (a TDR), a GPU taken away, or the graphics memory run
//! out does too, and in a browser the GPU process restarting loses every
//! page's at once (Lonewulf's crash reports of 2026-11-11: three pages of one
//! browser, three characters, `DXGI_ERROR_DEVICE_REMOVED` together). wgpu draws
//! nothing on a lost device again and makes every buffer on it invalid.
//!
//! A loss is recovered from in place (`viewer`'s `App::recover`): a new
//! adapter and device, and every GPU object made again from what the client
//! keeps on the CPU (the renderer, the world's zones, the creatures, the HUD's
//! layer, the sky), the game session going on meanwhile, its frames drawing
//! nothing until the new device is in (a page's comes a frame or more later,
//! `web::remake`). The player's connection, the world and the windows are
//! never touched.
//!
//! When that cannot be done (no new device, or the new one lost again within
//! `REPEAT` of the last recovery, or the new one refusing what the client
//! made on it), natively the window closes and the game starts again in its
//! place, at its boot, as the update's restart does
//! (`update::relaunch_if_requested`), but only once a run has lasted
//! `SETTLED`, or the run ends with its log saying why; a page remembers
//! WebGL2 for its next load on that machine and says to reload
//! (`web::give_up`).
//!
//! `EQOA_GPU_LOST_TEST=<ms>[,frame][,again]` (a page's `?gpu_lost_test=`, and
//! `window.eqoaLoseDevice()`): the device destroyed and taken as lost at that
//! time on the client's clock; `frame` uploads a model after, in the same
//! frame, as the native crash of 20261024-001640 did; `again` loses the new
//! device too, 5 s after it came (the fallback).

use render::wgpu;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use web_time::{Duration, Instant};

static LOST: AtomicBool = AtomicBool::new(false);
static WHY: Mutex<String> = Mutex::new(String::new());
/// The device the client draws on now, counted from 1: a lost callback of a
/// device let go before (its `Destroyed` as it is dropped, or a late one) is
/// not this one's.
static DEVICE: AtomicU64 = AtomicU64::new(0);
/// When the last recovery made its device, for `again_soon`.
static RECOVERED: Mutex<Option<Instant>> = Mutex::new(None);
/// How many recoveries this run.
static RECOVERIES: AtomicU64 = AtomicU64::new(0);
/// What a recovered device refused (the first), if it refused anything: an
/// object of the lost device's left in use, so its frames would draw
/// nothing; the recovery has failed.
static REFUSED: Mutex<Option<String>> = Mutex::new(None);

/// A device lost again this soon after a recovery is not recovered from
/// again: the graphics card (or the browser's graphics process) would only
/// lose it again.
pub const REPEAT: Duration = Duration::from_secs(60);

/// The window's device, watched for a loss (`Destroyed` is the program's own
/// doing as it closes, or a device let go after a recovery). `recovered`: the
/// device is a recovery's, and anything it refuses is taken as the
/// recovery failed (`refused`) instead of the panic wgpu raises by default.
pub fn watch(device: &wgpu::Device, recovered: bool) {
    let me = DEVICE.fetch_add(1, Ordering::SeqCst) + 1;
    device.set_device_lost_callback(move |reason, message| {
        if !matches!(reason, wgpu::DeviceLostReason::Destroyed) && DEVICE.load(Ordering::SeqCst) == me {
            mark(&format!("{reason:?}: {message}"));
        }
    });
    if recovered {
        device.on_uncaptured_error(std::sync::Arc::new(move |e: wgpu::Error| {
            if DEVICE.load(Ordering::SeqCst) != me || lost() {
                return;
            }
            let text = e.to_string();
            let line = text.lines().take(3).collect::<Vec<_>>().join(" ");
            if let Ok(mut r) = REFUSED.lock()
                && r.is_none()
            {
                crate::say(&format!("graphics: the new device refused something: {line}"));
                *r = Some(line);
            }
        }));
    }
}

/// The device taken as lost, said in the log once a loss.
pub fn mark(why: &str) {
    if let Ok(mut w) = WHY.lock()
        && w.is_empty()
    {
        *w = why.to_string();
    }
    if !LOST.swap(true, Ordering::SeqCst) {
        crate::say(&format!("graphics: the device was lost ({why}): a new one is made, the game going on"));
    }
}

/// Whether the device is lost (and not yet made again): nothing more is
/// drawn or uploaded on it.
pub fn lost() -> bool {
    LOST.load(Ordering::SeqCst)
}

/// Why, as the device said it.
pub fn why() -> String {
    WHY.lock().map(|w| w.clone()).unwrap_or_default()
}

/// Whether a loss now comes too soon after the last recovery to recover
/// from again (`REPEAT`).
pub fn again_soon() -> bool {
    RECOVERED.lock().ok().and_then(|r| *r).is_some_and(|t| t.elapsed() < REPEAT)
}

/// A new device is in place of the lost one: the loss is over, its time kept
/// for `again_soon`.
pub fn recovered() {
    let n = RECOVERIES.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut r) = RECOVERED.lock() {
        *r = Some(Instant::now());
    }
    if let Ok(mut r) = REFUSED.lock() {
        *r = None;
    }
    crate::say(&format!("graphics: a new device in place of the lost one ({}), the game went on (recovery {n} this run)", why()));
    if let Ok(mut w) = WHY.lock() {
        w.clear();
    }
    LOST.store(false, Ordering::SeqCst);
}

/// How many times this run recovered from a lost device.
pub fn recoveries() -> u64 {
    RECOVERIES.load(Ordering::SeqCst)
}

/// What a recovered device refused, if it did: the recovery failed.
pub fn refused() -> Option<String> {
    REFUSED.lock().ok().and_then(|r| r.clone())
}

#[cfg(not(target_arch = "wasm32"))]
mod restart {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// The run's start and the program's path as it started (an update
    /// renames the running program aside on Linux: `update`'s `Shared::exe`).
    static START: Mutex<Option<(Instant, PathBuf)>> = Mutex::new(None);
    /// The window closes for a start again (a recovery that could not be had).
    pub static GIVE_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    const SETTLED: Duration = Duration::from_secs(30);

    /// The run begins (main's `boot`, before the update may move the program).
    pub fn begin() {
        if let Ok(exe) = std::env::current_exe()
            && let Ok(mut s) = START.lock()
        {
            *s = Some((Instant::now(), exe));
        }
    }

    /// Whether this run has lasted long enough to start the game again.
    pub fn settled() -> bool {
        START.lock().ok().and_then(|s| s.as_ref().map(|(t, _)| t.elapsed() >= SETTLED)).unwrap_or(false)
    }

    /// The game started again in this one's place, with its arguments (on
    /// Linux and macOS the same process): after the window closed for it
    /// (main's `boot`), or from the panic hook. Returns when it did not start.
    pub fn relaunch() {
        // an Android app is the system's process, started by its launcher:
        // the run ends there instead (`android`)
        if cfg!(target_os = "android") {
            return;
        }
        let Some(exe) = START.lock().ok().and_then(|s| s.as_ref().map(|(_, e)| e.clone())) else { return };
        crate::multibox::leave();
        let mut c = std::process::Command::new(&exe);
        c.args(std::env::args_os().skip(1)).env_remove("EQOA_GPU_LOST_TEST");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let e = c.exec();
            crate::say(&format!("graphics: {} did not start again: {e}", exe.display()));
        }
        #[cfg(not(unix))]
        if let Err(e) = c.spawn() {
            crate::say(&format!("graphics: {} did not start again: {e}", exe.display()));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use restart::{begin, relaunch, settled};

/// Natively: the recovery could not be had (`why`), so the window closes for
/// the game to start again (main's `boot`).
#[cfg(not(target_arch = "wasm32"))]
pub fn give_up(why: &str) {
    crate::say(&format!("graphics: {why}: the window closes and the game starts again"));
    restart::GIVE_UP.store(true, Ordering::SeqCst);
}

/// Whether the window is closing for a start again (`give_up`).
#[cfg(not(target_arch = "wasm32"))]
pub fn gave_up() -> bool {
    restart::GIVE_UP.load(Ordering::SeqCst)
}

/// The test's spec (`EQOA_GPU_LOST_TEST`, a page's `?gpu_lost_test=`): when,
/// whether an upload follows in the same frame, whether the next device is
/// lost too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestSpec {
    pub at: i64,
    pub frame: bool,
    pub again: bool,
}

impl TestSpec {
    pub fn parse(s: &str) -> Option<TestSpec> {
        let mut parts = s.split(',');
        let at = parts.next()?.trim().parse().ok()?;
        let (mut frame, mut again) = (false, false);
        for p in parts {
            match p.trim() {
                "frame" => frame = true,
                "again" => again = true,
                _ => return None,
            }
        }
        Some(TestSpec { at, frame, again })
    }
}

fn test_spec() -> Option<TestSpec> {
    #[cfg(target_arch = "wasm32")]
    let s = crate::web::page_value("gpu_lost_test");
    #[cfg(not(target_arch = "wasm32"))]
    let s = std::env::var("EQOA_GPU_LOST_TEST").ok();
    s.as_deref().and_then(TestSpec::parse)
}

/// A loss asked for now (a page's `window.eqoaLoseDevice()`, `ask_loss`).
static ASKED: AtomicBool = AtomicBool::new(false);

/// The device lost at the next frame, as a test's (`window.eqoaLoseDevice`).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn ask_loss() {
    ASKED.store(true, Ordering::SeqCst);
}

/// The test's loss, once its time has come (`EQOA_GPU_LOST_TEST`, or
/// `ask_loss`): the device destroyed and taken as lost (a destroyed device
/// refuses buffers as a lost one does), and with `frame` a model's vertices
/// uploaded after it, as the crash's were. With `again`, the first recovered
/// device is lost too, 5 s after it came.
pub fn test(device: &wgpu::Device, now: i64) {
    use render::BufferInit;
    static DONE: AtomicBool = AtomicBool::new(false);
    static AGAIN: AtomicBool = AtomicBool::new(false);
    let asked = ASKED.swap(false, Ordering::SeqCst);
    let spec = test_spec();
    let due = match spec {
        _ if asked => true,
        Some(s) if now >= s.at && !DONE.load(Ordering::SeqCst) => {
            DONE.store(true, Ordering::SeqCst);
            true
        }
        Some(s) if s.again && recoveries() == 1 && !AGAIN.load(Ordering::SeqCst) && RECOVERED.lock().ok().and_then(|r| *r).is_some_and(|t| t.elapsed() >= Duration::from_secs(5)) => {
            AGAIN.store(true, Ordering::SeqCst);
            true
        }
        _ => false,
    };
    if !due || lost() {
        return;
    }
    device.destroy();
    mark("a test's");
    if spec.is_some_and(|s| s.frame) {
        device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: &[0; 64],
            usage: wgpu::BufferUsages::VERTEX,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A loss is taken, recovered from, and a loss soon after is too soon
    /// (`again_soon`, the fallback); before any recovery none is too soon (the
    /// control). The only test that touches the statics.
    #[test]
    fn a_loss_soon_after_a_recovery_falls_back() {
        assert!(!lost() && !again_soon(), "nothing lost, nothing recovered yet");
        mark("one");
        assert!(lost() && why() == "one");
        mark("two");
        assert_eq!(why(), "one", "the first reason is kept");
        assert!(!again_soon(), "the control: the first loss is recovered from");
        recovered();
        assert!(!lost() && why().is_empty() && recoveries() == 1);
        mark("three");
        assert!(lost() && again_soon(), "lost again within REPEAT: no second recovery");
        *RECOVERED.lock().unwrap() = Instant::now().checked_sub(REPEAT + Duration::from_secs(1));
        assert!(!again_soon(), "past REPEAT, recovered from again");
    }

    #[test]
    fn test_specs() {
        assert_eq!(TestSpec::parse("4000"), Some(TestSpec { at: 4000, frame: false, again: false }));
        assert_eq!(TestSpec::parse("4000,frame"), Some(TestSpec { at: 4000, frame: true, again: false }));
        assert_eq!(TestSpec::parse("4000,again,frame"), Some(TestSpec { at: 4000, frame: true, again: true }));
        // the control: what is not a spec is none
        assert_eq!(TestSpec::parse("soon"), None);
        assert_eq!(TestSpec::parse("4000,twice"), None);
    }
}
