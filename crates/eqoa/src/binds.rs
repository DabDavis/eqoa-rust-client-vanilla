//! The player's own key and button bindings (F3 20261021: jib asked to rebind
//! every control, the keyboard's and the controller's, and dab for a legend
//! of the keyboard). The client's addition, not the game's: the PS2 has no
//! such thing, and the game's own paths (`VIWndGame::OnKeyDown`, the windows'
//! keys, `PadKeys`, `UpdateControls`) and the client's keyboard dispatch
//! (`viewer.rs`) are left as they are. A binding works at the input's edge:
//! the key or button the player pressed is turned into the one the default
//! layout would have sent, before any of them sees it.
//!
//! The keyboard: each row of the default layout is a key (`ROWS`, the keys
//! today's dispatch takes, paired with the pad button the game pairs it with
//! in play: `OnKeyDown`'s case lists, 32 with 134, 81/113 with 133 and so on,
//! and the client's E with ○, the arrows with the d-pad, WASD and I/J/K/L with
//! the sticks). A row moved to another key makes that key act as the row's own
//! in every place a key is read (play, the windows, the map), and the row's
//! old key do nothing (`alias`). A key moves with everything it does: E is
//! the quick menu's use in play and ✕ in a window, and both go with it. A key
//! a row is moved to that another row was on swaps the two. A row the game
//! has no key for (△'s, let the target go) can be given one, which presses
//! that button as the pad's would (`Alias::Button`). Text typed into an edit
//! box is never remapped (the viewer does not ask).
//!
//! The controller: a permutation of the 16 buttons, the physical button on
//! each of the game's `VIConsolePad` fields (`remap`, before `VIPad::Read`):
//! a button moved onto a field another button drove swaps the two. The sticks
//! stay sticks.
//!
//! With nothing moved every key is itself and every button its own field
//! (`Binds::default`, the tests). Kept per device in `keys.opt` beside the
//! client's options (`settings`), as the PS2 keeps every option on its card.

use crate::input::bit;
use winit::keyboard::{Key, NamedKey};

/// A stick's way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// What a row's controller column is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pad {
    None,
    /// a button, as `input::bit` has it
    Button(u16),
    /// a stick's way (the right stick when set); not rebound
    Stick(bool, Dir),
}

/// A row of the default layout.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    /// its name in `keys.opt`
    pub id: &'static str,
    /// what it does in play: the client's words, a WORDS.TXT word by number
    /// in braces (`{1018}` Attack)
    pub label: &'static str,
    /// the keys today's dispatch takes for it, the first shown (the rest the
    /// same key shifted: ~ with `)
    pub keys: &'static [&'static str],
    pub pad: Pad,
    /// the rest of what the key and the button do, the window's keys above
    /// all (a glyph is `|x`, as the windows' legends write them)
    pub help: &'static str,
}

const fn row(id: &'static str, label: &'static str, keys: &'static [&'static str], pad: Pad, help: &'static str) -> Row {
    Row { id, label, keys, pad, help }
}

const fn b(k: u16) -> Pad {
    Pad::Button(k)
}

/// The default layout: every key the client's keyboard dispatch takes outside
/// an edit box, paired with the pad's button where the game pairs them, and
/// the pad's buttons the keyboard has none for.
pub const ROWS: &[Row] = &[
    row("walk_forward", "Walk forward", &["w"], Pad::Stick(false, Dir::Up), "Pushed past the run's edge, a stick runs. On the map: pan up."),
    row("walk_back", "Walk back", &["s"], Pad::Stick(false, Dir::Down), "On the map: pan down."),
    row("walk_left", "Move left", &["a"], Pad::Stick(false, Dir::Left), "Turns or steps aside, as the camera has it. On the map: pan left."),
    row("walk_right", "Move right", &["d"], Pad::Stick(false, Dir::Right), "Turns or steps aside, as the camera has it. On the map: pan right."),
    row("look_up", "Camera up", &["i"], Pad::Stick(true, Dir::Up), "Tilts the camera, or brings the orbit in."),
    row("look_down", "Camera down", &["k"], Pad::Stick(true, Dir::Down), "Tilts the camera, or takes the orbit out."),
    row("look_left", "Camera left", &["j"], Pad::Stick(true, Dir::Left), "Orbits the camera, or turns."),
    row("look_right", "Camera right", &["l"], Pad::Stick(true, Dir::Right), "Orbits the camera, or turns."),
    row("attack", "{1018} (hold)", &["space"], b(bit::CROSS), "With Auto Attack on, a toggle; on a corpse, its loot. In a window |x is {212}, and Space is |( (a trade's {424}). On the map: |x."),
    row("talk", "Talk (hold: {1403})", &["q"], b(bit::SQUARE), "In a window it is |s, the window's own: a trade's {478}, the who list asked again, the mail's inbox."),
    row("clear_target", "Let the target go", &[], b(bit::TRIANGLE), "The game has no key for it: a key put here presses |t. In a window |t is {217}, Escape or Backspace on the keyboard."),
    row("use", "Quick menu: {120}", &["e"], b(bit::CIRCLE), "In a window E is |x {212}, as Enter is."),
    row("menu_up", "Quick menu: up", &["up"], b(bit::UP), "In a window, up a row. On the map: pan up."),
    row("menu_down", "Quick menu: down", &["down"], b(bit::DOWN), "In a window, down a row. On the map: pan down."),
    row("menu_prev", "Quick menu: {1692}", &["left"], b(bit::LEFT), "In a window, left. On the map: pan left."),
    row("menu_next", "Quick menu: {216}", &["right"], b(bit::RIGHT), "In a window, right. On the map: pan right."),
    row("cycle_target", "Target: {1405}, group, {418}", &["`", "~"], b(bit::L1), "Each press the next of the player, the group and the pet. On the map |7 zooms out."),
    row("quick_chat", "Quick chat", &["z"], b(bit::L2), "Again, it closes the quick chat."),
    row("next_target", "Target: the next", &["tab"], b(bit::R1), "In a window Tab is |8 (Class Mastery's other list). On the map |8 zooms in."),
    row("history", "{424} {1694} (hold)", &["c"], b(bit::R2), "Up and down scroll it while held. In a window C is |c (the factions' next page)."),
    row("autorun", "Autorun", &["x"], b(bit::L3), "A stick pushed past the run's edge ends it."),
    row("chat", "{424}", &["enter"], b(bit::R3), "In a window Enter is |x {212}; over the quick chat, the chat line. On the map: |x."),
    row("start_menu", "Start menu", &["escape"], b(bit::START), "In a window Escape is |t {217}. Escape also cancels a key being chosen here."),
    row("camera_style", "Camera style", &["p"], b(bit::SELECT), "First person, and back."),
    row("back", "{217} (in a window)", &["backspace"], Pad::None, "In a window it is |t {217}. On the map it closes it."),
    row("command", "{927}: /", &["/"], Pad::None, "Opens the chat line with / begun."),
    row("reply", "{534}", &["r"], Pad::None, "Opens the chat line with /reply to the last tell."),
    row("tell", "{425}", &["t"], Pad::None, "Opens the chat line with /tell."),
    row("target_self", "Target: {1405}", &["1"], Pad::None, ""),
    row("target_2", "Target: group 2", &["2"], Pad::None, ""),
    row("target_3", "Target: group 3", &["3"], Pad::None, ""),
    row("target_4", "Target: group 4", &["4"], Pad::None, ""),
    row("target_5", "Target: group 5", &["5"], Pad::None, ""),
    row("target_pet", "Target: {418}", &["6"], Pad::None, ""),
    row("orbit_left", "Orbit left", &["home"], Pad::None, "The orbiting camera's. On the map: |x."),
    row("orbit_right", "Orbit right", &["end"], Pad::None, "The orbiting camera's."),
    row("tilt_up", "Tilt up, zoom in", &["pageup"], Pad::None, "On the map: zoom in."),
    row("tilt_down", "Tilt down, zoom out", &["pagedown"], Pad::None, "On the map: zoom out."),
    row("map", "{421}", &["m"], Pad::None, "Again, or Escape, closes it."),
    row("map_in", "{421}: zoom in", &["=", "+"], Pad::None, ""),
    row("map_out", "{421}: zoom out", &["-", "_"], Pad::None, ""),
    row("group_invite", "Group: {86}", &["f5"], Pad::None, "Invites the target."),
    row("group_remove", "Group: {516}", &["f6"], Pad::None, "Takes the target out of the group."),
    row("group_disband", "Group: {529}", &["f7"], Pad::None, ""),
    row("group_leave", "{994}", &["f8"], Pad::None, ""),
    row("controls", "Controls", &["f1"], Pad::None, "This page, from play."),
];

/// A row's index by its name.
pub fn row_named(id: &str) -> Option<usize> {
    ROWS.iter().position(|r| r.id == id)
}

/// The row whose default keys hold `name`.
fn default_row(name: &str) -> Option<usize> {
    ROWS.iter().position(|r| r.keys.contains(&name))
}

/// The keys a capture refuses: the page's and the window's own (F3 the
/// report, F11 full screen, F12 a browser's tools), and the modifiers, which
/// `name_of` gives no name.
pub fn reserved(name: &str) -> bool {
    matches!(name, "f3" | "f11" | "f12")
}

/// The 16 buttons by their bit (`input::bit`): their names in `keys.opt` and
/// `EQOA_PAD` and their glyphs as the windows write them.
pub const BUTTONS: [(&str, &str); 16] = [
    ("select", "|A"),
    ("l3", "|9"),
    ("r3", "|("),
    ("start", "|a"),
    ("up", "|Z"),
    ("right", "|v"),
    ("down", "|z"),
    ("left", "|V"),
    ("l2", "|&"),
    ("r2", "|*"),
    ("l1", "|7"),
    ("r1", "|8"),
    ("triangle", "|t"),
    ("circle", "|c"),
    ("cross", "|x"),
    ("square", "|s"),
];

/// The key `SetKey` takes for a button (`input::bit`): 128 + its
/// `VIConsolePad` field, as `PadKeys` makes it.
pub fn pad_key(b: u16) -> Option<i32> {
    // the fields by bit index: select, l3, r3, start, up, right, down, left,
    // l2, r2, l1, r1, triangle, circle, cross, square
    const FIELD: [i32; 16] = [17, 18, 19, 16, 10, 9, 11, 8, 13, 15, 12, 14, 4, 7, 6, 5];
    bit_index(b).map(|i| 128 + FIELD[i as usize])
}

/// A button's bit index (0..16) from its bit.
pub fn bit_index(b: u16) -> Option<u8> {
    (b.count_ones() == 1).then(|| b.trailing_zeros() as u8)
}

/// The glyph a button's bit index draws as.
pub fn glyph(i: u8) -> &'static str {
    BUTTONS.get(i as usize).map_or("", |b| b.1)
}

/// What a key pressed is to the dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alias {
    /// itself
    Same,
    /// the default key of the row moved onto it
    To(&'static str),
    /// nothing: its row was moved off it
    Nothing,
    /// a row with no key of its own (△'s) moved onto it: its button
    /// (`input::bit`), pressed as the pad's
    Button(u16),
}

/// The player's bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binds {
    /// each row's key where the player moved it (by `ROWS`' index; None: its
    /// default)
    keys: Vec<Option<String>>,
    /// the physical button (a bit index) on each of the game's fields
    pad: [u8; 16],
}

impl Default for Binds {
    fn default() -> Self {
        Binds { keys: vec![None; ROWS.len()], pad: std::array::from_fn(|i| i as u8) }
    }
}

/// What moving a row or a field did besides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Moved {
    /// nothing: it was there already
    Already,
    /// moved; the key or button it left is free (`Some`: which)
    Freed(Option<String>),
    /// moved, and the row or field that had it took the one it left
    Swapped(usize),
}

impl Binds {
    pub fn is_default(&self) -> bool {
        *self == Binds::default()
    }

    /// The key a row is on now, if it has one.
    pub fn key(&self, row: usize) -> Option<&str> {
        match &self.keys[row] {
            Some(k) => Some(k.as_str()),
            None => ROWS[row].keys.first().copied(),
        }
    }

    /// Whether a row was moved off its default keys.
    pub fn moved(&self, row: usize) -> bool {
        self.keys[row].is_some()
    }

    /// The row a key is on now: moved there, or its first default key and
    /// not moved. (A shifted alternate, ~ beside `, is no row's to take: a
    /// row moved onto it leaves its row on the other.)
    pub fn row_on(&self, name: &str) -> Option<usize> {
        if let Some(r) = self.keys.iter().position(|k| k.as_deref() == Some(name)) {
            return Some(r);
        }
        ROWS.iter().position(|r| r.keys.first() == Some(&name)).filter(|&r| !self.moved(r))
    }

    /// A key as the dispatch is to see it: a key a row was moved onto is that
    /// row's default key, a row's own key it was moved off nothing, every
    /// other key itself.
    pub fn alias(&self, name: &str) -> Alias {
        if let Some(r) = self.keys.iter().position(|k| k.as_deref() == Some(name)) {
            return match (ROWS[r].keys.first(), ROWS[r].pad) {
                (Some(k), _) => Alias::To(k),
                (None, Pad::Button(b)) => Alias::Button(b),
                (None, _) => Alias::Nothing,
            };
        }
        match default_row(name) {
            Some(r) if self.moved(r) => Alias::Nothing,
            _ => Alias::Same,
        }
    }

    /// Moves `row` onto key `name`. A row on it already swaps with it,
    /// taking the key `row` leaves (a row that had none, △'s, leaves it no
    /// key); else that key is free.
    pub fn bind_key(&mut self, row: usize, name: &str) -> Moved {
        let old = self.key(row).map(str::to_string);
        if old.as_deref() == Some(name) || (ROWS[row].keys.is_empty() && !matches!(ROWS[row].pad, Pad::Button(_))) {
            return Moved::Already;
        }
        let other = self.row_on(name).filter(|&o| o != row);
        self.set_key(row, Some(name.to_string()));
        match other {
            Some(o) => {
                self.set_key(o, old);
                Moved::Swapped(o)
            }
            None => Moved::Freed(old),
        }
    }

    /// A row on `name` (None: no key at all, only for a row with none of its
    /// own), kept as its default when it is that.
    fn set_key(&mut self, row: usize, name: Option<String>) {
        let default = ROWS[row].keys.first().copied();
        self.keys[row] = match name {
            Some(n) if Some(n.as_str()) == default => None,
            None if default.is_none() => None,
            // a row with a key of its own left with none: no key on it at
            // all, which no default key is ("" is no key's name)
            None => Some(String::new()),
            n => n,
        };
    }

    /// The physical button (bit index) on field `field` (bit index).
    pub fn button(&self, field: u8) -> u8 {
        self.pad[field as usize]
    }

    /// The field physical button `phys` drives.
    pub fn field_of(&self, phys: u8) -> u8 {
        self.pad.iter().position(|&p| p == phys).unwrap_or(phys as usize) as u8
    }

    /// Puts physical button `phys` on field `field`; the field it drove
    /// takes the button `field` had (a swap).
    pub fn bind_button(&mut self, field: u8, phys: u8) -> Moved {
        let (field, phys) = (field as usize & 15, phys & 15);
        let had = self.pad[field];
        if had == phys {
            return Moved::Already;
        }
        let other = self.field_of(phys) as usize;
        self.pad[field] = phys;
        self.pad[other] = had;
        Moved::Swapped(other)
    }

    /// The controllers' buttons (`input::bit`, physical) as the game's
    /// fields.
    pub fn remap(&self, buttons: u16) -> u16 {
        let mut out = 0;
        for (field, &phys) in self.pad.iter().enumerate() {
            if buttons & (1 << phys) != 0 {
                out |= 1 << field;
            }
        }
        out
    }

    /// `keys.opt`: a line for each row moved (`key.<row>=<key>`, an empty
    /// key for none) and each field on another button
    /// (`pad.<field>=<button>`).
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for (r, k) in self.keys.iter().enumerate() {
            if let Some(k) = k {
                s.push_str(&format!("key.{}={k}\n", ROWS[r].id));
            }
        }
        for (f, &p) in self.pad.iter().enumerate() {
            if p as usize != f {
                s.push_str(&format!("pad.{}={}\n", BUTTONS[f].0, BUTTONS[p as usize].0));
            }
        }
        s
    }

    /// What `to_text` wrote, each line moved in turn from the defaults, so a
    /// file edited by hand still makes a layout with no key or button twice;
    /// a line not understood is left out.
    pub fn from_text(s: &str) -> Binds {
        let mut b = Binds::default();
        let button = |n: &str| BUTTONS.iter().position(|x| x.0 == n).map(|i| i as u8);
        for line in s.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            if let Some(id) = k.strip_prefix("key.") {
                let Some(r) = row_named(id) else { continue };
                if v.is_empty() {
                    b.clear_key(r);
                } else if valid_name(v) {
                    b.bind_key(r, v);
                }
            } else if let Some(f) = k.strip_prefix("pad.")
                && let (Some(f), Some(p)) = (button(f), button(v))
            {
                b.bind_button(f, p);
            }
        }
        b
    }

    /// Takes a row's key away (a hand-written `key.<row>=`): its key does
    /// nothing.
    fn clear_key(&mut self, row: usize) {
        if !ROWS[row].keys.is_empty() {
            self.keys[row] = Some(String::new());
        }
    }

    /// What was moved, or that nothing was (said when the keys are read, and
    /// in a report).
    pub fn describe(&self) -> String {
        if self.is_default() {
            return "the default layout".into();
        }
        let mut parts = Vec::new();
        for (r, k) in self.keys.iter().enumerate() {
            if let Some(k) = k {
                parts.push(format!("{} on {}", ROWS[r].id, if k.is_empty() { "nothing" } else { k }));
            }
        }
        for (f, &p) in self.pad.iter().enumerate() {
            if p as usize != f {
                parts.push(format!("pad {} on {}", BUTTONS[f].0, BUTTONS[p as usize].0));
            }
        }
        parts.join(", ")
    }
}

/// The controllers' buttons from frame to frame through the bindings: the
/// buttons they hold as the game's fields (`Binds::remap`), and while the
/// Controls page waits for a button, the one newly pressed, which the game
/// then does not see until it is let go (nor any other pressed with it).
#[derive(Clone, Copy, Debug, Default)]
pub struct PadEdge {
    /// last frame's buttons, as the controllers had them
    last: u16,
    /// the buttons a capture took, held still
    swallow: u16,
}

impl PadEdge {
    /// One frame: `raw` the controllers' buttons, `capturing` whether a
    /// button is awaited. The game's fields, and the button a capture takes
    /// (its bit index).
    pub fn frame(&mut self, raw: u16, capturing: bool, b: &Binds) -> (u16, Option<u8>) {
        let fresh = raw & !self.last;
        self.last = raw;
        let mut took = None;
        if capturing && fresh != 0 {
            took = Some(fresh.trailing_zeros() as u8);
            self.swallow |= fresh;
        }
        self.swallow &= raw;
        (b.remap(raw & !self.swallow), took)
    }
}

/// What a key pressed or let go is sent on as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    /// a key, by name, to the keyboard's dispatch
    Key(String),
    /// a pad button (`input::bit`), to the game's pad path (`SetKey`)
    Button(u16),
}

/// The keys from press to release through the bindings: what each key
/// pressed was made, so it is let go of as that, whatever changed between
/// (an edit box opened, a binding moved).
#[derive(Clone, Debug, Default)]
pub struct KeyEdge {
    pressed: std::collections::HashMap<String, Option<Sent>>,
}

impl KeyEdge {
    /// Key `name` pressed (`down`, `repeat` the system's) or let go: what it
    /// is sent on as, None for nothing. While `typing` an edit box has the
    /// keyboard and no key is remapped.
    pub fn key(&mut self, name: &str, down: bool, repeat: bool, typing: bool, b: &Binds) -> Option<Sent> {
        let same = |name: &str| Some(Sent::Key(name.to_string()));
        let now = |name: &str| match b.alias(name) {
            Alias::Same => same(name),
            Alias::To(k) => same(k),
            Alias::Button(k) => Some(Sent::Button(k)),
            Alias::Nothing => None,
        };
        if !down {
            return self.pressed.remove(name).unwrap_or_else(|| if typing { same(name) } else { now(name) });
        }
        if typing {
            if !repeat {
                self.pressed.insert(name.to_string(), same(name));
            }
            return same(name);
        }
        if repeat && let Some(was) = self.pressed.get(name) {
            return was.clone();
        }
        let out = now(name);
        self.pressed.insert(name.to_string(), out.clone());
        out
    }

    /// A key a capture took: its release is nothing.
    pub fn hold_back(&mut self, name: &str) {
        self.pressed.insert(name.to_string(), None);
    }

    /// Every key let go (the window lost the keys).
    pub fn clear(&mut self) {
        self.pressed.clear();
    }
}

/// Whether a name read back can be a key's (`name_of`'s, or a character).
fn valid_name(v: &str) -> bool {
    !v.is_empty() && v.chars().count() <= 12 && !v.chars().any(char::is_control)
}

/// A key's name, as the client's dispatch and `keys.opt` use it: the cursor
/// block and the named keys by name, a character as itself in lower case.
/// None for the modifiers and the keys the client never binds.
pub fn name_of(k: &Key) -> Option<String> {
    Some(match k {
        Key::Character(c) => c.to_lowercase(),
        Key::Named(n) => match n {
            NamedKey::Space => "space",
            NamedKey::Enter => "enter",
            NamedKey::Escape => "escape",
            NamedKey::Backspace => "backspace",
            NamedKey::Tab => "tab",
            NamedKey::ArrowUp => "up",
            NamedKey::ArrowDown => "down",
            NamedKey::ArrowLeft => "left",
            NamedKey::ArrowRight => "right",
            NamedKey::PageUp => "pageup",
            NamedKey::PageDown => "pagedown",
            NamedKey::Home => "home",
            NamedKey::End => "end",
            NamedKey::Insert => "insert",
            NamedKey::Delete => "delete",
            NamedKey::F1 => "f1",
            NamedKey::F2 => "f2",
            NamedKey::F3 => "f3",
            NamedKey::F4 => "f4",
            NamedKey::F5 => "f5",
            NamedKey::F6 => "f6",
            NamedKey::F7 => "f7",
            NamedKey::F8 => "f8",
            NamedKey::F9 => "f9",
            NamedKey::F10 => "f10",
            NamedKey::F11 => "f11",
            NamedKey::F12 => "f12",
            _ => return None,
        }
        .to_string(),
        _ => return None,
    })
}

/// The key a name stands for (`name_of`'s inverse), as the dispatch matches
/// it.
pub fn key_of(name: &str) -> Key {
    let named = match name {
        "space" => NamedKey::Space,
        "enter" => NamedKey::Enter,
        "escape" => NamedKey::Escape,
        "backspace" => NamedKey::Backspace,
        "tab" => NamedKey::Tab,
        "up" => NamedKey::ArrowUp,
        "down" => NamedKey::ArrowDown,
        "left" => NamedKey::ArrowLeft,
        "right" => NamedKey::ArrowRight,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "insert" => NamedKey::Insert,
        "delete" => NamedKey::Delete,
        "f1" => NamedKey::F1,
        "f2" => NamedKey::F2,
        "f3" => NamedKey::F3,
        "f4" => NamedKey::F4,
        "f5" => NamedKey::F5,
        "f6" => NamedKey::F6,
        "f7" => NamedKey::F7,
        "f8" => NamedKey::F8,
        "f9" => NamedKey::F9,
        "f10" => NamedKey::F10,
        "f11" => NamedKey::F11,
        "f12" => NamedKey::F12,
        c => return Key::Character(c.into()),
    };
    Key::Named(named)
}

/// A key's name as the page shows it.
pub fn display(name: &str) -> String {
    match name {
        "" => "-".into(),
        "space" => "Space".into(),
        "enter" => "Enter".into(),
        "escape" => "Esc".into(),
        "backspace" => "Backspace".into(),
        "tab" => "Tab".into(),
        "up" => "\u{2191}".into(),
        "down" => "\u{2193}".into(),
        "left" => "\u{2190}".into(),
        "right" => "\u{2192}".into(),
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "insert" => "Insert".into(),
        "delete" => "Delete".into(),
        // the windows' text marks a glyph with |, which is written twice
        "|" => "||".into(),
        n => n.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default table is today's layout: every key the client's keyboard
    /// dispatch matches outside an edit box (read out of `viewer.rs` itself,
    /// so a key added there without a row here fails) is a row's, and the
    /// map's keys (`map::keyboard`) are too; each row's keys belong to it
    /// alone. The pad's rows are the game's pairs (`OnKeyDown`'s case lists).
    #[test]
    fn the_default_table_is_todays_layout() {
        let src = include_str!("viewer.rs");
        let mut names = Vec::new();
        for (pat, end) in [("eq_ignore_ascii_case(\"", '"'), ("c.as_str() == \"", '"'), ("NamedKey::", ')')] {
            for (i, _) in src.match_indices(pat) {
                let rest = &src[i + pat.len()..];
                let n: String = rest.chars().take_while(|&c| c != end && (c.is_alphanumeric() || pat != "NamedKey::")).collect();
                names.push(if pat == "NamedKey::" { name_of(&Key::Named(named(&n))).unwrap_or(n.to_lowercase()) } else { n.to_lowercase() });
            }
        }
        // 1 to 6 (`matches!(c.as_str(), "1" | ...)`) and the held keys the
        // walk and the camera read (`controls`, `game_keys`)
        names.extend(["1", "2", "3", "4", "5", "6", "w", "s", "a", "d", "i", "j", "k", "l", "pageup", "pagedown", "home", "end"].map(String::from));
        // read only while typing (the soft keyboard's key, an editor's
        // done), never bound (full screen, the fly camera's speed)
        let not_rows = ["insert", "f11", "shift"];
        names.retain(|n| !not_rows.contains(&n.as_str()));
        assert!(names.len() > 60, "the scan found the dispatch's keys: {names:?}");
        for n in &names {
            assert!(default_row(n).is_some(), "{n:?} is in the dispatch and in no row");
        }
        for n in ["escape", "backspace", "m", "space", "enter", "home", "+", "=", "pageup", "-", "_", "pagedown", "up", "w", "down", "s", "left", "a", "right", "d"] {
            assert!(crate::map::keyboard(n).is_some(), "{n} is the map's");
            assert!(default_row(n).is_some(), "the map's {n:?} is a row's");
        }
        // no key in two rows, no id twice
        let mut seen = std::collections::HashSet::new();
        for r in ROWS {
            assert!(seen.insert(r.id), "{} twice", r.id);
            for k in r.keys {
                assert_eq!(ROWS.iter().filter(|o| o.keys.contains(k)).count(), 1, "{k}");
            }
        }
        // the game's own pairs, `VIWndGame::OnKeyDown`'s cases: 27/144,
        // 80,112/145, 32/134, 81,113/133, 96,126/140, 9/142, 90,122/141,
        // 67,99/143, 13/147, 88,120 with 146 (both ToggleP74)
        let pair = |k: &str| ROWS.iter().find(|r| r.keys.contains(&k)).map(|r| r.pad);
        for (k, p) in [("escape", bit::START), ("p", bit::SELECT), ("space", bit::CROSS), ("q", bit::SQUARE), ("`", bit::L1), ("~", bit::L1), ("tab", bit::R1), ("z", bit::L2), ("c", bit::R2), ("enter", bit::R3), ("x", bit::L3)] {
            assert_eq!(pair(k), Some(Pad::Button(p)), "{k}");
        }
        // every button the game reads has its row
        for i in 0..16u16 {
            assert_eq!(ROWS.iter().filter(|r| r.pad == Pad::Button(1 << i)).count(), 1, "button {i}");
        }
    }

    fn named(n: &str) -> NamedKey {
        match n {
            "Escape" => NamedKey::Escape,
            "Backspace" => NamedKey::Backspace,
            "Enter" => NamedKey::Enter,
            "Space" => NamedKey::Space,
            "Tab" => NamedKey::Tab,
            "ArrowUp" => NamedKey::ArrowUp,
            "ArrowDown" => NamedKey::ArrowDown,
            "ArrowLeft" => NamedKey::ArrowLeft,
            "ArrowRight" => NamedKey::ArrowRight,
            "PageUp" => NamedKey::PageUp,
            "PageDown" => NamedKey::PageDown,
            "Home" => NamedKey::Home,
            "End" => NamedKey::End,
            "Insert" => NamedKey::Insert,
            "Shift" => NamedKey::Shift,
            "F1" => NamedKey::F1,
            "F5" => NamedKey::F5,
            "F6" => NamedKey::F6,
            "F7" => NamedKey::F7,
            "F8" => NamedKey::F8,
            "F11" => NamedKey::F11,
            other => panic!("the dispatch matches NamedKey::{other}: give it a name here and a row"),
        }
    }

    /// With nothing moved every key is itself and every button its own
    /// field; the control: one row moved changes exactly its two keys.
    #[test]
    fn nothing_moved_is_the_identity() {
        let b = Binds::default();
        assert!(b.is_default());
        let mut all: Vec<&str> = ROWS.iter().flat_map(|r| r.keys.iter().copied()).collect();
        all.extend(["7", "y", "f2", "delete", "é", "space", "shift"]);
        for k in &all {
            assert_eq!(b.alias(k), Alias::Same, "{k}");
        }
        for m in 0..=u16::MAX {
            assert_eq!(b.remap(m), m);
        }
        let mut moved = Binds::default();
        moved.bind_key(row_named("use").unwrap(), "7");
        let changed: Vec<&&str> = all.iter().filter(|k| moved.alias(k) != Alias::Same).collect();
        assert_eq!(changed, vec![&"e", &"7"]);
        assert_eq!((moved.alias("7"), moved.alias("e")), (Alias::To("e"), Alias::Nothing));
    }

    /// Moving a row: to a free key it goes there and its key is free; to
    /// another row's it swaps; onto itself nothing. A shifted alternate goes
    /// with its row.
    #[test]
    fn a_row_moves_or_swaps() {
        let mut b = Binds::default();
        let (talk, attack, cycle, use_) = (row_named("talk").unwrap(), row_named("attack").unwrap(), row_named("cycle_target").unwrap(), row_named("use").unwrap());
        // jib's: Confirm (E, ✕ in a window) on Q, Talk takes E
        assert_eq!(b.bind_key(use_, "q"), Moved::Swapped(talk));
        assert_eq!((b.key(use_), b.key(talk)), (Some("q"), Some("e")));
        assert_eq!((b.alias("q"), b.alias("e")), (Alias::To("e"), Alias::To("q")));
        // onto a free key: Q freed, does nothing
        assert_eq!(b.bind_key(use_, "7"), Moved::Freed(Some("q".into())));
        assert_eq!(b.alias("q"), Alias::Nothing, "Talk's own key, Talk moved off it");
        assert_eq!(b.alias("7"), Alias::To("e"));
        assert_eq!(b.bind_key(use_, "7"), Moved::Already);
        // back on its own key: kept as its default
        b.bind_key(use_, "e");
        b.bind_key(talk, "q");
        assert!(b.is_default(), "{b:?}");
        // the L1 row's ` and ~ go together
        b.bind_key(cycle, "f2");
        assert_eq!((b.alias("`"), b.alias("~"), b.alias("f2")), (Alias::Nothing, Alias::Nothing, Alias::To("`")));
        // a row moved onto another's alternate takes it; the other keeps its
        // first key (no swap: it is not on ~)
        let mut c = Binds::default();
        let map = row_named("map").unwrap();
        assert_eq!(c.bind_key(map, "~"), Moved::Freed(Some("m".into())));
        assert_eq!((c.alias("~"), c.alias("`")), (Alias::To("m"), Alias::Same));
        // a three-way: attack onto Q (talk takes Space), then talk onto E
        // (use takes Space)
        let mut d = Binds::default();
        d.bind_key(attack, "q");
        d.bind_key(talk, "e");
        assert_eq!((d.key(attack), d.key(talk), d.key(use_)), (Some("q"), Some("e"), Some("space")));
        assert_eq!(d.alias("space"), Alias::To("e"));
        // △'s row, with no key of its own: onto a free key it presses △;
        // onto another row's key, that row is left with none
        let mut t = Binds::default();
        let clear = row_named("clear_target").unwrap();
        assert_eq!(t.key(clear), None);
        assert_eq!(t.bind_key(clear, "q"), Moved::Swapped(talk));
        assert_eq!((t.key(clear), t.key(talk)), (Some("q"), Some("")), "Talk left with no key");
        assert_eq!(t.alias("q"), Alias::Button(bit::TRIANGLE));
        assert_eq!(t.bind_key(clear, "b"), Moved::Freed(Some("q".into())));
        assert_eq!((t.alias("b"), t.alias("q")), (Alias::Button(bit::TRIANGLE), Alias::Nothing));
        // from a key, onto another row's: that row takes the key it left
        assert_eq!(t.bind_key(clear, "e"), Moved::Swapped(use_));
        assert_eq!(t.key(use_), Some("b"));
        // a stick's row has keys; no row without keys or a button exists to refuse
        assert!(ROWS.iter().all(|r| !r.keys.is_empty() || matches!(r.pad, Pad::Button(_))));
    }

    /// The pad: a field moved onto another button swaps the two; the
    /// remap's bits follow; the control is the identity's.
    #[test]
    fn buttons_swap() {
        let mut b = Binds::default();
        let (cross, circle) = (bit_index(bit::CROSS).unwrap(), bit_index(bit::CIRCLE).unwrap());
        assert_eq!(b.bind_button(cross, circle), Moved::Swapped(circle as usize));
        assert_eq!((b.button(cross), b.button(circle)), (circle, cross));
        assert_eq!(b.remap(bit::CIRCLE), bit::CROSS, "the physical ○ is the game's ✕");
        assert_eq!(b.remap(bit::CROSS), bit::CIRCLE);
        assert_eq!(b.remap(bit::CROSS | bit::R1), bit::CIRCLE | bit::R1);
        assert_eq!(Binds::default().remap(bit::CIRCLE), bit::CIRCLE, "the control");
        assert_eq!(b.bind_button(cross, circle), Moved::Already);
        // a three-way keeps it a permutation
        let tri = bit_index(bit::TRIANGLE).unwrap();
        b.bind_button(tri, cross);
        let mut seen: Vec<u8> = (0..16).map(|f| b.button(f)).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..16).collect::<Vec<u8>>());
        assert_eq!(b.remap(u16::MAX), u16::MAX);
    }

    /// The controllers' edge: the fields remapped every frame; a press
    /// while a button is awaited is the capture's and the game sees nothing
    /// of it until it is let go and pressed again; a button held from before
    /// the capture is not taken. The control: not capturing, the same press
    /// is the game's at once.
    #[test]
    fn a_captured_press_is_kept_from_the_game() {
        let mut b = Binds::default();
        b.bind_button(bit_index(bit::CROSS).unwrap(), bit_index(bit::CIRCLE).unwrap());
        let mut e = PadEdge::default();
        assert_eq!(e.frame(bit::CIRCLE, false, &b), (bit::CROSS, None), "remapped");
        assert_eq!(e.frame(0, false, &b), (0, None));
        // held from before: not taken
        e.frame(bit::R1, false, &b);
        assert_eq!(e.frame(bit::R1, true, &b), (bit::R1, None));
        // pressed while awaited: the capture's
        assert_eq!(e.frame(bit::R1 | bit::SQUARE, true, &b), (bit::R1, Some(15)));
        assert_eq!(e.frame(bit::R1 | bit::SQUARE, false, &b), (bit::R1, None), "held: still not the game's");
        assert_eq!(e.frame(bit::R1, false, &b), (bit::R1, None));
        assert_eq!(e.frame(bit::R1 | bit::SQUARE, false, &b), (bit::R1 | bit::SQUARE, None), "pressed again: the game's");
        // the control
        let mut f = PadEdge::default();
        assert_eq!(f.frame(bit::SQUARE, false, &b), (bit::SQUARE, None));
    }

    /// The keyboard's edge: a moved key is its row's default key at the
    /// press and at the release, even when an edit box opened between (the
    /// release is not then typed as itself); typing is never remapped; a key
    /// a capture took lets go as nothing. The control: with nothing moved,
    /// every key passes as itself.
    #[test]
    fn a_key_lets_go_as_what_it_was_pressed_as() {
        let mut b = Binds::default();
        b.bind_key(row_named("walk_forward").unwrap(), "up");
        let key = |s: &str| Some(Sent::Key(s.to_string()));
        let mut e = KeyEdge::default();
        assert_eq!(e.key("up", true, false, false, &b), key("w"), "the arrow walks");
        assert_eq!(e.key("up", true, true, false, &b), key("w"), "a repeat");
        assert_eq!(e.key("up", false, false, true, &b), key("w"), "let go while typing: still w, so the walk stops");
        assert_eq!(e.key("w", true, false, false, &b), key("up"), "W took the arrow's place: the quick menu's up");
        assert_eq!(e.key("w", false, false, false, &b), key("up"));
        b.bind_key(row_named("menu_up").unwrap(), "f2");
        assert_eq!(e.key("w", true, false, false, &b), None, "W's rows both moved off it: nothing");
        assert_eq!(e.key("w", false, false, false, &b), None);
        // typing: as it is
        assert_eq!(e.key("up", true, false, true, &b), key("up"));
        assert_eq!(e.key("up", false, false, false, &b), key("up"), "let go as pressed");
        assert_eq!(e.key("w", true, false, true, &b), key("w"));
        // a capture's key
        e.hold_back("f2");
        assert_eq!(e.key("f2", false, false, false, &b), None);
        // △'s row, which has no key in the game, given B: B presses △
        b.bind_key(row_named("clear_target").unwrap(), "b");
        assert_eq!(e.key("b", true, false, false, &b), Some(Sent::Button(bit::TRIANGLE)));
        assert_eq!(e.key("b", false, false, true, &b), Some(Sent::Button(bit::TRIANGLE)), "let go as pressed");
        assert_eq!(e.key("b", true, false, true, &b), key("b"), "typed as itself");
        // the control
        let mut c = KeyEdge::default();
        for k in ["up", "w", "e", "space", "7", "b"] {
            assert_eq!(c.key(k, true, false, false, &Binds::default()), key(k));
            assert_eq!(c.key(k, false, false, false, &Binds::default()), key(k));
        }
    }

    /// keys.opt: what was moved, read back the same; nothing moved is
    /// empty; a line not understood is left out and a file edited to put two
    /// rows on one key still makes one key one row's.
    #[test]
    fn keys_opt_round_trips_and_forgives() {
        let mut b = Binds::default();
        assert_eq!(b.to_text(), "");
        b.bind_key(row_named("use").unwrap(), "q");
        b.bind_key(row_named("map").unwrap(), "=");
        b.bind_button(bit_index(bit::CROSS).unwrap(), bit_index(bit::CIRCLE).unwrap());
        let t = b.to_text();
        assert_eq!(t, "key.talk=e\nkey.use=q\nkey.map==\nkey.map_in=m\npad.circle=cross\npad.cross=circle\n");
        assert_eq!(Binds::from_text(&t), b);
        let odd = Binds::from_text("key.nope=7\nkey.use=\u{7}\npad.cross=nope\nnoise\nkey.attack=q\nkey.use=q\n");
        assert_eq!(odd.key(row_named("use").unwrap()), Some("q"));
        assert_eq!(odd.key(row_named("attack").unwrap()), Some("e"), "the later line swapped it");
        let owners = ROWS.iter().enumerate().filter_map(|(r, _)| odd.key(r)).filter(|k| *k == "q").count();
        assert_eq!(owners, 1);
        // a row cleared by hand: its key does nothing, no row on it
        let cleared = Binds::from_text("key.talk=\n");
        assert_eq!(cleared.alias("q"), Alias::Nothing);
        assert_eq!(cleared.key(row_named("talk").unwrap()), Some(""));
        assert_eq!(Binds::from_text(&cleared.to_text()), cleared);
    }

    /// `pad_key` is what the game's pad path makes of each button: the key
    /// `VIPad::Read` and `PadKeys` give for it alone.
    #[test]
    fn a_buttons_key_is_the_pads() {
        for (i, (name, _)) in BUTTONS.iter().enumerate() {
            let b = 1u16 << i;
            let mut c = crate::input::Console::new();
            let r = crate::input::Report { buttons: b, sticks: [0.0; 4] };
            c.poll(r, true, false, false);
            assert_eq!(c.poll(r, true, false, false), vec![(pad_key(b).unwrap(), true)], "{name}");
        }
        assert_eq!(pad_key(bit::TRIANGLE), Some(132));
        assert_eq!(pad_key(0), None);
    }

    #[test]
    fn names_round_trip() {
        for n in ["space", "enter", "escape", "backspace", "tab", "up", "down", "left", "right", "pageup", "pagedown", "home", "end", "f1", "f8", "f12", "q", "`", "/", "1"] {
            assert_eq!(name_of(&key_of(n)).as_deref(), Some(n));
        }
        assert_eq!(name_of(&Key::Character("Q".into())).as_deref(), Some("q"));
        assert_eq!(name_of(&Key::Named(NamedKey::Shift)), None);
        assert_eq!((display("pageup"), display("q"), display("f5"), display("|")), ("Page Up".into(), "Q".into(), "F5".into(), "||".into()));
    }
}
