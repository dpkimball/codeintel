# syntax=docker/dockerfile:1.6
# Produces /out/bin/codeintel. Requires --build-context bindb=../bindb.

FROM rust:1-slim-bookworm AS build
WORKDIR /src
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*
COPY --from=bindb Cargo.toml Cargo.lock README.md ./bindb/
COPY --from=bindb src ./bindb/src
COPY --from=bindb benches ./bindb/benches
COPY Cargo.toml Cargo.lock README.md build.rs ./codeintel/
COPY src ./codeintel/src
COPY profiles ./codeintel/profiles
WORKDIR /src/codeintel
ENV GITHUB_SHA=docker
RUN cargo install --locked --path . --root /out \
  && strip /out/bin/codeintel || true

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates git \
  && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/bin/codeintel /out/bin/codeintel
