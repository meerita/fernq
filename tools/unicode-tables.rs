//! Generates `crates/fernq/src/unicode/tables.rs`, the `XID_Start` and
//! `XID_Continue` tables of the compiler, from `DerivedCoreProperties.txt` of
//! the Unicode Character Database.
//!
//! Usage: `unicode-tables <DerivedCoreProperties.txt> <output>`.
//!
//! The input is pinned: the generator refuses any file whose first line or
//! SHA-256 differs from the pinned values, and writes nothing. The output
//! depends on the input bytes only.

use std::cmp::Ordering;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const UNICODE_VERSION: (u8, u8, u8) = (17, 0, 0);

const SOURCE_URL: &str = "https://www.unicode.org/Public/17.0.0/ucd/DerivedCoreProperties.txt";

const SOURCE_SHA256: &str = "24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08";

const VERSION_LINE: &str = "# DerivedCoreProperties-17.0.0.txt";

/// The Unicode License v3 notice, which must appear with every copy of data
/// derived from the Unicode Character Database.
const LICENSE_NOTICE: &str = "\
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2026 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the \"Data Files\") or
software and any associated documentation (the \"Software\") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.";

/// An inclusive range of Unicode scalar values.
type Range = (char, char);

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: unicode-tables <DerivedCoreProperties.txt> <output>");
        return ExitCode::from(2);
    };
    match run(Path::new(input), Path::new(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("unicode-tables: {message}; nothing was written");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, output: &Path) -> Result<(), String> {
    let bytes =
        fs::read(input).map_err(|error| format!("cannot read {}: {error}", input.display()))?;
    let tables = generate(&bytes)?;
    fs::write(output, tables).map_err(|error| format!("cannot write {}: {error}", output.display()))
}

/// Returns the source of the tables module for `input`, or the first check
/// that `input` fails.
fn generate(input: &[u8]) -> Result<String, String> {
    check_version_line(input)?;
    check_sha256(input)?;
    let text =
        std::str::from_utf8(input).map_err(|error| format!("input is not UTF-8: {error}"))?;
    let start = property_ranges(text, "XID_Start")?;
    let continue_ = property_ranges(text, "XID_Continue")?;
    if !is_subset(&start, &continue_) {
        return Err("XID_Start is not a subset of XID_Continue".to_owned());
    }
    Ok(render(&start, &continue_))
}

fn check_version_line(input: &[u8]) -> Result<(), String> {
    let first_line = input
        .split(|&byte| byte == b'\n')
        .next()
        .unwrap_or_default();
    if first_line != VERSION_LINE.as_bytes() {
        return Err(format!(
            "version line check failed: the first line is {:?}, expected {VERSION_LINE:?}",
            String::from_utf8_lossy(first_line),
        ));
    }
    Ok(())
}

fn check_sha256(input: &[u8]) -> Result<(), String> {
    let digest = hex(&sha256(input));
    if digest != SOURCE_SHA256 {
        return Err(format!(
            "SHA-256 check failed: the input hash is {digest}, expected {SOURCE_SHA256}"
        ));
    }
    Ok(())
}

/// Returns the sorted, non-overlapping, non-adjacent ranges of the code
/// points that `text` lists for `property`.
fn property_ranges(text: &str, property: &str) -> Result<Vec<Range>, String> {
    let mut ranges = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let data = line.split('#').next().unwrap_or_default().trim();
        if data.is_empty() {
            continue;
        }
        let line_number = index + 1;
        let Some((code_points, name)) = data.split_once(';') else {
            return Err(format!("line {line_number}: no ';' in {line:?}"));
        };
        if name.trim() == property {
            let range = parse_range(code_points.trim())
                .map_err(|reason| format!("line {line_number}: {reason}"))?;
            ranges.push(range);
        }
    }
    if ranges.is_empty() {
        return Err(format!("the input lists no {property} code point"));
    }
    Ok(merge(ranges))
}

/// Parses `XXXX` or `XXXX..YYYY`, both ends Unicode scalar values, with no
/// surrogate between them.
fn parse_range(field: &str) -> Result<Range, String> {
    let (lo, hi) = field.split_once("..").unwrap_or((field, field));
    let (lo, hi) = (parse_code_point(lo)?, parse_code_point(hi)?);
    let (Some(lo_char), Some(hi_char)) = (char::from_u32(lo), char::from_u32(hi)) else {
        return Err(format!("{field:?} is not a range of Unicode scalar values"));
    };
    if lo > hi || (lo < 0xD800 && hi > 0xDFFF) {
        return Err(format!("{field:?} is not a range of Unicode scalar values"));
    }
    Ok((lo_char, hi_char))
}

fn parse_code_point(field: &str) -> Result<u32, String> {
    let is_hex =
        (4..=6).contains(&field.len()) && field.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !is_hex {
        return Err(format!("{field:?} is not a code point"));
    }
    u32::from_str_radix(field, 16).map_err(|error| format!("{field:?}: {error}"))
}

/// Sorts `ranges` and merges the ranges that overlap or touch.
fn merge(mut ranges: Vec<Range>) -> Vec<Range> {
    ranges.sort_unstable();
    let mut merged: Vec<Range> = Vec::with_capacity(ranges.len());
    for (lo, hi) in ranges {
        match merged.last_mut() {
            Some(last) if u32::from(lo) <= u32::from(last.1) + 1 => last.1 = last.1.max(hi),
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

/// Whether every range of `inner` lies in one range of `outer`. Both are
/// sorted, non-overlapping, and non-adjacent.
fn is_subset(inner: &[Range], outer: &[Range]) -> bool {
    inner.iter().all(|&(lo, hi)| {
        outer
            .binary_search_by(|&(outer_lo, outer_hi)| {
                if outer_hi < lo {
                    Ordering::Less
                } else if outer_lo > lo {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .is_ok_and(|index| outer[index].1 >= hi)
    })
}

fn render(start: &[Range], continue_: &[Range]) -> String {
    let (major, minor, update) = UNICODE_VERSION;
    let mut out = String::new();
    out.push_str("// This file is generated. Do not edit it.\n//\n");
    out.push_str("// Generator: tools/unicode-tables.rs\n");
    out.push_str("// Regenerate: make unicode-tables UCD=<path of DerivedCoreProperties.txt>\n");
    out.push_str("//\n");
    let _ = writeln!(
        out,
        "// Unicode Character Database {major}.{minor}.{update}"
    );
    let _ = writeln!(out, "// Source: {SOURCE_URL}");
    let _ = writeln!(out, "// SHA-256: {SOURCE_SHA256}");
    out.push_str("//\n");
    for line in LICENSE_NOTICE.lines() {
        if line.is_empty() {
            out.push_str("//\n");
        } else {
            let _ = writeln!(out, "// {line}");
        }
    }
    out.push('\n');
    out.push_str("/// The Unicode version of the tables.\n");
    let _ = writeln!(
        out,
        "pub(super) const UNICODE_VERSION: (u8, u8, u8) = ({major}, {minor}, {update});"
    );
    render_table(&mut out, "XID_START", "XID_Start", start);
    render_table(&mut out, "XID_CONTINUE", "XID_Continue", continue_);
    out
}

fn render_table(out: &mut String, name: &str, property: &str, ranges: &[Range]) {
    out.push('\n');
    let _ = writeln!(
        out,
        "/// The `{property}` code points: sorted, non-overlapping, non-adjacent\n\
         /// inclusive ranges."
    );
    let _ = writeln!(out, "pub(super) const {name}: &[(char, char)] = &[");
    for &(lo, hi) in ranges {
        let _ = writeln!(
            out,
            "    ('\\u{{{:X}}}', '\\u{{{:X}}}'),",
            u32::from(lo),
            u32::from(hi)
        );
    }
    out.push_str("];\n");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The SHA-256 digest of `message`, FIPS 180-4.
fn sha256(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding: 0x80, zeros to 56 mod 64, then the bit length as a 64-bit big-endian integer.
    let bit_len = u64::try_from(message.len())
        .expect("a slice length fits in u64")
        .wrapping_mul(8);
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (word, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for (k, w) in K.into_iter().zip(w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k)
                .wrapping_add(w);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (word, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *word = word.wrapping_add(value);
        }
    }

    let mut digest = [0u8; 32];
    for (bytes, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *bytes = word.to_be_bytes();
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_fips_180_4_examples() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn parses_single_code_points_and_ranges() {
        assert_eq!(parse_range("00AA"), Ok(('\u{AA}', '\u{AA}')));
        assert_eq!(parse_range("0041..005A"), Ok(('A', 'Z')));
        assert_eq!(parse_range("10FFFF"), Ok(('\u{10FFFF}', '\u{10FFFF}')));
        for field in [
            "",
            "41",
            "0041..",
            "XYZW",
            "005A..0041",
            "D800",
            "D7FF..E000",
            "110000",
        ] {
            assert!(parse_range(field).is_err(), "{field:?}");
        }
    }

    #[test]
    fn reads_the_ranges_of_one_property() {
        let text = "\
# Derived Property: XID_Start
0041..005A    ; XID_Start # L&  [26] LATIN CAPITAL LETTER A..LATIN CAPITAL LETTER Z
00AA          ; XID_Start # Lo       FEMININE ORDINAL INDICATOR

0030..0039    ; XID_Continue # Nd  [10] DIGIT ZERO..DIGIT NINE
0041          ; XID_Start_Other
";
        assert_eq!(
            property_ranges(text, "XID_Start"),
            Ok(vec![('A', 'Z'), ('\u{AA}', '\u{AA}')])
        );
        assert_eq!(property_ranges(text, "XID_Continue"), Ok(vec![('0', '9')]));
        assert!(property_ranges(text, "ID_Start").is_err());
        assert!(property_ranges("0041 XID_Start\n", "XID_Start").is_err());
    }

    #[test]
    fn merges_overlapping_and_adjacent_ranges() {
        assert_eq!(
            merge(vec![
                ('d', 'f'),
                ('a', 'b'),
                ('c', 'c'),
                ('x', 'z'),
                ('e', 'g'),
                ('y', 'y')
            ]),
            vec![('a', 'g'), ('x', 'z')]
        );
        assert_eq!(
            merge(vec![('a', 'a'), ('c', 'c')]),
            vec![('a', 'a'), ('c', 'c')]
        );
    }

    #[test]
    fn checks_the_subset() {
        let outer = [('0', '9'), ('A', 'Z')];
        assert!(is_subset(&[('A', 'C'), ('Z', 'Z')], &outer));
        assert!(!is_subset(&[('8', 'A')], &outer));
        assert!(!is_subset(&[('a', 'a')], &outer));
    }

    #[test]
    fn rejects_a_wrong_hash() {
        let input = format!("{VERSION_LINE}\n0041 ; XID_Start\n0041 ; XID_Continue\n");
        let error = generate(input.as_bytes()).unwrap_err();
        assert!(error.starts_with("SHA-256 check failed"), "{error}");
    }

    #[test]
    fn rejects_a_wrong_version_line() {
        for input in [
            "# DerivedCoreProperties-18.0.0.txt\n",
            "",
            "\n# DerivedCoreProperties-17.0.0.txt\n",
        ] {
            let error = generate(input.as_bytes()).unwrap_err();
            assert!(error.starts_with("version line check failed"), "{error}");
        }
    }
}
