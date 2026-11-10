//! go-eqoa-server's own, no retail message: a raid's windows (the GM Jibrish's
//! petitions 1448 and 1451, 2026-11-10: "A small icon denoting who is raid
//! leader", the loot rules, "a pop up window that does not stop the persons
//! movement", the master looter's Assign and Abandon). A raid is a group whose
//! leader raised a raid flag or invited by `/raidinvite`, or one past four (the
//! server's `entity/raid.go`).
//!
//! The server offers them once the player is in the world
//! ([`OP_RAID_OFFER`](crate::session::OP_RAID_OFFER)); a client that shows
//! them answers 1 ([`OP_RAID_ANSWER`](crate::session::OP_RAID_ANSWER)) and is
//! then sent the raid's state ([`State`]), each loot roll it is offered
//! ([`Roll`]) and, as a master looter, whom it may give a corpse's items to
//! ([`MasterLoot`]); it sends its rolls' choices ([`roll_choice`]) and the raid
//! menu's commands ([`Command`]). A PS2 ignores the offer and is told the same
//! in chat lines, answered with `/t need 3` and the like. Every rule is the
//! server's: the client only shows and asks.
//!
//! Ids are the ones this client knows a player by, as the raid roster's
//! ([`OP_RAID_ROSTER`](crate::session::OP_RAID_ROSTER)): its own player
//! object's, a visibility channel's, or the stand-in `0x7F000000 | entity` for
//! one out of sight.

use crate::dump::NetItem;
use crate::session::{OP_RAID_COMMAND, OP_RAID_ROLL_CHOICE};
use crate::stream::{Reader, Writer};

/// The loot rules ([`State::rule`]): anyone takes anything, as every corpse
/// of the game.
pub const RULE_FREE_FOR_ALL: u8 = 0;
/// A raid boss's items, no-trade items and items with stats (never a stack
/// or vendor trash) rolled among the raid near the corpse: Need beats Greed.
pub const RULE_NEED_BEFORE_GREED: u8 = 1;
/// The raid's kills are the leader's to open, to give out (Assign) or to let
/// go to anyone (Abandon).
pub const RULE_MASTER_LOOTER: u8 = 2;

/// The raid as the server last told it ([`read_state`]): all zero in no raid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// The leader (0: in no raid), its helper and its main assist (0: none).
    pub leader: u32,
    pub helper: u32,
    pub assist: u32,
    /// [`RULE_FREE_FOR_ALL`], [`RULE_NEED_BEFORE_GREED`] or
    /// [`RULE_MASTER_LOOTER`] (another a later server may send).
    pub rule: u8,
    /// Bit 0 the raid is in combat (its loot rule cannot change, it cannot be
    /// disbanded), bit 1 this client leads it, bit 2 this client is its helper.
    pub flags: u8,
}

impl State {
    /// In a raid at all.
    pub fn in_raid(&self) -> bool {
        self.leader != 0
    }
    pub fn in_combat(&self) -> bool {
        self.in_raid() && self.flags & 1 != 0
    }
    pub fn leads(&self) -> bool {
        self.in_raid() && self.flags & 2 != 0
    }
    pub fn helps(&self) -> bool {
        self.in_raid() && self.flags & 4 != 0
    }
}

/// [`OP_RAID_STATE`](crate::session::OP_RAID_STATE)'s body: u32 leader, u32
/// helper, u32 main assist, u8 loot rule, u8 flags. None when short.
pub fn read_state(data: &[u8]) -> Option<State> {
    let mut r = Reader::new(data);
    let s = State { leader: r.u32(), helper: r.u32(), assist: r.u32(), rule: r.u8(), flags: r.u8() };
    (!r.error).then_some(s)
}

/// One loot roll ([`read_roll`]): its id (1-999, the server's, which a PS2's
/// `/t need 3` names), the seconds left of it (0: over for this client: its
/// window goes, and nothing else came), whether the server lets this player
/// Need it (it can wear or use it), and the item.
#[derive(Clone, Debug, Default)]
pub struct Roll {
    pub id: u32,
    pub seconds: u16,
    pub may_need: bool,
    pub item: Option<Box<NetItem>>,
}

/// [`OP_RAID_ROLL`](crate::session::OP_RAID_ROLL)'s body: u32 id, u16 seconds
/// left; while any are left, u8 flags (bit 0 may Need) and the item as an
/// inventory record (0x003B's body, its key the roll's id). None when short.
pub fn read_roll(data: &[u8]) -> Option<Roll> {
    let mut r = Reader::new(data);
    let id = r.u32();
    let seconds = r.u16();
    if r.error {
        return None;
    }
    if seconds == 0 {
        return Some(Roll { id, ..Default::default() });
    }
    let flags = r.u8();
    let item = crate::dump::read_net_item(&mut r);
    (!r.error).then(|| Roll { id, seconds, may_need: flags & 1 != 0, item: Some(Box::new(item)) })
}

/// What a master looter may do with the corpse it has open ([`read_master_loot`]):
/// the seconds left of its hold (then the corpse is anyone's), and the players
/// it may give an item to (the raid's members and anyone kicked from it in that
/// fight), each by id and name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MasterLoot {
    pub seconds: u16,
    pub members: Vec<(u32, String)>,
}

/// [`OP_RAID_MASTER_LOOT`](crate::session::OP_RAID_MASTER_LOOT)'s body: u8 on;
/// on, u16 seconds left, u8 count, each a u32 id and a name (u8 byte count and
/// the bytes). `Some(None)` when it is over (or off); None when short.
pub fn read_master_loot(data: &[u8]) -> Option<Option<MasterLoot>> {
    let mut r = Reader::new(data);
    let on = r.u8();
    if r.error {
        return None;
    }
    if on == 0 {
        return Some(None);
    }
    let seconds = r.u16();
    let n = r.u8() as usize;
    let mut members = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.u32();
        let len = r.u8() as usize;
        members.push((id, String::from_utf8_lossy(r.bytes(len)).into_owned()));
    }
    (!r.error).then_some(Some(MasterLoot { seconds, members }))
}

/// A roll's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Need = 1,
    Greed = 2,
    Pass = 3,
}

/// [`OP_RAID_ROLL_CHOICE`]: u32 the roll's id, u8 the choice; the whole
/// message, its opcode first.
pub fn roll_choice(id: u32, c: Choice) -> Vec<u8> {
    let mut w = Writer::op(OP_RAID_ROLL_CHOICE);
    w.u32(id).u8(c as u8);
    w.data
}

/// The raid menu's commands ([`OP_RAID_COMMAND`]), each its u8 op and what
/// follows it. The server refuses what is not the player's to do, in a red line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// 1: the loot rule (the leader's, never in combat)
    LootRule(u8),
    /// 2: out of the raid, as `/raidleave`
    Leave,
    /// 3: the master looter's Assign: the corpse record's key (as 0x0018 listed
    /// it) to the player of that id (from [`MasterLoot`]'s list)
    Assign { key: u32, id: u32 },
    /// 4: the master looter's Abandon: the corpse it has open is anyone's
    Abandon,
    /// 5: the target invited to the raid, as `/raidinvite` with no name
    InviteTarget(u32),
    /// 6: the raid flag, as `/raidaoe`
    Flag,
    /// 7: the main assist's target targeted, as `/raidassist`
    AssistMain,
    /// 8, 9, 10: the target made the leader, the helper, the main assist (0:
    /// none, for the helper and the main assist)
    Leader(u32),
    Helper(u32),
    Assist(u32),
    /// 11: the target kicked out of the raid
    Kick(u32),
}

impl Command {
    pub fn op(&self) -> u8 {
        match self {
            Command::LootRule(_) => 1,
            Command::Leave => 2,
            Command::Assign { .. } => 3,
            Command::Abandon => 4,
            Command::InviteTarget(_) => 5,
            Command::Flag => 6,
            Command::AssistMain => 7,
            Command::Leader(_) => 8,
            Command::Helper(_) => 9,
            Command::Assist(_) => 10,
            Command::Kick(_) => 11,
        }
    }

    /// The whole message, its opcode first.
    pub fn bytes(&self) -> Vec<u8> {
        let mut w = Writer::op(OP_RAID_COMMAND);
        w.u8(self.op());
        match *self {
            Command::LootRule(rule) => {
                w.u8(rule);
            }
            Command::Assign { key, id } => {
                w.u32(key).u32(id);
            }
            Command::InviteTarget(id) | Command::Leader(id) | Command::Helper(id) | Command::Assist(id) | Command::Kick(id) => {
                w.u32(id);
            }
            Command::Leave | Command::Abandon | Command::Flag | Command::AssistMain => {}
        }
        w.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_is_read_and_a_short_one_is_not() {
        let mut b = Vec::new();
        for v in [0x0012_3456u32, 0x7F00_0007, 0] {
            b.extend(v.to_le_bytes());
        }
        b.extend([RULE_NEED_BEFORE_GREED, 0b011]);
        let s = read_state(&b).unwrap();
        assert_eq!(s, State { leader: 0x0012_3456, helper: 0x7F00_0007, assist: 0, rule: 1, flags: 3 });
        assert!(s.in_raid() && s.in_combat() && s.leads() && !s.helps());
        // the control: one byte short reads nothing
        assert_eq!(read_state(&b[..b.len() - 1]), None);
        // all zero: no raid, whatever its flags say
        let none = read_state(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7]).unwrap();
        assert!(!none.in_raid() && !none.in_combat() && !none.leads() && !none.helps());
    }

    /// An inventory record as 0x003B's body (the server's `writeSingleItem`): the
    /// slot's six words, the item's 23 fields, its name and description, no stats.
    fn record(name: &str) -> Vec<u8> {
        let mut w = Writer::default();
        w.signed(1).signed(100).signed(0).signed(-1).signed(-1).u32(7);
        for f in 0..23i64 {
            w.signed(if f == 5 { 3 } else { f });
        }
        w.wide(name).wide("");
        w.varint(0);
        w.data
    }

    #[test]
    fn a_roll_and_its_end_are_read() {
        let mut b = 7u32.to_le_bytes().to_vec();
        b.extend(120u16.to_le_bytes());
        b.push(1);
        b.extend(record("Fine Steel Sword"));
        let r = read_roll(&b).unwrap();
        assert_eq!((r.id, r.seconds, r.may_need), (7, 120, true));
        let it = r.item.unwrap();
        assert_eq!((it.item.name.as_str(), it.item.wear_slot, it.inv_index), ("Fine Steel Sword", 3, 7));
        // the flags' other bits are not Need
        b[6] = 2;
        assert!(!read_roll(&b).unwrap().may_need);
        // the control: an item cut short is no roll
        assert!(read_roll(&b[..b.len() - 3]).is_none());
        // seconds 0: over, and nothing follows
        let over = read_roll(&[7, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!((over.id, over.seconds), (7, 0));
        assert!(over.item.is_none());
        assert!(read_roll(&[7, 0, 0, 0, 0]).is_none(), "short of its seconds");
    }

    #[test]
    fn the_master_looters_list_is_read() {
        let mut b = vec![1];
        b.extend(300u16.to_le_bytes());
        b.push(2);
        for (id, name) in [(0x1234u32, "Jibrish"), (0x7F00_0009, "Dab")] {
            b.extend(id.to_le_bytes());
            b.push(name.len() as u8);
            b.extend(name.as_bytes());
        }
        let m = read_master_loot(&b).unwrap().unwrap();
        assert_eq!(m, MasterLoot { seconds: 300, members: vec![(0x1234, "Jibrish".into()), (0x7F00_0009, "Dab".into())] });
        assert_eq!(read_master_loot(&[0]), Some(None), "over");
        // the control: a name cut short is no list
        assert_eq!(read_master_loot(&b[..b.len() - 1]), None);
        assert_eq!(read_master_loot(&[]), None);
    }

    #[test]
    fn the_choices_and_commands_are_written() {
        assert_eq!(roll_choice(7, Choice::Greed), [0x46, 0x7E, 7, 0, 0, 0, 2]);
        assert_eq!(roll_choice(0x01020304, Choice::Pass), [0x46, 0x7E, 4, 3, 2, 1, 3]);
        let id = 0x7F00_0005u32.to_le_bytes();
        let cases: [(Command, Vec<u8>); 11] = [
            (Command::LootRule(RULE_MASTER_LOOTER), vec![1, 2]),
            (Command::Leave, vec![2]),
            (Command::Assign { key: 3, id: 0x7F00_0005 }, [vec![3, 3, 0, 0, 0], id.to_vec()].concat()),
            (Command::Abandon, vec![4]),
            (Command::InviteTarget(0x7F00_0005), [vec![5], id.to_vec()].concat()),
            (Command::Flag, vec![6]),
            (Command::AssistMain, vec![7]),
            (Command::Leader(0x7F00_0005), [vec![8], id.to_vec()].concat()),
            (Command::Helper(0), vec![9, 0, 0, 0, 0]),
            (Command::Assist(0x7F00_0005), [vec![10], id.to_vec()].concat()),
            (Command::Kick(0x7F00_0005), [vec![11], id.to_vec()].concat()),
        ];
        for (c, body) in cases {
            assert_eq!(c.bytes(), [vec![0x48, 0x7E], body].concat(), "{c:?}");
        }
    }
}
