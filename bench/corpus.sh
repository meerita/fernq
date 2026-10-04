#!/bin/sh
# Builds the lexer benchmark corpora in bench/corpus/, which Git ignores.
#
# - synthetic/: the workloads of bench/src/bin/corpus-gen.rs;
# - real/<crate>-<version>/src/: the .rs files of three pinned crates.io
#   archives, each verified against its SHA-256 before extraction;
# - real/fernq-<commit>/: the Fernq source at FERNQ_CORPUS_REV (default
#   5b0b04a), exported with git archive;
# - MANIFEST: every file with its size, SHA-256, edition, source, and label.
#
# Doc comments in real/ are rewritten as plain comments of the same length,
# because the Fernq lexer reports doc comments as unsupported. Every file
# must then lex to the end of file with Fernq. Only a verified download
# enters download/; a kept archive is verified again on every run. A mismatch
# stops the run before any extraction.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
corpus="$root/bench/corpus"
bin="$root/bench/target/release"
rev=${FERNQ_CORPUS_REV:-5b0b04a}
normalized="normalized (doc comments rewritten as plain comments of the same length)"

# name, version, SHA-256 of the .crate archive (crates.io index checksum).
crates() {
    cat <<'EOF'
syn 2.0.119 872831b642d1a07999a962a351ed35b955ea2cfc8f3862091e2a240a84f17297
regex-syntax 0.8.8 7a2d987857b319362043e95f5353c0535c1f58eec5336fdfcf626430af7def58
serde 1.0.229 4148590afebada386688f18773da617792bf2ef03ffc1e4cbd2b1d45b023e0ba
EOF
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

# Keeps only regular .rs files under $1.
keep_rust_files() {
    find "$1" ! -type d \( -type l -o ! -name '*.rs' \) -exec rm -f {} +
    find "$1" -type d -empty -delete
}

commit=$(git -C "$root" rev-parse --verify --quiet --short=7 "$rev^{commit}") || {
    echo "corpus: FERNQ_CORPUS_REV '$rev' names no commit" >&2
    exit 2
}

cargo build --release --locked --manifest-path "$root/bench/Cargo.toml" --bins

rm -rf "$corpus/synthetic" "$corpus/real" "$corpus/staging" "$corpus/MANIFEST"
mkdir -p "$corpus/download" "$corpus/staging" "$corpus/real"

seed=$("$bin/corpus-gen" "$corpus/synthetic")

# Verify every archive before extracting any.
while read -r name version sum; do
    archive="$corpus/download/$name-$version.crate"
    if [ -f "$archive" ]; then
        actual=$(sha256 "$archive")
        if [ "$actual" != "$sum" ]; then
            echo "corpus: $archive has SHA-256 $actual, expected $sum; nothing extracted" >&2
            echo "corpus: delete the file to download it again" >&2
            exit 1
        fi
    else
        url="https://static.crates.io/crates/$name/$name-$version.crate"
        echo "corpus: downloading $url"
        curl --proto '=https' --tlsv1.2 -fsSL -o "$archive.part" "$url"
        actual=$(sha256 "$archive.part")
        if [ "$actual" != "$sum" ]; then
            rm -f "$archive.part"
            echo "corpus: $url has SHA-256 $actual, expected $sum; nothing extracted" >&2
            exit 1
        fi
        mv "$archive.part" "$archive"
    fi
done <<EOF
$(crates)
EOF

while read -r name version sum; do
    tar -xzf "$corpus/download/$name-$version.crate" -C "$corpus/staging" "$name-$version/src"
    keep_rust_files "$corpus/staging/$name-$version"
    mv "$corpus/staging/$name-$version" "$corpus/real/$name-$version"
done <<EOF
$(crates)
EOF

git -C "$root" archive --format=tar --prefix="fernq-$commit/" "$commit" \
    crates/fernq/src crates/fernq/tests tools fuzz/fuzz_targets |
    tar -xf - -C "$corpus/staging"
rm -rf "$corpus/staging/fernq-$commit/crates/fernq/tests/fixtures"
keep_rust_files "$corpus/staging/fernq-$commit"
mv "$corpus/staging/fernq-$commit" "$corpus/real/fernq-$commit"
rmdir "$corpus/staging"

"$bin/corpus-normalize" "$corpus/real"

{
    printf '# path\tbytes\tsha256\tedition\tsource\tlabel\n'
    (cd "$corpus" && find synthetic real -type f -name '*.rs' | LC_ALL=C sort) |
        while read -r path; do
            bytes=$(wc -c <"$corpus/$path" | tr -d ' ')
            sum=$(sha256 "$corpus/$path")
            dir=${path#*/}
            dir=${dir%%/*}
            case $path in
            synthetic/*)
                edition=2024 source="corpus-gen seed $seed" label=synthetic
                ;;
            real/fernq-*)
                edition=2024 source="fernq $commit" label=$normalized
                ;;
            real/*)
                archive_sum=$(sha256 "$corpus/download/$dir.crate")
                edition=2021 source="crates.io $dir.crate sha256 $archive_sum" label=$normalized
                ;;
            esac
            printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$path" "$bytes" "$sum" "$edition" "$source" "$label"
        done
} >"$corpus/MANIFEST.part"
mv "$corpus/MANIFEST.part" "$corpus/MANIFEST"

"$bin/corpus-check" "$corpus"
