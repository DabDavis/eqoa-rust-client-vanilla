//! A window in the back keeps its pace, natively (a tester multiboxing,
//! 2026-10-27: "if multibox, background window has low fps, but focus is
//! good"; Windows reports' focus lines had a window in the back drawing at
//! 40-55 frames a second against 60 in front). The client slows only a
//! window the system says is covered or minimised (`Gfx::acquire`); the
//! systems slow a program in the back of their own accord, and this asks
//! them not to, once, as the game starts:
//! - Windows: the process opts out of power throttling (EcoQoS, which
//!   Windows 11 puts on a program in the back: its threads on the slow
//!   cores at a low clock, its timers coarse), for its speed and for its
//!   timers' resolution, as `powercfg /powerthrottling disable /path <exe>`
//!   does (the usual fix for multiboxing on Windows 11); a Windows that
//!   knows no timers' part (before 11 22H2) is asked for the speed alone;
//! - macOS: an activity, user-initiated and latency-critical, begun and
//!   never ended, so App Nap and timer coalescing leave the process alone;
//!   the one that allows idle sleep, so a Mac left with the game open still
//!   sleeps by itself.
//!
//! Each says in the log what it got. A page has neither: its browser
//! decides (on Windows 11 Task Manager's Efficiency mode, or the power mode,
//! can slow a browser window in the back).

/// Asks the system not to slow the program while its window is in the
/// back, and says so in the log.
pub fn keep_pace() {
    #[cfg(windows)]
    windows();
    #[cfg(target_os = "macos")]
    macos();
}

#[cfg(windows)]
fn windows() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetProcessInformation,
    };
    // the parts `mask` names taken out of throttling: controlled, and off
    let ask = |mask: u32| -> std::io::Result<()> {
        let state = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ControlMask: mask, StateMask: 0 };
        // SAFETY: the process's own pseudo-handle, and a struct of the size given
        let ok = unsafe {
            SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(), size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32)
        };
        if ok != 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
    };
    match ask(PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION) {
        Ok(()) => crate::say("pace: Windows' power throttling is off for this program, its speed and its timers: a window in the back runs as in front"),
        Err(both) => match ask(PROCESS_POWER_THROTTLING_EXECUTION_SPEED) {
            Ok(()) => crate::say(&format!("pace: Windows' power throttling is off for this program's speed (its timers' part refused: {both})")),
            Err(e) => crate::say(&format!("pace: Windows' power throttling could not be turned off ({e}): a window in the back may be slowed")),
        },
    }
}

#[cfg(target_os = "macos")]
fn macos() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(NSActivityOptions::UserInitiatedAllowingIdleSystemSleep | NSActivityOptions::LatencyCritical, &NSString::from_str("a game window"));
    // held for the process's life: the activity ends only with it
    std::mem::forget(activity);
    crate::say("pace: macOS told the game is user-initiated (idle sleep allowed) and latency-critical: no App Nap or timer coalescing for a window in the back");
}
