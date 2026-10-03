# Command Line

This page is the reference for the `fernq` command line.

The `fernq` binary accepts this command line:

```text
fernq <INPUT> -o <OUTPUT> --edition <EDITION> [--max-input-bytes <BYTES>]
fernq -h | --help
```

| Command line | Result | Exit status |
|---|---|---|
| `-h` or `--help` as the only argument | Usage on stdout. | 0 |
| `<INPUT> -o <OUTPUT> --edition <EDITION>`, with an optional `--max-input-bytes <BYTES>` | `fernq` loads and lexes the input. A diagnostic on stderr names the first failure, or states that compilation is not implemented. `fernq` writes no output. | 1 |
| Any other command line | A diagnostic and a usage line on stderr. | 2 |

## Arguments

`<INPUT>`, `-o <OUTPUT>`, `--edition <EDITION>`, and `--max-input-bytes <BYTES>` can appear in any order. The token after `-o` is the output path, even when it starts with `-`. The token after `--edition` or `--max-input-bytes` is its value, in the same way. Paths do not need to be valid UTF-8. Every other token that starts with `-` is an unknown option, including `-`, `--`, `-oFILE`, `-o=FILE`, `--edition=2024`, and `--max-input-bytes=4`. An empty path, a repeated `-o`, a second input path, and `-h` or `--help` with other arguments are invalid.

`--edition` gives the Rust edition of the input. It is required and has no default. The accepted values are exactly `2015`, `2018`, `2021`, and `2024`. A missing `--edition`, a repeated `--edition`, `--edition` as the last token, and any other value, including an empty value, are invalid.

`--max-input-bytes` sets the largest input that `fernq` loads, in bytes. It is optional; without it the limit is 134,217,728 bytes (128 MiB). The value is decimal digits only, from 1 to 4,294,967,295. A repeated `--max-input-bytes`, `--max-input-bytes` as the last token, and any other value, including `0`, an empty value, a sign, and a unit such as `128MiB`, are invalid. [Input](#input) states what the limit does.

`fernq` has no target option. [Compiler Session](compiler-session.md) states the rule for the edition and the target.

`fernq` reads no environment variable, except the backtrace setting of the Rust standard library after a panic.

## Input

Rust requires source files to be UTF-8. `fernq` loads an input that it can open and read, that is at most the input size limit, and that is valid UTF-8. A directory, a larger file, and a file that is not valid UTF-8 cannot be loaded.

The input size limit is 134,217,728 bytes (128 MiB), or the value of `--max-input-bytes`, at most 4,294,967,295 bytes (`u32::MAX`), the largest text whose offsets `fernq` represents. `fernq` rejects a larger regular file from its size, before it reads it, with an `input-too-large` diagnostic. An input that is not a regular file, such as a FIFO, is read until it ends or exceeds the limit. Loading an input takes about one byte of memory per input byte, so the limit also bounds that memory.

The limit is a `fernq` resource policy, not a Rust rule: Rust sets no size limit on a source file, and `rustc` 1.99.0 reads files of up to 4,294,967,294 bytes. A valid Rust file over 128 MiB needs `--max-input-bytes`. A limit above the memory available to the process lets the operating system end it before `fernq` reports a diagnostic.

## Lexing

After loading, `fernq` lexes the input in the given edition. Lexing stops at the first position where the input is outside the supported lexical surface, and `fernq` reports one diagnostic for that position. When lexing reaches the end of the file, a diagnostic of kind `compilation-not-implemented` states that compilation is not implemented.

The supported lexical surface is:

- identifiers: a character of `XID_Start` or `_`, then characters of `XID_Continue`, such as `x`, `_tmp`, `café`, and `東京`. The Rust Reference for Rust 1.99.0 defines these classes with Unicode 17.0.0, and `fernq` uses Unicode 17.0.0. A non-ASCII identifier is never a keyword;
- raw identifiers: `r#` followed by an identifier or keyword, such as `r#fn`, in every edition. A raw identifier is never a keyword;
- the strict and reserved keywords of the Rust Reference for Rust 1.99.0, by edition: `async`, `await`, `dyn`, and `try` are keywords from edition 2018, and `gen` from edition 2024; in earlier editions they are identifiers; weak keywords such as `union` and `macro_rules` are identifiers;
- lifetimes: `'` followed by an identifier or keyword, such as `'a`, `'static`, `'_`, and `'fn`. Lexing does not reject a keyword lifetime; `rustc` rejects `'fn` only where the grammar requires a lifetime or a label. From edition 2021, raw lifetimes: `'r#` followed by an identifier or keyword, such as `'r#fn`;
- integer literals in decimal, binary (`0b`), octal (`0o`), and hexadecimal (`0x`), with `_` separators and an optional suffix, such as `98_222`, `0xff`, `0b1111_0000`, and `1u8`;
- floating-point literals: decimal digits with a fractional part, an exponent, or both, and decimal digits followed by a `.` that starts no other token, with an optional suffix, such as `1.0`, `1.`, `1e3`, `1E+3`, `1_000.5e-3`, and `2.5f32`. Without an exponent, the suffix cannot start with `e` or `E`: `1.0em` is an exponent with no digit. After an exponent, the suffix can: `2e5e6` is one token;
- character literals, such as `'a'`, `'\n'`, `'\u{E9}'`, and `'é'`, and byte literals, such as `b'a'` and `b'\xFF'`;
- string literals and raw string literals, such as `"a"`, `r"a"`, and `r#"a"b"#`; byte string literals and raw byte string literals, such as `b"a"` and `br"a"`; from edition 2021, C string literals and raw C string literals, such as `c"a"` and `cr#"a"#`. A raw literal has the same number of `#`, at most 255, on each side;
- the punctuation of the Rust Reference other than delimiters: `+` `-` `*` `/` `%` `^` `!` `&` `|` `&&` `||` `<<` `>>` `+=` `-=` `*=` `/=` `%=` `^=` `&=` `|=` `<<=` `>>=` `=` `==` `!=` `>` `<` `>=` `<=` `@` `.` `..` `...` `..=` `,` `;` `:` `::` `->` `=>` `<-` `#` `$` `?` `~`. Adjacent punctuation forms the longest token first: `&&&` is `&&` then `&`, and `x<-1` contains `<-`. Whitespace and comments separate tokens: `< <` is two tokens;
- the delimiters `(`, `)`, `[`, `]`, `{`, and `}`. Lexing does not check that delimiters pair, so `fn main( {}` lexes. A delimiter character inside a literal or a comment is not a delimiter;
- whitespace: U+0009 to U+000D, U+0020, U+0085, U+200E, U+200F, U+2028, and U+2029;
- line comments and nested block comments that are not doc comments;
- a byte order mark at the start of the file;
- a shebang line at the start of the file or after the byte order mark. As Rust requires, `#!` followed by `[`, after whitespace and non-doc comments, starts an inner attribute, not a shebang. After the start of the file, `#!` is the punctuation `#` and `!`.

Lexing checks the lexical form of each literal, as the Rust Reference defines it, and computes no value:

- Escapes: every literal class that has escapes accepts `\n`, `\r`, `\t`, `\\`, `\'`, and `\"`, and every class other than C string literals accepts `\0`. `\x` takes two hexadecimal digits, at most `\x7F` in character and string literals. `\u{...}` takes one to six hexadecimal digits, each optionally followed by `_`, whose value is a Unicode scalar value; byte and byte string literals do not accept it. `\` directly before a line break continues a string, byte string, or C string literal. Raw literals have no escapes.
- CR followed by LF inside a literal counts as LF. Any other CR inside a literal is a lexical error.
- Byte, byte string, and raw byte string literals contain only ASCII characters. C string and raw C string literals contain no NUL character and no escape whose value is 0.
- A character or byte literal contains one character or one escape. `'`, LF, CR, and TAB in it must be escaped.
- A suffix directly after a literal is part of the literal token: `"a"suffix`, `'a'b`, and `1.0f80` lex. A suffix is an identifier: an `XID_Start` character, or `_` followed by an `XID_Continue` character, then `XID_Continue` characters, such as `1é`. A suffix of `_` alone is a lexical error.

Lexing does not check whether a suffix is valid for its literal, and does not check that an integer value fits a type: `1suffix` and `340282366920938463463374607431768211456` lex. `rustc` reports these where it uses the literal value.

After the digits of an integer literal without a suffix, `.` followed by `.`, `_`, or an `XID_Start` character is the next token: `1..2` is `1`, `..`, `2`, `1.foo` is `1`, `.`, `foo`, `1.f32` is `1`, `.`, `f32`, and `1.é` is `1`, `.`, `é`. Any other `.` after decimal digits starts a floating-point literal: `1.0.0` is `1.0`, `.`, `0`, and `t.0.1` is `t`, `.`, `0.1`. After binary, octal, or hexadecimal digits, it is a lexical error. After a suffix, `.` is always the next token: `1u8.0` is `1u8`, `.`, `0`.

The edition changes how these inputs lex:

| Input | Editions 2015 and 2018 | Edition 2021 | Edition 2024 |
|---|---|---|---|
| An identifier or keyword directly followed by `#`, such as `a#b`, other than `r#`, `br#`, and from 2021 `cr#` | identifier, `#`, identifier | `lexical-error`: reserved prefix | `lexical-error`: reserved prefix |
| An identifier or keyword directly followed by `"` or `'` that is not a literal prefix, such as `a"x"`, `rb"x"`, and `r'x'` | identifier, then the literal | `lexical-error`: reserved prefix | `lexical-error`: reserved prefix |
| `c` or `cr` directly followed by `"`, such as `c"x"` | identifier, then a string literal | C string or raw C string literal | C string or raw C string literal |
| `cr#`, such as `cr#a` and `cr#"x"#` | identifier, `#`, then the rest | raw C string literal, or `lexical-error` | raw C string literal, or `lexical-error` |
| A lifetime directly followed by `#`, such as `'a#b` | lifetime, `#`, identifier | `lexical-error`: reserved prefix | `lexical-error`: reserved prefix |
| `'r#` followed by an identifier, such as `'r#a` | lifetime `'r`, `#`, identifier | raw lifetime | raw lifetime |
| Two or more `#` in a row, such as `##` | one `#` each | one `#` each | `lexical-error`: reserved |
| `#` directly followed by `"` | `#`, then a string literal | `#`, then a string literal | `lexical-error`: reserved |

The literal prefixes are `b`, `r`, and `br` before `"`, `b` before `'`, and from edition 2021 `c` and `cr` before `"`. In every edition, `r#`, `br#`, and from 2021 `cr#` start a raw literal, and `r#` also starts a raw identifier. No reserved prefix applies after a raw identifier or a raw lifetime: `r#a#b`, `r#x"a"#`, and `'r#a#b` lex. A raw string after `#` is not reserved: in edition 2024, `#r"x"` is `#` and a raw string literal.

Input outside this surface gets one of two diagnostic kinds:

| Kind | Input | Location |
|---|---|---|
| `lexical-error` | The input is not valid Rust: a backtick, a backslash, U+0000 to U+0008, U+000E to U+001F, U+007F outside a literal; outside a literal, a non-ASCII character other than whitespace that is not `XID_Start` where a token starts (`\u{301}`, `😀`, U+200D), including directly after an identifier, lifetime, or literal that it cannot continue (`a😀` is `a`, then the error at `😀`); a block comment that is not a doc comment and has no closing `*/`; a binary or octal digit outside the radix (`0b0102`); a radix prefix with no digit (`0b_`, `0xG`); a binary or octal literal followed by `e` or `E` (`0b101e`); a binary, octal, or hexadecimal literal followed by a `.` that starts no other token (`0x80.0`); an exponent with no digit (`2e`, `1.0em`); an invalid escape (`"\q"`, `"\x80"`, `'\u{D800}'`, `b"\u{41}"`); a CR not followed by LF inside a literal; a non-ASCII character in a byte or byte string literal (`b'é'`); a NUL character or an escape whose value is 0 in a C string literal (`c"\0"`); a literal with no closing quote, or a raw literal with no closing `#`; more than 255 `#` after a raw prefix; a raw prefix that starts no raw identifier, raw lifetime, or raw literal (`r##a`, `r# a`, `br#a`, `'r#1`); `_`, `crate`, `self`, `Self`, or `super` as a raw identifier or raw lifetime (`r#self`, `'r#self`); a character or byte literal that is empty, has more than one character, or is not closed (`''`, `'ab'`, `'1a`); an unescaped `'`, LF, CR, or TAB in a character or byte literal; a suffix of `_` alone (`"a"_`); the reserved forms in the table above. | The start of the literal or of the reserved form, the escape, or the character inside the literal that breaks the rule. For a character, the character. For an unterminated comment, the outermost `/*`. |
| `unsupported-syntax` | Doc comments, and ZWJ (U+200D) or ZWNJ (U+200C) after the first character of an identifier, raw identifier, lifetime, raw lifetime, or suffix. The input can be valid Rust. | The start of the doc comment, or the ZWJ or ZWNJ. |

Raw identifiers, lifetimes, raw lifetimes, and suffixes use the same identifier characters as identifiers: `r#é`, `'é`, `'r#é`, `1é`, and `"a"é` lex. `'é'` is a character literal and `'é` is a lifetime. Non-ASCII identifiers follow the literal prefix and reserved prefix rules in the table above: in edition 2021, `é#a`, `é"x"`, `é'x'`, and `'é#a` are reserved prefixes.

ZWJ and ZWNJ are `XID_Continue` in Unicode 17.0.0, and `rustc` 1.99.0 accepts them inside an identifier, but the Rust Reference forbids them in identifiers. Until the two agree, `fernq` reports them as `unsupported-syntax` after the first character of an identifier: `a\u{200D}b` reports `unsupported-syntax` at U+200D. They are not `XID_Start`, so where a token starts they are a lexical error, as in `rustc`.

Rust identifiers are equal when their NFC forms are equal. `fernq` does not normalize identifiers and does not compare them yet, so this equality is not observable in `fernq` output. The identifier Unicode version changes only with the Rust version that `fernq` targets.

## Diagnostics

`fernq` reports each failure on stderr as one diagnostic:

```text
error[<kind>]: <message>
 --> <path>:<line>:<column>
```

The kind identifies the failure, for example `input-not-found`, `input-not-utf8`, `lexical-error`, `unsupported-syntax`, `compilation-not-implemented`, or `invalid-command-line`. Only an input that is not valid UTF-8 and a lexing diagnostic get the location line. For an input that is not valid UTF-8, it gives the line and column of the first invalid byte. For a lexing diagnostic, it gives the line and column where the location in [Lexing](#lexing) starts. Lines end at LF. Columns count Unicode scalar values from 1 and do not count a byte order mark at the start of the file. For the inputs that are not valid UTF-8 that were compared, `rustc` 1.99.0 reports the same line and column. A panic in `fernq` is a Fernq defect: it reports an `internal-compiler-error` diagnostic before the panic message, and its exit status is not part of the command line contract.

## Help Output

Help exits with status 1 when stdout reports a write error, for example a closed pipe. When the process starts with stdout closed, the Rust standard library discards the help text without an error, and help exits with status 0.

## Stability

The command line is experimental. Options, exit statuses, diagnostic kinds, the diagnostic format, and messages can change without notice. Diagnostics are not a machine-readable format. The command line is not compatible with `rustc`, so Cargo cannot run `fernq` as its compiler.
