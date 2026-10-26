//! go-eqoa-server's group voice and its voice lobby (F3 pass 18; no retail
//! message): the server offers it, a client that has the Voice Chat option on
//! says so, and the server then sends a fallback relay (its TURN server,
//! credentials it minted) and the roster of whom the player may hear: its
//! group, and the room of the lobby it is in (rooms anyone makes, open or
//! locked by a password the server never sees). The voice itself goes through
//! the server's own forwarding server (go-eqoa-server's `internal/voice`), ONE
//! WebRTC connection a player, its signaling riding the game connection: no
//! player sees another's address. A PS2 ignores all of it, as it does
//! [`crate::session::OP_ENTITY_VIEW`]'s offer.
//!
//! On the wire, beside the opcode: integers little-endian, a string a u16 byte
//! length and its UTF-8.
//! - [`OP_VOICE`]: the server's offer, a u32 1, once the player is in the
//!   world; the client's answer, u32 [`FLAG_ON`] (voice on) or 0, at once
//!   after the offer and whenever the option changes.
//! - [`OP_VOICE_CONFIG`]: a u8 count, each { url, username, credential }, then
//!   a u32 time to live in seconds (sent again before it runs out).
//! - [`OP_VOICE_ROSTER`]: a u8 count, each { u32 peer, name, u8 channels }:
//!   whom the player may hear, each by its voice id (stable for its session),
//!   `channels` what joins the two ([`CHANNEL_GROUP`] a groupmate,
//!   [`CHANNEL_ROOM`] in the player's room). The whole set each time.
//! - [`OP_VOICE_SIGNAL`], both ways, between the client and the forwarding
//!   server: u32 seq (the sender's message number), u8 kind (1 the client's
//!   offer, 2 the server's answer, 3 an ICE candidate, 4 bye), u16 part, u16
//!   parts, then a chunk of the message's text (at most [`CHUNK`] bytes), the
//!   parts put back together by seq.
//! - [`OP_VOICE_ROOM`], the client's: u8 op, 1 join (a string name, a string
//!   H: the lowercase hex of SHA-256("eqoa-voice-room\n" + the name folded +
//!   "\n" + the password), empty for an open room, then u8 flags,
//!   [`ROOM_UNLISTED`] the room made unlisted when this join makes it), 2
//!   leave, 3 the lobby.
//! - [`OP_VOICE_LOBBY`]: u16 count, each { name, u16 members, u8 locked }:
//!   every listed room, after a list and after the player's own join or
//!   leave (an unlisted one is found only by its name).
//! - [`OP_VOICE_ROOM_STATE`]: u8 result ([`ROOM_IN`], [`ROOM_WRONG_PASSWORD`],
//!   [`ROOM_REFUSED`], [`ROOM_LEFT`]) and the room's name.

use crate::stream::{Reader, Writer};
use std::collections::HashMap;

// a block of their own: 0x7E00 is the entity view's, 0x7E01-0x7E02 the
// power bars', 0x7E03-0x7E06 the quest marks' and tracker's
pub const OP_VOICE: u16 = 0x7E10;
pub const OP_VOICE_CONFIG: u16 = 0x7E11;
pub const OP_VOICE_ROSTER: u16 = 0x7E12;
pub const OP_VOICE_SIGNAL: u16 = 0x7E13;
pub const OP_VOICE_ROOM: u16 = 0x7E14;
pub const OP_VOICE_LOBBY: u16 = 0x7E15;
pub const OP_VOICE_ROOM_STATE: u16 = 0x7E16;

/// The most UTF-8 bytes of a message one [`OP_VOICE_SIGNAL`] carries.
pub const CHUNK: usize = 900;
/// The most parts a message is taken in (a session description is a few
/// kilobytes: 64 parts is 57 KB), and the most messages being put together at
/// once (the oldest let go past it).
pub const MAX_PARTS: u16 = 64;
const MAX_PENDING: usize = 32;

/// [`OP_VOICE`]'s answer: voice on.
pub const FLAG_ON: u32 = 1;
/// [`OP_VOICE_ROSTER`]'s channels (and the forwarding server's `send`): a
/// groupmate, one in the player's room.
pub const CHANNEL_GROUP: u8 = 1;
pub const CHANNEL_ROOM: u8 = 2;

/// [`OP_VOICE_ROOM`]'s ops.
pub const ROOM_JOIN: u8 = 1;
pub const ROOM_LEAVE: u8 = 2;
pub const ROOM_LIST: u8 = 3;
/// A join's flags: the room it makes is unlisted (a later join's is ignored).
pub const ROOM_UNLISTED: u8 = 1;
/// [`OP_VOICE_ROOM_STATE`]'s results.
pub const ROOM_IN: u8 = 0;
pub const ROOM_WRONG_PASSWORD: u8 = 1;
pub const ROOM_REFUSED: u8 = 2;
pub const ROOM_LEFT: u8 = 3;

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

/// One the player may hear, as [`OP_VOICE_ROSTER`] names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireMember {
    pub peer: u32,
    pub name: String,
    /// [`CHANNEL_GROUP`], [`CHANNEL_ROOM`] or both
    pub channels: u8,
}

/// A room of the lobby, as [`OP_VOICE_LOBBY`] names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireRoom {
    pub name: String,
    pub members: u16,
    pub locked: bool,
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

/// [`OP_VOICE_ROSTER`]'s body (a server's, for tests).
pub fn encode_roster(roster: &[WireMember]) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_ROSTER);
    w.u8(roster.len().min(255) as u8);
    for m in roster.iter().take(255) {
        w.u32(m.peer);
        put_str(&mut w, &m.name);
        w.u8(m.channels);
    }
    w.data
}

/// [`OP_VOICE_ROSTER`] read past its opcode, none when it is cut short.
pub fn decode_roster(r: &mut Reader) -> Option<Vec<WireMember>> {
    let n = r.u8() as usize;
    let roster: Vec<WireMember> = (0..n)
        .map(|_| {
            let peer = r.u32();
            let name = get_str(r);
            WireMember { peer, name, channels: r.u8() }
        })
        .collect();
    (!r.error).then_some(roster)
}

/// [`OP_VOICE_ROOM`]'s join: the room's name, H (empty: an open room) and
/// whether a room it makes is unlisted.
pub fn encode_join(name: &str, hash: &str, unlisted: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_ROOM);
    w.u8(ROOM_JOIN);
    put_str(&mut w, name);
    put_str(&mut w, hash);
    w.u8(if unlisted { ROOM_UNLISTED } else { 0 });
    w.data
}

/// [`OP_VOICE_ROOM`]'s leave or list.
pub fn encode_room_op(op: u8) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_ROOM);
    w.u8(op);
    w.data
}

/// [`OP_VOICE_LOBBY`]'s body (a server's, for tests).
pub fn encode_lobby(rooms: &[WireRoom]) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_LOBBY);
    w.bytes(&(rooms.len().min(u16::MAX as usize) as u16).to_le_bytes());
    for r in rooms {
        put_str(&mut w, &r.name);
        w.bytes(&r.members.to_le_bytes()).u8(r.locked as u8);
    }
    w.data
}

/// [`OP_VOICE_LOBBY`] read past its opcode, none when it is cut short.
pub fn decode_lobby(r: &mut Reader) -> Option<Vec<WireRoom>> {
    let n = r.u16() as usize;
    let mut rooms = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        let name = get_str(r);
        let members = r.u16();
        let locked = r.u8() != 0;
        if r.error {
            return None;
        }
        rooms.push(WireRoom { name, members, locked });
    }
    (!r.error).then_some(rooms)
}

/// [`OP_VOICE_ROOM_STATE`]'s body (a server's, for tests).
pub fn encode_room_state(result: u8, name: &str) -> Vec<u8> {
    let mut w = Writer::op(OP_VOICE_ROOM_STATE);
    w.u8(result);
    put_str(&mut w, name);
    w.data
}

/// [`OP_VOICE_ROOM_STATE`] read past its opcode: the result and the room's
/// name, none when it is cut short or the result is not one of the four.
pub fn decode_room_state(r: &mut Reader) -> Option<(u8, String)> {
    let result = r.u8();
    let name = get_str(r);
    (!r.error && result <= ROOM_LEFT).then_some((result, name))
}

/// A message's text cut into parts of at most [`CHUNK`] bytes, each at a
/// character's end (an empty text is one empty part), at most [`MAX_PARTS`].
pub fn split(text: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = text;
    loop {
        let mut n = rest.len().min(CHUNK);
        while !rest.is_char_boundary(n) {
            n -= 1;
        }
        parts.push(&rest[..n]);
        rest = &rest[n..];
        if rest.is_empty() || parts.len() == MAX_PARTS as usize {
            break;
        }
    }
    parts
}

/// A message's [`OP_VOICE_SIGNAL`]s, one a part ([`split`]).
pub fn encode_signal(seq: u32, kind: u8, text: &str) -> Vec<Vec<u8>> {
    let parts = split(text);
    let total = parts.len() as u16;
    parts
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut w = Writer::op(OP_VOICE_SIGNAL);
            w.u32(seq).u8(kind).bytes(&(i as u16).to_le_bytes()).bytes(&total.to_le_bytes());
            put_str(&mut w, chunk);
            w.data
        })
        .collect()
}

/// Whether a part's numbers make sense: a kind in 1-4, part within parts,
/// parts within [`MAX_PARTS`].
pub fn sane(kind: u8, part: u16, parts: u16) -> bool {
    (KIND_OFFER..=KIND_BYE).contains(&kind) && parts >= 1 && parts <= MAX_PARTS && part < parts
}

/// One [`OP_VOICE_SIGNAL`] read past its opcode: seq, kind, part, parts and
/// the chunk; none when it is cut short or makes no sense.
pub fn decode_signal(r: &mut Reader) -> Option<(u32, u8, u16, u16, String)> {
    let (seq, kind) = (r.u32(), r.u8());
    let (part, parts) = (r.u16(), r.u16());
    let chunk = get_str(r);
    (!r.error && sane(kind, part, parts)).then_some((seq, kind, part, parts, chunk))
}

/// Messages being put back together from their parts, by seq.
#[derive(Debug, Default)]
pub struct Reassembly {
    pending: HashMap<u32, Pending>,
    /// the order they began in, for letting the oldest go
    order: Vec<u32>,
}

#[derive(Debug)]
struct Pending {
    kind: u8,
    parts: Vec<Option<String>>,
}

impl Reassembly {
    /// A part taken in: the whole message (kind, text) once its last part is
    /// in, in any order; a part that does not fit what came before (another
    /// count or kind) starts the message over.
    pub fn take(&mut self, seq: u32, kind: u8, part: u16, parts: u16, chunk: String) -> Option<(u8, String)> {
        if parts == 1 {
            return Some((kind, chunk));
        }
        let fits = self.pending.get(&seq).is_some_and(|p| p.kind == kind && p.parts.len() == parts as usize);
        if !fits {
            self.pending.insert(seq, Pending { kind, parts: vec![None; parts as usize] });
            self.order.retain(|k| *k != seq);
            self.order.push(seq);
            while self.order.len() > MAX_PENDING {
                let old = self.order.remove(0);
                self.pending.remove(&old);
            }
        }
        let p = self.pending.get_mut(&seq)?;
        *p.parts.get_mut(part as usize)? = Some(chunk);
        if p.parts.iter().any(Option::is_none) {
            return None;
        }
        let p = self.pending.remove(&seq)?;
        self.order.retain(|k| *k != seq);
        Some((p.kind, p.parts.into_iter().flatten().collect()))
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

    /// The config and the roster read back what a server writes, byte for
    /// byte as the contract spells them; cut short, neither is taken (the
    /// controls).
    #[test]
    fn config_and_roster_round_trip() {
        let ice = vec![
            WireIce { url: "turn:104.156.237.19:3479?transport=udp".into(), username: "1761500000:77".into(), credential: "c3VyZQ==".into() },
            WireIce { url: "turn:104.156.237.19:3479?transport=tcp".into(), username: "1761500000:77".into(), credential: "c3VyZQ==".into() },
        ];
        let m = encode_config(&ice, 86400);
        assert_eq!(&m[..3], &[0x11, 0x7E, 2]);
        assert_eq!(&m[3..5], &(ice[0].url.len() as u16).to_le_bytes(), "a string's length is a u16");
        assert_eq!(decode_config(&mut body(&m)), Some((ice.clone(), 86400)));
        assert_eq!(decode_config(&mut body(&m[..m.len() - 1])), None, "cut short");

        let roster = vec![
            WireMember { peer: 9, name: "Jibrish".into(), channels: CHANNEL_GROUP },
            WireMember { peer: 12, name: "Dabsha".into(), channels: CHANNEL_GROUP | CHANNEL_ROOM },
        ];
        let m = encode_roster(&roster);
        assert_eq!(m, [&[0x12, 0x7E, 2, 9, 0, 0, 0, 7, 0][..], b"Jibrish", &[1, 12, 0, 0, 0, 6, 0], b"Dabsha", &[3]].concat());
        assert_eq!(decode_roster(&mut body(&m)), Some(roster));
        assert_eq!(decode_roster(&mut body(&m[..m.len() - 1])), None, "cut short");
        assert_eq!(decode_roster(&mut body(&encode_roster(&[]))), Some(Vec::new()), "the empty set");
    }

    /// The lobby's messages as the contract spells them: a join with its
    /// name and H, an open room's H empty, leave and list a byte; the lobby
    /// and a room's state read back; cut short or a result past 3, neither
    /// is taken (the controls).
    #[test]
    fn the_lobby_s_messages() {
        let h = "ab".repeat(32);
        assert_eq!(encode_join("Raid", &h, false), [&[0x14, 0x7E, 1, 4, 0][..], b"Raid", &[64, 0], h.as_bytes(), &[0]].concat());
        assert_eq!(encode_join("Raid", "", false), [&[0x14, 0x7E, 1, 4, 0][..], b"Raid", &[0, 0, 0]].concat(), "an open room: H empty");
        assert_eq!(encode_join("Raid", "", true), [&[0x14, 0x7E, 1, 4, 0][..], b"Raid", &[0, 0, ROOM_UNLISTED]].concat(), "unlisted: the flags' bit 0");
        assert_eq!(encode_room_op(ROOM_LEAVE), [0x14, 0x7E, 2]);
        assert_eq!(encode_room_op(ROOM_LIST), [0x14, 0x7E, 3]);
        let rooms = vec![WireRoom { name: "Guild Hall".into(), members: 12, locked: true }, WireRoom { name: "Raid".into(), members: 3, locked: false }];
        let m = encode_lobby(&rooms);
        assert_eq!(&m[..4], &[0x15, 0x7E, 2, 0], "a u16 count");
        assert_eq!(decode_lobby(&mut body(&m)), Some(rooms));
        assert_eq!(decode_lobby(&mut body(&m[..m.len() - 1])), None, "cut short");
        assert_eq!(decode_lobby(&mut body(&encode_lobby(&[]))), Some(Vec::new()));
        let st = encode_room_state(ROOM_WRONG_PASSWORD, "Guild Hall");
        assert_eq!(st, [&[0x16, 0x7E, 1, 10, 0][..], b"Guild Hall"].concat());
        assert_eq!(decode_room_state(&mut body(&st)), Some((ROOM_WRONG_PASSWORD, "Guild Hall".into())));
        assert_eq!(decode_room_state(&mut body(&encode_room_state(4, "x"))), None, "a result past 3");
        assert_eq!(decode_room_state(&mut body(&st[..st.len() - 1])), None, "cut short");
    }

    /// A long description goes in parts of at most CHUNK bytes, cut at a
    /// character's end, and comes back whole in any order; one part alone
    /// is the whole; a part of another count starts the message over; a
    /// kind outside 1-4 or a part past the count is refused (the controls).
    #[test]
    fn a_signal_goes_in_parts_and_comes_back_whole() {
        let text: String = (0..2500).map(|i| if i % 97 == 0 { 'é' } else { (b'a' + (i % 26) as u8) as char }).collect();
        let msgs = encode_signal(41, KIND_OFFER, &text);
        assert_eq!(msgs.len(), 3);
        assert_eq!(&msgs[1][..11], &[0x13, 0x7E, 41, 0, 0, 0, KIND_OFFER, 1, 0, 3, 0], "seq, kind, part, parts: no peer");
        let mut decoded: Vec<_> = msgs.iter().map(|m| decode_signal(&mut body(m)).expect("a part")).collect();
        for (_, _, _, _, chunk) in &decoded {
            assert!(chunk.len() <= CHUNK, "{}", chunk.len());
        }
        assert_eq!(decoded.iter().map(|d| d.4.as_str()).collect::<String>(), text, "cut at characters' ends");
        decoded.swap(0, 2);
        let mut re = Reassembly::default();
        let mut got = None;
        for (seq, kind, part, parts, chunk) in decoded {
            assert!(got.is_none(), "whole before its last part");
            got = re.take(seq, kind, part, parts, chunk);
        }
        assert_eq!(got, Some((KIND_OFFER, text.clone())));
        assert!(re.is_empty());

        let one = encode_signal(42, KIND_CANDIDATE, "{\"candidate\":\"x\"}");
        assert_eq!(one.len(), 1);
        let (s, k, part, parts, c) = decode_signal(&mut body(&one[0])).unwrap();
        assert_eq!(re.take(s, k, part, parts, c), Some((KIND_CANDIDATE, "{\"candidate\":\"x\"}".into())));
        assert_eq!(encode_signal(43, KIND_BYE, "").len(), 1, "an empty bye is one part");

        // a part of another count: the message starts over
        assert_eq!(re.take(50, KIND_ANSWER, 0, 2, "ab".into()), None);
        assert_eq!(re.take(50, KIND_ANSWER, 1, 3, "cd".into()), None);
        assert_eq!(re.take(50, KIND_ANSWER, 0, 3, "ab".into()), None);
        assert_eq!(re.take(50, KIND_ANSWER, 2, 3, "ef".into()), Some((KIND_ANSWER, "abcdef".into())));

        let mut bad = encode_signal(44, KIND_OFFER, "x").remove(0);
        bad[6] = 9; // kind
        assert_eq!(decode_signal(&mut body(&bad)), None, "a kind outside 1-4");
        let mut past = encode_signal(45, KIND_OFFER, "x").remove(0);
        past[7] = 5; // part 5 of 1
        assert_eq!(decode_signal(&mut body(&past)), None, "a part past the count");
        assert_eq!(split(&"x".repeat(CHUNK * 70)).len(), MAX_PARTS as usize, "at most MAX_PARTS parts");
    }

    /// Messages begun and never finished are let go past MAX_PENDING, the
    /// oldest first.
    #[test]
    fn unfinished_messages_are_let_go() {
        let mut re = Reassembly::default();
        for seq in 0..(MAX_PENDING as u32 + 5) {
            assert_eq!(re.take(seq, KIND_OFFER, 0, 2, "a".into()), None);
        }
        assert_eq!(re.len(), MAX_PENDING);
        assert_eq!(re.take(0, KIND_OFFER, 1, 2, "b".into()), None, "the oldest went: its last part starts it over");
        assert_eq!(re.take(MAX_PENDING as u32 + 4, KIND_OFFER, 1, 2, "b".into()), Some((KIND_OFFER, "ab".into())));
    }
}
