// One HTTP request against a local API using the same imported fares database.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { writeFileSync } from "node:fs";
const require = createRequire(
  new URL("../apps/api/package.json", import.meta.url),
);
const Database = require("better-sqlite3");
const [database, base = "http://127.0.0.1:3000", noc = "ANEA", output] =
  process.argv.slice(2);
assert.ok(
  database && process.env.AUTH_KEY,
  "Usage: AUTH_KEY=... node scripts/verify-fare-geometry.mjs DB [BASE_URL] [NOC] [REPORT]",
);
const db = new Database(database, { readonly: true, fileMustExist: true });
try {
  db.exec("BEGIN");
  const expected = db
    .prepare(
      "SELECT id FROM catalogue_objects WHERE noc=? AND kind='area' AND active=1 ORDER BY id",
    )
    .all(noc)
    .map((r) => r.id);
  const state = db.prepare("SELECT * FROM geometry_state WHERE id=1").get();
  const stored = db
    .prepare("SELECT body FROM operator_geometry WHERE noc=?")
    .get(noc);
  assert.ok(state && stored, "Precompute geometry first");
  const started = performance.now();
  const response = await fetch(
    `${base}/api/fares/operators/${encodeURIComponent(noc)}/geometry`,
    {
      headers: { Authorization: `Bearer ${process.env.AUTH_KEY}` },
      redirect: "error",
    },
  );
  assert.equal(response.status, 200);
  const text = await response.text(),
    map = JSON.parse(text);
  assert.equal(
    response.headers.get("x-catalogue-generation"),
    String(state.fares_generation),
  );
  assert.ok(response.headers.get("etag"));
  assert.deepEqual(map, JSON.parse(stored.body));
  assert.deepEqual(
    map.zones.map((z) => z.id),
    expected,
  );
  assert.equal(map.timetableGeneration, state.timetable_generation);
  const zones = new Map(map.zones.map((z) => [z.id, z]));
  for (const field of ["areas", "points", "labels"]) {
    assert.equal(map[field].type, "FeatureCollection");
    for (const f of map[field].features) {
      assert.equal(f.type, "Feature");
      const zone = zones.get(f.properties.zoneId);
      assert.ok(zone);
      assert.equal(f.properties.name, zone.name);
      const points =
        f.geometry.type === "Polygon"
          ? f.geometry.coordinates[0]
          : [f.geometry.coordinates];
      for (const [lon, lat] of points)
        assert.ok(
          Number.isFinite(lon) &&
            Math.abs(lon) <= 180 &&
            Number.isFinite(lat) &&
            Math.abs(lat) <= 90,
        );
      if (field === "areas") {
        assert.equal(zone.membershipComplete, true);
        assert.equal(f.properties.approximate, true);
        assert.ok(points.length >= 4);
        assert.deepEqual(points[0], points.at(-1));
      }
    }
  }
  assert.equal(
    map.points.features.length,
    map.zones.reduce((n, z) => n + z.mappedStopCount, 0),
  );
  assert.deepEqual(
    map.missingAreaIds,
    map.zones.filter((z) => z.mappedStopCount === 0).map((z) => z.id),
  );
  const report = {
    noc,
    httpRequests: 1,
    faresGeneration: state.fares_generation,
    timetableGeneration: state.timetable_generation,
    zones: map.zones.length,
    stopMemberships: map.zones.reduce((n, z) => n + z.stopCount, 0),
    mappedStopMemberships: map.points.features.length,
    polygons: map.areas.features.length,
    labels: map.labels.features.length,
    missingAreaIds: map.missingAreaIds,
    unmatchedStopCount: map.unmatchedStopCodes.length,
    responseBytes: Buffer.byteLength(text),
    elapsedMs: Math.round(performance.now() - started),
  };
  if (output) writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report, null, 2));
} finally {
  db.close();
}
