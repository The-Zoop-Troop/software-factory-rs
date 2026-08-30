# syntax=docker/dockerfile:1.7
# rig-base: everything a rig needs except a language toolchain. Runtimes layer on top
# (docker/runtimes/<name>/Dockerfile: FROM factory-rig:base). Everything inside is untrusted.

ARG RUST_VERSION=1.97.1
ARG BD_VERSION=1.2.2
ARG CLAUDE_VERSION=2.1.250
ARG OPENCODE_VERSION=1.18.18
ARG CODEX_VERSION=0.149.1
ARG GH_VERSION=2.98.0
ARG JUST_VERSION=1.58.0
ARG YQ_VERSION=4.53.6

# --- build the web console (this stage never ships) ---
FROM node:24-slim AS ui
WORKDIR /ui
RUN corepack enable
COPY crates/console/ui/package.json crates/console/ui/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY crates/console/ui ./
RUN pnpm build

# --- build the factory binaries (this stage never ships) ---
FROM rust:${RUST_VERSION}-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml rustfmt.toml ./
COPY crates ./crates
COPY docker/console/tokens.toml.example ./docker/console/
COPY --from=ui /ui/dist ./crates/console/ui/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p factory -p stewardd -p console \
    && install -D target/release/factory /out/factory \
    && install -D target/release/stewardd /out/stewardd \
    && install -D target/release/console /out/console

# --- the base runtime ---
FROM debian:bookworm-slim AS base
ARG BD_VERSION
ARG CLAUDE_VERSION
ARG OPENCODE_VERSION
ARG CODEX_VERSION
ARG GH_VERSION
ARG JUST_VERSION
ARG YQ_VERSION

# OS layer: what agents shell out to regardless of language ("agent ergonomics"). Keep boring.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git git-lfs make jq openssl procps \
        ripgrep fd-find shellcheck shfmt sqlite3 unzip xz-utils bash \
    && ln -s /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

# Pinned single-binary tools not in Debian: gh (read-only use), just, yq.
RUN set -eu; cd /tmp; \
    curl -fsSLO "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.tar.gz"; \
    tar -xzf "gh_${GH_VERSION}_linux_amd64.tar.gz"; install -m 0755 "gh_${GH_VERSION}_linux_amd64/bin/gh" /usr/local/bin/gh; \
    curl -fsSLO "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    tar -xzf "just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz" just; install -m 0755 just /usr/local/bin/just; \
    curl -fsSL -o /usr/local/bin/yq "https://github.com/mikefarah/yq/releases/download/v${YQ_VERSION}/yq_linux_amd64"; chmod 0755 /usr/local/bin/yq; \
    rm -rf /tmp/*; gh --version | head -1; just --version; yq --version

# bd: pinned release tarball, checksum-verified (mirrors scripts/install.sh).
RUN set -eu; cd /tmp; \
    base="https://github.com/gastownhall/beads/releases/download/v${BD_VERSION}"; \
    asset="beads_${BD_VERSION}_linux_amd64.tar.gz"; \
    curl -fsSLO "${base}/${asset}"; curl -fsSLO "${base}/checksums.txt"; \
    expected=$(awk -v t="${asset}" '{n=$2; sub(/^\*/,"",n); if (n==t) {print $1; exit}}' checksums.txt); \
    echo "${expected}  ${asset}" | sha256sum -c -; \
    tar -xzf "${asset}"; install -m 0755 bd /usr/local/bin/bd; rm -rf /tmp/*; \
    bd version

RUN groupadd -g 10001 factory && useradd -m -u 10001 -g factory -s /bin/bash factory \
    # Pre-create the volume mount points so fresh named volumes inherit factory's ownership.
    && mkdir -p /work/rig/repo /work/cache && chown -R factory:factory /work
USER factory
ENV HOME=/home/factory PATH=/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin

# claude: the native installer, pinned (installs to $HOME/.local/bin for the invoking user).
RUN curl -fsSL https://claude.ai/install.sh | bash -s "${CLAUDE_VERSION}" && claude --version

# opencode: pinned native installer (providers' npm packages are fetched at runtime).
RUN curl -fsSL https://opencode.ai/install | VERSION="${OPENCODE_VERSION}" bash \
    && ln -s /home/factory/.opencode/bin/opencode /home/factory/.local/bin/opencode \
    && opencode --version

# codex: pinned static binaries (the CLI and its file-editing host).
RUN set -eu; cd /tmp; \
    for a in codex codex-code-mode-host; do \
      curl -fsSLO "https://github.com/openai/codex/releases/download/rust-v${CODEX_VERSION}/${a}-x86_64-unknown-linux-musl.tar.gz"; \
      tar -xzf "${a}-x86_64-unknown-linux-musl.tar.gz"; install -m 0755 "${a}-x86_64-unknown-linux-musl" "/home/factory/.local/bin/${a}"; \
    done; rm -rf /tmp/*; codex --version

COPY --from=build --chmod=0755 /out/factory /out/stewardd /out/console /usr/local/bin/
COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/rig-entrypoint

# /work/rig: .beads + .factory (ledger volume); /work/rig/repo: project clone (repo volume);
# /work/cache: runtime package caches (cache volume).
WORKDIR /work/rig
ENV RIG_RUNTIME=base
ENTRYPOINT ["rig-entrypoint"]
CMD ["shell"]
