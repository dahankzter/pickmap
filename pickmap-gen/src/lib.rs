//! Generator behind `pickmap::static_map!`: chooses a key representation and a perfect
//! hash for a key set, and emits the specialized lookup.

pub mod repr;
pub mod plan;
pub mod search;
pub mod parse;
