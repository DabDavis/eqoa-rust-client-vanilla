//! Engines against a stand-in for EQOA's voice server, in one process over loopback: webrtc-rs
//! answering each engine's offer as go-eqoa-server's forwarding server does (the client's voice
//! received on its first m-line, a track sent on each of its four slots, the data channel's JSON
//! both ways), forwarding each speaker's packets to the slots a test routes them to. The engines
//! run `Mode::Loopback` (host candidates on 127.0.0.1, no TURN; never the game's) with a tone
//! for a microphone, so CI runs it with no server, no network and no audio device.

use crate::native::{AUDIO_LEVEL_URI, media_engine, opus_codec};
use crate::{NativeEngine, SLOTS, SignalKind, VoiceEngine, VoiceEvent, VoiceOutput};
use rtc::ice::mdns::MulticastDnsMode;
use rtc::rtp;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::MediaStreamTrack;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceCandidateInit,
    RTCPeerConnectionIceEvent, RTCSessionDescription, Registry, SettingEngineBuilder, register_default_interceptors,
};

/// What the stand-in saw of one client.
#[derive(Clone, Debug, Default)]
struct Seen {
    packets: usize,
    /// the last packet's audio level extension: (−dBov, voice)
    level: Option<(u8, bool)>,
    /// the last `{"send": B}`
    send: Option<u64>,
}

/// A slot of a listener's, as the stand-in sends it.
struct SlotOut {
    track: Arc<TrackLocalStaticRTP>,
    ssrc: u32,
    payload_type: u8,
    seq: AtomicU16,
}

#[derive(Default)]
struct Hub {
    outgoing: Mutex<Vec<(u32, SignalKind, String)>>,
    /// speaker -> (listener, slot)
    routes: Mutex<HashMap<u32, Vec<(u32, usize)>>>,
    slots: Mutex<HashMap<u32, Arc<Vec<SlotOut>>>>,
    seen: Mutex<HashMap<u32, Seen>>,
    channels: Mutex<HashMap<u32, Arc<dyn DataChannel>>>,
}

/// The stand-in server, on a runtime of its own.
struct StandIn {
    rt: tokio::runtime::Runtime,
    hub: Arc<Hub>,
    inbox: HashMap<u32, mpsc::UnboundedSender<(SignalKind, String)>>,
}

impl StandIn {
    fn new() -> StandIn {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        StandIn { rt, hub: Arc::default(), inbox: HashMap::new() }
    }

    /// A client's signal, handled in its order.
    fn signal(&mut self, client: u32, kind: SignalKind, text: String) {
        let (rt, hub) = (&self.rt, &self.hub);
        let tx = self.inbox.entry(client).or_insert_with(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            rt.spawn(serve(client, hub.clone(), rx));
            tx
        });
        let _ = tx.send((kind, text));
    }

    fn route(&self, speaker: u32, listener: u32, slot: usize) {
        self.hub.routes.lock().unwrap().entry(speaker).or_default().push((listener, slot));
    }

    /// A data channel message to `client`.
    fn say(&self, client: u32, json: &str) {
        let channel = self.hub.channels.lock().unwrap().get(&client).cloned().expect("the client's data channel");
        let json = json.to_owned();
        self.rt.spawn(async move { channel.send_text(&json).await });
    }

    fn seen(&self, client: u32) -> Seen {
        self.hub.seen.lock().unwrap().get(&client).cloned().unwrap_or_default()
    }

    fn channels(&self) -> usize {
        self.hub.channels.lock().unwrap().len()
    }
}

async fn serve(client: u32, hub: Arc<Hub>, mut rx: mpsc::UnboundedReceiver<(SignalKind, String)>) {
    let mut pc: Option<Arc<dyn PeerConnection>> = None;
    let mut early = Vec::new();
    while let Some((kind, text)) = rx.recv().await {
        match kind {
            SignalKind::Offer => {
                if let Some(old) = pc.take() {
                    let _ = old.close().await; // an offer again: the client started over
                }
                let new = answer(client, &hub, text).await.expect("the stand-in answers");
                for c in early.drain(..) {
                    let _ = new.add_ice_candidate(c).await;
                }
                pc = Some(new);
            }
            SignalKind::Candidate => {
                let c: RTCIceCandidateInit = serde_json::from_str(&text).expect("a candidate's JSON");
                match &pc {
                    _ if c.candidate.is_empty() => {}
                    Some(pc) => {
                        let _ = pc.add_ice_candidate(c).await;
                    }
                    None => early.push(c),
                }
            }
            SignalKind::Bye => {
                if let Some(old) = pc.take() {
                    let _ = old.close().await;
                }
            }
            SignalKind::Answer => panic!("a client never answers"),
        }
    }
    if let Some(pc) = pc {
        let _ = pc.close().await;
    }
}

/// The server's half: the answer, a track on each of the client's slots.
async fn answer(client: u32, hub: &Arc<Hub>, sdp: String) -> webrtc::error::Result<Arc<dyn PeerConnection>> {
    let level_id: u8 = sdp
        .lines()
        .filter_map(|l| l.strip_prefix("a=extmap:"))
        .find(|l| l.contains(AUDIO_LEVEL_URI))
        .and_then(|l| l.split([' ', '/']).next()?.parse().ok())
        .expect("the offer carries the audio level extension");
    let mut media = media_engine()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;
    let handler = HubHandler { client, hub: hub.clone(), level_id };
    let pc = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_setting_engine(
            SettingEngineBuilder::new()
                .with_multicast_dns_mode(MulticastDnsMode::Disabled)
                .with_include_loopback_candidate(true)
                .build(),
        )
        .with_handler(Arc::new(handler))
        .with_udp_addrs(vec!["127.0.0.1:0"])
        .build()
        .await?;
    let pc: Arc<dyn PeerConnection> = Arc::new(pc);
    pc.set_remote_description(RTCSessionDescription::offer(sdp)?).await?;
    // a track a m-line: the first takes the client's voice's (answered recvonly, never sent
    // on), the next four its slots'
    let mut senders = Vec::new();
    for i in 0..=SLOTS as u32 {
        let ssrc = 1000 * client + i + 1;
        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            format!("hub-{client}"),
            format!("slot-{i}"),
            format!("slot-{i}"),
            RtpCodecKind::Audio,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters { ssrc: Some(ssrc), ..Default::default() },
                codec: opus_codec().rtp_codec,
                ..Default::default()
            }],
        )));
        let sender = pc.add_track(track.clone() as Arc<dyn TrackLocal>).await?;
        senders.push((track, ssrc, sender));
    }
    let answer = pc.create_answer(None).await?;
    hub.outgoing.lock().unwrap().push((client, SignalKind::Answer, answer.sdp.clone()));
    pc.set_local_description(answer).await?;
    let mut outs = Vec::new();
    for (track, ssrc, sender) in senders.into_iter().skip(1) {
        let payload_type = sender.get_parameters().await?.rtp_parameters.codecs.first().map_or(111, |c| c.payload_type);
        outs.push(SlotOut { track, ssrc, payload_type, seq: AtomicU16::new(0) });
    }
    hub.slots.lock().unwrap().insert(client, Arc::new(outs));
    Ok(pc)
}

struct HubHandler {
    client: u32,
    hub: Arc<Hub>,
    level_id: u8,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for HubHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(init) = event.candidate.to_json()
            && let Ok(json) = serde_json::to_string(&init)
        {
            self.hub.outgoing.lock().unwrap().push((self.client, SignalKind::Candidate, json));
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let (client, hub, level_id) = (self.client, self.hub.clone(), self.level_id);
        tokio::spawn(async move {
            while let Some(event) = track.poll().await {
                let TrackRemoteEvent::OnRtpPacket(p) = event else { continue };
                {
                    let mut seen = hub.seen.lock().unwrap();
                    let s = seen.entry(client).or_default();
                    s.packets += 1;
                    if let Some(b) = p.header.get_extension(level_id).filter(|b| b.len() == 1) {
                        s.level = Some((b[0] & 0x7F, b[0] & 0x80 != 0));
                    }
                }
                let routes = hub.routes.lock().unwrap().get(&client).cloned().unwrap_or_default();
                for (listener, slot) in routes {
                    let Some(outs) = hub.slots.lock().unwrap().get(&listener).cloned() else { continue };
                    let out = &outs[slot];
                    let packet = rtp::Packet {
                        header: rtp::header::Header {
                            version: 2,
                            marker: p.header.marker,
                            payload_type: out.payload_type,
                            sequence_number: out.seq.fetch_add(1, Ordering::Relaxed),
                            timestamp: p.header.timestamp,
                            ssrc: out.ssrc,
                            ..Default::default()
                        },
                        payload: p.payload.clone(),
                    };
                    let _ = out.track.write_rtp(packet).await;
                }
            }
        });
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        self.hub.channels.lock().unwrap().insert(self.client, channel.clone());
        let (client, hub) = (self.client, self.hub.clone());
        tokio::spawn(async move {
            while let Some(event) = channel.poll().await {
                if let DataChannelEvent::OnMessage(m) = event
                    && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&m.data)
                    && let Some(b) = v.get("send").and_then(|b| b.as_u64())
                {
                    hub.seen.lock().unwrap().entry(client).or_default().send = Some(b);
                }
            }
        });
    }
}

/// A sine at `freq`, 0.3 full scale (RMS 0.212, −13.5 dBFS), for a microphone.
fn tone(freq: f64) -> impl FnMut(&mut [f32]) + Send {
    let mut t = 0u64;
    move |frame| {
        for s in frame {
            *s = (0.3 * (2.0 * std::f64::consts::PI * freq * t as f64 / 48_000.0).sin()) as f32;
            t += 1;
        }
    }
}

/// The power at `freq` (Goertzel), per sample squared.
fn power(x: &[f32], freq: f64) -> f64 {
    let c = 2.0 * (2.0 * std::f64::consts::PI * freq / 48_000.0).cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s = v as f64 + c * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let n = x.len().max(1) as f64;
    (s1 * s1 + s2 * s2 - c * s1 * s2) / (n * n)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

struct Client {
    id: u32,
    engine: NativeEngine,
    output: VoiceOutput,
    /// what its output played, 48 kHz mono, pulled in real time
    heard: Vec<f32>,
}

/// The engines, the stand-in, the signaling between them, and each output pulled as a sound
/// card would.
struct Net {
    hub: StandIn,
    clients: Vec<Client>,
    t0: Instant,
    pulled: u64,
}

impl Net {
    fn new(clients: Vec<(u32, NativeEngine)>) -> Net {
        let clients = clients
            .into_iter()
            .map(|(id, mut engine)| {
                engine.start(&[]);
                Client { id, output: engine.output(), engine, heard: Vec::new() }
            })
            .collect();
        Net { hub: StandIn::new(), clients, t0: Instant::now(), pulled: 0 }
    }

    fn engine(&mut self, id: u32) -> &mut NativeEngine {
        &mut self.clients.iter_mut().find(|c| c.id == id).expect("a client").engine
    }

    /// The last `ms` its output played.
    fn tail(&self, id: u32, ms: usize) -> &[f32] {
        let heard = &self.clients.iter().find(|c| c.id == id).expect("a client").heard;
        &heard[heard.len().saturating_sub(ms * 48)..]
    }

    fn step(&mut self) {
        for c in &mut self.clients {
            for (kind, text) in c.engine.take_outgoing() {
                self.hub.signal(c.id, kind, text);
            }
        }
        let back = std::mem::take(&mut *self.hub.hub.outgoing.lock().unwrap());
        for (id, kind, text) in back {
            if let Some(c) = self.clients.iter_mut().find(|c| c.id == id) {
                c.engine.signal(kind, &text);
            }
        }
        let due = (self.t0.elapsed().as_secs_f64() * 48_000.0) as u64 - self.pulled;
        if due >= 240 {
            for c in &mut self.clients {
                let mut buf = vec![0.0f32; due as usize];
                c.output.mix(&mut buf, 1);
                c.heard.extend(buf);
            }
            self.pulled += due;
        }
    }

    fn run(&mut self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            self.step();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn run_until(&mut self, ms: u64, mut done: impl FnMut(&Net) -> bool) -> bool {
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
}

/// The offer's m-lines in the agreed order: our voice (sendonly), the four slots (recvonly),
/// the data channel, with Opus and the audio level extension; nothing goes out before
/// `start` and nothing after `stop`.
#[test]
fn the_offer_is_in_the_agreed_order() {
    let mut engine = NativeEngine::loopback(tone(440.0));
    assert!(engine.take_outgoing().is_empty(), "nothing before start");
    engine.start(&[]);
    let end = Instant::now() + Duration::from_secs(5);
    let offer = loop {
        if let Some((_, sdp)) = engine.take_outgoing().into_iter().find(|(k, _)| *k == SignalKind::Offer) {
            break sdp;
        }
        assert!(Instant::now() < end, "an offer within 5 s");
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut lines = Vec::new();
    for l in offer.lines() {
        if let Some(m) = l.strip_prefix("m=") {
            lines.push((m.split(' ').next().unwrap_or("").to_owned(), String::new()));
        } else if let Some(last) = lines.last_mut()
            && last.0 == "audio"
            && ["a=sendonly", "a=recvonly", "a=sendrecv", "a=inactive"].contains(&l)
        {
            last.1 = l[2..].to_owned();
        }
    }
    let got: Vec<(&str, &str)> = lines.iter().map(|(m, d)| (m.as_str(), d.as_str())).collect();
    assert_eq!(
        got,
        [
            ("audio", "sendonly"),
            ("audio", "recvonly"),
            ("audio", "recvonly"),
            ("audio", "recvonly"),
            ("audio", "recvonly"),
            ("application", "")
        ],
        "{offer}"
    );
    assert!(offer.contains("opus/48000/2") && offer.contains(AUDIO_LEVEL_URI));
    engine.stop();
    std::thread::sleep(Duration::from_millis(100));
    assert!(engine.take_outgoing().is_empty() && engine.take_events().is_empty(), "nothing after stop");
}

/// Two players through the stand-in, each on a slot of the other's: each hears the other's
/// tone and not its own (the negative control: the other frequency); the server sees the audio
/// level of the frame (13.5 −dBov, voice) and `{"send": 1}`; the levels; push-to-talk off sends
/// nothing and plays silence, the room's bit sends again and is told as is; stop is at once and a
/// start after it connects again, and one right after a stop too.
#[test]
fn a_voice_goes_through_the_server() {
    let mut net = Net::new(vec![(1, NativeEngine::loopback(tone(440.0))), (2, NativeEngine::loopback(tone(880.0)))]);
    net.hub.route(1, 2, 0);
    net.hub.route(2, 1, 2);
    net.engine(1).set_send(1);
    net.engine(2).set_send(1);
    assert!(
        net.run_until(20_000, |n| n.hub.channels() == 2 && rms(n.tail(1, 200)) > 0.05 && rms(n.tail(2, 200)) > 0.05),
        "each hears the other within 20 s"
    );
    net.hub.say(2, r#"{"slots":[[0,1]]}"#);
    net.hub.say(1, r#"{"slots":[[2,2]]}"#);
    net.run(1000);
    let b = net.tail(2, 800);
    assert!(power(b, 440.0) > 30.0 * power(b, 880.0).max(power(b, 620.0)), "2 hears 1's 440 Hz alone");
    let a = net.tail(1, 800);
    assert!(power(a, 880.0) > 30.0 * power(a, 440.0).max(power(a, 620.0)), "1 hears 2's 880 Hz alone");
    let seen = net.hub.seen(1);
    assert!(seen.packets > 40, "{} packets", seen.packets);
    // a 440 Hz frame of 20 ms is 8.8 cycles: its RMS is 0.212 give or take 1.5%, 13.5 -dBov
    assert!(matches!(seen.level, Some((13 | 14, true))), "the frame's level, -dBov: {:?}", seen.level);
    assert_eq!(seen.send, Some(1));
    assert!((net.engine(1).mic_level() - 0.7755).abs() < 0.01, "mic {}", net.engine(1).mic_level());
    let levels = net.engine(2).levels();
    assert!(levels.len() == 1 && levels[0].0 == 1 && levels[0].1 > 0.6, "2 hears 1 talking: {levels:?}");
    for id in [1, 2] {
        let events = net.engine(id).take_events();
        assert_eq!(events, [VoiceEvent::Connected], "{id}'s events");
    }

    net.engine(1).set_send(0);
    net.run(400);
    let before = net.hub.seen(1).packets;
    net.run(600);
    assert_eq!(net.hub.seen(1).packets, before, "nothing sent at 0");
    assert_eq!(net.hub.seen(1).send, Some(0));
    assert!(rms(net.tail(2, 400)) < 1e-4, "2 hears silence");
    assert!(rms(net.tail(1, 400)) > 0.05, "while 1 still hears 2");
    net.engine(1).set_send(2);
    assert!(net.run_until(3000, |n| rms(n.tail(2, 100)) > 0.05), "the room's bit sends again");
    assert_eq!(net.hub.seen(1).send, Some(2), "told as is");

    let t = Instant::now();
    net.engine(1).stop();
    assert!(t.elapsed() < Duration::from_millis(20), "stop took {:?}", t.elapsed());
    assert!(net.engine(1).levels().is_empty() && net.engine(1).mic_level() == 0.0);
    net.run(500);
    assert!(rms(net.tail(2, 200)) < 1e-4, "a stopped engine is silent");
    net.engine(1).start(&[]);
    net.engine(1).set_send(1);
    assert!(net.run_until(20_000, |n| rms(n.tail(2, 200)) > 0.05), "a start after stop connects again");
    assert_eq!(net.engine(1).take_events(), [VoiceEvent::Connected]);

    // a stop and a start at once: a connection again, sending 50 packets a second
    net.engine(1).stop();
    net.engine(1).start(&[]);
    let restarted = net.hub.seen(1).packets;
    assert!(net.run_until(20_000, |n| n.hub.seen(1).packets > restarted + 25), "a start right after a stop connects");
    assert_eq!(net.engine(1).take_events(), [VoiceEvent::Connected]);
    let before = net.hub.seen(1).packets;
    net.run(1000);
    let sent = net.hub.seen(1).packets - before;
    assert!((40..=60).contains(&sent), "{sent} packets a second, not 50");
    let b = net.tail(2, 800);
    assert!(power(b, 440.0) > 30.0 * power(b, 620.0), "and still the tone");
}

/// Four speakers on one listener's four slots, each played at the volume of the peer the
/// server says is on it: all four heard; then 3 muted (gone), 2 at 200% (twice the amplitude),
/// and 99, on no slot, muted (the control: nothing else changes); the speaking marks from the
/// slots, and a server level for a peer on none.
#[test]
fn each_slot_plays_at_its_peers_volume() {
    let freqs = [300.0, 500.0, 700.0, 900.0];
    let mut engines: Vec<(u32, NativeEngine)> = vec![(10, NativeEngine::loopback(|f: &mut [f32]| f.fill(0.0)))];
    for (i, f) in freqs.iter().enumerate() {
        engines.push((i as u32 + 1, NativeEngine::loopback(tone(*f))));
    }
    let mut net = Net::new(engines);
    for i in 1..=4 {
        net.hub.route(i, 10, i as usize - 1);
        net.engine(i).set_send(1);
    }
    assert!(net.run_until(30_000, |n| n.hub.channels() == 5 && rms(n.tail(10, 200)) > 0.2), "the four within 30 s");
    net.hub.say(10, r#"{"slots":[[0,1],[1,2],[2,3],[3,4]]}"#);
    net.run(1200);
    let before = net.tail(10, 1000).to_vec();
    for f in freqs {
        assert!(power(&before, f) > 20.0 * power(&before, f + 100.0), "{f} Hz heard");
    }
    let listener = net.engine(10);
    listener.set_peer_volume(3, 0.0);
    listener.set_peer_volume(2, 2.0);
    listener.set_peer_volume(99, 0.0);
    net.run(1200);
    let after = net.tail(10, 1000).to_vec();
    let gain = |f: f64| (power(&after, f) / power(&before, f)).sqrt();
    assert!(gain(700.0) < 0.01, "3 muted: {}", gain(700.0));
    assert!((gain(500.0) - 2.0).abs() < 0.3, "2 doubled: {}", gain(500.0));
    assert!((gain(300.0) - 1.0).abs() < 0.2 && (gain(900.0) - 1.0).abs() < 0.2, "1 and 4 as they were");
    net.hub.say(10, r#"{"levels":[[42,30]]}"#);
    net.run(200);
    let levels = net.engine(10).levels();
    let peers: Vec<u32> = levels.iter().map(|(p, _)| *p).collect();
    assert_eq!(peers, [1, 2, 3, 4, 42], "{levels:?}");
    assert!(levels[..4].iter().all(|(_, l)| *l > 0.6), "the four talking, muted or not: {levels:?}");
    assert!((levels[4].1 - 0.5).abs() < 1e-6, "42 at 30 -dBov");
}

/// The game's own network settings (every interface's host candidates, mDNS off, no loopback
/// candidate, ICE policy all) reach the stand-in too: a tone through it to a loopback listener.
#[test]
fn the_games_network_settings_connect() {
    let mut net = Net::new(vec![
        (1, NativeEngine::with_generator(tone(440.0))),
        (2, NativeEngine::loopback(|f: &mut [f32]| f.fill(0.0))),
    ]);
    net.hub.route(1, 2, 0);
    net.engine(1).set_send(1);
    assert!(net.run_until(20_000, |n| rms(n.tail(2, 200)) > 0.05), "heard within 20 s");
    net.run(800);
    let b = net.tail(2, 600);
    assert!(power(b, 440.0) > 30.0 * power(b, 620.0), "the tone");
    assert_eq!(net.engine(1).take_events(), [VoiceEvent::Connected]);
}
