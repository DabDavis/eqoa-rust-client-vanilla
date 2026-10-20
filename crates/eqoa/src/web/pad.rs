//! The pads in a page, straight from the Gamepad API: every connected pad's
//! id, mapping, buttons and axes as `navigator.getGamepads()` has them, for
//! `padmap` to lay out (`input::Gamepads`). A browser shows a pad to a page
//! only after one of its buttons is pressed while the page is in view. The
//! motors stay the page's (`window.eqoaRumble`, main.js).

use crate::padmap::{Browser, WebPad};
use wasm_bindgen::JsCast;

/// The connected pads, as they are now. Read by their properties, not their
/// classes (as gilrs read them): a stand-in with the same fields, as
/// `drive.py --gamepad` plays, is a pad too.
pub fn gamepads() -> Vec<WebPad> {
    let Some(list) = web_sys::window().and_then(|w| w.navigator().get_gamepads().ok()) else { return Vec::new() };
    list.iter()
        .filter(|g| g.is_object())
        .map(|g| g.unchecked_into::<web_sys::Gamepad>())
        .filter(|g| g.connected())
        .map(|g| {
            // the mapping as the browser spells it ("standard", "", "xr-standard")
            let mapping = js_sys::Reflect::get(&g, &"mapping".into()).ok().and_then(|m| m.as_string()).unwrap_or_default();
            let buttons = g
                .buttons()
                .iter()
                .map(|b| match b.is_object() {
                    true => {
                        let b = b.unchecked_into::<web_sys::GamepadButton>();
                        (b.pressed(), b.value() as f32)
                    }
                    false => (false, 0.0),
                })
                .collect();
            let axes = g.axes().iter().map(|a| a.as_f64().map_or(f32::NAN, |v| v as f32)).collect();
            WebPad { index: g.index(), id: g.id(), mapping, buttons, axes }
        })
        .collect()
}

/// The browser (`navigator.userAgent`) and the page's `?padxy=`.
pub fn browser() -> Browser {
    let ua = web_sys::window().and_then(|w| w.navigator().user_agent().ok()).unwrap_or_default();
    Browser::new(&ua, super::page_value("padxy").as_deref())
}
