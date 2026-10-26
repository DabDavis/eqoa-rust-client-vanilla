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
}

impl super::Wire for Replay<'_> {
    fn exchange(&mut self, _host: &str, _port: u16, request: &[u8], piece: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), Error> {
        // "GET /<path> HTTP/1.1": the host and port are the manifest's,
        // which the platform does not fetch from
        let req = String::from_utf8_lossy(request);
        let path = req.split(' ').nth(1).unwrap_or("/").trim_start_matches('/').to_string();
        let (status, body) = match self.got.get(&path) {
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

/// One play of `run` into `folder` over `got`: its status, and the paths it
/// asked for that are not fetched yet (none: this play stands).
pub fn play(cfg: &Config, got: &Fetched, folder: &mut dyn Folder) -> (Status, Vec<String>) {
    let mut wire = Replay { got, missing: Vec::new() };
    let st: Shared = Default::default();
    super::run(cfg, &mut wire, folder, &st);
    let s = st.lock().unwrap().clone();
    (s, wire.missing)
}
