//! The client above DRDP, ported from the decomp: the Station login
//! (`auth`), `VIMemStream`'s encodings (`stream`), and `VIClient`'s session
//! from the server list to standing in the world (`session`); a server's
//! name to its address (`addr`).

pub mod addr;
pub mod auth;
pub mod cape;
pub mod creation;
pub mod dump;
pub mod entity;
pub mod hidelook;
pub mod itemlink;
pub mod npctint;
pub mod raceopts;
pub mod raid;
pub mod serverworlds;
pub mod session;
pub mod stream;
