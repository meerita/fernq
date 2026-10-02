# Contributing to Fernq

Thank you for your interest in Fernq.

Fernq is an independent compiler for the Rust programming language and is currently in early development.

Because foundational compiler decisions can have long-term consequences, contributions should prioritize correctness, evidence, and focused changes over implementation speed.

## Before Contributing

For small fixes, documentation improvements, tests, and clearly scoped changes, opening a pull request directly is generally fine.

For substantial work such as:

- compiler architecture
- parsing
- name resolution
- type checking
- trait solving
- intermediate representations
- incremental compilation
- optimization
- code generation
- diagnostics architecture
- dependency tracking
- persistent compiler state
- compatibility strategy
- major dependencies

please open an issue or discussion first.

Large implementations based on assumptions that have not yet been agreed on may not be accepted even if the implementation itself works.

## Development Principles

### Correctness comes first

Compiler behavior must be supported by tests, specifications, or reproducible evidence.

Do not trade language correctness for performance.

### Investigate before designing

Major architectural decisions should begin by understanding:

1. the problem
2. Rust language requirements
3. ecosystem compatibility requirements
4. existing approaches
5. possible alternatives
6. measurable tradeoffs
7. unresolved questions

Implementation should follow evidence rather than precede it.

### Do not blindly reproduce `rustc`

`rustc` is an essential compatibility and implementation reference.

It is not automatically the architecture Fernq should reproduce.

At the same time, Fernq should not intentionally diverge from Rust semantics merely because another implementation would be easier.

### Keep changes focused

Pull requests should solve one coherent problem.

Avoid combining unrelated:

- features
- refactors
- formatting
- dependency updates
- performance changes
- architectural changes

Smaller changes are easier to review, validate, benchmark, and revert.

## Rust

Fernq is written in Rust.

Unless documented otherwise:

- use the repository's pinned Rust toolchain
- keep the project warning-free
- format code with `rustfmt`
- keep Clippy clean
- avoid unnecessary allocations and cloning
- avoid unnecessary synchronization
- avoid unnecessary dependencies
- avoid `unsafe` unless there is a demonstrated need

Every `unsafe` block must document the invariants that make it safe. The Clippy check rejects an `unsafe` block without a `// SAFETY:` comment.

## Dependencies

New dependencies should have a concrete justification.

Before adding one, consider:

- whether the functionality is small enough to implement locally
- maintenance history
- licensing
- dependency tree impact
- compile-time impact
- security history
- whether optional features can be disabled
- whether the dependency is appropriate for compiler infrastructure

Do not enable dependency features that Fernq does not use.

## Tests

New behavior should normally include tests.

Do not weaken or delete a valid test merely to make an implementation pass.

If an existing test is believed to be incorrect, demonstrate why before changing it.

Relevant validation should remain green before a pull request is considered complete.

Run the baseline checks from the repository root:

```sh
cargo build  --workspace --locked
cargo test   --workspace --locked
cargo fmt    --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo bench  --workspace --locked
```

Fernq has no hosted CI. Run the checks locally before you open a pull request.

To run the same checks on Linux in a container, install Docker and run from the repository root:

```sh
docker build --platform linux/arm64 -f docker/linux-check.Dockerfile -t fernq-linux-check .
docker run --rm --platform linux/arm64 -v "$PWD:/src:ro" fernq-linux-check
```

The container uses the toolchain that `rust-toolchain.toml` pins. It mounts the source tree read-only and keeps build output inside the container. It compiles benchmarks with `--no-run` and does not execute them.

The container is validated on an aarch64 host, where it runs natively. On other host architectures, Docker must emulate `linux/arm64`. That setup is not validated.

The root `Makefile` runs the same commands. `make` with no target lists the targets. The main targets are:

```text
make check       host checks: fmt, clippy, build, test, bench
make up          build the Linux container image
make linux       run the checks in the Linux container
make check-all   host checks, then the Linux container
make down        remove the Linux container image
```

`make scan` scans the pinned base image with Docker Scout and requires `docker login`.

Additional compiler, integration, compatibility, or benchmark validation may be required depending on the change.

## Performance Changes

Performance work must include enough information to reproduce the result.

Where applicable, document:

- hypothesis
- benchmark
- workload
- environment
- baseline
- implementation change
- result
- variance
- regression checks
- conclusion

Do not describe a change as faster, cheaper, more efficient, or more scalable without measurements that support the claim.

Microbenchmarks should not be treated as proof of end-to-end compiler performance.

## Compatibility

When reproducing behavior observed in `rustc`, try to determine whether the behavior is:

1. required by the Rust language
2. required for practical ecosystem compatibility
3. an implementation detail of `rustc`

These are not always the same thing.

Compatibility behavior should have tests whenever practical.

## Documentation

Documentation, source comments, commit messages, and other public project material should use American English.

Prefer precise technical language.

Avoid marketing language in technical documentation.

Document current behavior separately from:

- proposals
- hypotheses
- experiments
- planned functionality

## Pull Requests

A good pull request should explain:

- what problem it addresses
- why the change is necessary
- what changed
- how it was validated
- relevant compatibility implications
- relevant performance implications
- unresolved limitations, if any

Do not claim that a task is complete if relevant validation is still failing.

## Commit Messages

Keep commits logically coherent.

Prefer messages that describe the actual change:

```text
parser: reject invalid lifetime syntax

resolver: cache successful module lookups

bench: add incremental import workload
```

Avoid vague messages such as:

```text
fix stuff
updates
changes
cleanup
```

## Generated and Vendored Files

Do not manually modify generated files.

Do not commit build artifacts, local configuration, credentials, profiling dumps, temporary data, or machine-specific files.

## Licensing

By contributing to Fernq, you agree that your contributions will be licensed under the Apache License 2.0 used by the project.
