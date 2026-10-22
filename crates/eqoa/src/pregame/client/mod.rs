//! CLIENT's windows before the world (`VIClient::InitBody`'s): the server
//! list (`VIWndServerSelect`), character select (`VIWndCharacterSelect`) and
//! creation (`VIWndCharCreateBlank` and its pages). STAND-IN: the first shard
//! and the first character (`EQOA_CHAR` another); the lanes porting CLIENT's
//! windows replace this file.

pub mod create;

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
    stage: Option<crate::pregame::stage::Stage>,
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

    /// Back from the world: the logout (None) or the connection lost.
    pub fn back_from_world(&mut self, res: &mut Res, shared: &mut Shared, disconnect: Option<i32>) {
        let _ = disconnect;
        self.start(res, shared);
    }

    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        cx.vui.draw_centered_string_ex(cx.r, &wide(&self.text), 320, 224, [1.0, 1.0, 1.0, 1.0], 0, true);
        // the chosen character in character select's viewport, then played
        if let Some((c, at)) = self.chosen.clone() {
            if self.figure.is_none()
                && let Some(g) = cx.gpu.as_mut()
            {
                // the Human's set (`GetRace(0)`'s select place), facing the camera's committed heading
                let at = [0.0, 0.0, -26.0];
                let look = vi::cust::Look { hair: [c.hair_color as u8, c.hair_length as u8, c.hair_style as u8], face: c.face as u8, robe: -1, ..Default::default() };
                self.figure = crate::pregame::figure::Figure::new(&cx.res.lib, g, c.model as u32, look, [0; 3], at, [0.0; 3], cx.now);
                self.stage = crate::pregame::stage::Stage::new(&cx.res.lib, g);
                if let Some(st) = self.stage.as_mut() {
                    st.aim(at, [0.0; 3]);
                }
            }
            if let (Some(f), Some(g)) = (self.figure.as_mut(), cx.gpu.as_ref()) {
                f.pose(g, cx.now);
            }
            // `ApplyCamera` with no race camera: 1.1, 1.7, 40 degrees, 0.75; `OrbitRight(0.3)`
            if let Some(st) = self.stage.as_mut() {
                let f = vi::fpu::F::<vi::fpu::Ee>::new;
                st.cam.target_height = f(1.1);
                st.cam.look_at_height_above = f(1.7);
                st.cam.set_def_orbit_pitch_angle(f(0.698_131_74));
                st.cam.set_orbit_distance(f(0.75));
                st.cam.orbit_right(f(0.3));
                st.process(cx.now);
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
        let st = self.stage.as_ref()?;
        Some(super::Scene { view: st.view(), viewport: crate::pregame::stage::VIEWPORT, models: st.models().chain(f.models()).collect() })
    }
}
