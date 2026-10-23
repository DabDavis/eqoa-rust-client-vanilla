//! `VUrl` (SETUP `vurl.cpp`): the HTTP/1.1 client every download ends in,
//! and `Http_get` (`http_get.cpp`): what is done with the body (the gzip
//! header skipped, raw deflate, the trailer never checked).
//!
//! The rules, as the decomp has them (`claude-notes/go-client-patcher-20261016.md`):
//! the request is `GET /<path> HTTP/1.1\r\nHost: <server>\r\n\r\n` and
//! nothing else; the header ends at the first CR after an LF; a header line
//! that contains `Content-Length` (case-sensitively) gives the length as
//! `atoi` of the text two characters past its colon; the reply counts only
//! when that length was given and met, and reading stops once it is.

use super::Wire;

/// `VUrl::m_error` and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub code: i32,
    pub text: String,
}

impl Error {
    pub fn new(code: i32, text: impl Into<String>) -> Error {
        Error { code, text: text.into() }
    }

    /// A connection that never came up (the server's name or its port):
    /// `OpenSocket`'s 3 and 5.
    pub fn unreachable(&self) -> bool {
        matches!(self.code, 3 | 5)
    }
}

/// `VUrl::ParseURL` of `http://server[:port]/path`: the server, the port
/// (80 unless named; `atoi`, so "7000:7000" is 7000) and the path after the
/// server's slash, up to a '#', spaces as %20.
pub fn parse_url(url: &str) -> Result<(String, u16, String), Error> {
    let mut t = Strtok::new(url);
    if t.next("/") != Some("http:") {
        return Err(Error::new(1, "Unsupported protocol -1"));
    }
    let Some(host) = t.next("/") else { return Err(Error::new(3, "Cannot set server to NULL")) };
    let (server, port) = match host.find(':') {
        Some(c) => (host[..c].to_string(), crate::pregame::station::xml::atoi(&host[c + 1..]) as u16),
        None => (host.to_string(), 80),
    };
    let path = t.next("#").map_or_else(|| "/".to_string(), |p| p.replace(' ', "%20"));
    Ok((server, port, path))
}

/// The request `VUrl::Get` sends.
pub fn request(server: &str, path: &str) -> Vec<u8> {
    format!("GET /{path} HTTP/1.1\r\nHost: {server}\r\n\r\n").into_bytes()
}

/// `VUrl::Get`'s reading of the reply, fed as it comes: the header up to
/// the first CR after an LF (and past the blank line's LF), the body after
/// it, `Content-Length` read off the header once it is whole.
#[derive(Debug)]
pub struct Reply {
    header: Vec<u8>,
    last: u8,
    header_done: bool,
    /// the blank line's LF is still to come, in the next piece
    skip_lf: bool,
    length: Option<usize>,
    pub body: Vec<u8>,
    /// the buffer the PS2 reads into: a body past it fails
    cap: usize,
    overflow: bool,
}

impl Reply {
    pub fn new(cap: usize) -> Reply {
        Reply { header: Vec::new(), last: 0, header_done: false, skip_lf: false, length: None, body: Vec::new(), cap, overflow: false }
    }

    /// A piece of the reply; true once the body is whole (the length met).
    pub fn feed(&mut self, mut data: &[u8]) -> bool {
        if self.skip_lf {
            self.skip_lf = false;
            data = &data[1.min(data.len())..];
        }
        if !self.header_done {
            let mut at = None;
            for (i, &c) in data.iter().enumerate() {
                if c == b'\r' && self.last == b'\n' {
                    at = Some(i);
                    break;
                }
                self.last = c;
                self.header.push(c);
            }
            match at {
                Some(i) => {
                    self.header_done = true;
                    if i + 2 > data.len() {
                        self.skip_lf = i + 1 == data.len();
                        data = &[];
                    } else {
                        data = &data[i + 2..];
                    }
                    self.length = content_length(&self.header);
                }
                None => return false,
            }
        }
        if self.body.len() + data.len() > self.cap {
            self.overflow = true;
            return true;
        }
        self.body.extend_from_slice(data);
        self.length.is_some_and(|n| self.body.len() >= n)
    }

    /// The reply as `Get` judges it once the connection closed or the length
    /// was met: the body, or why not.
    pub fn finish(self) -> Result<Vec<u8>, Error> {
        if self.overflow {
            return Err(Error::new(6, format!("Body larger than the {}-byte buffer", self.cap)));
        }
        match self.length {
            Some(n) if self.body.len() >= n => Ok(self.body),
            Some(n) => Err(Error::new(6, format!("Short reply: {} of {n} bytes", self.body.len()))),
            None => Err(Error::new(6, "No Content-Length in the reply")),
        }
    }
}

/// The header's `Content-Length`: each line holding the words (case as
/// written) gives `atoi` of what follows its colon by two characters; the
/// last such line wins.
fn content_length(header: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(header);
    let mut n = None;
    for line in text.split(['\r', '\n']).filter(|l| !l.is_empty()) {
        if line.contains("Content-Length")
            && let Some(c) = line.find(':')
            && c + 2 <= line.len()
        {
            n = Some(crate::pregame::station::xml::atoi(&line[c + 2..]).max(0) as usize);
        }
    }
    n
}

/// `VUrl::Get` of `url` over the platform's wire, into a buffer of `cap`
/// bytes: the body, or the error `VUrl` would have set.
pub fn get(wire: &mut dyn Wire, url: &str, cap: usize, progress: &mut dyn FnMut(usize)) -> Result<Vec<u8>, Error> {
    let (server, port, path) = parse_url(url)?;
    let mut reply = Reply::new(cap);
    wire.exchange(&server, port, &request(&server, &path), &mut |piece: &[u8]| {
        let before = reply.body.len();
        let done = reply.feed(piece);
        progress(reply.body.len() - before);
        done
    })?;
    reply.finish()
}

/// C's `strtok` over one string: each call skips leading delimiters, takes
/// the token, and consumes the delimiter that ends it.
struct Strtok<'a> {
    s: &'a str,
    at: usize,
}

impl<'a> Strtok<'a> {
    fn new(s: &'a str) -> Self {
        Strtok { s, at: 0 }
    }

    fn next(&mut self, delims: &str) -> Option<&'a str> {
        let b = self.s.as_bytes();
        while self.at < b.len() && delims.as_bytes().contains(&b[self.at]) {
            self.at += 1;
        }
        if self.at >= b.len() {
            return None;
        }
        let start = self.at;
        while self.at < b.len() && !delims.as_bytes().contains(&b[self.at]) {
            self.at += 1;
        }
        let tok = &self.s[start..self.at];
        if self.at < b.len() {
            self.at += 1;
        }
        Some(tok)
    }
}

/// `Http_get`'s inflate (`HttpUncompress`, minigzip's `check_header` then
/// raw deflate): what it wrote, at most `cap` bytes; nothing for a body
/// that is not gzip's deflate (an uncompressed file is a 0-byte image, as
/// on the PS2). The trailer is never checked.
pub fn inflate(body: &[u8], cap: usize) -> Vec<u8> {
    use std::io::Read;
    let raw = skip_gzip_header(body);
    let mut out = Vec::new();
    let mut d = flate2::read::DeflateDecoder::new(raw).take(cap as u64);
    let mut buf = [0u8; 16384];
    loop {
        match d.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            // what was read before the error stays (`io.ReadFull`'s count)
            Err(_) => break,
        }
    }
    out
}

/// minigzip's `check_header`: the magic, the method (8), the flags' extra
/// field, name, comment and header CRC skipped. A body without the magic is
/// read from its start.
fn skip_gzip_header(b: &[u8]) -> &[u8] {
    if b.len() < 2 || b[0] != 0x1F || b[1] != 0x8B {
        return b;
    }
    let mut at = 2;
    let method = b.get(at).copied().unwrap_or(0);
    let flags = b.get(at + 1).copied().unwrap_or(0);
    at += 2;
    if method != 8 {
        return &b[at.min(b.len())..];
    }
    at += 6;
    if flags & 0x04 != 0 {
        let n = b.get(at).copied().unwrap_or(0) as usize | (b.get(at + 1).copied().unwrap_or(0) as usize) << 8;
        at += 2 + n;
    }
    if flags & 0x08 != 0 {
        while at < b.len() && b[at] != 0 {
            at += 1;
        }
        at += 1;
    }
    if flags & 0x10 != 0 {
        while at < b.len() && b[at] != 0 {
            at += 1;
        }
        at += 1;
    }
    if flags & 0x02 != 0 {
        at += 2;
    }
    &b[at.min(b.len())..]
}

/// `Http_get::Get_thread`'s memory fetch (the manifest, the release notes):
/// the inflated text when the body inflates (`compressed`), else the body
/// itself as a C string (the "Hello" check), so either form reads.
pub fn memory_text(body: &[u8], compressed: bool, cap: usize) -> Vec<u8> {
    let inflated = if compressed { inflate(body, cap) } else { Vec::new() };
    let src = if inflated.is_empty() { body } else { &inflated };
    let end = src.iter().position(|&c| c == 0).unwrap_or(src.len()).min(cap.saturating_sub(1));
    src[..end].to_vec()
}

/// `Http_get::Get`'s file: the inflated bytes when `compressed` (their
/// length is the file's), else nothing (length 0).
pub fn file_bytes(body: &[u8], compressed: bool, cap: usize) -> Vec<u8> {
    if compressed { inflate(body, cap) } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gz(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    /// The URL as `ParseURL` reads it, and the request as `Get` sends it:
    /// no port in Host, nothing but Host.
    #[test]
    fn the_request_is_the_ps2s() {
        let (s, p, path) = parse_url("http://patch.example.com:7000:7000/patch/eqoa/eqoa-frontierslive-update.xml.gz").unwrap();
        assert_eq!((s.as_str(), p, path.as_str()), ("patch.example.com", 7000, "patch/eqoa/eqoa-frontierslive-update.xml.gz"));
        assert_eq!(request(&s, &path), b"GET /patch/eqoa/eqoa-frontierslive-update.xml.gz HTTP/1.1\r\nHost: patch.example.com\r\n\r\n".to_vec());
        let (_, p, path) = parse_url("http://192.0.2.1/a b#frag").unwrap();
        assert_eq!((p, path.as_str()), (80, "a%20b"));
        // the control: not http
        assert!(parse_url("ftp://192.0.2.1/x").is_err());
    }

    /// Content-Length is mandatory, case-sensitive, and met; a reply split
    /// anywhere reads the same.
    #[test]
    fn content_length_is_mandatory_and_met() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: x\r\n\r\nhello".to_vec();
        for cut in 0..ok.len() {
            let mut r = Reply::new(1000);
            let a = r.feed(&ok[..cut]);
            let b = r.feed(&ok[cut..]);
            assert!(a || b, "cut at {cut}");
            assert_eq!(r.finish().unwrap(), b"hello", "cut at {cut}");
        }
        // the controls: none, lower case, short
        let mut r = Reply::new(1000);
        r.feed(b"HTTP/1.1 200 OK\r\n\r\nhello");
        assert_eq!(r.finish().unwrap_err().code, 6, "no Content-Length");
        let mut r = Reply::new(1000);
        r.feed(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello");
        assert!(r.finish().is_err(), "lower case does not count");
        let mut r = Reply::new(1000);
        r.feed(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello");
        assert!(r.finish().is_err(), "short");
        let mut r = Reply::new(3);
        r.feed(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
        assert!(r.finish().is_err(), "past the buffer");
    }

    /// Every file must be gzipped: a gzip with a name inflates, an
    /// uncompressed body is 0 bytes; the manifest reads either way.
    #[test]
    fn a_file_must_be_gzipped() {
        let data = b"the file's bytes".repeat(20);
        let mut z = gz(&data);
        // FNAME, as the server's files carry
        z[3] |= 0x08;
        z.splice(10..10, b"S2\0".iter().copied());
        assert_eq!(file_bytes(&z, true, 1 << 21), data);
        assert!(file_bytes(&data, true, 1 << 21).is_empty(), "not gzip: nothing");
        assert!(file_bytes(&z, false, 1 << 21).is_empty(), "Compressed=False: nothing");
        assert_eq!(memory_text(&gz(b"<VerantPatcher/>"), true, 0x19000), b"<VerantPatcher/>");
        assert_eq!(memory_text(b"<VerantPatcher/>\0junk", true, 0x19000), b"<VerantPatcher/>", "plain text reads as a C string");
    }
}
