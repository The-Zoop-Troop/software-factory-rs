# syntax=docker/dockerfile:1.7
# rig-polyglot: rust + python (uv) + node (pnpm) on one image, for monorepos. Large; opt in.
ARG BASE=factory-rig:rust
FROM ${BASE}
ARG UV_VERSION=0.12.7
ARG PYTHON_VERSION=3.13
ARG NODE_VERSION=24.20.0
ARG PNPM_VERSION=11.24.0
ENV UV_CACHE_DIR=/work/cache/uv UV_PYTHON_INSTALL_DIR=/work/cache/uv-python UV_LINK_MODE=copy \
    PNPM_HOME=/work/cache/pnpm COREPACK_HOME=/work/cache/corepack npm_config_cache=/work/cache/npm \
    PATH=/work/cache/pnpm:/home/factory/.local/node/bin:/work/cache/cargo/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends build-essential libffi-dev && rm -rf /var/lib/apt/lists/*
USER factory
RUN curl -fsSL "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-x86_64-unknown-linux-gnu.tar.gz" | tar -xz --strip-components=1 -C /home/factory/.local/bin \
    && uv python install "${PYTHON_VERSION}" && uv tool install ruff && uv tool install pytest \
    && mkdir -p /home/factory/.local/node && curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" | tar -xJ --strip-components=1 -C /home/factory/.local/node \
    && corepack enable --install-directory /home/factory/.local/bin && corepack prepare "pnpm@${PNPM_VERSION}" --activate \
    && cargo --version && uv --version && node --version && pnpm --version
ENV RIG_RUNTIME=polyglot
