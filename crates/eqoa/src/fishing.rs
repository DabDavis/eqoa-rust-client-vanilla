//! Fishing (CLIENT): the window, `VIWndFishing` (`viwndfishing.cpp`, the game
//! window's mode 36), and the client's part of it, `VIClientPart3FC10`
//! (`viclientpart3fc10.cpp`, at `VIClient+0x3FC10`: 0x0F02 in, 0x0F01 out).
//! `fishplay.rs` ties them to play.
//!
//! A session: the server's 0x0F02 1 (Start) opens the window ("Let's go
//! fishing!", state 1). ✕ (Space), or the left stick pulled back, starts a
//! cast: the power meter left of the window falls for a second, then rises
//! for one; ✕ again (or the stick pushed forward) throws it there, else it
//! is thrown full at the top. The power picks the cast's quality (0 a perfect
//! cast in 56..63, 5 in 41..78, 15 else); the line goes ten ahead of the
//! walk's committed place, and must cross nothing on the way and find water
//! before anything solid below the end (`find_water`), where the splash plays
//! half a second. Two and a half seconds after the throw a cast on water is
//! sent (0x0F01 1 with the quality, once the pole, wear slot 20, and the bait,
//! 21, are checked), else "Your cast missed the water". The server answers
//! with the catch (7: the list opens, "TAKE ALL" on top (Reel, 0x0F01 3), the
//! items (Keep, 5, each the record's key and count), "LEAVE ALL" after them
//! (Pull, 4)), one caught item (4: into the inventory), the list closed (6),
//! or a word (8 missed, 9 the bait lost, 10 the pole damaged); △ (Escape)
//! leaves the catch (Pull) or the session (Cancel, 2; 0x0F02 2 closes the
//! window). Enter or R3 opens a chat line, C or R2 held scrolls the chat box.
//!
//! Every time here is VITimer's (`m_p07568`, 983.04 ticks a second:
//! `net::session::Session::timer`), as the window reads it.

#![allow(clippy::too_many_arguments)]

use crate::chat::WndChat;
use crate::craft::{Arg, printf16};
use crate::editbox::{EditBox, EditUp};
use crate::infotext::Info;
use crate::ui::Layout;
use crate::wndlist::WndList;
use net::dump::{ItemList, NetItem};
use net::stream::Writer;
use vi::coll::{CollRay, EnvRay};
use vi::fpu::{Ee, F};
use vi::scene::{Filter, FilterKind};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};
use vi::vect::Vect3;
use vi::world::World;

/// `VIClientPart3FC10::m_state`.
pub mod state {
    pub const OFF: i32 = 0;
    pub const WAITING: i32 = 1;
    pub const CAST: i32 = 2;
    pub const CAUGHT: i32 = 3;
    pub const KEEPING: i32 = 4;
}

/// 0x0F01, the client's (`3841`).
pub const OP_OUT: u16 = 0x0F01;
/// 0x0F02, the server's.
pub const OP_IN: u16 = 0x0F02;

/// The fishing pole's wear slot, and the bait's.
const WEAR_POLE: i64 = 20;
const WEAR_BAIT: i64 = 21;
/// The equipment slot a pole is worn in (the primary hand's).
const EQUIP_POLE: i64 = 14;

/// The list's rows (`m_list->Init(42)`).
const ROWS: usize = 42;

/// `VIClientPart3FC10`: the session's state and the catch on the line.
#[derive(Clone, Debug, Default)]
pub struct Part {
    pub state: i32,
    pub catch: ItemList,
}

/// 0x0F02 as `Recv0F02` reads it: a byte, then the case's.
#[derive(Clone, Debug)]
pub enum Recv {
    Start,
    Stop,
    Ready,
    /// the catch at `slot` into the inventory (`x` is read and not used)
    Caught { slot: i32, x: i32 },
    Waiting,
    /// the catch list closed ("Bite" in the decomp's provisional names)
    Bite,
    /// the catch, a `VIItemList`
    Loot(ItemList),
    Missed,
    Lost,
    Damaged,
}

/// `Recv0F02`'s read: the sub-op, then what its case reads (Caught two raw
/// words, Loot `VIItemList::Read`). None for a sub-op the switch has no case
/// for (the byte is read and nothing done), and for a message cut short.
pub fn read_0f02(data: &[u8]) -> Option<Recv> {
    let mut r = net::stream::Reader::new(data);
    let op = r.u8();
    if r.error {
        return None;
    }
    let m = match op {
        1 => Recv::Start,
        2 => Recv::Stop,
        3 => Recv::Ready,
        4 => {
            let slot = r.u32() as i32;
            let x = r.u32() as i32;
            Recv::Caught { slot, x }
        }
        5 => Recv::Waiting,
        6 => Recv::Bite,
        7 => Recv::Loot(net::dump::read_item_list(&mut r)),
        8 => Recv::Missed,
        9 => Recv::Lost,
        10 => Recv::Damaged,
        _ => return None,
    };
    (!r.error).then_some(m)
}

/// A 0x0F01 with sub-op `sub` alone: `Cancel` 2, `Reel` 3, `Pull` 4.
pub fn sub_msg(sub: u8) -> Vec<u8> {
    let mut w = Writer::op(OP_OUT);
    w.u8(sub);
    w.data
}

/// `Cast(quality)`'s 0x0F01: 1, the quality as a raw word.
pub fn cast_msg(quality: i32) -> Vec<u8> {
    let mut w = Writer::op(OP_OUT);
    w.u8(1).u32(quality as u32);
    w.data
}

/// `Keep(slot, x)`'s 0x0F01: 5, the catch's key (`GetSlotId`), `x` (the
/// window's: the record's count).
pub fn keep_msg(id: u32, x: i32) -> Vec<u8> {
    let mut w = Writer::op(OP_OUT);
    w.u8(5).u32(id).u32(x as u32);
    w.data
}

/// `HasEquipped(wear)`: an inventory item of that wear slot that is worn.
pub fn has_equipped(inv: &ItemList, wear: i64) -> bool {
    inv.items.iter().any(|it| it.item.wear_slot == wear && it.equip_slot != -1)
}

/// `FindEquipped(equip)`: the inventory item in that equipment slot.
pub fn find_equipped(inv: &ItemList, equip: i64) -> Option<&NetItem> {
    inv.items.iter().find(|it| it.equip_slot == equip)
}

impl Part {
    /// `Cast(quality)`: no pole (word 1464), no bait (1465), a pole worn
    /// out (1466: its hit points gone) stop it; else word 1433 for a perfect
    /// cast (0) or 1432, the cast sent, and state 2. The word to show, and
    /// the message.
    pub fn cast(&mut self, inv: &ItemList, quality: i32) -> (i32, Option<Vec<u8>>) {
        if !has_equipped(inv, WEAR_POLE) {
            return (1464, None);
        }
        if !has_equipped(inv, WEAR_BAIT) {
            return (1465, None);
        }
        if find_equipped(inv, EQUIP_POLE).is_some_and(|p| p.cur_hp <= 0) {
            return (1466, None);
        }
        let word = if quality == 0 { 1433 } else { 1432 };
        self.state = state::CAST;
        (word, Some(cast_msg(quality)))
    }

    /// `Keep(slot, x)`: nothing while one is being kept (4); else the
    /// catch's key at `slot` sent with `x`, and state 4. (`GetSlotId`
    /// asserts a slot the list holds; the window asks for no other.)
    pub fn keep(&mut self, slot: usize, x: i32) -> Option<Vec<u8>> {
        if self.state == state::KEEPING {
            return None;
        }
        let id = self.catch.items.get(slot)?.inv_index;
        self.state = state::KEEPING;
        Some(keep_msg(id, x))
    }

    /// `Damaged`'s second line: a pole worn and out of hit points.
    pub fn pole_broken(inv: &ItemList) -> bool {
        has_equipped(inv, WEAR_POLE) && find_equipped(inv, EQUIP_POLE).is_some_and(|p| p.cur_hp <= 0)
    }
}

/// `Splash`'s quality out of the power: 0 for a perfect cast (56..63), 5 a
/// fair one (41..78), 15 else (`m_power - 56 < 8u`, `m_power - 41 < 38u`).
pub fn quality(power: i32) -> i32 {
    if (power.wrapping_sub(56) as u32) < 8 {
        0
    } else if (power.wrapping_sub(41) as u32) < 38 {
        5
    } else {
        15
    }
}

/// How `FindWater` decided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Landing {
    /// something between the rod and the end (the first ray)
    Blocked,
    /// no water (or slime) surface under the end (the environment ray)
    NoWater,
    /// something solid under the end above the surface
    Solid,
    /// on the water, there
    Water(Vect3<Ee>),
}

/// `FindWater(actor, pos, hpr, scene, out)`: where a line cast from `pos`
/// ten units along the heading `hpr.x` lands. Nothing may be in the way from
/// one above `pos` to one above the end; an environment ray (water or
/// slime, `VITransEnvRay` mask 3) from one above the end to ten below must
/// cross a surface, and a ray down the same line must hit nothing solid
/// before it.
pub fn landing(sc: &mut World<Ee>, actor: i32, pos: Vect3<Ee>, hpr: Vect3<Ee>) -> Landing {
    let mut start = pos;
    start.y += F::ONE;
    let mut out = pos;
    let mut dir = Vect3::<Ee>::default();
    dir.hpr(hpr.x, F::ZERO, F::ZERO);
    out.add_scaled(&dir, F::new(10.0));
    let a = Vect3::new(out.x, out.y + F::ONE, out.z);
    let b = Vect3::new(out.x, out.y - F::new(10.0), out.z);
    let mut env = EnvRay::default();
    env.init(a, b, 3);
    let mut filter = Filter::new(FilterKind::Scene, actor);
    let mut ray = CollRay::default();
    ray.init(start, a);
    if sc.collide_ray(&mut ray, &mut filter) {
        return Landing::Blocked;
    }
    if !sc.collide_env(&mut env) {
        return Landing::NoWater;
    }
    let t = env.hit_time;
    let at = env.hit_point();
    ray.init(a, b);
    if !sc.collide_ray(&mut ray, &mut filter) {
        return Landing::Water(at);
    }
    if ray.hit_time < t {
        return Landing::Solid;
    }
    Landing::Water(at)
}

/// `FindWater`'s answer: the surface's point, or None.
pub fn find_water(sc: &mut World<Ee>, actor: i32, pos: Vect3<Ee>, hpr: Vect3<Ee>) -> Option<Vect3<Ee>> {
    match landing(sc, actor, pos, hpr) {
        Landing::Water(at) => Some(at),
        _ => None,
    }
}

/// What the window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum FishOut {
    /// a 0x0F01 message (the part's senders)
    Send(Vec<u8>),
    /// `Splash`: `PlayAction(65)`, and `FindWater` from the walk's committed
    /// place and heading (`fishplay`, which has the scene)
    Splash,
    /// `Renamed`: the edit box's text, to `ChatLine`
    ChatLine(String),
}

/// A window init owed to the next draw, which has the font (`Init` measures
/// it): what a message did in the game at once, in its order.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Owed {
    /// `OnMessage` 295's: the window and the message box
    Focus,
    /// `OpenBait`'s: the bait box
    Bait,
}

/// `VIWndFishing`.
pub struct FishingWnd {
    window: Window,
    /// `m_win2`: the bait box
    win2: Window,
    /// `m_win3`: the message box
    win3: Window,
    pub list: WndList,
    pub list_visible: bool,
    pub edit: EditBox,
    pub edit_visible: bool,
    /// `m_desc`, `m_text` and `m_textEnd`
    pub desc: Vec<u16>,
    pub text: Vec<u16>,
    pub text_end: i64,
    /// `m_hit`: the last cast found water
    pub hit: bool,
    pub chat_on: bool,
    pub chat_key: i32,
    pub casting: bool,
    pub cast_time: i64,
    /// `m_p7E0`: the cast's quality
    pub quality: i32,
    pub power: i32,
    pub ready: bool,
    pub splashing: bool,
    pub splash_time: i64,
    pub release: bool,
    pub on_water: bool,
    pub target: Vect3<Ee>,
    /// `sTargetEmitter.Update()` this frame (`Tick`)
    pub emitting: bool,
    owed: Vec<Owed>,
}

impl Default for FishingWnd {
    /// `VIWndFishing()` and `Init`: the list's 42 rows, both children hidden.
    fn default() -> Self {
        let mut list = WndList::default();
        list.init(ROWS);
        let mut w = FishingWnd {
            window: Window::default(),
            win2: Window::default(),
            win3: Window::default(),
            list,
            list_visible: false,
            edit: EditBox::default(),
            edit_visible: false,
            desc: Vec::new(),
            text: Vec::new(),
            text_end: 0,
            hit: false,
            chat_on: false,
            chat_key: 0,
            casting: false,
            cast_time: 0,
            quality: 0,
            power: 0,
            ready: true,
            splashing: false,
            splash_time: 0,
            release: false,
            on_water: false,
            target: Vect3::default(),
            emitting: false,
            owed: Vec::new(),
        };
        w.reset_cast();
        w
    }
}

/// Word `id` of the game's table, as `GetString` hands it.
fn word(vui: &Vui, id: i32) -> Vec<u16> {
    wide(vui.words.get(id, vui.language))
}

impl FishingWnd {
    /// `OnMessage` 295 with the window taking the focus from outside it
    /// (`UpdateMode`'s mode 36): sound 15, the window titled "Fishing" (word
    /// 1205) at (75, 224) and the message box over it (both with the next
    /// draw), the texts gone, the list and the edit box hidden, the cast
    /// reset, the chat box's scroll forgotten.
    pub fn focus(&mut self, cues: &mut Vec<i32>) {
        cues.push(15);
        self.owed.push(Owed::Focus);
        self.desc.clear();
        self.text.clear();
        self.text_end = 0;
        self.list_visible = false;
        self.edit_visible = false;
        self.reset_cast();
        self.chat_on = false;
        self.chat_key = 0;
    }

    /// `OnMessage` 295 with the focus gone from the window to another
    /// outside it: the chat box's scroll forgotten (not ended).
    pub fn blur(&mut self) {
        self.chat_on = false;
        self.chat_key = 0;
    }

    /// The inits the messages owed, in their order.
    fn pay(&mut self, vui: &Vui, r: &dyn Raster) {
        for o in std::mem::take(&mut self.owed) {
            match o {
                Owed::Focus => {
                    self.window.init(vui, r, [75, 224], 125, 11, 0);
                    self.window.set_title(Some(&word(vui, 1205)));
                    self.window.close(true);
                    self.window.open(false);
                    // Init(ui, pos, center, size, x): the size as given (-1, -1),
                    // no title bar's height, no rows
                    self.win3 = Window { center: [320, self.window.top_left()[1] + 22], size: [-1, -1], x178: 1, title_h: 0, rows: 0, ..self.win3 };
                    self.win3.close(true);
                }
                Owed::Bait => {
                    self.win2.init(vui, r, [460, 152], 340, 10, 1);
                    if self.win2.is_open() {
                        self.win2.close(true);
                    }
                    self.win2.open(false);
                }
            }
        }
    }

    /// `ResetCast`.
    pub fn reset_cast(&mut self) {
        self.casting = false;
        self.power = 0;
        self.ready = true;
        self.release = false;
        self.splashing = false;
    }

    /// `Cast`: the meter full and falling from `now`.
    pub fn cast(&mut self, now: i64) {
        self.casting = true;
        self.cast_time = now;
        self.power = 120;
        self.ready = false;
        self.release = false;
    }

    /// `Tick` at `now`, the left stick's vertical `ly`: the meter falls from
    /// 120 over 500..1500 after the cast and rises over 1500..2500, where a
    /// release or the stick pushed forward throws (`Splash`); at its top it
    /// throws itself. From 1000 after the throw the splash plays half a
    /// second (`sTargetEmitter.Update()`, on water), and from 2500 the cast
    /// is over. Whether it threw this frame.
    pub fn tick(&mut self, now: i64, ly: F<Ee>) -> bool {
        let dt = now - self.cast_time;
        let mut threw = false;
        if dt > 500 {
            if (501..1501).contains(&dt) {
                let t = F::ONE - F::<Ee>::from_int((dt - 500) as i32) / F::new(1000.0);
                self.power = (t * F::new(120.0)).to_int();
            } else if (1501..2500).contains(&dt) {
                if !self.ready {
                    let t = F::<Ee>::from_int((dt - 1500) as i32) / F::new(1000.0);
                    self.power = (t * F::new(120.0)).to_int();
                    if self.release || ly < F::new(-0.25) {
                        self.ready = true;
                        self.splash(now);
                        threw = true;
                    }
                }
            } else if !self.ready {
                self.power = 120;
                self.ready = true;
                self.splash(now);
                threw = true;
            }
        }
        self.emitting = false;
        if self.splashing {
            let d = now - self.splash_time;
            if d > 1000 {
                if (1001..1501).contains(&d) {
                    self.emitting = self.on_water;
                } else if !(1501..2501).contains(&d) {
                    self.splashing = false;
                    self.casting = false;
                }
            }
        }
        self.release = false;
        threw
    }

    /// `Splash`'s own half: the quality, and the splash's start. The
    /// action and `FindWater` are the client's (`FishOut::Splash`), and come
    /// back through `water`.
    fn splash(&mut self, now: i64) {
        self.quality = quality(self.power);
        self.hit = false;
        self.on_water = false;
        self.splashing = true;
        self.splash_time = now;
    }

    /// `FindWater`'s answer for the splash: `m_hit`, and on water with the
    /// splash's definition there (`gTargetEffectDef`) the place the emitter
    /// is put (purged, located there, facing up).
    pub fn water(&mut self, at: Option<Vect3<Ee>>, have_effect: bool) -> Option<Vect3<Ee>> {
        self.hit = at.is_some();
        self.on_water = self.hit && have_effect;
        if self.on_water {
            self.target = at.unwrap_or_default();
            return Some(self.target);
        }
        None
    }

    /// `SetText`: a line in the message box for 5,000 ticks (127 characters
    /// at most).
    pub fn set_text(&mut self, s: &[u16], now: i64) {
        self.text_end = now + 5000;
        self.text = s.iter().copied().take_while(|&c| c != 0).take(127).collect();
    }

    /// `OpenBait` (`FishingA`, the catch in): the chat box's scroll ended,
    /// the edit box hidden, the list from its top and showing (it takes the
    /// keys), the bait box (with the next draw), sound 15.
    pub fn open_bait(&mut self, chat: Option<&mut WndChat>, now_ms: i64, cues: &mut Vec<i32>) {
        self.chat_off(chat);
        self.edit_visible = false;
        self.list.reset_scroll();
        self.list.open(now_ms);
        self.list_visible = true;
        self.desc.clear();
        self.owed.push(Owed::Bait);
        cues.push(15);
    }

    /// `CloseBait` (`FishingB`).
    pub fn close_bait(&mut self) {
        self.list.reset_scroll();
        self.list_visible = false;
        self.desc.clear();
    }

    /// `ChatOn`: the chat box scrolls while the key is held.
    fn chat_on(&mut self, chat: Option<&mut WndChat>) {
        if let Some(c) = chat {
            c.set_scroll(1);
        }
        self.chat_key = 0;
        self.chat_on = true;
    }

    /// `ChatOff`.
    fn chat_off(&mut self, chat: Option<&mut WndChat>) {
        if let Some(c) = chat
            && self.chat_on
        {
            c.set_scroll(0);
            if self.chat_key != 0 {
                c.key_up(self.chat_key);
            }
        }
        self.chat_on = false;
        self.chat_key = 0;
    }

    /// `Rename`: the edit box, one line of 127 characters round (320, 224),
    /// takes the keys (empty: `SetMaxLength` makes its buffer anew).
    fn rename(&mut self, chat: Option<&mut WndChat>, now_ms: i64) {
        self.chat_off(chat);
        self.edit.set_rows(1);
        self.edit.set_multi_line(false);
        self.edit.set_center([320, 224]);
        self.edit.set_max_length(128);
        self.edit_visible = true;
        self.edit.focus(now_ms);
    }

    /// A key to the window with the focus: the edit box when it is up, the
    /// list when it is up (Enter and ✕ its pick, its up and down; the rest
    /// on to this window, `VIWnd::OnKeyDown`), else this one's
    /// (`OnKeyDown`/`OnKeyUp`). `now` the tick clock, `now_ms` the HUD's.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, part: &mut Part, chat: Option<&mut WndChat>, key: i32, down: bool, now: i64, now_ms: i64, cues: &mut Vec<i32>, out: &mut Vec<FishOut>) {
        if self.edit_visible {
            if !down {
                self.edit.key_up(key);
                return;
            }
            match self.edit.key(vui, r, key, now_ms, cues) {
                // 259: Renamed
                EditUp::Done(_) => {
                    out.push(FishOut::ChatLine(String::from_utf16_lossy(&self.edit.text)));
                    self.edit_visible = false;
                }
                // 260 from the edit box
                EditUp::Cancel => self.edit_visible = false,
                EditUp::None => {}
            }
            return;
        }
        if self.list_visible {
            match (key, down) {
                (13 | 134, true) => {
                    self.list_pick(part, cues, out);
                    return;
                }
                (19 | 138 | 20 | 139, _) => {
                    self.list.key_up_down(key, down);
                    return;
                }
                _ => {}
            }
        }
        if down {
            self.key_down(part, chat, key, now, now_ms, cues, out);
        } else {
            self.key_up(chat, key);
        }
    }

    /// `OnKeyDown`.
    fn key_down(&mut self, part: &mut Part, chat: Option<&mut WndChat>, key: i32, now: i64, now_ms: i64, cues: &mut Vec<i32>, out: &mut Vec<FishOut>) {
        match key {
            32 | 134 => {
                if part.state == state::WAITING {
                    if !self.casting {
                        cues.push(11);
                        self.cast(now);
                    } else {
                        self.release = true;
                    }
                }
            }
            27 | 132 => {
                cues.push(8);
                out.push(FishOut::Send(sub_msg(if part.state == state::CAUGHT { 4 } else { 2 })));
            }
            13 | 147 => {
                if part.state != state::CAUGHT && part.state != state::KEEPING {
                    self.rename(chat, now_ms);
                }
            }
            67 | 99 | 143 => {
                if part.state != state::CAUGHT && part.state != state::KEEPING {
                    self.chat_on(chat);
                }
            }
            19 | 20 | 138 | 139 => {
                if let Some(c) = chat
                    && self.chat_on
                {
                    c.key_down(key);
                    self.chat_key = key;
                }
            }
            _ => {}
        }
    }

    /// `OnKeyUp`.
    fn key_up(&mut self, chat: Option<&mut WndChat>, key: i32) {
        match key {
            67 | 99 | 143 => self.chat_off(chat),
            19 | 20 | 138 | 139 => {
                if let Some(c) = chat
                    && self.chat_on
                {
                    c.key_up(key);
                    self.chat_key = 0;
                }
            }
            _ => {}
        }
    }

    /// `OnMessage` 256 from the list: sound 13; the top row Reels, an item
    /// is Kept (its count), the row after them Pulls.
    fn list_pick(&mut self, part: &mut Part, cues: &mut Vec<i32>, out: &mut Vec<FishOut>) {
        cues.push(13);
        let count = part.catch.items.len() as i32;
        let p1 = self.list.sel;
        if p1 == 0 {
            out.push(FishOut::Send(sub_msg(3)));
        } else if p1 < count + 1 {
            let slot = p1 - 1;
            if slot < count {
                let x = part.catch.items[slot as usize].quantity as i32;
                if let Some(m) = part.keep(slot as usize, x) {
                    out.push(FishOut::Send(m));
                }
            }
        } else if p1 == count + 1 {
            out.push(FishOut::Send(sub_msg(4)));
        }
    }

    /// A pointer at frame point `p`: the edit box's when it is up; a row of
    /// the list when it is up, selected (a second click on it picks it).
    /// The keys to press. The legend's glyphs are the HUD's (`pointer`).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.edit_visible {
            return self.edit.point(p).flatten().into_iter().collect();
        }
        if self.list_visible {
            return self.list.row_at(p).map(|row| self.list.pick(row)).into_iter().collect();
        }
        Vec::new()
    }

    /// `FillBait`: a heading (118 "TAKE ALL", or 641 "Empty" with no
    /// catch), the catch (a stack showing its count), 119 "LEAVE ALL" after
    /// it when there is one, the rest empty; then `Describe`.
    fn fill_bait(&mut self, catch: &ItemList, info: &Info, icon: &dyn Fn(&NetItem) -> i32) {
        let n = catch.items.len();
        self.list.set_item(0, 2, -1, -1, if n > 0 { 118 } else { 641 }, 0);
        for (i, it) in catch.items.iter().enumerate() {
            let ty = if it.quantity >= 2 { 5 } else { 1 };
            self.list.set_item(i + 1, ty, icon(it), -1, -1, it.quantity as i32);
        }
        if n > 0 {
            self.list.set_item(n + 1, 2, -1, -1, 119, 0);
        } else {
            self.list.set_item(n + 1, 0, -1, -1, -1, 0);
        }
        for i in n + 2..ROWS {
            self.list.set_item(i, 0, -1, -1, -1, 0);
        }
        // Describe: the selected catch's text
        self.desc.clear();
        let slot = self.list.sel - 1;
        if n > 0 && slot >= 0 && (slot as usize) < n {
            self.desc = wide(&info.describe_item(&catch.items[slot as usize]));
        }
    }

    /// `OnDrawSelf` at `now` (ticks; `now_ms` the HUD's clock for the
    /// children), the left stick's vertical `ly`; then the children (the
    /// list, the edit box) when they show. A cast over calls `Cast` on the
    /// part (before the legend, which shows it).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, part: &mut Part, info: &Info, icon: &dyn Fn(&NetItem) -> i32, now: i64, now_ms: i64, ly: F<Ee>, cues: &mut Vec<i32>, out: &mut Vec<FishOut>) {
        self.pay(vui, r);
        self.list.pos = [10, 58];
        self.fill_bait(&part.catch, info, icon);
        if self.casting {
            if self.tick(now, ly) {
                out.push(FishOut::Splash);
            }
            if !self.casting {
                if self.hit {
                    let (w, m) = part.cast(info.inv, self.quality);
                    self.set_text(&word(vui, w), now);
                    out.extend(m.map(FishOut::Send));
                } else {
                    self.set_text(&word(vui, 1421), now);
                }
            }
        } else {
            self.emitting = false;
            if part.state == state::WAITING && ly > F::new(0.25) {
                self.cast(now);
            }
        }
        let back = printf16(&wide("|t%S"), &[Arg::S(&word(vui, 161))], 64);
        let fmt = if self.casting || part.state == state::CAST { "|-|x%S" } else { "|x%S" };
        let cast = printf16(&wide(fmt), &[Arg::S(&word(vui, 1438))], 64);
        self.window.draw(vui, r);
        let width = self.window.size[0];
        self.window.text_box(vui, r, &cast, 20, 9, width - 40, 1, UI_COLORS[0], 0);
        self.window.text_box(vui, r, &back, 20, 10, width - 40, 1, UI_COLORS[0], 0);
        // the message box, while its time lasts
        if now < self.text_end {
            self.win3.set_title(Some(&self.text));
            self.win3.open(false);
        } else {
            self.text_end = 0;
            self.text.clear();
            self.win3.close(false);
        }
        if self.win3.is_open() || self.win3.is_opening() || self.win3.is_closing() {
            self.win3.draw(vui, r);
        }
        // the bait box
        if self.desc.is_empty() {
            self.win2.close(true);
        } else {
            let lines = self.win2.count_lines(vui, r, &self.desc, self.win2.size[0] - 40);
            self.win2.init(vui, r, [340, 185], 340, lines, 1);
            self.win2.open(false);
            self.win2.draw(vui, r);
            let w2 = self.win2.size[0];
            self.win2.text_box(vui, r, &self.desc, 20, 0, w2 - 40, lines, [1.0, 1.0, 1.0, self.win2.open], 3);
        }
        draw_meter(r, self.power);
        if self.list_visible {
            self.list.draw(vui, r, now_ms, cues);
        }
        if self.edit_visible {
            self.edit.draw(vui, r, now_ms, cues);
        }
    }
}

/// The power meter: a black frame, the green bar, its yellow band and red
/// middle (`SetSolidFillUIMaterial`'s fans), and over them the power from the
/// bottom (`SetSolidFillUIAVGMaterial`, blended): blue at its top to white at
/// the bottom, at 0.7. The 2D layer's quads have one colour, so the fan's
/// Gouraud ramp is drawn a pixel row at a time, each the ramp's colour at
/// the row's middle (the ramp runs down only: both top corners blue, both
/// bottom white).
pub fn draw_meter(r: &mut dyn Layout, power: i32) {
    let fan = |a: [i32; 2], b: [i32; 2]| [a, [b[0], a[1]], b, [a[0], b[1]]];
    r.fill_fan([0.0, 0.0, 0.0, 1.0], fan([64, 144], [85, 265]));
    r.fill_fan([0.0, 1.0, 0.0, 1.0], fan([65, 145], [84, 264]));
    r.fill_fan([1.0, 1.0, 0.0, 1.0], fan([65, 185], [84, 224]));
    r.fill_fan([1.0, 0.0, 0.0, 1.0], fan([65, 200], [84, 209]));
    if power != 0 {
        for (y, c) in meter_rows(power) {
            r.fill_blended(c, fan([65, y], [84, y + 1]));
        }
    }
}

/// The power's rows (frame y, colour): `power` rows up from 264 when it is
/// positive (none past the fan's top otherwise: a fan whose top is below its
/// bottom is the same fan turned over, here as there).
pub fn meter_rows(power: i32) -> Vec<(i32, [f32; 4])> {
    let (top, bottom) = (264 - power, 264);
    let (y0, y1) = (top.min(bottom), top.max(bottom));
    let h = (y1 - y0) as f32;
    (y0..y1)
        .map(|y| {
            // 0 at the blue corners, 1 at the white
            let mut t = (y as f32 + 0.5 - y0 as f32) / h;
            if top > bottom {
                t = 1.0 - t;
            }
            (y, [t, t, 1.0, 0.7])
        })
        .collect()
}

/// `LoadTargetEffect`'s splash: a definition on texture 0x2B6BF329 (additive,
/// blend 1), a fountain up the y axis spread 0.39 every way, 100 a second
/// for 0.8 s, 0.2 big, falling, from pale blue to white; and its emitter, on
/// a particle system of its own (the game's is the scene's).
pub struct SplashFx {
    sys: vi::particle::System<Ee>,
    rand: vi::csprite::Rand,
    emitter: usize,
}

/// The splash's texture, as `FindTyped(0x2B6BF329, 1)` looks it up.
pub const SPLASH_TEXTURE: u32 = 0x2B6B_F329;

/// The splash's definition, as `LoadTargetEffect` sets it up.
pub fn splash_definition() -> vi::particle::Definition<Ee> {
    use vi::particle::{Color, Definition, VecField};
    let mut d = Definition::<Ee>::default();
    d.texture = SPLASH_TEXTURE;
    // SetBlendMode(1)
    d.blend = 1;
    let a = &mut d.attrs;
    a.set_nozzle_axis(Vect3::of(0.0, 1.0, 0.0));
    a.set_vec(VecField::NozzleHprVar, Vect3::of(0.39, 0.39, 0.39));
    a.birthrate = F::new(100.0);
    a.version += 1;
    a.lifespan = F::new(0.8);
    a.version += 1;
    a.start_size = F::new(0.2);
    a.version += 1;
    a.end_size = F::new(0.2);
    a.version += 1;
    a.set_gravity_on(1);
    let from = Color::<Ee> { r: F::new(0.8), g: F::new(0.8), b: F::ONE, a: F::ONE };
    let to = Color::<Ee> { r: F::ONE, g: F::ONE, b: F::ONE, a: F::ONE };
    for i in 0..32 {
        let mut c = to;
        let t = F::<Ee>::from_int(i) * F::new(0.03125);
        c.r = (c.r - from.r) * t + from.r;
        c.g = (c.g - from.g) * t + from.g;
        c.b = (c.b - from.b) * t + from.b;
        c.a = (c.a - from.a) * t + from.a;
        a.gradient[i as usize] = c;
        a.version += 1;
    }
    d
}

impl SplashFx {
    /// `LoadTargetEffect`: None when the dictionary has no such surface
    /// (`gTargetEffectDef` -1: no splash is ever placed).
    pub fn load(lib: &esf::Library, time: i64) -> Option<SplashFx> {
        lib.surface(SPLASH_TEXTURE as i32)?;
        let mut sys = vi::particle::System::new(time, 2000);
        let def = sys.add_definition(splash_definition());
        let emitter = sys.create_emitter(def);
        Some(SplashFx { sys, rand: vi::csprite::Rand(1), emitter })
    }

    /// `Splash` on water: the emitter purged, located at `at` (twice, so it
    /// has no trail behind it), aimed with the identity.
    pub fn place(&mut self, at: Vect3<Ee>) {
        self.sys.purge(self.emitter);
        let e = self.sys.emitter(self.emitter);
        e.set_location(at);
        e.set_location(at);
        e.direction = vi::particle::Matrix33::identity();
    }

    /// `sTargetEmitter.Update()`: a frame's births.
    pub fn update(&mut self) {
        self.sys.update(self.emitter, &mut self.rand);
    }

    /// The particles live, rendered at scene time `time` (ms).
    pub fn render(&mut self, time: i64) -> Vec<render::billboard::Batch> {
        crate::particles::render(&mut self.sys, time)
    }

    /// How many particles are live.
    pub fn count(&self) -> usize {
        self.sys.particle_count(self.emitter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};
    use net::dump::Item;

    fn record(id: i64, quantity: i64, key: u32, name: &str) -> NetItem {
        let item = Item { id, max_stack: 50, name: name.into(), wear_slot: -1, rent: 1, icon: 100 + id, ..Default::default() };
        NetItem { quantity, equip_slot: -1, quick_slot: -1, inv_index: key, item, ..Default::default() }
    }

    /// A worn pole (wear 20 in equipment slot 14, `hp` hit points) and a
    /// worn bait (wear 21 in 15), each when asked for.
    fn gear(pole: Option<i64>, bait: bool) -> ItemList {
        let mut l = ItemList::default();
        if let Some(hp) = pole {
            let mut p = record(1806, 1, 1, "Fishing Pole");
            p.item.wear_slot = WEAR_POLE;
            p.equip_slot = EQUIP_POLE;
            p.cur_hp = hp;
            l.items.push(p);
        }
        if bait {
            let mut b = record(1807, 20, 2, "can of worms");
            b.item.wear_slot = WEAR_BAIT;
            b.equip_slot = 15;
            l.items.push(b);
        }
        l
    }

    fn info<'a>(vui: &'a Vui, inv: &'a ItemList, base: &'a [i32; 29]) -> Info<'a> {
        Info { words: &vui.words, inv, hp: [0; 4], base, class: 0, level: 1, attack_words: &[] }
    }

    /// Every 0x0F01, byte for byte: the opcode (0x0F01 little-endian), the
    /// sub-op, then Cast's quality and Keep's key and count as raw words
    /// (`WriteU32`). The control: a seven-bit (zigzag) quality, as the
    /// client writes most numbers, is not what goes.
    #[test]
    fn the_messages_to_the_server() {
        assert_eq!(cast_msg(5), vec![0x01, 0x0F, 1, 5, 0, 0, 0]);
        assert_eq!(cast_msg(0), vec![0x01, 0x0F, 1, 0, 0, 0, 0]);
        assert_eq!(sub_msg(2), vec![0x01, 0x0F, 2], "Cancel");
        assert_eq!(sub_msg(3), vec![0x01, 0x0F, 3], "Reel");
        assert_eq!(sub_msg(4), vec![0x01, 0x0F, 4], "Pull");
        assert_eq!(keep_msg(0x0102_0304, 3), vec![0x01, 0x0F, 5, 4, 3, 2, 1, 3, 0, 0, 0]);
        let mut w = Writer::op(OP_OUT);
        w.u8(1).signed(15);
        assert_ne!(w.data, cast_msg(15), "the control: zigzag is another message");
    }

    /// `Cast(quality)` checks the pole and the bait: none worn (1464), no
    /// bait (1465), a pole of no hit points (1466) send nothing and leave
    /// the state; with both, word 1433 for a perfect cast (0), 1432 for any
    /// other, the cast sent, state 2. The control: the same item carried but
    /// not worn (equipment slot -1) is no pole.
    #[test]
    fn a_cast_needs_a_worn_pole_and_bait() {
        let mut p = Part { state: state::WAITING, ..Default::default() };
        assert_eq!(p.cast(&gear(None, true), 0), (1464, None));
        assert_eq!(p.cast(&gear(Some(1000), false), 0), (1465, None));
        assert_eq!(p.cast(&gear(Some(0), true), 0), (1466, None));
        assert_eq!(p.state, state::WAITING);
        let mut carried = gear(Some(1000), true);
        carried.items[0].equip_slot = -1;
        assert_eq!(p.cast(&carried, 0), (1464, None), "the control: carried, not worn");
        assert_eq!(p.cast(&gear(Some(1000), true), 0), (1433, Some(cast_msg(0))));
        assert_eq!(p.state, state::CAST);
        assert_eq!(p.cast(&gear(Some(1), true), 15), (1432, Some(cast_msg(15))));
    }

    /// `Keep` sends the record's key (`GetSlotId`, not the slot) and goes to
    /// 4, and does nothing more until the server answers; the control: a
    /// second Keep while keeping sends nothing.
    #[test]
    fn keeping_one_waits_for_the_answer() {
        let mut p = Part { state: state::CAUGHT, catch: ItemList { p04: 0, items: vec![record(1329, 3, 40, "grass carp"), record(1223, 1, 41, "soup")] } };
        assert_eq!(p.keep(1, 1), Some(keep_msg(41, 1)));
        assert_eq!(p.state, state::KEEPING);
        assert_eq!(p.keep(0, 3), None, "the control: one at a time");
    }

    /// `Splash`'s thresholds, unsigned: 56..63 a perfect cast (0), 41..78 a
    /// fair one (5), else 15, at each edge. The control: the same tests
    /// signed would make every power under 56 perfect.
    #[test]
    fn the_casts_quality_by_power() {
        let want = [(0, 15), (40, 15), (41, 5), (55, 5), (56, 0), (63, 0), (64, 5), (78, 5), (79, 15), (120, 15)];
        for (power, q) in want {
            assert_eq!(quality(power), q, "power {power}");
        }
        let signed = |p: i32| if p - 56 < 8 { 0 } else if p - 41 < 38 { 5 } else { 15 };
        assert_ne!(signed(40), quality(40), "the control: signed, 40 is perfect");
    }

    /// `Tick` at its boundaries, on the tick clock (983.04 a second): the
    /// meter stays full to 500, falls from 501 (119) to 1500 (0), rises from
    /// 1501 to 2499 (119), where a release throws (the power then is the
    /// cast's); at 2500 it throws itself full. From the throw: nothing to
    /// 1000, the splash 1001..1500 (on water), the cast over past 2500. The
    /// controls: a release while it falls throws nothing (and is
    /// forgotten), and so does the stick pushed only to -0.25.
    #[test]
    fn the_meter_and_the_splash_at_their_boundaries() {
        let still = F::<Ee>::ZERO;
        let mut w = FishingWnd::default();
        w.cast(1000);
        let at = |w: &mut FishingWnd, dt: i64| {
            let threw = w.tick(1000 + dt, still);
            (w.power, threw)
        };
        assert_eq!(at(&mut w, 500), (120, false));
        assert_eq!(at(&mut w, 501), (119, false));
        assert_eq!(at(&mut w, 1000), (60, false));
        w.release = true;
        assert_eq!(at(&mut w, 1500), (0, false), "the control: a release while it falls");
        assert!(!w.release, "and it is forgotten");
        assert_eq!(at(&mut w, 1501), (0, false));
        assert_eq!(at(&mut w, 2000), (60, false));
        assert_eq!(at(&mut w, 2499), (119, false));
        assert!(!w.tick(1000 + 2200, F::new(-0.25)), "the control: -0.25 is not past it");
        assert_eq!(w.tick(1000 + 2200, F::new(-0.26)), true, "the stick pushed forward throws");
        // 0.7 x 120 chopped, as the EE's multiply leaves it: 83 (the
        // control: the host's IEEE rounding would make it 84)
        assert_eq!((w.power, w.quality, w.splash_time), (83, 15, 3200));
        assert_eq!((700.0f32 / 1000.0 * 120.0) as i32, 84, "the control: IEEE");
        // thrown: the meter holds, the splash after 1000 on water only
        w.water(Some(Vect3::of(1.0, 2.0, 3.0)), true);
        for (d, emit, casting) in [(1000, false, true), (1001, true, true), (1500, true, true), (1501, false, true), (2500, false, true), (2501, false, false)] {
            w.tick(3200 + d, still);
            assert_eq!((w.emitting, w.casting), (emit, casting), "{d} after the throw");
        }
        // full at the top: thrown itself at 2500, a power of 120
        let mut v = FishingWnd::default();
        v.cast(0);
        assert!(!v.tick(2499, still));
        assert!(v.tick(2500, still));
        assert_eq!((v.power, v.quality), (120, 15));
        // a release in the rising part: thrown at that power
        let mut u = FishingWnd::default();
        u.cast(0);
        u.tick(2058, still);
        u.release = true;
        assert!(u.tick(2059, still));
        assert_eq!((u.power, u.quality), (67, 5));
        // off the water: no splash however long
        let mut o = FishingWnd::default();
        o.cast(0);
        assert!(o.tick(2500, still));
        o.water(None, true);
        o.tick(3800, still);
        assert!(!o.emitting && !o.hit);
    }

    /// The catch list (`FillBait`) out of a real catch (0x0F02 7's bytes,
    /// as `VIItemList::Read` reads them): "TAKE ALL" (118) on top, a stack
    /// with its count (type 5), one alone (1), "LEAVE ALL" (119), then
    /// empty rows to 42; no catch, "Empty" (641) alone. The selected item is
    /// described (`DescribeItem`); the heading row, nothing.
    #[test]
    fn the_catch_list_from_a_catch() {
        let mut w = Writer::default();
        w.u8(7).signed(2).u32(2);
        for (id, n, key, name) in [(1329i64, 3i64, 10u32, "grass carp"), (1223, 1, 11, "sondereel fish soup")] {
            w.signed(n).signed(0).signed(0).signed(-1).signed(-1).u32(key);
            for v in [id, 0, 0, 7000 + id, 0, -1, 0, 0, 1, 0, 0, 0, 0, 1, 50, 0, 0, 0x7FFF, 0x3FF, 0, 0, 0, 0] {
                w.signed(v);
            }
            w.wide(name).wide("").signed(0);
        }
        let Some(Recv::Loot(catch)) = read_0f02(&w.data) else { panic!("not read") };
        let vui = vui();
        let base = [0; 29];
        let inv = ItemList::default();
        let mut f = FishingWnd::default();
        f.fill_bait(&catch, &info(&vui, &inv, &base), &|it| it.item.icon as i32);
        let rows: Vec<_> = f.list.items[..5].iter().map(|r| (r.flags, r.icon, r.word, r.number)).collect();
        assert_eq!(rows, vec![(2, -1, 118, 0), (5, 8329, -1, 3), (1, 8223, -1, 1), (2, -1, 119, 0), (0, -1, -1, 0)]);
        assert!(f.list.items[4..].iter().all(|r| r.flags == 0) && f.list.items.len() == ROWS);
        assert!(f.desc.is_empty(), "the heading: nothing to describe");
        f.list.sel = 2;
        f.fill_bait(&catch, &info(&vui, &inv, &base), &|it| it.item.icon as i32);
        assert_eq!(String::from_utf16_lossy(&f.desc), "sondereel fish soup");
        f.fill_bait(&ItemList::default(), &info(&vui, &inv, &base), &|_| 0);
        assert_eq!((f.list.items[0].word, f.list.items[1].flags), (641, 0), "the control: no catch, Empty alone");
    }

    /// The catch list's keys: Enter on the top row Reels (3), on an item
    /// Keeps it (5, its key and count), on the row after them Pulls (4);
    /// each with sound 13. Escape (△) is the window's: with the catch up
    /// (state 3) Pull, else Cancel (2). The control: ✕ with no list is the
    /// cast, and sends nothing.
    #[test]
    fn the_catch_lists_rows_and_the_back_key() {
        let (vui, r) = (vui(), NullRaster);
        let mut part = Part { state: state::CAUGHT, catch: ItemList { p04: 0, items: vec![record(1329, 3, 40, "grass carp"), record(1223, 1, 41, "soup")] } };
        let mut w = FishingWnd::default();
        let mut cues = Vec::new();
        w.open_bait(None, 0, &mut cues);
        let press = |w: &mut FishingWnd, part: &mut Part, cues: &mut Vec<i32>, sel: i32, key: i32| {
            w.list.sel = sel;
            let mut out = Vec::new();
            w.key(&vui, &r, part, None, key, true, 0, 0, cues, &mut out);
            out
        };
        assert_eq!(press(&mut w, &mut part, &mut cues, 0, 134), vec![FishOut::Send(sub_msg(3))]);
        assert_eq!(press(&mut w, &mut part, &mut cues, 1, 13), vec![FishOut::Send(keep_msg(40, 3))]);
        part.state = state::CAUGHT;
        assert_eq!(press(&mut w, &mut part, &mut cues, 3, 134), vec![FishOut::Send(sub_msg(4))]);
        assert_eq!(press(&mut w, &mut part, &mut cues, 4, 134), vec![], "past the rows: nothing");
        assert_eq!(press(&mut w, &mut part, &mut cues, 0, 132), vec![FishOut::Send(sub_msg(4))], "△ with the catch up");
        part.state = state::WAITING;
        assert_eq!(press(&mut w, &mut part, &mut cues, 0, 27), vec![FishOut::Send(sub_msg(2))], "Escape: Cancel");
        assert_eq!(cues, vec![15, 13, 13, 13, 13, 8, 8]);
        w.close_bait();
        assert_eq!(press(&mut w, &mut part, &mut cues, 0, 134), vec![], "the control: ✕ casts");
        assert!(w.casting && w.power == 120);
    }

    /// The window through a pointer: with the catch up, a click selects the
    /// list's row where it was drawn and a second one picks it; with the
    /// edit box up its own; with neither nothing (the legend's glyphs are
    /// the HUD's). The control: before a draw has noted the rows, a click
    /// finds none.
    #[test]
    fn a_pointer_on_the_catch() {
        use crate::pointer::{MOVED, key};
        let (vui, mut r) = (vui(), NullRaster);
        let base = [0; 29];
        let inv = gear(Some(1000), true);
        let mut part = Part { state: state::CAUGHT, catch: ItemList { p04: 0, items: vec![record(1329, 3, 40, "grass carp")] } };
        let mut w = FishingWnd::default();
        let mut cues = Vec::new();
        w.focus(&mut cues);
        w.open_bait(None, 0, &mut cues);
        let row = |i: i32| [10.0 + 57.0, 58.0 + 44.0 * i as f32 + 22.0];
        assert!(w.point(row(1)).is_empty(), "the control: nothing drawn yet");
        let mut out = Vec::new();
        w.draw(&vui, &mut r, &mut part, &info(&vui, &inv, &base), &|_| 1, 0, 0, F::ZERO, &mut cues, &mut out);
        assert_eq!(w.point(row(1)), vec![MOVED]);
        assert_eq!(w.list.sel, 1);
        assert_eq!(w.point(row(1)), vec![key::CROSS]);
        assert!(w.point(row(3)).is_empty(), "past LEAVE ALL: no row");
        w.close_bait();
        assert!(w.point(row(1)).is_empty(), "no list, no rows");
    }

    /// `Rename`: Enter (or R3) opens the edit box (not with the catch up),
    /// what it is given typed, and its Done sends the line (`ChatLine`);
    /// the next opens empty. The controls: with the catch up
    /// (state 3) Enter opens nothing, and the box's Cancel sends nothing.
    #[test]
    fn the_chat_line() {
        let (vui, r) = (vui(), NullRaster);
        let mut part = Part { state: state::WAITING, ..Default::default() };
        let mut w = FishingWnd::default();
        let mut cues = Vec::new();
        let mut out = Vec::new();
        for k in [13, 'h' as i32, 'i' as i32, 13] {
            w.key(&vui, &r, &mut part, None, k, true, 0, 0, &mut cues, &mut out);
        }
        assert_eq!(out, vec![FishOut::ChatLine("hi".into())]);
        assert!(!w.edit_visible);
        out.clear();
        w.key(&vui, &r, &mut part, None, 147, true, 0, 0, &mut cues, &mut out);
        assert!(w.edit_visible && w.edit.text.is_empty(), "R3: the box, empty again");
        w.key(&vui, &r, &mut part, None, 27, true, 0, 0, &mut cues, &mut out);
        assert!(out.is_empty() && !w.edit_visible, "the control: cancelled, nothing said");
        part.state = state::CAUGHT;
        w.key(&vui, &r, &mut part, None, 13, true, 0, 0, &mut cues, &mut out);
        assert!(!w.edit_visible, "the control: not with the catch up");
    }

    /// C (or R2) held scrolls the chat box (`ChatOn`): its up and down go to
    /// the box while held, and letting go ends the scroll and lets the
    /// arrow go. The control: up and down without C held do nothing to it.
    #[test]
    fn c_held_scrolls_the_chat_box() {
        let (vui, r) = (vui(), NullRaster);
        let mut chat = WndChat::new();
        for i in 0..8 {
            chat.text.add_text(&wide(&format!("line {i}")), [1.0; 4], 0, |s| s.len() as i32 * 8);
        }
        let mut part = Part { state: state::WAITING, ..Default::default() };
        let mut w = FishingWnd::default();
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        w.key(&vui, &r, &mut part, Some(&mut chat), 19, true, 0, 0, &mut cues, &mut out);
        assert!(chat.cues.is_empty(), "the control: not scrolling");
        w.key(&vui, &r, &mut part, Some(&mut chat), 99, true, 0, 0, &mut cues, &mut out);
        assert!(w.chat_on);
        w.key(&vui, &r, &mut part, Some(&mut chat), 19, true, 0, 0, &mut cues, &mut out);
        assert_eq!((chat.cues.clone(), w.chat_key), (vec![11], 19), "a line up");
        w.key(&vui, &r, &mut part, Some(&mut chat), 99, false, 0, 0, &mut cues, &mut out);
        assert!(!w.chat_on && w.chat_key == 0);
    }

    /// The message box: a line shows 5,000 ticks (5.09 s) on VITimer's
    /// clock, then goes; the control: at 4,999 it is still up.
    #[test]
    fn a_message_lasts_five_thousand_ticks() {
        let (vui, mut r) = (vui(), NullRaster);
        let base = [0; 29];
        let inv = ItemList::default();
        let mut part = Part::default();
        let mut w = FishingWnd::default();
        let mut cues = Vec::new();
        w.focus(&mut cues);
        w.set_text(&wide("Let's go fishing!"), 100);
        let mut out = Vec::new();
        let mut draw = |w: &mut FishingWnd, now: i64| w.draw(&vui, &mut r, &mut part, &info(&vui, &inv, &base), &|_| 0, now, 0, F::ZERO, &mut cues, &mut out);
        draw(&mut w, 100 + 4999);
        assert_eq!(w.text, wide("Let's go fishing!"), "the control: still up");
        assert_eq!(w.win3.title.chars(), wide("Let's go fishing!"), "on the box's bar");
        draw(&mut w, 100 + 5000);
        assert!(w.text.is_empty() && w.text_end == 0);
    }

    /// The meter's power rows: `power` of them up from 264, blue at the top
    /// to white at the bottom at 0.7; none at 0 (`if (m_power)`).
    #[test]
    fn the_meters_ramp() {
        let rows = meter_rows(120);
        assert_eq!((rows.len(), rows[0].0, rows[119].0), (120, 144, 263));
        assert!(rows[0].1[0] < 0.01 && rows[0].1[2] == 1.0, "blue at the top");
        assert!(rows[119].1[0] > 0.99 && rows[119].1.iter().all(|&c| c > 0.69), "white at the bottom");
        assert!(rows.windows(2).all(|w| w[0].1[0] < w[1].1[0]), "lighter down");
        assert_eq!(meter_rows(1), vec![(263, [0.5, 0.5, 1.0, 0.7])]);
    }

    /// `LoadTargetEffect`'s definition: its texture, blend 1, the fountain's
    /// attributes and the 32-step ramp from (0.8, 0.8, 1) to white. An
    /// emitter on it placed and updated a second at 30 frames makes about
    /// a hundred particles; the control: not updated, none.
    #[test]
    fn the_splash() {
        let d = splash_definition();
        assert_eq!((d.texture, d.blend, d.zwrite, d.ztest, d.tex_config), (SPLASH_TEXTURE, 1, false, true, 1));
        let a = &d.attrs;
        assert_eq!((a.birthrate.0, a.lifespan.0, a.start_size.0, a.end_size.0, a.gravity.0), (100.0, 0.8, 0.2, 0.2, -5.0));
        assert_eq!((a.nozzle_axis.y.0, a.nozzle_hpr_var.x.0, a.nozzle_hpr_var_set), (1.0, 0.39, 1));
        assert_eq!((a.gradient[0].r.0, a.gradient[0].b.0), (0.8, 1.0));
        assert!((a.gradient[31].r.0 - 0.99375).abs() < 1e-6 && a.gradient[16].g.0 == 0.9);
        let mut sys = vi::particle::System::<Ee>::new(0, 2000);
        let id = sys.add_definition(d);
        let e = sys.create_emitter(id);
        let mut fx = SplashFx { sys, rand: vi::csprite::Rand(1), emitter: e };
        fx.place(Vect3::of(10.0, 51.32, 20.0));
        let mut n = 0;
        for k in 1..=30 {
            fx.render(k * 33);
            fx.update();
            n = n.max(fx.count());
        }
        assert!((70..=110).contains(&n), "{n} particles live");
        let mut quiet = SplashFx { sys: vi::particle::System::new(0, 2000), rand: vi::csprite::Rand(1), emitter: 0 };
        let d2 = quiet.sys.add_definition(splash_definition());
        quiet.emitter = quiet.sys.create_emitter(d2);
        quiet.render(500);
        assert_eq!(quiet.count(), 0, "the control: no births without Update");
    }

    /// `FindWater` on the real Freeport docks (TUNARIA.ESF round the dock
    /// Ltcomdata stands on), every one of its outcomes: on the planks over
    /// the sea at (25400, 15816.95) a cast lands on the water (y 51.32,
    /// the sea's surface: water just under it, not just over) in eleven of
    /// sixteen headings; at the pier's foot (25330, 15771.95) a cast inland
    /// is blocked by the first ray; out over the planks at (25505, 15721.95)
    /// the water under the end is under the deck (solid first); on the roof
    /// at (25340, 15721.95) there is no water at all. The controls: where it
    /// says solid or blocked, the environment ray alone does find the water,
    /// so it is the solid and the first ray that refuse it.
    #[test]
    fn find_water_on_freeports_docks() {
        let (x, z) = (25330.0f32, 15721.95f32);
        let Some(mut w) = crate::walker::tests::tunaria_round(x, z) else { return };
        let at = |w: &mut World<Ee>, px: f32, pz: f32| -> Vec<Landing> {
            let y = crate::walker::ground(w, px, pz).expect("ground");
            (0..16).map(|k| landing(w, -1, Vect3::of(px, y, pz), Vect3::of(k as f32 * std::f32::consts::PI / 8.0, 0.0, 0.0))).collect()
        };
        let code = |l: &[Landing]| -> String {
            l.iter().map(|l| match l { Landing::Blocked => 'B', Landing::NoWater => '-', Landing::Solid => 'S', Landing::Water(_) => 'W' }).collect()
        };
        let planks = at(&mut w, 25400.0, 15816.95);
        assert_eq!(code(&planks), "WWWWBWSSSSWWWWWW");
        let Landing::Water(p) = planks[0] else { unreachable!() };
        assert!((p.y.0 - 51.32).abs() < 0.01, "{p:?}");
        assert_eq!(w.env_flags(&Vect3::of(p.x.0, p.y.0 - 0.05, p.z.0)) & 1, 1, "water under the surface");
        assert_eq!(w.env_flags(&Vect3::of(p.x.0, p.y.0 + 0.05, p.z.0)) & 1, 0, "not over it");
        assert_eq!(code(&at(&mut w, 25330.0, 15771.95)), "WWWWWWWWBBBBBBBW");
        assert_eq!(code(&at(&mut w, 25505.0, 15721.95)), "SBWSSSSSSSSSSSSS");
        assert_eq!(code(&at(&mut w, 25340.0, 15721.95)), "-----BBBBB------");
        // the controls: the environment ray alone at a solid and a blocked end
        let env_alone = |w: &mut World<Ee>, px: f32, pz: f32, h: f32| {
            let y = crate::walker::ground(w, px, pz).unwrap();
            let mut d = Vect3::<Ee>::default();
            d.hpr(F::new(h), F::ZERO, F::ZERO);
            let mut end = Vect3::<Ee>::of(px, y, pz);
            end.add_scaled(&d, F::new(10.0));
            let mut env = EnvRay::default();
            env.init(Vect3::new(end.x, end.y + F::ONE, end.z), Vect3::new(end.x, end.y - F::new(10.0), end.z), 3);
            w.collide_env(&mut env)
        };
        assert!(env_alone(&mut w, 25505.0, 15721.95, 0.0), "under the deck there is water");
        assert!(env_alone(&mut w, 25400.0, 15816.95, std::f32::consts::PI / 2.0), "past the post there is water");
    }

    /// The window steps on the game's frames (`frames.rs`): drawn on the
    /// HUD's clock, its frame opens in seven of them (0.15 a draw, 233 ms)
    /// and a cast left alone throws itself full at its 2,500th tick (2.54
    /// s), at a 165 Hz display as at 30. The control: drawn every frame of a
    /// 165 Hz display it opened in 42 ms.
    #[test]
    fn the_window_keeps_the_consoles_time_at_any_rate() {
        let run = |hz: f64, gated: bool| -> (f64, f64) {
            let (vui, mut r) = (vui(), NullRaster);
            let base = [0; 29];
            let inv = gear(Some(1000), true);
            let mut part = Part { state: state::WAITING, ..Default::default() };
            let mut w = FishingWnd::default();
            let (mut cues, mut out) = (Vec::new(), Vec::new());
            w.focus(&mut cues);
            let mut clock = crate::frames::Frames::default();
            let (mut opened, mut threw) = (None, None);
            let mut k = 0.0;
            while threw.is_none() && k < 2000.0 {
                let t = k * 1000.0 / hz;
                k += 1.0;
                if !clock.due(t) && gated {
                    continue;
                }
                // the tick clock: 983.04 a second
                let now = (t * 0.98304) as i64;
                if t == 0.0 {
                    w.cast(now);
                }
                w.draw(&vui, &mut r, &mut part, &info(&vui, &inv, &base), &|_| 0, now, t as i64, F::ZERO, &mut cues, &mut out);
                if opened.is_none() && w.window.is_open() {
                    opened = Some(t);
                }
                if out.contains(&FishOut::Splash) {
                    threw = Some(t);
                }
            }
            (opened.unwrap_or(-1.0), threw.unwrap_or(-1.0))
        };
        for hz in [30.0, 60.0, 144.0, 165.0] {
            let (open, throw) = run(hz, true);
            assert!((open - 200.0).abs() <= 1000.0 / 30.0 + 1.0, "{hz} Hz: open at {open} ms");
            assert!((throw - 2543.1).abs() <= 1000.0 / 30.0 + 1.0, "{hz} Hz: thrown at {throw} ms");
        }
        let (open, _) = run(165.0, false);
        assert!(open < 50.0, "the control: every display frame, open at {open} ms");
    }

    /// The splash's texture (`FindTyped(0x2B6BF329, 1)`) is SPELLFX.CSF's,
    /// which play's library holds from the start (`main.rs`'s
    /// `world_library`), so the splash is made; the control: none of the
    /// other resident files, nor the world, has it.
    #[test]
    fn the_splash_texture_is_spellfxs() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let mut disc = iso::Iso::open(&iso).unwrap();
        for f in ["CHAR.ESF", "ITEM.ESF", "UI.CSF", "ITEMICON.ESF", "SPELLFX.CSF", "FX.CSF", "ADDART.CSF", "TUNARIA.ESF"] {
            let mut lib = esf::Library::new();
            lib.add(f, crate::stream::open_file(&iso, f).unwrap_or_else(|_| esf::Container::open(disc.read(f).unwrap()).unwrap()));
            assert_eq!(lib.surface(SPLASH_TEXTURE as i32).is_some(), f == "SPELLFX.CSF", "{f}");
        }
    }
}
