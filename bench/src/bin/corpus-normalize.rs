//! Rewrites doc comments as plain comments of the same length.
//!
//! Usage: `corpus-normalize <directory>`. In every `.rs` file under the
//! directory, `///` and `//!` become `// `, and `/**` and `/*!` become `/* `,
//! where they open a doc comment in code: not in a string, character, or
//! comment, and `/**` not before `*` or `/`. Every byte offset is unchanged,
//! which the tool checks before it writes a file. The input must be valid
//! Rust source.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs, io};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [dir] = args.as_slice() else {
        eprintln!("usage: corpus-normalize <directory>");
        return ExitCode::from(2);
    };
    let mut files = Vec::new();
    if let Err(error) = collect(Path::new(dir), &mut files) {
        eprintln!("corpus-normalize: {dir}: {error}");
        return ExitCode::FAILURE;
    }
    files.sort();
    let (mut changed, mut comments) = (0, 0);
    for path in &files {
        let result = fs::read(path).and_then(|mut text| {
            let len = text.len();
            let count = normalize(&mut text);
            assert_eq!(text.len(), len, "normalization keeps the length");
            if count > 0 {
                fs::write(path, &text)?;
            }
            Ok(count)
        });
        match result {
            Ok(0) => {}
            Ok(count) => {
                changed += 1;
                comments += count;
            }
            Err(error) => {
                eprintln!("corpus-normalize: {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    println!(
        "corpus-normalize: {comments} doc comments rewritten in {changed} of {} files",
        files.len()
    );
    ExitCode::SUCCESS
}

/// Collects the `.rs` files under `dir`.
fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            collect(&path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// Rewrites the doc comments of `text` in place and returns their number.
fn normalize(text: &mut [u8]) -> usize {
    let at = |text: &[u8], i: usize| text.get(i).copied().unwrap_or(0);
    let mut count = 0;
    let mut i = 0;
    while i < text.len() {
        match (text[i], at(text, i + 1)) {
            (b'/', b'/') => {
                let third = at(text, i + 2);
                if (third == b'/' && at(text, i + 3) != b'/') || third == b'!' {
                    text[i + 2] = b' ';
                    count += 1;
                }
                i = end_of_line(text, i);
            }
            (b'/', b'*') => {
                let third = at(text, i + 2);
                if (third == b'*' && !matches!(at(text, i + 3), b'*' | b'/')) || third == b'!' {
                    text[i + 2] = b' ';
                    count += 1;
                }
                i = end_of_block_comment(text, i);
            }
            (b'"', _) => i = end_of_string(text, i + 1),
            (b'\'', _) => i = end_of_quote(text, i),
            (c, _) if c == b'_' || c.is_ascii_alphanumeric() || c >= 0x80 => {
                let start = i;
                while i < text.len()
                    && (text[i] == b'_' || text[i].is_ascii_alphanumeric() || text[i] >= 0x80)
                {
                    i += 1;
                }
                if matches!(&text[start..i], b"r" | b"br" | b"cr") {
                    i = end_of_raw_string(text, i);
                }
            }
            _ => i += 1,
        }
    }
    count
}

fn end_of_line(text: &[u8], i: usize) -> usize {
    text[i..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(text.len(), |n| i + n + 1)
}

/// The offset after the nested block comment that opens at `i`.
fn end_of_block_comment(text: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < text.len() {
        match (text[i], text.get(i + 1)) {
            (b'/', Some(b'*')) => {
                depth += 1;
                i += 2;
            }
            (b'*', Some(b'/')) => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    text.len()
}

/// The offset after the closing quote of a string whose body starts at `i`.
fn end_of_string(text: &[u8], mut i: usize) -> usize {
    while i < text.len() {
        match text[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    text.len()
}

/// The offset after a character literal at `i`, or after the quote of a
/// lifetime or label.
fn end_of_quote(text: &[u8], i: usize) -> usize {
    match text.get(i + 1) {
        Some(b'\\') => {
            let rest = text.get(i + 3..).unwrap_or_default();
            let close = rest.iter().position(|&b| b == b'\'');
            close.map_or(text.len(), |n| i + 3 + n + 1)
        }
        Some(&lead) => {
            let width = match lead {
                0x00..=0x7F => 1,
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                _ => 4,
            };
            if text.get(i + 1 + width) == Some(&b'\'') {
                i + 2 + width
            } else {
                i + 1
            }
        }
        None => text.len(),
    }
}

/// The offset after a raw string whose prefix ends at `i`, or `i` when no
/// raw string follows.
fn end_of_raw_string(text: &[u8], i: usize) -> usize {
    let pounds = text[i..].iter().take_while(|&&b| b == b'#').count();
    if text.get(i + pounds) != Some(&b'"') {
        return i;
    }
    let mut j = i + pounds + 1;
    while j < text.len() {
        if text[j] == b'"'
            && text[j + 1..]
                .iter()
                .take(pounds)
                .filter(|&&b| b == b'#')
                .count()
                == pounds
        {
            return j + 1 + pounds;
        }
        j += 1;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(text: &str) -> (String, usize) {
        let mut bytes = text.as_bytes().to_vec();
        let count = normalize(&mut bytes);
        (String::from_utf8(bytes).unwrap(), count)
    }

    #[test]
    fn rewrites_the_four_doc_comment_forms() {
        assert_eq!(
            normalized("/// a\n//! b\n"),
            ("//  a\n//  b\n".to_owned(), 2)
        );
        assert_eq!(
            normalized("/** a */ /*! b */"),
            ("/*  a */ /*  b */".to_owned(), 2)
        );
        assert_eq!(normalized("///"), ("// ".to_owned(), 1));
    }

    #[test]
    fn keeps_comments_that_are_not_doc_comments() {
        for text in [
            "//// a\n",
            "// a /// b\n",
            "/**/",
            "/***/",
            "/*** a */",
            "/* /** a */ */",
        ] {
            assert_eq!(normalized(text), (text.to_owned(), 0), "{text:?}");
        }
    }

    #[test]
    fn keeps_literals() {
        for text in [
            "\"/// a\"",
            "\"\\\" /// a\"",
            "r#\"\" /// \"#",
            "br\"//!\"",
            "'\"' \"//!\"",
            "'\\'' \"///\"",
            "'é' \"/**\"",
        ] {
            assert_eq!(normalized(text), (text.to_owned(), 0), "{text:?}");
        }
    }

    #[test]
    fn finds_doc_comments_after_lifetimes_and_raw_identifiers() {
        assert_eq!(
            normalized("fn f<'a>(r#x: &'a u8) {} /// d\n"),
            ("fn f<'a>(r#x: &'a u8) {} //  d\n".to_owned(), 1)
        );
    }
}
