FROM rust:1.94-bookworm AS builder
WORKDIR /app/apps/fares
COPY apps/fares/Cargo.toml apps/fares/Cargo.lock ./
COPY apps/fares/src ./src
COPY packages/fares-db/migrations /app/packages/fares-db/migrations
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home buslane \
    && mkdir -p /data /scratch && chown buslane:buslane /data /scratch
COPY --from=builder /app/apps/fares/target/release/fares /usr/local/bin/fares
USER buslane
ENV FARES_DB_PATH=/data/fares.sqlite FARES_SCRATCH_PATH=/scratch TMPDIR=/scratch SQLITE_TMPDIR=/scratch
ENTRYPOINT ["fares"]
