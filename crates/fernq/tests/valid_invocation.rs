//! A valid invocation exits with status 1 and writes no output file, whether
//! its input loads or not, because compilation is not implemented.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn existing_input_is_not_compiled() {
    let dir = clear_test_dir("existing_input_is_not_compiled");
    let input = dir.join("main.rs");
    fs::write(&input, "fn main() {}\n").expect("the input file is written");
    assert_not_compiled(input.as_os_str(), &dir.join("main"));
}

#[test]
fn missing_input_is_not_compiled() {
    let dir = clear_test_dir("missing_input_is_not_compiled");
    assert_not_compiled(dir.join("missing.rs").as_os_str(), &dir.join("main"));
}

#[cfg(unix)]
#[test]
fn non_utf8_input_path_is_not_compiled() {
    use std::os::unix::ffi::OsStrExt;

    let dir = clear_test_dir("non_utf8_input_path_is_not_compiled");
    let input = dir.join(OsStr::from_bytes(b"in\xffput.rs"));
    assert_not_compiled(input.as_os_str(), &dir.join("main"));
}

/// Runs `fernq <input> -o <output>` and asserts status 1, empty stdout,
/// non-empty stderr, and no file at `output`.
fn assert_not_compiled(input: &OsStr, output_path: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
        .arg(input)
        .arg("-o")
        .arg(output_path)
        .output()
        .expect("the fernq binary runs");

    assert!(
        output.status.code() == Some(1) && output.stdout.is_empty() && !output.stderr.is_empty(),
        "expected status 1, empty stdout, non-empty stderr; \
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
