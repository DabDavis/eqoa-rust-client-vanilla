//! The voice demo (the user: "lets add as a demo mode for sandstorm using
//! vanilla rust client"): on a server that offers no voice (Sandstorm's,
//! eqoa.live's), the Extended page's Voice Demo uses eqoaclassic's voice
//! server directly, its signaling over a WebSocket of its own ([`URL`], JSON)
//! instead of the game connection. Its group channel is an id every member's
//! client derives alike (`ids::demo_group`, from the server's name and the
//! members' names), so anyone who knows a group's names can join it: a demo,
//! and said so. The lobby is eqoaclassic's own (its players and the demo's
//! share rooms). The engine and the UI are the game's voice; only the
//! transport differs (`Socket`: the page's WebSocket, natively tungstenite on
//! rustls).
//!
//! The JSON (go-eqoa-server's demo mode), a message an object of one key: the
//! client sends `{"hello": {"name", "server", "channels": [id]}}` once the
//! socket is open, `{"channels": [...]}` when the group changes, `{"room":
//! {"op": "join", "name", "hash", "unlisted"}}`, `{"room": {"op": "leave"}}`,
//! `{"room": {"op": "list"}}`, and `{"signal": {"seq", "kind", "part", "parts",
//! "text"}}` as 0x7E13 carries one; it takes `{"config": {"ice": [{"url",
//! "username", "credential"}], "ttl"}}`, `{"roster": [{"peer", "name",
//! "channels"}]}` (the game's channel bits), `{"lobby": [{"name", "members",
//! "locked"}]}`, `{"roomState": {"result", "name"}}` and `{"signal": {...}}`;
//! a `{"line": text}` or `{"error": text}` is the player's.

use super::Link;
use net::voice::{Reassembly, WireIce, WireMember, WireRoom};
use serde_json::{Value, json};

/// eqoaclassic's voice demo.
pub const URL: &str = "wss://eqoaclassic.com/eqoa/account/api/voice/demo";
/// The waits before the socket is opened again after it closed.
const RECONNECT_MS: [i64; 4] = [5_000, 10_000, 20_000, 60_000];

/// A WebSocket as the demo uses it: text messages both ways.
pub trait Socket {
    fn send(&mut self, text: &str);
    /// The messages come in since the last call.
    fn take(&mut self) -> Vec<String>;
    fn state(&self) -> SocketState;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketState {
    Connecting,
    Open,
    Closed,
}

/// Who the player is to the demo: its character's name, the server's name
/// as the client knows it (the shard list's), its group's names (its own
/// among them; none out of a group).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    pub name: String,
    pub server: String,
    pub group: Vec<String>,
}

/// The demo's link.
pub struct Demo {
    connect: Box<dyn FnMut(&str) -> Box<dyn Socket>>,
    url: String,
    socket: Option<Box<dyn Socket>>,
    who: Who,
    now: i64,
    /// the channels told the server, none before the hello
    sent: Option<Vec<String>>,
    /// lobby ops asked before the hello went
    queued: Vec<Value>,
    ice: Option<Vec<WireIce>>,
    roster: Vec<WireMember>,
    lobby: Option<Vec<WireRoom>>,
    room_states: Vec<(u8, String)>,
    signals: Vec<(u8, String)>,
    parts: Reassembly,
    seq: u32,
    lines: Vec<String>,
    /// when the socket may be opened again, and how many times it closed
    reopen_at: i64,
    closes: usize,
    on: bool,
}

impl Demo {
    pub fn new(connect: Box<dyn FnMut(&str) -> Box<dyn Socket>>) -> Self {
        Self::with_url(URL, connect)
    }

    /// The platform's socket to eqoaclassic's demo.
    pub fn for_platform() -> Self {
        Self::new(Box::new(connect))
    }

    pub fn with_url(url: &str, connect: Box<dyn FnMut(&str) -> Box<dyn Socket>>) -> Self {
        Demo {
            connect,
            url: url.to_string(),
            socket: None,
            who: Who::default(),
            now: 0,
            sent: None,
            queued: Vec::new(),
            ice: None,
            roster: Vec::new(),
            lobby: None,
            room_states: Vec::new(),
            signals: Vec::new(),
            parts: Reassembly::default(),
            seq: 0,
            lines: Vec::new(),
            reopen_at: 0,
            closes: 0,
            on: false,
        }
    }

    /// The player as it is this frame (Play's, before voice's frame).
    pub fn prepare(&mut self, who: Who, now: i64) {
        self.who = who;
        self.now = now;
    }

    /// The channels the player is in now: the group's, in a group.
    pub fn channels(&self) -> Vec<String> {
        if self.who.group.len() >= 2 { vec![super::ids::demo_group(&self.who.server, self.who.group.iter().map(String::as_str))] } else { Vec::new() }
    }

    fn open(&self) -> bool {
        self.socket.as_ref().is_some_and(|s| s.state() == SocketState::Open)
    }

    fn send_json(&mut self, v: Value) {
        if let Some(s) = self.socket.as_mut()
            && s.state() == SocketState::Open
        {
            s.send(&v.to_string());
        }
    }

    /// A lobby op: sent, or kept until the hello has gone.
    fn room(&mut self, op: Value) {
        if self.sent.is_some() && self.open() {
            self.send_json(json!({ "room": op }));
        } else if self.queued.len() < 8 {
            self.queued.push(op);
        }
    }

    /// Everything the server said forgotten, a Bye for the engine (the
    /// forwarding server lets the connection go with the socket).
    fn lost(&mut self) {
        let had = self.ice.is_some();
        self.socket = None;
        self.sent = None;
        self.ice = None;
        self.roster.clear();
        self.lobby = None;
        self.parts.clear();
        if had {
            self.signals.push((net::voice::KIND_BYE, String::new()));
            self.room_states.push((net::voice::ROOM_LEFT, String::new()));
        }
    }

    /// One message from the server.
    fn take_message(&mut self, text: &str) {
        let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) else { return };
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64);
        if let Some(c) = m.get("config") {
            let ice = c.get("ice").and_then(Value::as_array).map(|a| {
                a.iter().filter(|i| i.get("url").and_then(Value::as_str).is_some()).map(|i| WireIce { url: s(i, "url"), username: s(i, "username"), credential: s(i, "credential") }).collect()
            });
            self.ice = Some(ice.unwrap_or_default());
        }
        if let Some(r) = m.get("roster").and_then(Value::as_array) {
            self.roster = r
                .iter()
                .filter_map(|e| Some(WireMember { peer: u32::try_from(n(e, "peer")?).ok()?, name: s(e, "name"), channels: n(e, "channels").unwrap_or(0).min(255) as u8 }))
                .collect();
        }
        if let Some(l) = m.get("lobby").and_then(Value::as_array) {
            self.lobby = Some(
                l.iter()
                    .map(|e| WireRoom { name: s(e, "name"), members: n(e, "members").unwrap_or(0).min(u16::MAX as u64) as u16, locked: e.get("locked").and_then(Value::as_bool).unwrap_or(false) })
                    .collect(),
            );
        }
        if let Some(st) = m.get("roomState")
            && let Some(result) = n(st, "result").filter(|&r| r <= net::voice::ROOM_LEFT as u64)
            && self.room_states.len() < 64
        {
            self.room_states.push((result as u8, s(st, "name")));
        }
        if let Some(sig) = m.get("signal")
            && let (Some(seq), Some(kind), Some(part), Some(parts), Some(text)) = (n(sig, "seq"), n(sig, "kind"), n(sig, "part"), n(sig, "parts"), sig.get("text").and_then(Value::as_str))
        {
            let (kind, part, parts) = (kind.min(255) as u8, part.min(u16::MAX as u64) as u16, parts.min(u16::MAX as u64) as u16);
            if net::voice::sane(kind, part, parts)
                && let Some(whole) = self.parts.take(seq as u32, kind, part, parts, text.to_string())
                && self.signals.len() < 256
            {
                self.signals.push(whole);
            }
        }
        for k in ["line", "error"] {
            if let Some(t) = m.get(k).and_then(Value::as_str) {
                self.lines.push(format!("Voice demo: {t}"));
            }
        }
    }
}

impl Link for Demo {
    /// On: the socket opened (again a while after it closed), the hello sent
    /// once it is open, the group's channel again when it changes; off:
    /// closed, everything forgotten.
    fn agree(&mut self, on: bool) {
        if !on {
            if self.on {
                self.lost();
                self.signals.clear();
                self.room_states.clear();
                self.queued.clear();
                self.closes = 0;
                self.reopen_at = 0;
            }
            self.on = false;
            return;
        }
        self.on = true;
        if self.socket.as_ref().is_some_and(|s| s.state() == SocketState::Closed) {
            self.lost();
            self.reopen_at = self.now + RECONNECT_MS[self.closes.min(RECONNECT_MS.len() - 1)];
            if self.closes == 0 {
                self.lines.push("Voice demo: the connection to eqoaclassic's voice server closed; trying again.".to_string());
            }
            self.closes += 1;
        }
        if self.socket.is_none() && self.now >= self.reopen_at {
            let url = self.url.clone();
            self.socket = Some((self.connect)(&url));
        }
        if !self.open() {
            return;
        }
        let msgs = self.socket.as_mut().map(|s| s.take()).unwrap_or_default();
        for m in msgs {
            self.take_message(&m);
        }
        if self.ice.is_some() {
            self.closes = 0;
        }
        let channels = self.channels();
        match &self.sent {
            None => {
                let hello = json!({ "hello": { "name": self.who.name, "server": self.who.server, "channels": channels } });
                self.send_json(hello);
                self.sent = Some(channels);
                for op in std::mem::take(&mut self.queued) {
                    self.send_json(json!({ "room": op }));
                }
            }
            Some(s) if *s != channels => {
                let msg = json!({ "channels": channels });
                self.send_json(msg);
                self.sent = Some(channels);
            }
            _ => {}
        }
    }

    fn offered(&self) -> bool {
        true
    }

    fn ice(&self) -> Option<Vec<WireIce>> {
        self.ice.clone()
    }

    fn roster(&self) -> Vec<WireMember> {
        self.roster.clone()
    }

    fn take_signals(&mut self) -> Vec<(u8, String)> {
        std::mem::take(&mut self.signals)
    }

    fn send_signal(&mut self, kind: u8, text: &str) {
        if self.ice.is_none() {
            return;
        }
        self.seq = self.seq.wrapping_add(1);
        let parts = net::voice::split(text);
        let n = parts.len();
        for (i, chunk) in parts.into_iter().enumerate() {
            let m = json!({ "signal": { "seq": self.seq, "kind": kind, "part": i, "parts": n, "text": chunk } });
            self.send_json(m);
        }
    }

    fn room_join(&mut self, name: &str, hash: &str, unlisted: bool) {
        self.room(json!({ "op": "join", "name": name, "hash": hash, "unlisted": unlisted }));
    }

    fn room_op(&mut self, op: u8) {
        match op {
            net::voice::ROOM_LEAVE => self.room(json!({ "op": "leave" })),
            net::voice::ROOM_LIST => self.room(json!({ "op": "list" })),
            _ => {}
        }
    }

    fn take_lobby(&mut self) -> Option<Vec<WireRoom>> {
        self.lobby.take()
    }

    fn take_room_states(&mut self) -> Vec<(u8, String)> {
        std::mem::take(&mut self.room_states)
    }

    fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }
}

/// The platform's WebSocket to `url`.
pub fn connect(url: &str) -> Box<dyn Socket> {
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::voice::socket(url)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(native::Ws::open(url))
    }
}

/// Natively: tungstenite on rustls (Mozilla's roots built in, as the
/// updater's), on a thread of its own, the messages handed over through
/// channels.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Socket, SocketState};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::mpsc;

    pub struct Ws {
        out: mpsc::Sender<String>,
        inbox: mpsc::Receiver<String>,
        state: Arc<AtomicU8>,
    }

    const CONNECTING: u8 = 0;
    const OPEN: u8 = 1;
    const CLOSED: u8 = 2;

    impl Ws {
        pub fn open(url: &str) -> Ws {
            let (out, from_game) = mpsc::channel::<String>();
            let (to_game, inbox) = mpsc::channel::<String>();
            let state = Arc::new(AtomicU8::new(CONNECTING));
            let st = state.clone();
            let url = url.to_string();
            let spawned = std::thread::Builder::new().name("voice-demo".into()).spawn(move || {
                run(&url, &from_game, &to_game, &st);
                st.store(CLOSED, Ordering::Relaxed);
            });
            if spawned.is_err() {
                state.store(CLOSED, Ordering::Relaxed);
            }
            Ws { out, inbox, state }
        }
    }

    fn run(url: &str, from_game: &mpsc::Receiver<String>, to_game: &mpsc::Sender<String>, state: &AtomicU8) {
        use tungstenite::Message;
        let Ok((mut ws, _)) = tungstenite::connect(url) else {
            crate::say("voice demo: the socket did not open");
            return;
        };
        let timeout = Some(std::time::Duration::from_millis(30));
        match ws.get_mut() {
            tungstenite::stream::MaybeTlsStream::Plain(s) => {
                let _ = s.set_read_timeout(timeout);
            }
            tungstenite::stream::MaybeTlsStream::Rustls(s) => {
                let _ = s.get_ref().set_read_timeout(timeout);
            }
            _ => {}
        }
        state.store(OPEN, Ordering::Relaxed);
        loop {
            loop {
                match from_game.try_recv() {
                    Ok(t) => {
                        if ws.send(Message::text(t)).is_err() {
                            return;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    // the game let the socket go
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let _ = ws.close(None);
                        let _ = ws.flush();
                        return;
                    }
                }
            }
            match ws.read() {
                Ok(Message::Text(t)) => {
                    if to_game.send(t.to_string()).is_err() {
                        return;
                    }
                }
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => return,
            }
        }
    }

    impl Socket for Ws {
        fn send(&mut self, text: &str) {
            let _ = self.out.send(text.to_string());
        }
        fn take(&mut self) -> Vec<String> {
            self.inbox.try_iter().collect()
        }
        fn state(&self) -> SocketState {
            match self.state.load(Ordering::Relaxed) {
                CONNECTING => SocketState::Connecting,
                OPEN => SocketState::Open,
                _ => SocketState::Closed,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::voice::{CHANNEL_GROUP, CHANNEL_ROOM, ROOM_IN, ROOM_LEAVE, ROOM_LIST};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A socket a test plays the server on.
    #[derive(Default)]
    struct Wire {
        state: Option<SocketState>,
        sent: Vec<String>,
        inbox: Vec<String>,
        opened: usize,
    }

    struct Fake(Rc<RefCell<Wire>>);

    impl Socket for Fake {
        fn send(&mut self, text: &str) {
            self.0.borrow_mut().sent.push(text.to_string());
        }
        fn take(&mut self) -> Vec<String> {
            std::mem::take(&mut self.0.borrow_mut().inbox)
        }
        fn state(&self) -> SocketState {
            self.0.borrow().state.unwrap_or(SocketState::Open)
        }
    }

    fn demo() -> (Demo, Rc<RefCell<Wire>>) {
        let wire = Rc::new(RefCell::new(Wire::default()));
        let w = wire.clone();
        let d = Demo::with_url(
            "wss://test/voice/demo",
            Box::new(move |_url: &str| {
                w.borrow_mut().opened += 1;
                w.borrow_mut().state = Some(SocketState::Open);
                Box::new(Fake(w.clone())) as Box<dyn Socket>
            }),
        );
        (d, wire)
    }

    fn sent(w: &Rc<RefCell<Wire>>) -> Vec<Value> {
        std::mem::take(&mut w.borrow_mut().sent).iter().map(|s| serde_json::from_str(s).unwrap()).collect()
    }

    fn who() -> Who {
        Who { name: "Jibrish".into(), server: "Sandstorm".into(), group: vec!["Jibrish".into(), "Dabsha".into()] }
    }

    /// The hello carries the name, the server and the group's channel, once,
    /// a lobby op asked before it going after it; a change of group sends the
    /// channels alone; the room's ops as the contract spells them; off closes
    /// it and sends nothing more (the control: out of a group, no channel).
    #[test]
    fn hello_the_group_channel_and_the_lobby_s_ops() {
        let (mut d, wire) = demo();
        d.prepare(who(), 0);
        d.room_join("Raid", "", true);
        d.agree(true);
        let group = crate::voice::ids::demo_group("Sandstorm", ["Jibrish", "Dabsha"]);
        assert_eq!(
            sent(&wire),
            [json!({ "hello": { "name": "Jibrish", "server": "Sandstorm", "channels": [group] } }), json!({ "room": { "op": "join", "name": "Raid", "hash": "", "unlisted": true } })],
            "the hello, then the op asked before it"
        );
        d.agree(true);
        assert!(sent(&wire).is_empty(), "told once");
        d.room_op(ROOM_LIST);
        d.room_op(ROOM_LEAVE);
        let h = crate::voice::ids::room_hash("Raid", Some("pw"));
        d.room_join("Raid", &h, false);
        assert_eq!(
            sent(&wire),
            [json!({ "room": { "op": "list" } }), json!({ "room": { "op": "leave" } }), json!({ "room": { "op": "join", "name": "Raid", "hash": h, "unlisted": false } })]
        );
        d.prepare(Who { group: Vec::new(), ..who() }, 10);
        d.agree(true);
        assert_eq!(sent(&wire), [json!({ "channels": [] })], "out of a group: none");
        d.agree(false);
        d.agree(false);
        d.room_op(ROOM_LIST);
        assert!(sent(&wire).is_empty());
        assert_eq!(wire.borrow().opened, 1);
    }

    /// What the server sends: the config, the roster (the game's channel
    /// bits), the lobby, a room's state, a signal in parts put back together;
    /// the signals sent in parts of the 0x7E13 size (the controls: a part
    /// that makes no sense, a room's result past 3).
    #[test]
    fn the_server_s_messages() {
        let (mut d, wire) = demo();
        d.prepare(who(), 0);
        d.agree(true);
        sent(&wire);
        assert_eq!(d.ice(), None, "no config yet");
        let cfg = json!({ "config": { "ice": [{ "url": "turn:104.156.237.19:3479?transport=udp", "username": "1:2", "credential": "x" }], "ttl": 600 } });
        let roster = json!({ "roster": [{ "peer": 7, "name": "Dabsha (demo)", "channels": 1 }, { "peer": 8, "name": "Kellina", "channels": 2 }] });
        let lobby = json!({ "lobby": [{ "name": "Raid", "members": 3, "locked": true }] });
        let states = [json!({ "roomState": { "result": 0, "name": "Raid" } }), json!({ "roomState": { "result": 7, "name": "Raid" } })];
        wire.borrow_mut().inbox = vec![cfg.to_string(), roster.to_string(), lobby.to_string(), states[0].to_string(), states[1].to_string()];
        d.agree(true);
        assert_eq!(d.ice().map(|i| i.len()), Some(1));
        assert_eq!(d.roster().iter().map(|m| (m.peer, m.name.as_str(), m.channels)).collect::<Vec<_>>(), [(7, "Dabsha (demo)", CHANNEL_GROUP), (8, "Kellina", CHANNEL_ROOM)]);
        assert_eq!(d.take_lobby(), Some(vec![WireRoom { name: "Raid".into(), members: 3, locked: true }]));
        assert_eq!(d.take_lobby(), None, "taken once");
        assert_eq!(d.take_room_states(), [(ROOM_IN, "Raid".to_string())], "the result 7 refused");

        let text = "v".repeat(2000);
        let parts: Vec<String> = net::voice::split(&text).iter().enumerate().map(|(i, c)| json!({ "signal": { "seq": 4, "kind": 2, "part": i, "parts": 3, "text": c } }).to_string()).collect();
        let bad = json!({ "signal": { "seq": 5, "kind": 9, "part": 0, "parts": 1, "text": "x" } }).to_string();
        wire.borrow_mut().inbox = vec![parts[2].clone(), bad, parts[0].clone(), parts[1].clone()];
        d.agree(true);
        assert_eq!(d.take_signals(), [(2, text.clone())], "whole once, the kind 9 refused");

        d.send_signal(1, &text);
        let out = sent(&wire);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|m| m["signal"]["seq"] == out[0]["signal"]["seq"] && m["signal"]["parts"] == 3 && m["signal"]["kind"] == 1));
        assert_eq!(out.iter().map(|m| m["signal"]["text"].as_str().unwrap()).collect::<String>(), text);
    }

    /// A socket that closes: forgotten, a Bye for the engine, said once, and
    /// opened again only after the wait (the control: not before it).
    #[test]
    fn a_closed_socket_is_opened_again_after_a_wait() {
        let (mut d, wire) = demo();
        d.prepare(who(), 0);
        d.agree(true);
        wire.borrow_mut().inbox = vec![json!({ "config": { "ice": [], "ttl": 60 } }).to_string()];
        d.agree(true);
        assert!(d.ice().is_some());
        wire.borrow_mut().state = Some(SocketState::Closed);
        d.prepare(who(), 1_000);
        d.agree(true);
        assert_eq!(d.take_signals(), [(net::voice::KIND_BYE, String::new())]);
        assert_eq!(d.take_lines().len(), 1);
        assert_eq!(d.ice(), None);
        d.prepare(who(), 5_999);
        d.agree(true);
        assert_eq!(wire.borrow().opened, 1, "not before the wait");
        d.prepare(who(), 6_000);
        d.agree(true);
        assert_eq!(wire.borrow().opened, 2);
        assert_eq!(sent(&wire).last().map(|m| m.get("hello").is_some()), Some(true), "a new hello");
    }
}
