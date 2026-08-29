# syntax=docker/dockerfile:1.7
# rig-jvm: the base plus OpenJDK 17 (bookworm LTS), Maven, and Gradle via the wrapper convention.
ARG BASE=factory-rig:base
FROM ${BASE}
ARG MAVEN_VERSION=3.9.16
ENV MAVEN_HOME=/home/factory/.local/maven MAVEN_OPTS=-Dmaven.repo.local=/work/cache/m2 GRADLE_USER_HOME=/work/cache/gradle \
    PATH=/home/factory/.local/maven/bin:/home/factory/.local/bin:/usr/local/bin:/usr/bin:/bin
USER root
RUN apt-get update && apt-get install -y --no-install-recommends default-jdk-headless \
    && rm -rf /var/lib/apt/lists/* && chown -R factory:factory /work/cache
USER factory
RUN mkdir -p "$MAVEN_HOME" && curl -fsSL "https://dlcdn.apache.org/maven/maven-3/${MAVEN_VERSION}/binaries/apache-maven-${MAVEN_VERSION}-bin.tar.gz" \
      | tar -xz --strip-components=1 -C "$MAVEN_HOME" && java -version 2>&1 | head -1 && mvn -v | head -1
ENV RIG_RUNTIME=jvm
