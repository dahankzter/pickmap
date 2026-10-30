//! Internal to [pickmap](https://docs.rs/pickmap); depend on that crate instead.
//!
//! Generator behind `pickmap::static_map!`: chooses a key representation and a perfect
//! hash for a key set, and emits the specialized lookup.

pub mod emit;
pub mod parse;
pub mod plan;
pub mod repr;
pub mod search;

use proc_macro2::TokenStream;

/// The whole macro: parse, plan, emit. Errors come back as `compile_error!` tokens.
pub fn expand(input: TokenStream) -> TokenStream {
    let input = match parse::parse(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };
    let keys: Vec<&str> = input.entries.iter().map(|e| e.key.as_str()).collect();
    match plan::plan(&keys) {
        Ok(plan) => emit::emit(&input, &plan),
        Err(message) => parse::Error { span: input.name.span(), message }.to_compile_error(),
    }
}
