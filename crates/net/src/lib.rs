//! The client above DRDP, ported from the decomp: the Station login
//! (`auth`), `VIMemStream`'s encodings (`stream`), and `VIClient`'s session
//! from the server list to standing in the world (`session`); a server's
//! name to its address (`addr`).

pub mod addr;
pub mod auth;
pub mod dump;
pub mod entity;
pub mod session;
pub mod stream;
pub mod voice;
