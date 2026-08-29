# syntax=docker/dockerfile:1.7
# rig-c-cpp: the base plus gcc/g++/clang, cmake, ninja, ccache, and conan.
ARG BASE=factory-rig:base
FROM ${BASE}
ENV CCACHE_DIR=/work/cache/ccache CONAN_HOME=/work/cache/conan2 CMAKE_C_COMPILER_LAUNCHER=ccache CMAKE_CXX_COMPILER_LAUNCHER=ccache \
    PATH=/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential clang cmake ninja-build ccache gdb pkg-config python3 python3-pip python3-venv \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN python3 -m venv /home/factory/.local/conan-venv && /home/factory/.local/conan-venv/bin/pip -q install conan \
    && ln -s /home/factory/.local/conan-venv/bin/conan /home/factory/.local/bin/conan \
    && gcc --version | head -1 && cmake --version | head -1 && conan --version
ENV RIG_RUNTIME=c-cpp
