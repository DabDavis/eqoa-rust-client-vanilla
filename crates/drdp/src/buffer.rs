//! `buffer.cpp`: the wire buffer. Little-endian on both ends, so it never
//! swaps. The overflow rules are the decomp's to the letter: a single read or
//! append past the end sets `overflow`; the array forms (`readm`/`appendm`)
//! do NOTHING past the end and set nothing; an append to a read-only buffer is
//! dropped but the array append ignores `read_only`. A failed read returns 0
//! where the original leaves the destination as it was.

#[derive(Clone, Debug, Default)]
pub struct Buffer {
    pub data: Vec<u8>,
    /// `size`: the read or write position.
    pub pos: usize,
    pub max: usize,
    pub overflow: bool,
    pub read_only: bool,
}

impl Buffer {
    /// `buffer_init`: an empty buffer that can take `max` bytes.
    pub fn new(max: usize) -> Self {
        Buffer { data: vec![0; max], pos: 0, max, overflow: false, read_only: false }
    }

    /// `buffer_init_read` over a copy of `data`.
    pub fn reader(data: &[u8]) -> Self {
        Buffer { data: data.to_vec(), pos: 0, max: data.len(), overflow: false, read_only: true }
    }

    /// What has been written.
    pub fn written(&self) -> &[u8] {
        &self.data[..self.pos]
    }

    /// `buffer_bytes_remaining`: -1 once overflowed.
    pub fn remaining(&self) -> i32 {
        if self.overflow { -1 } else { self.max as i32 - self.pos as i32 }
    }

    fn read_bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut v = [0u8; N];
        if !(self.pos + N > self.max || self.overflow) {
            v.copy_from_slice(&self.data[self.pos..self.pos + N]);
            self.pos += N;
        } else {
            self.overflow = true;
        }
        v
    }

    fn append_bytes(&mut self, v: &[u8]) {
        if self.read_only {
            return;
        }
        if !(self.pos + v.len() > self.max || self.overflow) {
            self.data[self.pos..self.pos + v.len()].copy_from_slice(v);
            self.pos += v.len();
        } else {
            self.overflow = true;
        }
    }

    pub fn u8(&mut self) -> u8 {
        self.read_bytes::<1>()[0]
    }
    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.read_bytes())
    }
    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.read_bytes())
    }
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.read_bytes())
    }
    pub fn put_u8(&mut self, v: u8) {
        self.append_bytes(&[v]);
    }
    pub fn put_u16(&mut self, v: u16) {
        self.append_bytes(&v.to_le_bytes());
    }
    pub fn put_u32(&mut self, v: u32) {
        self.append_bytes(&v.to_le_bytes());
    }
    pub fn put_u64(&mut self, v: u64) {
        self.append_bytes(&v.to_le_bytes());
    }

    /// `buffer_readm_uns8`: nothing at all if it does not fit.
    pub fn read_into(&mut self, out: &mut [u8]) {
        if self.pos + out.len() > self.max || self.overflow {
            return;
        }
        out.copy_from_slice(&self.data[self.pos..self.pos + out.len()]);
        self.pos += out.len();
    }

    /// `buffer_appendm_uns8`: nothing at all if it does not fit, and
    /// `read_only` is not consulted.
    pub fn put_bytes(&mut self, v: &[u8]) {
        if self.pos + v.len() > self.max || self.overflow {
            return;
        }
        self.data[self.pos..self.pos + v.len()].copy_from_slice(v);
        self.pos += v.len();
    }

    /// `buffer_skip`.
    pub fn skip(&mut self, n: usize) {
        if self.pos + n <= self.max && !self.overflow {
            self.pos += n;
        } else {
            self.overflow = true;
        }
    }

    /// The unread rest (`buffer_get_position_ptr` with the remaining size).
    pub fn rest(&self) -> &[u8] {
        &self.data[self.pos.min(self.max)..self.max]
    }

    /// `buffer_copy(from = self, to, n)`: the source advances even when the
    /// destination cannot take the bytes.
    pub fn copy_to(&mut self, to: &mut Buffer, n: usize) {
        if self.pos + n <= self.max {
            if to.pos + n <= to.max {
                to.data[to.pos..to.pos + n].copy_from_slice(&self.data[self.pos..self.pos + n]);
                self.pos += n;
                to.pos += n;
            } else {
                self.pos += n;
                to.overflow = true;
            }
        } else {
            self.overflow = true;
            to.overflow = true;
        }
    }

    /// `buffer_append_packed_uns64`: the 7-bit varint, low group first.
    pub fn put_packed(&mut self, mut v: u64) {
        loop {
            let c = v as u8;
            v >>= 7;
            let mut b = c & 0x7f;
            if v != 0 {
                b |= 0x80;
            }
            self.put_u8(b);
            if v == 0 {
                break;
            }
        }
    }

    /// `buffer_read_packed_uns64`: bounded at 64 bits of shift.
    pub fn packed(&mut self) -> u64 {
        let mut v = 0u64;
        let mut shift = 0u32;
        while shift < 64 {
            let c = self.u8();
            v |= ((c & 0x7f) as u64) << shift;
            if c & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        v
    }

    /// `buffer_append_packed_int32`: zigzag computed in 32 bits, then widened
    /// SIGNED, so an overflowing value goes out as a ten-byte varint.
    pub fn put_packed_i32(&mut self, mut v: i32) {
        let mut sign = 0;
        if v < 0 {
            v = !v;
            sign = 1;
        }
        v = v.wrapping_shl(1) | sign;
        self.put_packed(v as i64 as u64);
    }

    pub fn packed_i32(&mut self) -> i32 {
        let v = self.packed();
        let mut r = (v >> 1) as i32;
        if v & 1 != 0 {
            r = !r;
        }
        r
    }

    /// `buffer_append_offset_uns32`: the value minus the offset.
    pub fn put_offset_u32(&mut self, v: u32, off: u32) {
        self.put_u32(v.wrapping_sub(off));
    }

    /// `buffer_append_string`: bytes and a terminating zero.
    pub fn put_string(&mut self, s: &[u8]) {
        if !s.is_empty() && s[0] != 0 && !self.overflow {
            for &c in s.iter().take_while(|&&c| c != 0) {
                self.put_u8(c);
                if self.overflow {
                    break;
                }
            }
        }
        self.put_u8(0);
    }

    /// `buffer_append_size`: one byte under 255, else 255 and a u16.
    pub fn put_size(&mut self, n: u16) {
        if n < 255 {
            self.put_u8(n as u8);
        } else {
            self.put_u8(255);
            self.put_u16(n);
        }
    }

    pub fn size(&mut self) -> u16 {
        let c = self.u8();
        if c == 255 { self.u16() } else { c as u16 }
    }
}

/// `run_length_encode`: alternating runs of zeros (to 255) and non-zeros (to
/// 127); one header byte `nonzeros << 4 | zeros` when both are small, else
/// `0x80 | nonzeros` then `zeros`. A zero short header ends the stream.
pub fn rle_encode(data: &[u8], out: &mut Buffer) {
    let mut p = 0;
    let end = data.len();
    loop {
        let mut zeros = 0usize;
        let mut nonzeros = 0usize;
        while p + zeros < end && data[p + zeros] == 0 && zeros != 255 {
            zeros += 1;
        }
        p += zeros;
        while p + nonzeros < end && data[p + nonzeros] != 0 && nonzeros != 127 {
            nonzeros += 1;
        }
        if nonzeros >= 8 || zeros >= 16 {
            out.put_u8((nonzeros | 0x80) as u8);
            out.put_u8(zeros as u8);
        } else {
            let c = ((nonzeros << 4) | zeros) as u8;
            out.put_u8(c);
            if c == 0 {
                break;
            }
        }
        out.put_bytes(&data[p..p + nonzeros]);
        p += nonzeros;
    }
}

/// `run_length_decode` into `data` (its length is the decoded size). The
/// original loops for ever on a stream that overflows without its
/// terminator (the header read leaves the last header in place); this stops.
pub fn rle_decode(input: &mut Buffer, data: &mut [u8]) {
    let mut out = Buffer::new(data.len());
    loop {
        let c = input.u8();
        if c == 0 || input.overflow {
            break;
        }
        let (nonzeros, zeros) = if c & 0x80 != 0 { ((c & 0x7f) as usize, input.u8() as usize) } else { ((c >> 4) as usize, (c & 0xf) as usize) };
        for _ in 0..zeros {
            out.put_u8(0);
        }
        input.copy_to(&mut out, nonzeros);
    }
    data.copy_from_slice(&out.data);
}
