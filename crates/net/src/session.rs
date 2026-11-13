//! `VIClient`'s game connection (decomp `modules/client/src/viclient.cpp`):
//! DRDP over UDP, one opcode per message (`[u16 opcode][payload]`, sent as
//! type 251), routed on the DRDP message type the way `PumpMessages` does.
//!
//! The flow: `Connect` opens the login endpoint (drdp port 0xFFFE) and sends
//! the version hello and 0x0904; the server list 0x07B3 arrives on type 252;
//! `ConnectShard` opens the chosen game endpoint and sends the hello and
//! 0x0001; the character list 0x002C; `SelectCharacter` sends 0x002A; the
//! server then opens a NEW connection (a type-255 notice), which the client
//! adopts after resetting the world, and the MemoryDump 0x000D arrives on it
//! (its DRDP source is the player's own entity id), answered with 0x0014,
//! 0x00C9 and 0x00CA.

use crate::addr::Nat64;
use crate::stream::{Reader, Writer};
use drdp::{Addr, ConnId, Drdp, PORT_LEADER, address_to_endpoint};
use std::net::{SocketAddr, UdpSocket};
use vi::fpu::Ee;
use web_time::{Instant, SystemTime, UNIX_EPOCH};

/// What the session sends its datagrams through: a UDP socket natively, a
/// relay's WebSocket in a browser. Addresses are IPv4 and a port.
pub trait Datagrams {
    fn send_to(&mut self, data: &[u8], ip: [u8; 4], port: u16);
    /// The next datagram waiting, if any, and its sender.
    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)>;
}

/// A non-blocking UDP socket: IPv4, or on a network with IPv6 alone an IPv6
/// one through the network's gateway to IPv4 (`addr::Nat64`).
pub struct Udp {
    socket: UdpSocket,
    buf: Vec<u8>,
    nat64: Option<Nat64>,
}

impl Datagrams for Udp {
    fn send_to(&mut self, data: &[u8], ip: [u8; 4], port: u16) {
        let to = match &self.nat64 {
            Some(n) => n.to_v6(ip, port),
            None => SocketAddr::from((ip, port)),
        };
        let _ = self.socket.send_to(data, to);
    }

    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
        loop {
            let (n, p) = self.socket.recv_from(&mut self.buf).ok()?;
            let ip = match &self.nat64 {
                // one not from under the gateway's prefix is no server's
                Some(g) => match g.to_v4(&p) {
                    Some(ip) => ip,
                    None => continue,
                },
                None => ipv4(&p),
            };
            return Some((self.buf[..n].to_vec(), ip, p.port()));
        }
    }
}

/// `VIClient`'s protocol version (0x0000 carries it both ways).
pub const VERSION: u32 = 18;
/// The expansion flags a retail Frontiers disc sends.
pub const EXPANSION_FLAGS: u32 = 3;
/// The group's chat lines' colour (0x069E: CLIENT's dispatcher adds them in
/// (.5, .5, 1)).
pub const GROUP_CHAT: [f32; 4] = [0.5, 0.5, 1.0, 1.0];
/// go-eqoa-server's own, no retail message: what a client keeps in view, how
/// many entity slots (`entity`) and how far, two u32s after the opcode, both
/// ways. The server offers the most it fills and the farthest it looks once
/// the player is in the world; the client answers with its NPC/PC Display and
/// NPC/PC View Distance, and again whenever they change. CLIENT has no case
/// for it (its dispatcher has no default), so a PS2 ignores the offer, and
/// nothing is sent to a server that made none.
pub const OP_ENTITY_VIEW: u16 = 0x7E00;
/// go-eqoa-server's own, no retail message either: the GM's trigger names, a
/// u32 after the opcode, both ways. The server offers them (1) to a GM's
/// client beside the GM channel's 0x0700 35; this client answers
/// [`TRIGGERS_ALL`] at once and is then sent every trigger (0x7E36,
/// `admin::read_trigger_list`) whenever they change, or, from a server that
/// takes any answer but 0 as 1 (one from before 2026-11-09), the TEST menu's
/// eight lines' names (0x7E08, `admin::read_triggers`). A PS2 ignores the
/// offer, as it ignores [`OP_ENTITY_VIEW`]'s.
pub const OP_GM_TRIGGERS: u16 = 0x7E07;
/// The answer to [`OP_GM_TRIGGERS`] that asks for every trigger, by slot (1:
/// the eight lines' names alone).
pub const TRIGGERS_ALL: u32 = 2;
/// go-eqoa-server's own, no retail message either: the live game's duel
/// messages (0x1501 up, 0x1502 down, the client's `duel` part and its
/// window), a u32 after the opcode, both ways. The server offers them (1)
/// once the player is in the world; this client has the window and answers 1
/// at once, and is then sent 0x1502 where a client without it gets boxes. A
/// PS2 ignores the offer, as it ignores [`OP_ENTITY_VIEW`]'s.
pub const OP_DUEL_WINDOW: u16 = 0x7E09;
/// go-eqoa-server's own, no retail message either: a race the server has and
/// the game does not, by its own words (`racelabel`), a u32 after the opcode,
/// both ways. On the wire such a race passes as one of the game's. The server
/// offers its words (1) once the player is in the world; this client answers 1 at
/// once and is sent who reads differently ([`OP_RACE_LABELS_DATA`]) then and
/// whenever that changes. A PS2 ignores the offer, as it ignores
/// [`OP_ENTITY_VIEW`]'s; a server that never makes one gets no answer.
pub const OP_RACE_LABELS: u16 = 0x7E1C;
/// The list, to a client that answered [`OP_RACE_LABELS`]: `racelabel::read`.
pub const OP_RACE_LABELS_DATA: u16 = 0x7E1D;
/// go-eqoa-server's own, no retail message either: what character creation
/// takes past the game's own races and classes, a u32 after the opcode, both
/// ways. The server offers it (1) after the session's first character list, and
/// only where it takes something past the game's; this client answers 1 at once
/// and is sent the races ([`OP_CREATION_RACES`], `creation::read`), which the
/// create pages offer. A PS2 ignores the offer; a server that never makes one
/// gets no answer and its create pages are the game's.
pub const OP_CREATION_OFFER: u16 = 0x7E1E;
/// The races, to a client that answered [`OP_CREATION_OFFER`]: `creation::read`,
/// kept as [`Session::creation`].
pub const OP_CREATION_RACES: u16 = 0x7E1F;
/// go-eqoa-server's own, no retail message either: a custom race's own character
/// options and the looks its characters chose (`raceopts`), a u32 after the
/// opcode, both ways. The server offers them (1) after the session's first
/// character list; this client answers 1 at once and is sent each race's options
/// ([`OP_RACE_OPTIONS_DATA`], kept as [`Session::race_options`]) and the looks
/// ([`OP_RACE_LOOKS`], [`Session::race_looks`]), and the looks again
/// whenever they change. A PS2 ignores the offer; a server that never makes one
/// gets no answer, and its creations carry no choice ([`NewChar::options`]).
pub const OP_RACE_OPTIONS: u16 = 0x7E22;
/// The answer to [`OP_RACE_OPTIONS`]: 2, a client that shows a game race's
/// rows (go-eqoa-server's skin shades) under the four, and re-chooses them at
/// select's Customize (sent after 0x002A's ints); one that answered 1 (before
/// 2026-11-11) showed a race's rows instead of the four, so the server sends it
/// none of a game race's. 3 (since 2026-11-12): it draws a barbarian's tattoo
/// too (a colour of `render::recolor::TATTOO`), so the server sends it that
/// row; to one that answered 2 it sends the rows it did before.
pub const RACE_OPTIONS_ANSWER: u32 = 3;
/// Each race's options, to a client that answered [`OP_RACE_OPTIONS`]:
/// `raceopts::read_options`.
pub const OP_RACE_OPTIONS_DATA: u16 = 0x7E23;
/// The looks the characters chose, to a client that answered
/// [`OP_RACE_OPTIONS`]: `raceopts::read_looks`, each one the whole list, kept
/// as [`Session::race_looks`] for the game to take.
pub const OP_RACE_LOOKS: u16 = 0x7E24;
/// go-eqoa-server's own, no retail message either: SOE's live game's look, a
/// u32 after the opcode, both ways. The server offers it (1) after the
/// session's first character list; this client answers 1 at once: it reads
/// the live CLIENT's entity channel ([`crate::entity::LIVE_STATE_SIZE`]
/// bytes, a colour per held item, armour slot and robe) and its 0x002C
/// record ([`LIVE_GEAR_SIZE`] raw bytes). A PS2 ignores the offer; Sandstorm
/// never makes one, so its channels and lists stay the disc's.
pub const OP_LIVE_LOOK: u16 = 0x7E0C;
/// The server's answer to [`OP_LIVE_LOOK`], a u32: the look in force from it
/// on (1 the live game's, 0 the disc's). Every character list after it is in
/// that look (lists are guaranteed messages, in order); a client still
/// choosing is sent its list again.
pub const OP_LIVE_LOOK_IN_FORCE: u16 = 0x7E0D;
/// Bit 1 of this client's answer to [`OP_LIVE_LOOK`]: its character select
/// lists every character the server keeps and scrolls them; the look in
/// force then carries a second u32, the characters the account may hold
/// ([`Session::char_slots`]), and the list comes again, whole. A server that
/// knows no such bit reads bit 0 alone (go-eqoa-server before 2026-11-11).
pub const LIVE_LOOK_MANY_CHARS: u32 = 2;
/// Bit 2 of this client's answer to [`OP_LIVE_LOOK`]: it draws
/// go-eqoa-server's armour, set 9 (the ornate plate CHAR.ESF holds, which
/// Sandstorm's S2 adds) and the trim colours after the live game's channel
/// ([`crate::entity::ARMOR_STATE_SIZE`]) and record ([`ARMOR_GEAR_SIZE`]). The
/// look in force's third u32 says it is granted ([`Session::armor_look`]); a
/// server that knows no such bit sends none, and set 9 as set 4.
pub const LIVE_LOOK_ARMOR: u32 = 4;
/// CLIENT's character select: eight (`CharacterList`, `m_chars[8]`).
pub const CLIENT_CHAR_SLOTS: u32 = 8;
/// The most characters a list is read of, whatever a server says.
pub const CHAR_SLOTS_MOST: u32 = 64;
/// go-eqoa-server's own, no retail message either: bug reports, a u32 after
/// the opcode, both ways. The server offers them (1) once the player is in
/// the world; this client answers 1 at once, and then F3 opens Support's
/// request form on Bug Report, and each petition it files (the form, or a
/// typed `/bug`/`#bug`) goes after its own note ([`OP_BUG_NOTE`]). A second
/// u32 after the offer's is the most characters of a petition's text the
/// server takes (go-eqoa-server's since 2026-11-08: 2,047): the form holds
/// that many, up to [`PETITION_TEXT_MOST`] ([`Session::petition_text`]),
/// else a PS2's [`PETITION_TEXT_DISC`]. A PS2 ignores the offer; a server
/// that never makes one (Sandstorm's, eqoa.live's) gets no key and no note.
pub const OP_BUG_REPORTS: u16 = 0x7E0E;
/// This client's note for the petition it files next, to a server that
/// offered [`OP_BUG_REPORTS`]: a u32 version (1), then a u32 byte count and
/// that many bytes of UTF-8 text, "key: value" lines (`bugnote`), at most
/// [`BUG_NOTE_MAX`]. The server keeps it for the GMs beside that petition.
pub const OP_BUG_NOTE: u16 = 0x7E0F;
/// go-eqoa-server's own, no retail message either: crash reports
/// (`eqoa::crashfile`). The server offers to take them once the player is in
/// the world: a u32 version (1), then a u32, the most bytes of a report's text
/// it takes ([`Session::crash_offer`]); this client then sends each it kept
/// ([`OP_CRASH_REPORT`]). A PS2 ignores the offer; a server that never makes
/// one (Sandstorm's, eqoa.live's) is sent none.
pub const OP_CRASH_OFFER: u16 = 0x7E64;
/// A crash report, client to server: a u32 version (1), a u32 number of this
/// client's for it, a u32 byte count and that many bytes of UTF-8 text, at
/// most the offer's most (a report past 1,300 bytes goes in DRDP's pieces,
/// which a server that offers reports puts back together).
pub const OP_CRASH_REPORT: u16 = 0x7E65;
/// The server's answer to [`OP_CRASH_REPORT`]: the report's u32 number and a
/// u32 ticket, the petition it was filed as (0: not kept, and not to be sent
/// again) ([`Session::crash_filed`]).
pub const OP_CRASH_FILED: u16 = 0x7E66;
/// go-eqoa-server's own, no retail message either: more of each character at
/// select (`selectinfo`), a u32 after the opcode, both ways. The server offers it
/// (1) after the session's first character list; this client answers 1 at once and
/// is sent [`OP_SELECT_INFO_DATA`] after every list and at its answer. A PS2 ignores
/// the offer; a server that never makes one gets no answer, and select shows the
/// game's lines.
pub const OP_SELECT_INFO: u16 = 0x7E67;
/// Each listed character's own race, its race's name, its mastery points and the
/// rows its Customize re-chooses (`selectinfo::read`), by server id, the whole
/// list each time ([`Session::select_info`]).
pub const OP_SELECT_INFO_DATA: u16 = 0x7E68;
/// The most bytes of a crash report this client sends, whatever an offer says.
pub const CRASH_REPORT_MOST: usize = 12_000;
/// go-eqoa-server's own, no retail message either: any member of a group may
/// invite into it there, a u32 after the opcode (1), from the server once the
/// player is in the world; no answer. CLIENT lets only the leader invite
/// (`InviteEntity`, `InviteToGroup`: word 640), and so does this client on a
/// server that never offers it (Sandstorm's, eqoa.live's).
pub const OP_MEMBER_INVITES: u16 = 0x7E17;
/// go-eqoa-server's own: the mercenaries told apart (the GM Jibrish's
/// petitions 1475 and 1488, 2026-11-11). Both ways: the server's offer once the
/// player is in the world (a u8, the layout of [`OP_MERCS_DATA`], 1), and this
/// client's answer at once (a u8, 1). CLIENT has no such case: a PS2 ignores it.
pub const OP_MERCS_OFFER: u16 = 0x7E5A;
/// The mercs, from a server the client answered, at once and whenever they
/// change: a u8 layout (1), a u8 of flags (bit 0: the player's own merc is
/// out), a u16 count and each merc in the player's world by its name (a u8 byte
/// count and the bytes). Each is the whole set ([`Mercs`]). This client answers
/// the offer 3 (the owner, 2026-11-11 and 2026-11-12): a server that knows it
/// sends layout 3, each name followed by its hirer's (for the "<Hirer's
/// Mercenary>" tag under a merc's name plate), then a u8 its class and a u8 a
/// healer's mode (for its □ details box); one that knows 2 sends 2, and one
/// that knows only 1 takes any answer but 0 as 1.
pub const OP_MERCS_DATA: u16 = 0x7E5B;
/// The start menu's Group > Dismiss Mercenary, client to server: a u8, 1 (the
/// server's `/merc dismiss`).
pub const OP_MERC_DISMISS: u16 = 0x7E5C;
/// go-eqoa-server's own: the live group search's standing (the GM Jibrish's
/// petition 1476, 2026-11-11). Both ways, as [`OP_MERCS_OFFER`]: the offer (a u8
/// layout, 1) and this client's answer (a u8, 1).
pub const OP_GROUP_REG_OFFER: u16 = 0x7E5D;
/// The standing, from a server the client answered, at once and whenever it
/// changes: a u8 layout (1) and a u8 of flags ([`GroupReg`]).
pub const OP_GROUP_REG_STATE: u16 = 0x7E5E;
/// "Allow anyone in my group to invite", client to server: a u8, 1 yes, 0 no.
pub const OP_GROUP_INVITES_SETTING: u16 = 0x7E5F;

/// The mercenaries a server told ([`OP_MERCS_DATA`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mercs {
    /// the player's own is out (Group > Dismiss Mercenary)
    pub own: bool,
    /// every merc in the player's world, by the name its channel carries
    pub names: Vec<String>,
    /// each one's hirer's name, as `names` (layout 2); none from layout 1
    pub owners: Vec<String>,
    /// each one's class (0-14) and a healer's mode (0 none, 1 defensive, 2
    /// offensive), as `names` (layout 3, the owner's of 2026-11-12: "for
    /// mercs show class also in the square pop up"); none from an earlier one
    pub classes: Vec<(u8, u8)>,
}

impl Mercs {
    /// [`OP_MERCS_DATA`]'s body past the opcode; none for another layout or a
    /// short one.
    pub fn read(b: &[u8]) -> Option<Mercs> {
        let mut r = Reader::new(b);
        let (layout, flags, n) = (r.u8(), r.u8(), r.u16() as usize);
        if r.error || !(1..=3).contains(&layout) {
            return None;
        }
        let (mut names, mut owners, mut classes) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..n {
            let len = r.u8() as usize;
            names.push(String::from_utf8_lossy(r.bytes(len)).into_owned());
            if layout >= 2 {
                let len = r.u8() as usize;
                owners.push(String::from_utf8_lossy(r.bytes(len)).into_owned());
            }
            if layout >= 3 {
                classes.push((r.u8(), r.u8()));
            }
        }
        (!r.error).then_some(Mercs { own: flags & 1 != 0, names, owners, classes })
    }

    /// Whether a creature of that name is a merc.
    pub fn is(&self, name: &str) -> bool {
        !name.is_empty() && self.names.iter().any(|n| n == name)
    }

    /// The class and a healer's mode (0 none, 1 defensive, 2 offensive) of
    /// the merc of that name, when the server said (layout 3).
    pub fn class(&self, name: &str) -> Option<(u8, u8)> {
        let k = self.names.iter().position(|n| !name.is_empty() && n == name)?;
        self.classes.get(k).copied()
    }

    /// The hirer of the merc of that name, when the server said (layout 2).
    pub fn owner(&self, name: &str) -> Option<&str> {
        let k = self.names.iter().position(|n| !name.is_empty() && n == name)?;
        self.owners.get(k).map(|o| o.as_str()).filter(|o| !o.is_empty())
    }
}

/// The live group search's standing a server told ([`OP_GROUP_REG_STATE`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroupReg {
    /// registered as looking (0x0801 sub 2)
    pub registered: bool,
    /// its own "Allow anyone in my group to invite"
    pub invites_open: bool,
    /// its group's members invite now (in none, a raid group, or a regular
    /// group whose leader has the setting on)
    pub members_invite: bool,
}

impl GroupReg {
    /// [`OP_GROUP_REG_STATE`]'s body past the opcode.
    pub fn read(b: &[u8]) -> Option<GroupReg> {
        match b {
            [1, f, ..] => Some(GroupReg { registered: f & 1 != 0, invites_open: f & 2 != 0, members_invite: f & 4 != 0 }),
            _ => None,
        }
    }
}
/// go-eqoa-server's own, no retail message either: a GM's flight (`#fly`), a
/// u32 after the opcode, both ways. This client says it flies (1) when it is
/// offered the GM's trigger names ([`OP_GM_TRIGGERS`], a GM's client's alone), so
/// the server need send a client that cannot (a PS2) nothing at all, not even an
/// offer; the server then says 1 flight is on and 0 it is off. Nothing is said
/// to a server that made no such offer (Sandstorm's, eqoa.live's), and one that
/// never sends it leaves the walk as it was.
pub const OP_FLY: u16 = 0x7E1B;
/// go-eqoa-server's own, no retail message either: the character's combat
/// record, its PvP and its duels (`record`), a u32 after the opcode, both ways.
/// The server offers it (1) once the player is in the world; this client answers
/// 1 at once and shows Character > Combat Record, which asks for the record
/// ([`OP_COMBAT_RECORD_DATA`], a u32 1) as its window opens; the server sends it
/// then and whenever a count changes it. A PS2 ignores the offer; a server that
/// never makes one (Sandstorm's, eqoa.live's) gets no answer and no entry shows.
pub const OP_COMBAT_RECORD: u16 = 0x7E20;
/// The record: this client's ask (a u32 1), the server's rows (`record::read`).
pub const OP_COMBAT_RECORD_DATA: u16 = 0x7E21;
/// go-eqoa-server's own, no retail message either: worlds the server has past
/// the disc's, each made of the disc's own zones (`serverworlds`), a u32 after
/// the opcode, both ways. The server offers them (1) after the session's first
/// character list; this client answers 1 when it can build them
/// ([`set_world_builder`]) and is sent the list ([`OP_SERVER_WORLDS_DATA`]). A
/// PS2 ignores the offer; a server that never makes one has the disc's worlds
/// alone, as before.
pub const OP_SERVER_WORLDS: u16 = 0x7E30;
/// The list, to a client that answered [`OP_SERVER_WORLDS`]
/// (`serverworlds::read`); this client's answer once it has built them: the
/// worlds it holds (`serverworlds::encode_ready`), which are all the server
/// sends it into.
pub const OP_SERVER_WORLDS_DATA: u16 = 0x7E31;

/// Which of the session's characters a login needs the server's worlds for,
/// from a server that says (go-eqoa-server's since 2026-11-10, petition 1468),
/// to a client that answered [`OP_SERVER_WORLDS`] 1, before the list and
/// after every later character list (`serverworlds::read_chars`, kept as
/// [`Session::pick_needs_worlds`]): a character played while the worlds are
/// still being built waits for them only when it is one of these, natively
/// as in a page. A server that never sends it (an older one) leaves a page
/// holding every character, as before, and a native client none.
pub const OP_SERVER_WORLDS_CHARS: u16 = 0x7E54;

/// What builds a server's worlds off the player's disc (the game's
/// `serverworlds`, set once at its start): handed the list, it answers on the
/// channel, once, with the ids of those it holds. With none set this client
/// does not take the offer (a page, a test).
pub type WorldBuilder = fn(&[crate::serverworlds::World]) -> std::sync::mpsc::Receiver<Vec<u8>>;

static WORLD_BUILDER: std::sync::OnceLock<WorldBuilder> = std::sync::OnceLock::new();

/// Sets what builds a server's worlds ([`WorldBuilder`]).
pub fn set_world_builder(f: WorldBuilder) {
    let _ = WORLD_BUILDER.set(f);
}

/// go-eqoa-server's own, no retail message either: capes (`cape`), a u32 after
/// the opcode, both ways. The server offers them (1) after the session's first
/// character list, and only where it turns them on (its `world.capes`); this
/// client answers 1 at once, and from then on it is sent a cape item as one of
/// wear slot [`crate::cape::WEAR_SLOT`] worn in equip slot
/// [`crate::cape::EQUIP_SLOT`], the capes of its list's characters
/// ([`OP_CAPES_LIST`]) and of the players it sees ([`OP_CAPES_IN_VIEW`]). A PS2
/// ignores the offer and is sent a cape as a plain item; a server that never
/// makes one gets no answer, and nothing of capes is drawn or worn.
pub const OP_CAPES: u16 = 0x7E25;
/// The capes of the players this client sees, by the id it knows each by (its
/// own character's too): `cape::read`, the whole set each time.
pub const OP_CAPES_IN_VIEW: u16 = 0x7E26;
/// go-eqoa-server's own, no retail message either: a pack and a bank past
/// CLIENT's 40 records (its `world.pack_slots`, `world.bank_slots`), both ways.
/// The server offers them after the session's first character list: a u32 1,
/// then u32 the pack's records and u32 the bank's; this client answers at once
/// with a u32 1 and the records it holds of each (the offer's, at most
/// [`ITEM_LISTS_MOST`]), and is told the sizes in force before every
/// MemoryDump ([`OP_ITEM_LISTS_IN_FORCE`]). A PS2 ignores the offer and holds
/// 40; a server that never makes one gets no answer, and this client holds
/// CLIENT's 40 of each too.
pub const OP_ITEM_LISTS: u16 = 0x7E29;
/// The sizes in force, to a client that answered [`OP_ITEM_LISTS`]: u32 the
/// pack's records, u32 the bank's, before every MemoryDump, whose lists hold
/// that many at most; kept as [`Session::item_lists`] until the next.
pub const OP_ITEM_LISTS_IN_FORCE: u16 = 0x7E2A;
/// The records a list holds when no server said otherwise: CLIENT's
/// (`VIItemList`'s `ItemInternal[40]`, every `AddItem`'s 40).
pub const ITEM_LIST_DISC: u32 = 40;
/// The most this client holds in a list: the server names a record by its
/// index in a byte (0x003C, 0x003E, 0x003F, 0x1250).
pub const ITEM_LISTS_MOST: u32 = 255;
/// The capes of the characters in its list, by each one's server id:
/// `cape::read`, the whole list after each character list.
pub const OP_CAPES_LIST: u16 = 0x7E27;
/// go-eqoa-server's own, no retail message either: a jump (`walker`, the client's
/// own: the game has none), a u32 after the opcode, both ways. The server offers it
/// (1, `world.jump`) to a client of its own only, one that answered its bug report
/// offer ([`OP_BUG_REPORTS`]), so a PS2 is never sent it; this client answers 1 at
/// once and jumps from then on, and the server's movement check takes a jump's arc
/// from it. A 0 from the server (after a restart into one without jumps) ends it.
/// A server that never offers it (Sandstorm's, eqoa.live's) gets no answer, and the
/// walk never leaves the ground there, as the game's never does.
pub const OP_JUMP: u16 = 0x7E28;
/// go-eqoa-server's own, no retail message either: a GM's travel by the map. The
/// server offers it with the GM's channel at world entry, to a GM's client alone,
/// a u32 after the opcode (1, [`Session::map_travel`]); no answer. A PS2 ignores
/// the offer; a server that never makes one (Sandstorm's, eqoa.live's) is never
/// asked ([`OP_MAP_TRAVEL_TO`]).
pub const OP_MAP_TRAVEL: u16 = 0x7E34;
/// A GM's ask to travel to a spot of its map (`Session::map_travel_to`): u32 the
/// world (the MemoryDump's world index), f32 x, f32 z. The server moves a GM's
/// character there, on the ground's floor, and refuses anyone else's.
pub const OP_MAP_TRAVEL_TO: u16 = 0x7E35;
/// go-eqoa-server's own, no retail message either: the whole of a raid group,
/// where a roster holds four (`m_group`). The offer, from the server once the
/// player is in the world, a u32 after the opcode (1; 2 from a server that
/// also sends each member's group in the raid, petition 1687; 3 from one that
/// also sends each member's kind, a merc's or a player's, as it puts the
/// raid's mercs in the roster: the owner's "im not able to target mercs in a
/// raid group", 2026-11-12); a server without it never sends the rest.
pub const OP_RAID_ROSTER_OFFER: u16 = 0x7E18;
/// A client's answer to [`OP_RAID_ROSTER_OFFER`]: a u32, 1 to be sent the
/// roster, 2 with each member's group (to an offer of 2), 3 with its group and
/// its kind (to an offer of 3 or more), 0 to stop.
pub const OP_RAID_ROSTER_ANSWER: u16 = 0x7E19;
/// The roster, to a client that asked, on a change and at most once a second:
/// u16 members in all, u16 this message's first, u8 how many it holds, then
/// each [`RaidMember`] (u32 id, u8 and the bytes of its name, u8 class, u8
/// level, u8 HP of 255, u8 power of 255, when this client answered 2 or 3 a
/// u8 its group in the raid from 1 (a merc's, its owner's), and when it
/// answered 3 a u8 its kind, bit 0 a mercenary). 0 members: no raid group
/// (now).
pub const OP_RAID_ROSTER: u16 = 0x7E1A;
/// The most of a note sent, in bytes: one message, as a go-eqoa-server of
/// before 2026-11-08 takes each fragment of a longer one for a message of its
/// own (and says nothing of a petition's length in its offer).
pub const BUG_NOTE_MAX: usize = 1000;
/// The most characters of a petition's text (Support's request or reply,
/// 0x0AA1) for a server that says nothing of it: a PS2's form
/// (`VIWndCSRequest`'s `m_text[512]`, 511 and its end), which every server takes.
pub const PETITION_TEXT_DISC: usize = 511;
/// The most this client's form holds (2,048 with its end), for a server whose
/// bug report offer ([`OP_BUG_REPORTS`]) says it takes as many (petition 1300,
/// 2026-11-08: "Increase the text limit in the bug reporting window"). A request
/// that long is 4,103 bytes, past one message (1,300): DRDP sends it in four
/// pieces, which go-eqoa-server puts back together since it says so.
pub const PETITION_TEXT_MOST: usize = 2047;

/// One member of a raid group ([`OP_RAID_ROSTER`]): `id` as this client knows
/// it (its own, an entity's, or the roster's stand-in for one out of sight).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaidMember {
    pub id: u32,
    pub name: String,
    pub class: u8,
    pub level: u8,
    pub hp: u8,
    pub power: u8,
    /// Its group in the raid, from 1 (0: the server sends none). A merc's is
    /// its owner's.
    pub group: u8,
    /// It is a mercenary (a server of the offer of 3 says so): it takes no
    /// place in a raid's group of four and is never moved between them, but is
    /// targeted from its row as any member.
    pub merc: bool,
}

/// The raw part of a 0x002C record: the disc's, and the live game's
/// (`CharacterList`, Corsten's 0x012CC018: no robe tint, six ArmorGroup
/// variants where the disc's has tints, then the held items', armour slots'
/// and robe's colours).
pub const DISC_GEAR_SIZE: usize = 32;
pub const LIVE_GEAR_SIZE: usize = 71;
/// The live game's record with go-eqoa-server's six trim colours after it
/// ([`LIVE_LOOK_ARMOR`]).
pub const ARMOR_GEAR_SIZE: usize = LIVE_GEAR_SIZE + 24;
/// go-eqoa-server's own, no retail message: the quest marks the map draws (a
/// "!" at each giver of a quest the player may take, a "?" where a step of
/// its quests is to be done). A u32 after the opcode, both ways: offered once
/// the player is in the world, answered 1 to have them and 0 to stop
/// (`agree_quest_marks`, the Quest Helper's). A PS2 ignores the offer, as it
/// does [`OP_ENTITY_VIEW`]'s; a client with the Quest Helper off answers
/// nothing, and nothing is asked of a server that made none.
pub const OP_QUEST_MARKERS: u16 = 0x7E03;
/// go-eqoa-server's own, no retail message: others' power, for the Power Bars
/// the client draws under its group members' and its pet's health (the game
/// shows no one's but the player's own). A u32 after the opcode, both ways:
/// the server offers it (1) once the player is in the world, the client
/// answers 1 to have it and 0 to stop (`agree_power_bars`). A PS2 ignores the
/// offer, as it does [`OP_ENTITY_VIEW`]'s; nothing is asked of a server that
/// made none. (A raid's members' power comes in [`OP_RAID_ROSTER`]'s records.)
pub const OP_POWER_BARS: u16 = 0x7E01;
/// The power, from a server the client asked ([`OP_POWER_BARS`]): a u8
/// count, then each a u32 id (as this client knows the entity: a group
/// member, a player's or a merc, by its roster's id; the pet by its own) and a
/// u8 power of 255. Each is the whole set: an id it leaves out has no bar.
/// Sent only when the set changed, at most twice a second.
pub const OP_POWER_BARS_DATA: u16 = 0x7E02;
/// The marks, from a server the client asked ([`OP_QUEST_MARKERS`]): a u8
/// count, then each its u8 kind ([`QuestMark::kind`]), u8 world, f32 x and z,
/// the NPC's name and a u8 count of quests, each its name, u8 lowest and
/// highest levels, a line and a detail; every text a u8 byte count and UTF-8.
/// Each is the whole set.
pub const OP_QUEST_MARKERS_DATA: u16 = 0x7E04;
/// go-eqoa-server's own too: the quest tracker the HUD draws (each quest
/// under way and what its step asks, counted: "Bat Eye 0/3"), offered and
/// answered as [`OP_QUEST_MARKERS`] is (`agree_quest_progress`).
pub const OP_QUEST_PROGRESS: u16 = 0x7E05;
/// The tracker, from a server the client asked ([`OP_QUEST_PROGRESS`]): a u8
/// count of quests (the journal's, in its order), each its name and a u8
/// count of objectives, each its text, u16 had and u16 needed (needed 0:
/// nothing to count); every text a u8 byte count and UTF-8. Each is the
/// whole tracker.
pub const OP_QUEST_PROGRESS_DATA: u16 = 0x7E06;
/// go-eqoa-server's own: the effects on the player's pet, for the row under
/// the pet's bar (the user, 2026-11-09: "put the buff icons under the pet name
/// plate"; `Play::pet_effects`). A u32 after the opcode, both ways: offered
/// once the player is in the world, answered 1 to have them and 0 to stop, at
/// once and whenever that changes (`agree_pet_effects`, the Extended page's Pet
/// Effects). CLIENT shows the pet's effects nowhere (its effects window is
/// channel 0x43, the player's own), so a PS2 ignores the offer and is sent
/// none; nothing is asked of a server that made no offer.
pub const OP_PET_EFFECTS: u16 = 0x7E32;
/// The pet's effects, from a server the client asked ([`OP_PET_EFFECTS`]),
/// whenever they change: a u8 count, then each its u32 icon (a DictID, as
/// channel 0x43's), u32 milliseconds left as the server made the message, and
/// its name (a u8 byte count and UTF-8), in the order they went on. Each is the
/// whole set; none, no pet.
pub const OP_PET_EFFECTS_DATA: u16 = 0x7E33;
/// go-eqoa-server's own: what made each figure, for the Extended page's Number
/// Colors (`Play::damage_indication`; the user, 2026-11-09: "we can add the
/// number colors to extended menu"). The offer, server to client once the
/// player is in the world: a u8, the newest tag layout it tells (1; 2 from
/// 2026-11-11, the merc's bit: [`NumberTag::merc`]). CLIENT has no such case,
/// so a PS2 ignores it; nothing is asked of a server that made no offer.
pub const OP_NUMBER_TAGS_OFFER: u16 = 0x7E38;
/// The answer to [`OP_NUMBER_TAGS_OFFER`], client to server, at once and
/// whenever it changes (`agree_number_tags`): a u8, the layout read
/// ([`NUMBER_TAGS_LAYOUT`], at most the offer's) to have the figures tagged (a
/// Number Colors row not Default), 0 to stop. From then on each 0x00DB of a
/// figure the player, its pet or its merc made carries one byte after its body
/// ([`number_tag`]); every other is the retail body. A server that offered 1
/// reads any answer but 0 as 1 and never sets the merc's bit.
pub const OP_NUMBER_TAGS_ANSWER: u16 = 0x7E39;
/// The newest number tag layout this client reads: 2, the merc's bit
/// (go-eqoa-server, petition 1613: "Is there a way that the text color for
/// mercs damage be changed to be different than the chars?").
pub const NUMBER_TAGS_LAYOUT: u8 = 2;
/// go-eqoa-server's own: a damage meter, who in the player's group did how much
/// damage in the fight and how fast (`dpsmeter`, the Extended page's DPS Meter;
/// the server owner, 2026-11-10: "lets add dps meters, will work solo, group,
/// raid"). The offer, server to client once the player is in the world: a u8, 1,
/// [`OP_DPS_METER_DATA`]'s layout. CLIENT has no such case, so a PS2 ignores it;
/// nothing is asked of a server that made no offer.
pub const OP_DPS_METER_OFFER: u16 = 0x7E3A;
/// The answer to [`OP_DPS_METER_OFFER`], client to server, at once and whenever
/// it changes (`agree_dps_meter`): a u8, 1 to be sent the meter, 0 to stop.
pub const OP_DPS_METER_ANSWER: u16 = 0x7E3B;
/// The meter, from a server the client asked: the fight of the player's group
/// at most once a second while it changes, and once more as it ends (the server
/// ends it 10 s after the group's last damage). [`DpsMeter`] has the layout.
pub const OP_DPS_METER_DATA: u16 = 0x7E3C;
/// A reset, client to server (`reset_dps_meter`): a u8, 1. The server drops the
/// fight under way (nothing more is sent of it) and numbers its next another.
pub const OP_DPS_METER_RESET: u16 = 0x7E3D;

/// A fight on the meter ([`OP_DPS_METER_DATA`]): u8 layout (1), u32 its number
/// (another number, another fight), u8 1 when it is over (its last message), u32
/// its length in ms (from its first damage to its latest), u32 the damage of all
/// its lines, u16 how many lines it has, u16 the player's own line's place among
/// them (1 the most damage), u8 how many this message holds, then each
/// [`DpsLine`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpsMeter {
    pub fight: u32,
    pub over: bool,
    pub length_ms: u32,
    /// The damage of every line, those not sent too: what a line's share is of.
    pub total: u32,
    /// The fight's lines in all; at most the server's 24 are sent, the most
    /// damage first, the player's own always among them (in the last place
    /// when its own place is past them).
    pub count: u16,
    /// The player's own line's place among all of them, 1 the most damage.
    pub place: u16,
    pub lines: Vec<DpsLine>,
}

/// A line of the meter: u8 flags (1 the player's own, 2 no longer in its group),
/// u8 class, u32 damage (its pet's included), u32 its pet's share of that, and
/// its name (a u8 byte count and UTF-8).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DpsLine {
    pub own: bool,
    pub left: bool,
    pub class: u8,
    pub damage: u32,
    pub pet: u32,
    pub name: String,
}

/// [`OP_DPS_METER_DATA`]'s body, or None when it is short or of another layout.
fn read_dps_meter(r: &mut Reader) -> Option<DpsMeter> {
    if r.u8() != 1 {
        return None;
    }
    let (fight, over, length_ms, total, count, place) = (r.u32(), r.u8() == 1, r.u32(), r.u32(), r.u16(), r.u16());
    let n = r.u8() as usize;
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        let (flags, class, damage, pet) = (r.u8(), r.u8(), r.u32(), r.u32());
        let len = r.u8() as usize;
        let name = String::from_utf8_lossy(r.bytes(len)).into_owned();
        lines.push(DpsLine { own: flags & 1 != 0, left: flags & 2 != 0, class, damage, pet, name });
    }
    (!r.error).then_some(DpsMeter { fight, over, length_ms, total, count, place, lines })
}
/// go-eqoa-server's own, no retail message: a raid's windows (`raid`; the GM
/// Jibrish's petitions 1448 and 1451: the leader's mark, the loot rolls' pop-up,
/// the master looter's Assign and Abandon). The offer, server to client once the
/// player is in the world: a u8, 1. CLIENT has no such case, so a PS2 ignores it
/// and is told the same in chat lines; nothing is asked of a server that made no
/// offer.
pub const OP_RAID_OFFER: u16 = 0x7E42;
/// The answer to [`OP_RAID_OFFER`], client to server, at once and whenever it
/// changes (`agree_raid`): a u8, 1 to be sent the raid's state, its rolls and
/// the master looter's list, 0 for the chat lines a PS2 gets.
pub const OP_RAID_ANSWER: u16 = 0x7E43;
/// The raid as it is, to a client that answered 1, on a change and once all
/// zero when it is in no raid: `raid::read_state`.
pub const OP_RAID_STATE: u16 = 0x7E44;
/// A loot roll offered, or over for this client (0 seconds left):
/// `raid::read_roll`.
pub const OP_RAID_ROLL: u16 = 0x7E45;
/// A roll's answer, client to server: `raid::roll_choice`. The server answers
/// with the roll over ([`OP_RAID_ROLL`], 0 seconds).
pub const OP_RAID_ROLL_CHOICE: u16 = 0x7E46;
/// To the master looter, after the corpse it opened (0x0018): whom it may give
/// the corpse's items to, and for how long; 0 when that is over:
/// `raid::read_master_loot`.
pub const OP_RAID_MASTER_LOOT: u16 = 0x7E47;
/// The raid menu's commands, client to server: `raid::Command`.
pub const OP_RAID_COMMAND: u16 = 0x7E48;

/// What made a figure, the byte after a tagged 0x00DB's body
/// ([`OP_NUMBER_TAGS_ANSWER`]): its kind and whether the player's pet or merc
/// made it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberTag {
    /// The low 6 bits: 1 a swing, 2 a spell or an ability, 3 a damage over
    /// time's tick, 4 a damage shield, 5 a proc, 6 a heal, 7 a heal over
    /// time's tick (and any other a later server may send).
    pub kind: u8,
    /// Bit 7: the player's pet made it.
    pub pet: bool,
    /// Bit 6 (layout 2): the player's merc made it. A server of layout 1 never
    /// sets it (its kinds end at 7), and tells a merc's hit as the pet's.
    pub merc: bool,
}

/// A 0x00DB's tag: `rest` is what follows its body (u32 attacker, zigzag
/// amount, u32 defender), read only from a server that offered tags
/// (`offered`); one byte is the tag, a kind of 0 none, and anything else
/// (no byte, a server's patch's three colour bytes) none.
pub fn number_tag(rest: &[u8], offered: bool) -> Option<NumberTag> {
    let &[b] = rest else { return None };
    (offered && b & 0x3F != 0).then_some(NumberTag { kind: b & 0x3F, pet: b & 0x80 != 0, merc: b & 0x40 != 0 })
}

/// One effect on the pet ([`OP_PET_EFFECTS_DATA`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PetEffect {
    /// Its icon's DictID (UI.CSF's 32x32 surfaces).
    pub icon: u32,
    /// What was left of it when the server sent it, in ms.
    pub left_ms: u32,
    pub name: String,
}

/// [`OP_PET_EFFECTS_DATA`]'s body, or None when it is short.
fn read_pet_effects(r: &mut Reader) -> Option<Vec<PetEffect>> {
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let (icon, left_ms) = (r.u32(), r.u32());
        let len = r.u8() as usize;
        let name = String::from_utf8_lossy(r.bytes(len)).into_owned();
        out.push(PetEffect { icon, left_ms, name });
    }
    (!r.error).then_some(out)
}

/// go-eqoa-server's own: a tint on the bodies of the NPCs of a name (the server
/// owner, 2026-11-10: Yhafiran, "the black and green oriental dragon", drawn as
/// the disc's green one tinted; `crate::npctint`, the game's `npctint`). The
/// offer, server to client once the player is in the world: a u8, the newest
/// layout of [`OP_NPC_TINTS`] the server sends (1; 2 with a glow after each
/// colour, petition 1893). CLIENT has no such case, so a PS2 ignores it and draws
/// every body in its own colours.
pub const OP_NPC_TINTS_OFFER: u16 = 0x7E3E;
/// The answer to [`OP_NPC_TINTS_OFFER`], client to server, at once: a u8, the
/// layout this client reads (the offer's, at most `crate::npctint::LAYOUT`) to
/// be sent the tints (this client drops what it held and is sent the set there
/// is), 0 to stop.
pub const OP_NPC_TINTS_ANSWER: u16 = 0x7E3F;
/// The tints, from a server the client answered ([`OP_NPC_TINTS_ANSWER`]), at
/// once and whenever a GM changes them: a u16 count, then each its NPC name (a
/// u8 byte count and UTF-8, keyed as `crate::npctint::key` keys one), its u32
/// colour 0xRRGGBBAA and, in layout 2, its u32 glow (`crate::npctint::read`).
/// Each is the whole set.
pub const OP_NPC_TINTS: u16 = 0x7E40;

/// go-eqoa-server's own: a monster's name and level, shown when the player
/// talks to it (□, Q) and it has nothing to say (the GM Jibrish's petition 1459,
/// 2026-11-10: "press the mob information button on monster ... This should
/// ONLY show the monsters name and level, nothing else"; the server's
/// `world.mob_info`). The offer, server to client once the player is in the
/// world: a u8, 1, the layout of [`OP_MOB_INFO`]. CLIENT has no such case, so a
/// PS2 ignores it and is shown the two lines in a dialogue box (0x0046).
pub const OP_MOB_INFO_OFFER: u16 = 0x7E4C;
/// The answer to [`OP_MOB_INFO_OFFER`], client to server, at once: a u8, 1 to be
/// sent [`OP_MOB_INFO`] in place of the dialogue box.
pub const OP_MOB_INFO_ANSWER: u16 = 0x7E4D;
/// A monster's name and level, from a server the client answered, in answer to
/// its 0x0004 on a living monster with nothing to say (or out of the talk's
/// reach): a u8 layout (1), the u32 id the 0x0004 named, a u8 its level and its
/// name (a u8 byte count and the bytes). Shown in the target's details box as
/// CLIENT's `DrawTargetInfo` shows a pet's (`Play::target_info_text`), while □
/// is held.
pub const OP_MOB_INFO: u16 = 0x7E4E;

/// go-eqoa-server's own: item links in chat (the GM Jibrish's petition 1463,
/// `crate::itemlink`). Both ways: the server's offer at every world entry (a
/// u8, the layout, `itemlink::LAYOUT`), and the client's answer (a u8, 1 to be
/// sent [`OP_ITEM_LINK_LINE`] lines and to send [`OP_ITEM_LINK_SAY`], 0 for the
/// plain lines a PS2 gets). CLIENT has no case for it: a PS2 ignores the offer.
pub const OP_ITEM_LINKS: u16 = 0x7E4F;

/// go-eqoa-server's own: the profanity filter set from the client's Options >
/// Interface row (the GM Jibrish's petition 1490). Both ways: the server's
/// offer at every world entry (a u8, the layout, 1), and the client's answer
/// (a u8: 0 off, 1 High, 2 Medium), at once and whenever the row changes. The
/// filter is the server's own (retail's 0x0045, on at every login); a PS2
/// ignores the offer and keeps it on.
pub const OP_PROFANITY_CHOICE: u16 = 0x7E60;
/// [`OP_PROFANITY_CHOICE`]'s answers: off, High (every word; before the GM
/// Jibrish's petition 1625 the only on) and Medium (only racial slurs).
pub const PROFANITY_OFF: u8 = 0;
pub const PROFANITY_HIGH: u8 = 1;
pub const PROFANITY_MEDIUM: u8 = 2;
/// A typed line with links, client to server, in place of its retail message
/// (`itemlink::say`).
pub const OP_ITEM_LINK_SAY: u16 = 0x7E50;
/// A retail chat line with its links, server to client (`itemlink::read_line`).
pub const OP_ITEM_LINK_LINE: u16 = 0x7E51;
/// A link opened, client to server: the u32 link id (`itemlink::ask`).
pub const OP_ITEM_LINK_ASK: u16 = 0x7E52;
/// The answer to [`OP_ITEM_LINK_ASK`]: the link id, and its item's snapshot as
/// an inventory record or none (`itemlink::read_record`).
pub const OP_ITEM_LINK_RECORD: u16 = 0x7E53;

/// One quest on the tracker ([`OP_QUEST_PROGRESS_DATA`]): its name and its
/// step's objectives.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackedQuest {
    pub name: String,
    pub goals: Vec<QuestGoal>,
}

/// One objective: its words, and how many the player has of how many are
/// needed (`need` 0: a thing to do, nothing to count).
#[derive(Clone, Debug, PartialEq)]
pub struct QuestGoal {
    pub text: String,
    pub have: u16,
    pub need: u16,
}

impl QuestGoal {
    /// Counted, and all there.
    pub fn done(&self) -> bool {
        self.need > 0 && self.have >= self.need
    }
}

/// [`OP_QUEST_PROGRESS_DATA`]'s body, or None when it is short.
fn read_quest_progress(r: &mut Reader) -> Option<Vec<TrackedQuest>> {
    let text = |r: &mut Reader| {
        let n = r.u8() as usize;
        String::from_utf8_lossy(r.bytes(n)).into_owned()
    };
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let name = text(r);
        let g = r.u8() as usize;
        let goals = (0..g)
            .map(|_| {
                let text = text(r);
                QuestGoal { text, have: r.u16(), need: r.u16() }
            })
            .collect();
        out.push(TrackedQuest { name, goals });
    }
    (!r.error).then_some(out)
}

/// One quest mark ([`OP_QUEST_MARKERS_DATA`]).
#[derive(Clone, Debug, PartialEq)]
pub struct QuestMark {
    /// 1 a quest the player may take from this NPC ("!"), 2 a step of its
    /// quests to do at it ("?"), 3 a kill or loot step's creature ("?")
    pub kind: u8,
    /// `gWorldInfo`'s index
    pub world: u8,
    pub x: f32,
    pub z: f32,
    /// the NPC's (or the creature's) name
    pub npc: String,
    pub quests: Vec<MarkQuest>,
}

/// One quest on a mark: its name and levels, a line (the start of the
/// giver's offer, or the step's journal line) and a detail (its rewards, or
/// what the step needs).
#[derive(Clone, Debug, PartialEq)]
pub struct MarkQuest {
    pub name: String,
    pub levels: (u8, u8),
    pub line: String,
    pub detail: String,
}

/// [`OP_QUEST_MARKERS_DATA`]'s body, or None when it is short.
fn read_quest_marks(r: &mut Reader) -> Option<Vec<QuestMark>> {
    let text = |r: &mut Reader| {
        let n = r.u8() as usize;
        String::from_utf8_lossy(r.bytes(n)).into_owned()
    };
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let (kind, world, x, z) = (r.u8(), r.u8(), r.f32(), r.f32());
        let npc = text(r);
        let q = r.u8() as usize;
        let quests = (0..q)
            .map(|_| {
                let name = text(r);
                let levels = (r.u8(), r.u8());
                MarkQuest { name, levels, line: text(r), detail: text(r) }
            })
            .collect();
        out.push(QuestMark { kind, world, x, z, npc, quests });
    }
    (!r.error).then_some(out)
}

#[derive(Clone, Debug)]
pub struct Shard {
    pub name: String,
    pub recommended: u8,
    pub endpoint: u64,
    pub language: i8,
}

#[derive(Clone, Debug, Default)]
pub struct Character {
    pub name: String,
    pub server_id: i64,
    pub model: i64,
    pub class: i64,
    pub race: i64,
    pub level: i64,
    pub hair_color: i64,
    pub hair_length: i64,
    pub hair_style: i64,
    pub face: i64,
    /// The raw bytes after the zigzag fields (robe, weapons, armour): the
    /// disc's [`DISC_GEAR_SIZE`], or the live game's [`LIVE_GEAR_SIZE`] when
    /// its look is in force.
    pub gear: Vec<u8>,
    /// Its race's options re-chosen at select's Customize (a game race's skin
    /// shade, `raceopts`): sent after 0x002A's five ints as a creation's are
    /// after 0x002B's ([`crate::raceopts::choices_block`]), only when there
    /// are some, which there are only from a server that sent the race's
    /// options. Empty: as it was.
    pub options: Vec<(String, u8)>,
}

/// The head of the MemoryDump: `VIPlayer::Read`, as far as placing the
/// player needs.
#[derive(Clone, Debug, Default)]
pub struct Player {
    /// The dump's `m_p0662C`: the world is not on the disc, so `LoadWorld`
    /// loads `world_path`; clear, it loads the world table's file for `world`.
    pub world_not_on_disc: bool,
    pub world_path: String,
    pub server_id: i64,
    pub name: String,
    pub class: i64,
    pub race: i64,
    pub level: i64,
    pub world: i64,
    pub pos: [f32; 3],
    /// Heading, pitch, roll.
    pub hpr: [f32; 3],
}

/// `VINewChar`, the character-creation form `CreateCharacter` sends as
/// 0x002B (`SendNewChar`). The decomp's pages fill it: race, sex and city
/// (`VIWndCharCreateRace`), class and the stats (`VIWndCharCreateClass`), the
/// look (`VIWndCharCreateMisc`: face, hair style and length 0-3, hair colour
/// 0-7), the name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewChar {
    pub name: String,
    pub race: i32,
    pub class: i32,
    pub sex: i32,
    pub hair_color: i32,
    pub hair_length: i32,
    pub hair_style: i32,
    pub face: i32,
    /// `m_p38`, the city: a human's start (1 and 2), 0 for every other race.
    pub city: i32,
    /// The points spent over the race's and the class's bonuses (the page
    /// sends a stat less both bonuses), 20 at most in all.
    pub stats: [i32; 7],
    /// A custom race's own options chosen, each row's key and step (`raceopts`):
    /// sent after 0x002B's own fields, only when there are some, which there
    /// are only from a server that sent the race's options.
    pub options: Vec<(String, u8)>,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// 0x0000: the server's version.
    Version(u32),
    ShardList(Vec<Shard>),
    Characters(Vec<Character>),
    /// 0x0012: the name the server picked (`m_wndCharCreate->SetName`).
    RandomName(String),
    /// A creation refused, and the WORDS.TXT line the select page shows:
    /// 0x002E (269), 0x002F (829), 0x0065 (828).
    CreateRefused(i32),
    /// 0x0013: the clock pair (`SetTime`).
    Time(u64, u64),
    /// 0x07D1: the expansion-flag mask.
    Expansion(u32),
    /// 0x007E: the player's pet (`m_petId`); 0x007F: none.
    Pet(Option<u32>),
    /// 0x007B (`Msg007B`): the answer to an assist (0x00CB, 0x007B), the
    /// id to target, -1 for none.
    Assist(u32),
    /// 0x003B: an item into the inventory (`AddInvItem`).
    InvAdd(Box<crate::dump::NetItem>),
    /// 0x003C (`Msg003C`): `n` taken from slot `slot`; `sound` with a merchant.
    InvTook { slot: u8, sound: u8, n: i32 },
    /// 0x003D (`Op003D`): the item at `from` moved to `to`.
    InvMove { from: u8, to: u8 },
    /// 0x003E (`Msg003E`): a charge used (`charge`), else the item gone.
    InvUse { slot: u8, charge: u8 },
    /// 0x003F (`Msg003F`): item `i` worn in equip slot `slot`.
    InvEquip { i: u8, slot: u8 },
    /// 0x0040 (`Msg0040`): equip slot `slot` emptied.
    InvUnequip(u8),
    /// 0x000A (`Msg000A`): item `slot` onto quick slot `pos`.
    QuickSlot { slot: u8, pos: u8 },
    /// 0x0021 (SOE's live client, not the disc: its dispatcher's case at
    /// 0x012CADEC): weapon set `slot`, its name (kept to 15 characters, a
    /// 32-byte buffer) and its main and off hands' items by `VIItem::m_id`
    /// (-1 none).
    GearSet { slot: u8, name: String, main: u32, off: u32 },
    /// 0x0060 (`Msg0060`): the item with inventory id `id` at `hp`.
    ItemHp { id: u32, hp: i32 },
    /// 0x0772 (`Msg0772`): the group's roster, ids and names.
    GroupRoster { ids: Vec<u32>, names: Vec<String> },
    /// [`OP_RAID_ROSTER`]: a message's part of the whole raid group.
    RaidRoster { total: usize, first: usize, rows: Vec<RaidMember> },
    /// [`OP_DPS_METER_DATA`]: the meter's fight as it stands.
    DpsMeter(DpsMeter),
    /// [`OP_RAID_STATE`]: the raid as it is now.
    RaidState(crate::raid::State),
    /// [`OP_MOB_INFO`]: the monster the client knows by `id` has nothing to say;
    /// its name and level, for the target's details box.
    MobInfo { id: u32, level: u8, name: String },
    /// [`OP_ITEM_LINK_LINE`]: the links of the chat line the next event carries
    /// (the line's own message, read as ever, comes right after it).
    ChatLinks(Vec<crate::itemlink::ChatLink>),
    /// [`OP_ITEM_LINK_RECORD`]: a link's item as the server snapshotted it, or
    /// none (expired, or never made).
    ItemLinkRecord { id: u32, item: Option<Box<crate::dump::NetItem>> },
    /// [`OP_RAID_ROLL`]: a loot roll offered, or over (0 seconds left).
    RaidRoll(crate::raid::Roll),
    /// [`OP_RAID_MASTER_LOOT`]: the master looter's list for the corpse it
    /// opened, or none (over).
    MasterLoot(Option<crate::raid::MasterLoot>),
    /// [`crate::readycheck::OP_READY_CHECK`]: a ready check asked, or over for
    /// this client (0 seconds left).
    ReadyCheck(crate::readycheck::Check),
    /// 0x0773 (`Msg0773`): an invitation from `id`.
    GroupInvite { id: u32, x: i32, name: String },
    /// 0x0774 (`Msg0774`): the invitation from `id` withdrawn.
    GroupInviteGone(u32),
    /// 0x069A: out of the group, why (`sub`).
    GroupRemoved(u8),
    /// 0x062F: the group disbanded.
    GroupDisbanded,
    /// 0x069C: a group notice: sub 3 alone, 1/2/4/5 about a name.
    GroupNotice { sub: u8, name: String },
    /// 0x00D3 (`Msg00D3`): `name` offers to summon the player (`kind` 0,
    /// word 610) or to resurrect them (else, 611); the box's answer is
    /// 0x00D4.
    SummonPrompt { name: String, kind: u8 },
    /// The server opened the game connection (promotion).
    Promoted,
    /// 0x000D parsed: the player's own entity id and where they stand.
    WorldEntry { self_id: u64, player: Player, dump: Box<crate::dump::MemoryDump> },
    /// 0x00F8: the run speed (0 stops).
    Speed(f32),
    /// 0x0010, the target-info reply (`Msg0010`): the disposition (the con
    /// colour), two more bytes, the title and the guild. Sandstorm's reply
    /// stops before the trailing token, so it is taken as answered whatever
    /// the token, as eqoa-port's `VI_HOST_ONLY` does.
    TargetInfo { disposition: u8, b: u8, c: u8, title: String, guild: String },
    /// 0x0046: a dialog (an NPC's words), raw (`ShowDialog` expands it).
    Dialog { id: u32, text: Vec<u16> },
    /// 0x0034: a dialog with answers (raw; `Msg0034` expands them).
    DialogBox { id: u32, text: Vec<u16>, options: Vec<Vec<u16>> },
    /// 0x00B2 (`Msg00B2`, the respawn/teleport): the world, where, the
    /// heading, pitch and roll, and the token 0x0049 answers with
    /// ([`Session::answer_teleport`], once the move is done).
    Teleport { world: u8, pos: [f32; 3], hpr: [f32; 3], token: u32 },
    /// 0x0020: the player's level and experience.
    LevelExp { level: i64, exp: i64 },
    /// 0x0775: an entity plays an action (`VICSprite::SetAction`).
    Action { id: u32, action: u8 },
    /// 0x07A3: who last sent a tell (`/reply` answers them).
    ReplyName(String),
    /// 0x0045 (`Msg0045`): the profanity filter's state, the server's answer
    /// to the client's own 0x0045 (action 0x0B052000).
    ProfanityFilter(bool),
    /// 0x00D9: the player's +0x2C.
    PlayerP2C(i32),
    /// A line for the chat log, in its colour (RGBA 0..1): 0x0A7A white,
    /// 0x0A7B its own colour, 0x0758 (narrow) white, 0x00CE (an error: "Tell
    /// Failed", an unknown command) red.
    Chat(String, [f32; 4]),
    /// 0x069E, the group's chat: a line for the chat log in `GROUP_CHAT`'s
    /// blue, "<Name> tells the group: <text>" as the servers word it, and the
    /// u32 before it (the sender's id as the server knows it; CLIENT reads
    /// it and lets it go). Its own event, so what reads the group's lines
    /// (the client's group waypoint) never takes another channel's.
    GroupChat { from: u32, text: String },
    /// A state channel's full value (0x00-0x17 entities, 0x42 stats...).
    State { channel: u8, data: Vec<u8> },
    /// The ability list (each carries the list's token, `m_p2BB60`, which
    /// 0x0032 echoes): 0x00B3 one ability (flag: say "learned"), 0x00B4 an
    /// ability by display index onto a slot or off it, 0x00B5 a slot moved,
    /// 0x00B6 a display index moved, 0x00B8 one gone, 0x00CD a lockout.
    AbilityAdd { token: u32, flag: u32, ability: Box<crate::dump::Ability> },
    AbilitySlot { token: u32, index: u8, slot: u8 },
    AbilityMoveSlot { token: u32, from: u8, to: u8 },
    AbilityMoveIndex { token: u32, from: u8, to: u8 },
    AbilityRemove { token: u32, index: i32 },
    AbilityLockout { index: u8, token: u32, lockout: i32 },
    /// 0x00C5 (fades) and 0x00C7 (instant): the weather byte
    /// (`VIClient::SetWeather`).
    Weather { weather: u8, instant: bool },
    /// 0x00FF (`Msg00FF`): every ability (display index below 252) off
    /// the bar; the answer to 0x00FE.
    AbilityClearBar { token: u32 },
    /// 0x00FD (`Msg00FD`): an ability's usable flag (`m_p14`); 0 also
    /// takes it off the bar.
    AbilityUsable { token: u32, id: i32, p14: i32 },
    /// Any other opcode, not handled here yet.
    Opcode { op: u16, data: Vec<u8> },
    /// The connection ended (`VIClient::Disconnect`), with CLIENT's reason
    /// (`m_p4F0E8`, read by `ConnectStatusString`; see `disconnect_word`):
    /// a DRDP 0xFD with its byte mapped as `PumpMessages` maps it, or one of
    /// the dispatcher's disconnect opcodes (0x07A4, "You have logged in
    /// from another location.", is reason 8).
    Disconnected(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Login = 0,
    Shards = 1,
    Shard = 2,
    Characters = 3,
    Selecting = 4,
    World = 5,
}

static DEBUG_SINK: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Where `EQOA_NET_DEBUG`'s lines go when not to stderr: a page has no
/// stderr, so it sets its console (`?netdebug`).
pub fn set_debug_sink(f: fn(&str)) {
    let _ = DEBUG_SINK.set(f);
}

fn debug_line(s: &str) {
    match DEBUG_SINK.get() {
        Some(f) => f(s),
        None => eprintln!("{s}"),
    }
}

/// VITimer's ticks in `us` microseconds: 983.04 a second (`Session::timer`).
pub fn ticks(us: i64) -> i64 {
    us.saturating_mul(98_304) / 100_000_000
}

pub struct Session {
    pub drdp: Drdp<Ee>,
    socket: Box<dyn Datagrams>,
    pub conn: Option<ConnId>,
    /// Messages not sent since the game connection went from under the session
    /// (`game_conn`), and whether it did.
    pub sends_lost: u32,
    conn_gone: bool,
    pub state: State,
    pub self_id: Option<u64>,
    /// 0x07D1's mask (`VIClient+0x50304`), as the server last sent it.
    pub expansion: u32,
    /// The most entity slots and the farthest view distance the server
    /// offered on this connection ([`OP_ENTITY_VIEW`]); none from one that
    /// never does (every retail server, Sandstorm's too).
    pub entity_view_offer: Option<(u32, u32)>,
    /// The live game's look is in force ([`OP_LIVE_LOOK_IN_FORCE`] said so):
    /// character lists are read as the live CLIENT's, and the entity channels
    /// of its length too (`entity::Entities::live_look`). Kept for the
    /// session, as the server keeps it, through the world's connection.
    pub live_look: bool,
    /// go-eqoa-server's armour is in force ([`OP_LIVE_LOOK_IN_FORCE`]'s third
    /// u32 has [`LIVE_LOOK_ARMOR`]): set 9 is drawn and the trims are read,
    /// each record's gear [`ARMOR_GEAR_SIZE`]. Kept for the session.
    pub armor_look: bool,
    /// The characters this account may hold and character select lists
    /// (go-eqoa-server's `world.max_characters`), told with the look in
    /// force ([`OP_LIVE_LOOK_IN_FORCE`]'s second u32) to a client that said
    /// its select page scrolls; [`CLIENT_CHAR_SLOTS`] (CLIENT's) until then
    /// and on every other server.
    pub char_slots: u32,
    /// The server offered bug reports ([`OP_BUG_REPORTS`]) and this client
    /// took them: F3 and the note. Kept for the session.
    pub bug_reports: bool,
    /// The most characters of a petition's text Support's forms hold: what
    /// the offer said the server takes, up to [`PETITION_TEXT_MOST`], else
    /// [`PETITION_TEXT_DISC`]. Kept for the session.
    pub petition_text: usize,
    /// The server offered to take crash reports ([`OP_CRASH_OFFER`]): the
    /// most bytes of each, for the game to send what it kept
    /// (`eqoa::crashfile`), which takes it.
    pub crash_offer: Option<usize>,
    /// The server's answers to the crash reports sent ([`OP_CRASH_FILED`]):
    /// each report's number and its ticket (0 not kept), for the game.
    pub crash_filed: Vec<(u32, u32)>,
    /// The server lets any member of a group invite into it
    /// ([`OP_MEMBER_INVITES`]). Kept for the session.
    pub member_invites: bool,
    /// The mercenaries the server last told ([`OP_MERCS_DATA`]); none from a
    /// server that never offered them, and at a new connection.
    pub mercs: Option<Mercs>,
    /// The group search's standing the server last told
    /// ([`OP_GROUP_REG_STATE`]); none from a server that never offered it, and
    /// at a new connection.
    pub group_reg: Option<GroupReg>,
    /// What character creation takes past the game's, as the server last told
    /// it ([`OP_CREATION_RACES`]); none from a server that never offers it.
    pub creation: Option<crate::creation::Creation>,
    /// Each custom race's own options, as the server last told them
    /// ([`OP_RACE_OPTIONS_DATA`]); none from a server that never offers them.
    pub race_options: Option<crate::raceopts::Options>,
    /// The looks list the server last sent ([`OP_RACE_LOOKS`]: its body),
    /// until the game takes it (`raceopts`), whatever screen is up: the
    /// pregame's, the world's, a script's straight login.
    pub race_looks: Option<Vec<u8>>,
    /// The NPCs' body tints the server last sent ([`OP_NPC_TINTS`], by name as
    /// `crate::npctint::key` keys one), until the game takes them (`npctint`):
    /// the empty set at its offer and at a new connection, this client holding
    /// none then.
    pub npc_tints: Option<Vec<(String, crate::npctint::Tint)>>,
    /// The layout of [`OP_NPC_TINTS`] this client answered the offer with.
    pub npc_tints_layout: u8,
    /// The server has this client's character flying ([`OP_FLY`]); gone with
    /// the connection, as the server's goes with its login. A dump (a respawn,
    /// a restart of the server's) leaves it.
    pub flying: bool,
    /// The server offered the whole of a raid group
    /// ([`OP_RAID_ROSTER_OFFER`]), and what it was last told since the offer.
    pub raid_roster_offer: bool,
    raid_roster_asked: Option<bool>,
    /// Whether the server offered others' power on this connection
    /// ([`OP_POWER_BARS`]), what it was last told (none since the offer),
    /// and the set it last sent ([`OP_POWER_BARS_DATA`]: ids and power of
    /// 255).
    pub power_bars_offer: bool,
    power_bars_asked: Option<bool>,
    pub power_bars: Vec<(u32, u8)>,
    /// The offer said each member's group comes too, when asked (2).
    raid_roster_groups: bool,
    /// The offer said each member's kind comes too, after its group (3).
    raid_roster_kinds: bool,
    /// The server's worlds past the disc's, as it last listed them
    /// ([`OP_SERVER_WORLDS_DATA`]); none from a server that never offers them.
    pub server_worlds: Option<Vec<crate::serverworlds::World>>,
    /// The builder's answer still to come: the list is in and being built.
    worlds_building: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    /// The offer was taken and the server not yet told what this client holds.
    worlds_pending: bool,
    /// The characters a login needs the server's worlds for, as the server
    /// last said ([`OP_SERVER_WORLDS_CHARS`]); none from a server that never
    /// says.
    worlds_chars: Option<Vec<u32>>,
    /// A character was played and the game connection has not opened yet
    /// (`select_character` to `TYPE_CONNECT`): what is sent now goes on a
    /// connection the server may already have left.
    picked: bool,
    /// The worlds held, as the server was told while `picked`: told again on
    /// the game connection (`worlds_answer`).
    worlds_retell: Option<Vec<u8>>,
    /// The server offered the combat record ([`OP_COMBAT_RECORD`]) and this
    /// client took it: Character > Combat Record. Kept for the session.
    pub combat_record: bool,
    /// The server offered capes ([`OP_CAPES`]) and this client took them: a
    /// cape is worn in its slot and drawn. Kept for the session.
    pub capes: bool,
    /// The records the pack (worn items among them) and the bank hold, as the
    /// server last said before a MemoryDump ([`OP_ITEM_LISTS_IN_FORCE`]);
    /// CLIENT's [`ITEM_LIST_DISC`] of each from a server that never says.
    pub item_lists: [u32; 2],
    /// The capes of the characters in the list, by server id
    /// ([`OP_CAPES_LIST`]), and of the players seen, by the id this client
    /// knows each by ([`OP_CAPES_IN_VIEW`]); `capes_changed` counts the
    /// messages, so what draws them sees a new one.
    pub cape_list: std::collections::HashMap<i64, crate::cape::Cape>,
    /// More of each listed character, by server id ([`OP_SELECT_INFO_DATA`]): its
    /// own race and its name, its mastery points, the rows Customize re-chooses;
    /// empty from a server that sends none.
    pub select_info: std::collections::HashMap<i64, crate::selectinfo::Info>,
    pub capes_in_view: std::collections::HashMap<u32, crate::cape::Cape>,
    pub capes_changed: u64,
    /// The server offered the jump ([`OP_JUMP`]) and this client took it (the
    /// jump key, `walker`); gone with the connection, as the server's goes with
    /// its player.
    pub jumps: bool,
    /// The server offered a GM's travel by the map ([`OP_MAP_TRAVEL`]): the
    /// map's Ctrl+click asks for its spot (`map_travel_to`); gone with the
    /// connection, as the GM's channel goes with its login.
    pub map_travel: bool,
    /// Whether the server offered quest marks ([`OP_QUEST_MARKERS`]), what it
    /// was last told (none since the offer), and the set it last sent
    /// ([`OP_QUEST_MARKERS_DATA`]).
    pub quest_marks_offer: bool,
    quest_marks_asked: Option<bool>,
    pub quest_marks: Vec<QuestMark>,
    /// The same for the quest tracker ([`OP_QUEST_PROGRESS`],
    /// [`OP_QUEST_PROGRESS_DATA`]).
    pub quest_progress_offer: bool,
    quest_progress_asked: Option<bool>,
    pub quest_progress: Vec<TrackedQuest>,
    /// Whether the server offered the pet's effects ([`OP_PET_EFFECTS`]),
    /// what it was last told (none since the offer), the set it last sent
    /// ([`OP_PET_EFFECTS_DATA`]) and when it came (`now`'s clock): what is
    /// left of each is counted down from then (`pet_effects_left`).
    pub pet_effects_offer: bool,
    pet_effects_asked: Option<bool>,
    pub pet_effects: Vec<PetEffect>,
    pub pet_effects_at: u32,
    /// Whether the server offered the number tags ([`OP_NUMBER_TAGS_OFFER`])
    /// on this connection, and what it was last told (none since the offer).
    pub number_tags_offer: bool,
    /// The layout the offer named ([`OP_NUMBER_TAGS_OFFER`]).
    number_tags_layout: u8,
    number_tags_asked: Option<bool>,
    /// The same for the damage meter ([`OP_DPS_METER_OFFER`]).
    pub dps_meter_offer: bool,
    dps_meter_asked: Option<bool>,
    /// Whether the server offered the raid's windows ([`OP_RAID_OFFER`]) on
    /// this connection, and what it was last told (none since the offer).
    pub raid_offer: bool,
    raid_asked: Option<bool>,
    /// The same for the ready check's window
    /// ([`crate::readycheck::OP_READY_OFFER`]).
    pub ready_offer: bool,
    ready_asked: Option<bool>,
    /// The same for the buff window ([`crate::buffs::OP_BUFF_WINDOW`]), and
    /// the set it last sent ([`crate::buffs::OP_BUFF_WINDOW_DATA`]) and when it
    /// came (`now`'s clock): what is left of each is counted down from then.
    pub buffs_offer: bool,
    buffs_asked: Option<bool>,
    pub buffs: Vec<crate::buffs::Line>,
    pub buffs_at: u32,
    /// Whether the server offered the quest marks over the NPCs' heads
    /// ([`crate::headmarks::OP_OFFER`]) on this connection, what it was last
    /// told (none since the offer), and the marks it last sent
    /// ([`crate::headmarks::OP_MARKS`]) by the entity's id.
    pub head_marks_offer: bool,
    head_marks_asked: Option<bool>,
    pub head_marks: std::collections::HashMap<u32, u8>,
    /// Whether the server offered item links ([`OP_ITEM_LINKS`]) on this
    /// connection, and what it was last told (none since the offer).
    pub item_links_offer: bool,
    item_links_asked: Option<bool>,
    /// Whether the server offered to take the profanity filter's setting
    /// ([`OP_PROFANITY_CHOICE`]) on this connection, and what it was last told
    /// (none since the offer).
    pub profanity_offer: bool,
    profanity_asked: Option<u8>,
    /// The worn looks the character hides, as the server told them
    /// (`crate::hidelook`): none offered until it does.
    pub hide_look: crate::hidelook::State,
    user: String,
    token: [u8; 32],
    start: Instant,
    base: u32,
    /// Datagrams sent so far: `usocket_sendto_addr` draws newlib's `rand()`
    /// twice for each, so a client keeping animation in step draws as many.
    pub packets_sent: u64,
    /// Datagrams read so far, and when `pump` last read one: what a stall's
    /// line says of the server (the client's `stall`: still sending while
    /// the client was held, or gone quiet before).
    pub datagrams_in: u64,
    pub last_datagram: Option<Instant>,
    /// `EQOA_NET_DEBUG`: every datagram and message on stderr.
    debug: bool,
    /// `m_p382E4`, the chat mode: the last one 0x000E sent (`GroupChanged`
    /// sends it again at every world entry).
    pub chat_mode: u8,
    /// Events handed back to come out of the next `pump` first: what
    /// arrived with the world entry, after it (0x1005, 0x03D5), for the
    /// game to read once it is up.
    pub held: Vec<Event>,
    /// The state channels' updates that came before the world entry, in
    /// order: CLIENT's connection keeps every channel whatever the screen,
    /// but the screens before the world read none of ours, and a server
    /// sends a channel again only when it changes (its own body, compass
    /// and bars were left out until then). `route` keeps them, and the
    /// world entry hands them out right after its `WorldEntry`.
    early_states: Vec<Event>,
    /// `m_clockA`, `m_clockB`: the server's two clocks (0x0013), synced on
    /// the local timer (`timer`); the time of day is clock B's
    pub clock_a: SyncTime,
    pub clock_b: SyncTime,
    /// While some, every message the game hands the connection, in order,
    /// by its DRDP type (251 guaranteed, under 248 its state channel): a
    /// test's view of what was sent and when, before the transport's own
    /// flow decides when it leaves. None in play.
    pub record: Option<Vec<(u8, Vec<u8>)>>,
    /// 0x0790's probe (`m_p4F0F0`, `m_p4F0F8`, `m_p4F100`, `m_p4F108`),
    /// clear from `Connect` on.
    pub probe: Probe,
}

/// 0x0790's probe: `Msg0790` keeps an endpoint, a token and eight more
/// bytes; `Msg0790Done`, before each of the frame's flushes, sends 0x0992
/// (the token and the bytes) outside any connection to that endpoint, at
/// most once a connection timeout (a second at most), for as long as the
/// session lasts; and `Msg0993` answers the token coming back (a DRDP type
/// 0xF8: a datagram outside any connection) with 0x0791 on the game
/// connection. No server we know sends 0x0790 (none in Sandstorm's
/// captures, none in go-eqoa-server).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// `m_p4F0F0`: the endpoint 0x0992 goes to; -1 none (`Connect`'s)
    pub endpoint: i64,
    /// `m_p4F0F8`: the token
    pub token: u32,
    /// `m_p4F100`: the eight bytes after it
    pub data: u64,
    /// `m_p4F108`: when 0x0992 last went, on `timer`'s clock; -1 never
    pub last: i64,
}

impl Default for Probe {
    fn default() -> Self {
        Probe { endpoint: -1, token: 0, data: 0, last: -1 }
    }
}

/// Where the DRDP clock (`Session::now`) starts: the wall clock's
/// milliseconds, their low 30 bits. As the whole u32 the clock passed 2^31
/// at 2026-10-28 00:54:06 UTC, and the loading screen's gears, an animation
/// begun at 0 on it, stopped every client entering the world; it would have
/// wrapped at 2026-11-21 21:25:29.7 UTC, and every timer on it that
/// subtracts as an i64 (the loading screen's, the alarm's, the windows')
/// would have stalled in a session across that moment. From here it stays
/// below 2^31 for 12 days of a session and never wraps, as the console's,
/// which is its own clock since boot. The low 16 bits, the client's drdp
/// port (`drdp_create_addr`), are the wall clock's as before, and the
/// instance (the clock at creation) keeps 30 of its bits.
pub fn clock_base(wall_ms: u128) -> u32 {
    (wall_ms % (1 << 30)) as u32
}

fn ipv4(a: &SocketAddr) -> [u8; 4] {
    match a {
        SocketAddr::V4(v) => v.ip().octets(),
        SocketAddr::V6(_) => [0; 4],
    }
}

impl Session {
    /// `VIClient::Connect`: a DRDP client on a fresh UDP socket, a connection
    /// to the login endpoint, the hello and 0x0904. A server at an IPv6
    /// address (a name with no IPv4 one, `addr::resolve`) is reached through
    /// the network's gateway to IPv4, as is every shard after it.
    pub fn connect(server: SocketAddr, user: &str, token: [u8; 32]) -> std::io::Result<Self> {
        let (nat64, server_ip) = match Nat64::of(&server) {
            Some((n, ip)) => (Some(n), ip),
            None => (None, ipv4(&server)),
        };
        let any = if nat64.is_some() { "[::]:0" } else { "0.0.0.0:0" };
        let socket = UdpSocket::bind(any)?;
        // The local address as the server will see it: route a probe.
        let probe = UdpSocket::bind(any)?;
        probe.connect(server)?;
        let local = SocketAddr::new(probe.local_addr()?.ip(), socket.local_addr()?.port());
        socket.set_nonblocking(true)?;
        let udp = Udp { socket, buf: vec![0u8; 65536], nat64 };
        Self::connect_with(Box::new(udp), (ipv4(&local), local.port()), (server_ip, server.port()), user, token)
            .ok_or_else(|| std::io::Error::other("connection_open failed"))
    }

    /// `connect` over any datagram carrier: `local` is the address the
    /// server will see the client at (a relay's own, through one).
    pub fn connect_with(socket: Box<dyn Datagrams>, local: ([u8; 4], u16), server: ([u8; 4], u16), user: &str, token: [u8; 32]) -> Option<Self> {
        let base = clock_base(SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_millis()));
        let mut drdp = Drdp::<Ee>::create_client(Addr { ip: local.0, port: local.1 }, base);
        drdp.set_port_udp(local.1);
        let mut s = Session {
            drdp,
            socket,
            conn: None,
            state: State::Login,
            self_id: None,
            expansion: 0,
            entity_view_offer: None,
            live_look: false,
            armor_look: false,
            char_slots: CLIENT_CHAR_SLOTS,
            bug_reports: false,
            petition_text: PETITION_TEXT_DISC,
            crash_offer: None,
            crash_filed: Vec::new(),
            member_invites: false,
            mercs: None,
            group_reg: None,
            creation: None,
            server_worlds: None,
            worlds_building: None,
            worlds_pending: false,
            worlds_chars: None,
            picked: false,
            worlds_retell: None,

            race_options: None,
            race_looks: None,
            npc_tints: None,
            npc_tints_layout: 1,
            flying: false,
            raid_roster_offer: false,
            raid_roster_asked: None,
            power_bars_offer: false,
            power_bars_asked: None,
            power_bars: Vec::new(),
            raid_roster_groups: false,
            raid_roster_kinds: false,
            combat_record: false,
            capes: false,
            item_lists: [ITEM_LIST_DISC; 2],
            cape_list: Default::default(),
            select_info: Default::default(),
            capes_in_view: Default::default(),
            capes_changed: 0,
            jumps: false,
            map_travel: false,
            quest_marks_offer: false,
            quest_marks_asked: None,
            quest_marks: Vec::new(),
            quest_progress_offer: false,
            quest_progress_asked: None,
            quest_progress: Vec::new(),
            pet_effects_offer: false,
            pet_effects_asked: None,
            pet_effects: Vec::new(),
            pet_effects_at: 0,
            number_tags_offer: false,
            number_tags_layout: 0,
            number_tags_asked: None,
            dps_meter_offer: false,
            dps_meter_asked: None,
            raid_offer: false,
            raid_asked: None,
            ready_offer: false,
            ready_asked: None,
            buffs_offer: false,
            buffs_asked: None,
            buffs: Vec::new(),
            buffs_at: 0,
            head_marks_offer: false,
            head_marks_asked: None,
            head_marks: Default::default(),
            item_links_offer: false,
            item_links_asked: None,
            profanity_offer: false,
            profanity_asked: None,
            hide_look: crate::hidelook::State::default(),
            user: user.to_string(),
            token,
            start: Instant::now(),
            base,
            packets_sent: 0,
            datagrams_in: 0,
            last_datagram: None,
            debug: std::env::var_os("EQOA_NET_DEBUG").is_some(),
            sends_lost: 0,
            conn_gone: false,
            chat_mode: 0,
            held: Vec::new(),
            early_states: Vec::new(),
            clock_a: SyncTime::default(),
            clock_b: SyncTime::default(),
            record: None,
            probe: Probe::default(),
        };
        let dst = address_to_endpoint(server.0, server.1, PORT_LEADER);
        let sys = s.drdp.system_endpoint;
        let c = s.drdp.connection_open(sys, dst)?;
        s.drdp.connection_set_failure(c, 1000, 30000, 60000);
        s.drdp.connection_set_simulated(c, 0, 0, 0);
        s.conn = Some(c);
        s.send_hello();
        let mut w = Writer::op(0x0904);
        w.u8(0).u32(EXPANSION_FLAGS);
        s.send_login(w);
        Some(s)
    }

    fn send_hello(&mut self) {
        let mut w = Writer::op(0x0000);
        w.u32(VERSION);
        self.send(&w.data);
    }

    /// The login tail 0x0904 and 0x0001 share: "EQOA", the name, and the
    /// token in place of the password.
    fn send_login(&mut self, mut w: Writer) {
        let user = self.user.clone();
        w.string(b"EQOA").string(user.as_bytes()).u8(1).bytes(&self.token);
        self.send(&w.data);
    }

    /// `SendRaw`: a guaranteed message on the current connection.
    pub fn send(&mut self, data: &[u8]) {
        if let Some(c) = self.game_conn() {
            if let Some(r) = self.record.as_mut() {
                r.push((drdp::TYPE_GUARANTEED, data.to_vec()));
            }
            self.drdp.connection_send(c, data, drdp::TYPE_GUARANTEED);
        }
    }

    /// A state channel update (0x40 is the player's own state).
    pub fn send_state(&mut self, channel: u8, data: &[u8]) {
        if let Some(c) = self.game_conn() {
            if let Some(r) = self.record.as_mut() {
                r.push((channel, data.to_vec()));
            }
            self.drdp.connection_send(c, data, channel);
        }
    }

    /// The game connection, while the DRDP library still has it. The library
    /// can let a connection go with no DISCONNECT reaching the session (system
    /// messages 20 and 21 close every connection to an endpoint,
    /// `drdp_close_connections_to_endpoint`), and its handle then names a freed
    /// slot: a send on it panicked ("dangling handle"), or once the slot was
    /// taken again went on another connection (petition 1355's crashes are
    /// unexplained; this one is prevented). The handle is let go, its loss said
    /// once, and every message after it counted (`sends_lost`) and not sent,
    /// as `msg_07e0` and `msg_0790_done` already skip a connection gone.
    pub fn game_conn(&mut self) -> Option<ConnId> {
        match self.conn {
            Some(c) if self.drdp.conns.contains(c) => Some(c),
            Some(_) => {
                self.conn = None;
                self.conn_gone = true;
                self.sends_lost += 1;
                debug_line("[net] the game connection is gone: DRDP let it go with no disconnect (as its system messages 20 and 21 do); nothing more is sent on it");
                None
            }
            None => {
                if self.conn_gone {
                    self.sends_lost += 1;
                }
                None
            }
        }
    }

    /// `SendOpWStringInt(11, text, target)`: a chat line (wide) and the
    /// target (-1 for none), on the channel in use.
    pub fn say(&mut self, text: &str, target: u32) {
        let mut w = Writer::op(0x000B);
        w.wide(text).u32(target);
        self.send(&w.data);
    }

    /// `SendMsg(op, v, b)`: a u32 and a byte (0x0035 answers a dialog).
    pub fn send_msg(&mut self, op: u16, v: u32, b: u8) {
        let mut w = Writer::op(op);
        w.u32(v).u8(b);
        self.send(&w.data);
    }

    /// `Msg00B2`'s `SendOpSigned(73, token)`: a move of the server's is done
    /// (`Respawn` has stood the player at the arrival). The PS2's frame stops
    /// in `Respawn` while it loads, so this goes in the same packet as its
    /// first 0x40 from there; Sandstorm resumes the state channels on it.
    pub fn answer_teleport(&mut self, token: u32) {
        let mut w = Writer::op(0x0049);
        w.signed(token as i32 as i64);
        self.send(&w.data);
    }

    /// `SendOpcode(op)`: the opcode alone.
    pub fn send_opcode(&mut self, op: u16) {
        let w = Writer::op(op);
        self.send(&w.data);
    }

    /// `SendOpInt(op, v)`.
    pub fn send_op_int(&mut self, op: u16, v: u32) {
        let mut w = Writer::op(op);
        w.u32(v);
        self.send(&w.data);
    }

    /// A GM's travel to (`x`, `z`) of world `world` (the map's Ctrl+click), asked
    /// of a server that offered it ([`OP_MAP_TRAVEL`]); false, and nothing sent,
    /// otherwise (a player's map, Sandstorm's, eqoa.live's).
    pub fn map_travel_to(&mut self, world: u32, x: f32, z: f32) -> bool {
        if !self.map_travel {
            return false;
        }
        let mut w = Writer::op(OP_MAP_TRAVEL_TO);
        w.u32(world).u32(x.to_bits()).u32(z.to_bits());
        self.send(&w.data);
        true
    }

    /// The combat record asked for now (its window opening), of a server that
    /// offered it ([`OP_COMBAT_RECORD`]); false, and nothing sent, otherwise.
    pub fn ask_combat_record(&mut self) -> bool {
        if self.combat_record {
            self.send_op_int(OP_COMBAT_RECORD_DATA, 1);
        }
        self.combat_record
    }

    /// The Raid List: a server that offered the whole of a raid group
    /// ([`OP_RAID_ROSTER_OFFER`]) is told whether to send it (`want`), at once
    /// and whenever that changes; one that made no offer is told nothing.
    pub fn agree_raid_roster(&mut self, want: bool) {
        if !self.raid_roster_offer || self.raid_roster_asked == Some(want) {
            return;
        }
        self.raid_roster_asked = Some(want);
        // 2 asks for the members' groups too, where the offer said they come;
        // 3 their kinds as well (a raid's mercs told apart)
        let answer = match (want, self.raid_roster_kinds, self.raid_roster_groups) {
            (true, true, _) => 3,
            (true, false, true) => 2,
            _ => want as u32,
        };
        self.send_op_int(OP_RAID_ROSTER_ANSWER, answer);
    }

    /// Power Bars: a server that offered others' power ([`OP_POWER_BARS`])
    /// is told whether to send it (`want`), at once and whenever that
    /// changes; one that made no offer is told nothing. Off, the set it sent
    /// is forgotten.
    pub fn agree_power_bars(&mut self, want: bool) {
        if !want {
            self.power_bars.clear();
        }
        if !self.power_bars_offer || self.power_bars_asked == Some(want) {
            return;
        }
        self.power_bars_asked = Some(want);
        self.send_op_int(OP_POWER_BARS, want as u32);
    }

    /// The power of 255 the server last sent for entity `id`
    /// ([`OP_POWER_BARS_DATA`]), if it sent one.
    pub fn power_of(&self, id: u32) -> Option<u8> {
        self.power_bars.iter().find(|&&(i, _)| i == id).map(|&(_, p)| p)
    }

    /// [`OP_BUG_NOTE`]: the note for the petition this client files next,
    /// cut to [`BUG_NOTE_MAX`] bytes on a character's edge.
    pub fn send_bug_note(&mut self, note: &str) {
        let mut end = note.len().min(BUG_NOTE_MAX);
        while !note.is_char_boundary(end) {
            end -= 1;
        }
        let b = &note.as_bytes()[..end];
        let mut w = Writer::op(OP_BUG_NOTE);
        w.u32(1).u32(b.len() as u32).bytes(b);
        self.send(&w.data);
    }

    /// [`OP_CRASH_REPORT`]: a crash report kept (`number`, its text), cut to
    /// `most` bytes (and [`CRASH_REPORT_MOST`]) on a character's edge.
    pub fn send_crash_report(&mut self, number: u32, text: &str, most: usize) {
        let mut end = text.len().min(most).min(CRASH_REPORT_MOST);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let b = &text.as_bytes()[..end];
        let mut w = Writer::op(OP_CRASH_REPORT);
        w.u32(1).u32(number).u32(b.len() as u32).bytes(b);
        self.send(&w.data);
    }

    /// [`OP_ENTITY_VIEW`] to the server: how many entity slots this client
    /// keeps and how far it draws them (a server that offered them fills that
    /// many from that far).
    pub fn send_entity_view(&mut self, slots: u32, view: u32) {
        let mut w = Writer::op(OP_ENTITY_VIEW);
        w.u32(slots).u32(view);
        self.send(&w.data);
    }

    /// Quest Markers (the Quest Helper's): a server that offered quest marks
    /// ([`OP_QUEST_MARKERS`]) is told to send them (`want`) at once, and to
    /// stop when that changes; one that made no offer is told nothing, nor is
    /// one never asked for them (the Quest Helper off from the start: as a
    /// client without it, it answers nothing). Off, the set it sent is
    /// forgotten.
    pub fn agree_quest_marks(&mut self, want: bool) {
        if !want {
            self.quest_marks.clear();
        }
        if !self.quest_marks_offer || self.quest_marks_asked.unwrap_or(false) == want {
            return;
        }
        self.quest_marks_asked = Some(want);
        let mut w = Writer::op(OP_QUEST_MARKERS);
        w.u32(want as u32);
        self.send(&w.data);
    }

    /// Quest Tracker: a server that offered the tracker
    /// ([`OP_QUEST_PROGRESS`]) is told whether to send it (`want`), as
    /// `agree_quest_marks` tells it of the marks. Off, the tracker it sent is
    /// forgotten.
    pub fn agree_quest_progress(&mut self, want: bool) {
        if !want {
            self.quest_progress.clear();
        }
        if !self.quest_progress_offer || self.quest_progress_asked.unwrap_or(false) == want {
            return;
        }
        self.quest_progress_asked = Some(want);
        let mut w = Writer::op(OP_QUEST_PROGRESS);
        w.u32(want as u32);
        self.send(&w.data);
    }

    /// Pet Effects: a server that offered the pet's effects
    /// ([`OP_PET_EFFECTS`]) is told whether to send them (`want`), at once and
    /// whenever that changes, and afresh at another offer (each world entry's
    /// player is a new one on the server); one that made no offer is told
    /// nothing. Off, the set it sent is forgotten.
    pub fn agree_pet_effects(&mut self, want: bool) {
        if !want {
            self.pet_effects.clear();
        }
        if !self.pet_effects_offer || self.pet_effects_asked == Some(want) {
            return;
        }
        self.pet_effects_asked = Some(want);
        self.send_op_int(OP_PET_EFFECTS, want as u32);
    }

    /// Number Colors: a server that offered the number tags
    /// ([`OP_NUMBER_TAGS_OFFER`]) is told whether to send them (`want`: a row
    /// not Default), at once and whenever that changes, and afresh at another
    /// offer; one that made no offer is told nothing. Wanted, the answer is
    /// the layout read: [`NUMBER_TAGS_LAYOUT`], or the offer's when that is
    /// older (1 at least).
    pub fn agree_number_tags(&mut self, want: bool) {
        if !self.number_tags_offer || self.number_tags_asked == Some(want) {
            return;
        }
        self.number_tags_asked = Some(want);
        let mut w = Writer::op(OP_NUMBER_TAGS_ANSWER);
        w.u8(if want { self.number_tags_layout.clamp(1, NUMBER_TAGS_LAYOUT) } else { 0 });
        self.send(&w.data);
    }

    /// The DPS Meter: a server that offered a damage meter
    /// ([`OP_DPS_METER_OFFER`]) is told whether to send it (`want`), at once and
    /// whenever that changes, and afresh at another offer; one that made no offer
    /// is told nothing.
    pub fn agree_dps_meter(&mut self, want: bool) {
        if !self.dps_meter_offer || self.dps_meter_asked == Some(want) {
            return;
        }
        self.dps_meter_asked = Some(want);
        let mut w = Writer::op(OP_DPS_METER_ANSWER);
        w.u8(want as u8);
        self.send(&w.data);
    }

    /// The raid's windows: a server that offered them ([`OP_RAID_OFFER`]) is
    /// told whether to send them (`want`), at once and whenever that changes,
    /// and afresh at another offer; one that made no offer is told nothing.
    pub fn agree_raid(&mut self, want: bool) {
        if !self.raid_offer || self.raid_asked == Some(want) {
            return;
        }
        self.raid_asked = Some(want);
        let mut w = Writer::op(OP_RAID_ANSWER);
        w.u8(want as u8);
        self.send(&w.data);
    }

    /// The ready check's window: a server that offered it
    /// ([`crate::readycheck::OP_READY_OFFER`]) is told whether to send checks
    /// as windows (`want`), at once and afresh at another offer; one that
    /// made no offer is told nothing.
    pub fn agree_ready(&mut self, want: bool) {
        if !self.ready_offer || self.ready_asked == Some(want) {
            return;
        }
        self.ready_asked = Some(want);
        self.send(&crate::readycheck::answer(want));
    }

    /// The buff window: a server that offered it
    /// ([`crate::buffs::OP_BUFF_WINDOW`]) is told whether to send its set
    /// (`want`), at once and whenever that changes, and afresh at another offer;
    /// one that made no offer is told nothing. Off, the set it sent is forgotten.
    pub fn agree_buffs(&mut self, want: bool) {
        if !want {
            self.buffs.clear();
        }
        if !self.buffs_offer || self.buffs_asked == Some(want) {
            return;
        }
        self.buffs_asked = Some(want);
        self.send(&crate::buffs::answer(want));
    }

    /// The quest marks over the NPCs' heads: a server that offered them
    /// ([`crate::headmarks::OP_OFFER`]) is told whether to send them (`want`),
    /// at once and whenever that changes, and afresh at another offer; one
    /// that made no offer is told nothing. Off, the marks it sent are
    /// forgotten.
    pub fn agree_head_marks(&mut self, want: bool) {
        if !want {
            self.head_marks.clear();
        }
        if !self.head_marks_offer || self.head_marks_asked == Some(want) {
            return;
        }
        self.head_marks_asked = Some(want);
        self.send(&crate::headmarks::answer(want));
    }

    /// The mark over the entity `id`'s head, from a server that sends them
    /// (offered and asked): [`crate::headmarks::TAKE`], [`crate::headmarks::STEP`],
    /// or None.
    pub fn head_mark(&self, id: u32) -> Option<u8> {
        if !self.head_marks_offer || self.head_marks_asked != Some(true) {
            return None;
        }
        self.head_marks.get(&id).copied()
    }

    /// Whether the server sends the buff window's set (offered and asked).
    pub fn buffs_on(&self) -> bool {
        self.buffs_offer && self.buffs_asked == Some(true)
    }

    /// A line of the set asked to be taken off
    /// ([`crate::buffs::OP_BUFF_REMOVE`]); false, and nothing sent, to a server
    /// that made no offer.
    pub fn send_buff_remove(&mut self, l: &crate::buffs::Line) -> bool {
        if !self.buffs_offer {
            return false;
        }
        self.send(&crate::buffs::remove(l));
        true
    }

    /// The set's lines not over at `now` (`now`'s clock), each with what is
    /// left of it then (a line of no end kept as it is).
    pub fn buffs_left(&self, now: u32) -> Vec<crate::buffs::Line> {
        let gone = now.wrapping_sub(self.buffs_at);
        let gone = if (gone as i32) < 0 { 0 } else { gone };
        self.buffs
            .iter()
            .filter(|l| l.left_ms == crate::buffs::NO_END || l.left_ms > gone)
            .map(|l| crate::buffs::Line { left_ms: if l.left_ms == crate::buffs::NO_END { l.left_ms } else { l.left_ms - gone }, ..l.clone() })
            .collect()
    }

    /// A ready check's answer ([`crate::readycheck::OP_READY_REPLY`]) to a
    /// server that offered the window; false, and nothing sent, to any other.
    pub fn send_ready_reply(&mut self, id: u32, ready: bool) -> bool {
        if !self.ready_offer {
            return false;
        }
        self.send(&crate::readycheck::reply(id, ready));
        true
    }

    /// Item Links: a server that offered item links ([`OP_ITEM_LINKS`]) is told
    /// whether this client takes them (`want`, the Extended row), at once and
    /// whenever that changes, and afresh at another offer; one that made no
    /// offer is told nothing.
    pub fn agree_item_links(&mut self, want: bool) {
        if !self.item_links_offer || self.item_links_asked == Some(want) {
            return;
        }
        self.item_links_asked = Some(want);
        let mut w = Writer::op(OP_ITEM_LINKS);
        w.u8(want as u8);
        self.send(&w.data);
    }

    /// Profanity Filter (Options > Interface): a server that offered to take it
    /// ([`OP_PROFANITY_CHOICE`]) is told it (`tier`: [`PROFANITY_OFF`],
    /// [`PROFANITY_HIGH`] or [`PROFANITY_MEDIUM`]), at once and whenever it
    /// changes, and afresh at another offer; one that made no offer is told
    /// nothing (its filter stays as the game's 0x0045 leaves it). A server
    /// from before the tiers reads any nonzero as on: Medium is High there.
    pub fn agree_profanity_filter(&mut self, tier: u8) {
        if !self.profanity_offer || self.profanity_asked == Some(tier) {
            return;
        }
        self.profanity_asked = Some(tier);
        let mut w = Writer::op(OP_PROFANITY_CHOICE);
        w.u8(tier);
        self.send(&w.data);
    }

    /// Hide Look and Show Look on a worn item (`crate::hidelook`): the look of
    /// what is worn in `equip_slot` hidden or shown, told only to a server that
    /// offered it; the server answers the slots after it.
    pub fn hide_look(&mut self, equip_slot: i32, hide: bool) {
        if self.hide_look.offered && crate::hidelook::shown(equip_slot) {
            self.send(&crate::hidelook::set(equip_slot, hide));
        }
    }

    /// Links are in force on this connection: the server offered them and was
    /// told this client takes them. Only then does a typed line carry links
    /// ([`say_linked`](Self::say_linked)) or a link open.
    pub fn item_links(&self) -> bool {
        self.item_links_offer && self.item_links_asked == Some(true)
    }

    /// A typed line with links ([`OP_ITEM_LINK_SAY`]) in place of its retail
    /// message, where links are in force; false, and nothing sent, elsewhere
    /// (the caller sends the plain line).
    pub fn say_linked(&mut self, route: crate::itemlink::Route, text: &str, target: u32, links: &[crate::itemlink::Linked]) -> bool {
        if !self.item_links() || links.is_empty() {
            return false;
        }
        let m = crate::itemlink::say(route, text, target, links);
        self.send(&m);
        true
    }

    /// A link opened ([`OP_ITEM_LINK_ASK`]), where links are in force.
    pub fn ask_item_link(&mut self, id: u32) -> bool {
        if !self.item_links() {
            return false;
        }
        let m = crate::itemlink::ask(id);
        self.send(&m);
        true
    }

    /// The meter's reset: a server that offered a meter and was asked for it
    /// drops the fight under way ([`OP_DPS_METER_RESET`]); false (nothing sent)
    /// otherwise.
    pub fn reset_dps_meter(&mut self) -> bool {
        if !self.dps_meter_offer || self.dps_meter_asked != Some(true) {
            return false;
        }
        let mut w = Writer::op(OP_DPS_METER_RESET);
        w.u8(1);
        self.send(&w.data);
        true
    }

    /// Whether the server is sending the raid's windows: it offered them and
    /// was told yes.
    pub fn raid_windows(&self) -> bool {
        self.raid_offer && self.raid_asked == Some(true)
    }

    /// A roll's answer ([`OP_RAID_ROLL_CHOICE`]) to a server that offered the
    /// raid's windows; false, and nothing sent, to any other.
    pub fn send_roll_choice(&mut self, id: u32, c: crate::raid::Choice) -> bool {
        if !self.raid_offer {
            return false;
        }
        self.send(&crate::raid::roll_choice(id, c));
        true
    }

    /// A raid command ([`OP_RAID_COMMAND`]) to a server that offered the
    /// raid's windows; false, and nothing sent, to any other.
    pub fn send_raid_command(&mut self, c: crate::raid::Command) -> bool {
        if !self.raid_offer {
            return false;
        }
        self.send(&c.bytes());
        true
    }

    /// The pet's effects the server last sent that are not over at `now`
    /// (`now`'s clock): each its icon and the ms left of it, in their order.
    pub fn pet_effects_left(&self, now: u32) -> Vec<(u32, u32)> {
        let gone = now.wrapping_sub(self.pet_effects_at);
        // a clock read before the set came (none: it is read after) counts as no time gone
        let gone = if (gone as i32) < 0 { 0 } else { gone };
        self.pet_effects.iter().filter(|e| e.left_ms > gone).map(|e| (e.icon, e.left_ms - gone)).collect()
    }

    pub fn send_op_byte(&mut self, op: u16, v: u8) {
        if op == 0x000E {
            self.chat_mode = v;
        }
        let mut w = Writer::op(op);
        w.u8(v);
        self.send(&w.data);
    }

    /// A sub-op byte and a wide string: `VIClientPart2EF24::Say` (0x0578,
    /// 12: the guild) and `VIClientPart2EFCC::SendText` (0x0C01, 2: a shout).
    pub fn send_op_sub_wide(&mut self, op: u16, sub: u8, text: &str) {
        let mut w = Writer::op(op);
        w.u8(sub).wide(text);
        self.send(&w.data);
    }

    /// `ConnectShard`: the game endpoint, the hello and 0x0001. `hello` is
    /// what follows the hello's opcode when a server's patch makes it more
    /// than retail's DiscVersion (the live S2's: its version, its numbers
    /// and its device block); `None` is retail's.
    pub fn connect_shard(&mut self, endpoint: u64, hello: Option<&[u8]>) {
        self.state = State::Shard;
        // a shard is a server of its own: its lists' sizes are what it says
        self.item_lists = [ITEM_LIST_DISC; 2];
        if let Some(c) = self.conn.take()
            && self.drdp.conns.contains(c)
        {
            self.drdp.connection_close(c);
        }
        let sys = self.drdp.system_endpoint;
        self.conn = self.drdp.connection_open(sys, endpoint);
        self.conn_gone = false;
        match hello {
            Some(h) => {
                let mut w = Writer::op(0x0000);
                w.bytes(h);
                self.send(&w.data);
            }
            None => self.send_hello(),
        }
        let mut w = Writer::op(0x0001);
        w.u8(0).u32(EXPANSION_FLAGS);
        self.send_login(w);
    }

    /// `SendNewChar(43, c)`: the creation form as 0x002B. `m_p18` is 1, as
    /// `VIWndCharCreateBlank::Reset` leaves it; the stats go raw.
    pub fn create_character(&mut self, c: &NewChar) {
        self.send(&new_char_message(c));
    }

    /// `DeleteCharacter`: `SendOpSigned(45, serverId)`.
    pub fn delete_character(&mut self, server_id: i64) {
        let mut w = Writer::op(0x002D);
        w.signed(server_id);
        self.send(&w.data);
    }

    /// `Send0012(race, sex)`: the name page asks the server for a name.
    pub fn random_name(&mut self, race: i32, sex: i32) {
        let mut w = Writer::op(0x0012);
        w.signed(race as i64).signed(sex as i64);
        self.send(&w.data);
    }

    /// `SelectCharacter`: 0x002A with the server id and the face and hair.
    pub fn select_character(&mut self, c: &Character) {
        self.state = State::Selecting;
        self.picked = true;
        let mut w = Writer::op(0x002A);
        for v in [c.server_id, c.face, c.hair_style, c.hair_length, c.hair_color] {
            w.u32(v as u32);
        }
        if self.race_options.is_some()
            && let Some(b) = crate::raceopts::choices_block(&c.options)
        {
            w.bytes(&b);
        }
        self.send(&w.data);
    }

    /// The DRDP clock: milliseconds, from the wall clock at connect.
    /// The datagram and message trace `EQOA_NET_DEBUG` turns on.
    pub fn set_debug(&mut self, on: bool) {
        self.debug = on;
    }

    pub fn now(&self) -> u32 {
        self.base.wrapping_add(self.start.elapsed().as_millis() as u32)
    }

    /// The local clock `VIClient` keeps (`m_p07568`), in VITimer's ticks,
    /// 983.04 a second (BUSCLK/16 with a compare of 18,750, plus 2), from
    /// the session's start: what the synced clocks are set and read against.
    pub fn timer(&self) -> i64 {
        ticks(self.start.elapsed().as_micros() as i64)
    }

    /// `VIClient::UpdateTimeOfDay`: the hour, clock B (0x0013's second
    /// reading, blended forward on the local timer) modulo 3,600,000 of its
    /// units as 24 hours, as `VIScene::SetTimeOfDay` clamps it; none until
    /// the server set the clock.
    pub fn time_of_day(&self) -> Option<vi::fpu::F<Ee>> {
        hour_of(&self.clock_b, self.timer())
    }


    /// One frame: the clock, every datagram waiting, the arrived messages
    /// (as events), a flush, and the sends.
    pub fn pump(&mut self) -> Vec<Event> {
        let t = self.now();
        self.drdp.set_time(t);
        while let Some((buf, ip, port)) = self.socket.recv_from() {
            self.datagrams_in += 1;
            self.last_datagram = Some(Instant::now());
            let n = buf.len();
            let mut d = ip.to_vec();
            d.extend(port.to_le_bytes());
            d.extend(&buf);
            let r = self.drdp.process_packet(&d);
            if self.debug {
                let mut b = drdp::buffer::Buffer::reader(&buf[..n.saturating_sub(4)]);
                let (from, to) = (b.u16(), b.u16());
                let mut segs = Vec::new();
                while b.remaining() > 0 && !b.overflow {
                    let h = drdp::read_segment_header(&mut b, 0, 0);
                    let body = b.rest()[..(h.size as usize).min(b.rest().len()).min(16)].to_vec();
                    debug_line(&format!("[net]    body head {body:02x?}"));
                    b.skip(h.size as usize);
                    segs.push(format!("flags {:#x} size {} crc {:#x} src {:#x} dst {:#x}", h.flags, h.size, h.crc, h.src, h.dst));
                }
                debug_line(&format!("[net] <- {n} bytes, drdp ports {from:#x}->{to:#x}: {r}; {segs:?}"));
                let sys = self.drdp.system_endpoint;
                for (&dst, &c) in &self.drdp.eps[sys].connections {
                    let k = &self.drdp.conns[c];
                    debug_line(&format!("[net]    conn {dst:#x}: flags {:#x} flush {} arrival {} rx_seq {} tx_seq {} tx_ack {} inst {:#x}/{:#x} cstat {:?}", k.flags, k.flush, k.arrival_number, k.rx_seqnum, k.tx_seqnum, k.tx_ack, k.instance_local, k.instance_remote, k.cstat));
                }
            }
        }
        let mut events = std::mem::take(&mut self.held);
        while let Some(m) = self.drdp.receive() {
            if self.debug {
                debug_line(&format!("[net] message type {} size {} src {:#x}", m.ty, m.data.len(), m.src));
            }
            self.route(m, &mut events);
        }
        self.worlds_answer();
        self.flush();
        events
    }

    /// The server told which of its worlds this client holds, once they are
    /// built (`OP_SERVER_WORLDS_DATA`): the builder's answer, or none when it
    /// stopped without one. One that goes after a character was played and
    /// before the game connection opened (`picked`) is told again on that
    /// connection: the server promotes the session when it takes the pick and
    /// leaves the lobby connection's later packets alone, so an answer a
    /// moment behind the pick never reached it and the session held none of
    /// the server's worlds for the visit (no arena, no GM's move there; seen
    /// in a page whose kept worlds were checked 60 ms after its pick). Told
    /// twice is the same set twice.
    fn worlds_answer(&mut self) {
        let Some(rx) = &self.worlds_building else { return };
        let ids = match rx.try_recv() {
            Ok(ids) => ids,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
        };
        self.worlds_building = None;
        self.worlds_pending = false;
        self.tell_worlds(&ids);
        if self.picked {
            self.worlds_retell = Some(ids);
        }
    }

    /// `OP_SERVER_WORLDS_DATA` to the server: the worlds this client holds.
    fn tell_worlds(&mut self, ids: &[u8]) {
        let mut w = Writer::op(OP_SERVER_WORLDS_DATA);
        w.bytes(&crate::serverworlds::encode_ready(ids));
        self.send(&w.data);
    }

    /// Whether the server's worlds were offered and taken and it is not yet
    /// told which this client holds: a character picked now would be placed
    /// by a server that does not know yet (`eqoa play` waits).
    pub fn server_worlds_pending(&self) -> bool {
        self.worlds_pending
    }

    /// Whether a login of the character `server_id` needs the server's
    /// worlds, as the server last said ([`OP_SERVER_WORLDS_CHARS`]): it
    /// starts in one of them for a client that holds it, and in its mainland
    /// city, for good, for one that does not, so its pick waits while they
    /// are pending. `Some(false)`: the server placed it the same either way.
    /// `None`: the server never said (an older one, or its word not yet in).
    pub fn pick_needs_worlds(&self, server_id: i64) -> Option<bool> {
        self.worlds_chars.as_ref().map(|c| u32::try_from(server_id).is_ok_and(|id| c.contains(&id)))
    }

    /// The flush `pump` ends with, and the sends: what was sent goes out now
    /// (Quit's logout, before the client leaves).
    pub fn flush(&mut self) {
        // TickNet and Process: Msg0790Done, then drdp_flush
        self.msg_0790_done();
        self.drdp.flush();
        for p in std::mem::take(&mut self.drdp.outbox) {
            if self.debug {
                let mut b = drdp::buffer::Buffer::reader(&p.data[..p.data.len().saturating_sub(4)]);
                let (from, to) = (b.u16(), b.u16());
                let mut segs = Vec::new();
                while b.remaining() > 0 && !b.overflow {
                    let h = drdp::read_segment_header(&mut b, 0, 0);
                    let body = b.rest()[..(h.size as usize).min(b.rest().len()).min(12)].to_vec();
                    b.skip(h.size as usize);
                    segs.push(format!("flags {:#x} dst {:#x} body {body:02x?}", h.flags, h.dst));
                }
                debug_line(&format!("[net] -> {} bytes, drdp ports {from:#x}->{to:#x}: {segs:?}", p.data.len()));
            }
            self.socket.send_to(&p.data, p.to.ip, p.to.port);
            self.packets_sent += 1;
        }
    }

    /// `PumpMessages`' routing by DRDP message type.
    fn route(&mut self, m: drdp::Message, ev: &mut Vec<Event>) {
        match m.ty {
            drdp::TYPE_GUARANTEED => self.dispatch(&m, ev),
            drdp::TYPE_BEST_EFFORT => {
                let mut r = Reader::new(&m.data);
                if r.u16() == 0x07B3 && matches!(self.state, State::Login | State::Shards) {
                    ev.push(Event::ShardList(read_shards(&mut r)));
                    self.state = State::Shards;
                }
            }
            drdp::TYPE_SYSTEM => {}
            drdp::TYPE_CONNECT => {
                // The server opened the game connection: the old one goes,
                // the world resets, the new one is ours.
                if let Some(c) = self.conn.take()
                    && Some(c) != m.connection
                    && self.drdp.conns.contains(c)
                {
                    self.drdp.connection_close(c);
                }
                self.conn = m.connection;
                self.conn_gone = false;
                self.self_id = None;
                // the server's worlds, taken and their list never in whole:
                // the character went in before it (one the worlds were not
                // waited for), and what the server still had to send of it
                // went with the old connection. Asked for again here, so
                // they are built while the game is played and the server can
                // send the player into them (its arena, a GM's move).
                if self.worlds_pending && self.worlds_building.is_none() {
                    self.send_op_int(OP_SERVER_WORLDS, 1);
                }
                // and the worlds held, told while the pick was on its way:
                // told again, on the connection the server now reads
                self.picked = false;
                if let Some(ids) = self.worlds_retell.take() {
                    self.tell_worlds(&ids);
                }
                self.entity_view_offer = None;
                self.flying = false;
                self.jumps = false;
                self.map_travel = false;
                // the NPCs' body tints too (the next world entry offers them)
                self.npc_tints = Some(Vec::new());
                // the mercs and the group search's standing (offered again)
                self.mercs = None;
                self.group_reg = None;
                // the old connection's channels go with it
                self.early_states.clear();
                self.quest_marks_offer = false;
                self.quest_marks_asked = None;
                // others' power: offered again at the next world entry
                self.power_bars_offer = false;
                self.power_bars_asked = None;
                self.power_bars.clear();
                self.quest_marks.clear();
                self.quest_progress_offer = false;
                self.quest_progress_asked = None;
                self.quest_progress.clear();
                self.pet_effects_offer = false;
                self.pet_effects_asked = None;
                self.pet_effects.clear();
                self.buffs_offer = false;
                self.buffs_asked = None;
                self.buffs.clear();
                self.head_marks_offer = false;
                self.head_marks_asked = None;
                self.head_marks.clear();
                self.number_tags_offer = false;
                self.number_tags_asked = None;
                self.dps_meter_offer = false;
                self.dps_meter_asked = None;
                self.item_links_offer = false;
                self.item_links_asked = None;
                self.profanity_offer = false;
                self.profanity_asked = None;
                self.hide_look = crate::hidelook::State::default();
                ev.push(Event::Promoted);
            }
            drdp::TYPE_DISCONNECT => {
                if m.connection == self.conn {
                    self.conn = None;
                }
                // m_4F0EC = data[0]; reason 14 -> 16, 1 -> 1, 15 -> 15, else 10
                let reason = match m.data.first().copied().unwrap_or(0) {
                    14 => 16,
                    1 => 1,
                    15 => 15,
                    _ => 10,
                };
                ev.push(Event::Disconnected(reason));
            }
            ty if ty < drdp::TYPE_STATE_CHANNELS => {
                let e = Event::State { channel: ty, data: m.data };
                // before the world entry: kept for it (`early_states`)
                if matches!(self.state, State::World) {
                    ev.push(e);
                } else {
                    if self.debug {
                        debug_line(&format!("[net] state {ty:#04x} before the world entry: kept for it"));
                    }
                    self.early_states.push(e);
                }
            }
            // ProcessMessageF8: a datagram outside any connection (drdp
            // port 0xffff) comes as type 0xF8; its 0x0993 answers the probe
            0xF8 => self.process_f8(&m),
            _ => {}
        }
    }

    /// `ProcessMessageF8` and `Msg0993`: the probe's token coming back is
    /// answered with 0x0791 and the token, on the game connection.
    fn process_f8(&mut self, m: &drdp::Message) {
        let mut r = Reader::new(&m.data);
        if r.u16() == 0x0993 {
            let token = r.u32();
            if token == self.probe.token {
                let mut w = Writer::op(0x0791);
                w.u32(token);
                self.send(&w.data);
            }
        }
    }

    /// `Msg0790Done`: with a probe set and a game connection, 0x0992 (the
    /// token, the eight bytes) to the probe's endpoint outside any
    /// connection (`VINetSendTo`: `drdp_send_addr` to the endpoint's
    /// address, its drdp port unused), when the connection's timeout (a
    /// second at most) has gone by since the last, on the game's clock.
    fn msg_0790_done(&mut self) {
        let Some(c) = self.conn.filter(|&c| self.drdp.conns.contains(c)) else { return };
        let timeout = (self.drdp.conns[c].timeout as i64).min(1000);
        let now = self.timer();
        if self.probe.endpoint != -1 && now >= self.probe.last + timeout {
            let mut w = Writer::op(0x0992);
            w.u32(self.probe.token).u64(self.probe.data);
            let (to, _) = drdp::endpoint_to_address(self.probe.endpoint as u64);
            self.drdp.send_addr(&[to], &w.data);
            self.probe.last = now;
        }
    }

    /// `Msg07E0`: 0x07E1, the game connection's eleven counters as text
    /// (`VINetStats`: "%I: %s\n" each, the count as a signed 64-bit
    /// number), then whatever followed 0x07E0, on the game connection. Both
    /// Sandstorm and go-eqoa-server send 0x07E0 once a session, and a PS2
    /// answers (`pcaps/ps2_sandstorm.pcap`).
    fn msg_07e0(&mut self, rest: &[u8]) {
        let Some(c) = self.conn.filter(|&c| self.drdp.conns.contains(c)) else { return };
        let cstat = self.drdp.conns[c].cstat;
        let text: String = cstat.iter().zip(drdp::CSTAT_LABELS).map(|(&n, label)| format!("{}: {label}\n", n as i64)).collect();
        let mut w = Writer::op(0x07E1);
        w.wide(&text).bytes(rest);
        self.send(&w.data);
    }

    /// `ProcessMessage`: one opcode.
    fn dispatch(&mut self, m: &drdp::Message, ev: &mut Vec<Event>) {
        let mut r = Reader::new(&m.data);
        let op = r.u16();
        match op {
            0x0000 => ev.push(Event::Version(r.u32())),
            0x07E0 => self.msg_07e0(&m.data[2.min(m.data.len())..]),
            0x0790 => {
                // Msg0790: the probe, sent at once if due
                let endpoint = r.u64() as i64;
                let token = r.u32();
                let data = r.u64();
                self.probe = Probe { endpoint, token, data, last: -1 };
                self.msg_0790_done();
            }
            0x07D1 => {
                self.expansion = r.u32();
                ev.push(Event::Expansion(self.expansion));
            }
            OP_ENTITY_VIEW => {
                let slots = r.u32();
                let view = r.u32();
                if !r.error {
                    self.entity_view_offer = Some((slots, view));
                }
            }
            // the trigger names offered: taken, every trigger (the TEST menu
            // lists them), and this GM's client says it flies
            OP_GM_TRIGGERS => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_GM_TRIGGERS, TRIGGERS_ALL);
                    self.send_op_int(OP_FLY, 1);
                }
            }
            // a GM's travel by the map offered (the map's Ctrl+click), or no more
            OP_MAP_TRAVEL => {
                let on = r.u32() != 0;
                if !r.error {
                    self.map_travel = on;
                }
            }
            // flight on or off (`Play::control_client`, `walker`)
            OP_FLY => {
                let on = r.u32() != 0;
                if !r.error {
                    self.flying = on;
                }
            }
            // the jump offered: taken (the jump key, `walker`); a 0 ends it
            OP_JUMP => {
                let on = r.u32() != 0;
                if !r.error {
                    if on && !self.jumps {
                        self.send_op_int(OP_JUMP, 1);
                    }
                    self.jumps = on;
                }
            }
            // the combat record offered: taken (`record`, Character > Combat
            // Record)
            OP_COMBAT_RECORD => {
                if r.u32() != 0 && !r.error {
                    self.combat_record = true;
                    self.send_op_int(OP_COMBAT_RECORD, 1);
                }
            }
            // the live duel's messages offered: taken (the client has the
            // window, `duel`)
            OP_DUEL_WINDOW => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_DUEL_WINDOW, 1);
                }
            }
            // the live game's look offered: taken (this client reads it),
            // unless a test's `EQOA_LIVE_LOOK=0` keeps the disc's; and
            // every character listed (character select scrolls them)
            OP_LIVE_LOOK => {
                if r.u32() & 1 != 0 && !r.error {
                    // the server's armour with the live look alone (its trims ride the live channel)
                    let live = if wants_live_look() { 1 | LIVE_LOOK_ARMOR } else { 0 };
                    self.send_op_int(OP_LIVE_LOOK, live | LIVE_LOOK_MANY_CHARS);
                }
            }
            // bug reports offered: taken (F3 and the note, `bugnote`), with the
            // most of a petition's text the server takes when it says
            OP_BUG_REPORTS => {
                if r.u32() != 0 && !r.error {
                    self.bug_reports = true;
                    if m.data.len() >= r.pos + 4 {
                        let most = r.u32() as usize;
                        if most > 0 {
                            self.petition_text = most.min(PETITION_TEXT_MOST);
                        }
                    }
                    self.send_op_int(OP_BUG_REPORTS, 1);
                }
            }
            // crash reports taken: the game sends what it kept (`crashfile`)
            OP_CRASH_OFFER => {
                let (version, most) = (r.u32(), r.u32());
                if version != 0 && !r.error && most > 0 {
                    self.crash_offer = Some((most as usize).min(CRASH_REPORT_MOST));
                }
            }
            // a crash report answered: the game lets it go
            OP_CRASH_FILED => {
                let (number, ticket) = (r.u32(), r.u32());
                if !r.error {
                    self.crash_filed.push((number, ticket));
                }
            }
            // a race's own words offered: taken (`racelabel`)
            OP_RACE_LABELS => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_RACE_LABELS, 1);
                }
            }
            // capes offered: taken (`cape`: worn in their slot, drawn)
            OP_CAPES => {
                if r.u32() != 0 && !r.error {
                    self.capes = true;
                    self.send_op_int(OP_CAPES, 1);
                }
            }
            // a pack and a bank past 40 offered: taken, as many as offered up
            // to what this client holds; their sizes count from the server's
            // word before a dump
            OP_ITEM_LISTS => {
                let (on, pack, bank) = (r.u32(), r.u32(), r.u32());
                if on != 0 && !r.error {
                    let mut w = Writer::op(OP_ITEM_LISTS);
                    w.u32(1);
                    w.u32(pack.clamp(ITEM_LIST_DISC, ITEM_LISTS_MOST));
                    w.u32(bank.clamp(ITEM_LIST_DISC, ITEM_LISTS_MOST));
                    self.send(&w.data);
                }
            }
            OP_ITEM_LISTS_IN_FORCE => {
                let (pack, bank) = (r.u32(), r.u32());
                if !r.error {
                    self.item_lists = [pack.clamp(ITEM_LIST_DISC, ITEM_LISTS_MOST), bank.clamp(ITEM_LIST_DISC, ITEM_LISTS_MOST)];
                }
            }
            OP_CAPES_LIST | OP_CAPES_IN_VIEW if self.capes => match crate::cape::read(&m.data[2.min(m.data.len())..]) {
                Some(list) => {
                    if op == OP_CAPES_LIST {
                        self.cape_list = list.into_iter().map(|(id, c)| (id as i64, c)).collect();
                    } else {
                        self.capes_in_view = list.into_iter().collect();
                    }
                    self.capes_changed += 1;
                }
                None => debug_line(&format!("[net] capes: {} bytes not read", m.data.len())),
            },
            // more of each character at select offered: taken (`selectinfo`)
            OP_SELECT_INFO => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_SELECT_INFO, 1);
                }
            }
            OP_SELECT_INFO_DATA => match crate::selectinfo::read(&m.data[2.min(m.data.len())..]) {
                Some(list) => self.select_info = list.into_iter().map(|(id, i)| (id as i64, i)).collect(),
                None => debug_line(&format!("[net] select info: {} bytes not read", m.data.len())),
            },
            // creation's races offered: taken (the create pages offer them)
            OP_CREATION_OFFER => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_CREATION_OFFER, 1);
                }
            }
            OP_CREATION_RACES => match crate::creation::read(&m.data[2.min(m.data.len())..]) {
                Some(c) => self.creation = Some(c),
                None => debug_line(&format!("[net] creation's races: {} bytes not read", m.data.len())),
            },
            // the server's own worlds offered: taken when they can be built
            // here (`set_world_builder`)
            OP_SERVER_WORLDS => {
                if r.u32() != 0 && !r.error && WORLD_BUILDER.get().is_some() {
                    self.send_op_int(OP_SERVER_WORLDS, 1);
                    self.worlds_pending = true;
                }
            }
            OP_SERVER_WORLDS_DATA => match crate::serverworlds::read(&m.data[2.min(m.data.len())..]) {
                Some(w) => {
                    if let Some(build) = WORLD_BUILDER.get() {
                        self.worlds_building = Some(build(&w));
                        self.worlds_pending = true;
                    }
                    self.server_worlds = Some(w);
                }
                None => debug_line(&format!("[net] the server's worlds: {} bytes not read", m.data.len())),
            },
            // which characters a login needs them for: the whole set each
            // time; one not read leaves what was known
            OP_SERVER_WORLDS_CHARS => match crate::serverworlds::read_chars(&m.data[2.min(m.data.len())..]) {
                Some(c) => self.worlds_chars = Some(c),
                None => debug_line(&format!("[net] the characters that need the server's worlds: {} bytes not read", m.data.len())),
            },

            // a custom race's own options offered: taken (the create pages and
            // the bodies drawn, `raceopts`)
            OP_RACE_OPTIONS => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_RACE_OPTIONS, RACE_OPTIONS_ANSWER);
                }
            }
            OP_RACE_OPTIONS_DATA => match crate::raceopts::read_options(&m.data[2.min(m.data.len())..]) {
                Some(o) => self.race_options = Some(o),
                None => debug_line(&format!("[net] race options: {} bytes not read", m.data.len())),
            },
            OP_RACE_LOOKS => self.race_looks = Some(m.data[2.min(m.data.len())..].to_vec()),
            // the NPCs' body tints offered: taken (`npctint`), what was held
            // dropped, the server sending the set there is
            // in the newest layout both read (an older server's 1: no glows)
            OP_NPC_TINTS_OFFER => {
                let layout = r.u8().clamp(1, crate::npctint::LAYOUT);
                if !r.error {
                    self.npc_tints = Some(Vec::new());
                    self.npc_tints_layout = layout;
                    let mut w = Writer::op(OP_NPC_TINTS_ANSWER);
                    w.u8(layout);
                    self.send(&w.data);
                }
            }
            OP_NPC_TINTS => match crate::npctint::read(&m.data[2.min(m.data.len())..], self.npc_tints_layout) {
                Some(t) => self.npc_tints = Some(t),
                None => debug_line(&format!("[net] NPC tints: {} bytes not read", m.data.len())),
            },
            // a monster's name and level offered (each world entry): taken at
            // once, in the layout this client reads, for the target's details
            // box (`Play::mob_info`); another layout is left to the box
            OP_MOB_INFO_OFFER => {
                let layout = r.u8();
                if !r.error && layout == 1 {
                    let mut w = Writer::op(OP_MOB_INFO_ANSWER);
                    w.u8(1);
                    self.send(&w.data);
                }
            }
            // item links offered (again at every world entry), in the layout
            // this client reads: told afresh whether it takes them
            // (`agree_item_links`); another layout is left alone
            // the profanity filter's setting asked for (again at every world
            // entry): told afresh (`agree_profanity_filter`); another layout
            // is left alone
            OP_PROFANITY_CHOICE => {
                let layout = r.u8();
                if !r.error && layout == 1 {
                    self.profanity_offer = true;
                    self.profanity_asked = None;
                }
            }
            // the worn looks hidden: offered (with the slots hidden) and
            // answered (`crate::hidelook`)
            crate::hidelook::OP_OFFER | crate::hidelook::OP_STATE => {
                self.hide_look.read(op, &m.data[2.min(m.data.len())..]);
            }
            OP_ITEM_LINKS => {
                let layout = r.u8();
                if !r.error && layout == crate::itemlink::LAYOUT {
                    self.item_links_offer = true;
                    self.item_links_asked = None;
                }
            }
            // a chat line with its links: the links, then the line's own
            // message read as it always is (a line that wraps another of
            // these, or none, is the plain message alone)
            OP_ITEM_LINK_LINE => match crate::itemlink::read_line(&m.data[2.min(m.data.len())..]) {
                Some((links, inner)) => {
                    let inner = drdp::Message { data: inner.to_vec(), ..m.clone() };
                    ev.push(Event::ChatLinks(links));
                    self.dispatch(&inner, ev);
                }
                None => debug_line(&format!("[net] a linked chat line: {} bytes not read", m.data.len())),
            },
            OP_ITEM_LINK_RECORD => match crate::itemlink::read_record(&m.data[2.min(m.data.len())..]) {
                Some((id, item)) => ev.push(Event::ItemLinkRecord { id, item: item.map(Box::new) }),
                None => debug_line(&format!("[net] an item link's record: {} bytes not read", m.data.len())),
            },
            OP_MOB_INFO => {
                let (layout, id, level, len) = (r.u8(), r.u32(), r.u8(), r.u8() as usize);
                let name = String::from_utf8_lossy(r.bytes(len)).into_owned();
                if !r.error && layout == 1 {
                    ev.push(Event::MobInfo { id, level, name });
                } else {
                    debug_line(&format!("[net] a monster's name and level: {} bytes not read", m.data.len()));
                }
            }
            // the mercs offered (each world entry): taken at once, answered 3
            // (the names with their hirers': the tag under a merc's name;
            // each one's class and mode: its details box; a server that knows
            // only 1 or 2 sends that)
            OP_MERCS_OFFER => {
                let layout = r.u8();
                if !r.error && layout == 1 {
                    let mut w = Writer::op(OP_MERCS_OFFER);
                    w.u8(3);
                    self.send(&w.data);
                }
            }
            OP_MERCS_DATA => match Mercs::read(&m.data[2.min(m.data.len())..]) {
                Some(v) => self.mercs = Some(v),
                None => debug_line(&format!("[net] the mercenaries: {} bytes not read", m.data.len())),
            },
            // the group search's standing offered: taken likewise
            OP_GROUP_REG_OFFER => {
                let layout = r.u8();
                if !r.error && layout == 1 {
                    let mut w = Writer::op(OP_GROUP_REG_OFFER);
                    w.u8(1);
                    self.send(&w.data);
                }
            }
            OP_GROUP_REG_STATE => match GroupReg::read(&m.data[2.min(m.data.len())..]) {
                Some(v) => self.group_reg = Some(v),
                None => debug_line(&format!("[net] the group search's standing: {} bytes not read", m.data.len())),
            },
            // any member invites into its group there (`Play::may_invite`)
            OP_MEMBER_INVITES => {
                if r.u32() != 0 && !r.error {
                    self.member_invites = true;
                }
            }
            OP_POWER_BARS => {
                // an offer (again at every world entry): told afresh what
                // the client wants, the server's player being a new one
                r.u32();
                if !r.error {
                    self.power_bars_offer = true;
                    self.power_bars_asked = None;
                }
            }
            OP_POWER_BARS_DATA => {
                let n = r.u8() as usize;
                let set: Vec<(u32, u8)> = (0..n).map(|_| (r.u32(), r.u8())).collect();
                if !r.error {
                    self.power_bars = set;
                }
            }
            OP_RAID_ROSTER_OFFER => {
                // an offer (again at every world entry): told afresh what the
                // client wants, the server's player being a new one
                let v = r.u32();
                if !r.error {
                    self.raid_roster_offer = true;
                    self.raid_roster_asked = None;
                    self.raid_roster_groups = v >= 2;
                    self.raid_roster_kinds = v >= 3;
                }
            }
            OP_RAID_ROSTER => {
                let (total, first, n) = (r.u16() as usize, r.u16() as usize, r.u8() as usize);
                // a group after each record, where this client asked for them
                let groups = self.raid_roster_groups && self.raid_roster_asked == Some(true);
                // and its kind after that, where it asked for them (3)
                let kinds = groups && self.raid_roster_kinds;
                let rows: Vec<RaidMember> = (0..n)
                    .map(|_| {
                        let id = r.u32();
                        let len = r.u8() as usize;
                        let name = String::from_utf8_lossy(r.bytes(len)).into_owned();
                        let (class, level, hp, power) = (r.u8(), r.u8(), r.u8(), r.u8());
                        let group = if groups { r.u8() } else { 0 };
                        let merc = kinds && r.u8() & 1 != 0;
                        RaidMember { id, name, class, level, hp, power, group, merc }
                    })
                    .collect();
                if !r.error {
                    ev.push(Event::RaidRoster { total, first, rows });
                }
            }
            OP_LIVE_LOOK_IN_FORCE => {
                let v = r.u32();
                if !r.error {
                    self.live_look = v & 1 != 0;
                    // the characters listed, from a server that says
                    if m.data.len() >= r.pos + 4 {
                        self.char_slots = r.u32().clamp(1, CHAR_SLOTS_MOST);
                    }
                    // its armour, from a server that says (go-eqoa-server)
                    self.armor_look = self.live_look && m.data.len() >= r.pos + 4 && r.u32() & LIVE_LOOK_ARMOR != 0;
                }
            }
            // the quest marks and tracker offered (again at every world
            // entry): told afresh what the client wants (`agree_quest_marks`,
            // `agree_quest_progress`), the server's player being a new one
            OP_QUEST_MARKERS => {
                r.u32();
                if !r.error {
                    self.quest_marks_offer = true;
                    self.quest_marks_asked = None;
                }
            }
            OP_QUEST_MARKERS_DATA => {
                if let Some(set) = read_quest_marks(&mut r) {
                    self.quest_marks = set;
                }
            }
            OP_QUEST_PROGRESS => {
                r.u32();
                if !r.error {
                    self.quest_progress_offer = true;
                    self.quest_progress_asked = None;
                }
            }
            OP_QUEST_PROGRESS_DATA => {
                if let Some(set) = read_quest_progress(&mut r) {
                    self.quest_progress = set;
                }
            }
            // the pet's effects offered (again at every world entry): told
            // afresh what the client wants (`agree_pet_effects`), the server's
            // player being a new one
            OP_PET_EFFECTS => {
                r.u32();
                if !r.error {
                    self.pet_effects_offer = true;
                    self.pet_effects_asked = None;
                }
            }
            OP_PET_EFFECTS_DATA => {
                if let Some(set) = read_pet_effects(&mut r) {
                    self.pet_effects = set;
                    self.pet_effects_at = self.now();
                }
            }
            // the number tags offered (again at every world entry): told
            // afresh whether the client wants them (`agree_number_tags`)
            OP_NUMBER_TAGS_OFFER => {
                let layout = r.u8();
                if !r.error {
                    self.number_tags_offer = true;
                    self.number_tags_layout = layout;
                    self.number_tags_asked = None;
                }
            }
            // the damage meter offered (again at every world entry): told
            // afresh whether the client wants it (`agree_dps_meter`)
            OP_DPS_METER_OFFER => {
                r.u8();
                if !r.error {
                    self.dps_meter_offer = true;
                    self.dps_meter_asked = None;
                }
            }
            OP_DPS_METER_DATA => {
                if let Some(m) = read_dps_meter(&mut r) {
                    ev.push(Event::DpsMeter(m));
                }
            }
            // the raid's windows offered (again at every world entry): told
            // afresh whether the client wants them (`agree_raid`)
            OP_RAID_OFFER => {
                r.u8();
                if !r.error {
                    self.raid_offer = true;
                    self.raid_asked = None;
                }
            }
            // the ready check's window offered (again at every world entry):
            // told afresh whether the client wants it (`agree_ready`)
            crate::readycheck::OP_READY_OFFER => {
                r.u8();
                if !r.error {
                    self.ready_offer = true;
                    self.ready_asked = None;
                }
            }
            // the buff window offered (again at every world entry): told
            // afresh whether the client wants it (`agree_buffs`)
            crate::buffs::OP_BUFF_WINDOW => {
                r.u8();
                if !r.error {
                    self.buffs_offer = true;
                    self.buffs_asked = None;
                }
            }
            // the quest marks over the NPCs' heads offered (again at every
            // world entry), in the layout this client reads: told afresh
            // whether the client wants them (`agree_head_marks`); another
            // layout is no offer
            crate::headmarks::OP_OFFER => {
                let layout = r.u8();
                if !r.error && layout == crate::headmarks::LAYOUT {
                    self.head_marks_offer = true;
                    self.head_marks_asked = None;
                }
            }
            crate::headmarks::OP_MARKS => match crate::headmarks::read(&m.data[2.min(m.data.len())..]) {
                Some(set) if self.head_marks_asked == Some(true) => self.head_marks = set.into_iter().collect(),
                Some(_) => {}
                None => eprintln!("net: quest marks of {} bytes not read", m.data.len()),
            },
            crate::buffs::OP_BUFF_WINDOW_DATA => match crate::buffs::read_set(&m.data[2.min(m.data.len())..]) {
                Some(set) => {
                    self.buffs = set;
                    self.buffs_at = self.now();
                }
                None => eprintln!("net: a buff window's set of {} bytes not read", m.data.len()),
            },
            crate::readycheck::OP_READY_CHECK => match crate::readycheck::read_check(&m.data[2.min(m.data.len())..]) {
                Some(c) => ev.push(Event::ReadyCheck(c)),
                None => eprintln!("net: a ready check of {} bytes not read", m.data.len()),
            },
            OP_RAID_STATE => match crate::raid::read_state(&m.data[2.min(m.data.len())..]) {
                Some(st) => ev.push(Event::RaidState(st)),
                None => eprintln!("net: a raid state of {} bytes not read", m.data.len()),
            },
            OP_RAID_ROLL => match crate::raid::read_roll(&m.data[2.min(m.data.len())..]) {
                Some(roll) => ev.push(Event::RaidRoll(roll)),
                None => eprintln!("net: a loot roll of {} bytes not read", m.data.len()),
            },
            OP_RAID_MASTER_LOOT => match crate::raid::read_master_loot(&m.data[2.min(m.data.len())..]) {
                Some(ml) => ev.push(Event::MasterLoot(ml)),
                None => eprintln!("net: a master looter's list of {} bytes not read", m.data.len()),
            },
            0x0013 => {
                // SetTime: each clock moved to the server's reading at the
                // local timer's now
                let (a, b) = (r.u64(), r.u64());
                let now = self.timer();
                self.clock_a.set(a as i64, now);
                self.clock_b.set(b as i64, now);
                ev.push(Event::Time(a, b));
            }
            0x002C if matches!(self.state, State::Shard | State::Characters) => {
                self.state = State::Characters;
                ev.push(Event::Characters(read_characters(&mut r, self.live_look, self.armor_look, self.char_slots.max(CLIENT_CHAR_SLOTS))));
            }
            0x0012 if self.state == State::Characters => ev.push(Event::RandomName(String::from_utf8_lossy(&r.string(24)).into_owned())),
            0x002E if self.state == State::Characters => ev.push(Event::CreateRefused(269)),
            0x002F if self.state == State::Characters => ev.push(Event::CreateRefused(829)),
            0x0065 if self.state == State::Characters => ev.push(Event::CreateRefused(828)),
            0x000D => {
                self.state = State::World;
                let dump = crate::dump::read(&mut r);
                // Every retail dump ends in 39 zero bytes past the last
                // field read (tests/dump_capture.rs).
                if self.debug || r.error || r.pos + 39 != r.data.len() {
                    debug_line(&format!("[net] MemoryDump: {} of {} bytes read{}", r.pos, r.data.len(), if r.error { ", SHORT" } else { "" }));
                }
                let dump_player = dump.player.clone();
                // Msg000D goes on only when the whole dump read.
                if !r.error {
                    let p = &dump.player;
                    let player = Player {
                        world_not_on_disc: dump.world_not_on_disc,
                        world_path: dump.world_path.clone(),
                        server_id: p.id,
                        name: p.name.clone(),
                        class: p.class,
                        race: p.race,
                        level: p.level,
                        world: p.world,
                        pos: p.pos,
                        hpr: p.hpr,
                    };
                    self.self_id = Some(m.src);
                    self.send(&Writer::op(0x0014).data);
                    // GroupChanged(m_p382E4): the chat mode kept
                    self.send_op_byte(0x000E, self.chat_mode);
                    ev.push(Event::WorldEntry { self_id: m.src, player, dump: Box::new(dump) });
                    // the channels that came before it, in order
                    ev.append(&mut self.early_states);
                }
                // SendOpByteByte(201, !m_p502D4, 0), (202, !m_p502D8, 0): the
                // options' Faction and Loot Messages OFF bits (m_x93, m_x94),
                // clear by default, as every capture sends them
                let mut w = Writer::op(0x00C9);
                w.u8(0).u8(0);
                self.send(&w.data);
                let mut w = Writer::op(0x00CA);
                w.u8(0).u8(0);
                self.send(&w.data);
                if !r.error {
                    // CLIENT's first frame after the dump writes the 0x40,
                    // and its flush carries it beside the batch above (every
                    // PS2 capture has 0x00CA beside its first 0x40): the
                    // walker as SetLocation leaves it, still, on the floor.
                    // Sandstorm needs it there: a first 0x40 in a later
                    // packet leaves the player out of its world (its own
                    // entity is all it sends, and it never moves it).
                    let p = &dump_player;
                    let phys = vi::walk::WalkPhysics::<Ee> {
                        position: vi::vect::Vect3::of(p.pos[0], p.pos[1], p.pos[2]),
                        angles: vi::vect::Vect3::of(p.hpr[0], 0.0, 0.0),
                        target_heading: vi::fpu::F::new(p.hpr[0]),
                        floor: vi::fpu::F::new(p.pos[1]),
                        flags: vi::walk::flags::ON_FLOOR | vi::walk::flags::TRACK_HEADING,
                        ..Default::default()
                    };
                    let msg = vi::selfstate::encode(p.world as u8, &phys, 0, u32::MAX);
                    self.send_state(0x40, &msg);
                }
            }
            0x00F8 => ev.push(Event::Speed(r.f32())),
            0x00C5 | 0x00C7 => ev.push(Event::Weather { weather: r.u8(), instant: op == 0x00C7 }),
            0x0046 => {
                let id = r.u32();
                let n = r.u32() as usize;
                let raw = r.bytes(n * 2);
                // ReadString(buf, 1024): 511 characters fit.
                let text = if n < 512 { raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect() } else { Vec::new() };
                ev.push(Event::Dialog { id, text });
            }
            0x0034 => {
                let id = r.u32();
                let wide = |r: &mut Reader, max: usize| -> Vec<u16> {
                    let n = r.u32() as usize;
                    let raw = r.bytes(n * 2);
                    if n < max { raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect() } else { Vec::new() }
                };
                // ReadString(raw, 256) and each option's ReadString(opt, 128).
                let text = wide(&mut r, 128);
                let n = r.u8();
                let options = (0..n).map(|_| wide(&mut r, 64)).collect();
                ev.push(Event::DialogBox { id, text, options });
            }
            0x00B2 => {
                let world = r.u8();
                let pos = [r.f32(), r.f32(), r.f32()];
                let hpr = [r.f32(), r.f32(), r.f32()];
                let token = r.u32();
                // SendOpSigned(73, token) comes after Respawn is done, so it
                // is the game's to send (`answer_teleport`)
                ev.push(Event::Teleport { world, pos, hpr, token });
            }
            0x0020 => {
                let level = r.signed();
                ev.push(Event::LevelExp { level, exp: r.signed() });
            }
            0x0775 => {
                let id = r.u32();
                ev.push(Event::Action { id, action: r.u8() });
            }
            0x07A3 => ev.push(Event::ReplyName(String::from_utf8_lossy(&r.string(24)).into_owned())),
            0x0045 => ev.push(Event::ProfanityFilter(r.signed() != 0)),
            0x00D9 => ev.push(Event::PlayerP2C(r.u32() as i32)),
            0x007E => ev.push(Event::Pet(Some(r.u32()))),
            0x007B => ev.push(Event::Assist(r.u32())),
            0x003B => ev.push(Event::InvAdd(Box::new(crate::dump::read_net_item(&mut r)))),
            0x003C => {
                let (slot, sound) = (r.u8(), r.u8());
                ev.push(Event::InvTook { slot, sound, n: r.signed() as i32 });
            }
            0x003D => ev.push(Event::InvMove { from: r.u8(), to: r.u8() }),
            0x003E => ev.push(Event::InvUse { slot: r.u8(), charge: r.u8() }),
            0x003F => ev.push(Event::InvEquip { i: r.u8(), slot: r.u8() }),
            0x0040 => ev.push(Event::InvUnequip(r.u8())),
            0x000A => ev.push(Event::QuickSlot { slot: r.u8(), pos: r.u8() }),
            0x0021 => {
                let slot = r.u8();
                let name = r.wide(16);
                ev.push(Event::GearSet { slot, name, main: r.u32(), off: r.u32() });
            }
            0x0060 => {
                let id = r.u32();
                ev.push(Event::ItemHp { id, hp: r.signed() as i32 });
            }
            0x0772 => {
                let _x = r.u32();
                let n = r.u32() as usize;
                // m_group has four members
                let n = n.min(4);
                let ids: Vec<u32> = (0..n).map(|_| r.u32()).collect();
                let names = (0..n).map(|_| String::from_utf8_lossy(&r.string(24)).into_owned()).collect();
                ev.push(Event::GroupRoster { ids, names });
            }
            0x0773 => {
                let (id, x) = (r.u32(), r.u32() as i32);
                let name = String::from_utf8_lossy(&r.string(24)).into_owned();
                ev.push(Event::GroupInvite { id, x, name });
            }
            0x0774 => ev.push(Event::GroupInviteGone(r.u32())),
            0x069A => ev.push(Event::GroupRemoved(r.u8())),
            0x062F => ev.push(Event::GroupDisbanded),
            0x069C => {
                let sub = r.u8();
                let name = if sub == 3 { String::new() } else { String::from_utf8_lossy(&r.string(24)).into_owned() };
                ev.push(Event::GroupNotice { sub, name });
            }
            0x00D3 => {
                // ReadString(name, 64), widened by str8to16 up to a NUL
                let name = r.string(64);
                let n = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                let name = String::from_utf8_lossy(&name[..n]).into_owned();
                ev.push(Event::SummonPrompt { name, kind: r.u8() });
            }
            0x007F => ev.push(Event::Pet(None)),
            0x0010 => {
                let (disposition, b, c) = (r.u8(), r.u8(), r.u8());
                let mut fixed = || {
                    let u: Vec<u16> = (0..64).map(|_| r.u16()).collect();
                    let n = u.iter().position(|&c| c == 0).unwrap_or(64);
                    String::from_utf16_lossy(&u[..n])
                };
                let (title, guild) = (fixed(), fixed());
                ev.push(Event::TargetInfo { disposition, b, c, title, guild });
            }
            0x0A7A => ev.push(Event::Chat(r.wide(1024), [1.0; 4])),
            // ReadString(buf, 2048), AddText in (1, .25, .25)
            0x00CE => ev.push(Event::Chat(r.wide(1024), [1.0, 0.25, 0.25, 1.0])),
            0x0A7B => {
                let text = r.wide(1024);
                let (cr, cg, cb) = (r.u8(), r.u8(), r.u8());
                ev.push(Event::Chat(text, [cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0, 1.0]));
            }
            0x069E => {
                let from = r.u32();
                ev.push(Event::GroupChat { from, text: r.wide(1024) });
            }
            0x0758 => ev.push(Event::Chat(String::from_utf8_lossy(&r.string(512)).into_owned(), [1.0; 4])),
            0x00B3 => {
                let (token, flag) = (r.u32(), r.u32());
                let ability = Box::new(crate::dump::read_ability(&mut r));
                ev.push(Event::AbilityAdd { token, flag, ability });
            }
            0x00B4..=0x00B6 => {
                let token = r.u32();
                let (a, b) = (r.u8(), r.u8());
                ev.push(match op {
                    0x00B4 => Event::AbilitySlot { token, index: a, slot: b },
                    0x00B5 => Event::AbilityMoveSlot { token, from: a, to: b },
                    _ => Event::AbilityMoveIndex { token, from: a, to: b },
                });
            }
            0x00FF => ev.push(Event::AbilityClearBar { token: r.u32() }),
            0x00FD => {
                let token = r.u32();
                let (id, p14) = (r.u32() as i32, r.u32() as i32);
                ev.push(Event::AbilityUsable { token, id, p14 });
            }
            0x00B8 => {
                let token = r.u32();
                ev.push(Event::AbilityRemove { token, index: r.signed() as i32 });
            }
            0x00CD => {
                let index = r.u8();
                let token = r.u32();
                ev.push(Event::AbilityLockout { index, token, lockout: r.signed() as i32 });
            }
            // The dispatcher's `Disconnect(reason, 0)` cases: the connection
            // goes, with the reason the notice shows.
            0x0729 | 0x072B | 0x0728 | 0x072A | 0x00EC | 0x07A4 | 0x07A1 | 0x07A2 | 0x07C0 | 0x07C1 | 0x07D2 => {
                let reason = match op {
                    0x0729 => 3,
                    0x072B => 4,
                    0x0728 => 6,
                    0x072A => 7,
                    0x00EC | 0x07A4 => 8,
                    0x07A1 => 13,
                    0x07A2 => 14,
                    0x07C0 => 17,
                    0x07C1 => 18,
                    _ => 19,
                };
                if let Some(c) = self.conn.take()
                    && self.drdp.conns.contains(c)
                {
                    self.drdp.connection_close(c);
                }
                ev.push(Event::Disconnected(reason));
            }
            _ => ev.push(Event::Opcode { op, data: m.data[2.min(m.data.len())..].to_vec() }),
        }
    }
}

/// `VIShardInfo::Read`: entries for the client's language (0) only.
/// `VIClient::ConnectStatusString`: the WORDS.TXT row the disconnect notice
/// shows for a reason (`Event::Disconnected`). Reasons 0 and 10 add a code
/// when there is one, which is left off here.
pub fn disconnect_word(reason: i32) -> Option<i32> {
    Some(match reason {
        0 => 272,
        1 => 666,
        2 => 431,
        3 => 270,
        4 => 593,
        5 => 594,
        6 => 620,
        7 => 621,
        8 => 619,
        9 => 820,
        10 => 667,
        11 => 1017,
        12 => 821,
        13 => 677,
        14 => 716,
        15 => 719,
        16 => 718,
        17 | 18 => 1024,
        19 => 1420,
        20 => 758,
        21 => 822,
        _ => return None,
    })
}

fn read_shards(r: &mut Reader) -> Vec<Shard> {
    let n = r.signed().max(0) as usize;
    let mut v = Vec::new();
    for _ in 0..n {
        let name = r.wide(32);
        let recommended = r.u8();
        let endpoint = r.u64();
        let language = r.u8() as i8;
        if !r.error && language == 0 {
            v.push(Shard { name, recommended, endpoint, language });
        }
    }
    v
}

/// `CharacterList`: at most eight (`m_charCount > 8 ? 8`).
/// 0x002B's bytes (`SendNewChar`): the name as `WriteString` (the form's
/// buffer is 24 bytes), nine zigzags, seven raw ints.
pub fn new_char_message(c: &NewChar) -> Vec<u8> {
    let mut w = Writer::op(0x002B);
    let name: Vec<u8> = c.name.bytes().take(23).collect();
    w.string(&name).signed(1);
    for v in [c.race, c.class, c.sex, c.hair_color, c.hair_length, c.hair_style, c.face, c.city] {
        w.signed(v as i64);
    }
    for v in c.stats {
        w.u32(v as u32);
    }
    if let Some(b) = crate::raceopts::choices_block(&c.options) {
        w.bytes(&b);
    }
    w.data
}

/// Whether to take the live game's look when a server offers it: always, but
/// for a test's `EQOA_LIVE_LOOK=0`, which keeps the disc's (a PS2's view of
/// the same server).
fn wants_live_look() -> bool {
    std::env::var("EQOA_LIVE_LOOK").map_or(true, |v| v != "0")
}

/// `CharacterList`: the disc's records, or with `live` the live game's
/// ([`LIVE_GEAR_SIZE`] raw bytes each, [`ARMOR_GEAR_SIZE`] with `armor`,
/// go-eqoa-server's armour); at most `most` of them (CLIENT's eight, or the
/// characters a server said it lists, [`Session::char_slots`]).
fn read_characters(r: &mut Reader, live: bool, armor: bool, most: u32) -> Vec<Character> {
    let gear = match (live, armor) {
        (true, true) => ARMOR_GEAR_SIZE,
        (true, false) => LIVE_GEAR_SIZE,
        _ => DISC_GEAR_SIZE,
    };
    let n = r.signed().clamp(0, most as i64) as usize;
    (0..n)
        .map(|_| {
            let name = String::from_utf8_lossy(&r.string(24)).into_owned();
            let mut f = [0i64; 9];
            for v in &mut f {
                *v = r.signed();
            }
            let gear = r.bytes(gear).to_vec();
            Character {
                name,
                server_id: f[0],
                model: f[1],
                class: f[2],
                race: f[3],
                level: f[4],
                hair_color: f[5],
                hair_length: f[6],
                hair_style: f[7],
                face: f[8],
                gear,
                options: Vec::new(),
            }
        })
        .collect()
}

/// `VISyncTime` (CLIENT `visynctime.cpp`): a clock synced to the server's.
/// `set` moves it to a new reading without a jump: the difference is
/// blended in over `BLEND_INTERVAL` of local time; `get` reads it at a local
/// time. VIClient keeps two, both set by 0x0013 (`SetTime`); the time of day
/// is clock B's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncTime {
    /// `m_bReady`
    pub ready: bool,
    /// `m_Blend`: what is left to blend in
    pub blend: i64,
    /// `m_local`: the local clock at the last `set`
    pub local: i64,
    /// `m_remote`: the synced time then
    pub remote: i64,
}

/// `BLEND_INTERVAL`, in the local clock's units.
pub const BLEND_INTERVAL: i64 = 60_000;

impl SyncTime {
    /// `VISyncTime::Set(t, base)`.
    pub fn set(&mut self, t: i64, base: i64) {
        if self.ready {
            let cur = self.get(base);
            self.blend = t.wrapping_sub(cur);
            self.remote = cur;
            self.local = base;
        } else {
            self.ready = true;
            self.local = base;
            self.remote = t;
            self.blend = 0;
        }
    }

    /// `VISyncTime::GetTime(now)` (its `m_bReady` assert left out: callers
    /// ask `ready` first).
    pub fn get(&self, now: i64) -> i64 {
        let dt = now.wrapping_sub(self.local);
        let m = if dt > BLEND_INTERVAL { BLEND_INTERVAL } else { dt };
        let r = self.blend.wrapping_mul(m) / BLEND_INTERVAL;
        self.remote.wrapping_add(dt).wrapping_add(r)
    }
}

/// `UpdateTimeOfDay`'s hour out of `clock` at local time `now`:
/// `(float)(t % 3600000) / 3600000.0f * 24.0f`, then `SetTimeOfDay`'s clamp
/// to 0..23.99999.
pub fn hour_of(clock: &SyncTime, now: i64) -> Option<vi::fpu::F<Ee>> {
    use vi::fpu::F;
    if !clock.ready {
        return None;
    }
    let t = clock.get(now) % 3_600_000;
    let h = F::<Ee>::new(t as f32) / F::new(3_600_000.0) * F::new(24.0);
    Some(vi::fpu::vimin(F::new(23.99999), vi::fpu::vimax(F::ZERO, h)))
}

#[cfg(test)]
mod tests {

    /// A session through the gateway to IPv4 (`addr::Nat64`), over real
    /// sockets on the IPv6 loopback: a server at [::1] is IPv4 0.0.0.1 under
    /// the prefix ::/96; a datagram to 0.0.0.1 reaches it, and its answer
    /// comes back as from 0.0.0.1. (Skipped where there is no IPv6 loopback.)
    #[test]
    fn a_datagram_goes_through_the_gateway_and_back() {
        let Ok(server) = UdpSocket::bind("[::1]:0") else { return };
        let at = server.local_addr().unwrap();
        let (n, ip) = Nat64::of(&at).unwrap();
        assert_eq!(ip, [0, 0, 0, 1]);
        let socket = UdpSocket::bind("[::]:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        socket.set_nonblocking(true).unwrap();
        let mut udp = Udp { socket, buf: vec![0u8; 2048], nat64: Some(n) };
        udp.send_to(b"hello", ip, at.port());
        server.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let mut b = [0u8; 16];
        let (k, from) = server.recv_from(&mut b).unwrap();
        assert_eq!((&b[..k], from.port()), (&b"hello"[..], port));
        server.send_to(b"answer", from).unwrap();
        let mut got = None;
        for _ in 0..200 {
            if let Some(d) = udp.recv_from() {
                got = Some(d);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(got, Some((b"answer".to_vec(), [0, 0, 0, 1], at.port())));
    }
    /// The DRDP clock starts low: the wall clock at 2026-10-28 00:58 UTC,
    /// past 2^31 as a u32, starts it below 2^30, so 12 days of a session
    /// stay below 2^31 and it never wraps, its low 16 bits (the drdp port)
    /// the wall clock's as before; any wall clock does. The control: the
    /// whole u32 started past 2^31.
    #[test]
    fn the_clock_starts_below_2_pow_30() {
        let today: u128 = 1_793_149_080_000;
        let b = super::clock_base(today);
        assert!(b < 1 << 30, "{b}");
        assert_eq!(b & 0xFFFF, today as u32 & 0xFFFF, "the port's bits");
        assert!(b + 12 * 24 * 3_600_000 < 1 << 31, "12 days");
        assert!(today as u32 >= 1 << 31, "the control: the u32 itself");
        for wall in [0u128, 1, (1 << 30) - 1, 1 << 30, 1 << 31, (1 << 32) - 1, 1 << 32, u64::MAX as u128, u128::MAX] {
            assert!(super::clock_base(wall) < 1 << 30, "{wall}");
        }
    }

    /// `VISyncTime` as visynctime.cpp has it: the first reading taken as it
    /// is and read forward on the local clock; a later one blended in over
    /// 60,000 of it (half the difference halfway, all of it after), never
    /// jumped to. The control: a clock that jumps to the reading.
    #[test]
    fn a_synced_clock_blends_a_new_reading_in() {
        use super::{BLEND_INTERVAL, SyncTime};
        let mut c = SyncTime::default();
        assert!(!c.ready);
        c.set(1_000_000, 500);
        assert_eq!((c.get(500), c.get(10_500)), (1_000_000, 1_010_000));
        // 2,000 ahead at local 20,500, where the clock read 1,020,000
        c.set(1_022_000, 20_500);
        assert_eq!(c.get(20_500), 1_020_000, "no jump");
        assert_eq!(c.get(50_500), 1_020_000 + 30_000 + 1_000, "half of it halfway");
        assert_eq!(c.get(80_500), 1_022_000 + 60_000, "all of it after the interval");
        assert_eq!(c.get(180_500), 1_022_000 + 160_000);
        // C's division: toward zero
        let at = c.get(200_000);
        c.set(at - 7, 200_000);
        assert_eq!(c.get(200_000 + BLEND_INTERVAL / 2), at + BLEND_INTERVAL / 2 - 3);
        // the control
        let jumped = |t: i64, base: i64, now: i64| t + (now - base);
        assert_ne!(jumped(1_022_000, 20_500, 50_500), 1_020_000 + 30_000 + 1_000);
    }

    /// `UpdateTimeOfDay`: clock B modulo 3,600,000 as 24 hours, clamped as
    /// `SetTimeOfDay` clamps; none before the server set the clock.
    #[test]
    fn the_time_of_day_is_clock_b_round_an_hour() {
        use super::{SyncTime, hour_of};
        let mut c = SyncTime::default();
        assert!(hour_of(&c, 0).is_none(), "not set yet");
        c.set(3_600_000 * 500_000 + 1_800_000, 0);
        assert_eq!(hour_of(&c, 0).unwrap().0, 12.0);
        assert_eq!(hour_of(&c, 900_000).unwrap().0, 18.0, "a quarter of the units on");
        assert_eq!(hour_of(&c, 1_800_000).unwrap().0, 0.0, "round again");
        assert_eq!(hour_of(&c, 1_799_999).unwrap().0, 23.99999, "clamped below 24");
    }

    use super::*;

    struct Null;
    impl Datagrams for Null {
        fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
        fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
            None
        }
    }

    fn session() -> Session {
        Session::connect_with(Box::new(Null), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap()
    }

    fn deliver(s: &mut Session, ty: u8, data: Vec<u8>) -> Vec<Event> {
        let mut ev = Vec::new();
        s.route(drdp::Message { ty, data, connection: s.conn, ..Default::default() }, &mut ev);
        ev
    }

    // 0x00B2 is the game's to answer, after its Respawn (`Play::msg_00b2`):
    // the session hands the move on with its token and sends nothing (it
    // answered at once until 2026-10-25, which with a world still loading
    // is not a PS2's order); `answer_teleport` is 0x0049 and the token,
    // zigzag. Control: the record does see a send.
    #[test]
    fn a_move_is_handed_on_unanswered() {
        let mut s = session();
        s.record = Some(Vec::new());
        let mut w = Writer::op(0x00B2);
        w.u8(2);
        for v in [1.0f32, 2.0, 3.0, 0.5, 0.0, 0.0] {
            w.u32(v.to_bits());
        }
        w.u32(7);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, w.data);
        assert!(matches!(ev[..], [Event::Teleport { world: 2, token: 7, .. }]), "{ev:?}");
        assert_eq!(s.record.as_deref(), Some(&[][..]), "nothing sent on receipt");
        s.answer_teleport(7);
        assert_eq!(s.record.unwrap(), [(drdp::TYPE_GUARANTEED, vec![0x49, 0x00, 14])]);
    }

    /// Item links (`itemlink`): nothing is said to a server that made no offer
    /// and no link goes there (the line goes plain: `say_linked` false); an
    /// offer in this layout is told the Extended row at once and again only as
    /// it changes, and links are in force only while it is on. The controls: an
    /// offer of another layout is not taken; a promotion forgets the offer.
    /// The profanity filter's setting goes only to a server that asked for it
    /// (0x7E60 in layout 1), once and again only as it changes, and afresh at
    /// the next offer. The controls: nothing before the offer, an offer of
    /// another layout is not taken, a new connection forgets the offer.
    #[test]
    fn the_profanity_filter_is_told_only_to_a_server_that_asked() {
        let mut s = session();
        s.record = Some(Vec::new());
        s.agree_profanity_filter(PROFANITY_OFF);
        assert_eq!(s.record.as_ref().unwrap().len(), 0, "nothing to a server that made no offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x60, 0x7E, 2]);
        s.agree_profanity_filter(PROFANITY_OFF);
        assert!(!s.profanity_offer, "another layout is not taken");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x60, 0x7E, 1]);
        s.agree_profanity_filter(PROFANITY_OFF);
        s.agree_profanity_filter(PROFANITY_OFF);
        s.agree_profanity_filter(PROFANITY_MEDIUM);
        s.agree_profanity_filter(PROFANITY_HIGH);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x60, 0x7E, 1]);
        s.agree_profanity_filter(PROFANITY_HIGH);
        let sent: Vec<Vec<u8>> = s.record.take().unwrap().into_iter().map(|(_, d)| d).collect();
        assert_eq!(
            sent,
            vec![vec![0x60, 0x7E, 0], vec![0x60, 0x7E, 2], vec![0x60, 0x7E, 1], vec![0x60, 0x7E, 1]],
            "off once, Medium, High, High again at the second offer"
        );
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.profanity_offer);
    }

    #[test]
    fn item_links_are_told_only_to_a_server_that_offered_them() {
        use crate::itemlink::{Linked, Route};
        let l = Linked { start: 0, len: 5, list: 0, key: 3 };
        let mut s = session();
        s.record = Some(Vec::new());
        s.agree_item_links(true);
        assert!(!s.item_links() && !s.say_linked(Route::Say, "Sword", u32::MAX, &[l]) && !s.ask_item_link(9));
        assert_eq!(s.record.as_ref().unwrap().len(), 0, "nothing to a server that made no offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4F, 0x7E, 2]);
        s.agree_item_links(true);
        assert!(!s.item_links_offer, "another layout is not taken");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4F, 0x7E, 1]);
        s.agree_item_links(true);
        s.agree_item_links(true);
        assert!(s.item_links());
        assert_eq!(s.record.as_ref().unwrap()[..], [(drdp::TYPE_GUARANTEED, vec![0x4F, 0x7E, 1])], "told once");
        assert!(s.say_linked(Route::Say, "Sword", u32::MAX, &[l]));
        assert!(!s.say_linked(Route::Say, "Sword", u32::MAX, &[]), "a line with no link goes plain");
        assert!(s.ask_item_link(9));
        s.agree_item_links(false);
        assert!(!s.item_links() && !s.ask_item_link(9), "the row off: none");
        let sent: Vec<Vec<u8>> = s.record.take().unwrap().into_iter().map(|(_, d)| d).collect();
        assert_eq!(sent.len(), 4);
        assert_eq!(&sent[1][..3], &[0x50, 0x7E, 0], "the line in place of 0x000B");
        assert_eq!(sent[2], vec![0x52, 0x7E, 9, 0, 0, 0]);
        assert_eq!(sent[3], vec![0x4F, 0x7E, 0]);
        // a new connection: the offer is the next world entry's
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.item_links_offer);
    }

    /// A linked line is its links' event, then the line's own message read as
    /// ever (a group line, a white one); a link's record is the item or none.
    /// The control: the same message unwrapped has no links' event.
    #[test]
    fn a_linked_line_is_read_as_its_own_message() {
        let mut s = session();
        let mut inner = Writer::op(0x069E);
        inner.u32(0x14F51).wide("Bob tells the group: see Rusty Sword");
        let mut w = Writer::op(OP_ITEM_LINK_LINE);
        w.u8(1).u16(25).u16(11).u32(4242).bytes(&inner.data);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, w.data);
        assert!(
            matches!(&ev[..], [Event::ChatLinks(l), Event::GroupChat { from: 0x14F51, text }] if l.len() == 1 && l[0].id == 4242 && l[0].start == 25 && text.ends_with("Rusty Sword")),
            "{ev:?}"
        );
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, inner.data);
        assert!(matches!(&ev[..], [Event::GroupChat { .. }]), "the control: {ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x53, 0x7E, 7, 0, 0, 0, 0]);
        assert!(matches!(&ev[..], [Event::ItemLinkRecord { id: 7, item: None }]), "{ev:?}");
    }

    /// The net library's lines, as a test reads them (`set_debug_sink`: once
    /// for the process, so every test's lines come here after it is set).
    static NET_LINES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    fn keep_net_line(s: &str) {
        NET_LINES.lock().unwrap().push(s.to_string());
    }

    // A game connection the DRDP library let go with no DISCONNECT reaching
    // the session, as its system messages 20 and 21 close every connection to
    // an endpoint (`drdp_close_connections_to_endpoint`): a send on its handle
    // panicked ("dangling handle"). Now the handle is let go, the loss said
    // once, every message after it counted and not sent, and a game connection
    // opened again sends. Control: with the connection live, the same sends go
    // into its queue and nothing is lost or said.
    #[test]
    fn a_connection_drdp_let_go_is_never_sent_on() {
        set_debug_sink(keep_net_line);
        let said = || NET_LINES.lock().unwrap().iter().filter(|l| l.contains("the game connection is gone")).count();
        // the control
        let mut s = session();
        s.record = Some(Vec::new());
        let c = s.conn.unwrap();
        let queued = s.drdp.conns[c].send_queue.len();
        s.send(&[0x0B, 0x00]);
        s.send_state(0x40, &[1, 2, 3]);
        assert_eq!(s.drdp.conns[c].send_queue.len(), queued + 2, "the control: both queued");
        assert_eq!((s.sends_lost, s.record.as_ref().unwrap().len()), (0, 2));
        let before = said();
        // gone from under the session, as system messages 20 and 21 make it
        let remote = s.drdp.conns[c].remote_id;
        s.drdp.close_connections_to_endpoint(remote);
        assert!(!s.drdp.conns.contains(c), "the connection is gone");
        assert_eq!(s.conn, Some(c), "and the session still holds its handle");
        s.send(&[0x0B, 0x00]);
        s.send_state(0x40, &[1, 2, 3]);
        s.send(&[0x0E, 0x00, 0]);
        assert_eq!(s.conn, None, "the handle let go");
        assert_eq!(s.sends_lost, 3, "every message after it counted");
        assert_eq!(s.record.as_ref().unwrap().len(), 2, "none of them sent");
        assert_eq!(said() - before, 1, "said once");
        // a new game connection sends again (as connect_shard opens one)
        s.connect_shard(remote, None);
        let c2 = s.conn.expect("a new connection");
        let queued = s.drdp.conns[c2].send_queue.len();
        s.send(&[0x0B, 0x00]);
        assert_eq!(s.drdp.conns[c2].send_queue.len(), queued + 1);
        assert_eq!(s.sends_lost, 3, "nothing lost on the new one");
    }

    // 0x07A4, what go-eqoa-server sends a session whose character logged in
    // from another window: CLIENT's Disconnect(8), word 619, and the
    // connection goes. Control: an opcode that is not a disconnect keeps it.
    #[test]
    fn logged_in_elsewhere_disconnects() {
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xA4, 0x07]);
        assert!(matches!(ev[..], [Event::Disconnected(8)]), "{ev:?}");
        assert!(s.conn.is_none());
        assert_eq!(disconnect_word(8), Some(619));

        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xA5, 0x07]);
        assert!(!ev.iter().any(|e| matches!(e, Event::Disconnected(_))), "{ev:?}");
        assert!(s.conn.is_some());
    }

    // 0x00CE, the server's error line, is a chat line in red, as CLIENT's
    // dispatcher adds it; it was dropped, so a refused tell or an unknown
    // command said nothing. The control: 0x0A7A is white.
    #[test]
    fn error_text_is_a_red_line() {
        let wide = |op: u16, t: &str| {
            let mut v = op.to_le_bytes().to_vec();
            v.extend((t.encode_utf16().count() as u32).to_le_bytes());
            v.extend(t.encode_utf16().flat_map(|c| c.to_le_bytes()));
            v
        };
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, wide(0x00CE, "Tell Failed"));
        assert!(matches!(&ev[..], [Event::Chat(t, c)] if t == "Tell Failed" && *c == [1.0, 0.25, 0.25, 1.0]), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, wide(0x0A7A, "hi"));
        assert!(matches!(&ev[..], [Event::Chat(t, c)] if t == "hi" && *c == [1.0; 4]), "{ev:?}");
    }

    // 0x069E, the group's chat (a u32, then the line), is its own event: what
    // reads the group's lines never takes another channel's. The control: the
    // same words on 0x0A7A are a plain chat line.
    #[test]
    fn a_group_line_is_its_own_event() {
        let text = "Dairine tells the group: Waypoint in Tunaria at x 1, z 2";
        let mut v = 0x069Eu16.to_le_bytes().to_vec();
        v.extend(0x0001_4F51u32.to_le_bytes());
        v.extend((text.encode_utf16().count() as u32).to_le_bytes());
        v.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, v);
        assert!(matches!(&ev[..], [Event::GroupChat { from: 0x0001_4F51, text: t }] if t == text), "{ev:?}");
        let mut w = 0x0A7Au16.to_le_bytes().to_vec();
        w.extend((text.encode_utf16().count() as u32).to_le_bytes());
        w.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, w);
        assert!(matches!(&ev[..], [Event::Chat(t, _)] if t == text), "the control: {ev:?}");
    }

    /// 0x0013 (`SetTime`) sets both clocks at the local timer's now, and the
    /// time of day is clock B's; none before it came (the control).
    #[test]
    fn set_time_sets_the_clocks() {
        let mut s = session();
        assert!(s.time_of_day().is_none());
        let (a, b) = (1_792_732_374_253u64, 3_600_000u64 * 498_000 + 900_000);
        let mut m = 0x0013u16.to_le_bytes().to_vec();
        m.extend(a.to_le_bytes());
        m.extend(b.to_le_bytes());
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m);
        assert!(matches!(&ev[..], [Event::Time(x, y)] if (*x, *y) == (a, b)), "{ev:?}");
        assert!(s.clock_a.ready && s.clock_b.ready);
        assert_eq!(s.clock_b.remote, b as i64);
        let h = s.time_of_day().unwrap().0;
        assert!((6.0..6.01).contains(&h), "a quarter round clock B's hour: {h}");
    }

    // 0x0045's zigzag: 2 is on, 0 off (Msg0045's ReadSigned). The control:
    // the byte read raw would make 2 "on" and 1 "on" too.
    #[test]
    fn profanity_filter_state() {
        for (byte, on) in [(2u8, true), (0, false), (1, true)] {
            let mut s = session();
            let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x45, 0x00, byte]);
            assert!(matches!(ev[..], [Event::ProfanityFilter(o)] if o == on), "{byte}: {ev:?}");
        }
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x45, 0x00, 0x00]);
        assert!(!matches!(ev[..], [Event::ProfanityFilter(true)]), "zero is off");
    }

    // 0x00D3, the summons and resurrection offer: the caster's name
    // (ReadString(name, 64)) and the kind. It went by as an unhandled
    // opcode, so a resurrection on eqoa.live put up no box. The control:
    // a name of 64 bytes or more is read as empty, the kind still after it.
    #[test]
    fn summon_prompt_reads_the_name_and_kind() {
        let msg = |name: &[u8], kind: u8| {
            let mut w = Writer::op(0x00D3);
            w.string(name).u8(kind);
            w.data
        };
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg(b"Healer", 1));
        assert!(matches!(&ev[..], [Event::SummonPrompt { name, kind: 1 }] if name == "Healer"), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg(b"Mage\0junk", 0));
        assert!(matches!(&ev[..], [Event::SummonPrompt { name, kind: 0 }] if name == "Mage"), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg(&[b'x'; 64], 1));
        assert!(matches!(&ev[..], [Event::SummonPrompt { name, kind: 1 }] if name.is_empty()), "{ev:?}");
    }

    // A DRDP 0xFD carries the library's byte, mapped as PumpMessages maps it.
    #[test]
    fn drdp_disconnect_maps_its_byte() {
        for (byte, reason) in [(14, 16), (1, 1), (15, 15), (0, 10), (7, 10)] {
            let mut s = session();
            let ev = deliver(&mut s, drdp::TYPE_DISCONNECT, vec![byte]);
            assert!(matches!(ev[..], [Event::Disconnected(r)] if r == reason), "{byte}: {ev:?}");
        }
    }

    // 0x07E0 is answered with 0x07E1: the game connection's eleven counters
    // as VINetStats writes them, then what followed 0x07E0. The counters of
    // the PS2's own answer in pcaps/ps2_sandstorm.pcap (packet 115, 512
    // bytes after the opcode, a count of 254 characters) give that answer's
    // text exactly. Controls: trailing bytes are echoed after the text, and
    // with no game connection nothing is sent.
    #[test]
    fn stats_are_answered_as_a_ps2_answers_them() {
        let mut s = session();
        s.record = Some(Vec::new());
        let c = s.conn.unwrap();
        s.drdp.conns[c].cstat = [0, 0, 0, 0, 0, 62, 19, 148, 290, 24, 56];
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xE0, 0x07]);
        let sent = s.record.take().unwrap();
        assert_eq!(sent.len(), 1, "{sent:02x?}");
        let (ty, d) = &sent[0];
        assert_eq!(*ty, drdp::TYPE_GUARANTEED);
        assert_eq!(d[..2], [0xE1, 0x07]);
        assert_eq!(d.len() - 2, 512, "the capture's payload size");
        let text = "0: discarded (old)\n0: discarded (no memory was available to process message)\n0: discarded (corrupt)\n0: discarded (crc)\n0: out of sequence\n62: recv\n19: sent\n148: guaranteed recv (duplicates)\n290: guaranteed recv\n24: state recv (duplicates)\n56: state recv\n";
        assert_eq!(d[2..6], 254u32.to_le_bytes(), "the capture's count");
        let wide: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        assert_eq!(d[6..], wide[..]);

        // control: what followed the opcode comes after the text
        s.record = Some(Vec::new());
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xE0, 0x07, 1, 2, 3]);
        let sent = s.record.take().unwrap();
        assert_eq!(sent[0].1[sent[0].1.len() - 3..], [1, 2, 3]);
        assert_eq!(sent[0].1.len(), 2 + 512 + 3);

        // control: no game connection, nothing sent
        let mut s = session();
        s.conn = None;
        s.record = Some(Vec::new());
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xE0, 0x07]);
        assert_eq!(s.record.unwrap(), []);
    }

    // 0x0790's probe: 0x0992 (the token, the eight bytes) outside any
    // connection to the endpoint 0x0790 names, at once and then no more
    // often than the connection's timeout (a second); 0x0993 coming back
    // as a type 0xF8 with the token is answered with 0x0791 on the game
    // connection. Controls: no probe, no 0x0992; a second flush within the
    // timeout sends none; a wrong token is not answered.
    #[test]
    fn a_probe_is_sent_and_its_echo_answered() {
        let mut s = session();
        // the game's clock well on, as it is in play
        s.start -= std::time::Duration::from_secs(20);
        s.drdp.outbox.clear();
        s.msg_0790_done();
        assert!(s.drdp.outbox.is_empty(), "control: no probe, no 0x0992");

        let endpoint = address_to_endpoint([10, 0, 0, 9], 5555, 0x1234);
        let mut w = Writer::op(0x0790);
        w.u64(endpoint).u32(0xAABB_CCDD).u64(0x1122_3344_5566_7788);
        deliver(&mut s, drdp::TYPE_GUARANTEED, w.data);
        assert_eq!(s.probe.endpoint, endpoint as i64);
        let probes: Vec<_> = s.drdp.outbox.drain(..).collect();
        assert_eq!(probes.len(), 1, "sent at once: {probes:02x?}");
        assert_eq!(probes[0].to, drdp::Addr { ip: [10, 0, 0, 9], port: 5555 });
        let mut want = vec![0xff, 0xff, 0x92, 0x09];
        want.extend(0xAABB_CCDDu32.to_le_bytes());
        want.extend(0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(probes[0].data, want, "drdp port 0xffff, no CRC");

        s.msg_0790_done();
        assert!(s.drdp.outbox.is_empty(), "control: not again within the timeout");
        s.start -= std::time::Duration::from_millis(1100);
        s.msg_0790_done();
        assert_eq!(s.drdp.outbox.drain(..).count(), 1, "again a timeout on");

        s.record = Some(Vec::new());
        let mut w = Writer::op(0x0993);
        w.u32(0x1234_5678);
        deliver(&mut s, 0xF8, w.data);
        assert_eq!(s.record.as_deref(), Some(&[][..]), "control: a wrong token");
        let mut w = Writer::op(0x0993);
        w.u32(0xAABB_CCDD);
        deliver(&mut s, 0xF8, w.data);
        assert_eq!(s.record.unwrap(), [(drdp::TYPE_GUARANTEED, vec![0x91, 0x07, 0xDD, 0xCC, 0xBB, 0xAA])]);
    }

    /// Every datagram sent, kept.
    struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
    impl Datagrams for Rec {
        fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
            self.0.borrow_mut().push(d.to_vec());
        }
        fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
            None
        }
    }

    /// A whole MemoryDump with nothing in its lists: the player at `pos`.
    fn empty_dump(pos: [f32; 3], heading: f32) -> Vec<u8> {
        let mut w = Writer::op(0x000D);
        w.u8(0).string(b"data\\tunaria.esf");
        w.signed(7).string(b"Tester");
        for v in [12, 8, 49, 0, 0] {
            w.signed(v);
        }
        w.u8(255);
        for v in [0, 0, 0, 0, 0] {
            w.signed(v);
        }
        for v in pos.into_iter().chain([heading, 0.0, 0.0]) {
            w.u32(v.to_bits());
        }
        w.signed(0).u32(0).u32(0).u32(0); // text sets, the two tokens, quests
        w.signed(0).u32(0); // inventory
        w.signed(0).u32(0).signed(0).signed(0).u8(0); // bank, lots, bids
        w.u8(0).signed(0).u32(8.0f32.to_bits()); // no corpse, no abilities, the speed
        for _ in 0..25 {
            w.u32(0); // the crafting slots
        }
        w.u32(0);
        for _ in 0..4 {
            w.signed(0);
        }
        w.u32(8.0f32.to_bits());
        for _ in 0..36 {
            w.signed(0); // Combat Mastery
        }
        w.bytes(&[0; 39]);
        w.data
    }

    /// The messages in one of the client's datagrams: (type, payload), a
    /// guaranteed one's payload starting with its opcode, a state's RLE undone.
    fn messages(d: &[u8]) -> Vec<(u8, Vec<u8>)> {
        use drdp::buffer::{Buffer, rle_decode};
        let mut out = Vec::new();
        let mut b = Buffer::reader(&d[..d.len() - 4]);
        b.u16();
        b.u16();
        while b.remaining() > 0 && !b.overflow {
            let h = drdp::read_segment_header(&mut b, 0, 0);
            let mut s = Buffer::reader(&b.rest()[..(h.size as usize).min(b.rest().len())]);
            b.skip(h.size as usize);
            if h.flags & 0x30000 != 0 || s.max == 0 {
                continue;
            }
            let f = s.u8();
            if f & 0x40 != 0 {
                s.u32();
            }
            s.u16();
            if f & 1 != 0 {
                s.u16();
                if f & 4 != 0 {
                    s.packed();
                }
            }
            if f & 2 != 0 {
                s.u16();
                if f & 8 != 0 {
                    s.packed();
                }
            }
            if f & 0x10 != 0 {
                while s.u8() != 248 && !s.overflow {
                    s.u16();
                }
            }
            while s.remaining() > 0 && !s.overflow {
                let ty = s.u8();
                let size = s.size() as usize;
                if ty.wrapping_add(7) < 3 {
                    s.u16();
                    out.push((ty, s.rest()[..size].to_vec()));
                    s.skip(size);
                } else if ty == 252 {
                    s.skip(size);
                } else if ty < 248 {
                    s.u16();
                    s.u8();
                    let mut v = vec![0; size];
                    rle_decode(&mut s, &mut v);
                    out.push((ty, v));
                }
            }
        }
        out
    }

    // go-eqoa-server's 0x7E00 offer is kept for the connection, and gone
    // with it (a new one starts with none); a short one is not taken. The
    // answer is the opcode and two u32s. The control: no offer before one came.
    #[test]
    fn entity_view_offer_and_answer() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        assert_eq!(s.entity_view_offer, None, "no offer before one came");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x00, 0x7E, 204, 0, 0, 0]);
        assert_eq!(s.entity_view_offer, None, "a short offer");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x00, 0x7E, 204, 0, 0, 0, 0xE8, 0x03, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(s.entity_view_offer, Some((204, 1000)));

        s.flush();
        sent.borrow_mut().clear();
        s.send_entity_view(128, 600);
        s.flush();
        let m: Vec<_> = sent.borrow().iter().flat_map(|d| messages(d)).collect();
        assert_eq!(m, [(251, vec![0x00, 0x7E, 128, 0, 0, 0, 0x58, 0x02, 0, 0])]);

        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(s.entity_view_offer, None, "a new connection, no offer yet");
    }

    // Bug reports: the offer taken (answered 1, kept), a short one not; the
    // note as the server reads it (version 1, its byte count, its bytes), cut
    // to BUG_NOTE_MAX on a character's edge. The control: no answer before an
    // offer came.
    #[test]
    fn bug_reports_offer_and_note() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0E, 0x7E, 1, 0]);
        assert!(!s.bug_reports, "a short offer");
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "the control: nothing answered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0E, 0x7E, 1, 0, 0, 0]);
        assert!(s.bug_reports);
        assert_eq!(ours(&mut s), [vec![0x0E, 0x7E, 1, 0, 0, 0]]);
        s.send_bug_note("build: local\nplace: here");
        let mut want = vec![0x0F, 0x7E, 1, 0, 0, 0, 24, 0, 0, 0];
        want.extend(b"build: local\nplace: here");
        assert_eq!(ours(&mut s), [want]);
        let long = "é".repeat(BUG_NOTE_MAX);
        s.send_bug_note(&long);
        let m = ours(&mut s);
        let n = u32::from_le_bytes(m[0][6..10].try_into().unwrap()) as usize;
        assert!(n <= BUG_NOTE_MAX && n % 2 == 0 && m[0].len() == 10 + n, "cut on a character's edge: {n}");
    }

    // The crash report offer: taken with its most (this client's at most), a
    // short one not; a report goes as its number, count and text cut to the
    // most on a character's edge; an answer is kept for the game. The control:
    // before an offer nothing is held to send.
    #[test]
    fn crash_reports_offer_report_and_answer() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        assert_eq!(s.crash_offer, None, "the control: no offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x64, 0x7E, 1, 0, 0, 0]);
        assert_eq!(s.crash_offer, None, "a short offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x64, 0x7E, 1, 0, 0, 0, 0xE0, 0x2E, 0, 0]);
        assert_eq!(s.crash_offer, Some(12_000));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x64, 0x7E, 1, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0]);
        assert_eq!(s.crash_offer, Some(CRASH_REPORT_MOST), "at most this client's");
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "the offer is not answered");
        s.send_crash_report(7, "panic: boom", 100);
        let mut want = vec![0x65, 0x7E, 1, 0, 0, 0, 7, 0, 0, 0, 11, 0, 0, 0];
        want.extend(b"panic: boom");
        assert_eq!(ours(&mut s), [want]);
        s.send_crash_report(8, &"é".repeat(100), 51);
        let m = ours(&mut s);
        let n = u32::from_le_bytes(m[0][10..14].try_into().unwrap()) as usize;
        assert_eq!((n, m[0].len()), (50, 14 + 50), "cut on a character's edge");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x66, 0x7E, 7, 0, 0, 0, 0xE8, 0x03, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x66, 0x7E, 8, 0, 0, 0]);
        assert_eq!(s.crash_filed, [(7, 1000)], "an answer cut short is not kept");
    }

    // The most of a petition's text: a PS2's 511 from an offer that says
    // nothing more (a go-eqoa-server of before 2026-11-08), else what it says,
    // up to this client's form; a 0 or a second field cut short says nothing
    // (the last kept). The answer is 1 either way.
    #[test]
    fn the_offer_says_how_long_a_petition_may_be() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0E, 0x7E, 1, 0, 0, 0]);
        assert!(s.bug_reports);
        assert_eq!(s.petition_text, PETITION_TEXT_DISC, "an offer that says nothing of a petition's length");
        for (said, want) in [(vec![0xFF, 0x07, 0, 0], 2047), (vec![0xFF, 0xFF, 0, 0], PETITION_TEXT_MOST), (vec![0x2C, 0x01, 0, 0], 300), (vec![0, 0, 0, 0], 300), (vec![0xFF, 0x07], 300)] {
            let mut offer = vec![0x0E, 0x7E, 1, 0, 0, 0];
            offer.extend(said);
            deliver(&mut s, drdp::TYPE_GUARANTEED, offer);
            assert_eq!(s.petition_text, want);
        }
        s.flush();
        let answers: Vec<Vec<u8>> = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
        assert!(!answers.is_empty() && answers.iter().all(|a| *a == [0x0E, 0x7E, 1, 0, 0, 0]), "{answers:?}");
    }

    // Any member's invites (0x7E17): the offer kept, a short one not, and
    // nothing answered. The control: a session never offered it keeps the
    // leader's rule.
    #[test]
    fn member_invites_offer() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        assert!(!s.member_invites, "the control: never offered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x17, 0x7E, 1, 0]);
        assert!(!s.member_invites, "a short offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x17, 0x7E, 1, 0, 0, 0]);
        assert!(s.member_invites);
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "nothing answered");
    }

    // The mercs (0x7E5A/0x7E5B) and the group search's standing (0x7E5D/
    // 0x7E5E): each offer answered 1 at once, another layout nothing; the sets
    // kept as told (own merc, the names; the three flags), a short one not.
    // The control: never offered, nothing held.
    #[test]
    fn mercs_and_group_reg_offered_answered_kept() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        assert_eq!((s.mercs.clone(), s.group_reg), (None, None), "the control: never offered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x5A, 0x7E, 2]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x5D, 0x7E, 2]);
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "another layout: no answer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x5A, 0x7E, 1]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x5D, 0x7E, 1]);
        assert_eq!(ours(&mut s), vec![vec![0x5A, 0x7E, 3], vec![0x5D, 0x7E, 1]], "the mercs' answered 3 (their hirers, classes and modes too)");
        let mut d = vec![0x5B, 0x7E, 1, 1, 2, 0, 7];
        d.extend_from_slice(b"Ralthas");
        d.push(3);
        d.extend_from_slice(b"Kel");
        deliver(&mut s, drdp::TYPE_GUARANTEED, d.clone());
        assert_eq!(s.mercs, Some(Mercs { own: true, names: vec!["Ralthas".into(), "Kel".into()], owners: Vec::new(), classes: Vec::new() }));
        assert_eq!(s.mercs.as_ref().unwrap().owner("Kel"), None, "layout 1 names no hirer");
        assert!(s.mercs.as_ref().unwrap().is("Kel") && !s.mercs.as_ref().unwrap().is("Kelso") && !s.mercs.as_ref().unwrap().is(""));
        d.pop();
        deliver(&mut s, drdp::TYPE_GUARANTEED, d);
        assert_eq!(s.mercs.as_ref().map(|m| m.names.len()), Some(2), "a short set is not taken");
        // layout 2: each with its hirer's name
        let mut d = vec![0x5B, 0x7E, 2, 0, 2, 0, 7];
        d.extend_from_slice(b"Ralthas");
        d.push(8);
        d.extend_from_slice(b"Lonewulf");
        d.push(3);
        d.extend_from_slice(b"Kel");
        d.push(4);
        d.extend_from_slice(b"Abby");
        deliver(&mut s, drdp::TYPE_GUARANTEED, d.clone());
        let m = s.mercs.clone().unwrap();
        assert_eq!((m.own, m.owner("Ralthas"), m.owner("Kel"), m.owner("Lonewulf")), (false, Some("Lonewulf"), Some("Abby"), None));
        d.pop();
        deliver(&mut s, drdp::TYPE_GUARANTEED, d);
        assert_eq!(s.mercs.as_ref().and_then(|m| m.owner("Kel")), Some("Abby"), "a short layout 2 is not taken");
        // layout 3: each with its class and a healer's mode as well
        let mut d = vec![0x5B, 0x7E, 3, 1, 1, 0, 7];
        d.extend_from_slice(b"Jorador");
        d.push(7);
        d.extend_from_slice(b"Jibrish");
        d.extend_from_slice(&[9, 2]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, d.clone());
        let m = s.mercs.clone().unwrap();
        assert_eq!((m.owner("Jorador"), m.class("Jorador"), m.class("Kel")), (Some("Jibrish"), Some((9, 2)), None));
        d.pop();
        deliver(&mut s, drdp::TYPE_GUARANTEED, d);
        assert_eq!(s.mercs.as_ref().and_then(|m| m.class("Jorador")), Some((9, 2)), "a short layout 3 is not taken");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x5E, 0x7E, 1, 0b101]);
        assert_eq!(s.group_reg, Some(GroupReg { registered: true, invites_open: false, members_invite: true }));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x5E, 0x7E, 1]);
        assert_eq!(s.group_reg.map(|g| g.registered), Some(true), "a short one is not taken");
    }

    // A GM's flight (0x7E1B): this client says it flies when it is offered the
    // GM's trigger names, in answer to nothing else (the control: the other
    // offers, a short one and a 0 say nothing); the server's word is kept, a
    // short one not, and goes with the connection, not with a world entry's dump.
    #[test]
    fn fly_said_when_offered_and_kept_for_the_connection() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x17, 0x7E, 1, 0, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 0, 0, 0, 0]);
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "the control: nothing said to anything else");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0, 0]);
        assert_eq!(ours(&mut s), [vec![0x07, 0x7E, 2, 0, 0, 0], vec![0x1B, 0x7E, 1, 0, 0, 0]], "the offer answered (every trigger), and that it flies");

        assert!(!s.flying, "not until the server says so");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1B, 0x7E, 1, 0]);
        assert!(!s.flying, "a short word");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1B, 0x7E, 1, 0, 0, 0]);
        assert!(s.flying);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "nothing answered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1B, 0x7E, 0, 0, 0, 0]);
        assert!(!s.flying, "off");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1B, 0x7E, 1, 0, 0, 0]);
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.flying, "a new connection, a new login: not flying");
    }

    // The jump (0x7E28): the offer answered with 1 at once and kept; a 0 ends it
    // and is answered nothing; it goes with the connection. The controls: an
    // offer cut short and an offer of 0 are neither answered nor kept, and a
    // session never offered one says nothing and does not jump.
    #[test]
    fn the_jump_offer_is_answered_and_kept_for_the_connection() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        assert!(!s.jumps, "never offered: no jump");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x28, 0x7E, 1, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x28, 0x7E, 0, 0, 0, 0]);
        assert!(!s.jumps, "the controls: cut short, or 0");
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "nothing said to them");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x28, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty() && s.jumps, "the offer is the session's: {ev:?}");
        assert_eq!(ours(&mut s), [vec![0x28, 0x7E, 1, 0, 0, 0]], "answered: it jumps");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x28, 0x7E, 0, 0, 0, 0]);
        assert!(!s.jumps, "the server's 0 ends it");
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "and is answered nothing");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x28, 0x7E, 1, 0, 0, 0]);
        assert_eq!(ours(&mut s), [vec![0x28, 0x7E, 1, 0, 0, 0]], "offered again: answered again");
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.jumps, "a new connection, a new login: no jump until offered");
    }

    // The whole of a raid group: told nothing with no offer; offered, told what
    // is wanted once and when it changes, and afresh at another offer; each
    // message read whole, a short one not at all.
    #[test]
    fn the_raid_roster_is_agreed_and_read() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        s.agree_raid_roster(true);
        assert_eq!(answers(&mut s), Vec::<Vec<u8>>::new(), "no offer: the control");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x18, 0x7E, 1, 0, 0, 0]);
        s.agree_raid_roster(true);
        s.agree_raid_roster(true);
        assert_eq!(answers(&mut s), [vec![0x19, 0x7E, 1, 0, 0, 0]], "once");
        s.agree_raid_roster(false);
        assert_eq!(answers(&mut s), [vec![0x19, 0x7E, 0, 0, 0, 0]], "when it changes");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x18, 0x7E, 1, 0, 0, 0]);
        s.agree_raid_roster(false);
        assert_eq!(answers(&mut s), [vec![0x19, 0x7E, 0, 0, 0, 0]], "afresh at another offer");

        let mut m = vec![0x1A, 0x7E, 6, 0, 2, 0, 2];
        m.extend([0xA1, 0x86, 1, 0, 3]);
        m.extend(b"Ann");
        m.extend([9, 60, 200, 100]);
        m.extend([0x01, 0x65, 0xCD, 0x1D, 2]);
        m.extend(b"Bo");
        m.extend([0, 7, 255, 0]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        let want = vec![
            RaidMember { id: 100001, name: "Ann".into(), class: 9, level: 60, hp: 200, power: 100, group: 0, merc: false },
            RaidMember { id: 0x1DCD_6501, name: "Bo".into(), class: 0, level: 7, hp: 255, power: 0, group: 0, merc: false },
        ];
        assert!(matches!(&ev[..], [Event::RaidRoster { total: 6, first: 2, rows }] if *rows == want), "{ev:?}");
        let mut short = m.clone();
        short.pop();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, short);
        assert!(!ev.iter().any(|e| matches!(e, Event::RaidRoster { .. })), "a short one: {ev:?}");

        // an offer of 2 (petition 1687): answered 2, and a group after each record
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x18, 0x7E, 2, 0, 0, 0]);
        s.agree_raid_roster(true);
        assert_eq!(answers(&mut s), [vec![0x19, 0x7E, 2, 0, 0, 0]], "2 to an offer of 2");
        let mut g = vec![0x1A, 0x7E, 6, 0, 2, 0, 2];
        g.extend([0xA1, 0x86, 1, 0, 3]);
        g.extend(b"Ann");
        g.extend([9, 60, 200, 100, 1]);
        g.extend([0x01, 0x65, 0xCD, 0x1D, 2]);
        g.extend(b"Bo");
        g.extend([0, 7, 255, 0, 3]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, g);
        let groups: Vec<u8> = match &ev[..] {
            [Event::RaidRoster { rows, .. }] => rows.iter().map(|r| r.group).collect(),
            _ => panic!("{ev:?}"),
        };
        assert_eq!(groups, [1, 3]);
        // the control: the old records after an offer of 2 asked off read short
        s.agree_raid_roster(false);
        answers(&mut s);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m);
        assert!(matches!(&ev[..], [Event::RaidRoster { rows, .. }] if *rows == want), "asked off, read as before: {ev:?}");
    }

    /// An offer of 3 (go-eqoa-server's raid mercs in the roster, 2026-11-12):
    /// answered 3, and a kind after each record's group, a merc's bit 0. The
    /// control: an offer of 2 is answered 2 still and its records read with no
    /// kind, none a merc.
    #[test]
    fn the_raid_rosters_kinds_are_agreed_and_read() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x18, 0x7E, 3, 0, 0, 0]);
        s.agree_raid_roster(true);
        assert_eq!(answers(&mut s), [vec![0x19, 0x7E, 3, 0, 0, 0]], "3 to an offer of 3");
        let mut k = vec![0x1A, 0x7E, 6, 0, 2, 0, 2];
        k.extend([0xA1, 0x86, 1, 0, 3]);
        k.extend(b"Ann");
        k.extend([9, 60, 200, 100, 1, 0]);
        k.extend([0xEE, 0xFF, 0xC0, 0x00, 2]);
        k.extend(b"Bo");
        k.extend([0, 7, 255, 0, 1, 1]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, k);
        let got: Vec<(u32, u8, bool)> = match &ev[..] {
            [Event::RaidRoster { rows, .. }] => rows.iter().map(|r| (r.id, r.group, r.merc)).collect(),
            _ => panic!("{ev:?}"),
        };
        assert_eq!(got, [(100001, 1, false), (0x00C0_FFEE, 1, true)]);
        // the control: an offer of 2 is answered 2 still, its records read with no kind
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x18, 0x7E, 2, 0, 0, 0]);
        s.agree_raid_roster(true);
        assert_eq!(answers(&mut s), [vec![0x19, 0x7E, 2, 0, 0, 0]], "2 to an offer of 2");
        let mut g2 = vec![0x1A, 0x7E, 6, 0, 2, 0, 2];
        g2.extend([0xA1, 0x86, 1, 0, 3]);
        g2.extend(b"Ann");
        g2.extend([9, 60, 200, 100, 1]);
        g2.extend([0xEE, 0xFF, 0xC0, 0x00, 2]);
        g2.extend(b"Bo");
        g2.extend([0, 7, 255, 0, 1]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, g2);
        assert!(matches!(&ev[..], [Event::RaidRoster { rows, .. }] if rows.len() == 2 && rows.iter().all(|r| !r.merc)), "{ev:?}");
    }

    /// The ready check's window (0x7E49-0x7E4B, 0x7E56): answered only after an
    /// offer, once, and afresh at another; a check read into an event, a
    /// short one not; a reply sent only to a server that offered.
    #[test]
    fn the_ready_check_is_agreed_read_and_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        s.agree_ready(true);
        assert!(!s.send_ready_reply(3, true));
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "no offer: nothing (the control)");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x49, 0x7E, 1]);
        s.agree_ready(true);
        s.agree_ready(true);
        assert_eq!(ours(&mut s), [vec![0x4A, 0x7E, 1]], "once");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x49, 0x7E, 1]);
        s.agree_ready(true);
        assert_eq!(ours(&mut s), [vec![0x4A, 0x7E, 1]], "afresh at another offer");
        assert!(s.send_ready_reply(3, false));
        assert_eq!(ours(&mut s), [vec![0x56, 0x7E, 3, 0, 0, 0, 0]]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4B, 0x7E, 3, 0, 0, 0, 30, 3, b'A', b'n', b'n']);
        assert!(matches!(&ev[..], [Event::ReadyCheck(c)] if c.id == 3 && c.seconds == 30 && c.asker == "Ann"), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4B, 0x7E, 3, 0, 0, 0, 30, 3, b'A']);
        assert!(!ev.iter().any(|e| matches!(e, Event::ReadyCheck(_))), "a short one: {ev:?}");
    }

    /// The raid's windows (0x7E42-0x7E48): answered only after an offer, once,
    /// when the answer changes and afresh at another offer; the state, a roll,
    /// its end and the master looter's list read into events, and a short one
    /// not; a roll's choice and a command sent only to a server that offered.
    #[test]
    fn the_raid_windows_are_agreed_read_and_sent() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let ours = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        ours(&mut s);
        s.agree_raid(true);
        assert!(!s.send_roll_choice(3, crate::raid::Choice::Need) && !s.send_raid_command(crate::raid::Command::Leave));
        assert_eq!(ours(&mut s), Vec::<Vec<u8>>::new(), "no offer: nothing (the control)");
        assert!(!s.raid_windows());
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x42, 0x7E, 1]);
        s.agree_raid(true);
        s.agree_raid(true);
        assert_eq!(ours(&mut s), [vec![0x43, 0x7E, 1]], "once");
        assert!(s.raid_windows());
        s.agree_raid(false);
        assert_eq!(ours(&mut s), [vec![0x43, 0x7E, 0]], "when it changes");
        assert!(!s.raid_windows());
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x42, 0x7E, 1]);
        s.agree_raid(false);
        assert_eq!(ours(&mut s), [vec![0x43, 0x7E, 0]], "afresh at another offer");
        assert!(s.send_roll_choice(3, crate::raid::Choice::Need));
        assert!(s.send_raid_command(crate::raid::Command::Kick(0x1234)));
        assert_eq!(ours(&mut s), [vec![0x46, 0x7E, 3, 0, 0, 0, 1], vec![0x48, 0x7E, 11, 0x34, 0x12, 0, 0]]);

        let mut st = vec![0x44, 0x7E];
        st.extend([1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 3]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, st.clone());
        assert!(matches!(&ev[..], [Event::RaidState(x)] if x.leader == 1 && x.helper == 2 && x.rule == 2 && x.leads()), "{ev:?}");
        st.pop();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, st);
        assert!(!ev.iter().any(|e| matches!(e, Event::RaidState(_))), "a short one: {ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x45, 0x7E, 9, 0, 0, 0, 0, 0]);
        assert!(matches!(&ev[..], [Event::RaidRoll(r)] if r.id == 9 && r.seconds == 0 && r.item.is_none()), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x45, 0x7E, 9, 0, 0, 0, 120, 0, 1]);
        assert!(!ev.iter().any(|e| matches!(e, Event::RaidRoll(_))), "a roll without its item: {ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x47, 0x7E, 0]);
        assert!(matches!(&ev[..], [Event::MasterLoot(None)]), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x47, 0x7E, 1, 44, 1, 1, 5, 0, 0, 0, 2, b'B', b'o']);
        assert!(matches!(&ev[..], [Event::MasterLoot(Some(m))] if m.seconds == 300 && m.members == [(5, "Bo".to_string())]), "{ev:?}");
    }

    // go-eqoa-server's 0x7E01 offer: nothing is asked before it, the answer
    // goes once for each change of mind (1 to have the power, 0 to stop), and
    // a new offer (a world entry) is answered afresh. Its 0x7E02 set replaces
    // the last whole; a short one is not taken; off, or on a new connection,
    // there is none.
    #[test]
    fn power_bars_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x01, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x01, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert!(s.power_bars_offer);
        s.agree_power_bars(true);
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 1, 0, 0, 0])], "asked once");
        s.agree_power_bars(false);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 0, 0, 0, 0])], "and stopped");
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 1, 0, 0, 0])]);

        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x02, 0x7E, 2, 0xA1, 0x86, 1, 0, 63, 0xEA, 3, 0, 0x7F, 255]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!((s.power_of(100001), s.power_of(0x7F0003EA), s.power_of(100002)), (Some(63), Some(255), None));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x02, 0x7E, 2, 0xA1, 0x86, 1, 0, 9]);
        assert_eq!(s.power_of(100001), Some(63), "a short set is not taken");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x02, 0x7E, 1, 0xA2, 0x86, 1, 0, 9]);
        assert_eq!((s.power_of(100001), s.power_of(100002)), (None, Some(9)), "each set is the whole set");

        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x01, 0x7E, 1, 0, 0, 0]);
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 1, 0, 0, 0])], "a new offer, asked again");
        s.agree_power_bars(false);
        assert_eq!(s.power_of(100002), None, "off: the set forgotten");

        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.power_bars_offer && s.power_bars.is_empty(), "a new connection, no offer yet");
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [], "and nothing asked");
    }

    // A GM's map travel (go-eqoa-server's 0x7E34/0x7E35): nothing asked without
    // the offer; offered, the spot sent as the world, x and z; a 0 ends it, and
    // so does a new connection.
    #[test]
    fn map_travel_offered_then_asked() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let asks = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x35, 0x7E])).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        assert!(!s.map_travel_to(0, 25500.0, 15600.0), "not offered");
        assert_eq!(asks(&mut s), Vec::<Vec<u8>>::new(), "nothing asked without the offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x34, 0x7E, 1, 0, 0, 0]);
        assert!(s.map_travel);
        assert!(s.map_travel_to(2, 25500.5, -15600.0));
        let mut want = vec![0x35, 0x7E, 2, 0, 0, 0];
        want.extend_from_slice(&25500.5f32.to_le_bytes());
        want.extend_from_slice(&(-15600.0f32).to_le_bytes());
        assert_eq!(asks(&mut s), [want]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x34, 0x7E, 0, 0, 0, 0]);
        assert!(!s.map_travel && !s.map_travel_to(0, 1.0, 1.0), "a 0 ends it");
        assert_eq!(asks(&mut s), Vec::<Vec<u8>>::new());
    }

    // Quest marks (go-eqoa-server's 0x7E03/0x7E04): nothing asked without an
    // offer; an offer answered once for each change of mind; a set read whole
    // (a giver's "!" with its quest's levels, offer and rewards), a short one
    // not taken, off forgotten.
    #[test]
    fn quest_marks_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x03, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_quest_marks(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x03, 0x7E, 1, 0, 0, 0]);
        s.agree_quest_marks(false);
        assert_eq!(answers(&mut s), [], "the Quest Helper off from the start: nothing answered, as today's");
        s.agree_quest_marks(true);
        s.agree_quest_marks(true);
        assert_eq!(answers(&mut s), [(251, vec![0x03, 0x7E, 1, 0, 0, 0])], "asked once");

        let text = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend_from_slice(t.as_bytes());
        };
        let mut m = vec![0x04, 0x7E, 1, 1, 0];
        m.extend_from_slice(&25403.5f32.to_le_bytes());
        m.extend_from_slice(&15725.5f32.to_le_bytes());
        text(&mut m, "Malsis");
        m.push(1);
        text(&mut m, "The pincers");
        m.extend_from_slice(&[1, 10]);
        text(&mut m, "You there.");
        text(&mut m, "430 XP, Smoldering Aura");
        deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        assert_eq!(
            s.quest_marks,
            [QuestMark {
                kind: 1,
                world: 0,
                x: 25403.5,
                z: 15725.5,
                npc: "Malsis".into(),
                quests: vec![MarkQuest { name: "The pincers".into(), levels: (1, 10), line: "You there.".into(), detail: "430 XP, Smoldering Aura".into() }],
            }]
        );
        deliver(&mut s, drdp::TYPE_GUARANTEED, m[..m.len() - 4].to_vec());
        assert_eq!(s.quest_marks.len(), 1, "a short set is not taken");
        s.agree_quest_marks(false);
        assert_eq!(answers(&mut s), [(251, vec![0x03, 0x7E, 0, 0, 0, 0])], "and stopped");
        assert!(s.quest_marks.is_empty(), "off: the set forgotten");
    }

    // The pet's effects (go-eqoa-server's 0x7E32/0x7E33): nothing asked with
    // no offer (the control); an offer answered at once with what is wanted
    // (0 too, as 0x7E01 is) and again only when that changes; a set read whole
    // and counted down from when it came, an effect over gone from it, a short
    // set not taken; off, stopped and forgotten; a new connection asks nothing
    // until it is offered again.
    #[test]
    fn pet_effects_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x32, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_pet_effects(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x32, 0x7E, 1, 0, 0, 0]);
        s.agree_pet_effects(false);
        assert_eq!(answers(&mut s), [(251, vec![0x32, 0x7E, 0, 0, 0, 0])], "off from the start: told at once, as 0x7E01");
        s.agree_pet_effects(true);
        s.agree_pet_effects(true);
        assert_eq!(answers(&mut s), [(251, vec![0x32, 0x7E, 1, 0, 0, 0])], "asked once");

        let effect = |m: &mut Vec<u8>, icon: u32, left: u32, name: &str| {
            m.extend_from_slice(&icon.to_le_bytes());
            m.extend_from_slice(&left.to_le_bytes());
            m.push(name.len() as u8);
            m.extend_from_slice(name.as_bytes());
        };
        let mut m = vec![0x33, 0x7E, 2];
        effect(&mut m, 0xBDD6_0F01, 1_780_000, "Undead Proc");
        effect(&mut m, 0xBDD6_0F02, 900, "Soul Mastery");
        deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        let want = vec![
            PetEffect { icon: 0xBDD6_0F01, left_ms: 1_780_000, name: "Undead Proc".into() },
            PetEffect { icon: 0xBDD6_0F02, left_ms: 900, name: "Soul Mastery".into() },
        ];
        assert_eq!(s.pet_effects, want);
        let at = s.pet_effects_at;
        assert_eq!(s.pet_effects_left(at), [(0xBDD6_0F01, 1_780_000), (0xBDD6_0F02, 900)], "as it came");
        assert_eq!(s.pet_effects_left(at.wrapping_add(400)), [(0xBDD6_0F01, 1_779_600), (0xBDD6_0F02, 500)], "counted down");
        assert_eq!(s.pet_effects_left(at.wrapping_add(900)), [(0xBDD6_0F01, 1_779_100)], "one over");
        assert_eq!(s.pet_effects_left(at.wrapping_sub(5)), [(0xBDD6_0F01, 1_780_000), (0xBDD6_0F02, 900)], "a clock from before it: no time gone");
        deliver(&mut s, drdp::TYPE_GUARANTEED, m[..m.len() - 3].to_vec());
        assert_eq!(s.pet_effects, want, "a short set is not taken");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x33, 0x7E, 0]);
        assert!(s.pet_effects.is_empty(), "the pet gone: an empty set");
        deliver(&mut s, drdp::TYPE_GUARANTEED, m);
        s.agree_pet_effects(false);
        assert_eq!(answers(&mut s), [(251, vec![0x32, 0x7E, 0, 0, 0, 0])], "and stopped");
        assert!(s.pet_effects.is_empty(), "off: the set forgotten");

        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        s.agree_pet_effects(true);
        assert_eq!(answers(&mut s), [], "a new connection: nothing asked before its offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x32, 0x7E, 1, 0, 0, 0]);
        s.agree_pet_effects(true);
        assert_eq!(answers(&mut s), [(251, vec![0x32, 0x7E, 1, 0, 0, 0])], "its offer answered");
    }

    // The number tags (go-eqoa-server's 0x7E38/0x7E39): nothing asked with no
    // offer (the control); an offer answered at once with what is wanted (0
    // too), a u8, and again only when that changes; a new connection asks
    // nothing until it is offered again.
    #[test]
    fn number_tags_offer_and_answer() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x39, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_number_tags(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        assert!(!s.number_tags_offer);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x38, 0x7E]);
        assert!(!s.number_tags_offer, "a short offer is no offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x38, 0x7E, 1]);
        assert!(s.number_tags_offer);
        s.agree_number_tags(false);
        assert_eq!(answers(&mut s), [(251, vec![0x39, 0x7E, 0])], "every row Default: told at once, 0");
        s.agree_number_tags(true);
        s.agree_number_tags(true);
        assert_eq!(answers(&mut s), [(251, vec![0x39, 0x7E, 1])], "a colour chosen: asked once");
        s.agree_number_tags(false);
        assert_eq!(answers(&mut s), [(251, vec![0x39, 0x7E, 0])], "back to Default: stopped");
        // the offer again (the next world entry): told afresh
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x38, 0x7E, 1]);
        s.agree_number_tags(false);
        assert_eq!(answers(&mut s), [(251, vec![0x39, 0x7E, 0])], "a new offer is answered again");
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.number_tags_offer, "a new connection: no offer");
        s.agree_number_tags(true);
        assert_eq!(answers(&mut s), [], "a new connection: nothing asked before its offer");
    }

    // The number tags' layout (go-eqoa-server's layout 2, the merc's bit): a
    // wanted answer names the layout read, the offer's when older (an offer of
    // 1, every server before, answered 1 as before), the newest this client
    // reads when newer; each a fresh session (the control: the u8 is the
    // offer's, not a constant).
    #[test]
    fn number_tags_answer_names_the_layout() {
        for (offer, want) in [(1u8, 1u8), (2, 2), (9, NUMBER_TAGS_LAYOUT), (0, 1)] {
            let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
            deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x38, 0x7E, offer]);
            s.agree_number_tags(true);
            s.flush();
            let got: Vec<_> = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x39, 0x7E])).collect();
            assert_eq!(got, [(251, vec![0x39, 0x7E, want])], "offered {offer}");
        }
        assert_eq!(NUMBER_TAGS_LAYOUT, 2);
    }

    // The NPCs' body tints (go-eqoa-server's 0x7E3E-0x7E40): an offer is
    // answered at once with a u8 1, each time it comes, and leaves the game the
    // empty set (this client holds none until the server sends them); a set is
    // the session's until the game takes it, and answered with nothing; a new
    // connection leaves the empty set. The controls: an offer cut short is not
    // answered, and a set cut short leaves nothing new.
    // The quest marks over the NPCs' heads (go-eqoa-server's 0x7E61-0x7E63):
    // an offer of layout 1 is the session's, answered as the game agrees
    // (`agree_head_marks`), once a want; the marks read as the server writes
    // them are kept by entity id while asked, and dropped when the client stops.
    // The controls: no answer to a server that made no offer, an offer cut
    // short or of another layout is no offer, marks the client did not ask
    // for are not kept, a new connection holds none.
    #[test]
    fn head_marks_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<Vec<u8>> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x62, 0x7E])).map(|(_, d)| d).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_head_marks(true);
        assert_eq!(answers(&mut s), Vec::<Vec<u8>>::new(), "the control: no offer, nothing told");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x61, 0x7E]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x61, 0x7E, 2]);
        s.agree_head_marks(true);
        assert_eq!(answers(&mut s), Vec::<Vec<u8>>::new(), "a short offer, or another layout, is no offer");
        let marks = vec![0x63, 0x7E, 1, 0, 0xA1, 0x86, 0x01, 0x00, 1];
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x61, 0x7E, 1]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, marks.clone());
        assert_eq!(s.head_mark(100_001), None, "the control: marks not asked for are not kept");
        s.agree_head_marks(true);
        s.agree_head_marks(true);
        assert_eq!(answers(&mut s), [vec![0x62, 0x7E, 1]], "told once");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, marks.clone());
        assert!(ev.is_empty(), "the marks are the session's: {ev:?}");
        assert_eq!((s.head_mark(100_001), s.head_mark(100_002)), (Some(crate::headmarks::TAKE), None));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x63, 0x7E, 1, 0, 0xA1, 0x86]);
        assert_eq!(s.head_mark(100_001), Some(crate::headmarks::TAKE), "a set cut short changes nothing");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x63, 0x7E, 1, 0, 0xA2, 0x86, 0x01, 0x00, 2]);
        assert_eq!((s.head_mark(100_001), s.head_mark(100_002)), (None, Some(crate::headmarks::STEP)), "the whole set each time");
        s.agree_head_marks(false);
        assert_eq!(answers(&mut s), [vec![0x62, 0x7E, 0]]);
        assert_eq!(s.head_mark(100_002), None, "stopped: dropped");
        s.agree_head_marks(true);
        answers(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, marks);
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(s.head_mark(100_001), None, "a new connection: none held");
    }

    #[test]
    fn npc_tints_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x3F, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3E, 0x7E]);
        assert_eq!((answers(&mut s), s.npc_tints.take()), (vec![], None), "a short offer is no offer");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3E, 0x7E, 1]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x3F, 0x7E, 1])], "answered at once, a u8 1");
        assert_eq!(s.npc_tints.take(), Some(vec![]), "what was held dropped");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x40, 0x7E, 1, 0, 8, b'y', b'h', b'a', b'f', b'i', b'r', b'a', b'n', 0xFF, 0x50, 0x8C, 0x55]);
        assert!(ev.is_empty(), "the set is the session's until taken: {ev:?}");
        assert_eq!(s.npc_tints.take(), Some(vec![("yhafiran".to_string(), crate::npctint::Tint::color(0x558C_50FF))]));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x40, 0x7E, 1, 0, 8, b'y', b'h']);
        assert_eq!(s.npc_tints, None, "the control: a set cut short is nothing new");
        assert_eq!(answers(&mut s), [], "the set answered nothing");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3E, 0x7E, 1]);
        assert_eq!(answers(&mut s), [(251, vec![0x3F, 0x7E, 1])], "a new offer is answered again");
        // build180's offer of 2: answered 2, each colour's glow read after it;
        // an offer above it answered 2 too
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3E, 0x7E, 2]);
        assert_eq!(answers(&mut s), [(251, vec![0x3F, 0x7E, 2])], "an offer of 2: answered 2");
        let mut moogie = vec![0x40, 0x7E, 1, 0, 3, b'm', b'o', b'o'];
        moogie.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        moogie.extend_from_slice(&0xFF40_4080u32.to_le_bytes());
        s.npc_tints = None;
        deliver(&mut s, drdp::TYPE_GUARANTEED, moogie);
        assert_eq!(s.npc_tints.take(), Some(vec![("moo".to_string(), crate::npctint::Tint { color: 0xFFFF_FFFF, glow: 0xFF40_4080 })]));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3E, 0x7E, 9]);
        assert_eq!(answers(&mut s), [(251, vec![0x3F, 0x7E, 2])], "a newer offer: the newest this client reads");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3E, 0x7E, 1]);
        answers(&mut s);
        s.npc_tints = None;
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(s.npc_tints.take(), Some(vec![]), "a new connection: none held");
    }

    // A monster's name and level (go-eqoa-server's 0x7E4C-0x7E4E): an offer of
    // layout 1 is answered at once with a u8 1, each time it comes, and passed
    // on as nothing; the message read as the server writes it (go-eqoa-server's
    // TestMobInfoOnTheWire's bytes) is its event, answered with nothing. The
    // controls: an offer cut short or of another layout is not answered (the
    // server's dialogue box stays), and a message cut short or of another layout
    // is no event.
    #[test]
    fn mob_info_offer_answer_and_show() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x4D, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4C, 0x7E]);
        assert_eq!(answers(&mut s), [], "a short offer is no offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4C, 0x7E, 2]);
        assert_eq!(answers(&mut s), [], "another layout is left to the box");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4C, 0x7E, 1]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x4D, 0x7E, 1])], "answered at once, a u8 1");
        let wire = vec![0x4E, 0x7E, 1, 0xA1, 0x86, 0x01, 0x00, 3, 10, b'm', b'o', b's', b's', b' ', b's', b'n', b'a', b'k', b'e'];
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, wire.clone());
        assert!(matches!(&ev[..], [Event::MobInfo { id: 0x186A1, level: 3, name }] if name == "moss snake"), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the message answered nothing");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, wire[..wire.len() - 1].to_vec());
        assert!(ev.is_empty(), "the control: one cut short is nothing: {ev:?}");
        let mut other = wire.clone();
        other[2] = 2;
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, other);
        assert!(ev.is_empty(), "the control: another layout is nothing: {ev:?}");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x4C, 0x7E, 1]);
        assert_eq!(answers(&mut s), [(251, vec![0x4D, 0x7E, 1])], "a new offer is answered again");
    }

    // The damage meter (go-eqoa-server's 0x7E3A-0x7E3D): nothing asked with no
    // offer (the control); an offer answered at once with what is wanted (0
    // too), a u8, and again only when that changes; the fight read as the server
    // writes it (go-eqoa-server's TestDPSMeterOnTheWire's bytes), one of another
    // layout or cut short not taken; a reset sent only to a server that was asked
    // for the meter; a new connection asks nothing until it is offered again.
    #[test]
    fn dps_meter_offer_answer_fight_and_reset() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.len() >= 2 && d[1] == 0x7E && matches!(d[0], 0x3B | 0x3D)).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_dps_meter(true);
        assert!(!s.reset_dps_meter());
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3A, 0x7E]);
        assert!(!s.dps_meter_offer, "a short offer is no offer");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3A, 0x7E, 1]);
        assert!(s.dps_meter_offer);
        s.agree_dps_meter(false);
        assert!(!s.reset_dps_meter(), "not asked for: no reset");
        assert_eq!(answers(&mut s), [(251, vec![0x3B, 0x7E, 0])], "off: told at once, 0");
        s.agree_dps_meter(true);
        s.agree_dps_meter(true);
        assert_eq!(answers(&mut s), [(251, vec![0x3B, 0x7E, 1])], "on: asked once");
        assert!(s.reset_dps_meter());
        assert_eq!(answers(&mut s), [(251, vec![0x3D, 0x7E, 1])], "the reset");

        // a fight of two lines: Dab's own 1000, Jib's 300 (120 its pet's), gone
        // from the group; over, 1.5 s long, number 0x01020304
        let body = [
            1, 4, 3, 2, 1, 1, 0xDC, 5, 0, 0, 0x14, 5, 0, 0, 2, 0, 1, 0, 2, 1, 3, 0xE8, 3, 0, 0, 0, 0, 0, 0, 3, b'D', b'a', b'b', 2, 10, 0x2C, 1, 0, 0, 0x78, 0, 0, 0, 3, b'J', b'i', b'b',
        ];
        let mut m = vec![0x3C, 0x7E];
        m.extend_from_slice(&body);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        let want = DpsMeter {
            fight: 0x0102_0304,
            over: true,
            length_ms: 1500,
            total: 1300,
            count: 2,
            place: 1,
            lines: vec![
                DpsLine { own: true, left: false, class: 3, damage: 1000, pet: 0, name: "Dab".into() },
                DpsLine { own: false, left: true, class: 10, damage: 300, pet: 120, name: "Jib".into() },
            ],
        };
        assert!(matches!(&ev[..], [Event::DpsMeter(got)] if *got == want), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m[..m.len() - 1].to_vec());
        assert!(!ev.iter().any(|e| matches!(e, Event::DpsMeter(_))), "cut short: {ev:?}");
        let mut other = m.clone();
        other[2] = 2;
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, other);
        assert!(!ev.iter().any(|e| matches!(e, Event::DpsMeter(_))), "another layout: {ev:?}");

        // the offer again (the next world entry): told afresh
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x3A, 0x7E, 1]);
        s.agree_dps_meter(true);
        assert_eq!(answers(&mut s), [(251, vec![0x3B, 0x7E, 1])], "a new offer is answered again");
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.dps_meter_offer, "a new connection: no offer");
        s.agree_dps_meter(true);
        assert!(!s.reset_dps_meter());
        assert_eq!(answers(&mut s), [], "a new connection: nothing asked before its offer");
    }

    /// A 0x00DB's tag: one byte after its body from a server that offered
    /// them, its low 6 bits the kind, bit 7 the pet's, bit 6 the merc's; none with no offer
    /// (the control), no byte, a kind of 0, or three bytes (a server's patch's
    /// colour).
    #[test]
    fn a_number_tag_is_one_byte_from_a_server_that_offered() {
        assert_eq!(number_tag(&[0x05], true), Some(NumberTag { kind: 5, pet: false, merc: false }));
        assert_eq!(number_tag(&[0x86], true), Some(NumberTag { kind: 6, pet: true, merc: false }));
        assert_eq!(number_tag(&[0x81], true), Some(NumberTag { kind: 1, pet: true, merc: false }));
        assert_eq!(number_tag(&[0x09], true), Some(NumberTag { kind: 9, pet: false, merc: false }), "a kind a later server sends");
        // layout 2: bit 6 the merc's
        assert_eq!(number_tag(&[0x41], true), Some(NumberTag { kind: 1, pet: false, merc: true }), "the merc's swing");
        assert_eq!(number_tag(&[0x46], true), Some(NumberTag { kind: 6, pet: false, merc: true }), "the merc's heal");
        assert_eq!(number_tag(&[0x40], true), None, "the merc's bit alone");
        assert_eq!(number_tag(&[0x05], false), None, "no offer: a stray byte is no tag");
        assert_eq!(number_tag(&[], true), None);
        assert_eq!(number_tag(&[0x00], true), None);
        assert_eq!(number_tag(&[0x80], true), None, "the pet's bit alone");
        assert_eq!(number_tag(&[255, 128, 0], true), None, "three bytes are a patch's colour");
    }

    /// The quest tracker's offer is answered once, its body read whole (the
    /// counts, a goal done only counted and all there), a short one not
    /// taken (the control), and off it is stopped and forgotten.
    #[test]
    fn quest_progress_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x05, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_quest_progress(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x05, 0x7E, 1, 0, 0, 0]);
        s.agree_quest_progress(false);
        assert_eq!(answers(&mut s), [], "the Quest Helper off from the start: nothing answered");
        s.agree_quest_progress(true);
        s.agree_quest_progress(true);
        assert_eq!(answers(&mut s), [(251, vec![0x05, 0x7E, 1, 0, 0, 0])], "asked once");
        let text = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend_from_slice(t.as_bytes());
        };
        let mut m = vec![0x06, 0x7E, 1];
        text(&mut m, "Bat eyes");
        m.push(2);
        text(&mut m, "Bat Eye");
        m.extend_from_slice(&[1, 0, 3, 0]);
        text(&mut m, "Give to Kellina");
        m.extend_from_slice(&[0, 0, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        let want = [TrackedQuest { name: "Bat eyes".into(), goals: vec![QuestGoal { text: "Bat Eye".into(), have: 1, need: 3 }, QuestGoal { text: "Give to Kellina".into(), have: 0, need: 0 }] }];
        assert_eq!(s.quest_progress, want);
        assert!(!s.quest_progress[0].goals[0].done() && !s.quest_progress[0].goals[1].done(), "1 of 3, and nothing counted");
        assert!(QuestGoal { text: String::new(), have: 3, need: 3 }.done());
        deliver(&mut s, drdp::TYPE_GUARANTEED, m[..m.len() - 2].to_vec());
        assert_eq!(s.quest_progress, want, "a short one is not taken");
        s.agree_quest_progress(false);
        assert_eq!(answers(&mut s), [(251, vec![0x05, 0x7E, 0, 0, 0, 0])], "and stopped");
        assert!(s.quest_progress.is_empty(), "off: forgotten");
    }

    // go-eqoa-server's offer of the live game's look (0x7E0C) is answered
    // with 1 at once; the server's answer (0x7E0D) decides how the character
    // lists after it are read: the disc's 32 raw bytes a record until it says
    // 1, the live game's 71 after, the disc's again after a 0. It is kept
    // through the world's connection, as the server keeps it for the
    // session. The control: a list that comes before the server's answer is
    // the disc's, though this client asked already.
    /// go-eqoa-server's armour (`LIVE_LOOK_ARMOR`): a look in force whose
    /// third u32 has bit 2 puts it in force, and a list is then read with 95
    /// bytes of gear a record (the live game's 71 and six trim colours); a
    /// look in force of two u32s (an older server) or a third of 0 leaves it
    /// off and the records 71. The disc's look never has it.
    #[test]
    fn armor_look_in_force_and_lists() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.state = State::Shard;
        let list = |gear: usize| {
            let mut w = Writer::op(0x002C);
            w.signed(1).u32(4).bytes(b"Gild");
            for v in [17, 0x70D8_98C6, 2, 0, 60, 1, 2, 3, 2] {
                w.signed(v);
            }
            let mut g = vec![0u8; gear];
            if gear > LIVE_GEAR_SIZE {
                g[LIVE_GEAR_SIZE..LIVE_GEAR_SIZE + 4].copy_from_slice(&[0xD1, 0, 0, 0xFF]);
            }
            w.bytes(&g);
            w.data
        };
        let gear_of = |ev: &[Event]| match ev {
            [Event::Characters(c)] => c.iter().map(|c| c.gear.clone()).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        let in_force = |s: &mut Session, words: &[u32]| {
            let mut d = vec![0x0D, 0x7E];
            for w in words {
                d.extend(w.to_le_bytes());
            }
            deliver(s, drdp::TYPE_GUARANTEED, d);
        };
        in_force(&mut s, &[1, 8]);
        assert!(s.live_look && !s.armor_look, "an older server's two words");
        assert_eq!(gear_of(&deliver(&mut s, drdp::TYPE_GUARANTEED, list(LIVE_GEAR_SIZE)))[0].len(), LIVE_GEAR_SIZE);
        in_force(&mut s, &[1, 8, 0]);
        assert!(!s.armor_look, "a third word of 0");
        in_force(&mut s, &[1, 8, LIVE_LOOK_ARMOR]);
        assert!(s.armor_look);
        let g = gear_of(&deliver(&mut s, drdp::TYPE_GUARANTEED, list(ARMOR_GEAR_SIZE)));
        assert_eq!(g[0].len(), ARMOR_GEAR_SIZE);
        assert_eq!(&g[0][LIVE_GEAR_SIZE..LIVE_GEAR_SIZE + 4], &[0xD1, 0, 0, 0xFF], "the chest's trim");
        in_force(&mut s, &[0, 8, LIVE_LOOK_ARMOR]);
        assert!(!s.live_look && !s.armor_look, "the disc's look has none");
    }

    #[test]
    fn live_look_offer_answer_and_lists() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        let list = |gear: usize| {
            let mut w = Writer::op(0x002C);
            w.signed(1).u32(4).bytes(b"Dyed");
            for v in [17, 0x70D8_98C6, 13, 3, 42, 1, 2, 3, 2] {
                w.signed(v);
            }
            w.bytes(&vec![0xAB; gear]);
            w.data
        };
        let gear_of = |ev: &[Event]| match ev {
            [Event::Characters(c)] => c.iter().map(|c| c.gear.len()).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        answers(&mut s);
        s.state = State::Shard;
        assert_eq!(gear_of(&deliver(&mut s, drdp::TYPE_GUARANTEED, list(DISC_GEAR_SIZE))), [32], "a first list: the disc's");
        assert!(deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0C, 0x7E, 1, 0, 0, 0]).is_empty());
        let bits = if wants_live_look() { 1 | LIVE_LOOK_ARMOR as u8 } else { 0 } | LIVE_LOOK_MANY_CHARS as u8;
        assert_eq!(answers(&mut s), [(251, vec![0x0C, 0x7E, bits, 0, 0, 0])], "the offer answered with 1, every character listed (2) and the server's armour drawn (4)");
        assert!(!s.live_look, "not in force until the server says so");
        assert_eq!(gear_of(&deliver(&mut s, drdp::TYPE_GUARANTEED, list(DISC_GEAR_SIZE))), [32], "the control: a list before the server's answer");

        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0D, 0x7E, 1, 0, 0, 0]);
        assert!(s.live_look);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, list(LIVE_GEAR_SIZE));
        assert_eq!(gear_of(&ev), [71], "the live game's list");
        if let [Event::Characters(c)] = &ev[..] {
            assert_eq!((c[0].name.as_str(), c[0].level, c[0].face), ("Dyed", 42, 2));
        }
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(s.live_look, "kept through the world's connection");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0D, 0x7E, 0, 0, 0, 0]);
        assert!(!s.live_look, "the server went back to the disc's");
        assert_eq!(answers(&mut s), [], "nothing answered but the offer");
    }

    // go-eqoa-server's offer of the GM's trigger names (0x7E07) is answered
    // with 2 (every trigger) at once, every time it comes, with this client's word
    // that it flies (0x7E1B), and its whole list (0x7E36) and an older server's
    // names (0x7E08) are passed on for the TEST menu with nothing answered. The
    // controls: an offer cut short and an offer of 0 are not answered.
    #[test]
    fn the_trigger_names_offer_is_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        let answer = [(251, vec![0x07, 0x7E, 2, 0, 0, 0]), (251, vec![0x1B, 0x7E, 1, 0, 0, 0])];
        assert_eq!(answers(&mut s), answer);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), answer, "again for another offer");
        let names = vec![0x08, 0x7E, 1, 2, 1, 3, b'O', b'n', b'e'];
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, names);
        assert!(matches!(&ev[..], [Event::Opcode { op: 0x7E08, data }] if data[..] == [1, 2, 1, 3, b'O', b'n', b'e']), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the names answered nothing");
        let list = vec![0x36, 0x7E, 1, 0, 140, 0, 0, 0, 1, 3, b'O', b'n', b'e'];
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, list);
        assert!(matches!(&ev[..], [Event::Opcode { op: 0x7E36, data }] if data[..] == [1, 0, 140, 0, 0, 0, 1, 3, b'O', b'n', b'e']), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the list answered nothing");
    }

    // A race's own words (0x7E1C): the offer answered with 1 at once, every time
    // it comes, and the list (0x7E1D) passed on for `racelabel` with nothing
    // answered. The controls: an offer cut short and an offer of 0 are not answered,
    // and a session never offered one says nothing.
    #[test]
    fn the_race_labels_offer_is_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        assert_eq!(answers(&mut s), [], "never offered");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1C, 0x7E, 1, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1C, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1C, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x1C, 0x7E, 1, 0, 0, 0])]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1C, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), [(251, vec![0x1C, 0x7E, 1, 0, 0, 0])], "again for another offer");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1D, 0x7E, 0]);
        assert!(matches!(&ev[..], [Event::Opcode { op: OP_RACE_LABELS_DATA, data }] if data[..] == [0]), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the list answered nothing");
    }

    // Creation's races (0x7E1E): the offer answered with 1 at once, and the races
    // (0x7E1F) kept on the session for the create pages, answering nothing and
    // passing nothing on. The controls: a session never offered one says nothing
    // and keeps none; an offer cut short or of 0 is not answered; races that do
    // not read leave what was kept.
    #[test]
    fn the_creation_offer_is_answered_and_its_races_kept() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        assert_eq!((answers(&mut s), s.creation.is_none()), (vec![], true), "never offered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1E, 0x7E, 1, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1E, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1E, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x1E, 0x7E, 1, 0, 0, 0])]);
        let mut races = vec![0x1F, 0x7E, 1, 1, 1, 13, 2, 0, 0, 5, 10, 15, 0, 5, 0, 0x49, 0x7E, 0xE0, 0x08, 0x13, 0x6C, 0, 0, 0, 0];
        races.extend([6, b'G', b'o', b'b', b'l', b'i', b'n', 3, b'G', b'O', b'B', 4, 0, b'S', b'l', b'y', b'.']);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, races);
        assert!(ev.is_empty(), "the races are the session's: {ev:?}");
        assert_eq!(answers(&mut s), [], "the races answered nothing");
        let c = s.creation.clone().expect("kept");
        assert!(c.all_classes);
        let g = c.race(13).expect("the goblin");
        assert_eq!((g.like, g.class_mask, g.body_for(1), g.name.as_str(), g.desc.as_str()), (2, 0x7E49, 0x6C13_08E0, "Goblin", "Sly."));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x1F, 0x7E, 1, 0, 1, 13]);
        assert_eq!(s.creation, Some(c), "the control: races cut short leave what was kept");
    }

    fn ready_builder(w: &[crate::serverworlds::World]) -> std::sync::mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(w.iter().filter(|w| w.name != "not here").map(|w| w.id).collect()).unwrap();
        rx
    }

    // The server's worlds (0x7E30): with no builder the offer is not taken; with
    // one it is answered with 1, the list (0x7E31) kept and built, and the server
    // told which are held, once, then nothing more. The controls: no builder, an
    // offer cut short, an offer of 0, a list cut short (kept as before, nothing
    // told); a world the builder could not make is left out of the answer.
    #[test]
    fn the_server_worlds_are_built_and_the_server_told_which() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.pump();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        if WORLD_BUILDER.get().is_none() {
            deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x30, 0x7E, 1, 0, 0, 0]);
            assert_eq!((answers(&mut s), s.server_worlds_pending()), (vec![], false), "the control: no builder, no answer");
        }
        set_world_builder(ready_builder);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x30, 0x7E, 1, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x30, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x30, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x30, 0x7E, 1, 0, 0, 0])]);
        assert!(s.server_worlds_pending(), "taken, not yet answered");
        let w = |id: u8, name: &str| crate::serverworlds::World { id, name: name.into(), size: 1, crc: 2, parts: vec![] };
        let list = crate::serverworlds::encode(&[w(10, "the Homelands"), w(11, "not here")]);
        let mut cut = vec![0x31, 0x7E];
        cut.extend(&list[..list.len() - 1]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, cut);
        assert_eq!((answers(&mut s), s.server_worlds.is_none(), s.server_worlds_pending()), (vec![], true, true), "the control: a list cut short");
        let mut msg = vec![0x31, 0x7E];
        msg.extend(&list);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg);
        assert!(ev.is_empty(), "the list is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x31, 0x7E, 1, 10])], "only the world built");
        assert_eq!((s.server_worlds.as_ref().map(Vec::len), s.server_worlds_pending()), (Some(2), false));
        assert_eq!(answers(&mut s), [], "told once");
    }

    // Which characters a login needs the server's worlds for (0x7E54): kept
    // whole each time, a character it names waits and one it does not need
    // not, and nothing is answered. The controls: a server that never said
    // (None, not "none of them"); a message cut short or of another version
    // leaves what was known.
    #[test]
    fn the_server_says_which_characters_need_its_worlds() {
        let mut s = session();
        s.record = Some(Vec::new());
        assert_eq!(s.pick_needs_worlds(630), None, "the control: never said");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x54, 0x7E, 1, 2, 0x76, 0x02, 0, 0, 4, 3, 2, 1]);
        assert!(ev.is_empty(), "the set is the session's: {ev:?}");
        let needs = |s: &Session| [630, 0x0102_0304, 558, -1, 1 << 40].map(|id| s.pick_needs_worlds(id));
        assert_eq!(needs(&s), [Some(true), Some(true), Some(false), Some(false), Some(false)]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x54, 0x7E, 1, 2, 0x76, 0x02, 0, 0, 4]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x54, 0x7E, 2, 0]);
        assert_eq!(needs(&s)[..3], [Some(true), Some(true), Some(false)], "the controls: cut short, another version");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x54, 0x7E, 1, 1, 0x2E, 0x02, 0, 0]);
        assert_eq!(needs(&s)[..3], [Some(false), Some(false), Some(true)], "the whole set each time");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x54, 0x7E, 1, 0]);
        assert_eq!(needs(&s)[..3], [Some(false); 3], "none of them: said, and no character waits");
        assert_eq!(s.record.as_deref(), Some(&[][..]), "nothing answered");
    }

    // The server's worlds taken and a character gone in before their list
    // was in (one that did not wait for them: the promotion empties the queue
    // the list waited in): the new game connection asks for them again (0x7E30
    // 1), and the list that then comes is built and answered as ever. The
    // controls: a session never offered them asks nothing at a promotion; one
    // whose list was in and being built is not sent it twice; one that
    // answered asks nothing more.
    #[test]
    fn the_worlds_are_asked_for_again_when_a_character_went_in_before_their_list() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.pump();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        set_world_builder(ready_builder);
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [], "the control: never offered, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x30, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), [(251, vec![0x30, 0x7E, 1, 0, 0, 0])], "the offer taken");
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [(251, vec![0x30, 0x7E, 1, 0, 0, 0])], "in the world before the list: asked again");
        assert!(s.server_worlds_pending());
        let w = crate::serverworlds::World { id: 10, name: "the Homelands".into(), size: 1, crc: 2, parts: vec![] };
        let mut list = vec![0x31, 0x7E];
        list.extend(crate::serverworlds::encode(std::slice::from_ref(&w)));
        // the list in and its build not yet answered: a promotion now asks nothing
        deliver(&mut s, drdp::TYPE_GUARANTEED, list.clone());
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [(251, vec![0x31, 0x7E, 1, 10])], "the control: the list was in; built and answered, not asked for twice");
        assert!(!s.server_worlds_pending());
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [], "the control: answered, nothing more asked");
    }

    // 0x002A carries the race's options re-chosen at Customize (a game race's
    // skin) after its five ints, as 0x002B carries a creation's, to a server
    // that sent the races' options. The controls: none chosen, the five ints
    // alone; chosen but no options from the server (a PS2's server, Sandstorm),
    // the five ints alone.
    #[test]
    fn select_carries_the_options_rechosen() {
        let mut s = session();
        s.record = Some(Vec::new());
        let sent = |s: &mut Session| -> Vec<Vec<u8>> {
            s.pump();
            s.record.replace(Vec::new()).unwrap().into_iter().map(|(_, d)| d).filter(|d| d.starts_with(&[0x2A, 0x00])).collect()
        };
        let mut c = Character { server_id: 558, face: 1, hair_style: 2, hair_length: 3, hair_color: 4, ..Default::default() };
        let ints = |v: &[u32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let five = [vec![0x2A, 0x00], ints(&[558, 1, 2, 3, 4])].concat();
        c.options = vec![("skin".into(), 8)];
        s.select_character(&c);
        assert_eq!(sent(&mut s), [five.clone()], "the control: no options from the server");
        s.race_options = Some(crate::raceopts::Options::default());
        s.select_character(&Character { options: Vec::new(), ..c.clone() });
        assert_eq!(sent(&mut s), [five.clone()], "the control: none chosen");
        s.select_character(&c);
        assert_eq!(sent(&mut s), [[five, vec![b'R', 1, 1, 4, b's', b'k', b'i', b'n', 8]].concat()]);
    }

    // The worlds held, told the server after a character was played and before
    // the game connection opened (their check ended a moment after the pick):
    // the server has left the lobby connection by then and never reads it, so
    // the new connection tells it again, once. The controls: told before the
    // pick, nothing is told again at the promotion; a later promotion tells
    // nothing more.
    #[test]
    fn the_worlds_held_are_told_again_when_the_answer_went_behind_the_pick() {
        let mut s = session();
        s.record = Some(Vec::new());
        // what the session sent of ours since the last look (its record: the
        // wire holds a message back while those before it are unacknowledged)
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.pump();
            s.record.replace(Vec::new()).unwrap().into_iter().filter(|(_, d)| d.get(1) == Some(&0x7E)).collect()
        };
        let c = Character { server_id: 558, ..Default::default() };
        let building = |s: &mut Session| {
            let (tx, rx) = std::sync::mpsc::channel();
            s.worlds_building = Some(rx);
            s.worlds_pending = true;
            tx
        };
        // the control: answered, then the pick, then the promotion
        let tx = building(&mut s);
        tx.send(vec![10, 11]).unwrap();
        assert_eq!(answers(&mut s), [(251, vec![0x31, 0x7E, 2, 10, 11])]);
        s.select_character(&c);
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [], "the control: told before the pick, not told again");
        // the pick, then the answer, then the promotion
        let tx = building(&mut s);
        s.select_character(&c);
        assert_eq!(answers(&mut s), [], "still being checked");
        tx.send(vec![10, 11]).unwrap();
        assert_eq!(answers(&mut s), [(251, vec![0x31, 0x7E, 2, 10, 11])], "told on the lobby connection");
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [(251, vec![0x31, 0x7E, 2, 10, 11])], "told again on the game connection");
        assert!(!s.server_worlds_pending());
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(answers(&mut s), [], "the control: once");
    }

    // A custom race's own options (0x7E22): the offer answered with 1 at once,
    // the options (0x7E23) kept on the session for the create pages and the
    // bodies drawn, the looks (0x7E24) passed on whole for the game to keep. The
    // controls: a session never offered says nothing and keeps none; an offer cut
    // short or of 0 is not answered; options that do not read leave what was kept.
    #[test]
    fn the_race_options_offer_is_answered_and_its_lists_kept() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        assert_eq!((answers(&mut s), s.race_options.is_none()), (vec![], true), "never offered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x22, 0x7E, 1, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x22, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x22, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x22, 0x7E, 3, 0, 0, 0])], "3: a game race's rows shown under the four, a barbarian's tattoo drawn");
        // a race of one body row, two steps
        let mut options = vec![0x23, 0x7E, 1, 1, 13, 1, 4, b'b', b'o', b'd', b'y', 5, b'T', b'r', b'i', b'b', b'e', 1, 0, 2];
        options.extend([1, b'G']);
        options.extend(0x6C13_08E0u32.to_le_bytes());
        options.extend(1000u16.to_le_bytes());
        options.extend([1, b'I']);
        options.extend(0xE961_79D4u32.to_le_bytes());
        options.extend(875u16.to_le_bytes());
        options.push(0);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, options);
        assert!(ev.is_empty(), "the options are the session's: {ev:?}");
        let o = s.race_options.clone().expect("kept");
        assert_eq!(o.rows(13)[0].steps(0).iter().map(|s| s.value).collect::<Vec<_>>(), vec![0x6C13_08E0, 0xE961_79D4]);
        assert_eq!(o.body_size(13, 0xE961_79D4), 0.875);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x23, 0x7E, 1, 1, 13]);
        assert_eq!(s.race_options, Some(o), "the control: options cut short leave what was kept");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x24, 0x7E, 1, 0, 0]);
        assert!(ev.is_empty(), "the looks are the session's until taken: {ev:?}");
        assert_eq!(s.race_looks.take(), Some(vec![1, 0, 0]));
        assert_eq!(answers(&mut s), [], "the lists answered nothing");
    }

    // go-eqoa-server's offer of the live duel's messages (0x7E09) is answered
    // with 1 at once, every time it comes, and passed on as nothing. The
    // controls: an offer cut short and an offer of 0 are not answered.
    #[test]
    fn the_duel_window_offer_is_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 1, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x09, 0x7E, 1, 0, 0, 0])]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), [(251, vec![0x09, 0x7E, 1, 0, 0, 0])], "again for another offer (each world entry)");
    }

    // The combat record (0x7E20): the offer answered with 1 at once and kept, so the
    // record may be asked for (0x7E21, a u32 1); the record itself passed on for
    // `record`, answered with nothing. The controls: an offer cut short and an offer
    // of 0 are neither answered nor kept, and a session never offered one asks nothing.
    #[test]
    fn the_combat_record_offer_is_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        assert!(!s.ask_combat_record(), "never offered: no ask");
        assert_eq!(answers(&mut s), [], "never offered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x20, 0x7E, 1, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x20, 0x7E, 0, 0, 0, 0]);
        assert!(!s.combat_record, "the controls: cut short, or 0");
        assert_eq!(answers(&mut s), []);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x20, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty() && s.combat_record, "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x20, 0x7E, 1, 0, 0, 0])]);
        assert!(s.ask_combat_record());
        assert_eq!(answers(&mut s), [(251, vec![0x21, 0x7E, 1, 0, 0, 0])]);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x21, 0x7E, 1, 0, 0]);
        assert!(matches!(&ev[..], [Event::Opcode { op: OP_COMBAT_RECORD_DATA, data }] if data[..] == [1, 0, 0]), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the record answered nothing");
    }

    // Capes (0x7E25): the offer answered with 1 at once and kept; then the list's
    // capes (0x7E27, by server id) and those in view (0x7E26, by entity id) kept,
    // each message the whole of it, a style of 0 left out and a count of each.
    // The controls: an offer cut short or of 0 is neither answered nor kept, a
    // session never offered capes keeps no list sent to it anyway, and a list cut
    // short changes nothing.
    // The select info's offer (0x7E67) is answered 1 at once and its list
    // (0x7E68) kept by server id, each message the whole of it. The controls: an
    // offer cut short or of 0 is not answered, and a list that does not read
    // leaves the one before.
    #[test]
    fn the_select_info_is_answered_and_kept() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x67, 0x7E, 1, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x67, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the controls: cut short, or 0");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x67, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), [(251, vec![0x67, 0x7E, 1, 0, 0, 0])]);
        let mut m = OP_SELECT_INFO_DATA.to_le_bytes().to_vec();
        m.extend(crate::selectinfo::tests::sample());
        deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        assert_eq!(s.select_info.len(), 3);
        assert_eq!(s.select_info.get(&8).map(|i| (i.race, i.race_name.as_str(), i.cms)), Some((24, "Ratman", 900)));
        assert_eq!(s.select_info.get(&9).map(|i| i.rows.clone()), Some(vec!["wispcolor".to_string()]));
        m.truncate(m.len() - 1);
        deliver(&mut s, drdp::TYPE_GUARANTEED, m);
        assert_eq!(s.select_info.len(), 3, "the control: one cut short leaves the last");
    }

    #[test]
    fn the_cape_offer_is_answered_and_its_lists_kept() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        let list = |op: u16, entries: &[(u32, u8, u32)]| {
            let mut m = op.to_le_bytes().to_vec();
            m.push(entries.len() as u8);
            for &(id, style, color) in entries {
                m.extend_from_slice(&id.to_le_bytes());
                m.push(style);
                m.extend_from_slice(&color.to_le_bytes());
            }
            m
        };
        answers(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, list(OP_CAPES_IN_VIEW, &[(5, 1, 0)]));
        assert!(s.capes_in_view.is_empty() && s.capes_changed == 0, "the control: never offered, nothing kept");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x25, 0x7E, 1, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x25, 0x7E, 0, 0, 0, 0]);
        assert!(!s.capes, "the controls: cut short, or 0");
        assert_eq!(answers(&mut s), []);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x25, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty() && s.capes, "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x25, 0x7E, 1, 0, 0, 0])]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, list(OP_CAPES_LIST, &[(1234, 3, 0xFF0000FF), (99, 0, 0)]));
        assert_eq!(s.cape_list.get(&1234), Some(&crate::cape::Cape { style: 3, color: 0xFF0000FF }));
        assert_eq!((s.cape_list.len(), s.capes_changed), (1, 1), "a style of 0 wears none");
        deliver(&mut s, drdp::TYPE_GUARANTEED, list(OP_CAPES_IN_VIEW, &[(0x7F00_0001, 2, 0), (40, 1, 0)]));
        assert_eq!((s.capes_in_view.len(), s.capes_changed), (2, 2));
        let mut short = list(OP_CAPES_IN_VIEW, &[(40, 4, 0)]);
        short.pop();
        deliver(&mut s, drdp::TYPE_GUARANTEED, short);
        assert_eq!((s.capes_in_view.len(), s.capes_changed), (2, 2), "the control: cut short, the set as it was");
        deliver(&mut s, drdp::TYPE_GUARANTEED, list(OP_CAPES_IN_VIEW, &[]));
        assert!(s.capes_in_view.is_empty(), "the whole set each time: none now");
        assert_eq!(s.cape_list.len(), 1, "the list kept apart");
        assert_eq!(answers(&mut s), [], "the lists answered nothing");
    }

    // A pack and a bank past 40 (0x7E29): the offer answered at once with what
    // this client holds of each (the offer's, between 40 and 255), the sizes
    // taken only from the server's word before a dump (0x7E2A). The controls:
    // an offer cut short or of 0 is not answered, a session never told keeps
    // CLIENT's 40, and a word cut short changes nothing.
    #[test]
    fn the_item_lists_offer_is_answered_and_its_sizes_kept() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        let words = |op: u16, w: &[u32]| {
            let mut m = op.to_le_bytes().to_vec();
            for v in w {
                m.extend_from_slice(&v.to_le_bytes());
            }
            m
        };
        answers(&mut s);
        assert_eq!(s.item_lists, [40, 40], "the control: never told, CLIENT's");
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_ITEM_LISTS, &[1, 100]));
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_ITEM_LISTS, &[0, 100, 100]));
        assert_eq!(answers(&mut s), [], "the controls: cut short, or 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_ITEM_LISTS, &[1, 100, 1000]));
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, words(OP_ITEM_LISTS, &[1, 100, 255]))]);
        assert_eq!(s.item_lists, [40, 40], "an answer is not the sizes: the server's word is");
        let mut short = words(OP_ITEM_LISTS_IN_FORCE, &[100, 100]);
        short.pop();
        deliver(&mut s, drdp::TYPE_GUARANTEED, short);
        assert_eq!(s.item_lists, [40, 40], "the control: cut short, as it was");
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_ITEM_LISTS_IN_FORCE, &[100, 100]));
        assert_eq!(s.item_lists, [100, 100]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_ITEM_LISTS_IN_FORCE, &[12, 9000]));
        assert_eq!(s.item_lists, [40, 255], "never under CLIENT's, never past a byte's index");
        assert_eq!(answers(&mut s), [], "the sizes answered nothing");
    }

    // The live look's offer (0x7E0C) answered with bit 1 too: this client's
    // character select lists every character. The look in force's second u32
    // is the characters listed (`char_slots`), kept between 1 and 64; a server
    // that sends the first alone leaves CLIENT's 8, and a list is read to
    // `char_slots` (at least 8), the rest of a longer one unread.
    #[test]
    fn the_live_look_answer_asks_for_every_character() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        let words = |op: u16, w: &[u32]| {
            let mut m = op.to_le_bytes().to_vec();
            for v in w {
                m.extend_from_slice(&v.to_le_bytes());
            }
            m
        };
        answers(&mut s);
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_LIVE_LOOK, &[1]));
        let want = if wants_live_look() { 1 | LIVE_LOOK_ARMOR | 2 } else { 2 };
        assert_eq!(answers(&mut s), [(251, words(OP_LIVE_LOOK, &[want]))]);
        assert_eq!(s.char_slots, CLIENT_CHAR_SLOTS);
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_LIVE_LOOK_IN_FORCE, &[1]));
        assert_eq!((s.live_look, s.char_slots), (true, 8), "the control: an older server's word");
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_LIVE_LOOK_IN_FORCE, &[0, 20]));
        assert_eq!((s.live_look, s.char_slots), (false, 20));
        deliver(&mut s, drdp::TYPE_GUARANTEED, words(OP_LIVE_LOOK_IN_FORCE, &[0, 9000]));
        assert_eq!(s.char_slots, CHAR_SLOTS_MOST);
        // a list of twelve: read whole at 20, eight of it at CLIENT's
        let mut list = Writer::op(0x002C);
        list.signed(12);
        for i in 0..12 {
            list.string(format!("C{i}").as_bytes());
            for v in [i as i64 + 1, 0, 0, 0, 1, 0, 0, 0, 0] {
                list.signed(v);
            }
            list.bytes(&[0; DISC_GEAR_SIZE]);
        }
        for (slots, n) in [(20, 12), (8, 8)] {
            s.char_slots = slots;
            let mut r = Reader::new(&list.data[2..]);
            assert_eq!(read_characters(&mut r, false, false, s.char_slots.max(CLIENT_CHAR_SLOTS)).len(), n);
        }
    }

    // A state channel's update that comes before the world entry (live, a
    // moment before the dump) is the game's once it is in the world: the
    // session keeps it and hands it out right after the WorldEntry, in
    // arrival order. The screens before the world read no channel, and a
    // server sends one again only when it changes, so a dropped one left the
    // player's own body, compass and bars out until then (1 playshot in 4 on a
    // test server). The controls: in the world an update comes out at once,
    // and a second world entry hands out nothing kept.
    #[test]
    fn state_before_the_world_entry_comes_right_after_it() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let states = |ev: &[Event]| -> Vec<(u8, Vec<u8>)> { ev.iter().filter_map(|e| if let Event::State { channel, data } = e { Some((*channel, data.clone())) } else { None }).collect() };
        let enter = |s: &mut Session| {
            let mut ev = Vec::new();
            s.route(drdp::Message { ty: drdp::TYPE_GUARANTEED, data: empty_dump([0.0; 3], 0.0), connection: s.conn, src: 0x1d3d_8fa9, ..Default::default() }, &mut ev);
            ev
        };
        assert!(deliver(&mut s, 0x00, vec![1; 4]).is_empty(), "before the world entry: kept");
        assert!(deliver(&mut s, 0x42, vec![2; 4]).is_empty());
        assert!(deliver(&mut s, 0x00, vec![3; 4]).is_empty());
        let ev = enter(&mut s);
        assert!(matches!(ev.first(), Some(Event::WorldEntry { self_id: 0x1d3d_8fa9, .. })), "{ev:?}");
        assert_eq!(ev.len(), 4, "{ev:?}");
        assert_eq!(states(&ev[1..]), [(0x00, vec![1; 4]), (0x42, vec![2; 4]), (0x00, vec![3; 4])], "right after it, in order");
        assert_eq!(states(&deliver(&mut s, 0x00, vec![4; 4])), [(0x00, vec![4; 4])], "the control: in the world, at once");
        let ev = enter(&mut s);
        assert!(matches!(ev[..], [Event::WorldEntry { .. }]), "the control: a second world entry, nothing kept: {ev:?}");
    }

    // A new game connection (TYPE_CONNECT: the world resets) forgets the old
    // one's channels; the control is the test above.
    #[test]
    fn a_new_game_connection_forgets_the_kept_states() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        assert!(deliver(&mut s, 0x00, vec![1; 4]).is_empty());
        let ev = deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(matches!(ev[..], [Event::Promoted]), "{ev:?}");
        let mut ev = Vec::new();
        s.route(drdp::Message { ty: drdp::TYPE_GUARANTEED, data: empty_dump([0.0; 3], 0.0), connection: s.conn, src: 0x1d3d_8fa9, ..Default::default() }, &mut ev);
        assert!(matches!(ev[..], [Event::WorldEntry { .. }]), "{ev:?}");
    }

    // Msg000D's world entry is ONE datagram, as every PS2 capture's is: 0x0014,
    // GroupChanged's 0x000E with the chat mode kept, 0x00C9 and 0x00CA, and the
    // first frame's 0x40 at the dump's place. Sandstorm needs the 0x40 there:
    // sent in a later packet, the player is left out of its world (no entity
    // but its own, and it never moves). The control: a dump cut short sends
    // 0x00C9 and 0x00CA alone (Msg000D's Msg0777 path), and no 0x40.
    #[test]
    fn world_entry_carries_the_first_self_state() {
        let pos = [25296.398, 54.125977, 15785.668];
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.flush();
        s.send_op_byte(0x000E, 3);
        s.flush();
        sent.borrow_mut().clear();
        let dump = empty_dump(pos, 3.5588355);
        let mut ev = Vec::new();
        s.route(drdp::Message { ty: drdp::TYPE_GUARANTEED, data: dump.clone(), connection: s.conn, src: 0x1d3d_8fa9, ..Default::default() }, &mut ev);
        assert!(matches!(ev[..], [Event::WorldEntry { self_id: 0x1d3d_8fa9, .. }]), "{ev:?}");
        s.flush();
        let sent_now = sent.borrow().clone();
        assert_eq!(sent_now.len(), 1, "the world entry is one datagram");
        let m = messages(&sent_now[0]);
        let kinds: Vec<(u8, u16)> = m.iter().map(|(t, d)| (*t, if *t == 251 { u16::from_le_bytes([d[0], d[1]]) } else { 0 })).collect();
        assert_eq!(kinds, [(251, 0x0014), (251, 0x000E), (251, 0x00C9), (251, 0x00CA), (0x40, 0)], "{m:02x?}");
        assert_eq!(m[1].1[2..], [3], "the chat mode kept");
        assert_eq!(m[2].1[2..], [0, 0]);
        assert_eq!(m[3].1[2..], [0, 0]);
        let phys = vi::walk::WalkPhysics::<Ee> {
            position: vi::vect::Vect3::of(pos[0], pos[1], pos[2]),
            angles: vi::vect::Vect3::of(3.5588355, 0.0, 0.0),
            target_heading: vi::fpu::F::new(3.5588355),
            floor: vi::fpu::F::new(pos[1]),
            flags: vi::walk::flags::ON_FLOOR | vi::walk::flags::TRACK_HEADING,
            ..Default::default()
        };
        assert_eq!(m[4].1, vi::selfstate::encode(0, &phys, 0, u32::MAX), "the 0x40 at the dump's place");

        // A dump cut short: no world entry, no 0x40.
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.flush();
        sent.borrow_mut().clear();
        let mut ev = Vec::new();
        s.route(drdp::Message { ty: drdp::TYPE_GUARANTEED, data: dump[..dump.len() - 80].to_vec(), connection: s.conn, src: 0x1d3d_8fa9, ..Default::default() }, &mut ev);
        assert!(ev.is_empty(), "{ev:?}");
        s.flush();
        let m = messages(&sent.borrow()[0]);
        let kinds: Vec<(u8, u16)> = m.iter().map(|(t, d)| (*t, if *t == 251 { u16::from_le_bytes([d[0], d[1]]) } else { 0 })).collect();
        assert_eq!(kinds, [(251, 0x00C9), (251, 0x00CA)], "{m:02x?}");
    }
}
