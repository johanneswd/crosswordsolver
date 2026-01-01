# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.92
ARG APP_DIR=/app

FROM rust:${RUST_VERSION}-bookworm AS builder
WORKDIR /app

# Cache deps
COPY Cargo.toml Cargo.lock ./
COPY crates/crosswordsolver-jw/Cargo.toml crates/crosswordsolver-jw/Cargo.toml
RUN cargo fetch

# Build
COPY . .
RUN cargo build --release --bin crosswordsolver -p crosswordsolver-jw

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/crosswordsolver /app/crosswordsolver
COPY --from=builder /app/words.txt /app/words.txt

ENV HOST=0.0.0.0
ENV PORT=8080
ENV WORDLIST_PATH=/app/words.txt
ENV RUST_LOG=info

EXPOSE 8080
CMD ["./crosswordsolver"]
