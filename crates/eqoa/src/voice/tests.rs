use super::*;
use net::voice::{KIND_ANSWER, KIND_BYE, KIND_CANDIDATE, KIND_OFFER};
use std::cell::RefCell;
use std::rc::Rc;

/// A `NullEngine` a test keeps a hold of while `Voice` drives it.
struct Shared(Rc<RefCell<NullEngine>>);

impl VoiceEngine for Shared {
    fn start(&mut self, ice: &[IceServer]) {
        self.0.borrow_mut().start(ice)
    }
    fn signal(&mut self, kind: SignalKind, text: &str) {
        self.0.borrow_mut().signal(kind, text)
    }
    fn take_outgoing(&mut self) -> Vec<(SignalKind, String)> {
        self.0.borrow_mut().take_outgoing()
    }
    fn set_send(&mut self, channels: u8) {
        self.0.borrow_mut().set_send(channels)
    }
    fn set_peer_volume(&mut self, peer: u32, volume: f32) {
        self.0.borrow_mut().set_peer_volume(peer, volume)
    }
    fn levels(&self) -> Vec<(u32, f32)> {
        self.0.borrow().levels()
    }
    fn mic_level(&self) -> f32 {
        self.0.borrow().mic_level()
    }
    fn stop(&mut self) {
        self.0.borrow_mut().stop()
    }
    fn take_events(&mut self) -> Vec<VoiceEvent> {
        self.0.borrow_mut().take_events()
    }
}

/// A link a test plays the server on.
#[derive(Default)]
struct FakeLink {
    offered: bool,
    on: bool,
    agreed: Vec<bool>,
    ice: Option<Vec<WireIce>>,
    roster: Vec<WireMember>,
    signals_in: Vec<(u8, String)>,
    signals_out: Vec<(u8, String)>,
    joins: Vec<(String, String, bool)>,
    ops: Vec<u8>,
    lobby: Option<Vec<WireRoom>>,
    states: Vec<(u8, String)>,
}

impl Link for FakeLink {
    fn agree(&mut self, on: bool) {
        if self.on != on {
            self.agreed.push(on);
        }
        self.on = on;
    }
    fn offered(&self) -> bool {
        self.offered
    }
    fn ice(&self) -> Option<Vec<WireIce>> {
        self.ice.clone()
    }
    fn roster(&self) -> Vec<WireMember> {
        self.roster.clone()
    }
    fn take_signals(&mut self) -> Vec<(u8, String)> {
        std::mem::take(&mut self.signals_in)
    }
    fn send_signal(&mut self, kind: u8, text: &str) {
        self.signals_out.push((kind, text.to_string()));
    }
    fn room_join(&mut self, name: &str, hash: &str, unlisted: bool) {
        self.joins.push((name.to_string(), hash.to_string(), unlisted));
    }
    fn room_op(&mut self, op: u8) {
        self.ops.push(op);
    }
    fn take_lobby(&mut self) -> Option<Vec<WireRoom>> {
        self.lobby.take()
    }
    fn take_room_states(&mut self) -> Vec<(u8, String)> {
        std::mem::take(&mut self.states)
    }
}

fn voice() -> (Voice, Rc<RefCell<NullEngine>>) {
    let rec = Rc::new(RefCell::new(NullEngine::default()));
    (Voice::new(Box::new(Shared(rec.clone())), true), rec)
}

fn ice() -> Vec<WireIce> {
    vec![WireIce { url: "turn:104.156.237.19:3479?transport=udp".into(), username: "1761500000:5".into(), credential: "c2VjcmV0".into() }]
}

fn member(peer: u32, name: &str, channels: u8) -> WireMember {
    WireMember { peer, name: name.into(), channels }
}

fn ptt(group: bool, room: bool) -> Wants {
    Wants { chat: VoiceChat::PushToTalk, demo: false, keys: [group, room], volume: 10 }
}

/// A game link that offered voice and answered: the engine starts.
fn offered() -> FakeLink {
    FakeLink { offered: true, ice: Some(ice()), ..Default::default() }
}

/// Off: the server is told nothing and the engine never starts. On with no
/// offer: nothing starts, and ten seconds into the world the player is told
/// once that the server has no voice (the control: before then, nothing).
#[test]
fn off_asks_nothing_and_a_server_without_voice_is_said_so_once() {
    let (mut v, rec) = voice();
    let (mut game, mut demo) = (FakeLink::default(), FakeLink::default());
    v.frame(&mut game, &mut demo, Wants::default(), 0);
    assert!(game.agreed.is_empty() && rec.borrow().started.is_empty(), "off: nothing told, nothing started");
    v.frame(&mut game, &mut demo, ptt(false, false), 1_000);
    assert_eq!(game.agreed, [true], "the game's link told voice on");
    v.frame(&mut game, &mut demo, ptt(false, false), 9_999);
    assert!(v.take_lines().is_empty(), "not yet");
    v.frame(&mut game, &mut demo, ptt(false, false), 10_000);
    v.frame(&mut game, &mut demo, ptt(false, false), 20_000);
    assert_eq!(v.take_lines().len(), 1, "said once");
    assert!(rec.borrow().started.is_empty());
    assert!(demo.agreed.is_empty(), "the demo untouched without Voice Demo");
}

/// Offered and on: the engine started once the server answered, its offer
/// and candidates sent on, the server's answer and candidates given it; a
/// Bye stops it and it starts again after the wait (the control: not
/// before); off stops it and tells the server.
#[test]
fn the_engine_follows_the_server() {
    let (mut v, rec) = voice();
    let (mut game, mut demo) = (FakeLink { offered: true, ..Default::default() }, FakeLink::default());
    v.frame(&mut game, &mut demo, ptt(false, false), 0);
    assert!(rec.borrow().started.is_empty(), "no answer yet: not started");
    game.ice = Some(ice());
    v.frame(&mut game, &mut demo, ptt(false, false), 100);
    assert_eq!(rec.borrow().started.len(), 1);
    assert_eq!(rec.borrow().started[0][0].url, "turn:104.156.237.19:3479?transport=udp");
    rec.borrow_mut().outgoing = vec![(SignalKind::Offer, "v=0 offer".into()), (SignalKind::Candidate, "{\"candidate\":\"c\"}".into())];
    game.signals_in = vec![(KIND_ANSWER, "v=0 answer".into()), (KIND_CANDIDATE, "{}".into()), (9, "junk".into())];
    v.frame(&mut game, &mut demo, ptt(false, false), 200);
    assert_eq!(game.signals_out, [(KIND_OFFER, "v=0 offer".to_string()), (KIND_CANDIDATE, "{\"candidate\":\"c\"}".to_string())]);
    assert_eq!(rec.borrow().signals, [(SignalKind::Answer, "v=0 answer".to_string()), (SignalKind::Candidate, "{}".to_string())], "the kind 9 dropped");
    rec.borrow_mut().events = vec![VoiceEvent::Connected];
    v.frame(&mut game, &mut demo, ptt(false, false), 300);
    assert!(v.connected());
    assert_eq!(v.take_lines(), ["Voice chat connected."]);

    game.signals_in = vec![(KIND_BYE, String::new())];
    v.frame(&mut game, &mut demo, ptt(false, false), 1_000);
    assert_eq!(rec.borrow().stopped, 1);
    assert!(!v.connected());
    assert_eq!(v.take_lines(), ["Voice chat lost its connection; trying again."]);
    v.frame(&mut game, &mut demo, ptt(false, false), 5_999);
    assert_eq!(rec.borrow().started.len(), 1, "not before the wait");
    v.frame(&mut game, &mut demo, ptt(false, false), 6_000);
    assert_eq!(rec.borrow().started.len(), 2, "again after it");

    v.frame(&mut game, &mut demo, Wants::default(), 7_000);
    assert_eq!(game.agreed, [true, false], "off told");
    assert_eq!(rec.borrow().stopped, 2);
}

/// The keys: the group's sends to the group, the room's to the room (only
/// while in one), both to both; open mic both while the microphone hears
/// speech and a little after; nothing held, nothing sent.
#[test]
fn the_keys_choose_what_is_sent() {
    let (mut v, rec) = voice();
    let (mut game, mut demo) = (offered(), FakeLink::default());
    let mut step = |v: &mut Voice, game: &mut FakeLink, w: Wants, t: i64| {
        v.frame(game, &mut demo, w, t);
        *rec.borrow().send.last().unwrap()
    };
    assert_eq!(step(&mut v, &mut game, ptt(false, false), 0), 0, "nothing held");
    assert_eq!(step(&mut v, &mut game, ptt(true, false), 10), CHANNEL_GROUP, "the group's key");
    assert_eq!(step(&mut v, &mut game, ptt(false, true), 20), 0, "the room's key out of a room: nothing");
    game.states = vec![(ROOM_IN, "Raid".into())];
    assert_eq!(step(&mut v, &mut game, ptt(false, true), 30), CHANNEL_ROOM, "in a room: the room's key");
    assert_eq!(step(&mut v, &mut game, ptt(true, true), 40), CHANNEL_GROUP | CHANNEL_ROOM, "both");
    let open = Wants { chat: VoiceChat::OpenMic, ..ptt(false, false) };
    assert_eq!(step(&mut v, &mut game, open, 50), 0, "open mic, silence");
    rec.borrow_mut().mic = 0.6;
    step(&mut v, &mut game, open, 60);
    rec.borrow_mut().mic = 0.0;
    assert_eq!(step(&mut v, &mut game, open, 70), CHANNEL_GROUP | CHANNEL_ROOM, "speech: both");
    assert_eq!(step(&mut v, &mut game, open, 70 + 590), CHANNEL_GROUP | CHANNEL_ROOM, "held a little after");
    assert_eq!(step(&mut v, &mut game, open, 70 + 700), 0, "and closed");
    let told = rec.borrow().send.len();
    step(&mut v, &mut game, open, 800);
    assert_eq!(rec.borrow().send.len(), told, "told only on a change");
}

/// The lobby: a join carries only the room's H (none for an open room), the
/// answers become lines (made, in with others, a wrong password, a refused
/// name, left), a list asked for is said; `/voice` never asks anything of
/// the server while Voice Chat is off (the control).
#[test]
fn the_lobby_and_its_lines() {
    let (mut v, _rec) = voice();
    let (mut game, mut demo) = (offered(), FakeLink::default());
    v.command("join Raid hunter2", false);
    assert_eq!(v.take_lines(), ["Voice: Voice Chat is off (Options, Extended, Sound)."]);
    v.frame(&mut game, &mut demo, ptt(false, false), 0);
    assert!(game.joins.is_empty(), "nothing asked while off");

    v.command("join \"Raid Night\" hunter2", true);
    v.command("unlisted Hideout", true);
    v.frame(&mut game, &mut demo, ptt(false, false), 10);
    assert_eq!(game.joins, [("Raid Night".to_string(), ids::room_hash("Raid Night", Some("hunter2")), false), ("Hideout".to_string(), String::new(), true)]);
    assert!(!game.joins.iter().any(|(_, h, _)| h.contains("hunter2")), "the password goes nowhere");

    game.states = vec![(ROOM_WRONG_PASSWORD, "Raid Night".into()), (ROOM_IN, "Hideout".into())];
    v.frame(&mut game, &mut demo, ptt(false, false), 20);
    assert_eq!(v.take_lines(), ["Voice: wrong password for Raid Night."]);
    v.frame(&mut game, &mut demo, ptt(false, false), 20 + ROOM_LINE_MS);
    assert_eq!(v.take_lines(), ["Voice: made unlisted room Hideout: only those who know its name can find it."], "nobody else in it: made");
    assert_eq!(v.view(ptt(false, false), 2_000).room, Some(("Hideout".to_string(), false, true)));

    game.states = vec![(ROOM_LEFT, "Hideout".into())];
    v.frame(&mut game, &mut demo, ptt(false, false), 2_000);
    assert_eq!(v.take_lines(), ["Voice: left room Hideout."]);
    v.join("Raid", "", false);
    game.states = vec![(ROOM_IN, "Raid".into())];
    v.frame(&mut game, &mut demo, ptt(false, false), 2_010);
    game.roster = vec![member(9, "Jibrish", CHANNEL_ROOM), member(12, "Dabsha", CHANNEL_GROUP | CHANNEL_ROOM)];
    v.frame(&mut game, &mut demo, ptt(false, false), 2_020);
    assert_eq!(v.take_lines(), ["Voice: with Jibrish (room), Dabsha.", "Voice: in room Raid, 2 others."], "the roster's line, then the room's with its count");
    v.command("who", true);
    assert_eq!(v.take_lines(), ["Voice: in room Raid: Jibrish, Dabsha."]);

    v.command("rooms", true);
    v.frame(&mut game, &mut demo, ptt(false, false), 2_030);
    assert_eq!(game.ops, [ROOM_LIST]);
    game.lobby = Some(vec![WireRoom { name: "Raid".into(), members: 3, locked: false }, WireRoom { name: "Guild Hall".into(), members: 12, locked: true }]);
    v.frame(&mut game, &mut demo, ptt(false, false), 2_040);
    assert_eq!(v.take_lines(), ["Voice: rooms: Raid (3), Guild Hall (12, locked)."]);
    game.lobby = Some(Vec::new());
    v.frame(&mut game, &mut demo, ptt(false, false), 2_050);
    assert!(v.take_lines().is_empty(), "a lobby not asked for is not said");
    v.refresh_lobby(false);
    v.frame(&mut game, &mut demo, ptt(false, false), 2_052);
    assert_eq!(game.ops, [ROOM_LIST, ROOM_LIST], "the window's asks");
    game.lobby = Some(vec![WireRoom { name: "Raid".into(), members: 3, locked: false }]);
    v.frame(&mut game, &mut demo, ptt(false, false), 2_054);
    assert!(v.take_lines().is_empty(), "and is not said: the window shows it");
    assert_eq!(v.view(ptt(false, false), 2_054).lobby, [VoiceRoomInfo { name: "Raid".into(), members: 3, locked: false }]);
    game.states = vec![(ROOM_REFUSED, "Bad".into())];
    v.frame(&mut game, &mut demo, ptt(false, false), 2_060);
    assert_eq!(v.take_lines(), ["Voice: that name is refused."]);
    v.command("join ab", true);
    assert_eq!(v.take_lines(), ["Voice: a room's name is 3 to 24 letters."]);
}

/// Voice Volume times a member's own sets each voice, a muted member none
/// (by name: it stays muted when it comes back with another id); the
/// engine's events become the player's lines; the view has the members,
/// the mode and what is sent.
#[test]
fn volumes_mutes_and_the_view() {
    let (mut v, rec) = voice();
    let (mut game, mut demo) = (offered(), FakeLink::default());
    game.roster = vec![member(9, "Jibrish", CHANNEL_GROUP), member(12, "Dabsha", CHANNEL_ROOM)];
    let w = Wants { volume: 15, ..ptt(false, false) };
    v.frame(&mut game, &mut demo, w, 0);
    let vol = |p: u32| rec.borrow().volumes.iter().find(|&&(q, _)| q == p).map(|&(_, v)| v);
    assert_eq!((vol(9), vol(12)), (Some(1.5), Some(1.5)));
    v.set_volume(9, 0.5);
    assert!(v.toggle_mute(12));
    v.frame(&mut game, &mut demo, w, 10);
    assert_eq!((vol(9), vol(12)), (Some(0.75), Some(0.0)), "Jibrish at half, Dabsha muted");
    game.roster = vec![member(9, "Jibrish", CHANNEL_GROUP), member(40, "dabsha", CHANNEL_ROOM)];
    v.frame(&mut game, &mut demo, w, 20);
    assert_eq!(vol(40), Some(0.0), "muted by name, whatever its id");
    assert!(!v.toggle_mute(40));
    v.frame(&mut game, &mut demo, w, 30);
    assert_eq!(vol(40), Some(1.5), "and back");

    rec.borrow_mut().events = vec![VoiceEvent::Connected, VoiceEvent::MicDenied];
    rec.borrow_mut().levels = vec![(9, 0.5), (40, 0.6)];
    rec.borrow_mut().mic = 0.3;
    v.frame(&mut game, &mut demo, Wants { keys: [true, false], ..w }, 40);
    let lines = v.take_lines();
    assert!(lines.contains(&"Voice chat connected.".to_string()) && lines.iter().any(|l| l.contains("microphone was not allowed")), "{lines:?}");
    v.frame(&mut game, &mut demo, Wants { keys: [true, false], ..w }, 50);
    let view = v.view(Wants { keys: [true, false], ..w }, 50);
    assert_eq!(view.mode, VoiceMode::PushToTalk);
    assert!(view.available && view.connected && !view.demo_only);
    assert_eq!(view.transmitting, CHANNEL_GROUP);
    assert_eq!(view.members.iter().map(|m| (m.peer, m.name.as_str(), m.group, m.room, m.volume, m.muted)).collect::<Vec<_>>(), [(9, "Jibrish", true, false, 0.5, false), (40, "dabsha", false, true, 1.0, false)]);
    assert_eq!(v.room_speakers().iter().map(|m| m.peer).collect::<Vec<_>>(), [40], "the room's speaker listed, the groupmate on its bar");
    assert_eq!(v.transmitting(), Some(0.3));
}

/// Voice Demo: on a server that offered none, ten seconds in, the demo's
/// link is used (said once, the game's told off), the view's mode Demo; the
/// control: with Voice Demo off the demo's link is never used; and a server
/// that offers voice is never left for the demo.
#[test]
fn the_demo_where_the_server_offers_none() {
    let (mut v, rec) = voice();
    let (mut game, mut demo) = (FakeLink::default(), FakeLink { offered: true, ice: Some(ice()), ..Default::default() });
    let w = Wants { demo: true, ..ptt(false, false) };
    v.frame(&mut game, &mut demo, w, 0);
    assert!(demo.agreed.is_empty(), "not before the wait");
    assert_eq!(v.view(w, 0).mode, VoiceMode::PushToTalk);
    v.frame(&mut game, &mut demo, w, 10_000);
    assert_eq!((game.agreed.as_slice(), demo.agreed.as_slice()), (&[true, false][..], &[true][..]), "the game's told off, the demo's on");
    assert_eq!(rec.borrow().started.len(), 1);
    assert!(v.take_lines().iter().any(|l| l.starts_with("Voice demo:")));
    let view = v.view(w, 10_000);
    assert_eq!((view.mode, view.demo_only), (VoiceMode::Demo, true));

    let (mut v2, rec2) = voice();
    let (mut game2, mut demo2) = (FakeLink::default(), FakeLink { offered: true, ice: Some(ice()), ..Default::default() });
    v2.frame(&mut game2, &mut demo2, ptt(false, false), 0);
    v2.frame(&mut game2, &mut demo2, ptt(false, false), 20_000);
    assert!(demo2.agreed.is_empty() && rec2.borrow().started.is_empty(), "the control: no Voice Demo, no demo");

    let (mut v3, _rec3) = voice();
    let (mut game3, mut demo3) = (offered(), FakeLink { offered: true, ice: Some(ice()), ..Default::default() });
    v3.frame(&mut game3, &mut demo3, w, 0);
    v3.frame(&mut game3, &mut demo3, w, 20_000);
    assert!(demo3.agreed.is_empty(), "a server with voice keeps its own");
}

/// A platform with no media (natively, until crate `voice`'s engine): Voice
/// Chat on tells nothing and says so once; the view is not available.
#[test]
fn no_media_says_so() {
    let mut v = Voice::new(Box::new(NullEngine::default()), false);
    let (mut game, mut demo) = (offered(), FakeLink::default());
    v.frame(&mut game, &mut demo, ptt(false, false), 0);
    v.frame(&mut game, &mut demo, ptt(false, false), 10);
    assert!(game.agreed.is_empty(), "nothing told");
    assert_eq!(v.take_lines().len(), 1);
    assert!(!v.view(ptt(false, false), 10).available);
}
