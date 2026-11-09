import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createServer } from "node:net";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import Database from "better-sqlite3";
import { fixture, root, workerPath } from "./helpers.ts";
import { FaresRepository } from "../db/fares/repository.ts";

async function freePort() {
  const server = createServer();
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const port = (server.address() as { port: number }).port;
  await new Promise<void>((r) => server.close(() => r()));
  return port;
}

test("precomputed operator fare map: one HTTP response, auth, versioning, backfill and uncached failures", async () => {
  const f = fixture();
  const database = join(f.directory, "fares.sqlite");
  const source = join(f.directory, "catalogue.xml");
  copyFileSync(resolve(root, "tests/fixtures/netex/catalogue.xml"), source);
  const run = (...args: string[]) =>
    execFileSync(
      resolve(root, "apps/fares/target/debug/fares"),
      [
        "--db",
        database,
        "--scratch",
        join(f.directory, "scratch-fares"),
        "--min-free-bytes",
        "0",
        ...args,
      ],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
  run("--source", source); // Existing imported catalogue, without a map.
  const port = await freePort();
  const server = spawn(
    process.execPath,
    [resolve(root, "apps/api/.output/server/index.mjs")],
    {
      env: {
        ...process.env,
        TIMETABLES_DB_PATH: f.database,
        TIMETABLES_WORKER_PATH: workerPath,
        FARES_DB_PATH: database,
        FARES_WORKERS: "1",
        AUTH_KEY: "fare-map-test",
        SENTRY_DSN: "",
        HOST: "127.0.0.1",
        PORT: String(port),
      },
    },
  );
  let output = "";
  server.stdout.on("data", (d) => (output += d));
  server.stderr.on("data", (d) => (output += d));
  const base = `http://127.0.0.1:${port}`;
  const path = "/api/fares/operators/ONE/geometry";
  const get = (path: string, extra: Record<string, string> = {}) =>
    fetch(base + path, {
      headers: { Authorization: "Bearer fare-map-test", ...extra },
    });
  try {
    let ready = false;
    for (let i = 0; i < 100; i++) {
      try {
        if ((await fetch(base + "/up")).ok) {
          ready = true;
          break;
        }
      } catch {}
      await new Promise((r) => setTimeout(r, 50));
    }
    assert.ok(ready, output);
    assert.equal((await fetch(base + path)).status, 400);
    const absent = await get(path, { "If-None-Match": "*" });
    assert.equal(absent.status, 503);
    assert.equal(absent.headers.get("cache-control"), "no-store");
    run("--rebuild-geometry", "--timetables-db", f.database);
    const first = await get(path);
    assert.equal(first.status, 200);
    assert.equal(first.headers.get("x-catalogue-version"), "1");
    assert.equal(first.headers.get("x-catalogue-generation"), "2");
    assert.equal(first.headers.get("cache-control"), "private, no-cache");
    const etag = first.headers.get("etag")!;
    const body = await first.json();
    assert.equal(body.zones.length, 1);
    assert.equal(body.zones[0].stopCount, 2);
    assert.equal(body.points.features.length, 2);
    assert.equal(body.labels.features.length, 1);
    assert.equal(body.areas.features.length, 0); // Two stops do not form a polygon.
    assert.deepEqual(body.missingAreaIds, []);
    assert.deepEqual(body.unmatchedStopCodes, []);
    assert.equal(body.timetableGeneration, 1);
    assert.equal(body.points.features[0].properties.zoneId, body.zones[0].id);
    assert.equal((await get(path, { "If-None-Match": etag })).status, 304);
    assert.equal(
      (await get("/api/fares/operators/OTHER/geometry")).status,
      404,
    );
    assert.equal((await get(path + "?generation=1")).status, 409);
    assert.equal((await get(path + "?q=county")).status, 400);

    // The endpoint reads the persisted document rather than enumerating memberships.
    const statements: string[] = [];
    const traced = new Database(database, {
      verbose: (sql) => statements.push(String(sql)),
    });
    try {
      const plan = new FaresRepository(traced).prepare(
        { kind: "catalogue", action: "operatorGeometry", noc: "ONE" },
        2,
      );
      assert.deepEqual(JSON.parse([...plan.chunks].join("")), body);
      assert.equal(statements.length, 4);
      assert.ok(
        statements.every(
          (sql) => !/catalogue_stops|entities|documents/.test(sql),
        ),
      );
    } finally {
      traced.close();
    }

    // New coordinate snapshot changes the map and invalidates the HTTP validator.
    const stops = join(f.feed, "stops.txt");
    writeFileSync(
      stops,
      readFileSync(stops, "utf8").replace("51.5000", "51.6000"),
    );
    f.apply();
    run("--source", source, "--timetables-db", f.database);
    const updated = await get(path, { "If-None-Match": etag });
    assert.equal(updated.status, 200);
    assert.notEqual(updated.headers.get("etag"), etag);
    const newer = await updated.json();
    assert.equal(newer.timetableGeneration, 2);
    assert.notDeepEqual(newer.points, body.points);
    assert.equal(updated.headers.get("x-catalogue-generation"), "3");
    // Unchanged input and coordinates remain zero-write.
    const repeat = JSON.parse(
      run("--source", source, "--timetables-db", f.database),
    );
    assert.equal(repeat.report.sqlite_changes, 0);
    // More than 50 areas still arrive completely through one HTTP response.
    const many = new Database(database);
    const original = many
      .prepare(
        "SELECT * FROM catalogue_objects WHERE kind='area' AND active=1 LIMIT 1",
      )
      .get() as any;
    const insert = many.prepare(
      "INSERT INTO catalogue_objects(id,kind,noc,semantic_key,name,search,summary,detail) VALUES(?,'area','ONE',?,?,?,'{\"membership\":\"published_stops\"}','{}')",
    );
    for (let i = 0; i < 60; i++) {
      const id = "area_" + i.toString(16).padStart(64, "0");
      insert.run(id, id, "Extra area " + i, "extra area " + i);
      many
        .prepare(
          "INSERT INTO catalogue_stops SELECT ?,code,name FROM catalogue_stops WHERE object_id=?",
        )
        .run(id, original.id);
    }
    many.close();
    run("--rebuild-geometry", "--timetables-db", f.database);
    const allAreas = await (await get(path)).json();
    assert.equal(allAreas.zones.length, 61);
    assert.equal(allAreas.points.features.length, 122);
    // Large persisted documents can return 304 and release the single fare worker.
    const large = new Database(database);
    large
      .prepare(
        "UPDATE operator_geometry SET body=json_set(body,'$.testPadding',?) WHERE noc='ONE'",
      )
      .run("x".repeat(1100000));
    large.exec(
      "UPDATE dataset SET generation=generation+1; UPDATE geometry_state SET fares_generation=(SELECT generation FROM dataset)",
    );
    large.close();
    const streamed = await get(path);
    assert.equal(streamed.status, 200);
    assert.ok((await streamed.text()).length > 1024 * 1024);
    const streamedEtag = streamed.headers.get("etag")!;
    assert.equal(
      (await get(path, { "If-None-Match": streamedEtag })).status,
      304,
    );
    assert.equal((await get("/api/fares/operators/ONE/areas")).status, 200);
    const writer = new Database(database);
    writer.exec(
      "DELETE FROM geometry_state; UPDATE dataset SET generation=generation+1",
    );
    writer.close();
    assert.equal((await get(path, { "If-None-Match": "*" })).status, 503);
    const broken = new Database(database);
    broken.exec(
      "DROP TABLE operator_geometry; INSERT INTO geometry_state VALUES(1,1,7,2); UPDATE dataset SET generation=7",
    );
    broken.close();
    const failure = await get(path);
    assert.equal(failure.status, 500);
    assert.equal(failure.headers.get("cache-control"), "no-store");
    assert.equal(failure.headers.get("etag"), null);
  } finally {
    server.kill("SIGTERM");
    await new Promise<void>((r) => server.once("exit", () => r()));
    f.close();
  }
});
