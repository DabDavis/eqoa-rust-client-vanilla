//! Fishing in play: `VIClientPart3FC10` (`VIClient+0x3FC10`: 0x0F02 in,
//! 0x0F01 out) and `VIWndGame`'s mode 36, the fishing window
//! (`VIWndFishing`). A child of `play`, for `Play`'s fields.

use super::Play;

/// `VIClientPart3FC10` and `VIWndFishing`.
#[derive(Default)]
pub struct Fishing {
    keys: Vec<(i32, bool)>,
}

/// The mode the fishing window owns.
pub fn owns(mode: i32) -> bool {
    mode == 36
}

impl Play {
    /// A key for the fishing window, handled with the next HUD frame.
    pub fn fishing_key(&mut self, key: i32, down: bool) {
        self.fish.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the fishing window (`pointer`): the
    /// keys to press.
    pub fn fishing_point(&mut self, _f: [f32; 2]) -> Vec<i32> {
        Vec::new()
    }

    /// The mode is the fishing window's.
    pub fn fishing_owns(&self) -> bool {
        owns(self.mode)
    }

    /// The fishing window's edit box is up (it takes the keyboard).
    pub fn fishing_typing(&self) -> bool {
        false
    }

    /// The opcodes fishing reads (0x0F02); true when handled.
    pub(super) fn fishing_opcode(&mut self, _op: u16, _data: &[u8]) -> bool {
        false
    }

    /// Msg00B2's tail: a fishing session going is cancelled.
    pub(super) fn fishing_respawn(&mut self) {}

    /// The fishing window, each HUD frame: its keys, then drawn.
    pub(super) fn run_fishing(&mut self, _vui: &vi::ui::Vui, _r: &mut dyn vi::ui::Raster) {
        self.fish.keys.clear();
    }
}
