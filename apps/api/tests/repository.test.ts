import { test } from "node:test";
import assert from "node:assert/strict";
import { writeFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { Repository, QueryError, type Query } from "../db/repository.ts";
import { fixture } from "./helpers.ts";

function execute(repo: Repository, query: Query, generation = 1) {
  const result = repo.prepare(query, generation);
  return {
    body: JSON.parse([...result.chunks].join("")),
    cursor: result.nextCursor(),
  };
}
test("all timetable endpoint projections preserve their public contract", () => {
  const f = fixture();
  const db = f.open();
  const repo = new Repository(db);
  try {
    const agencies = execute(repo, { kind: "agencies" }).body;
    assert.equal(agencies.length, 2);
    assert.equal(agencies[0].id, "A1");
    assert.deepEqual(
      execute(repo, { kind: "agency", id: "noc:ONE" }).body,
      agencies[0],
    );
    const route = execute(repo, { kind: "route", id: "101" }).body;
    assert.equal(route.id, 101);
    assert.equal(route.agency_id, "A1");
    assert.equal(route.route_type, "Bus Service");
    assert.equal(route.trips.length, 2);
    assert.deepEqual(execute(repo, { kind: "agencyRoutes", id: "A1" }).body, [
      route,
    ]);
    const trip = execute(repo, { kind: "trip", id: "T1" }).body;
    assert.equal(trip.id, "T1");
    assert.equal(trip.route_id, 101);
    assert.equal(trip.service_id, 201);
    assert.equal(trip.shape_id, "SH1");
    assert.equal(trip.direction_id, "outbound");
    assert.equal(trip.wheelchair_accessible, true);
    assert.equal(trip.stop_times[0].arrival_time, "25:00:00");
    assert.equal(trip.stop_times[0].stop.id, "S1");
    assert.equal(trip.stop_times[0].stop.lat, "51.5000");
    assert.equal(trip.stop_times[0].stop.wheelchair_boarding, true);
    assert.equal(trip.stop_times[0].shape_dist_traveled, "0");
    assert.equal(trip.stop_times[0].pickup_type, "regularly_scheduled");
    assert.equal(trip.stop_times[0].timepoint, "exact");
    assert.deepEqual(
      trip.stop_times.map((s: any) => s.stop_sequence),
      [1, 2],
    );
    const trips = execute(repo, { kind: "routeTrips", id: "101" }).body;
    assert.equal(trips.length, 2);
    assert.equal(trips[1].frequencies[0].exact_times, "frequency_based");
    assert.equal(trips[1].service.monday, null);
    assert.ok(!("route" in trips[0]));
    const service = execute(repo, { kind: "service", id: "201" }).body;
    assert.equal(service.monday, true);
    assert.equal(service.saturday, false);
    assert.equal(service.start_date, "2026-01-01");
    assert.equal(service.calendar_dates[0].date, "2026-12-25");
    const stops = execute(repo, { kind: "stops" }).body;
    assert.equal(stops.length, 3);
    assert.equal(stops[0].location_type, "stop");
    assert.equal(stops[2].location_type, "station");
    const agencyStops = execute(repo, { kind: "agencyStops", id: "A1" }).body;
    assert.deepEqual(
      agencyStops.map((s: any) => s.id),
      ["S1", "S2"],
    );
    const times = execute(repo, { kind: "stopTimes", id: "S1" }).body;
    assert.equal(times.length, 2);
    assert.equal(times[0].trip.route.agency.id, "A1");
    assert.ok(!("stop" in times[0]));
    const walk = (value: any) => {
      if (!value || typeof value !== "object") return;
      for (const [key, child] of Object.entries(value)) {
        assert.ok(!key.startsWith("_") && key !== "public_id", key);
        walk(child);
      }
    };
    walk(trip);
  } finally {
    db.close();
    f.close();
  }
});
test("pagination retains arrays, validates scope/generation and has no terminal cursor", () => {
  const f = fixture();
  const db = f.open();
  const repo = new Repository(db);
  try {
    for (const query of [
      { kind: "agencies" },
      { kind: "stops" },
      { kind: "agencyStops", id: "A1" },
      { kind: "agencyRoutes", id: "A1" },
      { kind: "routeTrips", id: "101" },
      { kind: "stopTimes", id: "S1" },
    ] as Query[]) {
      const full = execute(repo, query).body;
      const collected: any[] = [];
      let cursor: string | undefined;
      let pages = 0;
      do {
        const result = execute(repo, { ...query, limit: 1, cursor });
        assert.ok(Array.isArray(result.body));
        collected.push(...result.body);
        cursor = result.cursor;
        assert.ok(++pages < 10);
      } while (cursor);
      assert.deepEqual(collected, full, query.kind);
    }
    const cursor = execute(repo, { kind: "stops", limit: 1 }).cursor;
    assert.ok(cursor);
    assert.throws(
      () => execute(repo, { kind: "stops", limit: 1, cursor }, 2),
      /Cursor does not match/,
    );
    assert.throws(
      () => execute(repo, { kind: "agency", id: "A1", limit: 1 }),
      /collection/,
    );
    assert.throws(
      () => execute(repo, { kind: "trip", id: "missing" }),
      (e: any) => e instanceof QueryError && e.status === 404,
    );
  } finally {
    db.close();
    f.close();
  }
});
test("relationship updates are visible together with the committed generation", () => {
  const f = fixture();
  const db = f.open();
  try {
    const repo = new Repository(db);
    assert.equal(
      execute(repo, { kind: "agencyStops", id: "A2" }).body.length,
      1,
    );
    const p = join(f.feed, "routes.txt");
    writeFileSync(p, readFileSync(p, "utf8").replace("101,A1,", "101,A2,"));
    f.apply();
    assert.equal(
      execute(repo, { kind: "agencyStops", id: "A1" }, 2).body.length,
      0,
    );
    assert.equal(
      execute(repo, { kind: "agencyStops", id: "A2" }, 2).body.length,
      2,
    );
  } finally {
    db.close();
    f.close();
  }
});
