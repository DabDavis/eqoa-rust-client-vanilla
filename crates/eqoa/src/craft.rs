//! Tradeskills (crafting), as CLIENT keeps them: `VIClientPart3834C`
//! (`viclientpart3834c.cpp`, VIClient+0x3834C) — the six skill slots the
//! MemoryDump and the replies carry, the server's token, the recipe book, the
//! recipe opened, the inventory as the combine sees it and the combine itself —
//! with requests on 0x1101 (a sub-op byte, the token, arguments) and replies
//! on 0x1102. The windows are `VIWndRecipe` (the recipe book, VIWndGame mode
//! 6) and `VIWndExperiment` (mode 37), in `craftwnd.rs`.
//!
//! The start menu's Character > Tradeskills holds Experiment (0xA7020000)
//! and Recipes, one entry a skill (0xA7010000 with the skill as its id), which
//! `Use` takes. The sub-ops, by what the windows do with them: 1 remove a
//! recipe ("Remove", after a yes), 2 create (the menu's "Create Item"), 3 move
//! a recipe ("Arrange", then the place picked), 4 combine (the experiment's
//! items, zigzag), 5 scribe a discovery (the experiment's yes), 6 the skill's
//! list (on opening), 7 a recipe's detail ("Inspect").

use crate::inventory;
use net::dump::{CraftSlot, ItemList, NetItem};
use net::stream::{Reader, Writer};
use vi::strtab::StringTable;

/// `VIRec144_C017C0`: a recipe in the book.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    pub x00: i32,
    /// the book's order (`Select` sorts by it)
    pub order: i32,
    pub id: i32,
    pub level: u8,
    /// the skill and the component it is listed under
    pub skill: u8,
    pub comp: u8,
    /// 0..3: C, UC, R, UR
    pub difficulty: u8,
    pub name: Vec<u16>,
}

fn wide(r: &mut Reader, max: usize) -> Vec<u16> {
    r.wide(max).encode_utf16().collect()
}

impl Recipe {
    /// `VIRec144_C017C0::Read`: three zigzags, four bytes, the name (64).
    pub fn read(r: &mut Reader) -> Recipe {
        Recipe { x00: r.signed() as i32, order: r.signed() as i32, id: r.signed() as i32, level: r.u8(), skill: r.u8(), comp: r.u8(), difficulty: r.u8(), name: wide(r, 64) }
    }
}

/// `VICraftDetail`: the recipe opened (sub-op 16).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detail {
    pub x00: i32,
    pub x04: i32,
    pub x08: i32,
    pub x0c: u8,
    pub x0d: u8,
    pub x0e: u8,
    /// 0 Common .. 3 Ultra Rare
    pub rarity: u8,
    /// what one make yields
    pub count: u16,
    /// "Product Item Lvl"
    pub level: u8,
    pub name: Vec<u16>,
    pub desc: Vec<u16>,
    pub stats: Vec<(i32, i32)>,
    /// (name, how many; 0 is not shown)
    pub ingredients: Vec<(Vec<u16>, u16)>,
    /// (name, whether the player has it)
    pub tools: Vec<(Vec<u16>, u8)>,
}

impl Detail {
    /// `VICraftDetail::Read`: a list whose count will not allocate (a
    /// negative one here) ends the read where it stands.
    pub fn read(r: &mut Reader) -> Detail {
        let mut d = Detail { x00: r.signed() as i32, x04: r.signed() as i32, x08: r.signed() as i32, ..Default::default() };
        d.x0c = r.u8();
        d.x0d = r.u8();
        d.x0e = r.u8();
        d.rarity = r.u8();
        d.count = r.u16();
        d.level = r.u8();
        d.name = wide(r, 64);
        d.desc = wide(r, 256);
        let n = r.signed();
        if !(0..=4096).contains(&n) {
            return d;
        }
        d.stats = (0..n).map(|_| (r.signed() as i32, r.signed() as i32)).collect();
        let n = r.signed();
        if !(0..=4096).contains(&n) {
            return d;
        }
        d.ingredients = (0..n).map(|_| (wide(r, 64), r.u16())).collect();
        let n = r.signed();
        if !(0..=4096).contains(&n) {
            return d;
        }
        d.tools = (0..n).map(|_| (wide(r, 64), r.u8())).collect();
        d
    }
}

/// Which crafting window is up (`VIWndGame::IsRecipeMode`, mode 6;
/// `IsExperimentMode`, mode 37): replies check it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Modes {
    pub recipe: bool,
    pub experiment: bool,
}

/// What the part asks of the client and its windows.
#[derive(Clone, Debug, PartialEq)]
pub enum CraftOut {
    Send(Vec<u8>),
    /// `ShowMsg(word)`: the word to the chat log
    ShowWord(i32),
    /// `AddText(text, colour)`
    Chat(String, [f32; 4]),
    /// `VIWndGame::OpenRecipe(skill)` (mode 6)
    OpenRecipe(i32),
    /// `VIWndGame::OpenExperiment` (mode 37)
    OpenExperiment,
    /// `m_wndRecipe->Message(text, icon)`
    RecipeMessage(Vec<u16>, i32),
    /// `m_wndRecipe->Inspect()`
    RecipeInspect,
    /// `m_wndExperiment->Notify(text, icon)`
    Notify(Vec<u16>, i32),
    /// `m_wndExperiment->Ask(text, p1, p2, icon)`
    Ask(Vec<u16>, i32, i32, i32),
}

/// The red of the part's `AddText` lines.
const RED: [f32; 4] = [1.0, 0.25, 0.25, 1.0];

/// One `VIString::snprintf16` argument.
pub enum Arg<'a> {
    D(i64),
    S(&'a [u16]),
}

/// `VIString::snprintf16(buf, bytes, fmt, ...)`: `%d`, `%s`/`%S` and `%%`
/// in turn, the result cut to the buffer (`bytes / 2 - 1` characters).
pub fn printf16(fmt: &[u16], args: &[Arg], bytes: usize) -> Vec<u16> {
    let mut out = Vec::new();
    let mut a = args.iter();
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        if c == '%' as u16 && i + 1 < fmt.len() {
            let k = fmt[i + 1];
            match char::from_u32(k as u32) {
                Some('d') => {
                    if let Some(Arg::D(v)) = a.next() {
                        out.extend(v.to_string().encode_utf16());
                    }
                    i += 2;
                    continue;
                }
                Some('s') | Some('S') => {
                    if let Some(Arg::S(s)) = a.next() {
                        out.extend(s.iter().take_while(|&&c| c != 0));
                    }
                    i += 2;
                    continue;
                }
                Some('%') => {
                    out.push(c);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
        i += 1;
    }
    out.truncate((bytes / 2).saturating_sub(1));
    out
}

fn w16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `strcat16(dst, bytes, src)`, cut to the buffer.
fn cat(dst: &mut Vec<u16>, src: &[u16], bytes: usize) {
    dst.extend_from_slice(src);
    dst.truncate((bytes / 2).saturating_sub(1));
}

/// The skill-up words a slot's report uses: (level reached, XP, rank).
fn slot_words(slot: i32) -> (i32, i32, i32) {
    match slot {
        1 => (1555, 1561, 1618),
        0 => (1556, 1562, 1619),
        2 => (1558, 1564, 1622),
        3 => (1554, 1560, 1620),
        4 => (1559, 1565, 1621),
        _ => (1557, 1563, 1623),
    }
}

/// `VIClientPart3834C`.
#[derive(Clone, Debug, Default)]
pub struct Craft {
    pub slots: [CraftSlot; 6],
    /// the inventory as the combine sees it (copied when an experiment opens
    /// and after each combine; the replies take from and add to this copy)
    pub items: ItemList,
    /// the combine (each an item, one of)
    pub combine: Vec<NetItem>,
    /// the book, in the pool's order
    pub recipes: Vec<Recipe>,
    pub detail: Detail,
    /// the request waited on (0 none)
    pub pending: i32,
    /// `m_p78C0`: the server's token
    pub token: u32,
}

impl Craft {
    /// `Clear`: the book and the detail.
    pub fn clear(&mut self) {
        self.recipes.clear();
        self.detail = Detail::default();
    }

    /// `GetSlot04(i)` / `GetSlot08(i)`: a slot's level and rank, 0 out of
    /// range.
    pub fn slot04(&self, i: i32) -> i32 {
        if (0..6).contains(&i) { self.slots[i as usize].0[1] as i32 } else { 0 }
    }

    pub fn slot08(&self, i: i32) -> i32 {
        if (0..6).contains(&i) { self.slots[i as usize].0[2] as i32 } else { 0 }
    }

    /// `Find(id)`: the recipe's place in the book.
    pub fn find(&self, id: i32) -> Option<usize> {
        self.recipes.iter().position(|r| r.id == id)
    }

    /// `Select(skill, comp)`: that skill's recipes for the component, by
    /// their order. (The game's `qsort` is not stable; equal orders keep the
    /// book's order here.)
    pub fn select(&self, skill: i32, comp: i32) -> Vec<Recipe> {
        let mut v: Vec<Recipe> = self.recipes.iter().filter(|r| r.skill as i32 == skill && r.comp as i32 == comp).cloned().collect();
        v.sort_by_key(|r| r.order);
        v
    }

    /// `IsMine(id)`: an 0xA7xxxxxx id.
    pub fn is_mine(id: u32) -> bool {
        id & 0xFF00_0000 == 0xA700_0000
    }

    /// `Use(id, x)`: the recipe book of skill `x` (its list asked for), or
    /// the experiment (the inventory copied, the combine emptied).
    pub fn use_id(&mut self, id: u32, x: i32, inventory: &ItemList) -> Vec<CraftOut> {
        let mut out = Vec::new();
        match id {
            0xA701_0000 => {
                self.pending = 0;
                out.extend(self.send6(x).map(CraftOut::Send));
                out.push(CraftOut::OpenRecipe(x));
            }
            0xA702_0000 => {
                self.pending = 0;
                self.items = inventory.clone();
                self.combine.clear();
                out.push(CraftOut::OpenExperiment);
            }
            _ => {}
        }
        out
    }

    fn request(&self, sub: u8) -> Writer {
        let mut w = Writer::op(0x1101);
        w.u8(sub).u32(self.token);
        w
    }

    /// `Send1(id)`: remove a recipe; waits (1).
    pub fn send1(&mut self, a: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        let mut w = self.request(1);
        w.u32(a as u32);
        self.pending = 1;
        Some(w.data)
    }

    /// `Send2(id, b)`: create; waits (2).
    pub fn send2(&mut self, a: i32, b: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        let mut w = self.request(2);
        w.u32(a as u32).u32(b as u32);
        self.pending = 2;
        Some(w.data)
    }

    /// `Send3(from, to)`: move a recipe; waits (3).
    pub fn send3(&mut self, a: i32, b: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        let mut w = self.request(3);
        w.u32(a as u32).u32(b as u32);
        self.pending = 3;
        Some(w.data)
    }

    /// `Combine`: the combine's item ids (a zigzag count and zigzag ids);
    /// the inventory copied again, the combine emptied; waits (4).
    pub fn send_combine(&mut self, inventory: &ItemList) -> Option<Vec<u8>> {
        if self.pending != 0 || self.combine.is_empty() {
            return None;
        }
        let mut w = self.request(4);
        w.signed(self.combine.len() as i64);
        for it in &self.combine {
            w.signed(it.item.id as i32 as i64);
        }
        self.items = inventory.clone();
        self.combine.clear();
        self.pending = 4;
        Some(w.data)
    }

    /// `Send5(x)`: scribe the discovery; does NOT wait.
    pub fn send5(&mut self, a: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        let mut w = self.request(5);
        w.u32(a as u32);
        Some(w.data)
    }

    /// `Send6(skill)`: the book cleared, its list asked for; waits (5).
    pub fn send6(&mut self, a: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        self.clear();
        let mut w = self.request(6);
        w.u32(a as u32);
        self.pending = 5;
        Some(w.data)
    }

    /// `Send7(id)`: the detail cleared and asked for; waits (6).
    pub fn send7(&mut self, a: i32) -> Option<Vec<u8>> {
        if self.pending != 0 {
            return None;
        }
        self.detail = Detail::default();
        let mut w = self.request(7);
        w.u32(a as u32);
        self.pending = 6;
        Some(w.data)
    }

    /// `ReadList`: the book; a list cut short empties it.
    fn read_list(&mut self, r: &mut Reader) {
        let n = r.u32();
        self.recipes.clear();
        for _ in 0..n {
            let k = Recipe::read(r);
            if r.error {
                self.recipes.clear();
                break;
            }
            self.recipes.push(k);
        }
        self.pending = 0;
    }

    /// `ReadSlots`: the six slots (sub-op 14, and the MemoryDump's).
    fn read_slots(&mut self, r: &mut Reader) {
        for s in &mut self.slots {
            *s = CraftSlot([r.u32(), r.u32(), r.u32(), r.u32()]);
        }
    }

    /// `Recv1102`: a reply, by its sub-op.
    pub fn recv(&mut self, data: &[u8], modes: Modes, words: &StringTable, expansion: u32) -> Vec<CraftOut> {
        let mut r = Reader::new(data);
        let mut out = Vec::new();
        let op = r.u8();
        if r.error {
            return out;
        }
        let w = |id: i32| w16(words.get(id, 0));
        match op {
            1 => {
                self.token = r.u32();
                out.push(CraftOut::ShowWord(1544));
            }
            2 => {
                let id = r.u32() as i32;
                self.token = r.u32();
                if let Some(i) = self.find(id) {
                    self.recipes.remove(i);
                }
                self.pending = 0;
            }
            3 | 7 => self.pending = 0,
            4 => {
                self.pending = 0;
                self.token = r.u32();
            }
            5 => {
                self.pending = 0;
                self.token = r.u32();
                out.push(CraftOut::Chat(words.get(1567, 0).to_string(), RED));
            }
            8 => self.recv08(&mut r),
            9 => {
                self.pending = 0;
                self.token = r.u32();
                let v: Vec<i32> = (0..8).map(|_| r.u32() as i32).collect();
                let name = wide(&mut r, 64);
                let (text, _) = self.skill_report(&v, &name, words);
                out.push(CraftOut::RecipeMessage(text, v[7]));
            }
            10 | 11 => {
                self.pending = 0;
                self.token = r.u32();
                let fail = r.u32();
                let failed = if fail != 0 { printf16(&w(1589), &[], 256) } else { Vec::new() };
                let mut text = printf16(&w16("%S"), &[Arg::S(&w(if op == 10 { 1569 } else { 1570 }))], 512);
                if !failed.is_empty() {
                    cat(&mut text, &w16("  "), 512);
                    cat(&mut text, &failed, 512);
                }
                out.push(CraftOut::RecipeMessage(text, -1));
            }
            12 => out.extend(self.recv12(&mut r, modes, words, expansion)),
            13 => {
                self.token = r.u32();
                let id = r.u32();
                let hp = r.u32() as i32;
                // Op0060(Find(id), hp): the item's hit points (a missing one
                // is the game's assert)
                if let Some(k) = inventory::find(&self.items, id) {
                    inventory::set_hp(&mut self.items, k, hp as i64);
                }
            }
            14 => self.read_slots(&mut r),
            15 => {
                self.token = r.u32();
                if modes.recipe {
                    self.read_list(&mut r);
                } else {
                    self.recipes.clear();
                }
                self.pending = 0;
            }
            16 => {
                self.token = r.u32();
                self.detail = Detail::read(&mut r);
                if modes.recipe {
                    out.push(CraftOut::RecipeInspect);
                }
                self.pending = 0;
            }
            _ => {}
        }
        out
    }

    /// `Recv08`: recipe `from` moved to `to`'s place, the rest closing up.
    fn recv08(&mut self, r: &mut Reader) {
        self.token = r.u32();
        let from = r.u32() as i32;
        let to = r.u32() as i32;
        let (Some(f), t) = (self.find(from), self.find(to)) else {
            self.pending = 0;
            return;
        };
        if let Some(t) = t {
            let at = self.recipes[t].order;
            let mut moved = None;
            for i in 0..self.recipes.len() {
                // the moved one's order read afresh each time, as shipped
                let fo = self.recipes[f].order;
                if self.recipes[i].order == fo {
                    moved = Some(i);
                }
                if self.recipes[i].order > fo {
                    self.recipes[i].order -= 1;
                }
            }
            for r in &mut self.recipes {
                if r.order >= at {
                    r.order += 1;
                }
            }
            if let Some(m) = moved {
                self.recipes[m].order = at;
            }
        }
        self.pending = 0;
    }

    /// The slot's new values and the report's line (`Recv09` and
    /// `ItemResult`): "created", a broken reagent, the XP, a rank, a level;
    /// `v` is (ok, slot, x04, x08, x0C, rise, fail, x2C). Also the XP part
    /// alone (the experiment's question).
    fn skill_report(&mut self, v: &[i32], name: &[u16], words: &StringTable) -> (Vec<u16>, Vec<u16>) {
        let w = |id: i32| w16(words.get(id, 0));
        let (ok, slot, rise, fail) = (v[0], v[1], v[5], v[6]);
        let (mut level, mut skill, mut next) = (Vec::new(), Vec::new(), Vec::new());
        if ok >= 0 && (0..6).contains(&slot) {
            let sl = &mut self.slots[slot as usize].0;
            let (old04, old08) = (sl[1], sl[2]);
            sl[1] = v[2] as u32;
            sl[2] = v[3] as u32;
            sl[3] = v[4] as u32;
            let (f1, f2, f3) = slot_words(slot);
            if sl[1] != old04 {
                level = printf16(&w(f1), &[Arg::D(sl[1] as i32 as i64)], 256);
            }
            skill = printf16(&w(f2), &[Arg::D(rise as i64)], 256);
            if sl[2] != old08 && sl[2] != 0 {
                next = printf16(&w(f3), &[], 256);
            }
        }
        let what = printf16(&w(1588), &[Arg::S(name)], 256);
        let failed = if fail != 0 { printf16(&w(1589), &[], 256) } else { Vec::new() };
        let mut text = what.clone();
        text.truncate(511);
        for part in [&failed, &skill, &next, &level] {
            if !part.is_empty() {
                cat(&mut text, &w16("  "), 1024);
                cat(&mut text, part, 1024);
            }
        }
        (text, skill)
    }

    /// `Recv12`: the experiment's results.
    fn recv12(&mut self, r: &mut Reader, modes: Modes, words: &StringTable, expansion: u32) -> Vec<CraftOut> {
        let mut out = Vec::new();
        let op = r.u8();
        self.token = r.u32();
        let w = |id: i32| w16(words.get(id, 0));
        match op {
            3 if modes.experiment => out.push(CraftOut::Notify(w(1598), -1)),
            2 if modes.experiment => out.push(CraftOut::Notify(w(1599), -1)),
            4 | 5 => {
                let fail = r.u32();
                self.took(r);
                let first = w(if op == 4 { 1569 } else { 1570 });
                let text = if fail != 0 { printf16(&w16("%S  %S"), &[Arg::S(&first), Arg::S(&w(1589))], 512) } else { printf16(&w16("%S"), &[Arg::S(&first)], 512) };
                if modes.experiment {
                    out.push(CraftOut::Notify(text, -1));
                }
            }
            6 | 7 => out.extend(self.item_result(r, op == 7, modes, words, expansion)),
            _ => {}
        }
        self.pending = 0;
        out
    }

    /// The components a combine took: (item id, how many), each off the
    /// first stack of that item.
    fn took(&mut self, r: &mut Reader) {
        let n = r.u32();
        for _ in 0..n {
            let id = r.u32() as i32 as i64;
            let qty = r.u32() as i32 as i64;
            if r.error {
                break;
            }
            if let Some(j) = self.items.items.iter().position(|it| it.item.id == id) {
                inventory::took(&mut self.items, j, qty);
            }
        }
    }

    /// `ItemResult(s, experiment)`: the report, the components taken, the
    /// item made (added to the copy); an experiment asks whether to scribe
    /// the recipe (0xA7030004 with x2C), else tells.
    fn item_result(&mut self, r: &mut Reader, experiment: bool, modes: Modes, words: &StringTable, expansion: u32) -> Vec<CraftOut> {
        let w = |id: i32| w16(words.get(id, 0));
        let v: Vec<i32> = (0..8).map(|_| r.u32() as i32).collect();
        self.took(r);
        let item = net::dump::read_net_item(r);
        inventory::add_item(&mut self.items, &item, item.item.max_stack, inventory::CAPACITY);
        let name = w16(&item.item.name);
        let (text, skill) = self.skill_report(&v, &name, words);
        let icon = inventory::item_icon(&item, expansion) as i32;
        let mut out = Vec::new();
        if experiment {
            let ask = printf16(&w(1600), &[Arg::S(&name), Arg::S(&skill)], 1024);
            if modes.experiment {
                out.push(CraftOut::Ask(ask, 0xA703_0004u32 as i32, v[7], icon));
            }
        } else if modes.experiment {
            out.push(CraftOut::Notify(text, icon));
        }
        out
    }

    /// `AddToCombine(slot)`: one of the slot's item, if it is a component
    /// (`m_p54`), the combine holds fewer of it than the stack, and fewer
    /// than four in all; else a red line (1601, 1602).
    pub fn add_to_combine(&mut self, slot: i32, words: &StringTable) -> Vec<CraftOut> {
        let Some(it) = usize::try_from(slot).ok().and_then(|s| self.items.items.get(s)) else { return Vec::new() };
        if it.item.p54 == 0 {
            return Vec::new();
        }
        let n = self.combine.iter().filter(|c| c.item.id == it.item.id).count() as i64;
        if n >= it.quantity {
            vec![CraftOut::Chat(words.get(1601, 0).to_string(), RED)]
        } else if self.combine.len() >= 4 {
            vec![CraftOut::Chat(words.get(1602, 0).to_string(), RED)]
        } else {
            let mut one = it.clone();
            one.quantity = 1;
            self.combine.push(one);
            Vec::new()
        }
    }

    /// `RemoveFromCombine(slot)`: the first of the slot's item out.
    pub fn remove_from_combine(&mut self, slot: i32) {
        let Some(it) = usize::try_from(slot).ok().and_then(|s| self.items.items.get(s)) else { return };
        if it.item.p54 == 0 {
            return;
        }
        let id = it.item.id;
        if let Some(i) = self.combine.iter().position(|c| c.item.id == id) {
            self.combine.remove(i);
        }
    }
}

/// The MemoryDump's token and slots (`Msg000D`: `SetP78C0`, `ReadSlots`).
pub fn from_dump(token: u32, slots: &[CraftSlot; 6]) -> Craft {
    Craft { token, slots: slots.clone(), ..Default::default() }
}
