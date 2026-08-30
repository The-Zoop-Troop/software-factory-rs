# syntax=docker/dockerfile:1.7
# rig-rust: the base plus a pinned Rust toolchain and the test tooling the factory's own repo uses.
ARG BASE=factory-rig:base
FROM ${BASE}
ARG RUST_VERSION=1.97.1
ARG BINSTALL_VERSION=1.22.0
ENV RUSTUP_HOME=/work/cache/rustup CARGO_HOME=/work/cache/cargo \
    PATH=/work/cache/cargo/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends gcc libc6-dev pkg-config libssl-dev protobuf-compiler \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain "${RUST_VERSION}" \
        --profile minimal --component rustfmt,clippy,llvm-tools-preview \
    && curl -fsSL "https://github.com/cargo-bins/cargo-binstall/releases/download/v${BINSTALL_VERSION}/cargo-binstall-x86_64-unknown-linux-musl.tgz" | tar -xz -C /work/cache/cargo/bin \
    && cargo binstall --no-confirm cargo-nextest cargo-llvm-cov \
    && cargo --version && cargo nextest --version | head -1
ENV RIG_RUNTIME=rust
