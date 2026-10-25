//! The mip levels off the disc (`Surface::decode_levels`): every level the
//! game keeps is in the record whole, its sides halving, and each is the art's
//! own smaller copy of the level above it (its texels near the 2 x 2 means
//! there, much nearer than to the level above turned upside down), which a
//! level read from the wrong place is not. The control: the same test the
//! other way round, upside down nearer, which hardly any surface passes.
//!
//! Inputs: `tests/common/mod.rs` (the disc only).

mod common;

/// The mean of |a - b| over the colour channels, per texel, between
/// `small` and the 2 x 2 means of `big` (twice its sides).
fn off_box(big: &esf::Image, small: &esf::Image, flip: bool) -> f64 {
    let (w, h) = (small.width as usize, small.height as usize);
    let bw = big.width as usize;
    let mut sum = 0.0;
    for y in 0..h {
        for x in 0..w {
            let by = if flip { (h - 1 - y) * 2 } else { y * 2 };
            for c in 0..3 {
                let at = |xx: usize, yy: usize| big.rgba[(yy * bw + xx) * 4 + c] as f64;
                let mean = (at(x * 2, by) + at(x * 2 + 1, by) + at(x * 2, by + 1) + at(x * 2 + 1, by + 1)) / 4.0;
                sum += (mean - small.rgba[(y * w + x) * 4 + c] as f64).abs();
            }
        }
    }
    sum / (w * h * 3) as f64
}

#[test]
fn the_disc_carries_every_level_the_game_keeps() {
    let Some(path) = std::env::var_os("EQOA_ISO")
        .map(std::path::PathBuf::from)
        .or_else(|| Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso")))
        .filter(|p| p.exists())
    else {
        eprintln!("SKIP: no disc (set EQOA_ISO)");
        return;
    };
    let mut disc = iso::Iso::open(path).unwrap();
    for name in ["DATA/CHAR.ESF", "DATA/ITEM.ESF", "DATA2/LAVASTM.ESF"] {
        let c = common::container(&mut disc, name);
        let (mut surfaces, mut deep, mut telling, mut near, mut flipped_near) = (0, 0, 0, 0, 0);
        for s in &c.surfaces {
            let levels = s.decode_levels(&c.data);
            if levels.is_empty() {
                continue;
            }
            surfaces += 1;
            assert_eq!(levels.len(), s.levels(), "{name} {:08x}: every kept level is whole", s.dict_id);
            for (k, l) in levels.iter().enumerate() {
                assert_eq!((l.width, l.height), ((s.width as u32 >> k).max(1), (s.height as u32 >> k).max(1)), "{name} {:08x} level {k}", s.dict_id);
            }
            if levels.len() > 1 {
                deep += 1;
                // nearer its own 2 x 2 means than half its distance from the
                // upside-down ones, and the other way round for the control;
                // of the surfaces upside down tells apart (not one colour,
                // not a mirror of itself top to bottom)
                let (own, flipped) = (off_box(&levels[0], &levels[1], false), off_box(&levels[0], &levels[1], true));
                if own.max(flipped) >= 2.0 {
                    telling += 1;
                    near += (own * 2.0 < flipped) as usize;
                    flipped_near += (flipped * 2.0 < own) as usize;
                }
            }
        }
        eprintln!("{name}: {surfaces} surfaces, {deep} with levels, {telling} told apart upside down; level 1 nearer the base's 2 x 2 means: {near}, nearer them upside down: {flipped_near}");
        assert!(deep * 10 > surfaces * 8, "{name}: most surfaces keep more than their base");
        assert!(telling * 2 > deep, "{name}: most are told apart upside down");
        assert!(near * 100 >= telling * 98, "{name}: level 1 is the base made smaller ({near} of {telling})");
        assert!(flipped_near * 100 < telling, "{name}: the control, upside down, is not ({flipped_near} of {telling})");
    }
}
