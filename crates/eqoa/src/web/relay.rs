//! The sockets a page cannot open, held by a relay (go-eqoa-server's
//! `cmd/wsrelay`, or eqoa-port's `tools/wsrelay.py`: the same protocol), each
//! one WebSocket:
//!
//! - `/udp`: the game's DRDP. The relay answers `ok <addr> <port>` (its own
//!   address toward the server, which is what the server will see), then
//!   every binary message is one datagram behind a 6-byte header, the far
//!   end's IPv4 address and its port (big-endian).
//! - `/tcp?host=A.B.C.D&port=N`: the Station login, and the news page's
//!   HTTP. `ok ...` or `err ...`, then the stream's bytes.
//!
//! A server named by host name is resolved by the relay first
//! (`GET /resolve?name=`): it only reaches addresses.

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

/// The game's `/udp` socket as the log tells it at its close: when it
/// opened, when the last datagram came in, and how it closed. Nothing else
/// says the link went quiet: the game only times out (F3 20261025-211736,
/// typed commands that never left a link dead for 37 s).
#[derive(Default)]
struct Game {
    opened: Option<web_time::Instant>,
    last_in: Option<web_time::Instant>,
    /// when, the close code, and whether it closed cleanly
    closed: Option<(web_time::Instant, u16, bool)>,
    ws: Option<WebSocket>,
}

thread_local! {
    static GAME: RefCell<Game> = RefCell::new(Game::default());
}

/// Seconds since `t`.
fn ago(t: web_time::Instant) -> f64 {
    t.elapsed().as_secs_f64()
}

impl Socket {
    fn open(url: &str) -> Result<Socket, String> {
        Self::open_as(url, false)
    }

    /// The socket; `game` the game's datagrams, kept for `describe`.
    fn open_as(url: &str, game: bool) -> Result<Socket, String> {
        let ws = WebSocket::new(url).map_err(|e| format!("{url}: {e:?}"))?;
        ws.set_binary_type(BinaryType::Arraybuffer);
        let inbox = Rc::new(RefCell::new(VecDeque::new()));
        let q = inbox.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if game {
                GAME.with(|g| g.borrow_mut().last_in = Some(web_time::Instant::now()));
            }
            let d = e.data();
            if let Some(t) = d.as_string() {
                q.borrow_mut().push_back(Msg::Text(t));
            } else if let Ok(b) = d.dyn_into::<js_sys::ArrayBuffer>() {
                q.borrow_mut().push_back(Msg::Bin(js_sys::Uint8Array::new(&b).to_vec()));
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        let q = inbox.clone();
        let on_close = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            if game {
                closed(e.dyn_ref::<web_sys::CloseEvent>());
            }
            q.borrow_mut().push_back(Msg::Closed)
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        if game {
            GAME.with(|g| *g.borrow_mut() = Game { opened: Some(web_time::Instant::now()), ws: Some(ws.clone()), ..Game::default() });
        }
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

/// The game's socket closed under it (one let go by the game itself closes
/// with its handlers gone, `Drop`): kept for `describe`, and said, since the
/// game takes it only as a link gone quiet until it times out.
fn closed(e: Option<&web_sys::CloseEvent>) {
    let (code, clean, reason) = e.map_or((0, false, String::new()), |e| (e.code(), e.was_clean(), e.reason()));
    let line = GAME.with(|g| {
        let mut g = g.borrow_mut();
        g.closed = Some((web_time::Instant::now(), code, clean));
        format!(
            "relay: the game's connection closed after {:.1} s (code {code}{}{}), the last datagram in {}: nothing more reaches the server",
            g.opened.map_or(0.0, ago),
            if clean { "" } else { ", not cleanly" },
            if reason.is_empty() { String::new() } else { format!(", \"{reason}\"") },
            g.last_in.map_or("none".to_string(), |t| format!("{:.1} s before", ago(t))),
        )
    });
    crate::say(&line);
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
        let sock = Socket::open_as(&format!("{relay}/udp"), true)?;
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

/// `host`'s IPv4 address: itself when it is one, else as the relay
/// resolves it (`GET /resolve?name=`, the relay's own resolver).
pub async fn resolve(relay: &str, host: &str) -> Result<[u8; 4], String> {
    if let Some(ip) = parse_ip(host) {
        return Ok(ip);
    }
    let base = relay.replacen("wss://", "https://", 1).replacen("ws://", "http://", 1);
    let url = format!("{base}/resolve?name={}", js_sys::encode_uri_component(host));
    let w = web_sys::window().ok_or("no window")?;
    let r = wasm_bindgen_futures::JsFuture::from(w.fetch_with_str(&url)).await.map_err(|e| format!("{url}: {e:?}"))?;
    let r: web_sys::Response = r.dyn_into().map_err(|_| "not a response")?;
    if !r.ok() {
        return Err(format!("the relay could not resolve {host} ({})", r.status()));
    }
    let t = wasm_bindgen_futures::JsFuture::from(r.text().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{e:?}"))?;
    let t = t.as_string().unwrap_or_default();
    parse_ip(t.trim()).ok_or_else(|| format!("the relay resolved {host} to {t:?}"))
}

/// A TCP stream through the relay's `/tcp`, as the pregame's
/// `link::Carrier`: the host resolved, the socket opened and its `ok`
/// waited for on a task of its own; what is sent before then waits.
pub struct RelayTcp {
    state: Rc<RefCell<TcpState>>,
}

#[derive(Default)]
struct TcpState {
    sock: Option<Socket>,
    waiting: Vec<Vec<u8>>,
    failed: Option<String>,
}

impl RelayTcp {
    pub fn open(relay: &str, host: &str, port: u16) -> RelayTcp {
        let state: Rc<RefCell<TcpState>> = Default::default();
        let (st, relay, host) = (state.clone(), relay.to_string(), host.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            let r = async {
                let [a, b, c, d] = resolve(&relay, &host).await?;
                let sock = Socket::open(&format!("{relay}/tcp?host={a}.{b}.{c}.{d}&port={port}"))?;
                sock.ready().await?;
                Ok::<_, String>(sock)
            }
            .await;
            let mut s = st.borrow_mut();
            match r {
                Ok(sock) => {
                    for b in s.waiting.drain(..) {
                        sock.send(&b);
                    }
                    s.sock = Some(sock);
                }
                Err(e) => s.failed = Some(e),
            }
        });
        RelayTcp { state }
    }
}

impl crate::pregame::link::Carrier for RelayTcp {
    fn send(&mut self, b: &[u8]) {
        let mut s = self.state.borrow_mut();
        match &s.sock {
            Some(sock) => sock.send(b),
            None => s.waiting.push(b.to_vec()),
        }
    }

    fn poll(&mut self) -> Option<crate::pregame::link::Got> {
        use crate::pregame::link::Got;
        let mut s = self.state.borrow_mut();
        if let Some(e) = s.failed.take() {
            return Some(Got::Failed(e));
        }
        let sock = s.sock.as_ref()?;
        loop {
            match sock.inbox.borrow_mut().pop_front()? {
                Msg::Bin(b) => return Some(Got::Data(b)),
                Msg::Text(_) => continue,
                Msg::Closed => return Some(Got::Closed),
            }
        }
    }
}

/// The game session through the relay's `/udp`, as the pregame's
/// `link::session`.
pub fn session(relay: &str, host: &str, port: u16, user: &str, token: [u8; 32]) -> crate::pregame::link::Later<net::session::Session> {
    let slot: Rc<RefCell<Option<Result<net::session::Session, String>>>> = Default::default();
    let (out, relay, host, user) = (slot.clone(), relay.to_string(), host.to_string(), user.to_string());
    wasm_bindgen_futures::spawn_local(async move {
        let r = async {
            let ip = resolve(&relay, &host).await?;
            let (sock, local) = RelayUdp::open(&relay).await?;
            net::session::Session::connect_with(Box::new(sock), local, (ip, port), &user, token).ok_or_else(|| "could not open the game connection".to_string())
        }
        .await;
        *out.borrow_mut() = Some(r);
    });
    crate::pregame::link::Later::Slot(slot)
}
