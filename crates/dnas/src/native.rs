//! Native versions of the library's hottest functions.
//!
//! About half of the instructions an unpack runs are in a few functions that
//! are not DNAS's own: SHA-1's compression and RC6's block cipher (both
//! standard, as BSAFE and the library have them), libgcc's `__muldi3` and
//! three of its soft-float double routines, and BSAFE's bignum
//! multiply-accumulate. Here each is computed natively when the interpreter
//! reaches its entry, as it was called: arguments in `a0`.., the return in
//! `v0`, back to `ra`.
//!
//! A native stands in only for the code it was proved against: a function is
//! found by its export (or its place beside one) and then its bytes must have
//! the CRC-32 below, the checksum of that function in the Frontiers disc's
//! `UTIL.REL` relocated at `UTIL_BASE`, else the interpreter runs it. The proof
//! is `tests/natives.rs`: each native against the interpreter on the calls of
//! a whole unpack, registers and all of RAM, and the pure ones on random
//! arguments. A native may also decline a call (`None`), which the
//! interpreter then runs: the soft-float ones decline anything but normal
//! numbers. No key and no part of the library is here, only what the
//! functions compute.

use crate::cpu::{Cpu, Mem, A0, RA, SP, V0};

/// A native function: `None` declines the call.
pub(crate) type Run = fn(&mut Cpu, &mut Mem) -> Option<()>;

pub(crate) struct Native {
    pub name: &'static str,
    pub at: u32,
    /// the function's bytes, from `at`
    pub len: u32,
    pub run: Run,
    /// whether `v0` is its result (the others are void: their callers leave
    /// `v0` alone, so `tests/natives.rs` does not compare it)
    pub returns: bool,
}

/// How a native is found, the length of the function, its CRC-32, and the
/// native.
struct Known {
    name: &'static str,
    /// an export, and the function's offset from it
    export: &'static str,
    offset: u32,
    len: u32,
    crc: u32,
    run: Run,
    returns: bool,
}

// CBCInit's address plus this: BSAFE's CBC decrypt update, unexported,
// reached through the feedback cipher's table. (Its encrypt update is only
// ever handed DES here, which is interpreted, so it has no native.)
const CBC_DECRYPT: u32 = 0x2D0;

const KNOWN: &[Known] = &[
    Known { name: "SHA-1 compression", export: "__sce_dnas2_symbol16095", offset: 0x70, len: 0x3D8, crc: 0x5FA1_B246, run: sha1_compress, returns: false },
    Known { name: "A_RC6Encrypt_20", export: "A_RC6Encrypt_20", offset: 0, len: 0x880, crc: 0x4174_C558, run: rc6_encrypt, returns: false },
    Known { name: "A_RC6Decrypt_20", export: "A_RC6Decrypt_20", offset: 0, len: 0x870, crc: 0x1A4E_D271, run: rc6_decrypt, returns: false },
    Known { name: "__muldi3", export: "__muldi3", offset: 0, len: 0x60, crc: 0xE66C_3EEB, run: muldi3, returns: true },
    Known { name: "CMP_VectorMultiply", export: "CMP_VectorMultiply", offset: 0, len: 0x1E0, crc: 0x03B2_EA12, run: cmp_vector_multiply, returns: false },
    Known { name: "dpmul", export: "dpmul", offset: 0, len: 0x2A8, crc: 0x957C_943B, run: dpmul, returns: true },
    Known { name: "litodp", export: "litodp", offset: 0, len: 0xB8, crc: 0x5756_DBD3, run: litodp, returns: true },
    Known { name: "dptoul", export: "dptoul", offset: 0, len: 0xA0, crc: 0x2F8A_CD6F, run: dptoul, returns: true },
    Known { name: "CBC decrypt", export: "CBCInit", offset: CBC_DECRYPT, len: 0x220, crc: 0xDE35_6060, run: cbc_decrypt, returns: true },
    Known { name: "buffer read", export: "sceDNAS2InstExtractData_nohdd", offset: 0x1C08, len: 0x74, crc: 0x57B8_F3C0, run: buffer_read, returns: true },
];

/// The entry of native `name`, while it stands (`Mem::natives`).
fn entry(mem: &Mem, name: &str) -> Option<u32> {
    mem.natives.iter().find(|n| n.2 == name && n.0 != u32::MAX).map(|n| n.0)
}

/// The natives whose function is in `mem` as proved, by the exports' addresses.
pub(crate) fn find(mem: &Mem, export: impl Fn(&str) -> Option<u32>) -> Vec<Native> {
    KNOWN
        .iter()
        .filter_map(|k| {
            let at = export(k.export)?.checked_add(k.offset)?;
            let bytes = mem.slice(at, k.len as usize)?;
            (crc32fast::hash(bytes) == k.crc).then_some(Native { name: k.name, at, len: k.len, run: k.run, returns: k.returns })
        })
        .collect()
}

/// The CRC-32 a `KNOWN` entry is checked against, for each entry (what
/// `tests/natives.rs` prints when the table needs making again).
pub fn checksums(mem: &Mem, export: impl Fn(&str) -> Option<u32>) -> Vec<(&'static str, Option<u32>)> {
    KNOWN
        .iter()
        .map(|k| (k.name, export(k.export).and_then(|a| mem.slice(a + k.offset, k.len as usize)).map(crc32fast::hash)))
        .collect()
}

fn arg(cpu: &Cpu, k: usize) -> u64 {
    cpu.gpr[A0 + k]
}

fn words<const N: usize>(mem: &Mem, at: u32) -> Option<[u32; N]> {
    let mut w = [0u32; N];
    for (k, x) in w.iter_mut().enumerate() {
        *x = mem.r32(at.wrapping_add(4 * k as u32))?;
    }
    Some(w)
}

fn store(mem: &mut Mem, at: u32, w: &[u32]) -> Option<()> {
    for (k, &x) in w.iter().enumerate() {
        mem.w32(at.wrapping_add(4 * k as u32), x)?;
    }
    Some(())
}

/// `sha1_compress(w, state)`: the block's sixteen words (already in the
/// order the rounds read them) at `w`, rewritten in place as the message
/// schedule (each slot ends holding the last of W[16..80] that went there),
/// and the five words of `state` added to.
fn sha1_compress(cpu: &mut Cpu, mem: &mut Mem) -> Option<()> {
    let (w_at, s_at) = (arg(cpu, 0) as u32, arg(cpu, 1) as u32);
    let mut w: [u32; 16] = words(mem, w_at)?;
    let s: [u32; 5] = words(mem, s_at)?;
    let [mut a, mut b, mut c, mut d, mut e] = s;
    for t in 0..80 {
        let i = t & 15;
        if t >= 16 {
            w[i] = (w[(i + 13) & 15] ^ w[(i + 8) & 15] ^ w[(i + 2) & 15] ^ w[i]).rotate_left(1);
        }
        let (f, k) = match t / 20 {
            0 => ((b & c) | (!b & d), 0x5A82_7999),
            1 => (b ^ c ^ d, 0x6ED9_EBA1),
            2 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
            _ => (b ^ c ^ d, 0xCA62_C1D6),
        };
        let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
        (e, d, c, b, a) = (d, c, b.rotate_left(30), a, temp);
    }
    store(mem, w_at, &w)?;
    let out = [s[0].wrapping_add(a), s[1].wrapping_add(b), s[2].wrapping_add(c), s[3].wrapping_add(d), s[4].wrapping_add(e)];
    store(mem, s_at, &out)
}

/// RC6-32/20: `(ctx, out, in)`, the 44 round keys at `ctx + 12`, a block of
/// four little-endian words.
fn rc6_keys(mem: &Mem, ctx: u32) -> Option<[u32; 44]> {
    words(mem, ctx.wrapping_add(12))
}

fn rc6_encrypt(cpu: &mut Cpu, mem: &mut Mem) -> Option<()> {
    let (ctx, out, inp) = (arg(cpu, 0) as u32, arg(cpu, 1) as u32, arg(cpu, 2) as u32);
    let s = rc6_keys(mem, ctx)?;
    let block = rc6_encrypt_block(&s, words(mem, inp)?);
    store(mem, out, &block)
}

fn rc6_decrypt(cpu: &mut Cpu, mem: &mut Mem) -> Option<()> {
    let (ctx, out, inp) = (arg(cpu, 0) as u32, arg(cpu, 1) as u32, arg(cpu, 2) as u32);
    let s = rc6_keys(mem, ctx)?;
    let block = rc6_decrypt_block(&s, words(mem, inp)?);
    store(mem, out, &block)
}

fn rc6_encrypt_block(s: &[u32; 44], [mut a, mut b, mut c, mut d]: [u32; 4]) -> [u32; 4] {
    b = b.wrapping_add(s[0]);
    d = d.wrapping_add(s[1]);
    for i in 1..=20 {
        let t = b.wrapping_mul(b.wrapping_mul(2).wrapping_add(1)).rotate_left(5);
        let u = d.wrapping_mul(d.wrapping_mul(2).wrapping_add(1)).rotate_left(5);
        a = (a ^ t).rotate_left(u & 31).wrapping_add(s[2 * i]);
        c = (c ^ u).rotate_left(t & 31).wrapping_add(s[2 * i + 1]);
        (a, b, c, d) = (b, c, d, a);
    }
    a = a.wrapping_add(s[42]);
    c = c.wrapping_add(s[43]);
    [a, b, c, d]
}

fn rc6_decrypt_block(s: &[u32; 44], [mut a, mut b, mut c, mut d]: [u32; 4]) -> [u32; 4] {
    c = c.wrapping_sub(s[43]);
    a = a.wrapping_sub(s[42]);
    for i in (1..=20).rev() {
        (a, b, c, d) = (d, a, b, c);
        let u = d.wrapping_mul(d.wrapping_mul(2).wrapping_add(1)).rotate_left(5);
        let t = b.wrapping_mul(b.wrapping_mul(2).wrapping_add(1)).rotate_left(5);
        c = c.wrapping_sub(s[2 * i + 1]).rotate_right(t & 31) ^ u;
        a = a.wrapping_sub(s[2 * i]).rotate_right(u & 31) ^ t;
    }
    d = d.wrapping_sub(s[1]);
    b = b.wrapping_sub(s[0]);
    [a, b, c, d]
}

/// libgcc's `__muldi3`: the low 64 bits of the product.
fn muldi3(cpu: &mut Cpu, _: &mut Mem) -> Option<()> {
    cpu.gpr[V0] = arg(cpu, 0).wrapping_mul(arg(cpu, 1));
    Some(())
}

/// BSAFE's `CMP_VectorMultiply(m, src, src_at, n, dst, dst_at)`: `dst[dst_at..]
/// += m * src[src_at..src_at + n]` over 64-bit words (a CMPInt's words are at
/// the pointer in its +8), the carry run on past the `n` words until it stops.
fn cmp_vector_multiply(cpu: &mut Cpu, mem: &mut Mem) -> Option<()> {
    let m = arg(cpu, 0);
    let src = mem.r32(arg(cpu, 1) as u32 + 8)?.wrapping_add((arg(cpu, 2) as u32) << 3);
    let n = arg(cpu, 3) as u32 as i32;
    let dst = mem.r32(arg(cpu, 4) as u32 + 8)?.wrapping_add((arg(cpu, 5) as u32) << 3);
    let mut carry = 0u64;
    let mut i = 0u32;
    while (i as i32) < n {
        let at = dst.wrapping_add(i << 3);
        let p = m as u128 * mem.r64(src.wrapping_add(i << 3))? as u128 + mem.r64(at)? as u128 + carry as u128;
        mem.w64(at, p as u64)?;
        carry = (p >> 64) as u64;
        i += 1;
    }
    let at = dst.wrapping_add(i << 3);
    let d = mem.r64(at)?.wrapping_add(carry);
    mem.w64(at, d)?;
    if d < carry {
        loop {
            i += 1;
            let at = dst.wrapping_add(i << 3);
            let w = mem.r64(at)?;
            if w != u64::MAX {
                mem.w64(at, w.wrapping_add(1))?;
                break;
            }
            mem.w64(at, 0)?;
        }
    }
    Some(())
}

/// BSAFE's block-function dispatcher, which a cipher object names and which
/// calls its key's own function (the key's +8), `(key, out, in)`: its 32
/// bytes' CRC-32, as the Frontiers library has them.
const DISPATCH: (u32, u32) = (32, 0x1C09_F058);

/// RC6 on one block in place at `at` with the keys at `ctx`, as the native
/// whose entry `f` is (the block function a CBC update was handed), or
/// `None` if `f` is not an RC6 native.
fn rc6_block(mem: &mut Mem, f: u32, ctx: u32, at: u32) -> Option<()> {
    let s = rc6_keys(mem, ctx)?;
    let mut b: [u32; 4] = words(mem, at)?;
    if Some(f) == entry(mem, "A_RC6Encrypt_20") {
        b = rc6_encrypt_block(&s, b);
    } else if Some(f) == entry(mem, "A_RC6Decrypt_20") {
        b = rc6_decrypt_block(&s, b);
    } else {
        return None;
    }
    store(mem, at, &b)
}

/// BSAFE's CBC decrypt update, `(cbc, cipher, key, out, &outlen, in,
/// inlen)`: the CBC context's +4 the chaining block, +8 a scratch block,
/// +12 the block length; the cipher's +12 its block function, called `(key,
/// scratch, scratch)`. Whole blocks only; `*outlen` gets the bytes written,
/// `v0` 0. Declined, before anything is written, unless the block function is
/// the dispatcher and the key's own an RC6 native (16-byte blocks; DES's
/// 8-byte ones are interpreted).
fn cbc_decrypt(cpu: &mut Cpu, mem: &mut Mem) -> Option<()> {
    let (ctx, cipher, key, out, outlen) = (arg(cpu, 0) as u32, arg(cpu, 1) as u32, arg(cpu, 2) as u32, arg(cpu, 3) as u32, arg(cpu, 4) as u32);
    let (inp, inlen) = (arg(cpu, 5) as u32, cpu.gpr[A0 + 6] as u32);
    let chain = mem.r32(ctx + 4)?;
    let scratch = mem.r32(ctx + 8)?;
    let dispatch = mem.r32(cipher + 12)?;
    if mem.slice(dispatch, DISPATCH.0 as usize).map(crc32fast::hash) != Some(DISPATCH.1) {
        return None;
    }
    let f = mem.r32(key + 8)?;
    let rc6 = [entry(mem, "A_RC6Encrypt_20"), entry(mem, "A_RC6Decrypt_20")];
    if mem.r32(ctx + 12)? != 16 || !rc6.contains(&Some(f)) {
        return None;
    }
    let (mut at, mut left, mut done) = (0u32, inlen, 0u32);
    while left >= 16 {
        let block: [u32; 4] = words(mem, inp.wrapping_add(at))?;
        store(mem, scratch, &block)?;
        rc6_block(mem, f, key, scratch)?;
        let iv: [u32; 4] = words(mem, chain)?;
        let p: [u32; 4] = words(mem, scratch)?;
        store(mem, scratch, &[p[0] ^ iv[0], p[1] ^ iv[1], p[2] ^ iv[2], p[3] ^ iv[3]])?;
        // the chain takes the ciphertext as it is now in the input
        let c: [u32; 4] = words(mem, inp.wrapping_add(at))?;
        store(mem, chain, &c)?;
        let r: [u32; 4] = words(mem, scratch)?;
        store(mem, out.wrapping_add(at), &r)?;
        (at, left, done) = (at + 16, left - 16, done + 16);
    }
    mem.w32(outlen, done)?;
    cpu.gpr[V0] = 0;
    Some(())
}

/// Where the reader's source starts, how far in it is, and how long it is
/// (its three globals, as its code names them).
const READ_BASE: u32 = 0x0022_0CD0;
const READ_POS: u32 = 0x0022_0CD4;
const READ_LEN: u32 = 0x0022_0CD8;

/// The library's reader over a buffer, `read(dst, n)`: up to `n` bytes from
/// where it is, its place moved on, the count returned (the DNAS stream asks
/// for one byte at a time). The registers' 64-bit arithmetic is the code's.
fn buffer_read(cpu: &mut Cpu, mem: &mut Mem) -> Option<()> {
    let sx = |w: u32| w as i32 as i64 as u64;
    let (dst, n) = (arg(cpu, 0), arg(cpu, 1));
    let pos = mem.r32(READ_POS)?;
    let len = mem.r32(READ_LEN)?;
    let base = mem.r32(READ_BASE)?;
    let end = sx(pos.wrapping_add(n as u32));
    let count = if sx(len) < end { sx(len.wrapping_sub(pos)) } else { n };
    let src = base.wrapping_add(pos);
    let mut i = 0i64;
    while i < count as i64 {
        let b = mem.r8(src.wrapping_add(i as u32))?;
        mem.w8((dst as u32).wrapping_add(i as u32), b)?;
        i += 1;
    }
    mem.w32(READ_POS, pos.wrapping_add(count as u32))?;
    cpu.gpr[V0] = count;
    Some(())
}

/// Whether a double's bits are a normal number (not zero, a denormal, an
/// infinity or a NaN).
fn normal(bits: u64) -> bool {
    let e = (bits >> 52) & 0x7FF;
    e != 0 && e != 0x7FF
}

/// libgcc's fp-bit `dpmul` (`__muldf3`): IEEE round-to-nearest-even, which
/// is the host's. Declined unless both operands and the product are normal.
fn dpmul(cpu: &mut Cpu, _: &mut Mem) -> Option<()> {
    let (x, y) = (arg(cpu, 0), arg(cpu, 1));
    if !normal(x) || !normal(y) {
        return None;
    }
    let r = (f64::from_bits(x) * f64::from_bits(y)).to_bits();
    if !normal(r) {
        return None;
    }
    cpu.gpr[V0] = r;
    Some(())
}

/// fp-bit `litodp` (`__floatsidf`): a 32-bit int to a double, which is exact.
/// Declined for a register that is not a sign-extended word.
fn litodp(cpu: &mut Cpu, _: &mut Mem) -> Option<()> {
    let a = arg(cpu, 0);
    if a != a as u32 as i32 as i64 as u64 {
        return None;
    }
    cpu.gpr[V0] = (a as u32 as i32 as f64).to_bits();
    Some(())
}

/// fp-bit `dptoul` (`__fixunsdfsi`): toward zero to an unsigned word, 0 for
/// a NaN or a negative, all ones from 2^32 up and for +inf, the word then
/// sign-extended as the register holds it.
fn dptoul(cpu: &mut Cpu, _: &mut Mem) -> Option<()> {
    let x = f64::from_bits(arg(cpu, 0));
    let w: u32 = if x.is_nan() || x.is_sign_negative() {
        0
    } else if x >= 4_294_967_296.0 {
        u32::MAX
    } else {
        x as u32
    };
    cpu.gpr[V0] = w as i32 as i64 as u64;
    Some(())
}

/// The registers a call must leave as they were: `s0`-`s7`, `gp`, `sp`,
/// `fp` and `ra` (`tests/natives.rs` compares these, and `v0` for a native
/// that `returns`).
pub(crate) const SAVED: [usize; 12] = [16, 17, 18, 19, 20, 21, 22, 23, 28, SP, 30, RA];
