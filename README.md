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

Performance is one of Fernq's primary goals. Fernq publishes no benchmark results.

[Performance](docs/performance/README.md) defines the performance classes, the rule for comparison with other compilers, and the benchmark tiers.

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

Run the compiler with an input file, an output path, and the Rust edition of the input:

```text
fernq <INPUT> -o <OUTPUT> --edition <EDITION>
```

Compilation is not implemented. `fernq` loads and lexes the input, reports one diagnostic on stderr, exits with status 1, and writes no output. [docs/cli.md](docs/cli.md) is the command-line reference.

Builds are validated on macOS aarch64, and on Linux aarch64 in the Debian-based container that [CONTRIBUTING.md](CONTRIBUTING.md) describes. Other hosts, including Linux x86_64, are not validated.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the test, format, lint, and benchmark commands.

## Documentation

- [Command Line](docs/cli.md): arguments, input loading, lexing, diagnostics, and exit statuses.
- [Compiler Session](docs/compiler-session.md): the state and inputs of one compilation.
- [Performance](docs/performance/README.md): performance classes, comparison rule, benchmark tiers, and result context.
- [Lexing Performance](docs/performance/lexing.md): the lexer as a benchmark subject, with planned measurements and workloads.

## Contributing

Fernq is at a very early stage, so substantial implementation work should generally begin with discussion or investigation rather than a large unsolicited pull request.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development and contribution guidelines.

## Security

Please see [SECURITY.md](SECURITY.md) for information about reporting security vulnerabilities.

## License

Fernq is licensed under the [Apache License 2.0](LICENSE).

The Unicode character tables in `crates/fernq/src/unicode/tables.rs` are generated from Unicode Character Database data, which is distributed under the [Unicode License v3](LICENSE-UNICODE).

Unless explicitly stated otherwise, contributions submitted to this repository are provided under the same license.
