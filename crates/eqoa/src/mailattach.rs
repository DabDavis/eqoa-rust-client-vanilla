//! What a tell-gram carries: tunar and an item, on a server that offers it
//! (go-eqoa-server's mail attachments, [`OP_MAIL_ATTACH`]; no retail
//! message, no CLIENT code). The compose window gains an Attach row (a pick
//! of the pack's records, and how many of a stack) and a Tunar row; the
//! reader shows what a gram carries under its text and offers Take; the
//! inbox marks a gram that carries something with "+" before its subject.
//! Every rule is the server's: what may be mailed (no No Trade, No Rent or
//! worn item; a Lore item only to one who has none), the postage, a pack
//! with no room. A server that never offers it (the reference server, an older
//! go-eqoa-server) is mailed as before, and nothing here shows.
//!
//! The windows are `gram`'s (`Compose`, `Read`, `Inbox`); this is the
//! state and the wire they share, kept on `Grams::attach`.

use net::dump::{NetItem, read_net_item};
use net::stream::{Reader, Writer};

/// go-eqoa-server's own: what a gram carries, both ways, a sub-op byte
/// first. From the server: 0 the offer (u8 version, u8 the records a gram
/// may carry, u32 the postage), 2 what a gram carries (u32 its id, u32
/// tunar, a zigzag count of records as 0x003B writes one, u8 1 when it came
/// back to its sender), 3 the inbox's marks (a zigzag count, each u32 id,
/// u32 tunar, u8 records), 4 a take's answer (u32 id, u8 1 taken), 5 why a
/// send was refused (a wide line; its 0x0D41 status 12 follows). From the
/// client: 0 the answer (u8 1), 1 a live send that carries something (the
/// live 0x0D01 sub-op 1's fields, u32 tunar, a zigzag count, each u32 a
/// pack key and a zigzag count), 4 take (u32 id). A PS2 never answers, and
/// is told in a gram's text what it carries.
pub const OP_MAIL_ATTACH: u16 = 0x7EA1;

/// The server's offer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offer {
    /// the records one gram may carry
    pub items: u8,
    /// the tunar a gram that carries something costs, on top
    pub postage: u32,
}

/// What a gram carries, as the server last said.
#[derive(Clone, Debug, Default)]
pub struct Carried {
    pub id: u32,
    pub tunar: u32,
    pub items: Vec<NetItem>,
    /// it came back to its sender: it cannot be deleted until taken
    pub returned: bool,
}

impl Carried {
    pub fn is_empty(&self) -> bool {
        self.tunar == 0 && self.items.is_empty()
    }

    /// "500 tunar and 3 Bone Arrow", as the server says it.
    pub fn words(&self) -> String {
        let mut parts = Vec::new();
        if self.tunar > 0 {
            parts.push(format!("{} tunar", self.tunar));
        }
        for it in &self.items {
            if it.quantity > 1 {
                parts.push(format!("{} {}", it.quantity, it.item.name));
            } else {
                parts.push(it.item.name.clone());
            }
        }
        parts.join(" and ")
    }
}

/// A pack record a gram may carry, as the compose's pick lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct PackItem {
    pub key: u32,
    pub name: String,
    pub quantity: i64,
}

/// The attachments' state, `Grams::attach`.
#[derive(Debug, Default)]
pub struct Attach {
    /// the server's offer; `None`: this server's mail carries nothing
    pub offer: Option<Offer>,
    /// the inbox's grams that carry something: (id, tunar, records)
    pub marks: Vec<(u32, u32, u8)>,
    /// what the gram last opened carries
    pub carried: Option<Carried>,
    /// why the last send was refused (shown in its box)
    pub refusal: Option<String>,
    /// the pack's records a gram may carry, set by the play while a compose
    /// window is up
    pub pack: Vec<PackItem>,
}

/// Whether a record may go on a gram, as far as the client can tell (the
/// server judges): in the pack, not worn, not No Trade, not No Rent.
pub fn mailable(it: &NetItem) -> bool {
    it.equip_slot < 0 && it.item.no_trade == 0 && it.item.rent != 0 && it.quantity > 0
}

impl Attach {
    /// A message from the server. True for the offer, which the play
    /// answers ([`answer`]).
    pub fn recv(&mut self, data: &[u8]) -> bool {
        let mut r = Reader::new(data);
        let sub = r.u8();
        if r.error {
            return false;
        }
        match sub {
            0 => {
                let _version = r.u8();
                let items = r.u8();
                let postage = r.u32();
                if r.error {
                    return false;
                }
                self.offer = Some(Offer { items, postage });
                true
            }
            2 => {
                let id = r.u32();
                let tunar = r.u32();
                let n = r.signed();
                let mut items = Vec::new();
                for _ in 0..n.clamp(0, 16) {
                    let it = read_net_item(&mut r);
                    if r.error {
                        break;
                    }
                    items.push(it);
                }
                let returned = r.u8() != 0;
                if !r.error {
                    self.carried = Some(Carried { id, tunar, items, returned });
                    let left = self.carried.as_ref().map(|c| (c.tunar, c.items.len() as u8)).unwrap_or_default();
                    self.marks.retain(|m| m.0 != id);
                    if left != (0, 0) {
                        self.marks.push((id, left.0, left.1));
                    }
                }
                false
            }
            3 => {
                let n = r.signed();
                let mut marks = Vec::new();
                for _ in 0..n.clamp(0, 4096) {
                    let m = (r.u32(), r.u32(), r.u8());
                    if r.error {
                        break;
                    }
                    marks.push(m);
                }
                self.marks = marks;
                false
            }
            5 => {
                let line = r.wide(1024);
                if !r.error {
                    self.refusal = Some(line);
                }
                false
            }
            _ => false,
        }
    }

    /// Whether gram `id` carries something (the inbox's mark).
    pub fn marked(&self, id: u32) -> bool {
        self.marks.iter().any(|m| m.0 == id)
    }

    /// What gram `id` carries, when the server has said.
    pub fn carried_by(&self, id: u32) -> Option<&Carried> {
        self.carried.as_ref().filter(|c| c.id == id && !c.is_empty())
    }
}

/// The answer to the offer: this client shows and takes attachments.
pub fn answer() -> Vec<u8> {
    let mut w = Writer::op(OP_MAIL_ATTACH);
    w.u8(0).u8(1);
    w.data
}

/// A live send that carries `tunar` and `item` (a pack key and how many).
pub fn send(names: &[Vec<u8>], subject: &[u16], text: &[u16], tunar: u32, item: Option<(u32, i64)>) -> Vec<u8> {
    let mut w = Writer::op(OP_MAIL_ATTACH);
    w.u8(1).signed(names.len() as i64);
    for n in names {
        w.string(n);
    }
    w.wide(&String::from_utf16_lossy(subject)).wide(&String::from_utf16_lossy(text));
    w.u32(tunar);
    match item {
        Some((key, n)) => {
            w.signed(1).u32(key).signed(n);
        }
        None => {
            w.signed(0);
        }
    }
    w.data
}

/// Take what gram `id` carries.
pub fn take(id: u32) -> Vec<u8> {
    let mut w = Writer::op(OP_MAIL_ATTACH);
    w.u8(4).u32(id);
    w.data
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The offer is kept and answered; what a gram carries is read as the
    /// server writes it (a record as 0x003B's), and its mark follows it; a
    /// refusal's line is kept; the control: a message cut short changes
    /// nothing.
    #[test]
    fn the_server_messages() {
        let mut a = Attach::default();
        assert!(!a.recv(&[0, 1, 1]), "cut short");
        assert_eq!(a.offer, None);
        assert!(a.recv(&[0, 1, 1, 15, 0, 0, 0]));
        assert_eq!(a.offer, Some(Offer { items: 1, postage: 15 }));
        assert_eq!(answer(), [0xA1, 0x7E, 0, 1]);

        let mut w = Writer::default();
        w.u8(3).signed(1).u32(9).u32(75).u8(1);
        a.recv(&w.data);
        assert!(a.marked(9) && !a.marked(8));

        let mut w = Writer::default();
        w.u8(2).u32(9).u32(75).signed(1);
        // a record: six instance fields, 23 zigzag words, the name, the
        // text, no stats
        w.signed(2).signed(100).signed(0).signed(-1).signed(-1).u32(0);
        for i in 0..23 {
            w.signed(if i == 14 { 20 } else { 0 });
        }
        w.wide("Bone Arrow").wide("").signed(0);
        w.u8(0);
        a.recv(&w.data);
        let c = a.carried_by(9).expect("carried");
        assert_eq!(c.words(), "75 tunar and 2 Bone Arrow");

        // taken: the server says it carries nothing; the mark goes
        let mut w = Writer::default();
        w.u8(2).u32(9).u32(0).signed(0).u8(0);
        a.recv(&w.data);
        assert!(a.carried_by(9).is_none() && !a.marked(9));

        let mut w = Writer::default();
        w.u8(5).wide("Bound Ring is No Trade and cannot be mailed.");
        a.recv(&w.data);
        assert_eq!(a.refusal.as_deref(), Some("Bound Ring is No Trade and cannot be mailed."));
    }

    /// A send carries the live send's fields, then the tunar and the record;
    /// a take names the gram.
    #[test]
    fn the_client_messages() {
        let names = vec![b"Bob".to_vec()];
        let d = send(&names, &[b'H' as u16], &[b'x' as u16], 500, Some((3, 2)));
        let mut want = vec![0xA1, 0x7E, 1, 2, 3, 0, 0, 0, b'B', b'o', b'b', 1, 0, 0, 0, b'H', 0, 1, 0, 0, 0, b'x', 0];
        want.extend(500u32.to_le_bytes());
        want.extend([2, 3, 0, 0, 0, 4]);
        assert_eq!(d, want);
        assert_eq!(take(7), [0xA1, 0x7E, 4, 7, 0, 0, 0]);
    }
}
