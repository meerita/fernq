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
- raw identifiers: `r#` followed by an identifier or keyword, such as `r#fn`, in every edition. A raw identifier is never a keyword;
- the strict and reserved keywords of the Rust Reference for Rust 1.99.0, by edition: `async`, `await`, `dyn`, and `try` are keywords from edition 2018, and `gen` from edition 2024; in earlier editions they are identifiers; weak keywords such as `union` and `macro_rules` are identifiers;
- lifetimes: `'` followed by an identifier or keyword, such as `'a`, `'static`, `'_`, and `'fn`. Lexing does not reject a keyword lifetime; `rustc` rejects `'fn` only where the grammar requires a lifetime or a label. From edition 2021, raw lifetimes: `'r#` followed by an identifier or keyword, such as `'r#fn`;
- integer literals in decimal, binary (`0b`), octal (`0o`), and hexadecimal (`0x`), with `_` separators and an optional suffix of ASCII letters, digits, and `_`, such as `98_222`, `0xff`, `0b1111_0000`, and `1u8`;
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
- A suffix directly after a literal is part of the literal token: `"a"suffix`, `'a'b`, and `1.0f80` lex. A suffix is an ASCII letter, or `_` followed by an ASCII letter, digit, or `_`, then ASCII letters, digits, and `_`. A suffix of `_` alone is a lexical error.

Lexing does not check whether a suffix is valid for its literal, and does not check that an integer value fits a type: `1suffix` and `340282366920938463463374607431768211456` lex. `rustc` reports these where it uses the literal value.

After the digits of an integer literal without a suffix, `.` followed by `.`, `_`, or an ASCII letter is the next token: `1..2` is `1`, `..`, `2`, `1.foo` is `1`, `.`, `foo`, and `1.f32` is `1`, `.`, `f32`. Any other `.` after decimal digits starts a floating-point literal: `1.0.0` is `1.0`, `.`, `0`, and `t.0.1` is `t`, `.`, `0.1`. After binary, octal, or hexadecimal digits, it is a lexical error. After a suffix, `.` is always the next token: `1u8.0` is `1u8`, `.`, `0`.

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
| `lexical-error` | The input is not valid Rust: a backtick, a backslash, U+0000 to U+0008, U+000E to U+001F, U+007F outside a literal; a block comment that is not a doc comment and has no closing `*/`; a binary or octal digit outside the radix (`0b0102`); a radix prefix with no digit (`0b_`, `0xG`); a binary or octal literal followed by `e` or `E` (`0b101e`); a binary, octal, or hexadecimal literal followed by a `.` that starts no other token (`0x80.0`); an exponent with no digit (`2e`, `1.0em`); an invalid escape (`"\q"`, `"\x80"`, `'\u{D800}'`, `b"\u{41}"`); a CR not followed by LF inside a literal; a non-ASCII character in a byte or byte string literal (`b'é'`); a NUL character or an escape whose value is 0 in a C string literal (`c"\0"`); a literal with no closing quote, or a raw literal with no closing `#`; more than 255 `#` after a raw prefix; a raw prefix that starts no raw identifier, raw lifetime, or raw literal (`r##a`, `r# a`, `br#a`, `'r#1`); `_`, `crate`, `self`, `Self`, or `super` as a raw identifier or raw lifetime (`r#self`, `'r#self`); a character or byte literal that is empty, has more than one character, or is not closed (`''`, `'ab'`, `'1a`); an unescaped `'`, LF, CR, or TAB in a character or byte literal; a suffix of `_` alone (`"a"_`); the reserved forms in the table above. | The start of the literal or of the reserved form, the escape, or the character inside the literal that breaks the rule. For a character, the character. For an unterminated comment, the outermost `/*`. |
| `unsupported-syntax` | Doc comments, and non-ASCII characters other than whitespace outside literals. The input can be valid Rust. | The start of the doc comment, or the non-ASCII character. |

Non-ASCII identifiers, raw identifiers, lifetimes, raw lifetimes, and suffixes are unsupported. `fernq` does not report a non-ASCII character outside a literal as a lexical error, because it does not yet distinguish identifier characters from characters that no Rust token can start. For the same reason, a non-ASCII character other than whitespace is unsupported at that character when it directly follows an identifier, keyword, raw identifier, lifetime, raw lifetime, `r#`, `'r#`, or a literal and its suffix, and when it follows `'` without a closing `'` after it, because it can start a lifetime. `café`, `1é`, `1.é`, `1.0é`, `r#é`, `'é`, `'aé`, and `"a"é` report `unsupported-syntax` at `é`. Non-ASCII characters inside string, raw string, C string, raw C string, and character literals are supported: `"é"`, `r"é"`, and `'é'` lex.

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
