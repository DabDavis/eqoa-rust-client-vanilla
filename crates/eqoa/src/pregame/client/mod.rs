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

#[derive(Default)]
pub struct Client {
    session: Option<Session>,
    text: String,
    failed: bool,
    /// the character about to be played, shown for two seconds first (the
    /// 3D path's check), and when it was chosen
    chosen: Option<(net::session::Character, i64)>,
    figure: Option<crate::pregame::figure::Figure>,
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
        // the chosen character in character select's viewport, then played
        if let Some((c, at)) = self.chosen.clone() {
            if self.figure.is_none()
                && let Some(g) = cx.gpu.as_mut()
            {
                let look = vi::cust::Look { hair: [c.hair_color as u8, c.hair_length as u8, c.hair_style as u8], face: c.face as u8, robe: -1, ..Default::default() };
                self.figure = crate::pregame::figure::Figure::new(&cx.res.lib, g, c.model as u32, look, [0; 3], [0.0; 3], [0.0; 3], cx.now);
            }
            if let (Some(f), Some(g)) = (self.figure.as_mut(), cx.gpu.as_ref()) {
                f.pose(g, cx.now);
            }
            cx.r.mark_split();
            if cx.now - at >= 2000
                && let Some(s) = self.session.as_mut()
            {
                self.chosen = None;
                s.select_character(&c);
            }
        }
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
                        self.chosen = Some((c.clone(), cx.now));
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

    /// The 3D drawn this frame (character select's preview, creation's model).
    pub fn scene(&self) -> Option<super::Scene<'_>> {
        let f = self.figure.as_ref()?;
        // a stand-in camera: 2.4 ahead of the figure at 1.1 up, looking at its chest
        let (eye, at) = (glam::Vec3::new(0.0, 1.3, 2.6), glam::Vec3::new(0.0, 1.0, 0.0));
        let aspect = (620.0 - 358.0 + 1.0) / (396.0 - 48.0 + 1.0);
        let proj = glam::camera::rh::proj::directx::perspective(55f32.to_radians() / (10.0 / 7.0), aspect, 0.1, 900.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, at, glam::Vec3::Y);
        let mut v = render::View::daylight(proj * view);
        v.eye = eye;
        Some(super::Scene { view: v, viewport: [358, 48, 620, 396], models: f.models().collect() })
    }
}
