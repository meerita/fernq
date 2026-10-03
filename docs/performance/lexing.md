# Lexing Performance

This page describes the lexer as a performance subject. [Performance](README.md) defines the benchmark model. [Command Line](../cli.md#lexing) owns the supported lexical surface.

No Fernq-vs-`rustc` lexer performance result has been established yet. Fernq has no lexer benchmark, and this page contains no measurements.

## Current Lexer

The lexer is the first Fernq compiler component with a separable interface that can support direct measurement once a benchmark harness exists. It reads the text of one loaded source file in the edition of that source.

The lexer:

- is pull-based: the caller requests each token;
- returns one token per call;
- retains no token collection: the caller decides which tokens to keep;
- stops at end of file, or at the first position outside the supported lexical surface.

The lexer supports only the lexical surface that [Command Line](../cli.md#lexing) lists. A lexer benchmark on input outside that surface measures lexing up to the first diagnostic, not lexing of the whole input.

The lexer is internal to the `fernq` crate. It is not a public API.

The `fernq` driver lexes the whole input and keeps no token. The time of a complete `fernq` process also includes process start, command-line parsing, input loading, and diagnostic output. It is not a lexer measurement.

## Planned Measurements

A planned lexer performance investigation will measure applicable:

- wall-clock time;
- throughput, in source bytes per second, for example MB/s;
- cost per byte, where it is useful;
- allocations;
- bytes allocated;
- token representation size;
- memory behavior, where it is meaningful.

These are measurement dimensions. They are not results.

## Planned Workloads

The provisional corpus has these workload classes:

| Workload | Content |
|---|---|
| Tiny source | A minimal program, for example `fn main() {}` |
| Small, medium, and large source | The same content mix at increasing sizes |
| Identifier-heavy | Mostly identifiers and keywords |
| Delimiter-heavy | Mostly delimiters |
| Whitespace-heavy | Mostly whitespace between few tokens |
| Comment-heavy | Mostly line comments and block comments |
| Token-dense | Tokens with little or no whitespace between them |

Every workload uses only the supported lexical surface. Byte sizes are not fixed. The planned investigation defines the final benchmark matrix.

## Comparison with rustc

The planned investigation may compare the Fernq lexer with the corresponding `rustc` lexical implementation. The comparison is valid only when equivalent work and equivalent measurement boundaries can be established.

A complete `rustc` invocation is not a lexer benchmark. `rustc` does more work than lexing, so its process time does not isolate lexing.

A future comparison controls:

- identical input bytes;
- equivalent lexical work;
- equivalent output consumption;
- the Fernq revision and the exact `rustc` or tool version;
- the host environment;
- the benchmark tier.

The two lexical implementations can differ in the tokens they produce and in the work they do for each token. A comparison states each difference that affects the result.

Fernq does not adopt `rustc` lexer architecture to make a comparison easier.
