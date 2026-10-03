//! The `fernq` compiler driver.
//!
//! The driver owns process entry, command-line parsing and validation, the
//! mapping from compilation outcomes to exit status, the mapping from errors
//! to diagnostics and their wording, the selection of work for a requested
//! mode, and writing rendered output to process streams.
//!
//! The driver does not own Rust semantics, the source model contract, the
//! diagnostic structure, or the semantics of any compiler stage. Module
//! `source` owns the source model. Module `diagnostic` owns the diagnostic
//! structure and its rendering.
//!
//! No workspace crate depends on the driver.
//!
//! `compile` owns the one compilation of a process. Its configuration is the
//! parsed `Invocation`, and its only mutable state is the `SourceTable`. Each
//! stage receives the values it reads as parameters. `docs/compiler-session.md`
//! owns the session contract, including global state, process input, and the
//! edition and target.
//!
//! `docs/cli.md` owns the command-line reference: accepted command lines,
//! input loading, diagnostics, exit statuses, and panic reporting.

mod cli;
mod diagnostic;
mod source;

use std::env;
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::process::ExitCode;

use cli::{Command, Invocation, USAGE, UsageError};
use diagnostic::{Diagnostic, DiagnosticKind, Location};
use source::{ByteOffset, LoadError, SourceTable};

const HELP: &str = "\
Compile a Rust source file. Compilation is not implemented.

Arguments:
  <INPUT>        Rust source file to compile

Options:
  -o <OUTPUT>    Write the compiled output to OUTPUT
  -h, --help     Print this help

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
        // The previous hook runs even when stderr is unwritable.
        let _ = io::stderr().write_all(diagnostic::render(&diagnostic).as_bytes());
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
            not_implemented(input.path(), input.len(), invocation.output())
        }
        Err(error) => load_failure(invocation.input(), error),
    };
    // The exit status reports the failure even when stderr is unwritable.
    let _ = io::stderr().write_all(diagnostic::render(&diagnostic).as_bytes());
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
            Some(Location::new(input.to_path_buf(), invalid.into_valid())),
        ),
        LoadError::TooManyFiles => (
            DiagnosticKind::TooManySourceFiles,
            format!("too many source files to load input file {input_display}"),
            None,
        ),
    };
    Diagnostic::error(kind, message, location)
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
        UsageError::EmptyOutput => "the output path is empty".to_owned(),
        UsageError::EmptyInput => "the input path is empty".to_owned(),
        UsageError::ExtraInput(token) => format!(
            "unexpected argument '{}'; only one input path is accepted",
            token.display()
        ),
        UsageError::MissingInput => "missing input path".to_owned(),
        UsageError::MissingOutput => "missing output path; pass -o <OUTPUT>".to_owned(),
        UsageError::HelpWithArguments => "-h and --help take no other arguments".to_owned(),
    };
    let diagnostic = Diagnostic::error(DiagnosticKind::InvalidCommandLine, message, None);
    // The exit status reports the failure even when stderr is unwritable.
    let _ = writeln!(io::stderr(), "{}{USAGE}", diagnostic::render(&diagnostic));
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

    #[test]
    fn every_load_failure_and_the_not_implemented_outcome_have_distinct_kinds() {
        let input = Path::new("main.rs");
        let mut kinds: Vec<DiagnosticKind> = load_failures()
            .into_iter()
            .map(|error| load_failure(input, error).kind)
            .collect();
        let len = ByteOffset::try_from(13_usize).unwrap();
        kinds.push(not_implemented(input, len, Path::new("main")).kind);

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
    fn the_not_implemented_outcome_has_no_location() {
        let len = ByteOffset::try_from(13_usize).unwrap();
        let diagnostic = not_implemented(Path::new("main.rs"), len, Path::new("main"));
        assert!(diagnostic.location.is_none(), "{diagnostic:?}");
    }
}
