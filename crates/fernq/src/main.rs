//! The `fernq` binary. The compiler is the `fernq` library; this file is only
//! the process entry.

use std::process::ExitCode;

fn main() -> ExitCode {
    fernq::run()
}
