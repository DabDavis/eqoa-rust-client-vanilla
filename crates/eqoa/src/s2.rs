//! A server's patch, S2, as the client a server patches plays: Sandstorm's
//! players run the retail disc with S2 applied over the patch server, and S2
//! changes things retail fixes. On a PS2 the patcher only puts S2 on the
//! card; a loader in the launcher reads SS off the card, SS installs a stub
//! and a hook in CLIENT, and the stub, once SUPPORT is resident, writes S2's
//! records over CLIENT, SUPPORT and a 32 KB arena of new code and tables.
//! This client runs no MIPS, so it reads the same records the other way
//! round: `Patch::parse` checks an S2 as the stub checks it, `Patch::apply`
//! writes it into an `Image` as the stub writes it (records, then the
//! relocations), and each feature is read at the address S2 hooks, the hook's
//! table, limit or string being what the client then uses. A hook that is
//! absent, or not the shape its reader knows, leaves that feature retail's.
//!
//! What it changes here (`Rules`): the armour tints (30, the clamp gone), the
//! armour set clamp (set 9), set 9's helm, the robes (4 and 5 out of the
//! patch's art file), the damage number's colour, the Auctions window's Max.
//! Level, the world table (8 worlds), and CLIENT's data as the patch leaves
//! it (`image`: the Group menu's Search, Register and Unregister, read by
//! `client_image` like any other menu). What a patch server delivers is kept
//! in the server's own folder; with no S2 there, or one its stub would
//! refuse, the client is retail's, as before. Not taken from S2, on purpose:
//! its identity and telemetry layer (DiscVersion 21 with a hardware address
//! and an install id, the pad timing on UseSkill, 0x07E1's heap lines, the
//! 0x1505/0x1507 crash reports) and its PS2 memory layer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// CLIENT's SNR1 header in every EE dump and in the decomp's addresses: S2's
/// CLIENT-relative records are kept at this plus their offset.
pub const CLIENT_BASE: u32 = 0x00BB4980;

/// The cave window: a record or relocation whose address, high bit cleared,
/// lies in `[CAVE_LO, CAVE_LO + CAVE_SIZE)` is in the arena the stub
/// allocates.
const CAVE_LO: u32 = 0x000F_8000;
const CAVE_SIZE: u32 = 0x8000;

/// The stub's staging buffer, `malloc(0x40000)`: `VIMemCardReadFile` reads
/// at most this much, and a read that fills it is refused.
const MAX_FILE: usize = 0x40000;

/// "RLC1", the relocation table's tag.
const RLC1: u32 = 0x3143_4C52;

/// One (address, data) pair of the patch, its address as the file gives it:
/// high bit clear is CLIENT-relative, set is absolute (or a cave).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub addr: u32,
    pub data: Vec<u8>,
}

/// A parsed S2.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Patch {
    pub records: Vec<Record>,
    pub relocs: Vec<u32>,
}

fn le(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

impl Patch {
    /// An S2 checked as the stub checks it before it disables interrupts and
    /// writes anything (ss-stage1-6): fewer than 8 bytes, or exactly the
    /// 0x40000 its read buffer holds, is refused; each record is an 8-byte
    /// header and its data padded to 4, and must fit in the file, a cave
    /// record in the 32 KB window too; the records end at (0, 0), then
    /// "RLC1", a count, and exactly that many relocation words to the end.
    /// The stub applies nothing unless all of it holds; neither does this.
    pub fn parse(data: &[u8]) -> Result<Patch, String> {
        let n = data.len();
        if !(8..MAX_FILE).contains(&n) {
            return Err(format!("{n} bytes"));
        }
        let mut p = Patch::default();
        let mut pos = 0;
        loop {
            if n < pos + 8 {
                return Err("a record header runs past the end".into());
            }
            let (addr, size) = (le(data, pos), le(data, pos + 4));
            pos += 8;
            if addr | size == 0 {
                break;
            }
            if addr & 0x8000_0000 != 0 {
                let off = (addr & 0x7FFF_FFFF).wrapping_sub(CAVE_LO);
                if off < CAVE_SIZE && off as u64 + size as u64 > CAVE_SIZE as u64 {
                    return Err(format!("record {addr:08X} runs out of the cave"));
                }
            }
            if ((n - pos) as u64) < size as u64 {
                return Err(format!("record {addr:08X} runs past the end"));
            }
            p.records.push(Record { addr, data: data[pos..pos + size as usize].to_vec() });
            pos += (size as usize + 3) & !3;
        }
        let rest = n.saturating_sub(pos);
        if rest < 8 || le(data, pos) != RLC1 {
            return Err("no RLC1 after the records".into());
        }
        let count = le(data, pos + 4) as u64;
        if count * 4 + 8 != rest as u64 {
            return Err(format!("{count} relocations do not fill the {} bytes left", rest - 8));
        }
        p.relocs = (0..count as usize).map(|i| le(data, pos + 8 + 4 * i)).collect();
        Ok(p)
    }

    /// The records written, then the relocations, as the stub does, with the
    /// arena at `CAVE_LO + delta` (always a multiple of 0x10000 there). The
    /// features are read off an image applied at delta 0, where every cave
    /// pointer is the address the file was linked at.
    pub fn apply(&self, delta: u32) -> Image {
        let mut img = Image::default();
        for r in &self.records {
            let a = resolve(r.addr, delta);
            for (i, &b) in r.data.iter().enumerate() {
                img.mem.insert(a.wrapping_add(i as u32), b);
            }
            img.records.push((a, a.wrapping_add(r.data.len() as u32)));
        }
        for &w in &self.relocs {
            let a = resolve((w & 0x7FFF_FFFC) | (w & 0x8000_0000), delta);
            // a word of retail code or data: not this client's to have
            let Some(v) = img.word(a) else { continue };
            let v = match w & 3 {
                0 => v.wrapping_add(delta),
                // a J-type target
                1 => {
                    let target = (((v & 0x03FF_FFFF) << 2).wrapping_add(delta)) & 0x0FFF_FFFF;
                    (v & 0xFC00_0000) | (target >> 2)
                }
                // the lui half of a lui/addiu pair; 3 is taken as 2
                _ => (v & 0xFFFF_0000) | ((v & 0xFFFF).wrapping_add(delta >> 16) & 0xFFFF),
            };
            img.put_word(a, v);
        }
        img
    }
}

/// Where the stub writes an address: CLIENT-relative onto `CLIENT_BASE`, a
/// cave address moved by `delta`, anything else as it is.
fn resolve(addr: u32, delta: u32) -> u32 {
    if addr & 0x8000_0000 == 0 {
        return addr.wrapping_add(CLIENT_BASE);
    }
    let a = addr & 0x7FFF_FFFF;
    if a.wrapping_sub(CAVE_LO) < CAVE_SIZE { a + delta } else { a }
}

/// The EE memory the patch writes, and only that: every byte a record put
/// there (the rest of memory is not this client's to have).
#[derive(Clone, Debug, Default)]
pub struct Image {
    mem: HashMap<u32, u8>,
    /// where each record landed, in file order
    records: Vec<(u32, u32)>,
}

impl Image {
    /// A little-endian word, when the patch wrote all of it.
    pub fn word(&self, a: u32) -> Option<u32> {
        let mut v = 0u32;
        for i in 0..4 {
            v |= (*self.mem.get(&a.wrapping_add(i))? as u32) << (8 * i);
        }
        Some(v)
    }

    /// A byte the patch wrote.
    pub fn byte(&self, a: u32) -> Option<u8> {
        self.mem.get(&a).copied()
    }

    fn put_word(&mut self, a: u32, v: u32) {
        for i in 0..4 {
            self.mem.insert(a.wrapping_add(i), (v >> (8 * i)) as u8);
        }
    }

    /// `n` patched bytes at `a`.
    pub fn bytes(&self, a: u32, n: usize) -> Option<Vec<u8>> {
        (0..n as u32).map(|i| self.mem.get(&a.wrapping_add(i)).copied()).collect()
    }

    /// The NUL-terminated string at `a`, when the patch wrote it.
    pub fn cstring(&self, a: u32) -> Option<String> {
        let mut s = Vec::new();
        for i in 0..256u32 {
            match *self.mem.get(&a.wrapping_add(i))? {
                0 => return Some(String::from_utf8_lossy(&s).into_owned()),
                b => s.push(b),
            }
        }
        None
    }

    /// The end of the record that wrote `a`: a table's length is read off
    /// the record that holds it.
    fn record_end(&self, a: u32) -> Option<u32> {
        self.records.iter().find(|&&(lo, hi)| a >= lo && a < hi).map(|&(_, hi)| hi)
    }

    /// Whether the patch put a call into its arena at `a`.
    fn jals_into_cave(&self, a: u32) -> bool {
        match self.word(a) {
            Some(w) if w >> 26 == 3 => in_cave(j_target(a, w)),
            _ => false,
        }
    }

    /// Where a hook's j or jal at `a` goes.
    fn jump_target(&self, a: u32) -> Option<u32> {
        let w = self.word(a)?;
        (w >> 26 == 2 || w >> 26 == 3).then(|| j_target(a, w))
    }

    /// `f` over straight-line code from `a`, in address order (branches are
    /// not followed, which is what the hooks' short caves need), with the
    /// registers as the constants so far leave them. A jal clobbers the
    /// caller's registers once `f` has seen it (its delay slot in
    /// `Regs::after`). It stops at jr ra, at a j, at an unpatched word, after
    /// `n` words, or when `f` says so.
    fn walk(&self, a: u32, n: usize, mut f: impl FnMut(u32, u32, &Regs) -> bool) {
        let mut r = Regs::default();
        let mut i = 0;
        while i < n {
            let pc = a + 4 * i as u32;
            let Some(w) = self.word(pc) else { return };
            if w >> 26 == 3 {
                // jal: the delay slot runs first
                let Some(ds) = self.word(pc + 4) else { return };
                let mut after = r.clone();
                after.delay = None;
                after.step(ds);
                r.delay = Some(Box::new(after.clone()));
                if !f(pc, w, &r) {
                    return;
                }
                r = after;
                for k in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25, 31] {
                    r.forget(k); // what a call may change
                }
                i += 2; // the delay slot is done
                continue;
            }
            if !f(pc, w, &r) {
                return;
            }
            r.step(w);
            if w == 0x03E0_0008 || w >> 26 == 2 {
                // jr ra, j: the delay slot, then stop
                if let Some(ds) = self.word(pc + 4) {
                    f(pc + 4, ds, &r);
                }
                return;
            }
            i += 1;
        }
    }
}

fn in_cave(a: u32) -> bool {
    a.wrapping_sub(CAVE_LO) < CAVE_SIZE
}

fn j_target(pc: u32, w: u32) -> u32 {
    (pc.wrapping_add(4) & 0xF000_0000) | ((w & 0x03FF_FFFF) << 2)
}

/// `lui r, hi` then `ori r, r, lo` or `addiu r, r, lo`: the address.
fn lui_pair(hi: u32, lo: u32) -> Option<u32> {
    let r = (hi >> 16) & 31;
    if hi >> 26 != 15 || (lo >> 21) & 31 != r || (lo >> 16) & 31 != r {
        return None;
    }
    match lo >> 26 {
        13 => Some((hi << 16) | (lo & 0xFFFF)),
        9 => Some((hi << 16).wrapping_add(lo as u16 as i16 as i32 as u32)),
        _ => None,
    }
}

/// What a walk knows of the registers: the ones that hold a constant.
#[derive(Clone, Debug, Default)]
struct Regs {
    val: [u32; 32],
    known: [bool; 32],
    /// the registers as they stand after a jal's delay slot
    delay: Option<Box<Regs>>,
}

impl Regs {
    fn get(&self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.known[n as usize].then_some(self.val[n as usize])
    }

    /// A register as the call sees it: after the delay slot.
    fn after(&self, n: u32) -> Option<u32> {
        match &self.delay {
            Some(d) => d.get(n),
            None => self.get(n),
        }
    }

    fn set(&mut self, n: u32, v: u32) {
        if n != 0 {
            self.val[n as usize] = v;
            self.known[n as usize] = true;
        }
    }

    fn forget(&mut self, n: u32) {
        self.known[n as usize] = false;
    }

    /// One instruction folded in. Only the constant builders are followed
    /// (lui, ori, addiu, and addu/daddu/or of known registers); anything else
    /// forgets what it writes.
    fn step(&mut self, w: u32) {
        let (op, rs, rt, rd) = (w >> 26, (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31);
        let imm = w & 0xFFFF;
        match op {
            15 => self.set(rt, imm << 16),
            13 => match self.get(rs) {
                Some(v) => self.set(rt, v | imm),
                None => self.forget(rt),
            },
            9 => match self.get(rs) {
                Some(v) => self.set(rt, v.wrapping_add(imm as u16 as i16 as i32 as u32)),
                None => self.forget(rt),
            },
            0 => match w & 63 {
                0x21 | 0x2D | 0x25 => match (self.get(rs), self.get(rt)) {
                    (Some(a), Some(b)) => self.set(rd, if w & 63 == 0x25 { a | b } else { a.wrapping_add(b) }),
                    _ => self.forget(rd),
                },
                // jr, syscall, break write nothing
                8 | 0x0C | 0x0D => {}
                _ => self.forget(rd),
            },
            // jal writes ra
            3 => self.forget(31),
            8 | 10 | 11 | 12 | 14 | 24 | 25 | 16 | 17 | 18 | 26 | 27 | 30 | 32..=39 | 55 => self.forget(rt),
            // MMI
            28 => self.forget(rd),
            _ => {}
        }
    }
}

/// The addresses S2 hooks, as the July capture found them and as the live S2
/// (ss-patch-60, 2026-10-15) still writes them; retail's instruction at each
/// is in the comment.
mod at {
    // SUPPORT, at its fixed base.
    /// `VICSpriteCust::GetTintColor`: lui v0, %hi($Tints) ...
    pub const GET_TINT_COLOR: u32 = 0x0042_94E8;
    /// `SetArmorSlot`+0x44: slti v0, s1, 9 — the armour set clamp
    pub const SET_ARMOR_SLOT_SET: u32 = 0x0042_2574;
    /// `SetArmorSlot`+0x58: movz s3, zero, v1 — the tint clamp
    pub const SET_ARMOR_SLOT_TINT: u32 = 0x0042_2588;
    /// `VICSpriteCust::GetHelm`
    pub const GET_HELM: u32 = 0x0042_94D0;
    /// `VICSpriteCust::GetRobeTexture`: slti v0, a0, 4
    pub const GET_ROBE_TEXTURE: u32 = 0x0042_9480;
    /// `VICSpriteCust::GetArmorSetTexture`
    pub const GET_ARMOR_SET_TEXTURE: u32 = 0x0042_93C8;
    /// The arena's table of set 9's five new DictIDs and, after it, of the
    /// CHAR.ESF records (by file offset) its SetResources hook registers
    /// under them (the July and live S2s, and our server's, all keep them
    /// here).
    pub const SET9_IDS: u32 = 0x000F_9040;
    pub const SET9_RECORDS: u32 = 0x000F_9060;
    /// `VIDictionary::FindTyped(id, type, &out)`
    pub const FIND_TYPED: u32 = 0x003E_43A8;
    /// `VIMemCardReadFile(name, buf, size)`
    pub const MEM_CARD_READ_FILE: u32 = 0x003E_8098;

    // CLIENT, by offset from its base.
    /// jal VIMemCardReadFile("addart.csf", buf, 16384)
    pub const CLIENT_ART_READ: u32 = 0x0C904;
    /// addiu a0, s2, 4: the word-826 branch for an unknown /command
    pub const CLIENT_UNKNOWN_CMD: u32 = 0x1BAA0;
    /// lhu a1, 32(sp): the head of the opcode switch
    pub const CLIENT_OPCODE_SWITCH: u32 = 0x1C4A8;
    /// lw a1, 2268(sp), in the DamageIndication handler
    pub const CLIENT_DAMAGE: u32 = 0x1E424;
    /// lui v0, %hi(the world table)
    pub const CLIENT_WORLD_HI: u32 = 0x11010;
    /// addiu v0, v0, %lo(the world table)
    pub const CLIENT_WORLD_LO: u32 = 0x11018;
    /// li a1, 50: the Auctions window's Max. Level
    pub const CLIENT_AUCTION_LEVEL: u32 = 0x5CFE4;
}

/// One entry of a world table (`gWorldInfo`'s shape, 32 bytes): what CLIENT
/// knows a world by.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    /// "tunaria"
    pub name: String,
    /// "data\tunaria.esf"
    pub esf: String,
    /// its WORDS.TXT word
    pub name_id: i32,
    /// the zone grid's columns, then rows, both bounds inclusive
    pub bounds: [i32; 4],
}

impl World {
    /// Its ESF as a file name on the disc: "data2\Rathe.esf" is "RATHE.ESF".
    pub fn esf_name(&self) -> String {
        let name = self.esf.rsplit(['\\', '/']).next().unwrap_or(&self.esf);
        name.to_ascii_uppercase()
    }
}

/// What a patch changes, for the parts this client takes; a field left at
/// its default is retail's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Features {
    /// the table GetTintColor returns into
    pub tints: Vec<[u8; 4]>,
    /// SetArmorSlot keeps a tint past 14
    pub tint_clamp_removed: bool,
    /// SetArmorSlot turns a set at or past it into 0
    pub armor_set_limit: Option<i32>,
    /// GetHelm: armour set -> the set whose helm it wears
    pub helm_remap: Vec<(u8, u8)>,
    /// GetRobeTexture's robes are 0 .. limit-1
    pub robe_limit: Option<i32>,
    /// a robe past retail's four: its (chest, legs) DictIDs
    pub robe_art: Vec<(u8, [u32; 2])>,
    /// the card file CLIENT reads its added art from
    pub art_file: Option<String>,
    /// the opcode switch takes 0x1500 / 0x1501
    pub player_search: bool,
    /// DamageIndication reads three trailing colour bytes
    pub damage_rgb: bool,
    /// an unknown /command goes to the server as say
    pub commands_to_server: bool,
    /// the world table
    pub worlds: Vec<World>,
    /// the Auctions window opens on this Max. Level
    pub auction_max_level: Option<i32>,
    /// an armour set past retail's: its five slots' DictIDs, and the
    /// CHAR.ESF records (by file offset) registered under them
    pub armor_art: Vec<(u8, [u32; 5], [u32; 5])>,
}

impl Features {
    /// The patch applied at delta 0 and its hooks read.
    pub fn read(img: &Image) -> Features {
        let mut f = Features::default();
        f.read_tints(img);
        // slti v0, s1, N
        if let Some(w) = img.word(at::SET_ARMOR_SLOT_SET)
            && w >> 16 == 0x2A22
        {
            f.armor_set_limit = Some(w as u16 as i16 as i32);
        }
        if img.word(at::SET_ARMOR_SLOT_TINT) == Some(0) {
            f.tint_clamp_removed = true;
        }
        f.read_helm(img);
        f.read_robes_and_art(img);
        f.player_search = img.jals_into_cave(CLIENT_BASE + at::CLIENT_OPCODE_SWITCH);
        f.damage_rgb = img.jals_into_cave(CLIENT_BASE + at::CLIENT_DAMAGE);
        // b: beq zero, zero
        if img.word(CLIENT_BASE + at::CLIENT_UNKNOWN_CMD).is_some_and(|w| w >> 16 == 0x1000) {
            f.commands_to_server = true;
        }
        // li a1, N
        if let Some(w) = img.word(CLIENT_BASE + at::CLIENT_AUCTION_LEVEL)
            && w >> 16 == 0x2405
        {
            f.auction_max_level = Some(w as u16 as i16 as i32);
        }
        f.read_worlds(img);
        f.read_set9(img);
        f
    }

    /// Set 9: GetArmorSetTexture jumps to the arena, which reads set 9 from
    /// a slot table its SetResources hook fills, registering five CHAR.ESF
    /// surfaces (by file offset) under five DictIDs of its own; both tables
    /// are where every build keeps them, and only whole ones count.
    fn read_set9(&mut self, img: &Image) {
        if !img.jump_target(at::GET_ARMOR_SET_TEXTURE).is_some_and(in_cave) {
            return;
        }
        let ids: Option<Vec<u32>> = (0..5).map(|i| img.word(at::SET9_IDS + 4 * i)).collect();
        let records: Option<Vec<u32>> = (0..5).map(|i| img.word(at::SET9_RECORDS + 4 * i)).collect();
        if let (Some(ids), Some(records)) = (ids, records) {
            self.armor_art.push((9, ids.try_into().unwrap(), records.try_into().unwrap()));
        }
    }

    /// GetTintColor becomes `return table + tint*4` (lui and ori or addiu of
    /// the table into one register, then sll a0, a0, 2), and the table runs
    /// to the end of the record that holds it.
    fn read_tints(&mut self, img: &Image) {
        let (Some(hi), Some(lo), Some(sll)) = (img.word(at::GET_TINT_COLOR), img.word(at::GET_TINT_COLOR + 4), img.word(at::GET_TINT_COLOR + 8)) else {
            return;
        };
        if sll != 0x0004_2080 {
            return;
        }
        let Some(table) = lui_pair(hi, lo) else { return };
        let Some(end) = img.record_end(table) else { return };
        let mut a = table;
        while a + 4 <= end {
            if let Some(b) = img.bytes(a, 4) {
                self.tints.push([b[0], b[1], b[2], b[3]]);
            }
            a += 4;
        }
    }

    /// GetHelm jumps to a cave that does `li v0, A; bne a0, v0, +2; nop;
    /// li a0, B` before indexing $Helm: the slot at byte offset A reads B's,
    /// so set A/4 wears set B/4's helm.
    fn read_helm(&mut self, img: &Image) {
        let Some(cave) = img.jump_target(at::GET_HELM) else { return };
        let w: Option<Vec<u32>> = (0..4).map(|i| img.word(cave + 4 * i)).collect();
        let Some(w) = w else { return };
        if w[0] >> 16 != 0x2402 || w[1] >> 16 != 0x1482 || w[2] != 0 || w[3] >> 16 != 0x2404 {
            return;
        }
        let (a, b) = (w[0] as u16 as i16 as i32 / 4, w[3] as u16 as i16 as i32 / 4);
        if let (Ok(a), Ok(b)) = (u8::try_from(a), u8::try_from(b)) {
            self.helm_remap.push((a, b));
        }
    }

    /// Two hooks that work together: GetRobeTexture jumps to a cave whose
    /// `sltiu v0, a0, N` after retail's `sltiu v0, a0, 4` is the new robe
    /// count, and whose lui/ori into t0 is a table of (chest, legs) raster
    /// ids, eight bytes a robe from robe 4; CLIENT's addart.csf read jumps to
    /// a cave that reads its own art file instead (the first
    /// VIMemCardReadFile's a0), parses it, and fills that table with
    /// FindTyped(DictID, 1, &slot): each call's a1 is the DictID of the slot
    /// a3 names.
    fn read_robes_and_art(&mut self, img: &Image) {
        let mut table = 0u32;
        let mut limit: Option<i32> = None;
        if let Some(cave) = img.jump_target(at::GET_ROBE_TEXTURE) {
            let mut seen4 = false;
            img.walk(cave, 64, |_, w, r| {
                if w >> 16 == 0x2C82 {
                    // sltiu v0, a0, N
                    let n = w as u16 as i16 as i32;
                    if n == 4 {
                        seen4 = true;
                    } else if seen4 && limit.is_none() {
                        limit = Some(n);
                    }
                }
                if table == 0
                    && let Some(t) = r.get(8)
                    && in_cave(t)
                {
                    table = t;
                }
                true
            });
        }
        self.robe_limit = limit;
        let site = CLIENT_BASE + at::CLIENT_ART_READ;
        let Some(call) = img.word(site) else { return };
        if call >> 26 != 3 {
            return;
        }
        let cave = j_target(site, call);
        if !in_cave(cave) {
            return;
        }
        let mut art_file = None;
        let mut art: Vec<(u8, [u32; 2])> = Vec::new();
        img.walk(cave, 128, |pc, w, r| {
            if w >> 26 != 3 {
                return true;
            }
            match j_target(pc, w) {
                at::MEM_CARD_READ_FILE => {
                    if art_file.is_none()
                        && let Some(a0) = r.after(4)
                    {
                        art_file = img.cstring(a0);
                    }
                }
                at::FIND_TYPED => {
                    if let (Some(id), Some(slot), Some(n)) = (r.after(5), r.after(7), limit)
                        && table != 0
                        && n > 4
                        && slot >= table
                        && slot < table + (n as u32 - 4) * 8
                    {
                        let robe = 4 + ((slot - table) / 8) as u8;
                        let half = ((slot - table) % 8 / 4) as usize;
                        match art.iter_mut().find(|(k, _)| *k == robe) {
                            Some((_, pair)) => pair[half] = id,
                            None => {
                                let mut pair = [0u32; 2];
                                pair[half] = id;
                                art.push((robe, pair));
                            }
                        }
                    }
                }
                _ => {}
            }
            true
        });
        art.sort_by_key(|(k, _)| *k);
        self.art_file = art_file;
        self.robe_art = art;
    }

    /// Six lui/addiu pairs in CLIENT point at the world table; the first is
    /// read. Each entry is eight words (name, ESF path, the name's word, the
    /// zone grid's column and row bounds, a data index), and the table runs
    /// to the end of its record.
    fn read_worlds(&mut self, img: &Image) {
        let (Some(hi), Some(lo)) = (img.word(CLIENT_BASE + at::CLIENT_WORLD_HI), img.word(CLIENT_BASE + at::CLIENT_WORLD_LO)) else {
            return;
        };
        let Some(table) = lui_pair(hi, lo) else { return };
        let Some(end) = img.record_end(table) else { return };
        let mut a = table;
        while a + 32 <= end {
            let w: Vec<u32> = (0..8).map(|i| img.word(a + 4 * i).unwrap_or(0)).collect();
            let (Some(name), Some(esf)) = (img.cstring(w[0]), img.cstring(w[1])) else { return };
            self.worlds.push(World { name, esf, name_id: w[2] as i32, bounds: [w[3] as i32, w[4] as i32, w[5] as i32, w[6] as i32] });
            a += 32;
        }
    }
}

/// The client this session plays as: the retail disc, as the server's patch
/// makes it. A field is retail's unless the patch hooks it.
#[derive(Clone, Debug)]
pub struct Rules {
    /// "retail", or "S2" (a server folder's)
    pub source: String,
    /// GetTintColor's table; `None` is retail's 15
    pub tints: Option<Vec<[u8; 4]>>,
    /// SetArmorSlot's tint clamp gone: a tint reaches its table's end
    pub tint_clamp_removed: bool,
    /// SetArmorSlot: a set at or past it is 0 (retail 9)
    pub armor_set_limit: u8,
    /// GetHelm: a set wears another's helm
    pub helm_remap: Vec<(u8, u8)>,
    /// GetRobeTexture's robes are 0 .. limit-1 (retail 4)
    pub robe_limit: u8,
    /// a patch robe's (chest, legs) DictIDs (in its art file)
    pub robe_art: Vec<(u8, [u32; 2])>,
    /// the card file CLIENT reads its added art from (retail addart.csf,
    /// which the disc carries as DATA2/ADDART.CSF)
    pub art_file: String,
    /// the Group menu's Search, Register and Unregister, and 0x1500/0x1501
    pub player_search: bool,
    /// 0x00DB's three trailing bytes colour the number
    pub damage_rgb: bool,
    /// an unknown /command goes to the server (vanilla forwards them
    /// anyway: the flag is what the patch says)
    pub commands_to_server: bool,
    /// the Auctions window opens on this Max. Level (retail 50)
    pub auction_max_level: i32,
    /// the world table; `None` is CLIENT's own six
    pub worlds: Option<Vec<World>>,
    /// CLIENT's data as the patch leaves it (`client_image` reads its
    /// tables through it: the Group menu)
    pub image: Option<Arc<Image>>,
    /// the server's folder the patch came from: its art file, a world the
    /// disc does not carry
    pub dir: Option<PathBuf>,
    /// an armour set past retail's: its slots' DictIDs, and the CHAR.ESF
    /// records (by file offset) registered under them (`register_art`)
    pub armor_art: Vec<(u8, [u32; 5], [u32; 5])>,
}

impl Default for Rules {
    fn default() -> Self {
        Rules::retail()
    }
}

impl Rules {
    /// The retail disc's: the client a server that sends no S2 talks to.
    pub fn retail() -> Rules {
        Rules {
            source: "retail".into(),
            tints: None,
            tint_clamp_removed: false,
            armor_set_limit: 9,
            helm_remap: Vec::new(),
            robe_limit: 4,
            robe_art: Vec::new(),
            art_file: "addart.csf".into(),
            player_search: false,
            damage_rgb: false,
            commands_to_server: false,
            auction_max_level: 50,
            worlds: None,
            image: None,
            dir: None,
            armor_art: Vec::new(),
        }
    }

    /// The rules of a server folder: its S2 as the stub applies it, else
    /// retail's (no S2, or one the stub would refuse: a PS2 applies nothing
    /// then either).
    pub fn from_dir(dir: &Path) -> Rules {
        let path = dir.join("S2");
        let Ok(data) = std::fs::read(&path) else { return Rules::retail() };
        match Patch::parse(&data) {
            Ok(p) => {
                let mut r = Rules::from_patch(&p);
                r.dir = Some(dir.to_path_buf());
                r
            }
            Err(e) => {
                eprintln!("s2: {}: {e}; playing as retail", path.display());
                Rules::retail()
            }
        }
    }

    /// The rules a patch's hooks say.
    pub fn from_patch(p: &Patch) -> Rules {
        let img = p.apply(0);
        let f = Features::read(&img);
        let mut r = Rules::retail();
        r.source = "S2".into();
        if !f.tints.is_empty() {
            r.tints = Some(f.tints.clone());
        }
        r.tint_clamp_removed = f.tint_clamp_removed;
        if let Some(n) = f.armor_set_limit.and_then(|n| u8::try_from(n).ok()).filter(|&n| n > 0) {
            r.armor_set_limit = n;
        }
        r.helm_remap = f.helm_remap.clone();
        if let Some(n) = f.robe_limit.and_then(|n| u8::try_from(n).ok()).filter(|&n| n > 4) {
            r.robe_limit = n;
        }
        r.robe_art = f.robe_art.clone();
        if let Some(a) = &f.art_file {
            r.art_file = a.clone();
        }
        r.player_search = f.player_search;
        r.damage_rgb = f.damage_rgb;
        r.commands_to_server = f.commands_to_server;
        if let Some(n) = f.auction_max_level.filter(|&n| n > 0) {
            r.auction_max_level = n;
        }
        if !f.worlds.is_empty() {
            r.worlds = Some(f.worlds.clone());
        }
        r.armor_art = f.armor_art.clone();
        r.image = Some(Arc::new(img));
        r
    }

    /// The CHAR.ESF surfaces the patch registers under DictIDs of its own
    /// (set 9's), registered in `lib` too once CHAR.ESF is in it: each a
    /// Surface record at its file offset, else not registered.
    pub fn register_art(&self, lib: &mut esf::Library) {
        let Some(file) = lib.files.iter().position(|(n, _)| n.eq_ignore_ascii_case("CHAR.ESF")) else { return };
        for (_, ids, records) in &self.armor_art {
            for (&id, &off) in ids.iter().zip(records) {
                let c = lib.file(file);
                // a record's body follows its 12-byte header
                let node = c.nodes.iter().position(|n| n.ty == esf::types::SURFACE && n.body == off as usize + 12);
                if let Some(node) = node {
                    lib.alias(id as i32, esf::Ref { file, node });
                }
            }
        }
    }

    /// One line for the log and a problem report.
    pub fn describe(&self) -> String {
        let tints = self.tints.as_ref().map_or(15, |t| self.tint_count_of(t.len()));
        let worlds = self.worlds.as_ref().map_or(6, Vec::len);
        format!(
            "{} (tints {tints}, sets below {}, robes {}, worlds {worlds}, art {}, search {}, damage colours {}, commands to the server {}, auction level {})",
            self.source, self.armor_set_limit, self.robe_limit, self.art_file, self.player_search, self.damage_rgb, self.commands_to_server, self.auction_max_level
        )
    }

    fn tint_count_of(&self, len: usize) -> usize {
        if self.tint_clamp_removed { len } else { len.min(15) }
    }

    /// The customisation tables `vi::cust` dresses a look by, as this patch
    /// leaves them (`Tables::retail()` without one).
    pub fn cust(&self) -> vi::cust::Tables {
        let mut t = vi::cust::Tables::retail();
        if let Some(tints) = &self.tints {
            t.tints = tints.clone();
            t.tint_limit = self.tint_count_of(tints.len());
        }
        t.armor_limit = self.armor_set_limit;
        t.helm_remap = self.helm_remap.clone();
        t.robe_limit = self.robe_limit;
        t.robe_art = self.robe_art.clone();
        t.armor_extra = self.armor_art.iter().map(|&(set, ids, _)| (set, ids)).collect();
        t
    }

    /// The disc file a world id loads, the patch's world table or CLIENT's
    /// six; `None` past the table.
    pub fn world_file(&self, world: i64) -> Option<String> {
        let i = usize::try_from(world).ok()?;
        match &self.worlds {
            Some(w) => w.get(i).map(World::esf_name),
            None => crate::play::WORLD_FILES.get(i).map(|s| s.to_string()),
        }
    }

    /// The patch's art file in its server folder, when it is there (robes 4
    /// and 5 come out of it).
    pub fn art_path(&self) -> Option<PathBuf> {
        self.folder_file(&self.art_file)
    }

    /// The game's words as the patcher left them in the server's folder
    /// (WORDS.TXT: Sandstorm's adds 1815 Register and 1816 Unregister), the
    /// card's copy read before the disc's as the game reads it; `None` with
    /// no copy there.
    pub fn words(&self) -> Option<vi::strtab::StringTable> {
        let bytes = std::fs::read(self.folder_file("WORDS.TXT")?).ok()?;
        vi::strtab::StringTable::parse(&bytes)
    }

    /// A file in the server's folder, its name in any case ("UNDERFOOT.ESF"
    /// finds "Underfoot.esf").
    pub fn folder_file(&self, name: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
    }
}

static CURRENT: RwLock<Option<Arc<Rules>>> = RwLock::new(None);

#[cfg(test)]
thread_local! {
    /// A test's own rules (`with_rules`), so tests running at once never
    /// see each other's.
    static TEST_RULES: std::cell::RefCell<Option<Arc<Rules>>> = const { std::cell::RefCell::new(None) };
}

/// The session's rules (retail until a server's are set).
pub fn get() -> Arc<Rules> {
    #[cfg(test)]
    if let Some(r) = TEST_RULES.with(|t| t.borrow().clone()) {
        return r;
    }
    CURRENT.read().ok().and_then(|g| g.clone()).unwrap_or_else(|| Arc::new(Rules::retail()))
}

/// `f` run with `r` as this thread's rules (a test's).
#[cfg(test)]
pub fn with_rules<T>(r: Rules, f: impl FnOnce() -> T) -> T {
    TEST_RULES.with(|t| *t.borrow_mut() = Some(Arc::new(r)));
    let out = f();
    TEST_RULES.with(|t| *t.borrow_mut() = None);
    out
}

/// This session's rules: a server's folder once its patch is in (the
/// patcher), retail for a server that sends none.
pub fn set(r: Rules) {
    eprintln!("s2: playing as {}", r.describe());
    if let Ok(mut g) = CURRENT.write() {
        *g = Some(Arc::new(r));
    }
}

/// A session begins (`Play::new`): `EQOA_S2_DIR` names a folder whose S2
/// the session plays by (a test, or a folder of one's own); without it the
/// rules stay as the patcher set them for the server (retail when none).
pub fn begin_session() {
    if let Some(dir) = std::env::var_os("EQOA_S2_DIR") {
        let dir = Path::new(&dir);
        if get().dir.as_deref() != Some(dir) {
            set(Rules::from_dir(dir));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The S2 builds read here, none of them in the repository (Sandstorm's
    /// code, and our own server's): `$EQOA_S2_BUILDS`, else the folders the
    /// patch was fetched into beside this repository.
    fn build(which: &str) -> Option<Vec<u8>> {
        let root = std::env::var_os("EQOA_S2_BUILDS").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let rel = match which {
            // Sandstorm's live ss-patch-60, fetched 2026-10-15
            "live" => "extracted-assets/sandstorm-patch-20261015/S2",
            // its July build
            "july" => "go-eqoa-server/patches/S2-stock",
            // what our own server serves
            "ours" => "go-eqoa-server/patches/serve/S2",
            _ => return None,
        };
        let path = root.join(rel);
        match std::fs::read(&path) {
            Ok(d) => Some(d),
            Err(_) => {
                eprintln!("SKIP: no {which} S2 at {}", path.display());
                None
            }
        }
    }

    fn rules(which: &str) -> Option<Rules> {
        let data = build(which)?;
        Some(Rules::from_patch(&Patch::parse(&data).expect("the stub takes it")))
    }

    /// Every feature, read off Sandstorm's live build, is what the July and
    /// live notes found it to be.
    #[test]
    fn the_live_s2s_features() {
        let Some(r) = rules("live") else { return };
        let tints = r.tints.as_ref().expect("its tint table");
        assert_eq!(tints.len(), 30);
        assert!(r.tint_clamp_removed);
        assert_eq!(&tints[..3], &[[0xFF, 0xFF, 0xFF, 0xFF], [0x5A, 0x5A, 0x5A, 0xFF], [0x00, 0xB4, 0xFF, 0xFF]]);
        assert_eq!(tints[15], [0, 0, 0, 0], "15 is clear");
        assert_eq!(tints[29], [0xFF, 0xD7, 0x00, 0xFF]);
        assert_eq!(r.armor_set_limit, 10);
        assert_eq!(r.helm_remap, vec![(9, 4)]);
        assert_eq!(r.robe_limit, 6);
        assert_eq!(r.robe_art, vec![(4, [0xDEAD_BEEF, 0xCAFE_BABE]), (5, [0xDEAD_BEFE, 0xCAFE_BAEB])]);
        assert_eq!(r.art_file, "ss-art.csf");
        assert!(r.player_search && r.damage_rgb && r.commands_to_server);
        assert_eq!(r.auction_max_level, 60);
        let names: Vec<String> = r.worlds.as_ref().expect("its world table").iter().map(World::esf_name).collect();
        assert_eq!(names, ["TUNARIA.ESF", "RATHE.ESF", "ODUS.ESF", "LAVASTM.ESF", "PLANESKY.ESF", "SECRETS.ESF", "UNDERFOOT.ESF", "SECRETS.ESF"]);
        let w0 = &r.worlds.as_ref().unwrap()[0];
        assert_eq!((w0.name.as_str(), w0.name_id, w0.bounds[1], w0.bounds[3]), ("tunaria", 0x413, 12, 15));
        assert_eq!(r.world_file(6).as_deref(), Some("UNDERFOOT.ESF"));
        assert_eq!(r.armor_art, vec![(9, [0xC214_1C0A, 0x03C1_BE9E, 0x85EA_2A1D, 0x432C_4890, 0xB179_23D2], [0x073BEC, 0x072257, 0x0C7D31, 0x06C8BE, 0x06AF29])]);
    }

    /// July's build is the live one without the auction change; ours raises
    /// the armour set clamp for its set 10.
    #[test]
    fn the_other_builds() {
        if let Some(r) = rules("july") {
            assert_eq!((r.auction_max_level, r.armor_set_limit, r.tints.as_ref().map(Vec::len), r.worlds.as_ref().map(Vec::len)), (50, 10, Some(30), Some(8)));
        }
        if let Some(r) = rules("ours") {
            assert_eq!((r.armor_set_limit, r.robe_limit, r.player_search), (11, 6, true));
        }
    }

    /// A world the disc does not carry comes whole out of the server's
    /// folder (Sandstorm's world 6), its name in any case; without the
    /// folder (the control) it is not there. Needs the disc (`run/eqoa.iso`)
    /// and a container to stand in for the world (Sandstorm's ss-art.csf).
    #[test]
    fn a_world_out_of_the_servers_folder() {
        let iso = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let art = std::env::var_os("EQOA_S2_BUILDS").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).join("extracted-assets/sandstorm-patch-20261015/ss-art.csf");
        let (true, Ok(bytes)) = (iso.is_file(), std::fs::read(&art)) else {
            eprintln!("SKIP: no disc or no ss-art.csf");
            return;
        };
        let dir = std::env::temp_dir().join(format!("eqoa-s2-world-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Underfoot.esf"), &bytes).unwrap();
        let folder = Rules { dir: Some(dir.clone()), ..Rules::retail() };
        assert!(with_rules(folder, || crate::stream::open_file(&iso, "UNDERFOOT.ESF")).is_ok());
        assert!(crate::stream::open_file(&iso, "UNDERFOOT.ESF").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Set 9's art: the five CHAR.ESF records at the patch's file offsets
    /// are surfaces, and once registered under its DictIDs they resolve;
    /// retail's rules (the control) register nothing. Needs the disc.
    #[test]
    fn set_9s_art_out_of_char_esf() {
        let iso = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Some(r) = rules("live") else { return };
        if !iso.is_file() {
            eprintln!("SKIP: no disc");
            return;
        }
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", crate::stream::open_file(&iso, "CHAR.ESF").unwrap());
        let ids = r.armor_art[0].1;
        Rules::retail().register_art(&mut lib);
        assert!(ids.iter().all(|&id| lib.surface(id as i32).is_none()));
        r.register_art(&mut lib);
        for id in ids {
            assert!(lib.surface(id as i32).is_some(), "{id:08X}");
        }
    }

    /// The words come out of the server's folder when the patcher put a
    /// WORDS.TXT there (Sandstorm's: Register and Unregister at 1815, 1816);
    /// without a folder (the control) there are none of its own.
    #[test]
    fn the_words_out_of_the_servers_folder() {
        let dir = std::env::var_os("EQOA_S2_BUILDS").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).join("extracted-assets/sandstorm-patch-20261015");
        if !dir.join("WORDS.TXT").is_file() {
            eprintln!("SKIP: no WORDS.TXT at {}", dir.display());
            return;
        }
        let r = Rules { dir: Some(dir), ..Rules::retail() };
        let w = r.words().expect("its words");
        assert_eq!((w.get(1815, 0), w.get(1816, 0)), ("Register", "Unregister"));
        assert!(Rules::retail().words().is_none());
    }

    fn rec(addr: u32, data: &[u8]) -> Vec<u8> {
        let mut b = addr.to_le_bytes().to_vec();
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b
    }

    fn tail(relocs: &[u32]) -> Vec<u8> {
        let mut b = vec![0u8; 8];
        b.extend(RLC1.to_le_bytes());
        b.extend((relocs.len() as u32).to_le_bytes());
        for r in relocs {
            b.extend(r.to_le_bytes());
        }
        b
    }

    /// The stub's checks on a patch built here (so a machine with no S2 has
    /// one): a cave table, GetTintColor pointed at it, the relocation of its
    /// lui; and what the stub refuses.
    #[test]
    fn the_stubs_rules() {
        let mut good = rec(0x800F_CD00, &[1, 2, 3, 4, 5, 6, 7, 8]);
        good.extend(rec(0x8042_94E8, &[0x0F, 0x00, 0x08, 0x3C, 0x00, 0xCD, 0x08, 0x35, 0x80, 0x20, 0x04, 0x00]));
        good.extend(tail(&[0x8042_94EA]));
        let p = Patch::parse(&good).unwrap();
        let r = Rules::from_patch(&p);
        assert_eq!(r.tints, Some(vec![[1, 2, 3, 4], [5, 6, 7, 8]]));
        // the clamp still in: 15 at most, and two here
        assert_eq!(r.cust().tint_limit, 2);
        // moved by a delta, the cave and the lui move together
        let img = p.apply(0x0066_0000);
        assert_eq!(img.word(at::GET_TINT_COLOR), Some(0x3C08_0075));
        assert!(img.word(0x000F_CD00 + 0x0066_0000).is_some(), "the cave record moved");
        let short: &[u8] = &[1, 2, 3];
        let mut no_rlc1 = rec(0x800F_CD00, &[1, 2, 3, 4]);
        no_rlc1.extend([0u8; 16]);
        let mut count = rec(0x800F_CD00, &[1, 2, 3, 4]);
        count.extend(tail(&[]));
        count.extend([0u8; 4]);
        let mut out_of_cave = rec(0x800F_FFFC, &[1, 2, 3, 4, 5, 6, 7, 8]);
        out_of_cave.extend(tail(&[]));
        let past_end = rec(0x800F_CD00, &[1, 2, 3, 4])[..10].to_vec();
        let full = vec![0u8; MAX_FILE];
        for (name, bad) in [("short", short.to_vec()), ("no RLC1", no_rlc1), ("count", count), ("out of the cave", out_of_cave), ("past the end", past_end), ("buffer full", full)] {
            assert!(Patch::parse(&bad).is_err(), "{name}: accepted");
        }
    }

    /// No S2 in a folder, or one the stub refuses, is retail: every feature
    /// off, the tables the disc's.
    #[test]
    fn no_patch_is_retail() {
        let dir = std::env::temp_dir().join(format!("eqoa-s2-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let retail = Rules::retail();
        for r in [Rules::from_dir(&dir), {
            std::fs::write(dir.join("S2"), [1u8, 2, 3]).unwrap();
            Rules::from_dir(&dir)
        }] {
            assert_eq!(r.source, "retail");
            assert!(r.image.is_none() && r.worlds.is_none() && r.tints.is_none());
            assert!(!r.player_search && !r.damage_rgb);
            assert_eq!((r.armor_set_limit, r.robe_limit, r.auction_max_level), (9, 4, 50));
            assert_eq!(r.cust(), vi::cust::Tables::retail());
            assert_eq!(r.world_file(5).as_deref(), Some("SECRETS.ESF"));
            assert_eq!(r.world_file(6), None);
        }
        // the control: a patch that hooks the auction level is not retail
        let mut p = rec(at::CLIENT_AUCTION_LEVEL, &0x2405_003Cu32.to_le_bytes());
        p.extend(tail(&[]));
        std::fs::write(dir.join("S2"), &p).unwrap();
        let r = Rules::from_dir(&dir);
        assert_eq!((r.source.as_str(), r.auction_max_level), ("S2", 60));
        assert_ne!(r.auction_max_level, retail.auction_max_level);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
