//! The entries of the lexer benchmark harness in `bench/`.
//!
//! With the Cargo feature `bench`, [`lex`] lexes one text for timing,
//! [`tokens`] returns the token classes and spans for the equivalence check
//! against a reference lexer, and [`sizes`] reports the sizes of the lexer
//! representation. [`TokenClass`] is owned here and is not the lexer's token
//! kind. None of these is an API.

use crate::edition::Edition;
use crate::lexer::{LexError, LexErrorKind, Lexer, Token, TokenKind};
use crate::source::{ByteOffset, SourceId, Span};

/// The class of a token, coarser than its kind: a keyword, a punctuation
/// token, or a delimiter is told apart from another of its class by its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenClass {
    Identifier,
    Keyword,
    RawIdentifier,
    Lifetime,
    RawLifetime,
    IntegerLiteral,
    FloatLiteral,
    CharLiteral,
    ByteLiteral,
    StringLiteral,
    RawStringLiteral,
    ByteStringLiteral,
    RawByteStringLiteral,
    CStringLiteral,
    RawCStringLiteral,
    Punctuation,
    OpenDelimiter,
    CloseDelimiter,
}

/// Lexes `text` in `edition` to its end and returns the number of tokens and
/// a hash of their spans, or `None` when lexing ends with an error or
/// `edition` is not 2015, 2018, 2021, or 2024.
///
/// The end of file is not a token. The hash starts at 0 and, for each token
/// `lo..hi` in order, becomes `hash.wrapping_mul(0x100_0000_01b3) ^ (lo << 32 | hi)`
/// in `u64`. It allocates nothing: no source table entry, and no storage
/// for the tokens.
///
/// # Panics
///
/// Panics if `text` is longer than `u32::MAX` bytes.
pub fn lex(text: &str, edition: u16) -> Option<(usize, u64)> {
    let edition = edition_of(edition)?;
    let mut lexer = Lexer::new(SourceId::benchmark(), text, edition);
    let (mut count, mut hash) = (0, 0u64);
    loop {
        let token = lexer.next_token().ok()?;
        if token.kind == TokenKind::EndOfFile {
            return Some((count, hash));
        }
        let (lo, hi) = offsets(token.span);
        hash = hash.wrapping_mul(0x100_0000_01b3) ^ (u64::from(lo) << 32 | u64::from(hi));
        count += 1;
    }
}

/// Lexes `text` in `edition` to its end and returns the class and the byte
/// range `lo..hi` of each token, in order, or `None` as [`lex`] does.
///
/// # Panics
///
/// Panics if `text` is longer than `u32::MAX` bytes.
pub fn tokens(text: &str, edition: u16) -> Option<Vec<(TokenClass, u32, u32)>> {
    let edition = edition_of(edition)?;
    let mut lexer = Lexer::new(SourceId::benchmark(), text, edition);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token().ok()?;
        let Some(class) = class(token.kind) else {
            return Some(tokens);
        };
        let (lo, hi) = offsets(token.span);
        tokens.push((class, lo, hi));
    }
}

/// The size in bytes of each lexer representation type, by name.
pub fn sizes() -> Vec<(&'static str, usize)> {
    vec![
        ("Token", size_of::<Token>()),
        ("TokenKind", size_of::<TokenKind>()),
        ("Span", size_of::<Span>()),
        ("LexError", size_of::<LexError>()),
        ("LexErrorKind", size_of::<LexErrorKind>()),
        (
            "Result<Token, LexError>",
            size_of::<Result<Token, LexError>>(),
        ),
        ("Lexer", size_of::<Lexer<'static>>()),
    ]
}

fn edition_of(year: u16) -> Option<Edition> {
    match year {
        2015 => Some(Edition::E2015),
        2018 => Some(Edition::E2018),
        2021 => Some(Edition::E2021),
        2024 => Some(Edition::E2024),
        _ => None,
    }
}

/// The class of `kind`, or `None` for the end of file.
fn class(kind: TokenKind) -> Option<TokenClass> {
    let class = match kind {
        TokenKind::Identifier => TokenClass::Identifier,
        TokenKind::Keyword(_) => TokenClass::Keyword,
        TokenKind::IntegerLiteral => TokenClass::IntegerLiteral,
        TokenKind::FloatLiteral => TokenClass::FloatLiteral,
        TokenKind::RawIdentifier => TokenClass::RawIdentifier,
        TokenKind::Lifetime => TokenClass::Lifetime,
        TokenKind::RawLifetime => TokenClass::RawLifetime,
        TokenKind::CharLiteral => TokenClass::CharLiteral,
        TokenKind::ByteLiteral => TokenClass::ByteLiteral,
        TokenKind::StringLiteral => TokenClass::StringLiteral,
        TokenKind::RawStringLiteral => TokenClass::RawStringLiteral,
        TokenKind::ByteStringLiteral => TokenClass::ByteStringLiteral,
        TokenKind::RawByteStringLiteral => TokenClass::RawByteStringLiteral,
        TokenKind::CStringLiteral => TokenClass::CStringLiteral,
        TokenKind::RawCStringLiteral => TokenClass::RawCStringLiteral,
        TokenKind::Punctuation(_) => TokenClass::Punctuation,
        TokenKind::OpenDelimiter(_) => TokenClass::OpenDelimiter,
        TokenKind::CloseDelimiter(_) => TokenClass::CloseDelimiter,
        TokenKind::EndOfFile => return None,
    };
    Some(class)
}

fn offsets(span: Span) -> (u32, u32) {
    (offset(span.lo()), offset(span.hi()))
}

fn offset(offset: ByteOffset) -> u32 {
    usize::try_from(offset)
        .ok()
        .and_then(|offset| u32::try_from(offset).ok())
        .expect("a ByteOffset fits a u32")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED: &str = "\
// a line comment
fn r#match<'a, 'r#fn>(x: &'a [u8]) -> Option<u32> {
    let s = \"a\\n\"; let r = r#\"raw\"#; let b = b\"bytes\"; let br = br\"raw bytes\";
    let c = c\"c\"; let cr = cr#\"raw c\"#; let ch = 'c'; let by = b'b';
    let n = 0x1F_u32 + 1_000; let f = 1.5e3f64; /* nested /* block */ comment */
    x.iter().map(|v| *v as u32 >> 2).sum::<u32>().checked_add(n)
}
";

    #[test]
    fn lex_and_tokens_agree_on_count_and_spans() {
        for text in ["fn main() {}", MIXED] {
            let tokens = tokens(text, 2021).unwrap();
            let mut hash = 0u64;
            for &(_, lo, hi) in &tokens {
                hash = hash.wrapping_mul(0x100_0000_01b3) ^ (u64::from(lo) << 32 | u64::from(hi));
            }
            assert_eq!(lex(text, 2021), Some((tokens.len(), hash)), "{text:?}");
        }
    }

    #[test]
    fn tokens_returns_classes_and_spans() {
        assert_eq!(
            tokens("fn main() {}", 2024).unwrap(),
            [
                (TokenClass::Keyword, 0, 2),
                (TokenClass::Identifier, 3, 7),
                (TokenClass::OpenDelimiter, 7, 8),
                (TokenClass::CloseDelimiter, 8, 9),
                (TokenClass::OpenDelimiter, 10, 11),
                (TokenClass::CloseDelimiter, 11, 12),
            ]
        );
        assert_eq!(tokens("", 2015), Some(Vec::new()));
        assert_eq!(lex("", 2015), Some((0, 0)));
    }

    #[test]
    fn every_class_is_produced() {
        let classes = tokens(MIXED, 2021).unwrap();
        for class in [
            TokenClass::Identifier,
            TokenClass::Keyword,
            TokenClass::RawIdentifier,
            TokenClass::Lifetime,
            TokenClass::RawLifetime,
            TokenClass::IntegerLiteral,
            TokenClass::FloatLiteral,
            TokenClass::CharLiteral,
            TokenClass::ByteLiteral,
            TokenClass::StringLiteral,
            TokenClass::RawStringLiteral,
            TokenClass::ByteStringLiteral,
            TokenClass::RawByteStringLiteral,
            TokenClass::CStringLiteral,
            TokenClass::RawCStringLiteral,
            TokenClass::Punctuation,
            TokenClass::OpenDelimiter,
            TokenClass::CloseDelimiter,
        ] {
            assert!(classes.iter().any(|&(c, _, _)| c == class), "{class:?}");
        }
    }

    #[test]
    fn an_unknown_edition_returns_none() {
        for edition in [0, 2012, 2016, 2027, u16::MAX] {
            assert_eq!(lex("fn main() {}", edition), None, "{edition}");
            assert_eq!(tokens("fn main() {}", edition), None, "{edition}");
        }
    }

    #[test]
    fn a_lexical_error_returns_none() {
        for text in ["fn main() { ` }", "\"unterminated", "/* open"] {
            assert_eq!(lex(text, 2021), None, "{text:?}");
            assert_eq!(tokens(text, 2021), None, "{text:?}");
        }
    }

    #[test]
    fn the_edition_changes_the_result() {
        // `c"c"` is one C string literal from 2021 and two tokens before it.
        assert_eq!(tokens("c\"c\"", 2021).map(|t| t.len()), Some(1));
        assert_eq!(tokens("c\"c\"", 2018).map(|t| t.len()), Some(2));
    }

    #[test]
    fn sizes_lists_seven_representations() {
        let sizes = sizes();
        let names: Vec<_> = sizes.iter().map(|&(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "Token",
                "TokenKind",
                "Span",
                "LexError",
                "LexErrorKind",
                "Result<Token, LexError>",
                "Lexer",
            ]
        );
        assert!(sizes.iter().all(|&(_, size)| size > 0));
    }
}
