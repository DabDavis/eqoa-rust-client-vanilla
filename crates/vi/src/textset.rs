//! The quick-chat menus' tree (CLIENT's VITextSet.cpp): up to 256 sets in one
//! table, by id, each four (label, text) pairs; `VIClient::LoadTextSets`,
//! which fills it from the world entry's records; and the escapes a choice's
//! title may hold (`VIWndMenu4::ParseEscape`, `Expand`). An id is the set's
//! path: its low two bits are its depth (0 the root .. 3), and each level
//! above it adds a 2-bit child index at bits 2, 4 and 6 (`MakeId`,
//! `ChildId`), so the root is 0 and a set at depth 3 has no children. Held to
//! the decomp by `tests/oracle_textset.rs` (eqoa-port's op 810, CLIENT's own
//! functions out of its libclient.so).

/// `VITextPair`: a choice's label and its line, each optional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextPair {
    /// `m_p00`: the label (`VIRec388::m_p104`'s copy)
    pub label: Option<Vec<u16>>,
    /// `m_p04`: the line (`VIRec388::m_p04`'s copy); `|<n>` is command n
    pub text: Option<Vec<u16>>,
}

/// `VITextSet`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextSet {
    /// `m_id`: the path
    pub id: u8,
    /// `m_p01`: `Mark` sets 2
    pub p01: u8,
    pub p02: u8,
    pub p03: u8,
    pub pairs: [TextPair; 4],
}

/// One of a `VIRec6A`'s records (`VIRec388`): the pair's slot, its line and
/// its label, as the world entry carries them (`Msg000D`) and 0x006A and
/// 0x006C send them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    /// `m_p00`
    pub slot: i32,
    /// `m_p04`, at most 127 characters
    pub text: Vec<u16>,
    /// `m_p104`, at most 63
    pub label: Vec<u16>,
}

/// `MakeId(a, b, c)`: the set at the path a, b, c, cut at its first -1.
pub fn make_id(a: i32, b: i32, c: i32) -> i32 {
    if a == -1 {
        0
    } else if b == -1 {
        (a << 2) | 1
    } else if c == -1 {
        (a << 2) | 2 | (b << 4)
    } else {
        (a << 2) | 3 | (b << 4) | (c << 6)
    }
}

/// `ChildId(k)` of set `id`. A set at depth 3 has none: its k past 0 is 256
/// or more, off the table (`Child` never asks; `GetChild` would).
pub fn child_id(id: u8, k: i32) -> i32 {
    let depth = (id as i32 & 3) + 1;
    (id as i32 & !3) | depth | (k << (depth * 2))
}

/// The string up to its first NUL (`strlen16`).
fn cut(s: &[u16]) -> &[u16] {
    &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
}

/// The table (`sSets`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextSets {
    sets: Vec<Option<TextSet>>,
}

impl Default for TextSets {
    fn default() -> Self {
        TextSets { sets: vec![None; 256] }
    }
}

impl TextSets {
    /// `Reset`: every slot empty (the game forgets the sets without freeing
    /// them; here they go).
    pub fn reset(&mut self) {
        self.sets.iter_mut().for_each(|s| *s = None);
    }

    /// `FreeAll`
    pub fn free_all(&mut self) {
        self.reset();
    }

    /// `GetTable()[id]`
    pub fn set(&self, id: i32) -> Option<&TextSet> {
        self.sets.get(usize::try_from(id).ok()?)?.as_ref()
    }

    pub fn set_mut(&mut self, id: i32) -> Option<&mut TextSet> {
        self.sets.get_mut(usize::try_from(id).ok()?)?.as_mut()
    }

    /// `GetRoot`
    pub fn root(&self) -> Option<&TextSet> {
        self.set(0)
    }

    /// `Child(k)` of set `id`: its id, when it is in the table; none at
    /// depth 3.
    pub fn child(&self, id: u8, k: i32) -> Option<u8> {
        if id & 3 == 3 {
            return None;
        }
        self.set(child_id(id, k)).map(|s| s.id)
    }

    /// `Get(id)`: the set, made empty on first use. The game indexes its
    /// table with any `id`; here one off it is none.
    pub fn get(&mut self, id: i32) -> Option<&mut TextSet> {
        let slot = self.sets.get_mut(usize::try_from(id).ok()?)?;
        Some(slot.get_or_insert_with(|| TextSet { id: id as u8, ..Default::default() }))
    }

    /// `GetChild(k)`: `Get(ChildId(k))`.
    pub fn get_child(&mut self, id: u8, k: i32) -> Option<&mut TextSet> {
        self.get(child_id(id, k))
    }

    /// `Mark(deep)`: with `deep`, each child's whole tree first; then the
    /// set's `m_p01` is 2.
    pub fn mark(&mut self, id: u8, deep: bool) {
        for k in 0..4 {
            if let Some(c) = self.child(id, k)
                && deep
            {
                self.mark(c, true);
            }
        }
        if let Some(s) = self.set_mut(id as i32) {
            s.p01 = 2;
        }
    }

    /// `Remove`: the set out of the table.
    pub fn remove(&mut self, id: u8) {
        self.sets[id as usize] = None;
    }

    /// `VIClient::LoadTextSets`: the table emptied, then each record's set
    /// (made on first use) takes each of its records' strings into the pair
    /// its slot names (0..3; any other is skipped), a string that is empty
    /// or a lone space leaving the pair's as it was. (A set id off the table
    /// is skipped: the game would write past it.)
    pub fn load(&mut self, sets: &[(i32, Vec<Record>)]) {
        self.reset();
        for (id, records) in sets {
            let Some(t) = self.get(*id) else { continue };
            for r in records {
                if !(0..4).contains(&r.slot) {
                    continue;
                }
                let kept = |s: &[u16]| !s.is_empty() && (s[0] != b' ' as u16 || s.len() > 1);
                let (label, text) = (cut(&r.label), cut(&r.text));
                let pair = &mut t.pairs[r.slot as usize];
                if kept(label) {
                    pair.label = Some(label.to_vec());
                }
                if kept(text) {
                    pair.text = Some(text.to_vec());
                }
            }
        }
    }
}

/// `VIWndMenu4::ParseEscape`, at `s[p]` (just after a '%'): a letter, C E P
/// T or W in either case, then digits up to a closing '%' or the end. Gives
/// where it stopped, the letter (upper case) and the number; anything else
/// is -1 and -1, skipped to just past the next '%' or to the end. (A '%'
/// straight after the letter stops AT it, and is read as the next escape's.)
pub fn parse_escape(s: &[u16], p: usize) -> (usize, i32, i32) {
    let at = |p: usize| s.get(p).copied().unwrap_or(0);
    let skip = |mut p: usize| -> usize {
        loop {
            if at(p) == b'%' as u16 {
                return p + 1;
            }
            p += 1;
            if at(p) == 0 {
                return p;
            }
        }
    };
    if at(p) == 0 {
        return (p, -1, -1);
    }
    let c = match at(p) as u8 {
        _ if at(p) > 0xFF => return (skip(p), -1, -1),
        b'C' | b'c' => b'C',
        b'E' | b'e' => b'E',
        b'W' | b'w' => b'W',
        b'P' | b'p' => b'P',
        b'T' | b't' => b'T',
        _ => return (skip(p), -1, -1),
    } as i32;
    let mut p = p + 1;
    if at(p) == 0 || at(p) == b'%' as u16 {
        return (p, -1, -1);
    }
    let mut n = 0i32;
    while at(p) != 0 {
        let ch = at(p);
        if ch == b'%' as u16 {
            p += 1;
            break;
        }
        if (ch as u32).wrapping_sub(b'0' as u32) < 10 {
            n = n.wrapping_mul(10).wrapping_add(ch as i32 - b'0' as i32);
        } else {
            return (skip(p), -1, -1);
        }
        p += 1;
    }
    (p, c, n)
}

/// `VIWndMenu4::Expand(src, dst, size)`: a choice's title with each `%W<n>%`
/// the word n (`word`, WORDS.TXT's row in the language), the other escapes
/// dropped, at most `size - 1` characters (a word that would not fit ends
/// it). None where `src` is (the game's -1 for a null pointer).
///
/// As shipped, a word is copied with `size` less what is written as its
/// limit in BYTES, so one near the end is cut short where it is copied; the
/// string ends at that cut.
pub fn expand(src: Option<&[u16]>, size: u16, word: &dyn Fn(i32) -> Option<Vec<u16>>) -> Option<Vec<u16>> {
    let src = cut(src?);
    let size = size as usize;
    // the game's buffer, written as it writes it; read back to its first NUL
    let mut dst: Vec<u16> = vec![0xAAAA; size.max(1)];
    let mut p = 0usize;
    let mut i = 0usize;
    let mut full = false;
    while i < src.len() {
        let c = src[i];
        i += 1;
        if c == b'%' as u16 {
            let (next, ty, num) = parse_escape(src, i);
            i = next;
            if ty == b'W' as i32
                && let Some(w) = word(num)
            {
                let w = cut(&w);
                if p + w.len() < size {
                    // strcpy16(p, size - p, w): a byte limit
                    let room = size - p;
                    let mut k = 0;
                    while k < w.len() && (k + 1) * 2 < room {
                        dst[p + k] = w[k];
                        k += 1;
                    }
                    dst[p + k] = 0;
                    p += w.len();
                } else {
                    full = true;
                }
            }
        } else if p + 1 < size {
            dst[p] = c;
            p += 1;
        } else {
            full = true;
        }
        if full {
            break;
        }
    }
    let last = dst.len() - 1;
    dst[p.min(last)] = 0;
    Some(cut(&dst).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// The paths: the root, each child's id, a grandchild's; depth 3 has no
    /// children in the table.
    #[test]
    fn ids_are_paths() {
        assert_eq!(make_id(-1, 2, 3), 0);
        assert_eq!(make_id(2, -1, -1), 9);
        assert_eq!(make_id(1, 3, -1), 54);
        assert_eq!(make_id(3, 2, 1), 111);
        assert_eq!(child_id(0, 2), 9);
        assert_eq!(child_id(9, 1), 26);
        assert_eq!(child_id(26, 3), make_id(2, 1, 3));
        assert_eq!(child_id(3, 1), 260, "past the table");
        let mut t = TextSets::default();
        t.get(0);
        t.get(child_id(0, 1));
        assert_eq!(t.child(0, 1), Some(5));
        assert_eq!(t.child(0, 2), None);
        t.get(make_id(0, 0, 0));
        assert_eq!(t.child(make_id(0, 0, 0) as u8, 0), None, "depth 3");
    }

    /// Marking a set marks its whole tree with `deep`, else only itself.
    #[test]
    fn mark_takes_the_tree() {
        let mut t = TextSets::default();
        for id in [0, 1, 5, 2, 6, 18, 3] {
            t.get(id);
        }
        t.mark(1, false);
        assert_eq!(t.set(1).unwrap().p01, 2);
        assert_eq!(t.set(2).unwrap().p01, 0);
        t.mark(0, true);
        assert!([0, 1, 5, 2, 6, 18, 3].iter().all(|&i| t.set(i).unwrap().p01 == 2));
    }

    /// A record's empty string or lone space leaves its pair as it was; a
    /// slot outside 0..3 is skipped; a string stops at a NUL.
    #[test]
    fn load_keeps_what_says_something() {
        let mut t = TextSets::default();
        t.get(77);
        let r = |slot, text: &str, label: &str| Record { slot, text: w(text), label: w(label) };
        t.load(&[(0, vec![r(0, " ", "Responses"), r(1, "Yes", ""), r(4, "x", "x"), r(2, " a", "b\0c")]), (0, vec![r(1, "", "Y")])]);
        assert!(t.set(77).is_none(), "the table was emptied");
        let s = t.set(0).unwrap();
        assert_eq!(s.pairs[0], TextPair { label: Some(w("Responses")), text: None });
        assert_eq!(s.pairs[1], TextPair { label: Some(w("Y")), text: Some(w("Yes")) });
        assert_eq!(s.pairs[2], TextPair { label: Some(w("b")), text: Some(w(" a")) });
        assert_eq!(s.pairs[3], TextPair::default());
    }

    /// `%W<n>%` is the word; other escapes go; a word that does not fit ends
    /// the title, one that only just fits is cut where it is copied.
    #[test]
    fn expand_puts_in_words() {
        let words = |n: i32| (n == 7).then(|| w("seven"));
        assert_eq!(expand(Some(&w("a%W7%b%C3%c")), 256, &words), Some(w("asevenbc")));
        assert_eq!(expand(Some(&w("%w7")), 256, &words), Some(w("seven")), "to the end, lower case");
        assert_eq!(expand(None, 256, &words), None);
        assert_eq!(expand(Some(&w("abcdef")), 4, &words), Some(w("abc")));
        assert_eq!(expand(Some(&w("ab%W7%z")), 7, &words), Some(w("ab")), "does not fit: ends");
        // fits by count (2 + 5 < 8) but its byte limit is 6: two characters
        assert_eq!(expand(Some(&w("ab%W7%z")), 8, &words), Some(w("abse")));
        // %W% stops at the '%', which then starts an escape of its own
        assert_eq!(expand(Some(&w("x%W%ab%cd")), 256, &words), Some(w("xcd")));
    }
}
