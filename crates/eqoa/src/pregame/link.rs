//! The pregame's network, the same on every platform: what finishes later
//! is polled once a frame, natively done on threads of its own, in a page
//! by the page's own carriers (the relay's WebSockets, `set_platform`).
//!
//! - [`Stream`]: a TCP connection's bytes both ways (STATION's connection to
//!   the Station server, `VIStationAPI`'s; the news page's HTTP GET).
//! - [`session`]: the game session (`VIClient::Connect`) on a UDP socket.
//!
//! (`::net` is the crate: this module is `link`, not to shadow it.)

use std::cell::RefCell;
use std::rc::Rc;

/// Something that comes later: polled until it gives its result, once.
pub enum Later<T> {
    /// a page's: its carrier fills the slot
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Slot(Rc<RefCell<Option<Result<T, String>>>>),
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Receiver<Result<T, String>>),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Done(Option<Result<T, String>>),
}

impl<T> Later<T> {
    pub fn poll(&mut self) -> Option<Result<T, String>> {
        match self {
            Later::Slot(s) => s.borrow_mut().take(),
            #[cfg(not(target_arch = "wasm32"))]
            Later::Thread(rx) => match rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the network thread stopped".into())),
            },
            Later::Done(r) => r.take(),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn ready(r: Result<T, String>) -> Self {
        Later::Done(Some(r))
    }
}

/// What a stream says when polled.
#[derive(Debug, PartialEq)]
pub enum Got {
    /// bytes arrived
    Data(Vec<u8>),
    /// the far end closed it (an HTTP reply's end)
    Closed,
    /// it could not connect, or broke
    Failed(String),
}

/// A TCP connection to `host:port`: bytes sent as they are handed over,
/// bytes read as they come. A send before the connection is up waits for it.
pub struct Stream {
    inner: Box<dyn Carrier>,
}

/// The platform's half of a stream.
pub trait Carrier {
    fn send(&mut self, b: &[u8]);
    fn poll(&mut self) -> Option<Got>;
}

/// Makes a page's carriers: a stream to (host, port), and the session.
pub struct Platform {
    pub stream: Box<dyn Fn(&str, u16) -> Box<dyn Carrier>>,
    pub session: Box<dyn Fn(&str, u16, &str, [u8; 32]) -> Later<::net::session::Session>>,
}

thread_local! {
    static PLATFORM: RefCell<Option<Platform>> = const { RefCell::new(None) };
}

/// Makes `p` the way the pregame reaches the server (a page's relay).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_platform(p: Platform) {
    PLATFORM.with(|c| *c.borrow_mut() = Some(p));
}

impl Stream {
    pub fn connect(host: &str, port: u16) -> Stream {
        if let Some(c) = PLATFORM.with(|p| p.borrow().as_ref().map(|p| (p.stream)(host, port))) {
            return Stream { inner: c };
        }
        #[cfg(not(target_arch = "wasm32"))]
        return Stream { inner: Box::new(native::Tcp::connect(host, port)) };
        #[cfg(target_arch = "wasm32")]
        Stream { inner: Box::new(Refused(Some(format!("no way to reach {host}:{port}")))) }
    }

    pub fn send(&mut self, b: &[u8]) {
        self.inner.send(b);
    }

    /// What came since the last poll, one piece at a time (None: nothing yet).
    pub fn poll(&mut self) -> Option<Got> {
        self.inner.poll()
    }

    /// A stream from a carrier of the caller's (a test's).
    #[cfg(test)]
    pub fn from_carrier(c: Box<dyn Carrier>) -> Stream {
        Stream { inner: c }
    }
}

/// A carrier that only says why there is none.
#[cfg(target_arch = "wasm32")]
pub struct Refused(pub Option<String>);

#[cfg(target_arch = "wasm32")]
impl Carrier for Refused {
    fn send(&mut self, _: &[u8]) {}
    fn poll(&mut self) -> Option<Got> {
        self.0.take().map(Got::Failed)
    }
}

/// The game session opening (`VIClient::Connect`: the hello and 0x0904 to
/// the login endpoint): natively the server's name resolved on a thread and
/// the socket made here; in a page the platform's.
pub enum Opening {
    #[cfg(not(target_arch = "wasm32"))]
    Resolving { addr: Later<std::net::SocketAddr>, user: String, token: [u8; 32] },
    Platform(Later<::net::session::Session>),
}

impl Opening {
    pub fn poll(&mut self) -> Option<Result<::net::session::Session, String>> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Opening::Resolving { addr, user, token } => addr.poll().map(|r| r.and_then(|a| ::net::session::Session::connect(a, user, *token).map_err(|e| e.to_string()))),
            Opening::Platform(l) => l.poll(),
        }
    }
}

/// The game session at `host:port` for the account.
pub fn session(host: &str, port: u16, user: &str, token: [u8; 32]) -> Opening {
    if let Some(l) = PLATFORM.with(|p| p.borrow().as_ref().map(|p| (p.session)(host, port, user, token))) {
        return Opening::Platform(l);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let h = host.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // its IPv4 address when it has one (`net::addr`)
            let r = net::addr::resolve(&h, port).map_err(|e| format!("{h}: {e}"));
            let _ = tx.send(r);
        });
        Opening::Resolving { addr: Later::Thread(rx), user: user.to_string(), token }
    }
    #[cfg(target_arch = "wasm32")]
    Opening::Platform(Later::ready(Err(format!("no way to reach {host}:{port}"))))
}

/// SETUP's patcher (`patcher::run`) against the player's server, into that
/// server's own folder (`servers::folder`): natively on a thread of its own
/// (`patcher::native`), the window reading the status it returns each
/// frame. None where it cannot run: in a page (a page cannot reach the
/// patch port), and in a run that keeps nothing (the test commands).
pub fn patch(host: &str, manifest: &str) -> Option<crate::patcher::Shared> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !crate::servers::keeps() {
            return None;
        }
        let st: crate::patcher::Shared = Default::default();
        let cfg = crate::patcher::Config { host: host.to_string(), port: super::port("EQOA_PATCH_PORT", crate::patcher::PORT), manifest: manifest.to_string(), frame_ms: 33 };
        let dir = crate::servers::folder(host);
        let s = st.clone();
        std::thread::spawn(move || {
            crate::patcher::run(&cfg, &mut crate::patcher::native::Tcp, &mut crate::patcher::native::Disk(dir), &s);
        });
        Some(st)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (host, manifest);
        None
    }
}

/// An HTTP GET of `path` at `host:port` over a [`Stream`] (the news page,
/// `VUrl::GetPoll`): the reply's body once the server closes, or why not.
pub struct HttpGet {
    stream: Stream,
    reply: Vec<u8>,
    done: Option<Result<Vec<u8>, String>>,
}

impl HttpGet {
    pub fn start(host: &str, port: u16, path: &str) -> HttpGet {
        let mut stream = Stream::connect(host, port);
        stream.send(format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes());
        HttpGet { stream, reply: Vec::new(), done: None }
    }

    /// The body, once the reply is whole (None until then).
    pub fn poll(&mut self) -> Option<Result<Vec<u8>, String>> {
        while self.done.is_none() {
            match self.stream.poll() {
                Some(Got::Data(b)) => self.reply.extend(b),
                Some(Got::Closed) => self.done = Some(body_of(&self.reply)),
                Some(Got::Failed(e)) => self.done = Some(Err(e)),
                None => break,
            }
        }
        self.done.take()
    }
}

/// An HTTP reply's body (a 200's), de-chunked when it was sent so.
pub fn body_of(reply: &[u8]) -> Result<Vec<u8>, String> {
    let end = reply.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no HTTP header")?;
    let head = String::from_utf8_lossy(&reply[..end]).to_ascii_lowercase();
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("HTTP {status}"));
    }
    let body = &reply[end + 4..];
    if !head.contains("transfer-encoding: chunked") {
        return Ok(body.to_vec());
    }
    let (mut out, mut at) = (Vec::new(), 0);
    loop {
        let line_end = body[at..].windows(2).position(|w| w == b"\r\n").ok_or("a chunk cut short")? + at;
        let size = usize::from_str_radix(String::from_utf8_lossy(&body[at..line_end]).split(';').next().unwrap_or("").trim(), 16).map_err(|e| e.to_string())?;
        at = line_end + 2;
        if size == 0 {
            return Ok(out);
        }
        out.extend(body.get(at..at + size).ok_or("a chunk cut short")?);
        at += size + 2;
    }
}

/// A queue of what a thread hands the frame.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Carrier, Got};
    use std::io::{Read, Write};
    use std::sync::mpsc;

    pub struct Tcp {
        to: Option<mpsc::Sender<Vec<u8>>>,
        from: mpsc::Receiver<Got>,
    }

    impl Tcp {
        /// Connects on a thread; another reads.
        pub fn connect(host: &str, port: u16) -> Tcp {
            let (to, rx_out) = mpsc::channel::<Vec<u8>>();
            let (tx_in, from) = mpsc::channel::<Got>();
            let host = host.to_string();
            std::thread::spawn(move || {
                let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)).ok().and_then(net::addr::pick);
                let Some(addr) = addr else {
                    let _ = tx_in.send(Got::Failed(format!("{host}: no address")));
                    return;
                };
                let s = match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(10)) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx_in.send(Got::Failed(format!("{host}:{port}: {e}")));
                        return;
                    }
                };
                let mut reader = match s.try_clone() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx_in.send(Got::Failed(e.to_string()));
                        return;
                    }
                };
                let tx = tx_in.clone();
                std::thread::spawn(move || {
                    let mut buf = vec![0u8; 16384];
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => {
                                let _ = tx.send(Got::Closed);
                                return;
                            }
                            Ok(n) => {
                                if tx.send(Got::Data(buf[..n].to_vec())).is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Got::Failed(e.to_string()));
                                return;
                            }
                        }
                    }
                });
                let mut w = s;
                for b in rx_out {
                    if let Err(e) = w.write_all(&b) {
                        let _ = tx_in.send(Got::Failed(e.to_string()));
                        return;
                    }
                }
                let _ = w.shutdown(std::net::Shutdown::Both);
            });
            Tcp { to: Some(to), from }
        }
    }

    impl Carrier for Tcp {
        fn send(&mut self, b: &[u8]) {
            if let Some(t) = &self.to {
                let _ = t.send(b.to_vec());
            }
        }
        fn poll(&mut self) -> Option<Got> {
            match self.from.try_recv() {
                Ok(g) => Some(g),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Got::Closed),
            }
        }
    }

    impl Drop for Tcp {
        fn drop(&mut self) {
            // the writer's loop ends, and with it the connection
            self.to = None;
        }
    }
}

/// A carrier a test drives: what was sent, and what is to come.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct Scripted {
    pub sent: Rc<RefCell<Vec<u8>>>,
    pub incoming: Rc<RefCell<std::collections::VecDeque<Got>>>,
}

#[cfg(test)]
impl Carrier for Scripted {
    fn send(&mut self, b: &[u8]) {
        self.sent.borrow_mut().extend_from_slice(b);
    }
    fn poll(&mut self) -> Option<Got> {
        self.incoming.borrow_mut().pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_http_reply_gives_its_body_plain_or_chunked() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec();
        assert_eq!(body_of(&plain).unwrap(), b"hello");
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec();
        assert_eq!(body_of(&chunked).unwrap(), b"hello world");
        // the controls: a 404 is no body, and neither is a header cut short
        assert!(body_of(b"HTTP/1.1 404 Not Found\r\n\r\nno").is_err());
        assert!(body_of(b"HTTP/1.1 200 OK\r\n").is_err());
    }

    #[test]
    fn a_get_sends_its_request_and_waits_for_the_close() {
        let c = Scripted::default();
        let mut g = HttpGet { stream: Stream::from_carrier(Box::new(c.clone())), reply: Vec::new(), done: None };
        g.stream.send(b"GET /news.html HTTP/1.0\r\n\r\n");
        c.incoming.borrow_mut().extend([Got::Data(b"HTTP/1.0 200 OK\r\n\r\nWel".to_vec()), Got::Data(b"come".to_vec())]);
        // the control: open still, so not whole
        assert_eq!(g.poll(), None, "not closed yet: not whole");
        // the far end closes: the body
        c.incoming.borrow_mut().push_back(Got::Closed);
        assert_eq!(g.poll(), Some(Ok(b"Welcome".to_vec())));
        assert!(c.sent.borrow().starts_with(b"GET /news.html"));
        // a failure is said
        let f = Scripted::default();
        let mut g = HttpGet { stream: Stream::from_carrier(Box::new(f.clone())), reply: Vec::new(), done: None };
        f.incoming.borrow_mut().push_back(Got::Failed("refused".into()));
        assert_eq!(g.poll(), Some(Err("refused".into())));
    }
}
