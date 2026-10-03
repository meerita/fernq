//! The lexer: the text of one loaded source file to lexical tokens.
//!
//! A [`Lexer`] reads one stored source text with the edition of that source
//! and returns one [`Token`] per call, until end of file or the first lexical
//! error. It stores no tokens; retention belongs to the caller.
//!
//! The supported lexical surface is ASCII identifiers, strict and reserved
//! keywords, and the three delimiter pairs. The lexer skips a byte order mark
//! at offset 0, a shebang at the start of the text, whitespace, and non-doc
//! comments. Any other start ends lexing with a [`LexError`]: invalid when no
//! Rust token can start there, unsupported otherwise. The lexer does not pair
//! delimiters.
//!
//! Every span is in the coordinates of the text as stored.

use crate::edition::Edition;
use crate::source::{ByteOffset, SourceId, Span};

/// A token: its kind and the span of its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// An identifier that is not a strict or reserved keyword in the edition
    /// of the source. Weak keywords are identifiers.
    Identifier,
    Keyword(Keyword),
    OpenDelimiter(Delimiter),
    CloseDelimiter(Delimiter),
    /// The end of the text. Its span is the empty span at the text length.
    EndOfFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delimiter {
    /// `(` and `)`.
    Parenthesis,
    /// `[` and `]`.
    Bracket,
    /// `{` and `}`.
    Brace,
}

/// A strict or reserved keyword of the Rust Reference for Rust 1.99.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keyword {
    As,
    Async,
    Await,
    Break,
    Const,
    Continue,
    Crate,
    Dyn,
    Else,
    Enum,
    Extern,
    False,
    Fn,
    For,
    If,
    Impl,
    In,
    Let,
    Loop,
    Match,
    Mod,
    Move,
    Mut,
    Pub,
    Ref,
    Return,
    /// `self`.
    SelfValue,
    /// `Self`.
    SelfType,
    Static,
    Struct,
    Super,
    Trait,
    True,
    Type,
    /// `_` alone.
    Underscore,
    Unsafe,
    Use,
    Where,
    While,
    Abstract,
    Become,
    Box,
    Do,
    Final,
    Gen,
    Macro,
    Override,
    Priv,
    Try,
    Typeof,
    Unsized,
    Virtual,
    Yield,
}

impl Keyword {
    /// Returns the keyword spelled `text` in `edition`, or `None` when `text`
    /// is not a strict or reserved keyword in that edition.
    fn from_text(text: &[u8], edition: Edition) -> Option<Self> {
        let (keyword, since) = match text {
            b"as" => (Self::As, Edition::E2015),
            b"async" => (Self::Async, Edition::E2018),
            b"await" => (Self::Await, Edition::E2018),
            b"break" => (Self::Break, Edition::E2015),
            b"const" => (Self::Const, Edition::E2015),
            b"continue" => (Self::Continue, Edition::E2015),
            b"crate" => (Self::Crate, Edition::E2015),
            b"dyn" => (Self::Dyn, Edition::E2018),
            b"else" => (Self::Else, Edition::E2015),
            b"enum" => (Self::Enum, Edition::E2015),
            b"extern" => (Self::Extern, Edition::E2015),
            b"false" => (Self::False, Edition::E2015),
            b"fn" => (Self::Fn, Edition::E2015),
            b"for" => (Self::For, Edition::E2015),
            b"if" => (Self::If, Edition::E2015),
            b"impl" => (Self::Impl, Edition::E2015),
            b"in" => (Self::In, Edition::E2015),
            b"let" => (Self::Let, Edition::E2015),
            b"loop" => (Self::Loop, Edition::E2015),
            b"match" => (Self::Match, Edition::E2015),
            b"mod" => (Self::Mod, Edition::E2015),
            b"move" => (Self::Move, Edition::E2015),
            b"mut" => (Self::Mut, Edition::E2015),
            b"pub" => (Self::Pub, Edition::E2015),
            b"ref" => (Self::Ref, Edition::E2015),
            b"return" => (Self::Return, Edition::E2015),
            b"self" => (Self::SelfValue, Edition::E2015),
            b"Self" => (Self::SelfType, Edition::E2015),
            b"static" => (Self::Static, Edition::E2015),
            b"struct" => (Self::Struct, Edition::E2015),
            b"super" => (Self::Super, Edition::E2015),
            b"trait" => (Self::Trait, Edition::E2015),
            b"true" => (Self::True, Edition::E2015),
            b"type" => (Self::Type, Edition::E2015),
            b"_" => (Self::Underscore, Edition::E2015),
            b"unsafe" => (Self::Unsafe, Edition::E2015),
            b"use" => (Self::Use, Edition::E2015),
            b"where" => (Self::Where, Edition::E2015),
            b"while" => (Self::While, Edition::E2015),
            b"abstract" => (Self::Abstract, Edition::E2015),
            b"become" => (Self::Become, Edition::E2015),
            b"box" => (Self::Box, Edition::E2015),
            b"do" => (Self::Do, Edition::E2015),
            b"final" => (Self::Final, Edition::E2015),
            b"gen" => (Self::Gen, Edition::E2024),
            b"macro" => (Self::Macro, Edition::E2015),
            b"override" => (Self::Override, Edition::E2015),
            b"priv" => (Self::Priv, Edition::E2015),
            b"try" => (Self::Try, Edition::E2018),
            b"typeof" => (Self::Typeof, Edition::E2015),
            b"unsized" => (Self::Unsized, Edition::E2015),
            b"virtual" => (Self::Virtual, Edition::E2015),
            b"yield" => (Self::Yield, Edition::E2015),
            _ => return None,
        };
        (edition >= since).then_some(keyword)
    }
}

/// The error that ends lexing: where it is and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LexError {
    source: SourceId,
    span: Span,
    kind: LexErrorKind,
}

impl LexError {
    /// The source whose text the lexer read.
    pub(crate) fn source(self) -> SourceId {
        self.source
    }

    pub(crate) fn span(self) -> Span {
        self.span
    }

    pub(crate) fn kind(self) -> LexErrorKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LexErrorKind {
    /// No Rust token can start at the span, so the text is not valid Rust.
    Invalid(Invalid),
    /// Input outside the supported lexical surface starts at the span. The
    /// text may or may not be valid Rust.
    Unsupported(Unsupported),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invalid {
    /// A character that starts no Rust token. The span is the character.
    Character(char),
    /// A block comment without its closing `*/`. The span runs from the
    /// outermost `/*` to the end of the text.
    UnterminatedBlockComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// A character that starts input the lexer does not support. The span is
    /// the character.
    Character(char),
    /// A doc comment. The span is its opening `///`, `//!`, `/**`, or `/*!`.
    DocComment,
}

/// A pull lexer over the stored text of one source file.
pub(crate) struct Lexer<'text> {
    source: SourceId,
    text: &'text str,
    edition: Edition,
    /// The offset of the next unread byte, at a character boundary.
    pos: usize,
    /// The result of every later call, once lexing has ended.
    end: Option<Result<Token, LexError>>,
}

impl<'text> Lexer<'text> {
    /// Returns a lexer for `text`, the stored text of the source `source`,
    /// whose edition is `edition`.
    ///
    /// # Panics
    ///
    /// Panics if `text` is longer than `u32::MAX` bytes, which the source
    /// table never admits.
    pub(crate) fn new(source: SourceId, text: &'text str, edition: Edition) -> Self {
        assert!(
            ByteOffset::try_from(text.len()).is_ok(),
            "the source table admits no text longer than MAX_SOURCE_LEN bytes"
        );
        let mut lexer = Self {
            source,
            text,
            edition,
            pos: 0,
            end: None,
        };
        lexer.pos = lexer.start_of_tokens();
        lexer
    }

    /// Returns the next token, or the error that ends lexing.
    ///
    /// After an [`TokenKind::EndOfFile`] token or an error, every later call
    /// returns the same result.
    pub(crate) fn next_token(&mut self) -> Result<Token, LexError> {
        if let Some(end) = self.end {
            return end;
        }
        let result = self.lex_token();
        if !matches!(result, Ok(token) if token.kind != TokenKind::EndOfFile) {
            self.end = Some(result);
        }
        result
    }

    /// Returns the offset after a byte order mark at offset 0 and a shebang
    /// that follows it.
    fn start_of_tokens(&self) -> usize {
        let start = if self.text.starts_with('\u{FEFF}') {
            '\u{FEFF}'.len_utf8()
        } else {
            0
        };
        if self.is_shebang(start) {
            self.line_end(start)
        } else {
            start
        }
    }

    /// Whether a shebang starts at `pos`: `#!` not followed by `[` after
    /// whitespace and non-doc comments.
    fn is_shebang(&self, pos: usize) -> bool {
        if !self.rest(pos).starts_with(b"#!") {
            return false;
        }
        match self.skip_trivia(pos + 2) {
            Ok(next) => !self.rest(next).starts_with(b"["),
            // A doc comment or an unterminated block comment ends the lookahead.
            Err(_) => true,
        }
    }

    fn lex_token(&mut self) -> Result<Token, LexError> {
        let start = self.skip_trivia(self.pos)?;
        let Some(c) = self.char_at(start) else {
            let len = self.text.len();
            return Ok(Self::token(TokenKind::EndOfFile, len, len));
        };
        let (kind, end) = match c {
            '(' => (TokenKind::OpenDelimiter(Delimiter::Parenthesis), start + 1),
            '[' => (TokenKind::OpenDelimiter(Delimiter::Bracket), start + 1),
            '{' => (TokenKind::OpenDelimiter(Delimiter::Brace), start + 1),
            ')' => (TokenKind::CloseDelimiter(Delimiter::Parenthesis), start + 1),
            ']' => (TokenKind::CloseDelimiter(Delimiter::Bracket), start + 1),
            '}' => (TokenKind::CloseDelimiter(Delimiter::Brace), start + 1),
            'A'..='Z' | 'a'..='z' | '_' => {
                let identifier = self
                    .rest(start)
                    .split(|&byte| !is_identifier_continue(byte))
                    .next()
                    .unwrap_or_default();
                let kind = Keyword::from_text(identifier, self.edition)
                    .map_or(TokenKind::Identifier, TokenKind::Keyword);
                (kind, start + identifier.len())
            }
            '`' | '\\' | '\u{0}'..='\u{8}' | '\u{E}'..='\u{1F}' | '\u{7F}' => {
                let kind = LexErrorKind::Invalid(Invalid::Character(c));
                return Err(self.error(kind, start, start + c.len_utf8()));
            }
            _ => {
                let kind = LexErrorKind::Unsupported(Unsupported::Character(c));
                return Err(self.error(kind, start, start + c.len_utf8()));
            }
        };
        self.pos = end;
        Ok(Self::token(kind, start, end))
    }

    /// Returns the offset after the whitespace and non-doc comments at `pos`.
    fn skip_trivia(&self, mut pos: usize) -> Result<usize, LexError> {
        loop {
            let rest = self.rest(pos);
            if rest.starts_with(b"//") {
                if is_line_doc_comment(rest) {
                    let kind = LexErrorKind::Unsupported(Unsupported::DocComment);
                    return Err(self.error(kind, pos, pos + 3));
                }
                pos = self.line_end(pos);
            } else if rest.starts_with(b"/*") {
                if is_block_doc_comment(rest) {
                    let kind = LexErrorKind::Unsupported(Unsupported::DocComment);
                    return Err(self.error(kind, pos, pos + 3));
                }
                pos = self.block_comment_end(pos)?;
            } else if let Some(c) = self.char_at(pos).filter(|&c| is_whitespace(c)) {
                pos += c.len_utf8();
            } else {
                return Ok(pos);
            }
        }
    }

    /// Returns the offset of the first LF at or after `pos`, or the text
    /// length when there is none.
    fn line_end(&self, pos: usize) -> usize {
        let rest = self.rest(pos);
        pos + rest
            .iter()
            .position(|&byte| byte == b'\n')
            .unwrap_or(rest.len())
    }

    /// Returns the offset after the block comment that opens at `start`.
    ///
    /// Block comments nest. A depth counter, not recursion, tracks nesting.
    fn block_comment_end(&self, start: usize) -> Result<usize, LexError> {
        let bytes = self.text.as_bytes();
        let mut depth: usize = 1;
        let mut pos = start + 2;
        while let Some(&byte) = bytes.get(pos) {
            let next = bytes.get(pos + 1).copied();
            if byte == b'/' && next == Some(b'*') {
                depth += 1;
                pos += 2;
            } else if byte == b'*' && next == Some(b'/') {
                depth -= 1;
                pos += 2;
                if depth == 0 {
                    return Ok(pos);
                }
            } else {
                pos += 1;
            }
        }
        let kind = LexErrorKind::Invalid(Invalid::UnterminatedBlockComment);
        Err(self.error(kind, start, bytes.len()))
    }

    /// The bytes from `pos` to the end of the text.
    fn rest(&self, pos: usize) -> &'text [u8] {
        self.text.as_bytes().get(pos..).unwrap_or_default()
    }

    /// The character that starts at `pos`, or `None` at the end of the text.
    fn char_at(&self, pos: usize) -> Option<char> {
        self.text.get(pos..)?.chars().next()
    }

    fn token(kind: TokenKind, lo: usize, hi: usize) -> Token {
        Token {
            kind,
            span: span(lo, hi),
        }
    }

    fn error(&self, kind: LexErrorKind, lo: usize, hi: usize) -> LexError {
        LexError {
            source: self.source,
            span: span(lo, hi),
            kind,
        }
    }
}

/// Returns the span `lo..hi` of offsets in a text that [`Lexer::new`] admitted.
fn span(lo: usize, hi: usize) -> Span {
    let offset =
        |pos| ByteOffset::try_from(pos).expect("an offset in an admitted text fits a ByteOffset");
    Span::new(offset(lo), offset(hi)).expect("the lexer creates no span that ends before it starts")
}

/// Whether `rest`, which starts with `//`, starts a doc comment: `//!`, or
/// `///` not followed by another `/`.
fn is_line_doc_comment(rest: &[u8]) -> bool {
    match rest.get(2) {
        Some(b'!') => true,
        Some(b'/') => rest.get(3) != Some(&b'/'),
        _ => false,
    }
}

/// Whether `rest`, which starts with `/*`, starts a doc comment: `/*!`, or
/// `/**` followed by a byte other than `*` and `/`.
fn is_block_doc_comment(rest: &[u8]) -> bool {
    match rest.get(2) {
        Some(b'!') => true,
        Some(b'*') => rest
            .get(3)
            .is_some_and(|&byte| byte != b'*' && byte != b'/'),
        _ => false,
    }
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `c` is Rust whitespace, `[lex.whitespace]`.
fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{D}' | ' ' | '\u{85}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::source::SourceTable;

    const EDITIONS: [Edition; 4] = [
        Edition::E2015,
        Edition::E2018,
        Edition::E2021,
        Edition::E2024,
    ];

    /// Every strict keyword, with the first edition in which it is one.
    const STRICT: [(&str, Keyword, Edition); 39] = [
        ("as", Keyword::As, Edition::E2015),
        ("break", Keyword::Break, Edition::E2015),
        ("const", Keyword::Const, Edition::E2015),
        ("continue", Keyword::Continue, Edition::E2015),
        ("crate", Keyword::Crate, Edition::E2015),
        ("else", Keyword::Else, Edition::E2015),
        ("enum", Keyword::Enum, Edition::E2015),
        ("extern", Keyword::Extern, Edition::E2015),
        ("false", Keyword::False, Edition::E2015),
        ("fn", Keyword::Fn, Edition::E2015),
        ("for", Keyword::For, Edition::E2015),
        ("if", Keyword::If, Edition::E2015),
        ("impl", Keyword::Impl, Edition::E2015),
        ("in", Keyword::In, Edition::E2015),
        ("let", Keyword::Let, Edition::E2015),
        ("loop", Keyword::Loop, Edition::E2015),
        ("match", Keyword::Match, Edition::E2015),
        ("mod", Keyword::Mod, Edition::E2015),
        ("move", Keyword::Move, Edition::E2015),
        ("mut", Keyword::Mut, Edition::E2015),
        ("pub", Keyword::Pub, Edition::E2015),
        ("ref", Keyword::Ref, Edition::E2015),
        ("return", Keyword::Return, Edition::E2015),
        ("self", Keyword::SelfValue, Edition::E2015),
        ("Self", Keyword::SelfType, Edition::E2015),
        ("static", Keyword::Static, Edition::E2015),
        ("struct", Keyword::Struct, Edition::E2015),
        ("super", Keyword::Super, Edition::E2015),
        ("trait", Keyword::Trait, Edition::E2015),
        ("true", Keyword::True, Edition::E2015),
        ("type", Keyword::Type, Edition::E2015),
        ("unsafe", Keyword::Unsafe, Edition::E2015),
        ("use", Keyword::Use, Edition::E2015),
        ("where", Keyword::Where, Edition::E2015),
        ("while", Keyword::While, Edition::E2015),
        ("async", Keyword::Async, Edition::E2018),
        ("await", Keyword::Await, Edition::E2018),
        ("dyn", Keyword::Dyn, Edition::E2018),
        ("_", Keyword::Underscore, Edition::E2015),
    ];

    /// Every reserved keyword, with the first edition in which it is one.
    const RESERVED: [(&str, Keyword, Edition); 14] = [
        ("abstract", Keyword::Abstract, Edition::E2015),
        ("become", Keyword::Become, Edition::E2015),
        ("box", Keyword::Box, Edition::E2015),
        ("do", Keyword::Do, Edition::E2015),
        ("final", Keyword::Final, Edition::E2015),
        ("macro", Keyword::Macro, Edition::E2015),
        ("override", Keyword::Override, Edition::E2015),
        ("priv", Keyword::Priv, Edition::E2015),
        ("typeof", Keyword::Typeof, Edition::E2015),
        ("unsized", Keyword::Unsized, Edition::E2015),
        ("virtual", Keyword::Virtual, Edition::E2015),
        ("yield", Keyword::Yield, Edition::E2015),
        ("try", Keyword::Try, Edition::E2018),
        ("gen", Keyword::Gen, Edition::E2024),
    ];

    /// The weak keywords that are identifiers in form. `'static` is a
    /// lifetime and `dyn` is weak only in 2015.
    const WEAK: [&str; 4] = ["macro_rules", "raw", "safe", "union"];

    fn span(lo: usize, hi: usize) -> Span {
        super::span(lo, hi)
    }

    fn token(kind: TokenKind, lo: usize, hi: usize) -> Result<Token, LexError> {
        Ok(Token {
            kind,
            span: span(lo, hi),
        })
    }

    fn end(len: usize) -> Result<Token, LexError> {
        token(TokenKind::EndOfFile, len, len)
    }

    /// Lexes `text` in `edition` and returns every result up to and
    /// including the end-of-file token or the first error.
    fn lex_in(text: &str, edition: Edition) -> Vec<Result<Token, LexError>> {
        let mut sources = SourceTable::default();
        let id = sources.add_text("main.rs", text);
        let mut lexer = Lexer::new(id, text, edition);
        let mut results = Vec::new();
        loop {
            let result = lexer.next_token();
            results.push(result);
            if !matches!(result, Ok(token) if token.kind != TokenKind::EndOfFile) {
                return results;
            }
            assert!(results.len() <= text.len(), "{text:?} does not end");
        }
    }

    fn lex(text: &str) -> Vec<Result<Token, LexError>> {
        lex_in(text, Edition::E2024)
    }

    /// The error of the source that [`lex_in`] creates.
    fn error(kind: LexErrorKind, lo: usize, hi: usize) -> Result<Token, LexError> {
        let mut sources = SourceTable::default();
        Err(LexError {
            source: sources.add_text("main.rs", ""),
            span: span(lo, hi),
            kind,
        })
    }

    fn invalid(c: char, lo: usize) -> Result<Token, LexError> {
        error(
            LexErrorKind::Invalid(Invalid::Character(c)),
            lo,
            lo + c.len_utf8(),
        )
    }

    fn unsupported(c: char, lo: usize) -> Result<Token, LexError> {
        error(
            LexErrorKind::Unsupported(Unsupported::Character(c)),
            lo,
            lo + c.len_utf8(),
        )
    }

    fn doc_comment(lo: usize) -> Result<Token, LexError> {
        error(
            LexErrorKind::Unsupported(Unsupported::DocComment),
            lo,
            lo + 3,
        )
    }

    fn unterminated(lo: usize, hi: usize) -> Result<Token, LexError> {
        error(
            LexErrorKind::Invalid(Invalid::UnterminatedBlockComment),
            lo,
            hi,
        )
    }

    fn ident(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::Identifier, lo, hi)
    }

    fn keyword(keyword: Keyword, lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::Keyword(keyword), lo, hi)
    }

    fn open(delimiter: Delimiter, lo: usize) -> Result<Token, LexError> {
        token(TokenKind::OpenDelimiter(delimiter), lo, lo + 1)
    }

    fn close(delimiter: Delimiter, lo: usize) -> Result<Token, LexError> {
        token(TokenKind::CloseDelimiter(delimiter), lo, lo + 1)
    }

    /// The tokens of `fn main() {}` when `fn` starts at `lo`.
    fn fn_main_at(lo: usize) -> Vec<Result<Token, LexError>> {
        vec![
            keyword(Keyword::Fn, lo, lo + 2),
            ident(lo + 3, lo + 7),
            open(Delimiter::Parenthesis, lo + 7),
            close(Delimiter::Parenthesis, lo + 8),
            open(Delimiter::Brace, lo + 10),
            close(Delimiter::Brace, lo + 11),
        ]
    }

    fn with_end(
        mut results: Vec<Result<Token, LexError>>,
        last: Result<Token, LexError>,
    ) -> Vec<Result<Token, LexError>> {
        results.push(last);
        results
    }

    #[test]
    fn the_keyword_tables_have_the_reference_counts() {
        assert_eq!(STRICT.len(), 39);
        assert_eq!(RESERVED.len(), 14);
    }

    #[test]
    fn classifies_every_strict_and_reserved_keyword_by_edition() {
        for edition in EDITIONS {
            for (text, expected, since) in STRICT.iter().chain(&RESERVED) {
                let kind = if edition >= *since {
                    TokenKind::Keyword(*expected)
                } else {
                    TokenKind::Identifier
                };
                assert_eq!(
                    lex_in(text, edition),
                    vec![token(kind, 0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn edition_dependent_keywords_change_in_their_editions() {
        let first_keyword_edition = [
            ("async", Edition::E2018),
            ("await", Edition::E2018),
            ("dyn", Edition::E2018),
            ("try", Edition::E2018),
            ("gen", Edition::E2024),
        ];
        for (text, since) in first_keyword_edition {
            for edition in EDITIONS {
                let is_keyword = matches!(
                    lex_in(text, edition)[0],
                    Ok(Token {
                        kind: TokenKind::Keyword(_),
                        ..
                    })
                );
                assert_eq!(is_keyword, edition >= since, "{text:?} in {edition:?}");
            }
        }
    }

    #[test]
    fn weak_keywords_are_identifiers_in_every_edition() {
        for edition in EDITIONS {
            for text in WEAK {
                assert_eq!(
                    lex_in(text, edition),
                    vec![ident(0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn a_lifetime_is_unsupported() {
        assert_eq!(lex("'static"), vec![unsupported('\'', 0)]);
    }

    #[test]
    fn identifiers_that_resemble_keywords_are_identifiers() {
        for text in [
            "_x", "__", "x_", "fnx", "as1", "FN", "SELF", "self_", "_1", "r",
        ] {
            assert_eq!(
                lex(text),
                vec![ident(0, text.len()), end(text.len())],
                "{text:?}"
            );
        }
    }

    #[test]
    fn lexes_fn_main() {
        assert_eq!(lex("fn main() {}"), with_end(fn_main_at(0), end(12)));
    }

    #[test]
    fn lexes_every_delimiter_without_pairing() {
        assert_eq!(
            lex("([{)]}"),
            vec![
                open(Delimiter::Parenthesis, 0),
                open(Delimiter::Bracket, 1),
                open(Delimiter::Brace, 2),
                close(Delimiter::Parenthesis, 3),
                close(Delimiter::Bracket, 4),
                close(Delimiter::Brace, 5),
                end(6),
            ]
        );
        assert_eq!(
            lex("fn main( {}"),
            vec![
                keyword(Keyword::Fn, 0, 2),
                ident(3, 7),
                open(Delimiter::Parenthesis, 7),
                open(Delimiter::Brace, 9),
                close(Delimiter::Brace, 10),
                end(11),
            ]
        );
    }

    #[test]
    fn spans_after_a_byte_order_mark() {
        assert_eq!(
            lex("\u{FEFF}fn main() {}"),
            with_end(fn_main_at(3), end(15))
        );
    }

    #[test]
    fn spans_after_a_shebang() {
        assert_eq!(
            lex("#!/usr/bin/env run\nfn main() {}\n"),
            with_end(fn_main_at(19), end(32))
        );
    }

    #[test]
    fn spans_with_crlf() {
        assert_eq!(
            lex("fn\r\nmain\r\n()\r\n{}\r\n"),
            vec![
                keyword(Keyword::Fn, 0, 2),
                ident(4, 8),
                open(Delimiter::Parenthesis, 10),
                close(Delimiter::Parenthesis, 11),
                open(Delimiter::Brace, 14),
                close(Delimiter::Brace, 15),
                end(18),
            ]
        );
    }

    #[test]
    fn spans_with_a_lone_cr() {
        assert_eq!(
            lex("fn\rmain"),
            vec![keyword(Keyword::Fn, 0, 2), ident(3, 7), end(7)]
        );
    }

    #[test]
    fn spans_with_a_line_separator() {
        assert_eq!(
            lex("fn\u{2028}main"),
            vec![keyword(Keyword::Fn, 0, 2), ident(5, 9), end(9)]
        );
    }

    #[test]
    fn spans_after_multibyte_characters_in_comments() {
        assert_eq!(
            lex("// café 中\nfn /* é 😀 */ x"),
            vec![keyword(Keyword::Fn, 13, 15), ident(30, 31), end(31),]
        );
    }

    #[test]
    fn skips_every_whitespace_character() {
        for c in [
            '\t', '\n', '\u{B}', '\u{C}', '\r', ' ', '\u{85}', '\u{200E}', '\u{200F}', '\u{2028}',
            '\u{2029}',
        ] {
            let text = format!("a{c}b");
            let b = 1 + c.len_utf8();
            assert_eq!(
                lex(&text),
                vec![ident(0, 1), ident(b, b + 1), end(b + 1)],
                "{c:?}"
            );
        }
    }

    #[test]
    fn a_shebang_at_the_start_or_after_a_byte_order_mark_is_skipped() {
        assert_eq!(
            lex("#!/usr/bin/env run\nfn main() {}\n"),
            with_end(fn_main_at(19), end(32))
        );
        assert_eq!(
            lex("\u{FEFF}#!/usr/bin/env run\nfn main() {}\n"),
            with_end(fn_main_at(22), end(35))
        );
    }

    #[test]
    fn a_shebang_after_the_start_is_unsupported() {
        assert_eq!(
            lex("\n#!/usr/bin/env run\nfn main() {}\n"),
            vec![unsupported('#', 1)]
        );
        assert_eq!(lex(" #!x"), vec![unsupported('#', 1)]);
    }

    #[test]
    fn an_inner_attribute_is_not_a_shebang() {
        for text in [
            "#![allow(unused)]",
            "#!/*c*/ [allow(unused)]\nfn main() {}\n",
            "#!\n[x]",
            "#!// c\n[x]",
            "#! /* a /* b */ */ \t[x]",
        ] {
            assert_eq!(lex(text), vec![unsupported('#', 0)], "{text:?}");
        }
    }

    #[test]
    fn a_shebang_runs_to_the_first_lf_or_the_end_of_the_text() {
        assert_eq!(lex("#!"), vec![end(2)]);
        assert_eq!(lex("\u{FEFF}#!"), vec![end(5)]);
        assert_eq!(lex("#!x `\\ é"), vec![end(9)]);
        assert_eq!(lex("#!\r\nfn"), vec![keyword(Keyword::Fn, 4, 6), end(6)]);
    }

    #[test]
    fn an_unterminated_block_comment_ends_the_shebang_lookahead() {
        assert_eq!(
            lex("#!/* x\nfn [\n"),
            vec![
                keyword(Keyword::Fn, 7, 9),
                open(Delimiter::Bracket, 10),
                end(12),
            ]
        );
    }

    #[test]
    fn a_doc_comment_ends_the_shebang_lookahead() {
        assert_eq!(
            lex("#!/// d\n[x]"),
            vec![
                open(Delimiter::Bracket, 8),
                ident(9, 10),
                close(Delimiter::Bracket, 10),
                end(11),
            ]
        );
        assert_eq!(lex("#!//! d\n["), vec![open(Delimiter::Bracket, 8), end(9)]);
        assert_eq!(lex("#!/** d */["), vec![end(11)]);
    }

    #[test]
    fn block_comments_nest() {
        assert_eq!(
            lex("/* a /* b */ c */fn"),
            vec![keyword(Keyword::Fn, 17, 19), end(19)]
        );
        assert_eq!(lex("/*/**/*/x"), vec![ident(8, 9), end(9)]);
    }

    #[test]
    fn non_doc_comments_are_skipped() {
        for text in [
            "/**/", "/***/", "/****/", "/* * */", "////", "//// x", "// x", "//", "/**/ ",
        ] {
            assert_eq!(lex(text), vec![end(text.len())], "{text:?}");
        }
        assert_eq!(lex("/**/fn"), vec![keyword(Keyword::Fn, 4, 6), end(6)]);
        assert_eq!(lex("/***/fn"), vec![keyword(Keyword::Fn, 5, 7), end(7)]);
        assert_eq!(lex("//// x\nfn"), vec![keyword(Keyword::Fn, 7, 9), end(9)]);
        assert_eq!(lex("fn // end"), vec![keyword(Keyword::Fn, 0, 2), end(9)]);
        assert_eq!(lex("fn //"), vec![keyword(Keyword::Fn, 0, 2), end(5)]);
        assert_eq!(lex("// a\rb\nx"), vec![ident(7, 8), end(8)]);
        assert_eq!(lex("/* a\rb */x"), vec![ident(9, 10), end(10)]);
    }

    #[test]
    fn every_doc_comment_form_is_unsupported_at_its_start() {
        for text in [
            "/// x", "///", "///\n", "//! x", "//!", "/** x */", "/**x*/", "/*! x */", "/*!",
            "/*!*/", "/** x",
        ] {
            assert_eq!(lex(text), vec![doc_comment(0)], "{text:?}");
        }
        assert_eq!(
            lex("fn main() {}\n/// doc\n"),
            with_end(fn_main_at(0), doc_comment(13))
        );
    }

    #[test]
    fn an_unterminated_block_comment_is_invalid_from_its_outermost_start() {
        assert_eq!(
            lex("fn /* a /* b */"),
            vec![keyword(Keyword::Fn, 0, 2), unterminated(3, 15)]
        );
        assert_eq!(
            lex("fn main() {}\n/* a /* b */\n"),
            with_end(fn_main_at(0), unterminated(13, 26))
        );
        assert_eq!(lex("/*"), vec![unterminated(0, 2)]);
        assert_eq!(lex("/*/"), vec![unterminated(0, 3)]);
        assert_eq!(lex("/**"), vec![unterminated(0, 3)]);
        assert_eq!(lex("/* é"), vec![unterminated(0, 5)]);
    }

    #[test]
    fn every_invalid_character_is_invalid() {
        let mut characters = vec!['`', '\\', '\u{7F}'];
        characters.extend('\u{0}'..='\u{8}');
        characters.extend('\u{E}'..='\u{1F}');
        for c in characters {
            let text = format!("fn {c} x");
            assert_eq!(
                lex(&text),
                vec![keyword(Keyword::Fn, 0, 2), invalid(c, 3)],
                "{c:?}"
            );
        }
    }

    #[test]
    fn every_other_start_is_unsupported() {
        let mut characters: Vec<char> = "0123456789\"'#!$%&*+,-./:;<=>?@^|~".chars().collect();
        characters.extend([
            '\u{A0}',
            'é',
            '€',
            '中',
            '😀',
            '\u{FEFF}',
            '\u{200B}',
            '\u{200D}',
            '\u{202E}',
            '\u{3000}',
            '\u{80}',
            '\u{FFFD}',
            '\u{10FFFF}',
        ]);
        for c in characters {
            let text = format!("fn {c} x");
            assert_eq!(
                lex(&text),
                vec![keyword(Keyword::Fn, 0, 2), unsupported(c, 3)],
                "{c:?}"
            );
        }
    }

    #[test]
    fn an_identifier_ends_at_the_first_other_character() {
        assert_eq!(lex("café"), vec![ident(0, 3), unsupported('é', 3)]);
        assert_eq!(lex("r#fn"), vec![ident(0, 1), unsupported('#', 1)]);
        assert_eq!(lex("b\"x\""), vec![ident(0, 1), unsupported('"', 1)]);
        assert_eq!(lex("x/y"), vec![ident(0, 1), unsupported('/', 1)]);
    }

    #[test]
    fn the_end_of_file_token_repeats() {
        for (text, len) in [
            ("", 0),
            ("\u{FEFF}", 3),
            ("// c", 4),
            ("/* c */", 7),
            ("fn", 2),
        ] {
            let mut sources = SourceTable::default();
            let id = sources.add_text("main.rs", text);
            let mut lexer = Lexer::new(id, text, Edition::E2024);
            while lexer
                .next_token()
                .is_ok_and(|token| token.kind != TokenKind::EndOfFile)
            {}
            for _ in 0..3 {
                assert_eq!(lexer.next_token(), end(len), "{text:?}");
            }
        }
    }

    #[test]
    fn an_error_repeats() {
        for (text, expected) in [
            ("fn `", invalid('`', 3)),
            ("x 1", unsupported('1', 2)),
            ("/// d", doc_comment(0)),
            ("/*", unterminated(0, 2)),
        ] {
            let mut sources = SourceTable::default();
            let id = sources.add_text("main.rs", text);
            let mut lexer = Lexer::new(id, text, Edition::E2024);
            while lexer.next_token().is_ok() {}
            for _ in 0..3 {
                assert_eq!(lexer.next_token(), expected, "{text:?}");
            }
        }
    }

    #[test]
    fn an_error_holds_the_source_of_the_lexer() {
        let mut sources = SourceTable::default();
        sources.add_text("a.rs", "");
        let id = sources.add_text("b.rs", "`");
        let error = Lexer::new(id, "`", Edition::E2024)
            .next_token()
            .unwrap_err();
        assert_eq!(error.source(), id);
        assert_eq!(error.span(), span(0, 1));
        assert_eq!(error.kind(), LexErrorKind::Invalid(Invalid::Character('`')));
    }

    /// Lexes every text of up to five characters from an alphabet of
    /// comment, shebang, delimiter, identifier, whitespace, invalid, and
    /// multibyte characters, and checks that each result lies in the text.
    #[test]
    fn every_short_text_lexes_to_spans_inside_the_text() {
        const ALPHABET: [char; 11] = [
            '/', '*', '!', '#', '[', '\n', 'a', ' ', 'é', '\u{FEFF}', '`',
        ];
        let mut texts = vec![String::new()];
        for _ in 0..5 {
            let longer: Vec<String> = texts
                .iter()
                .filter(|text| {
                    text.chars().count() == texts.last().map_or(0, |last| last.chars().count())
                })
                .flat_map(|text| ALPHABET.iter().map(move |c| format!("{text}{c}")))
                .collect();
            texts.extend(longer);
        }
        for text in &texts {
            let mut previous_hi = 0;
            for result in lex(text) {
                let span = match result {
                    Ok(token) => token.span,
                    Err(error) => error.span(),
                };
                let lo = usize::try_from(span.lo()).unwrap();
                let hi = usize::try_from(span.hi()).unwrap();
                assert!(
                    previous_hi <= lo && lo <= hi && hi <= text.len(),
                    "{text:?}: {result:?}"
                );
                assert!(
                    text.is_char_boundary(lo) && text.is_char_boundary(hi),
                    "{text:?}: {result:?}"
                );
                previous_hi = hi;
            }
        }
    }
}
