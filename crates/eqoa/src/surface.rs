//! The window's surface, what is done with it frame by frame (`viewer`'s
//! `Gfx::acquire`, `Gfx::configure_surface`). A surface is the window's, not
//! the device's: Windows takes it away or changes it under the game without
//! the device being lost, and wgpu raises that as a validation error, which
//! the device's error handler took as wgpu's default does, a panic (two crash
//! reports of 2026-11-19/21, both inside a window message on Windows' Vulkan:
//! "In Surface::configure  Invalid surface" on an NVIDIA T550, the window
//! minimised, its frame's acquire answering Outdated and the surface
//! configured again at the size it had, which a minimised window's surface
//! (no size at all) refuses; and "Surface::get_current_texture_view: Surface
//! is not configured for presentation" on an AMD Radeon, the surface of a
//! recovered device (`gpulost`) whose configuring had failed and been let go,
//! acquired from all the same: wgpu panics there with no handler to ask).
//!
//! So: a minimised window (a size of nothing) is neither configured nor drawn
//! on until it has a size again; an outdated surface is configured again; a
//! lost one, or one that refused its configuring or its acquire, is made
//! again from the window and configured (`Remake`), tried again a second on
//! while it keeps failing; and a surface is acquired from only once it has
//! been configured without an error. A device lost is still `gpulost`'s.

use std::time::Duration;

/// Where the window's surface stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Configured without an error: frames are acquired from it.
    Ready,
    /// To be configured before the next frame is acquired (outdated, or the
    /// window was minimised when it would have been).
    Configure,
    /// To be made again from the window and configured (lost, or it refused
    /// its configuring or its acquire).
    Remake,
}

/// What a frame does with the surface before drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing to draw on (the window minimised, or a surface made again too
    /// recently to try again): the frame skipped, as a covered one is.
    Skip,
    Configure,
    Remake,
    Acquire,
}

/// While a surface keeps failing to be made again, it is tried this often.
pub const RETRY: Duration = Duration::from_secs(1);

/// The frame's step: `size` the window's (a minimised Windows window's is
/// nothing), `retry_due` whether a remake that failed may be tried again.
pub fn step(state: State, size: (u32, u32), retry_due: bool) -> Step {
    if size.0 == 0 || size.1 == 0 {
        return Step::Skip;
    }
    match state {
        State::Ready => Step::Acquire,
        State::Configure => Step::Configure,
        State::Remake if retry_due => Step::Remake,
        State::Remake => Step::Skip,
    }
}

/// What an acquire answered, without its texture (`wgpu::CurrentSurfaceTexture`
/// but `Success` and `Suboptimal`, which are drawn on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    Timeout,
    Occluded,
    Outdated,
    Lost,
    /// A validation error raised by the acquire (caught by its error scope,
    /// or by the device's handler).
    Validation,
}

/// The surface's state after an acquire that gave no texture: timed out or
/// covered, it stays as it is (the frame skipped); outdated, configured
/// again; lost or refused, made again.
pub fn after_refused(r: Refused) -> State {
    match r {
        Refused::Timeout | Refused::Occluded => State::Ready,
        Refused::Outdated => State::Configure,
        Refused::Lost | Refused::Validation => State::Remake,
    }
}

/// The surface's state after configuring it: without an error, ready; with
/// one (wgpu's "Invalid surface": the window's surface lost, or in use by a
/// surface let go too late; a size it does not take), made again.
pub fn after_configure(error: bool) -> State {
    if error { State::Remake } else { State::Ready }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimised window (Windows sends a size of nothing) is neither
    /// configured nor acquired from, whatever the surface's state; with a
    /// size again, the state's step. The control: a window with a size and a
    /// ready surface is acquired from.
    #[test]
    fn minimised_skips() {
        for s in [State::Ready, State::Configure, State::Remake] {
            for size in [(0, 0), (0, 600), (800, 0)] {
                assert_eq!(step(s, size, true), Step::Skip, "{s:?} at {size:?}");
            }
        }
        assert_eq!(step(State::Ready, (800, 600), true), Step::Acquire, "the control");
        assert_eq!(step(State::Configure, (800, 600), false), Step::Configure);
        assert_eq!(step(State::Remake, (800, 600), true), Step::Remake);
        assert_eq!(step(State::Remake, (800, 600), false), Step::Skip, "a failing remake waits its second");
    }

    /// Outdated: configured again; lost, or a validation error from the
    /// acquire: made again, never acquired from as it is (the AMD report's
    /// panic); timed out or covered: the frame skipped, the surface kept.
    #[test]
    fn refused_acquires() {
        assert_eq!(after_refused(Refused::Outdated), State::Configure);
        assert_eq!(after_refused(Refused::Lost), State::Remake);
        assert_eq!(after_refused(Refused::Validation), State::Remake);
        assert_eq!(after_refused(Refused::Timeout), State::Ready);
        assert_eq!(after_refused(Refused::Occluded), State::Ready);
        // and the next frame's step from each, with a size
        assert_eq!(step(after_refused(Refused::Outdated), (800, 600), false), Step::Configure);
        assert_eq!(step(after_refused(Refused::Validation), (800, 600), true), Step::Remake);
    }

    /// A configure that raised an error (the NVIDIA report's "Invalid
    /// surface") leaves the surface to be made again, not acquired from; one
    /// without is ready (the control).
    #[test]
    fn configure_errors_remake() {
        assert_eq!(after_configure(true), State::Remake);
        assert_eq!(step(after_configure(true), (800, 600), true), Step::Remake);
        assert_ne!(step(after_configure(true), (800, 600), true), Step::Acquire);
        assert_eq!(after_configure(false), State::Ready);
    }
}
