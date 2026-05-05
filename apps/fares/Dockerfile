FROM rust:1-bookworm AS builder

WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl unzip wget libxml2-utils \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/fares /usr/local/bin/fares
COPY entrypoint.sh /usr/local/bin/entrypoint

RUN chmod +x /usr/local/bin/fares
RUN chmod +x /usr/local/bin/entrypoint

ENTRYPOINT ["/usr/local/bin/entrypoint"]
