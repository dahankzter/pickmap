//! The `static_map!` proc macro. Use it through the `pickmap` crate.

use proc_macro::TokenStream;

/// Declares a static map whose lookup is generated for its exact keys.
///
/// ```ignore
/// pickmap::static_map! {
///     pub METHODS: Method;
///     "GET" => Method::Get,
///     "POST" => Method::Post,
/// }
/// assert_eq!(METHODS.get("POST"), Some(&Method::Post));
/// ```
#[proc_macro]
pub fn static_map(input: TokenStream) -> TokenStream {
    pickmap_gen::expand(input.into()).into()
}
