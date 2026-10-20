//! The tradeskill windows: `VIWndRecipe` (`viwndrecipe.cpp`, the recipe
//! book, VIWndGame mode 6) and `VIWndExperiment` (`viwndexperiment.cpp`,
//! mode 37), over [`crate::craft::Craft`]. Each has its message box; the book
//! has a pop-up menu on a recipe and an inspect window, the experiment a menu
//! of the inventory's components (40 rows, three options each). What they ask
//! of the client comes back as [`CraftUp`].
//!
//! A pointer (`pointer`) takes them as their keys reach them: in the book a
//! component or a recipe selected and the selected one used, the popup's
//! rows at once (a click off it closes it), the box's buttons; in the
//! experiment the component menu (`WndMenu::point`) and the box.
#![allow(clippy::too_many_arguments)]

use crate::client_image::{Menu, MenuEntry, Menus};
use crate::craft::{Arg, Craft, CraftOut, printf16};
use crate::dialog::{BoxAnswer, Key, MBox};
use crate::infotext::Info;
use crate::itemmenu::Inspect;
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::startmenu::KeyFlash;
use crate::wndlist::{ListUp, MiniMenu, WndMenu};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// What a crafting window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum CraftUp {
    /// `ClosePanel708` / `ClosePanel70C`: back to the start menu
    Close,
    /// the part's requests and lines
    Out(CraftOut),
}

fn word(vui: &Vui, id: i32) -> Vec<u16> {
    wide(vui.words.get(id, vui.language))
}

/// Each skill's component words (0x00CEF118 ..); `Components()` says how
/// many of them are listed.
const COMPONENTS01: [i32; 11] = [1492, 1482, 1483, 1493, 1484, 1495, 1490, 1491, 1496, 1497, 1481];
const COMPONENTS2: [i32; 19] = [1508, 1509, 1510, 1511, 1512, 1513, 1514, 1515, 1516, 1517, 1518, 1519, 1520, 1521, 1522, 1523, 1524, 1494, 1481];
const COMPONENTS3: [i32; 11] = [1482, 1483, 1484, 1485, 1486, 1487, 1488, 1489, 1490, 1491, 1481];
const COMPONENTS4: [i32; 11] = [1498, 1499, 1500, 1501, 1502, 1503, 1504, 1505, 1506, 1507, 1481];
const COMPONENTS5: [i32; 13] = [1526, 1527, 1528, 1529, 1530, 1531, 1532, 1533, 1534, 1535, 1536, 1537, 1481];

/// The recipe pop-up's entries: (word, message); every one's id is the
/// recipe's.
const RECIPE_MENU: [(i32, u32); 4] = [(1525, 0xA704_0000), (122, 0xA704_0002), (200, 0xA704_0003), (516, 0xA704_0001)];

/// What the book notes where it is drawn, for a pointer: a recipe by its
/// index, a component by `COMP` + its index.
const COMP: i32 = 1000;

/// Who has the focus in a crafting window (`SetActiveWindow`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Focus {
    #[default]
    Main,
    Menu,
    Inspect,
    MBox,
}

fn box_key(key: i32) -> Option<Key> {
    Some(match key {
        13 | 134 => Key::Enter,
        27 | 132 => Key::Escape,
        17 | 136 => Key::Left,
        18 | 137 => Key::Right,
        _ => return None,
    })
}

/// The inspect window's keys: Triangle closes it (true), the arrows scroll.
fn inspect_key(i: &mut Inspect, key: i32, down: bool, cues: &mut Vec<i32>) -> bool {
    if !down {
        match key {
            19 | 138 => i.flash.release(138),
            20 | 139 => i.flash.release(139),
            _ => {}
        }
        return false;
    }
    match key {
        27 | 132 => {
            cues.push(8);
            return true;
        }
        19 | 138 => i.flash.press(138),
        20 | 139 => i.flash.press(139),
        _ => {}
    }
    false
}

/// `VIWndRecipe`.
pub struct RecipeWnd {
    pub skill: i32,
    comp: i32,
    comp_top: i32,
    sel: i32,
    sort: i32,
    top: i32,
    pick: i32,
    /// 0 the components, 1 the recipes, 2 picking a place
    pub mode: i32,
    list: Vec<crate::craft::Recipe>,
    window: Window,
    status: Window,
    keys: KeyFlash,
    menu: MiniMenu,
    inspect: Inspect,
    mbox: MBox,
    focus: Focus,
    dots: i8,
    ticks: i8,
    hits: Hits,
}

impl Default for RecipeWnd {
    fn default() -> Self {
        let mut menu = MiniMenu::fixed(&RECIPE_MENU.map(|(w, m)| (w, m, -1)));
        menu.kind = 1;
        RecipeWnd { skill: 0, comp: 0, comp_top: 0, sel: 0, sort: 0, top: 0, pick: -1, mode: 0, list: Vec::new(), window: Window::default(), status: Window::default(), keys: KeyFlash::default(), menu, inspect: Inspect::default(), mbox: MBox::default(), focus: Focus::Main, dots: 0, ticks: 15, hits: Hits::default() }
    }
}

impl RecipeWnd {
    /// A pointer at frame point `p` (`pointer`), to the window with the
    /// focus: the box's buttons; the popup's rows at once, a click off it
    /// closing it (△); the inspect window none (its legend is glyphs, the
    /// wheel scrolls it). In the book: a component selected (`MOVED`, the
    /// recipe back to the first, as the keys' move does), the selected one's
    /// recipes entered (✕); from the recipes a component is back to the
    /// components (△) on it. A recipe selected (from the components, the
    /// recipes entered on it: ✕), the selected one's popup or, picking a
    /// place, the place taken (✕); while a place is picked the components
    /// wait. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.focus {
            Focus::MBox => return self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Focus::Inspect => return Vec::new(),
            Focus::Menu => {
                return match self.menu.point(p) {
                    Some(k) => k.into_iter().collect(),
                    None => vec![key::TRIANGLE],
                };
            }
            Focus::Main => {}
        }
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        if h >= COMP {
            let c = h - COMP;
            let mut keys = Vec::new();
            if self.mode == 2 {
                return keys;
            }
            if c == self.comp && self.mode == 0 {
                return vec![key::CROSS];
            }
            if c != self.comp {
                self.comp = c;
                self.sel = 0;
                keys.push(MOVED);
            }
            if self.mode == 1 {
                keys.push(key::TRIANGLE);
            }
            return keys;
        }
        match self.mode {
            0 => {
                self.sel = h;
                vec![key::CROSS]
            }
            _ if h == self.sel => vec![key::CROSS],
            _ => {
                self.sel = h;
                vec![MOVED]
            }
        }
    }
    /// `OnMessage` 295 from outside its own windows: the windows, the
    /// children down, everything back to the first component.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 400, 12, 0);
        self.window.open(true);
        self.status.init(vui, r, [320, 180], 310, 3, 0);
        self.status.close(true);
        self.inspect.visible = false;
        self.menu.visible = false;
        self.list.clear();
        self.sel = 0;
        self.sort = 0;
        self.comp = 0;
        self.mode = 0;
        self.top = 0;
        self.comp_top = 0;
        self.dots = 0;
        self.pick = -1;
        self.ticks = 15;
        self.keys.reset();
        self.focus = Focus::Main;
    }

    /// `Components()`: how many the skill lists.
    fn components(&self) -> i32 {
        match self.skill {
            2 => 19,
            5 => 13,
            _ => 11,
        }
    }

    /// `ComponentName(i)`: "" (413) out of range.
    fn component_name(&self, vui: &Vui, i: i32) -> Vec<u16> {
        if i < 0 || i >= self.components() {
            return word(vui, 413);
        }
        let tab: &[i32] = match self.skill {
            0 | 1 => &COMPONENTS01,
            2 => &COMPONENTS2,
            3 => &COMPONENTS3,
            4 => &COMPONENTS4,
            5 => &COMPONENTS5,
            _ => return word(vui, 413),
        };
        word(vui, tab[i as usize])
    }

    /// `Difficulty(d)`: C, UC, R, UR.
    fn difficulty(vui: &Vui, d: u8) -> Vec<u16> {
        match d {
            0..=3 => word(vui, 1648 + d as i32),
            _ => word(vui, 413),
        }
    }

    /// `SkillName(skill)`.
    fn skill_name(vui: &Vui, skill: i32) -> Vec<u16> {
        word(
            vui,
            match skill {
                1 => 1583,
                0 => 1584,
                2 => 1586,
                3 => 1585,
                4 => 1587,
                5 => 179,
                _ => 413,
            },
        )
    }

    /// `Message(text, x)`: a one-button box (the item icon is not drawn).
    pub fn message(&mut self, text: &[u16], now: i64) {
        self.menu.visible = false;
        self.inspect.visible = false;
        self.mbox.prompt(text, 0, -1, -1, now);
        self.focus = Focus::MBox;
    }

    /// `Ask(text, p1, p2)`: a yes/no box; 268 comes back with them.
    fn ask(&mut self, text: &[u16], p1: i32, p2: i32, now: i64) {
        self.menu.visible = false;
        self.inspect.visible = false;
        self.mbox.prompt(text, 1, p1, p2, now);
        self.focus = Focus::MBox;
    }

    /// `Inspect()`: the detail described, the inspect window up.
    pub fn inspect(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Craft) {
        let text = describe_recipe(vui, part);
        self.inspect.set(1, -1, &String::from_utf16_lossy(&text));
        self.inspect.visible = true;
        self.inspect.focus(vui, r);
        self.focus = Focus::Inspect;
    }

    fn prev(&mut self, cues: &mut Vec<i32>) {
        let mut snd = 11;
        if self.mode == 0 {
            self.sel = 0;
            if self.comp <= 0 {
                self.comp = (self.components() - 1).max(0);
            } else {
                self.comp -= 1;
                if self.comp == 0 {
                    self.keys.reset();
                }
            }
        } else if self.sel <= 0 {
            self.sel = 0;
            snd = 9;
        } else {
            self.sel -= 1;
        }
        cues.push(snd);
    }

    fn next(&mut self, cues: &mut Vec<i32>) {
        let mut snd = 11;
        if self.mode == 0 {
            self.sel = 0;
            if self.comp >= self.components() - 1 {
                self.comp = 0;
            } else {
                self.comp += 1;
                if self.comp == self.components() - 1 {
                    self.keys.reset();
                }
            }
        } else if self.sel >= self.list.len() as i32 - 1 {
            self.sel = self.list.len() as i32 - 1;
            snd = 9;
        } else {
            self.sel += 1;
        }
        cues.push(snd);
    }

    /// `Select()`: components -> recipes -> the recipe's menu -> the place
    /// picked; nothing while a request is out.
    fn select(&mut self, part: &mut Craft, now: i64, cues: &mut Vec<i32>, ups: &mut Vec<CraftUp>) {
        let mut snd = 13;
        if part.pending == 0 {
            match self.mode {
                0 => {
                    if !self.list.is_empty() {
                        self.mode = 1;
                    } else {
                        snd = 9;
                    }
                }
                1 => {
                    if !self.list.is_empty() {
                        self.show_menu(now);
                    }
                }
                2 => {
                    if let Some(rec) = self.list.get(self.sel.max(0) as usize) {
                        // Pick
                        if self.pick != rec.id {
                            ups.extend(part.send3(self.pick, rec.id).map(|m| CraftUp::Out(CraftOut::Send(m))));
                        }
                        self.mode = 1;
                    }
                }
                _ => {}
            }
        } else {
            snd = 9;
        }
        cues.push(snd);
    }

    /// `ShowMenu()`: the pop-up on the recipe, titled with its name, every
    /// entry carrying its id.
    fn show_menu(&mut self, now: i64) {
        let Some(rec) = self.list.get(self.sel.max(0) as usize) else { return };
        let mut menu = MiniMenu::fixed(&RECIPE_MENU.map(|(w, m)| (w, m, rec.id)));
        menu.kind = 1;
        menu.title = rec.name.clone();
        menu.list.pos = [170, 100];
        menu.reset_scroll();
        menu.visible = true;
        menu.focus(now);
        self.menu = menu;
        self.inspect.visible = false;
        self.focus = Focus::Menu;
    }

    /// `OnKeyDown`/`OnKeyUp`, or the focused child's.
    pub fn key(&mut self, vui: &Vui, part: &mut Craft, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Vec<CraftUp> {
        let mut ups = Vec::new();
        match self.focus {
            Focus::MBox => {
                if !down {
                    return ups;
                }
                let Some(k) = box_key(key) else { return ups };
                let Some(a) = self.mbox.key(k, cues) else { return ups };
                // 268: Remove's yes sends 1; 260 just closes
                if let BoxAnswer::Yes(p1, p2) = a
                    && p1 == 0xA704_0001u32 as i32
                {
                    ups.extend(part.send1(p2).map(|m| CraftUp::Out(CraftOut::Send(m))));
                }
                self.focus = Focus::Main;
            }
            Focus::Inspect => {
                if inspect_key(&mut self.inspect, key, down, cues) {
                    self.inspect.visible = false;
                    self.focus = Focus::Main;
                }
            }
            Focus::Menu => match self.menu.key(&Menus::default(), key, down, cues) {
                ListUp::Back => {
                    self.menu.visible = false;
                    self.focus = Focus::Main;
                }
                ListUp::Action(msg, id) => {
                    // 261
                    self.menu.visible = false;
                    cues.push(13);
                    self.focus = Focus::Main;
                    match msg {
                        // Create Item: Send2(id, 0)
                        0xA704_0000 => ups.extend(part.send2(id, 0).map(|m| CraftUp::Out(CraftOut::Send(m)))),
                        // Remove: asks first (1590 with the name, 1591)
                        0xA704_0001 => {
                            let text = match part.find(id) {
                                Some(i) => printf16(&word(vui, 1590), &[Arg::S(&part.recipes[i].name)], 256),
                                None => printf16(&word(vui, 1591), &[], 256),
                            };
                            self.ask(&text, 0xA704_0001u32 as i32, id, now);
                        }
                        // Inspect: the detail asked for (Send7)
                        0xA704_0002 => ups.extend(part.send7(id).map(|m| CraftUp::Out(CraftOut::Send(m)))),
                        // Arrange: pick the place
                        0xA704_0003 => {
                            self.pick = id;
                            self.mode = 2;
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            Focus::Main => {
                if !down {
                    match key {
                        19 | 138 => self.keys.release(138),
                        20 | 139 => self.keys.release(139),
                        _ => {}
                    }
                    return ups;
                }
                match key {
                    19 | 138 => self.keys.press(138),
                    20 | 139 => self.keys.press(139),
                    // Left / Right: empty
                    13 | 134 => self.select(part, now, cues, &mut ups),
                    27 | 132 => {
                        // Back: a step, or close (without the part's Clear)
                        match self.mode {
                            0 => ups.push(CraftUp::Close),
                            1 => self.mode = 0,
                            2 => self.mode = 1,
                            _ => {}
                        }
                        cues.push(8);
                    }
                    _ => {}
                }
            }
        }
        ups
    }

    /// `OnDrawSelf`, the children over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Craft, player: &str, now: i64, cues: &mut Vec<i32>) {
        self.list = part.select(self.skill, self.comp);
        if self.list.is_empty() {
            self.mode = 0;
        }
        self.mbox.pos = [160, 115];
        let white = [1.0; 4];
        self.hits.clear();
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
        let head = printf16(
            &wide("%s: %S %d, %S %d %S"),
            &[Arg::S(&wide(player)), Arg::S(&word(vui, 503)), Arg::D(part.slot04(self.skill) as i64), Arg::S(&word(vui, 537)), Arg::D(part.slot08(self.skill) as i64), Arg::S(&Self::skill_name(vui, self.skill))],
            256,
        );
        let hw = vui.string_width(r, &head) + 64;
        vui.blit_bar(r, &head, white, [320 - hw / 2, 10], 33, 34, hw, 0, 1.0, 0, true);
        // the recipes
        self.window.init(vui, r, [410, 180], 430, 14, 0);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        let width = self.window.size[0];
        let cols = [35, 305, 345];
        let c0 = UI_COLORS[0];
        let buf = printf16(&wide("Component: %S"), &[Arg::S(&self.component_name(vui, self.comp))], 256);
        self.window.text_box(vui, r, &buf, cols[0], 0, width - 60, 1, c0, 2);
        self.window.text_box(vui, r, &word(vui, 509), cols[0], 1, cols[1] - cols[0], 1, c0, 0);
        if self.sort == 1 {
            let b = printf16(&wide("|}%S|{"), &[Arg::S(&word(vui, 299))], 256);
            self.window.text_box(vui, r, &b, cols[1] - 12, 1, cols[2] - cols[1] + 12, 1, c0, 0);
        } else {
            self.window.text_box(vui, r, &word(vui, 299), cols[1], 1, cols[2] - cols[1], 1, c0, 0);
        }
        if self.sort == 2 {
            let b = printf16(&wide("|}%S|{"), &[Arg::S(&word(vui, 1542))], 256);
            self.window.text_box(vui, r, &b, cols[2] - 12, 1, 115, 1, c0, 0);
        } else {
            self.window.text_box(vui, r, &word(vui, 1542), cols[2], 1, 115, 1, c0, 0);
        }
        let n = self.list.len() as i32;
        let last = (n - 1).max(0);
        self.sel = last.min(self.sel.max(0));
        if self.sel - self.top >= 12 {
            self.top = self.sel - 11;
        } else if self.sel < self.top {
            self.top = self.sel;
        } else if self.top > n - 12 {
            self.top = (self.top - 1).max(0);
        }
        let t = if n == 12 { 0.0 } else { (self.top as f32 / (n - 12) as f32).clamp(0.0, 1.0) };
        self.window.draw_frame_vscroll(vui, r, 22, 2, width - 55, 12, t);
        let mut i = self.top;
        let mut row = 2;
        while i < n && row < 14 {
            let rec = &self.list[i as usize];
            let hl = i == self.sel && self.mode == 1;
            if hl {
                self.window.tick_r(vui, r, cols[0], row, self.focus == Focus::Main);
            } else if i == self.sel && self.mode == 2 {
                let mut p = self.window.row_center(vui, r, row);
                p[0] = self.window.top_left()[0] + cols[0];
                p[1] -= 8;
                vui.stretch_blit_ex(r, 61, p, [width - 80, 18], 3, 0.25);
            }
            let c = UI_COLORS[hl as usize];
            self.hits.push(rows_rect(&self.window, vui, r, cols[0], row, width - 55 - cols[0] + 22, 1), i);
            self.window.text_box(vui, r, &rec.name, cols[0], row, cols[1] - cols[0], 1, c, 2);
            self.window.text_box(vui, r, &wide(&rec.level.to_string()), cols[1], row, cols[2] - cols[1], 1, c, 0);
            self.window.text_box(vui, r, &Self::difficulty(vui, rec.difficulty), cols[2], row, 110, 1, c, 2);
            i += 1;
            row += 1;
        }
        // the components
        self.window.init(vui, r, [110, 180], 190, 14, 0);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        let width = self.window.size[0];
        if self.comp - self.comp_top >= 11 {
            self.comp_top = self.comp - 10;
        } else if self.comp < self.comp_top {
            self.comp_top = self.comp;
        }
        let mut i = self.comp_top;
        let mut row = 0;
        while i < self.components() && row < 11 {
            if i == self.comp {
                self.window.tick_r(vui, r, 30, row, self.mode == 0);
            }
            self.hits.push(rows_rect(&self.window, vui, r, 30, row, width - 55, 1), COMP + i);
            self.window.text_box(vui, r, &self.component_name(vui, i), 30, row, width - 55, 1, UI_COLORS[(i == self.comp) as usize], 2);
            i += 1;
            row += 1;
        }
        let t = if self.components() == 11 { 0.0 } else { self.comp_top as f32 / (self.components() - 11) as f32 };
        let sp = [self.window.top_left()[0] + width - 22, self.window.top_left()[1] + 10];
        let h = self.window.bottom_right()[1] - self.window.top_left()[1] - 19;
        self.window.draw_vscroll(vui, r, sp, h, t);
        let legend = if part.pending != 0 { "|-|x%S" } else { "|x%S" };
        self.window.text(vui, r, &printf16(&wide(legend), &[Arg::S(&word(vui, 212))], 256), 20, 12, c0, 0);
        self.window.text(vui, r, &printf16(&wide("|t%S"), &[Arg::S(&word(vui, 161))], 256), 20, 13, c0, 0);
        // "Getting Recipes..." while the list or a detail is out
        if part.pending == 5 || part.pending == 6 {
            self.ticks -= 1;
            if self.ticks <= 0 {
                self.ticks = 15;
                self.dots += 1;
                if self.dots == 3 {
                    self.dots = 0;
                }
            }
            let mut buf = word(vui, if part.pending == 5 { 1668 } else { 1667 });
            for _ in 0..self.dots + 1 {
                buf.push('.' as u16);
            }
            self.status.open(false);
            self.status.draw(vui, r);
            self.status.set_title(Some(&word(vui, 271)));
            let sw = self.status.size[0];
            self.status.text_box(vui, r, &buf, 40, 1, sw - 50, 1, c0, 0);
        } else {
            self.status.close(true);
        }
        if self.keys.pulse(138) {
            self.prev(cues);
        }
        if self.keys.pulse(139) {
            self.next(cues);
        }
        if self.menu.visible {
            self.menu.draw(vui, r, &Menus::default(), now, cues);
        }
        if self.inspect.visible {
            self.inspect.draw(vui, r, cues);
        }
        if self.focus == Focus::MBox {
            self.mbox.draw(vui, r, now);
        }
    }
}

/// `VIInfoText::DescribeRecipe`: the product (and its count), the
/// ingredients (a count of 0 left out), the reagents (equipped ones marked),
/// the product's level and rarity, its stats, the text.
pub fn describe_recipe(vui: &Vui, part: &Craft) -> Vec<u16> {
    let d = &part.detail;
    let w = |id: i32| word(vui, id);
    let mut s = if d.count == 1 { printf16(&wide("%S\n"), &[Arg::S(&d.name)], 4096) } else { printf16(&wide("%S (%d)\n"), &[Arg::S(&d.name), Arg::D(d.count as i64)], 4096) };
    if !d.ingredients.is_empty() {
        s.extend(printf16(&wide("%S:\n"), &[Arg::S(&w(1571))], 512));
        for (name, n) in &d.ingredients {
            if *n != 0 {
                s.extend(printf16(&wide("  %S (%d)\n"), &[Arg::S(name), Arg::D(*n as i64)], 512));
            }
        }
    }
    if !d.tools.is_empty() {
        s.extend(printf16(&wide("%S:\n"), &[Arg::S(&w(1572))], 512));
        for (name, have) in &d.tools {
            if *have != 0 {
                s.extend(printf16(&wide("  %S (%S)\n"), &[Arg::S(name), Arg::S(&w(1573))], 512));
            } else {
                s.extend(printf16(&wide("  %S\n"), &[Arg::S(name)], 512));
            }
        }
    }
    s.extend(printf16(&wide("%S: %d\n"), &[Arg::S(&w(1574)), Arg::D(d.level as i64)], 512));
    if d.rarity <= 3 {
        s.extend(printf16(&wide("%S: %S\n"), &[Arg::S(&w(1542)), Arg::S(&w(1538 + d.rarity as i32))], 512));
    }
    if !d.stats.is_empty() {
        s.extend(printf16(&wide("%S:\n"), &[Arg::S(&w(1575))], 512));
        // AddStats(.., 0, 7, all): "  Stat: v" (unknown ids as numbers)
        for &(id, v) in &d.stats {
            match crate::infotext::stat_word(id as i64) {
                Some(sw) => s.extend(printf16(&wide("  %S: %d\n"), &[Arg::S(&w(sw)), Arg::D(v as i64)], 512)),
                None => s.extend(wide(&format!("{id}: {v}\n"))),
            }
        }
    }
    s.extend(d.desc.iter().copied());
    // the 1024-character buffer
    s.truncate(1023);
    s
}

/// `VIWndExperiment`.
pub struct ExperimentWnd {
    menu: WndMenu,
    menus: Menus,
    window: Window,
    inspect: Inspect,
    mbox: MBox,
    focus: Focus,
}

impl Default for ExperimentWnd {
    fn default() -> Self {
        let mut mbox = MBox::default();
        mbox.kind = 1;
        mbox.pos = [160, 115];
        let mut menu = WndMenu::new([16, 37], 0);
        menu.list.x84 = true;
        ExperimentWnd { menu, menus: Menus::default(), window: Window::default(), inspect: Inspect::default(), mbox, focus: Focus::Menu }
    }
}

impl ExperimentWnd {
    /// A pointer at frame point `p` (`pointer`), to the window with the
    /// focus: the box's buttons; the inspect window none; else the component
    /// menu's rows and popups (`WndMenu::point`). The combine is the
    /// legend's □. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.focus {
            Focus::MBox => self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Focus::Inspect => Vec::new(),
            _ => self.menu.point(p),
        }
    }
    /// `Refresh()`: 40 rows emptied, then one per component in the copy (an
    /// item with `m_p54`, not of type 8), each with its three options.
    fn refresh(&mut self, part: &Craft, icon: &dyn Fn(usize) -> i32) {
        let mut menus = Menus::default();
        let mut rows = vec![MenuEntry::default(); 40];
        let mut row = 0;
        for (i, it) in part.items.items.iter().enumerate() {
            if row >= rows.len() {
                break;
            }
            if it.item.p54 > 0 && it.item.p4c != 8 {
                let id = i as i32;
                let opt = |word: i32, msg: u32| MenuEntry { word, msg, id, icon: -1, number: -1, ..Default::default() };
                let sub = menus.push(Menu { x00: -1, x04: -1, entries: vec![opt(1603, 0xA703_0000), opt(516, 0xA703_0001), opt(122, 0xA703_0002)] });
                let low = it.item.max_hp > 0 && (it.cur_hp as f32 / it.item.max_hp as f32) < 0.25;
                rows[row] = MenuEntry { icon: icon(i), icon2: -1, x08: if low { 1 } else { -1 }, number: it.quantity as i32, word: -1, sub: Some(sub), msg: 0, id };
                row += 1;
            }
        }
        self.menu.menu = menus.push(Menu { x00: -1, x04: -1, entries: rows });
        self.menus = menus;
    }

    /// `Open()`: the window, the rows, the box down, the menu reset and given
    /// the focus.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, part: &Craft, icon: &dyn Fn(usize) -> i32, now: i64) {
        self.window.init(vui, r, [320, 180], 400, 14, 0);
        self.window.open(true);
        self.refresh(part, icon);
        self.focus = Focus::Menu;
        self.menu.reset();
        self.menu.list.set_x64(true);
        self.menu.focus(true, now);
    }

    /// `Notify(text, x)`: a one-button box (the item icon is not drawn).
    pub fn notify(&mut self, text: &[u16], now: i64) {
        self.inspect.visible = false;
        self.mbox.prompt(text, 0, -1, -1, now);
        self.focus = Focus::MBox;
    }

    /// `Ask(text, p1, p2, x)`: a yes/no box.
    pub fn ask(&mut self, text: &[u16], p1: i32, p2: i32, now: i64) {
        self.inspect.visible = false;
        self.mbox.prompt(text, 1, p1, p2, now);
        self.focus = Focus::MBox;
    }

    /// The selected row's item (`m_menu->GetId(m_menu->GetSel())`).
    fn selected(&self) -> i32 {
        self.menu.id(&self.menus, self.menu.list.sel)
    }

    /// The keys: to the box, the inspect window or the menu; square (and Q)
    /// reach the window past the menu and combine; the menu's 260 closes.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, part: &mut Craft, info: &Info, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Vec<CraftUp> {
        let mut ups = Vec::new();
        match self.focus {
            Focus::MBox => {
                if !down {
                    return ups;
                }
                let Some(k) = box_key(key) else { return ups };
                let Some(a) = self.mbox.key(k, cues) else { return ups };
                self.focus = Focus::Menu;
                self.menu.focus(true, now);
                // 268: the discovery's yes scribes it (Send5)
                if let BoxAnswer::Yes(p1, p2) = a
                    && p1 == 0xA703_0004u32 as i32
                {
                    ups.extend(part.send5(p2).map(|m| CraftUp::Out(CraftOut::Send(m))));
                }
            }
            Focus::Inspect => {
                if inspect_key(&mut self.inspect, key, down, cues) {
                    self.inspect.visible = false;
                    self.focus = Focus::Menu;
                    self.menu.focus(true, now);
                }
            }
            _ => {
                let popup = self.menu.sub.as_ref().is_some_and(|s| s.visible);
                if down && !popup && matches!(key, 81 | 113 | 133) {
                    cues.push(13);
                    ups.extend(part.send_combine(&info_inventory(info)).map(|m| CraftUp::Out(CraftOut::Send(m))));
                    return ups;
                }
                match self.menu.key(&self.menus, key, down, now, cues) {
                    ListUp::Back => {
                        cues.push(8);
                        ups.push(CraftUp::Close);
                    }
                    ListUp::Action(msg, id) => {
                        cues.push(13);
                        match msg {
                            0xA703_0000 => ups.extend(part.add_to_combine(id, vui_words(vui)).into_iter().map(CraftUp::Out)),
                            0xA703_0001 => part.remove_from_combine(id),
                            0xA703_0002 => {
                                // Inspect: the selected row's description
                                let sel = self.selected();
                                let probe = Info { inv: &part.items, ..*info };
                                let text = probe.inv_charged(sel);
                                self.inspect.set(1, -1, &text);
                                self.inspect.visible = true;
                                self.inspect.focus(vui, r);
                                self.focus = Focus::Inspect;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        ups
    }

    /// `OnDrawSelf`: the rows refreshed, the title, the selected item's
    /// description, the four combine slots and their names, the legend, the
    /// menu; then the children.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Craft, info: &Info, icon: &dyn Fn(usize) -> i32, now: i64, cues: &mut Vec<i32>) {
        let sel_row = (self.menu.list.sel, self.menu.list.top);
        self.refresh(part, icon);
        (self.menu.list.sel, self.menu.list.top) = sel_row;
        let white = [1.0; 4];
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
        let title = word(vui, 1546);
        let w = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, white, [320 - w / 2, 10], 33, 34, w, 0, 1.0, 0, true);
        self.window.init(vui, r, [300, 180], 290, 14, 0);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        let probe = Info { inv: &part.items, ..*info };
        let desc = wide(&probe.inv_charged(self.selected()));
        let width = self.window.size[0];
        self.window.text_box(vui, r, &desc, 20, 0, width - 40, 14, UI_COLORS[0], 2);
        self.window.init(vui, r, [536, 180], 200, 14, 2);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        let tl = self.window.top_left();
        let mut p = [tl[0] + 14, tl[1] + 19];
        for _ in 0..4 {
            vui.blit_bar3(r, &[], white, p, 53, 55, 54, 175, 0, 1.0, 0, true);
            p[1] += 34;
        }
        let width = self.window.size[0];
        for (row, it) in part.combine.iter().enumerate() {
            self.window.text_box(vui, r, &wide(&it.item.name), 20, row as i32 * 2, width - 40, 1, white, 3);
        }
        let legend = |f: &str, id: i32| printf16(&wide(f), &[Arg::S(&word(vui, id))], 1024);
        self.window.text_box(vui, r, &legend("|x%S", 212), 20, 11, width - 40, 1, white, 1);
        self.window.text_box(vui, r, &legend("|s%S", 1475), 20, 12, width - 40, 1, white, 1);
        self.window.text_box(vui, r, &legend("|t%S", 161), 20, 13, width - 40, 1, white, 1);
        self.menu.draw(vui, r, &self.menus, now, cues);
        if self.inspect.visible {
            self.inspect.draw(vui, r, cues);
        }
        if self.focus == Focus::MBox {
            self.mbox.draw(vui, r, now);
        }
    }
}

/// The client's inventory, which `Combine` copies.
fn info_inventory(info: &Info) -> net::dump::ItemList {
    info.inv.clone()
}

fn vui_words(vui: &Vui) -> &vi::strtab::StringTable {
    &vui.words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::craft::Recipe;
    use crate::pointer::testing::{NullRaster, mid, vui};

    fn recipe(order: i32, id: i32, comp: u8) -> Recipe {
        Recipe { order, id, comp, level: 1, name: wide(&format!("Recipe {id}")), ..Default::default() }
    }

    /// The book: a component selected, the selected one's recipes entered;
    /// a recipe selected, the selected one's popup, whose rows a click uses
    /// and off which a click closes it; from the recipes a component goes
    /// back to the components; picking a place, the components wait; the
    /// box's buttons only while it is up.
    #[test]
    fn a_pointer_takes_the_recipe_book() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut part = Craft { recipes: vec![recipe(0, 10, 0), recipe(1, 11, 0), recipe(2, 12, 0), recipe(0, 20, 3), recipe(1, 21, 3)], ..Default::default() };
        let mut w = RecipeWnd::default();
        w.open(&vui, &r);
        let mut cues = Vec::new();
        w.draw(&vui, &mut r, &part, "X", 0, &mut cues);
        // the components' window ([110, 180], 190 wide) and the recipes'
        // ([410, 180], 430 wide), as the draw sets them up
        let mut comps = Window::default();
        comps.init(&vui, &NullRaster, [110, 180], 190, 14, 0);
        comps.set_title(Some(&[]));
        let mut recs = Window::default();
        recs.init(&vui, &NullRaster, [410, 180], 430, 14, 0);
        recs.set_title(Some(&[]));
        let comp = |i: i32| mid(rows_rect(&comps, &vui, &NullRaster, 30, i, 135, 1));
        let rec = |i: i32| mid(rows_rect(&recs, &vui, &NullRaster, 35, 2 + i, 340, 1));
        assert_eq!(w.point(comp(3)), vec![MOVED]);
        assert_eq!((w.comp, w.sel), (3, 0));
        w.draw(&vui, &mut r, &part, "X", 0, &mut cues);
        assert!(w.point(rec(1)) == vec![key::CROSS] && w.sel == 1, "a recipe from the components: entered on it");
        w.sel = 0;
        assert_eq!(w.point(comp(0)), vec![MOVED]);
        w.draw(&vui, &mut r, &part, "X", 0, &mut cues);
        assert_eq!(w.point(comp(0)), vec![key::CROSS], "the selected component: its recipes");
        w.key(&vui, &mut part, key::CROSS, true, 0, &mut cues);
        assert_eq!(w.mode, 1);
        w.draw(&vui, &mut r, &part, "X", 0, &mut cues);
        assert_eq!(w.point(rec(2)), vec![MOVED]);
        assert_eq!(w.sel, 2);
        assert!(w.point(rec(3)).is_empty(), "past the three recipes");
        assert_eq!(w.point(rec(2)), vec![key::CROSS], "the selection: its popup");
        assert_eq!(w.point(comp(1)), vec![MOVED, key::TRIANGLE], "a component from the recipes: back on it");
        assert_eq!((w.comp, w.sel), (1, 0));
        // the popup (titled, so its rows are there at once)
        w.comp = 0;
        w.draw(&vui, &mut r, &part, "X", 0, &mut cues);
        w.mode = 1;
        w.sel = 2;
        w.key(&vui, &mut part, key::CROSS, true, 0, &mut cues);
        assert_eq!(w.focus, Focus::Menu);
        w.draw(&vui, &mut r, &part, "X", 100, &mut cues);
        let at = w.menu.list.pos;
        // DrawTitled: the rows from (+58, +48), 19 apart
        assert_eq!(w.point([(at[0] + 60) as f32, (at[1] + 48 + 19 * 2) as f32]), vec![key::CROSS]);
        assert_eq!(w.menu.list.sel, 2);
        assert_eq!(w.point([600.0, 400.0]), vec![key::TRIANGLE], "off the popup: closed");
        // picking a place: the recipes take a click, the components none
        w.focus = Focus::Main;
        w.menu.visible = false;
        w.mode = 2;
        w.draw(&vui, &mut r, &part, "X", 200, &mut cues);
        assert!(w.point(comp(2)).is_empty());
        assert_eq!(w.point(rec(0)), vec![MOVED]);
        assert_eq!(w.point(rec(0)), vec![key::CROSS], "the place taken");
        // the box: its buttons only
        w.message(&wide("Done"), 300);
        w.draw(&vui, &mut r, &part, "X", 300, &mut cues);
        assert!(w.point(rec(1)).is_empty());
        assert_eq!(w.sel, 0);
    }

    /// The experiment: the component menu's rows (a click selects, a second
    /// opens the options), the box's buttons only while it is up.
    #[test]
    fn a_pointer_takes_the_experiment() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut part = Craft::default();
        part.items.items = (0..3)
            .map(|k| {
                let mut it = net::dump::NetItem { quantity: 1, equip_slot: -1, quick_slot: -1, ..Default::default() };
                it.item.p54 = 1;
                it.item.name = format!("Thing {k}");
                it
            })
            .collect();
        let mut w = ExperimentWnd::default();
        w.open(&vui, &r, &part, &|_| 5, 0);
        let words = vi::strtab::StringTable::default();
        let base = [0; 29];
        let attack = Vec::new();
        let info = Info { words: &words, inv: &part.items, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &attack };
        let mut cues = Vec::new();
        w.draw(&vui, &mut r, &part, &info, &|_| 5, 0, &mut cues);
        // style 0: the rows from the studs' corner (no atlas here: (16, 37))
        let row = |i: i32| [40.0, (37 + 44 * i + 20) as f32];
        assert_eq!(w.point(row(1)), vec![MOVED]);
        assert_eq!(w.menu.list.sel, 1);
        assert_eq!(w.point(row(1)), vec![key::CROSS]);
        assert!(w.point(row(3)).is_empty(), "past the three components");
        w.notify(&wide("Nothing"), 0);
        w.draw(&vui, &mut r, &part, &info, &|_| 5, 0, &mut cues);
        assert!(w.point(row(0)).is_empty(), "the box is up");
        assert_eq!(w.menu.list.sel, 1);
    }
}
