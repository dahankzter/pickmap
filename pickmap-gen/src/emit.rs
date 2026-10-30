//! Turns a parsed map and its plan into the generated static, type and lookup.

use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};

use crate::parse::{MapInput, Workload};
use crate::plan::Plan;
use crate::repr::Repr;
use crate::search::Strategy;

pub fn emit(input: &MapInput, plan: &Plan) -> TokenStream {
    let MapInput { attrs, cfg_attrs, vis, name, value_ty: ty, entries, .. } = input;
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

            /// The value for the key at `buf[range]`; always `None` for an empty map.
            ///
            /// # Panics
            ///
            /// If `range` is out of bounds for `buf`, like indexing.
            #[inline]
            pub fn get_in<B: ::core::convert::AsRef<[::core::primitive::u8]> + ?::core::marker::Sized>(
                &self,
                buf: &B,
                range: ::core::ops::Range<::core::primitive::usize>,
            ) -> ::core::option::Option<&'static #ty> {
                let _ = &buf.as_ref()[range];
                ::core::option::Option::None
            }
        }
    } else {
        lookup(input, plan, &p, ty, &keys_name, &values_name)
    };

    quote! {
        #(#cfg_attrs)*
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        #vis struct #ty_name;

        #(#cfg_attrs)*
        #(#attrs)*
        #vis static #name: #ty_name = #ty_name;

        #(#cfg_attrs)*
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #keys_name: [&::core::primitive::str; #n] = [#(#keys),*];
        #(#cfg_attrs)*
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #values_name: [#ty; #n] = [#(#values),*];

        #(#cfg_attrs)*
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
                fn get_in<B: ::core::convert::AsRef<[::core::primitive::u8]> + ?::core::marker::Sized>(
                    &self,
                    buf: &B,
                    range: ::core::ops::Range<::core::primitive::usize>,
                ) -> ::core::option::Option<&'static #ty> {
                    #ty_name::get_in(self, buf, range)
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

/// `get` and `get_in` for a non-empty map. Each keeps its tables inside its own body, so a
/// program that never calls `get_in` doesn't carry its tables.
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
        Repr::Split { c, short, .. } => quote!(#p::split_value::<#words, #c, #short>(k)),
        Repr::Padded { .. } => unreachable!("get never uses the padded representation"),
    };
    let (tables, slot_stmts, find) = table_parts(input, plan, p, keys_name);
    let some_value = quote! {
        match #find {
            ::core::option::Option::Some(i) => ::core::option::Option::Some(&#values_name[i]),
            ::core::option::Option::None => ::core::option::Option::None,
        }
    };
    // `get`'s body, always inlined: `get` stays a small wrapper, so callers can inline it
    // as before, and `get_in`'s fallback gets its own copy rather than a call.
    let get_bytes = quote! {
        #[inline(always)]
        #[allow(dead_code)]
        fn __pickmap_get_bytes(&self, k: &[::core::primitive::u8]) -> ::core::option::Option<&'static #ty> {
            #tables
            let v = #value_expr;
            #slot_stmts
            #some_value
        }
    };
    let get_doc = quote! {
        /// The value for `key`, if the map has it.
    };
    let get_in_doc = quote! {
        /// The value for the key at `buf[range]`. The same answer as
        /// `get(&buf[range])`, but faster when `buf` continues past the key: the key is
        /// read with one wide load from `buf` and masked, without leaving `buf`.
        ///
        /// # Panics
        ///
        /// If `range` is out of bounds for `buf`, like indexing.
    };
    let get_in_sig = quote! {
        pub fn get_in<B: ::core::convert::AsRef<[::core::primitive::u8]> + ?::core::marker::Sized>(
            &self,
            buf: &B,
            range: ::core::ops::Range<::core::primitive::usize>,
        ) -> ::core::option::Option<&'static #ty>
    };

    let Some(pad) = &plan.pad else {
        // Maps with keys over 32 bytes: no padded tables; `get_in` is `get` on the bytes.
        // No UTF-8 check is needed there: invalid UTF-8 never equals a key.
        return quote! {
            #get_bytes

            #get_doc
            #[inline]
            pub fn get(&self, key: &::core::primitive::str) -> ::core::option::Option<&'static #ty> {
                self.__pickmap_get_bytes(key.as_bytes())
            }

            #get_in_doc
            #[inline]
            #get_in_sig {
                let buf = buf.as_ref();
                let ::core::ops::Range { start, end } = range;
                self.__pickmap_get_bytes(&buf[start..end])
            }
        };
    };

    let pw = pad.repr.words();
    let load = 8 * pw;
    let (pad_tables, pad_slot_stmts, pad_find) = table_parts(input, pad, p, keys_name);
    // Builds with AVX-512BW+VL read every key with one masked load straight from the key
    // (`load::masked_key`), into the same zero-padded value `get_in`'s tables use. Not for
    // PtHash maps of 2 or 4 words: their hash needs the vector's upper lanes, and moving
    // those to general registers sits on the critical path (misses measured 15-25% slower).
    let use_avx512 = !(pad.tagged() && pw >= 2);
    let avx512 = if use_avx512 {
        quote! {
            all(target_arch = "x86_64", target_feature = "avx512bw", target_feature = "avx512vl", not(miri))
        }
    } else {
        quote!(any())
    };
    quote! {
        #get_bytes

        // The lookup over zero-padded keys, shared by `get_in` and AVX-512 `get`.
        #[inline(always)]
        fn __pickmap_get_padded(
            &self,
            k: &[::core::primitive::u8],
            v: [::core::primitive::u64; #pw],
        ) -> ::core::option::Option<&'static #ty> {
            #pad_tables
            #pad_slot_stmts
            match #pad_find {
                ::core::option::Option::Some(i) => ::core::option::Option::Some(&#values_name[i]),
                ::core::option::Option::None => ::core::option::Option::None,
            }
        }

        #get_doc
        #[cfg(#avx512)]
        #[inline]
        pub fn get(&self, key: &::core::primitive::str) -> ::core::option::Option<&'static #ty> {
            let k = key.as_bytes();
            self.__pickmap_get_padded(k, #p::masked_key::<#pw>(k))
        }

        #get_doc
        #[cfg(not(#avx512))]
        #[inline]
        pub fn get(&self, key: &::core::primitive::str) -> ::core::option::Option<&'static #ty> {
            self.__pickmap_get_bytes(key.as_bytes())
        }

        #get_in_doc
        #[cfg(#avx512)]
        #[inline]
        #get_in_sig {
            let buf = buf.as_ref();
            let ::core::ops::Range { start, end } = range;
            let k = &buf[start..end];
            self.__pickmap_get_padded(k, #p::masked_key::<#pw>(k))
        }

        #get_in_doc
        #[cfg(not(#avx512))]
        #[inline]
        #get_in_sig {
            let buf = buf.as_ref();
            let ::core::ops::Range { start, end } = range;
            let k = &buf[start..end];
            // One wide load of #load bytes from `buf`, when that stays inside it; near the
            // end, `get`'s lookup (invalid UTF-8 never equals a key, so no check is needed).
            if buf.len() - start < #load {
                return self.__pickmap_get_bytes(k);
            }
            self.__pickmap_get_padded(k, #p::masked_in::<#pw>(buf, start, k.len()))
        }
    }
}

/// For one plan: its tables, the statements computing `slot` (and `meta` on tagged maps)
/// from `k` and `v`, and the expression that finds the key index in `SLOTS[slot]`.
fn table_parts(
    input: &MapInput,
    plan: &Plan,
    p: &TokenStream,
    keys_name: &proc_macro2::Ident,
) -> (TokenStream, TokenStream, TokenStream) {
    let words = plan.repr.words();
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

    let (strategy_tables, slot_stmts) = match &plan.strategy {
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

    // The workload only changes picked-byte maps: PtHash maps' tagged compare is already
    // as fast as early exits, without their risk.
    let early = input.workload != Workload::General;
    let find = match (plan.tagged(), plan.has_long) {
        (false, false) if early => quote!(SLOTS[slot].find_early(&v, k.len())),
        (false, true) if early => quote!(SLOTS[slot].find_early_long(&v, k, &#keys_name)),
        (false, false) => quote!(SLOTS[slot].find(&v, k.len())),
        (false, true) => quote!(SLOTS[slot].find_long(&v, k, &#keys_name)),
        (true, false) => quote!(SLOTS[slot].find_tagged(&v, meta)),
        (true, true) => quote!(SLOTS[slot].find_tagged_long(&v, meta, k, &#keys_name)),
    };
    (
        quote! {
            #strategy_tables
            static SLOTS: [#p::Slot<#words>; #table] = [#(#slots),*];
        },
        slot_stmts,
        find,
    )
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
