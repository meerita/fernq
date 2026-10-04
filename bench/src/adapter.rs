//! The equivalence adapter: the token contract of the Fernq lexer, computed
//! over `rustc_lexer` of rustc 1.99.0.
//!
//! [`lex`] gives the kinds and byte ranges that `fernq::bench::tokens`
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
//! - names keywords by edition with a `match`;
//! - glues adjacent single-character punctuation, left to right, into the
//!   compound punctuation of the Rust Reference with a `match` that names
//!   the compound token;
//! - gives each token to a [`Sink`] with its exact `fernq::bench::TokenKind`.
//!   It allocates nothing per token.
//!
//! The identity of a token comes from the `rustc_lexer` token kind and from
//! the two matches above, which the contract needs anyway; the adapter reads
//! no token text only to name a token.
//!
//! It does not intern or NFC-normalize identifiers, lint bidirectional
//! characters, pair delimiters, or build diagnostics: the rustc lexer layer
//! does, and the Fernq lexer does not.
//!
//! The adapter is frozen: its timing is comparable only with the work list
//! above. A change to it is a new benchmark question with a new baseline.

use std::ops::Range;

use fernq::bench::{Delimiter, Keyword, Punctuation, TokenKind as Kind};
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
    fn token(&mut self, kind: Kind, lo: u32, hi: u32);
}

/// Folds the tokens as `fernq::bench::lex` does: their number and
/// `fernq::bench::fold` of their kinds and spans.
#[derive(Default)]
pub struct HashSink {
    pub count: usize,
    pub hash: u64,
}

impl Sink for HashSink {
    fn token(&mut self, kind: Kind, lo: u32, hi: u32) {
        self.hash = fernq::bench::fold(self.hash, kind, lo, hi);
        self.count += 1;
    }
}

/// The token being glued: punctuation that may still extend.
#[derive(Clone, Copy)]
struct Pending {
    lo: usize,
    hi: usize,
    punctuation: Punctuation,
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
            let kind = match token.kind {
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
                kind if let Some(punctuation) = single_punctuation(kind) => {
                    match pending {
                        Some(p)
                            if p.hi == lo
                                && let Some(punctuation) = compound(&text.as_bytes()[p.lo..hi]) =>
                        {
                            pending = Some(Pending {
                                lo: p.lo,
                                hi,
                                punctuation,
                            });
                        }
                        _ => {
                            let next = Pending {
                                lo,
                                hi,
                                punctuation,
                            };
                            if let Some(p) = pending.replace(next) {
                                emit(sink, Kind::Punctuation(p.punctuation), p.lo, p.hi);
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
                    let pound = Pending {
                        lo,
                        hi: lo + 1,
                        punctuation: Punctuation::Pound,
                    };
                    if let Some(p) = pending.replace(pound) {
                        emit(sink, Kind::Punctuation(p.punctuation), p.lo, p.hi);
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
                    keyword(name.as_bytes(), edition).map_or(Kind::Identifier, Kind::Keyword)
                }
                TokenKind::RawIdent => {
                    let name = &text[lo + 2..hi];
                    if is_reserved_raw_name(name) || has_zero_width_joiner(joiners, name) {
                        return None;
                    }
                    Kind::RawIdentifier
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
                    Kind::Lifetime
                }
                TokenKind::RawLifetime if since_2021 => {
                    let name = &text[lo + 3..hi];
                    if is_reserved_raw_name(name) || has_zero_width_joiner(joiners, name) {
                        return None;
                    }
                    Kind::RawLifetime
                }
                TokenKind::RawLifetime => {
                    // `'r`, then `#` and the name.
                    restart = Some(lo + 2);
                    Kind::Lifetime
                }
                TokenKind::Literal {
                    kind: LiteralKind::CStr { .. },
                    ..
                } if !since_2021 => {
                    // `c`, then the string.
                    restart = Some(lo + 1);
                    Kind::Identifier
                }
                TokenKind::Literal {
                    kind: LiteralKind::RawCStr { .. },
                    ..
                } if !since_2021 => {
                    // `cr`, then `#` or the string.
                    restart = Some(lo + 2);
                    Kind::Identifier
                }
                TokenKind::Literal { kind, suffix_start } => {
                    let suffix = lo + suffix_start as usize;
                    literal(text, lo, suffix, hi, kind, joiners)?
                }
                TokenKind::OpenParen => Kind::OpenDelimiter(Delimiter::Parenthesis),
                TokenKind::OpenBracket => Kind::OpenDelimiter(Delimiter::Bracket),
                TokenKind::OpenBrace => Kind::OpenDelimiter(Delimiter::Brace),
                TokenKind::CloseParen => Kind::CloseDelimiter(Delimiter::Parenthesis),
                TokenKind::CloseBracket => Kind::CloseDelimiter(Delimiter::Bracket),
                TokenKind::CloseBrace => Kind::CloseDelimiter(Delimiter::Brace),
                _ => return None,
            };
            if let Some(p) = pending.take() {
                emit(sink, Kind::Punctuation(p.punctuation), p.lo, p.hi);
            }
            match restart {
                Some(next) => {
                    emit(sink, kind, lo, next);
                    pos = next;
                    continue 'cursor;
                }
                None => emit(sink, kind, lo, hi),
            }
        }
    }
    if let Some(p) = pending {
        emit(sink, Kind::Punctuation(p.punctuation), p.lo, p.hi);
    }
    Some(())
}

/// Gives `lo..hi` to `sink`; both ends are at most the length of a text
/// that [`lex`] admitted, which fits a `u32`.
fn emit(sink: &mut impl Sink, kind: Kind, lo: usize, hi: usize) {
    sink.token(kind, lo as u32, hi as u32);
}

/// Validates the literal `lo..hi` whose suffix starts at `suffix`, and
/// returns its kind.
fn literal(
    text: &str,
    lo: usize,
    suffix: usize,
    hi: usize,
    kind: LiteralKind,
    joiners: bool,
) -> Option<Kind> {
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
                Base::Decimal | Base::Hexadecimal => return Some(Kind::IntegerLiteral),
            };
            // `rustc_lexer` reads decimal digits after `0b` and `0o`.
            let in_radix = body.as_bytes()[2..].iter().all(|&b| b == b'_' || b < radix);
            return in_radix.then_some(Kind::IntegerLiteral);
        }
        LiteralKind::Float {
            base: Base::Decimal,
            empty_exponent: false,
        } => {
            return Some(Kind::FloatLiteral);
        }
        LiteralKind::Float { .. } => return None,
        LiteralKind::Char { terminated: true } => {
            let ok = escaper::unescape_char(quoted(0, 0)).is_ok();
            return ok.then_some(Kind::CharLiteral);
        }
        LiteralKind::Byte { terminated: true } => {
            let ok = escaper::unescape_byte(quoted(1, 0)).is_ok();
            return ok.then_some(Kind::ByteLiteral);
        }
        LiteralKind::Str { terminated: true } => (Kind::StringLiteral, Mode::Str, quoted(0, 0)),
        LiteralKind::ByteStr { terminated: true } => {
            (Kind::ByteStringLiteral, Mode::ByteStr, quoted(1, 0))
        }
        LiteralKind::CStr { terminated: true } => (Kind::CStringLiteral, Mode::CStr, quoted(1, 0)),
        LiteralKind::RawStr { n_hashes: Some(n) } => (
            Kind::RawStringLiteral,
            Mode::RawStr,
            quoted(1, usize::from(n)),
        ),
        LiteralKind::RawByteStr { n_hashes: Some(n) } => (
            Kind::RawByteStringLiteral,
            Mode::RawByteStr,
            quoted(2, usize::from(n)),
        ),
        LiteralKind::RawCStr { n_hashes: Some(n) } => (
            Kind::RawCStringLiteral,
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

/// The punctuation token of a single-character `rustc_lexer` token.
fn single_punctuation(kind: TokenKind) -> Option<Punctuation> {
    Some(match kind {
        TokenKind::Semi => Punctuation::Semi,
        TokenKind::Comma => Punctuation::Comma,
        TokenKind::Dot => Punctuation::Dot,
        TokenKind::At => Punctuation::At,
        TokenKind::Pound => Punctuation::Pound,
        TokenKind::Tilde => Punctuation::Tilde,
        TokenKind::Question => Punctuation::Question,
        TokenKind::Colon => Punctuation::Colon,
        TokenKind::Dollar => Punctuation::Dollar,
        TokenKind::Eq => Punctuation::Eq,
        TokenKind::Bang => Punctuation::Not,
        TokenKind::Lt => Punctuation::Lt,
        TokenKind::Gt => Punctuation::Gt,
        TokenKind::Minus => Punctuation::Minus,
        TokenKind::And => Punctuation::And,
        TokenKind::Or => Punctuation::Or,
        TokenKind::Plus => Punctuation::Plus,
        TokenKind::Star => Punctuation::Star,
        TokenKind::Slash => Punctuation::Slash,
        TokenKind::Caret => Punctuation::Caret,
        TokenKind::Percent => Punctuation::Percent,
        _ => return None,
    })
}

/// The compound punctuation token of the Rust Reference spelled `text`, or
/// `None`. Each three-character compound starts with a two-character one, so
/// left-to-right gluing finds the longest token.
fn compound(text: &[u8]) -> Option<Punctuation> {
    Some(match text {
        b"..." => Punctuation::DotDotDot,
        b"..=" => Punctuation::DotDotEq,
        b"<<=" => Punctuation::ShlEq,
        b">>=" => Punctuation::ShrEq,
        b"!=" => Punctuation::Ne,
        b"%=" => Punctuation::PercentEq,
        b"&&" => Punctuation::AndAnd,
        b"&=" => Punctuation::AndEq,
        b"*=" => Punctuation::StarEq,
        b"+=" => Punctuation::PlusEq,
        b"-=" => Punctuation::MinusEq,
        b"->" => Punctuation::RArrow,
        b".." => Punctuation::DotDot,
        b"/=" => Punctuation::SlashEq,
        b"::" => Punctuation::PathSep,
        b"<-" => Punctuation::LArrow,
        b"<<" => Punctuation::Shl,
        b"<=" => Punctuation::Le,
        b"==" => Punctuation::EqEq,
        b"=>" => Punctuation::FatArrow,
        b">=" => Punctuation::Ge,
        b">>" => Punctuation::Shr,
        b"^=" => Punctuation::CaretEq,
        b"|=" => Punctuation::OrEq,
        b"||" => Punctuation::OrOr,
        _ => return None,
    })
}

/// The strict or reserved keyword spelled `name` in `edition`, or `None`.
fn keyword(name: &[u8], edition: Edition) -> Option<Keyword> {
    let since_2018 = edition >= Edition::E2018;
    Some(match name {
        b"as" => Keyword::As,
        b"break" => Keyword::Break,
        b"const" => Keyword::Const,
        b"continue" => Keyword::Continue,
        b"crate" => Keyword::Crate,
        b"else" => Keyword::Else,
        b"enum" => Keyword::Enum,
        b"extern" => Keyword::Extern,
        b"false" => Keyword::False,
        b"fn" => Keyword::Fn,
        b"for" => Keyword::For,
        b"if" => Keyword::If,
        b"impl" => Keyword::Impl,
        b"in" => Keyword::In,
        b"let" => Keyword::Let,
        b"loop" => Keyword::Loop,
        b"match" => Keyword::Match,
        b"mod" => Keyword::Mod,
        b"move" => Keyword::Move,
        b"mut" => Keyword::Mut,
        b"pub" => Keyword::Pub,
        b"ref" => Keyword::Ref,
        b"return" => Keyword::Return,
        b"self" => Keyword::SelfValue,
        b"Self" => Keyword::SelfType,
        b"static" => Keyword::Static,
        b"struct" => Keyword::Struct,
        b"super" => Keyword::Super,
        b"trait" => Keyword::Trait,
        b"true" => Keyword::True,
        b"type" => Keyword::Type,
        b"unsafe" => Keyword::Unsafe,
        b"use" => Keyword::Use,
        b"where" => Keyword::Where,
        b"while" => Keyword::While,
        b"_" => Keyword::Underscore,
        b"abstract" => Keyword::Abstract,
        b"become" => Keyword::Become,
        b"box" => Keyword::Box,
        b"do" => Keyword::Do,
        b"final" => Keyword::Final,
        b"macro" => Keyword::Macro,
        b"override" => Keyword::Override,
        b"priv" => Keyword::Priv,
        b"typeof" => Keyword::Typeof,
        b"unsized" => Keyword::Unsized,
        b"virtual" => Keyword::Virtual,
        b"yield" => Keyword::Yield,
        b"async" if since_2018 => Keyword::Async,
        b"await" if since_2018 => Keyword::Await,
        b"dyn" if since_2018 => Keyword::Dyn,
        b"try" if since_2018 => Keyword::Try,
        b"gen" if edition >= Edition::E2024 => Keyword::Gen,
        _ => return None,
    })
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
    struct Tokens(Vec<(Kind, u32, u32)>);

    impl Sink for Tokens {
        fn token(&mut self, kind: Kind, lo: u32, hi: u32) {
            self.0.push((kind, lo, hi));
        }
    }

    fn tokens(text: &str, edition: Edition) -> Option<Vec<(Kind, u32, u32)>> {
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
    fn names_keywords_by_edition() {
        let kind = |name: &str, edition| tokens(name, edition).unwrap()[0].0;
        assert_eq!(kind("fn", Edition::E2015), Kind::Keyword(Keyword::Fn));
        assert_eq!(kind("async", Edition::E2015), Kind::Identifier);
        assert_eq!(kind("async", Edition::E2018), Kind::Keyword(Keyword::Async));
        assert_eq!(kind("dyn", Edition::E2015), Kind::Identifier);
        assert_eq!(kind("try", Edition::E2018), Kind::Keyword(Keyword::Try));
        assert_eq!(kind("gen", Edition::E2021), Kind::Identifier);
        assert_eq!(kind("gen", Edition::E2024), Kind::Keyword(Keyword::Gen));
        assert_eq!(kind("union", Edition::E2024), Kind::Identifier);
        assert_eq!(
            kind("_", Edition::E2015),
            Kind::Keyword(Keyword::Underscore)
        );
        assert_eq!(
            kind("self", Edition::E2015),
            Kind::Keyword(Keyword::SelfValue)
        );
        assert_eq!(
            kind("Self", Edition::E2015),
            Kind::Keyword(Keyword::SelfType)
        );
        for text in [
            "async await dyn try gen union macro_rules raw safe _ Self self",
            "r#gen 'static",
        ] {
            agrees(text);
        }
    }

    #[test]
    fn names_punctuation_and_delimiters() {
        let kinds = |text: &str| -> Vec<Kind> {
            tokens(text, Edition::E2021)
                .unwrap()
                .iter()
                .map(|&(kind, _, _)| kind)
                .collect()
        };
        let p = Kind::Punctuation;
        assert_eq!(kinds("&&&"), [p(Punctuation::AndAnd), p(Punctuation::And)]);
        assert_eq!(kinds(">>= !"), [p(Punctuation::ShrEq), p(Punctuation::Not)]);
        assert_eq!(
            kinds("#\"x\""),
            [p(Punctuation::Pound), Kind::StringLiteral]
        );
        assert_eq!(
            kinds("([{}])"),
            [
                Kind::OpenDelimiter(Delimiter::Parenthesis),
                Kind::OpenDelimiter(Delimiter::Bracket),
                Kind::OpenDelimiter(Delimiter::Brace),
                Kind::CloseDelimiter(Delimiter::Brace),
                Kind::CloseDelimiter(Delimiter::Bracket),
                Kind::CloseDelimiter(Delimiter::Parenthesis),
            ]
        );
        let every = "... ..= <<= >>= != %= && &= *= += -= -> .. /= :: <- << <= == => >= >> ^= |= \
                     || ! # $ % & * + , - . / : ; < = > ? @ ^ | ~ ( ) [ ] { }";
        assert_eq!(kinds(every).len(), 52);
        agrees(every);
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
