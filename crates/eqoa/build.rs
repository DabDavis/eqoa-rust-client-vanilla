//! The main thread's stack on Windows: 8 MB, as Linux and macOS give it.
//!
//! A Windows program's main thread has the stack its header reserves, 1 MB
//! unless the link says otherwise, and the game's first frame needs more:
//! its frames hold their state by value, and when the first frame opens the
//! sound device the stack holds about 980 KB (measured under cdb on a Windows
//! machine: `eqoa::run` 241 KB, `viewer::run_app` 236, `App::host_frame` 174,
//! `run_boot` 123, `Audio::open` 111, `window_event` 54). Without this the
//! client died there, "thread 'main' has overflowed its stack", before its
//! window had drawn anything: a white window (2026-10-22). `dev.yml` refuses
//! a Windows build whose header reserves less.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:8388608"),
            Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,8388608"),
            _ => {}
        }
    }
}
