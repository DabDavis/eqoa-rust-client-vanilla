//! EQOA's sound, as the PS2 makes it.
//!
//! - `spu2`: the SPU2, ported from eqoa-port's `platform/spu2.c`;
//! - `iop`, `driver`, `xm`: SNDDRV, the IOP's sound driver, from the decomp's
//!   `modules/snddrv` (the command handlers, the BGM stream, the XM player),
//!   with its threads as state machines on the SPU2's sample clock.
//!
//! Held sample for sample to eqoa-port's `build/oracle_snd`, which runs the
//! decomp's own SNDDRV over the same SPU2 on the same clock
//! (`tests/oracle_snd.rs`).

pub mod driver;
mod gauss;
pub mod iop;
pub mod spu2;
pub mod xm;

pub use driver::{Disc, Driver, NoDisc};
