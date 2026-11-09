//! The GM's channel in play: `VIClientPart382E8` (`viadmin.cpp`, crate
//! `admin`: 0x0700 in, 0x0701 out), the start menu's TEST entry it enables
//! (`OpenStartMenu`), the TEST menu's own `DoAction` cases (the engine
//! toggles: Extrapolate, Interpolate, the day-night cycle, the personal
//! light, the weather, Flora, the HUD, Fast run, Show BPS, the position, the
//! stats), and the windows it opens: `VIWndBuffPC` (mode 27, `buffpc`),
//! `VIWndGoto` (mode 4, `gotownd`), `VIWndNetStat` and `DrawNetStats`
//! (`netstat`). A child of `play`, for `Play`'s fields.

use super::{Atmos, Play};
use crate::admin::{Out, Part, Recv};
use crate::buffpc::{BuffPc, BuffUp};
use crate::gotownd::{Goto, GotoUp, Worlds};
use crate::netstat::{ChannelRec, NetStat};
use crate::walker::{Placing, Walker};
use vi::fpu::Ee;

/// `VIClientPart382E8`, its windows, and the `VIClient` fields the TEST
/// menu's own entries toggle.
pub struct Admin {
    pub part: Part,
    buffs: BuffPc,
    goto: Goto,
    net: NetStat,
    /// `m_channels`: the server-load channels 0x44..0x6B (`Channel44`)
    channels: Vec<Option<ChannelRec>>,
    keys: Vec<(i32, bool)>,
    /// the mode opened, its window focused with the next HUD frame
    opening: Option<i32>,
    /// `m_p50308`..`m_p50314`: a move asked for, done at the frame's end:
    /// the world and the grid square's centre (`MoveToGrid`'s column and row
    /// plus one; `MoveToOrigin`'s 0, 0)
    go: Option<(i32, i32, i32)>,
    /// `m_p2BD48`: `UpdateCreatures` walks the creatures (Extrapolate)
    walks: bool,
    /// `m_p2BD4C`: `Extrapolate` glides them on (Interpolate)
    glides: bool,
    /// `m_p00064` clear: `UpdateTimeOfDay` stopped, the hour it held
    held_hour: Option<f32>,
    /// `m_p00068`: the personal light's mode (0 by the hour, 1 on, 2 off)
    light: i32,
    /// `m_p00070`: Fast run
    fast: bool,
    /// `m_p00084`: Toggle Stats (`VIWndNetStat`)
    stats: bool,
    /// `m_p502FC`: Show BPS
    bps: bool,
    /// `m_p00080`: DISPLAY POS (`DrawPosition`), its lines made with the
    /// walker
    position: bool,
    position_lines: Vec<(Vec<u16>, [i32; 2])>,
    /// the server's names for the TRIGGER entries (0x7E08, not retail),
    /// shown in their place (`trigger_labels`)
    pub triggers: [Option<crate::admin::Trigger>; crate::admin::TRIGGERS],
    /// the server's whole list of triggers (0x7E36, not retail), the TRIGGER
    /// menu a line a trigger in place of its eight (`set_trigger_list`); none
    /// from a server that sends only the names
    pub list: Option<Vec<crate::admin::Listed>>,
}

impl Default for Admin {
    /// `VIClientPart382E8`'s constructor and `VIClient`'s values for the
    /// rest (the creatures walked and glided, the day-night cycle running).
    fn default() -> Self {
        Admin { part: Part::default(), buffs: BuffPc::default(), goto: Goto::new(), net: NetStat::default(), channels: vec![None; 40], keys: Vec::new(), opening: None, go: None, walks: true, glides: true, held_hour: None, light: 0, fast: false, stats: false, bps: false, position: false, position_lines: Vec::new(), triggers: Default::default(), list: None }
    }
}

/// The TRIGGER entries' labels on `menus` (keyed by `Do2`'s message and the
/// entry's id): a named one's name and tail, dimmed when it is off; the rest
/// their word.
pub fn set_trigger_labels(menus: &mut crate::client_image::Menus, triggers: &[Option<crate::admin::Trigger>]) {
    menus.labels.retain(|k, _| k.0 != crate::admin::TRIGGER_ENTRY);
    for (i, t) in triggers.iter().enumerate() {
        if let Some(t) = t {
            let label = crate::client_image::Label { text: crate::ui::wide(&t.name), tail: crate::ui::wide(t.tail()), dim: !t.on };
            menus.labels.insert((crate::admin::TRIGGER_ENTRY, i as i32), label);
        }
    }
}

/// The TRIGGER menu as the server's whole list makes it (0x7E36, not
/// retail): a line a trigger that is on, by slot, each the list's message
/// (`MSG_TRIGGER_SLOT`) with the slot for its id, so it fires its own slot
/// (0x7E37), labelled with the slot and the name (an unnamed one's slot and
/// `unnamed`, the game's word), " *" after one running; seven shown at a time,
/// the rest scrolled to as in any of the game's longer menus. With no list,
/// or no trigger on, the menu's lines as CLIENT has them, the eight TRIGGERs.
/// The menu is the one CLIENT's TRIGGER lines are in; those lines are kept
/// (`Menus::trigger_lines`) to be put back.
pub fn set_trigger_list(menus: &mut crate::client_image::Menus, list: Option<&[crate::admin::Listed]>, unnamed: &str) {
    use crate::admin::{MSG_TRIGGER_SLOT, TRIGGER_ENTRY};
    use crate::client_image::{Label, MenuEntry};
    menus.labels.retain(|k, _| k.0 != MSG_TRIGGER_SLOT);
    if menus.trigger_lines.is_none() {
        let Some(k) = menus.list.iter().position(|m| m.entries.iter().any(|e| e.msg == TRIGGER_ENTRY)) else { return };
        menus.trigger_lines = Some((k, menus.list[k].entries.clone()));
    }
    let Some((k, lines)) = menus.trigger_lines.clone() else { return };
    let mut on: Vec<&crate::admin::Listed> = list.unwrap_or_default().iter().filter(|t| t.on).collect();
    on.sort_by_key(|t| t.slot);
    if on.is_empty() {
        menus.list[k].entries = lines;
        return;
    }
    let like = lines.first().copied().unwrap_or_default();
    menus.list[k].entries = on.iter().map(|t| MenuEntry { msg: MSG_TRIGGER_SLOT, id: t.slot as i32, ..like }).collect();
    for t in on {
        let name = if t.name.is_empty() { unnamed } else { t.name.as_str() };
        let label = Label { text: crate::ui::wide(&format!("{} {name}", t.slot)), tail: crate::ui::wide(t.tail()), dim: false };
        menus.labels.insert((MSG_TRIGGER_SLOT, t.slot as i32), label);
    }
}

/// The modes the GM's windows own: 4 the Goto window, 27 Buff PC.
pub fn owns(mode: i32) -> bool {
    matches!(mode, 4 | 27)
}

/// `ColorF(VIColor32(185, 80, 250, 255))`: `DrawPosition`'s.
const POSITION_COLOR: [f32; 4] = [185.0 / 255.0, 80.0 / 255.0, 250.0 / 255.0, 1.0];

impl Admin {
    /// `UpdateCreatures` walks the creatures (Extrapolate on).
    pub fn walks(&self) -> bool {
        self.walks
    }
}

impl Play {
    /// A key for a GM window, handled with the next HUD frame.
    pub fn admin_key(&mut self, key: i32, down: bool) {
        self.admin.keys.push((key, down));
    }

    /// A pointer at frame point `f` on a GM window (`pointer`): the keys to
    /// press.
    pub fn admin_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        match self.mode {
            4 => self.admin.goto.point(f),
            27 => self.admin.buffs.point(f),
            _ => Vec::new(),
        }
    }

    /// The mode is a GM window's.
    pub fn admin_owns(&self) -> bool {
        owns(self.mode)
    }

    /// A GM window's edit box is up (it takes the keyboard): none of them
    /// has one (their texts are the game window's edit box, mode 2).
    pub fn admin_typing(&self) -> bool {
        false
    }

    /// The opcodes the GM's channel reads (0x0700, `Recv0700`; and the
    /// server's trigger names, 0x7E08, and its whole list, 0x7E36); true
    /// when handled.
    pub(super) fn admin_opcode(&mut self, op: u16, data: &[u8]) -> bool {
        if op == crate::admin::OP_TRIGGER_LIST {
            match crate::admin::read_trigger_list(data) {
                Some(list) => {
                    let on = list.iter().filter(|t| t.on).count();
                    let running: Vec<String> = list.iter().filter(|t| t.running).map(|t| t.slot.to_string()).collect();
                    let slots: Vec<String> = list.iter().filter(|t| t.on).map(|t| t.slot.to_string()).collect();
                    eprintln!("gm: the trigger list: {} triggers, {on} on (slots {}), running: {}", list.len(), slots.join(" "), if running.is_empty() { "none".into() } else { running.join(" ") });
                    self.admin.list = Some(list);
                    self.trigger_labels();
                }
                None => eprintln!("gm: 0x7E36 of {} bytes cut short: the trigger list kept", data.len()),
            }
            return true;
        }
        if op == crate::admin::OP_TRIGGER_NAMES {
            match crate::admin::read_triggers(data) {
                Some(t) => {
                    let state = |t: &crate::admin::Trigger| [(!t.on, " (off)"), (t.running, " (running)")].iter().filter(|s| s.0).map(|s| s.1).collect::<String>();
                    let named: Vec<String> = t.iter().enumerate().filter_map(|(i, t)| t.as_ref().map(|t| format!("{} {}{}", i + 1, t.name, state(t)))).collect();
                    eprintln!("gm: trigger names: {}", if named.is_empty() { "none".into() } else { named.join(", ") });
                    self.admin.triggers = t;
                    self.trigger_labels();
                }
                None => eprintln!("gm: 0x7E08 of {} bytes cut short: the trigger names kept", data.len()),
            }
            return true;
        }
        if op != crate::admin::OP_ADMIN_IN {
            return false;
        }
        match self.admin.part.recv(data) {
            Some(Recv::Enabled(v)) => eprintln!("gm: the GM's channel {} (0x0700 35: {v})", if v > 0 { "on" } else if v != 0 { "shown, commands off" } else { "off" }),
            Some(Recv::Prompt(name)) => self.prompt_with_name(&name),
            None => eprintln!("gm: 0x0700 sub-op {:?}: not one Recv0700 reads", data.first()),
        }
        true
    }

    /// The TRIGGER entries' labels as the server named them (an entry it
    /// left unnamed shows the game's word), or the menu a line a trigger as
    /// its whole list makes it: set on the start menu's menus, now and
    /// whenever CLIENT's image makes them anew (`client_data`), an open menu's
    /// selection kept within its lines.
    pub(super) fn trigger_labels(&mut self) {
        let unnamed = self.words.as_ref().map_or_else(|| "TRIGGER".to_string(), |w| w.get(507, 0).to_string());
        let Some(s) = self.start.as_mut() else { return };
        set_trigger_labels(&mut s.menus, &self.admin.triggers);
        set_trigger_list(&mut s.menus, self.admin.list.as_deref(), &unnamed);
        s.menu.main.refit(&s.menus);
    }

    /// `PromptWithName(name)`: word 1189, a space and the name (128 at
    /// most) as `Prompt(text, 1, 3, 0)`, over whatever was open: Yes is
    /// 0x0701 sub-op 48, No 49 (`dialog_key`).
    fn prompt_with_name(&mut self, name: &[u16]) {
        if self.mode == 13 {
            self.close_start_menu(0);
        }
        let mut text = self.words.as_ref().map_or_else(Vec::new, |w| crate::ui::wide(w.get(1189, 0)));
        text.push(b' ' as u16);
        text.extend_from_slice(name);
        text.truncate(127);
        let now = self.session.now() as i64;
        self.mbox.prompt(&text, 1, 3, 0, now);
        self.mode = 14;
    }

    /// The prompt's answer (`OnMessage` 268 / 260 with `m_p1` 3):
    /// `Send0701a` / `Send0701b`.
    pub(super) fn admin_answer(&mut self, yes: bool) {
        self.admin_send(Part::answer(yes));
    }

    /// A message for the server, logged.
    fn admin_send(&mut self, m: Vec<u8>) {
        eprintln!("gm: sent 0x{:04X} sub-op {} ({} bytes)", u16::from_le_bytes([m[0], m[1]]), m[2], m.len());
        self.session.send(&m);
    }

    /// What a command asked for, in order.
    fn admin_outs(&mut self, outs: Vec<Out>) {
        for o in outs {
            match o {
                Out::Send(m) => self.admin_send(m),
                // OpenPanel27 -> OpenBuffs: SetMode(0), mode 27
                Out::Buffs => {
                    self.mode = 27;
                    self.admin.opening = Some(27);
                    self.admin.keys.clear();
                }
                // ShowNameClear: the edit box, 256 long, empty
                Out::Edit(id, word) => self.open_edit(id, 256, word, false),
                // ShowTargetPlayerName: 24 long, the target's name when a
                // player (`GetTargetPlayerName`)
                Out::EditTarget(id, word) => {
                    let name = self.gm_target_player().unwrap_or_default();
                    self.open_edit(id, 24, word, false);
                    self.edit.set_text(&crate::ui::wide(&name));
                }
                Out::Word(w) => self.show_word(w),
            }
        }
    }

    /// `GetTargetPlayerName`: the target's name when it is a creature of a
    /// player (`m_p256` 2).
    fn gm_target_player(&self) -> Option<String> {
        let e = self.entities.slots.iter().flatten().find(|e| e.id == self.target)?;
        e.creature.as_ref().filter(|c| c.p256 == 2).map(|c| c.name.clone())
    }

    /// `DoText(id, text)` (the edit box's 259, a slash command's rest).
    pub(super) fn admin_text(&mut self, id: u32, text: &[u16]) {
        let outs = self.admin.part.do_text(id, text, self.target);
        self.admin_outs(outs);
    }

    /// `/create`, `/mastery`, `/tradelvl`: `DoText` while `IsEnabled() >
    /// 0`; else the line is taken and nothing is done.
    pub(super) fn admin_slash(&mut self, id: u32, rest: &str) {
        if self.admin.part.commands() {
            self.admin_text(id, &crate::ui::wide(rest));
        }
    }

    /// `DoAction`'s first look (the GM's part: `Show`, `Do2`) and its cases
    /// that only the TEST menu reaches; true when it was one.
    pub(super) fn admin_action(&mut self, id: u32, arg: i32) -> bool {
        if crate::admin::is_command(id) {
            let me = self.self_id32().unwrap_or(u32::MAX);
            let outs = self.admin.part.show(id, arg, self.target, me);
            self.admin_outs(outs);
            return true;
        }
        if crate::admin::is_command2(id) {
            if let Some(m) = crate::admin::do2(id, arg) {
                self.admin_send(m);
            }
            return true;
        }
        // a line of the server's whole list: its own slot (0x7E37)
        if id == crate::admin::MSG_TRIGGER_SLOT {
            let slot = arg as u32;
            eprintln!("gm: fired trigger {slot} (0x7E37)");
            self.session.send(&crate::admin::fire_slot(slot));
            return true;
        }
        let word = |p: &Play, n: i32| p.words.as_ref().map_or_else(|| format!("(word {n})"), |w| w.get(n, 0).to_string());
        let on_off = |p: &Play, what: i32, on: bool| format!("{}: {}", word(p, what), word(p, if on { 551 } else { 552 }));
        match id {
            // Extrapolate (m_p2BD48), Interpolate (m_p2BD4C)
            0x0105_0400 => {
                self.admin.walks = !self.admin.walks;
                let t = on_off(self, 989, self.admin.walks);
                self.add_chat(t, [1.0; 4]);
            }
            0x0105_0500 => {
                self.admin.glides = !self.admin.glides;
                let t = on_off(self, 990, self.admin.glides);
                self.add_chat(t, [1.0; 4]);
            }
            // OpenGoto, or MoveToOrigin in a world not on the disc
            0x0509_1006 => {
                if self.dump.world_not_on_disc {
                    self.admin.go = Some((0, 0, 0));
                } else {
                    self.mode = 4;
                    self.admin.opening = Some(4);
                    self.admin.keys.clear();
                }
            }
            // Fast run (m_p00070): the run speed 15, or SetSpeed(the
            // mastery part's)
            0x0509_1007 => {
                self.admin.fast = !self.admin.fast;
                let t = on_off(self, 831, self.admin.fast);
                self.add_chat(t, [1.0; 4]);
                if self.admin.fast {
                    self.speed = 15.0;
                } else {
                    let v = self.cm.m.speed;
                    self.set_speed(v);
                }
            }
            // the radial flora's mode: the client's Flora (this run's, not
            // kept)
            0x0509_1008 => {
                let mut c = crate::settings::client();
                c.flora = !c.flora;
                crate::settings::set_client(c);
                let t = on_off(self, 1597, c.flora);
                self.add_chat(t, [1.0; 4]);
            }
            // the day-night cycle (m_p00064): UpdateTimeOfDay stopped or
            // let run, said with the hour
            0x0509_0100 => {
                // m_timeOfDay as it stands: the held hour, or the clock's
                let hour = self.session.time_of_day().map_or(0.0, |h| h.0);
                let shown = self.admin.held_hour.unwrap_or(hour);
                let running = self.admin.held_hour.is_some();
                self.admin.held_hour = if running { None } else { Some(shown) };
                let t = format!("{} ({:05.2})", on_off(self, 832, running), shown);
                self.add_chat(t, [1.0; 4]);
            }
            0x0509_0200 => self.weather.push(Atmos::Weather(0, false)),
            0x0509_0300 => self.weather.push(Atmos::Weather(2, false)),
            0x0509_0400 => self.weather.push(Atmos::Weather(5, false)),
            // the personal light's mode, 0 -> 1 (On) -> 2 (Off) -> 0 (Auto)
            0x0509_0410 => {
                let (next, w) = match self.admin.light {
                    0 => (1, 551),
                    1 => (2, 552),
                    _ => (0, 834),
                };
                self.admin.light = next;
                self.light.mode = next;
                let t = format!("{}: {}", word(self, 833), word(self, w));
                self.add_chat(t, [1.0; 4]);
            }
            // ToggleHud: what is over the world hidden or shown
            0x0509_1000 => crate::hideui::toggle(),
            0x0509_1004 => self.admin.bps = !self.admin.bps,
            0x0905_0000 => self.admin.position = !self.admin.position,
            0x0B0E_0002 => self.admin.stats = !self.admin.stats,
            _ => return false,
        }
        true
    }

    /// `UpdateTimeOfDay` stopped by the day-night entry: the hour it holds.
    pub fn frozen_hour(&self) -> Option<f32> {
        self.admin.held_hour
    }

    /// A server-load state channel (0x44..0x6B, `Channel44`).
    pub(super) fn admin_channel44(&mut self, channel: u8, data: &[u8]) {
        if let Some(k) = (channel as usize).checked_sub(0x44).filter(|&k| k < 40) {
            self.admin.channels[k] = ChannelRec::read(data);
        }
    }

    /// `ApplySimulation`, after each 0x40: the GM's loss, latency and
    /// jitter onto the connection (`connection_set_simulated`; vanilla's
    /// socket does not act on them).
    pub(super) fn admin_apply_simulation(&mut self) {
        let p = self.admin.part;
        // (a connection DRDP let go is no connection: `Session::game_conn`)
        if let Some(c) = self.session.game_conn() {
            self.session.drdp.connection_set_simulated(c, p.loss as i32, p.latency as i32, p.jitter as i32);
        }
    }

    /// `Extrapolate` with Interpolate off: `t` 0, each creature where its
    /// last update put it.
    pub(super) fn admin_hold_glide(&mut self) {
        if self.admin.glides {
            return;
        }
        let me = self.self_id32();
        for e in self.entities.slots.iter_mut().flatten() {
            if Some(e.id) != me {
                e.actor_pos = e.pos;
                e.actor_hpr = e.hpr;
            }
        }
    }

    /// The frame's end, with the walker: the move the Goto window (or
    /// `MoveToOrigin`) asked for (`MoveToGrid`), and `DrawPosition`'s lines.
    pub(super) fn admin_frame(&mut self, walker: &mut Walker) {
        if let Some((world, a, b)) = self.admin.go.take()
            && let Some(g) = walker.scene.grid.as_ref()
        {
            // GetWorldGridCenter on the world the client is in, then MoveTo:
            // EnterWorld at the square's centre, then the ground within 300
            // of it, cast once the zones there are in (a far square's are not
            // yet: the first cast met nothing and left the walk at the
            // centre's height, under the ground, where it fell)
            let c = g.center(a, b);
            let pos = [c.x.0, c.y.0, c.z.0];
            eprintln!("gm: MoveTo world {world} at {pos:?}");
            let now = self.session.now() as i64;
            self.teleport_as(walker, world as u8, pos, [0.0; 3], now, Placing::MoveTo);
            self.admin.fast = false;
        }
        self.admin.position_lines.clear();
        if self.admin.position {
            let pos = walker.cam.walk.committed.position;
            let grid = walker.scene.grid.as_ref().filter(|g| !(g.cols == 1 && g.rows == 1));
            let coords = |v: &vi::vect::Vect3<Ee>| format!("{:.2},{:.2},{:.2}", v.x.0, v.y.0, v.z.0);
            match grid {
                None => self.admin.position_lines.push((crate::ui::wide(&coords(&pos)), [130, 43])),
                Some(g) => {
                    let (x, z, off) = g.offset(&pos);
                    let world = self.world_id();
                    let name = (0..6).contains(&world).then(|| self.start.as_ref().map(|s| s.world_words[world as usize])).flatten();
                    let name = name.and_then(|n| self.words.as_ref().map(|w| w.get(n, 0).to_string())).unwrap_or_default();
                    self.admin.position_lines.push((crate::ui::wide(&format!("{name} ({:02}{:02})", x - 1, z - 1)), [130, 24]));
                    self.admin.position_lines.push((crate::ui::wide(&coords(&off)), [130, 43]));
                }
            }
        }
    }

    /// The GM's windows, each HUD frame: their keys, then drawn; and the
    /// readouts over the world (the ping, the server load, the bps, the
    /// position). The auction's minutes follow the channel's flag.
    pub(super) fn run_admin(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        crate::auction::set_in_minutes(self.admin.part.commands());
        let now = self.session.timer();
        let worlds = self.start.as_ref().map_or_else(Worlds::default, |s| Worlds { words: s.world_words, bounds: s.bounds });
        match self.admin.opening.take() {
            Some(4) => {
                let w = self.world_id();
                self.admin.goto.focus(w);
            }
            Some(27) => self.admin.buffs.focus(now),
            _ => {}
        }
        let mut cues = Vec::new();
        for (key, down) in std::mem::take(&mut self.admin.keys) {
            if !down {
                continue;
            }
            match self.mode {
                27 => match self.admin.buffs.key(key, now, &mut cues) {
                    // ShowA110(Value()), then 260
                    Some(BuffUp::Value(v)) => {
                        self.admin_action(0xA110_0000, v);
                        self.admin.buffs.visible = false;
                        self.mode = 0;
                    }
                    Some(BuffUp::Close) => {
                        self.admin.buffs.visible = false;
                        self.mode = 0;
                    }
                    None => {}
                },
                4 => match self.admin.goto.key(&worlds, key) {
                    // SetP50308(world, column, row), then 260
                    Some(GotoUp::Go { world, col, row }) => {
                        self.admin.go = Some((world, col + 1, row + 1));
                        self.admin.goto.visible = false;
                        self.mode = 0;
                    }
                    Some(GotoUp::Close) => {
                        self.admin.goto.visible = false;
                        self.mode = 0;
                    }
                    None => {}
                },
                _ => {}
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        match self.mode {
            27 => self.admin.buffs.draw(vui, r, now),
            4 => self.admin.goto.draw(vui, r, &worlds),
            _ => {}
        }
        // VIClient::Render's 2D: DrawNetStats, then Show BPS's line
        if self.admin.part.p0c != 0 {
            let h = vui.font_height(r);
            let mut at = [h + 189, h * 2];
            for (line, c) in crate::netstat::net_lines(&self.admin.channels) {
                r.draw_string(at, c, &crate::ui::wide(&line));
                at[1] += h;
            }
        }
        if self.admin.bps {
            let d = &self.session.drdp;
            let line = format!("s{:04}bps r{:04}bps", d.bps_sent() as u32, d.bps_recv() as u32);
            r.draw_string([448, vui.font_height(r) * 3], [1.0, 1.0, 0.0, 1.0], &crate::ui::wide(&line));
        }
        // ShowNetStats every frame; the window up with Toggle Stats while
        // the HUD is (m_p05C)
        if let Some(c) = self.session.game_conn() {
            let k = &self.session.drdp.conns[c];
            let ping = k.rtt.iter().skip(1).fold(k.rtt[0] as i32, |m, &v| m.min(v as i32));
            self.admin.net.set_data(0, k.arrival_history, u64::MAX, k.arrival_number, ping);
        }
        if self.admin.stats && !crate::hideui::hidden() {
            self.admin.net.draw(vui, r);
        }
        for (line, at) in &self.admin.position_lines {
            r.draw_string(*at, POSITION_COLOR, line);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dialog::Key;
    use crate::play::Play;
    use crate::pointer::testing::{NullRaster, vui};
    use net::session::{Player, Session};

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
        Play::new(s, Player::default(), Default::default())
    }

    /// What the session sent since the last look, its 0x0701s and 0x0702s.
    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        let r = p.session.record.replace(Vec::new()).unwrap_or_default();
        r.into_iter().map(|(_, m)| m).filter(|m| m.len() >= 2 && (m[..2] == [1, 7] || m[..2] == [2, 7])).collect()
    }

    /// The channel through `Play`: the slash commands take nothing until
    /// 0x0700 sub-op 35 turns it on; then the commands are sent as the
    /// decomp writes them, Buff PC's picker gives its number to sub-op 2,
    /// the edit box's text goes to DoText, and the prompt's answer is 48 or
    /// 49.
    #[test]
    fn the_channel_through_play() {
        let mut p = play();
        p.chat_line("/create Sword of Truth");
        assert!(sent(&mut p).is_empty(), "the control: off, /create sends nothing");
        assert!(p.admin_opcode(0x0700, &[35, 2]));
        assert!(!p.admin_opcode(0x0701, &[35, 2]), "only 0x0700 is the channel's");
        assert!(p.admin.part.commands());
        p.chat_line("/create Sword of Truth");
        p.chat_line("/mastery 3");
        p.chat_line("/tradelvl 9");
        let mut want = vec![1, 7, 53, 14, 0, 0, 0];
        want.extend(b"Sword of Truth");
        assert_eq!(sent(&mut p), vec![want, vec![1, 7, 54, 1, 0, 0, 0, b'3'], vec![1, 7, 55, 1, 0, 0, 0, b'9']]);
        // KILL on the target; PACKET LOSS 25%
        p.target = 0x55;
        p.do_action(0xA101_0000, 0);
        p.do_action(0xA10B_0000, 25);
        assert_eq!(sent(&mut p), vec![vec![1, 7, 16, 0x55, 0, 0, 0], vec![1, 7, 18, 25, 0, 0, 0, 0]]);
        p.admin_apply_simulation();
        if let Some(r) = p.session.conn.and_then(|c| p.session.drdp.conns[c].route) {
            assert_eq!(p.session.drdp.routes[r].drop, 25, "ApplySimulation: onto the route");
        }
        // TRIGGER 3: 0x0702
        p.do_action(0xA201_0000, 3);
        assert_eq!(sent(&mut p), vec![vec![2, 7, 100, 3]]);
        // Buff PC: up once (10), Enter: sub-op 2 with the target and 10
        p.do_action(0xA10F_0000, 0);
        assert_eq!(p.mode, 27);
        let (vui, mut r) = (vui(), NullRaster);
        p.run_admin(&vui, &mut r);
        for k in [138, 134] {
            p.admin_key(k, true);
        }
        p.run_admin(&vui, &mut r);
        assert_eq!(p.mode, 0, "closed");
        assert_eq!(sent(&mut p), vec![vec![1, 7, 2, 0x55, 0, 0, 0, 10, 0, 0, 0]]);
        // Silence: the edit box for the name, its text to sub-op 10
        p.do_action(0xA118_0000, 0);
        assert_eq!((p.mode, p.edit.purpose), (2, 0xA118_0000));
        p.edit.set_text(&crate::ui::wide("Bob"));
        p.edit_done(0xA118_0000);
        assert_eq!(sent(&mut p), vec![vec![1, 7, 10, 3, 0, 0, 0, b'B', b'o', b'b']]);
        assert_eq!(p.mode, 0);
        // the prompt: "Give item: Zak" as a Yes/No box; it starts on No
        assert!(p.admin_opcode(0x0700, &[50, 3, 0, 0, 0, b'Z', 0, b'a', 0, b'k', 0]));
        assert_eq!(p.mode, 14);
        assert!(p.dialog_key(Key::Enter));
        assert_eq!(sent(&mut p), vec![vec![1, 7, 49]]);
        // the channel off again: the slash commands take nothing
        p.admin_opcode(0x0700, &[35, 0]);
        p.chat_line("/mastery 3");
        assert!(sent(&mut p).is_empty());
    }

    /// SOE's final live client's GM slash commands through `Play`: taken and
    /// nothing sent while the GM's channel is off (no chat line either);
    /// with it on, each one's 0x0701 (the rest of the line after the
    /// command). The control: a slash command no one knows still goes to
    /// the server as said.
    #[test]
    fn the_live_gm_commands() {
        let mut p = play();
        let lines = ["/rst_mastr Bob", "/rst_both Bob", "/rem_abil Bob Fireball", "/banktunar 50", "/servdown"];
        for l in lines {
            p.chat_line(l);
        }
        let said = p.session.record.replace(Vec::new()).unwrap_or_default();
        assert!(said.is_empty(), "off: nothing, not even the line ({} sent)", said.len());
        p.admin_opcode(0x0700, &[35, 2]);
        for l in lines {
            p.chat_line(l);
        }
        let str_ = |sub: u8, s: &[u8]| {
            let mut v = vec![1, 7, sub, s.len() as u8, 0, 0, 0];
            v.extend(s);
            v
        };
        assert_eq!(sent(&mut p), vec![str_(57, b"Bob"), str_(58, b"Bob"), str_(59, b"Bob Fireball"), vec![1, 7, 56, 100], vec![1, 7, 101]]);
        p.chat_line("/nosuch x");
        assert!(!p.session.record.replace(Vec::new()).unwrap_or_default().is_empty(), "the control: said");
    }

    /// The TEST menu's own entries: Extrapolate and Interpolate flip, the
    /// personal light runs On, Off, Auto, the day-night cycle holds the
    /// hour and lets go, Fast run is 15 and back to the mastery's speed,
    /// the Goto window opens on mode 4 and its Enter asks for the move.
    #[test]
    fn the_test_toggles() {
        let mut p = play();
        assert!(p.admin.walks && p.admin.glides);
        p.do_action(0x0105_0400, 0);
        p.do_action(0x0105_0500, 0);
        assert!(!p.admin.walks && !p.admin.glides);
        let line = p.chat.back().unwrap().0.clone();
        assert_eq!(line, "(word 990): (word 552)", "Interpolate: Off");
        for want in [1, 2, 0] {
            p.do_action(0x0509_0410, 0);
            assert_eq!((p.admin.light, p.light.mode), (want, want));
        }
        assert_eq!(p.frozen_hour(), None);
        p.do_action(0x0509_0100, 0);
        assert_eq!(p.frozen_hour(), Some(0.0), "held at the hour it stood at");
        p.do_action(0x0509_0100, 0);
        assert_eq!(p.frozen_hour(), None);
        p.cm.m.speed = 8.5;
        p.do_action(0x0509_1007, 0);
        assert_eq!(p.speed, 15.0);
        p.do_action(0x0509_1007, 0);
        assert_eq!(p.speed, 8.5, "SetSpeed(the mastery part's)");
        // Goto Zone: mode 4; Enter: the move with the square's centre (the
        // column and row plus one)
        p.do_action(0x0509_1006, 0);
        assert_eq!(p.mode, 4);
        let (vui, mut r) = (vui(), NullRaster);
        p.run_admin(&vui, &mut r);
        p.admin_key(134, true);
        p.run_admin(&vui, &mut r);
        assert_eq!((p.mode, p.admin.go), (0, Some((0, 1, 1))));
        // the control: an id neither the part nor TEST has goes on to DoAction
        assert!(!p.admin_action(0x0B0E_0008, 0));
    }

    /// The Goto window's square through `Play` (`admin_frame`, `teleport_as`,
    /// the loading screen's end): square (11, 8) on Tunaria's plains, the walk
    /// standing at the square's centre's height (under the ground) as a far
    /// move's would before the zones there come, stands on the ground once the
    /// screen comes down. The control is the server's move (`teleport`,
    /// `Respawn`'s cast), which leaves it there.
    #[test]
    fn a_goto_lands_on_the_ground_once_the_zones_are_in() {
        use crate::walker::{Placing, Spawn, Walker};
        for goto in [true, false] {
            let Some(world) = crate::walker::tests::tunaria_round(23000.0, 17000.0) else { return };
            let c = world.grid.as_ref().unwrap().center(11, 8);
            let pos = [c.x.0, c.y.0, c.z.0];
            let mut w = Walker::arrive(world, Spawn { pos, heading: 0.0, placing: Placing::AsGiven });
            let mut p = play();
            p.player.world = 0;
            if goto {
                p.admin.go = Some((0, 11, 8));
                p.admin_frame(&mut w);
            } else {
                let now = p.session.now() as i64;
                p.teleport(&mut w, 0, pos, [0.0; 3], now);
            }
            let ground = crate::walker::move_to_floor(&mut w.scene, pos, -1).unwrap();
            p.loading_step(&mut w, true);
            let at = w.cam.walk.phys.position.y.0;
            eprintln!("goto {goto}: the centre at {}, the ground at {ground}, stands at {at}", pos[1]);
            if goto {
                assert!((at - ground).abs() < 0.5, "the Goto stands on the ground: {at}");
            } else {
                assert!((at - pos[1]).abs() < 0.5, "the control stays at the centre's height: {at}");
            }
            assert!(!p.loading(), "the screen is down");
        }
    }

    /// 0x7E08's body as go-eqoa-server writes it (`admin::read_triggers`).
    fn names(list: &[(u8, u8, &str)]) -> Vec<u8> {
        let mut m = vec![list.len() as u8];
        for (entry, flags, name) in list {
            m.extend([*entry, *flags, name.len() as u8]);
            m.extend(name.as_bytes());
        }
        m
    }

    /// The server's trigger names through `Play`: 0x7E08 is the channel's,
    /// each set replacing the last whole (a trigger taken away goes from its
    /// entry), and the labels go on the TRIGGER entries' keys, the rest
    /// left alone. The controls: a set cut short changes nothing; another
    /// opcode is not the channel's; without CLIENT's menus nothing is set,
    /// and they get the names when they come (`client_data`'s call).
    #[test]
    fn the_trigger_names_through_play() {
        use crate::admin::TRIGGER_ENTRY;
        let mut p = play();
        assert!(p.admin_opcode(0x7E08, &names(&[(0, 1, "Invasion"), (2, 3, "Zombies")])));
        assert_eq!(p.admin.triggers[0].as_ref().map(|t| t.name.as_str()), Some("Invasion"));
        assert_eq!(p.admin.triggers[2].as_ref().map(|t| t.tail()), Some(" *"));
        assert!(p.start.is_none(), "no menus here: nothing to label");
        p.trigger_labels();
        assert!(p.admin_opcode(0x7E08, &names(&[(2, 1, "Zombies")])));
        assert!(p.admin.triggers[0].is_none(), "the whole set: Invasion taken away");
        let before = p.admin.triggers.clone();
        assert!(p.admin_opcode(0x7E08, &[1, 0, 1, 9, b'x']), "a cut set is still the channel's");
        assert_eq!(p.admin.triggers, before, "the control: a set cut short changes nothing");
        assert!(!p.admin_opcode(0x7E09, &[0]), "the control: not the channel's");

        use crate::client_image::Label;
        let label = |text: &str, tail: &str, dim| Label { text: crate::ui::wide(text), tail: crate::ui::wide(tail), dim };
        let mut menus = crate::client_image::Menus::default();
        menus.labels.insert((0xA101_0000, 0), label("KILL!", "", false));
        super::set_trigger_labels(&mut menus, &[Some(crate::admin::Trigger { name: "Invasion".into(), on: false, running: true }), None]);
        assert_eq!(menus.labels.get(&(TRIGGER_ENTRY, 0)), Some(&label("Invasion", " *", true)), "off: dimmed; running: the star");
        assert_eq!(menus.labels.len(), 2, "one trigger's label, and another entry's left alone");
        super::set_trigger_labels(&mut menus, &[None, Some(crate::admin::Trigger { name: "Two".into(), on: true, running: false })]);
        assert!(!menus.labels.contains_key(&(TRIGGER_ENTRY, 0)), "a trigger taken away loses its label");
        assert_eq!(menus.labels.get(&(TRIGGER_ENTRY, 1)), Some(&label("Two", "", false)), "the control: on and idle, plain");
        assert_eq!(menus.labels.get(&(0xA101_0000, 0)), Some(&label("KILL!", "", false)), "the control: KILL's left alone");
    }

    /// The labels' keys are the TRIGGER entries the TEST menu reaches out
    /// of the player's CLIENT image (SKIPs without it): TEST, Programming,
    /// TRIGGER, each entry `Do2`'s message with its id 0 to 7 in order, so
    /// each named entry finds its label. The control: no other entry of
    /// any menu reached has a label.
    #[test]
    fn the_trigger_labels_meet_the_test_menus_entries() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/client.bin");
        let Some(img) = std::fs::read(&path).ok().and_then(crate::client_image::ClientImage::from_bytes) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        let mut d = crate::startmenu::StartData::from_image(&img);
        let named: Vec<_> = (0..8).map(|i| Some(crate::admin::Trigger { name: format!("T{i}"), on: true, running: false })).collect();
        super::set_trigger_labels(&mut d.menus, &named);
        let triggers: Vec<_> = d.menus.list.iter().flat_map(|m| m.entries.iter()).filter(|e| e.msg == crate::admin::TRIGGER_ENTRY).map(|e| (e.word, e.id)).collect();
        assert_eq!(triggers, (0..8).map(|i| (507, i)).collect::<Vec<_>>(), "TRIGGER's eight entries, in order");
        for m in &d.menus.list {
            for e in &m.entries {
                let label = d.menus.labels.get(&(e.msg, e.id));
                if e.msg == crate::admin::TRIGGER_ENTRY {
                    assert_eq!(label.map(|l| String::from_utf16_lossy(&l.text)), Some(format!("T{}", e.id)));
                } else {
                    assert!(label.is_none(), "the control: {:08X} {} has none", e.msg, e.id);
                }
            }
        }
    }

    /// The whole list as go-eqoa-server writes it (0x7E36,
    /// `admin::read_trigger_list`): a u16 count, each row's u32 slot, flags
    /// and name.
    fn list(rows: &[(u32, u8, &str)]) -> Vec<u8> {
        let mut m = (rows.len() as u16).to_le_bytes().to_vec();
        for (slot, flags, name) in rows {
            m.extend(slot.to_le_bytes());
            m.extend([*flags, name.len() as u8]);
            m.extend(name.as_bytes());
        }
        m
    }

    /// eqoaclassic's 41 triggers' slots as of 2026-11-09 (1 to 14, the
    /// raids' 101 to 139, and 140 to 142): each "T<slot>" and on, but
    /// 140, the Marr twins against Mr. Moogie by name; with one more, 50, off.
    fn classic() -> Vec<crate::admin::Listed> {
        let on = (1..=14).chain(101..=107).chain(110..=115).chain(120..=122).chain(130..=133).chain(136..=142);
        let mut v: Vec<_> = on.map(|slot| crate::admin::Listed { slot, name: if slot == 140 { "Marr Twins vs Mr. Moogie".into() } else { format!("T{slot}") }, on: true, running: false }).collect();
        v.push(crate::admin::Listed { slot: 50, name: "Off".into(), on: false, running: false });
        v
    }

    /// The whole list through `Play`: 0x7E36 is the channel's, each list
    /// replacing the last whole; a line's message fires its own slot, 0x7E37
    /// with the slot whole (140, and 300 past 0x0702's byte). The controls: a
    /// list cut short changes nothing; CLIENT's eight lines still fire 0x0702.
    #[test]
    fn the_trigger_list_through_play() {
        use crate::admin::{MSG_TRIGGER_SLOT, TRIGGER_ENTRY};
        let mut p = play();
        assert!(p.admin_opcode(0x7E36, &list(&[(1, 1, "Nagafen Attacks Mr. Moogie"), (9, 0, "Off"), (140, 3, "Marr Twins vs Mr. Moogie")])));
        let slots = |p: &Play| p.admin.list.as_ref().map(|l| l.iter().map(|t| (t.slot, t.on, t.running)).collect::<Vec<_>>());
        assert_eq!(slots(&p), Some(vec![(1, true, false), (9, false, false), (140, true, true)]));
        assert!(p.admin_opcode(0x7E36, &list(&[(140, 1, "Marr Twins vs Mr. Moogie")])));
        assert_eq!(slots(&p), Some(vec![(140, true, false)]), "the whole list each time");
        assert!(p.admin_opcode(0x7E36, &[2, 0, 140, 0]), "a list cut short is still the channel's");
        assert_eq!(slots(&p), Some(vec![(140, true, false)]), "the control: it changes nothing");
        p.session.record.replace(Vec::new());
        p.do_action(MSG_TRIGGER_SLOT, 140);
        p.do_action(MSG_TRIGGER_SLOT, 300);
        p.do_action(TRIGGER_ENTRY, 3);
        let r = p.session.record.replace(Vec::new()).unwrap_or_default();
        let fired: Vec<Vec<u8>> = r.into_iter().map(|(_, m)| m).filter(|m| m.len() >= 2 && (m[..2] == [0x37, 0x7E] || m[..2] == [2, 7])).collect();
        assert_eq!(fired, [vec![0x37, 0x7E, 140, 0, 0, 0], vec![0x37, 0x7E, 0x2C, 0x01, 0, 0], vec![2, 7, 100, 3]]);
    }

    /// The start menu's look for a test of its bars: SUPPORT's 252 atlas
    /// entries (the bars' caps 0 wide) and words up to 507, TRIGGER.
    fn bar_vui() -> vi::ui::Vui {
        let mut words: Vec<String> = (1..507).map(|n| format!("word {n}")).collect();
        words.push("TRIGGER".into());
        let words = vi::strtab::StringTable::parse(words.join("\n").as_bytes()).unwrap();
        vi::ui::Vui::new(words, Default::default(), vec![vi::ui::UiTexEntry::default(); 252])
    }

    /// A raster that notes the texts on the start menu's bars (each centred
    /// on its bar, x 18 + 70), a character 8 pixels wide.
    #[derive(Default)]
    struct Bars(Vec<String>);

    impl vi::ui::Raster for Bars {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, at: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
            if at[0] == 18 + 70 {
                self.0.push(String::from_utf16_lossy(s));
            }
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// The whole list on the TEST menu's TRIGGER menu, through the game's own
    /// `VIWndMainMenu` (Programming's second entry opens it): a line a trigger
    /// that is on, by slot, labelled with the slot and the name ("140 Marr
    /// Twins..." cut to its bar), the off one (50) left out; seven bars at
    /// a time, Down paging on past the eighth line to slot 9, Up from the top
    /// wrapping to the last (142), whose Enter is its own slot's message. A
    /// list that loses its last two while the menu is open reaches it at once,
    /// the selection kept within the lines. The controls: with no list, and
    /// with a list of none on, CLIENT's eight TRIGGER lines, as they were.
    #[test]
    fn the_trigger_list_pages_on_the_test_menu() {
        use crate::admin::{MSG_TRIGGER_SLOT, TRIGGER_ENTRY};
        use crate::client_image::{Menu, MenuEntry, Menus};
        use crate::startmenu::{Location, MainMenu, Stats, Up};
        let mut menus = Menus::default();
        let lines: Vec<MenuEntry> = (0..8).map(|i| MenuEntry { word: 507, msg: TRIGGER_ENTRY, id: i, ..Default::default() }).collect();
        let trigger = menus.push(Menu { x00: -1, x04: -1, entries: lines.clone() });
        let programming = menus.push(Menu { x00: -1, x04: -1, entries: vec![MenuEntry { word: 100, msg: 0x0B0B_0001, id: 0, ..Default::default() }, MenuEntry { word: 507, sub: Some(trigger), ..Default::default() }] });
        let retail = |m: &Menus| m.list[trigger].entries.iter().map(|e| (e.word, e.msg, e.id)).collect::<Vec<_>>();
        let was = retail(&menus);
        super::set_trigger_list(&mut menus, None, "TRIGGER");
        assert_eq!(retail(&menus), was, "the control: no list, CLIENT's eight lines");

        let all = classic();
        super::set_trigger_list(&mut menus, Some(&all), "TRIGGER");
        let on: Vec<u32> = all.iter().filter(|t| t.on).map(|t| t.slot).collect();
        assert_eq!(on.len(), 41);
        let shown: Vec<(u32, i32)> = menus.list[trigger].entries.iter().map(|e| (e.msg, e.id)).collect();
        assert_eq!(shown, on.iter().map(|&s| (MSG_TRIGGER_SLOT, s as i32)).collect::<Vec<_>>(), "a line a trigger on, by slot");
        assert!(!menus.labels.contains_key(&(MSG_TRIGGER_SLOT, 50)), "the off one left out");

        let (vui, loc) = (bar_vui(), Location { world: 0, pos: [0.0; 3], cols: 1, rows: 1, cell: 1.0, origin: [0.0; 3], bounds: None });
        let mut r = Bars::default();
        let mut m = MainMenu::new(programming, true);
        m.activate(&vui, &r, true, 0, &loc);
        // a key pressed for a frame: the menu's KeyFlash steps once as it draws
        let press = |m: &mut MainMenu, menus: &Menus, r: &mut Bars, key: i32| -> Up {
            let up = m.key(&vui, &*r, menus, key, true, 0, &loc, &mut Vec::new());
            m.draw(&vui, r, menus, 16, &Stats::default(), &mut Vec::new());
            m.key(&vui, &*r, menus, key, false, 0, &loc, &mut Vec::new());
            r.0.clear();
            m.draw(&vui, r, menus, 16, &Stats::default(), &mut Vec::new());
            up
        };
        press(&mut m, &menus, &mut r, 139);
        assert_eq!(press(&mut m, &menus, &mut r, 134), Up::None, "Programming's TRIGGER: the menu opens");
        let want = |slots: &[u32]| slots.iter().map(|&s| if s == 140 { "140 Marr Twins...".to_string() } else { format!("{s} T{s}") }).collect::<Vec<_>>();
        assert_eq!(r.0, want(&on[..7]), "seven bars, the first seven triggers");
        for _ in 0..8 {
            press(&mut m, &menus, &mut r, 139);
        }
        assert_eq!(r.0, want(&on[2..9]), "Down past the eighth line: slot 9 shown");
        for _ in 0..8 {
            press(&mut m, &menus, &mut r, 138);
        }
        assert_eq!(r.0, want(&on[0..7]), "back at the top");
        press(&mut m, &menus, &mut r, 138);
        assert_eq!(r.0, want(&on[34..41]), "Up from the top: the last seven");
        assert_eq!(*r.0.last().unwrap(), "142 T142");
        let up = m.key(&vui, &r, &menus, 134, true, 0, &loc, &mut Vec::new());
        assert_eq!(up, Up::Action(MSG_TRIGGER_SLOT, 142), "Enter: 142's own slot");

        // the list changes under the open menu: 141 and 142 gone
        let mut m = MainMenu::new(programming, true);
        m.activate(&vui, &r, true, 0, &loc);
        press(&mut m, &menus, &mut r, 139);
        press(&mut m, &menus, &mut r, 134);
        press(&mut m, &menus, &mut r, 138);
        let fewer: Vec<_> = all.iter().filter(|t| t.slot < 141).cloned().collect();
        super::set_trigger_list(&mut menus, Some(&fewer), "TRIGGER");
        m.refit(&menus);
        r.0.clear();
        m.draw(&vui, &mut r, &menus, 16, &Stats::default(), &mut Vec::new());
        assert_eq!(r.0, want(&on[32..39]), "the last seven of the new list, the last selected");
        let up = m.key(&vui, &r, &menus, 134, true, 0, &loc, &mut Vec::new());
        assert_eq!(up, Up::Action(MSG_TRIGGER_SLOT, 140));
        assert!(!menus.labels.contains_key(&(MSG_TRIGGER_SLOT, 142)), "a line gone loses its label");

        let none: Vec<_> = all.iter().map(|t| crate::admin::Listed { on: false, ..t.clone() }).collect();
        super::set_trigger_list(&mut menus, Some(&none), "TRIGGER");
        assert_eq!(retail(&menus), was, "the control: none on, CLIENT's eight lines back");
        assert!(menus.labels.keys().all(|k| k.0 != MSG_TRIGGER_SLOT), "and no list labels");
    }

    /// The whole list on the menus out of the player's CLIENT image (SKIPs
    /// without it): TEST > Programming > TRIGGER becomes a line a trigger that
    /// is on, reached from TEST, an unnamed one's label the slot and the
    /// game's word, and no other menu changes; a list of none on puts CLIENT's
    /// eight lines back as they were.
    #[test]
    fn the_trigger_list_meets_the_test_menu() {
        use crate::admin::{MSG_TRIGGER_SLOT, TRIGGER_ENTRY};
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/client.bin");
        let Some(img) = std::fs::read(&path).ok().and_then(crate::client_image::ClientImage::from_bytes) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        let mut d = crate::startmenu::StartData::from_image(&img);
        let entries = |d: &crate::startmenu::StartData| d.menus.list.iter().map(|m| m.entries.iter().map(|e| (e.icon, e.icon2, e.x08, e.word, e.number, e.sub, e.msg, e.id)).collect::<Vec<_>>()).collect::<Vec<_>>();
        let before = entries(&d);
        let k = before.iter().position(|m| m.iter().any(|e| e.6 == TRIGGER_ENTRY)).unwrap();
        let mut all = classic();
        all[0].name.clear();
        super::set_trigger_list(&mut d.menus, Some(&all), "TRIGGER");
        let after = entries(&d);
        for (i, (a, b)) in before.iter().zip(&after).enumerate() {
            if i != k {
                assert_eq!(a, b, "menu {i} unchanged");
            }
        }
        let ids: Vec<i32> = d.menus.list[k].entries.iter().filter(|e| e.msg == MSG_TRIGGER_SLOT).map(|e| e.id).collect();
        assert_eq!(ids.len(), 41);
        assert_eq!(d.menus.list[k].entries.len(), 41, "the eight lines gone");
        assert_eq!(d.menus.labels.get(&(MSG_TRIGGER_SLOT, 1)).map(|l| String::from_utf16_lossy(&l.text)), Some("1 TRIGGER".into()), "unnamed: the slot and the word");
        d.pick_test(false, 2, true);
        fn reach(menus: &crate::client_image::Menus, m: usize, out: &mut Vec<(u32, i32)>, depth: usize) {
            for e in &menus.list[m].entries {
                out.push((e.msg, e.id));
                if let Some(s) = e.sub.filter(|_| depth < 8) {
                    reach(menus, s, out, depth + 1);
                }
            }
        }
        let mut reached = Vec::new();
        reach(&d.menus, d.menu.main.menu, &mut reached, 0);
        assert!(reached.contains(&(MSG_TRIGGER_SLOT, 142)) && !reached.iter().any(|e| e.0 == TRIGGER_ENTRY), "TEST reaches the list's lines");
        let none: Vec<_> = all.iter().map(|t| crate::admin::Listed { on: false, ..t.clone() }).collect();
        super::set_trigger_list(&mut d.menus, Some(&none), "TRIGGER");
        assert_eq!(entries(&d), before, "the control: CLIENT's eight lines back as they were");
    }
}
