//! VISoundDevice against the decomp: eqoa-port's `build/oracle_snd` runs the
//! decomp's own VISoundDevice, VIWave and VIXm (in the EE's layout) over its
//! SNDDRV, and every call the game makes is made to it and to `snd::device`
//! over `snd::Driver`. The results, every packet the device ships (to the
//! byte, stale words included) and the PCM are compared.
//!
//! The float model is `Ieee`, the oracle's: the device's maths are products
//! and sums with no fused operations, then a truncating conversion.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port) or the disc.

mod common;
use common::{Files, Oracle, disc, pcm};
use snd::device::{Link, SoundDevice};
use snd::Driver;
use vi::Ieee;

/// The Rust device's link: the Rust driver, recording what goes across.
struct Local {
    drv: Driver,
    sent: Vec<(u32, [u32; 128])>,
    pcm: Vec<i16>,
}

impl Link for Local {
    fn call(&mut self, fno: u32, packet: &[u32; 128]) -> [u32; 128] {
        self.sent.push((fno, *packet));
        self.drv.call(fno, packet, &mut self.pcm)
    }
    fn dma(&mut self, addr: u32, data: &[u8]) {
        self.drv.io.dma(addr, data);
    }
}

struct Pair {
    o: Oracle,
    dev: SoundDevice<Ieee>,
    local: Local,
    ops: usize,
    samples: usize,
    loud: usize,
    packets: usize,
    /// a planted difference: one word of the first packet from the 300th op on
    planted: bool,
}

impl Pair {
    /// One op on the oracle (910) against the same on the rewrite.
    fn ee(&mut self, what: &str, payload: Vec<u8>, ours: impl FnOnce(&mut SoundDevice<Ieee>, &mut Local) -> i32) -> i32 {
        if std::env::var("SND_TRACE").is_ok() {
            eprintln!("op {} {what}", self.ops + 1);
        }
        let r = self.o.call(910, &payload);
        let want = i32::from_le_bytes(r[..4].try_into().unwrap());
        let n = u32::from_le_bytes(r[4..8].try_into().unwrap()) as usize;
        let mut packets = Vec::new();
        for k in 0..n {
            let b = &r[8 + k * 516..8 + (k + 1) * 516];
            let fno = u32::from_le_bytes(b[..4].try_into().unwrap());
            let w: Vec<u32> = b[4..].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
            packets.push((fno, w));
        }
        let (want_pcm, _) = pcm(&r[8 + n * 516..]);
        self.local.sent.clear();
        self.local.pcm.clear();
        let got = ours(&mut self.dev, &mut self.local);
        self.ops += 1;
        if self.planted && self.ops >= 300 && !self.local.sent.is_empty() {
            self.local.sent[0].1[1] ^= 1;
            self.planted = false;
        }
        assert_eq!(packets.len(), self.local.sent.len(), "{what} (op {}): packets sent", self.ops);
        for (k, ((fw, ww), (fg, wg))) in packets.iter().zip(&self.local.sent).enumerate() {
            assert_eq!(fw, fg, "{what}: packet {k} fno");
            if ww[..] != wg[..] {
                let at = (0..128).find(|&i| ww[i] != wg[i]).unwrap();
                panic!("{what} (op {}): packet {k} word {at}: oracle {:?}\n ours {:?}", self.ops, &ww[..at + 8], &wg[..at + 8]);
            }
        }
        self.packets += packets.len();
        self.compare(what, &want_pcm);
        assert_eq!(want, got, "{what} (op {}): result", self.ops);
        got
    }

    fn compare(&mut self, what: &str, want: &[i16]) {
        let got = std::mem::take(&mut self.local.pcm);
        assert_eq!(want.len(), got.len(), "{what}: PCM length");
        if let Some(k) = (0..want.len()).find(|&k| want[k] != got[k]) {
            panic!("{what}: PCM differs at sample {}: oracle {} ours {}", self.samples + k / 2, want[k], got[k]);
        }
        self.loud += want.iter().filter(|&&s| s != 0).count();
        self.samples += want.len() / 2;
    }

    fn call(&mut self, what: &str, m: i32, a: &[i32], f: &[f32], s: &str) -> i32 {
        let mut p = 3u32.to_le_bytes().to_vec();
        p.extend(m.to_le_bytes());
        let mut ai = [0i32; 6];
        ai[..a.len()].copy_from_slice(a);
        let mut fi = [0f32; 2];
        fi[..f.len()].copy_from_slice(f);
        for x in ai {
            p.extend(x.to_le_bytes());
        }
        for x in fi {
            p.extend(x.to_le_bytes());
        }
        p.extend((s.len() as u32).to_le_bytes());
        p.extend(s.as_bytes());
        let s = s.to_string();
        self.ee(what, p, move |d, l| match m {
            1 => d.play(ai[0], ai[1], ai[2]),
            2 => {
                d.stop(ai[0]);
                0
            }
            3 => {
                d.set_pitch(ai[0], ai[1]);
                0
            }
            4 => {
                d.set_volume(ai[0], fi[0]);
                0
            }
            5 => {
                d.set_pan(ai[0], fi[0]);
                0
            }
            6 => d.is_playing(ai[0]) as i32,
            7 => d.play_bgm(ai[0] as usize, &s),
            8 => {
                d.stop_bgm(ai[0] as usize);
                0
            }
            9 => {
                d.stop_all_bgm();
                0
            }
            10 => {
                d.set_bgm_volume(ai[0] as usize, fi[0]);
                0
            }
            11 => {
                d.set_sound_class_volume(ai[0], fi[0]);
                0
            }
            12 => {
                d.set_output_mode(ai[0]);
                0
            }
            13 => d.flush(l),
            14 => d.process(l),
            15 => d.release_sound(ai[0]),
            16 => d.reset(l),
            17 => d.pitch(ai[0]),
            18 => d.volume(ai[0]).to_bits() as i32,
            19 => d.pan(ai[0]).to_bits() as i32,
            20 => d.close(l),
            21 => d.is_bgm_playing(ai[0] as usize) as i32,
            22 => {
                d.set_repeat(ai[0], ai[1]);
                0
            }
            23 => d.share_sound(ai[0]),
            24 => d.channels() as u32 as i32,
            26 => (d.channels() >> 32) as i32,
            25 => d.frame,
            _ => -1,
        })
    }

    /// A game frame: Process, then 1,600 samples (30 fps).
    fn frame(&mut self, what: &str) {
        self.call(what, 14, &[], &[], "");
        self.call(what, 24, &[], &[], "");
        self.call(what, 26, &[], &[], "");
        let r = self.o.call(903, &1600u32.to_le_bytes());
        let (want, _) = pcm(&r);
        self.local.drv.run(1600, &mut self.local.pcm);
        self.compare(what, &want);
    }

    fn frames(&mut self, what: &str, n: usize) {
        for _ in 0..n {
            self.frame(what);
        }
    }

    fn wave(&mut self, w: &esf::sound::Wave) -> i32 {
        let mut p = 1u32.to_le_bytes().to_vec();
        for x in [w.channels, w.blocks, w.rate] {
            p.extend(x.to_le_bytes());
        }
        p.extend(w.volume.to_le_bytes());
        p.extend(w.pan.to_le_bytes());
        p.extend(w.looped.to_le_bytes());
        p.extend((w.adpcm.len() as u32).to_le_bytes());
        p.extend(&w.adpcm);
        let w = w.clone();
        self.ee("wave", p, move |d, _| d.create_wave(w.channels, w.blocks, w.rate, w.volume, w.pan, w.looped, &w.adpcm))
    }

    fn xm(&mut self, x: &esf::sound::Xm) -> i32 {
        let mut p = 2u32.to_le_bytes().to_vec();
        p.extend(x.volume.to_le_bytes());
        p.extend(x.pan.to_le_bytes());
        p.extend((x.module.len() as u32).to_le_bytes());
        p.extend(&x.module);
        p.extend((x.samples.len() as u32).to_le_bytes());
        p.extend(&x.samples);
        let x = x.clone();
        self.ee("xm", p, move |d, _| d.create_xm(x.volume, x.pan, x.module, x.samples))
    }
}

fn scenario(planted: bool) -> Option<Pair> {
    let mut o = Oracle::open()?;
    let mut disc = disc()?;
    let ui = esf::Container::open(disc.read("DATA2/UI.CSF").unwrap()).unwrap();
    let (waves, _) = esf::sound::all(&ui);
    let ch = esf::Container::open(disc.read("DATA/CHAR.ESF").unwrap()).unwrap();
    let (char_waves, _) = esf::sound::all(&ch);
    let amb = esf::Container::open(disc.read("DATA/AMBTRACK.ESF").unwrap()).unwrap();
    let (_, xms) = esf::sound::all(&amb);
    let bgms = ["BGM/ERUDITES.BGM", "BGM/GNOMES.BGM", "BGM/OGRE.BGM"];
    let files: Vec<(String, Vec<u8>)> = bgms.iter().map(|p| (format!("\\{};1", p.replace('/', "\\")), disc.read(p).unwrap())).collect();
    let mut init = (files.len() as u32).to_le_bytes().to_vec();
    for (name, data) in &files {
        init.extend((name.len() as u32).to_le_bytes());
        init.extend(name.as_bytes());
        init.extend((data.len() as u32).to_le_bytes());
        init.extend(data);
    }
    o.call(900, &init);
    // the buffers' addresses, off an empty call, as in oracle_snd.rs
    let mut payload = 0x80f0u32.to_le_bytes().to_vec();
    payload.extend([0u8; 512]);
    let r = o.call(901, &payload);
    let st = |k: usize| u32::from_le_bytes(r[k * 4..k * 4 + 4].try_into().unwrap());
    let mut drv = Driver::with_addresses(Box::new(Files(files)), st(0), st(2));
    drv.call(0x80f0, &[0; 128], &mut Vec::new());
    let mut p = Pair { o, dev: SoundDevice::new(1), local: Local { drv, sent: Vec::new(), pcm: Vec::new() }, ops: 0, samples: 0, loud: 0, packets: 0, planted };

    let mut init = 0u32.to_le_bytes().to_vec();
    init.extend(1i32.to_le_bytes());
    p.ee("init", init, |d, l| d.init(l));
    p.frames("idle", 3);

    // ---- waves, uploaded a block a frame; played as VIUI plays its clicks ------
    let mut w = Vec::new();
    for wv in waves.iter().take(5) {
        w.push(p.wave(wv));
    }
    for wv in char_waves.iter().take(6) {
        w.push(p.wave(wv));
    }
    // a stereo wave, made of two of them as its halves
    let mut st = char_waves[6].clone();
    let other = &char_waves[7];
    let blocks = st.blocks.min(other.blocks);
    let mut data = st.adpcm[..blocks as usize * 16].to_vec();
    data.extend(&other.adpcm[..blocks as usize * 16]);
    st.channels = 2;
    st.blocks = blocks;
    st.adpcm = data;
    st.pan = 0.25;
    w.push(p.wave(&st));
    assert!(w.iter().all(|&i| i >= 0));
    let h = p.call("play while uploading", 1, &[w[0], 127, 0], &[], "");
    assert_eq!(h, -2);
    p.frames("uploading", 30);
    let h0 = p.call("ui click", 1, &[w[0], 127, 0], &[], "");
    p.call("ui volume", 4, &[h0], &[0.35], "");
    p.frames("click", 10);
    let h1 = p.call("play 1", 1, &[w[1], 10, 0], &[], "");
    let h2 = p.call("play stereo", 1, &[*w.last().unwrap(), 5, 0], &[], "");
    p.call("pan", 5, &[h1], &[-0.6], "");
    p.call("pitch", 3, &[h1, 5000], &[], "");
    p.frames("two", 15);
    p.call("volume of a stopped one", 4, &[h2], &[1.5], "");
    p.call("pan over", 5, &[h2], &[3.0], "");
    p.call("class volume", 11, &[0], &[0.5], "");
    p.call("mono", 12, &[0], &[], "");
    for k in 0..w.len() {
        p.call("each", 1, &[w[k], k as i32, 0], &[], "");
        p.frame("each");
    }
    p.call("stereo again", 12, &[1], &[], "");
    // priorities: more than 46 voices asked for, from low to high
    for k in 0..50 {
        p.call("crowd", 1, &[w[5 + k % 6], k as i32 % 7, 0], &[], "");
    }
    p.frames("crowd", 20);
    p.call("is playing", 6, &[h0], &[], "");
    p.call("pitch of", 17, &[h1], &[], "");
    p.call("volume of", 18, &[h1], &[], "");
    p.call("pan of", 19, &[h1], &[], "");
    p.call("stop", 2, &[h1], &[], "");
    p.frames("after the crowd", 40);

    // ---- XM: two modules (version 1 gives each 0xc000 of the pool) -------------
    let x0 = p.xm(&xms[0]);
    let x1 = p.xm(&xms[3]);
    // (a third would not fit the 0x19000 pool, and that failure is undefined
    // on the console: VIXm::Destroy frees, then the parser releases sound -1)
    assert!(x0 >= 0 && x1 >= 0, "two modules fit");
    p.call("flush", 13, &[], &[], "");
    p.call("repeat", 22, &[x0, 1], &[], "");
    let hx0 = p.call("xm 0", 1, &[x0, 20, 1], &[], "");
    let hx1 = p.call("xm 1", 1, &[x1, 20, 1], &[], "");
    p.frames("xm", 60);
    p.call("xm volume", 4, &[hx0], &[0.5], "");
    p.call("xm pan", 5, &[hx1], &[-1.0], "");
    p.call("xm pitch", 3, &[hx1, 3000], &[], "");
    p.call("music class", 11, &[1], &[0.7], "");
    p.frames("xm changed", 90);
    p.call("xm stop", 2, &[hx1], &[], "");
    p.frames("xm 0", 30);

    // ---- BGM: a request, the second slot, a cross-fade, a stop -------------------
    p.call("bgm", 7, &[0], &[], "bgm\\erudites.bgm");
    p.call("bgm volume", 10, &[0], &[0.8], "");
    p.frames("bgm", 90);
    p.call("bgm slot 1", 7, &[1], &[], "BGM\\GNOMES.BGM");
    p.frames("slot 0 wins", 20);
    p.call("stop slot 0", 8, &[0], &[], "");
    p.frames("cross-fade", 120);
    p.call("is bgm", 21, &[0], &[], "");
    p.call("bgm 0 again", 7, &[0], &[], "bgm/ogre.bgm");
    p.frames("third", 60);
    p.call("stop all", 9, &[], &[], "");
    p.frames("silence", 20);

    // ---- releases and a reset ----------------------------------------------------
    p.call("share", 23, &[w[2]], &[], "");
    p.call("release shared", 15, &[w[2]], &[], "");
    p.call("release", 15, &[w[2]], &[], "");
    p.call("release xm", 15, &[x0], &[], "");
    p.frames("released", 10);
    let x3 = p.xm(&xms[7]);
    assert!(x3 >= 0, "the pool block came back");
    p.frames("third xm uploads", 10);
    p.call("bgm before reset", 7, &[0], &[], "BGM\\ERUDITES.BGM");
    p.frames("bgm before reset", 30);
    p.call("reset", 16, &[], &[], "");
    p.frames("reset", 30);
    p.call("close", 20, &[], &[], "");
    p.frames("closed", 5);
    Some(p)
}

#[test]
fn visounddevice_matches_the_decomp() {
    let Some(p) = scenario(false) else { return };
    eprintln!("{} ops, {} packets, {} samples equal ({} channel samples not silent)", p.ops, p.packets, p.samples, p.loud);
    assert!(p.loud > p.samples / 2);
}

#[test]
fn a_planted_difference_fails() {
    match std::panic::catch_unwind(|| scenario(true)) {
        Ok(None) => {}
        Ok(Some(_)) => panic!("a planted difference passed"),
        Err(_) => {}
    }
}
