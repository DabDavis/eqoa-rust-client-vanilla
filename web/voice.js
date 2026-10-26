// Voice chat in the page (crates/eqoa/src/voice, driven through web::voice):
// ONE RTCPeerConnection to the game server's forwarding server (go-eqoa-server's
// internal/voice), whose candidates are public, with the fallback TURN relay the
// server sends (0x7E11) as an ICE server (iceTransportPolicy "all"). No player
// connects to another: every voice goes through the server.
//
// The client makes the offer, in this m-line order (as go-eqoa-server's
// forwarding server reads it): one sendonly audio transceiver (the microphone: Opus, mono, DTX,
// the RFC 6464 audio-level header extension the browser adds for audio), then
// four recvonly ones (the slots: slot i is the i-th recvonly m-line), then the
// data channel "eqoa": {"send": B} out (bit 0 the group, bit 1 the room; 0
// silent), {"slots": [[slot, peer], ...]} and {"levels": [[peer, L], ...]} in
// (L RFC 6464's -dBov: 0 the loudest, 127 silence).
//
// The microphone is asked for only when voice starts (getUserMedia with echo
// cancellation, noise suppression and automatic gain), and goes out only while
// `send` is not 0 (push-to-talk, open mic's voice detection: the sent track's
// enabled flag). Each slot plays through a GainNode at the volume of the peer
// on it. A level, here as in the client, is (dBFS + 60) / 60 clamped to 0..1.

const SLOTS = 4;
// a level the server sent counts this long
const LEVEL_FRESH_MS = 600;
// the offer waits this long for the microphone (a permission asked), then goes
// without it; the microphone joins the connection when it comes
const MIC_WAIT_MS = 10000;
// the most the sent voice takes, bits a second
const MAX_BITRATE = 32000;

const v = {
  pc: null,
  dc: null,
  // the microphone, its analyser, and the copy of its track that is sent
  mic: null,
  micAnalyser: null,
  micTrack: null,
  sender: null,
  ctx: null,
  slots: [],
  volumes: new Map(),
  levels: new Map(),
  send: 0,
  outgoing: [],
  events: [],
  connected: false,
  remoteSet: false,
  pending: [],
  // a start's number: what an older connection's callbacks bring is dropped
  gen: 0,
};

const buf = new Float32Array(1024);

// The loudness of what an analyser hears now, 0..1.
function levelOf(an) {
  if (!an) return 0;
  const n = Math.min(buf.length, an.fftSize);
  const b = buf.subarray(0, n);
  an.getFloatTimeDomainData(b);
  let sum = 0;
  for (let i = 0; i < n; i++) sum += b[i] * b[i];
  const rms = Math.sqrt(sum / n);
  if (!(rms > 0)) return 0;
  return Math.max(0, Math.min(1, (20 * Math.log10(rms) + 60) / 60));
}

function ctx() {
  if (!v.ctx) {
    try {
      v.ctx = new AudioContext({ latencyHint: 'interactive' });
    } catch {
      return null;
    }
  }
  if (v.ctx.state === 'suspended') v.ctx.resume().catch(() => {});
  return v.ctx;
}

// a context the browser kept suspended starts on the next key or click
for (const ev of ['pointerdown', 'keydown']) {
  addEventListener(ev, () => { if (v.ctx && v.ctx.state === 'suspended') v.ctx.resume().catch(() => {}); });
}

// Opus as the client wants it from the browser's encoder: mono, DTX, at most
// MAX_BITRATE. The encoder takes these from the far end's description, so
// they go into the server's answer as it is set here (the server never sees
// it changed).
function mungeAnswer(sdp) {
  const want = [['usedtx', '1'], ['stereo', '0'], ['sprop-stereo', '0'], ['maxaveragebitrate', String(MAX_BITRATE)]];
  const sections = sdp.split(/\r\n(?=m=)/);
  return sections.map((sec) => {
    const lines = sec.split('\r\n');
    const opus = [];
    for (const l of lines) {
      const m = /^a=rtpmap:(\d+) opus\/48000/i.exec(l);
      if (m) opus.push(m[1]);
    }
    for (const pt of opus) {
      const at = lines.findIndex((l) => l.startsWith(`a=fmtp:${pt} `));
      if (at >= 0) {
        const params = lines[at].slice(`a=fmtp:${pt} `.length).split(';').map((p) => p.trim()).filter(Boolean);
        const keys = new Set(params.map((p) => p.split('=')[0].trim().toLowerCase()));
        for (const [k, val] of want) if (!keys.has(k)) params.push(`${k}=${val}`);
        lines[at] = `a=fmtp:${pt} ${params.join(';')}`;
      } else {
        const r = lines.findIndex((l) => l.startsWith(`a=rtpmap:${pt} `));
        lines.splice(r + 1, 0, `a=fmtp:${pt} ${want.map(([k, val]) => `${k}=${val}`).join(';')}`);
      }
    }
    return lines.join('\r\n');
  }).join('\r\n');
}

function sendNow() {
  if (v.dc && v.dc.readyState === 'open') {
    try {
      v.dc.send(JSON.stringify({ send: v.send }));
    } catch {}
  }
}

function applyGains() {
  for (const s of v.slots) {
    if (!s.gain) continue;
    const vol = s.peer == null ? 1 : (v.volumes.has(s.peer) ? v.volumes.get(s.peer) : 1);
    s.gain.gain.value = Math.max(0, Math.min(2, vol));
  }
}

function attachMic() {
  if (!v.mic || !v.sender) return;
  const t = v.mic.getAudioTracks()[0];
  if (!t) return;
  if (v.micTrack) v.micTrack.stop();
  v.micTrack = t.clone();
  v.micTrack.enabled = v.send !== 0;
  v.sender.replaceTrack(v.micTrack).catch(() => {});
}

async function askMic(gen) {
  if (v.mic) {
    attachMic();
    return;
  }
  if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
    v.events.push(['mic-denied']);
    return;
  }
  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
      video: false,
    });
    if (gen !== v.gen) {
      for (const t of stream.getTracks()) t.stop();
      return;
    }
    v.mic = stream;
    const c = ctx();
    if (c) {
      const src = c.createMediaStreamSource(stream);
      v.micAnalyser = c.createAnalyser();
      v.micAnalyser.fftSize = 1024;
      src.connect(v.micAnalyser);
    }
    attachMic();
  } catch (e) {
    if (gen === v.gen) {
      console.log(`voice: the microphone: ${e && e.name ? e.name : e}`);
      v.events.push(['mic-denied']);
    }
  }
}

async function offer(gen) {
  const pc = v.pc;
  if (!pc || gen !== v.gen) return;
  try {
    await pc.setLocalDescription(await pc.createOffer());
    if (gen === v.gen) v.outgoing.push([1, pc.localDescription.sdp]);
  } catch (e) {
    if (gen === v.gen) {
      console.warn('voice: the offer failed', e);
      v.events.push(['failed']);
    }
  }
}

// A slot's voice: played through a GainNode at its peer's volume, heard by
// an analyser for its level. (Chrome sends a remote track through Web Audio
// only while a media element holds it too: a muted one does.)
function slotTrack(e) {
  const s = v.slots.find((x) => x.transceiver === e.transceiver);
  const c = ctx();
  if (!s || !c) return;
  const stream = e.streams && e.streams[0] ? e.streams[0] : new MediaStream([e.track]);
  if (s.source) s.source.disconnect();
  if (s.audio) s.audio.srcObject = null;
  s.audio = new Audio();
  s.audio.muted = true;
  s.audio.srcObject = stream;
  s.audio.play().catch(() => {});
  s.source = c.createMediaStreamSource(stream);
  s.gain = c.createGain();
  s.analyser = c.createAnalyser();
  s.analyser.fftSize = 512;
  s.source.connect(s.analyser);
  s.source.connect(s.gain);
  s.gain.connect(c.destination);
  applyGains();
}

// Each forwarded packet carries its speaker's voice id as its one CSRC: the
// newest a slot's receiver saw names who is on it now, ahead of {"slots"}.
function slotsFromCsrc() {
  const now = performance.now();
  let changed = false;
  for (const s of v.slots) {
    const r = s.transceiver && s.transceiver.receiver;
    if (!r || !r.getContributingSources) continue;
    let best = null;
    for (const c of r.getContributingSources()) if (!best || c.timestamp > best.timestamp) best = c;
    if (best && now - best.timestamp < 1000 && best.source !== s.peer) {
      s.peer = best.source;
      changed = true;
    }
  }
  if (changed) applyGains();
}

function fromServer(data) {
  let m;
  try {
    m = JSON.parse(data);
  } catch {
    return;
  }
  if (Array.isArray(m.slots)) {
    for (const s of v.slots) s.peer = null;
    for (const [slot, peer] of m.slots) if (v.slots[slot]) v.slots[slot].peer = peer;
    applyGains();
  }
  if (Array.isArray(m.levels)) {
    const now = performance.now();
    v.levels = new Map(m.levels.map(([peer, l]) => [peer, [Math.max(0, Math.min(1, (60 - l) / 60)), now]]));
  }
}

function stop() {
  v.gen++;
  if (v.dc) {
    try { v.dc.close(); } catch {}
  }
  if (v.pc) {
    try { v.pc.close(); } catch {}
  }
  for (const s of v.slots) {
    if (s.source) s.source.disconnect();
    if (s.gain) s.gain.disconnect();
    if (s.audio) {
      s.audio.pause();
      s.audio.srcObject = null;
    }
  }
  if (v.micTrack) v.micTrack.stop();
  if (v.mic) for (const t of v.mic.getTracks()) t.stop();
  Object.assign(v, { pc: null, dc: null, mic: null, micAnalyser: null, micTrack: null, sender: null, slots: [], levels: new Map(), connected: false, remoteSet: false, pending: [], outgoing: [] });
}

window.eqoaVoice = {
  // the one connection, its ICE servers a JSON array of {urls, username, credential}
  start(iceJson) {
    stop();
    const gen = v.gen;
    if (typeof RTCPeerConnection === 'undefined') {
      v.events.push(['note', 'Voice chat: this browser has no WebRTC.']);
      v.events.push(['failed']);
      return;
    }
    let ice = [];
    try {
      ice = JSON.parse(iceJson).filter((s) => s && s.urls);
    } catch {}
    ctx();
    const config = { iceTransportPolicy: 'all', bundlePolicy: 'max-bundle', rtcpMuxPolicy: 'require' };
    let pc;
    try {
      pc = new RTCPeerConnection({ ...config, iceServers: ice });
    } catch (e) {
      // a relay the browser will not take: without it (the server's own
      // candidates are public)
      console.warn('voice: the relay was refused', e);
      pc = new RTCPeerConnection(config);
    }
    v.pc = pc;
    v.sender = pc.addTransceiver('audio', { direction: 'sendonly' }).sender;
    for (let i = 0; i < SLOTS; i++) v.slots.push({ transceiver: pc.addTransceiver('audio', { direction: 'recvonly' }), peer: null });
    const dc = pc.createDataChannel('eqoa');
    v.dc = dc;
    dc.onopen = () => { if (gen === v.gen) sendNow(); };
    dc.onmessage = (e) => { if (gen === v.gen) fromServer(e.data); };
    pc.onicecandidate = (e) => { if (gen === v.gen && e.candidate) v.outgoing.push([3, JSON.stringify(e.candidate.toJSON())]); };
    pc.ontrack = (e) => { if (gen === v.gen) slotTrack(e); };
    pc.onconnectionstatechange = () => {
      if (gen !== v.gen) return;
      const s = pc.connectionState;
      if (s === 'connected' && !v.connected) {
        v.connected = true;
        v.events.push(['connected']);
        const p = v.sender.getParameters();
        if (p.encodings && p.encodings[0]) {
          p.encodings[0].maxBitrate = MAX_BITRATE;
          v.sender.setParameters(p).catch(() => {});
        }
      } else if (s === 'failed' || (s === 'closed' && v.connected)) {
        v.events.push(['failed']);
      }
    };
    // the microphone first, so the offer carries its track; after a while
    // without it, the offer goes and the microphone joins when it comes
    Promise.race([askMic(gen), new Promise((done) => setTimeout(done, MIC_WAIT_MS))]).then(() => offer(gen));
  },
  // the server's answer (2) and candidates (3); a bye (4) is the client's to act on
  signal(kind, text) {
    const pc = v.pc;
    if (!pc) return;
    const gen = v.gen;
    if (kind === 2) {
      pc.setRemoteDescription({ type: 'answer', sdp: mungeAnswer(text) }).then(() => {
        if (gen !== v.gen) return;
        v.remoteSet = true;
        for (const c of v.pending.splice(0)) pc.addIceCandidate(c).catch(() => {});
      }).catch((e) => {
        if (gen === v.gen) {
          console.warn('voice: the answer was refused', e);
          v.events.push(['failed']);
        }
      });
    } else if (kind === 3) {
      let c;
      try {
        c = JSON.parse(text);
      } catch {
        return;
      }
      if (v.remoteSet) pc.addIceCandidate(c).catch(() => {});
      else if (v.pending.length < 64) v.pending.push(c);
    }
  },
  takeOutgoing: () => JSON.stringify(v.outgoing.splice(0)),
  setSend(b) {
    v.send = b | 0;
    if (v.micTrack) v.micTrack.enabled = v.send !== 0;
    sendNow();
  },
  setPeerVolume(peer, vol) {
    v.volumes.set(peer, vol);
    applyGains();
  },
  // each speaker's level: the server's, or its slot's where that is louder
  levels() {
    slotsFromCsrc();
    const now = performance.now();
    const out = new Map();
    for (const [peer, [l, at]] of v.levels) if (now - at < LEVEL_FRESH_MS) out.set(peer, l);
    for (const s of v.slots) {
      if (s.peer == null || !s.analyser) continue;
      out.set(s.peer, Math.max(out.get(s.peer) || 0, levelOf(s.analyser)));
    }
    return JSON.stringify([...out]);
  },
  micLevel: () => levelOf(v.micAnalyser),
  stop,
  takeEvents: () => JSON.stringify(v.events.splice(0)),
  // for a report and the page's tests
  stats: () => ({
    audio: v.ctx ? v.ctx.state : 'none',
    connection: v.pc ? v.pc.connectionState : 'none',
    channel: v.dc ? v.dc.readyState : 'none',
    microphone: v.mic ? 'on' : 'none',
    sending: v.micTrack ? v.micTrack.enabled : false,
    send: v.send,
    slots: v.slots.map((s) => s.peer),
    gains: v.slots.map((s) => (s.gain ? Math.round(s.gain.gain.value * 100) / 100 : null)),
    levels: [...v.levels].map(([p, [l]]) => [p, Math.round(l * 100) / 100]),
  }),
  mungeAnswer,
};
