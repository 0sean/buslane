FROM rust:1.94-bookworm AS builder
WORKDIR /app/apps/timetables
COPY apps/timetables/Cargo.toml apps/timetables/Cargo.lock ./
COPY apps/timetables/src ./src
COPY packages/timetable-db/migrations /app/packages/timetable-db/migrations
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home buslane \
    && mkdir -p /data /scratch && chown buslane:buslane /data /scratch
COPY --from=builder /app/apps/timetables/target/release/timetables /usr/local/bin/timetables
USER buslane
ENV TIMETABLES_DB_PATH=/data/timetables.sqlite TIMETABLES_SCRATCH_PATH=/scratch TMPDIR=/scratch SQLITE_TMPDIR=/scratch
ENTRYPOINT ["timetables"]
