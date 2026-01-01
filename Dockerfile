# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.92
ARG APP_DIR=/app

FROM rust:${RUST_VERSION}-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*

# Cache deps
COPY Cargo.toml Cargo.lock ./
COPY crates/wordnet-types/Cargo.toml crates/wordnet-types/
COPY crates/wordnet-db/Cargo.toml crates/wordnet-db/
COPY crates/wordnet-morphy/Cargo.toml crates/wordnet-morphy/
COPY crates/crosswordsolver-jw/Cargo.toml crates/crosswordsolver-jw/
COPY xtask/Cargo.toml xtask/
# Minimal sources so manifests are valid during fetch
COPY crates/wordnet-types/src crates/wordnet-types/src
COPY crates/wordnet-db/src crates/wordnet-db/src
COPY crates/wordnet-morphy/src crates/wordnet-morphy/src
COPY crates/crosswordsolver-jw/src crates/crosswordsolver-jw/src
COPY xtask/src xtask/src
RUN cargo fetch

# Build
COPY . .
RUN python3 download_wordnet.py
RUN cargo build --release --bin crosswordsolver -p crosswordsolver-jw

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/crosswordsolver /app/crosswordsolver
COPY --from=builder /app/words.txt /app/words.txt
COPY --from=builder /app/open_english_wordnet_2024/oewn2024 /app/wordnet

ENV HOST=0.0.0.0
ENV PORT=8080
ENV WORDLIST_PATH=/app/words.txt
ENV WORDNET_DIR=/app/wordnet
ENV RUST_LOG=info

EXPOSE 8080
CMD ["./crosswordsolver"]
