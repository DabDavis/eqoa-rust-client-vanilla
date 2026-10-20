//! The oracle harness, shared by the oracle tests: eqoa-port's `build/oracle`
//! over a pipe, byte views of the repr(C) structs, a seeded RNG, and the
//! player's SUPPORT image.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::mem::size_of;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use vi::fpu::Ieee;
use vi::vect::Vect3;

pub struct Oracle {
    _child: Child,
    tx: ChildStdin,
    rx: ChildStdout,
}

impl Oracle {
    pub fn open() -> Option<Oracle> {
        let path = std::env::var_os("EQOA_ORACLE").map(std::path::PathBuf::from).unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../eqoa-port/build/oracle")
        });
        Self::spawn(path)
    }

    /// Another of eqoa-port's oracle programs (`build/<name>`).
    pub fn open_at(name: &str) -> Option<Oracle> {
        Self::spawn(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../eqoa-port/build").join(name))
    }

    fn spawn(path: std::path::PathBuf) -> Option<Oracle> {
        if !path.exists() {
            eprintln!("SKIP: no oracle at {} (set EQOA_ORACLE, or `make oracle` in eqoa-port)", path.display());
            return None;
        }
        let mut child = Command::new(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().ok()?;
        let tx = child.stdin.take()?;
        let rx = child.stdout.take()?;
        Some(Oracle { _child: child, tx, rx })
    }

    pub fn call(&mut self, op: u32, payload: &[u8]) -> Vec<u8> {
        let mut msg = Vec::with_capacity(payload.len() + 8);
        msg.extend_from_slice(&op.to_le_bytes());
        msg.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        msg.extend_from_slice(payload);
        self.tx.write_all(&msg).unwrap();
        let mut len = [0u8; 4];
        self.rx.read_exact(&mut len).expect("oracle died");
        let mut out = vec![0u8; u32::from_le_bytes(len) as usize];
        self.rx.read_exact(&mut out).unwrap();
        out
    }

    /// Runs `op` on `obj` with `args`: (return value, object after, outputs).
    pub fn run<T: Copy>(&mut self, op: u32, obj: &T, args: &[f32]) -> (i32, T, Vec<f32>) {
        let mut p = bytes(obj).to_vec();
        for a in args {
            p.extend_from_slice(&a.to_le_bytes());
        }
        let r = self.call(op, &p);
        let ret = i32::from_le_bytes(r[0..4].try_into().unwrap());
        let n = size_of::<T>();
        let after = from_bytes::<T>(&r[4..4 + n]);
        let outs = r[4 + n..].chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        (ret, after, outs)
    }
}

pub fn bytes<T>(v: &T) -> &[u8] {
    // SAFETY: the structs are repr(C), padding-free and made of plain numbers.
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, size_of::<T>()) }
}

pub fn from_bytes<T: Copy>(b: &[u8]) -> T {
    assert!(b.len() >= size_of::<T>());
    // SAFETY: any bit pattern is a valid value of these structs.
    unsafe { std::ptr::read_unaligned(b.as_ptr() as *const T) }
}

/// The first differing 32-bit word, as (offset, ours, theirs).
pub fn first_diff<T>(a: &T, b: &T) -> Option<(usize, u32, u32)> {
    let (a, b) = (bytes(a), bytes(b));
    (0..a.len() / 4).find_map(|i| {
        let w = |s: &[u8]| u32::from_le_bytes(s[i * 4..i * 4 + 4].try_into().unwrap());
        (w(a) != w(b)).then(|| (i * 4, w(a), w(b)))
    })
}

/// xorshift64*, seeded: every run checks the same states.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    /// A value from the regimes the code branches on: zero, ±0, small, unit,
    /// angles past ±π, large.
    pub fn any(&mut self) -> f32 {
        match self.next() % 8 {
            0 => 0.0,
            1 => -0.0,
            2 => self.range(-0.01, 0.01),
            3 => self.range(-1.2, 1.2),
            4 => self.range(-7.0, 7.0),
            5 => self.range(-40.0, 40.0),
            _ => self.range(-3.0, 3.0),
        }
    }
    pub fn vec(&mut self) -> Vect3<Ieee> {
        Vect3::of(self.any(), self.any(), self.any())
    }
    pub fn flags(&mut self) -> u32 {
        // Mostly one or two bits, sometimes many.
        let mut f = 0;
        for _ in 0..(self.next() % 4) {
            f |= 1 << (self.next() % 14);
        }
        if self.next().is_multiple_of(6) {
            f |= (self.next() as u32) & 0x3FFF;
        }
        f
    }
}

/// SUPPORT's decrypted image: `run/support.bin`, else unpacked off the disc
/// (`run/eqoa.iso`, or `$EQOA_ISO`) by its own DNAS library (`dnas`, ~6 s),
/// else `None`.
pub fn support_image() -> Option<Vec<u8>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if let Ok(b) = std::fs::read(root.join("run/support.bin")) {
        return Some(b);
    }
    let iso = std::env::var_os("EQOA_ISO").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("run/eqoa.iso"));
    let mut disc = iso::Iso::open(iso).ok()?;
    dnas::unpack(&disc.read("UTIL.REL").ok()?, &disc.read("SUPPORT.AUT").ok()?).ok()
}
