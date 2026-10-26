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
//! refuse, the client is retail's, as before.
//!
//! What the live S2 tells the server is taken too (the user, 2026-10-25, for
//! a vampire's Sanctuary, which Sandstorm refuses "memcard patch 0
//! outdated" without it): the shard connection's hello (`Hello`: DiscVersion
//! 21, the boot patch's word, the patch's build, the device block) and
//! UseSkill's record of the pad (`Presses`), each made as S2 makes it but
//! from what this client has: no hardware address (S2 sends none when it
//! finds no network interface), an install id kept in the server's folder as
//! S2 keeps it on the card, the presses this client's own input made. Not
//! taken: 0x07E1's heap lines, the 0x1505/0x1507 crash reports, and the PS2
//! memory layer.

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

    /// The NUL-terminated UTF-16 string at `a` (256 characters at most),
    /// when the patch wrote it.
    pub fn wstring(&self, a: u32) -> Option<Vec<u16>> {
        let mut s = Vec::new();
        for i in 0..256u32 {
            let (lo, hi) = (*self.mem.get(&a.wrapping_add(2 * i))?, *self.mem.get(&a.wrapping_add(2 * i + 1))?);
            match u16::from_le_bytes([lo, hi]) {
                0 => return Some(s),
                c => s.push(c),
            }
        }
        None
    }

    /// The address a `lui r, hi` at `hi_at` and an `addiu` or `ori r, r, lo`
    /// at `lo_at` build, when the patch wrote both and both are into `r`.
    pub fn address(&self, hi_at: u32, lo_at: u32, r: u32) -> Option<u32> {
        let (hi, lo) = (self.word(hi_at)?, self.word(lo_at)?);
        if (hi >> 16) & 31 != r {
            return None;
        }
        lui_pair(hi, lo)
    }

    /// Where the patch's j or jal at `a` goes.
    pub fn call_target(&self, a: u32) -> Option<u32> {
        self.jump_target(a)
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

/// Whether `a` is in the arena the stub allocates (at delta 0).
pub fn in_cave(a: u32) -> bool {
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
    /// slti v0, v0, 2: `VIWndAuction::CanSell`'s last test, in the delay
    /// slot of its `jr ra` (`m_maxStack < 2`)
    pub const CLIENT_CAN_SELL_STACK: u32 = 0x5A2A0;
    /// slti v0, v0, 128: `VIClientPart2EF24::CanInvite`'s guild cap
    /// (`Count() >= 128` is full)
    pub const CLIENT_GUILD_CAP: u32 = 0x39D60;
    /// li v0, 18: `ConnectShard`'s hello, its DiscVersion
    pub const CLIENT_HELLO_VERSION: u32 = 0xD548;
    /// lw v1, 0(s2): `ConnectShard` once the DiscVersion is written, where
    /// S2 calls the rest of its hello
    pub const CLIENT_HELLO_MORE: u32 = 0xD564;
    /// ori v0, v0, 0x8000: `VIClient::Process` past its frame's wait, where
    /// S2 reads the pad itself (a frame counted)
    pub const CLIENT_PROCESS_PAD: u32 = 0x13784;
    /// `VIClient::SetKey` past its dead test, where S2 records a pad key
    pub const CLIENT_SET_KEY: u32 = 0x1B078;
    /// `VIClient::Send0032`'s SendRaw, before which S2 adds its record
    pub const CLIENT_SEND_0032: u32 = 0x17654;
    /// The boot patch's SS block in SLUS_207.44: its first word, which S2's
    /// hello reads
    pub const BOOT_SS_BLOCK: u32 = 0x0015_7200;
    /// S2's device block (the hardware address, a status byte, an interface
    /// byte, the install id, twelve bytes more), which its hello sends
    pub const DEVICE_BLOCK: u32 = 0x0015_7C84;
    pub const DEVICE_BLOCK_LEN: u32 = 28;
}

/// The boot patch's SS block word (`at::BOOT_SS_BLOCK`), which S2's hello
/// sends: 3 in the `SLUS_207.44` Sandstorm's disc patch makes (its xdelta as
/// of 2026-10-23; August's, which `EQOA-local.iso` and eqoa-boot carry, made
/// 2). This client boots no patched disc; it says what Sandstorm's own
/// patch says now.
pub const BOOT_SS_WORD: u32 = 3;

/// `ConnectShard`'s hello as a patch makes it: its DiscVersion, then what
/// the patch's cave writes after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    /// retail 18, the July S2 19, the live one 21
    pub version: u32,
    /// the cave's writes, in order
    pub more: Vec<HelloPart>,
    /// the card file the device block's install id is kept in (`ii`), when
    /// the cave makes the block
    pub id_file: Option<String>,
}

/// One write of S2's hello cave.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HelloPart {
    /// a u32 the cave holds (the patch's build: 4 in July, 0x31 live)
    Const(u32),
    /// the u32 at an address the cave reads (`at::BOOT_SS_BLOCK`)
    Word(u32),
    /// that many bytes from an address (`at::DEVICE_BLOCK`)
    Block(u32, u32),
}

impl Hello {
    /// Whether it is the live S2's: the patch's numbers and the device
    /// block whose install id is kept on the card.
    pub fn has_device_block(&self) -> bool {
        self.id_file.is_some() && self.more.contains(&HelloPart::Block(at::DEVICE_BLOCK, at::DEVICE_BLOCK_LEN))
    }
}

/// S2's device block as this client makes it, once a run for each server's
/// folder as S2 makes it once a boot (0x000FC630). S2 asks the network
/// interfaces for a hardware address (`sceInetInterfaceControl` code 13)
/// and, for the first with one, code 9's twelve bytes and an interface byte
/// (0x00157880); this client has no PS2 network adapter to name, so they
/// are the zeros S2 sends when it finds none, and the status byte's bit 1
/// (an address found) is clear. The install id (bytes 8 to 16) comes out of
/// the folder's `id_file` as S2 reads it off the card
/// (`VIMemCardReadFile(name, block + 8, 8)`), its result in the status
/// byte's bits 2 to 4; a short read mints one as S2 mints it (0x000FC760:
/// the clock, then the CLIENT base plus the address's halves plus the clock
/// << 13; the clock `clock`, the wall clock in play, where S2 has VITimer's
/// count) and writes it there, the write's result in bits 5 to 7.
fn make_device_block(dir: &Path, id_file: &str, clock: u32) -> [u8; 28] {
    let mut b = [0u8; 28];
    // VIMemCardReadFile: the bytes read, -2 for no file, -1 a failure
    let n: i32 = match folder_read(dir, id_file) {
        Ok(d) => {
            let k = d.len().min(8);
            b[8..8 + k].copy_from_slice(&d[..k]);
            k as i32
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => -2,
        Err(_) => -1,
    };
    let short = (n < 8) as i32;
    b[6] |= (((short + 1 - n.min(0)) & 7) << 2) as u8;
    if short != 0 {
        b[8..12].copy_from_slice(&clock.to_le_bytes());
        b[12..16].copy_from_slice(&CLIENT_BASE.wrapping_add(clock << 13).to_le_bytes());
        // VIMemCardWriteFile: 0 written, -1 not
        let w: i32 = if folder_write(dir, id_file, &b[8..16]).is_ok() { 0 } else { -1 };
        b[6] |= (((1 - w) & 7) << 5) as u8;
    }
    b
}

/// A file of a server's folder: natively on disk, in a page in the origin's
/// storage (`servers::page`).
fn folder_read(dir: &Path, name: &str) -> std::io::Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    return std::fs::read(dir.join(name));
    #[cfg(target_arch = "wasm32")]
    crate::servers::page::read(dir, name).ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

fn folder_write(dir: &Path, name: &str, b: &[u8]) -> std::io::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    return std::fs::write(dir.join(name), b);
    #[cfg(target_arch = "wasm32")]
    crate::servers::page::write(dir, name, b).map_err(std::io::Error::other)
}

/// `make_device_block` for this run: the first connection to a server's
/// shard makes its folder's, later ones send the same.
fn device_block(dir: &Path, id_file: &str) -> [u8; 28] {
    static MADE: std::sync::Mutex<Vec<(PathBuf, [u8; 28])>> = std::sync::Mutex::new(Vec::new());
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, b)) = made.iter().find(|(d, _)| d == dir) {
        return *b;
    }
    let clock = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u32);
    let b = make_device_block(dir, id_file, clock);
    made.push((dir.to_path_buf(), b));
    b
}

/// S2's record of the pad (its block at 0x000F9900), which a PS2 running
/// the live S2 adds to every UseSkill, kept from this client's own input:
/// `frame_end` is its read at the end of `VIClient::Process` (0x000F9A20: a
/// frame counted), `key` its record of a pad key in `SetKey` (0x000F9BC0),
/// `tail` the 11 bytes `Send0032` adds (0x000F9D20). The times are the
/// client's clock (`m_p07568`, VITimer's ticks).
#[derive(Clone, Debug, Default)]
pub struct Presses {
    /// +0x00: the frames counted
    frame: u32,
    /// +0x20: the last pad key pressed (128 + a `VIConsolePad` field)
    key: i32,
    /// +0x24: bit 0 S2's own pad read saw the press, bit 1 it read the pad
    flags: u32,
    /// +0x28: that key's hold the time before
    hold: u32,
    /// +0x2C: when it was pressed
    time: u32,
    /// +0x30: the frame it was pressed in
    frame_pressed: u32,
    /// +0x70: each of keys 132-147's press (0 while it is up) and last hold
    keys: [(u32, u32); 16],
}

impl Presses {
    /// The end of one of the game's frames.
    pub fn frame_end(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Pad key `key` (132-147, the buttons; the sticks' 128-131 are not
    /// kept) down or up at `now`. `seen`: the press is the player's own, on
    /// a controller, the keyboard, the mouse or the touch screen, where S2
    /// asks whether its own read of the pad has the button newly down (a
    /// script's is not seen). The pad was read: bit 1, which S2 sets when
    /// its read worked, is always set.
    pub fn key(&mut self, key: i32, down: bool, now: u32, seen: bool) {
        let Some(k) = usize::try_from(key - 132).ok().and_then(|i| self.keys.get_mut(i)) else { return };
        if down {
            k.0 = now;
            self.time = now;
            self.key = key;
            self.frame_pressed = self.frame;
            self.hold = k.1;
            self.flags = seen as u32 | 2;
        } else if k.0 != 0 {
            k.1 = now.wrapping_sub(k.0);
            k.0 = 0;
        }
    }

    /// What `Send0032` adds after the target: when the last pad key was
    /// pressed, the frame now, that key's hold the time before (0xFFFF when
    /// none was measured or it does not fit), and the flags (bit 2: pressed
    /// this frame) with the key's low four bits above them.
    pub fn tail(&self) -> [u8; 11] {
        let hold = if self.hold == 0 || self.hold >> 16 != 0 { 0xFFFF } else { self.hold as u16 };
        let mut flags = self.flags;
        if self.frame_pressed == self.frame {
            flags |= 4;
        }
        flags |= ((self.key & 0xF) as u32) << 4;
        let mut t = [0u8; 11];
        t[0..4].copy_from_slice(&self.time.to_le_bytes());
        t[4..8].copy_from_slice(&self.frame.to_le_bytes());
        t[8..10].copy_from_slice(&hold.to_le_bytes());
        t[10] = flags as u8;
        t
    }
}

/// `VIWndAuction::CanSell`'s last test, on the item's `m_maxStack` (the
/// instruction S2 rewrites in its return's delay slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackTest {
    /// `slti v0, v0, N`: a stack of fewer than N may be auctioned (retail 2:
    /// no stack at all)
    Below(i32),
    /// `addi v0, v0, K`: the test is `m_maxStack + K`, true unless it is 0
    /// (Sandstorm's 1: any item but one whose `m_maxStack` is -1)
    Plus(i32),
}

impl StackTest {
    /// Whether an item whose `m_maxStack` is `max_stack` passes (the game
    /// reads it as a 32-bit word).
    pub fn passes(self, max_stack: i64) -> bool {
        let m = max_stack as i32;
        match self {
            StackTest::Below(n) => m < n,
            StackTest::Plus(k) => m.wrapping_add(k) != 0,
        }
    }

    /// The test an instruction at the site makes, if it is one of the two
    /// shapes (`v0` in, `v0` out).
    fn of(word: u32) -> Option<StackTest> {
        let imm = word as u16 as i16 as i32;
        match word >> 16 {
            0x2842 => Some(StackTest::Below(imm)),
            // addi, and addiu (the same sum without the overflow trap)
            0x2042 | 0x2442 => Some(StackTest::Plus(imm)),
            _ => None,
        }
    }
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
    /// `CanSell`'s stack test
    pub sell_stack: Option<StackTest>,
    /// `CanInvite`'s guild cap
    pub guild_cap: Option<i32>,
    /// an armour set past retail's: its five slots' DictIDs, and the
    /// CHAR.ESF records (by file offset) registered under them
    pub armor_art: Vec<(u8, [u32; 5], [u32; 5])>,
    /// `ConnectShard`'s hello
    pub hello: Option<Hello>,
    /// UseSkill carries S2's record of the pad
    pub use_skill_record: bool,
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
        f.sell_stack = img.word(CLIENT_BASE + at::CLIENT_CAN_SELL_STACK).and_then(StackTest::of);
        // slti or sltiu v0, v0, N: a guild is full at N
        if let Some(w) = img.word(CLIENT_BASE + at::CLIENT_GUILD_CAP)
            && (w >> 16 == 0x2842 || w >> 16 == 0x2C42)
        {
            f.guild_cap = Some(w as u16 as i16 as i32);
        }
        f.read_worlds(img);
        f.read_set9(img);
        f.read_hello(img);
        f.read_presses(img);
        f
    }

    /// `ConnectShard`'s hello: its DiscVersion (`li v0, N`), and when S2
    /// calls a cave after it, that cave's writes, each a `jalr` into
    /// CLIENT's stream write of `a2` bytes: from an address the cave built,
    /// or the u32 it last stored at 16(sp), a constant or a word it loaded.
    /// A `jal` among them is the routine that makes the device block, whose
    /// VIMemCardReadFile of eight bytes into the block's install id names
    /// the id's card file. A write of another shape leaves the hello
    /// unread.
    fn read_hello(&mut self, img: &Image) {
        let Some(w) = img.word(CLIENT_BASE + at::CLIENT_HELLO_VERSION) else { return };
        if w >> 16 != 0x2402 {
            return;
        }
        let mut hello = Hello { version: w as u16 as i16 as i32 as u32, more: Vec::new(), id_file: None };
        if let Some(cave) = img.call_target(CLIENT_BASE + at::CLIENT_HELLO_MORE).filter(|&c| in_cave(c)) {
            let (mut stored, mut loaded, mut maker, mut ok) = (None, None, None, true);
            img.walk(cave, 96, |pc, w, r| {
                let (op, rs, rt) = (w >> 26, (w >> 21) & 31, (w >> 16) & 31);
                match op {
                    // lw rt, off(rs): the register and the address it read
                    35 => loaded = r.get(rs).map(|b| (rt, b.wrapping_add(w as u16 as i16 as i32 as u32))),
                    // sw rt, 16(sp): the u32 the next write sends
                    43 if rs == 29 && w & 0xFFFF == 16 => {
                        stored = match (r.get(rt), loaded) {
                            (Some(v), _) => Some(HelloPart::Const(v)),
                            (None, Some((reg, a))) if reg == rt => Some(HelloPart::Word(a)),
                            _ => None,
                        }
                    }
                    3 => maker = Some(j_target(pc, w)),
                    _ => {}
                }
                // jalr t9: the stream write
                if w == 0x0320_F809 {
                    match (r.get(6), r.get(5)) {
                        (Some(n), Some(src)) => hello.more.push(HelloPart::Block(src, n)),
                        (Some(4), None) => match stored.take() {
                            Some(p) => hello.more.push(p),
                            None => ok = false,
                        },
                        _ => ok = false,
                    }
                }
                ok
            });
            if !ok {
                return;
            }
            if let Some(m) = maker {
                img.walk(m, 64, |pc, w, r| {
                    if w >> 26 == 3
                        && j_target(pc, w) == at::MEM_CARD_READ_FILE
                        && r.after(5) == Some(at::DEVICE_BLOCK + 8)
                        && r.after(6) == Some(8)
                        && let Some(a0) = r.after(4)
                    {
                        hello.id_file = img.cstring(a0);
                        return false;
                    }
                    true
                });
            }
        }
        self.hello = Some(hello);
    }

    /// UseSkill's record of the pad: S2 hooks Process (its own pad read),
    /// SetKey (its record of a key) and Send0032, whose cave writes 11
    /// bytes (a `jalr` into the stream write with `a2` 11) before SendRaw.
    fn read_presses(&mut self, img: &Image) {
        let hooked = |site: u32| img.jals_into_cave(CLIENT_BASE + site);
        if !(hooked(at::CLIENT_PROCESS_PAD) && hooked(at::CLIENT_SET_KEY) && hooked(at::CLIENT_SEND_0032)) {
            return;
        }
        let Some(cave) = img.call_target(CLIENT_BASE + at::CLIENT_SEND_0032) else { return };
        let mut n = None;
        img.walk(cave, 64, |_, w, r| {
            if w == 0x0320_F809 {
                n = r.get(6);
                return false;
            }
            true
        });
        self.use_skill_record = n == Some(11);
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
    /// `CanSell`'s stack test: retail auctions no stack, Sandstorm's S2
    /// any item (`auction::can_sell`)
    pub sell_stack: StackTest,
    /// `CanInvite`: a guild of this many is full (retail 128, Sandstorm's
    /// S2 255; `guild::Cmds::can_invite`)
    pub guild_cap: i32,
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
    /// `ConnectShard`'s hello, the live S2's (`shard_hello`); `None` is
    /// retail's DiscVersion 18 alone
    pub hello: Option<Hello>,
    /// UseSkill carries S2's record of the pad (`Presses::tail`)
    pub use_skill_record: bool,
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
            sell_stack: StackTest::Below(2),
            guild_cap: 128,
            worlds: None,
            image: None,
            dir: None,
            armor_art: Vec::new(),
            hello: None,
            use_skill_record: false,
        }
    }

    /// The rules of a server folder: its S2 as the stub applies it, else
    /// retail's (no S2, or one the stub would refuse: a PS2 applies nothing
    /// then either).
    pub fn from_dir(dir: &Path) -> Rules {
        let path = dir.join("S2");
        let Ok(data) = folder_read(dir, "S2") else { return Rules::retail() };
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
        if let Some(t) = f.sell_stack {
            r.sell_stack = t;
        }
        if let Some(n) = f.guild_cap.filter(|&n| n > 0) {
            r.guild_cap = n;
        }
        if !f.worlds.is_empty() {
            r.worlds = Some(f.worlds.clone());
        }
        r.armor_art = f.armor_art.clone();
        // The hello in the live S2's shape alone (its numbers and the device
        // block; Sandstorm's server reads its patch out of it). The July S2's
        // (19, then its build) is not taken: our own server, whose S2 is July's,
        // reads 19 as that S2 with our hard-disk loader after it, and offers
        // the loader's 30 worlds (go-eqoa-server's `LoadableWorlds`), which this
        // client's table does not have.
        r.hello = f.hello.clone().filter(Hello::has_device_block);
        r.use_skill_record = f.use_skill_record;
        r.image = Some(Arc::new(img));
        r
    }

    /// What `ConnectShard`'s hello sends after its opcode under this patch
    /// (`net::session::Session::connect_shard`): the DiscVersion, then each
    /// of the cave's writes as a PS2 running it sends it, the boot patch's
    /// word as Sandstorm's makes it (`BOOT_SS_WORD`) and the device block as
    /// this client makes it (`make_device_block`, in the server's folder).
    /// `None` is retail's hello: none in the patch, or a part this client
    /// does not know how to make.
    pub fn shard_hello(&self) -> Option<Vec<u8>> {
        let h = self.hello.as_ref()?;
        let mut out = h.version.to_le_bytes().to_vec();
        for p in &h.more {
            match *p {
                HelloPart::Const(v) => out.extend(v.to_le_bytes()),
                HelloPart::Word(at::BOOT_SS_BLOCK) => out.extend(BOOT_SS_WORD.to_le_bytes()),
                HelloPart::Block(at::DEVICE_BLOCK, at::DEVICE_BLOCK_LEN) => out.extend(device_block(self.dir.as_deref()?, h.id_file.as_deref()?)),
                _ => return None,
            }
        }
        Some(out)
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
        let hello = self.hello.as_ref().map_or_else(|| "18".to_string(), |h| {
            let build = h.more.iter().find_map(|p| match p {
                HelloPart::Const(v) => Some(*v),
                _ => None,
            });
            format!("{} (build {})", h.version, build.map_or("?".to_string(), |b| format!("{b:#x}")))
        });
        format!(
            "{} (tints {tints}, sets below {}, robes {}, worlds {worlds}, art {}, search {}, damage colours {}, commands to the server {}, auction level {}, stacks auctioned {}, guilds of {}, hello {hello}, pad record {})",
            self.source,
            self.armor_set_limit,
            self.robe_limit,
            self.art_file,
            self.player_search,
            self.damage_rgb,
            self.commands_to_server,
            self.auction_max_level,
            self.sell_stack.passes(2),
            self.guild_cap,
            self.use_skill_record
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
        crate::spelling::words(&self.folder_bytes("WORDS.TXT")?)
    }

    /// A file of the server's folder, its name in any case (`folder_file`;
    /// in a page out of the origin's storage).
    pub fn folder_bytes(&self, name: &str) -> Option<Vec<u8>> {
        #[cfg(not(target_arch = "wasm32"))]
        return std::fs::read(self.folder_file(name)?).ok();
        #[cfg(target_arch = "wasm32")]
        crate::servers::page::read(self.dir.as_ref()?, name)
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

/// The rules of the server at `host`: the S2 its patch put in its folder
/// (`servers::folder`) when the game keeps one there, retail otherwise (a
/// page, a test command without a config directory, a server that sends no
/// S2). Called past the patch (the pregame, before STATION) and by `eqoa
/// play`, before the world's files and CLIENT's tables are read.
pub fn for_server(host: &str) {
    let r = if crate::servers::keeps() { Rules::from_dir(&crate::servers::folder(host)) } else { Rules::retail() };
    set(r);
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
        // record 3: CanSell's slti v0, v0, 2 made addi v0, v0, 1
        assert_eq!(r.sell_stack, StackTest::Plus(1));
        // record 4: CanInvite's slti v0, v0, 128 made sltiu v0, v0, 255
        assert_eq!(r.guild_cap, 255);
        let names: Vec<String> = r.worlds.as_ref().expect("its world table").iter().map(World::esf_name).collect();
        assert_eq!(names, ["TUNARIA.ESF", "RATHE.ESF", "ODUS.ESF", "LAVASTM.ESF", "PLANESKY.ESF", "SECRETS.ESF", "UNDERFOOT.ESF", "SECRETS.ESF"]);
        let w0 = &r.worlds.as_ref().unwrap()[0];
        assert_eq!((w0.name.as_str(), w0.name_id, w0.bounds[1], w0.bounds[3]), ("tunaria", 0x413, 12, 15));
        assert_eq!(r.world_file(6).as_deref(), Some("UNDERFOOT.ESF"));
        assert_eq!(r.armor_art, vec![(9, [0xC214_1C0A, 0x03C1_BE9E, 0x85EA_2A1D, 0x432C_4890, 0xB179_23D2], [0x073BEC, 0x072257, 0x0C7D31, 0x06C8BE, 0x06AF29])]);
    }

    /// The live S2's hello and record of the pad, read off its caves: 21,
    /// then the boot patch's word, the build 0x31 and the 28-byte device
    /// block whose install id is the card's `ii`; UseSkill's 11 bytes. The
    /// July build's hello (and ours, which is July's) is 19 and its build
    /// 4, no device block, and is not taken (retail's goes); neither has the
    /// record.
    #[test]
    fn the_hello_and_the_record_of_the_pad() {
        let live_hello = Hello { version: 21, more: vec![HelloPart::Word(at::BOOT_SS_BLOCK), HelloPart::Const(0x31), HelloPart::Block(at::DEVICE_BLOCK, 28)], id_file: Some("ii".into()) };
        if let Some(data) = build("live") {
            let p = Patch::parse(&data).unwrap();
            let f = Features::read(&p.apply(0));
            assert_eq!(f.hello.as_ref(), Some(&live_hello));
            assert!(f.use_skill_record);
            let r = Rules::from_patch(&p);
            assert_eq!(r.hello, Some(live_hello));
            assert!(r.use_skill_record);
        }
        for which in ["july", "ours"] {
            let Some(data) = build(which) else { continue };
            let p = Patch::parse(&data).unwrap();
            let f = Features::read(&p.apply(0));
            assert_eq!(f.hello, Some(Hello { version: 19, more: vec![HelloPart::Word(at::BOOT_SS_BLOCK), HelloPart::Const(4)], id_file: None }), "{which}");
            assert!(!f.use_skill_record, "{which}");
            let r = Rules::from_patch(&p);
            assert!(r.hello.is_none() && r.shard_hello().is_none() && !r.use_skill_record, "{which}: retail's hello");
        }
    }

    /// The device block as this client makes it: no hardware address, the
    /// install id minted from the clock as S2 mints it and written as the
    /// folder's `ii` (status 0x30: no file read, then written), read back
    /// later (0x04), a short file minted over (0x08 | 0x20). The hello's
    /// bytes after its opcode: 21, the boot word 3, the build, the block.
    /// The controls: retail's rules, and a hello with a part this client
    /// cannot make, send retail's.
    #[test]
    fn the_hello_s_bytes_and_the_install_id() {
        let dir = std::env::temp_dir().join(format!("eqoa-s2-hello-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = make_device_block(&dir, "ii", 0x1234_5678);
        assert_eq!(first[..6], [0; 6], "no hardware address");
        assert_eq!((first[6], first[7]), (0x30, 0));
        assert_eq!(first[8..12], 0x1234_5678u32.to_le_bytes());
        assert_eq!(first[12..16], CLIENT_BASE.wrapping_add(0x1234_5678 << 13).to_le_bytes());
        assert_eq!(first[16..], [0; 12]);
        assert_eq!(std::fs::read(dir.join("ii")).unwrap(), first[8..16]);
        let again = make_device_block(&dir, "ii", 0x9999);
        assert_eq!(again[6], 0x04, "read back");
        assert_eq!(again[8..16], first[8..16], "the same id");
        std::fs::write(dir.join("ii"), [1, 2, 3]).unwrap();
        let short = make_device_block(&dir, "ii", 7);
        assert_eq!(short[6], 0x08 | 0x20);
        assert_eq!(short[8..12], 7u32.to_le_bytes());
        let live = Hello { version: 21, more: vec![HelloPart::Word(at::BOOT_SS_BLOCK), HelloPart::Const(0x31), HelloPart::Block(at::DEVICE_BLOCK, 28)], id_file: Some("ii".into()) };
        let r = Rules { hello: Some(live.clone()), dir: Some(dir.clone()), ..Rules::retail() };
        let h = r.shard_hello().unwrap();
        assert_eq!(h[..12], [21, 0, 0, 0, 3, 0, 0, 0, 0x31, 0, 0, 0]);
        assert_eq!(h.len(), 12 + 28);
        assert_eq!(h[12 + 6], 0x04, "the folder's id");
        assert_eq!(h[12 + 8..12 + 16], std::fs::read(dir.join("ii")).unwrap()[..]);
        assert_eq!(r.shard_hello(), Some(h), "made once a run");
        assert!(Rules::retail().shard_hello().is_none());
        let odd = Rules { hello: Some(Hello { more: vec![HelloPart::Word(0x0015_7204)], ..live.clone() }), dir: Some(dir.clone()), ..Rules::retail() };
        assert!(odd.shard_hello().is_none());
        assert!(Rules { hello: Some(live), ..Rules::retail() }.shard_hello().is_none(), "no folder to keep the id in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// S2's record of the pad as its caves keep it: the press's time, the
    /// frame, the key's hold the time before (0xFFFF when none was measured
    /// or past 65535), the flags (bit 0 the player's own press, bit 1 the
    /// pad read, bit 2 pressed this frame) under the key's low four bits.
    /// The controls: a stick's key and one past the buttons change nothing.
    #[test]
    fn the_record_of_the_pad() {
        let mut p = Presses::default();
        assert_eq!(p.tail(), [0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0x04]);
        for _ in 0..3 {
            p.frame_end();
        }
        p.key(135, true, 1000, true);
        assert_eq!(p.tail(), [0xE8, 0x03, 0, 0, 3, 0, 0, 0, 0xFF, 0xFF, 0x77]);
        p.key(135, false, 1100, true);
        p.frame_end();
        p.key(135, true, 2000, false);
        assert_eq!(p.tail(), [0xD0, 0x07, 0, 0, 4, 0, 0, 0, 100, 0, 0x76], "the hold the time before; a script's press is not seen");
        p.frame_end();
        assert_eq!(p.tail()[10], 0x72, "a frame later");
        let before = p.tail();
        p.key(129, true, 3000, true);
        p.key(148, true, 3000, true);
        assert_eq!(p.tail(), before, "a stick's key and one past the buttons");
        p.key(135, false, 72_000, true);
        p.key(135, true, 80_000, true);
        assert_eq!(p.tail()[8..10], [0xFF, 0xFF], "a hold past 16 bits");
        p.key(132, true, 90_000, true);
        assert_eq!(p.tail()[10] >> 4, 132 & 0xF);
    }

    /// July's build is the live one without the auction change; ours raises
    /// the armour set clamp for its set 10.
    #[test]
    fn the_other_builds() {
        if let Some(r) = rules("july") {
            assert_eq!((r.auction_max_level, r.armor_set_limit, r.tints.as_ref().map(Vec::len), r.worlds.as_ref().map(Vec::len)), (50, 10, Some(30), Some(8)));
            assert_eq!((r.sell_stack, r.guild_cap), (StackTest::Plus(1), 255));
        }
        if let Some(r) = rules("ours") {
            assert_eq!((r.armor_set_limit, r.robe_limit, r.player_search), (11, 6, true));
            assert_eq!((r.sell_stack, r.guild_cap), (StackTest::Plus(1), 255));
        }
    }

    /// CanSell's stack test as each instruction makes it: retail's slti
    /// refuses any stack, S2's addi passes everything but a max stack of
    /// -1; another instruction at the site is no test this reader knows.
    #[test]
    fn the_stack_test_of_each_instruction() {
        let retail = StackTest::of(0x2842_0002).unwrap();
        assert_eq!(retail, StackTest::Below(2));
        assert!(retail.passes(0) && retail.passes(1));
        assert!(!retail.passes(2) && !retail.passes(20) && !retail.passes(100));
        let s2 = StackTest::of(0x2042_0001).unwrap();
        assert_eq!(s2, StackTest::Plus(1));
        assert!(s2.passes(1) && s2.passes(20) && s2.passes(100) && s2.passes(0));
        assert!(!s2.passes(-1), "the one item S2's sum leaves at 0");
        assert_eq!(StackTest::of(0x2442_0001), Some(StackTest::Plus(1)), "addiu");
        // the game's word is 32 bits
        assert!(retail.passes(1 << 32 | 1) && !retail.passes(1 << 32 | 2));
        for w in [0x0000_0000, 0x2843_0002, 0x2862_0002, 0x3442_0001] {
            assert_eq!(StackTest::of(w), None, "{w:08X}");
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
            assert_eq!((r.armor_set_limit, r.robe_limit, r.auction_max_level, r.sell_stack, r.guild_cap), (9, 4, 50, StackTest::Below(2), 128));
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
        assert_eq!(r.sell_stack, StackTest::Below(2), "no record at CanSell: retail's test");
        // and one that rewrites CanSell's test
        let mut p = rec(at::CLIENT_CAN_SELL_STACK, &0x2042_0001u32.to_le_bytes());
        p.extend(tail(&[]));
        std::fs::write(dir.join("S2"), &p).unwrap();
        assert_eq!(Rules::from_dir(&dir).sell_stack, StackTest::Plus(1));
        // CanInvite's cap, slti or sltiu; another instruction there is retail's
        for (w, cap) in [(0x2C42_00FFu32, 255), (0x2842_00C8, 200), (0x2843_00FF, 128)] {
            let mut p = rec(at::CLIENT_GUILD_CAP, &w.to_le_bytes());
            p.extend(tail(&[]));
            std::fs::write(dir.join("S2"), &p).unwrap();
            assert_eq!(Rules::from_dir(&dir).guild_cap, cap, "{w:08X}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
