//! The master looter's choices on a corpse (the GM Jibrish's petition 1451:
//! "master looter. This allows the raid leader to open the loot corpse on kill,
//! and only that, and then add an option to the item they can select below
//! "Inspect" that says assign or abandon. Assign should list next to them in a
//! single rectangle window they can scroll up and down in that displays the
//! people in their raid. The one they click X or accept on gets that individual
//! item. This NEEDS a confirmation box showing who they are assigning it to.
//! Abandon loot releases the corpse for ANYONE to loot ANYTHING on it. This also
//! needs a confirmation window explaining this clearly."): the client's own.
//!
//! While the server says this player is the master looter of the corpse it has
//! open (go-eqoa-server's 0x7E47 after the corpse, `net::raid::MasterLoot`), the
//! corpse window's choice of an item (✕, Enter, a click on the chosen row) opens
//! a menu beside the corpse list in its place, the item kept lit: Take (the
//! game's `LootOne`), Inspect (the item's whole description, its stats too, in
//! the item menu's inspect window), Assign and Abandon. Assign opens one
//! rectangle beside the menu listing the players the server named, `ROWS` at a
//! time, up and down through the rest (the wheel's notches are its keys); ✕ or a
//! click on one asks "Assign <item> to <name>?" in the game's Yes/No box, and Yes
//! sends it (0x7E48 op 3, the item's key and the player's id). Abandon asks the same way,
//! saying what it does, and Yes lets the whole corpse go to anyone (op 4). △
//! steps back a window. The server says what it did in raid chat, and sends the
//! corpse again. Every other corpse, and every other player's, is the game's
//! window as it was.

use crate::dialog::{BoxAnswer, Key, MBox};
use crate::itemmenu::Inspect;
use net::dump::{ItemList, NetItem};
use net::raid::MasterLoot;
use vi::ui::{FrameParam, Raster, Vui, icon_id, wide};

/// What the menu and its windows ask of the corpse window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MlOut {
    /// the game's `LootOne`: the corpse item's key, and its count
    Take(i32, i32),
    /// the corpse item of that key to the player of that id
    Assign { key: u32, id: u32 },
    /// the corpse let go to anyone
    Abandon,
}

/// The menu's rows, top to bottom.
const MENU: [&str; 4] = ["Take", "Inspect", "Assign", "Abandon"];
const TAKE: usize = 0;
const INSPECT: usize = 1;
const ASSIGN: usize = 2;
const ABANDON: usize = 3;

/// The menu's place and the list's, beside the corpse list (at (144, 10), 118
/// wide); the rows' height and how many the list shows at once.
const MENU_AT: [i32; 2] = [270, 40];
const MENU_W: i32 = 120;
const LIST_AT: [i32; 2] = [396, 40];
const LIST_W: i32 = 170;
const ROW_H: i32 = 20;
pub const ROWS: usize = 10;

/// Which of its windows is up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Step {
    #[default]
    None,
    Menu,
    Inspect,
    Members,
    /// the box asking, for that member of the list
    Assign(usize),
    Abandon,
}

/// What a pointer found.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Menu(usize),
    Member(usize),
}

#[derive(Default)]
pub struct MasterLooter {
    /// the server's list for the corpse open, while this player is its master looter
    list: Option<MasterLoot>,
    step: Step,
    /// the corpse item the menu is for (its place in the corpse's list)
    item: usize,
    menu_sel: usize,
    member_sel: usize,
    member_top: usize,
    mbox: MBox,
    inspect: Inspect,
    /// the inspect window's text set, to be fitted at the next draw (which has the raster)
    inspect_due: bool,
    hits: Vec<([i32; 4], Hit)>,
}

/// The game's window frame (`VIWndMBox`'s), over black.
fn frame(vui: &Vui, r: &mut dyn Raster, at: [i32; 2], size: [i32; 2]) {
    let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
    r.blit(icon_id("black"), 0, at, [0, 0], size, 3, 0.8);
    vui.blit_frame(r, at, size, &fp, 3, 1.0, true);
}

impl MasterLooter {
    /// The server's word on the corpse open (0x7E47): this player its master
    /// looter (`Some`), or no longer (every window of it closed).
    pub fn set(&mut self, list: Option<MasterLoot>) {
        if list.is_none() {
            self.step = Step::None;
        }
        self.member_sel = 0;
        self.member_top = 0;
        self.list = list;
    }

    /// This player is the open corpse's master looter.
    pub fn on(&self) -> bool {
        self.list.is_some()
    }

    /// One of its windows is up: the keys and the pointer are its.
    pub fn busy(&self) -> bool {
        self.step != Step::None
    }

    /// The menu for corpse item `item` (its place in the corpse's list).
    pub fn open(&mut self, item: usize) {
        self.item = item;
        self.menu_sel = 0;
        self.step = Step::Menu;
    }

    /// The corpse item the menu is for.
    fn it<'a>(&self, corpse: &'a ItemList) -> Option<&'a NetItem> {
        corpse.items.get(self.item)
    }

    fn members(&self) -> &[(u32, String)] {
        self.list.as_ref().map_or(&[], |l| &l.members)
    }

    /// Member `k` lit, the list scrolled to show it.
    fn light(&mut self, k: usize) {
        self.member_sel = k;
        if k < self.member_top {
            self.member_top = k;
        } else if k >= self.member_top + ROWS {
            self.member_top = k + 1 - ROWS;
        }
    }

    /// A key (the game's codes) to the window up: what the corpse window must do.
    /// `describe` is the item's whole description (Inspect's).
    pub fn key(&mut self, key: i32, down: bool, corpse: &ItemList, describe: &dyn Fn(&NetItem) -> String, now: i64, cues: &mut Vec<i32>) -> Option<MlOut> {
        if self.step == Step::Inspect {
            if self.inspect.key(key, down, cues) {
                self.inspect.visible = false;
                self.step = Step::Menu;
            }
            return None;
        }
        if !down {
            return None;
        }
        let k = match key {
            19 | 138 => Key::Up,
            20 | 139 => Key::Down,
            17 | 136 => Key::Left,
            18 | 137 => Key::Right,
            13 | 134 => Key::Enter,
            27 | 132 => Key::Escape,
            _ => return None,
        };
        match self.step {
            Step::None | Step::Inspect => None,
            Step::Menu => match k {
                Key::Up | Key::Down => {
                    let to = if matches!(k, Key::Up) { self.menu_sel.checked_sub(1) } else { Some(self.menu_sel + 1).filter(|&s| s < MENU.len()) };
                    cues.push(if to.is_some() { 11 } else { 9 });
                    self.menu_sel = to.unwrap_or(self.menu_sel);
                    None
                }
                Key::Enter => {
                    cues.push(13);
                    self.choose(self.menu_sel, corpse, describe, now)
                }
                Key::Escape => {
                    cues.push(8);
                    self.step = Step::None;
                    None
                }
                _ => None,
            },
            Step::Members => {
                let n = self.members().len();
                match k {
                    Key::Up | Key::Down => {
                        let to = if matches!(k, Key::Up) { self.member_sel.checked_sub(1) } else { Some(self.member_sel + 1).filter(|&s| s < n) };
                        cues.push(if to.is_some() { 11 } else { 9 });
                        if let Some(t) = to {
                            self.light(t);
                        }
                        None
                    }
                    Key::Enter if n > 0 => {
                        cues.push(13);
                        self.ask_assign(self.member_sel, corpse, now);
                        None
                    }
                    Key::Escape => {
                        cues.push(8);
                        self.step = Step::Menu;
                        None
                    }
                    _ => None,
                }
            }
            Step::Assign(_) | Step::Abandon => {
                let answer = self.mbox.key(k, cues)?;
                self.answered(answer, corpse)
            }
        }
    }

    /// Menu row `row` chosen.
    fn choose(&mut self, row: usize, corpse: &ItemList, describe: &dyn Fn(&NetItem) -> String, now: i64) -> Option<MlOut> {
        let it = self.it(corpse)?;
        match row {
            TAKE => {
                self.step = Step::None;
                Some(MlOut::Take(it.inv_index as i32, it.quantity as i32))
            }
            INSPECT => {
                self.inspect.set(1, -1, &describe(it));
                self.inspect.visible = true;
                self.inspect_due = true;
                self.step = Step::Inspect;
                None
            }
            ASSIGN => {
                self.step = Step::Members;
                self.light(self.member_sel.min(self.members().len().saturating_sub(1)));
                None
            }
            ABANDON => {
                let text = wide("Abandon this corpse's loot? Anyone may then loot anything on it, and you are no longer its master looter.");
                self.mbox.prompt(&text, 1, 0, 0, now);
                self.step = Step::Abandon;
                None
            }
            _ => None,
        }
    }

    /// The box asking whether the item goes to member `k`.
    fn ask_assign(&mut self, k: usize, corpse: &ItemList, now: i64) {
        let (Some(it), Some((_, name))) = (self.it(corpse), self.members().get(k)) else { return };
        let text = wide(&format!("Assign {} to {}?", it.item.name, name));
        self.mbox.prompt(&text, 1, k as i32, 0, now);
        self.step = Step::Assign(k);
    }

    /// The box answered: Yes sends; No goes back a window.
    fn answered(&mut self, a: BoxAnswer, corpse: &ItemList) -> Option<MlOut> {
        let step = self.step;
        match (a, step) {
            (BoxAnswer::Yes(..), Step::Assign(k)) => {
                self.step = Step::None;
                let key = self.it(corpse)?.inv_index;
                let id = self.members().get(k)?.0;
                Some(MlOut::Assign { key, id })
            }
            (BoxAnswer::Yes(..), Step::Abandon) => {
                self.step = Step::None;
                Some(MlOut::Abandon)
            }
            (BoxAnswer::No(..), Step::Assign(_)) => {
                self.step = Step::Members;
                None
            }
            _ => {
                self.step = Step::Menu;
                None
            }
        }
    }

    /// A pointer at frame point `p`: the keys to press (`pointer`): a row of the
    /// menu or the list selected (`pointer::MOVED` once moved), then ✕ to take
    /// it; the box's buttons as the box takes them.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        use crate::pointer::{MOVED, key};
        match self.step {
            Step::Assign(_) | Step::Abandon => self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Step::Inspect => Vec::new(),
            Step::Menu | Step::Members => {
                let (x, y) = (p[0] as i32, p[1] as i32);
                let Some(&(_, hit)) = self.hits.iter().find(|(r, _)| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y)) else {
                    return Vec::new();
                };
                match hit {
                    Hit::Menu(k) if self.step == Step::Menu => {
                        if k == self.menu_sel {
                            vec![key::CROSS]
                        } else {
                            self.menu_sel = k;
                            vec![MOVED, key::CROSS]
                        }
                    }
                    Hit::Member(k) if self.step == Step::Members => {
                        if k == self.member_sel {
                            vec![key::CROSS]
                        } else {
                            self.light(k);
                            vec![MOVED, key::CROSS]
                        }
                    }
                    _ => Vec::new(),
                }
            }
            Step::None => Vec::new(),
        }
    }

    /// Its windows over the corpse window, each row's place noted.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, corpse: &ItemList, now: i64, cues: &mut Vec<i32>) {
        self.hits.clear();
        if self.step == Step::None {
            return;
        }
        let (white, yellow) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0]);
        // the menu, under every other of its windows
        let menu_h = MENU.len() as i32 * ROW_H + 16;
        frame(vui, r, MENU_AT, [MENU_W, menu_h]);
        for (k, label) in MENU.iter().enumerate() {
            let y = MENU_AT[1] + 8 + k as i32 * ROW_H;
            let lit = k == self.menu_sel;
            vui.draw_string_ex(r, &wide(label), MENU_AT[0] + 30, y + 2, if lit && self.step == Step::Menu { yellow } else { white }, true);
            if lit {
                vui.blit_ex(r, 89, [MENU_AT[0] + 8, y + 4], 3, 1.0, 0);
            }
            self.hits.push(([MENU_AT[0], y, MENU_AT[0] + MENU_W, y + ROW_H], Hit::Menu(k)));
        }
        if matches!(self.step, Step::Members | Step::Assign(_)) {
            let members = self.members().to_vec();
            let shown = members.len().min(ROWS).max(1) as i32;
            let h = shown * ROW_H + 32;
            frame(vui, r, LIST_AT, [LIST_W, h]);
            vui.draw_string_ex(r, &wide("Assign to:"), LIST_AT[0] + 14, LIST_AT[1] + 8, white, true);
            if members.is_empty() {
                vui.draw_string_ex(r, &wide("No one"), LIST_AT[0] + 30, LIST_AT[1] + 28, white, true);
            }
            for (row, k) in (self.member_top..members.len()).take(ROWS).enumerate() {
                let y = LIST_AT[1] + 26 + row as i32 * ROW_H;
                let lit = k == self.member_sel;
                let mut name = wide(&members[k].1);
                while !name.is_empty() && vui.string_width(r, &name) > LIST_W - 44 {
                    name.pop();
                }
                vui.draw_string_ex(r, &name, LIST_AT[0] + 30, y + 2, if lit { yellow } else { white }, true);
                if lit {
                    vui.blit_ex(r, 89, [LIST_AT[0] + 8, y + 4], 3, 1.0, 0);
                }
                self.hits.push(([LIST_AT[0], y, LIST_AT[0] + LIST_W, y + ROW_H], Hit::Member(k)));
            }
            // more above or below: a mark at the list's edge
            if self.member_top > 0 {
                vui.draw_string_ex(r, &wide("^"), LIST_AT[0] + LIST_W - 20, LIST_AT[1] + 8, white, true);
            }
            if self.member_top + ROWS < members.len() {
                vui.draw_string_ex(r, &wide("v"), LIST_AT[0] + LIST_W - 20, LIST_AT[1] + h - 22, white, true);
            }
        }
        if self.step == Step::Inspect {
            if std::mem::take(&mut self.inspect_due) {
                self.inspect.focus(vui, r);
            }
            self.inspect.draw(vui, r, cues);
        }
        if matches!(self.step, Step::Assign(_) | Step::Abandon) {
            self.mbox.pos = [150, 120];
            if let Some(a) = self.mbox.draw(vui, r, now) {
                self.answered(a, corpse);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};
    use crate::pointer::{MOVED, key};

    fn corpse() -> ItemList {
        let items = ["Sword of X", "Robe of Y"]
            .iter()
            .enumerate()
            .map(|(k, n)| {
                let mut it = NetItem { quantity: 1, inv_index: k as u32, equip_slot: -1, quick_slot: -1, ..Default::default() };
                it.item.name = n.to_string();
                it
            })
            .collect();
        ItemList { p04: 0, items }
    }

    fn looter(n: usize) -> MasterLooter {
        let mut m = MasterLooter::default();
        m.set(Some(MasterLoot { seconds: 300, members: (0..n).map(|k| (100 + k as u32, format!("m{k}"))).collect() }));
        m
    }

    fn keys(m: &mut MasterLooter, c: &ItemList, ks: &[i32]) -> Vec<MlOut> {
        let mut cues = Vec::new();
        let mut out = Vec::new();
        for &k in ks {
            out.extend(m.key(k, true, c, &|it| format!("about {}", it.item.name), 0, &mut cues));
            m.key(k, false, c, &|_| String::new(), 0, &mut cues);
        }
        out
    }

    #[test]
    fn take_is_the_games_loot_one() {
        let c = corpse();
        let mut m = looter(3);
        m.open(1);
        assert!(m.busy());
        assert_eq!(keys(&mut m, &c, &[134]), [MlOut::Take(1, 1)]);
        assert!(!m.busy());
    }

    #[test]
    fn assign_asks_and_yes_sends_no_goes_back() {
        let c = corpse();
        let mut m = looter(3);
        m.open(0);
        // Assign, the third member, ✕: the box asks, starting on No
        assert!(keys(&mut m, &c, &[139, 139, 134, 139, 139, 134]).is_empty());
        assert_eq!(m.step, Step::Assign(2));
        assert_eq!(String::from_utf16_lossy(&m.mbox.text), "Assign Sword of X to m2?");
        // No (the box's start): back to the list
        assert!(keys(&mut m, &c, &[134]).is_empty());
        assert_eq!(m.step, Step::Members);
        // ✕ again, left to Yes, ✕
        assert_eq!(keys(&mut m, &c, &[134, 136, 134]), [MlOut::Assign { key: 0, id: 102 }]);
        assert!(!m.busy());
    }

    #[test]
    fn abandon_asks_saying_what_it_does() {
        let c = corpse();
        let mut m = looter(2);
        m.open(0);
        assert!(keys(&mut m, &c, &[139, 139, 139, 134]).is_empty());
        assert_eq!(m.step, Step::Abandon);
        assert!(String::from_utf16_lossy(&m.mbox.text).contains("Anyone may then loot anything on it"));
        // the control: no, nothing sent and back to the menu
        assert!(keys(&mut m, &c, &[134]).is_empty());
        assert_eq!(m.step, Step::Menu);
        assert_eq!(keys(&mut m, &c, &[134, 136, 134]), [MlOut::Abandon]);
    }

    #[test]
    fn inspect_and_triangle_step_back() {
        let c = corpse();
        let mut m = looter(2);
        m.open(1);
        keys(&mut m, &c, &[139, 134]);
        assert_eq!(m.step, Step::Inspect);
        assert!(m.inspect.visible);
        keys(&mut m, &c, &[132]);
        assert_eq!(m.step, Step::Menu, "△ closes the description");
        keys(&mut m, &c, &[132]);
        assert!(!m.busy(), "and the menu");
        // the server's end of the hold closes every window
        m.open(0);
        m.set(None);
        assert!(!m.busy() && !m.on());
    }

    /// The list scrolls to keep the lit one shown, up and down.
    #[test]
    fn the_list_scrolls_up_and_down() {
        let c = corpse();
        let mut m = looter(25);
        m.open(0);
        keys(&mut m, &c, &[139, 139, 134]);
        assert_eq!(m.step, Step::Members);
        keys(&mut m, &c, &[139; 14]);
        assert_eq!((m.member_sel, m.member_top), (14, 5));
        keys(&mut m, &c, &[139; 30]);
        assert_eq!((m.member_sel, m.member_top), (24, 15), "no farther than the last");
        keys(&mut m, &c, &[138; 10]);
        assert_eq!((m.member_sel, m.member_top), (14, 14), "and back up, the lit row kept on the page");
    }

    /// A click selects a row where it was drawn and takes it; before a frame drew
    /// the menu there is nothing to find (the control: no hit notes).
    #[test]
    fn a_pointer_on_the_menu_and_the_list() {
        let (vui, mut r) = (vui(), NullRaster);
        let c = corpse();
        let mut m = looter(3);
        m.open(0);
        let row = |at: [i32; 2], y0: i32, k: i32| [(at[0] + 40) as f32, (at[1] + y0 + k * ROW_H + ROW_H / 2) as f32];
        assert!(m.point(row(MENU_AT, 8, ASSIGN as i32)).is_empty(), "not drawn yet");
        let mut cues = Vec::new();
        m.draw(&vui, &mut r, &c, 0, &mut cues);
        assert_eq!(m.point(row(MENU_AT, 8, ASSIGN as i32)), [MOVED, key::CROSS]);
        assert_eq!(m.menu_sel, ASSIGN);
        keys(&mut m, &c, &[134]);
        m.draw(&vui, &mut r, &c, 0, &mut cues);
        assert_eq!(m.point(row(LIST_AT, 26, 1)), [MOVED, key::CROSS]);
        assert_eq!(m.member_sel, 1);
        assert_eq!(m.point(row(LIST_AT, 26, 1)), [key::CROSS], "the lit one: taken");
        assert!(m.point([5.0, 5.0]).is_empty());
    }
}
