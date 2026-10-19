//! The sockets a page cannot open, held by a relay (go-eqoa-server's
//! `cmd/wsrelay`, or eqoa-port's `tools/wsrelay.py`: the same protocol), each
//! one WebSocket:
//!
//! - `/udp`: the game's DRDP. The relay answers `ok <addr> <port>` (its own
//!   address toward the server, which is what the server will see), then
//!   every binary message is one datagram behind a 6-byte header, the far
//!   end's IPv4 address and its port (big-endian).
//! - `/tcp?host=A.B.C.D&port=N`: the Station login. `ok ...` or `err ...`,
//!   then the stream's bytes.

use super::sleep;
use net::auth::{Exchange, Login, Step};
use net::session::Datagrams;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{BinaryType, MessageEvent, WebSocket};

enum Msg {
    Text(String),
    Bin(Vec<u8>),
    Closed,
}

struct Socket {
    ws: WebSocket,
    inbox: Rc<RefCell<VecDeque<Msg>>>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(web_sys::Event)>,
}

impl Socket {
    fn open(url: &str) -> Result<Socket, String> {
        let ws = WebSocket::new(url).map_err(|e| format!("{url}: {e:?}"))?;
        ws.set_binary_type(BinaryType::Arraybuffer);
        let inbox = Rc::new(RefCell::new(VecDeque::new()));
        let q = inbox.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let d = e.data();
            if let Some(t) = d.as_string() {
                q.borrow_mut().push_back(Msg::Text(t));
            } else if let Ok(b) = d.dyn_into::<js_sys::ArrayBuffer>() {
                q.borrow_mut().push_back(Msg::Bin(js_sys::Uint8Array::new(&b).to_vec()));
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        let q = inbox.clone();
        let on_close = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| q.borrow_mut().push_back(Msg::Closed));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        Ok(Socket { ws, inbox, _on_message: on_message, _on_close: on_close })
    }

    /// The next message, waiting up to `ms`.
    async fn next(&self, ms: f64) -> Option<Msg> {
        let start = web_time::Instant::now();
        loop {
            if let Some(m) = self.inbox.borrow_mut().pop_front() {
                return Some(m);
            }
            if start.elapsed().as_secs_f64() * 1000.0 > ms {
                return None;
            }
            sleep(4).await;
        }
    }

    /// The relay's first word: `ok <addr> <port>`.
    async fn ready(&self) -> Result<([u8; 4], u16), String> {
        match self.next(10_000.0).await {
            Some(Msg::Text(t)) => {
                let w: Vec<&str> = t.split_whitespace().collect();
                match w.as_slice() {
                    ["ok", a, p] => Ok((parse_ip(a).ok_or_else(|| format!("relay: bad address {a}"))?, p.parse().map_err(|_| format!("relay: bad port {p}"))?)),
                    _ => Err(format!("relay: {t}")),
                }
            }
            Some(Msg::Closed) => Err("relay: closed before it answered (is it running, and does it allow this page?)".into()),
            _ => Err("relay: no answer in 10 s".into()),
        }
    }

    fn send(&self, b: &[u8]) {
        let _ = self.ws.send_with_u8_array(b);
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

pub fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let v: Vec<u8> = s.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    v.try_into().ok()
}

/// The game's datagrams over `/udp`.
pub struct RelayUdp {
    sock: Socket,
}

impl RelayUdp {
    /// Opens the socket; returns it and the address the server will see.
    pub async fn open(relay: &str) -> Result<(RelayUdp, ([u8; 4], u16)), String> {
        let sock = Socket::open(&format!("{relay}/udp"))?;
        let local = sock.ready().await?;
        Ok((RelayUdp { sock }, local))
    }
}

impl Datagrams for RelayUdp {
    fn send_to(&mut self, data: &[u8], ip: [u8; 4], port: u16) {
        let mut b = Vec::with_capacity(6 + data.len());
        b.extend_from_slice(&ip);
        b.extend_from_slice(&port.to_be_bytes());
        b.extend_from_slice(data);
        self.sock.send(&b);
    }

    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
        loop {
            match self.sock.inbox.borrow_mut().pop_front()? {
                Msg::Bin(b) if b.len() >= 6 => return Some((b[6..].to_vec(), [b[0], b[1], b[2], b[3]], u16::from_be_bytes([b[4], b[5]]))),
                _ => continue,
            }
        }
    }
}

/// The Station login over `/tcp`.
pub async fn login(relay: &str, host: [u8; 4], port: u16, user: &str, password: &str) -> Result<Login, String> {
    let [a, b, c, d] = host;
    let sock = Socket::open(&format!("{relay}/tcp?host={a}.{b}.{c}.{d}&port={port}"))?;
    sock.ready().await?;
    let mut x = Exchange::new(user, password);
    loop {
        match sock.next(10_000.0).await {
            Some(Msg::Bin(data)) => {
                let mut step = x.feed(&data).map_err(|e| e.to_string())?;
                loop {
                    match step {
                        Step::Send(r) => {
                            sock.send(&r);
                            step = x.feed(&[]).map_err(|e| e.to_string())?;
                        }
                        Step::Wait => break,
                        Step::Done(l) => return Ok(l),
                    }
                }
            }
            Some(Msg::Text(_)) => {}
            Some(Msg::Closed) => return Err("auth: the server closed the connection".into()),
            None => return Err("auth: no answer in 10 s".into()),
        }
    }
}
