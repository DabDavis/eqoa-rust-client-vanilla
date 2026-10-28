//! The GM's network readouts: `VIWndNetStat` (CLIENT `viwndnetstat.cpp`,
//! TEST > Avatar > RENDER > Toggle Stats, `m_p00084`), `DrawNetStats`
//! (TEST > Programming > SERVER LOAD, `VIClientPart382E8::m_p0C`, over the
//! server-load state channels 0x44..0x6B that `Channel44` reads) and Show
//! BPS's line (`m_p502FC`).
//!
//! `VIWndNetStat` is a meter texture of the last 64 sends and receives with
//! the ping over it; but nothing in CLIENT calls its `CreateTexture`, so its
//! material stays -1, `UpdateTexture` returns at once and the window draws
//! the ping alone, as here. The meter's pixels are kept (`meter`) as
//! `UpdateTexture` would write them, unused as on the console.

use vi::ui::{Raster, Vui, wide};

/// `VIWndNetStat`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetStat {
    /// `m_ping`
    pub ping: i32,
}

impl NetStat {
    /// `SetData(age, sent, recv, number, ping)`: histories older than two
    /// seconds' worth of ticks shifted on (for the meter, which is never
    /// made), the ping kept.
    pub fn set_data(&mut self, age: i64, sent: u64, recv: u64, number: u16, ping: i32) -> (u64, u64, i32) {
        let (mut sent, mut recv, mut number) = (sent, recv, number);
        if age > 200 {
            let n = age / 100 - 1;
            number = number.wrapping_add(n as u16);
            // dsllv: the shift is taken modulo 64
            sent = sent.wrapping_shl(n as u32);
            recv = recv.wrapping_shl(n as u32);
        }
        self.ping = ping;
        (sent, recv, (number & 7) as i32)
    }

    /// `OnDrawSelf`: the ping right-aligned at 624, a font height above the
    /// meter's place (496, 416), which has no material to blit.
    pub fn draw(&self, vui: &Vui, r: &mut dyn Raster) {
        let s = wide(&self.ping.to_string());
        let x = 624 - vui.string_width(r, &s);
        let y = 424 - 8 - vui.font_height(r);
        r.draw_string([x, y], [1.0; 4], &s);
    }
}

/// `UpdateTexture`'s 4-bit pixels (`VIPixelStoreI4`, 128 x 8 of the 128 x 32
/// surface, 15 lit): row 0 eight ticks either side of the middle, `phase`
/// pixels on from the frame's; row 1 a dot per bit of each history (the
/// sends right to left from bit 64, which `dsllv` takes as bit 0, the
/// receives left to right from bit 0, past the middle); rows 2..6 copy row 1
/// and row 7 row 0. Drawn by nothing, as on the console: `CreateTexture`
/// has no caller in CLIENT, so the meter's material stays -1.
#[allow(dead_code)]
pub fn meter(sent: u64, recv: u64, phase: i32) -> [[u8; 128]; 8] {
    let mut rows = [[0u8; 128]; 8];
    for i in 0..8 {
        let x = 64 - phase - i * 8;
        rows[0][x.rem_euclid(128) as usize] = 15;
        rows[0][(128 - x).rem_euclid(128) as usize] = 15;
    }
    for i in 0..64u32 {
        if sent & 1u64.wrapping_shl(64 - i) != 0 {
            rows[1][i as usize] = 15;
        }
        if recv & (1u64 << i) != 0 {
            rows[1][(i + 64) as usize] = 15;
        }
    }
    for k in 2..7 {
        rows[k] = rows[1];
    }
    rows[7] = rows[0];
    rows
}

/// One of the 40 records `Channel44` reads (`VIChannelRec`, state channels
/// 0x44..0x6B): what each counter is the decomp does not know; DrawNetStats
/// prints them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelRec {
    pub id: u64,
    pub p08: i32,
    pub p0c: i32,
    pub p10: i32,
    /// bits a second
    pub p14: i32,
    pub p18: i32,
    /// `m_ping`: the ring's head and its 64 samples
    pub head: u8,
    pub samples: [u16; 64],
}

impl ChannelRec {
    /// `Channel44`'s reads: an eight-byte id, five words, the ring's head
    /// byte and its 64 samples. `None` for an empty channel (`m_size` 0).
    pub fn read(data: &[u8]) -> Option<ChannelRec> {
        if data.is_empty() {
            return None;
        }
        let mut r = net::stream::Reader::new(data);
        let id = r.u64();
        let mut w = [0i32; 5];
        for v in &mut w {
            *v = r.u32() as i32;
        }
        let head = r.u8();
        let mut samples = [0u16; 64];
        for s in &mut samples {
            *s = r.u16();
        }
        Some(ChannelRec { id, p08: w[0], p0c: w[1], p10: w[2], p14: w[3], p18: w[4], head, samples })
    }

    /// `VIPingRing::Last`: the sample before the head.
    pub fn last(&self) -> u16 {
        let i = self.head as i32 - 1;
        let i = if i < 0 { self.head as i32 + 63 } else { i };
        self.samples[(i as usize).min(63)]
    }

    /// `VIPingRing::Max`: of `m_samples[0]`, 64 times, as shipped.
    pub fn max(&self) -> u16 {
        self.samples[0]
    }
}

/// `DrawNetStats`' lines: each record that came, the worst ping first, then
/// the larger second counter (`NetStatSort`; a tie keeps the channels'
/// order, where newlib's qsort may not), as "%02u: %05ue %03up %04ums
/// s%04ukbps r%04ukbps" in its channel's colour.
pub fn net_lines(recs: &[Option<ChannelRec>]) -> Vec<(String, [f32; 4])> {
    const COLORS: [[f32; 4]; 8] = [[1.0, 1.0, 0.0, 1.0], [1.0, 0.0, 1.0, 1.0], [0.0, 1.0, 1.0, 1.0], [0.7, 0.7, 0.7, 1.0], [0.7, 0.7, 1.0, 1.0], [0.7, 1.0, 0.7, 1.0], [1.0, 0.7, 0.7, 1.0], [1.0, 1.0, 1.0, 1.0]];
    let mut stats: Vec<(usize, &ChannelRec)> = recs.iter().enumerate().filter_map(|(i, r)| r.as_ref().map(|r| (i, r))).collect();
    stats.sort_by(|a, b| b.1.max().cmp(&a.1.max()).then((b.1.p0c as u32).cmp(&(a.1.p0c as u32))));
    stats
        .into_iter()
        .map(|(ch, r)| {
            let s = format!("{:02}: {:05}e {:03}p {:04}ms s{:04}kbps r{:04}kbps", r.p08 as u32, r.p0c as u32, r.p10 as u32, r.last(), r.p14 as u32 / 1000, r.p18 as u32 / 1000);
            (s, COLORS[ch % 8])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window draws the ping alone, right-aligned at 624; `SetData`
    /// shifts histories older than 200 ticks on by their whole hundreds,
    /// less one, the shift modulo 64.
    #[test]
    fn the_ping_and_set_data() {
        let mut n = NetStat::default();
        assert_eq!(n.set_data(150, 0b1011, 0b1, 9, 77), (0b1011, 0b1, 1));
        assert_eq!(n.ping, 77);
        assert_eq!(n.set_data(450, 0b1011, 0b1, 9, 5), (0b1011 << 3, 0b1000, 4), "450 ticks: on by 3");
        assert_eq!(n.set_data(6600, 1, 1, 0, 5), (1 << 1, 1 << 1, 1), "65 is 1 modulo 64");
        // the control: at 200 nothing moves
        assert_eq!(n.set_data(200, 0b1011, 0b1, 9, 5), (0b1011, 0b1, 1));
    }

    /// `UpdateTexture`'s ticks and dots: the middle's ticks 8 apart from the
    /// phase, the sends' bit 64 (`dsllv`: bit 0) first, the receives after
    /// the middle; rows 2..6 copy row 1, row 7 row 0.
    #[test]
    fn the_meter_as_update_texture_writes_it() {
        let m = meter(1 | 1 << 63, 1 << 2, 3);
        assert_eq!(m[1][0], 15, "bit 64 is taken as bit 0");
        assert_eq!(m[1][1], 15, "bit 63 is the second pixel");
        assert_eq!(m[1][64 + 2], 15, "receive bit 2 after the middle");
        assert_eq!(m[1].iter().filter(|&&p| p != 0).count(), 3);
        let ticks: Vec<usize> = (0..128).filter(|&x| m[0][x] != 0).collect();
        assert_eq!(ticks, vec![5, 13, 21, 29, 37, 45, 53, 61, 67, 75, 83, 91, 99, 107, 115, 123]);
        assert!((2..7).all(|k| m[k] == m[1]) && m[7] == m[0]);
        // the control: phase 0 moves every tick
        assert_ne!(meter(0, 0, 0)[0], m[0]);
    }

    /// `Channel44`'s record and DrawNetStats' order and text: the worst
    /// `Max` first (sample 0, as shipped), then the larger second counter.
    #[test]
    fn server_load_lines() {
        let rec = |p0c: u32, s0: u16, head: u8| {
            let mut d = 0x1122_3344_5566_7788u64.to_le_bytes().to_vec();
            for v in [7u32, p0c, 12, 64_000, 3_500] {
                d.extend(v.to_le_bytes());
            }
            d.push(head);
            for k in 0..64u16 {
                d.extend((if k == 0 { s0 } else { 900 + k }).to_le_bytes());
            }
            d
        };
        let a = ChannelRec::read(&rec(5, 40, 3)).unwrap();
        assert_eq!((a.id, a.p08, a.p0c, a.p10, a.p14, a.p18, a.head), (0x1122_3344_5566_7788, 7, 5, 12, 64_000, 3_500, 3));
        assert_eq!((a.last(), a.max()), (902, 40), "Max reads sample 0 only");
        assert_eq!(ChannelRec::read(&rec(1, 0, 0)).unwrap().last(), 963, "head 0: the ring's last");
        assert!(ChannelRec::read(&[]).is_none());
        let mut recs = vec![None; 40];
        recs[0] = Some(a);
        recs[3] = ChannelRec::read(&rec(9, 40, 3));
        recs[9] = ChannelRec::read(&rec(1, 80, 1));
        let lines = net_lines(&recs);
        assert_eq!(lines.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(), vec![
            "07: 00001e 012p 0080ms s0064kbps r0003kbps",
            "07: 00009e 012p 0902ms s0064kbps r0003kbps",
            "07: 00005e 012p 0902ms s0064kbps r0003kbps",
        ]);
        assert_eq!((lines[0].1, lines[1].1), ([1.0, 0.0, 1.0, 1.0], [0.7, 0.7, 0.7, 1.0]), "channels 9 and 3: colours 1 and 3");
        // the control: by the newest ping instead, the order differs
        let mut by_last: Vec<_> = recs.iter().flatten().collect();
        by_last.sort_by_key(|r| std::cmp::Reverse(r.last()));
        assert_ne!(by_last[0].p0c, 1);
    }
}
