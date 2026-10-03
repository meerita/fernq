//! The lexer: the text of one loaded source file to lexical tokens.
//!
//! A [`Lexer`] reads one stored source text with the edition of that source
//! and returns one [`Token`] per call, until end of file or the first lexical
//! error. It stores no tokens; retention belongs to the caller.
//!
//! The supported lexical surface is identifiers and raw identifiers, strict
//! and reserved keywords, lifetimes and raw lifetimes, integer and float
//! literals, character and byte literals, the six string literal classes
//! that open with `"`, punctuation, and the three delimiter pairs. An
//! identifier, a lifetime or raw name, and a literal suffix start with
//! `XID_Start` or `_` and continue with `XID_Continue`, in the Unicode version
//! of module `unicode`; the lexer does not normalize them. The lexer skips a byte order mark at offset 0, a shebang at the start of
//! the text, whitespace, and non-doc comments. Any other input ends lexing
//! with a [`LexError`]: invalid when the text is not valid Rust, unsupported
//! otherwise. The lexer does not pair delimiters. It checks the lexical form
//! of each literal, escapes included, and computes no value.
//!
//! The lexer returns no token whose extent an unsupported character directly
//! after it could still change. Every span is in the coordinates of the text
//! as stored.

use crate::edition::Edition;
use crate::source::{ByteOffset, SourceId, Span};
use crate::unicode;

/// The largest number of `#` that delimit a raw literal.
const MAX_RAW_POUNDS: usize = 255;

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
    /// A floating-point literal, with its suffix if any. The suffix is not
    /// checked.
    FloatLiteral,
    /// `r#` and an identifier or keyword. It is never a keyword.
    RawIdentifier,
    /// `'` and an identifier or keyword: `'a`, `'static`, `'_`, `'fn`.
    Lifetime,
    /// From edition 2021, `'r#` and an identifier or keyword.
    RawLifetime,
    /// A character literal, `'a'` or an escape between quotes, with its
    /// suffix if any. The suffix is not checked.
    CharLiteral,
    /// A byte literal, `b'a'` or a byte escape between quotes, with its suffix
    /// if any. The suffix is not checked.
    ByteLiteral,
    /// A string literal, `"..."`, with its suffix if any. The suffix of each
    /// string literal class is not checked.
    StringLiteral,
    /// `r"..."` or `r#"..."#` with up to 255 `#`.
    RawStringLiteral,
    /// `b"..."`.
    ByteStringLiteral,
    /// `br"..."` or `br#"..."#` with up to 255 `#`.
    RawByteStringLiteral,
    /// From edition 2021, `c"..."`.
    CStringLiteral,
    /// From edition 2021, `cr"..."` or `cr#"..."#` with up to 255 `#`.
    RawCStringLiteral,
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

/// A reason the text is not valid Rust. The span of a numeric literal reason
/// starts at the literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invalid {
    /// A character that starts no Rust token. The span is the character.
    Character(char),
    /// A block comment without its closing `*/`. The span runs from the
    /// outermost `/*` to the end of the text.
    UnterminatedBlockComment,
    /// From edition 2021, an identifier or keyword directly followed by `#`,
    /// `"`, or `'` that does not start a raw identifier or a literal, or a
    /// lifetime other than `'r` directly followed by `#`. The span is the
    /// identifier or lifetime and that character.
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
    /// A decimal literal, with or without a fractional part, followed by `e`
    /// or `E` and no exponent digit after an optional sign and underscores.
    /// The span ends after the sign and underscores.
    EmptyExponent,
    /// A raw identifier whose name is `_`, `crate`, `self`, `Self`, or
    /// `super`. The span is the `r#` and the name.
    ReservedRawIdentifier,
    /// From edition 2021, a raw lifetime whose name is `_`, `crate`, `self`,
    /// `Self`, or `super`. The span is the `'r#` and the name.
    ReservedRawLifetime,
    /// A raw prefix `r`, `br`, or `cr` followed by more than 255 `#`. The
    /// span runs from the prefix to the end of the `#` run.
    TooManyRawPounds,
    /// A raw prefix and its `#` run followed by neither `"` nor, after `r#`,
    /// an identifier, or from edition 2021 `'r#` not followed by an
    /// identifier. The span runs from the prefix to the end of the next
    /// character, or to the end of the text.
    MalformedRawPrefix,
    /// A literal without its closing quote, or without the `#` run that
    /// closes a raw literal. The span runs from the literal start to the end
    /// of the text.
    UnterminatedLiteral,
    /// A CR inside a literal that is not followed by LF. The span is the CR.
    BareCarriageReturn,
    /// A non-ASCII character in a byte, byte string, or raw byte string
    /// literal. The span is the character.
    NonAsciiInByteLiteral,
    /// A NUL character, or an escape whose value is 0, in a C string or raw
    /// C string literal. The span is the character or the escape.
    NulInCString,
    /// An escape that the literal class does not admit. The span runs from
    /// the `\` to the end of the characters read.
    Escape(InvalidEscape),
    /// A literal suffix that is `_` alone. The span is the `_`.
    UnderscoreSuffix,
    /// A character or byte literal with no character between its quotes.
    /// The span runs from the literal start to the second `'`.
    EmptyCharLiteral,
    /// `'`, LF, CR, or TAB unescaped in a character or byte literal. The span
    /// runs from the literal start to the end of that character.
    UnescapedCharacter,
    /// A character or byte literal whose character or escape is followed by
    /// a character other than `'`. The span runs from the literal start to
    /// the end of that character.
    UnclosedCharLiteral,
    /// `'`, a name of two or more characters or a raw name, and `'`: a
    /// character literal with more than one character. The span runs from
    /// the first `'` to the end of the second.
    CharLiteralTooLong,
}

/// Why an escape is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InvalidEscape {
    /// `\` followed by a character that starts no escape.
    Unknown,
    /// `\x` followed by fewer than two hexadecimal digits.
    ShortHex,
    /// `\x` above `\x7F` where only ASCII escapes are admitted.
    HexOutOfRange,
    /// `\u` not followed by `{`.
    UnicodeNoBrace,
    /// `\u{}`.
    UnicodeEmpty,
    /// `\u{` followed by `_`.
    UnicodeUnderscoreStart,
    /// `\u{` with more than six hexadecimal digits.
    UnicodeOverlong,
    /// `\u{` and its digits and underscores not followed by `}`.
    UnicodeUnclosed,
    /// `\u{...}` whose value is not a Unicode scalar value.
    UnicodeNotScalar,
    /// `\u` in a byte or byte string literal.
    UnicodeInByteLiteral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// ZWJ (U+200D) or ZWNJ (U+200C) after the first character of an
    /// identifier, raw identifier, lifetime, raw lifetime, or suffix. The
    /// Reference forbids them there and `rustc` accepts them, so Fernq does
    /// not classify them. The span is the character.
    Character(char),
    /// A doc comment. The span is its opening `///`, `//!`, `/**`, or `/*!`.
    DocComment,
}

/// The characters and escapes that the prefix of a quoted literal admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Content {
    /// No prefix or `r`: any character; quote, ASCII, and Unicode escapes.
    Unicode,
    /// `b` or `br`: ASCII characters; byte escapes.
    Byte,
    /// `c` or `cr`: any character but NUL; byte and Unicode escapes, none
    /// with the value 0.
    C,
}

impl Content {
    /// The kind of the string literal of this content, raw or not.
    fn string_kind(self, raw: bool) -> TokenKind {
        match (self, raw) {
            (Self::Unicode, false) => TokenKind::StringLiteral,
            (Self::Unicode, true) => TokenKind::RawStringLiteral,
            (Self::Byte, false) => TokenKind::ByteStringLiteral,
            (Self::Byte, true) => TokenKind::RawByteStringLiteral,
            (Self::C, false) => TokenKind::CStringLiteral,
            (Self::C, true) => TokenKind::RawCStringLiteral,
        }
    }
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
            'A'..='Z' | 'a'..='z' | '_' => self.identifier_or_literal(start)?,
            '"' => self.string_literal(start, start, Content::Unicode)?,
            '\'' => self.quote(start)?,
            '0'..='9' => self.number_literal(start)?,
            _ if !c.is_ascii() && unicode::is_xid_start(c) => self.identifier_or_literal(start)?,
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

    /// Returns the kind and end of the identifier, keyword, raw identifier,
    /// or prefixed literal at `start`.
    fn identifier_or_literal(&self, start: usize) -> Result<(TokenKind, usize), LexError> {
        let end = self.identifier_end(start)?;
        let identifier = self.rest(start).get(..end - start).unwrap_or_default();
        let since_2021 = self.edition >= Edition::E2021;
        match self.rest(end).first() {
            Some(b'#') => match identifier {
                b"r" => return self.raw_input(start, end, Content::Unicode),
                b"br" => return self.raw_input(start, end, Content::Byte),
                b"cr" if since_2021 => return self.raw_input(start, end, Content::C),
                _ if since_2021 => {
                    let kind = LexErrorKind::Invalid(Invalid::ReservedPrefix);
                    return Err(self.error(kind, start, end + 1));
                }
                _ => {}
            },
            Some(b'"') => match identifier {
                b"b" => return self.string_literal(start, end, Content::Byte),
                b"r" => return self.raw_input(start, end, Content::Unicode),
                b"br" => return self.raw_input(start, end, Content::Byte),
                b"c" if since_2021 => return self.string_literal(start, end, Content::C),
                b"cr" if since_2021 => return self.raw_input(start, end, Content::C),
                _ if since_2021 => {
                    let kind = LexErrorKind::Invalid(Invalid::ReservedPrefix);
                    return Err(self.error(kind, start, end + 1));
                }
                _ => {}
            },
            Some(b'\'') => match identifier {
                b"b" => return self.char_literal(start, end, Content::Byte),
                _ if since_2021 => {
                    let kind = LexErrorKind::Invalid(Invalid::ReservedPrefix);
                    return Err(self.error(kind, start, end + 1));
                }
                _ => {}
            },
            _ => {}
        }
        let kind = Keyword::from_text(identifier, self.edition)
            .map_or(TokenKind::Identifier, TokenKind::Keyword);
        Ok((kind, end))
    }

    /// Returns the kind and end of the raw identifier or raw literal at
    /// `start`, whose prefix ends at `pounds`, the first `#` or the `"`.
    ///
    /// `content` is [`Content::Unicode`] for the prefix `r`, the only prefix
    /// that can start a raw identifier.
    fn raw_input(
        &self,
        start: usize,
        pounds: usize,
        content: Content,
    ) -> Result<(TokenKind, usize), LexError> {
        let pound_count = self.count_while(pounds, |byte| byte == b'#');
        let after = pounds + pound_count;
        if pound_count > MAX_RAW_POUNDS {
            let kind = LexErrorKind::Invalid(Invalid::TooManyRawPounds);
            return Err(self.error(kind, start, after));
        }
        let may_be_identifier = content == Content::Unicode && pound_count == 1;
        match self.char_at(after) {
            Some('"') => self.raw_string(start, after, pound_count, content),
            Some(c) if may_be_identifier && is_identifier_start(c) => {
                self.raw_identifier(start, after)
            }
            next => {
                let kind = LexErrorKind::Invalid(Invalid::MalformedRawPrefix);
                let hi = after + next.map_or(0, char::len_utf8);
                Err(self.error(kind, start, hi))
            }
        }
    }

    /// Returns the kind and end of the raw identifier at `start` whose name
    /// starts at `name`.
    fn raw_identifier(&self, start: usize, name: usize) -> Result<(TokenKind, usize), LexError> {
        let end = self.identifier_end(name)?;
        if is_reserved_raw_name(self.rest(name).get(..end - name).unwrap_or_default()) {
            let kind = LexErrorKind::Invalid(Invalid::ReservedRawIdentifier);
            return Err(self.error(kind, start, end));
        }
        // No reserved prefix rule applies after a raw identifier.
        Ok((TokenKind::RawIdentifier, end))
    }

    /// Returns the kind and end of the character literal, lifetime, or raw
    /// lifetime at `start`, a `'`.
    ///
    /// The text after the `'` decides the class: a character then `'` is a
    /// character literal, and `XID_Start` or `_` not followed by `'` starts a
    /// lifetime.
    fn quote(&self, start: usize) -> Result<(TokenKind, usize), LexError> {
        let first = start + 1;
        let Some(c) = self.char_at(first) else {
            return Err(self.unterminated_literal(start));
        };
        let closes = self.rest(first + c.len_utf8()).first() == Some(&b'\'');
        if !closes && is_identifier_start(c) {
            return self.lifetime(start);
        }
        self.char_literal(start, start, Content::Unicode)
    }

    /// Returns the kind and end of the lifetime at `start`, a `'` followed by
    /// `XID_Start` or `_`, or of the raw lifetime that starts there.
    fn lifetime(&self, start: usize) -> Result<(TokenKind, usize), LexError> {
        let name = start + 1;
        let end = self.identifier_end(name)?;
        let reason = match self.rest(end).first() {
            Some(b'\'') => Invalid::CharLiteralTooLong,
            Some(b'#') if self.edition >= Edition::E2021 => {
                if self.rest(name).get(..end - name) == Some(b"r") {
                    return self.raw_lifetime(start, end + 1);
                }
                Invalid::ReservedPrefix
            }
            _ => return Ok((TokenKind::Lifetime, end)),
        };
        Err(self.error(LexErrorKind::Invalid(reason), start, end + 1))
    }

    /// Returns the kind and end of the raw lifetime at `start` whose name
    /// starts at `name`, after `'r#`.
    ///
    /// No reserved prefix rule applies after a raw lifetime.
    fn raw_lifetime(&self, start: usize, name: usize) -> Result<(TokenKind, usize), LexError> {
        let next = self.char_at(name);
        if !next.is_some_and(is_identifier_start) {
            let kind = LexErrorKind::Invalid(Invalid::MalformedRawPrefix);
            return Err(self.error(kind, start, name + next.map_or(0, char::len_utf8)));
        }
        let end = self.identifier_end(name)?;
        let (reason, hi) = if self.rest(end).first() == Some(&b'\'') {
            (Invalid::CharLiteralTooLong, end + 1)
        } else if is_reserved_raw_name(self.rest(name).get(..end - name).unwrap_or_default()) {
            (Invalid::ReservedRawLifetime, end)
        } else {
            return Ok((TokenKind::RawLifetime, end));
        };
        Err(self.error(LexErrorKind::Invalid(reason), start, hi))
    }

    /// Returns the kind and end of the character or byte literal at `start`
    /// whose opening `'` is at `quote`, suffix included.
    ///
    /// `content` is [`Content::Unicode`] for a character literal and
    /// [`Content::Byte`] for a byte literal.
    fn char_literal(
        &self,
        start: usize,
        quote: usize,
        content: Content,
    ) -> Result<(TokenKind, usize), LexError> {
        let body = quote + 1;
        let Some(c) = self.char_at(body) else {
            return Err(self.unterminated_literal(start));
        };
        let close = match c {
            '\\' => self.escape_end(body, content)?,
            '\'' | '\t' | '\n' | '\r' => {
                let is_empty = c == '\'' && self.rest(body + 1).first() != Some(&b'\'');
                let reason = if is_empty {
                    Invalid::EmptyCharLiteral
                } else {
                    Invalid::UnescapedCharacter
                };
                return Err(self.error(LexErrorKind::Invalid(reason), start, body + 1));
            }
            _ => self.content_char_end(body, c, content)?,
        };
        match self.char_at(close) {
            Some('\'') => {
                let kind = if content == Content::Byte {
                    TokenKind::ByteLiteral
                } else {
                    TokenKind::CharLiteral
                };
                self.suffixed(kind, close + 1)
            }
            Some(next) => {
                let kind = LexErrorKind::Invalid(Invalid::UnclosedCharLiteral);
                Err(self.error(kind, start, close + next.len_utf8()))
            }
            None => Err(self.unterminated_literal(start)),
        }
    }

    /// Returns the kind and end of the string, byte string, or C string
    /// literal at `start` whose opening `"` is at `quote`, suffix included.
    fn string_literal(
        &self,
        start: usize,
        quote: usize,
        content: Content,
    ) -> Result<(TokenKind, usize), LexError> {
        let mut pos = quote + 1;
        loop {
            let Some(c) = self.char_at(pos) else {
                return Err(self.unterminated_literal(start));
            };
            pos = match c {
                '"' => return self.suffixed(content.string_kind(false), pos + 1),
                '\\' => match self.rest(pos + 1) {
                    // `STRING_CONTINUE`. The whitespace after it is ordinary content.
                    [b'\n', ..] => pos + 2,
                    [b'\r', ..] => self.content_char_end(pos + 1, '\r', content)?,
                    _ => self.escape_end(pos, content)?,
                },
                _ => self.content_char_end(pos, c, content)?,
            };
        }
    }

    /// Returns the kind and end of the raw literal at `start` whose opening
    /// `"` is at `quote` after `pound_count` `#`, suffix included.
    fn raw_string(
        &self,
        start: usize,
        quote: usize,
        pound_count: usize,
        content: Content,
    ) -> Result<(TokenKind, usize), LexError> {
        let mut pos = quote + 1;
        loop {
            let Some(c) = self.char_at(pos) else {
                return Err(self.unterminated_literal(start));
            };
            if c == '"' {
                let closing = self
                    .rest(pos + 1)
                    .iter()
                    .take(pound_count)
                    .take_while(|&&byte| byte == b'#')
                    .count();
                if closing == pound_count {
                    return self.suffixed(content.string_kind(true), pos + 1 + closing);
                }
                pos += 1 + closing;
            } else {
                pos = self.content_char_end(pos, c, content)?;
            }
        }
    }

    /// Returns the offset after `c`, a literal content character at `pos`
    /// other than a quote or `\`, or the error when `content` does not admit
    /// it.
    ///
    /// CR LF counts as LF, `[input.crlf]`. Any other CR is rejected.
    fn content_char_end(&self, pos: usize, c: char, content: Content) -> Result<usize, LexError> {
        let reason = match c {
            '\r' if self.rest(pos + 1).first() == Some(&b'\n') => return Ok(pos + 2),
            '\r' => Invalid::BareCarriageReturn,
            '\0' if content == Content::C => Invalid::NulInCString,
            _ if content == Content::Byte && !c.is_ascii() => Invalid::NonAsciiInByteLiteral,
            _ => return Ok(pos + c.len_utf8()),
        };
        Err(self.error(LexErrorKind::Invalid(reason), pos, pos + c.len_utf8()))
    }

    /// Returns the offset after the escape whose `\` is at `backslash`, or the
    /// error when `content` does not admit it.
    ///
    /// Every content admits the quote escapes and `\n`, `\r`, `\t`, `\\`, and
    /// `\0`, except `\0` in [`Content::C`].
    ///
    /// When the text ends after the `\`, returns the end of the text and the
    /// caller reports the unterminated literal.
    fn escape_end(&self, backslash: usize, content: Content) -> Result<usize, LexError> {
        let pos = backslash + 1;
        let Some(c) = self.char_at(pos) else {
            return Ok(pos);
        };
        match c {
            'n' | 'r' | 't' | '\\' | '\'' | '"' => Ok(pos + 1),
            '0' if content == Content::C => {
                let kind = LexErrorKind::Invalid(Invalid::NulInCString);
                Err(self.error(kind, backslash, pos + 1))
            }
            '0' => Ok(pos + 1),
            'x' => self.hex_escape_end(backslash, content),
            'u' => self.unicode_escape_end(backslash, content),
            _ => Err(self.escape_error(InvalidEscape::Unknown, backslash, pos + c.len_utf8())),
        }
    }

    /// Returns the offset after the `\x` escape at `backslash`: two
    /// hexadecimal digits, the first octal for [`Content::Unicode`], not both
    /// zero for [`Content::C`].
    fn hex_escape_end(&self, backslash: usize, content: Content) -> Result<usize, LexError> {
        let digits = backslash + 2;
        let (high, low) = match self.rest(digits) {
            [high, low, ..] if high.is_ascii_hexdigit() && low.is_ascii_hexdigit() => (*high, *low),
            [high, ..] if high.is_ascii_hexdigit() => {
                return Err(self.escape_error(InvalidEscape::ShortHex, backslash, digits + 1));
            }
            _ => return Err(self.escape_error(InvalidEscape::ShortHex, backslash, digits)),
        };
        let end = digits + 2;
        match content {
            Content::Unicode if high > b'7' => {
                Err(self.escape_error(InvalidEscape::HexOutOfRange, backslash, end))
            }
            Content::C if high == b'0' && low == b'0' => {
                let kind = LexErrorKind::Invalid(Invalid::NulInCString);
                Err(self.error(kind, backslash, end))
            }
            _ => Ok(end),
        }
    }

    /// Returns the offset after the `\u{...}` escape at `backslash`: one to
    /// six hexadecimal digits, each followed by any `_`, whose value is a
    /// Unicode scalar value, not 0 for [`Content::C`].
    fn unicode_escape_end(&self, backslash: usize, content: Content) -> Result<usize, LexError> {
        let brace = backslash + 2;
        if content == Content::Byte {
            return Err(self.escape_error(InvalidEscape::UnicodeInByteLiteral, backslash, brace));
        }
        if self.rest(brace).first() != Some(&b'{') {
            return Err(self.escape_error(InvalidEscape::UnicodeNoBrace, backslash, brace));
        }
        let mut pos = brace + 1;
        if self.rest(pos).first() == Some(&b'_') {
            let reason = InvalidEscape::UnicodeUnderscoreStart;
            return Err(self.escape_error(reason, backslash, pos + 1));
        }
        let mut digit_count = 0;
        let mut value: u32 = 0;
        loop {
            match self.rest(pos).first() {
                Some(b'_') => pos += 1,
                Some(&byte) if byte.is_ascii_hexdigit() => {
                    pos += 1;
                    digit_count += 1;
                    if digit_count > 6 {
                        let reason = InvalidEscape::UnicodeOverlong;
                        return Err(self.escape_error(reason, backslash, pos));
                    }
                    // Six digits at most: the value stays below 2^24.
                    value = value * 16 + char::from(byte).to_digit(16).unwrap_or_default();
                }
                Some(b'}') if digit_count == 0 => {
                    let reason = InvalidEscape::UnicodeEmpty;
                    return Err(self.escape_error(reason, backslash, pos + 1));
                }
                Some(b'}') => break,
                _ => {
                    let reason = InvalidEscape::UnicodeUnclosed;
                    return Err(self.escape_error(reason, backslash, pos));
                }
            }
        }
        let end = pos + 1;
        if char::from_u32(value).is_none() {
            return Err(self.escape_error(InvalidEscape::UnicodeNotScalar, backslash, end));
        }
        if content == Content::C && value == 0 {
            let kind = LexErrorKind::Invalid(Invalid::NulInCString);
            return Err(self.error(kind, backslash, end));
        }
        Ok(end)
    }

    fn escape_error(&self, reason: InvalidEscape, lo: usize, hi: usize) -> LexError {
        self.error(LexErrorKind::Invalid(Invalid::Escape(reason)), lo, hi)
    }

    /// The error for the literal at `start` that the end of the text cuts.
    fn unterminated_literal(&self, start: usize) -> LexError {
        let kind = LexErrorKind::Invalid(Invalid::UnterminatedLiteral);
        self.error(kind, start, self.text.len())
    }

    /// Returns the kind and end of the integer or float literal at `start`,
    /// which is a decimal digit, suffix included.
    ///
    /// The scan follows `INTEGER_LITERAL` and `FLOAT_LITERAL` of the
    /// Reference without backtracking: once a radix prefix, an exponent
    /// letter, or a `.` that continues the literal is read, the text is that
    /// form or an error.
    fn number_literal(&self, start: usize) -> Result<(TokenKind, usize), LexError> {
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
            (10, [b'e' | b'E', ..]) => return self.float_exponent(start, end),
            (_, [b'.', ..]) => match self.char_at(end + 1) {
                // `..`, a field, or a method call: the literal ends before the `.`.
                Some(next) if next == '.' || is_identifier_start(next) => {
                    return Ok((TokenKind::IntegerLiteral, end));
                }
                Some(next) if radix == 10 && next.is_ascii_digit() => {
                    let fraction_end = end + 1 + self.count_while(end + 1, is_decimal_continue);
                    return match self.rest(fraction_end).first() {
                        Some(b'e' | b'E') => self.float_exponent(start, fraction_end),
                        _ => self.suffixed(TokenKind::FloatLiteral, fraction_end),
                    };
                }
                _ => {
                    if radix != 10 {
                        let kind = LexErrorKind::Invalid(Invalid::RadixPeriod);
                        return Err(self.error(kind, start, end + 1));
                    }
                    // A `.` with no fraction digit ends the literal: it takes no suffix.
                    return Ok((TokenKind::FloatLiteral, end + 1));
                }
            },
            _ => {}
        }
        self.suffixed(TokenKind::IntegerLiteral, end)
    }

    /// Returns the kind and end of the float literal at `start` whose digits,
    /// fractional part included, end at `exponent`, an `e` or `E`.
    ///
    /// The exponent letter is a hard cut: without an exponent digit after its
    /// optional sign and underscores, the text is an empty exponent.
    fn float_exponent(
        &self,
        start: usize,
        exponent: usize,
    ) -> Result<(TokenKind, usize), LexError> {
        let mut pos = exponent + 1;
        if matches!(self.rest(pos).first(), Some(b'+' | b'-')) {
            pos += 1;
        }
        pos += self.count_while(pos, |byte| byte == b'_');
        if !self.rest(pos).first().is_some_and(u8::is_ascii_digit) {
            let kind = LexErrorKind::Invalid(Invalid::EmptyExponent);
            return Err(self.error(kind, start, pos));
        }
        let end = pos + self.count_while(pos, is_decimal_continue);
        self.suffixed(TokenKind::FloatLiteral, end)
    }

    /// Returns `kind` and the end of the literal whose body ends at `end`,
    /// after its suffix if any.
    ///
    /// A suffix is `XID_Start`, or `_` and an `XID_Continue` character, then
    /// `XID_Continue` characters. A `_` alone is a hard cut of the Reference
    /// suffix grammar and is invalid. A caller whose suffix cannot start with
    /// `e` or `E` handles that letter before the call.
    fn suffixed(&self, kind: TokenKind, end: usize) -> Result<(TokenKind, usize), LexError> {
        match self.char_at(end) {
            Some('_') if !self.char_at(end + 1).is_some_and(unicode::is_xid_continue) => {
                let kind = LexErrorKind::Invalid(Invalid::UnderscoreSuffix);
                Err(self.error(kind, end, end + 1))
            }
            Some(c) if is_identifier_start(c) => Ok((kind, self.identifier_end(end)?)),
            _ => Ok((kind, end)),
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

    /// Returns the offset after the `XID_Continue` characters at `pos`.
    ///
    /// ZWJ (U+200D) and ZWNJ (U+200C) are `XID_Continue`, and `rustc` accepts
    /// them inside an identifier, but `[ident.zero-width-chars]` forbids them:
    /// either one ends lexing as unsupported, and no token is returned.
    fn identifier_end(&self, mut pos: usize) -> Result<usize, LexError> {
        loop {
            pos += self.count_while(pos, is_identifier_continue);
            match self.char_at(pos) {
                Some(c @ ('\u{200C}' | '\u{200D}')) => {
                    let kind = LexErrorKind::Unsupported(Unsupported::Character(c));
                    return Err(self.error(kind, pos, pos + c.len_utf8()));
                }
                Some(c) if !c.is_ascii() && unicode::is_xid_continue(c) => pos += c.len_utf8(),
                _ => return Ok(pos),
            }
        }
    }

    /// Returns the error for `c` at `pos`, a character that starts no token.
    fn character_error(&self, c: char, pos: usize) -> LexError {
        let kind = LexErrorKind::Invalid(Invalid::Character(c));
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

/// Whether `name` is `_`, `crate`, `self`, `Self`, or `super`, which no raw
/// identifier or raw lifetime can name.
fn is_reserved_raw_name(name: &[u8]) -> bool {
    matches!(name, b"_" | b"crate" | b"self" | b"Self" | b"super")
}

/// Whether `c` starts an identifier, a lifetime or raw name, or a suffix:
/// `XID_Start` or `_`.
fn is_identifier_start(c: char) -> bool {
    c == '_' || unicode::is_xid_start(c)
}

/// Whether `byte` is an ASCII `XID_Continue` character.
fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `byte` continues a `DEC_LITERAL`: a decimal digit or `_`.
fn is_decimal_continue(byte: u8) -> bool {
    byte.is_ascii_digit() || byte == b'_'
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
        lex_source(id, text, edition)
    }

    /// Lexes `text`, the text of `id`, like [`lex_in`].
    fn lex_source(id: SourceId, text: &str, edition: Edition) -> Vec<Result<Token, LexError>> {
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

    fn float(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::FloatLiteral, lo, hi)
    }

    fn string(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::StringLiteral, lo, hi)
    }

    fn raw_string(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::RawStringLiteral, lo, hi)
    }

    fn raw_ident(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::RawIdentifier, lo, hi)
    }

    fn lifetime(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::Lifetime, lo, hi)
    }

    fn character(lo: usize, hi: usize) -> Result<Token, LexError> {
        token(TokenKind::CharLiteral, lo, hi)
    }

    fn invalid_at(reason: Invalid, lo: usize, hi: usize) -> Result<Token, LexError> {
        error(LexErrorKind::Invalid(reason), lo, hi)
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
    fn every_lifetime_form_is_one_token_in_every_edition() {
        for text in [
            "'a", "'static", "'_", "'fn", "'abc_1", "'_x", "'r", "'self", "'b", "'r1",
        ] {
            for edition in EDITIONS {
                assert_eq!(
                    lex_in(text, edition),
                    vec![lifetime(0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
        assert_eq!(lex("'a 'b"), vec![lifetime(0, 2), lifetime(3, 5), end(5)]);
        assert_eq!(
            lex("&'a str"),
            vec![
                punct(Punctuation::And, 0, 1),
                lifetime(1, 3),
                ident(4, 7),
                end(7)
            ]
        );
        assert_eq!(
            lex("'outer: loop"),
            vec![
                lifetime(0, 6),
                punct(Punctuation::Colon, 6, 7),
                keyword(Keyword::Loop, 8, 12),
                end(12)
            ]
        );
        assert_eq!(
            lex("<'a>"),
            vec![
                punct(Punctuation::Lt, 0, 1),
                lifetime(1, 3),
                punct(Punctuation::Gt, 3, 4),
                end(4)
            ]
        );
    }

    #[test]
    fn raw_lifetimes_start_in_2021() {
        for text in ["'r#a", "'r#fn", "'r#async", "'r#_a", "'r#r", "'r#static"] {
            for edition in [Edition::E2021, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![
                        token(TokenKind::RawLifetime, 0, text.len()),
                        end(text.len())
                    ],
                    "{text:?} in {edition:?}"
                );
            }
        }
        for edition in [Edition::E2015, Edition::E2018] {
            assert_eq!(
                lex_in("'r#a", edition),
                vec![
                    lifetime(0, 2),
                    punct(Punctuation::Pound, 2, 3),
                    ident(3, 4),
                    end(4)
                ],
                "{edition:?}"
            );
        }
        assert_eq!(
            lex("'r#a#b"),
            vec![
                token(TokenKind::RawLifetime, 0, 4),
                punct(Punctuation::Pound, 4, 5),
                ident(5, 6),
                end(6)
            ]
        );
    }

    #[test]
    fn an_invalid_raw_lifetime_is_invalid_from_2021() {
        for (text, reason, hi) in [
            ("'r#_", Invalid::ReservedRawLifetime, 4),
            ("'r#crate", Invalid::ReservedRawLifetime, 8),
            ("'r#self", Invalid::ReservedRawLifetime, 7),
            ("'r#Self", Invalid::ReservedRawLifetime, 7),
            ("'r#super", Invalid::ReservedRawLifetime, 8),
            ("'r#1", Invalid::MalformedRawPrefix, 4),
            ("'r#", Invalid::MalformedRawPrefix, 3),
            ("'r# a", Invalid::MalformedRawPrefix, 4),
            ("'r##a", Invalid::MalformedRawPrefix, 4),
            ("'r#\u{2028}", Invalid::MalformedRawPrefix, 6),
            ("'r#a'", Invalid::CharLiteralTooLong, 5),
            ("'r#self'", Invalid::CharLiteralTooLong, 8),
        ] {
            for edition in [Edition::E2021, Edition::E2024] {
                assert_eq!(
                    lex_in(&format!("x {text}"), edition),
                    vec![ident(0, 1), invalid_at(reason, 2, 2 + hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn a_lifetime_followed_by_a_pound_is_a_reserved_prefix_from_2021() {
        for text in ["'a#b", "'fn#b", "'_#b", "'static#b", "'rb#b"] {
            let name = text.len() - 2;
            for edition in EDITIONS {
                let expected = if edition >= Edition::E2021 {
                    vec![invalid_at(Invalid::ReservedPrefix, 0, name + 1)]
                } else {
                    vec![
                        lifetime(0, name),
                        punct(Punctuation::Pound, name, name + 1),
                        ident(name + 1, name + 2),
                        end(name + 2),
                    ]
                };
                assert_eq!(lex_in(text, edition), expected, "{text:?} in {edition:?}");
            }
        }
    }

    #[test]
    fn every_char_and_byte_literal_form_is_one_token() {
        for (text, kind) in [
            ("'a'", TokenKind::CharLiteral),
            ("'\\''", TokenKind::CharLiteral),
            ("'\"'", TokenKind::CharLiteral),
            ("'\\\"'", TokenKind::CharLiteral),
            ("'\\x7F'", TokenKind::CharLiteral),
            ("'\\x00'", TokenKind::CharLiteral),
            ("'\\u{10FFFF}'", TokenKind::CharLiteral),
            ("'\\u{0}'", TokenKind::CharLiteral),
            ("'\\n'", TokenKind::CharLiteral),
            ("'\\\\'", TokenKind::CharLiteral),
            ("'\\0'", TokenKind::CharLiteral),
            ("'é'", TokenKind::CharLiteral),
            ("'中'", TokenKind::CharLiteral),
            ("'😀'", TokenKind::CharLiteral),
            ("'\u{2028}'", TokenKind::CharLiteral),
            ("'('", TokenKind::CharLiteral),
            ("'#'", TokenKind::CharLiteral),
            ("' '", TokenKind::CharLiteral),
            ("'_'", TokenKind::CharLiteral),
            ("'1'", TokenKind::CharLiteral),
            ("'\0'", TokenKind::CharLiteral),
            ("'a'b", TokenKind::CharLiteral),
            ("'a'_x", TokenKind::CharLiteral),
            ("b'a'", TokenKind::ByteLiteral),
            ("b'\\x80'", TokenKind::ByteLiteral),
            ("b'\\xFF'", TokenKind::ByteLiteral),
            ("b'\\''", TokenKind::ByteLiteral),
            ("b'\"'", TokenKind::ByteLiteral),
            ("b'\\\"'", TokenKind::ByteLiteral),
            ("b'\\0'", TokenKind::ByteLiteral),
            ("b' '", TokenKind::ByteLiteral),
            ("b'a'x", TokenKind::ByteLiteral),
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![token(kind, 0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
        assert_eq!(
            lex("'a' 'b'"),
            vec![character(0, 3), character(4, 7), end(7)]
        );
    }

    /// Each span is relative to the start of the literal.
    #[test]
    fn every_invalid_char_or_byte_form_is_invalid_inside_the_literal() {
        let escape = |reason, lo, hi| (Invalid::Escape(reason), lo, hi);
        for (text, (reason, lo, hi)) in [
            ("'ab'", (Invalid::CharLiteralTooLong, 0, 4)),
            ("'ab'c", (Invalid::CharLiteralTooLong, 0, 4)),
            ("''", (Invalid::EmptyCharLiteral, 0, 2)),
            ("'''", (Invalid::UnescapedCharacter, 0, 2)),
            ("'\t'", (Invalid::UnescapedCharacter, 0, 2)),
            ("'\n'", (Invalid::UnescapedCharacter, 0, 2)),
            ("'\r'", (Invalid::UnescapedCharacter, 0, 2)),
            ("'\r\n'", (Invalid::UnescapedCharacter, 0, 2)),
            ("'\\q'", escape(InvalidEscape::Unknown, 1, 3)),
            ("'\\\n'", escape(InvalidEscape::Unknown, 1, 3)),
            ("'\\x80'", escape(InvalidEscape::HexOutOfRange, 1, 5)),
            ("'\\u{D800}'", escape(InvalidEscape::UnicodeNotScalar, 1, 9)),
            ("'( ", (Invalid::UnclosedCharLiteral, 0, 3)),
            ("'1a", (Invalid::UnclosedCharLiteral, 0, 3)),
            ("'\\nx'", (Invalid::UnclosedCharLiteral, 0, 4)),
            ("'\u{2028}x", (Invalid::UnclosedCharLiteral, 0, 5)),
            ("'", (Invalid::UnterminatedLiteral, 0, 1)),
            ("'(", (Invalid::UnterminatedLiteral, 0, 2)),
            ("'\\n", (Invalid::UnterminatedLiteral, 0, 3)),
            ("'\\", (Invalid::UnterminatedLiteral, 0, 2)),
            ("'a'_", (Invalid::UnderscoreSuffix, 3, 4)),
            ("b'é'", (Invalid::NonAsciiInByteLiteral, 2, 4)),
            (
                "b'\\u{41}'",
                escape(InvalidEscape::UnicodeInByteLiteral, 2, 4),
            ),
            ("b'\\x4'", escape(InvalidEscape::ShortHex, 2, 5)),
            ("b'ab'", (Invalid::UnclosedCharLiteral, 0, 4)),
            ("b''", (Invalid::EmptyCharLiteral, 0, 3)),
            ("b'\t'", (Invalid::UnescapedCharacter, 0, 3)),
            ("b');", (Invalid::UnclosedCharLiteral, 0, 4)),
            ("b'", (Invalid::UnterminatedLiteral, 0, 2)),
            ("b'a'_", (Invalid::UnderscoreSuffix, 4, 5)),
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(&format!("x {text}"), edition),
                    vec![ident(0, 1), invalid_at(reason, 2 + lo, 2 + hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn an_identifier_followed_by_a_single_quote_is_a_reserved_prefix_from_2021() {
        for text in ["a'x'", "r'x'", "br'x'", "fn'x'", "B'x'", "c'x'"] {
            let prefix = text.len() - 3;
            for edition in EDITIONS {
                let expected = if edition >= Edition::E2021 {
                    vec![invalid_at(Invalid::ReservedPrefix, 0, prefix + 1)]
                } else {
                    let kind = Keyword::from_text(&text.as_bytes()[..prefix], edition)
                        .map_or(TokenKind::Identifier, TokenKind::Keyword);
                    vec![
                        token(kind, 0, prefix),
                        character(prefix, prefix + 3),
                        end(prefix + 3),
                    ]
                };
                assert_eq!(lex_in(text, edition), expected, "{text:?} in {edition:?}");
            }
        }
        assert_eq!(
            lex_in("continue'foo", Edition::E2021),
            vec![invalid_at(Invalid::ReservedPrefix, 0, 9)]
        );
        assert_eq!(
            lex_in("continue'foo", Edition::E2018),
            vec![keyword(Keyword::Continue, 0, 8), lifetime(8, 12), end(12)]
        );
        for edition in EDITIONS {
            assert_eq!(
                lex_in("r#a'x'", edition),
                vec![raw_ident(0, 3), character(3, 6), end(6)],
                "{edition:?}"
            );
        }
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
    fn a_non_ascii_start_is_an_identifier_only_when_it_is_xid_start() {
        for c in ['é', '中', '\u{0C5C}', '\u{212A}'] {
            let text = format!("fn {c} x");
            let hi = 3 + c.len_utf8();
            assert_eq!(
                lex(&text),
                vec![
                    keyword(Keyword::Fn, 0, 2),
                    ident(3, hi),
                    ident(hi + 1, hi + 2),
                    end(hi + 2)
                ],
                "{c:?}"
            );
        }
        for c in [
            '\u{A0}',
            '€',
            '😀',
            '\u{FEFF}',
            '\u{200B}',
            '\u{200C}',
            '\u{200D}',
            '\u{202E}',
            '\u{3000}',
            '\u{80}',
            '\u{FFFD}',
            '\u{10FFFF}',
            '\u{0301}',
            '\u{037A}',
            '\u{0558}',
            '\u{2E2F}',
        ] {
            let text = format!("fn {c} x");
            assert_eq!(
                lex(&text),
                vec![keyword(Keyword::Fn, 0, 2), invalid(c, 3)],
                "{c:?}"
            );
        }
    }

    /// For every non-ASCII character `c` other than whitespace: `c` at a token
    /// start is an identifier when it is `XID_Start` and invalid otherwise.
    /// After the identifier `a`, the lifetime `'a`, and the suffix `u` of
    /// `1u`, ZWJ and ZWNJ are unsupported, any other `XID_Continue` character
    /// extends the token, and any other character ends it and starts no token.
    #[test]
    fn every_non_ascii_character_is_classified_by_the_identifier_tables() {
        let mut sources = SourceTable::default();
        let id = sources.add_text("main.rs", "");
        let mut text = String::new();
        let mut count = 0;
        for c in ('\u{80}'..=char::MAX).filter(|&c| !is_whitespace(c)) {
            let len = c.len_utf8();
            // The space keeps U+FEFF from being a byte order mark.
            text.clear();
            text.push(' ');
            text.push(c);
            let alone = if unicode::is_xid_start(c) {
                vec![ident(1, 1 + len), end(1 + len)]
            } else {
                vec![invalid(c, 1)]
            };
            assert_eq!(lex_source(id, &text, Edition::E2024), alone, "{c:?}");

            for (prefix, kind) in [
                ("a", TokenKind::Identifier),
                ("'a", TokenKind::Lifetime),
                ("1u", TokenKind::IntegerLiteral),
            ] {
                text.clear();
                text.push_str(prefix);
                text.push(c);
                let at = prefix.len();
                let expected = if matches!(c, '\u{200C}' | '\u{200D}') {
                    vec![unsupported(c, at)]
                } else if unicode::is_xid_continue(c) {
                    vec![token(kind, 0, at + len), end(at + len)]
                } else {
                    vec![token(kind, 0, at), invalid(c, at)]
                };
                assert_eq!(
                    lex_source(id, &text, Edition::E2024),
                    expected,
                    "{prefix}{c:?}"
                );
            }
            count += 1;
        }
        // The scalar values, less the surrogates, ASCII, and five whitespace characters.
        assert_eq!(count, 0x11_0000 - 0x800 - 0x80 - 5);
    }

    #[test]
    fn a_character_that_no_identifier_can_contain_ends_the_token_before_it() {
        let emoji = '\u{1F600}';
        let acute = '\u{0301}';
        assert_eq!(lex("a\u{1F600}"), vec![ident(0, 1), invalid(emoji, 1)]);
        assert_eq!(lex("'a\u{1F600}"), vec![lifetime(0, 2), invalid(emoji, 2)]);
        assert_eq!(lex("1\u{301}"), vec![int(0, 1), invalid(acute, 1)]);
        assert_eq!(lex("\"a\"\u{301}"), vec![string(0, 3), invalid(acute, 3)]);
        assert_eq!(lex("1.\u{301}"), vec![float(0, 2), invalid(acute, 2)]);
        assert_eq!(lex("\u{200D}a"), vec![invalid('\u{200D}', 0)]);
    }

    #[test]
    fn a_raw_prefix_or_quote_before_a_character_that_starts_no_name_is_invalid() {
        assert_eq!(
            lex("r#\u{301}"),
            vec![invalid_at(Invalid::MalformedRawPrefix, 0, 4)]
        );
        assert_eq!(
            lex("'r#\u{301}"),
            vec![invalid_at(Invalid::MalformedRawPrefix, 0, 5)]
        );
        assert_eq!(
            lex("'+x"),
            vec![invalid_at(Invalid::UnclosedCharLiteral, 0, 3)]
        );
        assert_eq!(
            lex("'\u{301}x"),
            vec![invalid_at(Invalid::UnclosedCharLiteral, 0, 4)]
        );
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
    fn a_raw_identifier_is_one_token_in_every_edition() {
        for text in [
            "r#fn", "r#a", "r#async", "r#match", "r#dyn", "r#_a", "r#r", "r#gen", "r#x1_",
            "r#selfx",
        ] {
            for edition in EDITIONS {
                assert_eq!(
                    lex_in(text, edition),
                    vec![raw_ident(0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn a_reserved_raw_identifier_is_invalid_in_every_edition() {
        for text in ["r#_", "r#crate", "r#self", "r#Self", "r#super"] {
            for edition in EDITIONS {
                assert_eq!(
                    lex_in(&format!("x {text} y"), edition),
                    vec![
                        ident(0, 1),
                        invalid_at(Invalid::ReservedRawIdentifier, 2, 2 + text.len())
                    ],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn no_reserved_prefix_follows_a_raw_identifier() {
        for edition in EDITIONS {
            assert_eq!(
                lex_in("r#a#b", edition),
                vec![
                    raw_ident(0, 3),
                    punct(Punctuation::Pound, 3, 4),
                    ident(4, 5),
                    end(5)
                ],
                "{edition:?}"
            );
        }
        assert_eq!(
            lex_in("r#x\"a\"#", Edition::E2021),
            vec![
                raw_ident(0, 3),
                string(3, 6),
                punct(Punctuation::Pound, 6, 7),
                end(7)
            ]
        );
    }

    #[test]
    fn a_raw_prefix_that_starts_no_raw_input_is_invalid() {
        for (text, hi, editions) in [
            ("r##a", 4, &EDITIONS[..]),
            ("r# a", 3, &EDITIONS[..]),
            ("r# \"a\"#", 3, &EDITIONS[..]),
            ("r#)", 3, &EDITIONS[..]),
            ("r#1", 3, &EDITIONS[..]),
            ("r#", 2, &EDITIONS[..]),
            ("r#\u{2028}", 5, &EDITIONS[..]),
            ("br#a", 4, &EDITIONS[..]),
            ("br#", 3, &EDITIONS[..]),
            ("br#é", 5, &EDITIONS[..]),
            ("br##a", 5, &EDITIONS[..]),
            ("cr#a", 4, &EDITIONS[2..]),
            ("cr##é", 6, &EDITIONS[2..]),
        ] {
            for &edition in editions {
                assert_eq!(
                    lex_in(text, edition),
                    vec![invalid_at(Invalid::MalformedRawPrefix, 0, hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
        for edition in [Edition::E2015, Edition::E2018] {
            assert_eq!(
                lex_in("cr#a", edition),
                vec![
                    ident(0, 2),
                    punct(Punctuation::Pound, 2, 3),
                    ident(3, 4),
                    end(4),
                ],
                "{edition:?}"
            );
        }
    }

    #[test]
    fn a_raw_literal_has_at_most_255_pounds() {
        let pounds = "#".repeat(255);
        let text = format!("r{pounds}\"a\"#{pounds}");
        assert_eq!(
            lex(&text),
            vec![
                raw_string(0, text.len() - 1),
                punct(Punctuation::Pound, text.len() - 1, text.len()),
                end(text.len())
            ]
        );
        let text = format!("r{pounds}\"a\"{pounds}");
        assert_eq!(lex(&text), vec![raw_string(0, text.len()), end(text.len())]);
        let text = format!("br#{pounds}\"a\"#{pounds}");
        assert_eq!(
            lex(&text),
            vec![invalid_at(Invalid::TooManyRawPounds, 0, 258)]
        );
        assert_eq!(
            lex(&format!("r#{pounds}")),
            vec![invalid_at(Invalid::TooManyRawPounds, 0, 257)]
        );
    }

    #[test]
    fn every_string_literal_form_is_one_token() {
        for text in [
            "\"hello\"",
            "\"\"",
            "\"a\\\"b\"",
            "\"\\n\\r\\t\\\\\\0\\'\\\"\"",
            "\"\\x00\\x7F\\x41\\x7f\"",
            "\"\\u{7FFF}\"",
            "\"\\u{10FFFF}\"",
            "\"\\u{1_0_F}\"",
            "\"\\u{0000_41}\"",
            "\"\\u{41__}\"",
            "\"\\u{0}\"",
            "\"é中😀\"",
            "\"a\nb\"",
            "\"a\\\n    b\"",
            "\"a\r\nb\"",
            "\"a\\\r\n    b\"",
            "\"a\0b\"",
            "\"a\tb\"",
            "\"a\u{2028}b\"",
            "\"'\"",
            "\"// /* ( [ {\"",
            "\"\\\\\"",
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![string(0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn every_raw_string_literal_form_is_one_token() {
        for (text, kind) in [
            ("r\"a\\b\"", TokenKind::RawStringLiteral),
            ("r#\"a\"b\"#", TokenKind::RawStringLiteral),
            ("r##\"a\"#b\"##", TokenKind::RawStringLiteral),
            ("r\"\"", TokenKind::RawStringLiteral),
            ("r#\"\"#", TokenKind::RawStringLiteral),
            ("r\"é\"", TokenKind::RawStringLiteral),
            ("r\"\\u{D800}\\q\"", TokenKind::RawStringLiteral),
            ("r\"a\r\nb\"", TokenKind::RawStringLiteral),
            ("r\"a\0b\"", TokenKind::RawStringLiteral),
            ("r#\"\"\"#", TokenKind::RawStringLiteral),
            ("br\"a\\b\"", TokenKind::RawByteStringLiteral),
            ("br#\"a\"b\"#", TokenKind::RawByteStringLiteral),
            ("br\"\\xFF\"", TokenKind::RawByteStringLiteral),
            ("b\"abc\"", TokenKind::ByteStringLiteral),
            ("b\"\"", TokenKind::ByteStringLiteral),
            ("b\"\\xFF\\x00\\x80\"", TokenKind::ByteStringLiteral),
            ("b\"\\0\\n\\r\\t\\\\\\'\\\"\"", TokenKind::ByteStringLiteral),
            ("b\"a\\\n  b\"", TokenKind::ByteStringLiteral),
            ("b\"a\r\nb\0\"", TokenKind::ByteStringLiteral),
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![token(kind, 0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
        assert_eq!(
            lex("r#\"a\"##"),
            vec![raw_string(0, 6), punct(Punctuation::Pound, 6, 7), end(7)]
        );
    }

    #[test]
    fn c_string_literals_start_in_2021() {
        for (text, kind) in [
            ("c\"abc\"", TokenKind::CStringLiteral),
            ("c\"\\xFF\"", TokenKind::CStringLiteral),
            ("c\"\\x80\\x01\"", TokenKind::CStringLiteral),
            ("c\"\\u{E6}\\u{10}\"", TokenKind::CStringLiteral),
            ("c\"æ\"", TokenKind::CStringLiteral),
            ("c\"\\'\\\"\\n\\\n x\"", TokenKind::CStringLiteral),
            ("cr\"a\"", TokenKind::RawCStringLiteral),
            ("cr#\"a\"b\"#", TokenKind::RawCStringLiteral),
            ("cr\"\\0\"", TokenKind::RawCStringLiteral),
        ] {
            for edition in [Edition::E2021, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![token(kind, 0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
        for edition in [Edition::E2015, Edition::E2018] {
            assert_eq!(
                lex_in("c\"abc\"", edition),
                vec![ident(0, 1), string(1, 6), end(6)],
                "{edition:?}"
            );
            assert_eq!(
                lex_in("cr\"a\"", edition),
                vec![ident(0, 2), string(2, 5), end(5)],
                "{edition:?}"
            );
            assert_eq!(
                lex_in("cr#\"a\"#", edition),
                vec![
                    ident(0, 2),
                    punct(Punctuation::Pound, 2, 3),
                    string(3, 6),
                    punct(Punctuation::Pound, 6, 7),
                    end(7),
                ],
                "{edition:?}"
            );
        }
    }

    /// Each span is relative to the start of the literal.
    #[test]
    fn every_invalid_string_form_is_invalid_inside_the_literal() {
        let escape = |reason, lo, hi| (Invalid::Escape(reason), lo, hi);
        for (text, (reason, lo, hi), editions) in [
            (
                "\"\\q\"",
                escape(InvalidEscape::Unknown, 1, 3),
                &EDITIONS[..],
            ),
            (
                "\"\\é\"",
                escape(InvalidEscape::Unknown, 1, 4),
                &EDITIONS[..],
            ),
            (
                "\"\\\t\"",
                escape(InvalidEscape::Unknown, 1, 3),
                &EDITIONS[..],
            ),
            (
                "\"\\x80\"",
                escape(InvalidEscape::HexOutOfRange, 1, 5),
                &EDITIONS[..],
            ),
            (
                "\"\\xFF\"",
                escape(InvalidEscape::HexOutOfRange, 1, 5),
                &EDITIONS[..],
            ),
            (
                "\"\\x4\"",
                escape(InvalidEscape::ShortHex, 1, 4),
                &EDITIONS[..],
            ),
            (
                "\"\\xG0\"",
                escape(InvalidEscape::ShortHex, 1, 3),
                &EDITIONS[..],
            ),
            (
                "\"\\x4",
                escape(InvalidEscape::ShortHex, 1, 4),
                &EDITIONS[..],
            ),
            (
                "\"\\u{D800}\"",
                escape(InvalidEscape::UnicodeNotScalar, 1, 9),
                &EDITIONS[..],
            ),
            (
                "\"\\u{110000}\"",
                escape(InvalidEscape::UnicodeNotScalar, 1, 11),
                &EDITIONS[..],
            ),
            (
                "\"\\u{}\"",
                escape(InvalidEscape::UnicodeEmpty, 1, 5),
                &EDITIONS[..],
            ),
            (
                "\"\\u{__}\"",
                escape(InvalidEscape::UnicodeUnderscoreStart, 1, 5),
                &EDITIONS[..],
            ),
            (
                "\"\\u{1234567}\"",
                escape(InvalidEscape::UnicodeOverlong, 1, 11),
                &EDITIONS[..],
            ),
            (
                "\"\\u{_1}\"",
                escape(InvalidEscape::UnicodeUnderscoreStart, 1, 5),
                &EDITIONS[..],
            ),
            (
                "\"\\u0041\"",
                escape(InvalidEscape::UnicodeNoBrace, 1, 3),
                &EDITIONS[..],
            ),
            (
                "\"\\u{41\"",
                escape(InvalidEscape::UnicodeUnclosed, 1, 6),
                &EDITIONS[..],
            ),
            (
                "\"\\u{4g}\"",
                escape(InvalidEscape::UnicodeUnclosed, 1, 5),
                &EDITIONS[..],
            ),
            ("\"abc", (Invalid::UnterminatedLiteral, 0, 4), &EDITIONS[..]),
            (
                "\"a\\\"",
                (Invalid::UnterminatedLiteral, 0, 4),
                &EDITIONS[..],
            ),
            ("\"\\", (Invalid::UnterminatedLiteral, 0, 2), &EDITIONS[..]),
            (
                "\"a\rb\"",
                (Invalid::BareCarriageReturn, 2, 3),
                &EDITIONS[..],
            ),
            (
                "\"a\r\r\nb\"",
                (Invalid::BareCarriageReturn, 2, 3),
                &EDITIONS[..],
            ),
            (
                "\"a\\\rb\"",
                (Invalid::BareCarriageReturn, 3, 4),
                &EDITIONS[..],
            ),
            ("\"a\r", (Invalid::BareCarriageReturn, 2, 3), &EDITIONS[..]),
            (
                "r#\"abc\"",
                (Invalid::UnterminatedLiteral, 0, 7),
                &EDITIONS[..],
            ),
            (
                "r##\"a\"#",
                (Invalid::UnterminatedLiteral, 0, 7),
                &EDITIONS[..],
            ),
            (
                "r\"a\rb\"",
                (Invalid::BareCarriageReturn, 3, 4),
                &EDITIONS[..],
            ),
            (
                "b\"é\"",
                (Invalid::NonAsciiInByteLiteral, 2, 4),
                &EDITIONS[..],
            ),
            (
                "br\"é\"",
                (Invalid::NonAsciiInByteLiteral, 3, 5),
                &EDITIONS[..],
            ),
            (
                "br#\"a😀\"#",
                (Invalid::NonAsciiInByteLiteral, 5, 9),
                &EDITIONS[..],
            ),
            (
                "b\"\\u{41}\"",
                escape(InvalidEscape::UnicodeInByteLiteral, 2, 4),
                &EDITIONS[..],
            ),
            (
                "b\"\\q\"",
                escape(InvalidEscape::Unknown, 2, 4),
                &EDITIONS[..],
            ),
            (
                "b\"\\x4\"",
                escape(InvalidEscape::ShortHex, 2, 5),
                &EDITIONS[..],
            ),
            (
                "b\"a\rb\"",
                (Invalid::BareCarriageReturn, 3, 4),
                &EDITIONS[..],
            ),
            (
                "b\"abc",
                (Invalid::UnterminatedLiteral, 0, 5),
                &EDITIONS[..],
            ),
            ("c\"\\0\"", (Invalid::NulInCString, 2, 4), &EDITIONS[2..]),
            ("c\"\\x00\"", (Invalid::NulInCString, 2, 6), &EDITIONS[2..]),
            ("c\"\\u{0}\"", (Invalid::NulInCString, 2, 7), &EDITIONS[2..]),
            (
                "c\"\\u{0_0}\"",
                (Invalid::NulInCString, 2, 9),
                &EDITIONS[2..],
            ),
            ("c\"a\0b\"", (Invalid::NulInCString, 3, 4), &EDITIONS[2..]),
            ("cr\"a\0b\"", (Invalid::NulInCString, 4, 5), &EDITIONS[2..]),
            (
                "c\"\\x80\\q\"",
                escape(InvalidEscape::Unknown, 6, 8),
                &EDITIONS[2..],
            ),
            (
                "c\"\\u{D800}\"",
                escape(InvalidEscape::UnicodeNotScalar, 2, 10),
                &EDITIONS[2..],
            ),
            (
                "cr#\"a\"",
                (Invalid::UnterminatedLiteral, 0, 6),
                &EDITIONS[2..],
            ),
        ] {
            for &edition in editions {
                assert_eq!(
                    lex_in(&format!("x {text}"), edition),
                    vec![ident(0, 1), invalid_at(reason, 2 + lo, 2 + hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn a_literal_suffix_is_part_of_the_literal() {
        for text in [
            "\"a\"suffix",
            "\"a\"_x",
            "\"a\"__",
            "\"a\"e3",
            "\"a\"x_1",
            "r\"a\"x",
            "r#\"a\"#x",
            "b\"a\"x",
            "br\"a\"_b",
        ] {
            assert_eq!(lex(text).len(), 2, "{text:?}");
            assert_eq!(
                lex(text)[0].map(|token| token.span),
                Ok(span(0, text.len())),
                "{text:?}"
            );
        }
        assert_eq!(
            lex("\"a\" suffix"),
            vec![string(0, 3), ident(4, 10), end(10)]
        );
        assert_eq!(lex("\"a\"x\"b\""), vec![string(0, 4), string(4, 7), end(7)]);
        assert_eq!(lex("\"a\"1"), vec![string(0, 3), int(3, 4), end(4)]);
        for (text, lo) in [
            ("\"a\"_", 3),
            ("\"a\"_ ", 3),
            ("r\"a\"_", 4),
            ("b\"a\"_", 4),
            ("\"a\"_.", 3),
        ] {
            assert_eq!(
                lex(&format!("x {text}")),
                vec![
                    ident(0, 1),
                    invalid_at(Invalid::UnderscoreSuffix, 2 + lo, 3 + lo)
                ],
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_identifier_followed_by_a_double_quote_is_a_reserved_prefix_from_2021() {
        for text in [
            "a\"x\"", "rb\"x\"", "fn\"x\"", "_\"x\"", "bc\"x\"", "cb\"x\"", "Br\"x\"", "C\"x\"",
        ] {
            let prefix = text.len() - 3;
            for edition in EDITIONS {
                let expected = if edition >= Edition::E2021 {
                    vec![invalid_at(Invalid::ReservedPrefix, 0, prefix + 1)]
                } else {
                    let kind = Keyword::from_text(&text.as_bytes()[..prefix], edition)
                        .map_or(TokenKind::Identifier, TokenKind::Keyword);
                    vec![
                        token(kind, 0, prefix),
                        string(prefix, prefix + 3),
                        end(prefix + 3),
                    ]
                };
                assert_eq!(lex_in(text, edition), expected, "{text:?} in {edition:?}");
            }
        }
        for edition in EDITIONS {
            assert_eq!(
                lex_in("#r\"x\"", edition),
                vec![punct(Punctuation::Pound, 0, 1), raw_string(1, 5), end(5)],
                "{edition:?}"
            );
        }
    }

    #[test]
    fn delimiter_characters_inside_literals_and_comments_are_not_delimiters() {
        assert_eq!(
            lex("(\"(\" /* ( */ [r#\")\"#] b\"{\" c\"}\")"),
            vec![
                open(Delimiter::Parenthesis, 0),
                string(1, 4),
                open(Delimiter::Bracket, 13),
                raw_string(14, 20),
                close(Delimiter::Bracket, 20),
                token(TokenKind::ByteStringLiteral, 22, 26),
                token(TokenKind::CStringLiteral, 27, 31),
                close(Delimiter::Parenthesis, 31),
                end(32),
            ]
        );
        assert_eq!(
            lex("(\"(\" ')' /* ( */ [']'] r#\")\"# b'{')"),
            vec![
                open(Delimiter::Parenthesis, 0),
                string(1, 4),
                character(5, 8),
                open(Delimiter::Bracket, 17),
                character(18, 21),
                close(Delimiter::Bracket, 21),
                raw_string(23, 29),
                token(TokenKind::ByteLiteral, 30, 34),
                close(Delimiter::Parenthesis, 34),
                end(35),
            ]
        );
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
                vec![
                    punct(Punctuation::Pound, 0, 1),
                    string(1, 4),
                    punct(Punctuation::Pound, 4, 5),
                    end(5)
                ],
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
        for edition in [Edition::E2015, Edition::E2024] {
            assert_eq!(
                lex_in("1 u8", edition),
                vec![int(0, 1), ident(2, 4), end(4)],
                "{edition:?}"
            );
            assert_eq!(
                lex_in("0x1e+3", edition),
                vec![int(0, 4), punct(Punctuation::Plus, 4, 5), int(5, 6), end(6)],
                "{edition:?}"
            );
        }
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
            ("1._", 1, keyword(Keyword::Underscore, 2, 3)),
            ("1.e3", 1, ident(2, 4)),
            ("1.f32", 1, ident(2, 5)),
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
    fn every_float_literal_form_is_one_token() {
        for text in [
            "1.0",
            "1.",
            "0.1",
            "1e3",
            "1E+3",
            "1e-3",
            "1e_3",
            "1e+_3",
            "1_e3",
            "12_.5",
            "1_000.000_1",
            "1.0e10",
            "1.0e-3",
            "0e0",
            "00.5",
            "1.0_e3",
            "1.0e3_f32",
            "1.0f32",
            "1.0_f64",
            "1e3f32",
            "2e5e6",
            "1.0e3e",
            "1.0suffix",
            "1.0x",
            "1.0E3x_1",
            "340282366920938463463374607431768211456.0",
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![float(0, text.len()), end(text.len())],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn a_float_literal_ends_where_no_digit_exponent_or_suffix_continues_it() {
        for edition in [Edition::E2015, Edition::E2024] {
            assert_eq!(
                lex_in("1. ", edition),
                vec![float(0, 2), end(3)],
                "{edition:?}"
            );
        }
        assert_eq!(lex("1.\u{2028}"), vec![float(0, 2), end(5)]);
        assert_eq!(
            lex("1.;"),
            vec![float(0, 2), punct(Punctuation::Semi, 2, 3), end(3)]
        );
        assert_eq!(
            lex("1.+2"),
            vec![
                float(0, 2),
                punct(Punctuation::Plus, 2, 3),
                int(3, 4),
                end(4)
            ]
        );
        assert_eq!(lex("1.0 f32"), vec![float(0, 3), ident(4, 7), end(7)]);
        assert_eq!(
            lex("1e3-2"),
            vec![
                float(0, 3),
                punct(Punctuation::Minus, 3, 4),
                int(4, 5),
                end(5)
            ]
        );
        assert_eq!(
            lex("1.0.0"),
            vec![
                float(0, 3),
                punct(Punctuation::Dot, 3, 4),
                int(4, 5),
                end(5)
            ]
        );
        assert_eq!(
            lex("1.0..2"),
            vec![
                float(0, 3),
                punct(Punctuation::DotDot, 3, 5),
                int(5, 6),
                end(6)
            ]
        );
        assert_eq!(
            lex("1.0.x"),
            vec![
                float(0, 3),
                punct(Punctuation::Dot, 3, 4),
                ident(4, 5),
                end(5)
            ]
        );
        assert_eq!(
            lex("t.0.1"),
            vec![
                ident(0, 1),
                punct(Punctuation::Dot, 1, 2),
                float(2, 5),
                end(5)
            ]
        );
        assert_eq!(
            lex("1.0#"),
            vec![float(0, 3), punct(Punctuation::Pound, 3, 4), end(4)]
        );
    }

    #[test]
    fn an_empty_float_exponent_is_invalid_from_the_literal_start() {
        for (text, hi) in [
            ("1.0e", 4),
            ("1.0e+", 5),
            ("1.0E_", 5),
            ("1.0em", 4),
            ("1.0e-__x", 7),
            ("1.0e.5", 4),
            ("1.0eé", 4),
            ("1_.0_E+", 7),
        ] {
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(&format!("x {text}"), edition),
                    vec![ident(0, 1), invalid_at(Invalid::EmptyExponent, 2, 2 + hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
    }

    #[test]
    fn no_token_is_returned_before_a_zero_width_joiner_that_could_extend_it() {
        for (text, lo) in [
            ("caf\u{e9}\u{200D}", 5),
            ("_\u{200D}", 1),
            ("fn\u{200D}", 2),
            ("1u\u{200D}", 2),
            ("1_u\u{200D}", 3),
            ("1.0f32\u{200D}", 6),
            ("1e3f32\u{200D}", 6),
            ("0b1u\u{200D}", 4),
            ("'a\u{200D}", 2),
            ("'_\u{200D}", 2),
            ("'a'x\u{200D}", 4),
            ("b'a'x\u{200D}", 5),
            ("'r#a\u{200D}", 4),
            ("r#a\u{200D}", 3),
            ("\"a\"x\u{200D}", 4),
            ("\"a\"_\u{200D}", 4),
            ("r\"a\"x\u{200D}", 5),
            ("r#\"a\"#x\u{200D}", 7),
            ("b\"a\"x\u{200D}", 5),
            ("c\"a\"x\u{200D}", 5),
        ] {
            assert_eq!(lex(text), vec![unsupported('\u{200D}', lo)], "{text:?}");
        }
    }

    #[test]
    fn non_ascii_characters_continue_every_identifier_class() {
        let kind_of = |text: &str| -> Vec<Result<Token, LexError>> {
            let mut results = lex(text);
            results.pop();
            results
        };
        for (text, kind) in [
            ("café", TokenKind::Identifier),
            ("_é", TokenKind::Identifier),
            ("fn中", TokenKind::Identifier),
            ("1é", TokenKind::IntegerLiteral),
            ("1_é", TokenKind::IntegerLiteral),
            ("1ué", TokenKind::IntegerLiteral),
            ("1.0é", TokenKind::FloatLiteral),
            ("1.0_é", TokenKind::FloatLiteral),
            ("1e3é", TokenKind::FloatLiteral),
            ("1.0f32é", TokenKind::FloatLiteral),
            ("1e3f32é", TokenKind::FloatLiteral),
            ("0b1é", TokenKind::IntegerLiteral),
            ("'é", TokenKind::Lifetime),
            ("'aé", TokenKind::Lifetime),
            ("'_é", TokenKind::Lifetime),
            ("'a'é", TokenKind::CharLiteral),
            ("'a'xé", TokenKind::CharLiteral),
            ("b'a'é", TokenKind::ByteLiteral),
            ("'r#é", TokenKind::RawLifetime),
            ("'r#aé", TokenKind::RawLifetime),
            ("r#é", TokenKind::RawIdentifier),
            ("r#aé", TokenKind::RawIdentifier),
            ("\"a\"é", TokenKind::StringLiteral),
            ("\"a\"xé", TokenKind::StringLiteral),
            ("\"a\"_é", TokenKind::StringLiteral),
            ("r\"a\"é", TokenKind::RawStringLiteral),
            ("r#\"a\"#é", TokenKind::RawStringLiteral),
            ("b\"a\"é", TokenKind::ByteStringLiteral),
            ("c\"a\"é", TokenKind::CStringLiteral),
        ] {
            assert_eq!(kind_of(text), vec![token(kind, 0, text.len())], "{text:?}");
        }
        assert_eq!(
            lex("1.é"),
            vec![
                int(0, 1),
                punct(Punctuation::Dot, 1, 2),
                ident(2, 4),
                end(4)
            ]
        );
        assert_eq!(
            lex("0x1.é"),
            vec![
                int(0, 3),
                punct(Punctuation::Dot, 3, 4),
                ident(4, 6),
                end(6)
            ]
        );
        assert_eq!(
            lex("'é中'"),
            vec![invalid_at(Invalid::CharLiteralTooLong, 0, 7)]
        );
    }

    /// The probe tokens of the Unicode identifier compatibility run, in the
    /// edition of each probe.
    #[test]
    fn lexes_the_unicode_identifier_probe_tokens() {
        let single = |text: &str, kind: TokenKind, edition: Edition| {
            assert_eq!(
                lex_in(text, edition),
                vec![token(kind, 0, text.len()), end(text.len())],
                "{text:?} in {edition:?}"
            );
        };
        // u01, u02, u03, u05, u10, and U+212A.
        for edition in [Edition::E2015, Edition::E2024] {
            for text in ["café", "Москва", "東京", "_é"] {
                single(text, TokenKind::Identifier, edition);
            }
        }
        assert_eq!("cafe\u{301}".len(), 6);
        for text in [
            "cafe\u{301}",
            "\u{C5C}",
            "a\u{C5C}",
            "a\u{30FB}b",
            "\u{212A}",
        ] {
            single(text, TokenKind::Identifier, Edition::E2024);
        }
        // u04, u06 to u09, u11, u12, z03.
        for (text, c, lo) in [
            ("\u{301}", '\u{301}', 0),
            ("\u{558}", '\u{558}', 0),
            ("a\u{558}", '\u{558}', 1),
            ("\u{37A}", '\u{37A}', 0),
            ("a\u{2E2F}", '\u{2E2F}', 1),
            ("\u{1F600}", '\u{1F600}', 0),
            ("a\u{1F600}", '\u{1F600}', 1),
            ("\u{200D}a", '\u{200D}', 0),
        ] {
            let mut expected = if lo == 0 { vec![] } else { vec![ident(0, lo)] };
            expected.push(invalid(c, lo));
            assert_eq!(lex(text), expected, "{text:?}");
        }
        // z01, z02, and the identifiers of z04 to z07.
        for c in ['\u{200D}', '\u{200C}'] {
            assert_eq!(lex(&format!("a{c}b")), vec![unsupported(c, 1)], "{c:?}");
            for edition in [Edition::E2015, Edition::E2024] {
                assert_eq!(
                    lex_in(
                        &format!("fn main() {{ let a{c}b = 1; let _ = a{c}b; }}"),
                        edition
                    ),
                    vec![
                        keyword(Keyword::Fn, 0, 2),
                        ident(3, 7),
                        open(Delimiter::Parenthesis, 7),
                        close(Delimiter::Parenthesis, 8),
                        open(Delimiter::Brace, 10),
                        keyword(Keyword::Let, 12, 15),
                        unsupported(c, 17),
                    ],
                    "{c:?} in {edition:?}"
                );
            }
        }
        // r01 to r06.
        for text in ["r#café", "r#é"] {
            single(text, TokenKind::RawIdentifier, Edition::E2024);
        }
        for text in ["'é", "'café"] {
            single(text, TokenKind::Lifetime, Edition::E2024);
        }
        single("'r#é", TokenKind::RawLifetime, Edition::E2021);
        single("1é", TokenKind::IntegerLiteral, Edition::E2024);
        single("1.0é", TokenKind::FloatLiteral, Edition::E2024);
        single("\"a\"é", TokenKind::StringLiteral, Edition::E2024);
        single("'a'é", TokenKind::CharLiteral, Edition::E2024);
        single("'é'", TokenKind::CharLiteral, Edition::E2024);
        assert_eq!(
            lex("1.é"),
            vec![
                int(0, 1),
                punct(Punctuation::Dot, 1, 2),
                ident(2, 4),
                end(4)
            ]
        );
    }

    /// ZWJ and ZWNJ are `XID_Continue`, which `rustc 1.99.0` accepts inside
    /// identifiers, but `[ident.zero-width-chars]` forbids them there. Until
    /// the Reference and `rustc` agree, they are unsupported after the first
    /// character of an identifier, raw identifier, lifetime, raw lifetime, or
    /// suffix. This classification is provisional. Where no identifier has
    /// started, they start no token and are invalid.
    #[test]
    fn zero_width_joiners_are_provisionally_unsupported_inside_identifiers() {
        for c in ['\u{200D}', '\u{200C}'] {
            let len = c.len_utf8();
            for (text, lo) in [
                (format!("a{c}"), 1),
                (format!("_{c}"), 1),
                (format!("a{c}b"), 1),
                (format!("r#a{c}"), 3),
                (format!("'a{c}"), 2),
                (format!("'r#a{c}"), 4),
                (format!("\"x\"a{c}"), 4),
                (format!("\"x\"_{c}"), 4),
                (format!("1a{c}"), 2),
            ] {
                assert_eq!(lex(&text), vec![unsupported(c, lo)], "{text:?}");
            }
            for (text, expected) in [
                (format!("{c}a"), vec![invalid(c, 0)]),
                (
                    format!("r#{c}"),
                    vec![invalid_at(Invalid::MalformedRawPrefix, 0, 2 + len)],
                ),
                (
                    format!("'r#{c}"),
                    vec![invalid_at(Invalid::MalformedRawPrefix, 0, 3 + len)],
                ),
                (format!("\"x\"{c}"), vec![string(0, 3), invalid(c, 3)]),
                (format!("1{c}"), vec![int(0, 1), invalid(c, 1)]),
                (format!("1.{c}"), vec![float(0, 2), invalid(c, 2)]),
                (
                    format!("'{c}x"),
                    vec![invalid_at(Invalid::UnclosedCharLiteral, 0, 2 + len)],
                ),
            ] {
                assert_eq!(lex(&text), expected, "{text:?}");
            }
        }
    }

    /// The reserved prefix tokens of the Reference cover every identifier, so
    /// a non-ASCII identifier follows the edition rules of an ASCII one.
    #[test]
    fn non_ascii_identifiers_follow_the_reserved_prefix_rules() {
        for edition in [Edition::E2015, Edition::E2018] {
            assert_eq!(
                lex_in("é#a", edition),
                vec![
                    ident(0, 2),
                    punct(Punctuation::Pound, 2, 3),
                    ident(3, 4),
                    end(4)
                ]
            );
            assert_eq!(
                lex_in("é\"x\"", edition),
                vec![ident(0, 2), string(2, 5), end(5)]
            );
            assert_eq!(
                lex_in("é'x'", edition),
                vec![ident(0, 2), character(2, 5), end(5)]
            );
            assert_eq!(
                lex_in("'é#a", edition),
                vec![
                    lifetime(0, 3),
                    punct(Punctuation::Pound, 3, 4),
                    ident(4, 5),
                    end(5)
                ]
            );
        }
        for edition in [Edition::E2021, Edition::E2024] {
            for (text, hi) in [("é#a", 3), ("é\"x\"", 3), ("é'x'", 3), ("'é#a", 4)] {
                assert_eq!(
                    lex_in(text, edition),
                    vec![invalid_at(Invalid::ReservedPrefix, 0, hi)],
                    "{text:?} in {edition:?}"
                );
            }
        }
        assert_eq!(
            lex("r#é#a"),
            vec![
                raw_ident(0, 4),
                punct(Punctuation::Pound, 4, 5),
                ident(5, 6),
                end(6)
            ]
        );
    }

    #[test]
    fn punctuation_and_delimiters_are_returned_before_an_unsupported_character() {
        assert_eq!(
            lex("+a\u{200D}"),
            vec![punct(Punctuation::Plus, 0, 1), unsupported('\u{200D}', 2)]
        );
        assert_eq!(
            lex("(a\u{200C}"),
            vec![open(Delimiter::Parenthesis, 0), unsupported('\u{200C}', 2)]
        );
        assert_eq!(
            lex("+é"),
            vec![punct(Punctuation::Plus, 0, 1), ident(1, 3), end(3)]
        );
        assert_eq!(lex("1\"x\""), vec![int(0, 1), string(1, 4), end(4)]);
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
            ("x a\u{200D}", unsupported('\u{200D}', 3)),
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
    /// identifier, literal prefix, digit, exponent sign, punctuation, quote,
    /// escape, whitespace, CR, invalid, and multibyte characters, the last
    /// including a combining mark and ZWJ: up to five characters in edition
    /// 2024, up to four in the earlier editions. Checks that each result lies in the text
    /// after the previous one and that each token other than the end of file
    /// is not empty.
    #[test]
    fn every_short_text_lexes_to_spans_inside_the_text() {
        const ALPHABET: [char; 30] = [
            '/', '*', '!', '#', '[', '\n', 'a', ' ', 'é', '\u{FEFF}', '`', '0', '1', '.', 'e', 'x',
            '_', '<', '-', '=', '"', '+', '\\', 'r', 'b', 'c', '\r', '\'', '\u{301}', '\u{200D}',
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
        assert_eq!(count, 3 * 837_931 + 25_137_931);
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
