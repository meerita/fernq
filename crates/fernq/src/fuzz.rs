//! The lexer invariant check, and the entry of the lexer fuzz target.
//!
//! `check_lexing` lexes one text in every edition and panics when a lexer
//! invariant does not hold. With the Cargo feature `fuzzing`, [`lex`] runs it
//! on arbitrary bytes for the fuzz target in `fuzz/`. Neither is an API.

use crate::edition::Edition;
use crate::lexer::{LexError, Lexer, Token, TokenKind};
use crate::source::{SourceId, SourceTable};

const EDITIONS: [Edition; 4] = [
    Edition::E2015,
    Edition::E2018,
    Edition::E2021,
    Edition::E2024,
];

/// Lexes `data` when it is UTF-8, as `check_lexing` does, and returns
/// without lexing otherwise: a source file is UTF-8 before the lexer reads it.
///
/// # Panics
///
/// Panics when a lexer invariant does not hold for `data`.
#[cfg(feature = "fuzzing")]
pub fn lex(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        check_lexing(text);
    }
}

/// Lexes `text` in every edition and checks the lexer invariants:
///
/// - lexing ends, after at most one result per byte and the end of file;
/// - each result lies in the text, after the previous one, with both ends on
///   character boundaries;
/// - each token other than the end of file covers at least one byte;
/// - after the end of file or an error, every call returns the same result;
/// - a second lexer over the same text returns the same results.
///
/// # Panics
///
/// Panics when an invariant does not hold.
pub(crate) fn check_lexing(text: &str) {
    let mut sources = SourceTable::default();
    // The lexer reads only `text`; the source gives its errors an identity.
    let id = sources.add_text("fuzz.rs", "");
    for edition in EDITIONS {
        let results = lex_checked(id, text, edition);
        assert_eq!(
            lex_checked(id, text, edition),
            results,
            "a second lex in {edition:?} differs"
        );
    }
}

/// Lexes `text` to its end, checks every result, and returns them.
fn lex_checked(id: SourceId, text: &str, edition: Edition) -> Vec<Result<Token, LexError>> {
    let mut lexer = Lexer::new(id, text, edition);
    let mut results = Vec::new();
    let mut previous_hi = 0;
    loop {
        let result = lexer.next_token();
        let span = match result {
            Ok(token) => token.span,
            Err(error) => error.span(),
        };
        let lo = usize::try_from(span.lo()).expect("a ByteOffset fits a usize");
        let hi = usize::try_from(span.hi()).expect("a ByteOffset fits a usize");
        assert!(
            previous_hi <= lo && lo <= hi && hi <= text.len(),
            "{edition:?}: {result:?} is not inside the text after {previous_hi}"
        );
        assert!(
            text.is_char_boundary(lo) && text.is_char_boundary(hi),
            "{edition:?}: {result:?} is not on character boundaries"
        );
        let is_last = !matches!(result, Ok(token) if token.kind != TokenKind::EndOfFile);
        assert!(lo < hi || is_last, "{edition:?}: {result:?} is empty");
        results.push(result);
        if is_last {
            assert_eq!(
                lexer.next_token(),
                result,
                "{edition:?}: the last result does not repeat"
            );
            return results;
        }
        assert!(
            results.len() <= text.len(),
            "{edition:?}: lexing does not end"
        );
        previous_hi = hi;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compile fixtures, embedded at build time: unit tests read no file.
    #[test]
    fn every_compile_fixture_passes_the_check() {
        for text in [
            include_str!("../tests/fixtures/compile-fail/char-literal-too-long.rs"),
            include_str!("../tests/fixtures/compile-fail/float-empty-exponent.rs"),
            include_str!("../tests/fixtures/compile-fail/invalid-binary-digit.rs"),
            include_str!("../tests/fixtures/compile-fail/non-ascii-byte-literal.rs"),
            include_str!("../tests/fixtures/compile-fail/non-identifier-character.rs"),
            include_str!("../tests/fixtures/compile-fail/reserved-pounds.rs"),
            include_str!("../tests/fixtures/compile-fail/reserved-prefix-before-string.rs"),
            include_str!("../tests/fixtures/compile-fail/reserved-raw-identifier.rs"),
            include_str!("../tests/fixtures/compile-fail/reserved-raw-lifetime.rs"),
            include_str!("../tests/fixtures/compile-fail/stray-backtick.rs"),
            include_str!("../tests/fixtures/compile-fail/unclosed-paren.rs"),
            include_str!("../tests/fixtures/compile-fail/unknown-string-escape.rs"),
            include_str!("../tests/fixtures/compile-fail/unterminated-block-comment.rs"),
        ] {
            check_lexing(text);
        }
    }

    #[test]
    fn pathological_inputs_pass_the_check() {
        let pounds = "#".repeat(255);
        let near_miss = format!("\"{}", "#".repeat(254));
        for text in [
            format!("r{pounds}\"{}\"{pounds}", near_miss.repeat(4)),
            format!("r{pounds}\"{}", near_miss.repeat(4)),
            "\"unterminated".repeat(8),
            "/*".repeat(64),
            format!("{}{}", "/*".repeat(64), "*/".repeat(64)),
            "'a' '' 'ab' 'é'".repeat(8),
            "a\u{200D}b café 東京 r#é 'é 1é".to_owned(),
            "\u{FEFF}#!/usr/bin/env run\nfn main() {}".to_owned(),
            "#".repeat(300),
            "1.0e10f64 0x1.e 0b12 1e+".to_owned(),
        ] {
            check_lexing(&text);
        }
    }

    #[test]
    fn short_texts_pass_the_check() {
        for text in ["", " ", "\u{FEFF}", "\r", "'", "\"", "r#", "/*", "`"] {
            check_lexing(text);
        }
    }

    #[cfg(feature = "fuzzing")]
    #[test]
    fn invalid_utf8_is_not_lexed() {
        lex(b"fn main() {}\xff");
        lex(&[0xC3]);
        lex(b"");
    }
}
