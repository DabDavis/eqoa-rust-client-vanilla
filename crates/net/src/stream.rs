//! `VIMemStream`'s encodings: little-endian raw values, zigzag 7-bit varints
//! (`ReadSigned`/`WriteSigned`), and strings as a u32 length then the bytes.

#[derive(Debug)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    /// Set by any read past the end (`VIMemStream`'s error flag).
    pub error: bool,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0, error: false }
    }

    fn take(&mut self, n: usize) -> &'a [u8] {
        if self.pos + n > self.data.len() {
            self.error = true;
            self.pos = self.data.len();
            return &[];
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        s
    }

    pub fn u8(&mut self) -> u8 {
        self.take(1).first().copied().unwrap_or(0)
    }
    pub fn u16(&mut self) -> u16 {
        self.take(2).try_into().map(u16::from_le_bytes).unwrap_or(0)
    }
    pub fn u32(&mut self) -> u32 {
        self.take(4).try_into().map(u32::from_le_bytes).unwrap_or(0)
    }
    pub fn u64(&mut self) -> u64 {
        self.take(8).try_into().map(u64::from_le_bytes).unwrap_or(0)
    }
    pub fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    pub fn bytes(&mut self, n: usize) -> &'a [u8] {
        self.take(n)
    }

    /// The 7-bit varint, low group first.
    pub fn varint(&mut self) -> u64 {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let c = self.u8();
            v |= ((c & 0x7f) as u64) << shift;
            if c & 0x80 == 0 || self.error || shift >= 63 {
                return v;
            }
            shift += 7;
        }
    }

    /// `ReadSigned`: zigzag over the varint.
    pub fn signed(&mut self) -> i64 {
        let v = self.varint();
        ((v >> 1) as i64) ^ -((v & 1) as i64)
    }

    /// `ReadString(buf, max)`: a length of `max` or more is skipped and read
    /// as empty.
    pub fn string(&mut self, max: usize) -> Vec<u8> {
        let n = self.u32() as usize;
        let s = self.take(n);
        if n >= max { Vec::new() } else { s.to_vec() }
    }

    /// A wide string: a u32 character count, then UTF-16LE.
    pub fn wide(&mut self, max: usize) -> String {
        let n = self.u32() as usize;
        let s = self.take(n * 2);
        if n >= max {
            return String::new();
        }
        String::from_utf16_lossy(&s.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
    }
}

#[derive(Debug, Default)]
pub struct Writer {
    pub data: Vec<u8>,
}

impl Writer {
    pub fn op(op: u16) -> Self {
        Writer { data: op.to_le_bytes().to_vec() }
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.data.push(v);
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.data.extend(v.to_le_bytes());
        self
    }
    /// `WriteU64`: eight bytes, little end first.
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.data.extend(v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.data.extend(v);
        self
    }
    pub fn varint(&mut self, mut v: u64) -> &mut Self {
        loop {
            let c = (v & 0x7f) as u8;
            v >>= 7;
            self.data.push(if v != 0 { c | 0x80 } else { c });
            if v == 0 {
                return self;
            }
        }
    }
    pub fn signed(&mut self, v: i64) -> &mut Self {
        self.varint(((v << 1) ^ (v >> 63)) as u64)
    }
    /// The wide `WriteString`: a u32 character count, then UTF-16LE.
    pub fn wide(&mut self, s: &str) -> &mut Self {
        let u: Vec<u16> = s.encode_utf16().collect();
        self.u32(u.len() as u32);
        for c in u {
            self.data.extend(c.to_le_bytes());
        }
        self
    }

    /// `WriteString`: a u32 length and the bytes, no terminator.
    pub fn string(&mut self, s: &[u8]) -> &mut Self {
        self.u32(s.len() as u32).bytes(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag() {
        for v in [0i64, 1, -1, 7, -64, 300, -100000] {
            let mut w = Writer::default();
            w.signed(v);
            assert_eq!(Reader::new(&w.data).signed(), v);
        }
        let mut w = Writer::default();
        w.signed(7);
        assert_eq!(w.data, [0x0e], "seven characters are counted as 0x0E");
    }
}
