//! The voice engine's interface: what voice asks of a platform's media (a
//! page's WebRTC, `web::voice`; natively crate `voice`'s). The manager
//! (`voice::Voice`) drives it from the server's messages and the player's
//! keys; the engine owns the microphone, the ONE connection to the server's
//! forwarding server (go-eqoa-server's `internal/voice`: the client's offer,
//! one sendonly audio transceiver, four recvonly ones (the slots) and a data
//! channel "eqoa"), and the voices it plays.
//!
//! The types and the trait are PROTOCOL.md's (voice v2) and crate `voice`'s
//! (`voice_engine` here): natively its `NativeEngine`, in a page this
//! client's `web::voice`; `take_events` has a default body, so an engine
//! without it still fits.
//!
//! A level, the engine's and the forwarding server's alike, is a loudness
//! from 0 to 1: (dBFS + 60) / 60, clamped, so 0 is -60 dBFS and below (a
//! silence) and 1 full scale; the server's `levels` (RFC 6464's -dBov, 0 the
//! loudest, 127 a silence) come to (60 - L) / 60.

pub use voice_engine::{IceServer, SignalKind, VoiceEngine, VoiceEvent};

/// A signal's kind as its byte on the wire (`net::voice::KIND_*`).
pub fn kind_byte(k: SignalKind) -> u8 {
    match k {
        SignalKind::Offer => net::voice::KIND_OFFER,
        SignalKind::Answer => net::voice::KIND_ANSWER,
        SignalKind::Candidate => net::voice::KIND_CANDIDATE,
        SignalKind::Bye => net::voice::KIND_BYE,
    }
}

/// A wire byte's kind, none for one outside 1-4.
pub fn kind_of(b: u8) -> Option<SignalKind> {
    Some(match b {
        net::voice::KIND_OFFER => SignalKind::Offer,
        net::voice::KIND_ANSWER => SignalKind::Answer,
        net::voice::KIND_CANDIDATE => SignalKind::Candidate,
        net::voice::KIND_BYE => SignalKind::Bye,
        _ => return None,
    })
}

/// An engine that does nothing but remember what it was asked: tests.
#[cfg(test)]
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

#[cfg(test)]
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
