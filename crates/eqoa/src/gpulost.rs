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
    device.on_uncaptured_error(std::sync::Arc::new(move |e: wgpu::Error| {
        if DEVICE.load(Ordering::SeqCst) != me || lost() {
            return;
        }
        // out of graphics memory (Hoofbite's Iris Xe in Freeport, three
        // crashes of 2026-11-17, `wgpu error: Out of Memory` in
        // `create_texture`): taken as the device lost, made again with a
        // texture pack's budget halved (`render::out_of_memory`), never
        // wgpu's default panic
        if let wgpu::Error::OutOfMemory { .. } = e {
            let n = render::out_of_memory();
            mark(&format!("out of graphics memory ({n} times this run; a texture pack's budget now {} MB)", render::pack_limits().budget >> 20));
            return;
        }
        // the window's surface refusing a call (Windows took it away or
        // changed it: a minimised window, a display gone to sleep): the
        // window's, not the device's, made again from the window by the
        // next frame (`surface`), never a panic (the crash reports of
        // 2026-11-19/21, "In Surface::configure  Invalid surface")
        if let wgpu::Error::Validation { description, .. } = &e
            && is_surface_error(description)
        {
            surface_failed(description);
            return;
        }
        if recovered {
            let text = e.to_string();
            let line = text.lines().take(3).collect::<Vec<_>>().join(" ");
            if let Ok(mut r) = REFUSED.lock()
                && r.is_none()
            {
                crate::say(&format!("graphics: the new device refused something: {line}"));
                *r = Some(line);
            }
            return;
        }
        // anything else as wgpu's default handler takes it
        panic!("wgpu error: {e}\n");
    }));
}

/// A surface call's validation error that reached the device's handler (one
/// made outside the error scopes `viewer`'s `Gfx` puts round its own).
static SURFACE_FAILED: AtomicBool = AtomicBool::new(false);

/// Whether a validation error's words are a surface call's: wgpu names the
/// call it came from ("In Surface::configure", "In
/// Surface::get_current_texture_view", "In Surface::present").
pub fn is_surface_error(description: &str) -> bool {
    description.contains("In Surface::") || description.contains("Error in Surface::")
}

/// The window's surface refused a call (`is_surface_error`): taken by the
/// next frame (`take_surface_failed`), said in the log once until then.
pub fn surface_failed(why: &str) {
    if !SURFACE_FAILED.swap(true, Ordering::SeqCst) {
        let line = why.split_whitespace().collect::<Vec<_>>().join(" ");
        crate::say(&format!("graphics: the window's surface refused a call ({line}): it is made again"));
    }
}

/// Whether the window's surface refused a call since the last frame asked
/// (`surface_failed`).
pub fn take_surface_failed() -> bool {
    SURFACE_FAILED.swap(false, Ordering::SeqCst)
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

/// Why the device is taken as lost when the browser threw from a WebGPU call
/// on it (`what`: the call and its error, web/gpuguard.js): Safari throws an
/// InvalidStateError from a lost device's calls (petitions 5074, 5076, 5105,
/// 5106) where the spec gives invalid objects.
pub fn threw_why(what: &str) -> String {
    let what: String = what.lines().next().unwrap_or("").chars().take(200).collect();
    format!("the browser refused the device's calls, its graphics process gone ({what})")
}

/// Whether the browser's throw (`what`, web/gpuguard.js's words: the call and
/// its error) was createBuffer's RangeError, a buffer mapped at its making
/// that the browser found no memory for (Chrome's "createBuffer failed, size
/// (204064) is too large for the implementation when mappedAtCreation ==
/// true": Threnody's and Dust's crash reports, 2026-11-16/17): the graphics
/// memory run out, not the graphics process gone.
pub fn threw_out_of_memory(what: &str) -> bool {
    what.contains("createBuffer") && what.contains("RangeError")
}

/// The browser threw from a WebGPU call (web/gpuguard.js, `what` its words):
/// the device taken as lost; out of memory (`threw_out_of_memory`) a texture
/// pack's budget halved too (`render::out_of_memory`), as wgpu's own
/// out-of-memory error is taken (`watch`).
pub fn browser_threw_call(what: &str) {
    if threw_out_of_memory(what) {
        let n = render::out_of_memory();
        let what: String = what.lines().next().unwrap_or("").chars().take(200).collect();
        mark(&format!("out of graphics memory, the browser refused a buffer ({what}; {n} times this run; a texture pack's budget now {} MB)", render::pack_limits().budget >> 20));
    } else {
        mark(&threw_why(what));
    }
}

/// Whether a panic's text is wgpu's WebGPU backend unwrapping a call the
/// browser threw from with an InvalidStateError: a lost device's
/// (`threw_why`), where web/gpuguard.js did not stand in for it.
pub fn browser_threw(panic: &str) -> bool {
    panic.contains("backend/webgpu.rs") && (panic.contains("InvalidStateError") || (panic.contains("RangeError") && panic.contains("createBuffer")))
}

/// The page's notice for such a panic: the browser's words (the notice says
/// to reload).
pub fn threw_detail(panic: &str) -> String {
    let at = panic.find("InvalidStateError").or_else(|| panic.find("RangeError")).unwrap_or(0);
    let words: String = panic[at..].lines().next().unwrap_or("").chars().take(160).collect();
    let words = words.split(" @").next().unwrap_or("").trim().to_string();
    format!("the browser's graphics stopped answering ({words}): its graphics process ended, as a browser's does when it runs short of memory")
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

/// A page's way to a new device in place of a lost one (`web::remake`): the
/// backends it tries in order, each with the pauses before its tries. On
/// WebGPU first WebGPU again, for about 50 s (a Windows driver reset, a TDR,
/// can take longer than the 16 s this waited before: Roach's crash report of
/// 2026-11-13, `requestDevice` refused with `DXGI_ERROR_DEVICE_REMOVED` on
/// every try), each try a fresh adapter; then WebGL2, on a fresh canvas (one
/// that has had a "webgpu" context never gives a "webgl2" one), which a
/// Chrome whose GPU process keeps failing still draws on (in software if it
/// must); refused, WebGL2 on that canvas for 2 minutes more. On WebGL2
/// (chosen, or fallen back to: never back to WebGPU in that
/// page) WebGL2 again on its canvas once the browser gives the context back.
pub mod page {
    use render::wgpu::Backend;

    /// WebGPU's pauses: rising to 5 s, then 5 s apart, 50.85 s in all.
    pub const WEBGPU_PAUSES_MS: [i32; 15] = [100, 250, 500, 1000, 2000, 3000, 4000, 5000, 5000, 5000, 5000, 5000, 5000, 5000, 5000];
    /// WebGL2's on its own canvas, its context given back (as before: 15.85 s).
    pub const WEBGL_PAUSES_MS: [i32; 8] = [100, 250, 500, 1000, 2000, 3000, 4000, 5000];
    /// WebGL2's on a fresh canvas after WebGPU's 50 s: the browser has had
    /// its time; a few tries, 7.25 s.
    pub const FALLBACK_PAUSES_MS: [i32; 4] = [0, 250, 2000, 5000];
    /// Both refused (2026-11-21's crash report: Chrome after a
    /// `DXGI_ERROR_DEVICE_HUNG`, no WebGPU adapter for 50 s, then
    /// `getContext("webgl2")` null on the fresh canvas too: a browser blocks
    /// a page from the graphics card for a while after it hung): WebGL2 again
    /// on the fresh canvas, every 10 s for 2 minutes more, the game going on
    /// meanwhile, before the page stops. WebGL2 alone: `gfx_on` makes its
    /// surface before it asks for an adapter, so a WebGPU try would take the
    /// fresh canvas's "webgpu" context, and no "webgl2" one comes after it.
    pub const BLOCKED_PAUSES_MS: [i32; 12] = [10_000; 12];

    /// One leg of the way: the backends tried, in order, at each try, the
    /// pauses before each try, and whether it is on a fresh canvas in place
    /// of the lost device's (made by the first leg that wants it, kept by
    /// the legs after).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Leg {
        pub backends: &'static [Backend],
        pub pauses_ms: &'static [i32],
        pub fresh_canvas: bool,
    }

    /// The legs, in order, for a device lost on `drawn`.
    pub fn plan(drawn: Backend) -> Vec<Leg> {
        if drawn == Backend::Gl {
            return vec![Leg { backends: &[Backend::Gl], pauses_ms: &WEBGL_PAUSES_MS, fresh_canvas: false }];
        }
        vec![
            Leg { backends: &[Backend::BrowserWebGpu], pauses_ms: &WEBGPU_PAUSES_MS, fresh_canvas: false },
            Leg { backends: &[Backend::Gl], pauses_ms: &FALLBACK_PAUSES_MS, fresh_canvas: true },
            Leg { backends: &[Backend::Gl], pauses_ms: &BLOCKED_PAUSES_MS, fresh_canvas: true },
        ]
    }

    /// A leg's whole wait, in ms.
    pub fn total_ms(pauses: &[i32]) -> i32 {
        pauses.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use render::wgpu::Backend;

    /// WebGPU is tried again for 45-60 s, no pause over 5 s and none
    /// shorter than the one before; then WebGL2 on a fresh canvas; then,
    /// refused, WebGL2 on that canvas for 2 minutes more (never WebGPU there:
    /// its surface would take the canvas from WebGL2). Only the first leg
    /// is on the lost device's canvas, and it asks for WebGPU alone (that
    /// canvas gives no "webgl2" context). On WebGL2 only WebGL2, on its own
    /// canvas: a page fallen back never goes back to WebGPU (the backend it
    /// draws on is WebGL2's from then). The control: the old schedule's 16 s,
    /// too short for Roach's driver reset.
    #[test]
    fn page_remake_plan() {
        let p = page::plan(Backend::BrowserWebGpu);
        assert_eq!(
            p.iter().map(|l| (l.backends, l.fresh_canvas)).collect::<Vec<_>>(),
            [(&[Backend::BrowserWebGpu][..], false), (&[Backend::Gl][..], true), (&[Backend::Gl][..], true)]
        );
        assert!(p.iter().all(|l| l.fresh_canvas || !l.backends.contains(&Backend::Gl)), "no WebGL2 asked of WebGPU's canvas");
        assert!((100_000..=180_000).contains(&page::total_ms(p[2].pauses_ms)), "both refused: 2 minutes more");
        let w = p[0].pauses_ms;
        let total = page::total_ms(w);
        assert!((45_000..=60_000).contains(&total), "WebGPU waits {total} ms");
        assert!(w.iter().all(|&ms| (0..=5000).contains(&ms)), "no pause over 5 s");
        assert!(w.windows(2).all(|d| d[0] <= d[1]), "the pauses never shorten");
        assert!(w[0] <= 250, "the first try comes at once, as a lone GPU-process restart wants");
        let g = page::plan(Backend::Gl);
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].backends, g[0].fresh_canvas), (&[Backend::Gl][..], false), "no flip back to WebGPU");
        assert!(g[0].pauses_ms.iter().all(|&ms| ms <= 5000));
        assert!(p[1].pauses_ms.iter().all(|&ms| ms <= 5000) && page::total_ms(p[1].pauses_ms) < 15_000, "the fallback is quick once WebGPU had its 50 s");
        assert!(page::total_ms(&page::WEBGL_PAUSES_MS) < 45_000, "the control: the old 16 s schedule is short of a TDR");
    }

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

    /// The crash reports' panics (petitions 5076 and 5105, Safari's words)
    /// are the browser refusing a lost device's calls; the control: another
    /// wgpu panic, and an InvalidStateError not wgpu's, are not.
    #[test]
    fn a_browser_refusal_is_the_graphics_stopping() {
        let pass = "panicked at src/index.crates.io-1949cf8c6b5b557f/wgpu-30.0.1/src/backend/webgpu.rs:3334:70:\ncalled `Result::unwrap()` on an `Err` value: JsValue(InvalidStateError: GPUCommandEncoder.beginRenderPass: Unable to begin render pass. beginRenderPass@[native code] @https://eqoaclassic.com/eqoa/play/v/76e02bcff137/eqoa.js:524:45";
        let buffer = "panicked at src/index.crates.io-1949cf8c6b5b557f/wgpu-30.0.1/src/backend/webgpu.rs:2461:63:\ncalled `Result::unwrap()` on an `Err` value: JsValue(InvalidStateError: GPUDevice.createBuffer: Unable to create buffer. createBuffer@[native code]";
        assert!(browser_threw(pass) && browser_threw(buffer));
        let d = threw_detail(pass);
        assert!(d.starts_with("the browser's graphics stopped answering (InvalidStateError: GPUCommandEncoder.beginRenderPass: Unable to begin render pass. beginRenderPass@[native code])"), "{d}");
        assert!(d.ends_with("runs short of memory"));
        assert!(!browser_threw("panicked at src/backend/webgpu.rs:2015:90: called `Result::unwrap()` on an `Err` value: wgsl"), "the control: wgpu's own panic");
        assert!(!browser_threw("panicked at crates/eqoa/src/web/mod.rs:1:1: InvalidStateError"), "the control: not wgpu's");
        let w = threw_why("GPUDevice.createBuffer: Unable to create buffer.\nmore");
        assert_eq!(w, "the browser refused the device's calls, its graphics process gone (GPUDevice.createBuffer: Unable to create buffer.)");
        // Threnody's and Dust's (2026-11-16/17): Chrome's RangeError from a
        // buffer mapped at its making, short of memory: the same notice; and
        // gpuguard.js's words for it are out of memory, Safari's not
        let range = "panicked at src/index.crates.io-1949cf8c6b5b557f/wgpu-30.0.1/src/backend/webgpu.rs:2461:63:\ncalled `Result::unwrap()` on an `Err` value: JsValue(RangeError: createBuffer failed, size (204064) is too large for the implementation when mappedAtCreation == true\nRangeError: createBuffer failed";
        assert!(browser_threw(range));
        assert!(threw_detail(range).starts_with("the browser's graphics stopped answering (RangeError: createBuffer failed, size (204064)"), "{}", threw_detail(range));
        assert!(threw_out_of_memory("GPUDevice.createBuffer: RangeError: createBuffer failed, size (204064) is too large for the implementation when mappedAtCreation == true"));
        assert!(threw_out_of_memory("GPUDevice.createBuffer: RangeError: Failed to execute 'createBuffer' on 'GPUDevice': createBuffer failed, size (1099511627776) is too large for the implementation when mappedAtCreation == true"), "Chrome's words in the page test");
        assert!(!threw_out_of_memory("GPUDevice.createBuffer: Unable to create buffer."), "the control: Safari's lost device");
        assert!(!threw_out_of_memory("GPUQueue.writeTexture: RangeError: offset"), "the control: not createBuffer's");
    }

    /// The NVIDIA crash report's words (2026-11-21, "In Surface::configure
    /// Invalid surface") are the window's surface's, as an acquire's are;
    /// the controls: a buffer's or a pipeline's validation error is not.
    #[test]
    fn surface_errors_are_the_windows() {
        assert!(is_surface_error("Validation Error\n\nCaused by:\n  In Surface::configure\n    Invalid surface\n"));
        assert!(is_surface_error("Validation Error\n\nCaused by:\n  In Surface::get_current_texture_view\n    Surface is not configured for presentation\n"));
        assert!(!is_surface_error("Validation Error\n\nCaused by:\n  In Device::create_buffer, label = 'vertices'\n    Buffer size 0 is invalid\n"), "the control");
        assert!(!is_surface_error("In a render pass\n  Surface texture used after present"), "the control: a pass's words, not a surface call's");
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
