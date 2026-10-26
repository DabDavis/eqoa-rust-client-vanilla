//! Voice chat (F3 pass 18; the user: group voice and a lobby of rooms, passworded ones too, "build
//! our own in go with pion", "build the voice sfu into the go-eqoa-server"): the engine behind
//! [`VoiceEngine`], the interface the game's voice (crates/eqoa: the protocol, the options, the
//! push-to-talk keys, the group bars) drives both here and in a page.
//!
//! The wire contract is go-eqoa-server's (EQOA voice v2). Every player holds ONE connection, to
//! the forwarding server inside go-eqoa-server, so no player learns another's address. The game
//! connection carries the signaling (0x7E09 VoiceSignal; 0x7E07 VoiceConfig hands the fallback
//! TURN servers): the client offers, the server answers. The texts an engine takes and hands out
//! are what a browser's RTCPeerConnection sends, SDP for the offer and the answer and the JSON of
//! RTCIceCandidateInit for a candidate, so a native client and a page are the same to the server.
//!
//! The offer: one sendonly Opus track (mono, 48 kHz, 32 kbps, DTX, in-band FEC, each packet with
//! the RFC 6464 audio level of its frame, by which the server picks the loudest), FOUR recvonly
//! audio slots (slot k is the k-th recvonly m-line; the server forwards on them the four loudest
//! speakers this player shares a channel with), and the data channel "eqoa": `{"send": B}` to the
//! server (the channels sent to: bit 0 the group, bit 1 the room; 0 silent), and
//! from it `{"slots": [[slot, peer], ...]}` (who is on each slot) and `{"levels": [[peer, level],
//! ...]}` (RFC 6464's −dBov, 0 the loudest, 127 silence) for the speaking marks. The levels the
//! engine hands the game, the microphone's and each peer's, are 0..1 = (dBFS + 60) / 60, so a
//! server level L is (60 − L) / 60 (as go-eqoa-server's forwarding server sends them). A bye from the server ends the
//! connection; the game's voice then stops the engine and starts it again, with a backoff.
//!
//! Natively, [`NativeEngine`] is WebRTC (webrtc-rs: ICE over UDP, DTLS-SRTP, SCTP), Opus through
//! libopus, the default microphone through cpal, and [`VoiceOutput`], which the game's own audio
//! output mixes in at 48 kHz, each slot at the volume of the peer on it. It runs on a runtime of
//! its own: no call here blocks the game's frame. On wasm32 this crate is the interface alone; the
//! page's engine is the browser's own WebRTC.

/// A TURN server as 0x7E07 names it: `turn:<host>:<port>?transport=udp` (or tcp), with the
/// credential the game server minted for this session. A fallback: the voice server's own
/// candidates are public.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IceServer {
    pub url: String,
    pub username: String,
    pub credential: String,
}

/// What a 0x7E09 carries: the offer's or the answer's SDP, a candidate's RTCIceCandidateInit
/// JSON, or bye (the connection is over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalKind {
    Offer,
    Answer,
    Candidate,
    Bye,
}

/// What the engine has to tell the player (a chat line each).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceEvent {
    /// the connection to the voice server is up (ICE connected)
    Connected,
    /// the connection could not be made, or was lost (ICE failed)
    Failed,
    /// the microphone could not be opened (none, or not allowed)
    MicDenied,
    /// a line of the engine's own
    Note(String),
}

/// What the game's voice drives (EQOA voice v2; one connection to the forwarding server).
pub trait VoiceEngine {
    fn start(&mut self, ice: &[IceServer]);                // the one connection, the offer made
    fn signal(&mut self, kind: SignalKind, text: &str);    // the server's answer and candidates
    fn take_outgoing(&mut self) -> Vec<(SignalKind, String)>;
    fn set_send(&mut self, channels: u8);                  // bit 0 group, bit 1 room; 0 silent
    fn set_peer_volume(&mut self, peer: u32, volume: f32);
    fn levels(&self) -> Vec<(u32, f32)>;                   // per peer, from the slots and "levels"
    fn mic_level(&self) -> f32;
    fn stop(&mut self);
    fn take_events(&mut self) -> Vec<VoiceEvent> {
        Vec::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod capture;
#[cfg(not(target_arch = "wasm32"))]
mod jitter;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod live;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{NativeEngine, SLOTS, VoiceOutput};
