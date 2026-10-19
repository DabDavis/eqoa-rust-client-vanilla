//! `vi::ui` against the decomp's own UI layer: eqoa-port's `build/oracle_ui`
//! compiles viui.cpp, viuiwindow.cpp, viuitextscroll.cpp and vifont.cpp
//! over a raster that RECORDS each call (and a dictionary that answers with
//! the id asked for), and the same calls are made here through a `Raster`
//! that records the same way. Every call's result and every record must be
//! equal: blits (surface id, flags, position, texel rectangle, blend, alpha),
//! strings (font, position, colour, text, centring mode) and viewports.
//!
//! The script covers every atlas id (and three out of range) through the
//! blitters, tiling across row-1 and stretched entries, bars, frames, words,
//! window open/close animation in all three sets, `TextBox` with each
//! placement flag, ellipsis, asterisks and skips, `DrawTextInternal`'s
//! inline icons (the pulsing ones at a fixed clock), and 700 frames of both
//! scroll styles (long enough to pause at both ends). A recorder with a
//! planted bug must disagree.
//!
//! The atlas is SUPPORT's data: the decomp's side has it from libsupport
//! (gendata), this side from the decrypted image at run/support.bin. Needs
//! that, the disc (TEXTURES.TXT, WORDS.TXT, ARIALUI.CSF) and the oracle;
//! SKIPs without them.
#![allow(clippy::too_many_arguments)]

mod common;

use common::*;
use vi::strtab::StringTable;
use vi::ui::{FrameParam, Raster, TextScroll, Vui, Window, font_id, wide};

/// Records the calls as the oracle does.
struct Rec {
    out: Vec<u8>,
    n: u32,
    widths: std::collections::HashMap<u16, u32>,
    point: i32,
    vp: [i32; 4],
    /// A planted bug: shift every blit's x by this.
    bug: i32,
}

impl Rec {
    fn i(&mut self, v: i32) {
        self.out.extend(v.to_le_bytes());
    }
    fn f(&mut self, v: f32) {
        self.out.extend(v.to_le_bytes());
    }
    fn rec(&mut self, kind: i32) {
        self.n += 1;
        self.i(kind);
    }
    fn s(&mut self, s: &[u16]) {
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        self.i(n as i32);
        for &c in &s[..n] {
            self.out.extend(c.to_le_bytes());
        }
    }
    fn color(&mut self, c: [f32; 4]) {
        for v in c {
            self.f(v);
        }
    }
}

impl Raster for Rec {
    fn blit(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        self.rec(1);
        for v in [id as i32, flags as i32, at[0] + self.bug, at[1], a[0], a[1], b[0], b[1], mode] {
            self.i(v);
        }
        self.f(alpha);
    }
    fn stretch_blit(&mut self, id: u32, flags: u32, at: [i32; 2], size: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        self.rec(3);
        for v in [id as i32, flags as i32, at[0] + self.bug, at[1], size[0], size[1], a[0], a[1], b[0], b[1], mode] {
            self.i(v);
        }
        self.f(alpha);
    }
    fn draw_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]) {
        self.rec(4);
        self.i(font_id() as i32);
        self.i(at[0]);
        self.i(at[1]);
        self.color(color);
        self.s(s);
    }
    fn draw_centered_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16], mode: u32) {
        self.rec(5);
        self.i(font_id() as i32);
        self.i(at[0]);
        self.i(at[1]);
        self.color(color);
        self.i(mode as i32);
        self.s(s);
    }
    fn string_width(&self, s: &[u16]) -> i32 {
        let mut w = 0;
        for &c in s {
            let g = self.widths.get(&c).or_else(|| self.widths.get(&(b'?' as u16)));
            let Some(g) = g else { return -1 };
            w += *g as i32 + 1;
        }
        w
    }
    fn point_size(&self) -> i32 {
        self.point
    }
    fn viewport(&self) -> [i32; 4] {
        self.vp
    }
    fn set_viewport(&mut self, v: [i32; 4]) {
        self.rec(6);
        for x in v {
            self.i(x);
        }
        self.vp = v;
    }
}

/// One step: the function, its integer and float arguments, its string.
struct Call {
    f: i32,
    a: [i32; 24],
    x: [f32; 8],
    s: Vec<u16>,
}

fn call(f: i32, a: &[i32], x: &[f32], s: &str) -> Call {
    let mut c = Call { f, a: [0; 24], x: [0.0; 8], s: wide(s) };
    c.a[..a.len()].copy_from_slice(a);
    c.x[..x.len()].copy_from_slice(x);
    c
}

fn frame_call(f: i32, a: &[i32], fp: &FrameParam, x: &[f32]) -> Call {
    let mut c = call(f, a, x, "");
    let parts = [fp.tl, fp.top, fp.tr, fp.right, fp.br, fp.bottom, fp.bl, fp.left];
    for (i, p) in parts.iter().enumerate() {
        c.a[8 + i] = p.0;
        c.a[16 + i] = p.1 as i32;
    }
    c
}

struct Side {
    vui: Vui,
    win: [Window; 4],
    scroll: [TextScroll; 2],
    /// fn 41's studs and fill, for fns 15 and 16.
    studs: FrameParam,
    fill: i32,
}

impl Side {
    /// The Rust half of one step: (result, the calls recorded).
    fn run(&mut self, r: &mut Rec, c: &Call) -> i32 {
        let a = &c.a;
        let x = &c.x;
        let col = [x[0], x[1], x[2], x[3]];
        let s = &c.s;
        let fp = FrameParam {
            tl: (a[8], a[16] as u32),
            top: (a[9], a[17] as u32),
            tr: (a[10], a[18] as u32),
            right: (a[11], a[19] as u32),
            br: (a[12], a[20] as u32),
            bottom: (a[13], a[21] as u32),
            bl: (a[14], a[22] as u32),
            left: (a[15], a[23] as u32),
        };
        let k = (a[0] & 3) as usize;
        let j = (a[0] & 1) as usize;
        let v = &self.vui;
        match c.f {
            1 => v.blit(r, a[0], [a[1], a[2]]),
            2 => v.blit_size(r, a[0], [a[1], a[2]], [a[3], a[4]]),
            3 => v.blit_ex(r, a[0], [a[1], a[2]], a[3], x[0], a[4] as u32),
            4 => v.blit_ex_size(r, a[0], [a[1], a[2]], [a[3], a[4]], a[5], x[0], a[6] as u32),
            5 => v.blit_horz(r, [a[0], a[1]], a[2], a[3], a[4], x[0], a[5] as u32),
            6 => v.blit_vert(r, [a[0], a[1]], a[2], a[3], a[4], x[0], a[5] as u32),
            7 => v.blit_rect(r, a[0], [a[1], a[2]], [a[3], a[4]], a[5], x[0]),
            8 => v.blit_bar(r, s, col, [a[0], a[1]], a[2], a[3], a[4], a[5], x[4], a[6] as u32, a[7] != 0),
            9 => v.blit_frame(r, [a[0], a[1]], [a[2], a[3]], &fp, a[4], x[0], a[5] != 0),
            10 => v.blit_fill_frame(r, [a[0], a[1]], [a[2], a[3]], &fp, a[6], a[4], x[0], a[5] != 0),
            11 => v.draw_string_ex(r, s, a[0], a[1], col, a[2] != 0),
            12 => v.draw_word_ex(r, a[3] as u16 as i32, a[0], a[1], col, a[2] != 0),
            13 => v.draw_centered_string_ex(r, s, a[0], a[1], col, a[2] as u32, a[3] != 0),
            20 => self.win[k].init(v, r, [a[1], a[2]], a[3], a[4], a[5]),
            21 => self.win[k].open(a[1] != 0),
            22 => self.win[k].close(a[1] != 0),
            23 => return self.win[k].draw(v, r) as i32,
            24 => return self.win[k].text_box(v, r, s, a[1], a[2], a[3], a[4], col, a[5] as u32),
            25 => self.win[k].draw_text_internal(v, r, s, a[1], a[2], col, a[3] as u32),
            26 => return self.win[k].text_width(v, r, s),
            27 => return self.win[k].open.to_bits() as i32,
            34 => self.win[k].tick_r(v, r, a[1], a[2], a[3] != 0),
            35 => {
                let t = self.vui.time;
                self.win[k].reset_tick_pulse(t);
            }
            28 => self.win[k].text(v, r, s, a[1], a[2], col, a[3] as u32),
            29 => return self.win[k].count_lines(v, r, s, a[1]),
            30 => self.scroll[j].init(v, r, [a[1], a[2]], a[3], a[4], a[5]),
            31 => {
                self.scroll[j].window.open(true);
                self.scroll[j].text_scroll(v, r, s, col, a[1] as u32);
            }
            32 => self.scroll[j].reset(),
            33 => return self.scroll[j].offset,
            14 => v.stretch_blit_ex(r, a[0], [a[1], a[2]], [a[3], a[4]], a[5], x[0]),
            15 => v.blit_studded_frame(r, [a[0], a[1]], [a[2], a[3]], &fp, &self.studs, a[4], a[5], a[6], x[0], a[7] != 0),
            16 => v.blit_fill_studded_frame(r, [a[0], a[1]], [a[2], a[3]], &fp, &self.studs, self.fill, a[4], a[5], a[6], x[0], a[7] != 0),
            17 => v.blit_studded_horz(r, [a[0], a[1]], a[2], a[3], a[4], a[5], a[6], x[0], a[7] as u32),
            18 => v.blit_vert_bar(r, [a[0], a[1]], a[2], a[3], a[4], a[5], a[6], x[0]),
            19 => v.blit_bar3(r, s, col, [a[0], a[1]], a[2], a[3], a[4], a[5], a[6], x[4], a[7] as u32, a[8] != 0),
            70 => self.win[k].tick_u(v, r, a[1], a[2], a[3] != 0),
            71 => self.win[k].tick_d(v, r, a[1], a[2], a[3] != 0),
            72 => self.win[k].draw_frame(v, r, a[1], a[2], a[3], a[4], a[5] as u32),
            73 => self.win[k].draw_frame_vscroll(v, r, a[1], a[2], a[3], a[4], x[0]),
            36 => self.win[k].draw_vscroll(v, r, [a[1], a[2]], a[3], x[0]),
            41 => {
                self.studs = fp;
                self.fill = a[0];
            }
            40 => self.vui.time = a[0] as i64,
            _ => return -1,
        }
        0
    }
}

fn script() -> Vec<Call> {
    let mut v = Vec::new();
    let mut rng = Rng(7);
    let mut int = |lo: i32, hi: i32| lo + (rng.next() % (hi - lo + 1) as u64) as i32;
    // Every atlas id, and three out of range, through the four blitters.
    for id in (-1..=252).chain([300]) {
        let at = [int(-20, 600), int(-20, 430)];
        v.push(call(1, &[id, at[0], at[1]], &[], ""));
        v.push(call(2, &[id, at[0], at[1], int(-1, 90), int(-1, 60)], &[], ""));
        v.push(call(3, &[id, at[0], at[1], int(0, 4), int(0, 3)], &[0.5], ""));
        v.push(call(4, &[id, at[0], at[1], int(-1, 300), -1, int(0, 4), int(0, 3)], &[0.75], ""));
    }
    // Tiling: row-1 entries and stretched ones, widths through zero.
    for id in [2, 25, 26, 32, 33, 34, 37, 38, 41, 42, 43, 45, 47, 51, 52, 61, 102, 104, 136, 151] {
        for w in [-5, 0, 1, 7, 64, 100, 173, 300] {
            v.push(call(5, &[10, 20, id, w, 3, 0], &[1.0], ""));
            v.push(call(6, &[30, 40, id, w, 3, 1], &[0.3], ""));
            v.push(call(7, &[id, 5, 6, w, w / 2 + 3, 3], &[0.6], ""));
        }
    }
    // Bars, with text.
    for (just, sh) in [(0, 1), (1, 0), (3, 1)] {
        v.push(call(8, &[20, 373, 25, 26, 174, 3, just, sh], &[0.1, 0.2, 0.3, 0.9, 0.8], "Training Master"));
        v.push(call(8, &[200, 100, 33, 34, 260, 3, just, sh], &[1.0, 1.0, 1.0, 1.0, 1.0], ""));
    }
    // Frames: Draw's four sets, and a frame without its left side.
    let sets = [
        FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (36, 1), bottom: (37, 0), bl: (36, 0), left: (42, 0) },
        FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) },
        FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) },
    ];
    for fp in &sets {
        v.push(frame_call(9, &[50, 60, 300, 120, 3, 1], fp, &[0.9]));
        v.push(frame_call(9, &[50, 60, 300, 120, 3, 0], fp, &[0.9]));
        v.push(frame_call(9, &[50, 60, 0, 120, 3, 1], fp, &[0.9]));
        v.push(frame_call(10, &[40, 30, 462, 84, 3, 1, 61], fp, &[0.75]));
        v.push(frame_call(10, &[40, 30, 200, 300, 3, 1, 35], fp, &[1.0]));
    }
    // Stretched blits, studded frames and bars (the start menu's and the
    // lists'), the vertical bar and the scroll bar at its ends and past them.
    for id in [1, 36, 37, 82, 84, 138, 140, 250] {
        v.push(call(14, &[id, 12, 34, int(-5, 200), int(-5, 60), 3], &[0.3], ""));
    }
    let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
    let list = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (153, 1), bottom: (37, 2), bl: (153, 0), left: (42, 0) };
    for (fp, st, fill) in [(&sets[0], &studs, 51), (&list, &FrameParam { top: (38, 0), bottom: (38, 2), ..studs }, 51)] {
        v.push(frame_call(41, &[fill], st, &[]));
        for (nx, ny, left) in [(2, 3, 0), (1, 3, 1), (0, 0, 1), (5, 1, 1)] {
            v.push(frame_call(15, &[176, 37, 275, 294, nx, ny, 0, left], fp, &[1.0]));
            v.push(frame_call(16, &[16, 11, 160, 300, nx, ny, 3, left], fp, &[0.6]));
        }
    }
    for (bar, stud, w, n) in [(37, 38, 177, 1), (40, 41, 200, 1), (37, 38, 0, 2), (37, 38, 90, 0), (45, 38, 300, 4)] {
        v.push(call(17, &[-1, 37, bar, stud, w, n, 0, 0], &[1.0], ""));
    }
    for h in [268, 270, 40, 0] {
        v.push(call(18, &[126, 9, 81, 83, 82, h, 3], &[0.5], ""));
        v.push(call(18, &[126, 9, 81, 83, 26, h, 3], &[0.5], ""));
    }
    // The main menu's three-piece bars, with words and a lit colour.
    for (just, sh) in [(0, 0), (1, 1), (3, 0)] {
        v.push(call(19, &[18, 56, 141, 142, 150, 140, 0, just, sh], &[10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0, 1.0], "Inventory"));
        v.push(call(19, &[18, 83, 33, 34, 26, 90, 3, just, sh], &[0.0, 0.0, 0.0, 1.0, 0.5], ""));
    }
    v.push(call(20, &[1, 57, 132, 114, 10, 1], &[], ""));
    v.push(call(21, &[1, 1], &[], ""));
    for t in [-0.5, 0.0, 0.25, 0.999, 1.0, 1.5] {
        v.push(call(36, &[1, 126, 9, 268, 0], &[t], ""));
    }
    // The inspect and quantity windows' frames and ticks, opening and open,
    // at two clocks.
    v.push(call(20, &[2, 320, 170, 470, 17, 1], &[], ""));
    for (open, t) in [(0, 0), (1, 0), (1, 377)] {
        v.push(call(21, &[2, open], &[], ""));
        v.push(call(23, &[2], &[], ""));
        v.push(call(40, &[t], &[], ""));
        for (row, rows) in [(0, 15), (2, 3)] {
            for f in [0, 0x20, 0x40, 0x60] {
                v.push(call(72, &[2, 20, row, 420, rows, f], &[], ""));
            }
            for tt in [-0.1, 0.0, 0.5, 1.0, 2.0] {
                v.push(call(73, &[2, 20, row, 420, rows], &[tt], ""));
            }
        }
        for x in [100, 117, 134] {
            v.push(call(70, &[2, x, 2, 1], &[], ""));
            v.push(call(71, &[2, x, 0, 0], &[], ""));
        }
    }
    // Strings and words.
    v.push(call(11, &[10, 20, 1], &[1.0, 1.0, 1.0, 0.5], "Hello, Norrath"));
    v.push(call(11, &[10, 20, 0], &[1.0, 0.0, 0.0, 1.0], "\u{4e00}\u{263a}?"));
    for w in [1, 408, 723, 731, 911, 1805, 5000] {
        v.push(call(12, &[30, 40, 1, w], &[1.0, 1.0, 1.0, 1.0], ""));
    }
    for m in 0..5 {
        v.push(call(13, &[320, 224, m, m & 1], &[0.2, 0.4, 0.6, 0.8], "centred"));
    }
    // Windows: the three sets opening and closing, frame by frame.
    for (k, set) in [(0, 0), (1, 1), (2, 2)] {
        v.push(call(20, &[k, 320, 200, 300, 4, set], &[], ""));
        v.push(call(21, &[k, 0], &[], ""));
        for _ in 0..9 {
            v.push(call(23, &[k], &[], ""));
            v.push(call(27, &[k], &[], ""));
        }
        v.push(call(22, &[k, 0], &[], ""));
        for _ in 0..7 {
            v.push(call(23, &[k], &[], ""));
            v.push(call(27, &[k], &[], ""));
        }
    }
    // TextBox: wrapping at spaces and mid-word, newlines, the flags.
    v.push(call(20, &[3, 422, 300, 462, 6, 1], &[], ""));
    v.push(call(21, &[3, 1], &[], ""));
    let long = "The quick brown fox jumps over the lazy dog, and then a verylongwordthatcannotbreakanywhereatall keeps going\nand a second paragraph after a newline, which wraps too";
    for f in [0u32, 2, 8, 0x10, 0x80, 0x0100_0000, 0x0200_0002, 0xFFFF_FFFF, 1, 9] {
        for (w, rows) in [(417, 1), (417, 3), (200, 2), (120, 6), (40, 2)] {
            v.push(call(24, &[3, 25, 1, w, rows, f as i32], &[0.0, 0.0, 0.0, 1.0], long));
        }
    }
    v.push(call(24, &[3, 25, 0, 417, 1, 0], &[1.0, 1.0, 1.0, 1.0], "short"));
    // Text and CountLines: the dialog box's two.
    for w in [360, 200, 90] {
        v.push(call(29, &[3, w], &[], long));
    }
    v.push(call(28, &[3, 20, 4], &[1.0, 1.0, 1.0, 1.0], "|xClose"));
    v.push(call(28, &[3, 20, 2], &[0.8, 0.8, 0.8, 0.5], long));
    v.push(call(24, &[3, 25, 0, 417, 1, 0], &[1.0, 1.0, 1.0, 1.0], ""));
    // Inline icons, the pulsing ones at two clocks, and widths.
    let icons = "a|tb|sc|cd|xe|>f|<g|}h|{i|aj|Ak|7l|&m|8n|*o|9p|(q|zr|Zs|vt|Vu|-dim|+back||bar|qx|ey|wz|ri|yo|iu|uo|o";
    for t in [0, 157, 400] {
        v.push(call(40, &[t], &[], ""));
        for set in [0, 1, 2, 5] {
            v.push(call(20, &[2, 300, 200, 400, 2, set], &[], ""));
            v.push(call(25, &[2, 10, 50, 0], &[1.0, 1.0, 1.0, 1.0], icons));
            v.push(call(25, &[2, 10, 50, 1], &[0.5, 0.5, 0.5, 0.7], icons));
        }
    }
    // The pointing arrow, pulsing from a reset and steady.
    v.push(call(40, &[1000], &[], ""));
    v.push(call(35, &[2], &[], ""));
    for t in [1000, 1050, 1300, 1700] {
        v.push(call(40, &[t], &[], ""));
        v.push(call(34, &[2, 40, 3, 1], &[], ""));
        v.push(call(34, &[2, 40, 3, 0], &[], ""));
    }
    v.push(call(26, &[2], &[], icons));
    v.push(call(26, &[2], &[], "plain text"));
    v.push(call(26, &[2], &[], &"x".repeat(1100)));
    // Text scrolls: a name that fits, and ones that do not, both styles.
    for (j, style) in [(0, 1), (1, 0)] {
        v.push(call(30, &[j, 107, 385, 160, 0, style], &[], ""));
        for t in 0..700 {
            let s = if t < 400 { "A Rather Long Creature Name That Scrolls" } else if t < 430 { "Short" } else { "Another quite long name for the bar, longer still" };
            v.push(call(31, &[j, 8], &[0.0, 0.0, 0.0, 1.0], s));
            v.push(call(33, &[j], &[], ""));
            if t == 650 {
                v.push(call(32, &[j], &[], ""));
            }
        }
    }
    v
}

#[test]
fn the_ui_layer_records_what_the_decomp_records() {
    let Some(mut o) = Oracle::open_at("oracle_ui") else { return };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (iso_path, support) = (root.join("run/eqoa.iso"), root.join("run/support.bin"));
    let (Ok(image), true) = (std::fs::read(&support), iso_path.exists()) else {
        eprintln!("SKIP: needs run/eqoa.iso and run/support.bin");
        return;
    };
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let textures = iso.read("TEXTURES.TXT").unwrap();
    let words = iso.read("WORDS.TXT").unwrap();
    let arial = esf::Container::open(iso.read("ARIALUI.CSF").unwrap()).unwrap();
    let font = (0..arial.nodes.len()).filter(|&i| arial.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(&arial, i)).expect("ArialUI");
    let widths: std::collections::HashMap<u16, u32> = font.glyphs.iter().map(|g| (g.code, g.width as u32)).collect();

    let mut b = Vec::new();
    b.extend((textures.len() as u32).to_le_bytes());
    b.extend(&textures);
    b.extend((words.len() as u32).to_le_bytes());
    b.extend(&words);
    b.extend((font.height as i32).to_le_bytes());
    b.extend((font.glyphs.len() as u32).to_le_bytes());
    for g in &font.glyphs {
        b.extend(g.code.to_le_bytes());
        b.extend((g.width as u16).to_le_bytes());
    }
    let r = o.call(720, &b);
    assert_eq!(i32::from_le_bytes(r[0..4].try_into().unwrap()), 0, "oracle init");

    let atlas = vi::ui::read_atlas(&image).expect("atlas");
    let side = || Side {
        vui: Vui::new(StringTable::parse(&words).unwrap(), StringTable::parse(&textures).unwrap(), atlas.clone()),
        win: Default::default(),
        scroll: Default::default(),
        studs: FrameParam::default(),
        fill: 0,
    };
    let rec = |bug| Rec { out: Vec::new(), n: 0, widths: widths.clone(), point: font.height as i32, vp: [0, 0, 639, 447], bug };
    let mut ours = side();
    let mut planted = side();
    let mut planted_differs = false;
    let calls = script();
    let (mut records, mut kinds) = (0u32, std::collections::BTreeMap::new());
    for (n, c) in calls.iter().enumerate() {
        let mut req = Vec::new();
        req.extend(c.f.to_le_bytes());
        for v in c.a {
            req.extend(v.to_le_bytes());
        }
        for v in c.x {
            req.extend(v.to_le_bytes());
        }
        req.extend((c.s.len() as u32).to_le_bytes());
        for ch in &c.s {
            req.extend(ch.to_le_bytes());
        }
        let theirs = o.call(721, &req);
        let mut r = rec(0);
        let res = ours.run(&mut r, c);
        let mut mine = res.to_le_bytes().to_vec();
        mine.extend(r.n.to_le_bytes());
        mine.extend(&r.out);
        if mine != theirs {
            let tr = i32::from_le_bytes(theirs[0..4].try_into().unwrap());
            let tn = u32::from_le_bytes(theirs[4..8].try_into().unwrap());
            let at = mine.iter().zip(&theirs).position(|(a, b)| a != b).unwrap_or(mine.len().min(theirs.len()));
            panic!(
                "step {n} (fn {} a {:?} s {:?}): ours result {res} with {} records, theirs {tr} with {tn}; first difference at byte {at} of {} / {}",
                c.f,
                &c.a[..8],
                String::from_utf16_lossy(&c.s),
                r.n,
                mine.len(),
                theirs.len()
            );
        }
        records += r.n;
        let mut p = 8;
        while p < theirs.len() {
            let kind = i32::from_le_bytes(theirs[p..p + 4].try_into().unwrap());
            *kinds.entry(kind).or_insert(0) += 1;
            p += 4 + match kind {
                1 => 40,
                3 => 48,
                4 | 5 => {
                    let fixed = if kind == 4 { 28 } else { 32 };
                    let len = u32::from_le_bytes(theirs[p + 4 + fixed..p + 8 + fixed].try_into().unwrap()) as usize;
                    fixed + 4 + len * 2
                }
                6 => 16,
                _ => panic!("kind {kind}"),
            };
        }
        let mut pr = rec(3);
        planted.run(&mut pr, c);
        planted_differs |= pr.out != r.out;
    }
    eprintln!("{} calls, {records} raster calls equal to the decomp's (by kind: {kinds:?})", calls.len());
    assert!(records > 5000);
    assert!(planted_differs, "a planted blit offset went unnoticed");
}
