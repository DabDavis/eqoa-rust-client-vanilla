//! The GM's channel in play: `VIClientPart382E8` (`viadmin.cpp`: 0x0700 in,
//! 0x0701 out), the start menu's TEST entry it enables, and the windows it
//! opens (`VIWndBuffPC`, `VIWndGoto`, `VIWndNetStat`). A child of `play`, for
//! `Play`'s fields.

use super::Play;

/// `VIClientPart382E8` and its windows.
#[derive(Default)]
pub struct Admin {
    keys: Vec<(i32, bool)>,
}

/// The modes the GM's windows own.
pub fn owns(_mode: i32) -> bool {
    false
}

impl Play {
    /// A key for a GM window, handled with the next HUD frame.
    pub fn admin_key(&mut self, key: i32, down: bool) {
        self.admin.keys.push((key, down));
    }

    /// A pointer at frame point `f` on a GM window (`pointer`): the keys to
    /// press.
    pub fn admin_point(&mut self, _f: [f32; 2]) -> Vec<i32> {
        Vec::new()
    }

    /// The mode is a GM window's.
    pub fn admin_owns(&self) -> bool {
        owns(self.mode)
    }

    /// A GM window's edit box is up (it takes the keyboard).
    pub fn admin_typing(&self) -> bool {
        false
    }

    /// The opcodes the GM's channel reads (0x0700); true when handled.
    pub(super) fn admin_opcode(&mut self, _op: u16, _data: &[u8]) -> bool {
        false
    }

    /// The GM's windows, each HUD frame: their keys, then drawn.
    pub(super) fn run_admin(&mut self, _vui: &vi::ui::Vui, _r: &mut dyn vi::ui::Raster) {
        self.admin.keys.clear();
    }
}
