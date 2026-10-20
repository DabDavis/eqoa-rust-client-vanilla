//! The disc's DNAS-encrypted modules, unpacked by the disc's own DNAS library.
//!
//! SUPPORT, CLIENT, SETUP, STATION and MCERR ship as `.AUT` files: a 256-byte
//! DNAS header, the module (a 4-byte length and a zlib stream) under a stream
//! cipher, and a trailer. The launcher (`VILauncher::UnpackAndVerify`)
//! "personalizes" such an authored image for the console and then extracts
//! it, with `sceDNAS2InstPersonalizeData_nohdd` and
//! `sceDNAS2InstExtractData_nohdd` out of `UTIL.REL`, a module on the same
//! disc that is not encrypted, and inflates what comes out.
//!
//! This crate does the same thing by running that code: `UTIL.REL` is loaded
//! and relocated as SN's DLL loader does it, its imports (libc, the kernel's
//! semaphores, libcdvd's S-command RPC) are bound to stand-ins here, and an
//! interpreter for the EE's integer core runs its constructors, the init and
//! the four calls, in the launcher's order with the launcher's arguments.
//! What decrypts is the player's own disc, and no key is in this code. The
//! standard functions the library spends half its instructions in (SHA-1,
//! RC6, CBC, libgcc's multiply and doubles, BSAFE's bignum) are computed
//! natively where its code is the code they were proved against
//! (`native.rs`); DNAS's own logic is interpreted.
//!
//! Checked against the modules the game itself unpacked inside PCSX2 (the
//! tracer's `call` of `UnpackAndVerify`): `tests/discs.rs`; the natives
//! against the interpreter, call for call: `tests/natives.rs`.

mod cpu;
mod hle;
mod native;
mod snr1;

pub use cpu::{Cpu, Mem, Trap};
use cpu::{A0, RA, SP, Stop, V0};
use hle::Hle;

/// Where the launcher's DLL loader puts `UTIL.REL` (the first module after
/// the launcher's own `.bss`); keeping it there keeps addresses comparable
/// with a running game's.
pub const UTIL_BASE: u32 = 0x0017_0E80;
const STUB_BASE: u32 = 0x0001_0000;
const DATA_BASE: u32 = 0x0002_0000;
const DATA_END: u32 = 0x0010_0000;
const HEAP_BASE: u32 = 0x0030_0000;
const HEAP_END: u32 = 0x01F0_0000;
const STACK_TOP: u32 = 0x01FF_FF00;
/// `$ra` for a call from here: reaching it ends the call.
const RETURN: u32 = 0x0000_0100;
/// A guard against a library that never returns: SUPPORT takes ~340M
/// instructions, inflation included.
const BUDGET: u64 = 8_000_000_000;

#[derive(Debug)]
pub enum Error {
    /// `UTIL.REL` is not an SNR1 module, or lacks the DNAS exports.
    Util(&'static str),
    Trap(Trap),
    /// The library called an import there is no stand-in for.
    Import(String),
    Budget,
    /// A DNAS call returned an error (the launcher's message, and the code).
    Dnas(&'static str, i32),
    Inflate(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Util(e) => write!(f, "UTIL.REL: {e}"),
            Error::Trap(t) => write!(f, "{t}"),
            Error::Import(n) => write!(f, "the DNAS library called {n}, which has no stand-in"),
            Error::Budget => write!(f, "the DNAS library ran past {BUDGET} instructions"),
            Error::Dnas(what, code) => write!(f, "{what} failed ({code})"),
            Error::Inflate(e) => write!(f, "decompression failed: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// `UTIL.REL` loaded into an EE, its constructors and DNAS init run.
pub struct Machine {
    pub cpu: Cpu,
    pub mem: Mem,
    pub(crate) hle: Vec<Hle>,
    pub(crate) state: hle::State,
    exports: std::collections::HashMap<String, u32>,
    /// Every stand-in called, by name (for `examples/unpack.rs --trace`).
    pub calls: std::collections::BTreeMap<String, u64>,
    pub trace: bool,
    /// Instructions retired per word of `UTIL.REL`, when wanted.
    pub profile: Option<Vec<u64>>,
    /// A function called before the instruction at each marked address
    /// runs (an instrument for reading the library, `examples/unpack.rs`).
    pub hooks: Option<(std::collections::HashSet<u32>, Hook)>,
    /// The functions computed natively (`native.rs`), by their index in
    /// `Mem::natives`.
    natives: Vec<native::Native>,
    /// When set, natives are checked against the interpreter
    /// (`check_natives`).
    pub check: Option<Check>,
    /// Every instruction of the library interpreted, no natives (for reading
    /// the library through `hooks` and `profile` as it is).
    pub interpret_only: bool,
}

/// The natives checked against the interpreter: at a sampled call the native
/// runs, then the machine goes back to the call and the interpreter runs the
/// function to its return, and the two are compared: the registers a call
/// keeps (`v0` too where it is the result) and all of RAM but the stack below
/// the call. The interpreter's run is the one kept.
pub struct Check {
    /// every call up to `first`, then one in `every`
    pub first: u64,
    pub every: u64,
    /// per native: its name, calls, calls checked, calls it declined
    pub counts: Vec<(&'static str, u64, u64, u64)>,
    /// what differed, each the first thing for a call
    pub failures: Vec<String>,
}

/// Called with the machine and the address reached; it may mark further
/// addresses (a call's return, to see what it wrote).
pub type Hook = Box<dyn FnMut(&Cpu, &Mem, u32, &mut std::collections::HashSet<u32>)>;

impl Machine {
    pub fn new(util: &[u8]) -> Result<Self, Error> {
        let module = snr1::Module::parse(util).ok_or(Error::Util("not an SNR1 module"))?;
        let mut mem = Mem::default();
        let mut state = hle::State::new(HEAP_BASE, HEAP_END);
        let mut data = DATA_BASE;
        let mut hle = Vec::new();
        let image = module.relocate(UTIL_BASE, |i, name| {
            let h = Hle::bind(name);
            let addr = match h.data_size() {
                Some(size) => {
                    let a = data;
                    data = (data + size + 15) & !15;
                    a
                }
                None => STUB_BASE + 8 * i as u32,
            };
            hle.push((i, h, addr));
            addr
        });
        assert!(data <= DATA_END);
        let len = module.len() as usize;
        mem.slice_mut(UTIL_BASE, len).ok_or(Error::Util("does not fit"))?.copy_from_slice(&image[..len]);
        mem.watch_code(UTIL_BASE, len as u32);
        let mut table = vec![Hle::Unbound; module.symbols.len()];
        for (i, h, addr) in hle {
            h.init_data(&mut mem, &mut state, addr);
            table[i] = h;
        }
        // Each stub is `jr ra; nop`, though the interpreter takes it before.
        for i in 0..table.len() as u32 {
            mem.w32(STUB_BASE + 8 * i, 0x03E0_0008).unwrap();
        }
        let exports: std::collections::HashMap<String, u32> = module
            .symbols
            .iter()
            .skip(1)
            .filter(|s| s.offset != 0)
            .map(|s| (s.name.clone(), UTIL_BASE + s.offset))
            .collect();
        let natives = native::find(&mem, |n: &str| exports.get(n).copied());
        mem.natives = natives.iter().map(|n| (n.at, n.len, n.name)).collect();
        let mut m = Machine {
            cpu: Cpu::default(),
            mem,
            hle: table,
            state,
            exports,
            calls: Default::default(),
            trace: false,
            profile: None,
            hooks: None,
            natives,
            check: None,
            interpret_only: false,
        };
        // snDllLoaded: the module's init (its static constructors)
        let init = module.init();
        if init != 0 {
            m.call(UTIL_BASE + init, &[])?;
        }
        Ok(m)
    }

    pub fn export(&self, name: &str) -> Option<u32> {
        self.exports.get(name).copied()
    }

    /// Every export, by address.
    pub fn exports(&self) -> Vec<(u32, String)> {
        let mut v: Vec<(u32, String)> = self.exports.iter().map(|(n, &a)| (a, n.clone())).collect();
        v.sort();
        v
    }

    /// Calls a guest function with up to eight word arguments (EABI: `a0-a3`
    /// then `t0-t3`) and returns `v0`.
    pub fn call(&mut self, addr: u32, args: &[u32]) -> Result<u64, Error> {
        let args: Vec<u64> = args.iter().map(|&a| a as i32 as i64 as u64).collect();
        self.call_regs(addr, &args)
    }

    /// `call`, the argument registers given whole (64 bits each).
    pub fn call_regs(&mut self, addr: u32, args: &[u64]) -> Result<u64, Error> {
        for (k, &a) in args.iter().enumerate() {
            self.cpu.gpr[A0 + k] = a;
        }
        self.cpu.gpr[SP] = STACK_TOP as u64;
        self.cpu.gpr[RA] = RETURN as u64;
        self.cpu.jump(addr);
        let end = self.cpu.insns + BUDGET;
        let stubs = self.hle.len() as u32 * 8;
        // the instruments want each instruction, so they step one at a time
        let decoded = self.profile.is_none() && self.hooks.is_none() && !self.interpret_only;
        loop {
            if decoded && let Stop::Native(k) = self.cpu.run(&mut self.mem, end).map_err(Error::Trap)? {
                self.native(k)?;
                continue;
            }
            let pc = self.cpu.pc;
            if !decoded
                && !self.interpret_only
                && let Some(k) = self.mem.natives.iter().position(|&(at, _, _)| at == pc)
            {
                self.native(k)?;
                continue;
            }
            if pc == RETURN {
                return Ok(self.cpu.gpr[V0]);
            }
            if pc.wrapping_sub(STUB_BASE) < stubs {
                let i = ((pc - STUB_BASE) / 8) as usize;
                self.dispatch(i)?;
                let ra = self.cpu.gpr[RA] as u32;
                self.cpu.jump(ra);
                continue;
            }
            if let Some(p) = &mut self.profile
                && let Some(c) = p.get_mut((pc.wrapping_sub(UTIL_BASE) / 4) as usize)
            {
                *c += 1;
            }
            if let Some((at, _)) = &self.hooks
                && at.contains(&pc)
            {
                let (mut at, mut f) = self.hooks.take().unwrap();
                f(&self.cpu, &self.mem, pc, &mut at);
                self.hooks = Some((at, f));
            }
            self.cpu.step(&mut self.mem).map_err(Error::Trap)?;
            if self.cpu.insns >= end {
                return Err(Error::Budget);
            }
        }
    }

    /// The names of the functions computed natively (those whose code is as
    /// proved; the others are interpreted).
    pub fn natives(&self) -> Vec<&'static str> {
        self.natives.iter().map(|n| n.name).collect()
    }

    /// Checks the natives against the interpreter from now on: every call up
    /// to `first` of each, then one in `every` (`Check`).
    pub fn check_natives(&mut self, first: u64, every: u64) {
        let counts = self.natives.iter().map(|n| (n.name, 0, 0, 0)).collect();
        self.check = Some(Check { first, every: every.max(1), counts, failures: Vec::new() });
    }

    /// At the entry of native `k`: computed natively and returned from, or,
    /// declined, the entry's instruction interpreted.
    fn native(&mut self, k: usize) -> Result<(), Error> {
        if let Some(c) = self.check.as_mut() {
            let calls = &mut c.counts[k].1;
            *calls += 1;
            if *calls <= c.first || *calls % c.every == 0 {
                return self.check_native(k);
            }
        }
        if (self.natives[k].run)(&mut self.cpu, &mut self.mem).is_some() {
            let ra = self.cpu.gpr[RA] as u32;
            self.cpu.jump(ra);
            return Ok(());
        }
        self.cpu.step(&mut self.mem).map_err(Error::Trap)
    }

    fn check_native(&mut self, k: usize) -> Result<(), Error> {
        // the native's run, logged, then undone
        let cpu0 = self.cpu.clone();
        self.mem.log = Some(Vec::new());
        let ran = (self.natives[k].run)(&mut self.cpu, &mut self.mem).is_some();
        let written = self.mem.log.take().unwrap_or_default();
        let natively: Vec<(usize, Vec<u8>)> = written.iter().map(|(at, old)| (*at, self.mem.logged(*at, old.len()).to_vec())).collect();
        for (at, old) in written.iter().rev() {
            self.mem.restore(*at, old);
        }
        let cpu_n = std::mem::replace(&mut self.cpu, cpu0);
        let name = self.natives[k].name;
        let c = self.check.as_mut().unwrap();
        if !ran {
            c.counts[k].3 += 1;
            // a native that declines leaves the machine as it found it
            if !written.is_empty() || cpu_n.gpr != self.cpu.gpr {
                c.failures.push(format!("{name} declined a call and changed the machine"));
            }
            return self.cpu.step(&mut self.mem).map_err(Error::Trap);
        }
        c.counts[k].2 += 1;
        // the interpreter's run, logged, kept
        let (ra, sp) = (self.cpu.gpr[RA] as u32, self.cpu.gpr[SP]);
        let args: Vec<u64> = (0..6).map(|i| self.cpu.gpr[A0 + i]).collect();
        self.mem.log = Some(Vec::new());
        let r = self.interpret_until(ra, sp);
        let interpreted = self.mem.log.take().unwrap_or_default();
        r?;
        let mut differs = None;
        for &r in native::SAVED.iter().chain(self.natives[k].returns.then_some(&V0)) {
            if cpu_n.gpr[r] != self.cpu.gpr[r] {
                differs = Some(format!("r{r}: {:#x} natively, {:#x} interpreted", cpu_n.gpr[r], self.cpu.gpr[r]));
                break;
            }
        }
        // Every byte either run wrote: the native's last value (else what was
        // there before both) against the interpreter's. The stack below the
        // call is free: what the function left there is not its result.
        let mut native_byte = std::collections::HashMap::new();
        for (at, bytes) in &natively {
            for (i, &b) in bytes.iter().enumerate() {
                native_byte.insert(at + i, b);
            }
        }
        let mut before = std::collections::HashMap::new();
        for (at, old) in interpreted.iter().chain(&written).rev() {
            for (i, &b) in old.iter().enumerate() {
                before.insert(at + i, b);
            }
        }
        let free = HEAP_END as usize..(sp as u32 & 0x01FF_FFFF) as usize;
        let mut touched: Vec<usize> = before.keys().copied().filter(|a| !free.contains(a)).collect();
        touched.sort_unstable();
        if differs.is_none()
            && let Some(&at) = touched.iter().find(|&&a| native_byte.get(&a).unwrap_or(&before[&a]) != &self.mem.logged(a, 1)[0])
        {
            differs = Some(format!("memory at {at:#x}: {:#04x} natively, {:#04x} interpreted", native_byte.get(&at).unwrap_or(&before[&at]), self.mem.logged(at, 1)[0]));
        }
        if let Some(d) = differs {
            let c = self.check.as_mut().unwrap();
            c.failures.push(format!("{name}(a0..a5 = {args:x?}) called from {ra:#x}: {d}"));
        }
        Ok(())
    }

    /// Interprets (stand-ins included, no natives) until `pc` is `stop` with
    /// `sp` back at `sp`: a function run to its return.
    fn interpret_until(&mut self, stop: u32, sp: u64) -> Result<(), Error> {
        let stubs = self.hle.len() as u32 * 8;
        let end = self.cpu.insns + BUDGET;
        loop {
            let pc = self.cpu.pc;
            if pc == stop && self.cpu.gpr[SP] == sp {
                return Ok(());
            }
            if pc.wrapping_sub(STUB_BASE) < stubs {
                self.dispatch(((pc - STUB_BASE) / 8) as usize)?;
                let ra = self.cpu.gpr[RA] as u32;
                self.cpu.jump(ra);
                continue;
            }
            self.cpu.step(&mut self.mem).map_err(Error::Trap)?;
            if self.cpu.insns >= end {
                return Err(Error::Budget);
            }
        }
    }

    /// Swaps native `name`'s computation for `run` (the controls of
    /// `tests/natives.rs`: a wrong native must fail the check). Returns
    /// whether there was such a native.
    #[doc(hidden)]
    pub fn replace_native(&mut self, name: &str, run: fn(&mut Cpu, &mut Mem) -> Option<()>) -> bool {
        self.natives.iter_mut().find(|n| n.name == name).map(|n| n.run = run).is_some()
    }

    /// The CRC-32s `native.rs` checks each function against, as this
    /// library has them (`None`: no such export).
    pub fn native_checksums(&self) -> Vec<(&'static str, Option<u32>)> {
        native::checksums(&self.mem, |n| self.export(n))
    }

    fn dispatch(&mut self, i: usize) -> Result<(), Error> {
        let h = self.hle[i].clone();
        if self.trace {
            *self.calls.entry(h.name().to_string()).or_default() += 1;
            let r = &self.cpu.gpr;
            eprintln!(
                "{:>11} {:<20} a0={:#x} a1={:#x} a2={:#x} a3={:#x} ra={:#x}",
                self.cpu.insns, h.name(), r[A0] as u32, r[A0 + 1] as u32, r[A0 + 2] as u32, r[A0 + 3] as u32, r[RA] as u32
            );
        }
        h.run(self)
    }

    /// Room for the launcher's buffers, from the same heap the stand-ins use.
    pub fn alloc(&mut self, align: u32, size: u32) -> u32 {
        self.state.heap.alloc(align, size).expect("heap")
    }

    pub fn write(&mut self, va: u32, b: &[u8]) {
        self.mem.slice_mut(va, b.len()).expect("write").copy_from_slice(b);
    }

    pub fn read(&self, va: u32, len: usize) -> Vec<u8> {
        self.mem.slice(va, len).expect("read").to_vec()
    }
}

/// `VILauncher::UnpackAndVerify` for an authored image (a `.AUT` off the
/// disc): personalize, extract, inflate. Returns the SNR1 module, not
/// relocated.
pub fn unpack(util: &[u8], aut: &[u8]) -> Result<Vec<u8>, Error> {
    Machine::new(util)?.unpack(aut)
}

impl Machine {
    /// [`unpack`] on this machine.
    pub fn unpack(&mut self, aut: &[u8]) -> Result<Vec<u8>, Error> {
        let m = self;
        // libdnas2's names; the 2002 discs' library has the older ones, for
        // the same calls in the same order
        const NAMES: [[&str; 5]; 2] = [
            [
                "sceDNAS2InstInit_nohdd",
                "sceDNAS2InstPersonalizeDataLength_nohdd",
                "sceDNAS2InstPersonalizeData_nohdd",
                "sceDNAS2InstExtractDataLength_nohdd",
                "sceDNAS2InstExtractData_nohdd",
            ],
            [
                "sceDNAS2CryptInit_nohdd",
                "sceDNAS2EncryptDataLength_nohdd",
                "sceDNAS2EncryptData_nohdd",
                "sceDNAS2DecryptDataLength_nohdd",
                "sceDNAS2DecryptData_nohdd",
            ],
        ];
        let [init, plen, pdata, elen, edata] = NAMES
            .iter()
            .find_map(|names| {
                let a: Vec<u32> = names.iter().filter_map(|n| m.export(n)).collect();
                <[u32; 5]>::try_from(a).ok()
            })
            .ok_or(Error::Util("no sceDNAS2 data functions"))?;
        // LoadUtilRel: once the module is in
        m.call(init, &[])?;
        let size = aut.len() as u32;
        let cap = size + (size >> 1);
        let buf = m.alloc(128, cap);
        m.write(buf, aut);
        let len = m.alloc(16, 16);
        let r = m.call(plen, &[size, buf, len])? as i32;
        if r < 0 {
            return Err(Error::Dnas("DNAS Encrypt Length", r));
        }
        let n = m.mem.r32(len).unwrap();
        let r = m.call(pdata, &[size, n, buf])? as i32;
        if r < 0 {
            return Err(Error::Dnas("DNAS Encrypt", r));
        }
        let r2 = m.call(elen, &[r as u32, buf, len])? as i32;
        if r2 < 0 {
            return Err(Error::Dnas("DNAS Decrypt Length", r2));
        }
        let n = m.mem.r32(len).unwrap();
        let r = m.call(edata, &[r as u32, n, buf])? as i32;
        if r < 0 {
            return Err(Error::Dnas("DNAS Decrypt", r));
        }
        // unpack_buffer: a length, then zlib
        let out = m.mem.r32(buf).unwrap() as usize;
        let stream = m.read(buf + 4, (cap - 4) as usize);
        let mut module = Vec::with_capacity(out);
        use std::io::Read;
        flate2::read::ZlibDecoder::new(&stream[..]).read_to_end(&mut module).map_err(|e| Error::Inflate(e.to_string()))?;
        if module.len() != out {
            return Err(Error::Inflate(format!("{} bytes where the header says {out}", module.len())));
        }
        Ok(module)
    }
}
