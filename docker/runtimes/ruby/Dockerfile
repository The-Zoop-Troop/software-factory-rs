# syntax=docker/dockerfile:1.7
# rig-ruby: the base plus Ruby (bookworm 3.1), bundler, and the native deps Rails apps need.
ARG BASE=factory-rig:base
FROM ${BASE}
ENV BUNDLE_PATH=/work/cache/bundle GEM_HOME=/work/cache/gems \
    PATH=/work/cache/gems/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
        ruby-full build-essential libpq-dev libsqlite3-dev libyaml-dev zlib1g-dev libssl-dev \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN gem install --no-document bundler && ruby --version && bundle --version
ENV RIG_RUNTIME=ruby
