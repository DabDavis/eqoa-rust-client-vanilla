//! The newlib port against go-eqoa-vi/vimath, whose functions were checked
//! against the running console: 24,000 inputs from the regimes each function
//! branches on (`tools/vimath-corpus`, `go run . > crates/ee/tests/libm_corpus.bin`),
//! every answer compared bit for bit. Plus the console values the Go code
//! records as literals.
//!
//! ONE deliberate difference: `atan2f`'s wrapper tests its arguments with
//! `c.eq.s` (SUPPORT 0x0012711C, 0x0012712C), which on the EE flushes a
//! denormal operand first, so `atan2f(denormal, ±0)` takes the domain exit and
//! answers +0.0. The Go compares with a host `==` there; its `powf` port
//! measured the same flush in `powf`'s wrapper. Those records are checked
//! against the wrapper's answer instead, and counted.

use ee::libm;

#[test]
fn matches_console_checked_go() {
    let data = include_bytes!("libm_corpus.bin");
    let names = ["sinf", "cosf", "powf", "atanf", "atan2f", "acosf", "sqrtf", "fmodf"];
    let mut counts = [0usize; 8];
    let mut skipped = 0;
    let mut atan2_denormal_zero = 0;
    for (i, r) in data.chunks(14).enumerate() {
        let op = r[0] as usize;
        let u = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        let (a, b, want, ok) = (f32::from_bits(u(1)), f32::from_bits(u(5)), u(9), r[13] == 1);
        let got: Option<f32> = match op {
            0 => (!libm::huge_angle(a)).then(|| libm::sinf(a)),
            1 => (!libm::huge_angle(a)).then(|| libm::cosf(a)),
            2 => libm::powf(a, b),
            3 => Some(libm::atanf(a)),
            4 => Some(libm::atan2f(a, b)),
            5 => libm::acosf(a),
            6 => Some(libm::sqrtf(a)),
            _ => Some(libm::fmodf(a, b)),
        };
        if op == 4 && ee::eq(a, 0.0) && ee::eq(b, 0.0) && !(a == 0.0 && b == 0.0) {
            assert_eq!(got.unwrap().to_bits(), 0, "record {i}: atan2f of a denormal and a zero");
            atan2_denormal_zero += 1;
            continue;
        }
        match (got, ok) {
            (Some(g), true) => {
                assert_eq!(
                    g.to_bits(),
                    want,
                    "record {i}: {}({a:e} [{:08X}], {b:e} [{:08X}]) = {g:e} [{:08X}], Go {:e} [{want:08X}]",
                    names[op],
                    a.to_bits(),
                    b.to_bits(),
                    g.to_bits(),
                    f32::from_bits(want)
                );
                counts[op] += 1;
            }
            (None, false) => skipped += 1,
            (g, ok) => panic!("record {i}: {} answered {g:?}, Go's ok = {ok}", names[op]),
        }
    }
    let summary: Vec<String> = names.iter().zip(counts).map(|(n, c)| format!("{n} {c}")).collect();
    eprintln!(
        "equal to the Go: {} ({skipped} unported in both; {atan2_denormal_zero} atan2f(denormal, 0) answered +0 as c.eq.s does)",
        summary.join(", ")
    );
    assert!(counts.iter().all(|&c| c > 1000));
}

#[test]
fn atan2_of_signed_zeros_is_the_wrappers() {
    // Measured on the console (go-eqoa-vi/vimath/atan.go): the _XOPEN_ wrapper
    // answers +0.0 for every pair of zeros; the kernel answers by the signs.
    let z = 0.0f32;
    let nz = -0.0f32;
    for (y, x, kernel) in [(z, z, 0x0000_0000u32), (nz, nz, 0xC049_0FDA), (z, nz, 0x4049_0FDA), (nz, z, 0x8000_0000)] {
        assert_eq!(libm::atan2f(y, x).to_bits(), 0);
        assert_eq!(libm::atan2f_ieee(y, x).to_bits(), kernel);
    }
}

#[test]
fn pow_of_zero_to_a_negative_is_minus_huge_val() {
    // w_pow.c case 23 under _XOPEN_: the kernel saturates 1/0 to Fmax, the
    // wrapper answers -HUGE_VAL, the folded infinity's pattern.
    assert_eq!(libm::powf(0.0, -0.032).unwrap().to_bits(), 0xFF80_0000);
}
