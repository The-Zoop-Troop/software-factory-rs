# syntax=docker/dockerfile:1.7
# rig-go: the base plus the Go toolchain and golangci-lint.
ARG BASE=factory-rig:base
FROM ${BASE}
ARG GO_VERSION=1.27.0
ARG GOLANGCI_VERSION=2.13.2
ENV GOPATH=/work/cache/go GOMODCACHE=/work/cache/go/pkg/mod GOCACHE=/work/cache/go-build GOFLAGS=-mod=mod \
    PATH=/home/factory/.local/go/bin:/work/cache/go/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN mkdir -p /home/factory/.local/go \
    && curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" | tar -xz --strip-components=1 -C /home/factory/.local/go \
    && curl -fsSL "https://github.com/golangci/golangci-lint/releases/download/v${GOLANGCI_VERSION}/golangci-lint-${GOLANGCI_VERSION}-linux-amd64.tar.gz" \
      | tar -xz --strip-components=1 -C /home/factory/.local/bin "golangci-lint-${GOLANGCI_VERSION}-linux-amd64/golangci-lint" \
    && go version && golangci-lint --version
ENV RIG_RUNTIME=go
