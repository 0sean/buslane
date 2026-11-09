// Run against a local API and the same imported database, with no concurrent import.
// Exactly one HTTP request; route coverage is checked directly against SQLite.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { writeFileSync } from "node:fs";
const require = createRequire(new URL("../apps/api/package.json", import.meta.url));
const Database = require("better-sqlite3");
const [database, baseUrl = "http://127.0.0.1:3000", agencyId = "noc:ANEA", output] = process.argv.slice(2);
assert.ok(database && process.env.AUTH_KEY, "Usage: AUTH_KEY=... node scripts/verify-operator-geometry.mjs DB [BASE_URL] [AGENCY_ID] [REPORT_PATH]");
const db = new Database(database, { readonly: true, fileMustExist: true });
try {
  db.exec("BEGIN");
  const agency = agencyId.startsWith("noc:")
    ? db.prepare("SELECT * FROM agencies WHERE noc=? ORDER BY public_id LIMIT 1").get(agencyId.slice(4))
    : db.prepare("SELECT * FROM agencies WHERE public_id=?").get(agencyId);
  assert.ok(agency, "Agency exists");
  const routes = db.prepare("SELECT public_id FROM routes WHERE agency_id=? ORDER BY public_id").all(agency.id).map(r => r.public_id);
  const stored = db.prepare("SELECT generation,body FROM agency_geometry WHERE agency_id=?").get(agency.id);
  assert.ok(stored, "Geometry has been built");
  const started = performance.now();
  const response = await fetch(`${baseUrl}/api/timetables/agencies/${encodeURIComponent(agencyId)}/geometry`, {
    headers: { Authorization: `Bearer ${process.env.AUTH_KEY}` },
    redirect: "error",
  });
  assert.equal(response.status, 200);
  const text = await response.text();
  const body = JSON.parse(text);
  assert.deepEqual(body, JSON.parse(stored.body));
  assert.equal(response.headers.get("x-timetable-generation"), String(stored.generation));
  assert.ok(response.headers.get("etag"));
  assert.equal(body.network.type, "FeatureCollection");
  const ids = new Set();
  let lines = 0, points = 0;
  for (const feature of body.network.features) {
    assert.equal(feature.type, "Feature");
    assert.equal(feature.geometry.type, "MultiLineString");
    assert.equal(feature.id, feature.properties.routeId);
    assert.ok(routes.includes(feature.id));
    assert.ok(!ids.has(feature.id));
    ids.add(feature.id);
    const unique = new Set();
    assert.ok(feature.geometry.coordinates.length);
    for (const line of feature.geometry.coordinates) {
      assert.ok(line.length >= 2);
      const key = JSON.stringify(line);
      assert.ok(!unique.has(key)); unique.add(key);
      line.forEach(([lon,lat], i) => {
        assert.ok(Number.isFinite(lon) && Math.abs(lon) <= 180);
        assert.ok(Number.isFinite(lat) && Math.abs(lat) <= 90);
        if (i) assert.notDeepEqual(line[i-1], line[i]);
      });
      lines++; points += line.length;
    }
  }
  assert.equal(new Set(body.missingRouteIds).size, body.missingRouteIds.length);
  for (const id of body.missingRouteIds) { assert.ok(routes.includes(id)); ids.add(id); }
  assert.deepEqual([...ids].sort((a,b) => a-b), routes, "Every operator route is accounted for");
  const report = {
    agencyId, agencyPublicId: agency.public_id, httpRequests: 1,
    generation: stored.generation, routes: routes.length,
    features: body.network.features.length, missingRouteIds: body.missingRouteIds,
    lines, points, responseBytes: Buffer.byteLength(text),
    elapsedMs: Math.round(performance.now()-started), etag: response.headers.get("etag"),
  };
  if (output) writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report, null, 2));
} finally { db.close(); }
