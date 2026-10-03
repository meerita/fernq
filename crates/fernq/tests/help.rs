//! `-h` and `--help` print usage on stdout and exit with status 0.

use std::process::Command;

#[test]
fn both_help_spellings_print_usage_and_succeed() {
    for flag in ["-h", "--help"] {
        let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
            .arg(flag)
            .output()
            .expect("the fernq binary runs");

        assert!(
            output.status.code() == Some(0)
                && !output.stdout.is_empty()
                && output.stderr.is_empty(),
            "{flag}: expected status 0, non-empty stdout, empty stderr; \
             observed exit code {:?}, stdout {:?}, stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
