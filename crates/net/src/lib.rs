//! The client above DRDP, ported from the decomp: the Station login
//! (`auth`), `VIMemStream`'s encodings (`stream`), and `VIClient`'s session
//! from the server list to standing in the world (`session`).

pub mod auth;
pub mod session;
pub mod stream;
