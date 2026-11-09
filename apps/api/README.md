# Buslane API

Nitro's Node server preset with a bounded SQLite query-worker pool. See the root README for production deployment, API compatibility, pagination and verification.

`TIMETABLES_DB_PATH` and `AUTH_KEY` are required. Seed the database with the Rust importer before starting the API. The API validates database identity, SQLite version, schema version and initial import readiness; it never performs migrations or writes timetable data.

`bun run build` builds the HTTP server and its standalone query worker. Start it with Node 24 using `node .output/server/index.mjs`. Set `TIMETABLES_WORKER_PATH` when the output is installed in a different directory; Compose supplies it automatically.

Default settings are two workers, a 32-request pending queue, a 30-second total query deadline and a 32 MiB response cache per worker. `TIMETABLES_WORKERS` accepts 1–4; `TIMETABLES_CACHE_BYTES` adjusts the per-worker cache budget.

The shared Drizzle schema describes physical storage. Repository projections in `db/repository.ts` expose public IDs and preserve JSON types and nested structures. Large collections and nested route-trip lists stream bounded batches.

`bun run test` builds the server and runs schema, repository, worker and HTTP integration tests. Build the Rust importer first; root `bun run test` does this automatically.

Optional `FARES_DB_PATH` enables the fares catalogue and timetable-linked fares routes. Seed it with `apps/fares` first. `FARES_WORKERS` defaults to 1, `FARES_CACHE_BYTES` to 8 MiB; `FARES_WORKER_PATH` locates the additional built worker. See [the fares README](../fares/README.md) for endpoints, filters, incremental behavior and pricing semantics. Build both Rust importers before running the API tests.

`GET /api/timetables/agencies/:agencyId/geometry` serves precomputed operator GeoJSON with generation-bound ETags. See the [endpoint and rebuild guide](../../docs/operator-geometry.md) for its contract, schema-2 upgrade procedure and bus.rip integration.

`GET /api/fares/operators/:noc/geometry` serves precomputed fare-area coverage with the catalogue ETag and generation headers. Fares schema 3 and a map rebuild using timetable coordinates are required; see the [operator fares map guide](../../docs/operator-fare-geometry.md).
