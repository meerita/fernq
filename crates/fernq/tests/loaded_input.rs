//! A loaded input is reported on stderr as not compiled, with a diagnostic of
//! kind `compilation-not-implemented` and status 1, and no output file is
//! written.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn empty_input_is_not_compiled() {
    let dir = clear_test_dir("empty_input_is_not_compiled");
    let input = dir.join("main.rs");
    fs::write(&input, "").expect("the input file is written");
    let output_path = dir.join("main");
    let output = fernq(&input, &output_path);
    assert_not_compiled(&output, &output_path);
}

#[test]
fn source_input_is_not_compiled() {
    let dir = clear_test_dir("source_input_is_not_compiled");
    let input = dir.join("main.rs");
    fs::write(&input, "fn main() {}\n").expect("the input file is written");
    let output_path = dir.join("main");
    let output = fernq(&input, &output_path);
    assert_not_compiled(&output, &output_path);
}

/// The input is a FIFO fed by a writer thread.
///
/// Opening a FIFO for writing blocks until a reader opens it, and the writer
/// finishes only after its bytes are read or the reader closes. A finished
/// writer shows that `fernq` opened and read the input.
#[cfg(unix)]
#[test]
fn fifo_input_is_read_and_not_compiled() {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let dir = clear_test_dir("fifo_input_is_read_and_not_compiled");
    let input = dir.join("main.rs");
    let status = Command::new("mkfifo")
        .arg(&input)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "mkfifo {}: {status}", input.display());

    let (finished, writer) = mpsc::channel();
    let fifo = input.clone();
    thread::spawn(move || {
        let _ = finished.send(fs::write(&fifo, "fn main() {}\n"));
    });
    let output_path = dir.join("main");
    let output = fernq(&input, &output_path);

    let written = writer.recv_timeout(Duration::from_secs(10));
    assert!(
        matches!(written, Ok(Ok(()))),
        "the FIFO writer did not finish, so fernq did not read the input; \
         writer {written:?}, observed exit code {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_not_compiled(&output, &output_path);
}

fn fernq(input: &Path, output_path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fernq"))
        .arg(input)
        .arg("-o")
        .arg(output_path)
        .args(["--edition", "2024"])
        .output()
        .expect("the fernq binary runs")
}

/// Asserts status 1, empty stdout, a `compilation-not-implemented`
/// diagnostic on stderr, and no file at `output_path`.
fn assert_not_compiled(output: &Output, output_path: &Path) {
    let header = "error[compilation-not-implemented]:";
    assert!(
        output.status.code() == Some(1)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr).contains(header),
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
