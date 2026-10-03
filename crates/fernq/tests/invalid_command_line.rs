//! An invalid command line exits with status 2, writes nothing to stdout, and
//! reports a diagnostic of kind `invalid-command-line` on stderr.

use std::process::Command;

const CASES: &[(&str, &[&str])] = &[
    ("no arguments", &[]),
    (
        "unknown option",
        &["main.rs", "-o", "main", "--edition", "2024", "--verbose"],
    ),
    ("-o without a value", &["main.rs", "-o"]),
    ("-o twice", &["main.rs", "-o", "a", "-o", "b"]),
    ("two inputs", &["a.rs", "b.rs", "-o", "main"]),
    ("input without -o", &["main.rs"]),
    ("help with other arguments", &["-h", "main.rs"]),
    ("missing --edition", &["main.rs", "-o", "main"]),
    (
        "--edition twice",
        &[
            "main.rs",
            "-o",
            "main",
            "--edition",
            "2021",
            "--edition",
            "2024",
        ],
    ),
    (
        "--edition without a value",
        &["main.rs", "-o", "main", "--edition"],
    ),
    (
        "unknown edition",
        &["main.rs", "-o", "main", "--edition", "2027"],
    ),
    ("empty edition", &["main.rs", "-o", "main", "--edition", ""]),
    (
        "attached edition value",
        &["main.rs", "-o", "main", "--edition=2024"],
    ),
];

const HEADER: &str = "error[invalid-command-line]:";

#[test]
fn every_invalid_command_line_exits_with_status_2() {
    let mut failures = Vec::new();
    for (case, args) in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
            .args(*args)
            .output()
            .expect("the fernq binary runs");

        if !(output.status.code() == Some(2)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr).contains(HEADER))
        {
            failures.push(format!(
                "{case} {args:?}: expected status 2, empty stdout, {HEADER} on stderr; \
                 observed exit code {:?}, stdout {:?}, stderr {:?}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} invalid command line failure(s):\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
