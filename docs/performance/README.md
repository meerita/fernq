# Performance

This page defines how Fernq measures performance and how it reports a benchmark result. Component pages apply this model to one compiler component.

Fernq publishes no benchmark results. These pages contain no measurements.

## Performance Classes

Fernq measures performance in separate classes:

| Class | Subject | Example metrics |
|---|---|---|
| Compiler performance | The cost of compiling | wall-clock time, CPU time, time per compiler stage |
| Compiler resource use | The resources that compilation consumes | peak memory, allocations, bytes allocated, I/O, temporary disk use |
| Generated artifact quality | The files that compilation produces | executable size, object size, metadata size, debug information size |
| Generated-program performance | The program that Fernq produces, when it runs | runtime, throughput, memory use |
| Incremental compilation performance | Compilation that reuses valid prior state | rebuild time for each edit class |

Fernq does not combine these classes into one score. A result in one class does not establish a result in another class.

Fernq currently lexes its input and writes no output. [Command Line](../cli.md) states the current behavior. Generated artifact quality and generated-program performance have no Fernq subject until Fernq produces output. Incremental compilation performance has no Fernq subject until Fernq has incremental compilation.

## Separate Measurements

Fernq keeps these measurements separate:

```text
compiler wall time
compiler CPU time
peak compiler memory
incremental rebuild time
generated artifact size
generated-program runtime
generated-program memory
```

The measurements are independent:

- A faster compiler can emit slower code.
- A slower compiler can emit faster code.
- A smaller binary is not automatically a faster binary.
- A microbenchmark improvement is not automatically an end-to-end compiler improvement.

A component benchmark measures one compiler component, for example the lexer. A component result does not establish end-to-end compiler performance.

Clean compilation and incremental compilation are separate workloads. An incremental result names the edit and the reusable starting state.

## Comparison with Other Compilers

Fernq may be compared with another compiler only when both systems perform equivalent work for the measured contract.

Equivalent work means that both systems process the same input under aligned conditions, such as target, edition, optimization level, build mode, cache state, and required outputs, and that both measurements have the same boundary. When a condition cannot be aligned, the result states the difference and its consequence.

Before equivalence exists:

- Fernq measurements are engineering telemetry.
- `rustc` measurements may be recorded as an external baseline.
- The numbers are not a Fernq-vs-`rustc` performance claim.

A comparison measures one class at a time. It names the comparison compiler and its exact version. Both compilers use their normal configuration. When the comparison question requires a tuned configuration, the tuned results stay distinct from the default results.

A performance difference alone does not establish its architectural cause.

## Benchmark Tiers

Every benchmark result has one tier. A comparison uses results from one tier only.

| Tier | Purpose | Use |
|---|---|---|
| Dev | direction, candidate rejection, regression detection, cheap engineering feedback | Default tier during development. |
| Validation | confirm a material result, distinguish close candidates, support optimization acceptance | When a decision depends on the number. |
| Publication | public or durable external claims | Only for such a claim, with the methodology that the claim requires. |

A dev or validation result is engineering evidence. It does not support a public performance claim.

## Result Context

A benchmark result states the applicable context:

- Fernq version or revision;
- `rustc` or other comparison compiler, and its exact version;
- host CPU and architecture;
- operating system;
- target;
- Fernq build profile;
- workload;
- benchmark tier;
- sample count;
- measurement boundary;
- clean or incremental state;
- optimization level, when relevant.

A result keeps raw measurements, derived comparisons such as ratios, and interpretation separate. A ratio does not replace the raw measurements.

## Components

- [Lexing](lexing.md): the lexer as a benchmark subject.
