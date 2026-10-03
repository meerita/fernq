# Compiler Session

A compiler session is the state that one compilation holds and the inputs that reach it. This page states the current session contract of the `fernq` driver. Code review enforces the contract. No mechanical check exists.

## Ownership

A process runs at most one compilation. Help and an invalid command line run none.

The function `compile` in `crates/fernq/src/main.rs` owns the compilation. The compilation has:

- one configuration: the `Invocation` that the command-line parser produces. It does not change after parsing.
- one piece of mutable state: the `SourceTable` that `compile` creates and owns. It holds every source file that the compilation loads.

Each stage receives the values it reads as parameters. No value passes through a stage that does not use it. The session is a contract, not a type: no session or context object exists.

```mermaid
flowchart LR
    args["command-line arguments"]
    inv["Invocation"]
    compile["compile"]
    table["SourceTable"]
    stages["stages"]

    args -- "parse" --> inv
    inv -- "&Invocation" --> compile
    compile -- "creates" --> table
    compile -- "parameters" --> stages
    table -- "&SourceFile" --> stages
```

## Process Input and Global State

The command-line arguments are the only process input to a compilation. The input file is read through the path that they name. Compilation reads no environment variable.

Fernq keeps no compiler state in global variables.

`main` installs a panic hook once, before it parses the command line. The hook writes the internal compiler error diagnostic, then runs the previous hook. It holds no compiler state. The previous hook is the default hook of the Rust standard library, which reads `RUST_BACKTRACE`.

## Edition and Target

No stage takes an edition or a target, and no current outcome depends on either. The command line has no edition or target option: `--edition` and `--target` are unknown options.

A stage that depends on the edition or the target receives it as an explicit parameter, from configuration that the driver parses. It does not take the edition or the target from the host, the environment, or the source text.

## Limits

- Edition input, its default, and its accepted values are not defined. They belong to the first stage whose result depends on the edition.
- Target input is not defined. It belongs to target support.
- Configuration and mutable state are not split further. A split belongs to the first stage that needs a second piece of mutable state.
