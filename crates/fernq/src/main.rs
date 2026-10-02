//! The `fernq` compiler driver.
//!
//! The driver owns process entry, command-line parsing and validation, the
//! mapping from compilation outcomes to exit status, the selection of work for
//! a requested mode, and writing rendered output to process streams.
//!
//! The driver does not own Rust semantics, the source model contract, the
//! diagnostic structure, or the semantics of any compiler stage.
//!
//! No workspace crate depends on the driver.
//!
//! The driver currently implements no compiler functionality. It reports that
//! state on stderr and exits with a failure status.

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    // The exit status reports the failure even when stderr is unwritable.
    let _ = writeln!(
        io::stderr(),
        "fernq: no compiler functionality is implemented"
    );
    ExitCode::FAILURE
}
