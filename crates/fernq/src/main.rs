//! The `fernq` compiler driver.
//!
//! The driver owns process entry, command-line parsing and validation, the
//! mapping from compilation outcomes to exit status, the selection of work for
//! a requested mode, and writing rendered output to process streams.
//!
//! The driver does not own Rust semantics, the source model contract, the
//! diagnostic structure, or the semantics of any compiler stage. Module
//! `source` owns the source model.
//!
//! No workspace crate depends on the driver.
//!
//! The command line is experimental and carries no compatibility promise. It
//! accepts `fernq <INPUT> -o <OUTPUT>` and `fernq -h` or `fernq --help`:
//!
//! - Help prints usage on stdout and exits with status 0.
//! - A valid invocation loads the input into the source model. An input that
//!   cannot be opened or read, is a directory, is longer than `u32::MAX`
//!   bytes, or is not UTF-8 is reported on stderr with status 1. A loaded
//!   input is reported on stderr as not compiled, because compilation is not
//!   implemented, with status 1. The driver writes no output.
//! - Any other command line is invalid. The driver reports it on stderr and
//!   exits with status 2.

mod cli;
mod source;

use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use cli::{Command, Invocation, USAGE, UsageError};
use source::{LoadError, SourceFile, SourceTable};

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
    match cli::parse(env::args_os().skip(1).collect()) {
        Ok(Command::Help) => help(),
        Ok(Command::Compile(invocation)) => compile(&invocation),
        Err(error) => usage_error(&error),
    }
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
    match sources.load(invocation.input()) {
        Ok(id) => unsupported(sources.get(id), invocation.output()),
        Err(error) => load_failure(invocation.input(), &error),
    }
}

fn load_failure(input: &Path, error: &LoadError) -> ExitCode {
    // The exit status reports the failure even when stderr is unwritable.
    let _ = writeln!(
        io::stderr(),
        "fernq: cannot load {}: {error}",
        input.display()
    );
    ExitCode::FAILURE
}

fn unsupported(input: &SourceFile, output: &Path) -> ExitCode {
    // The exit status reports the failure even when stderr is unwritable.
    let _ = writeln!(
        io::stderr(),
        "fernq: compilation is not implemented; {} ({} bytes) was not compiled and {} was not written",
        input.path().display(),
        input.len(),
        output.display(),
    );
    ExitCode::FAILURE
}

fn usage_error(error: &UsageError) -> ExitCode {
    // The exit status reports the failure even when stderr is unwritable.
    let _ = writeln!(io::stderr(), "fernq: {error}\n{USAGE}");
    ExitCode::from(2)
}
