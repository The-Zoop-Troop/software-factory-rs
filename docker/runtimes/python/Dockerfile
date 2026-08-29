# syntax=docker/dockerfile:1.7
# rig-python: the base plus uv (Python versions, venvs, pyproject/lockfiles), ruff and pytest.
ARG BASE=factory-rig:base
FROM ${BASE}
ARG UV_VERSION=0.12.7
ARG PYTHON_VERSION=3.13
ENV UV_CACHE_DIR=/work/cache/uv UV_PYTHON_INSTALL_DIR=/work/cache/uv-python UV_LINK_MODE=copy \
    PATH=/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends build-essential libffi-dev \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN curl -fsSL "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-x86_64-unknown-linux-gnu.tar.gz" \
      | tar -xz --strip-components=1 -C /home/factory/.local/bin \
    && uv python install "${PYTHON_VERSION}" \
    && uv tool install ruff && uv tool install pytest \
    && uv --version && uv run --python "${PYTHON_VERSION}" python --version
ENV RIG_RUNTIME=python
