use pickmap::{StaticMap, static_map};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
}

static_map! {
    METHODS: Method;
    "GET" => Method::Get,
    "HEAD" => Method::Head,
    "POST" => Method::Post,
    "PUT" => Method::Put,
    "DELETE" => Method::Delete,
    "CONNECT" => Method::Connect,
    "OPTIONS" => Method::Options,
    "TRACE" => Method::Trace,
    "PATCH" => Method::Patch,
}

#[test]
fn methods_lookup() {
    assert_eq!(METHODS.get("GET"), Some(&Method::Get));
    assert_eq!(METHODS.get("PATCH"), Some(&Method::Patch));
    for miss in ["PATCHX", "get", ""] {
        assert_eq!(METHODS.get(miss), None, "{miss:?}");
    }
    assert!(METHODS.contains_key("PUT"));
    assert_eq!(METHODS.len(), 9);
    assert!(!METHODS.is_empty());
    let order: Vec<&str> = METHODS.iter().map(|(k, _)| k).collect();
    assert_eq!(order, ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH"]);
}

pub struct Point {
    pub x: i32,
    pub label: &'static str,
}

impl Point {
    pub const fn new(x: i32, label: &'static str) -> Self {
        Point { x, label }
    }
}

static_map! { STRS: &'static str; "a" => "alpha", "b" => "beta" }
static_map! { TUPLES: (u8, &'static str); "one" => (1, "x"), "two" => (2, "y") }
static_map! { POINTS: Point; "p" => Point::new(3, "three") }

#[test]
fn values_any_static_type() {
    assert_eq!(STRS.get("b"), Some(&"beta"));
    assert_eq!(TUPLES.get("two"), Some(&(2, "y")));
    let p = POINTS.get("p").unwrap();
    assert_eq!((p.x, p.label), (3, "three"));
}

static_map! { ESCAPED: u8; "a\"b" => 1, r#"x"y"# => 2, "\u{e9}" => 3 }

#[test]
fn escaped_keys_lookup() {
    assert_eq!(ESCAPED.get("a\"b"), Some(&1));
    assert_eq!(ESCAPED.get("x\"y"), Some(&2));
    assert_eq!(ESCAPED.get("é"), Some(&3));
}

// Keys over 32 bytes sharing their first and last 16 bytes; two share the length too.
static_map! {
    LONG: u8;
    "AAAAAAAAAAAAAAAAxZZZZZZZZZZZZZZZZ" => 33,
    "AAAAAAAAAAAAAAAAyyyyyyyyZZZZZZZZZZZZZZZZ" => 40,
    "AAAAAAAAAAAAAAAAyyyyzyyyZZZZZZZZZZZZZZZZ" => 41,
    "AAAAAAAAAAAAAAAAwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwZZZZZZZZZZZZZZZZ" => 64,
    "ab" => 2,
}

#[test]
fn long_keys_lookup() {
    for (k, v) in LONG.iter() {
        assert_eq!(LONG.get(k), Some(v), "{k:?}");
    }
    assert_eq!(LONG.get("AAAAAAAAAAAAAAAAyyyyqyyyZZZZZZZZZZZZZZZZ"), None);
    assert_eq!(LONG.get(&"A".repeat(40)), None);
    assert_eq!(LONG.get("ab"), Some(&2));
}

static_map! { E: u8; }
static_map! { WITH_EMPTY: u8; "" => 1, "a" => 2 }

#[test]
fn empty_map_and_empty_key() {
    assert_eq!(E.len(), 0);
    assert!(E.is_empty());
    assert_eq!(E.get(""), None);
    assert_eq!(E.iter().count(), 0);
    assert_eq!(WITH_EMPTY.get(""), Some(&1));
    assert_eq!(WITH_EMPTY.get("a"), Some(&2));
    assert_eq!(WITH_EMPTY.get("b"), None);
}

#[test]
fn odd_inputs_miss() {
    for q in ["", "G", &"x".repeat(100), "GÉT", "日本語"] {
        assert_eq!(METHODS.get(q), None, "{q:?}");
    }
}

#[allow(dead_code, non_camel_case_types)]
mod shadow {
    use pickmap::static_map;

    static SLOTS: u8 = 0;
    static VALUES: u8 = 0;
    struct Slot;
    type Option = ();
    type Some = ();

    static_map! { A: u8; "a" => 1 }
    static_map! { B: u8; "b" => 2 }

    pub fn check() {
        assert_eq!(A.get("a"), ::core::option::Option::Some(&1));
        assert_eq!(B.get("b"), ::core::option::Option::Some(&2));
        assert_eq!(A.get("b"), ::core::option::Option::None);
    }
}

#[test]
fn two_maps_and_shadowing_names() {
    shadow::check();
}

fn count<M: StaticMap>(m: &M) -> usize {
    m.iter().count()
}

#[test]
fn trait_generic_use() {
    assert_eq!(count(&METHODS), 9);
    assert_eq!(count(&STRS), 2);
    assert_eq!(StaticMap::get(&METHODS, "HEAD"), Some(&Method::Head));
}

mod inner {
    pickmap::static_map! { pub INNER: u8; "k" => 7 }
}

#[test]
fn visibility() {
    assert_eq!(inner::INNER.get("k"), Some(&7));
}

mod user_names_in_values {
    use pickmap::static_map;

    static KEYS: [&str; 2] = ["user-x", "user-y"];
    static VALUES: [u8; 2] = [7, 8];

    static_map! { K: &'static str; "a" => KEYS[0], "b" => KEYS[1] }
    static_map! { V: u8; "a" => VALUES[1] }

    pub fn check() {
        assert_eq!(K.get("a"), Some(&"user-x"));
        assert_eq!(K.get("b"), Some(&"user-y"));
        assert_eq!(V.get("a"), Some(&8));
    }
}

#[test]
fn values_resolve_in_user_scope() {
    user_names_in_values::check();
}

#[deny(missing_docs)]
pub mod documented {
    //! A documented public map must pass `missing_docs`.

    pickmap::static_map! {
        /// HTTP methods, documented.
        pub DOC_METHODS: u8;
        "GET" => 1,
    }
}

// Compiled out entirely: the value would not compile if any item survived.
static_map! { #[cfg(any())] pub GONE: u8; "a" => does_not_exist() }
static_map! { #[cfg(all())] pub KEPT: u8; "a" => 1 }

#[test]
fn doc_and_cfg_attributes() {
    assert_eq!(documented::DOC_METHODS.get("GET"), Some(&1));
    assert_eq!(KEPT.get("a"), Some(&1));
}

static_map! {
    #[pickmap(workload = "keyword_scan")]
    KW_SCAN: Method;
    "GET" => Method::Get,
    "HEAD" => Method::Head,
    "POST" => Method::Post,
    "PUT" => Method::Put,
    "DELETE" => Method::Delete,
}

static_map! {
    #[pickmap(workload = "mostly_hits")]
    LONG_HITS: u8;
    "AAAAAAAAAAAAAAAAxZZZZZZZZZZZZZZZZ" => 33,
    "AAAAAAAAAAAAAAAAyyyyyyyyZZZZZZZZZZZZZZZZ" => 40,
    "AAAAAAAAAAAAAAAAyyyyzyyyZZZZZZZZZZZZZZZZ" => 41,
    "ab" => 2,
}

#[test]
fn workloads_give_the_same_answers() {
    for (k, v) in KW_SCAN.iter() {
        assert_eq!(KW_SCAN.get(k), Some(v));
    }
    for q in ["", "G", "GETX", "get", &"x".repeat(100), "GÉT"] {
        assert_eq!(KW_SCAN.get(q), None, "{q:?}");
    }
    for (k, v) in LONG_HITS.iter() {
        assert_eq!(LONG_HITS.get(k), Some(v), "{k:?}");
    }
    assert_eq!(LONG_HITS.get("AAAAAAAAAAAAAAAAyyyyqyyyZZZZZZZZZZZZZZZZ"), None);
    assert_eq!(LONG_HITS.get("abc"), None);
}

// Value types written without `'static`: references in a static are `'static` anyway.
static_map! { RAW: &[u8]; "a" => b"alpha", "b" => b"beta" }
static_map! { PAIRS: (&str, Option<&[u8]>); "x" => ("ex", None), "y" => ("why", Some(b"y")) }
static_map! { COWS: std::borrow::Cow<'_, str>; "c" => std::borrow::Cow::Borrowed("cow") }

#[test]
fn elided_lifetimes_in_value_type() {
    assert_eq!(RAW.get("b"), Some(&&b"beta"[..]));
    assert_eq!(PAIRS.get("y"), Some(&("why", Some(&b"y"[..]))));
    assert_eq!(COWS.get("c").map(|c| c.as_ref()), Some("cow"));
}

fn str_len(s: &str) -> usize {
    s.len()
}

static_map! { HANDLERS: fn(&str) -> usize; "len" => str_len }

#[test]
fn fn_pointer_values_keep_their_lifetimes() {
    let local = String::from("four");
    // Callable with a non-'static string: the signature was not rewritten.
    assert_eq!(HANDLERS.get("len").map(|f| f(&local)), Some(4));
}

fn find_in<'a>(src: &'a str, word: &str) -> std::ops::Range<usize> {
    let start = src.find(word).expect("word in source");
    start..start + word.len()
}

#[test]
fn get_in_finds_tokens_inside_a_buffer() {
    let src = "GET /index.html HTTP/1.1 then POST, DELETE, PATCHX and finally get";
    assert_eq!(METHODS.get_in(src, find_in(src, "GET")), Some(&Method::Get));
    assert_eq!(METHODS.get_in(src, find_in(src, "POST")), Some(&Method::Post));
    assert_eq!(METHODS.get_in(src, find_in(src, "DELETE")), Some(&Method::Delete));
    assert_eq!(METHODS.get_in(src, find_in(src, "PATCHX")), None);
    // A prefix of a longer word is its own (missing) key.
    assert_eq!(METHODS.get_in(src, find_in(src, "PATC")), None);
    // The last token sits at the very end: the fallback path.
    assert_eq!(METHODS.get_in(src, find_in(src, "get")), None);
    assert_eq!(METHODS.get_in("xxPUT", 2..5), Some(&Method::Put));
    // Byte buffers work too.
    let bytes: &[u8] = b"..DELETE....................................";
    assert_eq!(METHODS.get_in(bytes, 2..8), Some(&Method::Delete));
    // Not on a char boundary: no key matches a broken UTF-8 sequence.
    let accented = "é GET é GET é";
    assert_eq!(METHODS.get_in(accented, 1..2), None);
    // Every entry of a bigger map, found inside a padded buffer.
    for (k, v) in LONG.iter() {
        let buf = format!("<{k}>{}", " ".repeat(40));
        assert_eq!(LONG.get_in(&buf, 1..1 + k.len()), Some(v), "{k:?}");
    }
    assert_eq!(E.get_in("abc", 0..3), None);
    assert_eq!(StaticMap::get_in(&METHODS, src, find_in(src, "POST")), Some(&Method::Post));
}

#[test]
#[should_panic]
fn get_in_panics_on_out_of_bounds_range() {
    let _ = METHODS.get_in("GET", 0..10);
}
