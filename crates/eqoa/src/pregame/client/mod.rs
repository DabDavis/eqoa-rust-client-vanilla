//! CLIENT's windows before the world (`VIClient::InitBody`'s): the server
//! list (`VIWndServerSelect`), character select (`VIWndCharacterSelect`) and
//! creation (`VIWndCharCreateBlank` and its pages). STAND-IN: the first shard
//! and the first character (`EQOA_CHAR` another); the lanes porting CLIENT's
//! windows replace this file.

use super::{Ctx, Entered, Next, Shared, res::Res};
use net::session::{Event, Session};
use vi::ui::wide;

/// What `VIClient::LoadWorld` ("PreGame Load") reads after the base files.
pub const FILES: &[&str] = &[
    "DATA2/PREGAME.CSF",
    "DATA2/ARIAL.CSF",
    "DATA2/ARENA.CSF",
    "DATA2/ITEM.CSF",
    "DATA2/CHARCUST.CSF",
    "DATA2/CHARFACE.CSF",
    "DATA2/CHARSEL1.CSF",
    "DATA2/CHARSEL2.CSF",
    "DATA2/CHARSEL3.CSF",
    "DATA2/CHARSEL4.CSF",
    "DATA2/DEBUG.CSF",
    "DATA2/FX.CSF",
    "DATA2/SKY.CSF",
];

/// The 3D a window wants drawn under its later 2D (character select's and
/// creation's scene): filled in by the 3D stage.
pub struct Scene;

#[derive(Default)]
pub struct Client {
    session: Option<Session>,
    text: String,
    failed: bool,
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    /// `VIClient::Connect`: the session to the server's login endpoint.
    pub fn start(&mut self, _res: &mut Res, shared: &mut Shared) {
        self.failed = false;
        self.text = "Connecting".into();
        let (Some(host), Some(login)) = (shared.server.clone(), shared.login.clone()) else {
            self.text = "not logged in".into();
            self.failed = true;
            return;
        };
        let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), super::port("EQOA_GAME_PORT", super::GAME_PORT))).ok().and_then(|mut a| a.next());
        match addr.map(|a| Session::connect(a, &login.user, login.token)) {
            Some(Ok(s)) => self.session = Some(s),
            Some(Err(e)) => {
                self.text = e.to_string();
                self.failed = true;
            }
            None => {
                self.text = format!("{host}: no address");
                self.failed = true;
            }
        }
    }

    pub fn stop(&mut self) {
        self.session = None;
    }

    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        cx.vui.draw_centered_string_ex(cx.r, &wide(&self.text), 320, 224, [1.0, 1.0, 1.0, 1.0], 0, true);
        let Some(s) = self.session.as_mut() else { return Next::Stay };
        let events = s.pump();
        for (k, e) in events.iter().enumerate() {
            match e {
                Event::ShardList(list) => {
                    self.text = format!("{} servers", list.len());
                    if let Some(sh) = list.first() {
                        let mut ep = sh.endpoint;
                        if std::env::var_os("EQOA_GAME_PORT").is_some() {
                            ep = (ep & !0xFFFF_0000) | (super::port("EQOA_GAME_PORT", super::GAME_PORT) as u64) << 16;
                        }
                        s.connect_shard(ep);
                    }
                }
                Event::Characters(list) => {
                    self.text = format!("{} characters", list.len());
                    let want = std::env::var("EQOA_CHAR").ok();
                    if let Some(c) = list.iter().find(|c| want.as_deref() == Some(c.name.as_str())).or(list.first()) {
                        s.select_character(&c.clone());
                    }
                }
                Event::WorldEntry { player, dump, .. } => {
                    let (player, dump) = (player.clone(), dump.clone());
                    let mut s = self.session.take().unwrap();
                    s.held = events[k + 1..].to_vec();
                    return Next::World(Box::new(Entered { session: s, player, dump }));
                }
                Event::Disconnected(r) => {
                    self.text = format!("disconnected ({r})");
                    self.session = None;
                    return Next::Stay;
                }
                _ => {}
            }
        }
        Next::Stay
    }

    pub fn key(&mut self, key: i32, down: bool) {
        let _ = (key, down, self.failed);
    }

    pub fn point(&mut self, _p: [f32; 2]) -> Vec<i32> {
        Vec::new()
    }

    pub fn typing(&self) -> bool {
        false
    }

    pub fn scene(&self) -> Option<&Scene> {
        None
    }
}
