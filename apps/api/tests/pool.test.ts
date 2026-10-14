import { test } from "node:test";
import assert from "node:assert/strict";
import { writeFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { QueryPool, PoolError, type PoolResult } from "../utils/pool.ts";
import { ByteCache } from "../db/cache.ts";
import { fixture, workerPath } from "./helpers.ts";
const body = async (result: PoolResult) =>
  JSON.parse(await new Response(result.body).text());
function largeStops(db: ReturnType<ReturnType<typeof fixture>["open"]>) {
  db.exec(`WITH RECURSIVE seq(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM seq WHERE x<12000)
    INSERT INTO stops(public_id,name,lat,lon,wheelchair_boarding,location_type)
    SELECT 'EXTRA'||printf('%06d',x),'Unicode 🚌 stop with a reasonably long name','51.5000','-0.1200',0,0 FROM seq;`);
}
test("byte cache evicts by byte budget and expires entries", () => {
  const cache = new ByteCache<string>(5);
  cache.put("a", "aaa", 3);
  cache.put("b", "bbb", 3);
  assert.equal(cache.get("a"), undefined);
  assert.equal(cache.get("b"), "bbb");
  assert.equal(cache.clear(), 1);
  const expired = new ByteCache<string>(10, 0);
  expired.put("x", "x", 1);
  assert.equal(expired.get("x"), undefined);
});
test("workers invalidate caches by committed generation", async () => {
  const f = fixture();
  const pool = new QueryPool({ database: f.database, workerPath });
  try {
    const first = await pool.query({ kind: "stops" });
    assert.equal(first.generation, 1);
    assert.equal((await body(first))[0].name, "First Stop");
    const p = join(f.feed, "stops.txt");
    writeFileSync(
      p,
      readFileSync(p, "utf8").replace("First Stop", "Fresh Stop"),
    );
    f.apply();
    const second = await pool.query({ kind: "stops" });
    assert.equal(second.generation, 2);
    assert.equal((await body(second))[0].name, "Fresh Stop");
    assert.ok((await pool.invalidate()) > 0);
  } finally {
    await pool.close();
    f.close();
  }
});
test("large responses stream one consistent snapshot, preserving Unicode", async () => {
  const f = fixture();
  const writer = f.open();
  largeStops(writer);
  const pool = new QueryPool({ database: f.database, workerPath });
  try {
    const result = await pool.query({ kind: "stops" });
    assert.ok(result.body instanceof ReadableStream);
    assert.equal(result.generation, 1);
    writer.transaction(() => {
      writer
        .prepare(
          "UPDATE stops SET name='Changed after stream started' WHERE public_id='S1'",
        )
        .run();
      writer.prepare("UPDATE dataset SET generation=2").run();
    })();
    const values = await body(result);
    assert.equal(values.length, 12003);
    assert.ok(
      values.some((s: any) => s.id === "S1" && s.name === "First Stop"),
    );
    assert.ok(values.some((s: any) => s.name.includes("🚌")));
    assert.ok(!values.some((s: any) => s.name.includes("�")));
    const second = await pool.query({ kind: "stopTimes", id: "S1" });
    assert.equal(second.generation, 2);
    await body(second);
    assert.ok((await pool.invalidate()) >= 1);
  } finally {
    await pool.close();
    writer.close();
    f.close();
  }
});
test("stream cancellation frees workers and bounded queues reject overload", async () => {
  const f = fixture();
  const writer = f.open();
  largeStops(writer);
  const pool = new QueryPool({
    database: f.database,
    workerPath,
    workers: 1,
    maximumQueue: 1,
  });
  try {
    const result = await pool.query({ kind: "stops" });
    assert.ok(result.body instanceof ReadableStream);
    const queued = pool.health();
    await assert.rejects(
      pool.health(),
      (e: any) => e instanceof PoolError && e.status === 503,
    );
    await (result.body as ReadableStream).cancel();
    assert.equal((await queued).generation, 1);
    assert.equal((await pool.health()).generation, 1);
  } finally {
    await pool.close();
    writer.close();
    f.close();
  }
});
