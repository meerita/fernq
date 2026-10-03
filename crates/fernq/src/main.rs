//! The `fernq` compiler driver.
//!
//! The driver owns process entry, command-line parsing and validation, the
//! mapping from compilation outcomes to exit status, the mapping from errors
//! to diagnostics and their wording, the selection of work for a requested
//! mode, and writing rendered output to process streams.
//!
//! The driver does not own Rust semantics, the source model contract, the
//! diagnostic structure, or the semantics of any compiler stage. Module
//! `source` owns the source model. Module `edition` owns the edition. Module
//! `lexer` owns tokens and lexical errors. Module `diagnostic` owns the
//! diagnostic structure and its rendering.
//!
//! No workspace crate depends on the driver.
//!
//! `compile` owns the one compilation of a process. Its configuration is the
//! parsed `Invocation`, which includes the edition of the input, and its only
//! mutable state is the `SourceTable`. It loads the input, lexes it to end of
//! file or to the first lexical error, and reports one diagnostic. Each stage
//! receives the values it reads as parameters; the lexer receives the edition.
//! `docs/compiler-session.md` owns the session contract, including global
//! state, process input, and the edition and target.
//!
//! `docs/cli.md` owns the command-line reference: accepted command lines,
//! input loading, diagnostics, exit statuses, and panic reporting.

mod cli;
mod diagnostic;
mod edition;
mod lexer;
mod source;

use std::env;
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::process::ExitCode;

use cli::{Command, Invocation, USAGE, UsageError};
use diagnostic::{Diagnostic, DiagnosticKind, Location};
use edition::Edition;
use lexer::{Invalid, InvalidEscape, LexError, LexErrorKind, Lexer, TokenKind, Unsupported};
use source::{ByteOffset, LoadError, SourceId, SourceTable};

const HELP: &str = "\
Compile a Rust source file. Fernq lexes the input; compilation after lexing
is not implemented.

Arguments:
  <INPUT>                Rust source file to compile

Options:
  -o <OUTPUT>            Write the compiled output to OUTPUT
  --edition <EDITION>    Rust edition of INPUT: 2015, 2018, 2021, or 2024.
                         Required; there is no default.
  -h, --help             Print this help

Exit status:
  0  help was printed
  1  compilation did not succeed
  2  invalid command line

The command line is experimental and may change without notice.";

fn main() -> ExitCode {
    install_panic_hook();
    match cli::parse(env::args_os().skip(1).collect()) {
        Ok(Command::Help) => help(),
        Ok(Command::Compile(invocation)) => compile(&invocation),
        Err(error) => usage_error(&error),
    }
}

/// Reports an uncaught panic as an internal compiler error, then runs the
/// previous hook, which prints the panic message and location.
fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let diagnostic = Diagnostic::internal_compiler_error(info.payload_as_str());
        // An internal compiler error has no location, so no source is needed.
        let rendered = diagnostic::render(&diagnostic, &SourceTable::default());
        // The previous hook runs even when stderr is unwritable.
        let _ = io::stderr().write_all(rendered.as_bytes());
        previous(info);
    }));
}

fn help() -> ExitCode {
    let mut stdout = io::stdout().lock();
    let written = writeln!(stdout, "{USAGE}\n       fernq -h | --help\n\n{HELP}")
        .and_then(|()| stdout.flush());
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn compile(invocation: &Invocation) -> ExitCode {
    let mut sources = SourceTable::default();
    let diagnostic = match sources.load(invocation.input()) {
        Ok(id) => {
            let input = sources.get(id);
            match lex(id, input.text(), invocation.edition()) {
                Ok(()) => not_implemented(input.path(), input.len(), invocation.output()),
                Err(error) => lex_failure(error),
            }
        }
        Err(error) => load_failure(invocation.input(), error),
    };
    // The exit status reports the failure even when stderr is unwritable.
    let _ = io::stderr().write_all(diagnostic::render(&diagnostic, &sources).as_bytes());
    // Every diagnostic is an error, and an error fails the compilation.
    ExitCode::FAILURE
}

fn load_failure(input: &Path, error: LoadError) -> Diagnostic {
    let input_display = input.display();
    let (kind, message, location) = match error {
        LoadError::NotFound => (
            DiagnosticKind::InputNotFound,
            format!("input file {input_display} does not exist"),
            None,
        ),
        LoadError::PermissionDenied => (
            DiagnosticKind::InputPermissionDenied,
            format!("permission denied when opening input file {input_display}"),
            None,
        ),
        LoadError::Directory => (
            DiagnosticKind::InputIsDirectory,
            format!("input {input_display} is a directory, not a file"),
            None,
        ),
        LoadError::Io(error) => (
            DiagnosticKind::InputUnreadable,
            format!("cannot read input file {input_display}: {error}"),
            None,
        ),
        LoadError::TooLarge { limit } => (
            DiagnosticKind::InputTooLarge,
            format!("input file {input_display} is larger than {limit} bytes"),
            None,
        ),
        LoadError::NotUtf8(invalid) => (
            DiagnosticKind::InputNotUtf8,
            format!(
                "input file is not valid UTF-8 at byte offset {}",
                invalid.offset()
            ),
            Some(Location::unloaded(
                input.to_path_buf(),
                invalid.into_valid(),
            )),
        ),
        LoadError::TooManyFiles => (
            DiagnosticKind::TooManySourceFiles,
            format!("too many source files to load input file {input_display}"),
            None,
        ),
    };
    Diagnostic::error(kind, message, location)
}

/// Lexes `text`, the text of the source `source`, to end of file.
fn lex(source: SourceId, text: &str, edition: Edition) -> Result<(), LexError> {
    let mut lexer = Lexer::new(source, text, edition);
    while lexer.next_token()?.kind != TokenKind::EndOfFile {}
    Ok(())
}

fn lex_failure(error: LexError) -> Diagnostic {
    let (kind, message) = match error.kind() {
        LexErrorKind::Invalid(reason) => (DiagnosticKind::LexicalError, invalid_message(reason)),
        LexErrorKind::Unsupported(reason) => (
            DiagnosticKind::UnsupportedSyntax,
            unsupported_message(reason),
        ),
    };
    let location = Location::source(error.source(), error.span());
    Diagnostic::error(kind, message, Some(location))
}

fn invalid_message(reason: Invalid) -> String {
    match reason {
        Invalid::Character(c) => format!("{} cannot start a token", describe(c)),
        Invalid::UnterminatedBlockComment => "block comment is not terminated".to_owned(),
        Invalid::ReservedPrefix => "an identifier directly followed by '#' or a quote, or a \
            lifetime directly followed by '#', is a reserved prefix since edition 2021 unless \
            it starts a literal or a raw lifetime"
            .to_owned(),
        Invalid::ReservedPounds => {
            "two or more '#' in a row are reserved since edition 2024".to_owned()
        }
        Invalid::ReservedGuardedString => {
            "'#' directly followed by '\"' is reserved since edition 2024".to_owned()
        }
        Invalid::DigitOutOfRadix => "integer literal has a digit outside its radix".to_owned(),
        Invalid::NoRadixDigits => "integer literal has no digit after its radix prefix".to_owned(),
        Invalid::RadixPeriod => {
            "binary, octal, and hexadecimal literals cannot have a fractional part".to_owned()
        }
        Invalid::RadixExponent => "binary and octal literals cannot have an exponent".to_owned(),
        Invalid::EmptyExponent => "exponent has no digit".to_owned(),
        Invalid::ReservedRawIdentifier => {
            "'_', 'crate', 'self', 'Self', and 'super' cannot be raw identifiers".to_owned()
        }
        Invalid::TooManyRawPounds => {
            "a raw literal is delimited by at most 255 '#' on each side".to_owned()
        }
        Invalid::ReservedRawLifetime => {
            "'_', 'crate', 'self', 'Self', and 'super' cannot be raw lifetimes".to_owned()
        }
        Invalid::MalformedRawPrefix => {
            "raw prefix starts no raw identifier, raw lifetime, or raw literal".to_owned()
        }
        Invalid::UnterminatedLiteral => "literal is not terminated".to_owned(),
        Invalid::BareCarriageReturn => {
            "carriage return in a literal is not followed by a line feed".to_owned()
        }
        Invalid::NonAsciiInByteLiteral => {
            "byte and byte string literals can contain only ASCII characters".to_owned()
        }
        Invalid::NulInCString => "C string literals cannot contain a NUL character".to_owned(),
        Invalid::Escape(reason) => escape_message(reason).to_owned(),
        Invalid::UnderscoreSuffix => "literal suffix cannot be '_' alone".to_owned(),
        Invalid::EmptyCharLiteral => "character literal is empty".to_owned(),
        Invalid::UnescapedCharacter => {
            "a quote, line feed, carriage return, or tab in a character literal must be escaped"
                .to_owned()
        }
        Invalid::UnclosedCharLiteral => {
            "character literal is not closed after one character".to_owned()
        }
        Invalid::CharLiteralTooLong => "character literal has more than one character".to_owned(),
    }
}

fn escape_message(reason: InvalidEscape) -> &'static str {
    match reason {
        InvalidEscape::Unknown => "unknown character escape",
        InvalidEscape::ShortHex => "'\\x' escape needs two hexadecimal digits",
        InvalidEscape::HexOutOfRange => {
            "'\\x' escape in a string or character literal is above '\\x7F'"
        }
        InvalidEscape::UnicodeNoBrace => "'\\u' escape is not followed by '{'",
        InvalidEscape::UnicodeEmpty => "'\\u{}' escape has no hexadecimal digit",
        InvalidEscape::UnicodeUnderscoreStart => "'\\u{...}' escape starts with '_'",
        InvalidEscape::UnicodeOverlong => "'\\u{...}' escape has more than six hexadecimal digits",
        InvalidEscape::UnicodeUnclosed => "'\\u{...}' escape is not closed by '}'",
        InvalidEscape::UnicodeNotScalar => "'\\u{...}' escape is not a Unicode scalar value",
        InvalidEscape::UnicodeInByteLiteral => {
            "byte and byte string literals cannot contain '\\u' escapes"
        }
    }
}

fn unsupported_message(reason: Unsupported) -> String {
    match reason {
        Unsupported::Character(c) => {
            format!("input that contains {} is not supported", describe(c))
        }
        Unsupported::DocComment => "doc comments are not supported".to_owned(),
    }
}

/// Names `c` by its code point, quoted as well when it is printable ASCII.
///
/// Other characters are not quoted: a control or bidirectional character
/// would change how the terminal shows the diagnostic.
fn describe(c: char) -> String {
    let code_point = u32::from(c);
    if c.is_ascii_graphic() {
        format!("character '{c}' (U+{code_point:04X})")
    } else {
        format!("character U+{code_point:04X}")
    }
}

fn not_implemented(input: &Path, len: ByteOffset, output: &Path) -> Diagnostic {
    Diagnostic::error(
        DiagnosticKind::CompilationNotImplemented,
        format!(
            "compilation is not implemented; {} ({len} bytes) was not compiled and {} was not written",
            input.display(),
            output.display(),
        ),
        None,
    )
}

fn usage_error(error: &UsageError) -> ExitCode {
    let message = match error {
        UsageError::NoArguments => "no arguments".to_owned(),
        UsageError::UnknownOption(token) => format!("unknown option '{}'", token.display()),
        UsageError::MissingOutputValue => "option -o requires an output path".to_owned(),
        UsageError::DuplicateOutput => "option -o is given more than once".to_owned(),
        UsageError::MissingEditionValue => {
            "option --edition requires a value: 2015, 2018, 2021, or 2024".to_owned()
        }
        UsageError::DuplicateEdition => "option --edition is given more than once".to_owned(),
        UsageError::UnknownEdition(token) => format!(
            "unknown edition '{}'; expected 2015, 2018, 2021, or 2024",
            token.display()
        ),
        UsageError::EmptyOutput => "the output path is empty".to_owned(),
        UsageError::EmptyInput => "the input path is empty".to_owned(),
        UsageError::ExtraInput(token) => format!(
            "unexpected argument '{}'; only one input path is accepted",
            token.display()
        ),
        UsageError::MissingInput => "missing input path".to_owned(),
        UsageError::MissingOutput => "missing output path; pass -o <OUTPUT>".to_owned(),
        UsageError::MissingEdition => "missing edition; pass --edition <EDITION>".to_owned(),
        UsageError::HelpWithArguments => "-h and --help take no other arguments".to_owned(),
    };
    let diagnostic = Diagnostic::error(DiagnosticKind::InvalidCommandLine, message, None);
    let rendered = diagnostic::render(&diagnostic, &SourceTable::default());
    // The exit status reports the failure even when stderr is unwritable.
    let _ = writeln!(io::stderr(), "{rendered}{USAGE}");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    use source::InvalidUtf8;

    fn load_failures() -> Vec<LoadError> {
        let invalid = String::from_utf8(b"fn main() {}\n// \xff".to_vec()).unwrap_err();
        vec![
            LoadError::NotFound,
            LoadError::PermissionDenied,
            LoadError::Directory,
            LoadError::Io(io::Error::other("device error")),
            LoadError::TooLarge { limit: 4 },
            LoadError::NotUtf8(InvalidUtf8::new(invalid).unwrap()),
            LoadError::TooManyFiles,
        ]
    }

    /// The first lexical error of `text` in edition 2024.
    fn lex_error(text: &str) -> LexError {
        let mut sources = SourceTable::default();
        let id = sources.add_text("main.rs", text);
        lex(id, text, Edition::E2024).unwrap_err()
    }

    #[test]
    fn every_load_failure_and_the_not_implemented_outcome_have_distinct_kinds() {
        let input = Path::new("main.rs");
        let mut kinds: Vec<DiagnosticKind> = load_failures()
            .into_iter()
            .map(|error| load_failure(input, error).kind)
            .collect();
        let len = ByteOffset::try_from(13_usize).unwrap();
        kinds.push(not_implemented(input, len, Path::new("main")).kind);
        kinds.push(lex_failure(lex_error("`")).kind);
        kinds.push(lex_failure(lex_error("é")).kind);

        for (index, kind) in kinds.iter().enumerate() {
            assert!(
                !kinds[index + 1..].contains(kind),
                "{kind:?} is not distinct: {kinds:?}"
            );
        }
    }

    #[test]
    fn only_the_not_utf8_load_failure_has_a_location() {
        for error in load_failures() {
            let is_not_utf8 = matches!(error, LoadError::NotUtf8(_));
            let diagnostic = load_failure(Path::new("main.rs"), error);
            assert_eq!(diagnostic.location.is_some(), is_not_utf8, "{diagnostic:?}");
        }
    }

    #[test]
    fn invalid_input_is_a_lexical_error_with_a_location() {
        for text in [
            "fn `",
            "/* a /* b */",
            "a#b",
            "##",
            "#\"",
            "0b2",
            "0x",
            "0x1.",
            "0b1e",
            "2e",
            "1.0em",
            "\"\\q\"",
            "r#self",
            "a\"x\"",
            "\"a\rb\"",
            "'ab'",
            "b'é'",
            "'r#self",
            "a'x'",
        ] {
            let diagnostic = lex_failure(lex_error(text));
            assert_eq!(diagnostic.kind, DiagnosticKind::LexicalError, "{text:?}");
            assert!(diagnostic.location.is_some(), "{diagnostic:?}");
        }
    }

    #[test]
    fn unsupported_input_is_unsupported_syntax_with_a_location() {
        for text in [
            "fn main() { 1.5é }",
            "/// doc",
            "fn café() {}",
            "r#é",
            "1e3é",
            "'é",
        ] {
            let diagnostic = lex_failure(lex_error(text));
            assert_eq!(
                diagnostic.kind,
                DiagnosticKind::UnsupportedSyntax,
                "{text:?}"
            );
            assert!(diagnostic.location.is_some(), "{diagnostic:?}");
        }
    }

    #[test]
    fn a_character_is_quoted_only_when_it_is_printable_ascii() {
        assert_eq!(describe('`'), "character '`' (U+0060)");
        assert_eq!(describe('\u{0}'), "character U+0000");
        assert_eq!(describe(' '), "character U+0020");
        assert_eq!(describe('\u{202E}'), "character U+202E");
        assert_eq!(describe('\u{1F600}'), "character U+1F600");
    }

    #[test]
    fn lexing_to_end_of_file_succeeds() {
        let mut sources = SourceTable::default();
        let text = "#!/usr/bin/env run\nfn main( {}\n";
        let id = sources.add_text("main.rs", text);
        assert_eq!(lex(id, text, Edition::E2024), Ok(()));
    }

    #[test]
    fn the_not_implemented_outcome_has_no_location() {
        let len = ByteOffset::try_from(13_usize).unwrap();
        let diagnostic = not_implemented(Path::new("main.rs"), len, Path::new("main"));
        assert!(diagnostic.location.is_none(), "{diagnostic:?}");
    }
}
