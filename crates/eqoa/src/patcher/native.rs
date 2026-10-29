//! The patcher's platform natively: a socket per fetch (`Tcp`, `VUrl`'s
//! connection: 10 s to connect, each receive waiting 100 ms, given up after
//! 30 s with no data, where a PS2 would hang) and the server's folder on
//! disk (`Disk`). `pregame::link::patch` runs `patcher::run` with them on a
//! thread of its own.

use super::http::Error;
use super::image::{self, Images};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// A TCP connection per fetch.
pub struct Tcp;

impl super::Wire for Tcp {
    fn exchange(&mut self, host: &str, port: u16, request: &[u8], piece: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), Error> {
        use std::net::ToSocketAddrs;
        let addr = (host, port).to_socket_addrs().ok().and_then(net::addr::pick).ok_or_else(|| Error::new(3, format!("Cannot resolve server {host}")))?;
        let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|_| Error::new(5, format!("Could not connect to {host}")))?;
        // sceInetSend: the PS2 does not look at the result
        let _ = s.write_all(request);
        let _ = s.set_read_timeout(Some(Duration::from_millis(100)));
        let mut buf = [0u8; 1460];
        let mut last = Instant::now();
        loop {
            match s.read(&mut buf) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    last = Instant::now();
                    if piece(&buf[..n]) {
                        return Ok(());
                    }
                }
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                    if last.elapsed() > Duration::from_secs(30) {
                        return Err(Error::new(5, format!("No data from {host} for 30 s")));
                    }
                }
                Err(e) => return Err(Error::new(6, format!("Error, code = {e}"))),
            }
        }
    }
}

/// The server's folder.
pub struct Disk(pub std::path::PathBuf);

impl super::Folder for Disk {
    fn read(&mut self, progress: &mut dyn FnMut(f32)) -> Result<Images, String> {
        image::read_folder(&self.0, progress)
    }

    fn write(&mut self, images: &Images, progress: &mut dyn FnMut(f32)) -> Result<bool, String> {
        image::write_folder(&self.0, images, progress)
    }

    fn record(&mut self, r: &crate::servers::Record) {
        if let Err(e) = std::fs::create_dir_all(&self.0).map_err(|e| e.to_string()).and_then(|_| crate::servers::write_record(&self.0, r)) {
            crate::say(&format!("patcher: the record was not kept: {e}"));
        }
    }
}
