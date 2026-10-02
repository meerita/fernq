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
