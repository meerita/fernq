//! The lexer: the text of one loaded source file to lexical tokens.
//!
//! A [`Lexer`] reads one stored source text with the edition of that source
//! and returns one [`Token`] per call, until end of file or the first lexical
//! error. It stores no tokens; retention belongs to the caller.
//!
//! The supported lexical surface is ASCII identifiers, strict and reserved
//! keywords, integer literals, punctuation, and the three delimiter pairs.
//! The lexer skips a byte order mark at offset 0, a shebang at the start of
//! the text, whitespace, and non-doc comments. Any other input ends lexing
//! with a [`LexError`]: invalid when the text is not valid Rust, unsupported
//! otherwise. The lexer does not pair delimiters and does not interpret
//! literals.
//!
//! The lexer returns no token whose extent an unsupported character directly
//! after it could still change. Every span is in the coordinates of the text
//! as stored.

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
    /// An integer literal in any radix, with its suffix if any. The suffix is
    /// not checked.
    IntegerLiteral,
    /// A punctuation token other than a delimiter.
    Punctuation(Punctuation),
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

/// A punctuation token of the Rust Reference other than a delimiter. Each is
/// one token, compound or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Punctuation {
    /// `+`.
    Plus,
    /// `-`.
    Minus,
    /// `*`.
    Star,
    /// `/`.
    Slash,
    /// `%`.
    Percent,
    /// `^`.
    Caret,
    /// `!`.
    Not,
    /// `&`.
    And,
    /// `|`.
    Or,
    /// `&&`.
    AndAnd,
    /// `||`.
    OrOr,
    /// `<<`.
    Shl,
    /// `>>`.
    Shr,
    /// `+=`.
    PlusEq,
    /// `-=`.
    MinusEq,
    /// `*=`.
    StarEq,
    /// `/=`.
    SlashEq,
    /// `%=`.
    PercentEq,
    /// `^=`.
    CaretEq,
    /// `&=`.
    AndEq,
    /// `|=`.
    OrEq,
    /// `<<=`.
    ShlEq,
    /// `>>=`.
    ShrEq,
    /// `=`.
    Eq,
    /// `==`.
    EqEq,
    /// `!=`.
    Ne,
    /// `>`.
    Gt,
    /// `<`.
    Lt,
    /// `>=`.
    Ge,
    /// `<=`.
    Le,
    /// `@`.
    At,
    /// `.`.
    Dot,
    /// `..`.
    DotDot,
    /// `...`.
    DotDotDot,
    /// `..=`.
    DotDotEq,
    /// `,`.
    Comma,
    /// `;`.
    Semi,
    /// `:`.
    Colon,
    /// `::`.
    PathSep,
    /// `->`.
    RArrow,
    /// `=>`.
    FatArrow,
    /// `<-`.
    LArrow,
    /// `#`.
    Pound,
    /// `$`.
    Dollar,
    /// `?`.
    Question,
    /// `~`.
    Tilde,
}

impl Punctuation {
    /// Returns the longest punctuation token at the start of `rest` and its
    /// length in bytes, or `None` when `rest` starts with no punctuation.
    fn at_start(rest: &[u8]) -> Option<(Self, usize)> {
        let found = match rest {
            [b'.', b'.', b'.', ..] => (Self::DotDotDot, 3),
            [b'.', b'.', b'=', ..] => (Self::DotDotEq, 3),
            [b'<', b'<', b'=', ..] => (Self::ShlEq, 3),
            [b'>', b'>', b'=', ..] => (Self::ShrEq, 3),
            [b'!', b'=', ..] => (Self::Ne, 2),
            [b'%', b'=', ..] => (Self::PercentEq, 2),
            [b'&', b'&', ..] => (Self::AndAnd, 2),
            [b'&', b'=', ..] => (Self::AndEq, 2),
            [b'*', b'=', ..] => (Self::StarEq, 2),
            [b'+', b'=', ..] => (Self::PlusEq, 2),
            [b'-', b'=', ..] => (Self::MinusEq, 2),
            [b'-', b'>', ..] => (Self::RArrow, 2),
            [b'.', b'.', ..] => (Self::DotDot, 2),
            [b'/', b'=', ..] => (Self::SlashEq, 2),
            [b':', b':', ..] => (Self::PathSep, 2),
            [b'<', b'-', ..] => (Self::LArrow, 2),
            [b'<', b'<', ..] => (Self::Shl, 2),
            [b'<', b'=', ..] => (Self::Le, 2),
            [b'=', b'=', ..] => (Self::EqEq, 2),
            [b'=', b'>', ..] => (Self::FatArrow, 2),
            [b'>', b'=', ..] => (Self::Ge, 2),
            [b'>', b'>', ..] => (Self::Shr, 2),
            [b'^', b'=', ..] => (Self::CaretEq, 2),
            [b'|', b'=', ..] => (Self::OrEq, 2),
            [b'|', b'|', ..] => (Self::OrOr, 2),
            [b'!', ..] => (Self::Not, 1),
            [b'#', ..] => (Self::Pound, 1),
            [b'$', ..] => (Self::Dollar, 1),
            [b'%', ..] => (Self::Percent, 1),
            [b'&', ..] => (Self::And, 1),
            [b'*', ..] => (Self::Star, 1),
            [b'+', ..] => (Self::Plus, 1),
            [b',', ..] => (Self::Comma, 1),
            [b'-', ..] => (Self::Minus, 1),
            [b'.', ..] => (Self::Dot, 1),
            [b'/', ..] => (Self::Slash, 1),
            [b':', ..] => (Self::Colon, 1),
            [b';', ..] => (Self::Semi, 1),
            [b'<', ..] => (Self::Lt, 1),
            [b'=', ..] => (Self::Eq, 1),
            [b'>', ..] => (Self::Gt, 1),
            [b'?', ..] => (Self::Question, 1),
            [b'@', ..] => (Self::At, 1),
            [b'^', ..] => (Self::Caret, 1),
            [b'|', ..] => (Self::Or, 1),
            [b'~', ..] => (Self::Tilde, 1),
            _ => return None,
        };
        Some(found)
    }
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
    /// The text at the span is not valid Rust in the edition of the source.
    Invalid(Invalid),
    /// Input outside the supported lexical surface is at the span. The text
    /// may or may not be valid Rust.
    Unsupported(Unsupported),
}

/// A reason the text is not valid Rust. The span of an integer literal
/// reason starts at the literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invalid {
    /// A character that starts no Rust token. The span is the character.
    Character(char),
    /// A block comment without its closing `*/`. The span runs from the
    /// outermost `/*` to the end of the text.
    UnterminatedBlockComment,
    /// From edition 2021, an identifier or keyword directly followed by `#`.
    /// The span is the identifier and the `#`.
    ReservedPrefix,
    /// From edition 2024, two or more `#` in a row not followed by `"`. The
    /// span is the run of `#`.
    ReservedPounds,
    /// From edition 2024, a run of `#` directly followed by `"`. The span is
    /// the run of `#`.
    ReservedGuardedString,
    /// A binary or octal literal followed by a decimal digit outside its
    /// radix. The span ends after that digit.
    DigitOutOfRadix,
    /// A radix prefix `0b`, `0o`, or `0x` with no digit of the radix after
    /// its underscores. The span is the prefix and the underscores.
    NoRadixDigits,
    /// A binary, octal, or hexadecimal literal without a suffix followed by
    /// `.` that starts no other token after it. The span ends after the `.`.
    RadixPeriod,
    /// A binary or octal literal followed by `e` or `E`. The span ends after
    /// that letter.
    RadixExponent,
    /// A decimal literal followed by `e` or `E` and no exponent digit after
    /// an optional sign and underscores. The span ends after the sign and
    /// underscores.
    EmptyExponent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// A character the lexer does not support, where it starts a token or
    /// where it could extend the token before it. The span is the character.
    Character(char),
    /// A doc comment. The span is its opening `///`, `//!`, `/**`, or `/*!`.
    DocComment,
    /// A raw identifier or raw string prefix: `r#` and `br#` in every
    /// edition, `cr#` from edition 2021. The span is the prefix and the `#`.
    RawPrefix,
    /// A floating-point literal. The span runs from the start of the literal
    /// to the `.` or the exponent letter.
    FloatLiteral,
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
            'A'..='Z' | 'a'..='z' | '_' => self.identifier_or_keyword(start)?,
            '0'..='9' => (TokenKind::IntegerLiteral, self.integer_literal_end(start)?),
            '#' => {
                self.check_reserved_guard(start)?;
                (TokenKind::Punctuation(Punctuation::Pound), start + 1)
            }
            _ => match Punctuation::at_start(self.rest(start)) {
                Some((punctuation, len)) => (TokenKind::Punctuation(punctuation), start + len),
                None => return Err(self.character_error(c, start)),
            },
        };
        self.pos = end;
        Ok(Self::token(kind, start, end))
    }

    /// Returns the kind and end of the identifier or keyword at `start`.
    fn identifier_or_keyword(&self, start: usize) -> Result<(TokenKind, usize), LexError> {
        let identifier = self
            .rest(start)
            .split(|&byte| !is_identifier_continue(byte))
            .next()
            .unwrap_or_default();
        let end = start + identifier.len();
        match self.rest(end).first() {
            Some(b'#') => {
                let is_raw_prefix = match identifier {
                    b"r" | b"br" => true,
                    b"cr" => self.edition >= Edition::E2021,
                    _ => false,
                };
                if is_raw_prefix {
                    let kind = LexErrorKind::Unsupported(Unsupported::RawPrefix);
                    return Err(self.error(kind, start, end + 1));
                }
                if self.edition >= Edition::E2021 {
                    let kind = LexErrorKind::Invalid(Invalid::ReservedPrefix);
                    return Err(self.error(kind, start, end + 1));
                }
            }
            // A quote can make the identifier a literal prefix or a reserved prefix.
            Some(&quote @ (b'"' | b'\'')) => {
                let kind = LexErrorKind::Unsupported(Unsupported::Character(char::from(quote)));
                return Err(self.error(kind, end, end + 1));
            }
            _ => self.check_extent_end(end)?,
        }
        let kind = Keyword::from_text(identifier, self.edition)
            .map_or(TokenKind::Identifier, TokenKind::Keyword);
        Ok((kind, end))
    }

    /// Returns the end of the integer literal at `start`, which is a decimal
    /// digit, suffix included.
    ///
    /// The scan follows `INTEGER_LITERAL` of the Reference without
    /// backtracking: once a radix prefix, an exponent letter, or a `.` that
    /// continues the literal is read, the text is that form or an error.
    fn integer_literal_end(&self, start: usize) -> Result<usize, LexError> {
        let (radix, digits) = match self.rest(start) {
            [b'0', b'b', ..] => (2, start + 2),
            [b'0', b'o', ..] => (8, start + 2),
            [b'0', b'x', ..] => (16, start + 2),
            _ => (10, start),
        };
        let is_digit = |byte: u8| char::from(byte).is_digit(radix);
        let first_digit = digits + self.count_while(digits, |byte| byte == b'_');
        match self.rest(first_digit).first() {
            Some(&byte) if is_digit(byte) => {}
            Some(byte) if byte.is_ascii_digit() => {
                let kind = LexErrorKind::Invalid(Invalid::DigitOutOfRadix);
                return Err(self.error(kind, start, first_digit + 1));
            }
            _ => {
                let kind = LexErrorKind::Invalid(Invalid::NoRadixDigits);
                return Err(self.error(kind, start, first_digit));
            }
        }
        let end =
            first_digit + self.count_while(first_digit, |byte| is_digit(byte) || byte == b'_');

        match (radix, self.rest(end)) {
            (2 | 8, [next, ..]) if next.is_ascii_digit() => {
                let kind = LexErrorKind::Invalid(Invalid::DigitOutOfRadix);
                return Err(self.error(kind, start, end + 1));
            }
            (2 | 8, [b'e' | b'E', ..]) => {
                let kind = LexErrorKind::Invalid(Invalid::RadixExponent);
                return Err(self.error(kind, start, end + 1));
            }
            (10, [b'e' | b'E', ..]) => return Err(self.exponent_error(start, end)),
            (_, [b'.', after @ ..]) => match after.first() {
                // `..`, a field, or a method call: the literal ends before the `.`.
                Some(&next) if next == b'.' || next == b'_' || next.is_ascii_alphabetic() => {
                    return Ok(end);
                }
                _ => {
                    self.check_extent_end(end + 1)?;
                    let kind = if radix == 10 {
                        LexErrorKind::Unsupported(Unsupported::FloatLiteral)
                    } else {
                        LexErrorKind::Invalid(Invalid::RadixPeriod)
                    };
                    return Err(self.error(kind, start, end + 1));
                }
            },
            _ => {}
        }

        let suffix_len = match self.rest(end).first() {
            Some(byte) if byte.is_ascii_alphabetic() => {
                self.count_while(end, is_identifier_continue)
            }
            _ => 0,
        };
        self.check_extent_end(end + suffix_len)?;
        Ok(end + suffix_len)
    }

    /// Returns the error for the decimal literal at `start` whose digits end
    /// at `exponent`, an `e` or `E`: a float literal when an exponent digit
    /// follows its optional sign and underscores, an empty exponent otherwise.
    fn exponent_error(&self, start: usize, exponent: usize) -> LexError {
        let mut pos = exponent + 1;
        if matches!(self.rest(pos).first(), Some(b'+' | b'-')) {
            pos += 1;
        }
        pos += self.count_while(pos, |byte| byte == b'_');
        if self.rest(pos).first().is_some_and(u8::is_ascii_digit) {
            let kind = LexErrorKind::Unsupported(Unsupported::FloatLiteral);
            self.error(kind, start, exponent + 1)
        } else {
            let kind = LexErrorKind::Invalid(Invalid::EmptyExponent);
            self.error(kind, start, pos)
        }
    }

    /// From edition 2024, rejects the run of `#` at `start` when it is longer
    /// than one `#` or directly followed by `"`.
    fn check_reserved_guard(&self, start: usize) -> Result<(), LexError> {
        if self.edition < Edition::E2024 {
            return Ok(());
        }
        let end = start + self.count_while(start, |byte| byte == b'#');
        let reason = if self.rest(end).first() == Some(&b'"') {
            Invalid::ReservedGuardedString
        } else if end - start > 1 {
            Invalid::ReservedPounds
        } else {
            return Ok(());
        };
        Err(self.error(LexErrorKind::Invalid(reason), start, end))
    }

    /// Rejects a non-ASCII character other than whitespace at `pos`, directly
    /// after a token that an identifier character could extend.
    ///
    /// Fernq does not classify non-ASCII identifier characters, so such a
    /// token is not returned.
    fn check_extent_end(&self, pos: usize) -> Result<(), LexError> {
        match self.char_at(pos) {
            Some(c) if !c.is_ascii() && !is_whitespace(c) => {
                let kind = LexErrorKind::Unsupported(Unsupported::Character(c));
                Err(self.error(kind, pos, pos + c.len_utf8()))
            }
            _ => Ok(()),
        }
    }

    /// Returns the error for `c` at `pos`, a character that starts no
    /// supported token.
    fn character_error(&self, c: char, pos: usize) -> LexError {
        let kind = match c {
            '`' | '\\' | '\u{0}'..='\u{8}' | '\u{E}'..='\u{1F}' | '\u{7F}' => {
                LexErrorKind::Invalid(Invalid::Character(c))
            }
            _ => LexErrorKind::Unsupported(Unsupported::Character(c)),
        };
        self.error(kind, pos, pos + c.len_utf8())
    }

    /// The number of bytes from `pos` that satisfy `predicate`.
    fn count_while(&self, pos: usize, predicate: impl Fn(u8) -> bool) -> usize {
        self.rest(pos)
            .iter()
            .take_while(|&&byte| predicate(byte))
            .count()
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

    /// Every punctuation token of the Reference other than the delimiters:
    /// 25 compound, then 21 single.
    const PUNCTUATION: [(&str, Punctuation); 46] = [
        ("...", Punctuation::DotDotDot),
        ("..=", Punctuation::DotDotEq),
        ("<<=", Punctuation::ShlEq),
        (">>=", Punctuation::ShrEq),
        ("!=", Punctuation::Ne),
        ("%=", Punctuation::PercentEq),
        ("&&", Punctuation::AndAnd),
        ("&=", Punctuation::AndEq),
        ("*=", Punctuation::StarEq),
        ("+=", Punctuation::PlusEq),
        ("-=", Punctuation::MinusEq),
        ("->", Punctuation::RArrow),
        ("..", Punctuation::DotDot),
        ("/=", Punctuation::SlashEq),
        ("::", Punctuation::PathSep),
        ("<-", Punctuation::LArrow),
        ("<<", Punctuation::Shl),
        ("<=", Punctuation::Le),
        ("==", Punctuation::EqEq),
        ("=>", Punctuation::FatArrow),
        (">=", Punctuation::Ge),
        (">>", Punctuation::Shr),
        ("^=", Punctuation::CaretEq),
        ("|=", Punctuation::OrEq),
        ("||", Punctuation::OrOr),
        ("!", Punctuation::Not),
        ("#", Punctuation::Pound),
        ("$", Punctuation::Dollar),
        ("%", Punctuation::Percent),
        ("&", Punctuation::And),
        ("*", Punctuation::Star),
        ("+", Punctuation::Plus),
        (",", Punctuation::Comma),
        ("-", Punctuation::Minus),
        (".", Punctuation::Dot),
        ("/", Punctuation::Slash),
        (":", Punctuation::Colon),
        (";", Punctuation::Semi),
        ("<", Punctuation::Lt),
        ("=", Punctuation::Eq),
        (">", Punctuation::Gt),
        ("?", Punctuation::Question),
        ("@", Punctuation::At),
        ("^", Punctuation::Caret),
        ("|", Punctuation::Or),
        ("~", Punctuation::Tilde),
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

    fn punct(punctuation: Punctuation, lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::Punctuation(punctuation), lo, hi)
    }

    fn int(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::IntegerLiteral, lo, hi)
    }

    fn invalid_at(reason: Invalid, lo: usize, hi: usize) -> Result<Token, LexError> {
        error(LexErrorKind::Invalid(reason), lo, hi)
    }

    fn unsupported_at(reason: Unsupported, lo: usize, hi: usize) -> Result<Token, LexError> {
        error(LexErrorKind::Unsupported(reason), lo, hi)
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
    fn a_shebang_after_the_start_is_punctuation() {
        let shebang = [
            punct(Punctuation::Pound, 1, 2),
            punct(Punctuation::Not, 2, 3),
            punct(Punctuation::Slash, 3, 4),
            ident(4, 7),
            punct(Punctuation::Slash, 7, 8),
            ident(8, 11),
            punct(Punctuation::Slash, 11, 12),
            ident(12, 15),
            ident(16, 19),
        ];
        assert_eq!(
            lex("\n#!/usr/bin/env run\nfn main() {}\n"),
            with_end([shebang.to_vec(), fn_main_at(20)].concat(), end(33))
        );
        assert_eq!(
            lex(" #!x"),
            vec![
                punct(Punctuation::Pound, 1, 2),
                punct(Punctuation::Not, 2, 3),
                ident(3, 4),
                end(4),
            ]
        );
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
            let results = lex(text);
            assert_eq!(
                results[..2],
                [
                    punct(Punctuation::Pound, 0, 1),
                    punct(Punctuation::Not, 1, 2)
                ],
                "{text:?}"
            );
            assert_eq!(results.last(), Some(&end(text.len())), "{text:?}");
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
        let mut characters = vec!['"', '\''];
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
        assert_eq!(
            lex("x/y"),
            vec![
                ident(0, 1),
                punct(Punctuation::Slash, 1, 2),
                ident(2, 3),
                end(3)
            ]
        );
        assert_eq!(
            lex("a.b"),
            vec![
                ident(0, 1),
                punct(Punctuation::Dot, 1, 2),
                ident(2, 3),
                end(3)
            ]
        );
        assert_eq!(lex("x\u{85}y"), vec![ident(0, 1), ident(3, 4), end(4)]);
    }

    #[test]
    fn every_punctuation_token_alone_has_its_kind_and_span() {
        for (index, (text, punctuation)) in PUNCTUATION.iter().enumerate() {
            assert_eq!(text.len() > 1, index < 25, "{text:?}");
            assert!(
                PUNCTUATION[index + 1..]
                    .iter()
                    .all(|(other, kind)| other != text && kind != punctuation),
                "{text:?} is listed twice"
            );
            for edition in EDITIONS {
                assert_eq!(
                    lex_in(text, edition),
                    vec![punct(*punctuation, 0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn adjacent_punctuation_is_munched_from_the_left() {
        for (text, first, split, second) in [
            ("&&&", Punctuation::AndAnd, 2, Punctuation::And),
            ("<<<", Punctuation::Shl, 2, Punctuation::Lt),
            ("...=", Punctuation::DotDotDot, 3, Punctuation::Eq),
            ("->>", Punctuation::RArrow, 2, Punctuation::Gt),
            ("=>=", Punctuation::FatArrow, 2, Punctuation::Eq),
            ("!==", Punctuation::Ne, 2, Punctuation::Eq),
            (":::", Punctuation::PathSep, 2, Punctuation::Colon),
            ("||=", Punctuation::OrOr, 2, Punctuation::Eq),
            ("<<-", Punctuation::Shl, 2, Punctuation::Minus),
            ("&&=", Punctuation::AndAnd, 2, Punctuation::Eq),
            ("<<==", Punctuation::ShlEq, 3, Punctuation::Eq),
            ("....", Punctuation::DotDotDot, 3, Punctuation::Dot),
            ("*/", Punctuation::Star, 1, Punctuation::Slash),
        ] {
            assert_eq!(
                lex(text),
                vec![
                    punct(first, 0, split),
                    punct(second, split, text.len()),
                    end(text.len())
                ],
                "{text:?}"
            );
        }
    }

    #[test]
    fn whitespace_and_comments_separate_punctuation() {
        for (text, first, second, second_lo) in [
            ("< <", Punctuation::Lt, Punctuation::Lt, 2),
            (": :", Punctuation::Colon, Punctuation::Colon, 2),
            ("- >", Punctuation::Minus, Punctuation::Gt, 2),
            ("</**/<", Punctuation::Lt, Punctuation::Lt, 5),
            ("+//\n=", Punctuation::Plus, Punctuation::Eq, 4),
        ] {
            assert_eq!(
                lex(text),
                vec![
                    punct(first, 0, 1),
                    punct(second, second_lo, second_lo + 1),
                    end(text.len())
                ],
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_identifier_followed_by_a_pound_is_a_reserved_prefix_from_2021() {
        for text in ["a#b", "fn#b", "_#b", "rb#b", "bc#b"] {
            let prefix = text.len() - 2;
            for edition in EDITIONS {
                let expected = if edition >= Edition::E2021 {
                    vec![invalid_at(Invalid::ReservedPrefix, 0, prefix + 1)]
                } else {
                    let kind = Keyword::from_text(&text.as_bytes()[..prefix], edition)
                        .map_or(TokenKind::Identifier, TokenKind::Keyword);
                    vec![
                        token(kind, 0, prefix),
                        punct(Punctuation::Pound, prefix, prefix + 1),
                        ident(prefix + 1, prefix + 2),
                        end(prefix + 2),
                    ]
                };
                assert_eq!(lex_in(text, edition), expected, "{text:?} in {edition:?}");
            }
        }
    }

    #[test]
    fn raw_prefixes_are_unsupported() {
        for edition in EDITIONS {
            assert_eq!(
                lex_in("r#x", edition),
                vec![unsupported_at(Unsupported::RawPrefix, 0, 2)],
                "{edition:?}"
            );
            assert_eq!(
                lex_in("br#x", edition),
                vec![unsupported_at(Unsupported::RawPrefix, 0, 3)],
                "{edition:?}"
            );
            let cr = if edition >= Edition::E2021 {
                vec![unsupported_at(Unsupported::RawPrefix, 0, 3)]
            } else {
                vec![
                    ident(0, 2),
                    punct(Punctuation::Pound, 2, 3),
                    ident(3, 4),
                    end(4),
                ]
            };
            assert_eq!(lex_in("cr#x", edition), cr, "{edition:?}");
        }
    }

    #[test]
    fn pound_runs_and_guarded_strings_are_reserved_from_2024() {
        for edition in [Edition::E2015, Edition::E2021] {
            assert_eq!(
                lex_in("##", edition),
                vec![
                    punct(Punctuation::Pound, 0, 1),
                    punct(Punctuation::Pound, 1, 2),
                    end(2)
                ],
                "{edition:?}"
            );
            assert_eq!(
                lex_in("#\"x\"#", edition),
                vec![punct(Punctuation::Pound, 0, 1), unsupported('"', 1)],
                "{edition:?}"
            );
        }
        assert_eq!(lex("##"), vec![invalid_at(Invalid::ReservedPounds, 0, 2)]);
        assert_eq!(
            lex("x ###;"),
            vec![ident(0, 1), invalid_at(Invalid::ReservedPounds, 2, 5)]
        );
        assert_eq!(
            lex("#\"x\"#"),
            vec![invalid_at(Invalid::ReservedGuardedString, 0, 1)]
        );
        assert_eq!(
            lex("##\"x\"##"),
            vec![invalid_at(Invalid::ReservedGuardedString, 0, 2)]
        );
        assert_eq!(
            lex("# #"),
            vec![
                punct(Punctuation::Pound, 0, 1),
                punct(Punctuation::Pound, 2, 3),
                end(3)
            ]
        );
        assert_eq!(
            lex("#[x]"),
            vec![
                punct(Punctuation::Pound, 0, 1),
                open(Delimiter::Bracket, 1),
                ident(2, 3),
                close(Delimiter::Bracket, 3),
                end(4),
            ]
        );
    }

    #[test]
    fn every_integer_literal_form_is_one_token() {
        for text in [
            "0",
            "0123",
            "1_000",
            "1_",
            "98_222",
            "0xff",
            "0o77",
            "0b1111_0000",
            "0x_1",
            "0b________1",
            "0x01_e3",
            "0xABCdef",
            "1u8",
            "123_u32",
            "0b1u8",
            "1suffix",
            "0b1f32",
            "5f32",
            "0x1u8",
            "0B1",
            "0X1",
            "1u8_x",
            "340282366920938463463374607431768211456",
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![int(0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn an_integer_literal_ends_where_no_digit_or_suffix_continues_it() {
        assert_eq!(lex("1 u8"), vec![int(0, 1), ident(2, 4), end(4)]);
        assert_eq!(
            lex("0x1e+3"),
            vec![int(0, 4), punct(Punctuation::Plus, 4, 5), int(5, 6), end(6)]
        );
        assert_eq!(
            lex("1-1"),
            vec![
                int(0, 1),
                punct(Punctuation::Minus, 1, 2),
                int(2, 3),
                end(3)
            ]
        );
        assert_eq!(
            lex("1#"),
            vec![int(0, 1), punct(Punctuation::Pound, 1, 2), end(2)]
        );
        assert_eq!(lex("1\u{2028}2"), vec![int(0, 1), int(4, 5), end(5)]);
        for edition in EDITIONS {
            assert_eq!(
                lex_in("1u8#b", edition),
                vec![
                    int(0, 3),
                    punct(Punctuation::Pound, 3, 4),
                    ident(4, 5),
                    end(5)
                ],
                "{edition:?}"
            );
        }
    }

    #[test]
    fn a_period_after_an_integer_literal_that_starts_a_token_is_punctuation() {
        for (text, dot, after) in [
            ("1.foo", 1, ident(2, 5)),
            ("1._x", 1, ident(2, 4)),
            ("1.e3", 1, ident(2, 4)),
            ("0b1.foo", 3, ident(4, 7)),
            ("0x1._", 3, keyword(Keyword::Underscore, 4, 5)),
            ("1u8.0", 3, int(4, 5)),
            ("0x1u8.0", 5, int(6, 7)),
        ] {
            assert_eq!(
                lex(text),
                vec![
                    int(0, dot),
                    punct(Punctuation::Dot, dot, dot + 1),
                    after,
                    end(text.len())
                ],
                "{text:?}"
            );
        }
        assert_eq!(
            lex("1..2"),
            vec![
                int(0, 1),
                punct(Punctuation::DotDot, 1, 3),
                int(3, 4),
                end(4)
            ]
        );
        assert_eq!(
            lex("0b1..=2"),
            vec![
                int(0, 3),
                punct(Punctuation::DotDotEq, 3, 6),
                int(6, 7),
                end(7)
            ]
        );
    }

    #[test]
    fn every_invalid_integer_form_is_invalid_from_the_literal_start() {
        for (text, reason, hi) in [
            ("0b0102", Invalid::DigitOutOfRadix, 6),
            ("0o1279", Invalid::DigitOutOfRadix, 6),
            ("0b2", Invalid::DigitOutOfRadix, 3),
            ("0b_2", Invalid::DigitOutOfRadix, 4),
            ("0o8", Invalid::DigitOutOfRadix, 3),
            ("0x80.0", Invalid::RadixPeriod, 5),
            ("0b1.", Invalid::RadixPeriod, 4),
            ("0o7. ", Invalid::RadixPeriod, 4),
            ("0x1.\u{2028}", Invalid::RadixPeriod, 4),
            ("0b101e", Invalid::RadixExponent, 6),
            ("0o7e", Invalid::RadixExponent, 4),
            ("0b1_E3", Invalid::RadixExponent, 5),
            ("0b", Invalid::NoRadixDigits, 2),
            ("0b_", Invalid::NoRadixDigits, 3),
            ("0xG", Invalid::NoRadixDigits, 2),
            ("0o", Invalid::NoRadixDigits, 2),
            ("0bé", Invalid::NoRadixDigits, 2),
            ("2em", Invalid::EmptyExponent, 2),
            ("2e", Invalid::EmptyExponent, 2),
            ("2E+", Invalid::EmptyExponent, 3),
            ("2e-_x", Invalid::EmptyExponent, 4),
            ("2eé", Invalid::EmptyExponent, 2),
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(&format!("x {text}"), edition),
                    vec![ident(0, 1), invalid_at(reason, 2, 2 + hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn a_float_literal_is_unsupported_from_the_literal_start() {
        for (text, hi) in [
            ("1.", 2),
            ("1.0", 2),
            ("1. ", 2),
            ("1.\u{2028}", 2),
            ("1.;", 2),
            ("12_.5", 4),
            ("1e3", 2),
            ("1E+3", 2),
            ("1e_3", 2),
            ("1.0e-3", 2),
            ("1_e3", 3),
        ] {
            assert_eq!(
                lex(text),
                vec![unsupported_at(Unsupported::FloatLiteral, 0, hi)],
                "{text:?}"
            );
        }
    }

    #[test]
    fn no_token_is_returned_before_a_character_that_could_extend_it() {
        for (text, c, lo) in [
            ("café", 'é', 3),
            ("_é", 'é', 1),
            ("fn中", '中', 2),
            ("1é", 'é', 1),
            ("1_é", 'é', 2),
            ("1ué", 'é', 2),
            ("1.é", 'é', 2),
            ("0x1.é", 'é', 4),
            ("0b1é", 'é', 3),
            ("b\"x\"", '"', 1),
            ("b'x'", '\'', 1),
            ("a'b", '\'', 1),
            ("let\"", '"', 3),
        ] {
            assert_eq!(lex(text), vec![unsupported(c, lo)], "{text:?}");
        }
    }

    #[test]
    fn punctuation_and_delimiters_are_returned_before_an_unsupported_character() {
        assert_eq!(
            lex("+é"),
            vec![punct(Punctuation::Plus, 0, 1), unsupported('é', 1)]
        );
        assert_eq!(
            lex("(\""),
            vec![open(Delimiter::Parenthesis, 0), unsupported('"', 1)]
        );
        assert_eq!(lex("1\"x\""), vec![int(0, 1), unsupported('"', 1)]);
    }

    #[test]
    fn lexes_an_arithmetic_program() {
        let text = "fn main() { let x: u32 = 1 + 2 * 3; }";
        assert_eq!(
            lex(text),
            vec![
                keyword(Keyword::Fn, 0, 2),
                ident(3, 7),
                open(Delimiter::Parenthesis, 7),
                close(Delimiter::Parenthesis, 8),
                open(Delimiter::Brace, 10),
                keyword(Keyword::Let, 12, 15),
                ident(16, 17),
                punct(Punctuation::Colon, 17, 18),
                ident(19, 22),
                punct(Punctuation::Eq, 23, 24),
                int(25, 26),
                punct(Punctuation::Plus, 27, 28),
                int(29, 30),
                punct(Punctuation::Star, 31, 32),
                int(33, 34),
                punct(Punctuation::Semi, 34, 35),
                close(Delimiter::Brace, 36),
                end(37),
            ]
        );
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
            ("x '", unsupported('\'', 2)),
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

    /// Lexes every text from an alphabet of comment, shebang, delimiter,
    /// identifier, digit, literal, punctuation, quote, whitespace, invalid,
    /// and multibyte characters: up to five characters in edition 2024, up to
    /// four in the earlier editions. Checks that each result lies in the text
    /// after the previous one and that each token other than the end of file
    /// is not empty.
    #[test]
    fn every_short_text_lexes_to_spans_inside_the_text() {
        const ALPHABET: [char; 21] = [
            '/', '*', '!', '#', '[', '\n', 'a', ' ', 'é', '\u{FEFF}', '`', '0', '1', '.', 'e', 'x',
            '_', '<', '-', '=', '"',
        ];
        let mut sources = SourceTable::default();
        let id = sources.add_text("main.rs", "");
        let mut text = String::new();
        let mut count = 0;
        for (edition, max_len) in [
            (Edition::E2015, 4),
            (Edition::E2018, 4),
            (Edition::E2021, 4),
            (Edition::E2024, 5),
        ] {
            for len in 0..=max_len {
                for index in 0..ALPHABET.len().pow(len) {
                    text.clear();
                    let mut rest = index;
                    for _ in 0..len {
                        text.push(ALPHABET[rest % ALPHABET.len()]);
                        rest /= ALPHABET.len();
                    }
                    check_spans(Lexer::new(id, &text, edition), &text);
                    count += 1;
                }
            }
        }
        assert_eq!(count, 3 * 204_205 + 4_288_306);
    }

    /// Runs `lexer` over `text` to its end and checks every span.
    fn check_spans(mut lexer: Lexer<'_>, text: &str) {
        let edition = lexer.edition;
        let mut previous_hi = 0;
        loop {
            let result = lexer.next_token();
            let span = match result {
                Ok(token) => token.span,
                Err(error) => error.span(),
            };
            let lo = usize::try_from(span.lo()).unwrap();
            let hi = usize::try_from(span.hi()).unwrap();
            assert!(
                previous_hi <= lo && lo <= hi && hi <= text.len(),
                "{text:?} in {edition:?}: {result:?}"
            );
            assert!(
                text.is_char_boundary(lo) && text.is_char_boundary(hi),
                "{text:?} in {edition:?}: {result:?}"
            );
            assert!(
                lo < hi || !matches!(result, Ok(token) if token.kind != TokenKind::EndOfFile),
                "{text:?} in {edition:?}: {result:?}"
            );
            if !matches!(result, Ok(token) if token.kind != TokenKind::EndOfFile) {
                return;
            }
            previous_hi = hi;
        }
    }
}
