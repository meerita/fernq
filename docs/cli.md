# Command Line

This page is the reference for the `fernq` command line.

The `fernq` binary accepts this command line:

```text
fernq <INPUT> -o <OUTPUT>
fernq -h | --help
```

| Command line | Result | Exit status |
|---|---|---|
| `-h` or `--help` as the only argument | Usage on stdout. | 0 |
| `<INPUT> -o <OUTPUT>` | `fernq` reads the input. If the input cannot be loaded, a diagnostic on stderr names the failure. Otherwise, a diagnostic on stderr states that compilation is not implemented. `fernq` writes no output in either case. | 1 |
| Any other command line | A diagnostic and a usage line on stderr. | 2 |

## Arguments

`<INPUT>` and `-o <OUTPUT>` can appear in either order. The token after `-o` is the output path, even when it starts with `-`. Paths do not need to be valid UTF-8. Every other token that starts with `-` is an unknown option, including `-`, `--`, `-oFILE`, and `-o=FILE`. An empty path, a repeated `-o`, a second input path, and `-h` or `--help` with other arguments are invalid.

`fernq` has no edition or target option. [Compiler Session](compiler-session.md) states the rule for these inputs.

`fernq` reads no environment variable, except the backtrace setting of the Rust standard library after a panic.

## Input

Rust requires source files to be UTF-8. `fernq` loads an input that it can open and read, that is at most 4,294,967,295 bytes (`u32::MAX`), and that is valid UTF-8. A directory, a larger file, and a file that is not valid UTF-8 cannot be loaded. An input that is not a regular file, such as a FIFO, is read until it ends or exceeds the size limit.

## Diagnostics

`fernq` reports each failure on stderr as one diagnostic:

```text
error[<kind>]: <message>
 --> <path>:<line>:<column>
```

The kind identifies the failure, for example `input-not-found`, `input-not-utf8`, `compilation-not-implemented`, or `invalid-command-line`. Only an input that is not valid UTF-8 gets the location line, which gives the line and column of its first invalid byte. Lines end at LF. Columns count Unicode scalar values from 1 and do not count a byte order mark at the start of the file. For the inputs compared, `rustc` 1.99.0 reports the same line and column. A panic in `fernq` is a Fernq defect: it reports an `internal-compiler-error` diagnostic before the panic message, and its exit status is not part of the command line contract.

## Help Output

Help exits with status 1 when stdout reports a write error, for example a closed pipe. When the process starts with stdout closed, the Rust standard library discards the help text without an error, and help exits with status 0.

## Stability

The command line is experimental. Options, exit statuses, diagnostic kinds, the diagnostic format, and messages can change without notice. Diagnostics are not a machine-readable format. The command line is not compatible with `rustc`, so Cargo cannot run `fernq` as its compiler.
