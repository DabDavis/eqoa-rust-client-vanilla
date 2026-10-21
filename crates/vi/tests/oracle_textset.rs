//! The quick-chat tree against the decomp: eqoa-port's oracle (op 810) runs
//! CLIENT's own `VITextSet` (Reset, FreeAll, Get, Child, GetChild, Mark,
//! Remove, MakeId and ChildId over their domains), `VIClient::LoadTextSets`
//! and `VIWndMenu4::ParseEscape`/`Expand` out of build/libclient.so on a
//! script, and `vi::textset` runs the same script: every answer, and the
//! whole table each time it is asked for, must be equal. Negative controls:
//! a load that keeps a lone space, and an escape that takes a '%' straight
//! after its letter as its end, must each differ. Needs the oracle (see
//! tests/common).

mod common;

use common::*;
use vi::textset::*;

#[derive(Clone, Debug)]
enum Step {
    Reset,
    FreeAll,
    Get(i32),
    Child(i32, i32),
    GetChild(i32, i32),
    Mark(i32, i32),
    Remove(i32),
    Load(Vec<(i32, Vec<Record>)>),
    Parse(Vec<u16>),
    Expand(Option<Vec<u16>>, i32),
    Table,
    Ids,
}

fn put_i(o: &mut Vec<u8>, v: i32) {
    o.extend(v.to_le_bytes());
}

fn put_wide(o: &mut Vec<u8>, s: Option<&[u16]>) {
    match s {
        None => put_i(o, -1),
        Some(s) => {
            put_i(o, s.len() as i32);
            for c in s {
                o.extend(c.to_le_bytes());
            }
        }
    }
}

fn request(steps: &[Step]) -> Vec<u8> {
    let mut o = Vec::new();
    for s in steps {
        match s {
            Step::Reset => put_i(&mut o, 0),
            Step::FreeAll => put_i(&mut o, 1),
            Step::Get(id) => [2, *id].iter().for_each(|&v| put_i(&mut o, v)),
            Step::Child(id, k) => [3, *id, *k].iter().for_each(|&v| put_i(&mut o, v)),
            Step::GetChild(id, k) => [4, *id, *k].iter().for_each(|&v| put_i(&mut o, v)),
            Step::Mark(id, d) => [5, *id, *d].iter().for_each(|&v| put_i(&mut o, v)),
            Step::Remove(id) => [6, *id].iter().for_each(|&v| put_i(&mut o, v)),
            Step::Load(sets) => {
                put_i(&mut o, 7);
                put_i(&mut o, sets.len() as i32);
                for (id, recs) in sets {
                    put_i(&mut o, *id);
                    assert_eq!(recs.len(), 4);
                    for r in recs {
                        put_i(&mut o, r.slot);
                        put_wide(&mut o, Some(&r.text));
                        put_wide(&mut o, Some(&r.label));
                    }
                }
            }
            Step::Parse(s) => {
                put_i(&mut o, 8);
                put_wide(&mut o, Some(s));
            }
            Step::Expand(s, size) => {
                put_i(&mut o, 9);
                put_wide(&mut o, s.as_deref());
                put_i(&mut o, *size);
            }
            Step::Table => put_i(&mut o, 10),
            Step::Ids => put_i(&mut o, 11),
        }
    }
    o
}

/// The plausible slips the controls plant.
#[derive(Clone, Copy, Default)]
struct Bugs {
    lone_space_kept: bool,
    pct_after_letter_ends: bool,
}

fn parse(s: &[u16], bugs: Bugs) -> (usize, i32, i32) {
    let r = parse_escape(s, 0);
    if bugs.pct_after_letter_ends && r.1 == -1 && r.0 == 1 && s.get(1) == Some(&(b'%' as u16)) {
        return (2, -1, -1);
    }
    r
}

fn ours(steps: &[Step], bugs: Bugs) -> Vec<u8> {
    let mut t = TextSets::default();
    let mut o = Vec::new();
    put_i(&mut o, 0);
    for s in steps {
        let present = |t: &TextSets, id: i32| t.set(id).is_some();
        match s {
            Step::Reset => t.reset(),
            Step::FreeAll => t.free_all(),
            Step::Get(id) => put_i(&mut o, t.get(*id).unwrap().id as i32),
            Step::Child(id, _) | Step::GetChild(id, _) | Step::Mark(id, _) | Step::Remove(id) if !present(&t, *id) => put_i(&mut o, -2),
            Step::Child(id, k) => put_i(&mut o, t.child(*id as u8, *k).map_or(-1, |c| c as i32)),
            Step::GetChild(id, k) => put_i(&mut o, t.get_child(*id as u8, *k).unwrap().id as i32),
            Step::Mark(id, d) => {
                t.mark(*id as u8, *d != 0);
                put_i(&mut o, 0);
            }
            Step::Remove(id) => {
                t.remove(*id as u8);
                put_i(&mut o, 0);
            }
            Step::Load(sets) => {
                t.load(sets);
                if bugs.lone_space_kept {
                    for (id, recs) in sets {
                        for r in recs.iter().filter(|r| (0..4).contains(&r.slot)) {
                            let pair = &mut t.set_mut(*id).unwrap().pairs[r.slot as usize];
                            if r.text == [b' ' as u16] {
                                pair.text = Some(r.text.clone());
                            }
                        }
                    }
                }
            }
            Step::Parse(s) => {
                let (p, ty, n) = parse(s, bugs);
                [p as i32, ty, n].iter().for_each(|&v| put_i(&mut o, v));
            }
            Step::Expand(s, size) => match expand(s.as_deref(), *size as u16, &|_| None) {
                None => put_i(&mut o, -1),
                Some(d) => {
                    put_i(&mut o, 0);
                    put_wide(&mut o, Some(&d));
                }
            },
            Step::Table => {
                for i in 0..256 {
                    match t.set(i) {
                        None => put_i(&mut o, 0),
                        Some(s) => {
                            put_i(&mut o, 1);
                            o.extend([s.id, s.p01, s.p02, s.p03]);
                            for p in &s.pairs {
                                put_wide(&mut o, p.label.as_deref());
                                put_wide(&mut o, p.text.as_deref());
                            }
                        }
                    }
                }
            }
            Step::Ids => {
                for a in -1..=3 {
                    for b in -1..=3 {
                        for c in -1..=3 {
                            put_i(&mut o, make_id(a, b, c));
                        }
                    }
                }
                for id in 0..=255u8 {
                    for k in 0..4 {
                        put_i(&mut o, child_id(id, k));
                    }
                }
            }
        }
    }
    o
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// A random string over the characters the escapes care about (and 0x157,
/// whose low byte is 'W'); `w` lets W and w in.
fn text(rng: &mut Rng, max: usize, w: bool) -> Vec<u16> {
    const CHARS: &[u16] = &[37, 37, 37, 67, 99, 69, 101, 87, 119, 80, 112, 84, 116, 88, 48, 49, 50, 55, 57, 32, 97, 233, 0x157];
    let n = (rng.next() % (max as u64 + 1)) as usize;
    (0..n).map(|_| CHARS[(rng.next() % CHARS.len() as u64) as usize]).filter(|&c| w || (c != 87 && c != 119)).collect()
}

/// The script: the id functions' domains, trees built and cut at random
/// (`GetChild` never at depth 3, which the game never asks and which runs
/// off its table), loads of records with empty strings, lone spaces, NULs,
/// stray slots and repeated sets, and the escapes over random strings
/// (Expand's without W: the oracle's menu has no words to give).
fn script(seed: u64) -> Vec<Step> {
    let mut rng = Rng(seed);
    let mut s = vec![Step::Ids, Step::Reset, Step::Table];
    let strings = ["", " ", "  ", " x", "Yes", "I need help!", "|22", "%W12%", "a\0b", "Attack %4!"];
    for round in 0..60 {
        match rng.next() % 4 {
            0 => {
                let n = 1 + (rng.next() % 12) as usize;
                let mut sets = Vec::new();
                for _ in 0..n {
                    let id = [0, 1, 5, 6, 9, 13, 22, 54, 255, (rng.next() % 256) as i32][(rng.next() % 10) as usize];
                    let recs = (0..4)
                        .map(|k| {
                            let slot = if rng.next() % 6 == 0 { [-1, 4, 5, 3][(rng.next() % 4) as usize] } else { k };
                            let pick = |rng: &mut Rng, cap: usize| {
                                let v = if rng.next() % 3 == 0 { text(rng, 20, true) } else { wide(strings[(rng.next() % strings.len() as u64) as usize]) };
                                v[..v.len().min(cap)].to_vec()
                            };
                            Record { slot, text: pick(&mut rng, 127), label: pick(&mut rng, 63) }
                        })
                        .collect();
                    sets.push((id, recs));
                }
                s.push(Step::Load(sets));
            }
            1 => {
                for _ in 0..20 {
                    let id = (rng.next() % 256) as i32;
                    let k = (rng.next() % 4) as i32;
                    s.push(match rng.next() % 8 {
                        0 | 1 => Step::Get(id),
                        2 | 3 => Step::Child(id, k),
                        4 if id & 3 != 3 => Step::GetChild(id, k),
                        5 => Step::Mark(id, (rng.next() % 2) as i32),
                        6 => Step::Remove(id),
                        _ => Step::Get([0, 1, 5, 2, 6, 3][(rng.next() % 6) as usize]),
                    });
                }
            }
            2 => {
                for _ in 0..40 {
                    s.push(Step::Parse(text(&mut rng, 12, true)));
                    let size = if rng.next() % 4 == 0 { 256 } else { 1 + (rng.next() % 16) as i32 };
                    s.push(Step::Expand(Some(text(&mut rng, 20, false)), size));
                }
                s.push(Step::Expand(None, 256));
            }
            _ => s.push(if round % 7 == 0 { Step::FreeAll } else { Step::Reset }),
        }
        s.push(Step::Table);
    }
    // the escapes' edges, written out
    for t in ["", "W", "W%", "W12", "W12%x", "w7%", "Wx%y", "W1x2%", "C%", "Q%rest", "Qrest", "%", "%%", "T00042%"] {
        s.push(Step::Parse(wide(t)));
    }
    for (t, n) in [("x%C%ab%cd", 256), ("%", 256), ("ab%", 3), ("abc", 1), ("abc", 2), ("%E9%%P1%z", 4)] {
        s.push(Step::Expand(Some(wide(t)), n));
    }
    s
}

#[test]
fn the_quick_chat_tree_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let mut steps_total = 0;
    for seed in [0x5EED_0001u64, 0x00C0_FFEE, 0x7E57_5E75] {
        let steps = script(seed);
        steps_total += steps.len();
        let theirs = o.call(810, &request(&steps));
        assert_eq!(theirs.get(0..4), Some(&[0u8, 0, 0, 0][..]), "the oracle could not load libclient.so");
        let mine = ours(&steps, Bugs::default());
        if let Some(i) = (0..mine.len().min(theirs.len())).find(|&i| mine[i] != theirs[i]) {
            panic!("seed {seed:#x}: byte {i} of {} differs (ours {:02x?}, theirs {:02x?})", mine.len(), &mine[i..(i + 16).min(mine.len())], &theirs[i..(i + 16).min(theirs.len())]);
        }
        assert_eq!(mine.len(), theirs.len(), "seed {seed:#x}");
        // Negative controls.
        for bugs in [Bugs { lone_space_kept: true, ..Default::default() }, Bugs { pct_after_letter_ends: true, ..Default::default() }] {
            assert_ne!(ours(&steps, bugs), theirs, "seed {seed:#x}: a planted slip went unnoticed");
        }
    }
    eprintln!("{steps_total} steps equal");
}
