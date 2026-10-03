//! Runs every compile fixture through the `fernq` binary.
//!
//! A fixture is one Rust source file at `tests/fixtures/<class>/<name>.rs`.
//! The class directory names whether the Rust compiler contract accepts the
//! fixture. The runner visits fixtures in sorted path order, collects every
//! mismatch and convention violation, and fails once with all of them. It
//! fails when no fixture exists.
//!
//! Each fixture runs as `fernq <fixture> -o <dir>/<fixture stem> --edition
//! 2024`, where `<dir>` is `CARGO_TARGET_TMPDIR/<test name>/`, cleared at
//! start.
//!
//! A fixture may start with the line `// expected-kind: <kind>`, which names
//! the diagnostic kind that Fernq reports today. Without it, the expected kind
//! is `compilation-not-implemented`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE_CLASSES: &[&str] = &["compile-fail"];

const TEST_NAME: &str = "every_compile_fixture_has_the_expected_outcome";

const DIRECTIVE: &str = "// expected-kind:";

const DEFAULT_KIND: &str = "compilation-not-implemented";

/// Every fixture expects status 1, empty stdout, a diagnostic of its expected
/// kind on stderr, and no file at the output path.
///
/// The runner identifies an outcome by its diagnostic kind and compares no
/// message wording.
#[test]
fn every_compile_fixture_has_the_expected_outcome() {
    let output_dir = clear_test_dir(TEST_NAME);
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let relative = |path: &Path| {
        path.strip_prefix(manifest_dir)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let mut failures = Vec::new();
    let mut fixture_count = 0;

    for entry in sorted_entries(&manifest_dir.join("tests/fixtures")) {
        if !entry.is_dir() {
            failures.push(format!(
                "{}: file directly in tests/fixtures/; place fixtures in a class directory",
                relative(&entry),
            ));
            continue;
        }
        let is_known_class = entry
            .file_name()
            .is_some_and(|name| FIXTURE_CLASSES.iter().any(|class| name == *class));
        if !is_known_class {
            failures.push(format!(
                "{}: unknown fixture class directory; known classes: {FIXTURE_CLASSES:?}",
                relative(&entry),
            ));
            continue;
        }

        for fixture in sorted_entries(&entry) {
            if !fixture.is_file() || fixture.extension().is_none_or(|ext| ext != "rs") {
                failures.push(format!(
                    "{}: not a `.rs` file in a fixture class directory",
                    relative(&fixture),
                ));
                continue;
            }
            fixture_count += 1;
            let kind = match expected_kind(&fixture) {
                Ok(kind) => kind,
                Err(reason) => {
                    failures.push(format!("{}: {reason}", relative(&fixture)));
                    continue;
                }
            };
            let header = format!("error[{kind}]:");

            let output_path = output_dir.join(
                fixture
                    .file_stem()
                    .expect("a `.rs` fixture path has a file stem"),
            );
            let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
                .arg(&fixture)
                .arg("-o")
                .arg(&output_path)
                .args(["--edition", "2024"])
                .output()
                .expect("the fernq binary runs");
            if !has_outcome(&output, &output_path, &header) {
                failures.push(format!(
                    "{}: expected exit code 1, empty stdout, \
                     {header} on stderr, no file at {}; \
                     observed exit code {:?}, output file present {}, stdout {:?}, stderr {:?}",
                    relative(&fixture),
                    output_path.display(),
                    output.status.code(),
                    output_path.exists(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                ));
            }
        }
    }

    if fixture_count == 0 {
        failures.push("no compile fixture exists under tests/fixtures/".to_owned());
    }
    assert!(
        failures.is_empty(),
        "{} compile fixture failure(s):\n{}",
        failures.len(),
        failures.join("\n"),
    );
}

/// Returns the kind named by the `// expected-kind:` directive on the first
/// line of `fixture`, or the default kind when the first line is not one.
fn expected_kind(fixture: &Path) -> Result<String, String> {
    let text = fs::read(fixture).map_err(|error| format!("cannot read the fixture: {error}"))?;
    let first_line = text.split(|&byte| byte == b'\n').next().unwrap_or_default();
    let Some(kind) = first_line.strip_prefix(DIRECTIVE.as_bytes()) else {
        return Ok(DEFAULT_KIND.to_owned());
    };
    let kind = String::from_utf8_lossy(kind).trim().to_owned();
    let is_kind_name = !kind.is_empty()
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !is_kind_name {
        return Err(format!("{DIRECTIVE} names no diagnostic kind: {kind:?}"));
    }
    Ok(kind)
}

fn has_outcome(output: &Output, output_path: &Path, header: &str) -> bool {
    output.status.code() == Some(1)
        && output.stdout.is_empty()
        && String::from_utf8_lossy(&output.stderr).contains(header)
        && !output_path.exists()
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

/// Returns the entries of `dir` in sorted path order, or no entries when
/// `dir` does not exist.
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("cannot read {}: {error}", dir.display()),
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()));
    paths.sort();
    paths
}
