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
use crate::walker::Walker;
use vi::fpu::{Ee, F};

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
}

impl Default for Admin {
    /// `VIClientPart382E8`'s constructor and `VIClient`'s values for the
    /// rest (the creatures walked and glided, the day-night cycle running).
    fn default() -> Self {
        Admin { part: Part::default(), buffs: BuffPc::default(), goto: Goto::new(), net: NetStat::default(), channels: vec![None; 40], keys: Vec::new(), opening: None, go: None, walks: true, glides: true, held_hour: None, light: 0, fast: false, stats: false, bps: false, position: false, position_lines: Vec::new() }
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

    /// The opcodes the GM's channel reads (0x0700, `Recv0700`); true when
    /// handled.
    pub(super) fn admin_opcode(&mut self, op: u16, data: &[u8]) -> bool {
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
        if let Some(c) = self.session.conn {
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
            // GetWorldGridCenter on the world the client is in, then MoveTo
            let c = g.center(a, b);
            let mut pos = [c.x.0, c.y.0, c.z.0];
            if world as i64 == self.player.world {
                pos[1] = move_to_floor(&mut walker.scene, pos, walker.cam.walk.parm64).unwrap_or(pos[1]);
            }
            eprintln!("gm: MoveTo world {world} at {pos:?}");
            let now = self.session.now() as i64;
            self.teleport(walker, world as u8, pos, [0.0; 3], now);
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
        if let Some(c) = self.session.conn {
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

/// `MoveTo`'s ground: a sphere of 0.1 cast from 300 above to 300 below (the
/// second `Collide` repeats the first: its sphere is not set again), the hit
/// 0.2 under; none without a hit.
fn move_to_floor(w: &mut vi::world::World<Ee>, pos: [f32; 3], actor: i32) -> Option<f32> {
    use vi::coll::CollSphere;
    use vi::vect::Vect3;
    let y = F::<Ee>::new(pos[1]);
    let mut s = CollSphere::<Ee>::default();
    s.init(Vect3::new(F::new(pos[0]), y + F::new(300.0), F::new(pos[2])), Vect3::new(F::new(pos[0]), y - F::new(300.0), F::new(pos[2])), F::new(0.1));
    let mut f = vi::scene::Filter::new(vi::scene::FilterKind::Scene, actor);
    w.collide_sphere(&mut s, &mut f).then(|| (s.hit_point().y - F::new(0.2)).0)
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
}
