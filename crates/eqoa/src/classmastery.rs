//! Class Mastery: the client part that keeps it (`VIClientPart4EC58`,
//! CLIENT `viclientpart4ec58.cpp`: requests out on 0x1401 as a sub-op, the
//! server's token and an argument, one at a time; replies on 0x1402) and its
//! window (`viwndclassmastery.cpp`, VIWndGame mode 40) with the three it
//! opens: the points (`VIWndClassMasteryPoints`, the share of experience
//! that goes to points), a skill opened (`VIWndClassMasteryInspect`, its
//! pages, buying it or switching it) and what a locked one needs
//! (`VIWndClassMasteryRequire`).
//!
//! The window sorts the category's skills into rank chains (a skill's
//! parent is the one whose `x00` its `parent` names) and lists the highest
//! rank owned of each, and the lowest rank available.
//!
//! A pointer (`pointer`) takes them as the keys reach them: the category
//! and the pages and the share are left and right by the halves of their
//! framed value, a skill is selected and the selected one opened, a window
//! over the list is closed by a click off it (△), the message's choices are
//! taken at once.

use crate::pointer::{Hits, MOVED, key};
use crate::startmenu::KeyFlash;
use crate::textlist::{Message, PROMPT_YESNO, TextList};
use net::dump::Mastery;
use net::stream::{Reader, Writer};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `sCategoryWords`: the five categories' names.
const CATEGORY_WORDS: [i32; 5] = [1653, 1654, 1655, 511, 1656];

/// The category that is a Master Class (word 1656), as the disc's records
/// say it. SOE's final live client's records carry a flag of their own for
/// it (`VICMSkill`/`VICMNode` +0x10, the category moved to +0x11); the
/// servers here send the disc's records.
pub const MASTER_CLASS: u8 = 4;

/// `DrawFrame(x, row, width, 1, 0x20)`'s frame and its two side arrows, for
/// a pointer: the left half is the left key, the right half the right key
/// (the arrows' sides; `pointer::Hits` ids are the keys).
fn push_halves(hits: &mut Hits, w: &Window, vui: &Vui, r: &dyn Raster, x: i32, row: i32, width: i32) {
    let h = vui.font_height(r) - 4;
    let at = [w.top_left()[0] + x - 4, w.row_center(vui, r, row)[1] - 7];
    let (x1, y1) = (at[0] + width + 8, at[1] + h + 4);
    let mid = (at[0] + x1) / 2;
    hits.push([at[0] - 16, at[1], mid, y1], key::LEFT);
    hits.push([mid, at[1], x1 + 16, y1], key::RIGHT);
}

/// A window's whole rectangle (a click on it is its own, off it closes it).
fn push_window(hits: &mut Hits, w: &Window) {
    let (tl, br) = (w.top_left(), w.bottom_right());
    hits.push([tl[0], tl[1], br[0], br[1]], -1);
}

/// `VICMSkill`: one skill of the list (sub-op 8).
#[derive(Clone, Debug, Default)]
pub struct Skill {
    pub x00: i32,
    /// `m_x04`: owned (sub-op 5 sets it, 6 clears it)
    pub on: bool,
    /// `m_x05`: known (sub-op 7 and the flag lists)
    pub known: i32,
    pub id: i32,
    /// `m_x0C`: the `x00` of the rank below
    pub parent: i32,
    /// `m_x10`: the category
    pub category: u8,
    /// `m_x14`: the cost
    pub cost: i32,
    pub name: Vec<u16>,
}

impl Skill {
    /// A Master Class (the live client's `VICMSkill` +0x10).
    pub fn master_class(&self) -> bool {
        self.category == MASTER_CLASS
    }

    /// `VICMSkill::Read`.
    fn read(r: &mut Reader) -> Skill {
        Skill {
            x00: r.signed() as i32,
            on: r.u8() != 0,
            known: r.u8() as i32,
            id: r.signed() as i32,
            parent: r.signed() as i32,
            category: r.u8(),
            cost: r.signed() as i32,
            name: r.wide(64).encode_utf16().collect(),
        }
    }
}

/// `VICMNode`: the skill opened (sub-op 9). `id`, `x0c`, `x14` and `xb8`
/// are read and kept, as the game does, though no window shows them.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub id: i32,
    /// owned, known
    pub x04: u8,
    pub x05: u8,
    /// `m_x08`: the skill's id in the list (Send02/Send03's argument)
    pub x08: i32,
    pub x0c: i32,
    pub x10: u8,
    /// `m_x11`: the level required
    pub x11: u8,
    pub x14: i32,
    /// `m_x18`: the points spent required
    pub x18: i32,
    /// `m_x1C..m_x34`: the seven minimum statistics
    pub mins: [i32; 7],
    pub name: Vec<u16>,
    pub xb8: i32,
    pub title: Vec<u16>,
    pub text: Vec<u16>,
    /// (statistic, change)
    pub reqs: Vec<(u8, i32)>,
    /// (ability, 1 adds / 0 removes)
    pub entries: Vec<(Vec<u16>, u8)>,
    /// the masteries required
    pub lines: Vec<Vec<u16>>,
}

/// A zigzag count; `VIArray::Init` refuses a negative one.
fn count(r: &mut Reader) -> Option<usize> {
    let n = r.signed();
    if n < 0 { None } else { Some(n as usize) }
}

fn wide64(r: &mut Reader, n: usize) -> Vec<u16> {
    r.wide(n).encode_utf16().collect()
}

impl Node {
    /// `VICMNode::Read`: None where a list will not allocate.
    fn read(r: &mut Reader) -> Option<Node> {
        let mut n = Node { id: r.signed() as i32, x04: r.u8(), x05: r.u8(), x08: r.signed() as i32, x0c: r.signed() as i32, x10: r.u8(), x11: r.u8(), ..Default::default() };
        n.x14 = r.signed() as i32;
        n.x18 = r.signed() as i32;
        for m in &mut n.mins {
            *m = r.signed() as i32;
        }
        n.name = wide64(r, 64);
        n.xb8 = r.signed() as i32;
        n.title = wide64(r, 64);
        n.text = wide64(r, 256);
        let k = count(r)?;
        n.reqs = (0..k.min(4096)).map(|_| (r.u8(), r.signed() as i32)).collect();
        let k = count(r)?;
        n.entries = (0..k.min(4096)).map(|_| (wide64(r, 64), r.u8())).collect();
        let k = count(r)?;
        n.lines = (0..k.min(4096)).map(|_| wide64(r, 64)).collect();
        Some(n)
    }
}

/// `VIClientPart4EC58`: the MemoryDump's state (`ReadState`), the skill
/// list, the skill opened, and the sub-op waited for.
#[derive(Clone, Debug, Default)]
pub struct Part {
    pub m: Mastery,
    pub skills: Vec<Skill>,
    pub node: Option<Node>,
    pub pending: i32,
}

/// What a 0x1402 asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum PartUp {
    /// sub-op 3: the points rose by this many (words 1627, 1628)
    Points(i32),
    /// sub-op 9: a skill opened, for the window
    Node,
    /// sub-ops 5 and 6: a skill on or off, and the run speed it leaves,
    /// for `SetSpeed`
    Speed(f32),
}

impl Part {
    fn read_stats(&mut self, r: &mut Reader) {
        for v in &mut self.m.stats {
            *v = r.signed();
        }
    }

    fn read_bonuses(&mut self, r: &mut Reader) {
        for v in &mut self.m.bonuses {
            *v = r.signed();
        }
    }

    /// `ReadFlags`: (id, known) pairs for the skills listed.
    fn read_flags(&mut self, r: &mut Reader) {
        let n = r.u32() as i32;
        for _ in 0..n.clamp(0, 4096) {
            let (id, flag) = (r.u32() as i32, r.u32() as i32);
            if let Some(k) = self.find(id) {
                self.skills[k].known = flag;
            }
        }
    }

    /// `Find(id)`.
    pub fn find(&self, id: i32) -> Option<usize> {
        self.skills.iter().position(|s| s.id == id)
    }

    /// `IsKnown(id)`: owned or known.
    pub fn is_known(&self, id: i32) -> bool {
        self.find(id).is_some_and(|k| self.skills[k].known != 0 || self.skills[k].on)
    }

    /// `Recv1402`.
    pub fn recv(&mut self, data: &[u8]) -> Option<PartUp> {
        let mut r = Reader::new(data);
        let op = r.u8();
        match op {
            1 => {
                self.m.p41c = r.u32();
                self.m.x08 = r.u32() as i32 as i64;
                self.pending = 0;
            }
            2 => {
                self.m.p41c = r.u32();
                self.pending = 0;
            }
            3 => {
                let old = self.m.x0c;
                self.m.x0c = r.u32() as i32 as i64;
                self.m.x10 = r.u32() as i32 as i64;
                if old < self.m.x0c {
                    return Some(PartUp::Points((self.m.x0c - old) as i32));
                }
            }
            4 => self.m.x14 = r.u32() as i32 as i64,
            5 | 6 => {
                self.m.p41c = r.u32();
                let id = r.u32() as i32;
                if op == 5 {
                    self.m.x0c = r.u32() as i32 as i64;
                    self.m.x10 = r.u32() as i32 as i64;
                }
                self.m.speed = r.f32();
                self.read_stats(&mut r);
                self.read_bonuses(&mut r);
                self.read_flags(&mut r);
                if let Some(k) = self.find(id) {
                    self.skills[k].on = op == 5;
                }
                self.pending = 0;
                return Some(PartUp::Speed(self.m.speed));
            }
            7 => {
                let (id, flag) = (r.u32() as i32, r.u32() as i32);
                if let Some(k) = self.find(id) {
                    self.skills[k].known = flag;
                }
            }
            8 => {
                // ReadList: the whole list again. `VICMSkill::Read` never
                // fails, so the game adds all n whatever a short message
                // leaves in them; here the list stops at the last skill read
                // whole (go-eqoa-server's captured tree says 96 and holds 43)
                self.m.p41c = r.u32();
                let n = r.u32() as i32;
                self.skills.clear();
                for _ in 0..n.clamp(0, 4096) {
                    let s = Skill::read(&mut r);
                    if r.error {
                        break;
                    }
                    self.skills.push(s);
                }
                self.pending = 0;
            }
            9 => {
                self.m.p41c = r.u32();
                self.node = Node::read(&mut r).filter(|_| !r.error);
                self.pending = 0;
                if self.node.is_some() {
                    return Some(PartUp::Node);
                }
            }
            // SOE's final live client's reset (0x012F7348, a sub-op the
            // disc's table has no entry for): the token, the points available
            // and spent, the skills taken away (a `VIPool<int>`, a zigzag
            // count and zigzag ids) made not owned, the run speed, the stats,
            // bonuses and flags; no text. go-eqoa-server resets with the
            // disc's 6 and 3 instead, which every client reads
            10 => {
                self.m.p41c = r.u32();
                self.m.x0c = r.u32() as i32 as i64;
                self.m.x10 = r.u32() as i32 as i64;
                let n = r.signed();
                let ids: Vec<i32> = (0..n.clamp(0, 4096)).map(|_| r.signed() as i32).collect();
                self.m.speed = r.f32();
                self.read_stats(&mut r);
                self.read_bonuses(&mut r);
                self.read_flags(&mut r);
                for id in ids {
                    if let Some(k) = self.find(id) {
                        self.skills[k].on = false;
                    }
                }
                self.pending = 0;
                return Some(PartUp::Speed(self.m.speed));
            }
            _ => {}
        }
        None
    }

    fn request(&mut self, sub: u8, arg: Option<i32>) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        let mut w = Writer::op(0x1401);
        w.u8(sub).u32(self.m.p41c);
        if let Some(a) = arg {
            w.u32(a as u32);
        }
        self.pending = sub as i32;
        Some(w.data)
    }

    /// `Select(n)`: the share of experience, 0..100 (sub-op 1).
    pub fn select(&mut self, n: i32) -> Option<Vec<u8>> {
        self.request(1, Some(n.clamp(0, 100)))
    }

    /// `Send02(x)`: buy the skill.
    pub fn send02(&mut self, x: i32) -> Option<Vec<u8>> {
        self.request(2, Some(x))
    }

    /// `Send03(x)`: an owned skill switched.
    pub fn send03(&mut self, x: i32) -> Option<Vec<u8>> {
        self.request(3, Some(x))
    }

    /// `Open`: `Close`, then the list asked for (sub-op 4).
    pub fn open(&mut self) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        self.close();
        self.request(4, None)
    }

    /// `Browse(id)`: one skill opened (sub-op 5), the node cleared first.
    pub fn browse(&mut self, id: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        self.node = None;
        self.request(5, Some(id))
    }

    /// `Close`: the list and the node emptied.
    pub fn close(&mut self) {
        self.skills.clear();
        self.node = None;
    }
}

/// `VIWndClassMastery::Chain`: one rank chain moved from `input` to the
/// result, root first (the skill whose parent is not in the list, then
/// each child in turn); None when there is no root.
fn chain(skills: &[Skill], input: &mut Vec<usize>) -> Option<Vec<usize>> {
    let root = input.iter().position(|&a| !input.iter().any(|&b| skills[a].parent == skills[b].x00))?;
    let mut out = vec![input.remove(root)];
    let mut id = skills[out[0]].x00;
    while let Some(k) = input.iter().position(|&b| skills[b].parent == id) {
        let b = input.remove(k);
        id = skills[b].x00;
        out.push(b);
    }
    Some(out)
}

/// Which of the windows has the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Focus {
    #[default]
    Main,
    Points,
    Inspect,
    Require,
}

/// What the windows ask of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum CmUp {
    /// `ClosePanel6C0`: back to the start menu
    Close,
    /// a 0x1401 request
    Send(Vec<u8>),
    /// `AddText(T(word), (1, .25, .25, 1))`: a refusal, red in the chat
    Warn(i32),
}

/// The page kinds of the inspector (0 text, 1 stats, 2 adds, 3 removes)
/// and of the requirements (1 general, 2 masteries, 3 abilities, 4 stats).
#[derive(Default)]
struct Pages {
    window: Window,
    list: TextList,
    keys: KeyFlash,
    pages: Vec<i32>,
    page: usize,
}

/// `ShowPage`'s statistic names (the inspector's, `T("...")`).
const STAT_NAMES: [&str; 29] = ["STR", "STA", "AGI", "DEX", "WIS", "INT", "CHA", "STR MAX", "STA MAX", "AGI MAX", "DEX MAX", "WIS MAX", "INT MAX", "CHA MAX", "HP MAX", "PWR MAX", "HOT", "POT", "AC", "OFF MOD", "DEF MOD", "HP FACTOR", "FIRE RESIST", "LIGHTNING RESIST", "COLD RESIST", "ARCANE RESIST", "POISON RESIST", "DISEASE RESIST", "MOVEMENT RATE"];

impl Pages {
    /// `ShowPage(page)` of either window.
    fn show_page(&mut self, vui: &Vui, r: &dyn Raster, node: &Node, page: i32, require: bool) {
        if page < 0 || page as usize >= self.pages.len() {
            return;
        }
        self.page = page as usize;
        self.list.clear();
        let cols = [0, 220];
        match (self.pages[self.page], require) {
            (0, _) => self.list.add_text(vui, r, &node.text),
            (1, false) => {
                for &(stat, v) in &node.reqs {
                    let sign = if v < 0 { "" } else { "+" };
                    // a statistic past 28 adds no line: the game's `line` is stale there
                    let Some(name) = STAT_NAMES.get(stat as usize) else { continue };
                    let line = self.list.add_line(&wide(name));
                    self.list.set_columns(line, &cols);
                    self.list.set_text(line, 1, &wide(&format!("{sign}{v}")));
                }
            }
            (1, true) => {
                self.list.add_line(&wide(&format!("Level: {}", node.x11)));
            }
            (2, false) => {
                for (t, _) in node.entries.iter().filter(|e| e.1 == 1) {
                    self.list.add_line(t);
                }
            }
            (2, true) => {
                for t in &node.lines {
                    self.list.add_line(t);
                }
            }
            (3, _) => {
                for (t, _) in node.entries.iter().filter(|e| e.1 == 0) {
                    self.list.add_line(t);
                }
            }
            (4, true) => {
                let names = ["STR:", "STA:", "AGI:", "DEX:", "WIS:", "INT:", "CHA:"];
                let vals = node.mins.iter().zip(names).map(|(&v, n)| (v, n)).chain(std::iter::once((node.x18, "Points Spent:")));
                for (v, n) in vals {
                    if v != 0 {
                        let line = self.list.add_line(&wide(n));
                        self.list.set_columns(line, &cols);
                        self.list.set_text(line, 1, &wide(&v.to_string()));
                    }
                }
            }
            _ => {}
        }
    }

    /// `DrawHeader`: the page's name in a frame.
    fn draw_header(&self, vui: &Vui, r: &mut dyn Raster, require: bool) {
        let w = self.window.size[0];
        self.window.draw_frame(vui, r, 40, 1, w - 80, 1, 32);
        let name = match (self.pages.get(self.page).copied().unwrap_or(-1), require) {
            (0, _) => "Description",
            (1, false) => "Modifies Statistics",
            (2, false) => "Adds Abilities",
            (3, false) => "Removes Abilities",
            (1, true) => "General Requirements",
            (2, true) => "Required Masteries",
            (3, true) => "Required Abilities",
            (4, true) => "Minimum Statistics",
            (5, true) => "Faction Standing",
            _ => "",
        };
        self.window.text_box(vui, r, &wide(name), 40, 1, w - 80, 1, UI_COLORS[0], 8);
    }
}

/// `VIWndClassMastery` and its three windows.
#[derive(Default)]
pub struct ClassMastery {
    category: i32,
    window: Window,
    owned: TextList,
    available: TextList,
    keys: KeyFlash,
    column: i32,
    busy: bool,
    dots: i8,
    ticks: i8,
    /// `sSkills`: the ids shown, [0] owned, [1] available
    shown: [Vec<i32>; 2],
    focus: Focus,
    /// `VIWndClassMasteryPoints`
    points: Window,
    points_keys: KeyFlash,
    percent: i32,
    inspect: Pages,
    message: Message,
    require: Pages,
    /// the category's halves; the window over the list (-1) and its halves
    hits: Hits,
    child_hits: Hits,
}

impl ClassMastery {
    /// A pointer at frame point `p` (`pointer`), to the window with the
    /// keys: in the list (not while a request is answered) the category's
    /// halves are left and right, a skill is selected (`MOVED`) and the
    /// selected one opened (✕); in the points or a skill's window the framed
    /// value's halves (the share, the pages) are left and right, a click off
    /// the window closes it (△); the message takes a click on a choice. The
    /// keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.focus {
            Focus::Main => {
                if self.busy {
                    return Vec::new();
                }
                if let Some(k) = self.hits.at(p) {
                    return vec![k];
                }
                let list = if self.column == 0 { &mut self.owned } else { &mut self.available };
                match list.line_at(p) {
                    Some(i) if i == list.sel => vec![key::CROSS],
                    Some(i) => {
                        list.set_sel(i);
                        vec![MOVED]
                    }
                    None => Vec::new(),
                }
            }
            Focus::Inspect if self.message.visible => self.message.point(p).into_iter().collect(),
            _ => match self.child_hits.at(p) {
                Some(-1) => Vec::new(),
                Some(k) => vec![k],
                None => vec![key::TRIANGLE],
            },
        }
    }
    /// `Refresh`, then `OnMessage` 295 (the focus from outside): the window,
    /// the two lists, the first category.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, part: &Part) {
        self.busy = false;
        self.focus = Focus::Main;
        self.message.visible = false;
        self.window.init(vui, r, [320, 180], 480, 14, 0);
        self.window.set_title(Some(&wide("Class Mastery")));
        self.window.open(true);
        self.owned.init(400, 7);
        self.available.init(400, 7);
        self.inspect.list.init(320, 6);
        self.require.list.init(320, 6);
        self.set_column(0);
        self.keys.reset();
        self.select_category(part, 0);
        self.dots = 0;
        self.ticks = 15;
    }

    /// `SetColumn`: the selection shows in the list with the focus.
    fn set_column(&mut self, c: i32) {
        self.column = c;
        self.owned.show_sel = c == 0;
        self.available.show_sel = c == 1;
    }

    /// `SelectCategory`: owned skills into the first list, known ones and
    /// then the rest into the second, each list chained by rank.
    fn select_category(&mut self, part: &Part, category: i32) {
        self.category = category;
        let sk = &part.skills;
        let in_cat = |s: &Skill| s.category as i32 == category;
        let mut lists = [Vec::new(), Vec::new()];
        for (i, s) in sk.iter().enumerate() {
            if s.on {
                if in_cat(s) {
                    lists[0].push(i);
                }
            } else if part.is_known(s.id) && in_cat(s) {
                lists[1].push(i);
            }
        }
        for (i, s) in sk.iter().enumerate() {
            if !s.on && !part.is_known(s.id) && in_cat(s) {
                lists[1].push(i);
            }
        }
        self.shown = [Vec::new(), Vec::new()];
        while let Some(c) = chain(sk, &mut lists[0]) {
            self.shown[0].push(sk[*c.last().unwrap()].id);
        }
        while let Some(c) = chain(sk, &mut lists[1]) {
            self.shown[1].push(sk[c[0]].id);
        }
        self.shown[0].truncate(200);
        self.shown[1].truncate(200);
        self.owned.clear();
        for id in &self.shown[0] {
            let k = part.find(*id).unwrap();
            self.owned.add_line(&sk[k].name);
        }
        self.available.clear();
        for id in &self.shown[1] {
            let s = &sk[part.find(*id).unwrap()];
            let (a, b) = if part.is_known(s.id) { (s.name.clone(), wide(&s.cost.to_string())) } else { (wide("???"), wide("Locked")) };
            let line = self.available.add_line(&a);
            self.available.set_columns(line, &[0, 330]);
            self.available.set_text(line, 1, &b);
        }
        self.owned.set_sel(0);
        self.available.set_sel(0);
    }

    /// `ShowNode`: the client opened a skill; the inspector shows an owned
    /// or known one, the requirements one that is not known yet. False when
    /// the window is not up (the node is dropped).
    pub fn show_node(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Part, up: bool) -> bool {
        let Some(node) = part.node.as_ref() else { return false };
        if !up {
            return false;
        }
        let list = if self.column == 0 { &self.owned } else { &self.available };
        if list.sel == -1 {
            return true;
        }
        if self.column == 0 || part.is_known(node.x08) {
            // Show, then 295: "Inspect: <title>"
            let p = &mut self.inspect;
            p.pages = vec![0];
            if !node.reqs.is_empty() {
                p.pages.push(1);
            }
            if node.entries.iter().any(|e| e.1 == 1) {
                p.pages.push(2);
            }
            if node.entries.iter().any(|e| e.1 != 1) {
                p.pages.push(3);
            }
            p.show_page(vui, r, node, 0, false);
            p.window.init(vui, r, [320, 180], 400, 11, 0);
            let mut t = wide("Inspect: ");
            t.extend(&node.title);
            p.window.set_title(Some(&t));
            p.window.open(true);
            self.focus = Focus::Inspect;
        } else {
            let p = &mut self.require;
            p.pages = vec![1];
            if !node.lines.is_empty() {
                p.pages.push(2);
            }
            if node.entries.iter().any(|e| e.1 == 0) {
                p.pages.push(3);
            }
            if node.mins.iter().any(|&v| v != 0) || node.x18 != 0 {
                p.pages.push(4);
            }
            p.show_page(vui, r, node, 0, true);
            p.window.init(vui, r, [320, 180], 400, 11, 0);
            p.window.set_title(Some(&wide("Requirements")));
            p.window.open(true);
            self.focus = Focus::Require;
        }
        true
    }

    /// The keys, to whichever window has them.
    #[allow(clippy::too_many_arguments)]
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, part: &mut Part, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<CmUp> {
        let up_down = |keys: &mut KeyFlash| match key {
            19 | 138 => {
                if down { keys.press(138) } else { keys.release(138) }
                true
            }
            20 | 139 => {
                if down { keys.press(139) } else { keys.release(139) }
                true
            }
            _ => false,
        };
        match self.focus {
            Focus::Main => {
                if !down {
                    up_down(&mut self.keys);
                    return None;
                }
                if self.busy {
                    return None;
                }
                if up_down(&mut self.keys) {
                    return None;
                }
                match key {
                    17 | 136 => {
                        cues.push(11);
                        self.select_category(part, (self.category + 1) % 5);
                    }
                    18 | 137 => {
                        cues.push(11);
                        self.select_category(part, (self.category + 4) % 5);
                    }
                    9 | 142 => {
                        cues.push(11);
                        self.set_column(1 - self.column);
                    }
                    81 | 113 | 133 => {
                        // the points window, 295: the share starts at the client's
                        self.points.init(vui, r, [320, 180], 400, 9, 0);
                        self.points.set_title(Some(&wide("Class Mastery Points")));
                        self.points.open(true);
                        self.percent = part.m.x08 as i32;
                        self.points_keys.reset();
                        self.focus = Focus::Points;
                        cues.push(8);
                    }
                    13 | 134 => {
                        let (list, ids) = if self.column == 0 { (&self.owned, &self.shown[0]) } else { (&self.available, &self.shown[1]) };
                        if list.sel != -1
                            && let Some(&id) = ids.get(list.sel as usize)
                        {
                            cues.push(15);
                            return part.browse(id).map(CmUp::Send);
                        }
                    }
                    27 | 132 => {
                        cues.push(8);
                        return Some(CmUp::Close);
                    }
                    _ => {}
                }
            }
            Focus::Points => match key {
                18 | 137 if down => self.points_keys.press(137),
                17 | 136 if down => self.points_keys.press(136),
                18 | 137 => self.points_keys.release(137),
                17 | 136 => self.points_keys.release(136),
                13 | 134 if down => {
                    self.focus = Focus::Main;
                    cues.push(8);
                    return part.select(self.percent).map(CmUp::Send);
                }
                27 | 132 if down => {
                    self.focus = Focus::Main;
                    cues.push(8);
                }
                _ => {}
            },
            Focus::Inspect if self.message.visible => {
                if !down {
                    return None;
                }
                // 307: 0 is yes
                let sel = self.message.key(key, now, cues)?;
                let node = part.node.clone()?;
                if sel != 0 {
                    return None;
                }
                let m = if node.x04 != 0 { part.send03(node.x08) } else { part.send02(node.x08) };
                self.focus = Focus::Main;
                cues.push(8);
                return m.map(CmUp::Send);
            }
            Focus::Inspect | Focus::Require => {
                let require = self.focus == Focus::Require;
                let p = if require { &mut self.require } else { &mut self.inspect };
                if up_down(&mut p.keys) || !down {
                    return None;
                }
                let n = p.pages.len() as i32;
                let node = part.node.clone()?;
                match key {
                    17 | 136 => {
                        cues.push(11);
                        p.show_page(vui, r, &node, (p.page as i32 + n - 1) % n, require);
                    }
                    18 | 137 => {
                        cues.push(11);
                        p.show_page(vui, r, &node, (p.page as i32 + 1) % n, require);
                    }
                    13 | 134 if !require => {
                        let w = |id: i32| wide(vui.words.get(id, vui.language));
                        let center = self.inspect.window.center;
                        if node.x04 != 0 {
                            self.message.setup(vui, r, &w(1665), &w(1666), PROMPT_YESNO, center, 300);
                            cues.push(15);
                        } else if node.x05 != 0 {
                            // SOE's final live client's rule (OnKeyDown,
                            // 0x01338978): a Master Class while another is
                            // owned is refused, word 1840 red in the chat, with
                            // no box, no sound and nothing sent. The disc's
                            // only warned (1807, as the box below still does)
                            if node.x10 == MASTER_CLASS && part.skills.iter().any(|s| s.master_class() && s.on) {
                                return Some(CmUp::Warn(1840));
                            }
                            let text = if node.x10 == MASTER_CLASS { wide(&format!("{} {}", vui.words.get(1807, vui.language), vui.words.get(1664, vui.language))) } else { w(1664) };
                            self.message.setup(vui, r, &w(1663), &text, PROMPT_YESNO, center, 350);
                            cues.push(15);
                        }
                    }
                    27 | 132 => {
                        self.focus = Focus::Main;
                        cues.push(8);
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// `OnDrawSelf`, and the windows over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Part, cues: &mut Vec<i32>) {
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        let c0 = UI_COLORS[0];
        self.hits.clear();
        self.child_hits.clear();
        crate::ui::window_backdrop(vui, r, 641);
        let w = self.window.size[0];
        self.window.draw(vui, r);
        self.window.text(vui, r, &wide("Points Available/Spent:"), 40, 1, c0, 0);
        self.window.text(vui, r, &wide(&format!("{}/{}", part.m.x0c, part.m.x10)), 260, 1, c0, 0);
        self.window.text(vui, r, &wide("Category:"), 40, 2, c0, 0);
        self.window.draw_frame(vui, r, 260, 2, w - 300, 1, 32);
        push_halves(&mut self.hits, &self.window, vui, r, 260, 2, w - 300);
        self.window.text_box(vui, r, &wide(&word(CATEGORY_WORDS[self.category as usize])), 260, 2, w - 300, 1, c0, 8);
        // DrawLists
        if self.column == 0 {
            self.owned.draw(vui, r, &self.window, 40, 5);
            self.window.text(vui, r, &wide("Owned (|8)"), 40, 4, c0, 0);
        } else {
            self.available.draw(vui, r, &self.window, 40, 5);
            self.window.text(vui, r, &wide("Available (|8)"), 40, 4, c0, 0);
            self.window.text(vui, r, &wide("Cost"), 370, 4, c0, 0);
        }
        let sel = if self.column == 0 { self.owned.sel } else { self.available.sel };
        let legend = if sel == -1 {
            format!("|t {}  |s {}", word(750), word(1658))
        } else {
            let buy = self.column == 1 && !self.shown[1].get(sel as usize).is_some_and(|&id| part.is_known(id));
            format!("|x {}  |t {}  |s {}", word(if buy { 1659 } else { 1657 }), word(750), word(1658))
        };
        self.window.text(vui, r, &wide(&legend), 40, 13, c0, 0);
        // what the client is waiting for, in a box over the lists
        self.ticks -= 1;
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
        let dots = ".".repeat(self.dots as usize + 1);
        let msg = match part.pending {
            0 => {
                if self.busy {
                    self.select_category(part, self.category);
                    self.busy = false;
                }
                None
            }
            1 => Some((word(1766), word(1766))),
            2 => {
                self.busy = true;
                Some((word(1767), word(1767)))
            }
            3 => {
                self.busy = true;
                Some((word(1768), word(1768)))
            }
            4 => {
                self.busy = true;
                Some((format!("{}{dots}", word(1811)), word(1811)))
            }
            5 => {
                self.busy = false;
                Some((format!("{}{dots}", word(1812)), word(1812)))
            }
            _ => None,
        };
        if let Some((text, measured)) = msg {
            let mut b = Window::default();
            let sw = vui.string_width(r, &wide(&measured));
            b.init(vui, r, self.window.center, sw + 120, 3, 0);
            b.set_title(Some(&wide(&word(271))));
            b.open(true);
            b.draw(vui, r);
            b.text(vui, r, &wide(&text), 40, 1, c0, 0);
        }
        // the held arrows, through the key repeat
        if self.focus == Focus::Main {
            let list = if self.column == 1 { &mut self.available } else { &mut self.owned };
            if self.keys.pulse(138) {
                if list.sel > 0 {
                    cues.push(11);
                    list.set_sel(list.sel - 1);
                } else {
                    cues.push(9);
                }
            }
            if self.keys.pulse(139) {
                if list.count() != 1 {
                    cues.push(11);
                    list.set_sel(list.sel + 1);
                } else {
                    cues.push(9);
                }
            }
        }
        match self.focus {
            Focus::Main => {}
            Focus::Points => self.draw_points(vui, r, part, cues),
            Focus::Inspect | Focus::Require => {
                let require = self.focus == Focus::Require;
                let Some(node) = part.node.as_ref() else {
                    self.focus = Focus::Main;
                    return;
                };
                let p = if require { &mut self.require } else { &mut self.inspect };
                p.window.draw(vui, r);
                push_window(&mut self.child_hits, &p.window);
                let legend = if require {
                    format!("|t {}", word(750))
                } else if node.x04 != 0 {
                    format!("|x {}  |t {}", word(516), word(750))
                } else if node.x05 != 0 {
                    format!("|x {}  |t {}", word(63), word(750))
                } else {
                    format!("|-|x {}|+  |t {}", word(63), word(750))
                };
                p.window.text(vui, r, &wide(&legend), 40, 10, c0, 0);
                p.draw_header(vui, r, require);
                let pw = p.window.size[0];
                push_halves(&mut self.child_hits, &p.window, vui, r, 40, 1, pw - 80);
                p.list.draw(vui, r, &p.window, 40, 3);
                if p.keys.pulse(138) {
                    let end = p.list.scroll(-1);
                    if !require {
                        cues.push(if end { 9 } else { 11 });
                    }
                }
                if p.keys.pulse(139) {
                    let end = p.list.scroll(1);
                    if !require {
                        cues.push(if end { 9 } else { 11 });
                    }
                }
                if self.message.visible {
                    self.message.draw(vui, r);
                }
            }
        }
    }

    /// `VIWndClassMasteryPoints::OnDrawSelf`.
    fn draw_points(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Part, cues: &mut Vec<i32>) {
        let c0 = UI_COLORS[0];
        let w = self.points.size[0];
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        self.points.draw(vui, r);
        push_window(&mut self.child_hits, &self.points);
        self.points.text_box(vui, r, &wide("Available Mastery Points:"), 40, 1, w - 80, 1, c0, 0);
        self.points.text_box(vui, r, &wide(&part.m.x0c.to_string()), 40, 1, w - 80, 1, c0, 16);
        self.points.text_box(vui, r, &wide("Percentage of experience points to apply towards mastery points:"), 40, 3, w - 80, 3, c0, 8);
        self.points.draw_frame(vui, r, 150, 6, w - 300, 1, 32);
        push_halves(&mut self.child_hits, &self.points, vui, r, 150, 6, w - 300);
        self.points.text_box(vui, r, &wide(&format!("{}%", self.percent)), 40, 6, w - 80, 1, c0, 8);
        self.points.text(vui, r, &wide(&format!("|x {}  |t {}", word(786), word(753))), 40, 8, c0, 0);
        for (k, d) in [(136, -1), (137, 1)] {
            if self.points_keys.pulse(k) {
                // Change(d)
                self.percent += d;
                if self.percent < 0 {
                    self.percent = 0;
                    cues.push(9);
                } else if self.percent > 100 {
                    self.percent = 100;
                    cues.push(9);
                } else {
                    cues.push(11);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(x00: i32, parent: i32, id: i32) -> Skill {
        Skill { x00, parent, id, ..Default::default() }
    }

    /// `Chain`: the root is the skill whose parent is not listed, then each
    /// child in turn; two chains come out one at a time; a cycle has no
    /// root.
    #[test]
    fn rank_chains_come_out_root_first() {
        let sk = vec![skill(2, 1, 20), skill(10, 0, 100), skill(1, 0, 10), skill(3, 2, 30), skill(11, 10, 110)];
        let mut input: Vec<usize> = (0..sk.len()).collect();
        let a = chain(&sk, &mut input).unwrap();
        assert_eq!(a.iter().map(|&k| sk[k].id).collect::<Vec<_>>(), vec![100, 110], "the first root in list order");
        let b = chain(&sk, &mut input).unwrap();
        assert_eq!(b.iter().map(|&k| sk[k].id).collect::<Vec<_>>(), vec![10, 20, 30]);
        assert!(chain(&sk, &mut input).is_none());
        let cyc = vec![skill(1, 2, 1), skill(2, 1, 2)];
        assert!(chain(&cyc, &mut vec![0, 1]).is_none(), "every skill has a parent listed");
    }

    /// Sub-ops 8, 7, 5 and the requests: one at a time, the token echoed.
    #[test]
    fn the_part_keeps_the_list_and_one_request() {
        let mut p = Part::default();
        let mut m = vec![8u8];
        m.extend(0x55u32.to_le_bytes());
        m.extend(1u32.to_le_bytes());
        // x00 1, off, unknown, id 7, parent 0, category 2, cost 3, "A"
        m.extend([2, 0, 0, 14, 0, 2, 6]);
        m.extend(1u32.to_le_bytes());
        m.extend([b'A', 0]);
        p.pending = 4;
        assert_eq!(p.recv(&m), None);
        assert_eq!((p.pending, p.m.p41c, p.skills.len(), p.skills[0].id, p.skills[0].cost), (0, 0x55, 1, 7, 3));
        assert!(!p.is_known(7));
        let mut k = vec![7u8];
        k.extend(7u32.to_le_bytes());
        k.extend(1u32.to_le_bytes());
        p.recv(&k);
        assert!(p.is_known(7));
        assert_eq!(p.browse(7), Some(vec![0x01, 0x14, 5, 0x55, 0, 0, 0, 7, 0, 0, 0]));
        assert_eq!(p.select(500), None, "one request at a time");
        p.pending = 0;
        assert_eq!(p.select(500).unwrap()[7..], [100, 0, 0, 0], "clamped to 100");
        m.truncate(m.len() - 1);
        p.pending = 0;
        p.recv(&m);
        assert!(p.skills.is_empty(), "a skill cut short is not kept");
        assert_eq!(p.pending, 0);
    }

    /// Sub-ops 5 and 6 turn a skill on and off and hand the run speed they
    /// carry on to `SetSpeed` (`m_owner->SetSpeed(m_speed)`), Sandstorm's
    /// 8.833333 and Spirit of the Wolf's 11.483333 here; the control is
    /// sub-op 4, which carries no speed and gives nothing.
    #[test]
    fn a_skill_on_or_off_gives_its_run_speed() {
        let mut p = Part::default();
        let msg = |op: u8, speed: u32| {
            let mut m = vec![op];
            m.extend(0x55u32.to_le_bytes());
            m.extend(7u32.to_le_bytes());
            if op == 5 {
                m.extend(3u32.to_le_bytes());
                m.extend(4u32.to_le_bytes());
            }
            m.extend(speed.to_le_bytes());
            // the 29 stats and 7 bonuses, zero each, and no flags
            m.extend([0u8; 36]);
            m.extend(0u32.to_le_bytes());
            m
        };
        assert_eq!(p.recv(&msg(5, 0x4137BBBB)), Some(PartUp::Speed(f32::from_bits(0x4137BBBB))));
        assert_eq!((p.m.x0c, p.m.x10), (3, 4));
        assert_eq!(p.recv(&msg(6, 0x410D5555)), Some(PartUp::Speed(f32::from_bits(0x410D5555))));
        assert_eq!(p.recv(&[4, 1, 0, 0, 0]), None);
    }

    /// The live reset (sub-op 10, 0x012F7348): the token, the points (no
    /// notice), each listed skill off, the run speed for `SetSpeed`, the
    /// stats; the request no longer waited for. The control: a skill not
    /// listed stays owned.
    #[test]
    fn the_live_reset_takes_the_listed_skills_away() {
        let mut p = Part::default();
        for (id, on) in [(7, true), (8, true), (9, true)] {
            p.skills.push(Skill { id, on, ..Default::default() });
        }
        p.pending = 2;
        let mut w = Writer::default();
        w.u8(10).u32(0x66).u32(12).u32(0);
        w.signed(2).signed(7).signed(9);
        w.u32(0x410D5555);
        for v in 0..36 {
            w.signed(v);
        }
        w.u32(0);
        assert_eq!(p.recv(&w.data), Some(PartUp::Speed(f32::from_bits(0x410D5555))));
        assert_eq!((p.m.p41c, p.m.x0c, p.m.x10, p.pending), (0x66, 12, 0, 0));
        assert_eq!(p.skills.iter().map(|s| s.on).collect::<Vec<_>>(), [false, true, false], "7 and 9 taken away, 8 kept");
        assert_eq!((p.m.stats[28], p.m.bonuses[6]), (28, 35));
    }

    /// A pointer on the list, the category, the points window, a skill's
    /// window and its message, each as their keys; nothing while a request
    /// is answered.
    #[test]
    fn a_pointer_takes_the_windows() {
        use crate::pointer::rows_rect;
        use crate::pointer::testing::{NullRaster, mid, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut part = Part::default();
        // three owned skills, each its own chain, and one known
        for k in 0..3 {
            part.skills.push(Skill { x00: k + 1, on: true, id: 10 + k, parent: 100 + k, name: wide(&format!("Skill {k}")), ..Default::default() });
        }
        part.skills.push(Skill { x00: 9, known: 1, id: 19, parent: 109, name: wide("Known"), ..Default::default() });
        let mut c = ClassMastery::default();
        c.open(&vui, &r, &part);
        let mut cues = Vec::new();
        c.draw(&vui, &mut r, &part, &mut cues);
        let line = |c: &ClassMastery, i: i32| mid(rows_rect(&c.window, &vui, &NullRaster, 40, 5 + i, 400, 1));
        assert_eq!(c.point(line(&c, 2)), vec![MOVED]);
        assert_eq!(c.owned.sel, 2);
        assert_eq!(c.point(line(&c, 2)), vec![key::CROSS], "the selection: opened");
        assert!(c.point(line(&c, 3)).is_empty(), "past the three owned");
        // the category's frame (x 260, row 2, 180 wide): its halves and arrows
        let y = c.window.row_center(&vui, &NullRaster, 2)[1] as f32;
        let x0 = (c.window.top_left()[0] + 256) as f32;
        assert_eq!(c.point([x0 + 20.0, y]), vec![key::LEFT]);
        assert_eq!(c.point([x0 - 10.0, y]), vec![key::LEFT], "the left arrow");
        assert_eq!(c.point([x0 + 170.0, y]), vec![key::RIGHT]);
        assert!(c.point([x0 + 20.0, y + 15.0]).is_empty(), "below the frame: the list's header");
        // a request out: nothing
        c.busy = true;
        assert!(c.point(line(&c, 0)).is_empty());
        assert!(c.point([x0 + 20.0, y]).is_empty());
        c.busy = false;
        // the points window: the share's halves, on it nothing, off it closed
        c.key(&vui, &mut r, &mut part, key::SQUARE, true, 0, &mut cues);
        assert_eq!(c.focus, Focus::Points);
        c.draw(&vui, &mut r, &part, &mut cues);
        let py = c.points.row_center(&vui, &NullRaster, 6)[1] as f32;
        let px = (c.points.top_left()[0] + 146) as f32;
        assert_eq!(c.point([px + 10.0, py]), vec![key::LEFT]);
        assert_eq!(c.point([px + 100.0, py]), vec![key::RIGHT]);
        assert!(c.point(mid([c.points.top_left()[0], c.points.top_left()[1], c.points.top_left()[0] + 40, c.points.top_left()[1] + 40])).is_empty());
        assert_eq!(c.point([5.0, 5.0]), vec![key::TRIANGLE]);
        assert_eq!(c.owned.sel, 2, "the list under it untouched");
        // a skill's window: the pages' halves; its message's choices
        c.focus = Focus::Main;
        part.node = Some(Node { x04: 1, x08: 12, text: wide("text"), reqs: vec![(0, 1)], ..Default::default() });
        assert!(c.show_node(&vui, &mut r, &part, true));
        assert_eq!(c.focus, Focus::Inspect);
        c.draw(&vui, &mut r, &part, &mut cues);
        let w = c.inspect.window;
        let hy = w.row_center(&vui, &NullRaster, 1)[1] as f32;
        assert_eq!(c.point([(w.top_left()[0] + 50) as f32, hy]), vec![key::LEFT]);
        assert_eq!(c.point([(w.top_left()[0] + 350) as f32, hy]), vec![key::RIGHT]);
        assert_eq!(c.point([5.0, 400.0]), vec![key::TRIANGLE]);
        c.key(&vui, &mut r, &mut part, key::CROSS, true, 0, &mut cues);
        assert!(c.message.visible);
        c.draw(&vui, &mut r, &part, &mut cues);
        assert!(c.point([5.0, 400.0]).is_empty(), "the message: off it, nothing");
        // its choices (a 300-wide box at the window's middle, three rows):
        // No, then Yes, each taken at once
        let mut mw = Window::default();
        mw.init(&vui, &NullRaster, [320, 180], 300, 3, 0);
        mw.set_title(Some(&[]));
        let (cx, cy) = (mw.top_left()[0] as f32, mw.row_center(&vui, &NullRaster, 2)[1] as f32);
        assert_eq!(c.point([cx + 250.0, cy]), vec![key::CROSS]);
        assert_eq!(c.key(&vui, &mut r, &mut part, key::CROSS, true, 0, &mut cues), None, "No: nothing sent");
        assert!(!c.message.visible);
        c.key(&vui, &mut r, &mut part, key::CROSS, true, 0, &mut cues);
        c.draw(&vui, &mut r, &part, &mut cues);
        assert_eq!(c.point([cx + 60.0, cy]), vec![key::CROSS]);
        assert!(matches!(c.key(&vui, &mut r, &mut part, key::CROSS, true, 0, &mut cues), Some(CmUp::Send(_))), "Yes: the request");
    }

    /// One Master Class at a time (the live client's OnKeyDown, 0x01338978):
    /// buying a Master Class (category 4) while another is owned is refused
    /// with word 1840, no box, no sound, nothing sent. The controls: with
    /// none owned (one only known, or owned in another category) the box
    /// opens with the disc's warning before the question; a skill of
    /// another category opens its plain question even with a Master Class
    /// owned.
    #[test]
    fn one_master_class_at_a_time() {
        use crate::pointer::testing::{NullRaster, vui};
        let vui = vui();
        let mc = |id: i32, on: bool, category: u8| Skill { x00: id, on, id, parent: 1000 + id, category, name: wide("S"), ..Default::default() };
        // an owned General skill, so the first list (General) has a line
        // selected and the node opens in the inspector
        let buy = |mut skills: Vec<Skill>, category: u8| {
            skills.push(mc(99, true, 0));
            let mut part = Part { skills, ..Default::default() };
            part.node = Some(Node { x04: 0, x05: 1, x08: 50, x10: category, text: wide("t"), ..Default::default() });
            let mut c = ClassMastery::default();
            c.open(&vui, &NullRaster, &part);
            assert!(c.show_node(&vui, &mut NullRaster, &part, true));
            let mut cues = Vec::new();
            let up = c.key(&vui, &mut NullRaster, &mut part, key::CROSS, true, 0, &mut cues);
            (up, c.message.visible, cues)
        };
        assert_eq!(buy(vec![mc(1, true, MASTER_CLASS), mc(2, false, 1)], MASTER_CLASS), (Some(CmUp::Warn(1840)), false, vec![]));
        assert_eq!(buy(vec![mc(1, false, MASTER_CLASS)], MASTER_CLASS), (None, true, vec![15]), "only known: the box");
        assert_eq!(buy(vec![mc(1, true, 2)], MASTER_CLASS), (None, true, vec![15]), "owned, another category");
        assert_eq!(buy(vec![mc(1, true, MASTER_CLASS)], 2), (None, true, vec![15]), "not a Master Class");
    }

    /// `VICMNode::Read`: the fields, three strings and three lists; a
    /// negative count (VIArray::Init refuses it) reads as no node.
    #[test]
    fn a_node_reads_its_lists() {
        let mut w = Writer::default();
        w.signed(5).u8(1).u8(0).signed(9).signed(-1).u8(4).u8(20);
        for v in 0..9 {
            w.signed(v);
        }
        w.wide("Name").signed(0).wide("Title").wide("Text");
        w.signed(1).u8(18).signed(-3);
        w.signed(2).wide("Add").u8(1).wide("Remove").u8(0);
        w.signed(1).wide("Vigor 2");
        let mut m = vec![9u8, 7, 0, 0, 0];
        m.extend(&w.data);
        let mut p = Part::default();
        assert_eq!(p.recv(&m), Some(PartUp::Node));
        let n = p.node.as_ref().unwrap();
        assert_eq!((n.id, n.x04, n.x08, n.x11, n.x18, n.mins[6]), (5, 1, 9, 20, 1, 8));
        assert_eq!((n.reqs.clone(), n.entries.len(), n.entries[1].1, n.lines.len()), (vec![(18, -3)], 2, 0, 1));
        assert_eq!(String::from_utf16_lossy(&n.title), "Title");
        // up to the masteries' count (one byte; "Vigor 2" is 18), made -1
        let mut bad = m[..m.len() - 18].to_vec();
        let k = bad.len();
        assert_eq!(bad[k - 1], 2);
        bad[k - 1] = 1;
        assert_eq!(p.recv(&bad), None);
        assert!(p.node.is_none());
    }
}
