# Command Line

This page is the reference for the `fernq` command line.

The `fernq` binary accepts this command line:

```text
fernq <INPUT> -o <OUTPUT> --edition <EDITION>
fernq -h | --help
```

| Command line | Result | Exit status |
|---|---|---|
| `-h` or `--help` as the only argument | Usage on stdout. | 0 |
| `<INPUT> -o <OUTPUT> --edition <EDITION>` | `fernq` loads and lexes the input. A diagnostic on stderr names the first failure, or states that compilation is not implemented. `fernq` writes no output. | 1 |
| Any other command line | A diagnostic and a usage line on stderr. | 2 |

## Arguments

`<INPUT>`, `-o <OUTPUT>`, and `--edition <EDITION>` can appear in any order. The token after `-o` is the output path, even when it starts with `-`. The token after `--edition` is the edition, in the same way. Paths do not need to be valid UTF-8. Every other token that starts with `-` is an unknown option, including `-`, `--`, `-oFILE`, `-o=FILE`, and `--edition=2024`. An empty path, a repeated `-o`, a second input path, and `-h` or `--help` with other arguments are invalid.

`--edition` gives the Rust edition of the input. It is required and has no default. The accepted values are exactly `2015`, `2018`, `2021`, and `2024`. A missing `--edition`, a repeated `--edition`, `--edition` as the last token, and any other value, including an empty value, are invalid.

`fernq` has no target option. [Compiler Session](compiler-session.md) states the rule for the edition and the target.

`fernq` reads no environment variable, except the backtrace setting of the Rust standard library after a panic.

## Input

Rust requires source files to be UTF-8. `fernq` loads an input that it can open and read, that is at most 4,294,967,295 bytes (`u32::MAX`), and that is valid UTF-8. A directory, a larger file, and a file that is not valid UTF-8 cannot be loaded. An input that is not a regular file, such as a FIFO, is read until it ends or exceeds the size limit.

## Lexing

After loading, `fernq` lexes the input in the given edition. Lexing stops at the first position where the input is outside the supported lexical surface, and `fernq` reports one diagnostic for that position. When lexing reaches the end of the file, a diagnostic of kind `compilation-not-implemented` states that compilation is not implemented.

The supported lexical surface is:

- identifiers of ASCII letters, ASCII digits, and `_` that do not start with a digit;
- the strict and reserved keywords of the Rust Reference for Rust 1.99.0, by edition: `async`, `await`, `dyn`, and `try` are keywords from edition 2018, and `gen` from edition 2024; in earlier editions they are identifiers; weak keywords such as `union` and `macro_rules` are identifiers;
- integer literals in decimal, binary (`0b`), octal (`0o`), and hexadecimal (`0x`), with `_` separators and an optional suffix of ASCII letters, digits, and `_`, such as `98_222`, `0xff`, `0b1111_0000`, and `1u8`. Lexing does not check the suffix or the value: `1suffix` and `340282366920938463463374607431768211456` lex;
- the punctuation of the Rust Reference other than delimiters: `+` `-` `*` `/` `%` `^` `!` `&` `|` `&&` `||` `<<` `>>` `+=` `-=` `*=` `/=` `%=` `^=` `&=` `|=` `<<=` `>>=` `=` `==` `!=` `>` `<` `>=` `<=` `@` `.` `..` `...` `..=` `,` `;` `:` `::` `->` `=>` `<-` `#` `$` `?` `~`. Adjacent punctuation forms the longest token first: `&&&` is `&&` then `&`, and `x<-1` contains `<-`. Whitespace and comments separate tokens: `< <` is two tokens;
- the delimiters `(`, `)`, `[`, `]`, `{`, and `}`; lexing does not check that delimiters pair, so `fn main( {}` lexes;
- whitespace: U+0009 to U+000D, U+0020, U+0085, U+200E, U+200F, U+2028, and U+2029;
- line comments and nested block comments that are not doc comments;
- a byte order mark at the start of the file;
- a shebang line at the start of the file or after the byte order mark. As Rust requires, `#!` followed by `[`, after whitespace and non-doc comments, starts an inner attribute, not a shebang. After the start of the file, `#!` is the punctuation `#` and `!`.

After the digits of an integer literal without a suffix, `.` followed by `.`, `_`, or an ASCII letter is the next token: `1..2` is `1`, `..`, `2`, and `1.foo` is `1`, `.`, `foo`. Any other `.` makes a decimal literal a floating-point literal, as does an exponent such as `e3`. After binary, octal, or hexadecimal digits, it is a lexical error. After a suffix, `.` is always the next token: `1u8.0` is `1u8`, `.`, `0`.

The edition changes how `#` lexes:

| Input | Editions 2015 and 2018 | Edition 2021 | Edition 2024 |
|---|---|---|---|
| An identifier or keyword directly followed by `#`, such as `a#b` | identifier, `#`, identifier | `lexical-error`: reserved prefix | `lexical-error`: reserved prefix |
| `r#` or `br#` | `unsupported-syntax` | `unsupported-syntax` | `unsupported-syntax` |
| `cr#` | identifier, `#` | `unsupported-syntax` | `unsupported-syntax` |
| Two or more `#` in a row, such as `##` | one `#` each | one `#` each | `lexical-error`: reserved |
| `#` directly followed by `"` | `#`, then `unsupported-syntax` at `"` | `#`, then `unsupported-syntax` at `"` | `lexical-error`: reserved |

Input outside this surface gets one of two diagnostic kinds:

| Kind | Input | Location |
|---|---|---|
| `lexical-error` | The input is not valid Rust: a backtick, a backslash, U+0000 to U+0008, U+000E to U+001F, U+007F; a block comment that is not a doc comment and has no closing `*/`; a binary or octal digit outside the radix (`0b0102`); a radix prefix with no digit (`0b_`, `0xG`); a binary or octal literal followed by `e` or `E` (`0b101e`); a binary, octal, or hexadecimal literal followed by a `.` that starts no other token (`0x80.0`); an exponent with no digit (`2e`, `2em`); the reserved `#` forms in the table above. | The character, the outermost `/*` of the unterminated comment, or the start of the literal or of the reserved form. |
| `unsupported-syntax` | Any other input. The input can be valid Rust. This includes floating-point literals, string, byte string, and character literals, raw identifiers and raw strings, lifetimes, doc comments, and non-ASCII characters other than whitespace. | The first unsupported character, the start of the doc comment, the start of the floating-point literal, or the start of the raw prefix. |

Non-ASCII identifiers and suffixes are unsupported. `fernq` does not report a non-ASCII character as a lexical error, because it does not yet distinguish identifier characters from characters that no Rust token can start. For the same reason, an identifier, keyword, or integer literal directly followed by a non-ASCII character other than whitespace is unsupported at that character. The same applies to an integer literal followed by `.` and such a character. `café`, `1é`, `1.é`, and `0x1.é` report `unsupported-syntax` at `é`. An identifier directly followed by `"` or `'`, such as `b"x"`, is unsupported at the quote.

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
