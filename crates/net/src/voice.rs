//! go-eqoa-server's group voice (F3 pass 18; no retail message): the server
//! offers it, a client that has the Voice Chat option on says so, and the
//! server then sends the ICE servers (its TURN relay, credentials it minted)
//! and the player's group members who have voice on; the WebRTC signaling
//! between two members rides the game connection, the media goes through the
//! relay alone (no player sees another's address). A PS2 ignores all of it,
//! as it does [`crate::session::OP_ENTITY_VIEW`]'s offer.
//!
//! On the wire, beside the opcode: integers little-endian, a string a u16 byte
//! length and its UTF-8.
//! - [`OP_VOICE`]: the server's offer, a u32 1, once the player is in the
//!   world; the client's answer, u32 flags ([`FLAG_ON`] voice on, [`FLAG_GUILD`]
//!   in its guild's voice channel; 0 off), at once after the offer and
//!   whenever the options change.
//! - [`OP_VOICE_CONFIG`]: a u8 count, each { url, username, credential }, then
//!   a u32 time to live in seconds (sent again before it runs out).
//! - [`OP_VOICE_PEERS`]: a u8 count, each { u32 peer, name, u8 offerer, u8
//!   channels }: the other members with voice on, each by its voice id;
//!   `offerer` 1 when THIS client makes the offer to that one; `channels` what
//!   joins the two ([`CHANNEL_GROUP`] a groupmate, [`CHANNEL_GUILD`] both in
//!   their guild's channel: one connection a pair). The whole set each time.
//! - [`OP_VOICE_SIGNAL`], both ways: u32 peer (to, or from), u32 seq (the
//!   sender's message number), u8 kind (1 offer, 2 answer, 3 an ICE candidate,
//!   4 bye), u16 part, u16 parts, then a chunk of the message's text (at most
//!   [`CHUNK`] bytes), the parts put back together by (peer, seq).

use crate::stream::{Reader, Writer};
use std::collections::HashMap;

pub const OP_VOICE: u16 = 0x7E06;
pub const OP_VOICE_CONFIG: u16 = 0x7E07;
pub const OP_VOICE_PEERS: u16 = 0x7E08;
pub const OP_VOICE_SIGNAL: u16 = 0x7E09;

/// The most UTF-8 bytes of a message one [`OP_VOICE_SIGNAL`] carries.
pub const CHUNK: usize = 900;
/// The most parts a message is taken in (a session description is a few
/// kilobytes: 64 parts is 57 KB), and the most messages being put together at
/// once (the oldest let go past it).
const MAX_PARTS: u16 = 64;
const MAX_PENDING: usize = 32;

/// [`OP_VOICE`]'s answer: voice on, and in the guild's voice channel (alone
/// it means nothing).
pub const FLAG_ON: u32 = 1;
pub const FLAG_GUILD: u32 = 2;
/// [`OP_VOICE_PEERS`]'s channels: a groupmate, and one in the guild's voice
/// channel with this player.
pub const CHANNEL_GROUP: u8 = 1;
pub const CHANNEL_GUILD: u8 = 2;

/// A signal's kinds (the `kind` byte).
pub const KIND_OFFER: u8 = 1;
pub const KIND_ANSWER: u8 = 2;
pub const KIND_CANDIDATE: u8 = 3;
pub const KIND_BYE: u8 = 4;

/// An ICE server as [`OP_VOICE_CONFIG`] names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireIce {
    pub url: String,
    pub username: String,
    pub credential: String,
}

/// A member with voice on, as [`OP_VOICE_PEERS`] names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WirePeer {
    pub peer: u32,
    pub name: String,
    pub offerer: bool,
    /// [`CHANNEL_GROUP`], [`CHANNEL_GUILD`] or both
    pub channels: u8,
}

/// A string: a u16 byte length and the UTF-8 (cut at a character's end to
/// fit).
pub fn put_str(w: &mut Writer, s: &str) {
    let mut n = s.len().min(u16::MAX as usize);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    w.bytes(&(n as u16).to_le_bytes()).bytes(&s.as_bytes()[..n]);
}

/// A string as [`put_str`] writes it.
pub fn get_str(r: &mut Reader) -> String {
    let n = r.u16() as usize;
    String::from_utf8_lossy(r.bytes(n)).into_owned()
}

/// [`OP_VOICE_CONFIG`]'s body (a server's, for tests).
pub fn encode_config(ice: &[WireIce], ttl: u32) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_CONFIG);
    w.u8(ice.len().min(255) as u8);
    for i in ice.iter().take(255) {
        put_str(&mut w, &i.url);
        put_str(&mut w, &i.username);
        put_str(&mut w, &i.credential);
    }
    w.u32(ttl);
    w.data
}

/// [`OP_VOICE_CONFIG`] read past its opcode: the servers and the time to
/// live, none when it is cut short.
pub fn decode_config(r: &mut Reader) -> Option<(Vec<WireIce>, u32)> {
    let n = r.u8() as usize;
    let ice: Vec<WireIce> = (0..n).map(|_| WireIce { url: get_str(r), username: get_str(r), credential: get_str(r) }).collect();
    let ttl = r.u32();
    (!r.error).then_some((ice, ttl))
}

/// [`OP_VOICE_PEERS`]'s body (a server's, for tests).
pub fn encode_peers(peers: &[WirePeer]) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_PEERS);
    w.u8(peers.len().min(255) as u8);
    for p in peers.iter().take(255) {
        w.u32(p.peer);
        put_str(&mut w, &p.name);
        w.u8(p.offerer as u8).u8(p.channels);
    }
    w.data
}

/// [`OP_VOICE_PEERS`] read past its opcode, none when it is cut short.
pub fn decode_peers(r: &mut Reader) -> Option<Vec<WirePeer>> {
    let n = r.u8() as usize;
    let peers: Vec<WirePeer> = (0..n)
        .map(|_| {
            let peer = r.u32();
            let name = get_str(r);
            let offerer = r.u8() != 0;
            WirePeer { peer, name, offerer, channels: r.u8() }
        })
        .collect();
    (!r.error).then_some(peers)
}

/// A message's [`OP_VOICE_SIGNAL`]s: its text cut into parts of at most
/// [`CHUNK`] bytes, each at a character's end (an empty text is one empty
/// part).
pub fn encode_signal(peer: u32, seq: u32, kind: u8, text: &str) -> Vec<Vec<u8>> {
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = text;
    loop {
        let mut n = rest.len().min(CHUNK);
        while !rest.is_char_boundary(n) {
            n -= 1;
        }
        parts.push(&rest[..n]);
        rest = &rest[n..];
        if rest.is_empty() {
            break;
        }
    }
    let total = parts.len().min(MAX_PARTS as usize) as u16;
    parts
        .iter()
        .take(total as usize)
        .enumerate()
        .map(|(i, chunk)| {
            let mut w = Writer::op(OP_VOICE_SIGNAL);
            w.u32(peer).u32(seq).u8(kind).bytes(&(i as u16).to_le_bytes()).bytes(&total.to_le_bytes());
            put_str(&mut w, chunk);
            w.data
        })
        .collect()
}

/// One [`OP_VOICE_SIGNAL`] read past its opcode: peer, seq, kind, part,
/// parts and the chunk; none when it is cut short or makes no sense.
pub fn decode_signal(r: &mut Reader) -> Option<(u32, u32, u8, u16, u16, String)> {
    let (peer, seq, kind) = (r.u32(), r.u32(), r.u8());
    let (part, parts) = (r.u16(), r.u16());
    let chunk = get_str(r);
    let sane = !r.error && (KIND_OFFER..=KIND_BYE).contains(&kind) && parts >= 1 && parts <= MAX_PARTS && part < parts;
    sane.then_some((peer, seq, kind, part, parts, chunk))
}

/// Messages being put back together from their parts, by (peer, seq).
#[derive(Debug, Default)]
pub struct Reassembly {
    pending: HashMap<(u32, u32), Pending>,
    /// the order they began in, for letting the oldest go
    order: Vec<(u32, u32)>,
}

#[derive(Debug)]
struct Pending {
    kind: u8,
    parts: Vec<Option<String>>,
}

impl Reassembly {
    /// A part taken in: the whole message (peer, kind, text) once its last
    /// part is in, in any order; a part that does not fit what came before
    /// (another count or kind) starts the message over.
    pub fn take(&mut self, peer: u32, seq: u32, kind: u8, part: u16, parts: u16, chunk: String) -> Option<(u32, u8, String)> {
        if parts == 1 {
            return Some((peer, kind, chunk));
        }
        let key = (peer, seq);
        let fits = self.pending.get(&key).is_some_and(|p| p.kind == kind && p.parts.len() == parts as usize);
        if !fits {
            self.pending.insert(key, Pending { kind, parts: vec![None; parts as usize] });
            self.order.retain(|k| *k != key);
            self.order.push(key);
            while self.order.len() > MAX_PENDING {
                let old = self.order.remove(0);
                self.pending.remove(&old);
            }
        }
        let p = self.pending.get_mut(&key)?;
        p.parts[part as usize] = Some(chunk);
        if p.parts.iter().any(Option::is_none) {
            return None;
        }
        let p = self.pending.remove(&key)?;
        self.order.retain(|k| *k != key);
        Some((peer, p.kind, p.parts.into_iter().flatten().collect()))
    }

    /// Everything being put together let go (a new connection).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.order.clear();
    }

    /// How many messages are being put together.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(m: &[u8]) -> Reader<'_> {
        let mut r = Reader::new(m);
        r.u16();
        r
    }

    /// The config and the peers read back what a server writes, byte for byte
    /// as the contract spells them; cut short, neither is taken (the
    /// controls).
    #[test]
    fn config_and_peers_round_trip() {
        let ice = vec![
            WireIce { url: "turn:104.156.237.19:3479?transport=udp".into(), username: "1761500000:77".into(), credential: "c3VyZQ==".into() },
            WireIce { url: "turn:104.156.237.19:3479?transport=tcp".into(), username: "1761500000:77".into(), credential: "c3VyZQ==".into() },
        ];
        let m = encode_config(&ice, 86400);
        assert_eq!(&m[..3], &[0x07, 0x7E, 2]);
        assert_eq!(&m[3..5], &(ice[0].url.len() as u16).to_le_bytes(), "a string's length is a u16");
        assert_eq!(decode_config(&mut body(&m)), Some((ice.clone(), 86400)));
        assert_eq!(decode_config(&mut body(&m[..m.len() - 1])), None, "cut short");

        let peers = vec![
            WirePeer { peer: 9, name: "Jibrish".into(), offerer: true, channels: CHANNEL_GROUP },
            WirePeer { peer: 12, name: "Dabsha".into(), offerer: false, channels: CHANNEL_GROUP | CHANNEL_GUILD },
        ];
        let m = encode_peers(&peers);
        assert_eq!(m, [&[0x08, 0x7E, 2, 9, 0, 0, 0, 7, 0][..], b"Jibrish", &[1, 1, 12, 0, 0, 0, 6, 0], b"Dabsha", &[0, 3]].concat());
        assert_eq!(decode_peers(&mut body(&m)), Some(peers));
        assert_eq!(decode_peers(&mut body(&m[..m.len() - 2])), None, "cut short");
        assert_eq!(decode_peers(&mut body(&encode_peers(&[]))), Some(Vec::new()), "the empty set");
    }

    /// A long description goes in parts of at most CHUNK bytes, cut at a
    /// character's end, and comes back whole in any order; one part alone
    /// is the whole; a part of another count starts the message over; a
    /// kind outside 1-4 or a part past the count is refused (the controls).
    #[test]
    fn a_signal_goes_in_parts_and_comes_back_whole() {
        let text: String = (0..2500).map(|i| if i % 97 == 0 { 'é' } else { (b'a' + (i % 26) as u8) as char }).collect();
        let msgs = encode_signal(7, 41, KIND_OFFER, &text);
        assert_eq!(msgs.len(), 3);
        let mut decoded: Vec<_> = msgs.iter().map(|m| decode_signal(&mut body(m)).expect("a part")).collect();
        for (_, _, _, _, _, chunk) in &decoded {
            assert!(chunk.len() <= CHUNK, "{}", chunk.len());
        }
        assert_eq!(decoded.iter().map(|d| d.5.as_str()).collect::<String>(), text, "cut at characters' ends");
        decoded.swap(0, 2);
        let mut re = Reassembly::default();
        let mut got = None;
        for (peer, seq, kind, part, parts, chunk) in decoded {
            assert!(got.is_none(), "whole before its last part");
            got = re.take(peer, seq, kind, part, parts, chunk);
        }
        assert_eq!(got, Some((7, KIND_OFFER, text.clone())));
        assert!(re.is_empty());

        let one = encode_signal(7, 42, KIND_CANDIDATE, "{\"candidate\":\"x\"}");
        assert_eq!(one.len(), 1);
        let (p, s, k, part, parts, c) = decode_signal(&mut body(&one[0])).unwrap();
        assert_eq!(re.take(p, s, k, part, parts, c), Some((7, KIND_CANDIDATE, "{\"candidate\":\"x\"}".into())));
        assert_eq!(encode_signal(7, 43, KIND_BYE, "").len(), 1, "an empty bye is one part");

        // a part of another count: the message starts over
        assert_eq!(re.take(7, 50, KIND_ANSWER, 0, 2, "ab".into()), None);
        assert_eq!(re.take(7, 50, KIND_ANSWER, 1, 3, "cd".into()), None);
        assert_eq!(re.take(7, 50, KIND_ANSWER, 0, 3, "ab".into()), None);
        assert_eq!(re.take(7, 50, KIND_ANSWER, 2, 3, "ef".into()), Some((7, KIND_ANSWER, "abcdef".into())));

        let mut bad = encode_signal(7, 44, KIND_OFFER, "x").remove(0);
        bad[10] = 9; // kind
        assert_eq!(decode_signal(&mut body(&bad)), None, "a kind outside 1-4");
        let mut past = encode_signal(7, 45, KIND_OFFER, "x").remove(0);
        past[11] = 5; // part 5 of 1
        assert_eq!(decode_signal(&mut body(&past)), None, "a part past the count");
    }

    /// Messages begun and never finished are let go past MAX_PENDING, the
    /// oldest first.
    #[test]
    fn unfinished_messages_are_let_go() {
        let mut re = Reassembly::default();
        for seq in 0..(MAX_PENDING as u32 + 5) {
            assert_eq!(re.take(1, seq, KIND_OFFER, 0, 2, "a".into()), None);
        }
        assert_eq!(re.len(), MAX_PENDING);
        assert_eq!(re.take(1, 0, KIND_OFFER, 1, 2, "b".into()), None, "the oldest went: its last part starts it over");
        assert_eq!(re.take(1, MAX_PENDING as u32 + 4, KIND_OFFER, 1, 2, "b".into()), Some((1, KIND_OFFER, "ab".into())));
    }
}
