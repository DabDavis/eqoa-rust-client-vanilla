//! SNDDRV and the SPU2 against the decomp, sample for sample: eqoa-port's
//! `build/oracle_snd` runs the decomp's SNDDRV over eqoa-port's SPU2 on a
//! deterministic IOP, and every call here is made to it and to this crate,
//! and the status blocks and the PCM compared.
//!
//! The game's own data drives it: UI.CSF's waves, AMBTRACK.ESF's XM modules
//! and two BGM streams, through every command the EE sends.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port) or the disc.

mod common;
use common::{Files, Oracle, disc, pcm};
use snd::Driver;

/// The two sides, driven together.
struct Pair {
    o: Oracle,
    r: Driver,
    /// samples compared so far
    samples: usize,
    /// of which not silent
    loud: usize,
    planted: Option<fn(&mut Vec<i16>)>,
}

impl Pair {
    fn compare(&mut self, what: &str, want: &[i16], mut got: Vec<i16>) {
        if let Some(f) = self.planted {
            f(&mut got);
        }
        assert_eq!(want.len(), got.len(), "{what}: PCM length at sample {}", self.samples);
        if let Some(k) = (0..want.len()).find(|&k| want[k] != got[k]) {
            panic!(
                "{what}: PCM differs at sample {} ({} channel): oracle {} ours {}",
                self.samples + k / 2,
                ["left", "right"][k % 2],
                want[k],
                got[k]
            );
        }
        self.loud += want.iter().filter(|&&s| s != 0).count();
        self.samples += want.len() / 2;
    }

    fn rpc(&mut self, what: &str, cmds: &[(u32, Vec<u32>)]) -> [u32; 128] {
        let mut p = [0u32; 128];
        let mut w = 1;
        p[0] = cmds.len() as u32;
        for (id, args) in cmds {
            p[w] = *id;
            p[w + 1] = args.len() as u32;
            p[w + 2..w + 2 + args.len()].copy_from_slice(args);
            w += 2 + args.len();
        }
        let mut payload = 0x80f0u32.to_le_bytes().to_vec();
        for x in p {
            payload.extend(x.to_le_bytes());
        }
        let r = self.o.call(901, &payload);
        let want: Vec<u32> = r[..512].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        let (want_pcm, _) = pcm(&r[512..]);
        let mut got_pcm = Vec::new();
        let got = self.r.call(0x80f0, &p, &mut got_pcm);
        self.compare(what, &want_pcm, got_pcm);
        assert_eq!(&want[..], &got[..], "{what}: status block");
        got
    }

    fn dma(&mut self, addr: u32, data: &[u8]) {
        let mut payload = addr.to_le_bytes().to_vec();
        payload.extend(data);
        self.o.call(902, &payload);
        self.r.io.dma(addr, data);
    }

    fn run(&mut self, what: &str, frames: u32) {
        let r = self.o.call(903, &frames.to_le_bytes());
        let (want, _) = pcm(&r);
        let mut got = Vec::new();
        self.r.run(frames as u64, &mut got);
        self.compare(what, &want, got);
    }

    /// ADPCM to SPU RAM as VISoundDevice sends it: through the IOP buffer, 64
    /// KB a time, each block followed by command 1.
    fn upload(&mut self, iop_buf: u32, mut spu: u32, data: &[u8]) {
        for chunk in data.chunks(0x10000) {
            let n = (chunk.len() + 63) & !63;
            let mut c = chunk.to_vec();
            c.resize(n, 0);
            self.dma(iop_buf, &c);
            self.rpc("adpcm transfer", &[(1, vec![n as u32, spu])]);
            spu += n as u32;
        }
    }
}

fn bgm_args(name: &str, spu: u32) -> Vec<u32> {
    let mut b = name.as_bytes().to_vec();
    b.resize(32, 0);
    let mut a: Vec<u32> = b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    a.push(spu);
    a
}

fn mask(first: u32, n: u32) -> (u32, u32) {
    let m: u64 = ((1u64 << n) - 1) << first;
    ((m & 0xffffff) as u32, (m >> 24) as u32)
}

fn scenario(planted: Option<fn(&mut Vec<i16>)>) -> Option<Pair> {
    let mut o = Oracle::open()?;
    let mut disc = disc()?;
    let ui = esf::Container::open(disc.read("DATA2/UI.CSF").unwrap()).unwrap();
    let (waves, _) = esf::sound::all(&ui);
    let amb = esf::Container::open(disc.read("DATA/AMBTRACK.ESF").unwrap()).unwrap();
    let (_, xms) = esf::sound::all(&amb);
    let bgms = ["BGM/ERUDITES.BGM", "BGM/GNOMES.BGM"];
    let files: Vec<(String, Vec<u8>)> = bgms.iter().map(|p| (format!("\\{};1", p.replace('/', "\\")), disc.read(p).unwrap())).collect();

    let mut init = (files.len() as u32).to_le_bytes().to_vec();
    for (name, data) in &files {
        init.extend((name.len() as u32).to_le_bytes());
        init.extend(name.as_bytes());
        init.extend((data.len() as u32).to_le_bytes());
        init.extend(data);
    }
    o.call(900, &init);

    // an empty call first: its status names the two buffers' IOP addresses,
    // which the rewrite takes as its own
    let mut payload = 0x80f0u32.to_le_bytes().to_vec();
    payload.extend([0u8; 512]);
    let r = o.call(901, &payload);
    let st = |k: usize| u32::from_le_bytes(r[k * 4..k * 4 + 4].try_into().unwrap());
    let (iop_buf, pool) = (st(0), st(2));
    let mut r = Driver::with_addresses(Box::new(Files(files)), iop_buf, pool);
    r.call(0x80f0, &[0; 128], &mut Vec::new());
    let mut p = Pair { o, r, samples: 0, loud: 0, planted };

    p.rpc("init", &[(0, vec![])]);
    p.run("after init", 1000);

    // ---- waves: UI.CSF's, mono and a stereo pair, on both cores ----------------
    let mut spu = 0x5040 + 0x40000;
    let mut at = Vec::new();
    for w in waves.iter().take(6) {
        p.upload(iop_buf, spu, &w.adpcm);
        at.push(spu);
        spu += ((w.adpcm.len() as u32) + 63) & !63;
    }
    let pitch = |w: &esf::sound::Wave| (((w.rate as i64) << 12) / 48000) as u32;
    p.rpc("play 0", &[(2, vec![0, at[0], (-1i32) as u32, 0, pitch(&waves[0]), 0x3fff, 0])]);
    p.run("wave 0", 4000);
    p.rpc("play 1, 2", &[(2, vec![1, at[1], (-1i32) as u32, 0, pitch(&waves[1]), 0x2000, 0x1000]), (2, vec![30, at[2], (-1i32) as u32, 0, pitch(&waves[2]) * 3 / 2, 0x3fff, (-0x2000i32) as u32])]);
    p.run("waves 1, 2", 3333);
    p.rpc("pitch, volume", &[(5, vec![1, (-1i32) as u32, 0x800]), (6, vec![30, (-1i32) as u32, 0x1800, 0x3000])]);
    p.run("waves 1, 2 bent", 5000);
    // a stereo pair: two waves as its halves
    p.rpc("stereo", &[(2, vec![3, at[3], 4, at[4], pitch(&waves[3]), 0x3fff, 0x800])]);
    p.run("stereo", 6000);
    p.rpc("stop some", &[(4, vec![0b1010, 1 << 6])]);
    p.run("after stop", 20000);

    // ---- XM: two AMBTRACK modules at once (the pool holds two), one without
    // repeat; then a third in the first one's place
    let base = (pool + 63) & !63;
    let load = |p: &mut Pair, x: &esf::sound::Xm, a: u32, spu: u32| {
        let mut m = x.module.clone();
        m.resize((m.len() + 63) & !63, 0);
        assert!(a + m.len() as u32 <= pool + 0x19000, "the XM pool is 0x19000 bytes");
        p.dma(a, &m);
        p.upload(iop_buf, spu, &x.samples);
        p.rpc("xm loaded", &[(7, vec![a, spu])]);
    };
    let xm0 = (base, spu);
    load(&mut p, &xms[0], xm0.0, xm0.1);
    spu += ((xms[0].samples.len() as u32) + 63) & !63;
    let xm1 = (base + (((xms[0].module.len() as u32) + 63) & !63), spu);
    load(&mut p, &xms[1], xm1.0, xm1.1);
    spu += ((xms[1].samples.len() as u32) + 63) & !63;
    let chans = |x: &esf::sound::Xm| i16::from_le_bytes([x.module[8], x.module[9]]) as u32;
    let m0 = mask(8, chans(&xms[0]));
    let m1 = mask(30, chans(&xms[1]));
    p.rpc("xm play", &[(8, vec![m0.0, m0.1, xm0.0, 1, 4096, 255, 128]), (8, vec![m1.0, m1.1, xm1.0, 0, 4096, 200, 60])]);
    p.run("xm 0, 1", 48000 * 4);
    p.rpc("xm volume, pitch", &[(10, vec![m0.0, m0.1, 128, 255]), (11, vec![m1.0, m1.1, 3500])]);
    p.run("xm changed", 48000 * 6);
    p.rpc("xm stop 0", &[(9, vec![m0.0, m0.1]), (4, vec![m0.0, m0.1])]);
    p.run("xm 1", 48000 * 2);
    // a smaller module where the first one was, its samples after the rest
    load(&mut p, &xms[7], xm0.0, spu);
    let m2 = mask(0, chans(&xms[7]));
    p.rpc("xm play 2", &[(8, vec![m2.0, m2.1, xm0.0, 1, 5000, 180, 200])]);
    p.run("xm 1, 2", 48000 * 8);

    // ---- BGM: play, its volume, another over it, stop, free and init ------------
    let bgm = 0x5040;
    p.rpc("bgm play", &[(13, bgm_args(bgms[0], bgm)), (15, vec![0x3fff])]);
    p.run("bgm", 48000 * 5);
    p.rpc("bgm volume", &[(15, vec![0x1800])]);
    p.run("bgm quieter", 48000 * 2);
    p.rpc("bgm over bgm", &[(13, bgm_args(bgms[1], bgm)), (15, vec![0x3000])]);
    p.run("second bgm", 48000 * 4);
    p.rpc("bgm stop", &[(14, vec![0])]);
    p.run("bgm stopped", 48000);
    p.rpc("bgm again", &[(13, bgm_args(bgms[0], bgm)), (15, vec![0x3fff])]);
    p.run("bgm again", 48000 * 3);
    p.rpc("missing bgm", &[(13, bgm_args("BGM/NOPE.BGM", bgm))]);
    p.run("after a missing bgm", 4800);
    p.rpc("bgm, then free", &[(13, bgm_args(bgms[1], bgm)), (15, vec![0x3fff])]);
    p.run("bgm before free", 48000 * 2);
    p.rpc("free", &[(12, vec![])]);
    p.run("freed", 4800);
    p.rpc("init again", &[(0, vec![])]);
    p.rpc("bgm after init", &[(13, bgm_args(bgms[0], bgm)), (15, vec![0x3fff])]);
    p.run("bgm after init", 48000 * 2);
    Some(p)
}

#[test]
fn snddrv_matches_the_decomp() {
    let Some(p) = scenario(None) else { return };
    eprintln!("{} samples equal, {} channel samples not silent", p.samples, p.loud);
    assert!(p.loud > p.samples, "the scenario should be mostly sound, not silence");
}

/// A planted bug: one sample off by one in the middle of the BGM.
#[test]
fn a_planted_difference_fails() {
    let r = std::panic::catch_unwind(|| {
        scenario(Some(|v: &mut Vec<i16>| {
            if v.len() > 48000 * 2 * 4 {
                v[48000 * 2 * 3 + 1] ^= 1;
            }
        }))
    });
    match r {
        Ok(None) => {}
        Ok(Some(_)) => panic!("a planted difference passed"),
        Err(_) => {}
    }
}
