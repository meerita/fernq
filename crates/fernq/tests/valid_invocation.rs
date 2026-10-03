//! A valid invocation exits with status 1 and writes no output file, whether
//! its input loads or not, because compilation is not implemented. The
//! diagnostic kind on stderr tells the two outcomes apart.

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
    assert_not_compiled(
        input.as_os_str(),
        &dir.join("main"),
        "compilation-not-implemented",
    );
}

/// Integer literals and punctuation lex, so the program reaches compilation.
#[test]
fn an_arithmetic_program_is_not_compiled() {
    let dir = clear_test_dir("an_arithmetic_program_is_not_compiled");
    let input = dir.join("main.rs");
    fs::write(&input, "fn main() {\n    let x: u32 = 1 + 2 * 3;\n}\n")
        .expect("the input file is written");
    assert_not_compiled(
        input.as_os_str(),
        &dir.join("main"),
        "compilation-not-implemented",
    );
}

#[test]
fn missing_input_is_not_found() {
    let dir = clear_test_dir("missing_input_is_not_found");
    assert_not_compiled(
        dir.join("missing.rs").as_os_str(),
        &dir.join("main"),
        "input-not-found",
    );
}

#[cfg(unix)]
#[test]
fn missing_non_utf8_input_path_is_not_found() {
    use std::os::unix::ffi::OsStrExt;

    let dir = clear_test_dir("missing_non_utf8_input_path_is_not_found");
    let input = dir.join(OsStr::from_bytes(b"in\xffput.rs"));
    assert_not_compiled(input.as_os_str(), &dir.join("main"), "input-not-found");
}

/// Each accepted edition, before and after the paths, reaches compilation.
#[test]
fn every_edition_in_any_position_reaches_compilation() {
    let dir = clear_test_dir("every_edition_in_any_position_reaches_compilation");
    let input = dir.join("main.rs");
    fs::write(&input, "fn main() {}\n").expect("the input file is written");
    let output_path = dir.join("main");
    for edition in ["2015", "2018", "2021", "2024"] {
        let edition_first = [
            OsStr::new("--edition"),
            OsStr::new(edition),
            input.as_os_str(),
            OsStr::new("-o"),
            output_path.as_os_str(),
        ];
        let edition_last = [
            input.as_os_str(),
            OsStr::new("-o"),
            output_path.as_os_str(),
            OsStr::new("--edition"),
            OsStr::new(edition),
        ];
        for args in [edition_first, edition_last] {
            assert_status_1(&args, &output_path, "compilation-not-implemented");
        }
    }
}

/// Runs `fernq <input> -o <output> --edition 2024` and asserts status 1,
/// empty stdout, a diagnostic of kind `kind` on stderr, and no file at
/// `output`.
fn assert_not_compiled(input: &OsStr, output_path: &Path, kind: &str) {
    let args = [
        input,
        OsStr::new("-o"),
        output_path.as_os_str(),
        OsStr::new("--edition"),
        OsStr::new("2024"),
    ];
    assert_status_1(&args, output_path, kind);
}

/// Runs `fernq` with `args` and asserts status 1, empty stdout, a diagnostic
/// of kind `kind` on stderr, and no file at `output_path`.
fn assert_status_1(args: &[&OsStr], output_path: &Path, kind: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
        .args(args)
        .output()
        .expect("the fernq binary runs");

    let header = format!("error[{kind}]:");
    assert!(
        output.status.code() == Some(1)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr).contains(&header),
        "{args:?}: expected status 1, empty stdout, {header} on stderr; \
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
