//! SETUP (`setup_main`, `VISetup`): the legal screen, the licence, the
//! title. STAND-IN: the title's painting and Play; the lane porting SETUP
//! replaces this file.

use super::{Ctx, Next, Shared, res::Res};
use vi::ui::wide;

/// What `VISetup::Init` reads after the base files.
pub const FILES: &[&str] = &["DATA2/SETUP.CSF", "DATA2/ADDART.CSF", "DATA/EULA.TXT"];

#[derive(Default)]
pub struct Setup {
    started: bool,
    go: bool,
}

impl Setup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, _res: &mut Res, _shared: &mut Shared) {
        self.started = true;
    }

    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        let (vui, r) = (cx.vui, &mut *cx.r);
        vui.blit_id(r, vi::hash::hash_resource_id("splash00.tga"), [0, 0], 0, 1.0, 0);
        vui.blit_id(r, vi::hash::hash_resource_id("splash10.tga"), [320, 0], 0, 1.0, 0);
        vui.draw_centered_string_ex(r, &wide(&cx.res.word(790)), 320, 360, [1.0, 1.0, 0.0, 1.0], 0, true);
        if std::mem::take(&mut self.go) {
            if cx.shared.server.is_none() {
                crate::say("no server: set one under Network Setup");
                return Next::Stay;
            }
            return Next::Station;
        }
        Next::Stay
    }

    pub fn key(&mut self, key: i32, down: bool) {
        if down && matches!(key, 13 | 134) {
            self.go = true;
        }
    }

    pub fn point(&mut self, _p: [f32; 2]) -> Vec<i32> {
        vec![134]
    }

    pub fn typing(&self) -> bool {
        false
    }
}
