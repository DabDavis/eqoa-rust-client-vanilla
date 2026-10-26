//! `run` played over what was fetched, for a platform whose fetches cannot
//! wait inside it (a page, `patcher::web`): a path not fetched yet is noted
//! and answered empty, the platform fetches the noted paths, and `run` is
//! played again until it asks for nothing new (the manifest, then the files
//! it needs). Only that last play counts: what an earlier one wrote is
//! thrown away with its folder.

use super::http::Error;
use super::{Config, Folder, Shared, Status};
use std::collections::HashMap;

/// Each path fetched (as the request names it, without its leading slash):
/// its status and body, or why none came.
pub type Fetched = HashMap<String, Result<(u16, Vec<u8>), String>>;

/// The wire over what was fetched.
struct Replay<'a> {
    got: &'a Fetched,
    /// paths asked for that are not fetched yet
    missing: Vec<String>,
    /// the manifest's path: a site that answers it with an error (its own
    /// 404 page) serves no patch, so the server is unreachable as a patch
    /// port that never answers is, and its folder is played as it stands;
    /// natively a patch port that answers 404 is a failed patch, but a
    /// server's web site need not serve its patch at all (eqoaclassic.com's
    /// does not), and a failed patch puts up the box whose only way on is
    /// Restart, at every Play
    manifest: &'a str,
}

impl super::Wire for Replay<'_> {
    fn exchange(&mut self, _host: &str, _port: u16, request: &[u8], piece: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), Error> {
        // "GET /<path> HTTP/1.1": the host and port are the manifest's,
        // which the platform does not fetch from
        let req = String::from_utf8_lossy(request);
        let path = req.split(' ').nth(1).unwrap_or("/").trim_start_matches('/').to_string();
        let (status, body) = match self.got.get(&path) {
            Some(Ok((s, _))) if path == self.manifest && !(200..300).contains(s) => {
                return Err(Error::new(5, format!("{path}: the site answers {s}: it serves no patch")));
            }
            Some(Ok((s, b))) => (*s, b.as_slice()),
            Some(Err(e)) => return Err(Error::new(5, e.clone())),
            None => {
                if !self.missing.contains(&path) {
                    self.missing.push(path);
                }
                (200, &[][..])
            }
        };
        let mut reply = format!("HTTP/1.1 {status} \r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        reply.extend_from_slice(body);
        piece(&reply);
        Ok(())
    }
}

/// One play of `run` into `folder` over `got`: its status, the paths it
/// asked for that are not fetched yet (none: this play stands), and the
/// lines it would have said, only when it stands: a play thrown away tells
/// nothing, its errors being the empty answers to what was not fetched yet
/// ("Document of the wrong type" for the manifest), not the patch's.
pub fn play(cfg: &Config, got: &Fetched, folder: &mut dyn Folder) -> (Status, Vec<String>, Vec<String>) {
    let mut wire = Replay { got, missing: Vec::new(), manifest: cfg.manifest.trim_start_matches('/') };
    let st: Shared = Default::default();
    super::HELD.with(|h| *h.borrow_mut() = Some(Vec::new()));
    super::run(cfg, &mut wire, folder, &st);
    let said = super::HELD.with(|h| h.borrow_mut().take()).unwrap_or_default();
    let s = st.lock().unwrap().clone();
    let said = if wire.missing.is_empty() { said } else { Vec::new() };
    (s, wire.missing, said)
}
