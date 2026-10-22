//! `VIWndDisconnect` (CLIENT `viwnddisconnect.cpp`): the disconnect notice
//! `VIClient::Notify` puts up when the connection goes with character select
//! (or the world) up: a dark screen, the reason (`ConnectStatusString`) in a
//! box sized to it, and Ok; Ok goes where `DisconnectKind` says
//! (`Msg0777Done`).

use super::UiCx;
use crate::pointer::{Hits, key};
use vi::ui::{UI_COLORS, Window, wide};

#[derive(Default)]
pub struct Notice {
    window: Window,
    hits: Hits,
}

impl Notice {
    /// `OnMessage` 295, the focus arriving: the window, titled
    /// "Disconnected", opening.
    pub fn focus(&mut self, cx: &UiCx) {
        self.window.close(true);
        self.window.init(cx.vui, cx.r, [320, 224], 350, 4, 0);
        self.window.set_title(Some(&wide(cx.vui.words.get(272, cx.vui.language))));
        self.window.open(false);
    }

    /// `OnDrawSelf`: the dark screen, the box sized to the reason (wider
    /// than 440 it wraps on five rows), its text and Ok.
    pub fn draw(&mut self, cx: &mut UiCx) {
        let (vui, r) = (cx.vui, &mut *cx.r);
        vui.blit_rect(r, 32, [-1, -1], [642, 450], 3, 1.0);
        let w = self.window.text_width(vui, r, &cx.status);
        let fits = w < 441;
        let w = if fits { w } else { 440 };
        let rows = if fits { 4 } else { 5 };
        self.window.init(vui, r, [320, 224], w + 100, rows, 0);
        self.window.open(true);
        self.window.draw(vui, r);
        let width = self.window.size[0];
        self.window.text_box(vui, r, &cx.status, 40, 1, width - 80, 2, UI_COLORS[0], 8);
        let mut ok = wide("|x");
        ok.extend(wide(vui.words.get(786, vui.language)));
        let row = if fits { 3 } else { 4 };
        self.window.text(vui, r, &ok, 40, row, UI_COLORS[0], 0);
        self.hits.clear();
        self.hits.push(crate::pointer::rows_rect(&self.window, vui, r, 40, row, width - 80, 1), 0);
    }

    /// `OnKeyDown`: Enter or ✕ is Ok (true).
    pub fn key(&mut self, k: i32, _cues: &mut Vec<i32>) -> bool {
        matches!(k, 13 | 134)
    }

    /// A pointer on Ok presses ✕.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.hits.at(p).is_some() { vec![key::CROSS] } else { Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ok by key and by pointer; nothing else closes it.
    #[test]
    fn ok_closes_the_notice() {
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut n = Notice::default();
        let mut cx = UiCx::bare(&vui, &mut r);
        cx.status = wide("Logged out.");
        n.focus(&cx);
        n.draw(&mut cx);
        let mut cues = Vec::new();
        assert!(n.key(134, &mut cues) && n.key(13, &mut cues));
        assert!(!n.key(132, &mut cues), "the control: triangle is not Ok");
        let rr = crate::pointer::rows_rect(&n.window, &vui, &r, 40, 3, n.window.size[0] - 80, 1);
        assert_eq!(n.point([(rr[0] + rr[2]) as f32 / 2.0, (rr[1] + rr[3]) as f32 / 2.0]), vec![key::CROSS]);
        assert!(n.point([5.0, 5.0]).is_empty());
    }
}
