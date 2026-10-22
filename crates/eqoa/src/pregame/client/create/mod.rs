//! Character creation (`VIWndCharCreateBlank` and its pages: race and sex,
//! class and attributes, appearance, name, review). STAND-IN: the lane
//! porting creation replaces this file. Character select (`super`) opens it
//! from an empty slot, hands it the session and the stage each frame, routes
//! it the keys, the pointer and the session's events, and draws its scene.

use crate::pregame::stage::Stage;
use crate::pregame::{Ctx, Scene};
use net::session::{Event, Session};

/// What creation tells character select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Stay,
    /// backed out of the first page (260): character select again
    Back,
    /// the character was made (the server's new list came): select again,
    /// on it
    Done,
}

#[derive(Default)]
pub struct Create {
    open: bool,
}

impl Create {
    pub fn new() -> Self {
        Self::default()
    }

    /// `VIWndCharCreateBlank::Reset`, its first page up.
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// One frame of the page up: its requests go on `session` (0x0012 a
    /// name, 0x002B the character), its model stands on `stage`.
    pub fn frame(&mut self, cx: &mut Ctx, session: &mut Session, stage: Option<&mut Stage>) -> Out {
        let _ = (cx, session, stage);
        self.open = false;
        Out::Back
    }

    /// A session event the pages answer (`RandomName`, `CreateRefused`,
    /// the new `Characters` after a create).
    pub fn event(&mut self, e: &Event) -> Out {
        let _ = e;
        Out::Stay
    }

    pub fn key(&mut self, key: i32, down: bool) {
        let _ = (key, down);
    }

    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let _ = p;
        Vec::new()
    }

    pub fn typing(&self) -> bool {
        false
    }

    /// The page's 3D: its model in its race's set, through the stage's view.
    pub fn scene<'a>(&'a self, stage: &'a Stage) -> Option<Scene<'a>> {
        let _ = stage;
        None
    }
}
