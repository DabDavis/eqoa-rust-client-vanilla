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

// ---- the rest of VIStationAPI: every request STATION's pages send, and the
// replies `OnMessage` dispatches (vistationapi.cpp). A request is built whole,
// header included; a reply is handed over as `VITCPConnection::Process`
// delivers it, the message less its length word, so its type is at 0 and the
// decomp's reply structs' first field after their 8-byte header is at 8 (wire
// offset 12). `result` of the login reply is that field: the go-eqoa-server
// error flag at wire 8 is a word STATION never reads (`Exchange` above keeps
// the server's own reading for `eqoa play`).

/// `VIGetGameCode()`: SLUS's `VISetGameCode("EQOA")`.
pub const GAME_CODE: &str = "EQOA";

/// `des_soe_encrypt(in, out, len)`: DES-CBC under the fixed key, the key as
/// the IV, over `input` zero-padded to a multiple of 8 (the game reads `len`
/// bytes from the string's address, past its NUL; the server reads up to it).
pub fn des_soe_encrypt(input: &[u8], len: usize) -> Vec<u8> {
    use des::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
    let c = des::Des::new(GenericArray::from_slice(&DES_KEY));
    let mut b = vec![0u8; len.div_ceil(8) * 8];
    let n = input.len().min(len);
    b[..n].copy_from_slice(&input[..n]);
    let mut iv = DES_KEY;
    for block in b.chunks_mut(8) {
        for (x, v) in block.iter_mut().zip(iv) {
            *x ^= v;
        }
        c.encrypt_block(GenericArray::from_mut_slice(block));
        iv.copy_from_slice(block);
    }
    b.truncate(len);
    b
}

/// `des_soe_decrypt`: the same CBC undone.
pub fn des_soe_decrypt(input: &[u8]) -> Vec<u8> {
    use des::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
    let c = des::Des::new(GenericArray::from_slice(&DES_KEY));
    let mut iv = DES_KEY;
    let mut out = Vec::with_capacity(input.len());
    for block in input.chunks_exact(8) {
        let mut b = [0u8; 8];
        b.copy_from_slice(block);
        let keep = b;
        c.decrypt_block(GenericArray::from_mut_slice(&mut b));
        for (x, v) in b.iter_mut().zip(iv) {
            *x ^= v;
        }
        iv = keep;
        out.extend_from_slice(&b);
    }
    out
}

/// `aes_ecb_encrypt_buffer` under `key` (rijndael-256, `len / 16` blocks), over
/// `input` zero-padded to `len`.
pub fn aes_ecb_encrypt(key: &[u8; 32], input: &[u8], len: usize) -> Vec<u8> {
    use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
    let c = aes::Aes256::new(GenericArray::from_slice(key));
    let mut b = vec![0u8; len];
    let n = input.len().min(len);
    b[..n].copy_from_slice(&input[..n]);
    for block in b.chunks_exact_mut(16) {
        c.encrypt_block(GenericArray::from_mut_slice(block));
    }
    b
}

/// A request `len` bytes long: the length and the type big-endian, the rest
/// zero (`memset`).
fn request(ty: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    b[0..4].copy_from_slice(&(len as u32).to_be_bytes());
    b[4..8].copy_from_slice(&ty.to_be_bytes());
    b
}

/// `strncpy(dst, s, n)` into `b[at..]` (the rest of the field is already zero).
fn put(b: &mut [u8], at: usize, n: usize, s: &[u8]) {
    let m = s.iter().position(|&c| c == 0).unwrap_or(s.len()).min(n);
    b[at..at + m].copy_from_slice(&s[..m]);
}

fn put_be(b: &mut [u8], at: usize, v: i32) {
    b[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// `VIStationAccount`: the account form the two creation pages fill
/// (`CreateAccount`'s argument).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub password: String,
    pub first: String,
    pub middle: String,
    pub last: String,
    pub country: String,
    pub postal: String,
    pub day: String,
    pub month: String,
    pub year: String,
    pub gender: String,
    pub email: String,
}

/// `VIStationBilling`, request 46's form (the billing pages').
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Billing {
    pub address: String,
    pub city: String,
    pub country: String,
    pub state: String,
    pub first: String,
    pub last: String,
    pub postal: String,
    pub year: String,
    pub month: String,
    /// card number, gamecard or rental number: AES-encrypted into the request
    pub number: String,
    pub kind: String,
}

/// Type 56, `CreateAccount`: 476 bytes, the password DES-encrypted, the
/// country cut to two letters.
pub fn create_account(a: &Account) -> Vec<u8> {
    let mut b = request(56, 0x1DC);
    put(&mut b, 0x0C, 32, a.name.as_bytes());
    b[0x2C..0x4C].copy_from_slice(&des_soe_encrypt(a.password.as_bytes(), 32));
    put(&mut b, 0x12C, 16, a.day.as_bytes());
    put(&mut b, 0x11C, 16, a.month.as_bytes());
    put(&mut b, 0x13C, 16, a.year.as_bytes());
    put(&mut b, 0xDC, 3, a.country.as_bytes());
    b[0xDC + 2] = 0;
    put(&mut b, 0x15C, 128, a.email.as_bytes());
    put(&mut b, 0x4C, 64, a.first.as_bytes());
    put(&mut b, 0x9C, 64, a.last.as_bytes());
    put(&mut b, 0x8C, 16, a.middle.as_bytes());
    put(&mut b, 0x14C, 16, a.gender.as_bytes());
    put(&mut b, 0xFC, 32, a.postal.as_bytes());
    b
}

/// Type 42, `ChangePassword`: the account id and both passwords DES-encrypted.
pub fn change_password(id: i32, old: &str, new: &str) -> Vec<u8> {
    let mut b = request(42, 80);
    put_be(&mut b, 12, id);
    b[16..48].copy_from_slice(&des_soe_encrypt(old.as_bytes(), 32));
    b[48..80].copy_from_slice(&des_soe_encrypt(new.as_bytes(), 32));
    b
}

/// Types 44 and 48: an id and two strings (`VIStationRequestIdAB`).
pub fn request_id_ab(ty: u32, id: i32, a: &str, b_: &str) -> Vec<u8> {
    let mut b = request(ty, 64);
    put_be(&mut b, 12, id);
    put(&mut b, 16, 16, a.as_bytes());
    put(&mut b, 32, 32, b_.as_bytes());
    b
}

/// Type 46: the billing form, its number AES-encrypted under the session key.
/// "EQOA Beta" goes out as the SKU the betas sold.
pub fn request46(id: i32, f: &Billing, sku: &str, key: &[u8; 32]) -> Vec<u8> {
    let mut b = request(46, 0x240);
    put_be(&mut b, 12, id);
    let sku = if sku == "EQOA Beta" { "EQ1RE-SU-SU0402-EQTIME-01MON" } else { sku };
    put(&mut b, 0x10, 64, sku.as_bytes());
    b[0x1F0..0x210].copy_from_slice(&aes_ecb_encrypt(key, f.number.as_bytes(), 32));
    put(&mut b, 0x50, 127, f.address.as_bytes());
    put(&mut b, 0x230, 15, f.kind.as_bytes());
    put(&mut b, 0xD0, 63, f.city.as_bytes());
    put(&mut b, 0x110, 31, f.country.as_bytes());
    put(&mut b, 0x1E0, 15, f.month.as_bytes());
    put(&mut b, 0x1D0, 15, f.year.as_bytes());
    put(&mut b, 0x170, 31, f.first.as_bytes());
    put(&mut b, 0x190, 31, f.last.as_bytes());
    put(&mut b, 0x1B0, 31, f.postal.as_bytes());
    put(&mut b, 0x130, 63, f.state.as_bytes());
    b
}

/// Type 66, forgotten password: five fields, each AES-encrypted under the
/// session key (the last 128 bytes, the others 32).
pub fn request66(user: &str, month: &str, day: &str, year: &str, email: &str, key: &[u8; 32]) -> Vec<u8> {
    let mut b = request(66, 268);
    for (at, s) in [(12, user), (44, month), (76, day), (108, year)] {
        b[at..at + 32].copy_from_slice(&aes_ecb_encrypt(key, s.as_bytes(), 32));
    }
    b[140..268].copy_from_slice(&aes_ecb_encrypt(key, email.as_bytes(), 128));
    b
}

/// Types 50 and 52: an id and one string (`VIStationRequestIdA`).
pub fn request_id_a(ty: u32, id: i32, a: &str) -> Vec<u8> {
    let mut b = request(ty, 32);
    put_be(&mut b, 12, id);
    put(&mut b, 16, 16, a.as_bytes());
    b
}

/// Type 54: an id, the game code and a game card's number.
pub fn request54(id: i32, a: &str, b_: &str) -> Vec<u8> {
    let mut b = request(54, 96);
    put_be(&mut b, 12, id);
    put(&mut b, 16, 16, a.as_bytes());
    put(&mut b, 32, 32, b_.as_bytes());
    b
}

/// Type 62: an id and the game code (copied 16 of 32).
pub fn request62(id: i32, a: &str) -> Vec<u8> {
    let mut b = request(62, 48);
    put_be(&mut b, 12, id);
    put(&mut b, 16, 16, a.as_bytes());
    b
}

/// Type 64, the account update: the country's two letters, the postal
/// code, the e-mail address.
pub fn request64(id: i32, country: &str, postal: &str, email: &str) -> Vec<u8> {
    let mut b = request(64, 208);
    put_be(&mut b, 12, id);
    put(&mut b, 16, 3, country.as_bytes());
    b[16 + 2] = 0;
    put(&mut b, 48, 32, postal.as_bytes());
    put(&mut b, 80, 128, email.as_bytes());
    b
}

/// Type 2, `Ping`: the header alone.
pub fn ping() -> Vec<u8> {
    request(2, 8)
}

/// One plan of reply 63 (`VISubscriptionPlan`, 212 bytes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub sku: String,
    pub long_desc: String,
    pub short_desc: String,
    pub price: String,
    pub duration: String,
}

/// A reply, as `VIStationAPI::OnMessage` hands it to the listener. A reply
/// whose size is not its struct's is dropped (`None`), as the game drops it.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// 4: the session key, DES-decrypted (`OnSessionKey`)
    SessionKey([u8; 32]),
    /// 37: `OnValidateLogin(result, uid, accstat, subtime, partime,
    /// subfeatures, gamefeatures, s92, s12)`
    Login { result: i32, uid: i32, accstat: i32, subtime: i32, partime: i32, subfeatures: i32, gamefeatures: i32, s92: String, s12: String },
    /// 41: account creation
    Created { success: bool, code: i32, text: String },
    /// 43: `OnChangePassword`
    PasswordChanged { success: bool, text: String },
    /// 45, 47, 49 (`OnConsumeAccountKey`), 53 (`OnCancelAccount`): two words
    Pair { ty: u32, a: i32, b: i32 },
    /// 51: `OnGetSubscriptionInfo`
    SubscriptionInfo { a: i32, b: i32, status: i32, payment: i32, sku: String },
    /// 55
    R55 { a: i32, b: i32, s16: String, sku: String, c: i32, d: i32 },
    /// 63: `OnGetSubscriptionList`
    Plans { a: i32, b: i32, plans: Vec<Plan> },
    /// 65 and 67: one word
    One { ty: u32, a: i32 },
    /// 69, 71, 73, 75, 77, 84: their handlers are empty in the renderer
    Ignored(u32),
}

fn cstr(b: &[u8]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..n].iter().map(|&c| c as char).collect()
}

/// `OnMessage`: the message less its length word (its type first).
pub fn parse_reply(d: &[u8]) -> Option<Reply> {
    if d.len() < 4 {
        return None;
    }
    let be = |o: usize| i32::from_be_bytes(d[o..o + 4].try_into().unwrap());
    let ty = be(0) as u32;
    let n = d.len();
    Some(match ty {
        4 if n == 36 => {
            let k = des_soe_decrypt(&d[4..36]);
            Reply::SessionKey(k.try_into().unwrap())
        }
        37 if n == 164 => Reply::Login {
            result: be(8),
            s12: cstr(&d[12..76]),
            uid: be(76),
            accstat: be(80),
            subtime: be(84),
            partime: be(88),
            s92: cstr(&d[92..156]),
            subfeatures: be(156),
            gamefeatures: be(160),
        },
        41 if n == 272 => Reply::Created { success: be(8) != 0, code: be(12), text: cstr(&d[16..272]) },
        43 if n == 268 => Reply::PasswordChanged { success: be(8) != 0, text: cstr(&d[12..268]) },
        45 if n == 124 => Reply::Pair { ty, a: be(8), b: be(12) },
        47 | 49 | 53 if n == 16 => Reply::Pair { ty, a: be(8), b: be(12) },
        51 if n == 56 => Reply::SubscriptionInfo { a: be(8), b: be(12), status: be(16), payment: be(20), sku: cstr(&d[24..56]) },
        55 if n == 76 => Reply::R55 { a: be(8), b: be(12), s16: cstr(&d[16..32]), sku: cstr(&d[32..64]), c: be(64), d: be(68) },
        63 if n >= 20 => {
            let count = be(16).max(0) as usize;
            let plans = (0..count)
                .map_while(|i| {
                    let p = d.get(20 + i * 212..20 + (i + 1) * 212)?;
                    Some(Plan { sku: cstr(&p[0..32]), long_desc: cstr(&p[32..160]), short_desc: cstr(&p[160..176]), price: cstr(&p[176..192]), duration: cstr(&p[192..208]) })
                })
                .collect();
            Reply::Plans { a: be(8), b: be(12), plans }
        }
        65 | 67 if n == 12 => Reply::One { ty, a: be(8) },
        69 | 71 | 73 | 75 | 77 | 84 => Reply::Ignored(ty),
        _ => return None,
    })
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

    /// A password of 31 characters reaches the server whole, in the request
    /// and in the UDP token (the server reads each to its NUL); the 32 bytes
    /// hold no more, so a longer one is cut (the page's editor stops at 31).
    #[test]
    fn a_31_character_password_goes_whole() {
        use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
        let pw = "abcdefghijklmnopqrstuvwxyz01234";
        assert_eq!(pw.len(), 31);
        let plain = des_soe_decrypt(&login_request("dab", pw)[48..80]);
        assert_eq!(&plain[..31], pw.as_bytes());
        assert_eq!(plain[31], 0, "the NUL the server reads to");
        let mut t = udp_token(pw);
        let c = aes::Aes256::new(GenericArray::from_slice(&AES_KEY));
        for block in t.chunks_mut(16) {
            c.decrypt_block(GenericArray::from_mut_slice(block));
        }
        assert_eq!((&t[..31], t[31]), (pw.as_bytes(), 0));
        // the control: a 40-character one keeps its first 31
        let plain = des_soe_decrypt(&login_request("dab", &format!("{pw}567890123"))[48..80]);
        assert_eq!((&plain[..31], plain[31]), (pw.as_bytes(), 0));
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

/// The rest of the Station API, against the decomp's struct layouts
/// (vistationapi.cpp): each request's size and every field's place, the
/// ciphers round-tripped, the replies' sizes and fields, each with a control.
#[cfg(test)]
mod station_tests {
    use super::*;

    fn be(b: &[u8], o: usize) -> i32 {
        i32::from_be_bytes(b[o..o + 4].try_into().unwrap())
    }

    #[test]
    fn des_soe_round_trips_and_is_the_login_requests() {
        let e = des_soe_encrypt(b"secret", 32);
        assert_eq!(e.len(), 32);
        assert_eq!(&e[..], &encrypt_password("secret")[..], "the login's own encryption");
        let d = des_soe_decrypt(&e);
        assert_eq!(&d[..6], b"secret");
        assert!(d[6..].iter().all(|&c| c == 0));
        // the control: one changed byte changes its block and every one after (CBC)
        let f = des_soe_encrypt(b"sedret", 32);
        assert_ne!(e[..8], f[..8]);
        assert_ne!(e[24..], f[24..]);
    }

    #[test]
    fn the_account_creation_request() {
        let a = Account {
            name: "newbie".into(),
            password: "pw1234".into(),
            first: "Van".into(),
            middle: "Q".into(),
            last: "Station".into(),
            country: "US - USA".into(),
            postal: "90210".into(),
            day: "7".into(),
            month: "Jan".into(),
            year: "1977".into(),
            gender: "Male".into(),
            email: "a@b.c".into(),
        };
        let b = create_account(&a);
        assert_eq!(b.len(), 476, "468 after the header, as go-eqoa-server reads it");
        assert_eq!((be(&b, 0), be(&b, 4)), (476, 56));
        assert_eq!(&b[0x0C..0x12], b"newbie");
        assert_eq!(&b[0x2C..0x4C], &des_soe_encrypt(b"pw1234", 32)[..]);
        assert_eq!(&b[0x4C..0x4F], b"Van");
        assert_eq!(&b[0x8C..0x8D], b"Q");
        assert_eq!(&b[0x9C..0xA3], b"Station");
        assert_eq!(&b[0xDC..0xDF], b"US\0", "the country cut to two letters");
        assert_eq!(&b[0xFC..0x101], b"90210");
        assert_eq!(&b[0x11C..0x11F], b"Jan", "the month at 0x11C");
        assert_eq!(&b[0x12C..0x12D], b"7", "the day at 0x12C");
        assert_eq!(&b[0x13C..0x140], b"1977");
        assert_eq!(&b[0x14C..0x150], b"Male");
        assert_eq!(&b[0x15C..0x161], b"a@b.c");
        // the control: the month and the day are not where their names in the form put them
        assert_ne!(&b[0x11C..0x11D], b"7");
    }

    #[test]
    fn the_other_requests_layouts() {
        let b = change_password(7, "old", "new");
        assert_eq!((b.len(), be(&b, 4), be(&b, 12)), (80, 42, 7));
        assert_eq!(&b[16..48], &des_soe_encrypt(b"old", 32)[..]);
        assert_eq!(&b[48..80], &des_soe_encrypt(b"new", 32)[..]);
        let b = request_id_ab(48, 3, "EQOA", "ABCD-1234");
        assert_eq!((b.len(), be(&b, 4), be(&b, 12)), (64, 48, 3));
        assert_eq!((&b[16..20], &b[32..41]), (&b"EQOA"[..], &b"ABCD-1234"[..]));
        let b = request_id_a(50, 3, "EQOA");
        assert_eq!((b.len(), be(&b, 4), &b[16..20]), (32, 50, &b"EQOA"[..]));
        let b = request54(3, "EQOA", "CARD");
        assert_eq!((b.len(), be(&b, 4), &b[32..36]), (96, 54, &b"CARD"[..]));
        let b = request62(3, "EQOA");
        assert_eq!((b.len(), be(&b, 4), &b[16..20]), (48, 62, &b"EQOA"[..]));
        let b = request64(3, "US - USA", "90210", "a@b.c");
        assert_eq!((b.len(), be(&b, 4), &b[16..19], &b[48..53], &b[80..85]), (208, 64, &b"US\0"[..], &b"90210"[..], &b"a@b.c"[..]));
        let key = [9u8; 32];
        let b = request66("newbie", "Jan", "7", "1977", "a@b.c", &key);
        assert_eq!((b.len(), be(&b, 4)), (268, 66));
        assert_eq!(&b[12..44], &aes_ecb_encrypt(&key, b"newbie", 32)[..]);
        assert_eq!(&b[140..268], &aes_ecb_encrypt(&key, b"a@b.c", 128)[..]);
        assert_ne!(&b[12..44], &aes_ecb_encrypt(&[8; 32], b"newbie", 32)[..], "the control: another key");
        let f = Billing { number: "4111".into(), kind: "gamecard".into(), address: "1 Main".into(), ..Default::default() };
        let b = request46(3, &f, "EQOA Beta", &key);
        assert_eq!((b.len(), be(&b, 4), be(&b, 12)), (576, 46, 3));
        assert_eq!(&b[0x10..0x2C], b"EQ1RE-SU-SU0402-EQTIME-01MON", "the betas' SKU");
        assert_eq!(&b[0x1F0..0x210], &aes_ecb_encrypt(&key, b"4111", 32)[..]);
        assert_eq!((&b[0x230..0x238], &b[0x50..0x56]), (&b"gamecard"[..], &b"1 Main"[..]));
        assert_eq!(ping(), vec![0, 0, 0, 8, 0, 0, 0, 2]);
    }

    /// A reply as `VITCPConnection::Process` delivers it: less its length word.
    fn reply(ty: u32, len: usize) -> Vec<u8> {
        let mut b = vec![0u8; len];
        b[0..4].copy_from_slice(&ty.to_be_bytes());
        b
    }

    #[test]
    fn the_login_reply_is_read_where_station_reads_it() {
        // go-eqoa-server's success: its error flag at wire 8 (here 4), zeros
        // from wire 12, uid at 80, status 3 at 84, 0xFFFFFFFF at 160 and 164
        let mut d = reply(37, 164);
        d[4..8].copy_from_slice(&1u32.to_be_bytes());
        d[76..80].copy_from_slice(&42u32.to_be_bytes());
        d[80..84].copy_from_slice(&3u32.to_be_bytes());
        d[92..108].copy_from_slice(b"EQOA_SESSION_OK!");
        d[156..164].copy_from_slice(&[0xFF; 8]);
        match parse_reply(&d) {
            Some(Reply::Login { result: 0, uid: 42, accstat: 3, gamefeatures: -1, s92, .. }) => assert_eq!(s92, "EQOA_SESSION_OK!"),
            r => panic!("{r:?}"),
        }
        // a text at wire 12 is STATION's result (nonzero): the refusal
        d[8..12].copy_from_slice(b"You ");
        assert!(matches!(parse_reply(&d), Some(Reply::Login { result: r, .. }) if r != 0));
        // the control: a reply a word short is dropped, as OnReply37 drops it
        assert_eq!(parse_reply(&d[..160]), None);
    }

    #[test]
    fn the_other_replies() {
        let mut d = reply(41, 272);
        d[8..12].copy_from_slice(&1u32.to_be_bytes());
        d[16..22].copy_from_slice(b"exists");
        assert_eq!(parse_reply(&d), Some(Reply::Created { success: true, code: 0, text: "exists".into() }));
        let mut d = reply(43, 268);
        d[8..12].copy_from_slice(&1u32.to_be_bytes());
        assert_eq!(parse_reply(&d), Some(Reply::PasswordChanged { success: true, text: String::new() }));
        let mut d = reply(53, 16);
        d[12..16].copy_from_slice(&2u32.to_be_bytes());
        assert_eq!(parse_reply(&d), Some(Reply::Pair { ty: 53, a: 0, b: 2 }));
        let mut d = reply(51, 56);
        d[16..20].copy_from_slice(&5u32.to_be_bytes());
        d[24..27].copy_from_slice(b"SKU");
        assert_eq!(parse_reply(&d), Some(Reply::SubscriptionInfo { a: 0, b: 0, status: 5, payment: 0, sku: "SKU".into() }));
        let mut d = reply(63, 20 + 212);
        d[16..20].copy_from_slice(&1u32.to_be_bytes());
        d[20 + 160..20 + 165].copy_from_slice(b"Month");
        d[20 + 176..20 + 180].copy_from_slice(b"9.99");
        assert!(matches!(parse_reply(&d), Some(Reply::Plans { plans, .. }) if plans.len() == 1 && plans[0].short_desc == "Month" && plans[0].price == "9.99"));
        // the session key, DES-encrypted on the wire
        let mut d = reply(4, 36);
        d[4..36].copy_from_slice(&des_soe_encrypt(&[7u8; 32], 32));
        assert_eq!(parse_reply(&d), Some(Reply::SessionKey([7; 32])));
        // the controls: go-eqoa-server's account-creation reply on the wire is
        // 276 with its length, 272 without; anything else, and an unknown type, is dropped
        assert_eq!(parse_reply(&reply(41, 276)), None);
        assert_eq!(parse_reply(&reply(99, 16)), None);
        assert_eq!(parse_reply(&reply(4, 40)), None);
    }
}
