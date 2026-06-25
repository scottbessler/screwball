FROM rust:1.90-slim AS build

# webauthn-rs links against system OpenSSL.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

ENV DATA_PATH=/data
ENV PORT=8080

WORKDIR /app

COPY --from=build /app/target/release/screwball /app/screwball
COPY public ./public

ENTRYPOINT ["/app/screwball"]
