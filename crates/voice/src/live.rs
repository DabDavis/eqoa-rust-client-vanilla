//! End to end against a REAL voice server: go-eqoa-server's forwarding server through its demo
//! WebSocket (PROTOCOL.md's demo mode: a hello naming one "d:" channel, then the signals in parts
//! as 0x7E13 carries them), engines with the game's network settings and a tone for a microphone.
//! Ignored by default, since CI has no server:
//!
//! ```text
//! cargo test -p voice -- --ignored live
//! EQOA_VOICE_DEMO=ws://<host>:18030/api/voice/demo cargo test -p voice -- --ignored live
//! ```
//!
//! The default URL is voicesrv18's private server (127.0.0.30). Its ICE over TCP cannot be
//! checked there: 127.0.0.30 is not loopback's own address.

use crate::tests::{power, rms, tone};
use crate::{IceServer, NativeEngine, SignalKind, VoiceEngine, VoiceEvent, VoiceOutput};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// A signal's parts at most this long (0x7E13's).
const CHUNK: usize = 900;

fn kind_byte(kind: SignalKind) -> u8 {
    match kind {
        SignalKind::Offer => 1,
        SignalKind::Answer => 2,
        SignalKind::Candidate => 3,
        SignalKind::Bye => 4,
    }
}

fn kind_of(b: u64) -> Option<SignalKind> {
    Some(match b {
        1 => SignalKind::Offer,
        2 => SignalKind::Answer,
        3 => SignalKind::Candidate,
        4 => SignalKind::Bye,
        _ => return None,
    })
}

/// A demo channel of its own: "d:" and 32 hex.
fn channel() -> String {
    let h = std::collections::hash_map::RandomState::new();
    format!("d:{:016x}{:016x}", h.hash_one(1), h.hash_one(2))
}

/// One player of the demo: its WebSocket, its engine, what its output played.
struct Player {
    name: String,
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    engine: NativeEngine,
    output: VoiceOutput,
    heard: Vec<f32>,
    seq: u32,
    /// signals coming in parts, by their seq
    parts: HashMap<u64, (SignalKind, Vec<Option<String>>)>,
    /// the roster: name, voice id
    roster: Vec<(String, u32)>,
    started: bool,
}

impl Player {
    fn join(url: &str, name: &str, channel: &str, engine: NativeEngine) -> Player {
        let (mut ws, _) = tungstenite::connect(url).unwrap_or_else(|e| panic!("the demo WebSocket at {url}: {e}"));
        if let MaybeTlsStream::Plain(s) = ws.get_mut() {
            s.set_read_timeout(Some(Duration::from_millis(2))).unwrap();
            let _ = s.set_nodelay(true);
        }
        let mut p = Player {
            name: name.to_owned(),
            ws,
            output: engine.output(),
            engine,
            heard: Vec::new(),
            seq: 0,
            parts: HashMap::new(),
            roster: Vec::new(),
            started: false,
        };
        p.send(json!({"hello": {"name": name, "server": "voicenat18", "channels": [channel]}}));
        p
    }

    fn send(&mut self, v: Value) {
        self.ws.send(Message::Text(v.to_string().into())).unwrap_or_else(|e| panic!("{}: {e}", self.name));
    }

    /// The engine's signals out in parts, and what the server sent in.
    fn step(&mut self) {
        for (kind, text) in self.engine.take_outgoing() {
            self.seq += 1;
            let chunks: Vec<&str> = if text.is_empty() {
                vec![""]
            } else {
                text.as_bytes().chunks(CHUNK).map(|c| std::str::from_utf8(c).expect("SDP and JSON are ASCII")).collect()
            };
            for (i, c) in chunks.iter().enumerate() {
                let part = json!({"seq": self.seq, "kind": kind_byte(kind), "part": i, "parts": chunks.len(), "text": c});
                self.send(json!({ "signal": part }));
            }
        }
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) => self.message(t.as_str()),
                Ok(Message::Close(f)) => panic!("{} closed by the server: {f:?}", self.name),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                Err(e) => panic!("{}: {e}", self.name),
            }
        }
    }

    fn message(&mut self, text: &str) {
        let v: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{}: not JSON ({e}): {text}", self.name));
        if let Some(why) = v.get("error") {
            panic!("{} refused: {why}", self.name);
        }
        if let Some(config) = v.get("config")
            && !self.started
        {
            let s = |x: &Value, k: &str| x[k].as_str().unwrap_or_default().to_owned();
            let ice: Vec<IceServer> = config["ice"]
                .as_array()
                .map(|a| a.iter().map(|x| IceServer { url: s(x, "url"), username: s(x, "username"), credential: s(x, "credential") }).collect())
                .unwrap_or_default();
            self.engine.start(&ice);
            self.started = true;
        }
        if let Some(roster) = v.get("roster").and_then(Value::as_array) {
            self.roster = roster.iter().filter_map(|m| Some((m["name"].as_str()?.to_owned(), m["peer"].as_u64()? as u32))).collect();
        }
        if let Some(sig) = v.get("signal") {
            let (Some(seq), Some(kind), Some(part), Some(parts), Some(chunk)) = (
                sig["seq"].as_u64(),
                sig["kind"].as_u64().and_then(kind_of),
                sig["part"].as_u64(),
                sig["parts"].as_u64(),
                sig["text"].as_str(),
            ) else {
                panic!("{}: a signal not understood: {sig}", self.name);
            };
            let entry = self.parts.entry(seq).or_insert_with(|| (kind, vec![None; parts as usize]));
            if let Some(slot) = entry.1.get_mut(part as usize) {
                *slot = Some(chunk.to_owned());
            }
            if entry.1.iter().all(Option::is_some) {
                let (kind, chunks) = self.parts.remove(&seq).unwrap();
                let whole: String = chunks.into_iter().flatten().collect();
                self.engine.signal(kind, &whole);
            }
        }
    }

    /// Who the roster names `name` (others are shown with " (demo)" after theirs).
    fn peer(&self, name: &str) -> Option<u32> {
        self.roster.iter().find(|(n, _)| n == name || n.starts_with(&format!("{name} "))).map(|&(_, p)| p)
    }
}

/// The players, each output pulled as a sound card would.
struct Demo {
    players: Vec<Player>,
    t0: Instant,
    pulled: u64,
}

impl Demo {
    fn step(&mut self) {
        for p in &mut self.players {
            p.step();
        }
        let due = (self.t0.elapsed().as_secs_f64() * 48_000.0) as u64 - self.pulled;
        if due >= 240 {
            for p in &mut self.players {
                let mut buf = vec![0.0f32; due as usize];
                p.output.mix(&mut buf, 1);
                p.heard.extend(buf);
            }
            self.pulled += due;
        }
    }

    fn run_until(&mut self, ms: u64, mut done: impl FnMut(&Demo) -> bool) -> bool {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            self.step();
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn run(&mut self, ms: u64) {
        self.run_until(ms, |_| false);
    }

    fn tail(&self, i: usize, ms: usize) -> &[f32] {
        let heard = &self.players[i].heard;
        &heard[heard.len().saturating_sub(ms * 48)..]
    }
}

/// Two engines in one demo channel and a third in another, on the real server: the talker's
/// tone heard by the listener (its frequency, and the roster's talker on a slot and talking),
/// nothing at all by the third (the control), and push-to-talk off silences it.
#[test]
#[ignore = "needs a running voice server: EQOA_VOICE_DEMO, default voicesrv18's private one"]
fn live_demo_server() {
    let url = std::env::var("EQOA_VOICE_DEMO").unwrap_or_else(|_| "ws://127.0.0.30:18030/api/voice/demo".to_owned());
    let (here, there) = (channel(), channel());
    let silence = |f: &mut [f32]| f.fill(0.0);
    let players = vec![
        Player::join(&url, "nat18-talker", &here, NativeEngine::with_generator(tone(440.0))),
        Player::join(&url, "nat18-listener", &here, NativeEngine::with_generator(silence)),
        Player::join(&url, "nat18-other", &there, NativeEngine::with_generator(silence)),
    ];
    let mut demo = Demo { players, t0: Instant::now(), pulled: 0 };
    demo.players[0].engine.set_send(1);
    assert!(demo.run_until(30_000, |d| rms(d.tail(1, 200)) > 0.05), "the listener hears the talker within 30 s");
    demo.run(1500);
    let heard = demo.tail(1, 1000).to_vec();
    assert!(power(&heard, 440.0) > 30.0 * power(&heard, 620.0), "the talker's 440 Hz");
    assert!(rms(demo.tail(2, 1500)) < 1e-4, "another channel hears nothing");
    let listener = &demo.players[1];
    let talker = listener.peer("nat18-talker").unwrap_or_else(|| panic!("the talker in the roster: {:?}", listener.roster));
    assert!(listener.peer("nat18-other").is_none(), "nor is the other channel's player in it: {:?}", listener.roster);
    let levels = listener.engine.levels();
    assert!(levels.iter().any(|&(p, l)| p == talker && l > 0.6), "the talker talking: {levels:?}");
    for p in &mut demo.players {
        assert!(p.engine.take_events().contains(&VoiceEvent::Connected), "{} connected", p.name);
    }
    demo.players[0].engine.set_send(0);
    demo.run(1000);
    assert!(rms(demo.tail(1, 400)) < 1e-4, "push-to-talk off: silence");
    for p in &mut demo.players {
        p.engine.stop();
        let _ = p.ws.close(None);
    }
}
