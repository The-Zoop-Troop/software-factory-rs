# syntax=docker/dockerfile:1.7
# rig-node: the base plus Node LTS and pnpm (npm/yarn via corepack).
ARG BASE=factory-rig:base
FROM ${BASE}
ARG NODE_VERSION=24.20.0
ARG PNPM_VERSION=11.24.0
# Node lives under /usr/local so a sandboxed shell with a reset PATH still finds it; only the
# package caches live on the cache volume.
ENV PNPM_HOME=/work/cache/pnpm COREPACK_HOME=/work/cache/corepack \
    npm_config_cache=/work/cache/npm \
    PATH=/work/cache/pnpm:/usr/local/bin:/usr/bin:/bin
USER root
RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ --strip-components=1 -C /usr/local \
    && corepack enable --install-directory /usr/local/bin \
    && chown -R factory:factory /work/cache
USER factory
RUN corepack prepare "pnpm@${PNPM_VERSION}" --activate \
    && node --version && npm --version && pnpm --version && test "$(command -v node)" = /usr/local/bin/node
ENV RIG_RUNTIME=node
