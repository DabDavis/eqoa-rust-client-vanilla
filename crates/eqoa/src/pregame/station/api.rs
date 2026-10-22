//! `VIStationAPI` (vistationapi.cpp) and its `VITCPConnection`: one TCP
//! connection to the account server, made at the first `Process` and kept
//! for every request after; requests that refuse to overlap ("Please wait for
//! the previous request to complete."), a 30-second timeout on each, and each
//! reply (`[u32 length][type]...`, the length counting itself) handed to the
//! renderer as an `Event`.
//!
//! The bytes go over the pregame's `link::Stream` (natively a socket on
//! threads, in a page the relay), so nothing here knows a platform. On the
//! console `Connect` blocks until the socket is up; a stream says nothing
//! until bytes come, so the connection counts as up when the server's first
//! bytes do (its hello, the session key, which it sends as it accepts), and
//! a request made before then is handed to the stream at once, which holds
//! it until the socket is up: the same order of things on the wire.

use crate::pregame::link::{Got, Stream};
use net::auth::{self, Reply};

/// Makes the stream to `(host, port)`: `Stream::connect`, or a test's.
pub type Connector = Box<dyn FnMut(&str, u16) -> Stream>;

/// What the API tells its listener (`VIStationListener`'s calls).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Reply(Reply),
    /// `OnError(code, text)`
    Error(i32, String),
    /// `OnStatus(text)`
    Status(String),
    /// `OnConnected()`
    Connected,
}

pub const PLEASE_WAIT: &str = "Please wait for the previous request to complete.";
pub const TIMED_OUT: &str = "Request timed out, please try again.  If the problem persists, please ensure that your network cables are connected.";
pub const LOST: &str = "Connection to server lost.  Please try again.";
pub const NO_CONNECT: &str = "Could not connect to the server. Please verify that your network connection is intact.  If the problem persists, return to network setup and try again.";

pub struct Api {
    connector: Connector,
    stream: Option<Stream>,
    /// the stream has spoken (until then a connect is under way)
    up: bool,
    servers: Vec<(String, u16)>,
    connect_time: i64,
    request_time: i64,
    /// the session key arrived (type 4), and the key
    pub have_key: bool,
    pub key: [u8; 32],
    /// 0, or -3 when the last connect failed (`Process` then stops trying)
    connect_result: i32,
    /// a request is outstanding
    pub busy: bool,
    /// what has arrived and not yet made a whole message
    rx: Vec<u8>,
    /// `srand(VITimer::GetTime())`'s `rand()`, for `ChooseServer`
    rand: vi::csprite::Rand,
}

impl Api {
    /// `VIStationAPI::VIStationAPI` at `now`.
    pub fn new(now: i64) -> Api {
        Api::with_connector(Box::new(|h: &str, p: u16| Stream::connect(h, p)), now)
    }

    pub fn with_connector(connector: Connector, now: i64) -> Api {
        Api {
            connector,
            stream: None,
            up: false,
            servers: Vec::new(),
            connect_time: 0,
            request_time: 0,
            have_key: false,
            key: [0; 32],
            connect_result: 0,
            busy: false,
            rx: Vec::new(),
            rand: vi::csprite::Rand(now as u64 & 0xFFFF_FFFF),
        }
    }

    /// `AddServer(host, port)`.
    pub fn add_server(&mut self, host: &str, port: i32) {
        if host.is_empty() || port <= 0 {
            return;
        }
        self.servers.push((host.chars().take(255).collect(), port as u16));
    }

    /// `ChooseServer`: one of them at random.
    fn choose(&mut self) -> Option<(String, u16)> {
        let n = self.servers.len();
        if n == 0 {
            return None;
        }
        let k = self.rand.rand() as usize % n;
        Some(self.servers[k].clone())
    }

    /// `Connect()`: true when connected. Neither connected nor connecting,
    /// a connect begins (`OnStatus("Connecting to server...")`).
    pub fn connect(&mut self, now: i64, events: &mut Vec<Event>) -> bool {
        self.connect_result = 0;
        self.connect_time = now;
        if self.up {
            return true;
        }
        if self.stream.is_some() {
            return false;
        }
        events.push(Event::Status("Connecting to server...".into()));
        match self.choose() {
            Some((host, port)) => {
                self.stream = Some((self.connector)(&host, port));
                self.rx.clear();
            }
            None => self.failed(events),
        }
        false
    }

    /// A connect under way (the console's `Connect` blocked on it).
    pub fn connecting(&self) -> bool {
        self.stream.is_some() && !self.up
    }

    /// The connect failed: `Connect`'s 0, the error, and (made to wait
    /// here, never made on the console) the request with it.
    fn failed(&mut self, events: &mut Vec<Event>) {
        self.stream = None;
        self.up = false;
        self.connect_result = -3;
        self.have_key = false;
        self.busy = false;
        events.push(Event::Error(0, NO_CONNECT.into()));
    }

    /// What the stream has said: the first bytes are the connect's success
    /// (`OnConnected`), its end before them the connect's failure, after
    /// them the connection lost (`VITCPConnection::Process`'s -50: closed,
    /// and `Process` connects again).
    fn pump(&mut self, events: &mut Vec<Event>) {
        while let Some(got) = self.stream.as_mut().and_then(Stream::poll) {
            match got {
                Got::Data(b) => {
                    if !self.up {
                        self.up = true;
                        events.push(Event::Connected);
                    }
                    self.rx.extend(b);
                }
                Got::Closed | Got::Failed(_) => {
                    if !self.up {
                        self.failed(events);
                    } else {
                        self.stream = None;
                        self.up = false;
                        self.rx.clear();
                        events.push(Event::Error(-50, LOST.into()));
                    }
                    return;
                }
            }
        }
    }

    /// The connect's answer, while one is under way (the renderer's
    /// `m_checkNet` frames, when the console's `Connect` had not returned).
    pub fn poll_connect(&mut self, events: &mut Vec<Event>) {
        if self.connecting() {
            self.pump(events);
        }
    }

    /// `Process()`: connected (or connecting), a ping after a minute
    /// connected (`Connect` sets the time it measures from, so never, as
    /// shipped), a request's timeout, and the connection pumped.
    pub fn process(&mut self, now: i64, events: &mut Vec<Event>) {
        if self.connect_result == -3 || self.connect_result == -2 {
            return;
        }
        self.connect(now, events);
        if now - self.connect_time > 60000 {
            self.ping(now, events);
        }
        if self.busy && now - self.request_time > 30000 {
            events.push(Event::Error(0, TIMED_OUT.into()));
            self.busy = false;
        }
        self.pump(events);
        if !self.up {
            return;
        }
        while self.rx.len() >= 4 {
            let len = u32::from_be_bytes(self.rx[0..4].try_into().unwrap()) as usize;
            let body = len.saturating_sub(4);
            if self.rx.len() < 4 + body {
                break;
            }
            let msg: Vec<u8> = self.rx[4..4 + body].to_vec();
            self.rx.drain(..4 + body);
            // `OnMessage`: any message ends the request
            if msg.len() >= 4 {
                self.busy = false;
            }
            match auth::parse_reply(&msg) {
                Some(Reply::SessionKey(k)) => {
                    self.key = k;
                    self.have_key = true;
                }
                Some(r) => events.push(Event::Reply(r)),
                None => {}
            }
        }
    }

    /// A request: connected (or connecting, when the stream holds it), not
    /// while another is outstanding, then the request timed. 1 when it could
    /// not go at all, else 0.
    fn request(&mut self, b: Vec<u8>, now: i64, events: &mut Vec<Event>) -> i32 {
        if !self.connect(now, events) && !self.connecting() {
            return 1;
        }
        if self.busy {
            events.push(Event::Error(0, PLEASE_WAIT.into()));
            return 0;
        }
        if let Some(s) = self.stream.as_mut() {
            s.send(&b);
        }
        self.busy = true;
        self.request_time = now;
        0
    }

    /// Type 36, `ValidateLogin(name, password)`.
    pub fn validate_login(&mut self, name: &str, password: &str, now: i64, events: &mut Vec<Event>) -> i32 {
        self.request(auth::login_request(name, password).to_vec(), now, events)
    }

    /// Type 56, `CreateAccount(account)`.
    pub fn create_account(&mut self, a: &auth::Account, now: i64, events: &mut Vec<Event>) -> i32 {
        self.request(auth::create_account(a), now, events)
    }

    /// Type 42, `ChangePassword(id, old, new)`: never for an id under 0.
    pub fn change_password(&mut self, id: i32, old: &str, new: &str, now: i64, events: &mut Vec<Event>) -> i32 {
        if id < 0 {
            return 1;
        }
        self.request(auth::change_password(id, old, new), now, events)
    }

    /// Types 48 (an account key), 54 (a game card), 50, 52 and 62: an id and
    /// the game code (and a string); never for an id under 0.
    pub fn request_id(&mut self, ty: u32, id: i32, b: &str, now: i64, events: &mut Vec<Event>) -> i32 {
        if id < 0 {
            return 1;
        }
        let req = match ty {
            48 | 44 => auth::request_id_ab(ty, id, auth::GAME_CODE, b),
            54 => auth::request54(id, auth::GAME_CODE, b),
            62 => auth::request62(id, auth::GAME_CODE),
            _ => auth::request_id_a(ty, id, auth::GAME_CODE),
        };
        self.request(req, now, events)
    }

    /// Type 46, the billing form under the session key.
    pub fn request46(&mut self, id: i32, f: &auth::Billing, sku: &str, now: i64, events: &mut Vec<Event>) -> i32 {
        if id < 0 {
            return 1;
        }
        let key = self.key;
        self.request(auth::request46(id, f, sku, &key), now, events)
    }

    /// Type 64, the account update.
    pub fn request64(&mut self, id: i32, country: &str, postal: &str, email: &str, now: i64, events: &mut Vec<Event>) -> i32 {
        if id < 0 {
            return 1;
        }
        self.request(auth::request64(id, country, postal, email), now, events)
    }

    /// Type 66, the forgotten password, under the session key.
    pub fn request66(&mut self, user: &str, month: &str, day: &str, year: &str, email: &str, now: i64, events: &mut Vec<Event>) -> i32 {
        let key = self.key;
        self.request(auth::request66(user, month, day, year, email, &key), now, events)
    }

    /// Type 2, `Ping`: no request outstanding is made of it.
    pub fn ping(&mut self, now: i64, events: &mut Vec<Event>) {
        if self.connect(now, events)
            && let Some(s) = self.stream.as_mut()
        {
            s.send(&auth::ping());
        }
    }
}

/// A scripted server, for tests: every connect a `link::Scripted` carrier
/// of its own, the last one the test's to drive.
#[cfg(test)]
pub mod testing {
    use super::Connector;
    use crate::pregame::link::{Got, Scripted, Stream};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    pub struct Fake {
        pub connects: Rc<RefCell<Vec<(String, u16)>>>,
        pub carrier: Rc<RefCell<Scripted>>,
    }

    impl Fake {
        pub fn connector(&self) -> Connector {
            let f = self.clone();
            Box::new(move |host: &str, port: u16| {
                f.connects.borrow_mut().push((host.to_string(), port));
                let c = Scripted::default();
                *f.carrier.borrow_mut() = c.clone();
                Stream::from_carrier(Box::new(c))
            })
        }

        /// What the client has sent on the last connection.
        pub fn sent(&self) -> Vec<u8> {
            self.carrier.borrow().sent.borrow().clone()
        }

        /// The server says `g` on the last connection.
        pub fn say(&self, g: Got) {
            self.carrier.borrow().incoming.borrow_mut().push_back(g);
        }
    }

    /// A message as the server frames it: the length counting itself.
    pub fn framed(ty: u32, len: usize, fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut b = vec![0u8; len];
        b[0..4].copy_from_slice(&(len as u32).to_be_bytes());
        b[4..8].copy_from_slice(&ty.to_be_bytes());
        fill(&mut b);
        b
    }

    /// The server's hello: type 4, the session key DES'd.
    pub fn hello(key: [u8; 32]) -> Vec<u8> {
        framed(4, 40, |b| b[8..40].copy_from_slice(&net::auth::des_soe_encrypt(&key, 32)))
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Fake, framed, hello};
    use super::*;

    fn api() -> (Api, Fake) {
        let f = Fake::default();
        let mut a = Api::with_connector(f.connector(), 0);
        a.add_server("192.0.2.1", 9735);
        (a, f)
    }

    #[test]
    fn a_login_made_while_connecting_goes_out_and_the_hello_is_the_connect() {
        let (mut a, f) = api();
        let mut ev = Vec::new();
        a.process(0, &mut ev);
        assert_eq!(ev, vec![Event::Status("Connecting to server...".into())]);
        assert_eq!(*f.connects.borrow(), vec![("192.0.2.1".to_string(), 9735)]);
        assert!(a.connecting());
        ev.clear();
        assert_eq!(a.validate_login("van", "pw", 10, &mut ev), 0);
        assert_eq!(f.sent(), net::auth::login_request("van", "pw").to_vec(), "handed to the stream, which holds it");
        assert!(a.busy);
        // the control: nothing heard, still connecting
        a.poll_connect(&mut ev);
        assert!(ev.is_empty() && a.connecting());
        // the hello's session key, then the reply, split across two reads
        let reply = framed(37, 168, |b| b[84..88].copy_from_slice(&3u32.to_be_bytes()));
        let mut both = hello([5; 32]);
        both.extend(&reply[..100]);
        f.say(Got::Data(both));
        a.poll_connect(&mut ev);
        assert_eq!(ev, vec![Event::Connected]);
        ev.clear();
        a.process(30, &mut ev);
        assert!(ev.is_empty() && a.have_key && a.key == [5; 32]);
        f.say(Got::Data(reply[100..].to_vec()));
        a.process(40, &mut ev);
        assert!(matches!(&ev[..], [Event::Reply(Reply::Login { result: 0, accstat: 3, .. })]));
        assert!(!a.busy, "the reply ended the request");
    }

    #[test]
    fn requests_do_not_overlap_and_time_out() {
        let (mut a, f) = api();
        let mut ev = Vec::new();
        a.process(0, &mut ev);
        f.say(Got::Data(hello([1; 32])));
        a.process(1, &mut ev);
        ev.clear();
        a.validate_login("van", "pw", 100, &mut ev);
        a.change_password(4, "pw", "new", 200, &mut ev);
        assert_eq!(ev, vec![Event::Error(0, PLEASE_WAIT.into())]);
        assert_eq!(f.sent(), net::auth::login_request("van", "pw").to_vec(), "only the first went");
        ev.clear();
        a.process(30_100, &mut ev);
        assert!(ev.is_empty(), "30 s is not yet past");
        a.process(30_101, &mut ev);
        assert_eq!(ev, vec![Event::Error(0, TIMED_OUT.into())]);
        assert!(!a.busy);
        // the control: an id under 0 sends nothing at all
        let before = f.sent().len();
        assert_eq!(a.change_password(-1, "a", "b", 30_200, &mut ev), 1);
        assert_eq!(f.sent().len(), before);
    }

    #[test]
    fn a_failed_connect_says_so_and_stops_trying_until_asked() {
        let (mut a, f) = api();
        let mut ev = Vec::new();
        a.process(0, &mut ev);
        a.validate_login("van", "pw", 1, &mut ev);
        f.say(Got::Failed("refused".into()));
        a.poll_connect(&mut ev);
        assert_eq!(ev.last(), Some(&Event::Error(0, NO_CONNECT.into())));
        assert!(!a.busy, "the waiting login went with it");
        ev.clear();
        a.process(3, &mut ev);
        assert!(ev.is_empty(), "Process stops trying after a failed connect");
        // a new request tries again
        a.validate_login("van", "pw", 4, &mut ev);
        assert_eq!(ev, vec![Event::Status("Connecting to server...".into())]);
        assert_eq!(f.connects.borrow().len(), 2);
    }

    #[test]
    fn a_lost_connection_is_said_and_made_again() {
        let (mut a, f) = api();
        let mut ev = Vec::new();
        a.process(0, &mut ev);
        f.say(Got::Data(hello([1; 32])));
        a.process(1, &mut ev);
        f.say(Got::Closed);
        ev.clear();
        a.process(2, &mut ev);
        assert_eq!(ev, vec![Event::Error(-50, LOST.into())]);
        ev.clear();
        a.process(3, &mut ev);
        assert_eq!(ev, vec![Event::Status("Connecting to server...".into())], "the next Process connects again");
        assert_eq!(f.connects.borrow().len(), 2);
    }
}
