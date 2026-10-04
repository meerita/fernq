# Entry points for the workspace checks and the Linux validation container.
# CONTRIBUTING.md documents the underlying commands.

IMAGE      ?= fernq-linux-check
PLATFORM   ?= linux/arm64
DOCKERFILE := docker/linux-check.Dockerfile
BASE_IMAGE := $(shell sed -n 's/^FROM //p' $(DOCKERFILE))

CARGO_BUILD  := cargo build --workspace --locked
CARGO_TEST   := cargo test --workspace --locked --no-fail-fast
CARGO_FMT    := cargo fmt --all --check
CARGO_CLIPPY := cargo clippy --workspace --all-targets --locked -- -D warnings
FUZZING_CLIPPY := cargo clippy -p fernq --all-targets --locked --features fuzzing -- -D warnings
FUZZING_TEST   := cargo test -p fernq --lib --locked --features fuzzing -- fuzz::
BENCH_CLIPPY   := cargo clippy -p fernq --all-targets --locked --features bench -- -D warnings
BENCH_TEST     := cargo test -p fernq --lib --locked --features bench -- bench::
CARGO_BENCH  := cargo bench --workspace --locked

# The Unicode table generator is one source file built by rustc, outside Cargo.
GENERATOR      := tools/unicode-tables.rs
UNICODE_TABLES := crates/fernq/src/unicode/tables.rs
TOOLS_DIR      := target/tools
TOOL_CLIPPY    := clippy-driver --edition 2024 -D warnings

# One fuzz session of the lexer. Every session lasts at most 120 seconds.
FUZZ_SECONDS     ?= 120
FUZZ_MAX_SECONDS := 120
FUZZ_CORPUS      := fuzz/corpus/lex
FUZZ_SEEDS       := fuzz/seeds/lex crates/fernq/tests/fixtures/compile-fail

DOCKER_RUN := docker run --rm --platform $(PLATFORM) -v "$(CURDIR):/src:ro"

.DEFAULT_GOAL := help

.PHONY: help build test fmt clippy bench features tools check fuzz unicode-tables up linux shell check-all scan down

help:
	@echo "Host checks:"
	@echo "  build      cargo build"
	@echo "  test       cargo test"
	@echo "  fmt        cargo fmt check"
	@echo "  clippy     cargo clippy with -D warnings"
	@echo "  bench      cargo bench; not part of check"
	@echo "  tools      format check, lint, and unit tests of the Unicode table generator"
	@echo "  features   clippy and entry tests with the fuzzing feature, then the bench feature"
	@echo "  check      fmt, clippy, build, test, features, tools"
	@echo "Fuzzing (needs cargo-fuzz and a C++ compiler; not part of check):"
	@echo "  fuzz       one lexer fuzz session of FUZZ_SECONDS (1 to $(FUZZ_MAX_SECONDS), default 120)"
	@echo "Generated source:"
	@echo "  unicode-tables UCD=<path>  regenerate $(UNICODE_TABLES) from DerivedCoreProperties.txt"
	@echo "Linux container ($(PLATFORM)):"
	@echo "  up         build the $(IMAGE) image"
	@echo "  linux      run the checks in the container"
	@echo "  shell      open a shell in the container"
	@echo "  down       remove the $(IMAGE) image"
	@echo "Combined and maintenance:"
	@echo "  check-all  check, then linux"
	@echo "  scan       scan the base image for critical and high CVEs (needs docker login)"

build:
	$(CARGO_BUILD)

test:
	$(CARGO_TEST)

fmt:
	$(CARGO_FMT)

clippy:
	$(CARGO_CLIPPY)

bench:
	$(CARGO_BENCH)

features:
	$(FUZZING_CLIPPY)
	$(FUZZING_TEST)
	$(BENCH_CLIPPY)
	$(BENCH_TEST)

tools:
	mkdir -p $(TOOLS_DIR)
	rustfmt --edition 2024 --check $(GENERATOR)
	$(TOOL_CLIPPY) -o $(TOOLS_DIR)/unicode-tables $(GENERATOR)
	$(TOOL_CLIPPY) --test -o $(TOOLS_DIR)/unicode-tables-test $(GENERATOR)
	$(TOOLS_DIR)/unicode-tables-test

check:
	$(CARGO_FMT)
	$(CARGO_CLIPPY)
	$(CARGO_BUILD)
	$(CARGO_TEST)
	$(MAKE) features
	$(MAKE) tools

fuzz:
	@# Match the accepted values as text: 1 to 120, no sign or leading zero.
	@case "$(FUZZ_SECONDS)" in \
		[1-9]|[1-9][0-9]|1[01][0-9]|120) ;; \
		*) echo "FUZZ_SECONDS must be a whole number of seconds from 1 to $(FUZZ_MAX_SECONDS), not '$(FUZZ_SECONDS)'" >&2; exit 2 ;; \
	esac
	cargo fuzz build -s none lex
	mkdir -p $(FUZZ_CORPUS) fuzz/artifacts/lex
	@# libFuzzer checks -max_total_time only between inputs, so a watchdog
	@# ends the session at FUZZ_SECONDS of wall time. A session that reaches
	@# the limit found no crash: a crash ends it and writes an artifact.
	@limit=$(FUZZ_SECONDS); soft=$$((limit > 1 ? limit - 1 : 1)); \
	bin=fuzz/target/$$(rustc -vV | sed -n 's/^host: //p')/release/lex; \
	crashes=$$(ls fuzz/artifacts/lex | wc -l); \
	echo "$$bin -max_total_time=$$soft (hard limit $$limit s)"; \
	$$bin -max_total_time=$$soft -print_final_stats=1 -artifact_prefix=fuzz/artifacts/lex/ \
		$(FUZZ_CORPUS) $(FUZZ_SEEDS) & fuzzer=$$!; \
	( sleep $$limit; kill -TERM $$fuzzer 2>/dev/null ) & watchdog=$$!; \
	wait $$fuzzer; status=$$?; \
	if kill $$watchdog 2>/dev/null; then stopped=no; else stopped=yes; fi; \
	if [ "$$(ls fuzz/artifacts/lex | wc -l)" -ne "$$crashes" ]; then \
		echo "the session wrote a crash artifact to fuzz/artifacts/lex" >&2; exit 1; \
	fi; \
	if [ "$$stopped" = yes ]; then echo "the session reached its $$limit s limit"; exit 0; fi; \
	exit $$status

unicode-tables:
	@test -n "$(UCD)" || { echo "usage: make unicode-tables UCD=<path of DerivedCoreProperties.txt>" >&2; exit 2; }
	mkdir -p $(TOOLS_DIR)
	rustc --edition 2024 -D warnings -O -o $(TOOLS_DIR)/unicode-tables $(GENERATOR)
	$(TOOLS_DIR)/unicode-tables "$(UCD)" $(UNICODE_TABLES)

up:
	docker build --platform $(PLATFORM) -f $(DOCKERFILE) -t $(IMAGE) .

linux: up
	$(DOCKER_RUN) $(IMAGE)

shell: up
	$(DOCKER_RUN) -it $(IMAGE) bash

check-all: check linux

scan:
	docker scout cves --only-severity critical,high --platform $(PLATFORM) $(BASE_IMAGE)

down:
	@if docker image inspect $(IMAGE) >/dev/null 2>&1; then \
		docker image rm $(IMAGE); \
	else \
		echo "$(IMAGE) image is not present"; \
	fi
