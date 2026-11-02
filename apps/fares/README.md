# BODS fares importer

The Rust job downloads [BODS bulk fares](https://data.bus-data.dft.gov.uk/fares/download/bulk_archive), traverses folders and nested ZIPs, and publishes a separate `fares.sqlite` database in one WAL transaction. API readers keep their previous committed snapshot during publication. The format follows the [BODS NeTEx fares profile](https://www.pti.org.uk/bus_open_data_fares).

```sh
cargo run --release --locked --manifest-path apps/fares/Cargo.toml -- \
  --db /mnt/ssd/buslane/fares.sqlite --scratch /mnt/ssd/buslane/fares-scratch
```

`--source` accepts an HTTP(S) URL, local ZIP, XML file, or directory containing XML/ZIP files. `FARES_DB_PATH`, `FARES_SCRATCH_PATH`, `FARES_SOURCE` and `MIN_FREE_BYTES` supply defaults: `./data/fares.sqlite`, `./scratch/fares`, the bulk URL, and a 512 MiB free-space floor. Use dedicated scratch storage per database. Put downloads, database, WAL and scratch on a local SSD with room for a complete initial import and transient WAL. The free-space floor is a reserve, not a capacity estimate.

## Incremental publication

SHA-256 checksums cover the outer ZIP, nested ZIPs and XML documents. Unchanged archives skip all descendant decompression and parsing; nested archives with known hashes are checked directly in the outer ZIP without scratch writes. Changed archives are processed one member at a time; nested ZIPs use temporary files, and no archive-supplied path is extracted. Unchanged XML skips parsing and SQL writes. For changed XML, canonical record hashes and link-set differences preserve unchanged rows and IDs. Removed documents and records are deleted. Identical input produces **zero SQLite changes**, keeps the generation, and does not grow WAL or modify database bytes.

For top-level names within the BODS bulk ZIP matching `organisation/datasetId_revisionId_YYYY-MM-DD_HH-MM-SS_name.zip`, the stable dataset ID identifies the archive across revisions. Its first observed path remains its logical namespace, preventing revision-only renames from replacing descendants. Other paths use exact names. Changed XML member names still change document identity; upstream identifier churn can reduce incremental savings.

HTTP conditional requests reuse a complete cached archive. Validators are saved after successful publication. A stable database lock prevents overlapping jobs. Failed downloads preserve the previous complete download; parsing failures, disk-full errors and killed writers preserve the committed dataset. The next CLI run removes interrupted scratch work. Normalization version changes force reparsing.

Limits are eight nested ZIP levels, 64 MiB per XML, 8 GiB per nested ZIP, one million archive members and 100 GiB expanded archive bytes. UTF-8 XML has a one-million-node and 128-level depth limit; DTD/entity expansion is disabled. XML structure, NeTEx namespace, decimal amounts and document ownership are validated. References can legitimately target other frames, versions or external resources; unresolved references remain explicit. Unknown non-XML/non-ZIP members are ignored. Empty input cannot erase the dataset.

## Storage and pricing meaning

`packages/fares-db/migrations/001.sql` is authoritative. Application ID `0x42555352` and schema version 1 distinguish fares from timetables. SQLite >=3.51.3, WAL, `synchronous=FULL`, a 128 MiB writer cache, strict tables and persistent sidecars follow the existing deployment pattern. API connections are read-only with `query_only` enabled.

`documents` stores logical paths, SHA-256, publication timestamp, currency and original XML compressed with raw DEFLATE. XML retention preserves conditions and extensions beyond the projections. `entities` projects products, prices, price groups, directional matrices, zones, offers, profiles, fare-table context and selected rules into indexed rows. `data` is a JSON tree with namespace, element name, sorted attributes, ordered children and meaningful text. Large nested table/matrix containers are omitted from summary rule/table JSON because their records are separately projected; original XML remains complete.

Record identity includes document, element kind, NeTEx ID, version and occurrence. Duplicate IDs do not merge different types or versions. Occurrence order distinguishes repeated definitions of the same type/ID/version; reordering these duplicates can change their associations. Numeric IDs persist for surviving identities and are never reused. Monetary amounts remain decimal strings. Currency comes from the price or nearest frame default; missing currency stays null. No floating-point or currency conversion is performed.

`links` stores operator public codes (NOC for bus operators), public line codes, NaPTAN/ATCO stop codes and zone membership. Shared UIC rail-operator metadata remains explicitly namespaced in document links and is excluded from NOC matching. These match timetable `agencies.noc`, `routes.short_name` with its agency, and `stops.public_id`. The databases have no cross-database foreign keys and can be published independently.

Prices retain `product_ref`, `profile_ref`, `package_ref`, `price_ref`, `matrix_ref` and `group_ref` where present. A table's `pricesFor` supplies context to its nested prices. Matrices retain directional `start_ref` and `end_ref`. The API returns all same-document monetary-price and matrix definitions matching a reference, including multiple versions, keeping ambiguity visible.

This is a catalogue and rule API. Document filters identify **candidates**, not a guaranteed currently valid journey fare. Clients must evaluate validity, passenger eligibility, direction, zones, sales channels and other conditions before quoting. The API does not combine every product and amount in a document into invented fares. Expired and future offers remain accessible with their source conditions.

## API

Set `FARES_DB_PATH` before starting the existing API. Without it, fares routes return 503 and timetables remain available. Routes require the existing bearer token.

| Path after `/api` | Response |
| --- | --- |
| `/fares/documents` | Document metadata array |
| `/fares/documents/:id` | Metadata and operator/line/stop/zone links |
| `/fares/documents/:id/xml` | Original XML (`application/xml`) |
| `/fares/products`, `/fares/products/:id` | Products or product detail |
| `/fares/prices`, `/fares/prices/:id` | Prices or price detail, with reference candidates |
| `/fares/entities`, `/fares/entities/:id` | All projected records or one record |
| `/timetables/agencies/:agencyId/fares` | Product candidates matched by agency NOC |
| `/timetables/routes/:routeId/fares` | Product candidates matched by agency NOC and route short name |
| `/timetables/stops/:stopId/fares` | Product candidates matched by stop public ID |

Collections return arrays, default to 100 records, and support `limit=1..1000`. `X-Next-Cursor` supplies the next keyset cursor. Cursors bind filters, page size and fares generation; generation changes return 409 and require restarting pagination. `X-Fares-Generation` identifies the response snapshot. Linked routes also return `X-Timetable-Generation`: these represent independently published datasets. Missing timetable matching codes return 422.

Exact collection filters: `document_id`, `noc`, `line`, `stop`, `ref`, `product_ref` and (for `/entities`) `category`. `ref`/`product_ref` apply to entities, not document lists. Reference strings are document-scoped; pass `document_id` when resolving them. Categories are `product`, `price`, `price_group`, `matrix`, `zone`, `offer`, `profile`, `table` and `rule`. NOC/line/stop restrict document membership; NOC plus line also checks that line's operator. Linked routes pin their matching filters to the timetable record.

```sh
curl -H "Authorization: Bearer $AUTH_KEY" \
  'http://127.0.0.1:3000/api/fares/products?noc=ONE&line=1&limit=100'
curl -H "Authorization: Bearer $AUTH_KEY" \
  'http://127.0.0.1:3000/api/fares/prices?document_id=1&product_ref=product%3Aadult'
curl -H "Authorization: Bearer $AUTH_KEY" \
  'http://127.0.0.1:3000/api/fares/entities?document_id=1&category=zone'
```

The fares pool defaults to one worker and an 8 MiB cache (`FARES_WORKERS`, `FARES_CACHE_BYTES`). It shares the timetable pool's bounded queue, deadlines, cancellation and streaming. Cache keys include every filter and generation; large responses stream with backpressure and bypass caching. `POST /api/invalidate` clears both configured pools. `/up` retains timetable readiness semantics.

## Deployment, maintenance and verification

```sh
docker compose --profile jobs build
docker compose --profile jobs run --rm --no-deps fares-importer
docker compose up -d --no-build api
```

Compose mounts `FARES_DATA_DIR` at `/fares` read-only in the API and at `/data` in the job; the job uses `FARES_SCRATCH_DIR`. Build `apps/fares/Dockerfile` for ARM64 for the Pi. The `deploy/systemd/buslane-fares-import.service` and `.timer` run daily at 04:00 UTC, after the 02:00 timetable schedule, and catch up after downtime.

`fares --check` verifies integrity and foreign keys. `fares --backup NEW_PATH` uses SQLite's online backup API and refuses existing destinations. To restore, stop API and job, preserve the previous database and sidecars, install the checked backup, run `--check` to initialize sidecars, then restart. Do not copy a live main database alone.

```sh
mkdir -p /mnt/ssd/buslane-tests
TMPDIR=/mnt/ssd/buslane-tests bun run test
bun run check-types
TMPDIR=/mnt/ssd/buslane-tests bash scripts/test-compose.sh --build
python3 scripts/benchmark-fares.py --feed /mnt/ssd/day-one.zip \
  --feed /mnt/ssd/day-two.zip --output /mnt/ssd/fares-import-benchmark
node scripts/benchmark-fares-api.mjs --database /mnt/ssd/fares-import-benchmark/fares.sqlite \
  --output /mnt/ssd/fares-api-benchmark --samples 100
```

Benchmark output directories must be new. The import harness measures initial, identical and subsequent feeds, recording wall/CPU time, per-process peak RSS, disk I/O, row counts and database/WAL sizes. It fails on identical-input SQLite changes or RSS >=512 MiB. The API harness starts an isolated authenticated API with a tiny timetable fixture, inspects plans, and measures cold/warm latency with five concurrent clients. Benchmark on the Pi before making production capacity claims.

The [national benchmark report](../../docs/benchmarks/fares-2026-11-02.md) records the measured first-load storage requirement, zero-write repeat and controlled update performance. `scripts/create-fares-price-change.py` creates a reproducible synthetic update from a single bulk archive. The live downloader test is opt-in because it transfers the entire national archive.
