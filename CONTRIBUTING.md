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
cargo test   --workspace --locked --no-fail-fast
cargo fmt    --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p fernq --all-targets --locked --features fuzzing -- -D warnings
cargo test   -p fernq --lib --locked --features fuzzing -- fuzz::
cargo clippy -p fernq --all-targets --locked --features bench -- -D warnings
cargo test   -p fernq --lib --locked --features bench -- bench::
```

The last four commands check the fuzz entry and the benchmark entry, which the Cargo features `fuzzing` and `bench` enable. Test targets build with `opt-level = 1` (`[profile.test]` in `Cargo.toml`); they keep debug assertions and overflow checks.

Benchmarks are not part of the baseline checks. Run them when a change raises a performance question, with the benchmark tier that question needs ([Performance](docs/performance/README.md); see also [Performance Changes](#performance-changes) and [Lexer benchmark](#lexer-benchmark)).

Fernq has no hosted CI. Run the checks locally before you open a pull request.

To run the same checks on Linux in a container, install Docker and run from the repository root:

```sh
docker build --platform linux/arm64 -f docker/linux-check.Dockerfile -t fernq-linux-check .
docker run --rm --platform linux/arm64 -v "$PWD:/src:ro" fernq-linux-check
```

The container uses the toolchain that `rust-toolchain.toml` pins. It mounts the source tree read-only and keeps build output inside the container. It does not build or run benchmarks or fuzz sessions. It also runs the feature checks that `make features` runs and the Unicode table generator checks that `make tools` runs.

The container is validated on an aarch64 host, where it runs natively. On other host architectures, Docker must emulate `linux/arm64`. That setup is not validated.

The root `Makefile` runs the same commands. `make` with no target lists the targets. The main targets are:

```text
make check         host checks: fmt, clippy, build, test, features, tools
make features      clippy and entry tests with the fuzzing feature, then the bench feature
make bench-corpus  build the lexer benchmark corpora; not part of check
make bench         run the lexer benchmark; not part of check
make fuzz          one lexer fuzz session; not part of check
make tools         format check, lint, and unit tests of the Unicode table generator
make up            build the Linux container image
make linux         run the checks in the Linux container
make check-all     host checks, then the Linux container
make down          remove the Linux container image
```

`make scan` scans the pinned base image with Docker Scout and requires `docker login`.

Additional compiler, integration, compatibility, or benchmark validation may be required depending on the change.

### Fuzzing

`fuzz/` holds the lexer fuzz target, a `cargo-fuzz` crate outside the workspace with its own `Cargo.lock`. Its dependencies are development tools and are not part of the compiler. It needs `cargo-fuzz` and a C++ compiler, and runs on the pinned stable toolchain without a sanitizer:

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
make fuzz                    # one session of 120 seconds
make fuzz FUZZ_SECONDS=30    # a shorter session
```

A fuzz session lasts at most 120 seconds. `FUZZ_SECONDS` must be a whole number from 1 to 120; `make fuzz` rejects any other value before it builds. A watchdog ends the session at that wall time. For more coverage, run more sessions: each one starts from the corpus that the previous ones found.

The fuzz target lexes each input in every edition and fails when a lexer invariant does not hold. Its inputs come from three directories:

- `fuzz/corpus/lex/`: the inputs that sessions find. It stays on your machine and is ignored by Git.
- `fuzz/seeds/lex/`: tracked seed inputs, small pathological and representative programs.
- `crates/fernq/tests/fixtures/compile-fail/`: the compile fixtures.

A crash writes its input to `fuzz/artifacts/lex/` and fails `make fuzz`. To fix it:

1. Minimize the input: `cargo fuzz tmin -s none lex fuzz/artifacts/lex/<crash> -- -max_total_time=110`. libFuzzer checks that time only between inputs, so 110 keeps the run under 120 seconds.
2. Add the minimized input as a named unit test in the module that owns the defect.
3. Fix the defect, then run `make fuzz` again.

`make fuzz` is not part of `make check` or the Linux container. The container has no C++ compiler, so fuzzing is validated on the host only.

### Lexer benchmark

`bench/` holds the lexer benchmark, a crate outside the workspace with its own `Cargo.lock`. Its dependencies are development tools and are not part of the compiler. [Lexing Performance](docs/performance/lexing.md) describes the method: the corpora, the equivalence gate, the reference adapter, and the output files.

```sh
make bench-corpus                       # build bench/corpus/; downloads three crates
make bench                              # dev tier: one run
make bench TIER=validation              # validation tier: two runs, the second in reversed order
make bench-corpus FERNQ_CORPUS_REV=dev  # take the Fernq part of the corpus from another revision
```

`make bench-corpus` needs `curl`, `git`, and `sha256sum` or `shasum`, and network access to `static.crates.io`. It verifies each crate archive against its pinned SHA-256 before it extracts any, and stops on a mismatch. `make bench` needs the corpus, accepts only `TIER=dev` and `TIER=validation` (any other value exits with status 2), and stops before timing when the equivalence gate finds a difference. Results go to `bench/results/<UTC time>-<tier>/`. `bench/corpus/` and `bench/results/` are ignored by Git.

Run one benchmark at a time, with no other build or benchmark on the host. Neither target is part of `make check` or the Linux container.

### Test conventions

Unit tests:

- Place unit tests in a `#[cfg(test)] mod tests` module at the end of the tested module.
- Unit tests do not read or write files. Unit tests of source loading read in-memory input.
- A test name describes the tested behavior.

Integration tests:

- Put one file per behavior in `crates/fernq/tests/`.
- Run the compiler through the `CARGO_BIN_EXE_fernq` path that Cargo provides.

Compile fixtures:

- A compile fixture is one Rust source file per case at `crates/fernq/tests/fixtures/<class>/<name>.rs`. The class directory names the expected result. The file name describes the case.
- Do not put a file directly in `tests/fixtures/`. Cargo builds `tests/<dir>/main.rs` as a test target.
- A fixture can start with the line `// expected-kind: <kind>`, which names the diagnostic kind that the fixture expects. Fixtures have no other directives.
- The fixture runner, `crates/fernq/tests/compile_fixtures.rs`, runs every fixture. A new fixture in an existing class needs no new test code. A new class directory needs an entry in the runner's class list.
- The fixture runner reports a file directly in `tests/fixtures/`, an unknown class directory, a non-`.rs` file in a class directory, and an `// expected-kind:` line that names no kind.

The only fixture class is `compile-fail`: source that Rust rejects. The runner runs each fixture as `fernq <fixture> -o <dir>/<fixture stem> --edition 2024`, where `<dir>` is the runner's `CARGO_TARGET_TMPDIR/<test-name>/` directory. Every fixture expects exit status 1, empty stdout, a diagnostic of its expected kind on stderr, and no file at the output path. The expected kind is the kind in the `// expected-kind:` line, or `compilation-not-implemented` when the fixture has no such line. A fixture that a current stage rejects names that kind, for example `lexical-error`. A fixture whose rejection belongs to a stage that does not exist yet expects `compilation-not-implemented`, which [docs/cli.md](docs/cli.md) describes. The runner identifies the outcome by the diagnostic kind, so a load failure does not pass as another outcome. The runner does not compare message wording.

Temporary output:

- Only integration tests write temporary output.
- A test writes under `CARGO_TARGET_TMPDIR/<test-name>/`. It clears that directory at start and leaves it after the run for inspection.
- A test that creates a large temporary file removes it before its assertions, and its doc comment states why.

Expected output:

- The repository has no snapshot files.
- A test states exact output inline only when exact output is the contract.
- A rejection check asserts the diagnostic kind, for example that stderr contains `error[input-not-found]:`, not the message wording.

Failure output:

- The test command runs with `--no-fail-fast`. One run reports every failing test target.
- The fixture runner reports every mismatch in one failure. Each mismatch shows the fixture path relative to `crates/fernq`, the expected outcome, the exit code, stdout, and stderr.
- Test output uses the default libtest format.

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

`crates/fernq/src/unicode/tables.rs` holds the `XID_Start` and `XID_Continue` tables that identifiers use. `tools/unicode-tables.rs` generates it from `DerivedCoreProperties.txt` of the Unicode Character Database 17.0.0. The repository does not contain that file. To regenerate the tables, download it and run the generator:

```sh
curl -fsSLO https://www.unicode.org/Public/17.0.0/ucd/DerivedCoreProperties.txt
make unicode-tables UCD=DerivedCoreProperties.txt
```

The generator accepts only the pinned file, with SHA-256 `24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08` and first line `# DerivedCoreProperties-17.0.0.txt`. For any other input it writes nothing and exits with a nonzero status. The output depends only on the input, so a regeneration from the pinned file leaves `tables.rs` unchanged. `make check` and the Linux container compile, lint, and test the generator, and do not run it. A change of the Unicode version changes which Rust programs `fernq` accepts, and goes with a change of the Rust version that `fernq` targets.

Do not commit build artifacts, local configuration, credentials, profiling dumps, temporary data, or machine-specific files.

## Licensing

By contributing to Fernq, you agree that your contributions will be licensed under the Apache License 2.0 used by the project.
