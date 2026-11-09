import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { fixture, root, workerPath } from "./helpers.ts";
async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = (server.address() as { port: number }).port;
  await new Promise<void>((resolve) => server.close(() => resolve()));
  return port;
}
test("built Nitro API preserves routes, auth, pagination, errors and readiness", async () => {
  const f = fixture();
  const port = await freePort();
  const server = spawn(
    process.execPath,
    [resolve(root, "apps/api/.output/server/index.mjs")],
    {
      cwd: root,
      env: {
        ...process.env,
        TIMETABLES_DB_PATH: f.database,
        TIMETABLES_WORKER_PATH: workerPath,
        AUTH_KEY: "fixture-token",
        SENTRY_DSN: "",
        FARES_DB_PATH: "",
        HOST: "127.0.0.1",
        PORT: String(port),
      },
    },
  );
  let output = "";
  server.stdout.on("data", (data) => (output += data));
  server.stderr.on("data", (data) => (output += data));
  const base = "http://127.0.0.1:" + port;
  const get = (path: string) =>
    fetch(base + path, { headers: { Authorization: "Bearer fixture-token" } });
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
    assert.equal((await fetch(base + "/api/timetables/stops")).status, 400);
    assert.equal((await get("/api/fares/products")).status, 503);
    for (const path of [
      "/agencies",
      "/agencies/A1",
      "/agencies/noc:ONE",
      "/agencies/A1/routes",
      "/agencies/A1/stops",
      "/agencies/noc:ONE/geometry",
      "/routes/101",
      "/routes/101/trips",
      "/trips/T1",
      "/services/201",
      "/stops",
      "/stops/S1/stop_times",
    ]) {
      const response = await get("/api/timetables" + path);
      assert.equal(
        response.status,
        200,
        path + " " + (await response.clone().text()),
      );
      assert.equal(response.headers.get("x-timetable-generation"), "1");
      await response.json();
    }
    const geometryPath = "/api/timetables/agencies/noc:ONE/geometry";
    assert.equal((await fetch(base + geometryPath)).status, 400);
    const geometry = await get(geometryPath);
    assert.equal(geometry.headers.get("cache-control"), "private, no-cache");
    const etag = geometry.headers.get("etag")!;
    assert.ok(etag);
    const network = await geometry.json();
    assert.equal(network.network.features[0].id, 101);
    assert.deepEqual(network.missingRouteIds, [101]);
    const conditional = await fetch(base + geometryPath, { headers: {
      Authorization: "Bearer fixture-token", "If-None-Match": etag,
    } });
    assert.equal(conditional.status, 304);
    assert.equal(await conditional.text(), "");
    assert.equal((await get(geometryPath + "?limit=1")).status, 400);
    assert.equal((await get("/api/timetables/agencies/missing/geometry")).status, 404);
    const missing = await get("/api/timetables/trips/missing");
    assert.equal(missing.status, 404);
    assert.equal((await missing.json()).error, "Trip not found");
    assert.equal((await get("/api/timetables/stops?limit=0")).status, 400);
    const page = await get("/api/timetables/stops?limit=1");
    assert.equal((await page.json()).length, 1);
    assert.ok(page.headers.get("x-next-cursor"));
    const cleared = await fetch(base + "/api/invalidate", {
      method: "POST",
      headers: { Authorization: "Bearer fixture-token" },
    });
    assert.equal(cleared.status, 200);
    assert.ok((await cleared.json()).deleted > 0);
    const stopsFile = join(f.feed, "stops.txt");
    writeFileSync(stopsFile, readFileSync(stopsFile, "utf8").replace("51.5000", "51.6000"));
    f.apply();
    const updated = await fetch(base + geometryPath, { headers: {
      Authorization: "Bearer fixture-token", "If-None-Match": etag,
    } });
    assert.equal(updated.status, 200);
    assert.notEqual(updated.headers.get("etag"), etag);
    assert.equal(updated.headers.get("x-timetable-generation"), "2");
    assert.notDeepEqual(await updated.json(), network);
    const broken = f.open();
    broken.exec("DELETE FROM agency_geometry; UPDATE dataset SET generation=generation+1");
    broken.close();
    const unavailable = await get(geometryPath);
    assert.equal(unavailable.status, 503);
    assert.equal(unavailable.headers.get("cache-control"), "no-store");
    const failedDb = f.open();
    failedDb.exec("DROP TABLE agency_geometry; UPDATE dataset SET generation=generation+1");
    failedDb.close();
    const failed = await fetch(base + geometryPath, { headers: {
      Authorization: "Bearer fixture-token", "If-None-Match": "*",
    } });
    assert.equal(failed.status, 500);
    assert.equal(failed.headers.get("cache-control"), "no-store");
    assert.equal(failed.headers.get("etag"), null);
    const writer = f.open();
    writer.prepare("UPDATE dataset SET generation=0").run();
    writer.close();
    assert.equal((await fetch(base + "/up")).status, 503);
  } finally {
    server.kill("SIGTERM");
    await new Promise<void>((resolve) => server.once("exit", () => resolve()));
    f.close();
  }
});
