//! `VIWndInfluence` (CLIENT 0x00C62310 .. 0x00C63200): the active effects'
//! icons (state channel 0x43, `net::dump::read_effects`) in a capped bar
//! right of the gauges. `VIWndGame::Init` places it at (143, 15) and adds it
//! as its first child, so it is drawn just after the HUD, in the modes
//! `shown` names.

use crate::ui::{Raster, Vui};
use net::dump::Effects;

/// Whether it is up in `mode`: `UpdateMode` hides it, then shows it again
/// in play (0, and 38 the manual camera) and over the boxes that leave the
/// HUD up (4, 7, 9, 14, 16, 17, 20, 33, 36). `OpenStartMenu` hides it for
/// the start menu (13), and so does the quick-chat menu (30, `VITextSet`'s).
pub fn shown(mode: i32) -> bool {
    matches!(mode, 0 | 38 | 4 | 7 | 9 | 14 | 16 | 17 | 20 | 33 | 36)
}

/// `OnDrawSelf`: the left cap (159's art), a 34-high cell of 157's art per
/// effect with its icon (by DictID, whole) 3 in and 6 down, the last cell 3
/// narrower, then the right cap (160).
pub fn draw(vui: &Vui, r: &mut dyn Raster, fx: &Effects) {
    let mut pos = [143, 15];
    let off = [3, 6];
    let count = fx.count;
    if count > 0 {
        let e = vui.entry(159);
        vui.blit_ex(r, e.x00, pos, 0, 1.0, 0);
        pos[0] += e.uv1[0];
    }
    for i in 0..count.max(0) {
        let e = vui.entry(157);
        let w = if i == count - 1 { e.uv1[0] - 3 } else { e.uv1[0] };
        vui.blit_ex_size(r, e.x00, pos, [w, 34], 0, 1.0, 0);
        vui.blit_id(r, fx.icon[i as usize], [pos[0] + off[0], pos[1] + off[1]], 0, 1.0, 0);
        pos[0] += w;
    }
    if count > 0 {
        vui.blit_ex(r, 160, pos, 0, 1.0, 0);
    }
}
