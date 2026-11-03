//! Android: the game in an activity (`android/`, the APK).
//!
//! Android loads the game as a library (`libeqoa_android.so`, main.rs built
//! as a library: `cargo build --example eqoa-android --features android`)
//! and calls [`android_main`] on a thread of its own. The activity,
//! `android/java/.../EqoaActivity.java`, is the platform's NativeActivity
//! with a little more: it owns the window winit draws in, and hands the game
//! everything else a desktop's system would, over JNI (`Java_..._native*`
//! below):
//!
//! - every touch, key and controller event: it takes the window's input
//!   queue from the native side (`takeInputQueue(null)`), because winit
//!   turns every motion event into a touch, a controller's sticks included,
//!   and has no way to a phone's own keyboard. Touches and keys come here as
//!   [`Event`]s, which the window's loop takes (`viewer`'s `android_input`)
//!   into the same paths winit's would go (`App::finger`, `App::key_input`);
//!   a controller's state is kept here, and read as the other platforms'
//!   controllers are (`input::Gamepads`, [`pads`]);
//! - the device's keyboard for the game's edit boxes ([`want_text`]), its
//!   characters as keys;
//! - the disc image, picked in Android's own file picker and handed over as
//!   an open file (`/proc/self/fd/<n>`, `iso::open_file`); the activity
//!   keeps the pick and opens it again at the next start (`$EQOA_ISO`);
//! - where the game keeps its files: the app's own folders (`$EQOA_CONFIG`,
//!   `$EQOA_CACHE`), set before the game starts;
//! - the log: what the game writes to stderr goes to logcat too
//!   ([`Logcat`]), tag `eqoa`.
//!
//! When the app goes to the background the window's surface goes with it
//! (`suspended`): the game goes on, drawing nothing, as a covered window's
//! does ([`hidden`]), until it comes back (`resumed`), or until Android
//! ends the process. A run that ends (Quit, or the game's own error) ends
//! the process: winit's event loop is made once a process.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::androidmap::{Axes, key_bit, key_of};
use crate::input::{Latch, Report};
use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jfloat, jint};
use winit::keyboard::{Key, NamedKey};
use winit::platform::android::activity::AndroidApp;

/// The activity's app, as `android_main` was handed it.
static APP: OnceLock<AndroidApp> = OnceLock::new();
/// Wakes the window's loop: what the activity hands over is taken there.
static WAKER: OnceLock<winit::event_loop::EventLoopProxy<()>> = OnceLock::new();
/// What the activity has handed over and the loop has not taken yet.
static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());
/// The controllers, by Android's device id.
static PADS: Mutex<Vec<Pad>> = Mutex::new(Vec::new());
/// The activity is paused (in the back, or behind another): no frame is
/// drawn, and winit asks for none.
static PAUSED: AtomicBool = AtomicBool::new(false);
/// The window has no surface (the app in the back, `viewer`'s `suspended`).
static NO_SURFACE: AtomicBool = AtomicBool::new(false);
/// The last key came from the device's own keyboard (`device_keyboard`).
static SOFT_KEY: AtomicBool = AtomicBool::new(false);
/// How much of the window's bottom the device's keyboard covers, in pixels.
static KEYBOARD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Android's entry: the game from its boot (main's `boot`), in the
/// activity's window. The run's end ends the process (a Quit, or the game
/// stopping with its line in the log).
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let _ = APP.set(app.clone());
    // where the game keeps its settings and its cache: the activity set
    // them before the game started; the app's own folder else
    if std::env::var_os("EQOA_CONFIG").is_none()
        && let Some(d) = app.internal_data_path()
    {
        // SAFETY: before any other thread of the game's is started
        unsafe {
            std::env::set_var("EQOA_CONFIG", d.join("config"));
            std::env::set_var("EQOA_CACHE", d.join("cache"));
        }
    }
    let code = match crate::boot(None, None) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("eqoa: {e}");
            crate::logfile::close();
            1
        }
    };
    // the activity closes with the run (on its own thread, a moment), and
    // the process after it
    call("exitGame", "()V", &[]);
    std::thread::sleep(std::time::Duration::from_millis(300));
    std::process::exit(code);
}

/// The activity's app (`viewer`'s event loop is made from it).
pub fn app() -> AndroidApp {
    APP.get().expect("android_main sets the app").clone()
}

/// The window's loop, to wake when the activity hands something over.
pub fn set_waker(p: winit::event_loop::EventLoopProxy<()>) {
    let _ = WAKER.set(p);
}

fn wake() {
    if let Some(w) = WAKER.get() {
        let _ = w.send_event(());
    }
}

fn push(e: Event) {
    lock(&EVENTS).push_back(e);
    wake();
}

/// What the activity handed over, for the window's loop (`viewer`).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// a finger (or a mouse's button) on the screen, in the window's pixels
    Touch { id: u64, phase: crate::touch::Phase, at: [f32; 2] },
    /// a key: a keyboard's, or the device's own keyboard's (`soft`)
    Key { key: Key, text: Option<String>, down: bool, repeat: bool, soft: bool },
    /// the disc image picked (`pick_disc`): its path, None when the player
    /// would rather not (the game ends)
    Disc(Option<String>),
    /// the answer to `confirm_disc`: play with it anyway
    DiscAnswer(bool),
}

/// Everything handed over since the last call, in order.
pub fn take() -> Vec<Event> {
    lock(&EVENTS).drain(..).collect()
}

/// Whether the activity is paused or has no window: frames are run as a
/// covered window's (`viewer`'s `page_hidden`), the game, its sound and its
/// network going on, nothing drawn.
pub fn hidden() -> bool {
    PAUSED.load(Ordering::Relaxed) || NO_SURFACE.load(Ordering::Relaxed)
}

/// Whether the window has its surface (`viewer`'s `suspended`, `resumed`).
pub fn set_surface(has: bool) {
    NO_SURFACE.store(!has, Ordering::Relaxed);
}

/// How much of the window's bottom the device's keyboard covers, in pixels
/// (0 while it is down): the frame is drawn above it (`viewer`'s
/// `fit_keyboard`), as a page's browser shrinks the page for it.
pub fn keyboard_height() -> u32 {
    KEYBOARD.load(Ordering::Relaxed)
}

/// Whether the last key came from the device's own keyboard: it leaves the
/// touch screen's pad where it is (`viewer`'s `device_keyboard`).
pub fn device_keyboard() -> bool {
    SOFT_KEY.load(Ordering::Relaxed)
}

// ---- the calls into the activity

/// `method` of the activity, on this thread (the activity's methods hand
/// their work to its own thread). Nothing when the activity is gone.
fn call(method: &str, sig: &str, args: &[JValue]) {
    let Some(app) = APP.get() else { return };
    // SAFETY: the app's JavaVM and its activity, held by the framework for
    // the process (the activity reference is never deleted here)
    let r = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }?;
        let mut env = vm.attach_current_thread_permanently()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        let r = env.call_method(&activity, method, sig, args).map(|_| ());
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        r
    })();
    if let Err(e) = r {
        crate::say(&format!("android: the activity's {method}: {e}"));
    }
}

/// `method(String)`, None as null.
fn call_text(method: &str, text: Option<&str>) {
    let Some(app) = APP.get() else { return };
    // SAFETY: as `call`'s
    let r = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }?;
        let mut env = vm.attach_current_thread_permanently()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        let s = match text {
            Some(t) => JObject::from(env.new_string(t)?),
            None => JObject::null(),
        };
        let r = env.call_method(&activity, method, "(Ljava/lang/String;)V", &[JValue::Object(&s)]).map(|_| ());
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        r
    })();
    if let Err(e) = r {
        crate::say(&format!("android: the activity's {method}: {e}"));
    }
}

/// An edit box of the game's has the keys now, or none has: the device's
/// keyboard comes up for it, or goes (as a page's does, `web::want_text`).
pub fn want_text(on: bool) {
    call("wantText", "(Z)V", &[JValue::Bool(on as jboolean)]);
}

/// The disc image asked for: the activity's picker, `note` saying why it
/// asks again. The pick comes back as `Event::Disc`.
pub fn pick_disc(note: Option<&str>) {
    call_text("pickDisc", note);
}

/// A disc that is not Frontiers: the player asked whether to play with it
/// anyway (`line` says which disc it is); the answer comes back as
/// `Event::DiscAnswer`.
pub fn confirm_disc(line: &str) {
    call_text("confirmDisc", Some(line));
}

/// The disc picked is the one played: the activity keeps it for the next
/// start.
pub fn keep_disc() {
    call("keepDisc", "()V", &[]);
}

// ---- the log

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}

/// One line to logcat, tag `eqoa`, at INFO.
pub fn logcat(text: &str) {
    for line in text.lines() {
        let line = std::ffi::CString::new(line.replace('\0', " ")).unwrap_or_default();
        // SAFETY: two NUL-terminated strings, read and not kept
        unsafe {
            __android_log_write(4, c"eqoa".as_ptr(), line.as_ptr());
        }
    }
}

/// Where the log's lines go besides `eqoa.log` (`logfile`'s terminal): logcat.
pub struct Logcat;

impl std::io::Write for Logcat {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        logcat(&String::from_utf8_lossy(b));
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ---- the controllers

/// One controller as the activity reports it: the buttons its keys hold, and
/// its axes.
struct Pad {
    device: i32,
    label: String,
    /// the buttons its key events hold (`key_bit`)
    keys: u16,
    axes: Axes,
    /// every state since the last read (`input::Latch`)
    latch: Latch,
}

impl Pad {
    /// What it holds now, as one source's report.
    fn report(&self) -> Report {
        crate::androidmap::report(self.keys, &self.axes)
    }
}

fn with_pad(device: i32, label: String, f: impl FnOnce(&mut Pad)) {
    let mut pads = lock(&PADS);
    let k = match pads.iter().position(|p| p.device == device) {
        Some(k) => k,
        None => {
            pads.push(Pad { device, label: label.clone(), keys: 0, axes: Axes::default(), latch: Latch::default() });
            pads.len() - 1
        }
    };
    let p = &mut pads[k];
    if !label.is_empty() {
        p.label = label;
    }
    f(p);
    let r = p.report();
    p.latch.saw(r.buttons);
    drop(pads);
    wake();
}

/// A connected controller as `input::Gamepads` reads it: its name (stable
/// while it stays connected), its label, its report, and the buttons it
/// pressed and let go since the last read.
pub struct Found {
    pub device: i32,
    pub name: String,
    pub label: String,
    pub report: Report,
    pub between: u16,
}

/// Every controller the activity has reported, in the order they came.
pub fn pads() -> Vec<Found> {
    let mut pads = lock(&PADS);
    pads.iter_mut()
        .map(|p| {
            let report = p.report();
            let between = p.latch.take(report.buttons);
            Found { device: p.device, name: format!("android{}", p.device), label: p.label.clone(), report, between }
        })
        .collect()
}

// ---- what the activity calls (EqoaActivity's `native*`)

fn phase_of(action: jint) -> Option<crate::touch::Phase> {
    use crate::touch::Phase;
    Some(match action {
        0 => Phase::Started,
        1 => Phase::Moved,
        2 => Phase::Ended,
        3 => Phase::Cancelled,
        _ => return None,
    })
}

/// A finger: `action` 0 down, 1 moved, 2 up, 3 cancelled; at (x, y) in the
/// window's pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativeTouch(_env: JNIEnv, _class: JClass, action: jint, id: jint, x: jfloat, y: jfloat) {
    if let Some(phase) = phase_of(action) {
        push(Event::Touch { id: id as u64, phase, at: [x, y] });
    }
}

/// A key that is not a controller's: its code, the character it types (0
/// for none), down or up, its repeat count, and whether the device's own
/// keyboard sent it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativeKey(_env: JNIEnv, _class: JClass, code: jint, unicode: jint, down: jboolean, repeat: jint, soft: jboolean) {
    let Some((key, text)) = key_of(code, unicode as u32) else { return };
    let down = down != 0;
    push(Event::Key { key, text: if down { text } else { None }, down, repeat: repeat > 0, soft: soft != 0 });
}

/// Characters the device's own keyboard put in (each a key pressed and let
/// go), and `back` of them taken away first (Backspace each).
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativeText(mut env: JNIEnv, _class: JClass, back: jint, text: JString) {
    let s: String = env.get_string(&text).map(Into::into).unwrap_or_default();
    let mut q = lock(&EVENTS);
    for _ in 0..back.max(0) {
        for down in [true, false] {
            q.push_back(Event::Key { key: Key::Named(NamedKey::Backspace), text: None, down, repeat: false, soft: true });
        }
    }
    for c in s.chars() {
        let Some((key, text)) = (if c == '\n' { key_of(66, 0) } else { key_of(0, c as u32) }) else { continue };
        q.push_back(Event::Key { key: key.clone(), text, down: true, repeat: false, soft: true });
        q.push_back(Event::Key { key, text: None, down: false, repeat: false, soft: true });
    }
    drop(q);
    wake();
}

/// A controller's key (`key_bit`'s), down or up.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativePadKey(mut env: JNIEnv, _class: JClass, device: jint, label: JString, code: jint, down: jboolean) {
    let label: String = env.get_string(&label).map(Into::into).unwrap_or_default();
    let b = key_bit(code);
    with_pad(device, label, |p| {
        if down != 0 {
            p.keys |= b;
        } else {
            p.keys &= !b;
        }
    });
}

/// A controller's axes, all of them as they are now.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativePadAxes(
    mut env: JNIEnv,
    _class: JClass,
    device: jint,
    label: JString,
    lx: jfloat,
    ly: jfloat,
    rx: jfloat,
    ry: jfloat,
    hat_x: jfloat,
    hat_y: jfloat,
    lt: jfloat,
    rt: jfloat,
) {
    let label: String = env.get_string(&label).map(Into::into).unwrap_or_default();
    with_pad(device, label, |p| p.axes = Axes { lx, ly, rx, ry, hat_x, hat_y, lt, rt });
}

/// A controller gone.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativePadGone(_env: JNIEnv, _class: JClass, device: jint) {
    lock(&PADS).retain(|p| p.device != device);
    wake();
}

/// The disc image picked: its path (`/proc/self/fd/<n>`), or null when the
/// player would rather not.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativeDisc(mut env: JNIEnv, _class: JClass, path: JString) {
    let p = if path.is_null() { None } else { env.get_string(&path).ok().map(String::from) };
    push(Event::Disc(p));
}

/// The answer to `confirm_disc`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativeDiscAnswer(_env: JNIEnv, _class: JClass, yes: jboolean) {
    push(Event::DiscAnswer(yes != 0));
}

/// The device's keyboard covers this much of the window's bottom (pixels).
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativeKeyboard(_env: JNIEnv, _class: JClass, height: jint) {
    KEYBOARD.store(height.max(0) as u32, Ordering::Relaxed);
    wake();
}

/// The activity paused (true) or resumed (false).
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dabdavis_eqoa_EqoaActivity_nativePaused(_env: JNIEnv, _class: JClass, paused: jboolean) {
    PAUSED.store(paused != 0, Ordering::Relaxed);
    wake();
}

/// The window's loop takes a key: whether it came from the device's own
/// keyboard is kept for `device_keyboard`.
pub fn key_taken(soft: bool) {
    SOFT_KEY.store(soft, Ordering::Relaxed);
}
