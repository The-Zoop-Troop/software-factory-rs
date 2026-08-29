# syntax=docker/dockerfile:1.7
# rig-web-e2e: the node runtime plus Playwright with Chromium, for UI verification.
ARG BASE=factory-rig:node
FROM ${BASE}
ARG PLAYWRIGHT_VERSION=1.62.1
ENV PLAYWRIGHT_BROWSERS_PATH=/work/cache/ms-playwright
USER root
RUN npx -y "playwright@${PLAYWRIGHT_VERSION}" install-deps chromium && rm -rf /var/lib/apt/lists/* /root/.npm && chown -R factory:factory /work/cache
USER factory
RUN npx -y "playwright@${PLAYWRIGHT_VERSION}" install chromium && ls "$PLAYWRIGHT_BROWSERS_PATH" | head -2
ENV RIG_RUNTIME=web-e2e
