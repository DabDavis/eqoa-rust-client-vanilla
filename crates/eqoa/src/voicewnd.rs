//! The voice chat window (the user, F3 pass 18: "add voice chat gui in
//! addition to the chat commands, so it is easy"): the client's own, over
//! play as a server's player search is (`searchwnd`): the game's titled
//! window (`VIUIWindow`, set 0) laid out in its rows, in the middle of the
//! picture whatever its shape, that while it is up takes the keys play would
//! have (not the push-to-talk keys: V and N still talk), holds the player and
//! the camera still and hides the HUD's right side. It opens from the
//! Controls page's Voice Window key (O), `/voice` alone in the chat, or a click
//! on the HUD's mic badge (`Badge`), and reads only voice's view
//! (`Play::voice_view`, `voice::view`'s types); what it
//! asks goes through the same `Play::voice_*` calls the `/voice` commands use.
//!
//! At its top, voice itself: Voice Chat (Off, Push to Talk, Open Mic; Off or
//! Demo on a server that has none of its own, Sandstorm's), the connection
//! and the microphone's level (green while the player is heard), which keys
//! talk, and voice's last line. Under them, framed with its scroll bar, the
//! list: the group's members with voice on, each with its speaking mark, its
//! own volume (0 to 200%) and Mute; the room the player is in (locked,
//! unlisted), Leave, and its members not in the group; a room joined by its
//! name or made (its name, a password, Unlisted, Join: the way into an
//! unlisted room, which no list shows); and the lobby's listed rooms with
//! their people and locks, Refresh, a room joined with ✕ (a locked one asks
//! its password first). Text is typed in the game's edit box (mode 2, its
//! password face for a password: stars).
//!
//! A password goes no further than `Ask::Join`, which voice turns into the
//! room's H (`voice::ids::room_hash`): it is drawn as stars, never in a line,
//! the report's state or a `Debug` print, scrubbed from reports
//! (`report::secret`), and the form forgets it once joined or closed.
//!
//! Its input: the d-pad and the arrows move between the rows (headings and
//! notes are passed over) and change a value with left and right; ✕ (Enter,
//! E) does the row's thing, △ (Escape, Backspace), Start or O again closes;
//! L1 and R1 (` and Tab) jump between the parts, Page Up and Page Down by a
//! page, Home and End to the ends; □ (Q) mutes a member too. A pointer
//! (`pointer`): a row is chosen, and chosen again pressed; a value's or a
//! volume's sides are left and right; Mute, Leave, Join, Refresh and the
//! text fields take a click at once; the wheel moves the selection.

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::ui::Layout;
use crate::voice::{VoiceMember, VoiceMode, VoiceRoomInfo, VoiceView};
use vi::ui::{FrameParam, UI_COLORS, Vui, Window, wide};

/// The window, and its rows: voice's own at the top (the mode, the
/// microphone, the connection, the keys, voice's last line), the list
/// framed under them with its scroll bar, the legend at the foot.
const WIDTH: i32 = 560;
const ROWS: i32 = 20;
const LIST_X: i32 = 34;
const LIST_W: i32 = WIDTH - 68;
const LIST_ROW: i32 = 6;
const LIST_ROWS: usize = 12;
const LEGEND_ROW: i32 = ROWS - 1;
/// The header's value frames: the mode's, the meter's.
const VALUE_X: i32 = WIDTH - 214;
const VALUE_W: i32 = 160;
/// In the list (from its frame's left): a member's speaking mark, its name,
/// "room", its volume's slider and number, Mute; a form row's value frame.
const MARK_X: i32 = 22;
const NAME_X: i32 = 36;
const NAME_W: i32 = 150;
const TAG_X: i32 = 190;
const SLIDER_X: i32 = 246;
const SLIDER_W: i32 = 100;
const PERCENT_X: i32 = 362;
const PERCENT_W: i32 = 46;
const MUTE_X: i32 = 418;
const MUTE_W: i32 = 62;
const FIELD_X: i32 = 200;
const FIELD_W: i32 = 170;
/// A member's own volume: its voice's level 1, at most twice it, in tenths.
const VOLUME_STEP: f32 = 0.1;
const VOLUME_MAX: f32 = 2.0;
/// The edit box's lengths (its terminator counted): a room's name, a
/// password.
const NAME_LEN: usize = crate::voice::command::NAME_MAX + 1;
const PASSWORD_LEN: usize = 33;
/// The edit box's purposes for the window's fields (no message of the
/// game's is 0x7E1A_xxxx).
const EDIT_NAME: u32 = 0x7E1A_0001;
const EDIT_PASSWORD: u32 = 0x7E1A_0002;
const EDIT_ROOM_PASSWORD: u32 = 0x7E1A_0003;
/// The HUD's mic badge in the HUD's 640x448 frame (`Anchor::Right`,
/// `Vertical::Top`, as the quick menus and the minimap): left of the
/// minimap's lower corner (`map::mini::FRAME`, x 474..586 to y 162), below
/// the breath gauge's bar under water (to y 80) and the quick menu's name
/// plate (to y 44), above the quest tracker's column (`tracker::COLUMN`, from
/// y 168) and right of nothing the HUD draws there.
pub const BADGE: [i32; 4] = [444, 136, 470, 162];

/// A text field the window has typed in the edit box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// the form's room name
    Name,
    /// the form's password
    Password,
    /// the password of a locked room of the lobby, joined once typed
    RoomPassword(String),
}

impl Field {
    /// The edit box's purpose (`m_x24`) while it types this.
    pub fn purpose(&self) -> u32 {
        match self {
            Field::Name => EDIT_NAME,
            Field::Password => EDIT_PASSWORD,
            Field::RoomPassword(_) => EDIT_ROOM_PASSWORD,
        }
    }

    /// The edit box's title.
    pub fn title(&self) -> String {
        match self {
            Field::Name => "Voice: the room's name".to_string(),
            Field::Password => "Voice: a password (none: an open room)".to_string(),
            Field::RoomPassword(n) => format!("Voice: the password of {n}"),
        }
    }

    /// The most it takes (`SetMaxLength`, its terminator counted).
    pub fn max_len(&self) -> usize {
        match self {
            Field::Name => NAME_LEN,
            _ => PASSWORD_LEN,
        }
    }

    /// Typed as stars (the edit box's password face).
    pub fn masked(&self) -> bool {
        !matches!(self, Field::Name)
    }
}

/// What the window asks of voice (`Play::voice_*`) and of play.
#[derive(Clone, PartialEq)]
pub enum Ask {
    Mode(VoiceMode),
    Refresh,
    Join { name: String, password: String, unlisted: bool },
    Leave,
    Volume(u32, f32),
    Muted(u32, bool),
    /// the edit box up for a field
    Edit(Field),
    Close,
}

/// A password never printed, even in a test's failure.
impl std::fmt::Debug for Ask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ask::Mode(m) => write!(f, "Mode({m:?})"),
            Ask::Refresh => write!(f, "Refresh"),
            Ask::Join { name, password, unlisted } => write!(f, "Join {{ name: {name:?}, password: {}, unlisted: {unlisted} }}", if password.is_empty() { "none" } else { "<hidden>" }),
            Ask::Leave => write!(f, "Leave"),
            Ask::Volume(p, v) => write!(f, "Volume({p}, {v})"),
            Ask::Muted(p, m) => write!(f, "Muted({p}, {m})"),
            Ask::Edit(x) => write!(f, "Edit({x:?})"),
            Ask::Close => write!(f, "Close"),
        }
    }
}

/// What the selection is on: a row by what it is, so it stays on it while
/// the rows round it come and go.
#[derive(Clone, Debug, PartialEq)]
enum Sel {
    Mode,
    Member(u32),
    Leave,
    Name,
    Password,
    Unlisted,
    Join,
    Refresh,
    Room(String),
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    /// a part's heading, and what is written at its right
    Heading(String, String),
    Note(String),
    Member(VoiceMember),
    Leave,
    Name,
    Password,
    Unlisted,
    Join,
    Refresh,
    Room(VoiceRoomInfo),
}

impl Row {
    /// What selecting it is, when it can be.
    fn sel(&self) -> Option<Sel> {
        Some(match self {
            Row::Heading(..) | Row::Note(_) => return None,
            Row::Member(m) => Sel::Member(m.peer),
            Row::Leave => Sel::Leave,
            Row::Name => Sel::Name,
            Row::Password => Sel::Password,
            Row::Unlisted => Sel::Unlisted,
            Row::Join => Sel::Join,
            Row::Refresh => Sel::Refresh,
            Row::Room(r) => Sel::Room(r.name.clone()),
        })
    }
}

/// The form: a room's name, its password, whether a room it makes is
/// unlisted.
#[derive(Clone, Default, PartialEq)]
struct Form {
    name: String,
    password: String,
    unlisted: bool,
}

/// Its password never printed.
impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Form {{ name: {:?}, password: {}, unlisted: {} }}", self.name, if self.password.is_empty() { "none" } else { "<hidden>" }, self.unlisted)
    }
}

/// Whether two room names are one (voice matches them case-folded).
fn same_room(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// The choices Voice Chat's row steps through: on a server with no voice
/// of its own, Off and the demo.
fn modes(v: &VoiceView) -> &'static [VoiceMode] {
    if v.demo_only { &[VoiceMode::Off, VoiceMode::Demo] } else { &[VoiceMode::Off, VoiceMode::PushToTalk, VoiceMode::OpenMic] }
}

fn mode_name(m: VoiceMode) -> &'static str {
    match m {
        VoiceMode::Off => "Off",
        VoiceMode::PushToTalk => "Push to Talk",
        VoiceMode::OpenMic => "Open Mic",
        VoiceMode::Demo => "Demo",
    }
}

/// A count of people.
fn people(n: usize) -> String {
    if n == 1 { "1 person".to_string() } else { format!("{n} people") }
}

/// Who hears the player while `bits` (bit 0 the group, bit 1 the room),
/// short enough for the status's half of its row.
fn heard_by(bits: u8) -> &'static str {
    match bits & 3 {
        1 => "your group hears you",
        2 => "your room hears you",
        3 => "group and room hear you",
        _ => "",
    }
}

/// The push-to-talk keys as the Controls page has them now (its rows'
/// `push_to_talk` and `push_to_talk_room`), as the page writes a key.
pub fn talk_keys() -> [String; 2] {
    let b = crate::settings::binds();
    ["push_to_talk", "push_to_talk_room"].map(|id| crate::binds::row_named(id).and_then(|r| b.key(r)).map_or_else(|| "-".to_string(), crate::binds::display))
}

/// The window.
#[derive(Default)]
pub struct VoiceWnd {
    pub open: bool,
    window: Window,
    sel: Option<Sel>,
    /// the list's first row shown
    top: usize,
    /// where the mode row and the list's rows were drawn: slot 0 the mode
    /// row, slot k + 1 the list's row k (`hit`), and what each slot was
    hits: Hits,
    drawn: Vec<Option<Sel>>,
    form: Form,
    /// the field the edit box is up for
    editing: Option<Field>,
    /// what the window itself last said (a name it cannot join), over
    /// voice's own line until voice says another
    said: Option<(String, String)>,
    cues: Vec<i32>,
    /// `EQOA_VOICE_WINDOW_TEST`: a view of its own, which the window's asks
    /// change (a native build has no voice engine yet: shots and tests)
    pub test: Option<VoiceView>,
}

/// A hit's parts: the row, its value's left and right, a member's Mute.
const PART_ROW: i32 = 0;
const PART_LESS: i32 = 1;
const PART_MORE: i32 = 2;
const PART_MUTE: i32 = 3;

fn hit(slot: usize, part: i32) -> i32 {
    slot as i32 * 4 + part
}

impl VoiceWnd {
    /// Put up: the selection on Voice Chat, the list at its top, the window
    /// opening; the lobby asked for.
    pub fn open(&mut self) -> Vec<Ask> {
        self.open = true;
        self.sel = Some(Sel::Mode);
        self.top = 0;
        self.editing = None;
        self.said = None;
        self.hits.clear();
        self.drawn.clear();
        self.window.close(true);
        self.window.open(false);
        vec![Ask::Refresh]
    }

    /// Taken down: the form's password forgotten (its name kept).
    pub fn close(&mut self) {
        self.open = false;
        self.editing = None;
        self.form.password.clear();
        self.hits.clear();
        self.drawn.clear();
    }

    /// The sounds asked for since the last call.
    pub fn take_cues(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.cues)
    }

    /// Whether the edit box is up for one of the window's fields with
    /// `purpose`.
    pub fn edits(&self, purpose: u32) -> bool {
        self.editing.as_ref().is_some_and(|f| f.purpose() == purpose)
    }

    /// The edit box went down for a field of the window's: `text` typed
    /// (Done), or none (cancelled).
    pub fn edited(&mut self, text: Option<String>) -> Vec<Ask> {
        let Some(field) = self.editing.take() else { return Vec::new() };
        let Some(text) = text else { return Vec::new() };
        match field {
            Field::Name => {
                self.form.name = text.trim().to_string();
                Vec::new()
            }
            Field::Password => {
                if !text.is_empty() {
                    crate::report::secret("", &text);
                }
                self.form.password = text;
                Vec::new()
            }
            // nothing typed for a locked room: nothing to join with
            Field::RoomPassword(name) if !text.is_empty() => {
                crate::report::secret("", &text);
                vec![Ask::Join { name, password: text, unlisted: false }]
            }
            Field::RoomPassword(_) => Vec::new(),
        }
    }

    /// The list's rows for view `v`: a note alone where voice is off or
    /// cannot be had; else the group, the room, the form and the lobby.
    fn rows(&self, v: &VoiceView) -> Vec<Row> {
        let note = |s: &str| vec![Row::Note(s.to_string())];
        if !v.available {
            return note("Voice chat is not in this build of the game yet.");
        }
        if v.mode == VoiceMode::Off {
            return note(if v.demo_only { "Voice chat is off. This server has none of its own: choose Demo above for eqoaclassic's." } else { "Voice chat is off. Choose Push to Talk or Open Mic above." });
        }
        if v.demo_only && v.mode != VoiceMode::Demo {
            return note("This server has no voice chat of its own: choose Demo above for eqoaclassic's.");
        }
        let mut out = Vec::new();
        // the group's members with voice on (a groupmate in the room too is
        // here, marked)
        let group: Vec<&VoiceMember> = v.members.iter().filter(|m| m.group).collect();
        out.push(Row::Heading("Your group".to_string(), if group.is_empty() { String::new() } else { people(group.len()) }));
        if group.is_empty() {
            out.push(Row::Note("No groupmate has voice chat on.".to_string()));
        }
        out.extend(group.into_iter().map(|m| Row::Member(m.clone())));
        // the room
        match &v.room {
            Some((name, locked, unlisted)) => {
                let marks = [(*locked, "Locked"), (*unlisted, "Unlisted")].iter().filter(|m| m.0).map(|m| m.1).collect::<Vec<_>>().join(", ");
                out.push(Row::Heading(format!("Room: {name}"), marks));
                out.push(Row::Leave);
                let others: Vec<&VoiceMember> = v.members.iter().filter(|m| m.room && !m.group).collect();
                if others.is_empty() && !v.members.iter().any(|m| m.room) {
                    out.push(Row::Note("No one else is in the room.".to_string()));
                }
                out.extend(others.into_iter().map(|m| Row::Member(m.clone())));
            }
            None => {
                out.push(Row::Heading("Room".to_string(), String::new()));
                out.push(Row::Note("You are in no room: join one below, or make one.".to_string()));
            }
        }
        // a room by its name: joined, or made
        out.push(Row::Heading("Join a room by its name, or make one".to_string(), String::new()));
        out.extend([Row::Name, Row::Password, Row::Unlisted, Row::Join]);
        // the lobby
        out.push(Row::Heading("Lobby".to_string(), if v.lobby.len() == 1 { "1 room".to_string() } else { format!("{} rooms", v.lobby.len()) }));
        out.push(Row::Refresh);
        if v.lobby.is_empty() {
            out.push(Row::Note("No room is listed. Make one above.".to_string()));
        }
        out.extend(v.lobby.iter().map(|r| Row::Room(r.clone())));
        out
    }

    /// Every selection there is, in order: Voice Chat, then the list's.
    fn selectable(rows: &[Row]) -> Vec<Sel> {
        std::iter::once(Sel::Mode).chain(rows.iter().filter_map(Row::sel)).collect()
    }

    /// The selection now: where it was, else (its row gone) the one in its
    /// place, else Voice Chat.
    fn current(&mut self, rows: &[Row]) -> Sel {
        let all = Self::selectable(rows);
        let sel = self.sel.clone().unwrap_or(Sel::Mode);
        if all.contains(&sel) {
            return sel;
        }
        // the row it was on is gone: the next one at its old place
        let at = self.drawn.iter().position(|d| d.as_ref() == Some(&sel)).map(|k| k.saturating_sub(1));
        let next = at.and_then(|k| rows.iter().skip(k).find_map(Row::sel)).or_else(|| rows.iter().rev().find_map(Row::sel)).unwrap_or(Sel::Mode);
        self.sel = Some(next.clone());
        next
    }

    /// The list row of selection `s` (none for Voice Chat).
    fn row_of(rows: &[Row], s: &Sel) -> Option<usize> {
        rows.iter().position(|r| r.sel().as_ref() == Some(s))
    }

    /// The list scrolled so that the selection is in it, with the heading
    /// or note just above it; on Voice Chat, at its top.
    fn follow(&mut self, rows: &[Row]) {
        let most = rows.len().saturating_sub(LIST_ROWS);
        if matches!(self.sel, None | Some(Sel::Mode)) {
            self.top = 0;
        }
        if let Some(s) = self.sel.clone()
            && let Some(at) = Self::row_of(rows, &s)
        {
            let lo = if at > 0 && rows[at - 1].sel().is_none() { at - 1 } else { at };
            if lo < self.top {
                self.top = lo;
            }
            if at >= self.top + LIST_ROWS {
                self.top = at + 1 - LIST_ROWS;
            }
        }
        self.top = self.top.min(most);
    }

    /// The selection moved to `to`: the move's sound, or the end's when it
    /// stays.
    fn select(&mut self, to: Sel, rows: &[Row], now: i64) {
        let moved = self.sel.as_ref() != Some(&to);
        self.cues.push(if moved { 11 } else { 9 });
        if moved {
            self.window.reset_tick_pulse(now);
        }
        self.sel = Some(to);
        self.follow(rows);
    }

    /// The selection `d` selectable rows on (kept at the ends).
    fn step(&mut self, d: i32, rows: &[Row], now: i64) {
        let all = Self::selectable(rows);
        let cur = self.current(rows);
        let k = all.iter().position(|s| *s == cur).unwrap_or(0) as i32;
        let to = (k + d).clamp(0, all.len() as i32 - 1) as usize;
        self.select(all[to].clone(), rows, now);
    }

    /// The selection a page of the list on (or back): the last row that
    /// can be chosen within its rows less one, else the next there is; back
    /// past the list's top, Voice Chat.
    fn page(&mut self, forward: bool, rows: &[Row], now: i64) {
        let cur = self.current(rows);
        let at = Self::row_of(rows, &cur).map_or(-1, |k| k as i32);
        let n = LIST_ROWS as i32 - 1;
        let sels: Vec<(i32, Sel)> = rows.iter().enumerate().filter_map(|(k, r)| r.sel().map(|s| (k as i32, s))).collect();
        let to = if forward {
            sels.iter().filter(|(k, _)| *k > at && *k <= at + n).last().or_else(|| sels.iter().find(|(k, _)| *k > at)).map(|x| x.1.clone())
        } else {
            sels.iter().find(|(k, _)| *k < at && *k >= at - n).or_else(|| sels.iter().rfind(|(k, _)| *k < at)).map(|x| x.1.clone()).or((at >= 0).then_some(Sel::Mode))
        };
        self.select(to.unwrap_or(cur), rows, now);
    }

    /// The selection to the first row of the part after (or before) the
    /// one it is in: the parts begin at their headings, Voice Chat its own.
    fn jump(&mut self, forward: bool, rows: &[Row], now: i64) {
        let cur = self.current(rows);
        // each part's first selectable row
        let mut starts = vec![Sel::Mode];
        let mut fresh = false;
        for r in rows {
            if matches!(r, Row::Heading(..)) {
                fresh = true;
            } else if let Some(s) = r.sel()
                && std::mem::take(&mut fresh)
            {
                starts.push(s);
            }
        }
        let here = match Self::row_of(rows, &cur) {
            None => 0,
            Some(at) => starts.iter().rposition(|s| Self::row_of(rows, s).is_some_and(|k| k <= at)).unwrap_or(0),
        };
        let to = if forward {
            (here + 1).min(starts.len() - 1)
        } else if cur != starts[here] {
            here
        } else {
            here.saturating_sub(1)
        };
        self.select(starts[to].clone(), rows, now);
    }

    /// A key, the game's code (128 + a pad field; the keyboard's 13 Enter,
    /// 27 Escape, 17-20 the cursor keys, 21 and 22 a page, 23 End, 24 Home)
    /// on view `v`: what it asks.
    pub fn key(&mut self, k: i32, v: &VoiceView, now: i64) -> Vec<Ask> {
        let rows = self.rows(v);
        let cur = self.current(&rows);
        let mut out = Vec::new();
        match k {
            key::UP | 19 => self.step(-1, &rows, now),
            key::DOWN | 20 => self.step(1, &rows, now),
            21 => self.page(false, &rows, now),
            22 => self.page(true, &rows, now),
            24 => self.step(-10_000, &rows, now),
            23 => self.step(10_000, &rows, now),
            key::L1 => self.jump(false, &rows, now),
            key::R1 => self.jump(true, &rows, now),
            key::LEFT | 17 => out.extend(self.change(&cur, -1, v)),
            key::RIGHT | 18 => out.extend(self.change(&cur, 1, v)),
            key::CROSS | 13 => out.extend(self.press(&cur, v)),
            key::SQUARE => {
                if let Sel::Member(peer) = cur
                    && let Some(m) = v.members.iter().find(|m| m.peer == peer)
                {
                    self.cues.push(11);
                    out.push(Ask::Muted(peer, !m.muted));
                }
            }
            key::TRIANGLE | 27 | key::START => {
                self.cues.push(8);
                out.push(Ask::Close);
            }
            _ => {}
        }
        out
    }

    /// Left (`d` -1) or right on the selection: Voice Chat's next choice
    /// (round), a member's volume a tenth, Unlisted turned.
    fn change(&mut self, cur: &Sel, d: i32, v: &VoiceView) -> Vec<Ask> {
        match cur {
            Sel::Mode => {
                let all = modes(v);
                let at = all.iter().position(|m| *m == v.mode);
                let n = all.len() as i32;
                let to = match at {
                    Some(k) => all[(k as i32 + d).rem_euclid(n) as usize],
                    // a mode this server does not offer: its first or last
                    None if d < 0 => all[n as usize - 1],
                    None => all[0],
                };
                self.cues.push(11);
                vec![Ask::Mode(to)]
            }
            Sel::Member(peer) => {
                let Some(m) = v.members.iter().find(|m| m.peer == *peer) else { return Vec::new() };
                // in tenths, so that ten steps of 0.1 come back to 1 exactly
                let tenths = (m.volume / VOLUME_STEP).round() as i32 + d;
                if !(0..=(VOLUME_MAX / VOLUME_STEP).round() as i32).contains(&tenths) {
                    self.cues.push(9);
                    return Vec::new();
                }
                self.cues.push(11);
                vec![Ask::Volume(*peer, tenths as f32 / 10.0)]
            }
            Sel::Unlisted => {
                self.form.unlisted = !self.form.unlisted;
                self.cues.push(11);
                Vec::new()
            }
            _ => {
                self.cues.push(9);
                Vec::new()
            }
        }
    }

    /// ✕ on the selection.
    fn press(&mut self, cur: &Sel, v: &VoiceView) -> Vec<Ask> {
        let edit = |me: &mut Self, f: Field| {
            me.editing = Some(f.clone());
            me.cues.push(13);
            vec![Ask::Edit(f)]
        };
        match cur {
            // Voice Chat steps on, as an options row's label does
            Sel::Mode => self.change(cur, 1, v),
            Sel::Member(peer) => match v.members.iter().find(|m| m.peer == *peer) {
                Some(m) => {
                    self.cues.push(11);
                    vec![Ask::Muted(*peer, !m.muted)]
                }
                None => Vec::new(),
            },
            Sel::Leave => {
                self.cues.push(13);
                vec![Ask::Leave]
            }
            Sel::Name => edit(self, Field::Name),
            Sel::Password => edit(self, Field::Password),
            Sel::Unlisted => self.change(cur, 1, v),
            Sel::Join => {
                let name = self.form.name.trim().to_string();
                if !crate::voice::command::good_name(&name) {
                    self.cues.push(9);
                    self.said = Some((format!("A room's name is {} to {} letters: type one under Join a room.", crate::voice::command::NAME_MIN, crate::voice::command::NAME_MAX), v.status.clone()));
                    return Vec::new();
                }
                self.said = None;
                self.cues.push(13);
                // the password goes with the join, and no further here
                let password = std::mem::take(&mut self.form.password);
                vec![Ask::Join { name, password, unlisted: self.form.unlisted }]
            }
            Sel::Refresh => {
                self.cues.push(13);
                vec![Ask::Refresh]
            }
            Sel::Room(name) => {
                if v.room.as_ref().is_some_and(|r| same_room(&r.0, name)) {
                    self.cues.push(9);
                    return Vec::new();
                }
                match v.lobby.iter().find(|r| same_room(&r.name, name)) {
                    Some(r) if r.locked => edit(self, Field::RoomPassword(r.name.clone())),
                    Some(r) => {
                        self.cues.push(13);
                        vec![Ask::Join { name: r.name.clone(), password: String::new(), unlisted: false }]
                    }
                    None => Vec::new(),
                }
            }
        }
    }

    /// A press at frame point `p` (where the window was last drawn): the
    /// selection moved there, and the keys to press (`pointer`).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        let (slot, part) = ((h / 4) as usize, h % 4);
        let Some(sel) = self.drawn.get(slot).cloned().flatten() else { return Vec::new() };
        let was = self.sel.as_ref() == Some(&sel);
        self.sel = Some(sel.clone());
        match (sel, part) {
            (_, PART_LESS) => vec![key::LEFT],
            (_, PART_MORE) => vec![key::RIGHT],
            (Sel::Member(_), PART_MUTE) => vec![key::CROSS],
            // the actions and the fields at once
            (Sel::Leave | Sel::Join | Sel::Refresh | Sel::Name | Sel::Password, _) => vec![key::CROSS],
            // a value's label steps it on, once chosen
            (Sel::Mode | Sel::Unlisted, _) if was => vec![key::RIGHT],
            (_, _) if was => vec![key::CROSS],
            _ => vec![MOVED],
        }
    }

    /// The legend under the list for selection `s`.
    fn legend(s: &Sel, v: &VoiceView) -> String {
        let close = "|t Close";
        let parts = "|7|8 Parts";
        match s {
            Sel::Mode => format!("|V|v Voice chat   {parts}   {close}"),
            Sel::Member(peer) => {
                let muted = v.members.iter().any(|m| m.peer == *peer && m.muted);
                format!("|V|v Volume   |x {}   {parts}   {close}", if muted { "Unmute" } else { "Mute" })
            }
            Sel::Leave => format!("|x Leave the room   {parts}   {close}"),
            Sel::Name | Sel::Password => format!("|x Type it   {parts}   {close}"),
            Sel::Unlisted => format!("|V|v Unlisted or not   {parts}   {close}"),
            Sel::Join => format!("|x Join   {parts}   {close}"),
            Sel::Refresh => format!("|x Ask for the list again   {parts}   {close}"),
            Sel::Room(name) if v.room.as_ref().is_some_and(|r| same_room(&r.0, name)) => format!("You are in this room   {parts}   {close}"),
            Sel::Room(_) => format!("|x Join   {parts}   {close}"),
        }
    }

    /// The status line: the connection, and whom the player is heard by.
    fn status(v: &VoiceView) -> String {
        if !v.available || v.mode == VoiceMode::Off {
            return "Off".to_string();
        }
        let demo = v.mode == VoiceMode::Demo;
        if !v.connected {
            return if demo { "Connecting to the demo..." } else { "Connecting..." }.to_string();
        }
        match heard_by(v.transmitting) {
            "" if demo => "Connected to the demo".to_string(),
            "" => "Connected".to_string(),
            who => format!("{}: {who}", if demo { "Demo" } else { "Connected" }),
        }
    }

    /// Which keys talk, or how the player is heard.
    fn hint(v: &VoiceView) -> String {
        let [group, room] = talk_keys();
        let held = format!("Hold {group} to talk to your group, {room} to your room.");
        match v.mode {
            VoiceMode::Off => String::new(),
            VoiceMode::PushToTalk => held,
            VoiceMode::OpenMic => "Open mic: your group and your room hear you whenever you speak.".to_string(),
            VoiceMode::Demo => format!("eqoaclassic's demo: {held}"),
        }
    }

    /// `OnDrawSelf`: the window, voice's rows, the list and its scroll bar,
    /// the legend; what a pointer meets noted.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, v: &VoiceView) {
        if !self.open {
            return;
        }
        let rows = self.rows(v);
        let cur = self.current(&rows);
        self.follow(&rows);
        self.hits.clear();
        self.drawn = std::iter::once(Some(Sel::Mode)).chain(rows.iter().map(Row::sel)).collect();
        self.window.init(vui, r, [320, 224], WIDTH, ROWS, 0);
        self.window.set_title(Some(&wide("Voice Chat")));
        self.window.draw(vui, r);
        let (black, blue, dim) = (UI_COLORS[0], UI_COLORS[1], UI_COLORS[2]);
        let w = &self.window;
        let on = cur == Sel::Mode;
        // Voice Chat and its value, the arrows on it while chosen
        w.text_box(vui, r, &wide("Voice Chat"), 40, 0, VALUE_X - 60, 1, UI_COLORS[on as usize], 0);
        if on {
            w.tick_r(vui, r, 36, 0, true);
        }
        w.draw_frame(vui, r, VALUE_X, 0, VALUE_W, 1, if on { 96 } else { 64 });
        w.text_box(vui, r, &wide(mode_name(v.mode)), VALUE_X, 0, VALUE_W, 1, black, 8 | 2);
        let rr = rows_rect(w, vui, r, 40, 0, VALUE_X - 60, 1);
        self.hits.push(rr, hit(0, PART_ROW));
        let at = w.top_left()[0] + VALUE_X;
        let vr = rows_rect(w, vui, r, VALUE_X, 0, VALUE_W, 1);
        self.hits.push([at - 4, vr[1], at + VALUE_W / 2, vr[3]], hit(0, PART_LESS));
        self.hits.push([at + VALUE_W / 2, vr[1], at + VALUE_W + 4, vr[3]], hit(0, PART_MORE));
        // the microphone's level (green while heard), then the connection
        if v.available && v.mode != VoiceMode::Off {
            w.text_box(vui, r, &wide("Microphone"), 40, 1, VALUE_X - 60, 1, black, 0);
            w.draw_frame(vui, r, VALUE_X, 1, VALUE_W, 1, 64);
            let h = vui.font_height(r) - 4;
            let y = w.row_center(vui, r, 1)[1] - 4;
            let x0 = at;
            let full = VALUE_W;
            let lit = (v.mic_level.clamp(0.0, 1.0) * full as f32) as i32;
            if lit > 0 {
                let c = if v.transmitting != 0 { [0.28, 0.82, 0.32, 1.0] } else { [0.55, 0.55, 0.52, 1.0] };
                vui.fill_rectangle(r, [[x0, y + 1], [x0 + lit, y + h - 2]], c);
            }
            // where speech begins (`voice::SPEAKING`)
            let s = x0 + (crate::voice::SPEAKING * full as f32) as i32;
            vui.fill_rectangle(r, [[s, y], [s + 1, y + h - 1]], [0.15, 0.15, 0.15, 1.0]);
        }
        w.text_box(vui, r, &wide(&Self::status(v)), 40, 2, WIDTH - 80, 1, black, 2);
        // the keys, and voice's last line (or the window's own)
        w.text_box(vui, r, &wide(&Self::hint(v)), 40, 3, WIDTH - 80, 1, black, 2);
        let said = self.said.as_ref().filter(|s| s.1 == v.status).map(|s| s.0.clone());
        let line = said.unwrap_or_else(|| v.status.clone());
        w.text_box(vui, r, &wide(&line), 40, 4, WIDTH - 80, 1, dim, 2);
        // the list, framed, its bar's thumb where the top is
        let most = rows.len().saturating_sub(LIST_ROWS);
        let t = if most > 0 { self.top as f32 / most as f32 } else { -1.0 };
        w.draw_frame_vscroll(vui, r, LIST_X, LIST_ROW, LIST_W, LIST_ROWS as i32, t);
        let pct = |x: f32| format!("{}%", (x * 100.0).round() as i32);
        for (k, row) in rows.iter().enumerate().skip(self.top).take(LIST_ROWS) {
            let wr = LIST_ROW + (k - self.top) as i32;
            let slot = k + 1;
            let chosen = row.sel().is_some_and(|s| s == cur);
            let c = UI_COLORS[chosen as usize];
            if chosen {
                w.tick_r(vui, r, LIST_X + 20, wr, true);
            }
            let whole = rows_rect(w, vui, r, LIST_X + NAME_X, wr, LIST_W - NAME_X - 8, 1);
            match row {
                Row::Heading(text, right) => {
                    w.text_box(vui, r, &wide(text), LIST_X + 6, wr, LIST_W - 150, 1, blue, 2);
                    w.text_box(vui, r, &wide(right), LIST_X + LIST_W - 160, wr, 144, 1, dim, 0x10 | 2);
                }
                Row::Note(text) => {
                    w.text_box(vui, r, &wide(text), LIST_X + 14, wr, LIST_W - 28, 1, dim, 2);
                }
                Row::Member(m) => {
                    // the speaking mark: lit while it speaks, red while muted
                    let y = w.row_center(vui, r, wr)[1];
                    let x = w.top_left()[0] + LIST_X + MARK_X;
                    let mc = if m.muted {
                        [0.78, 0.2, 0.16, 1.0]
                    } else if m.level >= crate::voice::SPEAKING {
                        [0.3, 0.86, 0.34, 1.0]
                    } else {
                        [0.4, 0.4, 0.38, 1.0]
                    };
                    vui.fill_rectangle(r, [[x, y - 5], [x + 8, y + 3]], mc);
                    w.text_box(vui, r, &wide(&m.name), LIST_X + NAME_X, wr, NAME_W, 1, c, 2);
                    if m.group && m.room {
                        w.text_box(vui, r, &wide("room"), LIST_X + TAG_X, wr, SLIDER_X - TAG_X - 6, 1, dim, 0);
                    }
                    let t = (m.volume / VOLUME_MAX).clamp(0.0, 1.0);
                    w.draw_hscroll(vui, r, LIST_X + SLIDER_X, wr, SLIDER_W, t, if chosen { 32 } else { 0 });
                    w.text_box(vui, r, &wide(&pct(m.volume)), LIST_X + PERCENT_X, wr, PERCENT_W, 1, c, 0x10);
                    w.draw_frame(vui, r, LIST_X + MUTE_X, wr, MUTE_W, 1, 64);
                    let (word, wc) = if m.muted { ("Muted", [0.7, 0.12, 0.1, 1.0]) } else { ("Mute", black) };
                    w.text_box(vui, r, &wide(word), LIST_X + MUTE_X, wr, MUTE_W, 1, wc, 8);
                    // the row; the slider's sides of its knob; Mute
                    let sl = rows_rect(w, vui, r, LIST_X + SLIDER_X, wr, SLIDER_W, 1);
                    let sx = w.top_left()[0] + LIST_X + SLIDER_X;
                    let knob = vui.entry(166).uv1[0];
                    let mid = sx + (t * (SLIDER_W - knob) as f32) as i32 + knob / 2;
                    let mx = w.top_left()[0] + LIST_X + MUTE_X;
                    self.hits.push([whole[0], whole[1], sx - 4, whole[3]], hit(slot, PART_ROW));
                    self.hits.push([sx - 4, sl[1], mid, sl[3]], hit(slot, PART_LESS));
                    self.hits.push([mid, sl[1], sx + SLIDER_W + 4, sl[3]], hit(slot, PART_MORE));
                    self.hits.push([mx - 4, sl[1], mx + MUTE_W + 4, sl[3]], hit(slot, PART_MUTE));
                }
                Row::Leave | Row::Join | Row::Refresh => {
                    let text = match row {
                        Row::Leave => "Leave the room",
                        Row::Join => "Join the room, or make it",
                        _ => "Ask for the list again",
                    };
                    w.text_box(vui, r, &wide(text), LIST_X + NAME_X, wr, LIST_W - NAME_X - 20, 1, c, 2);
                    self.hits.push(whole, hit(slot, PART_ROW));
                }
                Row::Name | Row::Password | Row::Unlisted => {
                    let (label, value, after) = match row {
                        Row::Name => ("Name", if self.form.name.is_empty() { None } else { Some(self.form.name.clone()) }, ""),
                        // as stars, one a character typed
                        Row::Password => ("Password", (!self.form.password.is_empty()).then(|| "*".repeat(self.form.password.chars().count())), "optional"),
                        _ => ("Unlisted", Some(if self.form.unlisted { "Yes" } else { "No" }.to_string()), "if new"),
                    };
                    w.text_box(vui, r, &wide(label), LIST_X + NAME_X, wr, FIELD_X - NAME_X - 8, 1, c, 0);
                    let arrows = chosen && matches!(row, Row::Unlisted);
                    w.draw_frame(vui, r, LIST_X + FIELD_X, wr, FIELD_W, 1, if arrows { 96 } else { 64 });
                    match value {
                        Some(s) => w.text_box(vui, r, &wide(&s), LIST_X + FIELD_X, wr, FIELD_W, 1, black, 8 | 2),
                        None => w.text_box(vui, r, &wide("none"), LIST_X + FIELD_X, wr, FIELD_W, 1, dim, 8),
                    };
                    // past the value's side arrow (atlas 128) while it is chosen
                    w.text_box(vui, r, &wide(after), LIST_X + FIELD_X + FIELD_W + 24, wr, LIST_W - FIELD_X - FIELD_W - 32, 1, dim, 2);
                    let fr = rows_rect(w, vui, r, LIST_X + FIELD_X, wr, FIELD_W, 1);
                    let fx = w.top_left()[0] + LIST_X + FIELD_X;
                    self.hits.push([whole[0], whole[1], fx - 4, whole[3]], hit(slot, PART_ROW));
                    if matches!(row, Row::Unlisted) {
                        self.hits.push([fx - 4, fr[1], fx + FIELD_W / 2, fr[3]], hit(slot, PART_LESS));
                        self.hits.push([fx + FIELD_W / 2, fr[1], fx + FIELD_W + 4, fr[3]], hit(slot, PART_MORE));
                    } else {
                        self.hits.push([fx - 4, fr[1], fx + FIELD_W + 4, fr[3]], hit(slot, PART_ROW));
                    }
                }
                Row::Room(room) => {
                    let here = v.room.as_ref().is_some_and(|x| same_room(&x.0, &room.name));
                    if here {
                        let y = w.row_center(vui, r, wr)[1];
                        let x = w.top_left()[0] + LIST_X + MARK_X;
                        vui.fill_rectangle(r, [[x, y - 5], [x + 8, y + 3]], [0.3, 0.86, 0.34, 1.0]);
                    }
                    w.text_box(vui, r, &wide(&room.name), LIST_X + NAME_X, wr, 250, 1, c, 2);
                    w.text_box(vui, r, &wide(&people(room.members as usize)), LIST_X + 290, wr, 100, 1, c, 0x10);
                    if room.locked {
                        w.text_box(vui, r, &wide("Locked"), LIST_X + 410, wr, LIST_W - 420, 1, dim, 0);
                    }
                    self.hits.push(whole, hit(slot, PART_ROW));
                }
            }
        }
        // the legend: its glyphs are their buttons (`pointer`)
        w.text_box(vui, r, &wide(&Self::legend(&cur, v)), 40, LEGEND_ROW, WIDTH - 80, 1, black, 0);
    }

    /// The form's room name, as typed (the edit box starts on it).
    pub fn form_name(&self) -> String {
        self.form.name.clone()
    }

    /// The selection put on a field of the form (tests).
    #[cfg(test)]
    pub fn select_field_for_test(&mut self, f: Field) {
        self.sel = Some(match f {
            Field::Name => Sel::Name,
            Field::Password => Sel::Password,
            Field::RoomPassword(n) => Sel::Room(n),
        });
    }

    /// Whether the form holds a password (tests).
    #[cfg(test)]
    pub fn has_password_for_test(&self) -> bool {
        !self.form.password.is_empty()
    }

    /// A report's line: open or not and where (no name, no password).
    pub fn describe(&self) -> String {
        let at = match &self.sel {
            None | Some(Sel::Mode) => "voice chat's row",
            Some(Sel::Member(_)) => "a member",
            Some(Sel::Leave) => "leave",
            Some(Sel::Name | Sel::Password | Sel::Unlisted | Sel::Join) => "the room form",
            Some(Sel::Refresh | Sel::Room(_)) => "the lobby",
        };
        if self.open { format!("voice window: open, on {at}, the list from row {}", self.top) } else { "voice window: closed".to_string() }
    }
}

/// The HUD's mic badge while voice is on: connected or not, heard or not,
/// and the microphone's level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Badge {
    pub connected: bool,
    pub transmitting: bool,
    pub level: f32,
}

/// The badge for view `v`: none while voice is off or cannot be had.
pub fn badge(v: &VoiceView) -> Option<Badge> {
    (v.available && v.mode != VoiceMode::Off).then_some(Badge { connected: v.connected, transmitting: v.connected && v.transmitting != 0, level: v.mic_level.clamp(0.0, 1.0) })
}

/// The badge at `BADGE` (the caller's anchors placing it): a darkened
/// square in the frame art a value's frame has, and a microphone in it:
/// grey, pulsing while it connects; green while the player is heard, its
/// head filling with the microphone's level.
pub fn draw_badge(vui: &Vui, r: &mut dyn Layout, b: &Badge, time_ms: i64) {
    let [x0, y0, x1, y1] = BADGE;
    r.fill_blended([0.0, 0.0, 0.0, 0.5], [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]);
    let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
    vui.blit_frame(r, [x0, y0], [x1 - x0, y1 - y0], &fp, 0, 1.0, true);
    let c = if b.transmitting {
        [0.3, 0.9, 0.35, 1.0]
    } else if b.connected {
        [0.86, 0.86, 0.82, 1.0]
    } else {
        // a second's slow throb while it connects
        let t = (time_ms.rem_euclid(1000) as f32 / 1000.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        let g = 0.35 + 0.35 * t;
        [g, g, g * 0.96, 1.0]
    };
    let cx = (x0 + x1) / 2;
    let fill = |r: &mut dyn Layout, a: [i32; 2], b: [i32; 2], c: [f32; 4]| vui.fill_rectangle(r, [a, b], c);
    // the head: rounded, hollow but for the level from its foot
    fill(r, [cx - 2, y0 + 4], [cx + 3, y0 + 16], c);
    fill(r, [cx - 3, y0 + 5], [cx + 4, y0 + 15], c);
    fill(r, [cx - 2, y0 + 6], [cx + 3, y0 + 14], [0.06, 0.06, 0.06, 1.0]);
    if b.transmitting {
        let h = (b.level * 8.0).round() as i32;
        if h > 0 {
            fill(r, [cx - 2, y0 + 14 - h], [cx + 3, y0 + 14], c);
        }
    }
    // the holder round its foot, the stem and the base
    fill(r, [cx - 6, y0 + 11], [cx - 5, y0 + 18], c);
    fill(r, [cx + 6, y0 + 11], [cx + 7, y0 + 18], c);
    fill(r, [cx - 6, y0 + 18], [cx + 7, y0 + 19], c);
    fill(r, [cx, y0 + 19], [cx + 1, y0 + 22], c);
    fill(r, [cx - 3, y0 + 22], [cx + 4, y0 + 23], c);
}

/// `EQOA_VOICE_WINDOW_TEST`'s view: a group of three with voice on (one
/// speaking, one muted, one in the room too), a locked room with two more,
/// and a lobby of five, connected, the group hearing the player.
pub fn test_view() -> VoiceView {
    let m = |peer: u32, name: &str, group: bool, room: bool, level: f32, volume: f32, muted: bool| VoiceMember { peer, name: name.to_string(), group, room, level, volume, muted };
    let room = |name: &str, members: u16, locked: bool| VoiceRoomInfo { name: name.to_string(), members, locked };
    VoiceView {
        available: true,
        demo_only: false,
        mode: VoiceMode::PushToTalk,
        connected: true,
        mic_level: 0.62,
        transmitting: 1,
        room: Some(("Den of Thieves".to_string(), true, false)),
        members: vec![
            m(11, "Ashwind", true, false, 0.7, 1.0, false),
            m(12, "Brimble", true, true, 0.0, 1.5, false),
            m(13, "Corva", true, false, 0.0, 1.0, true),
            m(21, "Dunmore", false, true, 0.5, 1.0, false),
            m(22, "Elsabeth", false, true, 0.0, 0.6, false),
        ],
        lobby: vec![room("Freeport Militia", 12, false), room("Qeynos Hills XP", 5, false), room("Den of Thieves", 4, true), room("Late Night Crafters", 2, true), room("Highpass Traders", 1, false)],
        status: "Voice: in room Den of Thieves (locked).".to_string(),
    }
}

/// An ask carried out on a test's own view (`VoiceWnd::test`), as voice
/// would come to show it.
pub fn apply_test(v: &mut VoiceView, a: &Ask) {
    match a {
        Ask::Mode(m) => {
            v.mode = *m;
            v.connected = *m != VoiceMode::Off;
            v.transmitting = 0;
        }
        Ask::Join { name, password, unlisted } => {
            for m in &mut v.members {
                m.room = false;
            }
            v.members.retain(|m| m.group);
            v.room = Some((name.clone(), !password.is_empty(), *unlisted));
            v.status = format!("Voice: in room {name}.");
        }
        Ask::Leave => {
            for m in &mut v.members {
                m.room = false;
            }
            v.members.retain(|m| m.group);
            v.room = None;
            v.status = "Voice: left the room.".to_string();
        }
        Ask::Volume(p, x) => v.members.iter_mut().filter(|m| m.peer == *p).for_each(|m| m.volume = *x),
        Ask::Muted(p, x) => v.members.iter_mut().filter(|m| m.peer == *p).for_each(|m| m.muted = *x),
        Ask::Refresh | Ask::Edit(_) | Ask::Close => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};
    use vi::ui::Raster;

    fn opened(v: &VoiceView) -> VoiceWnd {
        let mut w = VoiceWnd::default();
        assert_eq!(w.open(), vec![Ask::Refresh], "the lobby asked for at once");
        w.draw(&vui(), &mut NullRaster, v);
        w
    }

    /// The selection's kind, for asserting.
    fn on(w: &VoiceWnd) -> Sel {
        w.sel.clone().unwrap()
    }

    /// Voice off, unavailable, or on a server with none of its own but not
    /// the demo: one note and nothing to choose but Voice Chat; on: the
    /// group (each member once: one in the room too stays in the group's
    /// part), the room with Leave and its other members, the form, the
    /// lobby with Refresh.
    #[test]
    fn the_rows_follow_the_view() {
        let w = VoiceWnd::default();
        let on_view = test_view();
        for (v, has) in [
            (VoiceView { available: false, ..on_view.clone() }, "not in this build"),
            (VoiceView { mode: VoiceMode::Off, ..on_view.clone() }, "Push to Talk or Open Mic"),
            (VoiceView { mode: VoiceMode::Off, demo_only: true, ..on_view.clone() }, "Demo above"),
            (VoiceView { demo_only: true, ..on_view.clone() }, "Demo above"),
        ] {
            let rows = w.rows(&v);
            assert!(matches!(&rows[..], [Row::Note(n)] if n.contains(has)), "{rows:?}");
            assert_eq!(VoiceWnd::selectable(&rows), vec![Sel::Mode]);
        }
        let rows = w.rows(&VoiceView { demo_only: true, mode: VoiceMode::Demo, ..on_view.clone() });
        assert!(rows.len() > 10, "the demo: the whole list");
        let rows = w.rows(&on_view);
        let sels = VoiceWnd::selectable(&rows);
        use Sel::*;
        assert_eq!(
            sels,
            vec![Mode, Member(11), Member(12), Member(13), Leave, Member(21), Member(22), Name, Password, Unlisted, Join, Refresh, Room("Freeport Militia".into()), Room("Qeynos Hills XP".into()), Room("Den of Thieves".into()), Room("Late Night Crafters".into()), Room("Highpass Traders".into())]
        );
        assert!(matches!(&rows[0], Row::Heading(t, n) if t == "Your group" && n == "3 people"));
        assert!(rows.iter().any(|r| matches!(r, Row::Heading(t, m) if t == "Room: Den of Thieves" && m == "Locked")));
        // no room, no group: their notes
        let alone = VoiceView { room: None, members: Vec::new(), lobby: Vec::new(), ..on_view };
        let rows = w.rows(&alone);
        let notes: Vec<&str> = rows.iter().filter_map(|r| if let Row::Note(n) = r { Some(n.as_str()) } else { None }).collect();
        assert_eq!(notes.len(), 3, "{notes:?}");
        assert!(!rows.iter().any(|r| matches!(r, Row::Leave)), "no room, nothing to leave");
    }

    /// Up and down pass over headings and notes, kept at the ends (the
    /// end's sound); L1 and R1 go to the parts' first rows; the list
    /// follows the selection with its heading.
    #[test]
    fn the_keys_move_over_what_can_be_chosen() {
        let v = test_view();
        let mut w = opened(&v);
        assert_eq!(on(&w), Sel::Mode);
        w.key(key::UP, &v, 0);
        assert_eq!((on(&w), w.take_cues()), (Sel::Mode, vec![9]), "the top holds");
        w.key(key::DOWN, &v, 0);
        assert_eq!((on(&w), w.take_cues()), (Sel::Member(11), vec![11]), "the heading passed over");
        for _ in 0..3 {
            w.key(20, &v, 0);
        }
        assert_eq!(on(&w), Sel::Leave, "the keyboard's down, past the room's heading");
        w.key(key::R1, &v, 0);
        assert_eq!(on(&w), Sel::Name, "the next part");
        w.key(key::R1, &v, 0);
        assert_eq!(on(&w), Sel::Refresh, "the lobby");
        w.key(key::L1, &v, 0);
        assert_eq!(on(&w), Sel::Name, "back a part");
        w.key(key::DOWN, &v, 0);
        w.key(key::L1, &v, 0);
        assert_eq!(on(&w), Sel::Name, "inside a part, L1 is its first row");
        w.key(23, &v, 0);
        assert_eq!(on(&w), Sel::Room("Highpass Traders".into()), "End");
        let rows = w.rows(&v);
        assert!(w.top > 0 && w.top + LIST_ROWS == rows.len(), "the list scrolled to its end: {}", w.top);
        w.key(24, &v, 0);
        assert_eq!((on(&w), w.top), (Sel::Mode, 0), "Home");
        // a page is the list's rows less one: eleven
        assert_eq!(LIST_ROWS, 12);
        w.key(22, &v, 0);
        assert_eq!(on(&w), Sel::Password, "a page down: the last within eleven rows (the list's 10th)");
        w.key(22, &v, 0);
        assert_eq!(on(&w), Sel::Room("Highpass Traders".into()), "and the next: the list's end, within eleven");
        w.key(21, &v, 0);
        assert_eq!(on(&w), Sel::Name, "a page up: the first within eleven rows back (the list's 9th; the 8th is a heading)");
        w.key(21, &v, 0);
        w.key(21, &v, 0);
        assert_eq!(on(&w), Sel::Mode, "past the list's top: Voice Chat");
        w.key(119, &v, 0);
        assert_eq!((on(&w), w.take_cues().last().copied()), (Sel::Mode, Some(11)), "the control: a key not its own does nothing");
    }

    /// Left and right: Voice Chat round its choices (Off and Demo on a
    /// server with none of its own), a member's volume a tenth within 0 to
    /// 200% (the ends' sound past them), Unlisted turned; nothing else.
    #[test]
    fn left_and_right_change_values() {
        let mut v = test_view();
        let mut w = opened(&v);
        assert_eq!(w.key(key::RIGHT, &v, 0), vec![Ask::Mode(VoiceMode::OpenMic)]);
        assert_eq!(w.key(key::LEFT, &v, 0), vec![Ask::Mode(VoiceMode::Off)]);
        v.mode = VoiceMode::Off;
        assert_eq!(w.key(key::LEFT, &v, 0), vec![Ask::Mode(VoiceMode::OpenMic)], "round");
        assert_eq!(w.key(key::CROSS, &v, 0), vec![Ask::Mode(VoiceMode::PushToTalk)], "✕ steps on");
        let demo = VoiceView { demo_only: true, mode: VoiceMode::Off, ..test_view() };
        assert_eq!(w.key(key::RIGHT, &demo, 0), vec![Ask::Mode(VoiceMode::Demo)]);
        assert_eq!(w.key(key::RIGHT, &VoiceView { mode: VoiceMode::Demo, ..demo.clone() }, 0), vec![Ask::Mode(VoiceMode::Off)]);
        assert_eq!(w.key(key::RIGHT, &VoiceView { mode: VoiceMode::PushToTalk, ..demo }, 0), vec![Ask::Mode(VoiceMode::Off)], "a choice it does not offer: its first");
        let v = test_view();
        w.key(key::DOWN, &v, 0);
        assert_eq!(w.key(key::RIGHT, &v, 0), vec![Ask::Volume(11, 1.1)]);
        assert_eq!(w.key(key::LEFT, &v, 0), vec![Ask::Volume(11, 0.9)]);
        w.take_cues();
        let loud = VoiceView { members: vec![VoiceMember { volume: 2.0, ..v.members[0].clone() }], ..v.clone() };
        assert_eq!((w.key(key::RIGHT, &loud, 0), w.take_cues()), (vec![], vec![9]), "200% at most");
        let quiet = VoiceView { members: vec![VoiceMember { volume: 0.0, ..v.members[0].clone() }], ..v.clone() };
        assert_eq!((w.key(key::LEFT, &quiet, 0), w.take_cues()), (vec![], vec![9]), "0 at least");
        assert_eq!(w.key(key::LEFT, &quiet, 0), vec![], "the control: nothing at all past the end");
        w.sel = Some(Sel::Unlisted);
        assert!(w.key(key::RIGHT, &v, 0).is_empty() && w.form.unlisted);
        w.key(key::LEFT, &v, 0);
        assert!(!w.form.unlisted, "and back");
        w.sel = Some(Sel::Leave);
        w.take_cues();
        assert_eq!((w.key(key::RIGHT, &v, 0), w.take_cues()), (vec![], vec![9]), "the control: Leave has no value");
    }

    /// ✕ on each row: a member muted or unmuted (□ too), Leave, Refresh, the
    /// fields to the edit box, Join with the form (refused with no name, the
    /// password gone from the form once sent), a lobby room joined (a locked
    /// one asks its password first, the room the player is in nothing).
    #[test]
    fn cross_does_each_rows_thing() {
        let v = test_view();
        let mut w = opened(&v);
        let at = |w: &mut VoiceWnd, s: Sel, k: i32| {
            w.sel = Some(s);
            w.key(k, &v, 0)
        };
        assert_eq!(at(&mut w, Sel::Member(11), key::CROSS), vec![Ask::Muted(11, true)]);
        assert_eq!(at(&mut w, Sel::Member(13), key::CROSS), vec![Ask::Muted(13, false)], "muted: unmuted");
        assert_eq!(at(&mut w, Sel::Member(13), key::SQUARE), vec![Ask::Muted(13, false)], "□ too");
        assert_eq!(at(&mut w, Sel::Leave, key::CROSS), vec![Ask::Leave]);
        assert_eq!(at(&mut w, Sel::Refresh, 13), vec![Ask::Refresh], "Enter is ✕");
        assert_eq!(at(&mut w, Sel::Name, key::CROSS), vec![Ask::Edit(Field::Name)]);
        assert!(w.edits(EDIT_NAME) && !w.edits(EDIT_PASSWORD));
        assert_eq!(at(&mut w, Sel::Join, key::CROSS), vec![], "no name yet");
        assert!(w.said.as_ref().is_some_and(|s| s.0.contains("3 to 24")));
        w.form = Form { name: " Raid Night ".into(), password: "hunter22".into(), unlisted: true };
        assert_eq!(at(&mut w, Sel::Join, key::CROSS), vec![Ask::Join { name: "Raid Night".into(), password: "hunter22".into(), unlisted: true }]);
        assert!(w.form.password.is_empty() && w.form.name == " Raid Night ", "the password forgotten, the name kept");
        assert_eq!(at(&mut w, Sel::Room("Freeport Militia".into()), key::CROSS), vec![Ask::Join { name: "Freeport Militia".into(), password: String::new(), unlisted: false }]);
        assert_eq!(at(&mut w, Sel::Room("Late Night Crafters".into()), key::CROSS), vec![Ask::Edit(Field::RoomPassword("Late Night Crafters".into()))], "locked: its password first");
        assert_eq!(at(&mut w, Sel::Room("Den of Thieves".into()), key::CROSS), vec![], "the control: the room the player is in");
        // the player's room as voice spells it, the lobby's as the server
        // does: one room (voice matches names case-folded)
        let mut lower = v.clone();
        lower.room = Some(("den of thieves".into(), true, false));
        w.sel = Some(Sel::Room("Den of Thieves".into()));
        assert_eq!(w.key(key::CROSS, &lower, 0), vec![], "the same room, any case");
        assert_eq!(at(&mut w, Sel::Mode, key::TRIANGLE), vec![Ask::Close]);
        assert_eq!(at(&mut w, Sel::Name, 27), vec![Ask::Close], "Escape");
        assert_eq!(at(&mut w, Sel::Name, key::START), vec![Ask::Close], "Start");
    }

    /// What the edit box gives back: the name trimmed, the password kept
    /// (and given to the report's scrubber), a locked room's password
    /// joining it; nothing typed for a locked room, or a cancel, joins
    /// nothing; a field not asked for is no field's.
    #[test]
    fn the_edit_box_fills_the_fields() {
        let v = test_view();
        let mut w = opened(&v);
        assert_eq!(w.edited(Some("x".into())), vec![], "the control: no field asked for");
        assert_eq!(w.form, Form::default());
        w.sel = Some(Sel::Name);
        w.key(key::CROSS, &v, 0);
        assert_eq!(w.edited(Some("  Late Night  ".into())), vec![]);
        assert_eq!(w.form.name, "Late Night");
        w.sel = Some(Sel::Password);
        w.key(key::CROSS, &v, 0);
        assert!(w.edits(EDIT_PASSWORD));
        w.edited(Some("pa55word".into()));
        assert_eq!(w.form.password, "pa55word");
        w.sel = Some(Sel::Room("Late Night Crafters".into()));
        w.key(key::CROSS, &v, 0);
        assert!(w.edits(EDIT_ROOM_PASSWORD));
        assert_eq!(w.edited(Some("open sesame".into())), vec![Ask::Join { name: "Late Night Crafters".into(), password: "open sesame".into(), unlisted: false }]);
        w.key(key::CROSS, &v, 0);
        assert_eq!(w.edited(Some(String::new())), vec![], "nothing typed");
        w.key(key::CROSS, &v, 0);
        assert_eq!(w.edited(None), vec![], "cancelled");
        assert!(!w.edits(EDIT_ROOM_PASSWORD), "and let go");
        w.close();
        assert!(w.form.password.is_empty() && w.form.name == "Late Night", "closed: the password forgotten");
    }

    /// A raster that keeps every string drawn.
    #[derive(Default)]
    struct Strings(Vec<String>);

    impl crate::ui::Layout for Strings {}

    impl Raster for Strings {
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

    /// A password is drawn as stars and nowhere as itself, nor in a
    /// `Debug` print or the report's line; the control: the room's name is
    /// drawn as it is.
    #[test]
    fn a_password_is_never_shown() {
        let v = test_view();
        let mut w = opened(&v);
        w.form = Form { name: "Raid Night".into(), password: "hunter22".into(), unlisted: false };
        w.sel = Some(Sel::Password);
        let mut r = Strings::default();
        w.draw(&vui(), &mut r, &v);
        let all = r.0.join("\n");
        assert!(all.contains("********") && all.contains("Raid Night"), "{all}");
        assert!(!all.contains("hunter22"));
        let join = Ask::Join { name: "Raid Night".into(), password: "hunter22".into(), unlisted: false };
        for s in [format!("{join:?}"), format!("{:?}", w.form), w.describe()] {
            assert!(!s.contains("hunter22"), "{s}");
        }
        assert!(format!("{join:?}").contains("<hidden>") && format!("{join:?}").contains("Raid Night"));
    }

    /// The pointer on the rows as drawn: a row chosen (the move's sound),
    /// chosen again pressed; a slider's sides of its knob left and right;
    /// Mute, the actions and the fields at once; Voice Chat's value's
    /// halves; nothing off the window or before it was drawn (the
    /// controls).
    #[test]
    fn a_pointer_on_its_rows() {
        let v = test_view();
        let mut w = VoiceWnd::default();
        w.open();
        assert_eq!(w.point([320.0, 224.0]), Vec::<i32>::new(), "the control: not drawn yet");
        w.draw(&vui(), &mut NullRaster, &v);
        let rect = |w: &VoiceWnd, slot: usize, part: i32| w.hits.all().iter().find(|h| h.1 == hit(slot, part)).map(|h| h.0).unwrap();
        // the list's row 1 is Ashwind's (slot 2)
        let ash = mid(rect(&w, 2, PART_ROW));
        assert_eq!(w.point(ash), vec![MOVED]);
        assert_eq!(on(&w), Sel::Member(11));
        assert_eq!(w.point(ash), vec![key::CROSS], "chosen again: pressed (its mute)");
        assert_eq!(w.point(mid(rect(&w, 2, PART_LESS))), vec![key::LEFT]);
        assert_eq!(w.point(mid(rect(&w, 2, PART_MORE))), vec![key::RIGHT]);
        // Brimble's Mute at once (slot 3)
        assert_eq!(w.point(mid(rect(&w, 3, PART_MUTE))), vec![key::CROSS]);
        assert_eq!(on(&w), Sel::Member(12));
        // Voice Chat's value
        assert_eq!(w.point(mid(rect(&w, 0, PART_LESS))), vec![key::LEFT]);
        assert_eq!(w.point(mid(rect(&w, 0, PART_MORE))), vec![key::RIGHT]);
        assert_eq!(on(&w), Sel::Mode);
        // Leave, at once (slot 6: the room's heading is 5)
        assert_eq!(w.point(mid(rect(&w, 6, PART_ROW))), vec![key::CROSS]);
        assert_eq!(on(&w), Sel::Leave);
        assert!(w.hits.all().iter().all(|h| h.1 / 4 != 5 && h.1 / 4 != 1), "a heading is no hit");
        assert_eq!(w.point([2.0, 2.0]), Vec::<i32>::new(), "the control: off the window");
        // and the keys a point gave act where it put the selection
        w.point(mid(rect(&w, 2, PART_ROW)));
        assert_eq!(w.key(key::CROSS, &v, 0), vec![Ask::Muted(11, true)]);
    }

    /// The badge: none while voice is off or cannot be had; lit only while
    /// the player is heard on a connection.
    #[test]
    fn the_badge_follows_voice() {
        let v = test_view();
        assert_eq!(badge(&v), Some(Badge { connected: true, transmitting: true, level: 0.62 }));
        assert_eq!(badge(&VoiceView { mode: VoiceMode::Off, ..v.clone() }), None);
        assert_eq!(badge(&VoiceView { available: false, ..v.clone() }), None);
        assert_eq!(badge(&VoiceView { transmitting: 0, ..v.clone() }).map(|b| b.transmitting), Some(false));
        assert_eq!(badge(&VoiceView { connected: false, ..v.clone() }).map(|b| (b.connected, b.transmitting)), Some((false, false)), "not heard before it connects");
        // clear of the minimap, the quest tracker, the quick menu's plate and the breath gauge
        let clear = |a: [i32; 4], b: [i32; 4]| a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1];
        for other in [crate::map::mini::FRAME, crate::tracker::COLUMN, crate::map::mini::PLATE, [430, 50, 566, 80]] {
            assert!(clear(BADGE, other), "{other:?}");
        }
        assert!(!clear(BADGE, [460, 150, 480, 170]), "the control: an overlap is seen");
    }

    /// The hint names the push-to-talk keys the Controls page has (V and N
    /// by default).
    #[test]
    fn the_hint_names_the_talk_keys() {
        crate::settings::testing::reset(false);
        let h = VoiceWnd::hint(&test_view());
        assert!(h.contains("Hold V") && h.contains("N to your room"), "{h}");
        assert_eq!(VoiceWnd::hint(&VoiceView { mode: VoiceMode::Off, ..test_view() }), "");
        assert!(VoiceWnd::hint(&VoiceView { mode: VoiceMode::OpenMic, ..test_view() }).starts_with("Open mic"));
    }

    /// The test view takes the window's asks as voice would.
    #[test]
    fn the_test_view_takes_the_asks() {
        let mut v = test_view();
        apply_test(&mut v, &Ask::Volume(11, 1.7));
        apply_test(&mut v, &Ask::Muted(12, true));
        assert!(v.members[0].volume == 1.7 && v.members[1].muted);
        apply_test(&mut v, &Ask::Leave);
        assert!(v.room.is_none() && v.members.iter().all(|m| m.group && !m.room));
        apply_test(&mut v, &Ask::Join { name: "Raid".into(), password: "x".into(), unlisted: true });
        assert_eq!(v.room, Some(("Raid".into(), true, true)));
        apply_test(&mut v, &Ask::Mode(VoiceMode::Off));
        assert!(!v.connected && badge(&v).is_none());
    }
}
