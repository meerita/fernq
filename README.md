# Fernq

<p align="center">
  <strong>A next-generation compiler for Rust.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-early%20development-orange" alt="Status">
  <img src="https://img.shields.io/badge/language-Rust-black?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License">
</p>

---

Fernq is an independent compiler for the Rust programming language.

The project is being built from first principles with a focus on compiler performance, incremental compilation, correctness, generated code quality, predictable resource usage, and long-term maintainability.

Fernq is currently in **early development**. Architecture, compatibility targets, implementation details, and performance characteristics are still under investigation.

## Why Fernq?

Rust has evolved significantly, and so has the scale of the software being built with it.

Fernq explores what a Rust compiler could look like if its architecture were designed today around modern workloads, highly incremental development, persistent compilation state, parallel execution, and measurable performance.

The project does not assume that existing compiler architecture should be reproduced.

Likewise, it does not assume that a different design is automatically better.

Important architectural decisions are expected to be justified through research, implementation, testing, and measurement.

## Goals

Fernq is intended to pursue:

- compatibility with the Rust language and ecosystem
- fast clean compilation
- significantly improved incremental compilation
- high-quality generated machine code
- efficient use of CPU and memory
- scalable parallel compilation
- predictable performance
- strong diagnostics
- correctness as a non-negotiable requirement
- a maintainable compiler architecture

## Principles

### Correctness before performance

A compiler that produces incorrect programs quickly is not useful.

Performance work must preserve language semantics and correctness.

### Measure, do not assume

Performance claims must be backed by reproducible benchmarks.

Architectural decisions must be backed by evidence where practical.

### Compatibility matters

Fernq is intended to compile Rust, not a simplified language that merely resembles Rust.

Where Rust language semantics, ecosystem expectations, and `rustc` implementation details differ, those distinctions should be explicitly understood.

### Independent implementation

Fernq is not intended to be a wrapper around `rustc`.

Unsupported functionality must not be silently delegated to `rustc` and presented as native Fernq functionality.

### Architecture is earned

The project will not commit prematurely to a parser model, intermediate representation, query system, code generation backend, incremental model, or other major subsystem before the relevant problem has been investigated.

## Project Status

> [!WARNING]
> Fernq is not currently usable as a Rust compiler.

The project is in its initial research and bootstrap phase.

Expect significant changes to architecture, APIs, repository structure, and implementation as the project develops.

No compatibility or performance guarantees are currently made.

## Performance

Performance is one of Fernq's primary goals, but benchmarks will only be published once meaningful compiler functionality exists and comparisons can be reproduced fairly.

Fernq will distinguish between metrics such as:

- clean build time
- incremental build time
- frontend latency
- optimization time
- code generation time
- peak memory usage
- CPU usage
- generated code performance
- generated code size

A local microbenchmark improvement will not be presented as an end-to-end compiler improvement.

## Repository

The repository structure will evolve alongside the architecture.

Large subsystem boundaries will be introduced only after their responsibilities and interactions have been investigated.

## Building

Fernq does not yet have a usable compiler implementation.

The repository is a Cargo workspace. `rust-toolchain.toml` pins the Rust toolchain that builds Fernq. Install [rustup](https://rustup.rs), then run from the repository root:

```sh
rustup toolchain install
cargo build --workspace --locked
```

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

`<INPUT>` and `-o <OUTPUT>` can appear in either order. The token after `-o` is the output path, even when it starts with `-`. Paths do not need to be valid UTF-8. Every other token that starts with `-` is an unknown option, including `-`, `--`, `-oFILE`, and `-o=FILE`. An empty path, a repeated `-o`, a second input path, and `-h` or `--help` with other arguments are invalid.

Rust requires source files to be UTF-8. `fernq` loads an input that it can open and read, that is at most 4,294,967,295 bytes (`u32::MAX`), and that is valid UTF-8. A directory, a larger file, and a file that is not valid UTF-8 cannot be loaded. An input that is not a regular file, such as a FIFO, is read until it ends or exceeds the size limit.

`fernq` reports each failure on stderr as one diagnostic:

```text
error[<kind>]: <message>
 --> <path>:<line>:<column>
```

The kind identifies the failure, for example `input-not-found`, `input-not-utf8`, `compilation-not-implemented`, or `invalid-command-line`. Only an input that is not valid UTF-8 gets the location line, which gives the line and column of its first invalid byte. Lines end at LF. Columns count Unicode scalar values from 1 and do not count a byte order mark at the start of the file. For the inputs compared, `rustc` 1.99.0 reports the same line and column. A panic in `fernq` is a Fernq defect: it reports an `internal-compiler-error` diagnostic before the panic message, and its exit status is not part of the command line contract.

Help exits with status 1 when stdout reports a write error, for example a closed pipe. When the process starts with stdout closed, the Rust standard library discards the help text without an error, and help exits with status 0.

The command line is experimental. Options, exit statuses, diagnostic kinds, the diagnostic format, and messages can change without notice. Diagnostics are not a machine-readable format. The command line is not compatible with `rustc`, so Cargo cannot run `fernq` as its compiler.

Builds are validated on macOS aarch64, and on Linux aarch64 in the Debian-based container that [CONTRIBUTING.md](CONTRIBUTING.md) describes. Other hosts, including Linux x86_64, are not validated.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the test, format, lint, and benchmark commands.

## Contributing

Fernq is at a very early stage, so substantial implementation work should generally begin with discussion or investigation rather than a large unsolicited pull request.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development and contribution guidelines.

## Security

Please see [SECURITY.md](SECURITY.md) for information about reporting security vulnerabilities.

## License

Fernq is licensed under the [Apache License 2.0](LICENSE).

Unless explicitly stated otherwise, contributions submitted to this repository are provided under the same license.
