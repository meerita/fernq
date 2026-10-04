//! The equivalence adapter: the token contract of the Fernq lexer, computed
//! over `rustc_lexer` of rustc 1.99.0.
//!
//! [`lex`] gives the classes and byte ranges that `fernq::bench::tokens`
//! gives, or `None` where Fernq ends lexing with an error. It does the work of
//! that contract on top of `rustc_lexer::Cursor`:
//!
//! - removes a byte order mark at offset 0, as rustc does before lexing, then
//!   a shebang with `strip_shebang`;
//! - skips whitespace and non-doc comments; rejects doc comments, which
//!   Fernq does not support, and unterminated block comments;
//! - rejects unknown characters, identifiers with emoji, unknown prefixes and
//!   raw lifetimes before 2021, guarded string prefixes from 2024, lifetimes
//!   that start with a digit, and the raw names `_`, `crate`, `self`, `Self`,
//!   and `super`; before 2021, splits the C string, unknown prefix, raw
//!   lifetime, and guarded string tokens of `rustc_lexer` as Fernq lexes them;
//! - rejects ZWJ and ZWNJ in identifiers, lifetimes, and suffixes, and the
//!   suffix `_` alone, as Fernq does;
//! - validates every literal: escapes and content with
//!   `rustc-literal-escaper` (`unescape_char`, `unescape_byte`, and
//!   `check_for_errors`, which runs `unescape_*` and `check_raw_*`), empty
//!   integers and exponents, digits outside the radix, and binary or octal
//!   floats; CR LF inside a literal is accepted, as Fernq accepts it;
//! - classifies keywords by edition with a `match`;
//! - glues adjacent single-character punctuation, left to right, into the
//!   compound punctuation of the Rust Reference with a `match`;
//! - gives each token to a [`Sink`]. It allocates nothing per token.
//!
//! It does not intern or NFC-normalize identifiers, lint bidirectional
//! characters, pair delimiters, or build diagnostics: the rustc lexer layer
//! does, and the Fernq lexer does not.
//!
//! The adapter is frozen: its timing is comparable only with the work list
//! above. A change to it is a new benchmark question with a new baseline.

use std::ops::Range;

use fernq::bench::TokenClass;
use memchr::memmem;
use ra_ap_rustc_lexer::{Base, Cursor, FrontmatterAllowed, LiteralKind, TokenKind};
use rustc_literal_escaper::{self as escaper, EscapeError, Mode};

/// A Rust edition, as the year that names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Edition {
    E2015,
    E2018,
    E2021,
    E2024,
}

impl Edition {
    pub fn from_year(year: u16) -> Option<Self> {
        match year {
            2015 => Some(Self::E2015),
            2018 => Some(Self::E2018),
            2021 => Some(Self::E2021),
            2024 => Some(Self::E2024),
            _ => None,
        }
    }
}

/// Receives each token, in order.
pub trait Sink {
    fn token(&mut self, class: TokenClass, lo: u32, hi: u32);
}

/// Folds the tokens as `fernq::bench::lex` does: their number and the hash
/// of their spans.
#[derive(Default)]
pub struct HashSink {
    pub count: usize,
    pub hash: u64,
}

impl Sink for HashSink {
    fn token(&mut self, _class: TokenClass, lo: u32, hi: u32) {
        self.hash = self.hash.wrapping_mul(0x100_0000_01b3) ^ (u64::from(lo) << 32 | u64::from(hi));
        self.count += 1;
    }
}

/// The token being glued: punctuation that may still extend.
#[derive(Clone, Copy)]
struct Pending {
    lo: usize,
    hi: usize,
}

/// Lexes `text` in `edition` to its end and gives each token to `sink`.
/// Returns `None` where the Fernq lexer ends with an error; the sink has then
/// received the tokens before it.
///
/// # Panics
///
/// Panics if `text` is longer than `u32::MAX` bytes.
pub fn lex(text: &str, edition: Edition, sink: &mut impl Sink) -> Option<()> {
    assert!(u32::try_from(text.len()).is_ok(), "a text fits u32 offsets");
    let bom = if text.starts_with('\u{FEFF}') {
        '\u{FEFF}'.len_utf8()
    } else {
        0
    };
    let start = bom + ra_ap_rustc_lexer::strip_shebang(&text[bom..]).unwrap_or(0);
    let since_2021 = edition >= Edition::E2021;
    // Only a text that holds ZWNJ or ZWJ can have one in a token: one scan of
    // the text spares a scan of each token.
    let joiners = memmem::find(text.as_bytes(), "\u{200C}".as_bytes()).is_some()
        || memmem::find(text.as_bytes(), "\u{200D}".as_bytes()).is_some();
    let mut pending: Option<Pending> = None;
    let mut pos = start;
    // Where `rustc_lexer` and Fernq split differently, a new cursor starts at
    // Fernq's next token. The cursor is reassigned only here: reassigning it
    // inside the token loop measured slower.
    'cursor: loop {
        let mut cursor = Cursor::new(&text[pos..], FrontmatterAllowed::No);
        loop {
            let token = cursor.advance_token();
            let lo = pos;
            let hi = lo + token.len as usize;
            pos = hi;
            let mut restart = None;
            let class = match token.kind {
                TokenKind::Whitespace | TokenKind::LineComment { doc_style: None } => continue,
                TokenKind::BlockComment {
                    doc_style: None,
                    terminated: true,
                } => continue,
                TokenKind::LineComment { doc_style: Some(_) }
                | TokenKind::BlockComment { .. }
                | TokenKind::Frontmatter { .. }
                | TokenKind::InvalidIdent
                | TokenKind::Unknown
                | TokenKind::Lifetime {
                    starts_with_number: true,
                } => return None,
                TokenKind::Eof => break 'cursor,
                kind if punctuation_char(kind).is_some() => {
                    match pending {
                        Some(p) if p.hi == lo && is_compound(&text.as_bytes()[p.lo..hi]) => {
                            pending = Some(Pending { lo: p.lo, hi });
                        }
                        _ => {
                            if let Some(p) = pending.replace(Pending { lo, hi }) {
                                emit(sink, TokenClass::Punctuation, p.lo, p.hi);
                            }
                        }
                    }
                    continue;
                }
                TokenKind::GuardedStrPrefix => {
                    if edition >= Edition::E2024 {
                        return None;
                    }
                    // `#` then `"` or `#`: a `#` token, and lexing resumes after it.
                    if let Some(p) = pending.replace(Pending { lo, hi: lo + 1 }) {
                        emit(sink, TokenClass::Punctuation, p.lo, p.hi);
                    }
                    pos = lo + 1;
                    continue 'cursor;
                }
                TokenKind::Ident | TokenKind::UnknownPrefix => {
                    if token.kind == TokenKind::UnknownPrefix && since_2021 {
                        return None;
                    }
                    let name = &text[lo..hi];
                    if has_zero_width_joiner(joiners, name) {
                        return None;
                    }
                    if is_keyword(name.as_bytes(), edition) {
                        TokenClass::Keyword
                    } else {
                        TokenClass::Identifier
                    }
                }
                TokenKind::RawIdent => {
                    let name = &text[lo + 2..hi];
                    if is_reserved_raw_name(name) || has_zero_width_joiner(joiners, name) {
                        return None;
                    }
                    TokenClass::RawIdentifier
                }
                TokenKind::Lifetime {
                    starts_with_number: false,
                }
                | TokenKind::UnknownPrefixLifetime => {
                    if token.kind == TokenKind::UnknownPrefixLifetime && since_2021 {
                        return None;
                    }
                    if has_zero_width_joiner(joiners, &text[lo..hi]) {
                        return None;
                    }
                    TokenClass::Lifetime
                }
                TokenKind::RawLifetime if since_2021 => {
                    let name = &text[lo + 3..hi];
                    if is_reserved_raw_name(name) || has_zero_width_joiner(joiners, name) {
                        return None;
                    }
                    TokenClass::RawLifetime
                }
                TokenKind::RawLifetime => {
                    // `'r`, then `#` and the name.
                    restart = Some(lo + 2);
                    TokenClass::Lifetime
                }
                TokenKind::Literal {
                    kind: LiteralKind::CStr { .. },
                    ..
                } if !since_2021 => {
                    // `c`, then the string.
                    restart = Some(lo + 1);
                    TokenClass::Identifier
                }
                TokenKind::Literal {
                    kind: LiteralKind::RawCStr { .. },
                    ..
                } if !since_2021 => {
                    // `cr`, then `#` or the string.
                    restart = Some(lo + 2);
                    TokenClass::Identifier
                }
                TokenKind::Literal { kind, suffix_start } => {
                    let suffix = lo + suffix_start as usize;
                    literal(text, lo, suffix, hi, kind, joiners)?
                }
                TokenKind::OpenParen | TokenKind::OpenBrace | TokenKind::OpenBracket => {
                    TokenClass::OpenDelimiter
                }
                TokenKind::CloseParen | TokenKind::CloseBrace | TokenKind::CloseBracket => {
                    TokenClass::CloseDelimiter
                }
                _ => return None,
            };
            if let Some(p) = pending.take() {
                emit(sink, TokenClass::Punctuation, p.lo, p.hi);
            }
            match restart {
                Some(next) => {
                    emit(sink, class, lo, next);
                    pos = next;
                    continue 'cursor;
                }
                None => emit(sink, class, lo, hi),
            }
        }
    }
    if let Some(p) = pending {
        emit(sink, TokenClass::Punctuation, p.lo, p.hi);
    }
    Some(())
}

/// Gives `lo..hi` to `sink`; both ends are at most the length of a text
/// that [`lex`] admitted, which fits a `u32`.
fn emit(sink: &mut impl Sink, class: TokenClass, lo: usize, hi: usize) {
    sink.token(class, lo as u32, hi as u32);
}

/// Validates the literal `lo..hi` whose suffix starts at `suffix`, and
/// returns its class.
fn literal(
    text: &str,
    lo: usize,
    suffix: usize,
    hi: usize,
    kind: LiteralKind,
    joiners: bool,
) -> Option<TokenClass> {
    let body = &text[lo..suffix];
    let suffix = &text[suffix..hi];
    if suffix == "_" || has_zero_width_joiner(joiners, suffix) {
        return None;
    }
    let quoted = |prefix: usize, pounds: usize| &body[prefix + pounds + 1..body.len() - 1 - pounds];
    let (class, mode, content) = match kind {
        LiteralKind::Int {
            empty_int: true, ..
        } => return None,
        LiteralKind::Int { base, .. } => {
            let radix = match base {
                Base::Binary => b'2',
                Base::Octal => b'8',
                Base::Decimal | Base::Hexadecimal => return Some(TokenClass::IntegerLiteral),
            };
            // `rustc_lexer` reads decimal digits after `0b` and `0o`.
            let in_radix = body.as_bytes()[2..].iter().all(|&b| b == b'_' || b < radix);
            return in_radix.then_some(TokenClass::IntegerLiteral);
        }
        LiteralKind::Float {
            base: Base::Decimal,
            empty_exponent: false,
        } => {
            return Some(TokenClass::FloatLiteral);
        }
        LiteralKind::Float { .. } => return None,
        LiteralKind::Char { terminated: true } => {
            let ok = escaper::unescape_char(quoted(0, 0)).is_ok();
            return ok.then_some(TokenClass::CharLiteral);
        }
        LiteralKind::Byte { terminated: true } => {
            let ok = escaper::unescape_byte(quoted(1, 0)).is_ok();
            return ok.then_some(TokenClass::ByteLiteral);
        }
        LiteralKind::Str { terminated: true } => {
            (TokenClass::StringLiteral, Mode::Str, quoted(0, 0))
        }
        LiteralKind::ByteStr { terminated: true } => {
            (TokenClass::ByteStringLiteral, Mode::ByteStr, quoted(1, 0))
        }
        LiteralKind::CStr { terminated: true } => {
            (TokenClass::CStringLiteral, Mode::CStr, quoted(1, 0))
        }
        LiteralKind::RawStr { n_hashes: Some(n) } => (
            TokenClass::RawStringLiteral,
            Mode::RawStr,
            quoted(1, usize::from(n)),
        ),
        LiteralKind::RawByteStr { n_hashes: Some(n) } => (
            TokenClass::RawByteStringLiteral,
            Mode::RawByteStr,
            quoted(2, usize::from(n)),
        ),
        LiteralKind::RawCStr { n_hashes: Some(n) } => (
            TokenClass::RawCStringLiteral,
            Mode::RawCStr,
            quoted(2, usize::from(n)),
        ),
        _ => return None,
    };
    let mut ok = true;
    escaper::check_for_errors(content, mode, |range, error| {
        ok &= accepts(content, range, error)
    });
    ok.then_some(class)
}

/// Whether the escape error at `range` of `content` leaves the literal valid
/// for Fernq: a warning, or a CR that LF follows.
fn accepts(content: &str, range: Range<usize>, error: EscapeError) -> bool {
    let lf_follows = content.as_bytes().get(range.end) == Some(&b'\n');
    match error {
        _ if !error.is_fatal() => true,
        EscapeError::BareCarriageReturn | EscapeError::BareCarriageReturnInRawString => lf_follows,
        // `\` and CR LF: rustc sees `\` and LF once it has normalized line ends.
        EscapeError::InvalidEscape => lf_follows && &content[range] == "\\\r",
        _ => false,
    }
}

/// The character of a single-character punctuation token.
fn punctuation_char(kind: TokenKind) -> Option<u8> {
    Some(match kind {
        TokenKind::Semi => b';',
        TokenKind::Comma => b',',
        TokenKind::Dot => b'.',
        TokenKind::At => b'@',
        TokenKind::Pound => b'#',
        TokenKind::Tilde => b'~',
        TokenKind::Question => b'?',
        TokenKind::Colon => b':',
        TokenKind::Dollar => b'$',
        TokenKind::Eq => b'=',
        TokenKind::Bang => b'!',
        TokenKind::Lt => b'<',
        TokenKind::Gt => b'>',
        TokenKind::Minus => b'-',
        TokenKind::And => b'&',
        TokenKind::Or => b'|',
        TokenKind::Plus => b'+',
        TokenKind::Star => b'*',
        TokenKind::Slash => b'/',
        TokenKind::Caret => b'^',
        TokenKind::Percent => b'%',
        _ => return None,
    })
}

/// Whether `text` is compound punctuation of the Rust Reference. Each
/// three-character compound starts with a two-character one, so left-to-right
/// gluing finds the longest token.
fn is_compound(text: &[u8]) -> bool {
    matches!(
        text,
        b"..."
            | b"..="
            | b"<<="
            | b">>="
            | b"!="
            | b"%="
            | b"&&"
            | b"&="
            | b"*="
            | b"+="
            | b"-="
            | b"->"
            | b".."
            | b"/="
            | b"::"
            | b"<-"
            | b"<<"
            | b"<="
            | b"=="
            | b"=>"
            | b">="
            | b">>"
            | b"^="
            | b"|="
            | b"||"
    )
}

/// Whether `name` is a strict or reserved keyword in `edition`.
fn is_keyword(name: &[u8], edition: Edition) -> bool {
    match name {
        b"as" | b"break" | b"const" | b"continue" | b"crate" | b"else" | b"enum" | b"extern"
        | b"false" | b"fn" | b"for" | b"if" | b"impl" | b"in" | b"let" | b"loop" | b"match"
        | b"mod" | b"move" | b"mut" | b"pub" | b"ref" | b"return" | b"self" | b"Self"
        | b"static" | b"struct" | b"super" | b"trait" | b"true" | b"type" | b"unsafe" | b"use"
        | b"where" | b"while" | b"_" | b"abstract" | b"become" | b"box" | b"do" | b"final"
        | b"macro" | b"override" | b"priv" | b"typeof" | b"unsized" | b"virtual" | b"yield" => true,
        b"async" | b"await" | b"dyn" | b"try" => edition >= Edition::E2018,
        b"gen" => edition >= Edition::E2024,
        _ => false,
    }
}

fn is_reserved_raw_name(name: &str) -> bool {
    matches!(name, "_" | "crate" | "self" | "Self" | "super")
}

/// Whether `name` holds ZWNJ (U+200C) or ZWJ (U+200D), given whether its
/// text holds either, `joiners`.
fn has_zero_width_joiner(joiners: bool, name: &str) -> bool {
    joiners && name.contains(['\u{200C}', '\u{200D}'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Tokens(Vec<(TokenClass, u32, u32)>);

    impl Sink for Tokens {
        fn token(&mut self, class: TokenClass, lo: u32, hi: u32) {
            self.0.push((class, lo, hi));
        }
    }

    fn tokens(text: &str, edition: Edition) -> Option<Vec<(TokenClass, u32, u32)>> {
        let mut sink = Tokens::default();
        lex(text, edition, &mut sink).map(|()| sink.0)
    }

    /// The token texts of `text`, or `None` on an error.
    fn texts(text: &str, edition: Edition) -> Option<Vec<&str>> {
        tokens(text, edition).map(|tokens| {
            tokens
                .iter()
                .map(|&(_, lo, hi)| &text[lo as usize..hi as usize])
                .collect()
        })
    }

    /// The adapter and Fernq agree on `text` in every edition.
    fn agrees(text: &str) {
        for year in [2015, 2018, 2021, 2024] {
            let edition = Edition::from_year(year).unwrap();
            assert_eq!(
                tokens(text, edition),
                fernq::bench::tokens(text, year),
                "{year}: {text:?}"
            );
        }
    }

    #[test]
    fn glues_punctuation_left_to_right() {
        let e = Edition::E2024;
        assert_eq!(texts("&&&", e).unwrap(), ["&&", "&"]);
        assert_eq!(texts("<<=", e).unwrap(), ["<<="]);
        assert_eq!(texts("..=", e).unwrap(), ["..="]);
        assert_eq!(texts("...", e).unwrap(), ["..."]);
        assert_eq!(texts("->>", e).unwrap(), ["->", ">"]);
        assert_eq!(texts("<-", e).unwrap(), ["<-"]);
        assert_eq!(texts("::", e).unwrap(), ["::"]);
        assert_eq!(texts(": :", e).unwrap(), [":", ":"]);
        assert_eq!(texts("....", e).unwrap(), ["...", "."]);
        for text in [
            "&&&",
            "<<=",
            "..=",
            "...",
            "->>",
            "<-",
            "::",
            ": :",
            "a::<b>>=c",
            "x/ /y",
        ] {
            agrees(text);
        }
    }

    #[test]
    fn classifies_keywords_by_edition() {
        let class = |name: &str, edition| tokens(name, edition).unwrap()[0].0;
        assert_eq!(class("fn", Edition::E2015), TokenClass::Keyword);
        assert_eq!(class("async", Edition::E2015), TokenClass::Identifier);
        assert_eq!(class("async", Edition::E2018), TokenClass::Keyword);
        assert_eq!(class("dyn", Edition::E2015), TokenClass::Identifier);
        assert_eq!(class("try", Edition::E2018), TokenClass::Keyword);
        assert_eq!(class("gen", Edition::E2021), TokenClass::Identifier);
        assert_eq!(class("gen", Edition::E2024), TokenClass::Keyword);
        assert_eq!(class("union", Edition::E2024), TokenClass::Identifier);
        assert_eq!(class("_", Edition::E2015), TokenClass::Keyword);
        for text in [
            "async await dyn try gen union macro_rules raw safe _ Self self",
            "r#gen 'static",
        ] {
            agrees(text);
        }
    }

    #[test]
    fn rejects_invalid_literals() {
        for text in [
            "0b12",
            "0o8",
            "0x",
            "0b_",
            "0b1e5",
            "0b1.0",
            "0x1.",
            "1e",
            "1e+_",
            "1.0E",
            "'ab'",
            "''",
            "'\n'",
            "b'é'",
            "\"\\x80\"",
            "\"\\u{D800}\"",
            "\"\\u{}\"",
            "\"\\q\"",
            "b\"é\"",
            "b\"\\u{41}\"",
            "c\"\\0\"",
            "c\"\\x00\"",
            "c\"\\u{0}\"",
            "\"a\"_",
            "\"unterminated",
            "r\"open",
            "r#\"\"",
            "\"\r\"",
            "r\"\r\"",
        ] {
            assert_eq!(tokens(text, Edition::E2021), None, "{text:?}");
            agrees(text);
        }
    }

    #[test]
    fn accepts_valid_literals() {
        for text in [
            "0b1_0 0o7 0xFf 1_000u32 1.5e-3f64 1. 1.foo 1..2 0x1.e 2.max",
            "'a' '\\'' '\\u{10FFFF}' b'\\xFF' 'é'",
            "\"a\\\n   \n b\" \"\\x7F\" b\"\\xFF\" c\"\\xFF é\" r##\"\"# \"##",
            "\"crlf\r\nline\" r\"crlf\r\n\" \"cont\\\r\n x\"",
            "\"suffix\"_x 1u8 'a'b",
        ] {
            assert!(tokens(text, Edition::E2021).is_some(), "{text:?}");
            agrees(text);
        }
    }

    #[test]
    fn follows_fernq_on_edition_dependent_input() {
        for text in [
            "c\"s\" cr\"s\" cr#\"s\"#",
            "f\"s\" f'x f#x 'r#a 'a#b 'r#1",
            "#\"s\"# ##x # #",
            "r#_ r#self 'r#_ r#a",
            "\u{FEFF}#!/bin/run\nfn",
            "#![doc]",
            "a\u{200D}b",
            "/// doc",
            "/** doc */ /**/ /***/ //// x",
            "/* open",
            "`",
            "💩",
        ] {
            agrees(text);
        }
    }
}
