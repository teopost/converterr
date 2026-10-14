# syntax=docker/dockerfile:1
FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/converterr /converterr

# Debian's ffmpeg ships x265 with NEON assembly on arm64 (and SIMD on x86_64),
# several times faster than "noasm" builds on a Raspberry Pi.
FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates tini tzdata \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /converterr /usr/local/bin/converterr
ENV CONVERTERR_DATA=/config \
    CONVERTERR_PORT=8787
VOLUME ["/config", "/cache"]
EXPOSE 8787
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/converterr"]
