//! The `fernq` compiler.
//!
//! The library holds the whole compiler; the `fernq` binary only calls
//! [`run`]. It is not a stable API: no item is promised to external users.
//!
//! Module `driver` owns the process: the command line, the compilation, and
//! the diagnostics it prints. Module `cli` owns the command-line grammar.
//! Module `source` owns the source model. Module `edition` owns the edition.
//! Module `lexer` owns tokens and lexical errors. Module `unicode` owns the
//! Unicode character data and its version. Module `diagnostic` owns the
//! diagnostic structure and its rendering. With the Cargo feature `fuzzing`,
//! module `fuzz` gives the fuzz target in `fuzz/` its entry.
//!
//! `docs/compiler-session.md` owns the session contract. `docs/cli.md` owns
//! the command-line reference.

mod cli;
mod diagnostic;
mod driver;
mod edition;
#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub mod fuzz;
mod lexer;
mod source;
mod unicode;

use std::process::ExitCode;

/// Runs one `fernq` process and returns its exit status. The `fernq` binary
/// calls it from `main`; it is not an API.
#[doc(hidden)]
pub fn run() -> ExitCode {
    driver::run()
}
