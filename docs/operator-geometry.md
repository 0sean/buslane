# Operator timetable geometry

`GET /api/timetables/agencies/:agencyId/geometry` returns the complete operator map in one authenticated request. For Arriva North East:

```sh
curl --fail-with-body \
  -H "Authorization: Bearer $AUTH_KEY" \
  http://localhost:3000/api/timetables/agencies/noc:ANEA/geometry
```

Both public agency IDs and `noc:CODE` work with the same resolution as the existing agency routes endpoint. If several agencies have the same NOC, the agency with the first public ID is selected. Pagination is rejected with HTTP 400. The content type is `application/json` because the outer document wraps a GeoJSON collection:

```json
{
  "network": {
    "type": "FeatureCollection",
    "features": [{
      "type": "Feature",
      "id": 7360,
      "properties": { "routeId": 7360 },
      "geometry": {
        "type": "MultiLineString",
        "coordinates": [[[-1.61, 54.97], [-1.60, 54.98]]]
      }
    }]
  },
  "missingRouteIds": []
}
```

Feature IDs, `routeId`, and missing IDs are **public numeric route IDs**, matching the routes API. Features and missing IDs are sorted by route ID. Empty operators return an empty collection and missing list. Routes without any usable line have no feature.

## Geometry and missing data

Version 1 uses stop-to-stop lines, not GTFS shape points. It processes every trip so distinct branches survive even if their shape/direction/headsign group is shared or the shape is absent. Stops are ordered by `stop_sequence`. Longitude and latitude must be nonempty finite numbers in [-180,180] and [-90,90]. Missing stops or invalid coordinates terminate the current segment. Consecutive identical points are removed, and segments shorter than two points are discarded. Trips are always separate. Exact repeated directed lines are deduplicated within each route; reversed lines are retained so both directions survive. Line ordering is deterministic.

`missingRouteIds` contains routes with no trips, or with at least one **representative trip** that has no usable segment. Representatives match bus.rip's existing grouping: `[shape_id, direction_id, headsign]` within each route; the trip with the lexicographically smallest public trip ID represents each group, matching the routes API's trip order. A representative with a gap still has usable geometry if any segment has at least two distinct points. A route can have a feature and also be missing, because one group is usable and another is not. Empty nonrepresentative departures do not mark a route missing.

Missing IDs describe timetable content only. Database, generation and serialization failures never become missing IDs. The importer aborts its transaction on generation failure. Missing/outdated cached geometry returns HTTP 503, database failures return HTTP 500, and worker overload/deadlines retain their existing error statuses. These errors use `Cache-Control: no-store`. Unknown agencies return HTTP 404.

## Publication and caches

Migration `002.sql` raises the timetable schema to 2 and adds `agency_geometry`. The Rust importer is still the only writer. A single ordered bulk database scan generates documents, buffering one operator's deduplicated geometry at a time; it performs no per-trip database queries or HTTP requests. SQLite can use temporary disk space for sorting. Each agency document stores its source dataset generation and geometry algorithm version. The API reads the resolved agency and one persisted document, irrespective of trip count.

Every import that changes served data rebuilds geometry for all operators before committing the timetable rows, dataset generation, source checksums and geometry together in one WAL transaction. Existing read snapshots continue to see the old data and geometry. Geometry failure rolls the entire update back. Identical imports with current geometry perform no writes. Imports also detect absent or outdated geometry and backfill it even if feed checksums are unchanged.

The endpoint uses the existing generation-keyed worker cache (default 32 MiB per worker), plus the durable precomputed documents. Responses over 1 MiB bypass the in-memory response cache and stream the precomputed document; geometry is fully generated before any HTTP response starts.

Responses include `X-Timetable-Generation`, `ETag: W/"geometry-v1-…"` and `Cache-Control: private, no-cache`. Authenticated clients can retain the body and send `If-None-Match`; matching versions return HTTP 304 with no body. The weak ETag hashes the dataset generation and requested agency identity, with an algorithm prefix. Any timetable generation change invalidates it, including unrelated operator updates; it is deliberately conservative. Generation lookup, document lookup and worker caching use the same read snapshot. Error responses and readiness checks happen before conditional validation. Authorization runs for conditional requests too. Shared unauthenticated CDN caching is not enabled.

An explicit rebuild advances the global generation, invalidating response caches and cursors, but preserves `imported_at` because the source timetable has not changed. Algorithm changes must bump the Rust `ALGORITHM_VERSION`, API document-version predicate and ETag prefix together, then rebuild. Restore a backup with its matching documents; do not copy geometry across unrelated databases.

## Existing imports and rebuilds

Use the new importer and API together because schema 2 is incompatible with older API binaries. Before starting the new API, run the new importer against the existing database:

```sh
cargo build --manifest-path apps/timetables/Cargo.toml --locked
apps/timetables/target/debug/timetables \
  --db /path/to/timetables.sqlite --rebuild-geometry
```

Or, with the new image built through the existing process:

```sh
docker compose --profile jobs run --rm --no-deps importer --rebuild-geometry
```

This acquires the existing importer lock, migrates schema 1, reads existing timetable data without downloading GTFS, builds all operators and atomically advances the generation. It requires a previously successful timetable import. Keep sufficient SSD space for sorting and WAL growth; the existing free-space floor is checked before the rebuild. A normal successful import also performs the backfill. `--check`, `--backup` and `--vacuum` migrate the schema but do not build geometry; use the explicit rebuild before serving the new endpoint. Rebuilds can be repeated after repairs. A failed rebuild leaves the last committed timetable generation and geometry intact. No API-side fallback computes partial geometry.

## Exact bus.rip integration change

In `src/functions/buslane.ts`, replace the **entire** `getRouteNetwork` implementation with:

```ts
type OperatorGeometry = {
  network: RouteNetwork;
  missingRouteIds: number[];
};

export const getRouteNetwork = createServerFn({ method: "GET" })
  .middleware([rateLimit("route-network")])
  .validator(operatorCode)
  .handler(async ({ data: nocCode }) => {
    const result = await get<OperatorGeometry>(
      `/timetables/agencies/noc:${encodeURIComponent(nocCode)}/geometry`,
    );
    if ("error" in result) throw new Error(result.error);
    return result;
  });
```

The existing `get` helper supplies the bearer token, timeout and non-2xx error handling. This removes the routes request, representative-trip tasks, concurrency loop, coordinate processing and catch-as-missing behavior. `OperatorRouteMap` already accepts exactly this result. Keep `representativeTrips` and `routeLines` imports if other handlers still use them. No bus.rip files are changed by this Buslane implementation.

The snippet removes the route-network handler's fixed 43,200-second response cache so import changes/errors are visible. If caching is later added there, persist the body and upstream ETag together and revalidate with `If-None-Match`; serve the retained body on 304, replace both on 200, and propagate errors. Never convert an upstream exception into a successful empty network. Other handlers' caching and fare-route geometry remain separate work.

## One-request ANEA verification

With the built local API pointing at the same imported database, run:

```sh
AUTH_KEY=local-test-key node scripts/verify-operator-geometry.mjs \
  /path/to/timetables.sqlite http://127.0.0.1:3000 noc:ANEA \
  /tmp/anea-geometry.json
```

The script makes exactly one HTTP request (redirects are rejected), compares the document and generation against SQLite, checks every operator route is accounted for, checks public feature IDs, coordinates and deduplication, and writes an optional report. Do not import concurrently during this diagnostic.

The checked-in [verification report](verification/anea-geometry.json) was obtained locally on 9 November 2026 from the complete ANEA subset of the available BODS feed **20260317_034902**: 93 routes, 15,245 trips and 663,199 stop times, including parent stops and associated calendars. The import took approximately 30 seconds with 76 MiB peak RSS. ANEA's 269 original representative groups in this archive became **93 route features, 432 distinct lines, 18,799 points, no missing routes**, in one 422,883-byte HTTP response (99 ms on this development machine). These counts differ from the reported production feed's 258 representatives because this is an older local archive. This verifies the full operator in that archive, not the current production dataset. No deployment was performed.
