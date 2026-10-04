//! The entries of the lexer benchmark harness in `bench/`.
//!
//! With the Cargo feature `bench`, [`lex`] lexes one text for timing,
//! [`tokens`] returns the token kinds and spans for the equivalence check
//! against a reference lexer, [`fold`] is the hash that both sides of the
//! timed comparison compute, and [`sizes`] reports the sizes of the lexer
//! representation. [`TokenKind`] and the enums it holds are owned here: they
//! name the same tokens as the lexer's kinds, which stay private. None of
//! these is an API.

use crate::edition::Edition;
use crate::lexer::{self, LexError, LexErrorKind, Lexer, Token};
use crate::source::{ByteOffset, SourceId, Span};

/// The exact identity of a token: its class, and which keyword, punctuation
/// token, or delimiter it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    Keyword(Keyword),
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
    Punctuation(Punctuation),
    OpenDelimiter(Delimiter),
    CloseDelimiter(Delimiter),
}

impl TokenKind {
    /// The number that [`fold`] mixes in: distinct for distinct kinds.
    fn id(self) -> u64 {
        match self {
            Self::Identifier => 0,
            Self::RawIdentifier => 1,
            Self::Lifetime => 2,
            Self::RawLifetime => 3,
            Self::IntegerLiteral => 4,
            Self::FloatLiteral => 5,
            Self::CharLiteral => 6,
            Self::ByteLiteral => 7,
            Self::StringLiteral => 8,
            Self::RawStringLiteral => 9,
            Self::ByteStringLiteral => 10,
            Self::RawByteStringLiteral => 11,
            Self::CStringLiteral => 12,
            Self::RawCStringLiteral => 13,
            Self::Keyword(keyword) => 0x100 | keyword as u64,
            Self::Punctuation(punctuation) => 0x200 | punctuation as u64,
            Self::OpenDelimiter(delimiter) => 0x300 | delimiter as u64,
            Self::CloseDelimiter(delimiter) => 0x400 | delimiter as u64,
        }
    }

    /// The kind of a lexer token kind, or `None` for the end of file.
    fn of(kind: lexer::TokenKind) -> Option<Self> {
        let kind = match kind {
            lexer::TokenKind::Identifier => Self::Identifier,
            lexer::TokenKind::Keyword(keyword) => Self::Keyword(Keyword::of(keyword)),
            lexer::TokenKind::IntegerLiteral => Self::IntegerLiteral,
            lexer::TokenKind::FloatLiteral => Self::FloatLiteral,
            lexer::TokenKind::RawIdentifier => Self::RawIdentifier,
            lexer::TokenKind::Lifetime => Self::Lifetime,
            lexer::TokenKind::RawLifetime => Self::RawLifetime,
            lexer::TokenKind::CharLiteral => Self::CharLiteral,
            lexer::TokenKind::ByteLiteral => Self::ByteLiteral,
            lexer::TokenKind::StringLiteral => Self::StringLiteral,
            lexer::TokenKind::RawStringLiteral => Self::RawStringLiteral,
            lexer::TokenKind::ByteStringLiteral => Self::ByteStringLiteral,
            lexer::TokenKind::RawByteStringLiteral => Self::RawByteStringLiteral,
            lexer::TokenKind::CStringLiteral => Self::CStringLiteral,
            lexer::TokenKind::RawCStringLiteral => Self::RawCStringLiteral,
            lexer::TokenKind::Punctuation(punctuation) => {
                Self::Punctuation(Punctuation::of(punctuation))
            }
            lexer::TokenKind::OpenDelimiter(delimiter) => {
                Self::OpenDelimiter(Delimiter::of(delimiter))
            }
            lexer::TokenKind::CloseDelimiter(delimiter) => {
                Self::CloseDelimiter(Delimiter::of(delimiter))
            }
            lexer::TokenKind::EndOfFile => return None,
        };
        Some(kind)
    }
}

/// Declares a bench-owned enum with the variants of a lexer enum, in the same
/// order, and the conversion `of` from the lexer enum.
macro_rules! mirror {
    ($(#[$meta:meta])* $name:ident: $source:ty { $($(#[$doc:meta])* $variant:ident,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$doc])* $variant,)*
        }

        impl $name {
            #[cfg(test)]
            const ALL: &[Self] = &[$(Self::$variant,)*];

            fn of(value: $source) -> Self {
                match value {
                    $(<$source>::$variant => Self::$variant,)*
                }
            }
        }
    };
}

mirror! {
    /// A strict or reserved keyword, as the lexer classifies it by edition.
    Keyword: lexer::Keyword {
        As, Async, Await, Break, Const, Continue, Crate, Dyn, Else, Enum, Extern, False, Fn, For,
        If, Impl, In, Let, Loop, Match, Mod, Move, Mut, Pub, Ref, Return,
        /// `self`.
        SelfValue,
        /// `Self`.
        SelfType,
        Static, Struct, Super, Trait, True, Type,
        /// `_` alone.
        Underscore,
        Unsafe, Use, Where, While, Abstract, Become, Box, Do, Final, Gen, Macro, Override, Priv,
        Try, Typeof, Unsized, Virtual, Yield,
    }
}

mirror! {
    /// A punctuation token of the Rust Reference other than a delimiter.
    Punctuation: lexer::Punctuation {
        Plus, Minus, Star, Slash, Percent, Caret, Not, And, Or, AndAnd, OrOr, Shl, Shr, PlusEq,
        MinusEq, StarEq, SlashEq, PercentEq, CaretEq, AndEq, OrEq, ShlEq, ShrEq, Eq, EqEq, Ne, Gt,
        Lt, Ge, Le, At, Dot, DotDot, DotDotDot, DotDotEq, Comma, Semi, Colon, PathSep, RArrow,
        FatArrow, LArrow, Pound, Dollar, Question, Tilde,
    }
}

mirror! {
    /// The delimiter of an open or close delimiter token.
    Delimiter: lexer::Delimiter {
        Parenthesis, Bracket, Brace,
    }
}

/// Returns `hash` with the token `lo..hi` of kind `kind` mixed in.
///
/// For each token in order, the hash becomes
/// `hash.wrapping_mul(0x100_0000_01b3) ^ (lo << 32 | hi)`, then
/// `hash.wrapping_mul(0x100_0000_01b3) ^ id`, in `u64`, where `id` is a
/// number distinct for each kind. [`lex`] and the reference side of the
/// benchmark fold their tokens with it.
pub fn fold(hash: u64, kind: TokenKind, lo: u32, hi: u32) -> u64 {
    let hash = hash.wrapping_mul(0x100_0000_01b3) ^ (u64::from(lo) << 32 | u64::from(hi));
    hash.wrapping_mul(0x100_0000_01b3) ^ kind.id()
}

/// Lexes `text` in `edition` to its end and returns the number of tokens and
/// their [`fold`] from 0, or `None` when lexing ends with an error or
/// `edition` is not 2015, 2018, 2021, or 2024.
///
/// The end of file is not a token. It allocates nothing: no source table
/// entry, and no storage for the tokens.
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
        let Some(kind) = TokenKind::of(token.kind) else {
            return Some((count, hash));
        };
        let (lo, hi) = offsets(token.span);
        hash = fold(hash, kind, lo, hi);
        count += 1;
    }
}

/// Lexes `text` in `edition` to its end and returns the kind and the byte
/// range `lo..hi` of each token, in order, or `None` as [`lex`] does.
///
/// # Panics
///
/// Panics if `text` is longer than `u32::MAX` bytes.
pub fn tokens(text: &str, edition: u16) -> Option<Vec<(TokenKind, u32, u32)>> {
    let edition = edition_of(edition)?;
    let mut lexer = Lexer::new(SourceId::benchmark(), text, edition);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token().ok()?;
        let Some(kind) = TokenKind::of(token.kind) else {
            return Some(tokens);
        };
        let (lo, hi) = offsets(token.span);
        tokens.push((kind, lo, hi));
    }
}

/// The size in bytes of each lexer representation type, by name.
pub fn sizes() -> Vec<(&'static str, usize)> {
    vec![
        ("Token", size_of::<Token>()),
        ("TokenKind", size_of::<lexer::TokenKind>()),
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
    fn lex_folds_the_kinds_and_spans_that_tokens_returns() {
        for text in ["fn main() {}", MIXED] {
            let tokens = tokens(text, 2021).unwrap();
            let hash = tokens
                .iter()
                .fold(0, |hash, &(kind, lo, hi)| fold(hash, kind, lo, hi));
            assert_eq!(lex(text, 2021), Some((tokens.len(), hash)), "{text:?}");
        }
    }

    #[test]
    fn the_fold_depends_on_the_kind() {
        let keyword = fold(0, TokenKind::Keyword(Keyword::Fn), 0, 2);
        let identifier = fold(0, TokenKind::Identifier, 0, 2);
        assert_ne!(keyword, identifier);
    }

    #[test]
    fn every_kind_has_its_own_id() {
        let mut kinds = vec![
            TokenKind::Identifier,
            TokenKind::RawIdentifier,
            TokenKind::Lifetime,
            TokenKind::RawLifetime,
            TokenKind::IntegerLiteral,
            TokenKind::FloatLiteral,
            TokenKind::CharLiteral,
            TokenKind::ByteLiteral,
            TokenKind::StringLiteral,
            TokenKind::RawStringLiteral,
            TokenKind::ByteStringLiteral,
            TokenKind::RawByteStringLiteral,
            TokenKind::CStringLiteral,
            TokenKind::RawCStringLiteral,
        ];
        kinds.extend(Keyword::ALL.iter().map(|&k| TokenKind::Keyword(k)));
        kinds.extend(Punctuation::ALL.iter().map(|&p| TokenKind::Punctuation(p)));
        kinds.extend(Delimiter::ALL.iter().map(|&d| TokenKind::OpenDelimiter(d)));
        kinds.extend(Delimiter::ALL.iter().map(|&d| TokenKind::CloseDelimiter(d)));
        assert_eq!(kinds.len(), 14 + 53 + 46 + 3 + 3);
        let mut ids: Vec<u64> = kinds.iter().map(|kind| kind.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), kinds.len());
    }

    #[test]
    fn tokens_names_the_keyword_punctuation_and_delimiter() {
        assert_eq!(
            tokens("self Self _ gen ::<", 2024).unwrap(),
            [
                (TokenKind::Keyword(Keyword::SelfValue), 0, 4),
                (TokenKind::Keyword(Keyword::SelfType), 5, 9),
                (TokenKind::Keyword(Keyword::Underscore), 10, 11),
                (TokenKind::Keyword(Keyword::Gen), 12, 15),
                (TokenKind::Punctuation(Punctuation::PathSep), 16, 18),
                (TokenKind::Punctuation(Punctuation::Lt), 18, 19),
            ]
        );
        assert_eq!(
            tokens("gen [}", 2021).unwrap(),
            [
                (TokenKind::Identifier, 0, 3),
                (TokenKind::OpenDelimiter(Delimiter::Bracket), 4, 5),
                (TokenKind::CloseDelimiter(Delimiter::Brace), 5, 6),
            ]
        );
    }

    #[test]
    fn tokens_returns_classes_and_spans() {
        assert_eq!(
            tokens("fn main() {}", 2024).unwrap(),
            [
                (TokenKind::Keyword(Keyword::Fn), 0, 2),
                (TokenKind::Identifier, 3, 7),
                (TokenKind::OpenDelimiter(Delimiter::Parenthesis), 7, 8),
                (TokenKind::CloseDelimiter(Delimiter::Parenthesis), 8, 9),
                (TokenKind::OpenDelimiter(Delimiter::Brace), 10, 11),
                (TokenKind::CloseDelimiter(Delimiter::Brace), 11, 12),
            ]
        );
        assert_eq!(tokens("", 2015), Some(Vec::new()));
        assert_eq!(lex("", 2015), Some((0, 0)));
    }

    #[test]
    fn every_class_is_produced() {
        let produced = tokens(MIXED, 2021).unwrap();
        for kind in [
            TokenKind::Identifier,
            TokenKind::Keyword(Keyword::Fn),
            TokenKind::RawIdentifier,
            TokenKind::Lifetime,
            TokenKind::RawLifetime,
            TokenKind::IntegerLiteral,
            TokenKind::FloatLiteral,
            TokenKind::CharLiteral,
            TokenKind::ByteLiteral,
            TokenKind::StringLiteral,
            TokenKind::RawStringLiteral,
            TokenKind::ByteStringLiteral,
            TokenKind::RawByteStringLiteral,
            TokenKind::CStringLiteral,
            TokenKind::RawCStringLiteral,
            TokenKind::Punctuation(Punctuation::Shr),
            TokenKind::OpenDelimiter(Delimiter::Bracket),
            TokenKind::CloseDelimiter(Delimiter::Bracket),
        ] {
            let class = std::mem::discriminant(&kind);
            assert!(
                produced
                    .iter()
                    .any(|(k, _, _)| std::mem::discriminant(k) == class),
                "{kind:?}"
            );
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
