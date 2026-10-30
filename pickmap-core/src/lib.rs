//! Primitives shared by `pickmap-gen` (at build time) and the code it generates (at
//! run time), so the two compute exactly the same values.
#![no_std]

pub mod hash;
pub mod load;
pub mod slot;
