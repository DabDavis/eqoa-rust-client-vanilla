//! Stand-ins for `UTIL.REL`'s imports: what the launcher's libc, kernel and
//! libcdvd do for the DNAS library, written against their behaviour in the
//! launcher (`rand` is its newlib's LCG; `sceCdReadClock` its RPC). An import
//! with no stand-in stops the run and names itself.

use crate::cpu::{Mem, A0, V0, V1};
use crate::{Error, Machine};

const A1: usize = A0 + 1;

/// The console this runs as: an i.LINK ID with cdvdman's default model
/// (0x1A0002) in bytes 1-3.
const ILINK_ID: [u8; 8] = [0x21, 0x02, 0x00, 0x1A, 0x78, 0x56, 0x34, 0x12];
const A2: usize = A0 + 2;

/// A bump heap: what the library frees is not reused, and a block's size
/// sits in the 16 bytes before it (for `realloc`).
pub struct Heap {
    next: u32,
    end: u32,
}

impl Heap {
    pub fn alloc(&mut self, align: u32, size: u32) -> Option<u32> {
        let align = align.max(16);
        let p = (self.next + 16 + align - 1) & !(align - 1);
        let end = p.checked_add(size)?;
        if end > self.end {
            return None;
        }
        self.next = (end + 15) & !15;
        Some(p)
    }
}

pub struct State {
    pub heap: Heap,
    pub rand: u32,
    /// `_sceCd_cd_scmd`'s address: the RPC client of the S-command server.
    pub scmd_client: Option<u32>,
    sizes: std::collections::HashMap<u32, u32>,
    sema: u32,
}

impl State {
    pub fn new(base: u32, end: u32) -> Self {
        State { heap: Heap { next: base, end }, rand: 1, scmd_client: None, sizes: Default::default(), sema: 0 }
    }

    fn malloc(&mut self, mem: &mut Mem, align: u32, size: u32, zero: bool) -> u32 {
        match self.heap.alloc(align, size) {
            Some(p) => {
                self.sizes.insert(p, size);
                if zero {
                    mem.slice_mut(p, size as usize).unwrap().fill(0);
                }
                p
            }
            None => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Hle {
    Unbound,
    Unknown(String),
    // data
    Ctype,
    Impure,
    Data(&'static str, u32),
    // libc
    Memset,
    Memcpy,
    Memmove,
    Bcopy,
    Memcmp,
    Strlen,
    Strcmp,
    Strncmp,
    Strcpy,
    Strncpy,
    Strcat,
    Malloc,
    Free,
    Calloc,
    Realloc,
    Memalign,
    Rand,
    Srand,
    Printf(&'static str),
    Udivdi3,
    Umoddi3,
    Divdi3,
    Moddi3,
    // kernel
    FlushCache,
    CreateSema,
    DeleteSema,
    WaitSema,
    SignalSema,
    PollSema,
    GetThreadId,
    Getpid,
    // libcdvd
    ScmdPrechk,
    ReadClock,
    SifCallRpc,
}

impl Hle {
    pub fn bind(name: &str) -> Hle {
        use Hle::*;
        match name {
            "_ctype_" => Ctype,
            "_impure_ptr" => Impure,
            "_gp" => Data("_gp", 16),
            "_fbss" => Data("_fbss", 16),
            "_end" => Data("_end", 16),
            "_stack" => Data("_stack", 16),
            "_stack_size" => Data("_stack_size", 16),
            "_heap_size" => Data("_heap_size", 16),
            "_sceCd_cd_scmd" => Data("_sceCd_cd_scmd", 64),
            "_sceCd_scmdrdata" => Data("_sceCd_scmdrdata", 64),
            "_sceCd_scmd_semid" => Data("_sceCd_scmd_semid", 16),
            "_sceCd_cd_ncmd" => Data("_sceCd_cd_ncmd", 64),
            "_sceCd_ncmdrdata" => Data("_sceCd_ncmdrdata", 64),
            "_sceCd_ncmd_semid" => Data("_sceCd_ncmd_semid", 16),
            "memset" => Memset,
            "memcpy" => Memcpy,
            "memmove" => Memmove,
            "bcopy" => Bcopy,
            "memcmp" => Memcmp,
            "strlen" => Strlen,
            "strcmp" => Strcmp,
            "strncmp" => Strncmp,
            "strcpy" => Strcpy,
            "strncpy" => Strncpy,
            "strcat" => Strcat,
            "malloc" => Malloc,
            "free" => Free,
            "calloc" => Calloc,
            "realloc" => Realloc,
            "memalign" => Memalign,
            "rand" => Rand,
            "srand" => Srand,
            "printf" => Printf("printf"),
            "scePrintf" => Printf("scePrintf"),
            "__udivdi3" => Udivdi3,
            "__umoddi3" => Umoddi3,
            "__divdi3" => Divdi3,
            "__moddi3" => Moddi3,
            "FlushCache" => FlushCache,
            "CreateSema" => CreateSema,
            "DeleteSema" => DeleteSema,
            "WaitSema" => WaitSema,
            "SignalSema" => SignalSema,
            "PollSema" => PollSema,
            "GetThreadId" => GetThreadId,
            "getpid" => Getpid,
            "_sceCd_scmd_prechk" => ScmdPrechk,
            "sceCdReadClock" => ReadClock,
            "sceSifCallRpc" => SifCallRpc,
            n => Unknown(n.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        use Hle::*;
        match self {
            Unbound => "(unbound)",
            Unknown(n) => n,
            Data(n, _) => n,
            Printf(n) => n,
            h => match h {
                Ctype => "_ctype_",
                Impure => "_impure_ptr",
                Memset => "memset",
                Memcpy => "memcpy",
                Memmove => "memmove",
                Bcopy => "bcopy",
                Memcmp => "memcmp",
                Strlen => "strlen",
                Strcmp => "strcmp",
                Strncmp => "strncmp",
                Strcpy => "strcpy",
                Strncpy => "strncpy",
                Strcat => "strcat",
                Malloc => "malloc",
                Free => "free",
                Calloc => "calloc",
                Realloc => "realloc",
                Memalign => "memalign",
                Rand => "rand",
                Srand => "srand",
                Udivdi3 => "__udivdi3",
                Umoddi3 => "__umoddi3",
                Divdi3 => "__divdi3",
                Moddi3 => "__moddi3",
                FlushCache => "FlushCache",
                CreateSema => "CreateSema",
                DeleteSema => "DeleteSema",
                WaitSema => "WaitSema",
                SignalSema => "SignalSema",
                PollSema => "PollSema",
                GetThreadId => "GetThreadId",
                Getpid => "getpid",
                ScmdPrechk => "_sceCd_scmd_prechk",
                ReadClock => "sceCdReadClock",
                SifCallRpc => "sceSifCallRpc",
                _ => unreachable!(),
            },
        }
    }

    /// A data import's size: it is given room in the data area rather than
    /// a stub.
    pub fn data_size(&self) -> Option<u32> {
        match self {
            Hle::Ctype => Some(260),
            Hle::Impure => Some(16 + 0x400),
            Hle::Data(_, n) => Some(*n),
            _ => None,
        }
    }

    pub fn init_data(&self, mem: &mut Mem, state: &mut State, addr: u32) {
        match self {
            Hle::Data("_sceCd_cd_scmd", _) => state.scmd_client = Some(addr),
            Hle::Ctype => {
                // newlib's `_ctype_`: EOF, then the 128 ASCII classes
                const U: u8 = 0o1;
                const L: u8 = 0o2;
                const N: u8 = 0o4;
                const S: u8 = 0o10;
                const P: u8 = 0o20;
                const C: u8 = 0o40;
                const X: u8 = 0o100;
                const B: u8 = 0o200;
                for c in 0u32..128 {
                    let v = match c {
                        0x09..=0x0D => C | S,
                        0x00..=0x1F | 0x7F => C,
                        0x20 => S | B,
                        0x30..=0x39 => N,
                        0x41..=0x46 => U | X,
                        0x47..=0x5A => U,
                        0x61..=0x66 => L | X,
                        0x67..=0x7A => L,
                        _ => P,
                    };
                    mem.w8(addr + 1 + c, v).unwrap();
                }
            }
            Hle::Impure => {
                // the pointer, then the zeroed `struct _reent` it points at
                mem.w32(addr, addr + 16).unwrap();
            }
            _ => {}
        }
    }

    pub fn run(&self, m: &mut Machine) -> Result<(), Error> {
        use Hle::*;
        let a = |m: &Machine, k: usize| m.cpu.gpr[A0 + k] as u32;
        let ret = |m: &mut Machine, v: u64| m.cpu.gpr[V0] = v;
        let bad = |va: u32| Error::Trap(crate::Trap::Fault { pc: 0, va });
        match self {
            Unbound | Ctype | Impure | Data(..) => return Err(Error::Import(format!("a call to data import {}", self.name()))),
            Unknown(n) => return Err(Error::Import(n.clone())),
            Memset => {
                let (d, c, n) = (a(m, 0), a(m, 1), a(m, 2));
                m.mem.slice_mut(d, n as usize).ok_or(bad(d))?.fill(c as u8);
                ret(m, m.cpu.gpr[A0]);
            }
            Memcpy | Memmove | Bcopy => {
                let (d, s, n) = if matches!(self, Bcopy) { (a(m, 1), a(m, 0), a(m, 2)) } else { (a(m, 0), a(m, 1), a(m, 2)) };
                let src = m.mem.slice(s, n as usize).ok_or(bad(s))?.to_vec();
                m.mem.slice_mut(d, n as usize).ok_or(bad(d))?.copy_from_slice(&src);
                ret(m, m.cpu.gpr[A0]);
            }
            Memcmp => {
                let (p, q, n) = (a(m, 0), a(m, 1), a(m, 2) as usize);
                let x = m.mem.slice(p, n).ok_or(bad(p))?;
                let y = m.mem.slice(q, n).ok_or(bad(q))?;
                let r = x.iter().zip(y).find(|(u, v)| u != v).map_or(0, |(u, v)| *u as i32 - *v as i32);
                ret(m, r as i64 as u64);
            }
            Strlen => {
                let n = cstr(&m.mem, a(m, 0)).ok_or(bad(a(m, 0)))?.len();
                ret(m, n as u64);
            }
            Strcmp | Strncmp => {
                let x = cstr(&m.mem, a(m, 0)).ok_or(bad(a(m, 0)))?;
                let y = cstr(&m.mem, a(m, 1)).ok_or(bad(a(m, 1)))?;
                let lim = if matches!(self, Strncmp) { a(m, 2) as usize } else { usize::MAX };
                let mut r = 0i32;
                for k in 0..lim {
                    let (u, v) = (*x.get(k).unwrap_or(&0), *y.get(k).unwrap_or(&0));
                    if u != v || u == 0 {
                        r = u as i32 - v as i32;
                        break;
                    }
                }
                ret(m, r as i64 as u64);
            }
            Strcpy | Strcat => {
                let (d, s) = (a(m, 0), a(m, 1));
                let mut src = cstr(&m.mem, s).ok_or(bad(s))?;
                src.push(0);
                let at = if matches!(self, Strcat) { d + cstr(&m.mem, d).ok_or(bad(d))?.len() as u32 } else { d };
                m.mem.slice_mut(at, src.len()).ok_or(bad(at))?.copy_from_slice(&src);
                ret(m, d as u64);
            }
            Strncpy => {
                let (d, s, n) = (a(m, 0), a(m, 1), a(m, 2) as usize);
                let src = cstr(&m.mem, s).ok_or(bad(s))?;
                let dst = m.mem.slice_mut(d, n).ok_or(bad(d))?;
                for (k, b) in dst.iter_mut().enumerate() {
                    *b = *src.get(k).unwrap_or(&0);
                }
                ret(m, d as u64);
            }
            Malloc => {
                let n = a(m, 0);
                let p = m.state.malloc(&mut m.mem, 16, n, false);
                ret(m, p as u64);
            }
            Calloc => {
                let n = a(m, 0).wrapping_mul(a(m, 1));
                let p = m.state.malloc(&mut m.mem, 16, n, true);
                ret(m, p as u64);
            }
            Memalign => {
                let (al, n) = (a(m, 0), a(m, 1));
                let p = m.state.malloc(&mut m.mem, al, n, false);
                ret(m, p as u64);
            }
            Realloc => {
                let (old, n) = (a(m, 0), a(m, 1));
                let p = m.state.malloc(&mut m.mem, 16, n, false);
                if old != 0 && p != 0 {
                    let keep = m.state.sizes.get(&old).copied().unwrap_or(0).min(n) as usize;
                    let b = m.mem.slice(old, keep).ok_or(bad(old))?.to_vec();
                    m.mem.slice_mut(p, keep).unwrap().copy_from_slice(&b);
                }
                ret(m, p as u64);
            }
            Free => {}
            Rand => {
                // the launcher's newlib: next = next * 1103515245 + 12345
                m.state.rand = m.state.rand.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                ret(m, (m.state.rand & 0x7FFF_FFFF) as u64);
            }
            Srand => m.state.rand = a(m, 0),
            Printf(_) => {
                if m.trace {
                    let f = cstr(&m.mem, a(m, 0)).unwrap_or_default();
                    eprint!("[{}] {}", self.name(), String::from_utf8_lossy(&f));
                }
                ret(m, 0);
            }
            Udivdi3 | Umoddi3 | Divdi3 | Moddi3 => {
                let (x, y) = (m.cpu.gpr[A0], m.cpu.gpr[A1]);
                if y == 0 {
                    return Err(Error::Import(format!("{} by zero", self.name())));
                }
                let r = match self {
                    Udivdi3 => x / y,
                    Umoddi3 => x % y,
                    Divdi3 => (x as i64).wrapping_div(y as i64) as u64,
                    _ => (x as i64).wrapping_rem(y as i64) as u64,
                };
                ret(m, r);
            }
            FlushCache => {}
            CreateSema => {
                m.state.sema += 1;
                ret(m, m.state.sema as u64);
            }
            DeleteSema | WaitSema | SignalSema | PollSema => ret(m, a(m, 0) as u64),
            GetThreadId => ret(m, 1),
            Getpid => ret(m, 1),
            ScmdPrechk => ret(m, 1),
            ReadClock => {
                // sceCdCLOCK: status, then BCD second, minute, hour, pad,
                // day, month, year. A fixed moment: the library seeds rand
                // with it, and nothing it unpacks depends on rand.
                let p = a(m, 0);
                m.mem.slice_mut(p, 8).ok_or(bad(p))?.copy_from_slice(&[0, 0x00, 0x00, 0x12, 0, 0x01, 0x01, 0x04]);
                ret(m, 1);
            }
            SifCallRpc => {
                // sceSifCallRpc(client, fno, mode, send, ssize, recv, rsize, end)
                let (client, fno, recv) = (a(m, 0), a(m, 1), a(m, 5));
                let reply: Option<Vec<u8>> = if Some(client) == m.state.scmd_client {
                    // cdvdfsv's S-command server (ps2sdk's cdvdfsv.c,
                    // cbrpc_rpc3_cdvdscmds): {retres, packet}
                    // Both IDs come out of one i.LINK ID (cdvdman's
                    // cdvdman_readID over sceCdRI): what personalizing binds
                    // an image to, so any fixed one serves, extracting on
                    // the same "console".
                    let id = u32::from_le_bytes(ILINK_ID[0..4].try_into().unwrap());
                    let hi = u32::from_le_bytes(ILINK_ID[4..8].try_into().unwrap());
                    match fno {
                        // sceCdReadGUID
                        0x24 => Some([1u32.to_le_bytes(), (ILINK_ID[0] as u32 | 0x0800_4600).to_le_bytes(), hi.to_le_bytes()].concat()),
                        // sceCdReadModelID
                        0x26 => Some([1u32.to_le_bytes(), (id >> 8).to_le_bytes()].concat()),
                        _ => None,
                    }
                } else {
                    None
                };
                match reply {
                    Some(b) => {
                        m.mem.slice_mut(recv, b.len()).ok_or(bad(recv))?.copy_from_slice(&b);
                        ret(m, 0);
                    }
                    None => {
                        let regs: Vec<String> = (0..8).map(|k| format!("{:#x}", a(m, k))).collect();
                        return Err(Error::Import(format!("sceSifCallRpc({})", regs.join(", "))));
                    }
                }
            }
        }
        let _ = (A2, V1);
        Ok(())
    }
}

/// The NUL-terminated string at `va` (without the NUL).
fn cstr(mem: &Mem, va: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut p = va;
    loop {
        let c = mem.r8(p)?;
        if c == 0 {
            return Some(out);
        }
        out.push(c);
        p = p.wrapping_add(1);
        if out.len() > 1 << 20 {
            return None;
        }
    }
}
