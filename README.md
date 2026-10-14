# Buslane

Buslane imports the national BODS GTFS timetable feed and serves it through an authenticated Nitro API. Production runs on a 64-bit Raspberry Pi with local USB SSD storage.

The Rust importer is the only database writer. It publishes validated updates in one SQLite WAL transaction while the API's read-only connections continue serving the previous dataset. Public timetable routes and response shapes remain compatible with the previous API.

## Production setup

Use Docker Engine and Compose v2. Build images on a machine with ARM64 build support, then transfer the images and this repository to the Pi:

```sh
docker buildx build --platform linux/arm64 -f apps/api/Dockerfile -t buslane-api:local --output type=docker,dest=/tmp/buslane-api-arm64.tar .
docker buildx build --platform linux/arm64 -f apps/timetables/Dockerfile -t buslane-importer:local --output type=docker,dest=/tmp/buslane-importer-arm64.tar .
```

Load both archives on the Pi with `docker load -i PATH`. For local testing on the builder's architecture, use `docker compose --profile jobs build`.

Place the checkout at `/opt/buslane`, copy `.env.example` to `.env`, and set a strong `AUTH_KEY`. Use a dedicated scratch directory per database. Point the data and scratch directories to the USB SSD and set `BUSLANE_UID`/`BUSLANE_GID` to their owner's IDs. Create both directories with write access for that user. Keep SQLite on a local filesystem, not an NFS/SMB mount.

Seed the database before starting the API:

```sh
docker compose --profile jobs run --rm --no-deps importer
docker compose up -d --no-build api
curl --fail http://127.0.0.1:3000/up
```

The initial national import creates the full database. Subsequent imports reconcile changed files and rows. The importer accepts `--source /path/to/feed.zip` or an unpacked GTFS directory for reproducible imports; mount that path into the importer container when using Compose.

For your existing Cloudflare Tunnel, set the origin for `lane.transit.fyi` to `http://127.0.0.1:3000` when cloudflared runs on the host. If cloudflared runs in Docker, connect it to the Compose network and use `http://api:3000`. Switch the hostname's Worker assignment only after validating the new origin. Retain the previous Worker and PostgreSQL deployment until cutover verification succeeds.

Install the daily import timer, adjusting `WorkingDirectory` if your checkout is elsewhere:

```sh
sudo install -m 644 deploy/systemd/buslane-import.service deploy/systemd/buslane-import.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now buslane-import.timer
systemctl list-timers buslane-import.timer
journalctl -u buslane-import.service
```

The timer preserves the existing **02:00 UTC** schedule and catches up after downtime. A stable database lock prevents overlapping imports and maintenance jobs. Each importer container has a 512 MiB memory limit and one CPU; the API also has a 512 MiB memory limit.

## API compatibility

Send `Authorization: Bearer AUTH_KEY` to all `/api` routes. The existing paths remain:

| Resource | Paths after `/api/timetables` |
| --- | --- |
| Agencies | `/agencies`, `/agencies/:id`, `/agencies/:agencyId/routes`, `/agencies/:agencyId/stops` |
| Routes | `/routes/:id`, `/routes/:routeId/trips` |
| Trips and services | `/trips/:id`, `/services/:id` |
| Stops | `/stops`, `/stops/:stopId/stop_times` |

Agency IDs also accept `noc:CODE`. Responses retain snake-case fields, enum labels, boolean/null values, decimal strings, string dates/timestamps, public foreign keys and GTFS times beyond midnight. Nested stop times are ordered by sequence. Valid interpolated times remain empty strings.

Collection endpoints support optional `?limit=100&cursor=...`. Limits are 1–1000; responses remain arrays. Read the next cursor from `X-Next-Cursor` and pass it unchanged with the same endpoint. Cursors bind to the dataset generation and cannot continue across an import that changes served data. Omit pagination parameters to retain the original full response.

`X-Timetable-Generation` identifies the response snapshot. Each response uses one read transaction, including its generation lookup. Responses larger than 1 MiB stream with backpressure and bypass caching. The default query deadline is 30 seconds including queue time; pagination keeps large transfers manageable.

`POST /api/invalidate` clears the local worker caches and returns `{deleted: number}`. Imports invalidate caches automatically through generation changes.

`GET /up` is unauthenticated and returns `{success:true}` only when the database is compatible and an initial import has completed. Its generation header, API query logs and import reports provide operational visibility. Dataset age measures the last change to served data; use the timer journal to check the last successful feed check.

## Import and storage behaviour

SQL migrations in `packages/timetable-db/migrations` are authoritative. Drizzle definitions describe the same columns, keys, indexes and references; tests compare them against SQLite. Database identity and schema version checks prevent opening an unrelated database. SQL owns strict storage types, checks, deferred foreign keys and `WITHOUT ROWID` options.

High-volume references use internal integer keys. Times use seconds since the GTFS service-day start, dates use YYYYMMDD integers, and first-import timestamps use epoch milliseconds. API projections restore the existing string formats. Source IDs remain unique public identifiers; parent-station references retain their public text IDs to handle forward references without a second import pass.

Stop times, exceptions, frequencies and shape points use natural identities. Numeric child IDs are newly allocated at cutover and stable thereafter. Deleted IDs are not reused. Unchanged rows retain both IDs and first-import timestamps.

The importer hashes uncompressed ZIP members, skips unchanged files and uses prepared typed comparisons to write only changed records and columns, avoiding unnecessary index updates. Compact bitmaps track encountered row IDs without updating every record or writing a full staging copy. Calendar and exception files are reconciled together. Agency-stop membership is refreshed by set differences only for affected agencies.

The parser uses CSV header names, including BODS's `agency_noc` and `vehicle_journey_code` extensions. Missing mandatory files/headers, duplicate identities, invalid values or dangling references fail the update. Optional-file removal deletes that file's previous records. SQLite tracks deferred constraint violations during changes, so daily validation does not rescan every unchanged table. Full integrity/foreign-key scans are available through `--check`. A failed update rolls back; a killed process releases the lock and SQLite recovers the previous commit.

WAL, `synchronous=FULL`, 4 KiB pages, a 128 MiB importer page cache, 32 MiB reader caches and deferred foreign keys are the defaults. Reader drivers open the database read-only and additionally enable `query_only`; the API's database mount is read-only. The importer preserves initialized WAL/shared-memory sidecars so restarts work with that mount. Both engines require SQLite ≥3.51.3. Do not use `immutable=1` against this changing database.

After changed imports, the writer runs bounded query-statistics maintenance and checkpoint attempts. Busy readers defer truncation rather than invalidating the published update. Free pages are reused; full vacuuming is explicit maintenance. The default free-space floor is 512 MiB, checked before and during writes. Reserve additional space for the downloaded ZIP, the database, transient WAL growth and backups.

## Backups and restoration

Create backups with SQLite's online backup API:

```sh
docker compose --profile jobs run --rm --no-deps importer --backup /data/backup.sqlite
docker compose --profile jobs run --rm --no-deps importer --db /data/backup.sqlite --check
```

Backup destinations must not already exist. Copy completed backups to your backup location. Do not copy a live database file alone while committed data may remain in WAL.

To restore, stop the API and the timer, ensure no importer is running, preserve the current database and its sidecars as a recovery set, and install the checked backup as `timetables.sqlite`. Run `importer --check` against it to initialize WAL sidecars and verify integrity, then restart the API and timer. Ensure all files are owned by the configured user. Run `importer --vacuum` only as an explicit maintenance operation with sufficient free space.

## Development and verification

Install Node 24 LTS, Bun 1.3.6 and Rust 1.94 or newer. Then:

```sh
bun install --frozen-lockfile
bun run test
bun run check-types
```

Seed a development database from the small checked-in fixture:

```sh
cargo run --manifest-path apps/timetables/Cargo.toml -- --db "$PWD/data/timetables.sqlite" --source "$PWD/tests/fixtures/gtfs"
TIMETABLES_DB_PATH="$PWD/data/timetables.sqlite" AUTH_KEY=development-token bun run --cwd apps/api dev
```

Tests cover incremental reconciliation, deletions, service unions, validation rollback, database-full failures, a killed writer, locking, ZIP input, header order, reader snapshots, backups, schema agreement, API projections, pagination, cache freshness, streaming, cancellation and the built HTTP server.

Verify the actual container mounts and restart/restore flow in isolated temporary directories:

```sh
bash scripts/test-compose.sh --build
```

Benchmark **on the Pi** with two consecutive complete national archives:

```sh
bun run build
bun run benchmark --feed /ssd/feeds/day-one.zip --feed /ssd/feeds/day-two.zip --output benchmark-results/pi-run
```

The benchmark starts an isolated local API/database, performs an initial import, an identical repeat and each subsequent feed update, and measures warm and concurrent-import operator/trip latency at five clients. It records GNU time CPU/RSS/I/O counters, WAL growth, table/index sizes, query plans and acceptance checks in `results.json`. It requires `/usr/bin/time` and a new output directory. Optional `--agency` and `--trip` select representative public IDs; `--samples` controls the baseline sample count.

Targets are zero timetable writes for identical feeds, importer RSS below 512 MiB, completion before the next daily run, and warm operator/trip p95 below 250 ms at five concurrent clients. Inspect insert/delete counts for upstream identifier churn: incremental gains depend on ID stability. Local fixture tests do not establish national-feed performance on the Pi.
