//! Turns a parsed map and its plan into the generated static, type and lookup.

use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};

use crate::parse::MapInput;
use crate::plan::Plan;
use crate::repr::Repr;
use crate::search::Strategy;

pub fn emit(input: &MapInput, plan: &Plan) -> TokenStream {
    let MapInput { vis, name, value_ty: ty, entries } = input;
    let ty_name = format_ident!("__Pickmap_{}", name);
    // At module level, not inside the `const _` block: value expressions must resolve
    // names in the user's scope, not see these.
    let keys_name = format_ident!("__PICKMAP_{}_KEYS", name);
    let values_name = format_ident!("__PICKMAP_{}_VALUES", name);
    let p = quote!(::pickmap::__private);
    let n = entries.len();
    let keys: Vec<Literal> = entries.iter().map(|e| Literal::string(&e.key)).collect();
    let values = entries.iter().map(|e| &e.value);

    let tables_and_get = if n == 0 {
        quote! {
            #[inline]
            pub fn get(&self, _key: &::core::primitive::str) -> ::core::option::Option<&'static #ty> {
                ::core::option::Option::None
            }
        }
    } else {
        lookup(input, plan, &p, ty, &keys_name, &values_name)
    };

    quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        #vis struct #ty_name;

        #vis static #name: #ty_name = #ty_name;

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #keys_name: [&::core::primitive::str; #n] = [#(#keys),*];
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #values_name: [#ty; #n] = [#(#values),*];

        const _: () = {
            impl #ty_name {
                #tables_and_get

                #[inline]
                pub fn contains_key(&self, key: &::core::primitive::str) -> ::core::primitive::bool {
                    self.get(key).is_some()
                }

                #[inline]
                pub const fn len(&self) -> ::core::primitive::usize {
                    #n
                }

                #[inline]
                pub const fn is_empty(&self) -> ::core::primitive::bool {
                    #n == 0
                }

                /// All entries, in declaration order.
                #[inline]
                pub fn iter(&self) -> #p::Iter<#ty> {
                    #p::Iter::new(&#keys_name, &#values_name)
                }
            }

            impl ::pickmap::StaticMap for #ty_name {
                type Value = #ty;

                #[inline]
                fn get(&self, key: &::core::primitive::str) -> ::core::option::Option<&'static #ty> {
                    #ty_name::get(self, key)
                }

                #[inline]
                fn len(&self) -> ::core::primitive::usize {
                    #n
                }

                #[inline]
                fn iter(&self) -> #p::Iter<#ty> {
                    #ty_name::iter(self)
                }
            }
        };
    }
}

/// The slot tables, strategy tables and `get` for a non-empty map.
fn lookup(
    input: &MapInput,
    plan: &Plan,
    p: &TokenStream,
    ty: &TokenStream,
    keys_name: &proc_macro2::Ident,
    values_name: &proc_macro2::Ident,
) -> TokenStream {
    let words = plan.repr.words();
    let value_expr = match plan.repr {
        Repr::Classes { classes, .. } => quote!(#p::key_value::<#words, #classes>(k)),
        Repr::Sig { w, c, .. } => quote!(#p::signature::<#w, #c, #words>(k)),
    };

    let table = plan.slots.len();
    let slots = plan.slots.iter().map(|s| match s {
        None => quote!(#p::Slot::EMPTY),
        Some(i) => {
            let key = input.entries[*i as usize].key.as_bytes();
            let words = plan.repr.value(key).into_iter().map(Literal::u64_suffixed);
            let meta = Literal::u32_suffixed(plan.slot_and_meta(key).1);
            let idx = Literal::u32_suffixed(*i);
            quote!(#p::Slot { key: [#(#words),*], meta: #meta, idx: #idx })
        }
    });

    let (strategy_tables, slot_expr) = match &plan.strategy {
        Strategy::Picked { positions, asso, mask } => {
            let k = positions.len();
            let positions = positions.iter().map(|pos| match pos {
                pickmap_core::hash::Pos::Start(i) => quote!(#p::Pos::Start(#i)),
                pickmap_core::hash::Pos::End(i) => quote!(#p::Pos::End(#i)),
            });
            let asso = asso.iter().map(|t| quote!([#(#t),*]));
            (
                quote! {
                    const POSITIONS: [#p::Pos; #k] = [#(#positions),*];
                    static ASSO: [[::core::primitive::u16; 256]; #k] = [#(#asso),*];
                },
                quote! {
                    let slot = #p::picked_slot(k, &POSITIONS, &ASSO, #mask);
                },
            )
        }
        Strategy::PtHash { seed, bucket_bits, pilots, mask } => {
            let b = pilots.len();
            let h = if plan.has_long {
                quote!(if k.len() > 32 { #p::h_long(k, #seed) } else { #p::h_value(&v, k.len(), #seed) })
            } else {
                quote!(#p::h_value(&v, k.len(), #seed))
            };
            (
                quote! { static PILOTS: [::core::primitive::u32; #b] = [#(#pilots),*]; },
                quote! {
                    let h = #h;
                    let slot = #p::pthash_slot(h, #bucket_bits, &PILOTS, #mask);
                    let meta = #p::meta(k.len(), #p::hash_tag(h));
                },
            )
        }
    };

    let find = match (plan.tagged(), plan.has_long) {
        (false, false) => quote!(SLOTS[slot].find(&v, k.len())),
        (false, true) => quote!(SLOTS[slot].find_long(&v, k, &#keys_name)),
        (true, false) => quote!(SLOTS[slot].find_tagged(&v, meta)),
        (true, true) => quote!(SLOTS[slot].find_tagged_long(&v, meta, k, &#keys_name)),
    };

    quote! {
        #[inline]
        pub fn get(&self, key: &::core::primitive::str) -> ::core::option::Option<&'static #ty> {
            #strategy_tables
            static SLOTS: [#p::Slot<#words>; #table] = [#(#slots),*];

            let k = key.as_bytes();
            let v = #value_expr;
            #slot_expr
            match #find {
                ::core::option::Option::Some(i) => ::core::option::Option::Some(&#values_name[i]),
                ::core::option::Option::None => ::core::option::Option::None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use crate::plan::plan;

    fn expand_str(src: &str) -> String {
        let input = parse(src.parse().unwrap()).unwrap_or_else(|e| panic!("{}", e.message));
        let keys: Vec<&str> = input.entries.iter().map(|e| e.key.as_str()).collect();
        emit(&input, &plan(&keys).unwrap()).to_string()
    }

    #[test]
    fn emit_mentions_only_absolute_paths() {
        for src in [r#"M: u8; "a" => 1, "bb" => 2"#, "M: u8;"] {
            let out = expand_str(src);
            let toks: Vec<&str> = out.split_whitespace().collect();
            for (i, t) in toks.iter().enumerate() {
                if ["Option", "Some", "None"].contains(t) {
                    assert_eq!(toks[i - 1], "::", "bare `{t}` in {src:?}");
                }
            }
        }
    }
}
