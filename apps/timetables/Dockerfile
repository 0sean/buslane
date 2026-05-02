FROM rust:1-bookworm AS builder

WORKDIR /app

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends wget unzip ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/timetables /usr/local/bin/timetables
COPY entrypoint.sh /usr/local/bin/entrypoint

RUN chmod +x /usr/local/bin/timetables
RUN chmod +x /usr/local/bin/entrypoint

ENTRYPOINT ["/usr/local/bin/entrypoint"]
