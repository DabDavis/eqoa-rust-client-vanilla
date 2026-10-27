//! The group's waypoint between the map and the group's chat
//! (`map::group_waypoint`, F3 20261026-212034): what the map asked the group
//! to be told of goes out as a line of group chat (`take_map_group_waypoint`),
//! a group line heard from a member of the group puts the group's waypoint
//! on the map (`group_line`), and it goes when its sender or the player is
//! out of the group (`keep_group_waypoint`).

use super::Play;
use crate::map::group_waypoint::{self, GroupMark};

/// What the map says when the group's waypoint has to wait
/// (`group_waypoint::EVERY_MS`).
const WAIT: &str = "Wait a few seconds before the next group waypoint.";
/// What it says out of a group, when WORDS.TXT has no word 629 to say it.
const NOT_GROUPED: &str = "You are not in a group.";

impl Play {
    /// The map's ask (`Map::take_group_ask`): the spot said in the group's
    /// chat as `/g` says a line (0x000E 2, 0x000B, 0x000E back:
    /// `chat_line`), one every `group_waypoint::EVERY_MS` at most, and the
    /// player's own group waypoint put at once (the server's echo puts the
    /// same again). Out of a group (the game's word 629), too soon after the
    /// last, or in a world the map cannot name, the map says so and nothing
    /// is sent.
    pub(super) fn take_map_group_waypoint(&mut self) {
        let Some(spot) = self.map.take_group_ask() else { return };
        if !self.group.in_group || self.group.count() == 0 {
            let line = self.words.as_ref().map(|w| w.get(629, 0).to_string()).filter(|s| s != "(null)" && !s.is_empty()).unwrap_or_else(|| NOT_GROUPED.to_string());
            self.map.notice(&line);
            return;
        }
        let now = self.session.now();
        if self.map.group_wait(now).is_some() {
            self.map.notice(WAIT);
            return;
        }
        let (Some(line), Some(_)) = (self.map.group_line(spot, self.words.as_ref()), self.session.self_id) else {
            eprintln!("map: no group waypoint said: world {} has no name here, or no session", spot.world);
            return;
        };
        eprintln!("map: the group told: {line}");
        self.chat_line(&format!("/g {line}"));
        self.map.group_told(now);
        let at = group_waypoint::rounded(spot.at);
        let me = self.dump.player.name.clone();
        self.map.set_group_waypoint(GroupMark { world: spot.world, at, from: me });
        self.map.notice(&format!("Your group was told: x {:.0}, z {:.0}", at[0], at[1]));
    }

    /// A line of the group's chat (0x069E, `Event::GroupChat`): into the chat
    /// box as every line goes (`server_line`, in the group's colour), and when
    /// it is a group waypoint's (`group_waypoint::parse`) from a member of the
    /// group as it stands (the player's own line too), the group's waypoint
    /// on the map. Out of a group, from a name not in it, or in a world the
    /// map cannot name, nothing more.
    pub(super) fn group_line(&mut self, text: String) {
        self.heard_group_waypoint(&text);
        self.server_line(text, net::session::GROUP_CHAT);
    }

    fn heard_group_waypoint(&mut self, text: &str) {
        let Some(h) = group_waypoint::parse(text) else { return };
        let Some(from) = self.group_member_named(&h.from) else {
            eprintln!("map: a group waypoint's line from {:?}, who is not in the group: left", h.from);
            return;
        };
        let Some(world) = self.map.world_named(&h.world, self.words.as_ref()) else {
            eprintln!("map: a group waypoint in {:?}, a world the map cannot name: left", h.world);
            return;
        };
        self.map.set_group_waypoint(GroupMark { world, at: h.at, from });
    }

    /// The group's member called `name` as the roster spells it, the player
    /// among them, while in a group.
    fn group_member_named(&self, name: &str) -> Option<String> {
        if !self.group.in_group {
            return None;
        }
        if group_waypoint::same_name(name, &self.dump.player.name) {
            return Some(self.dump.player.name.clone());
        }
        let members = &self.group.members[..self.group.count()];
        members.iter().find(|m| group_waypoint::same_name(&m.name, name)).map(|m| m.name.trim_end_matches(['\0', ' ']).to_string())
    }

    /// Each frame: the group's waypoint gone once the player is out of the
    /// group, or its sender is (`Map::keep_group_waypoint`).
    pub(super) fn keep_group_waypoint(&mut self) {
        let in_group = self.group.in_group && self.group.count() > 0;
        let me = &self.dump.player.name;
        let members = &self.group.members[..self.group.count()];
        self.map.keep_group_waypoint(in_group, |n| group_waypoint::same_name(n, me) || members.iter().any(|m| group_waypoint::same_name(&m.name, n)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::Member;
    use crate::map::group_waypoint::PHRASE;
    use crate::pointer::key;
    use net::session::{Player, Session};

    /// Play on a session nothing answers, its messages recorded, its map up
    /// over Tunaria's layout (`map::testing::up`), the player Dairine.
    fn play() -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        let mut p = Play::new(s, Player::default(), Box::default());
        p.dump.player.name = "Dairine".into();
        p.map = crate::map::testing::up((1280, 720));
        p
    }

    /// Dairine and Bob in a group (Bob's name as a roster's fixed field).
    fn grouped(p: &mut Play) {
        p.group.in_group = true;
        p.group.members = vec![Member { id: 0x100, name: "Dairine".into(), ..Default::default() }, Member { id: 0x200, name: "Bob\0".into(), ..Default::default() }];
    }

    /// The wide strings 0x000B carried, in order.
    fn said(p: &mut Play) -> Vec<String> {
        let sent = p.session.record.replace(Vec::new()).unwrap_or_default();
        sent.iter()
            .filter(|(_, m)| m.get(..2) == Some(&[0x0B, 0x00]))
            .map(|(_, m)| {
                let n = u32::from_le_bytes(m[2..6].try_into().unwrap()) as usize;
                String::from_utf16_lossy(&m[6..6 + n * 2].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
            })
            .collect()
    }

    /// G on the map (`map::KEY_GROUP`), in a group: the spot is said in the
    /// group's channel (0x000E 2 around the 0x000B, the channel put back)
    /// in `group_waypoint::line`'s words, and the player's own group
    /// waypoint is there at once, from the player; a second within five
    /// seconds is not sent, the map saying why, and one after is. The
    /// control: out of a group nothing is sent and the map says so.
    #[test]
    fn g_tells_the_group_through_its_chat() {
        crate::settings::testing::reset(false);
        let mut p = play();
        p.session.record = Some(Vec::new());
        assert!(p.map.key(crate::map::KEY_GROUP, true));
        p.take_map_group_waypoint();
        assert!(said(&mut p).is_empty(), "the control: out of a group, nothing said");
        assert_eq!((p.map.group_mark(), p.map.community.notice_now()), (None, Some(NOT_GROUPED)), "the map says why");
        grouped(&mut p);
        let middle = p.map.following().then(|| crate::map::testing::FREEPORT).unwrap();
        p.map.key(crate::map::KEY_GROUP, true);
        p.take_map_group_waypoint();
        let sent = p.session.record.clone().unwrap();
        let line = group_waypoint::line("Tunaria", middle, None);
        assert_eq!(said(&mut p), vec![line.clone()]);
        let modes: Vec<u8> = sent.iter().filter(|(_, m)| m.get(..2) == Some(&[0x0E, 0x00])).map(|(_, m)| m[2]).collect();
        assert_eq!(modes, vec![2, 0], "the group's channel for the line, say put back");
        assert_eq!(p.map.group_mark(), Some(&GroupMark { world: 0, at: group_waypoint::rounded(middle), from: "Dairine".into() }), "the player's own, at once");
        assert_eq!(p.map.community.notice_now(), Some("Your group was told: x 25300, z 15200"));
        // too soon: nothing more
        p.map.key(crate::map::KEY_GROUP, true);
        p.take_map_group_waypoint();
        assert!(said(&mut p).is_empty(), "within five seconds");
        assert_eq!(p.map.community.notice_now(), Some(WAIT));
        // the clock past the wait: another goes
        p.map.group_told(p.session.now().wrapping_sub(group_waypoint::EVERY_MS));
        p.map.key(crate::map::KEY_GROUP, true);
        p.take_map_group_waypoint();
        assert_eq!(said(&mut p), vec![line], "after the wait");
    }

    /// A group line in those words from a member puts the group's waypoint
    /// (the player's own echo too, and a newer one replaces it); the chat box
    /// gets the line either way. The controls: the same words from a name not
    /// in the group, on another channel (a say, `Event::Chat`), out of a
    /// group, in a world the map cannot name, or in other words, put nothing.
    #[test]
    fn a_members_line_puts_the_groups_waypoint() {
        crate::settings::testing::reset(false);
        let mut p = play();
        let from = |name: &str, x: i32| format!("{name}{PHRASE}Waypoint in Tunaria at x {x}, z 15000 near Freeport");
        p.group_line(from("Bob", 20000));
        assert_eq!(p.map.group_mark(), None, "the control: out of a group");
        grouped(&mut p);
        p.group_line(from("Mallory", 20000));
        assert_eq!(p.map.group_mark(), None, "the control: not a member");
        p.server_line(from("Bob", 20000), [1.0; 4]);
        assert_eq!(p.map.group_mark(), None, "the control: another channel's line");
        p.group_line(format!("Bob{PHRASE}Waypoint in Faydwer at x 1, z 2"));
        assert_eq!(p.map.group_mark(), None, "the control: a world the map cannot name");
        p.group_line(format!("Bob{PHRASE}meet at x 1, z 2"));
        assert_eq!(p.map.group_mark(), None, "the control: other words");
        p.group_line(from("bob", 20000));
        assert_eq!(p.map.group_mark(), Some(&GroupMark { world: 0, at: [20000.0, 15000.0], from: "Bob".into() }), "a member's, its name as the roster has it");
        assert_eq!(p.map.group_waypoint(0), Some([20000.0, 15000.0]));
        p.group_line(from("Dairine", 21000));
        assert_eq!(p.map.group_mark().map(|g| (g.at, g.from.as_str())), Some(([21000.0, 15000.0], "Dairine")), "the player's own, a newer one");
        assert!(p.chat.iter().any(|(t, c)| t == &from("bob", 20000) && *c == net::session::GROUP_CHAT), "the line in the chat box, the group's colour");
        assert!(p.chat.iter().any(|(t, _)| t == &from("Mallory", 20000)), "every line still in the chat box");
    }

    /// The group's waypoint goes when its sender leaves the group (a roster
    /// without them) and when the player does. The control: a roster that
    /// still has the sender keeps it.
    #[test]
    fn it_goes_with_its_sender_or_the_group() {
        crate::settings::testing::reset(false);
        let mut p = play();
        grouped(&mut p);
        p.group_line(format!("Bob{PHRASE}Waypoint in Tunaria at x 20000, z 15000"));
        p.keep_group_waypoint();
        assert!(p.map.group_mark().is_some(), "the control: Bob still a member");
        p.group.members.retain(|m| m.id != 0x200);
        p.keep_group_waypoint();
        assert_eq!(p.map.group_mark(), None, "Bob left the group");
        p.group.members.push(Member { id: 0x200, name: "Bob".into(), ..Default::default() });
        p.group_line(format!("Bob{PHRASE}Waypoint in Tunaria at x 20000, z 15000"));
        assert!(p.map.group_mark().is_some());
        p.group.clear();
        p.keep_group_waypoint();
        assert_eq!(p.map.group_mark(), None, "the player out of the group");
    }

    /// □ held on the map in a group: the group is told of □'s spot, the
    /// target's while the map follows the player (the control: let go
    /// before the hold, the player's own waypoint goes on the target, and
    /// nothing is said).
    #[test]
    fn square_held_tells_the_group_of_the_target() {
        crate::settings::testing::reset(false);
        let mut p = play();
        grouped(&mut p);
        let merchant = [25475.9, 15704.6];
        p.map.target_for_test(Some(merchant));
        p.map.key(key::SQUARE, true);
        p.map.hold_for_test(0.3);
        p.map.key(key::SQUARE, false);
        p.take_map_group_waypoint();
        assert!(said(&mut p).is_empty(), "the control: a short press says nothing");
        assert_eq!(p.map.waypoint(0), Some(merchant), "the player's own on the target");
        p.map.key(key::SQUARE, true);
        p.map.hold_for_test(group_waypoint::HOLD);
        p.take_map_group_waypoint();
        assert_eq!(said(&mut p), vec![group_waypoint::line("Tunaria", merchant, None)]);
        p.map.key(key::SQUARE, false);
        assert_eq!(p.map.waypoint(0), Some(merchant), "the long press's release is nothing");
    }
}
