# syntax=docker/dockerfile:1.7
# rig-php: the base plus PHP 8.2 CLI (bookworm), common extensions, and Composer.
ARG BASE=factory-rig:base
FROM ${BASE}
ARG COMPOSER_VERSION=2.10.3
ENV COMPOSER_HOME=/work/cache/composer COMPOSER_CACHE_DIR=/work/cache/composer/cache \
    PATH=/work/cache/composer/vendor/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
        php-cli php-xml php-mbstring php-intl php-curl php-zip php-sqlite3 php-pgsql \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN curl -fsSL -o /home/factory/.local/bin/composer "https://getcomposer.org/download/${COMPOSER_VERSION}/composer.phar" \
    && chmod 0755 /home/factory/.local/bin/composer && php --version | head -1 && composer --version
ENV RIG_RUNTIME=php
