# Linux validation environment for the Fernq workspace.
# The source tree is mounted read-only at /src at run time.

# rust:1.99.0-slim-trixie
FROM rust@sha256:01dd4f9c24801cfc8ba9cf8a5dd6dcca451cd17d1ae73574edc22591de6e6816

WORKDIR /src

# Without an argument, rustup installs the toolchain from rust-toolchain.toml.
COPY rust-toolchain.toml ./
RUN rustup toolchain install

# Keep build output out of the read-only source mount.
ENV CARGO_TARGET_DIR=/tmp/target

CMD ["sh", "-ec", "\
rustup --version; \
rustc -Vv; \
cargo -V; \
cargo fmt --all --check; \
cargo clippy --workspace --all-targets --locked -- -D warnings; \
cargo build --workspace --locked; \
cargo test --workspace --locked; \
cargo bench --workspace --locked --no-run"]
