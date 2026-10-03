//! An input that cannot be loaded is reported on stderr as a diagnostic whose
//! kind names the failure, with status 1, and no output file is written.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn missing_input_is_rejected() {
    let dir = clear_test_dir("missing_input_is_rejected");
    let output_path = dir.join("main");
    let output = fernq(&dir.join("missing.rs"), &output_path);
    assert_rejected(&output, &output_path, "input-not-found");
}

#[test]
fn directory_input_is_rejected() {
    let dir = clear_test_dir("directory_input_is_rejected");
    let input = dir.join("input.rs");
    fs::create_dir(&input).expect("the input directory is created");
    let output_path = dir.join("main");
    let output = fernq(&input, &output_path);
    assert_rejected(&output, &output_path, "input-is-directory");
}

/// The first invalid byte follows a three-byte character on line 2, so its
/// column counts characters, not bytes.
#[test]
fn non_utf8_input_is_rejected_at_the_first_invalid_byte() {
    let dir = clear_test_dir("non_utf8_input_is_rejected_at_the_first_invalid_byte");
    let input = dir.join("main.rs");
    let text = ["fn main() {}\n// 中 ".as_bytes(), b"\xff\n"].concat();
    fs::write(&input, text).expect("the input file is written");
    let output_path = dir.join("main");
    let output = fernq(&input, &output_path);
    assert_rejected(&output, &output_path, "input-not-utf8");
    let location = format!("{}:2:6", input.display());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&location),
        "expected location {location} in stderr {:?}",
        String::from_utf8_lossy(&output.stderr),
    );
}

/// The input is a sparse file one byte longer than `u32::MAX` bytes, and the
/// command line raises the limit to `u32::MAX`, the representable maximum.
///
/// The test removes the file before asserting, so a failure does not leave
/// it behind: its apparent size is 4 GiB, and a tool that copies or scans the
/// target directory without sparse-file support would read all of it.
#[test]
fn oversized_input_is_rejected() {
    let dir = clear_test_dir("oversized_input_is_rejected");
    let input = dir.join("main.rs");
    File::create(&input)
        .and_then(|file| file.set_len(u64::from(u32::MAX) + 1))
        .expect("the sparse input file is created");
    let output_path = dir.join("main");
    let output = fernq_with(&input, &output_path, &["--max-input-bytes", "4294967295"]);
    fs::remove_file(&input).expect("the sparse input file is removed");
    assert_rejected(&output, &output_path, "input-too-large");
}

/// Without `--max-input-bytes`, a sparse file one byte over 128 MiB is
/// rejected from its metadata length, without reading it.
#[test]
fn input_over_the_default_limit_is_rejected() {
    let dir = clear_test_dir("input_over_the_default_limit_is_rejected");
    let input = dir.join("main.rs");
    File::create(&input)
        .and_then(|file| file.set_len(128 * 1024 * 1024 + 1))
        .expect("the sparse input file is created");
    let output_path = dir.join("main");
    let output = fernq(&input, &output_path);
    fs::remove_file(&input).expect("the sparse input file is removed");
    assert_rejected(&output, &output_path, "input-too-large");
}

/// A limit below the input size rejects it; a limit equal to it loads it.
#[test]
fn the_input_limit_is_inclusive() {
    let dir = clear_test_dir("the_input_limit_is_inclusive");
    let input = dir.join("main.rs");
    fs::write(&input, "fn main() {}\n").expect("the input file is written");
    let output_path = dir.join("main");
    let output = fernq_with(&input, &output_path, &["--max-input-bytes", "12"]);
    assert_rejected(&output, &output_path, "input-too-large");
    let output = fernq_with(&input, &output_path, &["--max-input-bytes", "13"]);
    assert_rejected(&output, &output_path, "compilation-not-implemented");
}

fn fernq(input: &Path, output_path: &Path) -> Output {
    fernq_with(input, output_path, &[])
}

/// Runs `fernq <input> -o <output_path> --edition 2024` followed by `extra`.
fn fernq_with(input: &Path, output_path: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fernq"))
        .arg(input)
        .arg("-o")
        .arg(output_path)
        .args(["--edition", "2024"])
        .args(extra)
        .output()
        .expect("the fernq binary runs")
}

/// Asserts status 1, empty stdout, a diagnostic of kind `kind` on stderr, and
/// no file at `output_path`.
fn assert_rejected(output: &Output, output_path: &Path, kind: &str) {
    let header = format!("error[{kind}]:");
    assert!(
        output.status.code() == Some(1)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr).contains(&header),
        "expected status 1, empty stdout, {header} on stderr; \
         observed exit code {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
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
