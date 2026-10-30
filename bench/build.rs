//! Generates the Rust competitor maps for every key set into `$OUT_DIR/competitors.rs`.

use std::fmt::Write;

fn main() {
    let dir = std::env::var("OUT_DIR").unwrap();
    let dir = std::path::Path::new(&dir);
    let mut out = String::new();
    let mut cpp = String::from(CPP_PRELUDE);
    for set in keysets::all() {
        emit_set(&mut out, set.name, &set.keys);
        emit_cpp(&mut cpp, set.name, &set.keys);
    }
    std::fs::write(dir.join("competitors.rs"), out).unwrap();
    std::fs::write(dir.join("lemire.cpp"), cpp).unwrap();
    println!("cargo::rerun-if-changed=cpp/vendor/perfect_hash/perfect_hash.h");
    cc::Build::new()
        .cpp(true)
        .compiler("clang++")
        .std("c++23")
        .opt_level(3)
        .flag("-fconstexpr-steps=1000000000")
        .include("cpp/vendor/perfect_hash")
        .file(dir.join("lemire.cpp"))
        .compile("lemire");
}

const CPP_PRELUDE: &str = r#"#include "perfect_hash.h"
#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string_view>
using namespace ConstexprCore;
"#;

/// Lemire's perfect_hash for one set, plus a batch loop identical to `bench::tput`.
fn emit_cpp(cpp: &mut String, name: &str, keys: &[String]) {
    writeln!(cpp, "namespace {name} {{").unwrap();
    if keys.len() <= 255 {
        let kvs: Vec<String> = keys.iter().enumerate().map(|(i, k)| format!("kv<{k:?}, {i}u>")).collect();
        writeln!(cpp, "constexpr auto map = make_perfect_map<{}>();", kvs.join(", ")).unwrap();
    } else {
        let ks: Vec<String> = keys.iter().map(|k| format!("{k:?}")).collect();
        let vs: Vec<String> = (0..keys.len()).map(|i| format!("{i}u")).collect();
        writeln!(cpp, "inline constexpr std::array<std::string_view, {}> keys = {{{}}};", keys.len(), ks.join(", ")).unwrap();
        writeln!(cpp, "inline constexpr std::array<std::uint32_t, {}> vals = {{{}}};", keys.len(), vs.join(", ")).unwrap();
        writeln!(cpp, "constexpr auto map = make_wide_perfect_map<keys, vals>();").unwrap();
    }
    writeln!(cpp, "}}").unwrap();
    writeln!(
        cpp,
        r#"extern "C" std::uint64_t lemire_{name}_tput(const char* const* p, const std::size_t* l, std::size_t n) {{
    std::uint64_t sum = 0;
    for (std::size_t i = 0; i < n; i++) sum += {name}::map.lookup(std::string_view(p[i], l[i])).value_or(UINT32_MAX);
    return sum;
}}
"#
    )
    .unwrap();
}

fn emit_set(out: &mut String, name: &str, keys: &[String]) {
    writeln!(out, "pub mod {name} {{").unwrap();
    writeln!(out, "    use std::collections::HashMap;").unwrap();
    writeln!(out, "    use std::sync::LazyLock;").unwrap();
    writeln!(out, "    pub static KEYS: [&str; {}] = {keys:?};", keys.len()).unwrap();

    // pickmap, through the real macro.
    writeln!(out, "    pickmap::static_map! {{\n        pub PICK: u32;").unwrap();
    for (i, k) in keys.iter().enumerate() {
        writeln!(out, "        {k:?} => {i},").unwrap();
    }
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    #[inline]\n    pub fn pick_get(k: &str) -> Option<u32> {{ PICK.get(k).copied() }}").unwrap();

    // The same map declared for a keyword-scan workload.
    writeln!(out, "    pickmap::static_map! {{\n        #[pickmap(workload = \"keyword_scan\")]\n        pub PICK_KW: u32;").unwrap();
    for (i, k) in keys.iter().enumerate() {
        writeln!(out, "        {k:?} => {i},").unwrap();
    }
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    #[inline]\n    pub fn pick_kw_get(k: &str) -> Option<u32> {{ PICK_KW.get(k).copied() }}").unwrap();

    // quickphf.
    let qkeys: Vec<&str> = keys.iter().map(String::as_str).collect();
    let qvals: Vec<u32> = (0..keys.len() as u32).collect();
    writeln!(
        out,
        "    pub static QUICKPHF: quickphf::PhfMap<&'static str, u32> = {};",
        quickphf_codegen::build_map(&qkeys, &qvals)
    )
    .unwrap();
    writeln!(out, "    #[inline]\n    pub fn quickphf_get(k: &str) -> Option<u32> {{ QUICKPHF.get(k).copied() }}").unwrap();

    // Plain `match`.
    writeln!(out, "    #[inline]\n    pub fn match_get(k: &str) -> Option<u32> {{\n        match k {{").unwrap();
    for (i, k) in keys.iter().enumerate() {
        writeln!(out, "            {k:?} => Some({i}),").unwrap();
    }
    writeln!(out, "            _ => None,\n        }}\n    }}").unwrap();

    // phf.
    let mut phf = phf_codegen::Map::new();
    for (i, k) in keys.iter().enumerate() {
        phf.entry(k.as_str(), i.to_string());
    }
    writeln!(out, "    pub static PHF: phf::Map<&'static str, u32> = {};", phf.build()).unwrap();
    writeln!(out, "    #[inline]\n    pub fn phf_get(k: &str) -> Option<u32> {{ PHF.get(k).copied() }}").unwrap();

    // frozen-collections.
    writeln!(out, "    frozen_collections::fz_string_map!(static FZ: FzMap<&'static str, u32>, {{").unwrap();
    for (i, k) in keys.iter().enumerate() {
        writeln!(out, "        {k:?}: {i},").unwrap();
    }
    writeln!(out, "    }});").unwrap();
    writeln!(out, "    #[inline]\n    pub fn fz_get(k: &str) -> Option<u32> {{ FZ.get(k).copied() }}").unwrap();

    // HashMap + foldhash.
    writeln!(
        out,
        "    pub static FOLD: LazyLock<HashMap<&'static str, u32, foldhash::fast::FixedState>> =\n        LazyLock::new(|| KEYS.iter().enumerate().map(|(i, k)| (*k, i as u32)).collect());"
    )
    .unwrap();
    writeln!(out, "    #[inline]\n    pub fn fold_get(k: &str) -> Option<u32> {{ FOLD.get(k).copied() }}").unwrap();
    writeln!(out, "}}").unwrap();
}
