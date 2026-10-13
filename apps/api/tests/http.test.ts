import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { resolve } from "node:path";
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
    for (const path of [
      "/agencies",
      "/agencies/A1",
      "/agencies/noc:ONE",
      "/agencies/A1/routes",
      "/agencies/A1/stops",
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
