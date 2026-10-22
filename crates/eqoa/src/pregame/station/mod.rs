//! STATION (`station_main`, `VIStation`, `VIWndGenericRenderer`): STATION.PAK's
//! pages, the Station login. STAND-IN: logs in with `EQOA_USER` and
//! `EQOA_PASSWORD`; the lane porting STATION replaces this file.

use super::{Ctx, Login, Next, Shared, res::Res};
use vi::ui::wide;

/// What `VIStation::Init` reads after the base files.
pub const FILES: &[&str] = &["DATA2/STATUI.CSF", "DATA2/PREGAME.CSF", "STATION.PAK"];

type Answer = std::sync::mpsc::Receiver<Result<Login, String>>;

#[derive(Default)]
pub struct Station {
    asked: Option<Answer>,
    failed: Option<String>,
}

impl Station {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, _res: &mut Res, shared: &mut Shared) {
        self.failed = None;
        let (Some(host), Ok(user), Ok(pass)) = (shared.server.clone(), std::env::var("EQOA_USER"), std::env::var("EQOA_PASSWORD")) else {
            self.failed = Some("set EQOA_USER and EQOA_PASSWORD (stand-in)".into());
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = net::auth::login((host.as_str(), super::port("EQOA_AUTH_PORT", super::AUTH_PORT)), &user, &pass)
                .map(|l| Login { user: user.clone(), token: l.token, account: l.account, status: l.status, features: l.game_features })
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.asked = Some(rx);
    }

    pub fn stop(&mut self) {
        self.asked = None;
    }

    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        let text = self.failed.clone().unwrap_or_else(|| "Logging in".into());
        cx.vui.draw_centered_string_ex(cx.r, &wide(&text), 320, 224, [1.0, 1.0, 1.0, 1.0], 0, true);
        if let Some(rx) = &self.asked
            && let Ok(r) = rx.try_recv()
        {
            self.asked = None;
            match r {
                Ok(l) => {
                    cx.shared.login = Some(l);
                    return Next::Client;
                }
                Err(e) => self.failed = Some(e),
            }
        }
        Next::Stay
    }

    pub fn key(&mut self, _key: i32, _down: bool) {}

    pub fn point(&mut self, _p: [f32; 2]) -> Vec<i32> {
        Vec::new()
    }

    pub fn typing(&self) -> bool {
        false
    }
}
