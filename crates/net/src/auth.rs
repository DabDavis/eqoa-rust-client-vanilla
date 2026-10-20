//! The Station login (STATION's `VIStationAPI`): TCP, big-endian frames of
//! `[u32 length incl. itself][u32 type][payload]`.
//!
//! The server opens with a 40-byte Hello (type 4: 32 bytes the client
//! DES-decrypts into a billing key it never uses for login). The client sends
//! the 148-byte Login Request (type 36) with its password DES-CBC encrypted
//! under `des_string_to_key("LPSS-V1.0$@Z>.ADJ")`, the key doubling as the
//! IV; the server answers with the 168-byte type 37 (`VIStationReply37`):
//! `result` at 8 must be 0, a text at 12 (64 bytes; STATION never shows it,
//! its refusal is word 1394), the account id at 80, its status at 84, the game
//! features at 164.
//!
//! What the game connection carries forward is not from the reply: STATION
//! hands CLIENT the password AES-256-ECB encrypted under a fixed key, and the
//! UDP login sends that token in place of the password.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// `des_string_to_key("LPSS-V1.0$@Z>.ADJ")`.
const DES_KEY: [u8; 8] = [0xdf, 0xc2, 0x04, 0x6b, 0x49, 0x83, 0xfd, 0x6b];

const AES_KEY: [u8; 32] = [
    0xec, 0xca, 0xbf, 0x71, 0xf4, 0xbf, 0x32, 0x6b, 0x57, 0x29, 0x72, 0x4d, 0xc1, 0xd7, 0xaa, 0xac, 0x53, 0x84, 0x37, 0xda, 0x68, 0xe4, 0x3b, 0xb4, 0xa5,
    0xe4, 0xa4, 0xe9, 0xd4, 0x89, 0x6d, 0x44,
];

#[derive(Clone, Debug)]
pub struct Login {
    pub account: u32,
    pub status: u32,
    pub game_features: u32,
    /// The UDP login's token: the password, AES-256-ECB encrypted.
    pub token: [u8; 32],
}

fn pad32(s: &str) -> [u8; 32] {
    let mut b = [0u8; 32];
    let n = s.len().min(31);
    b[..n].copy_from_slice(&s.as_bytes()[..n]);
    b
}

/// `des_soe_encrypt`: DES-CBC, the key as IV.
pub fn encrypt_password(password: &str) -> [u8; 32] {
    use des::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
    let c = des::Des::new(GenericArray::from_slice(&DES_KEY));
    let mut b = pad32(password);
    let mut iv = DES_KEY;
    for block in b.chunks_mut(8) {
        for (x, v) in block.iter_mut().zip(iv) {
            *x ^= v;
        }
        c.encrypt_block(GenericArray::from_mut_slice(block));
        iv.copy_from_slice(block);
    }
    b
}

/// The UDP token: AES-256-ECB of the zero-padded password.
pub fn udp_token(password: &str) -> [u8; 32] {
    use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
    let c = aes::Aes256::new(GenericArray::from_slice(&AES_KEY));
    let mut b = pad32(password);
    for block in b.chunks_mut(16) {
        c.encrypt_block(GenericArray::from_mut_slice(block));
    }
    b
}

/// `ValidateLogin`'s request.
pub fn login_request(user: &str, password: &str) -> [u8; 148] {
    let mut r = [0u8; 148];
    r[0..4].copy_from_slice(&148u32.to_be_bytes());
    r[4..8].copy_from_slice(&36u32.to_be_bytes());
    r[12..16].copy_from_slice(&2u32.to_be_bytes());
    let u = user.as_bytes();
    r[16..16 + u.len().min(31)].copy_from_slice(&u[..u.len().min(31)]);
    r[48..80].copy_from_slice(&encrypt_password(password));
    r[80..84].copy_from_slice(&1u32.to_be_bytes());
    r[84..88].copy_from_slice(b"EQOA");
    r
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The server answered with a type other than the one expected.
    Unexpected(u32),
    /// `result` was not 0 (the game shows word 1394), with the reply's text
    /// (empty when the server gave none).
    Refused(u32, String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "auth: {e}"),
            Error::Unexpected(t) => write!(f, "auth: unexpected message type {t:#x}"),
            Error::Refused(_, t) if !t.is_empty() => write!(f, "{t}"),
            Error::Refused(r, _) => write!(f, "auth: login refused (result {r})"),
        }
    }
}

impl std::error::Error for Error {}

/// The exchange without a socket, so anything that carries the stream can
/// drive it (a TCP connection here, a browser's relayed WebSocket): hand it
/// the bytes as they arrive and send what it returns.
pub struct Exchange {
    user: String,
    password: String,
    buf: Vec<u8>,
    sent: bool,
}

/// What the exchange wants next.
#[derive(Debug)]
pub enum Step {
    /// Send these bytes (the Login Request, after the Hello).
    Send(Vec<u8>),
    /// Wait for more bytes.
    Wait,
    Done(Login),
}

impl Exchange {
    pub fn new(user: &str, password: &str) -> Self {
        Exchange { user: user.into(), password: password.into(), buf: Vec::new(), sent: false }
    }

    /// How many more bytes the next step needs (a stream read can ask for
    /// exactly that).
    pub fn wants(&self) -> usize {
        match (self.sent, self.buf.len()) {
            (false, n) => 40usize.saturating_sub(n),
            (true, n) if n < 8 => 8 - n,
            (true, n) => 168usize.saturating_sub(n),
        }
    }

    /// Bytes arrived: the Hello (type 4) is answered with the request, the
    /// reply (type 37) ends it.
    pub fn feed(&mut self, data: &[u8]) -> Result<Step, Error> {
        self.buf.extend_from_slice(data);
        let ty = |b: &[u8]| u32::from_be_bytes(b[4..8].try_into().unwrap());
        if !self.sent {
            if self.buf.len() < 40 {
                return Ok(Step::Wait);
            }
            let t = ty(&self.buf);
            if t != 4 {
                return Err(Error::Unexpected(t));
            }
            self.buf.drain(..40);
            self.sent = true;
            return Ok(Step::Send(login_request(&self.user, &self.password).to_vec()));
        }
        if self.buf.len() < 8 {
            return Ok(Step::Wait);
        }
        let t = ty(&self.buf);
        if t != 37 {
            return Err(Error::Unexpected(t));
        }
        if self.buf.len() < 168 {
            return Ok(Step::Wait);
        }
        let r = &self.buf[..168];
        let be = |o: usize| u32::from_be_bytes(r[o..o + 4].try_into().unwrap());
        if be(8) != 0 {
            let t = &r[12..76];
            let t = &t[..t.iter().position(|&b| b == 0).unwrap_or(t.len())];
            return Err(Error::Refused(be(8), String::from_utf8_lossy(t).into_owned()));
        }
        Ok(Step::Done(Login { account: be(80), status: be(84), game_features: be(164), token: udp_token(&self.password) }))
    }
}

/// The whole exchange against `addr` (the auth server, port 9735).
pub fn login(addr: impl ToSocketAddrs, user: &str, password: &str) -> Result<Login, Error> {
    let addr = addr.to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other("no address"))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut x = Exchange::new(user, password);
    loop {
        let mut buf = vec![0u8; x.wants()];
        s.read_exact(&mut buf)?;
        match x.feed(&buf)? {
            Step::Send(b) => s.write_all(&b)?,
            Step::Wait => {}
            Step::Done(l) => return Ok(l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_layout() {
        let r = login_request("dab", "x");
        assert_eq!(&r[0..8], &[0, 0, 0, 148, 0, 0, 0, 36]);
        assert_eq!(&r[16..19], b"dab");
        assert_eq!(&r[84..88], b"EQOA");
        // CBC: an all-zero second block still encrypts to something else.
        assert_ne!(&r[56..64], &[0; 8]);
    }

    fn frame(ty: u32, len: usize) -> Vec<u8> {
        let mut b = vec![0u8; len];
        b[0..4].copy_from_slice(&(len as u32).to_be_bytes());
        b[4..8].copy_from_slice(&ty.to_be_bytes());
        b
    }

    #[test]
    fn the_exchange_a_byte_at_a_time() {
        let mut x = Exchange::new("dab", "x");
        let mut steps = Vec::new();
        let mut reply = frame(37, 168);
        reply[80..84].copy_from_slice(&7u32.to_be_bytes());
        for b in frame(4, 40).into_iter().chain(reply) {
            match x.feed(&[b]).unwrap() {
                Step::Wait => {}
                s => steps.push(s),
            }
        }
        assert!(matches!(&steps[0], Step::Send(b) if b[..] == login_request("dab", "x")[..]));
        assert!(matches!(&steps[1], Step::Done(l) if l.account == 7 && l.token == udp_token("x")));
        assert_eq!(steps.len(), 2);
        // a refusal, and a reply of the wrong type
        let mut x = Exchange::new("dab", "x");
        x.feed(&frame(4, 40)).unwrap();
        let mut refused = frame(37, 168);
        refused[8..12].copy_from_slice(&1u32.to_be_bytes());
        refused[12..38].copy_from_slice(b"You are already logged in.");
        match x.feed(&refused) {
            Err(e @ Error::Refused(1, _)) => assert_eq!(e.to_string(), "You are already logged in."),
            r => panic!("{r:?}"),
        }
        // control: text with result 0 is no refusal (OnValidateLogin tests
        // the result alone)
        let mut x = Exchange::new("dab", "x");
        x.feed(&frame(4, 40)).unwrap();
        let mut ok = frame(37, 168);
        ok[12..16].copy_from_slice(b"note");
        assert!(matches!(x.feed(&ok), Ok(Step::Done(_))));
        let mut x = Exchange::new("dab", "x");
        assert!(matches!(x.feed(&frame(0x43, 40)), Err(Error::Unexpected(0x43))));
    }
}
