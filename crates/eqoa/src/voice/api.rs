//! The voice engine's interface: what voice asks of a platform's media (a
//! page's WebRTC, `web::voice`; natively crate `voice`'s). The manager
//! (`voice::Voice`) drives it from the server's messages and the player's
//! keys; the engine owns the microphone, the ONE connection to the server's
//! forwarding server (go-eqoa-server's `internal/voice`: the client's offer,
//! one sendonly audio transceiver, four recvonly ones (the slots) and a data
//! channel "eqoa"), and the voices it plays.
//!
//! The types and the trait are PROTOCOL.md's (voice v2), kept identical to
//! crate `voice`'s (lane voicenat18's native engine, F3 pass 18) until the two
//! are one; `take_events` is the client's addition, with a default body, so
//! an engine without it still fits.
//!
//! A level, the engine's and the forwarding server's alike, is a loudness
//! from 0 to 1: (dBFS + 60) / 60, clamped, so 0 is -60 dBFS and below (a
//! silence) and 1 full scale; the server's `levels` (RFC 6464's -dBov, 0 the
//! loudest, 127 a silence) come to (60 - L) / 60.

/// An ICE server (the fallback TURN relay, with the credentials the server
/// minted).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IceServer {
    pub url: String,
    pub username: String,
    pub credential: String,
}

/// What a signal carries: the client's session description (its offer), the
/// server's answer, an ICE candidate (JSON of RTCIceCandidateInit), or the
/// end of the connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalKind {
    Offer,
    Answer,
    Candidate,
    Bye,
}

impl SignalKind {
    /// The kind's byte on the wire (`net::voice::KIND_*`).
    pub fn byte(self) -> u8 {
        match self {
            SignalKind::Offer => net::voice::KIND_OFFER,
            SignalKind::Answer => net::voice::KIND_ANSWER,
            SignalKind::Candidate => net::voice::KIND_CANDIDATE,
            SignalKind::Bye => net::voice::KIND_BYE,
        }
    }

    /// A wire byte's kind, none for one outside 1-4.
    pub fn of(b: u8) -> Option<SignalKind> {
        Some(match b {
            net::voice::KIND_OFFER => SignalKind::Offer,
            net::voice::KIND_ANSWER => SignalKind::Answer,
            net::voice::KIND_CANDIDATE => SignalKind::Candidate,
            net::voice::KIND_BYE => SignalKind::Bye,
            _ => return None,
        })
    }
}

/// What an engine has to tell the player (a chat line each, `voice::Voice`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceEvent {
    /// the connection to the forwarding server is up
    Connected,
    /// it could not be made, or was lost
    Failed,
    /// the microphone was not allowed (or there is none): the player hears,
    /// and is not heard
    MicDenied,
    /// a line of the engine's own
    Note(String),
}

/// A platform's voice media: one connection to the forwarding server.
pub trait VoiceEngine {
    /// The connection made: the microphone asked for, the offer made (taken
    /// by `take_outgoing`), `ice` the fallback relay.
    fn start(&mut self, ice: &[IceServer]);
    /// The server's answer and candidates.
    fn signal(&mut self, kind: SignalKind, text: &str);
    /// The offer and the candidates for the server since the last call.
    fn take_outgoing(&mut self) -> Vec<(SignalKind, String)>;
    /// The channels the microphone goes to now, as the forwarding server
    /// numbers them (bit 0 the group, bit 1 the guild, bit 2 the room); 0
    /// silent, the microphone's track off.
    fn set_send(&mut self, channels: u8);
    /// `peer`'s voice at `volume` (0 silent, 1 its own level, 2 twice), on
    /// whichever slot carries it.
    fn set_peer_volume(&mut self, peer: u32, volume: f32);
    /// Each speaker's level now (the server's levels, the slots').
    fn levels(&self) -> Vec<(u32, f32)>;
    /// The microphone's level now, sent or not (open mic's voice detection
    /// reads it).
    fn mic_level(&self) -> f32;
    fn stop(&mut self);
    /// What the engine has to tell the player since the last call.
    fn take_events(&mut self) -> Vec<VoiceEvent> {
        Vec::new()
    }
}

/// An engine that does nothing but remember what it was asked: tests.
#[derive(Debug, Default)]
pub struct NullEngine {
    pub started: Vec<Vec<IceServer>>,
    pub signals: Vec<(SignalKind, String)>,
    pub outgoing: Vec<(SignalKind, String)>,
    pub send: Vec<u8>,
    pub volumes: Vec<(u32, f32)>,
    pub levels: Vec<(u32, f32)>,
    pub mic: f32,
    pub events: Vec<VoiceEvent>,
    pub stopped: u32,
}

impl VoiceEngine for NullEngine {
    fn start(&mut self, ice: &[IceServer]) {
        self.started.push(ice.to_vec());
    }
    fn signal(&mut self, kind: SignalKind, text: &str) {
        self.signals.push((kind, text.to_string()));
    }
    fn take_outgoing(&mut self) -> Vec<(SignalKind, String)> {
        std::mem::take(&mut self.outgoing)
    }
    fn set_send(&mut self, channels: u8) {
        self.send.push(channels);
    }
    fn set_peer_volume(&mut self, peer: u32, volume: f32) {
        self.volumes.retain(|&(p, _)| p != peer);
        self.volumes.push((peer, volume));
    }
    fn levels(&self) -> Vec<(u32, f32)> {
        self.levels.clone()
    }
    fn mic_level(&self) -> f32 {
        self.mic
    }
    fn stop(&mut self) {
        self.stopped += 1;
    }
    fn take_events(&mut self) -> Vec<VoiceEvent> {
        std::mem::take(&mut self.events)
    }
}
