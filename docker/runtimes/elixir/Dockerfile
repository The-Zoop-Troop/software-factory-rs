# syntax=docker/dockerfile:1.7
# rig-elixir: the base plus Erlang/OTP and Elixir (bookworm), hex and rebar3.
ARG BASE=factory-rig:base
FROM ${BASE}
ENV MIX_HOME=/work/cache/mix HEX_HOME=/work/cache/hex \
    PATH=/work/cache/mix/escripts:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends erlang-base erlang-dev erlang-tools erlang-crypto erlang-ssl erlang-inets elixir build-essential \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN mix local.hex --force && mix local.rebar --force && elixir --version | tail -1
ENV RIG_RUNTIME=elixir
