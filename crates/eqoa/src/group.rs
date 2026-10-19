//! The group (`VIClient`'s `m_group`, `m_inGroup`, `m_p2BD78`, `m_p2BE4C`)
//! and the invitations waiting (`m_p2BD58`): the roster from 0x0772, each
//! member's health and place from state channel 0x40 (`Channel40`), and what
//! the HUD's `DrawGroup` reads.

use net::stream::Reader;

/// `VIPartyMember`.
#[derive(Clone, Debug, Default)]
pub struct Member {
    pub id: u32,
    pub name: String,
    pub active: bool,
    pub hp_known: u8,
    pub hp: u8,
    pub x: i32,
    pub pos: [f32; 3],
}

/// `VIRec32_BE2DE8`: an invitation (its second word, kept by the game,
/// is read by nothing here).
#[derive(Clone, Debug)]
pub struct Invite {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Group {
    /// `m_inGroup`
    pub in_group: bool,
    /// `m_p2BD78`: the player leads it
    pub leader: bool,
    /// `m_group[0 .. m_p2BE4C]`
    pub members: Vec<Member>,
    /// `m_p2BD58`
    pub invites: Vec<Invite>,
}

impl Group {
    /// `GetP2BE4C`: the members, when in a group.
    pub fn count(&self) -> usize {
        if self.in_group { self.members.len() } else { 0 }
    }

    /// Out of the group (`m_p2BD70 = -1; m_inGroup = m_p2BE4C = m_p2BD78 = 0`).
    pub fn clear(&mut self) {
        self.in_group = false;
        self.leader = false;
        self.members.clear();
    }

    /// `Msg0772`: the roster, each old member's last state replayed onto it;
    /// the player leads when member 0. Ignored out of a group.
    pub fn roster(&mut self, ids: &[u32], names: &[String], self_id: Option<u32>) {
        if !self.in_group {
            return;
        }
        let old = std::mem::take(&mut self.members);
        self.members = ids.iter().zip(names).map(|(&id, n)| Member { id, name: n.clone(), ..Default::default() }).collect();
        for m in old.iter().filter(|m| m.active) {
            self.update(m.id, m.hp_known, m.hp, m.x, m.pos);
        }
        if self.members.first().is_some_and(|m| Some(m.id) == self_id) {
            self.leader = true;
        }
    }

    /// `UpdateGroupMember`.
    fn update(&mut self, id: u32, known: u8, hp: u8, x: i32, pos: [f32; 3]) {
        if let Some(m) = self.members.iter_mut().find(|m| m.id == id) {
            m.active = true;
            m.hp_known = known;
            m.hp = hp;
            m.x = x;
            m.pos = pos;
        }
    }

    /// `Channel40`: every member inactive, then each record (u32 id, u8 hp
    /// known, u8 hp, s8, three s16 coordinates) updates its member. A record
    /// cut short updates nothing (the game's would update with its stale
    /// locals).
    pub fn channel40(&mut self, data: &[u8]) {
        for m in &mut self.members {
            m.active = false;
        }
        let mut r = Reader::new(data);
        while !r.error && r.pos != data.len() {
            let id = r.u32();
            let (known, hp, x) = (r.u8(), r.u8(), r.u8() as i8 as i32);
            let pos = [r.u16() as i16 as f32, r.u16() as i16 as f32, r.u16() as i16 as f32];
            if !r.error {
                self.update(id, known, hp, x, pos);
            }
        }
    }

    /// `DropInvites(id)`.
    pub fn drop_invites(&mut self, id: u32) {
        self.invites.retain(|i| i.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, known: u8, hp: u8, x: i8, pos: [i16; 3]) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend([known, hp, x as u8]);
        for c in pos {
            v.extend(c.to_le_bytes());
        }
        v
    }

    fn grouped(ids: &[u32]) -> Group {
        let mut g = Group { in_group: true, ..Default::default() };
        let names: Vec<String> = ids.iter().map(|i| format!("m{i}")).collect();
        g.roster(ids, &names, Some(1));
        g
    }

    #[test]
    fn roster_ignored_out_of_a_group_and_leader_is_member_0() {
        let mut g = Group::default();
        g.roster(&[1, 2], &["a".into(), "b".into()], Some(1));
        assert!(g.members.is_empty(), "Msg0772 returns when not InGroup");
        assert_eq!(g.count(), 0);
        let g = grouped(&[1, 2]);
        assert!(g.leader);
        let g = grouped(&[2, 1]);
        assert!(!g.leader, "member 0 is someone else");
    }

    #[test]
    fn channel40_updates_members_and_clears_the_rest() {
        let mut g = grouped(&[1, 2, 3]);
        let mut d = record(2, 1, 200, -3, [100, -5, 30000]);
        d.extend(record(9, 1, 50, 0, [0, 0, 0]));
        g.channel40(&d);
        let m2 = &g.members[1];
        assert!(m2.active && m2.hp == 200 && m2.hp_known == 1 && m2.x == -3);
        assert_eq!(m2.pos, [100.0, -5.0, 30000.0]);
        assert!(!g.members[2].active, "a member with no record is inactive");
        // a later frame without member 2 makes it inactive again
        g.channel40(&record(3, 0, 0, 0, [0, 0, 0]));
        assert!(!g.members[1].active && g.members[2].active);
        // a record cut short updates nothing
        let mut cut = record(2, 1, 9, 0, [0, 0, 0]);
        cut.truncate(8);
        g.channel40(&cut);
        assert!(!g.members[1].active);
    }

    #[test]
    fn a_new_roster_replays_the_last_known_states() {
        let mut g = grouped(&[1, 2]);
        g.channel40(&record(2, 1, 77, 0, [1, 2, 3]));
        g.roster(&[1, 3, 2], &["a".into(), "c".into(), "b".into()], Some(1));
        let b = g.members.iter().find(|m| m.id == 2).unwrap();
        assert!(b.active && b.hp == 77, "UpdateGroupMember replays old[i] onto the new roster");
        assert!(!g.members[1].active, "a new member starts inactive");
    }
}
