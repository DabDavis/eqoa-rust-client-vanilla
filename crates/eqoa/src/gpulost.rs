//! The graphics device lost, natively: Windows' drivers lose it across a
//! sleep and wake, and a driver reset or a GPU taken away does too. wgpu
//! draws nothing on a lost device again and makes every buffer on it invalid,
//! and `create_buffer_init` panics on one (F3 crash 20261024-001640: a PC let
//! sleep while the game ran, woken, the connection gone, and the first model
//! the game's screens uploaded). No picture can come back on it, so the window
//! closes (`lost` stops the event loop) and the game starts again in its
//! place, at its boot, as the update's restart does
//! (`update::relaunch_if_requested`). A loss found in the middle of a frame
//! may still reach an upload first: the panic hook starts the game again
//! then (`logfile`). A page has its own: `web::start`'s
//! callback raises the crash notice.
//!
//! Only once a run has lasted `SETTLED`: a device lost again that soon after
//! a start ends the run, its log saying why, instead of starting over and over.
//! `EQOA_GPU_LOST_TEST=<ms>[,frame]`: the device destroyed and taken as lost
//! at that time on the client's clock; `frame` uploads a model after, in the
//! same frame, as the crash did.

use render::wgpu;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static LOST: AtomicBool = AtomicBool::new(false);
static WHY: Mutex<String> = Mutex::new(String::new());
/// The run's start and the program's path as it started (an update renames
/// the running program aside on Linux: `update`'s `Shared::exe`).
static START: Mutex<Option<(Instant, PathBuf)>> = Mutex::new(None);

const SETTLED: Duration = Duration::from_secs(30);

/// The run begins (main's `boot`, before the update may move the program).
pub fn begin() {
    if let Ok(exe) = std::env::current_exe()
        && let Ok(mut s) = START.lock()
    {
        *s = Some((Instant::now(), exe));
    }
}

/// The window's device, watched for a loss (`Destroyed` is the program's own
/// doing as it closes).
pub fn watch(device: &wgpu::Device) {
    device.set_device_lost_callback(|reason, message| {
        if !matches!(reason, wgpu::DeviceLostReason::Destroyed) {
            mark(&format!("{reason:?}: {message}"));
        }
    });
}

/// The device taken as lost, the first time said in the log.
pub fn mark(why: &str) {
    if let Ok(mut w) = WHY.lock()
        && w.is_empty()
    {
        *w = why.to_string();
    }
    if !LOST.swap(true, Ordering::SeqCst) {
        crate::say(&format!("graphics: the device was lost ({why}): the window closes and the game starts again"));
    }
}

/// Whether the device was lost: nothing more is drawn or uploaded.
pub fn lost() -> bool {
    LOST.load(Ordering::SeqCst)
}

/// Why, as the device said it.
pub fn why() -> String {
    WHY.lock().map(|w| w.clone()).unwrap_or_default()
}

/// Whether this run has lasted long enough to start the game again.
pub fn settled() -> bool {
    START.lock().ok().and_then(|s| s.as_ref().map(|(t, _)| t.elapsed() >= SETTLED)).unwrap_or(false)
}

/// The game started again in this one's place, with its arguments (on Linux
/// and macOS the same process): after the window closed for it (main's
/// `boot`), or from the panic hook. Returns when it did not start.
pub fn relaunch() {
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

/// `EQOA_GPU_LOST_TEST`, once its time has come: the device destroyed and
/// taken as lost (a destroyed device refuses buffers as a lost one does), and
/// with `frame` a model's vertices uploaded after it, as the crash's were.
pub fn test(device: &wgpu::Device, now: i64) {
    use wgpu::util::DeviceExt;
    static DONE: AtomicBool = AtomicBool::new(false);
    let Ok(spec) = std::env::var("EQOA_GPU_LOST_TEST") else { return };
    let (at, frame) = match spec.split_once(',') {
        Some((at, how)) => (at, how == "frame"),
        None => (spec.as_str(), false),
    };
    if at.parse::<i64>().is_ok_and(|at| now < at) || at.parse::<i64>().is_err() || DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    device.destroy();
    mark("EQOA_GPU_LOST_TEST");
    if frame {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: &[0; 64],
            usage: wgpu::BufferUsages::VERTEX,
        });
    }
}
