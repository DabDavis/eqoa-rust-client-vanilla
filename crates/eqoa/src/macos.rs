//! macOS only: the app menu's Settings… with ⌘, (the system's own place
//! and key for a program's settings), opening the game's Options: in the
//! world the Options window, at the title its Options (`viewer`, which asks
//! `take` once a frame). winit makes the app menu (About, Services, Hide,
//! Quit); the item is added to it after About. A menu item's action goes to
//! its target, here an object of a class of its own whose one method sets a
//! flag for the frame to take. ⌘, as a key (no menu to take it) asks the
//! same (`ask`).

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenuItem};
use objc2_foundation::NSString;
use std::sync::atomic::{AtomicBool, Ordering};

/// Settings… chosen (or ⌘, pressed) since the frame last took it.
static ASKED: AtomicBool = AtomicBool::new(false);
/// The item is added once.
static ADDED: AtomicBool = AtomicBool::new(false);

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class adds
    // no state and no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "EQOASettingsTarget"]
    struct SettingsTarget;

    impl SettingsTarget {
        /// The item's action.
        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            ASKED.store(true, Ordering::Relaxed);
        }
    }
);

/// Adds Settings… (⌘,) to the app menu winit made, after About and its
/// separator, with a separator of its own under it; once, on the main thread,
/// after the application finished launching (winit's `resumed`). Without the
/// menu it adds nothing (⌘, still reaches the window as a key, `ask`).
pub fn add_settings_item() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if ADDED.swap(true, Ordering::Relaxed) {
        return;
    }
    let app = NSApplication::sharedApplication(mtm);
    let Some(main) = app.mainMenu() else { return };
    let Some(app_menu) = main.itemAtIndex(0).and_then(|i| i.submenu()) else { return };
    // SAFETY: `init` is NSObject's, and the class has nothing else to set up.
    let target: Retained<SettingsTarget> = unsafe { msg_send![SettingsTarget::alloc(mtm), init] };
    // SAFETY: the selector is the target's method above, taking the sender.
    let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str("Settings…"), Some(sel!(openSettings:)), &NSString::from_str(",")) };
    // SAFETY: the target answers the item's action, and lives as long as the
    // program (a menu item does not keep its target: it is kept below).
    unsafe { item.setTarget(Some(&target)) };
    let at = if app_menu.numberOfItems() > 1 { 2 } else { app_menu.numberOfItems() };
    app_menu.insertItem_atIndex(&item, at);
    app_menu.insertItem_atIndex(&NSMenuItem::separatorItem(mtm), at + 1);
    std::mem::forget(target);
}

/// ⌘, pressed with no menu to take it: asked as Settings… is.
pub fn ask() {
    ASKED.store(true, Ordering::Relaxed);
}

/// Whether Settings… was asked for since the last frame (taken).
pub fn take() -> bool {
    ASKED.swap(false, Ordering::Relaxed)
}
