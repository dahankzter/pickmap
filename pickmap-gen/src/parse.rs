//! Parsing `static_map!` input: `[vis] NAME: Type; "key" => value, ...`.
//!
//! Hand-written over `proc-macro2` tokens, so the macro doesn't depend on `syn`.

use std::collections::HashMap;

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::quote_spanned;

/// What lookups mostly look like, from `#[pickmap(workload = "...")]`. A performance
/// hint only: every workload gives the same answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Workload {
    /// Any input, including near-misses and input from users or the network.
    #[default]
    General,
    /// Nearly every lookup finds a key.
    MostlyHits,
    /// Most lookups miss with words that look nothing like the keys (lexers checking
    /// identifiers for keywords).
    KeywordScan,
}

pub struct MapInput {
    /// Attributes for the generated static, doc comments included.
    pub attrs: Vec<TokenStream>,
    /// `#[cfg]` and `#[cfg_attr]` attributes, for every generated item.
    pub cfg_attrs: Vec<TokenStream>,
    pub workload: Workload,
    pub vis: TokenStream,
    pub name: Ident,
    pub value_ty: TokenStream,
    pub entries: Vec<Entry>,
}

pub struct Entry {
    pub key: String,
    pub key_span: Span,
    pub value: TokenStream,
}

pub struct Error {
    pub span: Span,
    pub message: String,
}

impl Error {
    fn new(span: Span, message: impl Into<String>) -> Self {
        Error { span, message: message.into() }
    }

    pub fn to_compile_error(&self) -> TokenStream {
        let msg = &self.message;
        quote_spanned! { self.span => ::core::compile_error!(#msg); }
    }
}

struct Cursor {
    tokens: Vec<TokenTree>,
    at: usize,
}

impl Cursor {
    fn peek(&self) -> Option<&TokenTree> {
        self.tokens.get(self.at)
    }

    fn peek_at(&self, ahead: usize) -> Option<&TokenTree> {
        self.tokens.get(self.at + ahead)
    }

    fn next(&mut self) -> Option<TokenTree> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }

    /// The span of the next token, or of the last one at the end of input.
    fn span(&self) -> Span {
        self.peek().or(self.tokens.last()).map_or_else(Span::call_site, TokenTree::span)
    }

    fn is_punct(&self, ahead: usize, c: char) -> bool {
        matches!(self.peek_at(ahead), Some(TokenTree::Punct(p)) if p.as_char() == c)
    }
}

pub fn parse(input: TokenStream) -> Result<MapInput, Error> {
    let mut c = Cursor { tokens: input.into_iter().collect(), at: 0 };

    let mut attrs = Vec::new();
    let mut cfg_attrs = Vec::new();
    let mut workload = None;
    while c.is_punct(0, '#') {
        let hash = c.next().unwrap();
        let group = match c.next() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => g,
            _ => return Err(Error::new(hash.span(), "expected `[` after `#`")),
        };
        let mut inner = group.stream().into_iter();
        match inner.next() {
            Some(TokenTree::Ident(i)) if i == "pickmap" => parse_options(inner, &mut workload)?,
            Some(TokenTree::Ident(i)) if i == "cfg" || i == "cfg_attr" => {
                cfg_attrs.push(TokenStream::from_iter([hash, TokenTree::Group(group)]))
            }
            _ => attrs.push(TokenStream::from_iter([hash, TokenTree::Group(group)])),
        }
    }

    let mut vis = TokenStream::new();
    if matches!(c.peek(), Some(TokenTree::Ident(i)) if i == "pub") {
        vis.extend(c.next());
        if matches!(c.peek(), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
            vis.extend(c.next());
        }
    }
    let name = match c.peek() {
        Some(TokenTree::Ident(i)) => i.clone(),
        _ => return Err(Error::new(c.span(), "expected a map name, like `pub NAME: Type;`")),
    };
    c.next();
    // A lone `:`, not the start of `::`.
    if !c.is_punct(0, ':') || c.is_punct(1, ':') {
        return Err(Error::new(c.span(), "expected `:`"));
    }
    c.next();
    let mut value_ty = TokenStream::new();
    while !c.is_punct(0, ';') {
        match c.next() {
            Some(t) => value_ty.extend([t]),
            None => return Err(Error::new(c.span(), "expected `;` after the value type")),
        }
    }
    c.next();

    let mut entries = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    while let Some(t) = c.next() {
        let key_span = t.span();
        let key = match &t {
            TokenTree::Literal(lit) => match litrs::StringLit::try_from(lit) {
                // Rust rejects suffixes on string literals; so do keys.
                Ok(s) if !s.suffix().is_empty() => {
                    let msg = format!("keys can't have a suffix: write {:?}, not {lit}", s.value());
                    return Err(Error::new(key_span, msg));
                }
                Ok(s) => s.value().to_owned(),
                Err(_) => return Err(Error::new(key_span, "keys must be string literals")),
            },
            _ => return Err(Error::new(key_span, "keys must be string literals")),
        };
        if !(c.is_punct(0, '=') && c.is_punct(1, '>')) {
            return Err(Error::new(c.span(), "expected `=>` after the key"));
        }
        let arrow = c.span();
        c.next();
        c.next();
        let value = value_tokens(&mut c);
        if value.is_empty() {
            return Err(Error::new(arrow, "expected a value expression"));
        }
        if c.is_punct(0, ',') {
            c.next();
        }
        if seen.insert(key.clone(), ()).is_some() {
            return Err(Error::new(key_span, format!("duplicate key {key:?}")));
        }
        entries.push(Entry { key, key_span, value });
    }
    Ok(MapInput { attrs, cfg_attrs, workload: workload.unwrap_or_default(), vis, name, value_ty, entries })
}

/// The `(workload = "...")` part of `#[pickmap(...)]`.
fn parse_options(mut tokens: impl Iterator<Item = TokenTree>, workload: &mut Option<Workload>) -> Result<(), Error> {
    let group = match tokens.next() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => g,
        other => {
            let span = other.map_or_else(Span::call_site, |t| t.span());
            return Err(Error::new(span, "expected `pickmap(workload = \"...\")`"));
        }
    };
    let mut c = Cursor { tokens: group.stream().into_iter().collect(), at: 0 };
    while let Some(t) = c.next() {
        let TokenTree::Ident(key) = &t else {
            return Err(Error::new(t.span(), "expected `workload`"));
        };
        if key != "workload" {
            return Err(Error::new(key.span(), format!("unknown pickmap option `{key}`; expected `workload`")));
        }
        if !c.is_punct(0, '=') {
            return Err(Error::new(c.span(), "expected `=` after `workload`"));
        }
        c.next();
        let value = match c.next() {
            Some(TokenTree::Literal(lit)) => match litrs::StringLit::try_from(&lit) {
                Ok(s) => (s.value().to_owned(), lit.span()),
                Err(_) => return Err(Error::new(lit.span(), "expected a string, like `workload = \"keyword_scan\"`")),
            },
            other => {
                let span = other.map_or_else(|| key.span(), |t| t.span());
                return Err(Error::new(span, "expected a string, like `workload = \"keyword_scan\"`"));
            }
        };
        let parsed = match value.0.as_str() {
            "general" => Workload::General,
            "mostly_hits" => Workload::MostlyHits,
            "keyword_scan" => Workload::KeywordScan,
            other => {
                return Err(Error::new(
                    value.1,
                    format!("unknown workload {other:?}; expected \"general\", \"mostly_hits\" or \"keyword_scan\""),
                ));
            }
        };
        if workload.replace(parsed).is_some() {
            return Err(Error::new(key.span(), "duplicate `workload` option"));
        }
        if c.is_punct(0, ',') {
            c.next();
        }
    }
    Ok(())
}

/// Tokens up to the comma that ends this entry: one followed by the end of input, or by
/// a single token and `=>` (the next key). Commas anywhere else belong to the value, as
/// in closures (`|a, b| ...`), qualified paths (`<G<A, B>>::N`) or turbofish generics.
fn value_tokens(c: &mut Cursor) -> TokenStream {
    let mut out = TokenStream::new();
    while c.peek().is_some() {
        if c.is_punct(0, ',') && (c.peek_at(1).is_none() || (c.is_punct(2, '=') && c.is_punct(3, '>'))) {
            break;
        }
        out.extend(c.next());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> Result<MapInput, Error> {
        parse(src.parse().expect("tokens"))
    }

    #[test]
    fn parse_basic() {
        let m = p(r#"pub(crate) M: u8; "a" => 1, "b" => 2,"#).unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(m.vis.to_string(), "pub (crate)");
        assert_eq!(m.name.to_string(), "M");
        assert_eq!(m.value_ty.to_string(), "u8");
        let keys: Vec<&str> = m.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["a", "b"]);
        assert_eq!(m.entries[1].value.to_string(), "2");
    }

    #[test]
    fn parse_turbofish_value() {
        let m = p(r#"M: V; "a" => Foo::<A, B>::NEW, "b" => [1, 2]"#).unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(m.entries.len(), 2);
        let squash = |t: &TokenStream| t.to_string().replace(' ', "");
        assert_eq!(squash(&m.entries[0].value), "Foo::<A,B>::NEW");
        assert_eq!(squash(&m.entries[1].value), "[1,2]");
    }

    #[test]
    fn parse_commas_inside_values() {
        let m = p(r#"M: V; "add" => |a, b| a + b, "q" => <G<u8, u16>>::N, "f" => G::<fn() -> u8, u16>::N, "z" => 1"#)
            .unwrap_or_else(|e| panic!("{}", e.message));
        let keys: Vec<&str> = m.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["add", "q", "f", "z"]);
        let squash = |t: &TokenStream| t.to_string().replace(' ', "");
        assert_eq!(squash(&m.entries[0].value), "|a,b|a+b");
        assert_eq!(squash(&m.entries[1].value), "<G<u8,u16>>::N");
        assert_eq!(squash(&m.entries[2].value), "G::<fn()->u8,u16>::N");
    }

    #[test]
    fn parse_escaped_keys() {
        let m = p(r###"M: u8; "a\"b" => 1, r#"x"y"# => 2, "\u{e9}" => 3"###).unwrap_or_else(|e| panic!("{}", e.message));
        let keys: Vec<&str> = m.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["a\"b", "x\"y", "é"]);
    }

    #[test]
    fn parse_errors() {
        for (src, msg) in [
            ("", "expected a map name, like `pub NAME: Type;`"),
            ("pub", "expected a map name, like `pub NAME: Type;`"),
            ("M u8;", "expected `:`"),
            ("M: u8", "expected `;` after the value type"),
            ("M: u8; 1 => 2", "keys must be string literals"),
            (r#"M: u8; b"a" => 2"#, "keys must be string literals"),
            (r#"M: u8; "a" 1"#, "expected `=>` after the key"),
            (r#"M: u8; "a" =>"#, "expected a value expression"),
            (r#"M: u8; "a" => , "b" => 1"#, "expected a value expression"),
            (r#"M: u8; "a" => 1, "a" => 2"#, "duplicate key \"a\""),
            (r#"M: u8; "q"abc => 1"#, "keys can't have a suffix: write \"q\", not \"q\"abc"),
            (r#"M: u8; r"q"x => 1"#, "keys can't have a suffix: write \"q\", not r\"q\"x"),
        ] {
            match p(src) {
                Ok(_) => panic!("{src:?} parsed"),
                Err(e) => assert_eq!(e.message, msg, "{src:?}"),
            }
        }
    }

    #[test]
    fn parse_attributes() {
        let m = p(r#"
            /// Doc line.
            #[cfg(feature = "x")]
            #[pickmap(workload = "keyword_scan")]
            #[allow(dead_code)]
            pub M: u8; "a" => 1
        "#)
        .unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(m.workload, Workload::KeywordScan);
        let squash = |ts: &[TokenStream]| ts.iter().map(|t| t.to_string().replace(' ', "")).collect::<Vec<_>>();
        assert_eq!(squash(&m.cfg_attrs), ["#[cfg(feature=\"x\")]"]);
        let attrs = squash(&m.attrs);
        assert_eq!(attrs.len(), 2);
        assert!(attrs[0].starts_with("#[doc="), "{attrs:?}");
        assert_eq!(attrs[1], "#[allow(dead_code)]");
        assert_eq!(m.vis.to_string(), "pub");
    }

    #[test]
    fn parse_workloads() {
        for (src, want) in [
            ("M: u8;", Workload::General),
            (r#"#[pickmap(workload = "general")] M: u8;"#, Workload::General),
            (r#"#[pickmap(workload = "mostly_hits")] M: u8;"#, Workload::MostlyHits),
            (r#"#[pickmap(workload = "keyword_scan")] M: u8;"#, Workload::KeywordScan),
        ] {
            assert_eq!(p(src).unwrap_or_else(|e| panic!("{}", e.message)).workload, want, "{src}");
        }
    }

    #[test]
    fn parse_attribute_errors() {
        for (src, msg) in [
            (r#"#[pickmap(workload = "fast")] M: u8;"#, "unknown workload \"fast\"; expected \"general\", \"mostly_hits\" or \"keyword_scan\""),
            (r#"#[pickmap(speed = "fast")] M: u8;"#, "unknown pickmap option `speed`; expected `workload`"),
            (r#"#[pickmap(workload = fast)] M: u8;"#, "expected a string, like `workload = \"keyword_scan\"`"),
            (r#"#[pickmap(workload = "general")] #[pickmap(workload = "general")] M: u8;"#, "duplicate `workload` option"),
            ("# M: u8;", "expected `[` after `#`"),
        ] {
            match p(src) {
                Ok(_) => panic!("{src:?} parsed"),
                Err(e) => assert_eq!(e.message, msg, "{src:?}"),
            }
        }
    }

    #[test]
    fn parse_empty_map() {
        assert_eq!(p("M: u8;").unwrap_or_else(|e| panic!("{}", e.message)).entries.len(), 0);
    }

    #[test]
    fn error_becomes_compile_error() {
        let e = p("M u8;").err().unwrap();
        assert!(e.to_compile_error().to_string().contains("compile_error"));
    }
}
