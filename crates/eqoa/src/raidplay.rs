//! A raid's windows in play (`net::raid`, go-eqoa-server's 0x7E42-0x7E48; the
//! GM Jibrish's petitions 1448 and 1451): the raid's state as the server sends
//! it (the marks in the raid list and on the group's bars, Community > Group >
//! Raid in the start menu, the disband refused in combat), the loot rolls'
//! pop-up (`rollwnd`) and the master looter's list for the corpse window
//! (`masterloot`, kept with the corpse in `npcw`). A child of `play`, for
//! `Play`'s fields.
//!
//! Every rule is the server's; the client shows and asks. On a server that
//! offers none of it (a PS2's, Sandstorm's) nothing here is drawn or sent, and
//! the start menu is the game's.

use super::Play;
use crate::startmenu::LabelRow;
use net::raid::{Choice, Command, RULE_FREE_FOR_ALL, RULE_MASTER_LOOTER, RULE_NEED_BEFORE_GREED, Roll, State};

/// The start menu's Raid entries' messages (`DoAction`'s ids: none of the
/// game's), each a command's op in its second byte and an argument in its
/// first (`raid_action`).
pub const MSG_RAID: u32 = 0x7E48_0000;
pub const MSG_LOOT_FREE: u32 = MSG_RAID | 0x0100;
pub const MSG_LOOT_NEED: u32 = MSG_RAID | 0x0101;
pub const MSG_LOOT_MASTER: u32 = MSG_RAID | 0x0102;
pub const MSG_LEAVE: u32 = MSG_RAID | 0x0200;
pub const MSG_INVITE: u32 = MSG_RAID | 0x0500;
pub const MSG_FLAG: u32 = MSG_RAID | 0x0600;
pub const MSG_ASSIST: u32 = MSG_RAID | 0x0700;
pub const MSG_LEADER: u32 = MSG_RAID | 0x0800;
pub const MSG_HELPER: u32 = MSG_RAID | 0x0900;
pub const MSG_MAIN_ASSIST: u32 = MSG_RAID | 0x0A00;
pub const MSG_KICK: u32 = MSG_RAID | 0x0B00;
/// Move to Group's entries (petition 1687): the target to group `msg & 0xFF`;
/// with no group, the sub-menu itself (`startmenu::RAID_GROUPS_SUB`).
pub const MSG_GROUP: u32 = MSG_RAID | 0x0C00;

/// The lines the client says itself (red, as the server's refusals).
const IN_COMBAT_DISBAND: &str = "Your raid is in combat: it cannot be disbanded.";
const IN_COMBAT_RULES: &str = "Your raid is in combat: its loot rules cannot change.";
const NO_TARGET: &str = "You have no target.";
const NOT_IN_RAID: &str = "Your target is not in your raid.";
const REFUSED: [f32; 4] = [1.0, 0.25, 0.25, 1.0];

/// The raid's state, its rolls and the start menu's entries as last set.
#[derive(Default)]
pub struct RaidPlay {
    /// what the server last said (all zero: in no raid, or never told)
    pub state: State,
    pub rolls: crate::rollwnd::Rolls,
    /// the ready check's strip above them (`readywnd`; petition 1711)
    pub ready: crate::readywnd::Ready,
    /// the start menu's Raid entries as last set (none: taken out), once set
    menu: Option<Option<(Vec<LabelRow>, Vec<LabelRow>, Vec<LabelRow>)>>,
    /// a raid list's row dragged to another group (`raid_drag_press`)
    pub drag: Option<Drag>,
}

/// A raid list's row held by the pointer (the leader's or the helper's, on a
/// server that sends the groups): let go over another group it is moved there,
/// over a member of another group the two are swapped; let go where it was
/// pressed it is a click (the member targeted, as ever).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub id: u32,
    pub group: u8,
    /// where it was pressed and where the pointer is, in the HUD's frame
    pub from: [f32; 2],
    pub at: [f32; 2],
    /// it has gone farther than a click's wobble
    pub moved: bool,
}

/// How far a press goes before it is a drag, in frame units.
const DRAG_SLOP: f32 = 4.0;

/// Where held row `d` would go over `pick` (what the pointer is on): a group's
/// number, and the member there when it is on one. None when it has not moved,
/// or it is on its own group or nothing of the list.
pub fn drop_on(d: &Drag, pick: Option<crate::hud::Pick>, group_of: impl Fn(u32) -> u8) -> Option<(u8, Option<u32>)> {
    if !d.moved {
        return None;
    }
    match pick? {
        crate::hud::Pick::RaidGroup(g) => Some((g, None)),
        crate::hud::Pick::Raid(id) => Some((group_of(id), Some(id))),
        _ => None,
    }
    .filter(|&(g, _)| g != 0 && g != d.group)
}

/// The command a drop sends: onto a member, the two swapped; onto a group, the
/// member moved there.
pub fn drop_command(d: &Drag, drop: Option<(u8, Option<u32>)>) -> Option<Command> {
    match drop? {
        (_, Some(other)) => Some(Command::Swap(d.id, other)),
        (g, None) => Some(Command::MoveToGroup { id: d.id, group: g }),
    }
}

/// The groups of a raid roster: each group's number and how many it holds, in
/// their numbers.
pub fn group_counts(raid: &[net::session::RaidMember]) -> Vec<(u8, usize)> {
    let mut out: Vec<(u8, usize)> = Vec::new();
    for m in raid.iter().filter(|m| m.group > 0) {
        match out.iter_mut().find(|g| g.0 == m.group) {
            Some(g) => g.1 += 1,
            None => out.push((m.group, 1)),
        }
    }
    out.sort_unstable();
    out
}

/// The rows Community > Group > Raid shows for `st` (`startmenu::set_raid`),
/// and its Loot Rules menu's (the leader's alone, the rule in force marked " *",
/// every one dimmed in combat): in no raid, Invite and the Raid Flag (a raid is
/// begun by them); in one, everyone's Assist Main Assist and Leave Raid, the
/// helper's Invite, Raid Flag and Kick too, and the leader's every one. On a
/// server that sends the raid's groups (`groups`, each number and how many it
/// holds; none: it sends none) the leader and the helper have Move to Group
/// after Kick, its menu a row a group to the highest a member may go to
/// (`raidlist::most_group` of `n` members), each with how many it holds, a
/// full one dimmed (petition 1687).
pub fn rows(st: &State, groups: &[(u8, usize)], n: usize) -> (Vec<LabelRow>, Vec<LabelRow>, Vec<LabelRow>) {
    let row = |msg, text: &'static str| LabelRow { msg, text: text.into(), tail: "".into(), dim: false };
    let invite = [row(MSG_INVITE, "Invite"), row(MSG_FLAG, "Raid Flag")];
    if !st.in_raid() {
        return (invite.to_vec(), Vec::new(), Vec::new());
    }
    let mut moves = Vec::new();
    let mut out = Vec::new();
    let mut loot = Vec::new();
    if st.leads() {
        out.push(row(0, "Loot Rules"));
        for (msg, text, rule) in [(MSG_LOOT_FREE, "Free for All", RULE_FREE_FOR_ALL), (MSG_LOOT_NEED, "Need Before Greed", RULE_NEED_BEFORE_GREED), (MSG_LOOT_MASTER, "Master Looter", RULE_MASTER_LOOTER)] {
            loot.push(LabelRow { msg, text: text.into(), tail: if st.rule == rule { " *" } else { "" }.into(), dim: st.in_combat() });
        }
    }
    if st.leads() || st.helps() {
        out.extend(invite);
        out.push(row(MSG_KICK, "Kick"));
        if !groups.is_empty() {
            out.push(row(crate::startmenu::RAID_GROUPS_SUB, "Move to Group"));
            let top = groups.iter().map(|g| g.0).max().unwrap_or(1);
            for k in 1..=crate::raidlist::most_group(n, top) {
                let c = groups.iter().find(|g| g.0 == k).map_or(0, |g| g.1);
                moves.push(LabelRow { msg: MSG_GROUP | k as u32, text: format!("Group {k}").into(), tail: format!(" ({c}/4)").into(), dim: c >= 4 });
            }
        }
    }
    if st.leads() {
        out.extend([row(MSG_LEADER, "Make Leader"), row(MSG_HELPER, "Make Helper"), row(MSG_MAIN_ASSIST, "Make Main Assist")]);
    }
    out.extend([row(MSG_ASSIST, "Assist Main Assist"), row(MSG_LEAVE, "Leave Raid")]);
    (out, loot, moves)
}

impl Play {
    /// The raid's windows: always asked for where the server offers them (a
    /// server that made no offer is told nothing).
    pub(super) fn agree_raid(&mut self) {
        self.session.agree_raid(true);
        self.session.agree_ready(true);
    }

    /// 0x7E4B: a ready check asked (its strip up, a cue), or over (its strip
    /// gone).
    pub(super) fn ready_check(&mut self, c: net::readycheck::Check) {
        let now = self.session.now() as i64;
        if c.seconds > 0 {
            eprintln!("play: ready check {} from {}, {} s", c.id, c.asker, c.seconds);
            self.cues.push(crate::sound::Cue::Ui(11));
        }
        self.raid.ready.check(c, now);
    }

    /// The ready check answered (a click or a tap, ✕ while R2 holds the d-pad
    /// on it).
    fn make_ready(&mut self, id: u32, ready: bool) {
        eprintln!("play: ready check {id}: {}", if ready { "ready" } else { "not ready" });
        self.session.send_ready_reply(id, ready);
    }

    /// 0x7E44: the raid as it is now. In a raid the client leads as the server
    /// says (its flags' bit 1): `Msg0772` only ever sets the game's leader flag
    /// (`m_p2BD78`, from member 0), so after `/raidleader` handed the lead on,
    /// the old leader's client still took itself for the leader (its Boot,
    /// Disband and the leader's rows); out of one the game's rule stands.
    pub(super) fn raid_state(&mut self, st: State) {
        if st != self.raid.state {
            eprintln!("play: raid {:?}", st);
        }
        if st.in_raid() && self.group.in_group {
            self.group.leader = st.leads();
        }
        self.raid.state = st;
    }

    /// 0x7E45: a roll offered (its strip up, a cue), or over (its strip gone).
    pub(super) fn raid_roll(&mut self, r: Roll) {
        let now = self.session.now() as i64;
        if r.seconds > 0 && !self.raid.rolls.open.iter().any(|o| o.id == r.id) {
            eprintln!("play: loot roll {} offered, {} s{}", r.id, r.seconds, if r.may_need { "" } else { ", no Need" });
            self.cues.push(crate::sound::Cue::Ui(11));
        }
        self.raid.rolls.roll(r, now);
    }

    /// A roll's choice made (a click, ✕ while R2 holds the d-pad on it).
    fn make_roll(&mut self, id: u32, c: Choice) {
        eprintln!("play: loot roll {id}: {c:?}");
        self.session.send_roll_choice(id, c);
    }

    /// A raid member's marks (`raidlist`) by the id the client knows it by,
    /// as the server's state names it.
    pub(super) fn raid_marks(&self, id: u32) -> u8 {
        let st = &self.raid.state;
        if !st.in_raid() || id == 0 {
            return 0;
        }
        let mut m = 0;
        if st.leader == id {
            m |= crate::raidlist::LEADER;
        }
        if st.helper == id {
            m |= crate::raidlist::HELPER;
        }
        if st.assist == id {
            m |= crate::raidlist::ASSIST;
        }
        m
    }

    /// `DoAction`'s part: Community > Group > Raid's entries. True when the
    /// message was one of them.
    pub(super) fn raid_action(&mut self, msg: u32) -> bool {
        if msg & 0xFFFF_0000 != MSG_RAID {
            return false;
        }
        let target = (self.target != u32::MAX && self.target != 0).then_some(self.target);
        let cmd = match msg {
            MSG_LOOT_FREE | MSG_LOOT_NEED | MSG_LOOT_MASTER if self.raid.state.in_combat() => {
                self.add_chat(IN_COMBAT_RULES.to_string(), REFUSED);
                return true;
            }
            MSG_LOOT_FREE => Command::LootRule(RULE_FREE_FOR_ALL),
            MSG_LOOT_NEED => Command::LootRule(RULE_NEED_BEFORE_GREED),
            MSG_LOOT_MASTER => Command::LootRule(RULE_MASTER_LOOTER),
            MSG_LEAVE => Command::Leave,
            MSG_FLAG => Command::Flag,
            MSG_ASSIST => Command::AssistMain,
            m if m & 0xFFFF_FF00 == MSG_GROUP && m & 0xFF != 0 => {
                let Some(t) = target else {
                    self.add_chat(NO_TARGET.to_string(), REFUSED);
                    return true;
                };
                if !self.group.raid.iter().any(|r| r.id == t) {
                    self.add_chat(NOT_IN_RAID.to_string(), REFUSED);
                    return true;
                }
                Command::MoveToGroup { id: t, group: (m & 0xFF) as u8 }
            }
            MSG_INVITE | MSG_LEADER | MSG_HELPER | MSG_MAIN_ASSIST | MSG_KICK => {
                let Some(t) = target else {
                    self.add_chat(NO_TARGET.to_string(), REFUSED);
                    return true;
                };
                match msg {
                    MSG_INVITE => Command::InviteTarget(t),
                    MSG_LEADER => Command::Leader(t),
                    MSG_HELPER => Command::Helper(t),
                    MSG_MAIN_ASSIST => Command::Assist(t),
                    _ => Command::Kick(t),
                }
            }
            _ => return true,
        };
        eprintln!("play: raid command {cmd:?}");
        self.session.send_raid_command(cmd);
        true
    }

    /// The player may move the raid's members between its groups: it leads or
    /// helps, and the server sends the groups (petition 1687).
    pub(super) fn may_move_raid(&self) -> bool {
        let st = &self.raid.state;
        st.in_raid() && (st.leads() || st.helps()) && self.group.raid.iter().any(|m| m.group > 0)
    }

    /// The group of the raid member the client knows by `id` (0: none).
    fn raid_group_of(&self, id: u32) -> u8 {
        self.group.raid.iter().find(|m| m.id == id).map_or(0, |m| m.group)
    }

    /// A press (the left button, a finger) at frame point `f` in play: on a raid
    /// list's row, for one who may move members, the row is held (`Drag`). True
    /// when it was taken.
    pub(super) fn raid_drag_press(&mut self, f: [f32; 2]) -> bool {
        if self.mode != 0 || self.search.open || self.map.open || crate::hideui::hidden() || !self.may_move_raid() {
            return false;
        }
        let Some(crate::hud::Pick::Raid(id)) = self.hud.as_ref().and_then(|h| h.2.pick(f)) else { return false };
        let group = self.raid_group_of(id);
        if group == 0 {
            return false;
        }
        self.raid.drag = Some(Drag { id, group, from: f, at: f, moved: false });
        true
    }

    /// The pointer at frame point `f` while a row is held. False with none.
    pub(super) fn raid_drag_move(&mut self, f: [f32; 2]) -> bool {
        let Some(d) = self.raid.drag.as_mut() else { return false };
        d.at = f;
        d.moved |= (f[0] - d.from[0]).hypot(f[1] - d.from[1]) > DRAG_SLOP;
        true
    }

    /// Where a held row would go, let go now: a group's number (its line, the
    /// room under its rows, or one of its members). None where it would not.
    fn raid_drop_at(&self, d: &Drag) -> Option<(u8, Option<u32>)> {
        drop_on(d, self.hud.as_ref().and_then(|h| h.2.pick(d.at)), |id| self.raid_group_of(id))
    }

    /// The group lit under a held row, and the empty group a new one would be
    /// (drawn to drop on), for the HUD.
    pub(super) fn raid_drag_shown(&self) -> (Option<u8>, Option<u8>) {
        let Some(d) = self.raid.drag.filter(|d| d.moved) else { return (None, None) };
        let top = self.group.raid.iter().map(|m| m.group).max().unwrap_or(0);
        let next = (top < crate::raidlist::most_group(self.group.raid.len(), top)).then_some(top + 1);
        (self.raid_drop_at(&d).map(|g| g.0), next)
    }

    /// The held row let go: over another group, the member moved there (onto one
    /// of its members, the two swapped); not moved, a click (the member targeted
    /// as the list's click targets it). False with none held.
    pub(super) fn raid_drag_release(&mut self) -> bool {
        let Some(d) = self.raid.drag.take() else { return false };
        if !d.moved {
            if self.sees_entity(d.id) && d.id != self.target {
                self.target_id(d.id);
            }
            return true;
        }
        let Some(cmd) = drop_command(&d, self.raid_drop_at(&d)) else { return true };
        eprintln!("play: raid command {cmd:?} (dragged)");
        self.cues.push(crate::sound::Cue::Ui(13));
        self.session.send_raid_command(cmd);
        true
    }

    /// The group's Disband (`DisbandGroup`): refused, the leader kept in its
    /// raid, while the server says the raid is in combat. True when refused.
    pub(super) fn disband_refused(&mut self) -> bool {
        if !self.raid.state.in_combat() {
            return false;
        }
        self.add_chat(IN_COMBAT_DISBAND.to_string(), REFUSED);
        true
    }

    /// The start menu's Community > Group > Raid, there while the server offers
    /// the raid's windows, its rows the state's (`rows`).
    pub(super) fn raid_menu(&mut self) {
        let groups = group_counts(&self.group.raid);
        let n = self.group.raid.len();
        let want = self.session.raid_offer.then(|| rows(&self.raid.state, &groups, n));
        if self.raid.menu.as_ref() == Some(&want) {
            return;
        }
        let Some(s) = self.start.as_mut() else { return };
        s.set_raid_groups(want.as_ref().map(|(main, loot, moves)| (&main[..], &loot[..], &moves[..])));
        // the open menus' selections kept within their entries: the Raid menu's
        // rows change with the raid's state while it may be up
        s.menu.main.refit(&s.menus);
        self.raid.menu = Some(want);
    }

    /// The menus came again (a CLIENT image read): the entries are put in again.
    pub(super) fn raid_menu_again(&mut self) {
        self.raid.menu = None;
        self.raid_menu();
    }

    /// R2 held (mode 33): right gives the rolls' strips the d-pad while one is
    /// up; while they have it, their keys (left, right, up, down, ✕, △; the
    /// keyboard's arrows and Enter as those) are theirs and the chat box's
    /// scroll waits. True when the key was theirs.
    pub(super) fn roll_key(&mut self, key: i32, down: bool) -> bool {
        // the ready check's strip first: right gives it the d-pad while it is up
        let ready = &mut self.raid.ready;
        if ready.focused() || down && matches!(key, 137 | 18) && !self.raid.rolls.focused() && ready.is_open() {
            if !ready.focused() {
                ready.take_focus();
                self.cues.push(crate::sound::Cue::Ui(11));
                return true;
            }
            if !matches!(key, 136 | 137 | 17 | 18 | 134 | 13 | 132 | 27) {
                return matches!(key, 138 | 139 | 19 | 20);
            }
            if !down {
                return !matches!(key, 134 | 13);
            }
            let mut cues = Vec::new();
            let made = ready.key(key, &mut cues);
            self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
            if let Some((id, yes)) = made {
                self.make_ready(id, yes);
            }
            return true;
        }
        let rolls = &mut self.raid.rolls;
        if !rolls.focused() {
            if down && matches!(key, 137 | 18) && rolls.take_focus() {
                self.cues.push(crate::sound::Cue::Ui(11));
                return true;
            }
            return false;
        }
        if !matches!(key, 136 | 137 | 138 | 139 | 17 | 18 | 19 | 20 | 134 | 13 | 132 | 27) {
            return false;
        }
        if !down {
            // ✕'s release still ends an attack held from before (`chat_scroll_key`)
            return !matches!(key, 134 | 13);
        }
        let mut cues = Vec::new();
        let made = rolls.key(key, &mut cues);
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        if let Some((id, c)) = made {
            self.make_roll(id, c);
        }
        true
    }

    /// R2 let go: the d-pad back from the strips.
    pub(super) fn roll_let_go(&mut self) {
        self.raid.rolls.let_go();
        self.raid.ready.let_go();
    }

    /// A roll's strip is up (the keyboard's right, with C held, reaches it).
    pub fn rolls_up(&self) -> bool {
        self.raid.rolls.is_open() || self.raid.ready.is_open()
    }

    /// The strips have the d-pad (the keyboard's left, right and Enter are theirs).
    pub fn rolls_focused(&self) -> bool {
        self.raid.rolls.focused() || self.raid.ready.focused()
    }

    /// The rolls' strips are where frame point `f` is: a click there is
    /// theirs, a choice on one made. False when it is not on a strip.
    pub(super) fn roll_point(&mut self, f: [f32; 2]) -> bool {
        if matches!(self.mode, 0 | 33 | 38) && self.raid.ready.covers(f) {
            if let Some((id, yes)) = self.raid.ready.point(f) {
                self.cues.push(crate::sound::Cue::Ui(13));
                self.make_ready(id, yes);
            }
            return true;
        }
        if !self.rolls_shown() || !self.raid.rolls.covers(f) {
            return false;
        }
        if let Some((id, c)) = self.raid.rolls.point(f) {
            self.cues.push(crate::sound::Cue::Ui(13));
            self.make_roll(id, c);
        }
        true
    }

    /// The strips are drawn: in play, the chat box's history and the manual
    /// camera (the windows over them keep the rolls until they close).
    fn rolls_shown(&self) -> bool {
        matches!(self.mode, 0 | 33 | 38) && self.raid.rolls.is_open()
    }

    /// The rolls' strips for one of the HUD's frames, and while R2 holds the
    /// d-pad on one, its item's description beside them. `r` is in the middle
    /// anchor (the frame's own places, as a pointer's).
    pub(super) fn run_rolls(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = self.session.now() as i64;
        self.raid.rolls.expire(now);
        self.raid.ready.expire(now);
        self.raid.rolls.skip = self.raid.ready.is_open() as usize;
        if matches!(self.mode, 0 | 33 | 38) {
            self.raid.ready.draw(vui, r, now);
        }
        if !self.rolls_shown() {
            return;
        }
        let flags = net::session::EXPANSION_FLAGS;
        self.raid.rolls.draw(vui, r, now, &|it| crate::inventory::item_icon(it, flags) as i32);
        let Some(it) = self.raid.rolls.lit_item() else { return };
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let info = crate::infotext::Info {
            words: &vui.words,
            inv: &self.dump.inventory,
            hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
            base: &self.stats.base,
            class: pl.class,
            level: pl.level,
            attack_words: &attack_words,
        };
        let text = crate::ui::wide(&info.inspect_item(it));
        let mut w = vi::ui::Window::default();
        let rows = w.count_lines(vui, r, &text, 210).clamp(1, 14);
        w.init(vui, r, [310, 240], 250, rows, 1);
        w.open(true);
        w.draw(vui, r);
        w.text_box(vui, r, &text, 20, 0, 210, rows, [1.0; 4], 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::{Player, Session};

    fn state(leader: u32, flags: u8, rule: u8) -> State {
        State { leader, helper: 0, assist: 0, rule, flags }
    }

    #[test]
    fn the_raid_menus_rows_by_who_one_is() {
        let msgs = |st: &State| rows(st, &[], 0).0.iter().map(|r| r.msg).collect::<Vec<_>>();
        assert_eq!(msgs(&State::default()), [MSG_INVITE, MSG_FLAG], "in no raid: begin one");
        assert_eq!(msgs(&state(5, 0, 0)), [MSG_ASSIST, MSG_LEAVE], "a member");
        assert_eq!(msgs(&state(5, 4, 0)), [MSG_INVITE, MSG_FLAG, MSG_KICK, MSG_ASSIST, MSG_LEAVE], "the helper");
        let (lead, loot, moves) = rows(&state(5, 2, RULE_NEED_BEFORE_GREED), &[], 0);
        assert!(moves.is_empty(), "a server that sends no groups: no Move to Group");
        assert_eq!(lead.iter().map(|r| r.msg).collect::<Vec<_>>(), [0, MSG_INVITE, MSG_FLAG, MSG_KICK, MSG_LEADER, MSG_HELPER, MSG_MAIN_ASSIST, MSG_ASSIST, MSG_LEAVE], "the leader");
        assert_eq!(loot.iter().map(|r| (r.msg, r.tail.as_ref(), r.dim)).collect::<Vec<_>>(), [(MSG_LOOT_FREE, "", false), (MSG_LOOT_NEED, " *", false), (MSG_LOOT_MASTER, "", false)]);
        let (_, fighting, _) = rows(&state(5, 3, RULE_MASTER_LOOTER), &[], 0);
        assert!(fighting.iter().all(|r| r.dim), "in combat: dimmed");
        assert_eq!(fighting[2].tail, " *");
        assert!(rows(&state(5, 0, 0), &[], 0).1.is_empty(), "a member has no loot rules");
    }

    /// Play on a session that offered the raid's windows, recording what it sends.
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
        s.raid_offer = true;
        Play::new(s, Player::default(), Box::default())
    }

    /// The messages sent since the last look (each its bytes, its opcode first).
    fn taken(p: &mut Play) -> Vec<Vec<u8>> {
        std::mem::take(p.session.record.as_mut().unwrap()).into_iter().map(|(_, m)| m).collect()
    }

    /// Of them, the raid's (go-eqoa-server's 0x7Exx).
    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        taken(p).into_iter().filter(|m| m.get(1) == Some(&0x7E)).collect()
    }

    #[test]
    fn the_raid_entries_send_their_commands() {
        let mut p = play();
        p.target = 0x0012_3456;
        assert!(p.raid_action(MSG_KICK));
        assert!(p.raid_action(MSG_LOOT_MASTER));
        assert!(p.raid_action(MSG_LEAVE));
        assert_eq!(sent(&mut p), [Command::Kick(0x0012_3456).bytes(), Command::LootRule(2).bytes(), Command::Leave.bytes()]);
        // in combat the rules are refused here, nothing sent; and with no target
        p.raid_state(state(5, 3, 0));
        assert!(p.raid_action(MSG_LOOT_FREE));
        p.target = u32::MAX;
        assert!(p.raid_action(MSG_LEADER));
        assert!(sent(&mut p).is_empty());
        assert!(p.chat.iter().any(|(t, _)| t == IN_COMBAT_RULES) && p.chat.iter().any(|(t, _)| t == NO_TARGET));
        // the control: none of the game's messages is the raid's
        assert!(!p.raid_action(0x0705_0000));
    }

    #[test]
    fn a_disband_in_combat_is_refused_here() {
        let mut p = play();
        p.group.in_group = true;
        p.group.leader = true;
        p.group.members = vec![Default::default(); 2];
        p.raid_state(state(5, 3, 0));
        p.group_action(0x0705_0000);
        assert!(p.group.in_group, "still in its raid");
        assert!(taken(&mut p).iter().all(|m| m[..2] != [0x27, 0x06]), "no 0x0627");
        assert!(p.chat.iter().any(|(t, _)| t == IN_COMBAT_DISBAND));
        // the control: out of combat the game's disband
        p.raid_state(state(5, 2, 0));
        p.group_action(0x0705_0000);
        assert!(!p.group.in_group);
        assert!(taken(&mut p).iter().any(|m| m[..2] == [0x27, 0x06]));
    }

    fn roll(id: u32) -> Roll {
        let mut it = net::dump::NetItem::default();
        it.item.name = format!("item {id}");
        Roll { id, seconds: 120, may_need: true, item: Some(Box::new(it)) }
    }

    /// Held R2 (mode 33) and right: the strips take the d-pad and ✕ makes the
    /// lit choice; R2 let go hands it back. The controls: without a roll up,
    /// right is the chat box's as before, and nothing is sent.
    #[test]
    fn r2_and_right_reach_the_rolls() {
        let mut p = play();
        p.mode = 33;
        assert!(!p.roll_key(137, true), "no roll: the chat box's");
        p.raid_roll(roll(7));
        p.raid_roll(roll(8));
        assert!(p.roll_key(137, true) && p.raid.rolls.focused());
        assert!(p.roll_key(137, false), "its release theirs too");
        p.roll_key(139, true);
        p.roll_key(137, true);
        p.roll_key(134, true);
        assert!(!p.roll_key(134, false), "✕'s release goes on (the attack's)");
        assert_eq!(sent(&mut p), [net::raid::roll_choice(8, Choice::Greed)]);
        assert_eq!(p.raid.rolls.open.len(), 1);
        p.roll_let_go();
        assert!(!p.raid.rolls.focused());
        assert!(!p.roll_key(134, true), "let go: ✕ is not theirs");
        assert!(sent(&mut p).is_empty());
        // the server's end closes the strip
        p.raid_roll(Roll { id: 7, ..Default::default() });
        assert!(!p.raid.rolls.is_open());
    }

    /// The ready check's strip (petition 1711): with a roll up too, R2 and right
    /// reach the check first, ✕ answers it (0x7E56) and the next right the
    /// rolls; a click answers too; the server's end closes it. The control: a
    /// session that made no offer is sent nothing.
    #[test]
    fn the_ready_check_before_the_rolls() {
        let mut p = play();
        p.session.ready_offer = true;
        p.mode = 33;
        p.raid_roll(roll(7));
        p.ready_check(net::readycheck::Check { id: 4, seconds: 30, asker: "Ann".into() });
        assert!(p.rolls_up());
        assert!(p.roll_key(137, true) && p.raid.ready.focused() && !p.raid.rolls.focused());
        p.roll_key(137, true); // No
        p.roll_key(136, true); // Yes again
        p.roll_key(134, true);
        assert_eq!(sent(&mut p), [net::readycheck::reply(4, true)]);
        assert!(!p.raid.ready.is_open());
        assert!(p.roll_key(137, true) && p.raid.rolls.focused(), "then the rolls");
        p.roll_let_go();
        // a click on No where it was drawn
        p.ready_check(net::readycheck::Check { id: 5, seconds: 30, asker: "Ann".into() });
        let (vui, mut nr) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        p.raid.ready.draw(&vui, &mut nr, 0);
        assert!(p.roll_point([(crate::readywnd::AT[0] + 131) as f32, (crate::readywnd::AT[1] + 33) as f32]));
        assert_eq!(sent(&mut p), [net::readycheck::reply(5, false)]);
        // the server's end closes one
        p.ready_check(net::readycheck::Check { id: 6, seconds: 30, asker: "Ann".into() });
        p.ready_check(net::readycheck::Check { id: 6, seconds: 0, asker: String::new() });
        assert!(!p.raid.ready.is_open());
        // no offer: an answer sends nothing
        p.session.ready_offer = false;
        p.ready_check(net::readycheck::Check { id: 8, seconds: 30, asker: "Ann".into() });
        p.roll_key(137, true);
        p.roll_key(134, true);
        assert!(sent(&mut p).is_empty());
    }

    /// The server's raid commands, typed (or a quick chat line's), go to it as
    /// typed in a 0x000B: none is one of the client's own; the control, `/g`, is
    /// the client's (its channel set for the line, the words alone sent).
    #[test]
    fn the_raid_commands_go_out_as_typed() {
        let mut p = play();
        let said = |p: &mut Play| -> Vec<String> {
            taken(p)
                .into_iter()
                .filter(|m| m[..2] == [0x0B, 0x00])
                .map(|m| {
                    let mut r = net::stream::Reader::new(&m[2..]);
                    r.wide(1024)
                })
                .collect()
        };
        let lines = [
            "/raidaoe", "/raid", "/raidinvite Bob", "/raidinvite", "/raidleave", "/rsay pull on three", "/raidloot needgreed", "/raidleader Bob",
            "/raidhelper none", "/raidkick Bob", "/raidassist", "/raidassist Bob", "/invite Bob", "/target jib", "/tar jib", "/help", "/need 3",
            "/greed", "/pass 12", "/assign 2 Bob", "/abandon",
        ];
        for line in lines {
            p.chat_line(line);
            assert_eq!(said(&mut p), [line.to_string()], "{line}");
        }
        p.chat_line("/g hello");
        assert_eq!(said(&mut p), ["hello".to_string()], "the control: the client's own /g");
    }

    /// The server's /target, /tar and /raidassist answer with 0x007B: the
    /// client targets the id and asks the server of it (0x0010 with the token),
    /// as CLIENT's `Msg007B`; -1 targets nothing and asks nothing.
    #[test]
    fn a_targeting_answer_targets_and_asks() {
        let mut p = play();
        p.msg_007b(0x0012_3456);
        let asked: Vec<Vec<u8>> = taken(&mut p).into_iter().filter(|m| m[..2] == [0x10, 0x00]).collect();
        assert_eq!(p.target, 0x0012_3456);
        assert_eq!(asked, [vec![0x10, 0x00, 0, 0, 0, 0, 0x56, 0x34, 0x12, 0x00]]);
        p.msg_007b(u32::MAX);
        assert!(taken(&mut p).iter().all(|m| m[..2] != [0x10, 0x00]), "the control: none asked");
        assert_eq!(p.target, 0x0012_3456);
    }

    /// The lead handed on (`/raidleader`): the old leader's client stops taking
    /// itself for the leader, its Disband the game's word 639 and nothing sent;
    /// the new one's leads. The control: out of a raid the game's flag stands.
    #[test]
    fn the_lead_is_the_raid_states() {
        let mut p = play();
        p.group.in_group = true;
        p.group.leader = true;
        p.group.members = vec![Default::default(); 2];
        p.raid_state(state(0, 0, 0));
        assert!(p.group.leader, "no raid: the game's own leader flag");
        p.raid_state(state(5, 0, 0));
        assert!(!p.group.leader, "the lead handed on");
        p.group_action(0x0705_0000);
        assert!(p.group.in_group && taken(&mut p).iter().all(|m| m[..2] != [0x27, 0x06]), "no disband sent");
        p.raid_state(state(5, 2, 0));
        assert!(p.group.leader, "led again");
    }

    #[test]
    fn the_marks_follow_the_state() {
        let mut p = play();
        assert_eq!(p.raid_marks(5), 0, "no state: no marks (the control)");
        p.raid_state(State { leader: 5, helper: 6, assist: 5, rule: 0, flags: 0 });
        assert_eq!(p.raid_marks(5), crate::raidlist::LEADER | crate::raidlist::ASSIST);
        assert_eq!(p.raid_marks(6), crate::raidlist::HELPER);
        assert_eq!(p.raid_marks(7), 0);
        assert_eq!(p.raid_marks(0), 0, "no id");
    }

    fn member(id: u32, group: u8) -> net::session::RaidMember {
        net::session::RaidMember { id, name: format!("m{id}"), group, ..Default::default() }
    }

    /// Petition 1687's menu: the leader and the helper have Move to Group after
    /// Kick on a server that sends the groups, a row a group up to one past what
    /// the raid needs, a full one dimmed; a member has none, nor anyone on a
    /// server that sends no groups (the controls).
    #[test]
    fn move_to_group_is_the_leaders_and_helpers() {
        let raid: Vec<_> = (1..=9).map(|i| member(i, ((i - 1) / 4 + 1) as u8)).collect();
        let groups = group_counts(&raid);
        assert_eq!(groups, [(1, 4), (2, 4), (3, 1)]);
        let (lead, _, moves) = rows(&state(5, 2, 0), &groups, 9);
        let k = lead.iter().position(|r| r.msg == MSG_KICK).unwrap();
        assert_eq!(lead[k + 1].msg, crate::startmenu::RAID_GROUPS_SUB);
        assert_eq!(
            moves.iter().map(|r| (r.msg, r.text.to_string(), r.tail.to_string(), r.dim)).collect::<Vec<_>>(),
            [
                (MSG_GROUP | 1, "Group 1".into(), " (4/4)".into(), true),
                (MSG_GROUP | 2, "Group 2".into(), " (4/4)".into(), true),
                (MSG_GROUP | 3, "Group 3".into(), " (1/4)".into(), false),
                (MSG_GROUP | 4, "Group 4".into(), " (0/4)".into(), false),
            ]
        );
        let (help, _, hmoves) = rows(&state(5, 4, 0), &groups, 9);
        assert!(help.iter().any(|r| r.msg == crate::startmenu::RAID_GROUPS_SUB) && hmoves.len() == 4, "the helper");
        let (member_rows, _, none) = rows(&state(5, 0, 0), &groups, 9);
        assert!(none.is_empty() && !member_rows.iter().any(|r| r.msg == crate::startmenu::RAID_GROUPS_SUB), "a member: none");
    }

    /// Move to Group's row sends the target's move; a target not in the raid
    /// and no target are said here, nothing sent.
    #[test]
    fn move_to_group_sends_the_targets_move() {
        let mut p = play();
        p.group.raid = vec![member(0x0012_3456, 1), member(7, 2)];
        p.target = 0x0012_3456;
        assert!(p.raid_action(MSG_GROUP | 2));
        assert_eq!(sent(&mut p), [Command::MoveToGroup { id: 0x0012_3456, group: 2 }.bytes()]);
        p.target = 99;
        assert!(p.raid_action(MSG_GROUP | 2));
        p.target = u32::MAX;
        assert!(p.raid_action(MSG_GROUP | 2));
        assert!(sent(&mut p).is_empty());
        assert!(p.chat.iter().any(|(t, _)| t == NOT_IN_RAID) && p.chat.iter().any(|(t, _)| t == NO_TARGET));
    }

    /// A held row let go: on another group, moved; on a member of another,
    /// swapped; on its own group, nothing of the list, or before it moved,
    /// nothing (the controls).
    #[test]
    fn a_dropped_row_moves_or_swaps() {
        use crate::hud::Pick;
        let group_of = |id: u32| match id {
            1 | 2 => 1,
            3 => 2,
            _ => 0,
        };
        let d = Drag { id: 1, group: 1, from: [0.0; 2], at: [30.0, 40.0], moved: true };
        assert_eq!(drop_command(&d, drop_on(&d, Some(Pick::RaidGroup(3)), group_of)), Some(Command::MoveToGroup { id: 1, group: 3 }));
        assert_eq!(drop_command(&d, drop_on(&d, Some(Pick::Raid(3)), group_of)), Some(Command::Swap(1, 3)));
        assert_eq!(drop_on(&d, Some(Pick::Raid(2)), group_of), None, "its own group");
        assert_eq!(drop_on(&d, Some(Pick::RaidGroup(1)), group_of), None, "its own group's line");
        assert_eq!(drop_on(&d, Some(Pick::Me), group_of), None);
        assert_eq!(drop_on(&d, None, group_of), None);
        assert_eq!(drop_on(&Drag { moved: false, ..d }, Some(Pick::RaidGroup(3)), group_of), None, "a click");
    }

    /// Only one who leads or helps, on a server that sends the groups, may move.
    #[test]
    fn only_the_leader_and_helper_may_move() {
        let mut p = play();
        p.group.raid = vec![member(1, 1), member(2, 2)];
        p.raid_state(state(5, 2, 0));
        assert!(p.may_move_raid());
        p.raid_state(state(5, 4, 0));
        assert!(p.may_move_raid(), "the helper");
        p.raid_state(state(5, 0, 0));
        assert!(!p.may_move_raid(), "a member");
        p.raid_state(state(5, 2, 0));
        p.group.raid = vec![member(1, 0), member(2, 0)];
        assert!(!p.may_move_raid(), "no groups sent");
    }
}
