//! The VI engine's game logic, ported from the matching decomp
//! ([eqoa-decomp](https://github.com/DabDavis/eqoa-decomp)) one class at a
//! time, each checked against the decomp compiled for the host
//! (`tests/oracle.rs`, eqoa-port's `build/oracle`).
//!
//! The structs are `#[repr(C)]` and laid out as the EE lays them out, so the
//! oracle can be handed their bytes; the field names follow the decomp's.

// The decomp's float literals are kept exactly as it spells them (3.14159274,
// 0.00999999978), so a reader can match a line here to a line there.
#![allow(clippy::excessive_precision)]
// And its statements as it writes them: `p.x = p.x + d` where the C has it,
// and a zeroing constructor followed by field stores.
#![allow(clippy::assign_op_pattern, clippy::field_reassign_with_default)]
// A literal that looks like pi/4 is the GAME's float, not Rust's constant, and
// two branches the C writes out separately stay separate.
#![allow(clippy::approx_constant, clippy::if_same_then_else)]

pub mod camera;
pub mod character;
pub mod clipstack;
pub mod coll;
pub mod creature;
pub mod effect;
pub mod emit;
pub mod csprite;
pub mod cust;
pub mod collide;
pub mod flora;
pub mod fpu;
pub mod geom;
pub mod hash;
pub mod hsprite;
pub mod lightning;
pub mod pad;
pub mod particle;
pub mod pick;
pub mod quat;
pub mod scene;
pub mod textset;
pub mod tod;
pub mod trail;
pub mod atmosphere;
pub mod selfstate;
pub mod shadow;
pub mod spell;
pub mod strtab;
pub mod swing;
pub mod ui;
pub mod vect;
pub mod walk;
pub mod world;

pub use fpu::{Ee, F, Ieee, Model};
