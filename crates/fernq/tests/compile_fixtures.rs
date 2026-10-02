//! Runs every compile fixture through the `fernq` binary.
//!
//! A fixture is one Rust source file at `tests/fixtures/<class>/<name>.rs`.
//! The class directory names the expected outcome. The runner visits fixtures
//! in sorted path order, collects every mismatch and convention violation,
//! and fails once with all of them. It fails when no fixture exists.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE_CLASSES: &[&str] = &["compile-fail"];

/// The stderr of the `unsupported` outcome.
///
/// Every fixture expects `unsupported` because the driver does not read its
/// input. A class expectation such as rejection is not checked.
const UNSUPPORTED_STDERR: &str = "fernq: no compiler functionality is implemented\n";

#[test]
fn every_compile_fixture_has_the_expected_outcome() {
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

            let output = Command::new(env!("CARGO_BIN_EXE_fernq"))
                .arg(&fixture)
                .output()
                .expect("the fernq binary runs");
            if !is_unsupported(&output) {
                failures.push(format!(
                    "{}: expected unsupported (failure status, empty stdout, stderr {UNSUPPORTED_STDERR:?}); \
                     observed exit code {:?}, stdout {:?}, stderr {:?}",
                    relative(&fixture),
                    output.status.code(),
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

fn is_unsupported(output: &Output) -> bool {
    !output.status.success()
        && output.stdout.is_empty()
        && output.stderr == UNSUPPORTED_STDERR.as_bytes()
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
