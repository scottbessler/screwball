FROM rust:1.85-slim AS build

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

RUN cargo build --release

FROM debian:bookworm-slim

ENV DATA_PATH=/data
ENV PORT=8080

WORKDIR /app

COPY --from=build /app/target/release/screwball /app/screwball
COPY public ./public

ENTRYPOINT ["/app/screwball"]
