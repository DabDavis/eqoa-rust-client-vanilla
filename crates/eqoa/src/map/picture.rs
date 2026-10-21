//! The map's pictures as the cache keeps them: PNG, RGB (they are opaque),
//! compressed fast (`png`'s fdeflate); and the overview's cell out of a
//! tile, a quarter across.

/// RGBA rows (`w` x `h`) as an RGB PNG.
pub fn encode(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let ok = enc.write_header().and_then(|mut wr| {
        wr.write_image_data(&rgb)?;
        wr.finish()
    });
    if ok.is_ok() { out } else { Vec::new() }
}

/// A PNG made a few rows at a time (`RowEncoder::rows`), so making one of a
/// tile does not hold up a frame: RGB, compressed fast, as `encode` makes it.
pub struct RowEncoder {
    stream: Option<png::StreamWriter<'static, Shared>>,
    out: Rc<RefCell<Vec<u8>>>,
    w: u32,
    h: u32,
    row: u32,
}

use std::cell::RefCell;
use std::rc::Rc;

/// The PNG's bytes as the stream writes them.
struct Shared(Rc<RefCell<Vec<u8>>>);

impl std::io::Write for Shared {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl RowEncoder {
    pub fn new(w: u32, h: u32) -> Option<Self> {
        let out = Rc::new(RefCell::new(Vec::new()));
        let mut enc = png::Encoder::new(Shared(out.clone()), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let stream = enc.write_header().ok()?.into_stream_writer().ok()?;
        Some(RowEncoder { stream: Some(stream), out, w, h, row: 0 })
    }

    /// Up to `rows` more of `rgba`'s rows in; whether every row is.
    pub fn rows(&mut self, rgba: &[u8], rows: u32) -> bool {
        use std::io::Write;
        let end = (self.row + rows).min(self.h);
        let (a, b) = ((self.row * self.w * 4) as usize, (end * self.w * 4) as usize);
        let rgb: Vec<u8> = rgba[a..b].as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        if let Some(s) = self.stream.as_mut()
            && s.write_all(&rgb).is_err()
        {
            self.stream = None;
        }
        self.row = end;
        self.row >= self.h
    }

    /// The PNG, once every row is in (empty if anything failed).
    pub fn finish(mut self) -> Vec<u8> {
        let ok = self.row >= self.h && self.stream.take().is_some_and(|s| s.finish().is_ok());
        if ok { std::mem::take(&mut *self.out.borrow_mut()) } else { Vec::new() }
    }
}

/// A PNG's RGBA rows, when it is an 8-bit RGB or RGBA `w` x `h` picture.
pub fn decode(bytes: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut r = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    if (info.width, info.height) != (w, h) || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let px = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgb => Some(px.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()),
        png::ColorType::Rgba => Some(px.to_vec()),
        _ => None,
    }
}

/// A tile's overview cell: a quarter of it across (two levels down).
pub fn quarter(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let (w1, h1, a) = render::map::half(w, h, rgba);
    render::map::half(w1, h1, &a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture comes back from its PNG as it went in, opaque; a PNG of
    /// another size is refused. The control: a byte of the PNG's pixels
    /// changed changes what comes back.
    #[test]
    fn a_picture_keeps_through_its_png() {
        let (w, h) = (16u32, 8u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7) as u8, (i * 3) as u8, 200, 255]).collect();
        let png = encode(w, h, &rgba);
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(decode(&png, w, h).as_deref(), Some(rgba.as_slice()));
        assert_eq!(decode(&png, w, h + 1), None, "another size");
        assert_eq!(decode(b"not a png", w, h), None);
        let other: Vec<u8> = rgba.iter().enumerate().map(|(k, &b)| if k == 4 { b ^ 1 } else { b }).collect();
        assert_ne!(decode(&encode(w, h, &other), w, h).as_deref(), Some(rgba.as_slice()), "the control");
        let (qw, qh, q) = quarter(w, h, &rgba);
        assert_eq!((qw, qh, q.len()), (4, 2, 32));
    }

    /// A PNG made three rows at a time is the picture; one left short of its
    /// last row is none. The control: the rows given out of order.
    #[test]
    fn a_png_made_a_few_rows_at_a_time_is_the_picture() {
        let (w, h) = (16u32, 8u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i * 5) as u8, (i * 11) as u8, (i * 13) as u8, 255]).collect();
        let mut e = RowEncoder::new(w, h).unwrap();
        let mut steps = 0;
        while !e.rows(&rgba, 3) {
            steps += 1;
        }
        assert_eq!(steps, 2, "8 rows, 3 a step: done in the third");
        assert_eq!(decode(&e.finish(), w, h).as_deref(), Some(rgba.as_slice()));
        let mut short = RowEncoder::new(w, h).unwrap();
        short.rows(&rgba, 7);
        assert!(short.finish().is_empty());
        let swapped: Vec<u8> = [&rgba[64..], &rgba[..64]].concat();
        let mut s = RowEncoder::new(w, h).unwrap();
        while !s.rows(&swapped, 3) {}
        assert_ne!(decode(&s.finish(), w, h).as_deref(), Some(rgba.as_slice()), "the control");
    }
}
