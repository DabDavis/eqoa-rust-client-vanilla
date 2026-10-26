//! The native engine: ONE connection to EQOA's voice server, on a tokio runtime of its own.
//!
//! The tasks: the connection's (its signals, the data channel's sends and what it hears of the
//! connection, all in order), the encoder's (every 20 ms a frame off the microphone: its level
//! always, and while anything is sent to, Opus-coded ONCE), the writer's (each coded frame as an
//! RTP packet with its audio level), one a slot reading what the server forwards, and the data
//! channel's reader. The game's output mixes the slots in (`VoiceOutput`).
//!
//! The offer, in this order (PROTOCOL.md's agreed details): the sendonly audio, the four recvonly
//! slots, the data channel "eqoa". ICE takes every candidate type (the server's are public; the
//! TURN servers from 0x7E07 are a fallback, over UDP: webrtc-rs has no TURN over TCP, so a
//! `?transport=tcp` URL is left to the page's browser). Only UDP sockets are opened, so no
//! firewall asks the player about a listening port (a player the game reaches over UDP reaches
//! the voice server so too).

use crate::capture::{Capture, Mic};
use crate::jitter::{FRAME, Playback, RATE};
use crate::{IceServer, SignalKind, VoiceEngine, VoiceEvent};
use bytes::Bytes;
use rtc::ice::mdns::MulticastDnsMode;
use rtc::peer_connection::configuration::media_engine::MIME_TYPE_OPUS;
use rtc::rtp;
use rtc::rtp::extension::HeaderExtension;
use rtc::rtp::extension::audio_level_extension::AudioLevelExtension;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RTCRtpHeaderExtensionCapability, RtpCodecKind,
};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::MediaStreamTrack;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceCandidateInit, RTCIceConnectionState, RTCIceServer, RTCIceTransportPolicy, RTCPeerConnectionIceEvent,
    RTCSessionDescription, Registry, SettingEngineBuilder, register_default_interceptors,
};
use webrtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit, RtpSender};

/// The slots the server forwards speakers on.
pub const SLOTS: usize = 4;
/// RFC 6464's header extension: each packet's audio level, by which the server picks the loudest.
pub(crate) const AUDIO_LEVEL_URI: &str = "urn:ietf:params:rtp-hdrext:ssrc-audio-level";
/// A connection not up this long after its offer has failed.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// A level the server sent counts this long.
const HEARD_FOR: Duration = Duration::from_secs(1);
/// Coded frames waiting for the writer at most (a stuck connection drops, never holds the encoder).
const QUEUE: usize = 16;

/// Opus as browsers offer it (opus/48000/2 in SDP, though mono is sent).
pub(crate) fn opus_codec() -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: RATE,
            channels: 2,
            sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 111,
    }
}

/// The media engine both ends use: Opus, and the audio level extension.
pub(crate) fn media_engine() -> webrtc::error::Result<MediaEngine> {
    let mut media = MediaEngine::default();
    media.register_codec(opus_codec(), RtpCodecKind::Audio)?;
    media.register_header_extension(
        RTCRtpHeaderExtensionCapability { uri: AUDIO_LEVEL_URI.to_owned() },
        RtpCodecKind::Audio,
        None,
    )?;
    Ok(media)
}

/// A frame's RMS (full scale 1) as dBFS.
fn dbfs(rms: f32) -> f32 {
    if rms > 0.0 { 20.0 * rms.log10() } else { -200.0 }
}

/// A level for the game: 0..1 = (dBFS + 60) / 60.
pub(crate) fn level_of_rms(rms: f32) -> f32 {
    ((dbfs(rms) + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// A server level (RFC 6464 −dBov: 0 loudest, 127 silence) for the game: (60 − L) / 60.
pub(crate) fn level_of_dbov(level: u8) -> f32 {
    ((60.0 - level as f32) / 60.0).clamp(0.0, 1.0)
}

/// A frame's RFC 6464 level: −dBov, 0 loudest, 127 silence.
pub(crate) fn dbov_of_rms(rms: f32) -> u8 {
    (-dbfs(rms)).round().clamp(0.0, 127.0) as u8
}

fn random_u32() -> u32 {
    std::collections::hash_map::RandomState::new().hash_one(Instant::now()) as u32
}

/// How the engine reaches the server: the game's way, or (tests only) host candidates on
/// loopback with no TURN at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Server,
    #[cfg(test)]
    Loopback,
}

/// Tests only: what stands in for the microphone, kept across starts.
#[cfg(test)]
type Generator = Arc<Mutex<Box<dyn FnMut(&mut [f32]) + Send>>>;

/// Where the encoder's frames come from: the microphone, or (tests only) a generator.
enum Source {
    Mic(Arc<Mic>),
    #[cfg(test)]
    Generator(Generator),
}

/// One coded frame for the writer.
struct Frame {
    data: Bytes,
    timestamp: u32,
    level: u8,
    voice: bool,
}

/// What the tasks, the engine and the output share; it outlives a start and a stop (the
/// game's output keeps a `VoiceOutput`).
pub(crate) struct Shared {
    outgoing: Mutex<Vec<(SignalKind, String)>>,
    events: Mutex<Vec<VoiceEvent>>,
    ice: Mutex<Vec<IceServer>>,
    /// set_send's bits: bit 0 the group, bit 1 the room; 0 silent
    send: AtomicU8,
    /// f32 bits: the last captured frame's RMS
    mic_rms: AtomicU32,
    /// the connection's writer, while there is one
    packets: Mutex<Option<mpsc::Sender<Frame>>>,
    /// who the server says is on each slot
    slots: Mutex<[Option<u32>; SLOTS]>,
    audio: [Mutex<Playback>; SLOTS],
    /// the server's levels, each with when it came
    heard: Mutex<HashMap<u32, (f32, Instant)>>,
    volumes: Mutex<HashMap<u32, f32>>,
    /// `mix_i16`'s float frames, kept so the output's callback does not allocate
    scratch: Mutex<Vec<f32>>,
    /// the run going on (0: stopped); a run's tasks write only while it is theirs, so one
    /// still closing after a stop never reaches the next start's
    run: AtomicU64,
    runs: AtomicU64,
}

impl Shared {
    pub(crate) fn new() -> Shared {
        Shared {
            outgoing: Mutex::default(),
            events: Mutex::default(),
            ice: Mutex::default(),
            send: AtomicU8::new(0),
            mic_rms: AtomicU32::new(0),
            packets: Mutex::default(),
            slots: Mutex::new([None; SLOTS]),
            audio: std::array::from_fn(|_| Mutex::new(Playback::new())),
            heard: Mutex::default(),
            volumes: Mutex::default(),
            scratch: Mutex::default(),
            run: AtomicU64::new(0),
            runs: AtomicU64::new(0),
        }
    }

    /// A message off the data channel: who is on the slots, or the speakers' levels.
    pub(crate) fn server_says(&self, text: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return };
        let pairs = |key: &str| -> Option<Vec<(u64, u64)>> {
            Some(
                v.get(key)?
                    .as_array()?
                    .iter()
                    .filter_map(|p| Some((p.get(0)?.as_u64()?, p.get(1)?.as_u64()?)))
                    .collect(),
            )
        };
        if let Some(slots) = pairs("slots") {
            let mut now = [None; SLOTS];
            for (slot, peer) in slots {
                if let (Ok(slot), Ok(peer)) = (usize::try_from(slot), u32::try_from(peer))
                    && slot < SLOTS
                {
                    now[slot] = Some(peer);
                }
            }
            *self.slots.lock().unwrap() = now;
        }
        if let Some(levels) = pairs("levels") {
            let when = Instant::now();
            let mut heard = self.heard.lock().unwrap();
            heard.retain(|_, (_, at)| when.duration_since(*at) < 10 * HEARD_FOR);
            for (peer, level) in levels {
                if let Ok(peer) = u32::try_from(peer) {
                    heard.insert(peer, (level_of_dbov(level.min(127) as u8), when));
                }
            }
        }
    }

    /// Everything of a session dropped (`stop`).
    fn clear(&self) {
        self.outgoing.lock().unwrap().clear();
        self.events.lock().unwrap().clear();
        *self.packets.lock().unwrap() = None;
        *self.slots.lock().unwrap() = [None; SLOTS];
        for a in &self.audio {
            a.lock().unwrap().reset();
        }
        self.heard.lock().unwrap().clear();
        self.mic_rms.store(0f32.to_bits(), Ordering::Relaxed);
    }
}

/// One start's tasks' view of `Shared`: what they write counts only while the run is theirs.
#[derive(Clone)]
struct Run {
    shared: Arc<Shared>,
    id: u64,
}

impl Run {
    fn live(&self) -> bool {
        self.shared.run.load(Ordering::Relaxed) == self.id
    }

    fn push_out(&self, kind: SignalKind, text: String) {
        if self.live() {
            self.shared.outgoing.lock().unwrap().push((kind, text));
        }
    }

    fn event(&self, event: VoiceEvent) {
        if self.live() {
            self.shared.events.lock().unwrap().push(event);
        }
    }

    fn audio_in(&self, slot: usize, seq: u16, payload: &[u8]) {
        if self.live() {
            self.shared.audio[slot].lock().unwrap().push(seq, payload);
        }
    }

    fn server_says(&self, text: &str) {
        if self.live() {
            self.shared.server_says(text);
        }
    }
}

/// The slots' audio for the game's output: added into a 48 kHz buffer, each slot at the volume
/// of the peer the server says is on it. Cheap to clone; the output's thread keeps one, across
/// the engine's starts and stops.
#[derive(Clone)]
pub struct VoiceOutput(Arc<Shared>);

impl VoiceOutput {
    /// Adds the slots' audio into `out`: 48 kHz, `channels` interleaved, `out.len() /
    /// channels` frames (the same sample on every channel).
    pub fn mix(&self, out: &mut [f32], channels: usize) {
        let slots = *self.0.slots.lock().unwrap();
        let volumes: [f32; SLOTS] = {
            let v = self.0.volumes.lock().unwrap();
            std::array::from_fn(|k| slots[k].and_then(|p| v.get(&p).copied()).unwrap_or(1.0))
        };
        for (audio, volume) in self.0.audio.iter().zip(volumes) {
            let mut a = audio.lock().unwrap();
            a.volume = volume;
            a.mix_into(out, channels);
        }
    }

    /// `mix` into 16-bit frames (the game's mixer's: 48 kHz stereo), saturating.
    pub fn mix_i16(&self, out: &mut [i16], channels: usize) {
        let mut f = self.0.scratch.lock().unwrap();
        f.clear();
        f.resize(out.len(), 0.0);
        self.mix(&mut f, channels);
        for (o, v) in out.iter_mut().zip(f.iter()) {
            *o = (*o as f32 + v * 32767.0).clamp(-32768.0, 32767.0) as i16;
        }
    }
}

enum Cmd {
    Signal(SignalKind, String),
    /// tell the server what is sent to now
    Send,
    /// `start` again: a fresh connection if the last one is down
    Reconnect,
    ChannelOpen(u64),
    Connected(u64),
    Failed(u64),
    Stop,
}

/// The native engine (`VoiceEngine`).
pub struct NativeEngine {
    mode: Mode,
    shared: Arc<Shared>,
    runtime: Option<tokio::runtime::Runtime>,
    commands: Option<mpsc::UnboundedSender<Cmd>>,
    task: Option<tokio::task::JoinHandle<()>>,
    capture: Option<Capture>,
    #[cfg(test)]
    generator: Option<Generator>,
}

impl Default for NativeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeEngine {
    /// The game's engine: the default microphone, the voice server's candidates and the TURN
    /// servers `start` is given.
    pub fn new() -> NativeEngine {
        NativeEngine {
            mode: Mode::Server,
            shared: Arc::new(Shared::new()),
            runtime: None,
            commands: None,
            task: None,
            capture: None,
            #[cfg(test)]
            generator: None,
        }
    }

    /// Tests only: host candidates on loopback, no TURN, `generator` for the microphone.
    #[cfg(test)]
    pub(crate) fn loopback(generator: impl FnMut(&mut [f32]) + Send + 'static) -> NativeEngine {
        let mut engine = NativeEngine::with_generator(generator);
        engine.mode = Mode::Loopback;
        engine
    }

    /// Tests only: the game's network settings, `generator` for the microphone.
    #[cfg(test)]
    pub(crate) fn with_generator(generator: impl FnMut(&mut [f32]) + Send + 'static) -> NativeEngine {
        let mut engine = NativeEngine::new();
        engine.generator = Some(Arc::new(Mutex::new(Box::new(generator))));
        engine
    }

    /// What the game's output mixes in: take it once, it holds across `start` and `stop`.
    pub fn output(&self) -> VoiceOutput {
        VoiceOutput(self.shared.clone())
    }
}

impl VoiceEngine for NativeEngine {
    fn start(&mut self, ice: &[IceServer]) {
        // TURN over UDP only: webrtc-rs relays no other way (the page's browser takes the rest)
        *self.shared.ice.lock().unwrap() = ice
            .iter()
            .filter(|s| s.url.starts_with("stun:") || s.url.starts_with("turn:") && !s.url.contains("transport=tcp"))
            .cloned()
            .collect();
        if let Some(tx) = &self.commands {
            let _ = tx.send(Cmd::Reconnect);
            return;
        }
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("voice")
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                self.shared.events.lock().unwrap().push(VoiceEvent::Note(format!("voice: no runtime ({e})")));
                return;
            }
        };
        let id = self.shared.runs.fetch_add(1, Ordering::Relaxed) + 1;
        self.shared.run.store(id, Ordering::Relaxed);
        let run = Run { shared: self.shared.clone(), id };
        #[cfg(test)]
        let generator = self.generator.clone();
        #[cfg(not(test))]
        let generator: Option<()> = None;
        let source = match generator {
            #[cfg(test)]
            Some(g) => Source::Generator(g),
            _ => {
                let mic = Arc::new(Mic::default());
                let denied = run.clone();
                self.capture = Some(Capture::start(mic.clone(), move |_| denied.event(VoiceEvent::MicDenied)));
                Source::Mic(mic)
            }
        };
        runtime.spawn(encode(run.clone(), source));
        let (tx, rx) = mpsc::unbounded_channel();
        self.task = Some(runtime.spawn(connection_task(self.mode, run, tx.clone(), rx)));
        self.commands = Some(tx);
        self.runtime = Some(runtime);
    }

    fn signal(&mut self, kind: SignalKind, text: &str) {
        if let Some(tx) = &self.commands {
            let _ = tx.send(Cmd::Signal(kind, text.to_owned()));
        }
    }

    fn take_outgoing(&mut self) -> Vec<(SignalKind, String)> {
        std::mem::take(&mut *self.shared.outgoing.lock().unwrap())
    }

    fn set_send(&mut self, channels: u8) {
        if self.shared.send.swap(channels, Ordering::Relaxed) != channels
            && let Some(tx) = &self.commands
        {
            let _ = tx.send(Cmd::Send);
        }
    }

    fn set_peer_volume(&mut self, peer: u32, volume: f32) {
        let volume = if volume.is_nan() { 1.0 } else { volume.clamp(0.0, 2.0) };
        self.shared.volumes.lock().unwrap().insert(peer, volume);
    }

    fn levels(&self) -> Vec<(u32, f32)> {
        let now = Instant::now();
        let mut out: HashMap<u32, f32> = self
            .shared
            .heard
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, (_, when))| now.duration_since(*when) < HEARD_FOR)
            .map(|(&peer, &(level, _))| (peer, level))
            .collect();
        let slots = *self.shared.slots.lock().unwrap();
        for (k, peer) in slots.iter().enumerate() {
            if let Some(peer) = peer {
                let level = level_of_rms(self.shared.audio[k].lock().unwrap().level());
                let e = out.entry(*peer).or_insert(0.0);
                *e = e.max(level);
            }
        }
        let mut out: Vec<(u32, f32)> = out.into_iter().collect();
        out.sort_by_key(|(peer, _)| *peer);
        out
    }

    fn mic_level(&self) -> f32 {
        level_of_rms(f32::from_bits(self.shared.mic_rms.load(Ordering::Relaxed)))
    }

    fn stop(&mut self) {
        self.shared.run.store(0, Ordering::Relaxed);
        if let Some(tx) = self.commands.take() {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(mut c) = self.capture.take() {
            c.stop();
        }
        if let Some(runtime) = self.runtime.take() {
            // the connection closes on the runtime's own threads (its last packets, the
            // allocation released), and the runtime goes after: the game never waits on it
            let task = self.task.take();
            let _ = std::thread::Builder::new().name("voice-stop".into()).spawn(move || {
                if let Some(task) = task {
                    let _ = runtime.block_on(async { tokio::time::timeout(Duration::from_secs(2), task).await });
                }
                runtime.shutdown_timeout(Duration::from_millis(100));
            });
        }
        self.shared.clear();
    }

    fn take_events(&mut self) -> Vec<VoiceEvent> {
        std::mem::take(&mut *self.shared.events.lock().unwrap())
    }
}

impl Drop for NativeEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Every 20 ms: the frames the microphone has (up to three, catching up a late tick; a
/// generator's one), each one's level kept, and while anything is sent to, coded once for the
/// connection's writer.
async fn encode(run: Run, mut source: Source) {
    let shared = &run.shared;
    let mut enc = match opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip) {
        Ok(e) => e,
        Err(e) => {
            run.event(VoiceEvent::Note(format!("voice: no Opus encoder ({e})")));
            return;
        }
    };
    let _ = enc.set_bitrate(opus::Bitrate::Bits(32_000));
    let _ = enc.set_inband_fec(true);
    let _ = enc.set_packet_loss_perc(10);
    let _ = enc.set_dtx(true);
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut frame = vec![0.0f32; FRAME];
    let mut packet = vec![0u8; 1500];
    // the RTP clock: 960 a frame taken, sent or not, so a pause is a gap in time
    let mut timestamp = random_u32();
    let mut was_sending = false;
    let per_tick = match source {
        Source::Mic(_) => 3,
        #[cfg(test)]
        Source::Generator(_) => 1,
    };
    loop {
        tick.tick().await;
        if !run.live() {
            return;
        }
        for _ in 0..per_tick {
            let got = match &mut source {
                Source::Mic(m) => m.take_frame(&mut frame),
                #[cfg(test)]
                Source::Generator(g) => {
                    (g.lock().unwrap())(&mut frame);
                    true
                }
            };
            if !got {
                break;
            }
            let rms = (frame.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt();
            shared.mic_rms.store(rms.to_bits(), Ordering::Relaxed);
            let ts = timestamp;
            timestamp = timestamp.wrapping_add(FRAME as u32);
            let queue = shared.packets.lock().unwrap().clone();
            let sending = shared.send.load(Ordering::Relaxed) != 0 && queue.is_some();
            if sending && !was_sending {
                let _ = enc.reset_state(); // a talk spurt starts clean
            }
            was_sending = sending;
            let Some(queue) = queue.filter(|_| sending) else { continue };
            let Ok(n) = enc.encode_float(&frame, &mut packet) else { continue };
            if n <= 2 {
                continue; // DTX: silence, nothing to send
            }
            let level = dbov_of_rms(rms);
            let frame = Frame { data: Bytes::copy_from_slice(&packet[..n]), timestamp: ts, level, voice: level < 50 };
            if run.live() {
                let _ = queue.try_send(frame);
            }
        }
    }
}

/// The connection's writer: each frame an RTP packet (its own sequence, the encoder's clock, a
/// marker at a talk spurt's start) carrying its audio level.
async fn write(track: Arc<TrackLocalStaticRTP>, ssrc: u32, payload_type: Arc<AtomicU32>, mut frames: mpsc::Receiver<Frame>) {
    let mut seq = random_u32() as u16;
    let mut next_ts = None;
    while let Some(f) = frames.recv().await {
        let Ok(pt) = u8::try_from(payload_type.load(Ordering::Relaxed)) else { continue };
        let marker = next_ts != Some(f.timestamp);
        next_ts = Some(f.timestamp.wrapping_add(FRAME as u32));
        let packet = rtp::Packet {
            header: rtp::header::Header {
                version: 2,
                marker,
                payload_type: pt,
                sequence_number: seq,
                timestamp: f.timestamp,
                ssrc,
                ..Default::default()
            },
            payload: f.data,
        };
        seq = seq.wrapping_add(1);
        let level = HeaderExtension::AudioLevel(AudioLevelExtension { level: f.level, voice: f.voice });
        let _ = track.write_rtp_with_extensions(packet, &[level]).await;
    }
}

/// The connection's events.
struct Handler {
    session: u64,
    commands: mpsc::UnboundedSender<Cmd>,
    run: Run,
    /// the slots' transceiver ids, slot by slot
    slot_ids: Arc<Mutex<Vec<usize>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(init) = event.candidate.to_json()
            && let Ok(json) = serde_json::to_string(&init)
        {
            self.run.push_out(SignalKind::Candidate, json);
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let (run, slot_ids) = (self.run.clone(), self.slot_ids.clone());
        tokio::spawn(async move {
            let mut slot = None;
            while let Some(event) = track.poll().await {
                match event {
                    TrackRemoteEvent::OnOpen(init) => {
                        let id: usize = init.receiver_id.into();
                        slot = slot_ids.lock().unwrap().iter().position(|&s| s == id);
                    }
                    TrackRemoteEvent::OnRtpPacket(p) => {
                        if let Some(k) = slot {
                            run.audio_in(k, p.header.sequence_number, &p.payload);
                        }
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        });
    }

    async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
        let cmd = match state {
            RTCIceConnectionState::Connected | RTCIceConnectionState::Completed => Cmd::Connected(self.session),
            RTCIceConnectionState::Failed => Cmd::Failed(self.session),
            _ => return,
        };
        let _ = self.commands.send(cmd);
    }
}

/// The connection task: a session a connection, each made by `start` (the first, or one after
/// the last went down), until `stop`.
async fn connection_task(mode: Mode, run: Run, commands: mpsc::UnboundedSender<Cmd>, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut session = 0;
    loop {
        session += 1;
        let stop = match Connection::open(mode, &run, session, &commands).await {
            Ok(mut conn) => {
                let stop = conn.run(&run, session, &mut rx).await;
                conn.close(&run.shared).await;
                stop
            }
            Err(e) => {
                run.event(VoiceEvent::Note(format!("voice: no connection ({e})")));
                run.event(VoiceEvent::Failed);
                false
            }
        };
        if stop {
            return;
        }
        // down: until `start` asks again
        loop {
            match rx.recv().await {
                None | Some(Cmd::Stop) => return,
                Some(Cmd::Reconnect) => break,
                _ => {}
            }
        }
    }
}

struct Connection {
    pc: Arc<dyn PeerConnection>,
    sender: Arc<dyn RtpSender>,
    channel: Arc<dyn DataChannel>,
    /// its writer's queue, as `Shared::packets` holds it while this is the connection
    queue: mpsc::Sender<Frame>,
    payload_type: Arc<AtomicU32>,
    channel_open: bool,
    writer: tokio::task::JoinHandle<()>,
    reader: tokio::task::JoinHandle<()>,
}

impl Connection {
    async fn open(mode: Mode, run: &Run, session: u64, commands: &mpsc::UnboundedSender<Cmd>) -> Result<Connection, String> {
        let shared = &run.shared;
        let e = |e: webrtc::error::Error| e.to_string();
        let mut media = media_engine().map_err(e)?;
        let registry = register_default_interceptors(Registry::new(), &mut media).map_err(e)?;
        let ice = shared.ice.lock().unwrap().clone();
        let (config, setting, bind) = match mode {
            Mode::Server => (
                RTCConfigurationBuilder::new()
                    .with_ice_servers(
                        ice.iter()
                            .map(|s| RTCIceServer { urls: vec![s.url.clone()], username: s.username.clone(), credential: s.credential.clone() })
                            .collect(),
                    )
                    .with_ice_transport_policy(RTCIceTransportPolicy::All)
                    .build(),
                SettingEngineBuilder::new().with_multicast_dns_mode(MulticastDnsMode::Disabled).build(),
                "0.0.0.0:0",
            ),
            #[cfg(test)]
            Mode::Loopback => (
                RTCConfigurationBuilder::new().build(),
                SettingEngineBuilder::new()
                    .with_multicast_dns_mode(MulticastDnsMode::Disabled)
                    .with_include_loopback_candidate(true)
                    .build(),
                "127.0.0.1:0",
            ),
        };
        let slot_ids = Arc::new(Mutex::new(Vec::new()));
        let handler = Handler { session, commands: commands.clone(), run: run.clone(), slot_ids: slot_ids.clone() };
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting)
            .with_handler(Arc::new(handler))
            .with_udp_addrs(vec![bind])
            .build()
            .await
            .map_err(e)?;
        let pc: Arc<dyn PeerConnection> = Arc::new(pc);
        // the offer's order (PROTOCOL.md): our voice, the four slots, the data channel
        let ssrc = random_u32().max(1);
        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            "eqoa-voice".to_owned(),
            "voice".to_owned(),
            "voice".to_owned(),
            RtpCodecKind::Audio,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters { ssrc: Some(ssrc), ..Default::default() },
                codec: opus_codec().rtp_codec,
                ..Default::default()
            }],
        )));
        let ours = pc
            .add_transceiver_from_track(
                track.clone() as Arc<dyn TrackLocal>,
                Some(RTCRtpTransceiverInit { direction: RTCRtpTransceiverDirection::Sendonly, streams: vec![], send_encodings: vec![] }),
            )
            .await
            .map_err(e)?;
        let sender = ours.sender().await.map_err(e)?.ok_or("no sender for our voice")?;
        for _ in 0..SLOTS {
            let slot = pc
                .add_transceiver_from_kind(
                    RtpCodecKind::Audio,
                    Some(RTCRtpTransceiverInit { direction: RTCRtpTransceiverDirection::Recvonly, streams: vec![], send_encodings: vec![] }),
                )
                .await
                .map_err(e)?;
            slot_ids.lock().unwrap().push(slot.id());
        }
        let channel = pc.create_data_channel("eqoa", None).await.map_err(e)?;
        let reader = tokio::spawn(read_channel(channel.clone(), session, commands.clone(), run.clone()));
        let payload_type = Arc::new(AtomicU32::new(u32::MAX));
        let (queue, frames) = mpsc::channel(QUEUE);
        let writer = tokio::spawn(write(track, ssrc, payload_type.clone(), frames));
        {
            // only while this run is the engine's (a stop between the two would leave the
            // next start's encoder writing to a connection already closed)
            let mut packets = shared.packets.lock().unwrap();
            if run.live() {
                *packets = Some(queue.clone());
            }
        }
        Ok(Connection { pc, sender, channel, queue, payload_type, channel_open: false, writer, reader })
    }

    /// The offer, then every command in order until the connection is over: true at `stop`.
    async fn run(&mut self, run: &Run, session: u64, rx: &mut mpsc::UnboundedReceiver<Cmd>) -> bool {
        let shared = &run.shared;
        let offer = match self.pc.create_offer(None).await {
            Ok(o) => o,
            Err(err) => {
                run.event(VoiceEvent::Note(format!("voice: no offer ({err})")));
                run.event(VoiceEvent::Failed);
                return false;
            }
        };
        // the offer goes out before its candidates (they come once the description is set)
        run.push_out(SignalKind::Offer, offer.sdp.clone());
        if let Err(err) = self.pc.set_local_description(offer).await {
            run.event(VoiceEvent::Note(format!("voice: the offer refused ({err})")));
            run.event(VoiceEvent::Failed);
            return false;
        }
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        let (mut remote, mut connected) = (false, false);
        let mut early: Vec<RTCIceCandidateInit> = Vec::new();
        loop {
            let cmd = if connected {
                rx.recv().await
            } else {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(cmd) => cmd,
                    Err(_) => {
                        run.event(VoiceEvent::Failed);
                        return false;
                    }
                }
            };
            let Some(cmd) = cmd else { return true };
            match cmd {
                Cmd::Signal(SignalKind::Answer, sdp) if !remote => {
                    let set = match RTCSessionDescription::answer(sdp) {
                        Ok(answer) => self.pc.set_remote_description(answer).await,
                        Err(err) => Err(err),
                    };
                    if let Err(err) = set {
                        run.event(VoiceEvent::Note(format!("voice: the server's answer refused ({err})")));
                        run.event(VoiceEvent::Failed);
                        return false;
                    }
                    remote = true;
                    if let Some(pt) = self.negotiated_payload_type().await {
                        self.payload_type.store(pt as u32, Ordering::Relaxed);
                    }
                    for c in early.drain(..) {
                        let _ = self.pc.add_ice_candidate(c).await;
                    }
                }
                Cmd::Signal(SignalKind::Candidate, json) => match serde_json::from_str::<RTCIceCandidateInit>(&json) {
                    // a browser's end of candidates is an empty one
                    Ok(c) if c.candidate.is_empty() => {}
                    Ok(c) if remote => {
                        let _ = self.pc.add_ice_candidate(c).await;
                    }
                    Ok(c) => early.push(c),
                    Err(_) => {}
                },
                Cmd::Signal(SignalKind::Bye, _) => return false,
                // the client offers: an offer from the server, or a second answer, is ignored
                Cmd::Signal(_, _) => {}
                Cmd::Send => self.tell_send(shared).await,
                Cmd::ChannelOpen(s) if s == session => {
                    self.channel_open = true;
                    self.tell_send(shared).await;
                }
                Cmd::Connected(s) if s == session && !connected => {
                    connected = true;
                    run.event(VoiceEvent::Connected);
                }
                Cmd::Failed(s) if s == session => {
                    run.event(VoiceEvent::Failed);
                    return false;
                }
                Cmd::Stop => return true,
                _ => {}
            }
        }
    }

    async fn negotiated_payload_type(&self) -> Option<u8> {
        self.sender.get_parameters().await.ok()?.rtp_parameters.codecs.first().map(|c| c.payload_type)
    }

    /// `{"send": B}`: what this player sends to now (the server forwards only that).
    async fn tell_send(&self, shared: &Shared) {
        if self.channel_open {
            let text = format!("{{\"send\":{}}}", shared.send.load(Ordering::Relaxed));
            let _ = self.channel.send_text(&text).await;
        }
    }

    async fn close(&self, shared: &Shared) {
        {
            // its own queue only: a run closing after a stop must not take the next start's
            let mut packets = shared.packets.lock().unwrap();
            if packets.as_ref().is_some_and(|q| q.same_channel(&self.queue)) {
                *packets = None;
            }
        }
        self.writer.abort();
        self.reader.abort();
        let _ = self.channel.close().await;
        let _ = self.pc.close().await;
    }
}

/// The data channel's events: its opening for the connection task, the server's messages here.
async fn read_channel(channel: Arc<dyn DataChannel>, session: u64, commands: mpsc::UnboundedSender<Cmd>, run: Run) {
    while let Some(event) = channel.poll().await {
        match event {
            DataChannelEvent::OnOpen => {
                let _ = commands.send(Cmd::ChannelOpen(session));
            }
            DataChannelEvent::OnMessage(m) => {
                if let Ok(text) = std::str::from_utf8(&m.data) {
                    run.server_says(text);
                }
            }
            DataChannelEvent::OnClose => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The levels' scales (PROTOCOL.md's agreed details): 0..1 = (dBFS + 60) / 60 for the game,
    /// −dBov for the wire, (60 − L) / 60 for a server level; silence is 0 and 127.
    #[test]
    fn the_level_scales() {
        let sine = 0.3 / 2f32.sqrt(); // a 0.3 sine's RMS: -13.47 dBFS
        assert!((level_of_rms(sine) - 0.7755).abs() < 0.001, "{}", level_of_rms(sine));
        assert_eq!((level_of_rms(0.0), level_of_rms(0.001), level_of_rms(1.0)), (0.0, 0.0, 1.0));
        assert_eq!((dbov_of_rms(sine), dbov_of_rms(0.0), dbov_of_rms(1.0), dbov_of_rms(2.0)), (13, 127, 0, 0));
        assert_eq!((level_of_dbov(0), level_of_dbov(30), level_of_dbov(60), level_of_dbov(127)), (1.0, 0.5, 0.0, 0.0));
    }

    /// `mix_i16` adds what `mix` does, the same sample on both channels, saturating; nothing
    /// playing adds nothing.
    #[test]
    fn the_output_adds_into_the_games_frames() {
        let shared = Arc::new(Shared::new());
        let out = VoiceOutput(shared.clone());
        let mut frames = vec![100i16; 960 * 2];
        out.mix_i16(&mut frames, 2);
        assert!(frames.iter().all(|&s| s == 100), "silence adds nothing");
        let mut enc = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let mut t = 0;
        for seq in 0..6u16 {
            let tone: Vec<f32> = (0..FRAME)
                .map(|_| {
                    t += 1;
                    0.8 * (2.0 * std::f32::consts::PI * 1000.0 * t as f32 / RATE as f32).sin()
                })
                .collect();
            let packet = enc.encode_vec_float(&tone, 1500).unwrap();
            shared.audio[1].lock().unwrap().push(seq, &packet);
        }
        let mut frames = vec![30000i16; 960 * 2 * 2];
        out.mix_i16(&mut frames, 2);
        assert!(frames.chunks(2).all(|lr| lr[0] == lr[1]), "both channels the same");
        let top = frames[960 * 2..].iter().filter(|&&s| s == 32767).count();
        assert!(top > 100, "the peaks saturate: {top}");
        assert!(frames.iter().all(|&s| s > 0), "and do not wrap below");
    }

    /// The data channel's slots and levels as the server sends them; a slot past the four, a
    /// pair that is not two numbers, and text that is not JSON change nothing.
    #[test]
    fn the_server_says_who_is_on_the_slots_and_how_loud() {
        let shared = Shared::new();
        shared.server_says(r#"{"slots":[[0,7],[3,9],[4,11],["1",5]]}"#);
        assert_eq!(*shared.slots.lock().unwrap(), [Some(7), None, None, Some(9)]);
        shared.server_says(r#"{"slots":[[1,7]]}"#);
        assert_eq!(*shared.slots.lock().unwrap(), [None, Some(7), None, None], "each message is the whole mapping");
        shared.server_says(r#"{"levels":[[7,30],[8,200]]}"#);
        let heard = shared.heard.lock().unwrap().iter().map(|(&p, &(l, _))| (p, l)).collect::<HashMap<_, _>>();
        assert_eq!(heard, HashMap::from([(7, 0.5), (8, 0.0)]));
        shared.server_says("not json");
        shared.server_says(r#"{"other":1}"#);
        assert_eq!(*shared.slots.lock().unwrap(), [None, Some(7), None, None]);
    }
}
