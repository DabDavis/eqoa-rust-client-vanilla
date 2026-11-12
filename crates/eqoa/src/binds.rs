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
//! The controller: the physical button on each of the game's `VIConsolePad`
//! fields (`remap`, before `VIPad::Read`): a button moved onto a field another
//! button drove swaps the two. A row the game has no button for (the client's
//! keys: the map, F9's art, targets 1-6 and the rest) can be given one too
//! (F3 20261024-145732 and -164553: the map on R3), which presses that row's
//! key as the keyboard's would (`PadEdge`, `App::pad_rows`); the field it
//! drove is left with none, unless the row had a button, which that field
//! then takes. Every button is in one place, a field's or a row's. The sticks
//! stay sticks.
//!
//! Space was the attack's (the game's own pair, 32 with ✕) until 2026-11-09,
//! when the jump took it (the user: "make jump ... spacebar by default"): the
//! attack's row is on G, free in the game's keyboard table and the client's
//! dispatch alike, and V, the jump's key before, stays the jump's beside Space
//! (nothing else claims it). Space, while the jump's row has it, is still the
//! attack's wherever it makes no jump (`KeyEdge::space_attacks`): on a server
//! that never offered one (Sandstorm's, eqoa.live's, eqoaclassic's Live
//! shard) and with a window up (the corpse's ✕, a trade's chat line, the
//! fishing cast), so a player there finds Space as it was. A layout kept
//! before (`keys.opt`) keeps every row it moved, each line read onto today's
//! defaults (a row it put on G takes G, the attack the key that row left), and
//! a macro it put on G keeps G (`Binds::from_text`).
//!
//! With nothing moved every key is itself and every button its own field
//! (`Binds::default`, the tests). Kept per device in `keys.opt` beside the
//! client's options (`settings`), as the PS2 keeps every option on its card.
//!
//! Macros (the user, 2026-11-06: "pcsx2 has a nice macro system for
//! keys/buttons. lets make a similar native solution"; 2026-11-10: "we are
//! using pcsx2 macro system, but it is not the same yet": PCSX2's Macro
//! Buttons, `pcsx2/SIO/Pad/Pad.cpp`;
//! `claude-notes/vanilla-macros-pcsx2-20261110.md`): `MACROS` of them, each
//! as PCSX2 keeps one (`Macro`):
//! its triggers (any one of them runs it, each a chord of keys, controller
//! buttons and sticks' ways, every one of them held), the game's buttons
//! and sticks' ways it presses together, Press To Toggle, a Frequency, a
//! Pressure and a Deadzone; set on the Options window's Macros page
//! (`macropage`). Every input stays in one place: a key in a trigger is no
//! row's (a row's key is refused; a row moved onto it takes it back, and
//! with it every trigger it was in); a button in a trigger leaves the field
//! or row it was on (a field or row given it back takes it, and every
//! trigger it was in goes); a stick's way in a trigger is no longer the
//! game's (`mask_ways`). Several macros may share an input, as chords with
//! L1 in them do. The game sees only the buttons pressed (`macros::Run`,
//! before `VIPad::Read`): nothing new goes to a server.

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
    /// the keys today's dispatch takes for it, the first shown (the rest go
    /// with it: the same key shifted, ~ with `, and Jump's V, its key before
    /// it had Space)
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
    row("attack", "{1018} (hold)", &["g"], b(bit::CROSS), "With Auto Attack on, a toggle; on a corpse, its loot. In a window |x is {212}; G is |x too in the corpse's window, and |( in the others (a trade's {424}). Space does all of it too wherever it makes no jump (Jump's row)."),
    row("talk", "Talk (hold: {1403})", &["q"], b(bit::SQUARE), "In a window it is |s, the window's own: a trade's {478}, the who list asked again, the mail's inbox."),
    row("clear_target", "Let the target go", &[], b(bit::TRIANGLE), "The game has no key for it: a key put here presses |t. In a window |t is {217}, Escape or Backspace on the keyboard."),
    row("use", "Quick menu: {120}", &["e"], b(bit::CIRCLE), "In a window E is |x {212}, as Enter is."),
    row("menu_up", "Quick menu: up", &["up"], b(bit::UP), "In a window, up a row. On the map: pan up."),
    row("menu_down", "Quick menu: down", &["down"], b(bit::DOWN), "In a window, down a row. On the map: pan down."),
    row("menu_prev", "Quick menu: {1692}", &["left"], b(bit::LEFT), "In a window, left. On the map: pan left."),
    row("menu_next", "Quick menu: {216}", &["right"], b(bit::RIGHT), "In a window, right. On the map: pan right."),
    row("cycle_target", "Target: {1405}, group, {418}", &["`", "~"], b(bit::L1), "Each press the next of the player, the group and the pet; held, aim as with Tab. On the map |7 zooms out."),
    row("quick_chat", "Quick chat", &["z"], b(bit::L2), "Again, it closes the quick chat."),
    row("next_target", "Target: the next (hold: aim)", &["tab"], b(bit::R1), "Held: the view at the eyes, the target the one nearest the crosshair. In a window Tab is |8; on the map |8 zooms in."),
    row("history", "{424} {1694} (hold)", &["c"], b(bit::R2), "Up and down scroll it while held. In a window C is |c (the factions' next page)."),
    row("autorun", "Autorun", &["x"], b(bit::L3), "A stick pushed past the run's edge ends it."),
    row("jump", "Jump", &["space", "v"], Pad::None, "In play, on a server that has jumps (its own, not the game's); V too. Wherever it makes no jump (any other server, a window up) Space is {1018}'s key, as it was before it jumped. A controller's button can take it here: every button of the pad is the game's, so none has it till given. On the map Space is |x."),
    row("chat", "{424}", &["enter"], b(bit::R3), "In a window Enter is |x {212}; in a trade, its chat line (/, T and R too). Over the quick chat, the chat line. On the map: |x."),
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
    row("target_target", "Assist Target", &["f"], Pad::None, "The game's Assist Target: whom the target is targeting, as the server answers."),
    row("orbit_left", "Orbit left", &["home"], Pad::None, "The orbiting camera's. On the map: |x."),
    row("orbit_right", "Orbit right", &["end"], Pad::None, "The orbiting camera's."),
    row("tilt_up", "Tilt up, zoom in", &["pageup"], Pad::None, "On the map: zoom in."),
    row("tilt_down", "Tilt down, zoom out", &["pagedown"], Pad::None, "On the map: zoom out."),
    row("map", "{421}", &["m"], Pad::None, "Again, or Escape, closes it. With Extended's Map off it does nothing."),
    row("map_in", "{421}: zoom in", &["=", "+"], Pad::None, ""),
    row("map_out", "{421}: zoom out", &["-", "_"], Pad::None, ""),
    row("map_group_waypoint", "{421}: group waypoint", &["o"], Pad::None, "On the map, in a group: a waypoint for every member, said in the group's chat, where |s puts yours, else where you stand. Also |s held, or Shift+click."),
    row("group_invite", "Group: {86}", &["f5"], Pad::None, "Invites the target."),
    row("group_remove", "Group: {516}", &["f6"], Pad::None, "Takes the target out of the group."),
    row("group_disband", "Group: {529}", &["f7"], Pad::None, ""),
    row("group_leave", "{994}", &["f8"], Pad::None, ""),
    row("art", "Art: Frontiers' or 2002's", &["f9"], Pad::None, "Everything drawn swaps between Frontiers' art and the 2002 disc's (EverQuest Online Adventures, USA, SLUS-20470), at once; the first time, it asks for that disc."),
    row("hide_ui", "Hide UI", &["h"], Pad::None, "Hides everything over the world: the HUD, the chat, the names over heads, the damage numbers and the target's rings. Again, or any window coming up, shows them."),
    row("buffs", "Buff window", &["u"], Pad::None, "On a server with the buff window: the arrows walk your buffs, showing each one's name and time left, Enter takes the lit one off, U, Escape or Backspace let go. A controller: hold |* and press |&."),
    row("minimap", "Minimap", &["n"], Pad::None, "Shows or hides the minimap, as Extended's Map page does, and keeps it. On the map: a community marker suggested."),
    row("controls", "Controls", &["f1"], Pad::None, "This page, from play."),
];

/// A row's index by its name.
pub fn row_named(id: &str) -> Option<usize> {
    ROWS.iter().position(|r| r.id == id)
}

/// The attack's key as the dispatch takes it (its row's default, G):
/// Space's, where Space makes no jump (`KeyEdge::space_attacks`).
pub fn attack_key() -> &'static str {
    ROWS[row_named("attack").expect("the attack's row")].keys[0]
}

/// Whether Space is the jump's own key (the jump's row on it as by default,
/// no row or macro moved onto it): then Space is the attack's too wherever it
/// makes no jump (`KeyEdge::space_attacks`).
pub fn space_is_jumps(b: &Binds) -> bool {
    b.alias("space") == Alias::Same
}

/// The row whose default keys hold `name`.
fn default_row(name: &str) -> Option<usize> {
    ROWS.iter().position(|r| r.keys.contains(&name))
}

/// The keys a capture refuses: the page's and the window's own (F3 a bug
/// report, on a server that takes them; F11 full screen, F12 a browser's
/// tools), and the modifiers, which
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
    /// in a macro's trigger (one or more: `macros::Run` reads it by name)
    Macro,
}

/// A field whose button went to a row of the client's keys (`Binds::button`).
pub const NO_BUTTON: u8 = 0xFF;

/// How many macros there are: PCSX2's 16 a controller (`PadTypes.h`'s
/// `NUM_MACRO_BUTTONS_PER_CONTROLLER`; the user, 2026-11-06: "lets make it
/// 16 also"), each a row of the Macros page's list.
pub const MACROS: usize = 16;

/// The triggers a macro keeps, each on a row of the Macros page (PCSX2's
/// Shift+click adds another, with no limit): any one of them runs it.
pub const TRIGGERS_MOST: usize = 4;

/// The inputs one trigger's chord takes at most: PCSX2's
/// `MAX_KEYS_PER_BINDING`.
pub const CHORD_MOST: usize = 4;

/// Frequency's top on the Macros page: PCSX2's Big Picture's slider (0 to
/// 60); a `keys.opt` written by hand may hold up to 65535, as PCSX2's own
/// loader takes it (`LoadMacroButtonConfig` keeps a u16), and the page
/// steps down from there.
pub const FREQUENCY_PAGE_MOST: u16 = 60;

/// Pressure's least and its default, in percent: PCSX2's editor's slider,
/// 1 to 100, 100 unless set (`Macro{}Pressure`, 1.0).
pub const PRESSURE_LEAST: u8 = 1;
pub const PRESSURE_DEFAULT: u8 = 100;

/// Deadzone's default, in percent of 0 to 100 (`Macro{}Deadzone`, 0.0).
pub const DEADZONE_DEFAULT: u8 = 0;

/// The sticks' ways, by index, as PCSX2's DualShock 2 has them (its half
/// axes `LUp`, `LRight`, `LDown`, `LLeft`, `RUp`, `RRight`, `RDown`,
/// `RLeft`): their names in `keys.opt` and as the page writes them.
pub const WAYS: [(&str, &str); 8] = [
    ("lup", "L\u{2191}"),
    ("lright", "L\u{2192}"),
    ("ldown", "L\u{2193}"),
    ("lleft", "L\u{2190}"),
    ("rup", "R\u{2191}"),
    ("rright", "R\u{2192}"),
    ("rdown", "R\u{2193}"),
    ("rleft", "R\u{2190}"),
];

/// Each way's axis in a report's sticks (rx, ry, lx, ly, +y down) and
/// whether it is the axis's lesser half.
const WAY_AXES: [(usize, bool); 8] = [(3, true), (2, false), (3, false), (2, true), (1, true), (0, false), (1, false), (0, true)];

/// The sticks (rx, ry, lx, ly, +y down) as their eight ways, each 0..1
/// (PCSX2's half axes).
pub fn ways(sticks: [f32; 4]) -> [f32; 8] {
    std::array::from_fn(|i| {
        let (axis, negative) = WAY_AXES[i];
        let v = if negative { -sticks[axis] } else { sticks[axis] };
        if v > 0.0 { v } else { 0.0 }
    })
}

/// The sticks out of their ways (`ways`' inverse), as PCSX2's DualShock 2
/// merges its half axes (`PadDualshock2::Set`'s `MERGE`): the greater half
/// when it is pushed at all, else the lesser.
pub fn axes(w: [f32; 8]) -> [f32; 4] {
    let merge = |more: f32, less: f32| if more != 0.0 { more } else if less != 0.0 { -less } else { 0.0 };
    [merge(w[5], w[7]), merge(w[6], w[4]), merge(w[1], w[3]), merge(w[2], w[0])]
}

/// One input of a trigger's chord: anything PCSX2 binds a trigger to that
/// the client reads (not the mouse, which is the game's).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    /// a key, by `name_of`'s name
    Key(String),
    /// a controller's button, as the controller has it (a bit index,
    /// before any binding); L2 and R2 by their pull where the controller
    /// says it (`macros::Held::triggers`)
    Button(u8),
    /// a stick pushed one way (`WAYS`' index)
    Stick(u8),
}

impl Input {
    /// As the page writes it: a key's name, a button's glyph, a way.
    pub fn text(&self) -> String {
        match self {
            Input::Key(k) => display(k),
            Input::Button(b) => glyph(*b).to_string(),
            Input::Stick(w) => WAYS.get(*w as usize).map_or("?", |w| w.1).to_string(),
        }
    }

    /// As `keys.opt` keeps it: `key:<name>`, `pad:<button>`, `stick:<way>`.
    fn opt(&self) -> String {
        match self {
            Input::Key(k) => format!("key:{k}"),
            Input::Button(b) => format!("pad:{}", BUTTONS[*b as usize & 15].0),
            Input::Stick(w) => format!("stick:{}", WAYS[*w as usize & 7].0),
        }
    }

    /// `opt`'s inverse; a name not understood is none.
    fn from_opt(s: &str) -> Option<Input> {
        if let Some(k) = s.strip_prefix("key:") {
            return valid_name(k).then(|| Input::Key(k.to_string()));
        }
        if let Some(b) = s.strip_prefix("pad:") {
            return BUTTONS.iter().position(|x| x.0 == b).map(|i| Input::Button(i as u8));
        }
        let w = s.strip_prefix("stick:")?;
        WAYS.iter().position(|x| x.0 == w).map(|i| Input::Stick(i as u8))
    }
}

/// A trigger as the page writes it: its inputs joined by " + " (PCSX2's
/// `&`), "-" for none.
pub fn chord_text(chord: &[Input]) -> String {
    if chord.is_empty() {
        return "-".into();
    }
    chord.iter().map(Input::text).collect::<Vec<_>>().join(" + ")
}

/// A macro (`Binds::macros`): PCSX2's `MacroN` and its `MacroN*` keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Macro {
    /// what runs it (`MacroN`, a list of bindings): any one of these
    /// chords, each wholly held (every one of its inputs)
    pub triggers: Vec<Vec<Input>>,
    /// what it presses, all at once (`MacroNBinds`): the game's buttons
    /// (`input::bit`'s, as fields) and the sticks' ways (`WAYS`' bits)
    pub buttons: u16,
    pub ways: u8,
    /// Press To Toggle (`MacroNToggle`): from one press of a trigger to
    /// the next, rather than while one is held
    pub toggle: bool,
    /// `MacroNFrequency`: while it runs, what it presses is let go and
    /// pressed again every this many fields (PCSX2's VSyncs: two to each
    /// of the game's frames, `macros::FIELDS_PER_FRAME`); 0 holds it
    pub frequency: u16,
    /// `MacroNPressure`, percent: how far it pushes a stick's way (every
    /// button is pressed or not to the game: `VIPad::Read` reads no
    /// pressure)
    pub pressure: u8,
    /// `MacroNDeadzone`, percent: how far a stick's way or a trigger's pull
    /// in its trigger must go before it counts (a key or a button counts
    /// at any deadzone under 100: PCSX2's `value > deadzone`)
    pub deadzone: u8,
}

impl Default for Macro {
    /// PCSX2's: no trigger, nothing pressed, held, steady, at full pressure
    /// and with no deadzone.
    fn default() -> Self {
        Macro { triggers: Vec::new(), buttons: 0, ways: 0, toggle: false, frequency: 0, pressure: PRESSURE_DEFAULT, deadzone: DEADZONE_DEFAULT }
    }
}

impl Macro {
    /// Whether it presses anything (PCSX2 takes no notice of the trigger of
    /// a macro with no binds).
    pub fn presses(&self) -> bool {
        self.buttons != 0 || self.ways != 0
    }

    /// Whether it can run: a trigger, and something to press.
    pub fn set_up(&self) -> bool {
        self.presses() && self.triggers.iter().any(|c| !c.is_empty())
    }

    /// What it presses as the windows draw it: the buttons' glyphs, then
    /// the sticks' ways, "-" for none.
    pub fn binds_text(&self) -> String {
        let mut s: String = (0..16u8).filter(|i| self.buttons & (1 << i) != 0).map(glyph).collect();
        for (w, (_, t)) in WAYS.iter().enumerate() {
            if self.ways & (1 << w) != 0 {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(t);
            }
        }
        if s.is_empty() { "-".into() } else { s }
    }

    /// What it presses by name (`keys.opt`'s `buttons` line, the log).
    fn binds_names(&self) -> String {
        let mut v: Vec<&str> = (0..16).filter(|i| self.buttons & (1 << i) != 0).map(|i| BUTTONS[i].0).collect();
        v.extend((0..8).filter(|w| self.ways & (1 << w) != 0).map(|w| WAYS[w].0));
        v.join("+")
    }
}

/// Why a trigger cannot be set (`Binds::set_trigger`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroKey {
    /// a key of the page's and the window's own (its name)
    Reserved(String),
    /// a key a row is on now: its name, the row's index
    Row(String, usize),
    /// no input at all, or the macro has as many triggers as it keeps
    Nothing,
}

/// What setting a trigger took from the game, and gave back (the page says
/// it): fields and rows of the client's keys left with no button, and
/// buttons no trigger has any more, back on a field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Taken {
    /// (the button, the field it drove) for each button taken off a field
    pub fields: Vec<(u8, u8)>,
    /// (the button, the row) for each taken off a row of the client's keys
    pub rows: Vec<(u8, usize)>,
    /// the buttons given back, each now on a field
    pub freed: Vec<u8>,
}

/// The player's bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binds {
    /// each row's key where the player moved it (by `ROWS`' index; None: its
    /// default)
    keys: Vec<Option<String>>,
    /// the physical button (a bit index) on each of the game's fields, or
    /// `NO_BUTTON`
    pad: [u8; 16],
    /// the physical button on each row the game has no button for
    /// (`takes_button`), by `ROWS`' index
    rows: Vec<Option<u8>>,
    /// the macros
    pub macros: [Macro; MACROS],
}

impl Default for Binds {
    fn default() -> Self {
        Binds {
            keys: vec![None; ROWS.len()],
            pad: std::array::from_fn(|i| i as u8),
            rows: vec![None; ROWS.len()],
            macros: Default::default(),
        }
    }
}

/// Whether row `row` can be given a button of its own: a row of the
/// client's keys, which no field of the game's pad has (the sticks' rows
/// stay sticks, and a button's row is its field's).
pub fn takes_button(row: usize) -> bool {
    ROWS.get(row).is_some_and(|r| r.pad == Pad::None)
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
    /// a button moved, and the row of the client's keys that had it (by
    /// `ROWS`' index) took the one it left, or none
    SwappedRow(usize),
    /// a button taken back out of these macros' triggers (every trigger
    /// it was in went): the button its field or row had went to a free
    /// field (`Binds::place_free`)
    FromMacros(Vec<usize>),
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
        if self.key_in_macros(name) {
            return Alias::Macro;
        }
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
        // the macros' triggers with the key give it up: the row takes it
        // back, and every trigger it was in goes
        self.take_from_macros(&Input::Key(name.to_string()));
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

    /// The physical button (bit index) on field `field` (bit index), or
    /// `NO_BUTTON`.
    pub fn button(&self, field: u8) -> u8 {
        self.pad[field as usize]
    }

    /// The field physical button `phys` drives, if a field's.
    pub fn field_of(&self, phys: u8) -> Option<u8> {
        self.pad.iter().position(|&p| p == phys).map(|f| f as u8)
    }

    /// The physical button on row `row` of the client's keys, if it has one.
    pub fn row_button(&self, row: usize) -> Option<u8> {
        self.rows.get(row).copied().flatten()
    }

    /// The row of the client's keys physical button `phys` presses, if a
    /// row's.
    pub fn row_of_button(&self, phys: u8) -> Option<usize> {
        self.rows.iter().position(|&b| b == Some(phys))
    }

    /// Puts physical button `phys` on field `field`; the field or the row it
    /// was on takes the button `field` had (a swap; a row, none if `field`
    /// had none).
    pub fn bind_button(&mut self, field: u8, phys: u8) -> Moved {
        let (field, phys) = (field as usize & 15, phys & 15);
        let had = self.pad[field];
        if had == phys {
            return Moved::Already;
        }
        self.pad[field] = phys;
        if let Some(r) = self.row_of_button(phys) {
            self.rows[r] = (had != NO_BUTTON).then_some(had);
            return Moved::SwappedRow(r);
        }
        // out of the macros' triggers: each one it was in goes, and the
        // button the field had a free field (the button is on its field
        // before, so the triggers' other buttons do not take it)
        if self.button_in_macros(phys) {
            let ms = self.take_from_macros(&Input::Button(phys));
            if had != NO_BUTTON {
                self.place_free(had);
            }
            return Moved::FromMacros(ms);
        }
        match self.pad.iter().enumerate().position(|(f, &p)| p == phys && f != field) {
            Some(other) => {
                self.pad[other] = had;
                Moved::Swapped(other)
            }
            None => Moved::Freed((had != NO_BUTTON).then(|| BUTTONS[had as usize].0.to_string())),
        }
    }

    /// Puts physical button `phys` on row `row` of the client's keys
    /// (`takes_button`): the field it drove takes the button the row had,
    /// or is left with none (`Swapped`, the field); a row it was on takes
    /// it, or none (`SwappedRow`).
    pub fn bind_row_button(&mut self, row: usize, phys: u8) -> Moved {
        let phys = phys & 15;
        if !takes_button(row) {
            return Moved::Already;
        }
        let had = self.rows[row];
        if had == Some(phys) {
            return Moved::Already;
        }
        if let Some(f) = self.field_of(phys) {
            self.pad[f as usize] = had.unwrap_or(NO_BUTTON);
            self.rows[row] = Some(phys);
            return Moved::Swapped(f as usize);
        }
        if self.button_in_macros(phys) {
            self.rows[row] = Some(phys);
            let ms = self.take_from_macros(&Input::Button(phys));
            if let Some(h) = had {
                self.place_free(h);
            }
            return Moved::FromMacros(ms);
        }
        match self.row_of_button(phys) {
            Some(r) => {
                self.rows[r] = had;
                self.rows[row] = Some(phys);
                Moved::SwappedRow(r)
            }
            None => {
                self.rows[row] = Some(phys);
                Moved::Freed(had.map(|b| BUTTONS[b as usize].0.to_string()))
            }
        }
    }

    /// Whether a key is in a macro's trigger (any macro's).
    pub fn key_in_macros(&self, name: &str) -> bool {
        self.macros.iter().any(|m| m.triggers.iter().flatten().any(|i| matches!(i, Input::Key(k) if k == name)))
    }

    /// The macros with `input` in a trigger, by index.
    pub fn macros_with(&self, input: &Input) -> Vec<usize> {
        (0..MACROS).filter(|&m| self.macros[m].triggers.iter().flatten().any(|i| i == input)).collect()
    }

    /// Whether a controller's button (physical) is in a macro's trigger.
    pub fn button_in_macros(&self, phys: u8) -> bool {
        self.macro_buttons() & (1 << (phys & 15)) != 0
    }

    /// The controllers' buttons (physical, a bit each) in the macros'
    /// triggers: none of them drives a field or a row.
    pub fn macro_buttons(&self) -> u16 {
        let mut out = 0;
        for i in self.macros.iter().flat_map(|m| m.triggers.iter().flatten()) {
            if let Input::Button(b) = i {
                out |= 1 << (b & 15);
            }
        }
        out
    }

    /// The sticks' ways (`WAYS`' bits) in the macros' triggers.
    pub fn macro_ways(&self) -> u8 {
        let mut out = 0;
        for i in self.macros.iter().flat_map(|m| m.triggers.iter().flatten()) {
            if let Input::Stick(w) = i {
                out |= 1 << (w & 7);
            }
        }
        out
    }

    /// The controllers' sticks (rx, ry, lx, ly, +y down) as the game sees
    /// them: a way in a macro's trigger is the trigger's, at rest to the
    /// game (the stick's other way is still the game's).
    pub fn mask_ways(&self, sticks: [f32; 4]) -> [f32; 4] {
        let w = self.macro_ways();
        let mut s = sticks;
        for (i, &(axis, negative)) in WAY_AXES.iter().enumerate() {
            if w & (1 << i) != 0 && (if negative { s[axis] < 0.0 } else { s[axis] > 0.0 }) {
                s[axis] = 0.0;
            }
        }
        s
    }

    /// Whether a button (physical) is anywhere: a field's, a row's, a
    /// trigger's.
    fn placed(&self, phys: u8) -> bool {
        self.field_of(phys).is_some() || self.row_of_button(phys).is_some() || self.button_in_macros(phys)
    }

    /// A button that is in no place put on a field: its own when that has
    /// none, else the first with none (there is always one: sixteen buttons
    /// for sixteen fields, and the rows and the triggers hold the rest).
    /// The field, if it was put on one.
    fn place_free(&mut self, phys: u8) -> Option<u8> {
        let phys = phys & 15;
        if self.placed(phys) {
            return None;
        }
        let f = if self.pad[phys as usize] == NO_BUTTON { Some(phys as usize) } else { self.pad.iter().position(|&p| p == NO_BUTTON) }?;
        self.pad[f] = phys;
        Some(f as u8)
    }

    /// Every trigger with `input` in it taken off its macro (a field, a row
    /// or a row's key taking the input back): a trigger without one of its
    /// inputs would be another. Their other buttons that no trigger has now
    /// go back on a field. The macros that lost one, by index.
    fn take_from_macros(&mut self, input: &Input) -> Vec<usize> {
        let (mut changed, mut loose) = (Vec::new(), Vec::new());
        for (m, mac) in self.macros.iter_mut().enumerate() {
            let before = mac.triggers.len();
            mac.triggers.retain(|c| {
                let has = c.contains(input);
                if has {
                    loose.extend(c.iter().cloned());
                }
                !has
            });
            if mac.triggers.len() != before {
                changed.push(m);
            }
        }
        self.free(&loose);
        changed
    }

    /// Whether `chord` can be a trigger: some input, and no key of the
    /// page's own or one a row is on (a row moved off its key leaves it
    /// free; a row's other key, ~ beside `, is the row's while it is not
    /// moved).
    pub fn check_trigger(&self, chord: &[Input]) -> Result<(), MacroKey> {
        if chord.is_empty() {
            return Err(MacroKey::Nothing);
        }
        for i in chord {
            if let Input::Key(k) = i {
                if reserved(k) {
                    return Err(MacroKey::Reserved(k.clone()));
                }
                if let Some(r) = self.row_on(k).or_else(|| default_row(k).filter(|&r| !self.moved(r))) {
                    return Err(MacroKey::Row(k.clone(), r));
                }
            }
        }
        Ok(())
    }

    /// Macro `m`'s trigger `slot` made `chord` (`None`, or a slot past its
    /// last: another, after them; `TRIGGERS_MOST` at most), refused as
    /// `check_trigger` refuses: its buttons leave the fields and rows they
    /// drove, and the old trigger's buttons that no trigger has any more go
    /// back on a field (`Taken` says which).
    pub fn set_trigger(&mut self, m: usize, slot: Option<usize>, chord: Vec<Input>) -> Result<Taken, MacroKey> {
        self.check_trigger(&chord)?;
        let n = self.macros[m].triggers.len();
        if slot.is_none_or(|s| s >= n) && n >= TRIGGERS_MOST {
            return Err(MacroKey::Nothing);
        }
        Ok(self.put_trigger(m, slot, chord))
    }

    /// `set_trigger` with nothing refused (`keys.opt`'s lines are judged as
    /// they are read): each input once, `CHORD_MOST` of them; the same
    /// chord twice in one macro (in any order) kept once, where it is set.
    fn put_trigger(&mut self, m: usize, slot: Option<usize>, chord: Vec<Input>) -> Taken {
        let mut c: Vec<Input> = Vec::new();
        for i in chord {
            let i = match i {
                Input::Button(b) => Input::Button(b & 15),
                Input::Stick(w) => Input::Stick(w & 7),
                k => k,
            };
            if !c.contains(&i) && c.len() < CHORD_MOST {
                c.push(i);
            }
        }
        let mut taken = Taken::default();
        if c.is_empty() || m >= MACROS {
            return taken;
        }
        for i in &c {
            if let Input::Button(b) = *i {
                if let Some(f) = self.field_of(b) {
                    self.pad[f as usize] = NO_BUTTON;
                    taken.fields.push((b, f));
                } else if let Some(r) = self.row_of_button(b) {
                    self.rows[r] = None;
                    taken.rows.push((b, r));
                }
            }
        }
        let same = |a: &[Input], b: &[Input]| a.len() == b.len() && a.iter().all(|x| b.contains(x));
        let triggers = &mut self.macros[m].triggers;
        let (at, old) = match slot {
            Some(s) if s < triggers.len() => (s, std::mem::replace(&mut triggers[s], c.clone())),
            _ => {
                triggers.push(c.clone());
                (triggers.len() - 1, Vec::new())
            }
        };
        let mut k = 0;
        triggers.retain(|t| {
            let keep = k == at || !same(t, &c);
            k += 1;
            keep
        });
        taken.freed = self.free(&old);
        taken
    }

    /// Macro `m`'s trigger `slot` taken off: its buttons that no trigger
    /// has now go back on a field (those, as `free` gives them).
    pub fn remove_trigger(&mut self, m: usize, slot: usize) -> Vec<u8> {
        if slot >= self.macros[m].triggers.len() {
            return Vec::new();
        }
        let old = self.macros[m].triggers.remove(slot);
        self.free(&old)
    }

    /// Every trigger of macro `m` taken off (PCSX2's right click).
    pub fn clear_triggers(&mut self, m: usize) -> Vec<u8> {
        let old: Vec<Input> = std::mem::take(&mut self.macros[m].triggers).into_iter().flatten().collect();
        self.free(&old)
    }

    /// Every macro as PCSX2 starts one (the Macros page's Defaults): their
    /// buttons back on the fields.
    pub fn clear_macros(&mut self) -> Vec<u8> {
        let old: Vec<Input> = self.macros.iter_mut().flat_map(|m| std::mem::take(m).triggers).flatten().collect();
        self.free(&old)
    }

    /// The buttons of `inputs` that are in no place now, each put back on a
    /// field; those it put back.
    fn free(&mut self, inputs: &[Input]) -> Vec<u8> {
        let mut out = Vec::new();
        for i in inputs {
            if let Input::Button(b) = i
                && self.place_free(*b).is_some()
            {
                out.push(*b);
            }
        }
        out
    }

    /// The Controls page's Defaults: every key and button where the game
    /// began. The macros keep what they press and how, and the triggers
    /// that need no key a row has and no button (all of them the game's
    /// again): their keys of their own, the sticks' ways.
    pub fn controls_defaults(&self) -> Binds {
        let mut d = Binds::default();
        let kept = |c: &Vec<Input>| {
            c.iter().all(|i| match i {
                Input::Key(k) => !reserved(k) && default_row(k).is_none(),
                Input::Button(_) => false,
                Input::Stick(_) => true,
            })
        };
        for (m, mac) in self.macros.iter().enumerate() {
            d.macros[m] = Macro { triggers: mac.triggers.iter().filter(|c| kept(c)).cloned().collect(), ..mac.clone() };
        }
        d
    }

    /// The controllers' buttons (`input::bit`, physical) as the game's
    /// fields; a button on a row of the client's keys is none of them.
    pub fn remap(&self, buttons: u16) -> u16 {
        let mut out = 0;
        for (field, &phys) in self.pad.iter().enumerate() {
            if phys != NO_BUTTON && buttons & (1 << phys) != 0 {
                out |= 1 << field;
            }
        }
        out
    }

    /// `keys.opt`: a line for each row moved (`key.<row>=<key>`, an empty
    /// key for none), each row of the client's keys given a button
    /// (`button.<row>=<button>`), and each field on another button
    /// (`pad.<field>=<button>`), in that order: read back from the defaults
    /// line by line, the rows' buttons take theirs off the fields first, so
    /// a field a row took one from is left with none without a line of its
    /// own (and a file of a client before rows had buttons reads the same).
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for (r, k) in self.keys.iter().enumerate() {
            if let Some(k) = k {
                s.push_str(&format!("key.{}={k}\n", ROWS[r].id));
            }
        }
        for (r, b) in self.rows.iter().enumerate() {
            if let Some(b) = b {
                s.push_str(&format!("button.{}={}\n", ROWS[r].id, BUTTONS[*b as usize].0));
            }
        }
        for (f, &p) in self.pad.iter().enumerate() {
            if p as usize != f && p != NO_BUTTON {
                s.push_str(&format!("pad.{}={}\n", BUTTONS[f].0, BUTTONS[p as usize].0));
            }
        }
        for (i, m) in self.macros.iter().enumerate() {
            let n = i + 1;
            for c in &m.triggers {
                s.push_str(&format!("macro.{n}.trigger={}\n", c.iter().map(Input::opt).collect::<Vec<_>>().join(" & ")));
            }
            if m.presses() {
                s.push_str(&format!("macro.{n}.buttons={}\n", m.binds_names()));
            }
            if m.toggle {
                s.push_str(&format!("macro.{n}.toggle=1\n"));
            }
            if m.frequency != 0 {
                s.push_str(&format!("macro.{n}.frequency={}\n", m.frequency));
            }
            if m.pressure != PRESSURE_DEFAULT {
                s.push_str(&format!("macro.{n}.pressure={}\n", m.pressure));
            }
            if m.deadzone != DEADZONE_DEFAULT {
                s.push_str(&format!("macro.{n}.deadzone={}\n", m.deadzone));
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
            } else if let Some(id) = k.strip_prefix("button.")
                && let (Some(r), Some(p)) = (row_named(id), button(v))
            {
                b.bind_row_button(r, p);
            } else if let Some(rest) = k.strip_prefix("macro.")
                && let Some((n, what)) = rest.split_once('.')
                && let Ok(n) = n.parse::<usize>()
                && (1..=MACROS).contains(&n)
            {
                let m = n - 1;
                let room = b.macros[m].triggers.len() < TRIGGERS_MOST;
                match what {
                    // a trigger: its inputs joined by " & " (PCSX2's `&`)
                    "trigger" if room => {
                        let chord: Option<Vec<Input>> = split_chord(v).into_iter().map(Input::from_opt).collect();
                        if let Some(c) = chord
                            && b.file_keys(&c)
                        {
                            b.put_trigger(m, None, c);
                        }
                    }
                    // a build before 2026-11-10 kept a key and a button,
                    // either of which ran the macro: two triggers
                    "key" if room && valid_name(v) => {
                        let c = vec![Input::Key(v.to_string())];
                        if b.file_keys(&c) {
                            b.put_trigger(m, None, c);
                        }
                    }
                    "button" if room => {
                        if let Some(p) = button(v) {
                            b.put_trigger(m, None, vec![Input::Button(p)]);
                        }
                    }
                    "buttons" => {
                        let (mut buttons, mut ways) = (0u16, 0u8);
                        for n in v.split('+') {
                            if let Some(i) = button(n) {
                                buttons |= 1 << i;
                            } else if let Some(w) = WAYS.iter().position(|w| w.0 == n) {
                                ways |= 1 << w;
                            }
                        }
                        (b.macros[m].buttons, b.macros[m].ways) = (buttons, ways);
                    }
                    "toggle" => b.macros[m].toggle = v == "1",
                    // that build's turbo pressed for so many of the game's
                    // frames and let go for as many: twice as many fields
                    "turbo" => {
                        if let Ok(t) = v.parse::<u8>() {
                            b.macros[m].frequency = u16::from(t) * crate::macros::FIELDS_PER_FRAME as u16;
                        }
                    }
                    // as PCSX2 reads them: a frequency up to a u16's top, a
                    // pressure of 1 to 100, a deadzone of 0 to 100
                    "frequency" => {
                        if let Ok(f) = v.parse::<u64>() {
                            b.macros[m].frequency = f.min(u64::from(u16::MAX)) as u16;
                        }
                    }
                    "pressure" => {
                        if let Ok(p) = v.parse::<u64>() {
                            b.macros[m].pressure = p.clamp(u64::from(PRESSURE_LEAST), 100) as u8;
                        }
                    }
                    "deadzone" => {
                        if let Ok(d) = v.parse::<u64>() {
                            b.macros[m].deadzone = d.min(100) as u8;
                        }
                    }
                    _ => {}
                }
            }
        }
        b
    }

    /// Whether a trigger's keys read off keys.opt may stay, as
    /// `check_trigger` would let them, but for a key a row has only by its
    /// default (the defaults moved since the file was kept: G was free till
    /// the attack took it, M till the map came back), which stays the
    /// macro's: the row is left with none, or keeps its first key where the
    /// macro's is its other. A row the file moved onto a key keeps it.
    fn file_keys(&mut self, chord: &[Input]) -> bool {
        let mut clear = Vec::new();
        for i in chord {
            let Input::Key(k) = i else { continue };
            if reserved(k) {
                return false;
            }
            match self.row_on(k).or_else(|| default_row(k).filter(|&r| !self.moved(r))) {
                Some(r) if self.moved(r) => return false,
                // its first key: the row is left with none; its other: kept
                Some(r) if ROWS[r].keys.first() == Some(&k.as_str()) => clear.push(r),
                _ => {}
            }
        }
        for r in clear {
            self.clear_key(r);
        }
        !chord.is_empty()
    }

    /// Takes a row's key away (a hand-written `key.<row>=`): its key does
    /// nothing.
    fn clear_key(&mut self, row: usize) {
        if !ROWS[row].keys.is_empty() {
            self.keys[row] = Some(String::new());
        }
    }

    /// What was moved, or that nothing was (said when the keys are read).
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
        for (r, b) in self.rows.iter().enumerate() {
            if let Some(b) = b {
                parts.push(format!("{} on button {}", ROWS[r].id, BUTTONS[*b as usize].0));
            }
        }
        for (f, &p) in self.pad.iter().enumerate() {
            if p == NO_BUTTON {
                parts.push(format!("pad {} on nothing", BUTTONS[f].0));
            } else if p as usize != f {
                parts.push(format!("pad {} on {}", BUTTONS[f].0, BUTTONS[p as usize].0));
            }
        }
        for (i, m) in self.macros.iter().enumerate() {
            if *m != Macro::default() {
                let on: Vec<String> = m.triggers.iter().map(|c| c.iter().map(Input::opt).collect::<Vec<_>>().join("&")).collect();
                let mut how = vec![if m.toggle { "toggled" } else { "held" }.to_string()];
                if m.frequency != 0 {
                    how.push(format!("every {} fields", m.frequency));
                }
                if m.pressure != PRESSURE_DEFAULT {
                    how.push(format!("pressure {}%", m.pressure));
                }
                if m.deadzone != DEADZONE_DEFAULT {
                    how.push(format!("deadzone {}%", m.deadzone));
                }
                let on = if on.is_empty() { "nothing".to_string() } else { on.join(" or ") };
                let pressing = if m.presses() { m.binds_names() } else { "nothing".to_string() };
                parts.push(format!("macro {} on {on} pressing {pressing} ({})", i + 1, how.join(", ")));
            }
        }
        parts.join(", ")
    }
}

/// The controllers' buttons from frame to frame through the bindings: the
/// buttons they hold as the game's fields (`Binds::remap`); a button on a row
/// of the client's keys pressed and let go as that row's key, let go as the
/// row it was pressed as; and while a page of the options waits for a button
/// (the Controls page's, the Macros page's trigger), those newly pressed,
/// which the game then does not see until they are let go.
#[derive(Clone, Debug, Default)]
pub struct PadEdge {
    /// last frame's buttons, as the controllers had them
    last: u16,
    /// the buttons a capture took, held still
    swallow: u16,
    /// the buttons down as rows' keys: (button, row)
    rows: Vec<(u8, usize)>,
}

/// A frame of the controllers' buttons (`PadEdge::frame`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PadFrame {
    /// the game's fields held
    pub fields: u16,
    /// the button a capture took (its bit index)
    pub took: Option<u8>,
    /// every button a capture took this frame (physical, a bit each): a
    /// trigger's chord takes them all
    pub caught: u16,
    /// the buttons a capture took that were let go this frame (a trigger's
    /// chord is kept at the first)
    pub let_go: u16,
    /// the buttons as the controllers hold them, but those a capture took:
    /// what the macros' triggers read
    pub held: u16,
    /// rows of the client's keys (by `ROWS`' index) pressed (true) or let
    /// go, in the buttons' order, the let-go first
    pub rows: Vec<(usize, bool)>,
}

impl PadEdge {
    /// One frame: `raw` the controllers' buttons, `capturing` whether a
    /// button is awaited.
    pub fn frame(&mut self, raw: u16, capturing: bool, b: &Binds) -> PadFrame {
        let fresh = raw & !self.last;
        self.last = raw;
        let mut took = None;
        let mut caught = 0;
        if capturing && fresh != 0 {
            took = Some(fresh.trailing_zeros() as u8);
            caught = fresh;
            self.swallow |= fresh;
        }
        let let_go = self.swallow & !raw;
        self.swallow &= raw;
        let mut rows = Vec::new();
        self.rows.retain(|&(phys, row)| {
            let held = raw & (1 << phys) != 0;
            if !held {
                rows.push((row, false));
            }
            held
        });
        for phys in 0..16u8 {
            if fresh & !self.swallow & (1 << phys) != 0
                && let Some(row) = b.row_of_button(phys)
            {
                self.rows.push((phys, row));
                rows.push((row, true));
            }
        }
        PadFrame { fields: b.remap(raw & !self.swallow), took, caught, let_go, held: raw & !self.swallow, rows }
    }
}

/// What a key pressed or let go is sent on as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    /// a key, by name, to the keyboard's dispatch
    Key(String),
    /// a pad button (`input::bit`), to the game's pad path (`SetKey`)
    Button(u16),
    /// a key in a macros' trigger (its name), to the macros (`macros::Run`)
    Macro(String),
}

/// The keys from press to release through the bindings: what each key
/// pressed was made, so it is let go of as that, whatever changed between
/// (an edit box opened, a binding moved).
#[derive(Clone, Debug, Default)]
pub struct KeyEdge {
    pressed: std::collections::HashMap<String, Option<Sent>>,
    /// Whether Space, while the jump's row has it, is the attack's key now:
    /// wherever it makes no jump (in the world, and not in play on a server
    /// that offered jumps), as the viewer sets it before each key. A key
    /// let go is still what it was pressed as.
    pub space_attacks: bool,
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
            Alias::Macro => Some(Sent::Macro(name.to_string())),
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
        let out = match now(name) {
            // Space, the jump's own key, where it makes no jump: the attack's
            Some(Sent::Key(k)) if k == "space" && name == "space" && self.space_attacks => same(attack_key()),
            o => o,
        };
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

    /// Every key still down let go: what each was pressed as, to be let go
    /// of as that (a key a capture took, nothing), in the keys' order.
    pub fn let_go(&mut self) -> Vec<Sent> {
        let mut down: Vec<(String, Option<Sent>)> = self.pressed.drain().collect();
        down.sort_by(|a, b| a.0.cmp(&b.0));
        down.into_iter().filter_map(|(_, s)| s).collect()
    }
}

/// A trigger's line split into its inputs: at " & " as `to_text` writes it
/// (so the key `&` is `key:&`), or at a bare `&` written by hand where every
/// piece is an input.
fn split_chord(v: &str) -> Vec<&str> {
    if v.contains(" & ") {
        return v.split(" & ").map(str::trim).collect();
    }
    let parts: Vec<&str> = v.split('&').map(str::trim).collect();
    if parts.len() > 1 && parts.iter().all(|p| Input::from_opt(p).is_some()) { parts } else { vec![v.trim()] }
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
        // done), never bound (full screen, a bug report on a server that
        // takes them, the fly camera's speed)
        let not_rows = ["insert", "f11", "f3", "shift"];
        names.retain(|n| !not_rows.contains(&n.as_str()));
        assert!(names.len() > 60, "the scan found the dispatch's keys: {names:?}");
        for n in &names {
            assert!(default_row(n).is_some(), "{n:?} is in the dispatch and in no row");
        }
        for n in ["escape", "backspace", "m", "space", "enter", "home", "+", "=", "pageup", "-", "_", "pagedown", "up", "w", "down", "s", "left", "a", "right", "d", "q", "o"] {
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
        // 80,112/145, 81,113/133, 96,126/140, 9/142, 90,122/141, 67,99/143,
        // 13/147, 88,120 with 146 (both ToggleP74); and 32/134, Space with
        // ✕, which the attack's G has taken (Space jumps, and is ✕'s key
        // only where it makes no jump: `KeyEdge::space_attacks`)
        let pair = |k: &str| ROWS.iter().find(|r| r.keys.contains(&k)).map(|r| r.pad);
        for (k, p) in [("escape", bit::START), ("p", bit::SELECT), ("g", bit::CROSS), ("q", bit::SQUARE), ("`", bit::L1), ("~", bit::L1), ("tab", bit::R1), ("z", bit::L2), ("c", bit::R2), ("enter", bit::R3), ("x", bit::L3)] {
            assert_eq!(pair(k), Some(Pad::Button(p)), "{k}");
        }
        assert_eq!((pair("space"), pair("v")), (Some(Pad::None), Some(Pad::None)), "the jump's");
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
            "F3" => NamedKey::F3,
            "F5" => NamedKey::F5,
            "F6" => NamedKey::F6,
            "F7" => NamedKey::F7,
            "F8" => NamedKey::F8,
            "F9" => NamedKey::F9,
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

    /// A layout kept by a build with voice chat (its push-to-talk rows and
    /// the voice window's, gone with it: voice chat was only a test) still
    /// loads: those lines are left out and written back no more, and the rest
    /// of the file is read. The control: a known row's line beside them is.
    #[test]
    fn a_layout_with_the_voice_rows_still_loads() {
        let b = Binds::from_text("key.push_to_talk=b\nkey.push_to_talk_room=\nkey.voice_window=p\nbutton.push_to_talk=l3\nkey.attack=v\n");
        assert_eq!(b.key(row_named("attack").unwrap()), Some("v"), "the known line read");
        let text = b.to_text();
        assert!(!text.contains("push_to_talk") && !text.contains("voice"), "{text}");
        assert!(text.contains("key.attack=v"), "{text}");
    }

    /// A layout kept by a build with the map's rows (before they went on
    /// 2026-10-27, or since they came back) reads them again, and the Stats
    /// Box's line (its row gone for good) is left out and written back no
    /// more. The control: a known row's line beside them is read.
    #[test]
    fn a_layout_with_the_maps_rows_reads_them() {
        let b = Binds::from_text("key.map=y\nbutton.map=r3\nkey.stats_box=b\nkey.attack=u\n");
        assert_eq!(b.key(row_named("attack").unwrap()), Some("u"), "the known line read");
        assert_eq!(b.key(row_named("map").unwrap()), Some("y"), "the map's key read");
        assert_eq!(b.row_button(row_named("map").unwrap()), bit_index(bit::R3), "and its button");
        assert!(row_named("stats_box").is_none(), "no Stats Box row");
        let text = b.to_text();
        assert!(!text.contains("stats_box"), "{text}");
        assert!(text.contains("key.map=y\n") && text.contains("button.map=r3\n"), "{text}");
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
        let hide = row_named("hide_ui").unwrap();
        assert_eq!(c.bind_key(hide, "~"), Moved::Freed(Some("h".into())));
        assert_eq!((c.alias("~"), c.alias("`")), (Alias::To("h"), Alias::Same));
        // a three-way: attack onto Q (talk takes G), then talk onto E (use
        // takes G)
        let mut d = Binds::default();
        d.bind_key(attack, "q");
        d.bind_key(talk, "e");
        assert_eq!((d.key(attack), d.key(talk), d.key(use_)), (Some("q"), Some("e"), Some("g")));
        assert_eq!(d.alias("g"), Alias::To("e"));
        // △'s row, with no key of its own: onto a free key it presses △;
        // onto another row's key, that row is left with none
        let mut t = Binds::default();
        let clear = row_named("clear_target").unwrap();
        assert_eq!(t.key(clear), None);
        assert_eq!(t.bind_key(clear, "q"), Moved::Swapped(talk));
        assert_eq!((t.key(clear), t.key(talk)), (Some("q"), Some("")), "Talk left with no key");
        assert_eq!(t.alias("q"), Alias::Button(bit::TRIANGLE));
        assert_eq!(t.bind_key(clear, "y"), Moved::Freed(Some("q".into())));
        assert_eq!((t.alias("y"), t.alias("q")), (Alias::Button(bit::TRIANGLE), Alias::Nothing));
        // from a key, onto another row's: that row takes the key it left
        assert_eq!(t.bind_key(clear, "e"), Moved::Swapped(use_));
        assert_eq!(t.key(use_), Some("y"));
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
        let frame = |e: &mut PadEdge, raw, capturing| {
            let f = e.frame(raw, capturing, &b);
            assert!(f.rows.is_empty(), "no row has a button");
            (f.fields, f.took)
        };
        assert_eq!(frame(&mut e, bit::CIRCLE, false), (bit::CROSS, None), "remapped");
        assert_eq!(frame(&mut e, 0, false), (0, None));
        // held from before: not taken
        frame(&mut e, bit::R1, false);
        assert_eq!(frame(&mut e, bit::R1, true), (bit::R1, None));
        // pressed while awaited: the capture's
        assert_eq!(frame(&mut e, bit::R1 | bit::SQUARE, true), (bit::R1, Some(15)));
        assert_eq!(frame(&mut e, bit::R1 | bit::SQUARE, false), (bit::R1, None), "held: still not the game's");
        assert_eq!(frame(&mut e, bit::R1, false), (bit::R1, None));
        assert_eq!(frame(&mut e, bit::R1 | bit::SQUARE, false), (bit::R1 | bit::SQUARE, None), "pressed again: the game's");
        // the control
        let mut f = PadEdge::default();
        assert_eq!(frame(&mut f, bit::SQUARE, false), (bit::SQUARE, None));
    }

    /// F3 20261024-145732 (a key of the client's on R3). A button put on a row
    /// of the client's keys leaves the field it drove with none; one the row had
    /// goes to that field; a button taken from another such row leaves it
    /// the one this row had, or none; a field given a row's button gives the
    /// row its own. The rows the game has a button or a stick for take none.
    /// The control: the same moves on fields alone keep every field a button.
    #[test]
    fn a_button_on_a_row_of_the_clients_keys() {
        let (r3, l3, cross) = (bit_index(bit::R3).unwrap(), bit_index(bit::L3).unwrap(), bit_index(bit::CROSS).unwrap());
        let (hide, art, chat) = (row_named("hide_ui").unwrap(), row_named("art").unwrap(), row_named("chat").unwrap());
        let mut b = Binds::default();
        assert_eq!(b.bind_row_button(hide, r3), Moved::Swapped(r3 as usize));
        assert_eq!((b.row_button(hide), b.button(r3), b.field_of(r3), b.row_of_button(r3)), (Some(r3), NO_BUTTON, None, Some(hide)));
        assert_eq!(b.remap(bit::R3), 0, "R3 is Hide UI's: no field of the game's");
        assert_eq!(b.remap(bit::CROSS | bit::R3), bit::CROSS);
        assert_eq!(ROWS[chat].pad, Pad::Button(bit::R3), "the chat line's field is the one left with none");
        // Hide UI moved to L3: R3 goes back to the field L3 leaves (a swap)
        assert_eq!(b.bind_row_button(hide, l3), Moved::Swapped(l3 as usize));
        assert_eq!((b.button(l3), b.row_button(hide)), (r3, Some(l3)));
        assert_eq!(b.remap(bit::R3), bit::L3, "the physical R3 is autorun's now");
        // the art takes L3 off Hide UI, which had none to give
        assert_eq!(b.bind_row_button(art, l3), Moved::SwappedRow(hide));
        assert_eq!((b.row_button(art), b.row_button(hide)), (Some(l3), None));
        // the field takes its button back off the row: the row gets the
        // field's (R3), as a swap
        assert_eq!(b.bind_button(l3, l3), Moved::SwappedRow(art));
        assert_eq!((b.button(l3), b.row_button(art)), (l3, Some(r3)));
        // ✕ onto R3, the art's: the art takes ✕'s own
        assert_eq!(b.bind_button(cross, r3), Moved::SwappedRow(art));
        assert_eq!((b.button(cross), b.row_button(art), b.remap(bit::CROSS)), (r3, Some(cross), 0));
        // onto itself, and rows that take none
        assert_eq!(b.bind_row_button(art, cross), Moved::Already);
        for id in ["attack", "walk_forward", "clear_target"] {
            assert_eq!(b.bind_row_button(row_named(id).unwrap(), l3), Moved::Already, "{id}");
        }
        assert!(ROWS.iter().enumerate().filter(|(r, _)| takes_button(*r)).all(|(_, r)| r.pad == Pad::None && !r.keys.is_empty()));
        everything_once(&b);
        // the control: fields alone stay a permutation, none left without
        let mut c = Binds::default();
        c.bind_button(cross, r3);
        c.bind_button(l3, cross);
        assert!((0..16).all(|f| c.button(f) != NO_BUTTON));
        everything_once(&c);
    }

    /// Every physical button in exactly one place: a field's, a row's, or
    /// the macros' triggers (several of them may share one); every key a
    /// row's or the triggers', not both.
    fn everything_once(b: &Binds) {
        let mut seen: Vec<u8> = (0..16).map(|f| b.button(f)).filter(|&p| p != NO_BUTTON).collect();
        seen.extend((0..ROWS.len()).filter_map(|r| b.row_button(r)));
        seen.extend((0..16u8).filter(|&p| b.button_in_macros(p)));
        seen.sort_unstable();
        assert_eq!(seen, (0..16).collect::<Vec<u8>>(), "{b:?}");
        for m in &b.macros {
            for i in m.triggers.iter().flatten() {
                if let Input::Key(k) = i {
                    assert!(b.row_on(k).is_none(), "{k} a row's and a trigger's: {b:?}");
                }
            }
        }
    }

    /// Any sequence of moves keeps every button in one place, and keys.opt
    /// reads back the layout it was written from, every step of the way:
    /// fields, rows of the client's keys, rows moved onto keys, and macros'
    /// triggers set (chords of buttons, ways and keys, some refused),
    /// taken off and cleared. The control: a keys.opt without its `button.`
    /// lines reads back another.
    #[test]
    fn any_moves_keep_one_place_each_and_round_trip() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let rows: Vec<usize> = (0..ROWS.len()).filter(|&r| takes_button(r)).collect();
        let keyed: Vec<usize> = (0..ROWS.len()).filter(|&r| !ROWS[r].keys.is_empty()).collect();
        let keys = ["y", "u", "b", "7", "8", "9", "q", "g", "f2", "m"];
        let mut b = Binds::default();
        let (mut lossy, mut chords) = (0, 0);
        for _ in 0..3000 {
            let phys = next(16) as u8;
            match next(6) {
                0 => {
                    b.bind_button(next(16) as u8, phys);
                }
                1 => {
                    b.bind_row_button(rows[next(rows.len())], phys);
                }
                // a macro's trigger: a third place a button may be, shared
                2 | 3 => {
                    let mut c = vec![Input::Button(phys)];
                    for _ in 0..next(3) {
                        c.push(match next(3) {
                            0 => Input::Button(next(16) as u8),
                            1 => Input::Stick(next(8) as u8),
                            _ => Input::Key(keys[next(keys.len())].into()),
                        });
                    }
                    let slot = [None, Some(0), Some(1), Some(3)][next(4)];
                    chords += b.set_trigger(next(MACROS), slot, c).is_ok() as usize;
                }
                4 => {
                    let m = next(MACROS);
                    if next(4) == 0 {
                        b.clear_triggers(m);
                    } else {
                        b.remove_trigger(m, next(2));
                    }
                }
                _ => {
                    b.bind_key(keyed[next(keyed.len())], keys[next(keys.len())]);
                }
            }
            everything_once(&b);
            let t = b.to_text();
            assert_eq!(Binds::from_text(&t), b, "{t}");
            let without: String = t.lines().filter(|l| !l.starts_with("button.")).map(|l| format!("{l}\n")).collect();
            lossy += (Binds::from_text(&without) != b) as usize;
        }
        assert!(chords > 300, "triggers were set: {chords}");
        assert!(lossy > 300, "the control: {lossy}");
    }

    /// A button on a row presses the row's key and lets it go as the row it
    /// was pressed as, though the row moved meanwhile; one a capture takes
    /// presses nothing. The control: the same presses with nothing moved
    /// are the game's fields, no row's.
    #[test]
    fn a_rows_button_presses_its_key() {
        let (r3, l3) = (bit_index(bit::R3).unwrap(), bit_index(bit::L3).unwrap());
        let (hide, art) = (row_named("hide_ui").unwrap(), row_named("art").unwrap());
        let mut b = Binds::default();
        b.bind_row_button(hide, r3);
        let mut e = PadEdge::default();
        let f = e.frame(bit::R3 | bit::CROSS, false, &b);
        assert_eq!((f.fields, f.took, f.rows), (bit::CROSS, None, vec![(hide, true)]));
        assert_eq!(e.frame(bit::R3, false, &b).rows, vec![], "held: once");
        // moved while held (Hide UI to L3, the art onto R3): let go as Hide UI
        b.bind_row_button(hide, l3);
        b.bind_row_button(art, r3);
        assert_eq!(e.frame(0, false, &b).rows, vec![(hide, false)]);
        assert_eq!(e.frame(bit::R3 | bit::L3, false, &b).rows, vec![(hide, true), (art, true)], "in the buttons' order");
        assert_eq!(e.frame(0, false, &b).rows, vec![(hide, false), (art, false)]);
        // a capture's press is the capture's
        let f = e.frame(bit::R3, true, &b);
        assert_eq!((f.took, f.rows), (Some(r3), vec![]));
        assert_eq!(e.frame(0, false, &b).rows, vec![], "nothing to let go");
        // the control
        let mut c = PadEdge::default();
        let f = c.frame(bit::R3 | bit::L3, false, &Binds::default());
        assert_eq!((f.fields, f.rows), (bit::R3 | bit::L3, vec![]));
    }

    /// keys.opt from a client before rows had buttons reads as it did; a
    /// row's button is written and read back, the field it took from with
    /// none and no line of its own; a button for a row the game has a
    /// button for, or a name not a button's, is left out.
    #[test]
    fn keys_opt_keeps_the_rows_buttons() {
        let old = Binds::from_text("key.use=q\npad.circle=cross\npad.cross=circle\n");
        let mut want = Binds::default();
        want.bind_key(row_named("use").unwrap(), "q");
        want.bind_button(bit_index(bit::CROSS).unwrap(), bit_index(bit::CIRCLE).unwrap());
        assert_eq!(old, want);
        assert_eq!(old.to_text(), "key.talk=e\nkey.use=q\npad.circle=cross\npad.cross=circle\n", "written as before");
        let mut b = Binds::default();
        b.bind_row_button(row_named("hide_ui").unwrap(), bit_index(bit::R3).unwrap());
        assert_eq!(b.to_text(), "button.hide_ui=r3\n");
        assert_eq!(Binds::from_text("button.hide_ui=r3\n"), b);
        assert_eq!(b.describe(), "hide_ui on button r3, pad r3 on nothing");
        assert!(Binds::from_text("button.attack=r3\nbutton.hide_ui=nope\nbutton.nope=r3\n").is_default());
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

    /// The window losing the keys (`App::let_go_keys`): every key still down
    /// lets go as what it was pressed as (Tab as itself, a moved key as its
    /// row's, a key on △'s row as △), once; a key already let go, and one a
    /// capture took, are not. The control: `clear` alone forgets them, and
    /// nothing is let go of.
    #[test]
    fn keys_still_down_let_go_as_pressed() {
        let mut b = Binds::default();
        b.bind_key(row_named("walk_forward").unwrap(), "up");
        b.bind_key(row_named("clear_target").unwrap(), "b");
        let mut e = KeyEdge::default();
        for k in ["tab", "up", "b", "space"] {
            e.key(k, true, false, false, &b);
        }
        e.key("space", false, false, false, &b);
        e.hold_back("f2");
        assert_eq!(e.let_go(), vec![Sent::Button(bit::TRIANGLE), Sent::Key("tab".into()), Sent::Key("w".into())]);
        assert_eq!(e.let_go(), vec![], "once");
        assert_eq!(e.key("tab", false, false, false, &b), Some(Sent::Key("tab".into())), "a late release is the key's own");
        let mut c = KeyEdge::default();
        c.key("tab", true, false, false, &b);
        c.clear();
        assert_eq!(c.let_go(), vec![], "the control");
    }

    /// keys.opt: what was moved, read back the same; nothing moved is
    /// empty; a line not understood is left out and a file edited to put two
    /// rows on one key still makes one key one row's.
    #[test]
    fn keys_opt_round_trips_and_forgives() {
        let mut b = Binds::default();
        assert_eq!(b.to_text(), "");
        b.bind_key(row_named("use").unwrap(), "q");
        b.bind_key(row_named("hide_ui").unwrap(), "y");
        b.bind_button(bit_index(bit::CROSS).unwrap(), bit_index(bit::CIRCLE).unwrap());
        let t = b.to_text();
        assert_eq!(t, "key.talk=e\nkey.use=q\nkey.hide_ui=y\npad.circle=cross\npad.cross=circle\n");
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
            let r = crate::input::Report { buttons: b, ..crate::input::Report::default() };
            c.poll(r, true, false, false);
            assert_eq!(c.poll(r, true, false, false), vec![(pad_key(b).unwrap(), true)], "{name}");
        }
        assert_eq!(pad_key(bit::TRIANGLE), Some(132));
        assert_eq!(pad_key(0), None);
    }

    /// A macro's triggers (PCSX2's list of bindings, each a chord): a key a
    /// row is on is refused (its other key too, the attack's G, the page's
    /// own), a free key taken (its alias the macros'); a chord of a key and
    /// a button takes the button off its field (the game no longer sees it
    /// there, `PadEdge` gives it to the triggers); a second macro shares the
    /// button in a chord of its own; a stick's way leaves the game's stick;
    /// `keys.opt` round trips. A field given the button back takes every
    /// trigger it was in, and the chord's other button goes back to its
    /// field; a row moved onto the key takes the trigger with it. The
    /// control: with no macros, every button its field, the sticks as they
    /// are, no macro line.
    #[test]
    fn a_macros_triggers_take_their_inputs_and_give_them_back() {
        let control = Binds::default();
        assert!(!control.to_text().contains("macro."));
        assert_eq!((control.macro_buttons(), control.remap(0xFFFF), control.mask_ways([0.5, -0.5, 0.25, 1.0])), (0, 0xFFFF, [0.5, -0.5, 0.25, 1.0]));
        let key = |k: &str| Input::Key(k.to_string());
        let mut b = Binds::default();
        let (talk, cycle, attack) = (row_named("talk").unwrap(), row_named("cycle_target").unwrap(), row_named("attack").unwrap());
        assert_eq!(b.set_trigger(0, None, vec![key("q")]), Err(MacroKey::Row("q".into(), talk)));
        assert_eq!(b.set_trigger(0, None, vec![key("y"), key("~")]), Err(MacroKey::Row("~".into(), cycle)), "a row's other key, in a chord");
        assert_eq!(b.set_trigger(0, None, vec![key("f12")]), Err(MacroKey::Reserved("f12".into())));
        assert_eq!(b.set_trigger(0, None, vec![key("g")]), Err(MacroKey::Row("g".into(), attack)), "the attack's");
        assert_eq!(b.set_trigger(0, None, vec![]), Err(MacroKey::Nothing));
        assert!(b.is_default(), "nothing refused was kept");
        let (l1, cross, l3) = (bit_index(bit::L1).unwrap(), bit_index(bit::CROSS).unwrap(), bit_index(bit::L3).unwrap());
        let t = b.set_trigger(0, None, vec![key("y"), Input::Button(l1)]).unwrap();
        assert_eq!(t, Taken { fields: vec![(l1, l1)], ..Taken::default() });
        assert_eq!((b.alias("y"), b.remap(bit::L1), b.button(l1)), (Alias::Macro, 0, NO_BUTTON), "L1 is the triggers' now");
        let f = PadEdge::default().frame(bit::L1 | bit::CROSS, false, &b);
        assert_eq!((f.fields, f.held), (bit::CROSS, bit::L1 | bit::CROSS));
        // a second macro: L1 and ✕ together, and Y alone as another trigger
        b.set_trigger(1, None, vec![Input::Button(l1), Input::Button(cross)]).unwrap();
        b.set_trigger(1, None, vec![key("y")]).unwrap();
        assert_eq!((b.macro_buttons(), b.remap(bit::CROSS)), (bit::L1 | bit::CROSS, 0));
        // a stick's way: the game's stick at rest that way, the other way its
        b.set_trigger(2, None, vec![Input::Stick(4)]).unwrap();
        assert_eq!(b.mask_ways([0.5, -0.8, 0.0, 0.0]), [0.5, 0.0, 0.0, 0.0], "R up is the trigger's");
        assert_eq!(b.mask_ways([0.5, 0.8, 0.0, 0.0]), [0.5, 0.8, 0.0, 0.0], "R down still the game's");
        b.macros[0].buttons = bit::CROSS | bit::CIRCLE;
        b.macros[0].ways = 1 << 0;
        (b.macros[0].toggle, b.macros[0].frequency, b.macros[0].pressure, b.macros[0].deadzone) = (true, 4, 50, 25);
        b.macros[1].buttons = bit::R1;
        let t = b.to_text();
        assert!(
            t.contains("macro.1.trigger=key:y & pad:l1\nmacro.1.buttons=circle+cross+lup\nmacro.1.toggle=1\nmacro.1.frequency=4\nmacro.1.pressure=50\nmacro.1.deadzone=25\nmacro.2.trigger=pad:l1 & pad:cross\nmacro.2.trigger=key:y\nmacro.2.buttons=r1\nmacro.3.trigger=stick:rup\n"),
            "{t}"
        );
        assert_eq!(Binds::from_text(&t), b);
        assert!(b.describe().contains("macro 1 on key:y&pad:l1 pressing circle+cross+lup (toggled, every 4 fields, pressure 50%, deadzone 25%)"), "{}", b.describe());
        everything_once(&b);
        // L1 back on its field: every trigger with it goes (both macros'),
        // and ✕, in no trigger now, is back on its own
        assert_eq!(b.bind_button(l1, l1), Moved::FromMacros(vec![0, 1]));
        assert_eq!((b.macros[0].triggers.len(), b.macros[1].triggers.clone()), (0, vec![vec![key("y")]]));
        assert_eq!((b.remap(bit::L1 | bit::CROSS), b.macro_buttons()), (bit::L1 | bit::CROSS, 0));
        everything_once(&b);
        // a row moved onto Y takes it, and the trigger with it
        b.set_trigger(3, None, vec![key("y"), Input::Button(l3)]).unwrap();
        assert_eq!(b.remap(bit::L3), 0);
        b.bind_key(talk, "y");
        assert!(b.macros[1].triggers.is_empty() && b.macros[3].triggers.is_empty());
        assert_eq!((b.alias("y"), b.remap(bit::L3)), (Alias::To("q"), bit::L3), "L3 back on its field");
        everything_once(&b);
    }

    /// Setting a trigger again replaces it (the old one's buttons back on
    /// their fields, as the new one's leave theirs: a swap through the
    /// macro), the same chord twice in any order is one, a fifth trigger and
    /// a fifth input in a chord are not taken, and a row of the client's
    /// keys gives its button up to a trigger as a field does.
    #[test]
    fn a_trigger_set_again_gives_back_what_it_had() {
        let (l3, r3, cross) = (bit_index(bit::L3).unwrap(), bit_index(bit::R3).unwrap(), bit_index(bit::CROSS).unwrap());
        let mut b = Binds::default();
        b.set_trigger(0, None, vec![Input::Button(l3)]).unwrap();
        let t = b.set_trigger(0, Some(0), vec![Input::Button(r3)]).unwrap();
        assert_eq!((t.fields, t.freed), (vec![(r3, r3)], vec![l3]));
        assert_eq!((b.remap(bit::L3), b.remap(bit::R3)), (bit::L3, 0));
        b.set_trigger(0, None, vec![Input::Button(cross), Input::Stick(0)]).unwrap();
        b.set_trigger(0, None, vec![Input::Stick(0), Input::Button(cross)]).unwrap();
        assert_eq!(b.macros[0].triggers.len(), 2, "the same chord once");
        b.set_trigger(0, None, vec![Input::Stick(1)]).unwrap();
        b.set_trigger(0, None, vec![Input::Stick(2)]).unwrap();
        assert_eq!(b.set_trigger(0, None, vec![Input::Stick(3)]), Err(MacroKey::Nothing), "four triggers at most");
        b.set_trigger(1, None, (0..6).map(Input::Stick).collect()).unwrap();
        assert_eq!(b.macros[1].triggers[0].len(), CHORD_MOST);
        // the art's row on R3... then a trigger takes R3 from it
        let art = row_named("art").unwrap();
        b.remove_trigger(0, 0);
        b.bind_row_button(art, r3);
        let t = b.set_trigger(2, None, vec![Input::Button(r3)]).unwrap();
        assert_eq!((t.rows, b.row_button(art)), (vec![(r3, art)], None));
        everything_once(&b);
        // the control: taken off, R3 has no row to go back to, and is on its
        // field again
        assert_eq!(b.clear_triggers(2), vec![r3]);
        assert_eq!(b.remap(bit::R3), bit::R3);
        everything_once(&b);
    }

    /// A keys.opt of the build before 2026-11-10 (a key and a button, the
    /// buttons pressed, a toggle, a turbo in the game's frames) loads into
    /// today's macro: two triggers, either of which runs it, the same
    /// buttons, Press To Toggle, and the turbo's speed as a frequency in
    /// fields (two to each of the game's frames). Written back it is in
    /// today's lines and reads back the same; the control: none of the old
    /// lines is written again.
    #[test]
    fn an_old_keys_opt_keeps_its_macros() {
        let old = "pad.cross=circle\npad.circle=cross\nmacro.1.key=y\nmacro.1.button=l3\nmacro.1.buttons=circle+cross\nmacro.1.toggle=1\nmacro.1.turbo=4\nmacro.2.key=u\nmacro.2.buttons=square\n";
        let b = Binds::from_text(old);
        let l3 = bit_index(bit::L3).unwrap();
        let m = &b.macros[0];
        assert_eq!(m.triggers, vec![vec![Input::Key("y".into())], vec![Input::Button(l3)]]);
        assert_eq!((m.buttons, m.ways, m.toggle, m.frequency, m.pressure, m.deadzone), (bit::CIRCLE | bit::CROSS, 0, true, 8, 100, 0));
        assert_eq!((b.macros[1].triggers.len(), b.macros[1].frequency), (1, 0));
        assert_eq!((b.remap(bit::L3), b.remap(bit::CIRCLE)), (0, bit::CROSS), "L3 the macro's, the swap kept");
        let t = b.to_text();
        assert_eq!(Binds::from_text(&t), b, "{t}");
        for line in ["macro.1.key=", "macro.1.button=", "macro.1.turbo="] {
            assert!(!t.contains(line), "{line} in {t}");
        }
        everything_once(&b);
    }

    /// Today's lines read as PCSX2 reads its own: a frequency to a u16's
    /// top, a pressure of 1 to 100, a deadzone to 100; a trigger with an
    /// input not understood is left out; the key `&` and a bare `&`
    /// written by hand; the buttons line takes the sticks' ways. The
    /// control: in range, each as written.
    #[test]
    fn keys_opt_reads_a_macro_as_pcsx2_reads_one() {
        let b = Binds::from_text("macro.1.frequency=70000\nmacro.1.pressure=0\nmacro.1.deadzone=300\nmacro.2.pressure=250\nmacro.3.trigger=key:& & pad:r1\nmacro.4.trigger=pad:l1&pad:r1\nmacro.5.trigger=key:y & pad:nope\nmacro.6.buttons=cross+rleft+nope+ldown\n");
        assert_eq!((b.macros[0].frequency, b.macros[0].pressure, b.macros[0].deadzone, b.macros[1].pressure), (u16::MAX, 1, 100, 100));
        let r1 = bit_index(bit::R1).unwrap();
        assert_eq!(b.macros[2].triggers, vec![vec![Input::Key("&".into()), Input::Button(r1)]]);
        assert_eq!(b.macros[3].triggers, vec![vec![Input::Button(bit_index(bit::L1).unwrap()), Input::Button(r1)]]);
        assert!(b.macros[4].triggers.is_empty(), "an input not understood");
        assert_eq!((b.macros[5].buttons, b.macros[5].ways), (bit::CROSS, (1 << 7) | (1 << 2)));
        let c = Binds::from_text("macro.1.frequency=60\nmacro.1.pressure=37\nmacro.1.deadzone=12\n");
        assert_eq!((c.macros[0].frequency, c.macros[0].pressure, c.macros[0].deadzone), (60, 37, 12));
        assert_eq!(Binds::from_text(&b.to_text()), b);
    }

    /// The Controls page's Defaults puts every key and button back and keeps
    /// the macros' own: what they press and how, and their triggers of keys
    /// no row has and of the sticks' ways; a trigger with a button or a
    /// row's key goes (its button the game's again). The control: the
    /// layout's Defaults, nothing moved, keeps every macro whole.
    #[test]
    fn the_controls_defaults_keep_the_macros_own() {
        let mut b = Binds::default();
        let (l3, cross) = (bit_index(bit::L3).unwrap(), bit_index(bit::CROSS).unwrap());
        b.bind_key(row_named("talk").unwrap(), "7");
        b.bind_button(cross, bit_index(bit::CIRCLE).unwrap());
        b.set_trigger(0, None, vec![Input::Key("y".into())]).unwrap();
        b.set_trigger(0, None, vec![Input::Button(l3)]).unwrap();
        b.set_trigger(1, None, vec![Input::Key("q".into()), Input::Stick(3)]).unwrap();
        b.set_trigger(2, None, vec![Input::Stick(5)]).unwrap();
        (b.macros[0].buttons, b.macros[0].frequency) = (bit::SQUARE, 6);
        let d = b.controls_defaults();
        assert_eq!(d.macros[0].triggers, vec![vec![Input::Key("y".into())]]);
        assert_eq!((d.macros[0].buttons, d.macros[0].frequency), (bit::SQUARE, 6));
        assert!(d.macros[1].triggers.is_empty(), "Q is Talk's again");
        assert_eq!(d.macros[2].triggers, vec![vec![Input::Stick(5)]]);
        assert_eq!((d.remap(0xFFFF), d.alias("q")), (0xFFFF, Alias::Same));
        everything_once(&d);
        let mut kept = Binds::default();
        kept.set_trigger(0, None, vec![Input::Key("y".into()), Input::Stick(0)]).unwrap();
        assert_eq!(kept.controls_defaults(), kept, "the control");
    }

    /// The sticks' ways: each the half of its axis pushed, at rest the
    /// other way; back to the axes as PCSX2 merges them (the greater half
    /// over the lesser when both are set); every report's sticks the same
    /// after the round trip.
    #[test]
    fn a_sticks_ways_are_its_half_axes() {
        assert_eq!(ways([0.5, -0.25, -1.0, 0.75]), [0.0, 0.0, 0.75, 1.0, 0.25, 0.5, 0.0, 0.0]);
        for s in [[0.0; 4], [0.5, -0.25, -1.0, 0.75], [-0.1, 0.9, 0.3, -0.3], [1.0, 1.0, -1.0, -1.0]] {
            assert_eq!(axes(ways(s)), s);
        }
        let mut both = [0.0; 8];
        (both[0], both[2]) = (0.4, 0.6);
        assert_eq!(axes(both)[3], 0.6, "down over up, as PCSX2's MERGE");
        (both[5], both[7]) = (0.0, 0.3);
        assert_eq!(axes(both)[0], -0.3);
    }

    /// A macro kept on a key that was free when it was set and is a map row's
    /// by default now (M, the map's, gone from 2026-10-27 to 2026-11-09 while
    /// macros came): the macro keeps its key and the row is left with none,
    /// written back so, as master's rule keeps one on G; in today's lines as
    /// in the old. The control: a row the same file put on that key keeps
    /// it, the macro refused as ever.
    #[test]
    fn a_macro_kept_on_a_map_rows_key_keeps_it() {
        let map = row_named("map").unwrap();
        for file in ["macro.1.key=m\nmacro.1.buttons=cross\n", "macro.1.trigger=key:m\nmacro.1.buttons=cross\n"] {
            let b = Binds::from_text(file);
            assert_eq!(b.macros[0].triggers, vec![vec![Input::Key("m".into())]], "the macro keeps M");
            assert_eq!((b.key(map), b.alias("m")), (Some(""), Alias::Macro), "the row has no key, M is the macro's");
            assert_eq!(Binds::from_text(&b.to_text()), b, "kept so");
        }
        let control = Binds::from_text("key.talk=m\nmacro.1.key=m\nmacro.2.trigger=key:m\n");
        assert!(control.macros[0].triggers.is_empty() && control.macros[1].triggers.is_empty());
        assert_eq!(control.alias("m"), Alias::To("q"), "the control: a row put there in the same file keeps it");
    }

    /// Today's layout (the user, 2026-11-09: "make jump ... spacebar by
    /// default"): Space jumps and V with it, the attack is G, each itself
    /// with nothing moved; Space and V go with the jump's row when it moves,
    /// and Space taken by another row is that row's. The control: the layout
    /// before, Space the attack's and V the jump's, is not today's.
    #[test]
    fn space_jumps_and_g_attacks_by_default() {
        let b = Binds::default();
        let (jump, attack) = (row_named("jump").unwrap(), row_named("attack").unwrap());
        assert_eq!((b.key(jump), b.key(attack), attack_key()), (Some("space"), Some("g"), "g"));
        assert_eq!((b.alias("space"), b.alias("v"), b.alias("g")), (Alias::Same, Alias::Same, Alias::Same));
        assert!(space_is_jumps(&b));
        assert_eq!((default_row("space"), default_row("v"), default_row("g")), (Some(jump), Some(jump), Some(attack)));
        let mut moved = Binds::default();
        assert_eq!(moved.bind_key(jump, "b"), Moved::Freed(Some("space".into())));
        assert_eq!((moved.alias("b"), moved.alias("space"), moved.alias("v")), (Alias::To("space"), Alias::Nothing, Alias::Nothing));
        assert!(!space_is_jumps(&moved), "Space left with nothing");
        let mut taken = Binds::default();
        assert_eq!(taken.bind_key(row_named("talk").unwrap(), "space"), Moved::Swapped(jump));
        assert_eq!((taken.alias("space"), taken.key(jump)), (Alias::To("q"), Some("q")));
        assert!(!space_is_jumps(&taken), "Space Talk's");
        assert_ne!((ROWS[jump].keys, ROWS[attack].keys), (&["v"][..], &["space"][..]), "the control: the layout before");
    }

    /// Space, while the jump's row has it, is the attack's key wherever it
    /// makes no jump (`KeyEdge::space_attacks`, the viewer's: a window up,
    /// a server without jumps): pressed, held and let go as G, whatever the
    /// flag became between; while it jumps, itself. Never while typing, and
    /// never once the jump's row has moved off it or another row is on it,
    /// nor for the jump's V or a key moved onto its row. The controls: the
    /// flag clear, Space is Space, and G is G with it set or clear.
    #[test]
    fn space_is_the_attack_where_it_makes_no_jump() {
        let b = Binds::default();
        let key = |k: &str| Some(Sent::Key(k.to_string()));
        let mut e = KeyEdge::default();
        e.space_attacks = true;
        assert_eq!(e.key("space", true, false, false, &b), key("g"));
        assert_eq!(e.key("space", true, true, false, &b), key("g"), "its repeat");
        e.space_attacks = false;
        assert_eq!(e.key("space", false, false, false, &b), key("g"), "let go as pressed");
        assert_eq!(e.key("space", true, false, false, &b), key("space"), "the control: where it jumps");
        e.space_attacks = true;
        assert_eq!(e.key("space", false, false, false, &b), key("space"), "let go as pressed");
        // held as the window lost the keys: let go as G
        e.key("space", true, false, false, &b);
        assert_eq!(e.let_go(), vec![Sent::Key("g".into())]);
        assert_eq!(e.key("space", true, false, true, &b), key("space"), "typed as itself");
        e.key("space", false, false, true, &b);
        for k in ["g", "v"] {
            let mut c = KeyEdge::default();
            for on in [false, true] {
                c.space_attacks = on;
                assert_eq!(c.key(k, true, false, false, &b), key(k), "{k}");
                assert_eq!(c.key(k, false, false, false, &b), key(k), "{k}");
            }
        }
        // the jump's row moved onto B: B is the jump's everywhere, Space nothing
        let mut moved = Binds::default();
        moved.bind_key(row_named("jump").unwrap(), "b");
        assert_eq!(e.key("b", true, false, false, &moved), key("space"));
        e.key("b", false, false, false, &moved);
        assert_eq!(e.key("space", true, false, false, &moved), None);
        e.key("space", false, false, false, &moved);
        // Talk on Space: Talk's
        let mut taken = Binds::default();
        taken.bind_key(row_named("talk").unwrap(), "space");
        assert_eq!(e.key("space", true, false, false, &taken), key("q"));
    }

    /// A layout kept before Space jumped keeps every row it moved, each line
    /// read onto today's defaults: the attack it put on Q (Talk then on
    /// Space) stays on Q with Talk on Space, the jump taking G; a row it put
    /// on G keeps G, the attack taking that row's key; the jump it put on B
    /// stays on B (Space and V with nothing). A macro it put on G (free then)
    /// keeps G, the attack left with no key, and one on ~ (an alternate:
    /// never written by the client) keeps it beside the row's `. The
    /// controls: a macro on a key a line of the file moved a row onto is
    /// refused, as before, and a file of nothing is today's layout.
    #[test]
    fn a_layout_kept_before_space_jumped_keeps_what_was_moved() {
        let (attack, talk, jump, hide) = (row_named("attack").unwrap(), row_named("talk").unwrap(), row_named("jump").unwrap(), row_named("hide_ui").unwrap());
        let b = Binds::from_text("key.attack=q\nkey.talk=space\n");
        assert_eq!((b.key(attack), b.key(talk), b.key(jump)), (Some("q"), Some("space"), Some("g")));
        let b = Binds::from_text("key.hide_ui=g\n");
        assert_eq!((b.key(hide), b.key(attack), b.key(jump)), (Some("g"), Some("h"), Some("space")));
        let b = Binds::from_text("key.jump=b\n");
        assert_eq!((b.key(jump), b.alias("space"), b.alias("v"), b.alias("b")), (Some("b"), Alias::Nothing, Alias::Nothing, Alias::To("space")));
        let b = Binds::from_text("macro.3.key=g\nmacro.3.buttons=cross+circle\n");
        assert_eq!((b.macros[2].triggers.clone(), b.alias("g"), b.key(attack)), (vec![vec![Input::Key("g".into())]], Alias::Macro, Some("")));
        assert_eq!(b.alias("space"), Alias::Same, "Space still the jump's");
        assert_eq!(Binds::from_text(&b.to_text()), b, "kept so");
        let b = Binds::from_text("macro.1.key=~\n");
        assert_eq!((b.macros[0].triggers.clone(), b.alias("~"), b.alias("`")), (vec![vec![Input::Key("~".into())]], Alias::Macro, Alias::Same));
        let c = Binds::from_text("key.hide_ui=g\nmacro.1.key=g\n");
        assert_eq!((c.macros[0].triggers.is_empty(), c.key(hide)), (true, Some("g")), "the control: a row the file moved keeps it");
        assert!(Binds::from_text("").is_default());
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
