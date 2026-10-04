//! Checks that every corpus file lexes to the end of file with Fernq.
//!
//! Usage: `corpus-check <corpus directory>`. Reads `MANIFEST` in the
//! directory, checks that each listed file has its listed size, and lexes it
//! with `fernq::bench::lex` in its listed edition. Prints the files, bytes,
//! and tokens of each corpus class, and fails when a file is missing, has
//! another size, or does not lex to the end of file.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;
use std::{env, fs};

#[derive(Default)]
struct Totals {
    files: usize,
    bytes: usize,
    tokens: usize,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [dir] = args.as_slice() else {
        eprintln!("usage: corpus-check <corpus directory>");
        return ExitCode::from(2);
    };
    let dir = Path::new(dir);
    let manifest = match fs::read_to_string(dir.join("MANIFEST")) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("corpus-check: {}: {error}", dir.join("MANIFEST").display());
            return ExitCode::FAILURE;
        }
    };
    let mut classes: BTreeMap<String, Totals> = BTreeMap::new();
    let mut failures = 0;
    for line in manifest.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [path, bytes, _sha256, edition, ..] = fields.as_slice() else {
            eprintln!("corpus-check: malformed manifest line: {line:?}");
            return ExitCode::FAILURE;
        };
        let (Ok(bytes), Ok(edition)) = (bytes.parse::<usize>(), edition.parse::<u16>()) else {
            eprintln!("corpus-check: malformed manifest line: {line:?}");
            return ExitCode::FAILURE;
        };
        let text = match fs::read_to_string(dir.join(path)) {
            Ok(text) if text.len() == bytes => text,
            Ok(text) => {
                eprintln!(
                    "corpus-check: {path}: {} bytes, manifest says {bytes}",
                    text.len()
                );
                failures += 1;
                continue;
            }
            Err(error) => {
                eprintln!("corpus-check: {path}: {error}");
                failures += 1;
                continue;
            }
        };
        let Some((tokens, _)) = fernq::bench::lex(&text, edition) else {
            eprintln!("corpus-check: {path}: does not lex to the end of file in edition {edition}");
            failures += 1;
            continue;
        };
        let totals = classes.entry(class(path)).or_default();
        totals.files += 1;
        totals.bytes += bytes;
        totals.tokens += tokens;
    }
    println!(
        "{:<40} {:>6} {:>12} {:>10}",
        "class", "files", "bytes", "tokens"
    );
    for (class, totals) in &classes {
        println!(
            "{class:<40} {:>6} {:>12} {:>10}",
            totals.files, totals.bytes, totals.tokens
        );
    }
    if failures > 0 {
        eprintln!("corpus-check: {failures} files failed");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// The class of a corpus path: `synthetic/<class>` without the size, or
/// `real/<source>`.
fn class(path: &str) -> String {
    let mut parts = path.split('/');
    match (parts.next(), parts.next()) {
        (Some("synthetic"), Some(file)) => {
            let stem = file.strip_suffix(".rs").unwrap_or(file);
            let class = stem.rsplit_once('-').map_or(stem, |(class, _)| class);
            format!("synthetic/{class}")
        }
        (Some(group), Some(source)) => format!("{group}/{source}"),
        _ => path.to_owned(),
    }
}
