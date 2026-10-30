//! Parsing `static_map!` input: `[vis] NAME: Type; "key" => value, ...`.
//!
//! Hand-written over `proc-macro2` tokens, so the macro doesn't depend on `syn`.

use std::collections::HashMap;

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::quote_spanned;

pub struct MapInput {
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
    Ok(MapInput { vis, name, value_ty, entries })
}

/// Tokens up to the next top-level `,`. A comma inside a turbofish (`::<A, B>`) is not
/// top level; commas inside groups are already nested in the group's token tree.
fn value_tokens(c: &mut Cursor) -> TokenStream {
    let mut out = TokenStream::new();
    let mut angle = 0usize;
    loop {
        if angle == 0 && c.is_punct(0, ',') {
            break;
        }
        if c.is_punct(0, ':') && c.is_punct(1, ':') && c.is_punct(2, '<') {
            angle += 1;
            out.extend([c.next().unwrap(), c.next().unwrap(), c.next().unwrap()]);
            continue;
        }
        if angle > 0 && c.is_punct(0, '<') {
            angle += 1;
        } else if angle > 0 && c.is_punct(0, '>') {
            angle -= 1;
        }
        match c.next() {
            Some(t) => out.extend([t]),
            None => break,
        }
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
