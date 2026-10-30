//! SOE's live client's group search, looking for a group or for players
//! (read out of its CLIENT, `c:/eqoa-live-1500`, in the Corsten save state,
//! CLIENT at 0x012AC400; `live`): its window, `VIWndGroupSearch` (live
//! 0x013623C8..0x01366E18, the game window's mode 42), and its half of
//! `VIClientPart2EFC8` (live VIClient+0x308F8, 0x012EBD68..0x012ECD38), whose
//! messages are 0x0801 up (sub 1 the group chat shown or hidden, 2 register,
//! 3 unregister, 4 a search) and 0x0840 down (sub 0x40 the players found).
//!
//! The Group menu's Search opens the window over the game (the leader of a
//! group with room searches for players, state 0; a player out of a group for
//! a group, state 1; the same window either way, titled Search for Player). Its
//! left pane is the search: the races and the classes (pickers that tick any
//! number, ANY or CLEAR), the lowest and highest level (typed; -1 or nothing is
//! the range one could hunt with, `level_range`) and Search. Its right pane is
//! the list the server answers, sorted by a column (R1/Tab and L1 go round
//! them); ✕ there opens Options: Invite to group, or Tell (a line typed for
//! them). □ shows the help, △ goes back.
//!
//! The one change from the live client: a search no server answers is given
//! up after `SEARCH_GIVE_UP_MS` (the live client stays "Searching" and refuses
//! every search after it until it connects again).

use crate::editbox::{EditBox, EditUp};
use crate::listbox::ListBox;
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::startmenu::KeyFlash;
use net::stream::{Reader, Writer};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// A search unanswered this long is given up (the live client's waits for
/// ever).
pub const SEARCH_GIVE_UP_MS: i64 = 30_000;

/// A player found (0x0840 sub 0x40's record, the part's 56 bytes: `{name[24];
/// race@24; class@28; level@32; VIVect3@36; world@48; id@52}`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub name: String,
    pub race: i32,
    pub class: i32,
    pub level: u8,
    pub pos: [f32; 3],
    pub world: i32,
    pub id: u32,
}

/// What a search asks for (0x012EC530's record: `{u8 type; race mask@4;
/// class mask@8; u8 min@12; u8 max@13}`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Query {
    /// 0 players for a leader's group, 1 a group for a player
    pub kind: u8,
    pub races: i32,
    pub classes: i32,
    pub min: u8,
    pub max: u8,
}

/// `VIClientPart2EFC8`'s group search: the players found and whether a
/// search is out (`+0x10`).
#[derive(Clone, Debug, Default)]
pub struct Lfg {
    pub found: Vec<Found>,
    pub busy: bool,
    /// when the search went out (the client's own, `SEARCH_GIVE_UP_MS`)
    since: i64,
}

/// The 0x0801 head of a request, its sub-op.
fn op(sub: u8) -> Writer {
    let mut w = Writer::op(0x0801);
    w.u8(sub);
    w
}

impl Lfg {
    /// Sub 1 (0x012EBDB8): the group's chat shown or hidden, the disc's
    /// 0x0B01 `{1, show}` under the live opcode.
    pub fn show(show: bool) -> Vec<u8> {
        let mut w = op(1);
        w.u8(show as u8);
        w.data
    }

    /// Sub 2 (0x012EC2B8): registered as looking (the client keeps no state).
    pub fn register() -> Vec<u8> {
        op(2).data
    }

    /// Sub 3 (0x012EC3E0).
    pub fn unregister() -> Vec<u8> {
        op(3).data
    }

    /// Sub 4 (0x012EC530): nothing while a search is out; else the players
    /// let go, the search sent and the part busy. The class mask goes before
    /// the race mask.
    pub fn search(&mut self, q: &Query, now: i64) -> Option<Vec<u8>> {
        if self.busy {
            return None;
        }
        self.found.clear();
        let mut w = op(4);
        w.u8(q.kind).signed(q.classes as i64).signed(q.races as i64).u8(q.min).u8(q.max);
        self.busy = true;
        self.since = now;
        Some(w.data)
    }

    /// The client's own: a search out longer than `SEARCH_GIVE_UP_MS` given
    /// up (true when it was).
    pub fn give_up(&mut self, now: i64) -> bool {
        if self.busy && now - self.since >= SEARCH_GIVE_UP_MS {
            self.busy = false;
            return true;
        }
        false
    }

    /// 0x0840 (Recv 0x012EBED8): only sub 0x40 is read (0x012EBF38): `zz
    /// N`, then N players `{string8 name (23 at most; a longer one is kept
    /// empty), u8 level, zz class, zz race, f32 x, y, z, u8 world, u32 id}`;
    /// the search done. True when it was sub 0x40 (the window refreshed).
    pub fn recv(&mut self, data: &[u8]) -> bool {
        let mut r = Reader::new(data);
        if r.u8() != 0x40 || r.error {
            return false;
        }
        let n = r.signed();
        self.found = (0..n.max(0))
            .map_while(|_| {
                let raw = r.string(24);
                let name = String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string();
                let level = r.u8();
                let (class, race) = (r.signed() as i32, r.signed() as i32);
                let pos = [r.f32(), r.f32(), r.f32()];
                let world = r.u8() as i32;
                let id = r.u32();
                (!r.error).then_some(Found { name, race, class, level, pos, world, id })
            })
            .collect();
        self.busy = false;
        true
    }

    /// `0x012EC6B8(column)`: the players sorted in place (qsort over the
    /// records): 0 by name (case not counted), 1 by race and 2 by class in
    /// the order their names spell (out of range: equal), 3 by level, the
    /// lowest first; ties by name.
    pub fn sort(&mut self, column: i32) {
        const RACE_RANK: [i32; 10] = [7, 3, 1, 5, 2, 9, 0, 6, 4, 8];
        const CLASS_RANK: [i32; 15] = [13, 9, 8, 11, 6, 1, 10, 3, 12, 2, 5, 7, 4, 14, 0];
        let name = |a: &Found, b: &Found| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase());
        let rank = |table: &[i32], a: i32, b: i32| match (usize::try_from(a).ok().and_then(|a| table.get(a)), usize::try_from(b).ok().and_then(|b| table.get(b))) {
            (Some(x), Some(y)) => Some(x.cmp(y)),
            _ => None,
        };
        self.found.sort_by(|a, b| match column {
            1 => rank(&RACE_RANK, a.race, b.race).map_or(std::cmp::Ordering::Equal, |o| o.then_with(|| name(a, b))),
            2 => rank(&CLASS_RANK, a.class, b.class).map_or(std::cmp::Ordering::Equal, |o| o.then_with(|| name(a, b))),
            3 => a.level.cmp(&b.level).then_with(|| name(a, b)),
            _ => name(a, b),
        });
    }
}

/// `LevelRange` (0x012EC0C0): the range of levels one hunts with and gets
/// experience for, from the lowest and highest in `levels` (the group's,
/// each member's or, not in view, one's own; out of a group one's own):
/// down to three quarters of the highest (at least six under it), up to a
/// third over the lowest (more than five over it), the low end no higher than
/// the high, both 1..60. Its float steps are the EE's (`ee`).
pub fn level_range(levels: &[i32]) -> (i32, i32) {
    use vi::fpu::{Ee, F};
    let lo = levels.iter().copied().fold(100, i32::min);
    let hi = levels.iter().copied().fold(0, i32::max);
    let quarter = F::<Ee>::from_int(hi) * F::new(0.75);
    let mut low = quarter.to_int();
    // 1.33's float (0x3FAA3D71)
    let third = F::<Ee>::from_int(lo) * F::new(f32::from_bits(0x3FAA_3D71));
    let half = F::new(0.5);
    let mut high = if third < F::ZERO { (third - half).to_int() } else { (third + half).to_int() };
    if high - lo <= 5 {
        high = lo + 6;
    }
    if hi - low < 6 {
        low = hi - 6;
        if low <= 0 {
            low = 1;
        }
    }
    if high < low {
        low = high;
    }
    (low.max(1).min(60), high.max(1).min(60))
}

/// `GetRace(r)->m_shortStrID` (out of range race 0's; the empty slots 10..20
/// none).
fn race_short(r: i32) -> i32 {
    match r {
        0..=9 => crate::infotext::RACE_SHORT[r as usize],
        10..=20 => -1,
        _ => crate::infotext::RACE_SHORT[0],
    }
}

/// `GetClass(c)->m_shortStrID` (out of range class 0's).
fn class_short(c: i32) -> i32 {
    crate::infotext::CLASS_SHORT[if (0..15).contains(&c) { c as usize } else { 0 }]
}

/// The pickers' rows (0x013FB158, 0x013FB188): -1 ANY, -2 CLEAR, then the
/// races and the classes in the order their names spell.
const RACES: [i32; 12] = [-1, -2, 6, 2, 4, 1, 8, 3, 7, 0, 9, 5];
const CLASSES: [i32; 17] = [-1, -2, 14, 5, 9, 7, 12, 10, 4, 11, 2, 1, 6, 3, 8, 0, 13];

/// The list's columns (0x013FAD80) and their headings' x (0x013FB100).
const COLUMNS: [i32; 4] = [0, 195, 265, 335];
const HEADS: [i32; 4] = [170, 365, 435, 505];
const HEAD_WORDS: [i32; 4] = [509, 510, 511, 503];

/// What the window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// sub 4 (the part sends it when it is not busy)
    Search(Query),
    /// `CloseGroupSearch` (0x012C7998): the players let go, the start menu
    /// again
    Close,
    /// `InviteToGroup(id, name)` (0x012D19E0)
    Invite(u32, String),
    /// `ChatLine` (a tell typed in the window)
    Chat(String),
}

/// What the window reads besides its own state.
pub struct View<'a> {
    pub now: i64,
    /// the range `level_range` gives (the -1 a level takes)
    pub range: (i32, i32),
    /// the scene's world grid and every world's bounds (the Location line)
    pub loc: &'a crate::startmenu::Location,
    pub bounds: &'a [Option<[i32; 4]>; 6],
}

/// `VIWndGroupSearch` (2,784 bytes).
pub struct GroupSearch {
    /// `+0x24`: 0 a leader's search for players, 1 a player's for a group;
    /// 2 and 3 Options over them, 4/6 the races, 5/7 the classes
    pub state: i32,
    /// the pickers' rows (`+0x28`, `+0x34`)
    races: Vec<i32>,
    classes: Vec<i32>,
    main: Window,
    dialog: Window,
    status: Window,
    help: Window,
    list: ListBox,
    flash: KeyFlash,
    edit: EditBox,
    /// the edit is the active window
    editing: bool,
    /// players came while the list could be made again (0x0840's refresh,
    /// done with the words at the next frame)
    pub stale: bool,
    /// `+0x6A8`: the form's row 0..4 (races, classes, the two levels, Search)
    pub row: i32,
    /// `+0x6AC`, `+0x6B0`: a dialog's or a picker's cursor, a picker's scroll
    pub cursor: i32,
    scroll: i32,
    /// `+0x6B4`: 0 the form, 1 the list
    pub pane: i32,
    dots: i32,
    ticks: i32,
    /// `+0x6C0`: the column sorted by
    pub sort: i32,
    help_open: bool,
    help_lines: i32,
    help_scroll: i32,
    help_text: Vec<u16>,
    /// `+0xAD0`, `+0xAD4`: -1 any, else a bit a race or class
    pub race_mask: i32,
    pub class_mask: i32,
    /// `+0xAD8`, `+0xADC`
    pub min: i32,
    pub max: i32,
    /// where the form's rows, the list's rows, the headings and a dialog's
    /// or picker's rows were drawn, for a pointer
    form_hits: Hits,
    list_hits: Hits,
    head_hits: Hits,
    pick_hits: Hits,
}

impl Default for GroupSearch {
    fn default() -> Self {
        GroupSearch {
            state: 1,
            races: RACES.to_vec(),
            classes: CLASSES.to_vec(),
            main: Window::default(),
            dialog: Window::default(),
            status: Window::default(),
            help: Window::default(),
            list: ListBox::default(),
            flash: KeyFlash::default(),
            edit: EditBox::default(),
            editing: false,
            stale: false,
            row: 0,
            cursor: 0,
            scroll: 0,
            pane: 0,
            dots: 0,
            ticks: 15,
            sort: 0,
            help_open: false,
            help_lines: 0,
            help_scroll: 0,
            help_text: Vec::new(),
            race_mask: -1,
            class_mask: -1,
            min: 1,
            max: 60,
            form_hits: Hits::default(),
            list_hits: Hits::default(),
            head_hits: Hits::default(),
            pick_hits: Hits::default(),
        }
    }
}

impl GroupSearch {
    /// `Open` (0x01363200) once the client has let it (`Play`: in a group
    /// and not its leader, word 1848; a full group's leader, 1847): the
    /// state, the players let go, the list made again, the row, the pane and
    /// the sort put back. The focus (295) follows.
    pub fn open(&mut self, leader: bool, lfg: &mut Lfg, vui: &Vui, r: &dyn Raster, v: &View) {
        self.state = if leader { 0 } else { 1 };
        self.editing = false;
        lfg.found.clear();
        self.refresh(lfg, vui);
        (self.sort, self.pane, self.row) = (0, 0, 0);
        self.focus(vui, r, v);
    }

    /// `OnMessage` 295, the focus from outside: the four windows (the main
    /// at (320, 180) 600 wide and 14 rows, open; the searching box 310 by 3
    /// titled 271, the dialog 300 by 5, the help 400 by 10, closed), the help
    /// text (words 1876-1882), every row and cursor at 0, any race and class,
    /// the levels `level_range`'s, the pickers' rows (`0x01365AC8`: race 9
    /// and class 14 only with the feature word's bit 0x02).
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster, v: &View) {
        self.main.init(vui, r, [320, 180], 600, 14, 0);
        self.main.open(true);
        self.status.init(vui, r, [320, 180], 310, 3, 0);
        self.status.set_title(Some(&wide(vui.words.get(271, vui.language))));
        self.status.close(true);
        self.dialog.init(vui, r, [320, 180], 300, 5, 0);
        self.dialog.close(true);
        self.help.init(vui, r, [320, 180], 400, 10, 0);
        self.help.close(true);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        self.help_text = wide(&format!("{}\n{}\n{}\n\n{}\n\n{}\n\n{}\n\n{}", w(1876), w(1877), w(1878), w(1879), w(1880), w(1881), w(1882)));
        (self.row, self.ticks, self.cursor, self.scroll, self.pane, self.dots, self.sort) = (0, 15, 0, 0, 0, 0, 0);
        (self.help_open, self.help_scroll) = (false, 0);
        self.help_lines = self.help.count_lines(vui, &mut NoDraw(r), &self.help_text, self.help.size[0] - 40);
        (self.race_mask, self.class_mask) = (-1, -1);
        (self.min, self.max) = v.range;
        self.editing = false;
        self.flash.reset();
        let f = crate::live::features();
        self.races = RACES.into_iter().filter(|&r| f.shows_race(r)).collect();
        self.classes = CLASSES.into_iter().filter(|&c| f.shows_class(c)).collect();
    }

    /// The edit is up (it takes the keyboard).
    pub fn typing(&self) -> bool {
        self.editing
    }

    /// The list may be made again now (0x01362688's test): the state is 0
    /// or 1 and the edit is not up. 0x0840 marks it `stale` then.
    pub fn refreshable(&self) -> bool {
        !self.editing && self.state < 2
    }

    /// The list made again (0x01362688 and 0x013658E0) while it may be: the
    /// players sorted by the column, a row of name, race and class (their
    /// short words) and level each, the selection on the first.
    pub fn refresh(&mut self, lfg: &mut Lfg, vui: &Vui) {
        if !self.refreshable() {
            return;
        }
        self.stale = false;
        self.list.init(395, 10);
        self.list.active = self.pane != 0;
        lfg.sort(self.sort);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        for p in &lfg.found {
            let i = self.list.add_string(&wide(&p.name));
            self.list.set_columns(i, &COLUMNS);
            self.list.set_column_string(i, 1, &wd(race_short(p.race)));
            self.list.set_column_string(i, 2, &wd(class_short(p.class)));
            self.list.set_column_string(i, 3, &wide(&p.level.to_string()));
        }
        self.list.set_cur_sel(0);
    }

    /// A key down or up (`OnKeyDown` 0x01362CF8, `OnKeyUp` 0x01362E00), or the
    /// edit's while it is up.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, lfg: &mut Lfg, v: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<Out>) {
        if self.stale {
            self.refresh(lfg, vui);
        }
        if self.editing {
            if !down {
                self.edit.key_up(key);
                return;
            }
            match self.edit.key(vui, r, key, v.now, cues) {
                EditUp::Done(_) => self.take_text(lfg, v, cues, out),
                // 260 from the edit: the window active again
                EditUp::Cancel => self.editing = false,
                EditUp::None => {}
            }
            return;
        }
        if !down {
            if matches!(key, 19 | 138) && self.state < 2 {
                self.flash.release(138);
            }
            if matches!(key, 20 | 139) && self.state < 2 {
                self.flash.release(139);
            }
            return;
        }
        match key {
            9 | 142 => self.sort_by(lfg, vui, 1, v.now, cues),
            140 => self.sort_by(lfg, vui, -1, v.now, cues),
            13 | 134 => self.cross(lfg, vui, v, cues, out),
            17 | 136 => self.left_right(-1, v.now, cues),
            18 | 137 => self.left_right(1, v.now, cues),
            19 | 138 if self.state < 2 => self.flash.press(138),
            19 | 138 => self.up(v.now, cues),
            20 | 139 if self.state < 2 => self.flash.press(139),
            20 | 139 => self.down(v.now, cues),
            27 | 132 => self.back(cues, out),
            81 | 113 | 133 => self.show_help(cues),
            _ => {}
        }
    }

    fn reset_ticks(&mut self, now: i64) {
        self.main.reset_tick_pulse(now);
        self.dialog.reset_tick_pulse(now);
    }

    /// Down (0x01363300): the help scrolled (sound 9 at its end), the list's
    /// selection on (sound 11), the form's row on round to the first; a
    /// dialog's cursor round its two; a picker's round its rows. The ticks'
    /// pulse starts over.
    fn down(&mut self, now: i64, cues: &mut Vec<i32>) {
        self.reset_ticks(now);
        match self.state {
            0 | 1 => {
                if self.help_open {
                    if self.help_scroll < self.help_lines - 7 {
                        self.help_scroll += 1;
                        cues.push(11);
                    } else {
                        cues.push(9);
                    }
                } else if self.pane != 0 {
                    let s = self.list.sel;
                    self.list.set_cur_sel(s + 1);
                    cues.push(11);
                } else {
                    self.row = if self.row > 3 { 0 } else { self.row + 1 };
                    cues.push(11);
                }
            }
            2 | 3 => {
                self.cursor = if self.cursor > 0 { 0 } else { self.cursor + 1 };
                cues.push(11);
            }
            _ => {
                let n = self.picker_rows().len() as i32;
                self.cursor = if self.cursor < n - 1 { self.cursor + 1 } else { 0 };
                cues.push(11);
            }
        }
    }

    /// Up (0x01363478): as Down the other way; a dialog's cursor stops at
    /// its first.
    fn up(&mut self, now: i64, cues: &mut Vec<i32>) {
        self.reset_ticks(now);
        match self.state {
            0 | 1 => {
                if self.help_open {
                    if self.help_scroll > 0 {
                        self.help_scroll -= 1;
                        cues.push(11);
                    } else {
                        cues.push(9);
                    }
                } else if self.pane != 0 {
                    let s = self.list.sel;
                    self.list.set_cur_sel(s - 1);
                    cues.push(11);
                } else {
                    self.row = if self.row < 1 { 4 } else { self.row - 1 };
                    cues.push(11);
                }
            }
            2 | 3 => {
                self.cursor = if self.cursor < 1 { 0 } else { self.cursor - 1 };
                cues.push(11);
            }
            _ => {
                let n = self.picker_rows().len() as i32;
                self.cursor = if self.cursor <= 0 { n - 1 } else { self.cursor - 1 };
                cues.push(11);
            }
        }
    }

    /// A picker's rows: the races in 4 and 6, the classes in 5 and 7.
    fn picker_rows(&self) -> &[i32] {
        if self.state == 4 || self.state == 6 { &self.races } else { &self.classes }
    }

    /// Left and Right (0x01363A18, 0x01363A98), in 0 and 1: the form's pane
    /// or the list's (sound 9 already there).
    fn left_right(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        if (0..2).contains(&self.state) {
            let to = (self.pane + d).clamp(0, 1);
            cues.push(if to == self.pane { 9 } else { 11 });
            self.pane = to;
        }
        self.flash.reset();
        self.reset_ticks(now);
    }

    /// R1/Tab and L1 (0x01363B20, 0x01363BC8), in 0 and 1: the next or the
    /// last column round the four, the list made again, the selection where
    /// it was.
    fn sort_by(&mut self, lfg: &mut Lfg, vui: &Vui, d: i32, now: i64, cues: &mut Vec<i32>) {
        let sel = self.list.sel;
        if (0..2).contains(&self.state) {
            cues.push(11);
            self.sort = (self.sort + d).rem_euclid(4);
            self.refresh(lfg, vui);
            self.list.set_cur_sel(sel);
        }
        self.flash.reset();
        self.reset_ticks(now);
    }

    /// △/Escape (0x013638E8): the help closed if it is up, else the window
    /// (`CloseGroupSearch`); a dialog or picker back to its state's pane.
    fn back(&mut self, cues: &mut Vec<i32>, out: &mut Vec<Out>) {
        cues.push(8);
        match self.state {
            0 | 1 if self.help_open => {
                self.help_open = false;
                self.help.close(true);
            }
            0 | 1 => out.push(Out::Close),
            3 | 6 | 7 => (self.cursor, self.state) = (0, 1),
            _ => (self.cursor, self.state) = (0, 0),
        }
        self.flash.reset();
    }

    /// □/Q (0x013639A8), in 0 and 1: the help up (sound 15).
    fn show_help(&mut self, cues: &mut Vec<i32>) {
        if (0..2).contains(&self.state) {
            cues.push(15);
            self.help.close(true);
            self.help.open(false);
            self.help_open = true;
        }
    }

    /// ✕/Enter (0x013635D0).
    fn cross(&mut self, lfg: &mut Lfg, vui: &Vui, v: &View, cues: &mut Vec<i32>, out: &mut Vec<Out>) {
        match self.state {
            0 | 1 => {
                cues.push(13);
                if self.pane != 0 {
                    // Options, when there is a player
                    if !lfg.found.is_empty() {
                        self.cursor = 0;
                        self.state += 2;
                        self.dialog.close(true);
                    }
                } else {
                    (self.cursor, self.scroll) = (0, 0);
                    self.dialog.close(true);
                    match self.row {
                        0 | 1 => {
                            self.state = if self.state == 0 { 4 + self.row } else { 6 + self.row };
                            self.dialog.open(false);
                        }
                        2 | 3 => self.edit_open(v.now),
                        _ => {
                            // 0x01365CE0: nothing while a search is out
                            if !lfg.busy {
                                let q = Query { kind: self.state as u8, races: self.race_mask, classes: self.class_mask, min: self.min as u8, max: self.max as u8 };
                                out.push(Out::Search(q));
                                lfg.found.clear();
                            }
                            self.refresh(lfg, vui);
                        }
                    }
                }
            }
            2 | 3 => {
                if self.cursor == 0 {
                    // 0x01365D80: the selected player
                    if let Some(p) = usize::try_from(self.list.sel).ok().and_then(|s| lfg.found.get(s)) {
                        out.push(Out::Invite(p.id, p.name.clone()));
                    }
                } else {
                    self.edit_open(v.now);
                }
                cues.push(13);
                self.state -= 2;
            }
            _ => {
                let rows = self.picker_rows();
                let Some(&value) = usize::try_from(self.cursor).ok().and_then(|c| rows.get(c)) else { return };
                let races = self.state == 4 || self.state == 6;
                let mask = if races { &mut self.race_mask } else { &mut self.class_mask };
                *mask = match value {
                    -1 => -1,
                    -2 => 0,
                    b => *mask ^ (1 << b),
                };
            }
        }
        self.flash.reset();
        self.reset_ticks(v.now);
    }

    /// 0x013656E8: the edit up, one row, centred on the window, 104 long.
    fn edit_open(&mut self, now: i64) {
        let e = &mut self.edit;
        e.set_rows(1);
        e.set_multi_line(false);
        e.set_center(self.main.center);
        e.set_max_length(104);
        e.title.clear();
        e.focus(now);
        self.editing = true;
    }

    /// The edit done (259, 0x01365760): in 0 and 1, on the list a tell to
    /// the selected player (`/t <name> <text>`), on the form a level (none
    /// or not above 0: both levels `level_range`'s again; the lowest held to
    /// 1..60, the highest to the lowest..60); sound 13, the window active.
    fn take_text(&mut self, lfg: &mut Lfg, v: &View, cues: &mut Vec<i32>, out: &mut Vec<Out>) {
        let text = String::from_utf16_lossy(&self.edit.text);
        if (0..2).contains(&self.state) {
            if self.pane != 0 {
                if let Some(p) = usize::try_from(self.list.sel).ok().and_then(|s| lfg.found.get(s)) {
                    out.push(Out::Chat(format!("/t {} {}", p.name, text)));
                }
            } else {
                let n = crate::auction::atoi(&text.chars().take(31).collect::<String>());
                if n <= 0 {
                    (self.min, self.max) = v.range;
                } else if self.row == 2 {
                    self.min = n.clamp(1, 60);
                } else if self.row == 3 {
                    self.max = n.max(self.min).min(60);
                }
            }
        }
        cues.push(13);
        self.editing = false;
    }

    /// A pointer at frame point `p`, as the keys go: the edit's while it is
    /// up; in 0 and 1 a form row selected (the form's pane first, ←) or the
    /// selected one used (✕), a list row likewise (→ to the list), a heading
    /// sorted by (R1 round to it); a dialog's or picker's row selected, the
    /// selected one used. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.editing {
            return self.edit.point(p).flatten().into_iter().collect();
        }
        match self.state {
            0 | 1 if self.help_open => Vec::new(),
            0 | 1 => {
                if let Some(f) = self.form_hits.at(p) {
                    if self.pane != 0 {
                        self.row = f;
                        return vec![key::LEFT];
                    }
                    if f == self.row {
                        return vec![key::CROSS];
                    }
                    self.row = f;
                    return vec![MOVED];
                }
                if let Some(i) = self.list_hits.at(p) {
                    if self.pane == 0 {
                        self.list.sel = i;
                        return vec![key::RIGHT];
                    }
                    if i == self.list.sel {
                        return vec![key::CROSS];
                    }
                    self.list.sel = i;
                    return vec![MOVED];
                }
                if let Some(k) = self.head_hits.at(p) {
                    return vec![142; (k - self.sort).rem_euclid(4) as usize];
                }
                Vec::new()
            }
            _ => match self.pick_hits.at(p) {
                Some(c) if c == self.cursor => vec![key::CROSS],
                Some(c) => {
                    self.cursor = c;
                    vec![MOVED]
                }
                None => Vec::new(),
            },
        }
    }

    /// `OnDrawSelf` (0x01362710): the levels held to 1..60 and the highest
    /// to the lowest; the title (1846), the main window, the legend (✕
    /// Select, △ Exit, □ Info, L1/R1 Sort; greyed while a dialog, a picker or
    /// the help has the keys); the state's pane and its dialog or picker;
    /// "Searching" and its dots while a search is out; then the held Up and
    /// Down's repeats and the dots' count. The edit over it while it is up.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, lfg: &mut Lfg, v: &View, cues: &mut Vec<i32>) {
        if self.stale {
            self.refresh(lfg, vui);
        }
        self.min = self.min.clamp(1, 60);
        self.max = self.max.max(self.min).min(60);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        (self.form_hits, self.list_hits, self.head_hits, self.pick_hits) = Default::default();
        crate::ui::window_backdrop(vui, r, 642);
        let title = wd(1846);
        let tw = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, [1.0; 4], [320 - tw / 2, 10], 33, 34, tw, 0, 1.0, 0, true);
        self.main.set_title(Some(&[]));
        self.main.draw(vui, r);
        let active = !self.editing && !self.help_open && self.state < 2;
        // 201 as the live client words it ("Info", the disc's "INFO")
        let info = crate::livewords::live(&vui.words, 201, vui.language);
        let legend = format!("{}|x{}  |t{}  |s{info}  |7|8{}", if active { "" } else { "|-" }, word(212), word(161), word(1844));
        self.list.pulse = active;
        let mw = self.main.size[0];
        self.main.text_box(vui, r, &wide(&legend), 40, 13, mw - 40, 1, UI_COLORS[0], 0);
        self.draw_pane(vui, r, lfg, v, active);
        match self.state {
            2 | 3 => self.draw_options(vui, r),
            4..=7 => self.draw_picker(vui, r),
            _ => {}
        }
        if lfg.busy {
            self.status.open(false);
            self.status.draw(vui, r);
            let s = format!("{}{}", word(1693), ".".repeat(self.dots as usize + 1));
            let sw = self.status.size[0];
            self.status.text_box(vui, r, &wide(&s), 40, 1, sw - 40, 1, UI_COLORS[0], 0);
        }
        if self.editing {
            self.edit.draw(vui, r, v.now, cues);
        }
        if self.flash.pulse(138) {
            self.up(v.now, cues);
        }
        if self.flash.pulse(139) {
            self.down(v.now, cues);
        }
        self.ticks -= 1;
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots = if self.dots + 1 < 3 { self.dots + 1 } else { 0 };
        }
    }

    /// 0x01363C70 / 0x013640E8 (states 0 and 1 draw the same): the form, the
    /// headings (the sorted one between `|}` and `|{`), the list from the
    /// window's left plus 150, the count found, the selected player's
    /// Location (on the list), the help.
    fn draw_pane(&mut self, vui: &Vui, r: &mut dyn Raster, lfg: &Lfg, v: &View, active: bool) {
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        let form_ticks = active && matches!(self.state, 0 | 1);
        self.draw_form(vui, r, form_ticks);
        for (k, (&x, &id)) in HEADS.iter().zip(&HEAD_WORDS).enumerate() {
            let (s, at) = if self.sort == k as i32 { (wide(&format!("|}}{}|{{", vui.words.get(id, vui.language))), x - 12) } else { (wd(id), x) };
            self.main.text(vui, r, &s, at, 0, UI_COLORS[0], 0);
            let end = HEADS.get(k + 1).copied().unwrap_or(self.main.size[0] - 25);
            self.head_hits.push(rows_rect(&self.main, vui, r, x, 0, end - x, 1), k as i32);
        }
        self.list.active = self.pane != 0;
        // the box at the window's left plus 150, which `Draw` takes as from
        // the window's left (170 on the 4:3 frame, under the first heading)
        let x = self.main.top_left()[0] + 150;
        self.list.draw(&self.main, vui, r, x, 1);
        for (i, row) in self.list.shown(1) {
            self.list_hits.push(rows_rect(&self.main, vui, r, x, row, self.list.width, 1), i);
        }
        let n = lfg.found.len();
        if n > 0 {
            let s = vui.words.get(1849, vui.language).replacen("%d", &n.to_string(), 1);
            let mw = self.main.size[0];
            self.main.text_box(vui, r, &wide(&s), 40, 13, mw - 70, 1, UI_COLORS[0], 16);
        }
        if self.pane != 0
            && let Some(p) = usize::try_from(self.list.sel).ok().and_then(|s| lfg.found.get(s))
        {
            let text = location(vui, v, p.world, p.pos);
            let at = self.main.row_center(vui, r, 11);
            r.draw_string([self.main.top_left()[0] + HEADS[0], at[1] - 2], [0.0, 0.0, 0.0, 1.0], &wide(&text));
        }
        if self.help_open {
            self.draw_help(vui, r);
        }
    }

    /// 0x013646E8: Race and its value (none, the one race's short word, or
    /// Multiple), Class and its value, Min. Lvl and Max. Lvl and their
    /// numbers, Search; the row in the selection's colour, ticked while the
    /// form has the keys.
    fn draw_form(&mut self, vui: &Vui, r: &mut dyn Raster, ticks: bool) {
        let w = &self.main;
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        let c = |row: i32| UI_COLORS[(self.row == row) as usize];
        let x = HEADS[0];
        let tick = |me: &GroupSearch, r: &mut dyn Raster, row: i32, at: i32| {
            if me.row == row && me.pane == 0 {
                me.main.tick_r(vui, r, 40, at, ticks);
            }
        };
        let value = |mask: i32, bits: i32, short: &dyn Fn(i32) -> i32| -> Vec<u16> {
            if mask == 0 {
                return Vec::new();
            }
            let set: Vec<i32> = (0..bits).filter(|b| mask & (1 << b) != 0).collect();
            match set[..] {
                [one] => wd(short(one)),
                _ => wd(1874),
            }
        };
        w.text_box(vui, r, &wd(510), 40, 1, x - 40, 1, c(0), 0);
        tick(self, r, 0, 1);
        w.text_box(vui, r, &value(self.race_mask, 10, &race_short), 40, 2, x - 65, 1, c(0), 4);
        w.text_box(vui, r, &wd(511), 40, 4, x - 40, 1, c(1), 0);
        tick(self, r, 1, 4);
        w.text_box(vui, r, &value(self.class_mask, 15, &class_short), 40, 5, x - 65, 1, c(1), 4);
        let lvl = |id: i32| wide(&format!("{}.{}", vui.words.get(id, vui.language), vui.words.get(299, vui.language)));
        w.text_box(vui, r, &lvl(355), 40, 7, x - 90, 1, c(2), 0);
        tick(self, r, 2, 7);
        w.text_box(vui, r, &wide(&self.min.to_string()), 110, 7, 37, 1, c(2), 68);
        w.text_box(vui, r, &lvl(354), 40, 8, x - 90, 1, c(3), 0);
        tick(self, r, 3, 8);
        w.text_box(vui, r, &wide(&self.max.to_string()), 110, 8, 37, 1, c(3), 68);
        w.text_box(vui, r, &wd(1673), 40, 9, x - 40, 1, c(4), 0);
        tick(self, r, 4, 9);
        for (row, at, rows) in [(0, 1, 2), (1, 4, 2), (2, 7, 1), (3, 8, 1), (4, 9, 1)] {
            self.form_hits.push(rows_rect(&self.main, vui, r, 40, at, x - 40, rows), row);
        }
    }

    /// 0x01364570: the help, 400 by 10 titled 201, its text scrolled seven
    /// rows at a time beside a scroll bar (none under eight rows), and ✕'s
    /// Exit under it.
    fn draw_help(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.help.init(vui, r, [320, 180], 400, 10, 0);
        self.help.set_title(Some(&wide(&crate::livewords::live(&vui.words, 201, vui.language))));
        self.help.open(false);
        self.help.draw(vui, r);
        let t = if self.help_lines < 8 { -1.0 } else { self.help_scroll as f32 / (self.help_lines - 7) as f32 };
        let hw = self.help.size[0];
        self.help.draw_frame_vscroll(vui, r, 40, 1, hw - 80, 7, t);
        self.help.text_box(vui, r, &self.help_text, 40, 1, hw - 80, 7, UI_COLORS[0], (self.help_scroll as u32) << 24);
        let s = wide(&format!("|t{}", vui.words.get(161, vui.language)));
        self.help.text(vui, r, &s, 40, 9, UI_COLORS[0], 0);
    }

    /// 0x01364C18 (2, 260 wide) / 0x01364DF8 (3, 240): Options (422), Invite
    /// to group (942) and Tell (425), the cursor's ticked.
    fn draw_options(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let width = if self.state == 2 { 260 } else { 240 };
        self.dialog.init(vui, r, [320, 180], width, 5, 0);
        self.dialog.set_title(Some(&wide(vui.words.get(422, vui.language))));
        self.dialog.open(false);
        self.dialog.draw(vui, r);
        let dw = self.dialog.size[0];
        for (k, id) in [(0, 942), (1, 425)] {
            self.dialog.text_box(vui, r, &wide(vui.words.get(id, vui.language)), 40, k + 1, dw - 60, 1, UI_COLORS[(self.cursor == k) as usize], 0);
            if self.cursor == k {
                self.dialog.tick_r(vui, r, 40, k + 1, !self.editing);
            }
            self.pick_hits.push(rows_rect(&self.dialog, vui, r, 40, k + 1, dw - 60, 1), k);
        }
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language)));
        self.dialog.text_box(vui, r, &s, 20, 4, dw - 40, 1, UI_COLORS[0], 0);
    }

    /// 0x01365008 / 0x01365378: Select Race (1703) or Select Class (1702),
    /// seven rows from the scroll beside its bar, each a name, ANY (471) or
    /// CLEAR (1875), the chosen ones lit (atlas 61 at a quarter), the
    /// cursor's ticked; ✕ Select, △ Back.
    fn draw_picker(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let races = self.state == 4 || self.state == 6;
        self.dialog.init(vui, r, [320, 180], 290, 10, 0);
        if self.dialog.is_open() || self.dialog.is_opening() || self.dialog.is_closing() {
            self.dialog.set_title(Some(&wide(vui.words.get(if races { 1703 } else { 1702 }, vui.language))));
            self.dialog.draw(vui, r);
        }
        if self.cursor - self.scroll >= 7 {
            self.scroll = self.cursor - 6;
        } else if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        let rows = if races { self.races.clone() } else { self.classes.clone() };
        let mask = if races { self.race_mask } else { self.class_mask };
        let n = rows.len() as i32;
        let t = if n == 7 { 0.0 } else { self.scroll as f32 / (n - 7) as f32 };
        let dw = self.dialog.size[0];
        self.dialog.draw_frame_vscroll(vui, r, 22, 1, dw - 55, 7, t);
        let mut row = 1;
        let mut i = self.scroll;
        while i < n && row < 8 {
            let sel = i == self.cursor;
            if sel {
                self.dialog.tick_r(vui, r, 36, row, true);
            }
            let v = rows[i as usize];
            let word = match v {
                -1 => 471,
                -2 => 1875,
                _ if races => crate::auctionwnd::race_word(v),
                _ => crate::auctionwnd::class_word(v),
            };
            if v >= 0 && (mask >> v) & 1 != 0 {
                let at = self.dialog.row_center(vui, r, row);
                vui.stretch_blit_ex(r, 61, [self.dialog.top_left()[0] + 20, at[1] - 7], [dw - 53, 18], 3, 0.25);
            }
            self.dialog.text_box(vui, r, &wide(vui.words.get(word, vui.language)), 40, row, dw - 75, 1, UI_COLORS[sel as usize], 2);
            self.pick_hits.push(rows_rect(&self.dialog, vui, r, 40, row, dw - 75, 1), i);
            i += 1;
            row += 1;
        }
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(217, vui.language)));
        self.dialog.text_box(vui, r, &s, 20, 9, dw - 40, 1, UI_COLORS[0], 0);
    }
}

/// 0x01365DF0: "Location: ***" on a world of one grid cell; else the zone
/// under the player's place in the scene's grid (`CalcWorldGridOffset`), of
/// the player's own world (`ZoneName(world, row - 1, col - 1)`, 641 off its
/// bounds).
fn location(vui: &Vui, v: &View, world: i32, pos: [f32; 3]) -> String {
    let l = vui.words.get(18, vui.language);
    let g = v.loc;
    if g.cols == 1 && g.rows == 1 {
        return format!("{l}: ***");
    }
    // CalcWorldGridOffset's cells, the EE's divide
    use vi::fpu::{Ee, F};
    let cell = F::<Ee>::new(g.cell);
    let col = (F::<Ee>::new(pos[0]) / cell).to_int();
    let row = (F::<Ee>::new(pos[2]) / cell).to_int();
    let (row, col) = (row - 1, col - 1);
    let zone = match usize::try_from(world).ok().and_then(|w| v.bounds.get(w)).copied().flatten() {
        Some(b) if col >= b[0] && col <= b[1] && row >= b[2] && row <= b[3] => crate::zones::zone(world, row, col).unwrap_or(641),
        _ => 641,
    };
    format!("{l}: {}", vui.words.get(zone as i32, vui.language))
}

/// A raster that draws nothing, for measuring (`CountLines`).
struct NoDraw<'a>(&'a dyn Raster);

impl Raster for NoDraw<'_> {
    fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
    fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
    fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
    fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
    fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
    fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
    fn string_width(&self, s: &[u16]) -> i32 {
        self.0.string_width(s)
    }
    fn point_size(&self) -> i32 {
        self.0.point_size()
    }
    fn viewport(&self) -> [i32; 4] {
        self.0.viewport()
    }
    fn set_viewport(&mut self, _: [i32; 4]) {}
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid};

    /// Words as `wN` for WORDS.TXT line N, so what is drawn says which.
    pub(crate) fn numbered() -> vi::strtab::StringTable {
        let lines: Vec<String> = (1..=2000).map(|n| format!("w{n}")).collect();
        vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap()
    }

    fn found(name: &str, race: i32, class: i32, level: u8, id: u32) -> Found {
        Found { name: name.into(), race, class, level, pos: [100.0, 0.0, 4000.0], world: 0, id }
    }

    /// 0x0840 sub 0x40 as a server writes it.
    fn reply(players: &[Found]) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(0x40).signed(players.len() as i64);
        for p in players {
            w.string(p.name.as_bytes()).u8(p.level).signed(p.class as i64).signed(p.race as i64);
            for c in p.pos {
                w.u32(c.to_bits());
            }
            w.u8(p.world as u8).u32(p.id);
        }
        w.data
    }

    /// Sub 1 is the disc's 0x0B01 under 0x0801; 2 and 3 are bare; 4 the
    /// type, the class mask BEFORE the race mask (zigzag), the two levels;
    /// a search while one is out sends nothing (the part's busy flag), the
    /// players' reply ends it.
    #[test]
    fn the_messages_up() {
        assert_eq!(Lfg::show(true), vec![0x01, 0x08, 1, 1]);
        assert_eq!(Lfg::show(false), vec![0x01, 0x08, 1, 0]);
        assert_eq!(Lfg::register(), vec![0x01, 0x08, 2]);
        assert_eq!(Lfg::unregister(), vec![0x01, 0x08, 3]);
        let mut lfg = Lfg { found: vec![found("Old", 0, 0, 1, 1)], ..Default::default() };
        let q = Query { kind: 1, races: -1, classes: 1 << 9, min: 22, max: 40 };
        assert_eq!(lfg.search(&q, 0), Some(vec![0x01, 0x08, 4, 1, 0x80, 0x08, 0x01, 22, 40]));
        assert!(lfg.busy && lfg.found.is_empty(), "the players let go, the part busy");
        assert_eq!(lfg.search(&q, 10), None, "nothing while one is out");
        assert!(lfg.recv(&reply(&[])));
        assert!(!lfg.busy, "an answer of none ends it");
        assert!(lfg.search(&q, 20).is_some());
    }

    /// Sub 0x40's records (a name of 24 or more kept empty), every field; a
    /// sub not 0x40 changes nothing (the control).
    #[test]
    fn the_players_found_are_read() {
        let mut lfg = Lfg { busy: true, ..Default::default() };
        let mut long = found(&"x".repeat(24), 9, 14, 60, 7);
        let mut w = Writer::default();
        w.u8(0x41).signed(0);
        assert!(!lfg.recv(&w.data) && lfg.busy, "another sub is not read");
        let players = [found("Ann", 1, 2, 30, 0x0100_0001), long.clone()];
        assert!(lfg.recv(&reply(&players)));
        long.name.clear();
        assert_eq!(lfg.found, vec![players[0].clone(), long]);
        assert!(!lfg.busy);
        // a record cut short ends the list there
        let mut cut = reply(&players);
        cut.truncate(cut.len() - 2);
        lfg.recv(&cut);
        assert_eq!(lfg.found.len(), 1);
    }

    /// The four sorts: by name (case not counted), by race and by class in
    /// their names' order (Barbarian, Dark Elf, ...; Alchemist, Bard, ...),
    /// by level from the lowest; ties by name.
    #[test]
    fn the_players_sort_by_each_column() {
        let mut lfg = Lfg::default();
        // a human warrior 30, a barbarian shaman 30, a dark elf alchemist 12, an erudite bard 30
        lfg.found = vec![found("dave", 0, 0, 30, 1), found("Bob", 6, 8, 30, 2), found("Cid", 2, 14, 12, 3), found("abe", 8, 5, 30, 4)];
        let names = |l: &Lfg| l.found.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(" ");
        lfg.sort(0);
        assert_eq!(names(&lfg), "abe Bob Cid dave");
        lfg.sort(1);
        assert_eq!(names(&lfg), "Bob Cid abe dave", "Barbarian, Dark Elf, Erudite, Human");
        lfg.sort(2);
        assert_eq!(names(&lfg), "Cid abe Bob dave", "Alchemist, Bard, Shaman, Warrior");
        lfg.sort(3);
        assert_eq!(names(&lfg), "Cid abe Bob dave", "12, then the 30s by name");
    }

    /// `LevelRange` as 0x012EC0C0 computes it: alone from one's own level
    /// (1: 1..7; 10: 4..16; 30: 22..40; 60: 45..60), in a group from the
    /// lowest and highest (20 and 30: 22..27; 10 and 50, whose top would be
    /// under the bottom: 16..16).
    #[test]
    fn the_default_range_is_the_live_clients() {
        assert_eq!(level_range(&[1]), (1, 7));
        assert_eq!(level_range(&[10]), (4, 16));
        assert_eq!(level_range(&[30]), (22, 40));
        assert_eq!(level_range(&[60]), (45, 60));
        assert_eq!(level_range(&[20, 30]), (22, 27));
        assert_eq!(level_range(&[10, 50]), (16, 16));
        assert_eq!(level_range(&[50, 10, 30]), (16, 16), "whatever the order");
    }

    /// A search no server answers is given up after `SEARCH_GIVE_UP_MS`
    /// (the client's own); one that is answered is not (the control).
    #[test]
    fn an_unanswered_search_is_given_up() {
        let q = Query { kind: 0, races: -1, classes: -1, min: 1, max: 60 };
        let mut lfg = Lfg::default();
        lfg.search(&q, 1000);
        assert!(!lfg.give_up(1000 + SEARCH_GIVE_UP_MS - 1) && lfg.busy);
        assert!(lfg.give_up(1000 + SEARCH_GIVE_UP_MS) && !lfg.busy);
        assert!(!lfg.give_up(1_000_000), "nothing out, nothing given up");
    }

    struct Rig {
        w: GroupSearch,
        lfg: Lfg,
        vui: Vui,
        cues: Vec<i32>,
        out: Vec<Out>,
        loc: crate::startmenu::Location,
        bounds: [Option<[i32; 4]>; 6],
    }

    impl Rig {
        /// The window opened by a player out of a group (state 1), or a
        /// leader (0), whose range is 22..40.
        fn new(leader: bool) -> Self {
            let vui = Vui::new(numbered(), Default::default(), Vec::new());
            let loc = crate::startmenu::Location { world: 0, pos: [0.0; 3], cols: 50, rows: 50, cell: 1000.0, origin: [0.0; 3], bounds: None };
            let mut g = Rig { w: GroupSearch::default(), lfg: Lfg::default(), vui, cues: Vec::new(), out: Vec::new(), loc, bounds: [Some([0, 49, 0, 49]), None, None, None, None, None] };
            let v = View { now: 0, range: (22, 40), loc: &g.loc, bounds: &g.bounds };
            g.w.open(leader, &mut g.lfg, &g.vui, &NullRaster, &v);
            g
        }

        fn key(&mut self, k: i32) {
            let v = View { now: 0, range: (22, 40), loc: &self.loc, bounds: &self.bounds };
            self.w.key(&self.vui, &NullRaster, &mut self.lfg, &v, k, true, &mut self.cues, &mut self.out);
            self.w.key(&self.vui, &NullRaster, &mut self.lfg, &v, k, false, &mut self.cues, &mut self.out);
        }

        fn draw(&mut self) -> Vec<String> {
            let v = View { now: 0, range: (22, 40), loc: &self.loc, bounds: &self.bounds };
            let mut r = Text::default();
            self.w.draw(&self.vui, &mut r, &mut self.lfg, &v, &mut self.cues);
            r.0
        }

        /// Players come (0x0840), as `Play` takes them.
        fn reply(&mut self, players: &[Found]) {
            assert!(self.lfg.recv(&reply(players)));
            if self.w.refreshable() {
                self.w.stale = true;
            }
        }

        fn type_text(&mut self, s: &str) {
            assert!(self.w.typing());
            self.w.edit.set_text(&wide(s));
            self.key(13);
        }
    }

    /// A raster that keeps the strings drawn.
    #[derive(Default)]
    struct Text(Vec<String>);

    impl crate::ui::Layout for Text {}

    impl Raster for Text {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16]) {
            self.0.push(String::from_utf16_lossy(s));
        }
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
            self.0.push(String::from_utf16_lossy(s));
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// The window as it opens (295): a player's search for a group (state
    /// 1) or a leader's for players (0), any race and class, the range's
    /// levels; titled 1846 whichever; the form's rows go round with the d-pad
    /// (✕ on Race the race picker over the state, 6 or 4).
    #[test]
    fn it_opens_on_the_range_and_goes_round_the_form() {
        for (leader, state, picker) in [(false, 1, 6), (true, 0, 4)] {
            let mut g = Rig::new(leader);
            assert_eq!((g.w.state, g.w.race_mask, g.w.class_mask, g.w.min, g.w.max), (state, -1, -1, 22, 40));
            let drawn = g.draw();
            assert!(drawn.contains(&"w1846".to_string()), "{drawn:?}");
            assert!(drawn.contains(&"w1874".to_string()), "any race is Multiple");
            assert!(drawn.contains(&"w355.w299".to_string()) && drawn.contains(&"22".to_string()));
            // up from Race is Search, round to Race again
            g.key(138);
            g.draw();
            assert_eq!(g.w.row, 4);
            g.key(139);
            g.draw();
            assert_eq!(g.w.row, 0);
            g.key(134);
            assert_eq!(g.w.state, picker);
        }
    }

    /// The legend and the help's title say Info as the live client words
    /// row 201 (the disc's table says "INFO"; `livewords::live`).
    #[test]
    fn the_legend_says_info_as_the_live_client() {
        let mut g = Rig::new(false);
        let mut lines: Vec<String> = (1..=1814).map(|n| format!("w{n}")).collect();
        lines[200] = "INFO".into();
        g.vui = Vui::new(vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap(), Default::default(), Vec::new());
        let drawn = g.draw();
        assert!(drawn.contains(&"Info  ".to_string()) && !drawn.iter().any(|s| s.contains("INFO")), "{drawn:?}");
        g.key(133);
        assert!(g.draw().contains(&"Info".to_string()), "the help's title");
    }

    /// The race picker ticks races one by one (✕ toggles), ANY is every race
    /// and CLEAR none; the form shows one race's short word, Multiple for
    /// more; △ back to the form. Without the feature word's bit 0x02 the
    /// Ogre is not a row.
    #[test]
    fn the_pickers_tick_races_and_classes() {
        crate::live::with_features(Some(0x1B), || {
            let mut g = Rig::new(false);
            assert_eq!(g.w.races.len(), 12);
            g.key(134);
            // CLEAR (row 1), then the Barbarian (row 2, race 6)
            g.key(139);
            g.key(134);
            assert_eq!(g.w.race_mask, 0);
            g.key(139);
            g.key(134);
            assert_eq!(g.w.race_mask, 1 << 6);
            g.key(132);
            assert_eq!(g.w.state, 1);
            let drawn = g.draw();
            assert!(drawn.contains(&"w323".to_string()), "the Barbarian's short word: {drawn:?}");
            // the classes: ANY then the Alchemist (row 2) off: every class but it
            g.w.row = 1;
            g.key(134);
            assert_eq!(g.w.state, 7);
            g.key(139);
            g.key(139);
            g.key(134);
            assert_eq!(g.w.class_mask, -1 ^ (1 << 14));
            g.key(132);
            let drawn = g.draw();
            assert!(drawn.contains(&"w1874".to_string()));
        });
        crate::live::with_features(Some(0x10), || {
            let g = Rig::new(false);
            assert!(!g.w.races.contains(&9) && !g.w.classes.contains(&14), "no Ogre, no Alchemist");
            assert_eq!((g.w.races.len(), g.w.classes.len()), (11, 16));
        });
    }

    /// A level typed: the lowest held to 1..60, the highest to the lowest
    /// and 60; -1 (or nothing above 0) both back to the range. The edit is
    /// one row, 104 long, with no title.
    #[test]
    fn the_levels_are_typed() {
        let mut g = Rig::new(false);
        g.w.row = 2;
        g.key(134);
        assert!(g.w.typing() && g.w.edit.title.is_empty());
        g.type_text("70");
        assert_eq!((g.w.min, g.w.max), (60, 40));
        g.draw();
        assert_eq!((g.w.min, g.w.max), (60, 60), "the highest held to the lowest as it is drawn");
        g.w.row = 3;
        g.key(134);
        g.type_text("5");
        assert_eq!(g.w.max, 60, "not under the lowest");
        g.key(134);
        g.type_text("-1");
        assert_eq!((g.w.min, g.w.max), (22, 40), "the range again");
        g.w.row = 2;
        g.key(134);
        g.type_text("0");
        assert_eq!((g.w.min, g.w.max), (22, 40));
    }

    /// Search: the query of the state, masks and levels; the players come,
    /// the list is made sorted by name with the selection on the first; →
    /// to the list, ✕ Options (state 3 over 1), Invite to group the selected
    /// player; Tell asks a line and says it as a tell to them; R1 sorts by
    /// race and keeps the selection's place; the count's word.
    #[test]
    fn search_then_invite_and_tell() {
        let mut g = Rig::new(false);
        g.w.row = 4;
        g.key(134);
        assert_eq!(g.out, vec![Out::Search(Query { kind: 1, races: -1, classes: -1, min: 22, max: 40 })]);
        g.out.clear();
        // the client sends it (the part busy): "Searching..." is drawn
        g.lfg.search(&Query { kind: 1, races: -1, classes: -1, min: 22, max: 40 }, 0);
        assert!(g.draw().contains(&"w1693.".to_string()));
        g.key(134);
        assert!(g.out.is_empty(), "no second search while one is out");
        g.reply(&[found("Zed", 0, 0, 30, 0x10), found("Amy", 6, 9, 25, 0x11)]);
        let drawn = g.draw();
        assert!(drawn.contains(&"Amy".to_string()) && drawn.contains(&"w323".to_string()) && drawn.contains(&"w312".to_string()), "{drawn:?}");
        assert!(drawn.contains(&"w1849".to_string()), "the count's word");
        assert_eq!((g.w.list.sel, g.lfg.found[0].name.as_str()), (0, "Amy"));
        // → the list, ↓ to Zed, ✕ Options, ✕ Invite to group
        g.key(137);
        assert_eq!(g.w.pane, 1);
        g.key(139);
        g.draw();
        assert_eq!(g.w.list.sel, 1);
        g.key(134);
        assert_eq!(g.w.state, 3);
        assert!(g.draw().contains(&"w942".to_string()));
        g.key(134);
        assert_eq!((g.out.clone(), g.w.state), (vec![Out::Invite(0x10, "Zed".into())], 1));
        // Options again, ↓ Tell, a line: a tell to Zed
        g.out.clear();
        g.key(134);
        g.key(139);
        g.key(134);
        assert_eq!(g.w.state, 1);
        g.type_text("hunt?");
        assert_eq!(g.out, vec![Out::Chat("/t Zed hunt?".into())]);
        // R1: by race (Barbarian Amy, Human Zed), the selection kept on row 1
        g.key(142);
        assert_eq!((g.w.sort, g.w.list.sel), (1, 1));
        let drawn = g.draw();
        assert_eq!(g.lfg.found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Amy", "Zed"]);
        // the Location line of the selected player (out of world 0's bounds
        // there: word 641)
        assert!(drawn.contains(&"w18: w641".to_string()), "{drawn:?}");
        // in the bounds: the zone's word
        g.lfg.found[1].pos = [5000.0, 0.0, 5000.0];
        let zone = crate::zones::zone(0, 4, 4).unwrap();
        assert!(g.draw().contains(&format!("w18: w{zone}")));
    }

    /// △ closes the help when it is up, else the window; a dialog goes
    /// back to its pane (Options over 1 to 1).
    #[test]
    fn back_closes_the_help_then_the_window() {
        let mut g = Rig::new(false);
        g.key(133);
        assert_eq!(g.cues.last(), Some(&15));
        assert!(g.draw().contains(&"w1876".to_string()) || g.w.help_open, "the help up");
        g.key(132);
        assert!(g.out.is_empty() && !g.w.help_open);
        g.key(132);
        assert_eq!(g.out, vec![Out::Close]);
    }

    /// A pointer on the window: a form row is selected by a click and used
    /// by a second; a list row's click goes right to the list, the second
    /// opens Options; Options' rows and a picker's are selected and used;
    /// a heading sorts round to it. Each needs the draw's notes (the
    /// control: before any draw nothing is hit).
    #[test]
    fn a_pointer_on_the_group_search() {
        let mut g = Rig::new(false);
        assert!(g.w.point([200.0, 200.0]).is_empty(), "nothing drawn, nothing hit");
        g.reply(&[found("Amy", 6, 9, 25, 0x11), found("Bo", 1, 1, 20, 0x12)]);
        g.draw();
        let vui = crate::pointer::testing::vui();
        let r = NullRaster;
        let row = |g: &GroupSearch, at: i32| mid(rows_rect(&g.main, &vui, &r, 40, at, HEADS[0] - 40, 1));
        let p = row(&g.w, 7);
        assert_eq!(g.w.point(p), vec![MOVED]);
        assert_eq!(g.w.row, 2, "the lowest level's row");
        assert_eq!(g.w.point(p), vec![key::CROSS]);
        let x = g.w.main.top_left()[0] + 150;
        let list_row = |g: &GroupSearch, i: i32| mid(rows_rect(&g.main, &vui, &r, x, i + 1, 395, 1));
        let p = list_row(&g.w, 1);
        assert_eq!(g.w.point(p), vec![key::RIGHT]);
        assert_eq!(g.w.list.sel, 1);
        g.key(137);
        g.draw();
        assert_eq!(g.w.point(p), vec![key::CROSS]);
        g.key(134);
        g.draw();
        assert_eq!(g.w.state, 3);
        let tell = mid(rows_rect(&g.w.dialog, &vui, &r, 40, 2, g.w.dialog.size[0] - 60, 1));
        assert_eq!(g.w.point(tell), vec![MOVED]);
        assert_eq!(g.w.cursor, 1);
        g.key(132);
        g.draw();
        // the Level heading: R1 three times round from Name
        let head = mid(rows_rect(&g.w.main, &vui, &r, HEADS[3], 0, 60, 1));
        assert_eq!(g.w.point(head), vec![142, 142, 142]);
    }
}
