# Precomputed operator fares map

## Confirmed failure mode

bus.rip's operator overview (`FareOverviewMap` → `getFareMapCatalogue` → `collectFareMap`) currently traverses the areas list and then every area's stop-membership pages. Concurrency limits do not reduce Cloudflare's per-invocation subrequest count.

A read-only check of the configured Buslane API on 9 November 2026 found ANEA catalogue generation **8**, **19 areas** and **6,234 area-stop memberships**. At bus.rip's page size of 50, this requires **135 upstream HTTP requests**: one areas page and 134 stop pages. The Network area alone has 4,339 memberships, requiring 87 requests. This exceeds the reported 50-subrequest limit and explains the overview's generic “Fares could not load” error. The historical November 4 benchmark's 242 areas describe a different snapshot; the diagnosis uses the current API counts. The public `/fares/ANEA` URL returned 404 from this development environment, so a deployed browser error was not independently reproduced.

This is a different path from `getFareRouteNetwork`, which fetches timetable representative trips for individual route cards. This change addresses the operator fare-area overview. Fare lists, ticket pricing and journey-rule interpretation remain separate.

## Endpoint and contract

```text
GET /api/fares/operators/ANEA/geometry
Authorization: Bearer AUTH_KEY
```

The endpoint returns one complete operator map; it does not paginate. `generation=N` retains the catalogue's existing generation guard (409 on mismatch). `cursor` and `q` are rejected. The common `limit` parameter has no effect on this complete document.

```json
{
  "areas": { "type": "FeatureCollection", "features": [] },
  "points": { "type": "FeatureCollection", "features": [] },
  "labels": { "type": "FeatureCollection", "features": [] },
  "zones": [{
    "id": "area_…",
    "name": "Example area",
    "membershipComplete": true,
    "stopCount": 40,
    "mappedStopCount": 38
  }],
  "missingAreaIds": [],
  "unmatchedStopCodes": ["unmatched-code"],
  "timetableGeneration": 2,
  "approximate": true
}
```

`areas` contains Polygon features, `points` contains resolved stop Point features and `labels` contains one mean-coordinate Point per mapped zone. Each feature has `{zoneId, name, color, approximate}` properties matching bus.rip's existing map layers. Area IDs remain catalogue IDs, usable with the response's catalogue generation for navigation. Different zones are retained even when names or coordinates coincide. Empty-area operators return empty collections; unknown operators return 404.

The geometry mirrors `fareZoneFeatures`: it normalizes `atco:`/`naptan:` prefixes, deduplicates stop codes per zone, discards blank/non-numeric/non-finite/out-of-range coordinates, and uses a convex envelope only for explicitly complete published membership with at least three non-collinear resolved locations. It preserves the existing colour hash and draws larger envelopes behind smaller ones. Resolved points still appear for incomplete membership or degenerate envelopes. Envelopes use **matched stop locations**, which can be fewer than the published membership; diagnostics disclose that difference. Approximate coverage is never an official fare boundary or evidence that a ticket is valid everywhere inside it. Existing `/areas/:id/geometry` responses about unavailable source boundaries are unchanged.

`missingAreaIds` means areas with no mapped stop coordinates, not failed upstream requests. `unmatchedStopCodes` is a sorted unique list of unresolved or invalid-coordinate memberships across this operator. Conflicting timetable aliases fail generation rather than choosing an arbitrary coordinate.

## Imports, snapshots and caching

Fares schema **3** adds `operator_geometry` and `geometry_state` (migration `003.sql`). The geometry algorithm is version 1; catalogue interpretation stays version 1. The API requires the matching importer/schema.

When `--timetables-db PATH` or `TIMETABLES_DB_PATH` is configured, the fares importer opens that timetable database read-only, validates its identity/schema/readiness and pins a WAL read snapshot after downloading the source. Geometry uses two bulk reads: all timetable stop coordinates and all active catalogue area memberships, with one operator's features accumulated at a time. There are no per-area/per-stop HTTP requests or database read queries. The timetable coordinate lookup is held in memory; allow headroom in addition to the importer's existing caches when using a national stop database.

Changed fares, catalogue projection, map documents, geometry state and fares generation commit in the **same fares transaction**. A map generation/write failure aborts the whole import. A generation is never published with a successful partial map. An identical import with unchanged coordinate generation performs zero writes. Missing/outdated geometry, changed timetable generation or algorithm version triggers regeneration even when fare source checksums are unchanged. A map-only refresh advances the fares generation (invalidating worker caches, ETags and catalogue cursors), while preserving the source `imported_at` timestamp. Ordinary fare changes keep their existing timestamp behavior.

The datasets remain independently published. A map belongs to its **current fares generation** plus the explicit **timetable snapshot generation in its body**. A subsequent timetable import does not invalidate that internally consistent map immediately: it continues serving the saved coordinate snapshot until the next configured fares import or explicit rebuild. Compose's existing daily sequence (02:00 timetable, 04:00 fares) refreshes it automatically. Run a rebuild after an urgent coordinate correction. The API does not join independently changing databases while serving requests.

A standalone fares import without a timetable path remains supported. A changed such import clears map readiness; the geometry endpoint returns **503** until rebuilt. A configured but unreadable/incompatible/unimported timetable database fails the job. `--check` and `--backup` do not require timetable access and do not build maps.

Responses use `X-Fares-Generation`, `X-Catalogue-Version: 1`, `X-Catalogue-Generation`, the existing catalogue ETag, and `Cache-Control: private, no-cache`. Maps are durable cached JSON and also use the generation-keyed worker response cache where size allows. Documents over 1 MiB stream precomputed bytes and bypass the worker response cache. `If-None-Match` supports weak/list/star matching and 304, including large streamed documents; canceled conditional bodies release their worker snapshot. Authentication, generation guards and readiness checks run first. Missing maps return 503, database failures return 500, worker limits retain their existing errors; these error responses are `no-store` and cannot turn into a cacheable successful empty map.

## Existing-database rebuild and later deployment

No deployment was performed. For a later upgrade, use matching schema-3 API and fares-importer images. Take the normal online backup first. Stop the old API while migrating its fares database, then rebuild before starting the new API. The timetable importer does not need changing for this feature.

```sh
cargo build --locked --manifest-path apps/fares/Cargo.toml
apps/fares/target/debug/fares \
  --db /path/to/fares.sqlite \
  --timetables-db /path/to/timetables.sqlite \
  --rebuild-geometry
```

This uses existing catalogue rows and coordinates; it does not download or reparse fares, alter source identities, or require a new national import. The existing fares importer lock covers it. The timetable database must already have a successful import; its schema 1 or 2 is accepted. Free-space checks apply before generation and before commit. Generation failures preserve the last published data. `--rebuild-geometry` cannot be combined with `--check` or `--backup`.

The updated Compose service mounts `TIMETABLES_DATA_DIR` at `/timetables:ro` and sets `TIMETABLES_DB_PATH=/timetables/timetables.sqlite` for the fares importer. With the updated checkout/images, the equivalent command is:

```sh
docker compose --profile jobs run --rm --no-deps fares-importer --rebuild-geometry
```

A normal configured fares import also backfills maps after migration. No timer changes are needed. Restore matching fares data and geometry together; preserve the separately reported timetable generation as provenance. Do not import the operator-only verification database into production.

## Exact bus.rip integration

Only Buslane is modified here. The overview must switch to the new endpoint to remove its fan-out and browser-side geometry construction. Changes are confined to the following bus.rip paths; ticket/route detail handlers can keep their current behavior.

1. Export a `FareMapGeometry` type from `src/lib/fare-map.ts` for the response above. It contains `areas: FeatureCollection<Polygon>`, `points` and `labels: FeatureCollection<Point>`, the `zones` metadata array, diagnostics, and `timetableGeneration: number`.
2. Replace `getFareMapCatalogue` in `src/functions/fares.ts` with one fetch. Keep the existing `operatorCode`, `rateLimit`, `failure` and `CATALOGUE_VERSION` helpers:

```ts
export const getFareMapCatalogue = createServerFn({ method: "GET" })
  .middleware([rateLimit("fare-map")])
  .validator(operatorCode)
  .handler(async ({ data: noc }) => {
    if (process.env.BUSLANE_ENABLED === "0")
      return failure(503, "Fares are temporarily unavailable.");
    try {
      const response = await fetch(
        `${(process.env.BUSLANE_BASE_URL || "http://localhost:3000").replace(/\/$/, "")}/api/fares/operators/${encodeURIComponent(noc)}/geometry`,
        {
          headers: { Authorization: `Bearer ${process.env.BUSLANE_API_KEY}` },
          signal: AbortSignal.timeout(15_000),
        },
      );
      if (!response.ok)
        return failure(503, "The fare map is temporarily unavailable. Please try again.");
      const generation = response.headers.get("x-catalogue-generation");
      if (!generation || !/^[1-9]\d*$/.test(generation) ||
          response.headers.get("x-catalogue-version") !== CATALOGUE_VERSION)
        return failure(503, "The fares catalogue is not ready. Please try again later.");
      const geometry = await response.json();
      return Response.json({
        data: geometry, generation, version: CATALOGUE_VERSION, nextCursor: null,
      }, { headers: { "Cache-Control": "no-store" } });
    } catch {
      return failure(503, "The fare map is temporarily unavailable. Please try again.");
    }
  });
```

3. In `src/data/queries.ts`, change `fareMapOptions` to call `readFareResponse<FareMapGeometry>` instead of `readFareResponse<FareMapArea[]>`. Keep its existing query behavior and error propagation.
4. Add an optional `precomputed?: FareMapGeometry` prop to `src/components/map/FareMap.tsx` and choose `precomputed ?? fareZoneFeatures(zones, stops)` in its `useMemo`, with `precomputed` in the dependencies. Its existing sources, layers, bounding logic, colors and approximate-coverage captions can consume the precomputed collections directly. For the overview, use `precomputed.missingAreaIds.length` for the unmapped-zone count, and expose partial coordinate coverage from the returned counts rather than labelling it an infrastructure failure.
5. In `FareOverviewMap.tsx`, remove its `stopsOptions` query and pass:

```tsx
<FareMap
  noc={noc}
  generation={areas.data.generation}
  zones={areas.data.data.zones.map(zone => ({ ...zone, stops: [] }))}
  stops={[]}
  precomputed={areas.data.data}
  overview
/>
```

Its loading state only depends on the map query; remove the locations error/retry UI. Retain `FareError` for a failed map request. Other consumers of `FareMap` can continue using the existing stop-based fallback. Remove the `collectFareMap` import only if unused elsewhere; do not remove utilities still used by ticket coverage. The map now makes one upstream geometry request instead of traversing the catalogue plus separately loading timetable stop locations. Do not restore a fixed 12-hour success cache; use generation-aware revalidation if adding a proxy cache.

## Verification

```sh
AUTH_KEY=local-test-key node scripts/verify-fare-geometry.mjs \
  /path/to/fares.sqlite http://127.0.0.1:3000 ANEA /tmp/anea-fare-map.json
```

This diagnostic makes exactly one HTTP request, compares its document and generation to the stored map, checks all active operator areas are present, and validates feature IDs, coordinates, polygon rings and diagnostic counts. Run it with the API using the same database and no concurrent import.

The checked-in [report](verification/anea-fare-geometry.json) uses a **local copy of all 19 ANEA areas and all 6,234 memberships read from the configured generation-8 API**, plus the previously available BODS **20260317_034902** ANEA timetable snapshot. A small fixture seeds the local fares database; the copied live catalogue projection is then added for this transport/generation check. It is not a reimport of the complete current ANEA NeTEx publication, and source registry behavior is instead covered by import tests. Older coordinate data leaves some memberships unmatched; the report discloses the counts rather than asserting complete coordinate coverage. The new endpoint was verified locally; it is not deployed or connected to the live bus.rip overview yet.

Tests cover finite/in-range coordinates, prefixed aliases and conflicts, deduplication, incomplete membership, collinear points, polygons and labels, missing coordinates, schema-2 backfill, no-op imports, independent timetable snapshots, atomic rollback, private ETags/304, generation conflicts, readiness/database errors, fixed repository read work, and an operator with 61 areas returned completely in one response.
