# syntax=docker/dockerfile:1.7
# rig-web-e2e: the node runtime plus Playwright with Chromium, for UI verification.
ARG BASE=factory-rig:node
FROM ${BASE}
ARG PLAYWRIGHT_VERSION=1.62.1
# Outside /work/cache: that path is a volume at runtime and would hide the image's browsers.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
USER root
RUN npx -y "playwright@${PLAYWRIGHT_VERSION}" install-deps chromium && rm -rf /var/lib/apt/lists/* /root/.npm \
    && mkdir -p "$PLAYWRIGHT_BROWSERS_PATH" && chown factory:factory "$PLAYWRIGHT_BROWSERS_PATH"
USER factory
RUN npx -y "playwright@${PLAYWRIGHT_VERSION}" install chromium && ls "$PLAYWRIGHT_BROWSERS_PATH" | head -2 && rm -rf /home/factory/.npm/_npx
ENV RIG_RUNTIME=web-e2e
