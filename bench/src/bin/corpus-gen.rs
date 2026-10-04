//! Writes the synthetic workloads of the lexer benchmark.
//!
//! Usage: `corpus-gen <directory>`. Writes ten workload classes at five
//! sizes, `<class>-<size>.rs`, as edition 2024 source that lexes to the end of
//! file. Each file is a sequence of fragments from the grammar of its class,
//! each fragment ending in whitespace, padded with newlines to its exact size.
//! The output depends only on [`SEED`]: it is byte-identical across runs and
//! hosts. Prints the seed on success.

use std::path::Path;
use std::process::ExitCode;
use std::{env, fs};

/// The seed of every workload. Changing it changes the corpus.
const SEED: u64 = 0x5EED_2026_1004_0031;

const SIZES: [(&str, usize); 5] = [
    ("1k", 1 << 10),
    ("10k", 10 << 10),
    ("100k", 100 << 10),
    ("1m", 1 << 20),
    ("16m", 16 << 20),
];

/// Appends one fragment of a workload class, ending in whitespace.
type Fragment = fn(&mut Rng, &mut String);

/// The position of a class selects its random stream: a new class goes last,
/// so the bytes of the other classes stay the same.
const CLASSES: [(&str, Fragment); 10] = [
    ("identifiers", identifiers),
    ("keywords", keywords),
    ("delimiters", delimiters),
    ("whitespace", whitespace),
    ("line-comments", line_comments),
    ("punctuation", punctuation),
    ("strings", strings),
    ("raw-strings", raw_strings),
    ("non-ascii-identifiers", non_ascii_identifiers),
    ("block-comments", block_comments),
];

/// The strict and reserved keywords of edition 2024.
const KEYWORDS: [&str; 53] = [
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "_", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// The punctuation tokens of the Rust Reference other than the delimiters.
const PUNCTUATION: [&str; 46] = [
    "...", "..=", "<<=", ">>=", "!=", "%=", "&&", "&=", "*=", "+=", "-=", "->", "..", "/=", "::",
    "<-", "<<", "<=", "==", "=>", ">=", ">>", "^=", "|=", "||", "!", "#", "$", "%", "&", "*", "+",
    ",", "-", ".", "/", ":", ";", "<", "=", ">", "?", "@", "^", "|", "~",
];

const WORDS: [&str; 16] = [
    "the", "lexer", "reads", "one", "token", "at", "a", "time", "from", "source", "text", "and",
    "keeps", "its", "span", "offsets",
];

/// Ranges of `XID_Start` code points in NFC, assigned since Unicode 3.1.
const NON_ASCII_START: [(u32, u32); 7] = [
    (0x00E0, 0x00F6),   // Latin-1 lowercase letters
    (0x00F8, 0x00FF),   // Latin-1 lowercase letters
    (0x03B1, 0x03C9),   // Greek lowercase letters
    (0x0430, 0x044F),   // Cyrillic lowercase letters
    (0x4E00, 0x9FA5),   // CJK unified ideographs
    (0xAC00, 0xD7A3),   // Hangul syllables
    (0x20000, 0x2A6D6), // CJK unified ideographs extension B
];

/// A SplitMix64 generator: deterministic and platform-independent.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, for `n` far below `u64::MAX`.
    fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).expect("a bound fits a u64");
        usize::try_from(self.next() % n).expect("a value below a usize bound fits a usize")
    }

    /// A number in `lo..=hi`.
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [dir] = args.as_slice() else {
        eprintln!("usage: corpus-gen <directory>");
        return ExitCode::from(2);
    };
    let dir = Path::new(dir);
    if let Err(error) = fs::create_dir_all(dir) {
        eprintln!("corpus-gen: {}: {error}", dir.display());
        return ExitCode::FAILURE;
    }
    for (class_index, &(class, fragment)) in CLASSES.iter().enumerate() {
        for (size_index, &(size_name, size)) in SIZES.iter().enumerate() {
            let stream = u64::try_from(class_index * SIZES.len() + size_index)
                .expect("a stream index fits a u64");
            let mut rng = Rng(SEED ^ stream.wrapping_mul(0xD1B5_4A32_D192_ED03));
            let text = workload(&mut rng, fragment, size);
            let path = dir.join(format!("{class}-{size_name}.rs"));
            if let Err(error) = fs::write(&path, text) {
                eprintln!("corpus-gen: {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    println!("{SEED:#018x}");
    ExitCode::SUCCESS
}

/// Appends fragments while they fit in `size` bytes, then pads with newlines.
fn workload(rng: &mut Rng, fragment: Fragment, size: usize) -> String {
    let mut text = String::with_capacity(size);
    let mut next = String::new();
    loop {
        next.clear();
        fragment(rng, &mut next);
        if text.len() + next.len() > size {
            break;
        }
        text.push_str(&next);
    }
    while text.len() < size {
        text.push('\n');
    }
    text
}

/// An ASCII identifier that is not a keyword of edition 2024.
fn identifier(rng: &mut Rng, out: &mut String) {
    const START: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";
    const CONTINUE: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
    let start = out.len();
    out.push(char::from(*rng.pick(START)));
    for _ in 1..rng.range(1, 12) {
        out.push(char::from(*rng.pick(CONTINUE)));
    }
    if KEYWORDS.contains(&&out[start..]) {
        out.push_str("_x");
    }
}

fn identifiers(rng: &mut Rng, out: &mut String) {
    for _ in 0..rng.range(6, 10) {
        identifier(rng, out);
        out.push_str(rng.pick(&[" ", " ", " ", ", ", "."]));
    }
    out.push('\n');
}

fn keywords(rng: &mut Rng, out: &mut String) {
    for _ in 0..rng.range(6, 10) {
        out.push_str(rng.pick(&KEYWORDS));
        out.push(' ');
    }
    out.push('\n');
}

/// One balanced group of nested delimiters, up to 16 deep, with sparse atoms.
fn delimiters(rng: &mut Rng, out: &mut String) {
    let mut open: Vec<char> = Vec::new();
    let start = out.len();
    loop {
        let len = out.len() - start;
        let choice = rng.below(8);
        if open.is_empty() && len > 0 {
            break;
        } else if len >= 64 || (choice < 3 && !open.is_empty()) {
            out.push(open.pop().expect("a group is open"));
        } else if choice < 7 && open.len() < 16 {
            let (lhs, rhs) = *rng.pick(&[('(', ')'), ('[', ']'), ('{', '}')]);
            out.push(lhs);
            open.push(rhs);
        } else {
            out.push_str(rng.pick(&["x", ",", " ", "x,"]));
        }
    }
    out.push('\n');
}

fn whitespace(rng: &mut Rng, out: &mut String) {
    for _ in 0..rng.range(1, 4) {
        match rng.below(5) {
            0 => out.push_str(&" ".repeat(rng.range(1, 40))),
            1 => out.push_str(&"\t".repeat(rng.range(1, 8))),
            2 => out.push_str(&"\n".repeat(rng.range(1, 4))),
            3 => out.push_str(&"\r\n".repeat(rng.range(1, 3))),
            _ => out.push_str(&"    ".repeat(rng.range(1, 6))),
        }
    }
    out.push_str(rng.pick(&["x", ";", "x;", "{", "}"]));
    out.push('\n');
}

/// Between `lo` and `hi` words, each after a space.
fn words(rng: &mut Rng, out: &mut String, lo: usize, hi: usize) {
    for _ in 0..rng.range(lo, hi) {
        out.push(' ');
        out.push_str(rng.pick(&WORDS));
    }
}

/// A block comment nested up to `depth` more levels. It never opens with
/// `/**` or `/*!`, so it is not a doc comment.
fn block_comment(rng: &mut Rng, out: &mut String, depth: usize) {
    out.push_str("/*");
    words(rng, out, 1, 6);
    if depth > 0 && rng.one_in(2) {
        out.push(' ');
        block_comment(rng, out, depth - 1);
    }
    if rng.one_in(3) {
        out.push('\n');
    }
    words(rng, out, 0, 4);
    out.push_str(" */");
}

/// An indented line comment, or now and then a short statement between
/// comments.
fn line_comments(rng: &mut Rng, out: &mut String) {
    out.push_str(rng.pick(&["", "    ", "    ", "        ", "            "]));
    match rng.below(8) {
        0 => out.push_str("x;"),
        1 => {
            // Four or more slashes open a line comment that is not a doc comment.
            out.push_str(&"/".repeat(rng.range(4, 12)));
            words(rng, out, 0, 4);
        }
        _ => {
            out.push_str("//");
            words(rng, out, 3, 12);
        }
    }
    out.push('\n');
}

fn block_comments(rng: &mut Rng, out: &mut String) {
    match rng.below(4) {
        0..=2 => block_comment(rng, out, 3),
        _ => out.push_str(rng.pick(&["/**/", "/***/", "x; /* */"])),
    }
    out.push('\n');
}

/// Punctuation tokens, glued or separated by one space.
fn punctuation(rng: &mut Rng, out: &mut String) {
    let mut previous = "";
    for _ in 0..rng.range(8, 16) {
        let token = *rng.pick(&PUNCTUATION);
        // `/` before `/` or `*` opens a comment; `##` is reserved from 2024.
        let must_split = (previous.ends_with('/') && token.starts_with(['/', '*']))
            || (previous.ends_with('#') && token.starts_with('#'));
        if must_split || rng.one_in(2) {
            out.push(' ');
        }
        out.push_str(token);
        previous = token;
    }
    out.push('\n');
}

/// The body of a string literal; `kind` is `""`, `"b"`, or `"c"`.
fn string_body(rng: &mut Rng, out: &mut String, kind: &str) {
    const COMMON: [&str; 12] = [
        "text", " ", "lexer", "\\n", "\\t", "\\r", "\\\\", "\\\"", "\\'", "'", "\\\n    ", "\n",
    ];
    const STRING: [&str; 6] = ["\\0", "\\x41", "\\x7F", "\\u{1F600}", "\\u{E9}", "é日本"];
    const BYTE: [&str; 3] = ["\\0", "\\x00", "\\xFF"];
    const C: [&str; 4] = ["\\x7F", "\\xFF", "\\u{E9}", "é日本"];
    for _ in 0..rng.range(1, 8) {
        let piece = match (kind, rng.one_in(3)) {
            ("", true) => rng.pick(&STRING),
            ("b", true) => rng.pick(&BYTE),
            ("c", true) => rng.pick(&C),
            _ => rng.pick(&COMMON),
        };
        out.push_str(piece);
    }
}

fn strings(rng: &mut Rng, out: &mut String) {
    for _ in 0..rng.range(2, 4) {
        match rng.below(6) {
            0 | 1 => {
                out.push('"');
                string_body(rng, out, "");
                out.push('"');
            }
            2 => {
                out.push_str("b\"");
                string_body(rng, out, "b");
                out.push('"');
            }
            3 => {
                out.push_str("c\"");
                string_body(rng, out, "c");
                out.push('"');
            }
            4 => out.push_str(rng.pick(&[
                "'a'",
                "'\\n'",
                "'\\''",
                "'\"'",
                "'\\x7F'",
                "'\\u{10FFFF}'",
                "'é'",
                "'日'",
            ])),
            _ => out.push_str(rng.pick(&["b'a'", "b'\\xFF'", "b'\\''", "b'\"'", "b'\\0'"])),
        }
        out.push_str(", ");
    }
    out.push('\n');
}

/// A raw string literal whose content holds near misses of its terminator.
fn raw_strings(rng: &mut Rng, out: &mut String) {
    let prefix = *rng.pick(&["r", "r", "br", "cr"]);
    let pounds = match rng.below(16) {
        0..=3 => 0,
        4..=12 => rng.range(1, 4),
        13 | 14 => rng.range(5, 16),
        _ => rng.range(17, 64),
    };
    let fence = "#".repeat(pounds);
    out.push_str(prefix);
    out.push_str(&fence);
    out.push('"');
    for _ in 0..rng.range(1, 8) {
        match rng.below(6) {
            0 => out.push_str(rng.pick(&["text", "lexer", " ", "\\d+", "\\n"])),
            1 => out.push('\n'),
            2 if prefix != "br" => out.push_str("é日本"),
            3 | 4 if pounds > 0 => {
                // A quote and fewer pounds than the fence does not end the literal.
                out.push('"');
                out.push_str(&"#".repeat(rng.below(pounds)));
                out.push(' ');
            }
            _ => out.push_str("raw"),
        }
    }
    out.push('"');
    out.push_str(&fence);
    out.push('\n');
}

fn non_ascii_char(rng: &mut Rng) -> char {
    let (lo, hi) = *rng.pick(&NON_ASCII_START);
    let offset = u32::try_from(rng.below(usize::try_from(hi - lo + 1).expect("fits")))
        .expect("an offset in a code point range fits a u32");
    char::from_u32(lo + offset).expect("the ranges hold no surrogate")
}

fn non_ascii_identifiers(rng: &mut Rng, out: &mut String) {
    for _ in 0..rng.range(4, 8) {
        out.push(non_ascii_char(rng));
        for _ in 0..rng.range(0, 6) {
            if rng.one_in(4) {
                out.push(char::from(*rng.pick(b"abcxyz_0129")));
            } else {
                out.push(non_ascii_char(rng));
            }
        }
        out.push_str(rng.pick(&[" ", " ", "::", ".", ", "]));
    }
    out.push('\n');
}
