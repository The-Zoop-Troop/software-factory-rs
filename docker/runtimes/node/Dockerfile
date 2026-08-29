# syntax=docker/dockerfile:1.7
# rig-node: the base plus Node LTS and pnpm (npm/yarn via corepack).
ARG BASE=factory-rig:base
FROM ${BASE}
ARG NODE_VERSION=24.20.0
ARG PNPM_VERSION=11.24.0
ENV PNPM_HOME=/work/cache/pnpm COREPACK_HOME=/work/cache/corepack \
    npm_config_cache=/work/cache/npm \
    PATH=/work/cache/pnpm:/home/factory/.local/node/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN chown -R factory:factory /work/cache
USER factory
RUN mkdir -p /home/factory/.local/node \
    && curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ --strip-components=1 -C /home/factory/.local/node \
    && corepack enable --install-directory /home/factory/.local/bin \
    && corepack prepare "pnpm@${PNPM_VERSION}" --activate \
    && node --version && pnpm --version
ENV RIG_RUNTIME=node
