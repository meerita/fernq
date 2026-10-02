//! The outcome of the `fernq` binary while it implements no compiler
//! functionality.
//!
//! The test documents the current outcome. The outcome is not a stable
//! interface.

use std::process::Command;

const EXPECTED_STDERR: &str = "fernq: no compiler functionality is implemented\n";

#[test]
fn reports_no_compiler_functionality_and_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
        .output()
        .expect("the fernq binary runs");

    assert!(
        !output.status.success()
            && output.stdout.is_empty()
            && output.stderr == EXPECTED_STDERR.as_bytes(),
        "unexpected outcome: exit code {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
