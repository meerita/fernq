//! An input that the lexer rejects is reported on stderr with status 1, a
//! diagnostic whose kind names the class of the rejection, and the location
//! of the rejected input. No output file is written.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The backtick starts no Rust token.
#[test]
fn invalid_input_is_a_lexical_error_at_its_location() {
    assert_rejected_at(
        "invalid_input_is_a_lexical_error_at_its_location",
        "fn main() {\n    `\n}\n",
        "lexical-error",
        "2:5",
    );
}

/// `é` starts a non-ASCII identifier, which the lexer does not support. The
/// column counts the multibyte character in the comment as one.
#[test]
fn unsupported_input_is_unsupported_syntax_at_its_location() {
    assert_rejected_at(
        "unsupported_input_is_unsupported_syntax_at_its_location",
        "fn main() {\n    /* é */ let x = é;\n}\n",
        "unsupported-syntax",
        "2:21",
    );
}

/// Writes `text` as the input, runs `fernq` with edition 2024, and asserts
/// status 1, empty stdout, a diagnostic of kind `kind` at
/// `<input>:<line_column>` on stderr, and no output file.
fn assert_rejected_at(test_name: &str, text: &str, kind: &str, line_column: &str) {
    let dir = clear_test_dir(test_name);
    let input = dir.join("main.rs");
    fs::write(&input, text).expect("the input file is written");
    let output_path = dir.join("main");
    let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
        .arg(&input)
        .arg("-o")
        .arg(&output_path)
        .args(["--edition", "2024"])
        .output()
        .expect("the fernq binary runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let header = format!("error[{kind}]:");
    let location = format!(" --> {}:{line_column}\n", input.display());
    assert!(
        output.status.code() == Some(1)
            && output.stdout.is_empty()
            && stderr.contains(&header)
            && stderr.contains(&location),
        "expected status 1, empty stdout, {header} and {location:?} on stderr; \
         observed exit code {:?}, stdout {:?}, stderr {stderr:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
    );
    assert!(
        !output_path.exists(),
        "{}: the output file exists",
        output_path.display(),
    );
}

/// Returns an empty `CARGO_TARGET_TMPDIR/<test_name>/` directory.
fn clear_test_dir(test_name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test_name);
    match fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => panic!("cannot clear {}: {error}", dir.display()),
    }
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("cannot create {}: {error}", dir.display()));
    dir
}
