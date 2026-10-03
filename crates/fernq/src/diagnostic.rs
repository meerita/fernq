//! Diagnostics: the structured form of every failure that `fernq` reports.
//!
//! A [`Diagnostic`] has a kind, a severity, a message, and an optional
//! primary [`Location`]. The [`DiagnosticKind`] is the identity of a
//! diagnostic inside Fernq; the message is human-readable text and carries no
//! identity. Producers map their own errors to diagnostics; this module knows
//! no other Fernq module.
//!
//! [`render`] produces the human-readable text: a header line
//! `<severity>[<kind>]: <message>`, then ` --> <path>:<line>:<column>` when a
//! location exists. The format is experimental and is not a machine contract.
//! Lines and columns count from 1. A line ends only at LF. A column counts the
//! Unicode scalar values before the position on its line, excluding a byte
//! order mark at the start of line 1.
//!
//! The module performs no I/O.

use std::path::PathBuf;

/// The identity of a diagnostic, rendered as a lowercase hyphenated name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticKind {
    InputNotFound,
    InputPermissionDenied,
    InputIsDirectory,
    /// Any other failure to open, inspect, or read the input.
    InputUnreadable,
    InputTooLarge,
    InputNotUtf8,
    TooManySourceFiles,
    CompilationNotImplemented,
    InvalidCommandLine,
    /// An uncaught panic: Fernq violated one of its own invariants.
    InternalCompilerError,
}

impl DiagnosticKind {
    fn name(self) -> &'static str {
        match self {
            Self::InputNotFound => "input-not-found",
            Self::InputPermissionDenied => "input-permission-denied",
            Self::InputIsDirectory => "input-is-directory",
            Self::InputUnreadable => "input-unreadable",
            Self::InputTooLarge => "input-too-large",
            Self::InputNotUtf8 => "input-not-utf8",
            Self::TooManySourceFiles => "too-many-source-files",
            Self::CompilationNotImplemented => "compilation-not-implemented",
            Self::InvalidCommandLine => "invalid-command-line",
            Self::InternalCompilerError => "internal-compiler-error",
        }
    }
}

/// How a diagnostic affects the compilation. An error makes it fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
}

impl Severity {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
        }
    }
}

/// One byte position in a source.
///
/// The representation is provisional. It identifies a source by its path as
/// given, and the position by the valid UTF-8 text that precedes it, because
/// the only positioned diagnostic is for an input that never entered the
/// source table. Locations become byte ranges in loaded sources when source
/// spans have their first consumer.
#[derive(Debug)]
pub(crate) struct Location {
    path: PathBuf,
    before: String,
}

impl Location {
    /// The position just after `before`, which is the text of the source at
    /// `path` that precedes the position.
    pub(crate) fn new(path: PathBuf, before: String) -> Self {
        Self { path, before }
    }
}

#[derive(Debug)]
pub(crate) struct Diagnostic {
    pub(crate) kind: DiagnosticKind,
    pub(crate) severity: Severity,
    pub(crate) message: String,
    pub(crate) location: Option<Location>,
}

impl Diagnostic {
    pub(crate) fn error(kind: DiagnosticKind, message: String, location: Option<Location>) -> Self {
        Self {
            kind,
            severity: Severity::Error,
            message,
            location,
        }
    }

    /// The diagnostic for an uncaught panic whose payload is `payload`, or
    /// `None` when the payload is not a string.
    ///
    /// It has no location: the panic location is a Fernq source position, not
    /// a position in the user's input.
    pub(crate) fn internal_compiler_error(payload: Option<&str>) -> Self {
        let message = payload.unwrap_or("the panic payload is not a string");
        Self::error(
            DiagnosticKind::InternalCompilerError,
            message.to_owned(),
            None,
        )
    }
}

/// Returns the human-readable text of `diagnostic`, each line ending in LF.
pub(crate) fn render(diagnostic: &Diagnostic) -> String {
    let header = format!(
        "{}[{}]: {}\n",
        diagnostic.severity.name(),
        diagnostic.kind.name(),
        diagnostic.message,
    );
    let Some(location) = &diagnostic.location else {
        return header;
    };
    let (line, column) = line_column(&location.before);
    format!("{header} --> {}:{line}:{column}\n", location.path.display())
}

/// Returns the line and column of the position just after `before`.
fn line_column(before: &str) -> (u64, u64) {
    let line = 1 + count(before.bytes().filter(|&byte| byte == b'\n'));
    let current_line = match before.rsplit_once('\n') {
        Some((_, after_last_lf)) => after_last_lf,
        None => before.strip_prefix('\u{FEFF}').unwrap_or(before),
    };
    let column = 1 + count(current_line.chars());
    (line, column)
}

// A text is at most `isize::MAX` bytes long, so one more than any count fits in a u64.
fn count<T>(items: impl Iterator<Item = T>) -> u64 {
    items.fold(0, |count, _| count + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [DiagnosticKind; 10] = [
        DiagnosticKind::InputNotFound,
        DiagnosticKind::InputPermissionDenied,
        DiagnosticKind::InputIsDirectory,
        DiagnosticKind::InputUnreadable,
        DiagnosticKind::InputTooLarge,
        DiagnosticKind::InputNotUtf8,
        DiagnosticKind::TooManySourceFiles,
        DiagnosticKind::CompilationNotImplemented,
        DiagnosticKind::InvalidCommandLine,
        DiagnosticKind::InternalCompilerError,
    ];

    #[test]
    fn the_start_of_the_text_is_line_1_column_1() {
        assert_eq!(line_column(""), (1, 1));
    }

    #[test]
    fn ascii_counts_one_column_per_byte() {
        assert_eq!(line_column("fn main() { let _x: u32 = "), (1, 27));
    }

    #[test]
    fn a_tab_counts_one_column() {
        assert_eq!(line_column("fn main() {\n\tlet _x: u32 = "), (2, 16));
    }

    #[test]
    fn a_multibyte_character_counts_one_column() {
        assert_eq!(line_column("fn main() { let _é: u32 = "), (1, 27));
        assert_eq!(line_column("fn main() { let _中: u32 = "), (1, 27));
    }

    #[test]
    fn a_combining_mark_counts_one_column() {
        assert_eq!(
            line_column("fn main() { let _y = \"e\u{301}\"; let _x: u32 = "),
            (1, 42)
        );
    }

    #[test]
    fn an_emoji_counts_one_column() {
        assert_eq!(
            line_column("fn main() { let _y = \"\u{1F600}\"; let _x: u32 = "),
            (1, 41)
        );
    }

    #[test]
    fn a_lone_cr_does_not_end_a_line() {
        assert_eq!(
            line_column("fn f() {}\rfn main() { let _x: u32 = "),
            (1, 37)
        );
    }

    #[test]
    fn a_line_separator_does_not_end_a_line() {
        assert_eq!(
            line_column("// \u{2028} x\nfn main() { let _x: u32 = "),
            (2, 27)
        );
    }

    #[test]
    fn crlf_ends_a_line_at_its_lf() {
        assert_eq!(line_column("fn main() {\r\n\tlet _x: u32 = "), (2, 16));
    }

    #[test]
    fn a_byte_order_mark_at_the_start_is_not_counted() {
        assert_eq!(line_column("\u{FEFF}"), (1, 1));
        assert_eq!(line_column("\u{FEFF}fn main() { let _x: u32 = "), (1, 27));
    }

    #[test]
    fn a_byte_order_mark_after_line_1_counts_one_column() {
        assert_eq!(line_column("fn main() {}\n\u{FEFF}x"), (2, 3));
    }

    #[test]
    fn positions_of_the_first_invalid_utf8_byte() {
        assert_eq!(line_column("fn main() {}\n// 中 "), (2, 6));
        assert_eq!(line_column("fn main() {}\n// \t"), (2, 5));
    }

    #[test]
    fn text_ending_with_lf_ends_before_the_next_line() {
        assert_eq!(line_column("fn main() {}\n"), (2, 1));
    }

    #[test]
    fn renders_a_diagnostic_without_a_location() {
        let diagnostic = Diagnostic::error(
            DiagnosticKind::InputNotFound,
            "input main.rs does not exist".to_owned(),
            None,
        );
        assert_eq!(
            render(&diagnostic),
            "error[input-not-found]: input main.rs does not exist\n"
        );
    }

    #[test]
    fn renders_a_diagnostic_with_a_location() {
        let diagnostic = Diagnostic::error(
            DiagnosticKind::InputNotUtf8,
            "not UTF-8".to_owned(),
            Some(Location::new(
                PathBuf::from("src/main.rs"),
                "fn main() {}\n// 中 ".to_owned(),
            )),
        );
        assert_eq!(
            render(&diagnostic),
            "error[input-not-utf8]: not UTF-8\n --> src/main.rs:2:6\n"
        );
    }

    #[test]
    fn kind_names_are_distinct_lowercase_and_hyphenated() {
        for (index, kind) in ALL_KINDS.iter().enumerate() {
            let name = kind.name();
            assert!(
                !name.is_empty()
                    && name.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte == b'-'
                        || byte.is_ascii_digit())
                    && !name.starts_with('-')
                    && !name.ends_with('-')
                    && !name.contains("--"),
                "{kind:?}: {name:?}"
            );
            for other in &ALL_KINDS[index + 1..] {
                assert_ne!(name, other.name(), "{kind:?} and {other:?}");
            }
        }
    }

    #[test]
    fn renders_an_internal_compiler_error_for_a_string_payload() {
        let diagnostic = Diagnostic::internal_compiler_error(Some("index out of bounds"));
        assert_eq!(
            render(&diagnostic),
            "error[internal-compiler-error]: index out of bounds\n"
        );
    }

    #[test]
    fn renders_an_internal_compiler_error_for_a_non_string_payload() {
        let diagnostic = Diagnostic::internal_compiler_error(None);
        assert_eq!(
            render(&diagnostic),
            "error[internal-compiler-error]: the panic payload is not a string\n"
        );
    }
}
