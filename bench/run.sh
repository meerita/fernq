#!/bin/sh
# Runs the lexer benchmark harness at tier $1, dev or validation, on
# bench/corpus/, and writes bench/results/<UTC time>-<tier>/, which Git
# ignores: environment.txt, samples.tsv, summary.tsv, and sizes.tsv.

set -eu

tier=$1
root=$(cd "$(dirname "$0")/.." && pwd)
corpus="$root/bench/corpus"
out="$root/bench/results/$(date -u +%Y%m%dT%H%M%SZ)-$tier"

if [ ! -f "$corpus/MANIFEST" ]; then
    echo "bench: $corpus/MANIFEST is missing; run make bench-corpus" >&2
    exit 2
fi

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

cargo build --release --locked --manifest-path "$root/bench/Cargo.toml" --bin fernq-bench
mkdir -p "$out"

revision=$(git -C "$root" rev-parse HEAD)
if ! git -C "$root" diff --quiet HEAD -- crates bench; then
    revision="$revision (crates/ or bench/ modified)"
fi

{
    echo "tier: $tier"
    echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "fernq revision: $revision"
    echo "corpus MANIFEST sha256: $(sha256 "$corpus/MANIFEST")"
    echo "build: cargo release profile, Cargo defaults; bench/Cargo.lock"
    echo "measurement: 1 warm-up and 15 timed samples per implementation and workload; a sample covers at least 1 MiB"
    echo "system: $(uname -sm)"
    if [ "$(uname -s)" = Darwin ]; then
        sw_vers
        sysctl machdep.cpu.brand_string hw.model hw.ncpu hw.perflevel0.physicalcpu hw.perflevel1.physicalcpu hw.memsize
    elif [ -r /proc/cpuinfo ]; then
        grep -m 1 'model name' /proc/cpuinfo || true
        echo "cpus: $(grep -c '^processor' /proc/cpuinfo)"
        grep -m 1 MemTotal /proc/meminfo || true
    fi
    rustc -Vv
    cargo -V
} >"$out/environment.txt"

"$root/bench/target/release/fernq-bench" --tier "$tier" --corpus "$corpus" --out "$out"
echo "bench: results in $out"
