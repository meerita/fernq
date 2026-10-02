# Entry points for the workspace checks and the Linux validation container.
# CONTRIBUTING.md documents the underlying commands.

IMAGE      ?= fernq-linux-check
PLATFORM   ?= linux/arm64
DOCKERFILE := docker/linux-check.Dockerfile
BASE_IMAGE := $(shell sed -n 's/^FROM //p' $(DOCKERFILE))

CARGO_BUILD  := cargo build --workspace --locked
CARGO_TEST   := cargo test --workspace --locked
CARGO_FMT    := cargo fmt --all --check
CARGO_CLIPPY := cargo clippy --workspace --all-targets --locked -- -D warnings
CARGO_BENCH  := cargo bench --workspace --locked

DOCKER_RUN := docker run --rm --platform $(PLATFORM) -v "$(CURDIR):/src:ro"

.DEFAULT_GOAL := help

.PHONY: help build test fmt clippy bench check up linux shell check-all scan down

help:
	@echo "Host checks:"
	@echo "  build      cargo build"
	@echo "  test       cargo test"
	@echo "  fmt        cargo fmt check"
	@echo "  clippy     cargo clippy with -D warnings"
	@echo "  bench      cargo bench"
	@echo "  check      fmt, clippy, build, test, bench"
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

check:
	$(CARGO_FMT)
	$(CARGO_CLIPPY)
	$(CARGO_BUILD)
	$(CARGO_TEST)
	$(CARGO_BENCH)

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
