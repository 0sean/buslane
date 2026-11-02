import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { writeFileSync, readFileSync, copyFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createServer } from "node:net";
import { fixture, root, workerPath } from "./helpers.ts";
import { openFaresReader } from "../db/fares/index.ts";
import { FaresRepository, type FaresQuery } from "../db/fares/repository.ts";
import { QueryError } from "../db/repository.ts";
import { QueryPool, type PoolResult } from "../utils/pool.ts";
import Database from "better-sqlite3";

const faresWorker = resolve(root, "apps/api/.output/server/fares-worker.mjs");
function setup() {
  const f = fixture();
  const database = join(f.directory, "fares.sqlite");
  const xml = join(f.directory, "fares.xml");
  copyFileSync(resolve(root, "tests/fixtures/netex/fares.xml"), xml);
  const apply = () =>
    JSON.parse(
      execFileSync(
        resolve(root, "apps/fares/target/debug/fares"),
        [
          "--db",
          database,
          "--source",
          xml,
          "--scratch",
          join(f.directory, "fare-scratch"),
          "--min-free-bytes",
          "0",
        ],
        { encoding: "utf8" },
      ),
    );
  apply();
  return { ...f, fareDatabase: database, xml, applyFares: apply };
}
function result(repo: FaresRepository, query: FaresQuery, generation = 1) {
  const plan = repo.prepare(query, generation);
  return {
    value: JSON.parse([...plan.chunks].join("")),
    cursor: plan.nextCursor(),
  };
}
const body = async (r: PoolResult) =>
  new Response(r.body).json() as Promise<any>;

test("fares catalogue preserves references, decimals, namespaces, document scoping and timetable join keys", () => {
  const f = setup();
  const db = openFaresReader(f.fareDatabase);
  try {
    const repo = new FaresRepository(db);
    const products = result(repo, {
      kind: "fareProducts",
      noc: "ONE",
      line: "1",
      stop: "S1",
    }).value;
    assert.equal(products.length, 2);
    assert.equal(products[0].data.namespace, "http://www.netex.org.uk/netex");
    const prices = result(repo, {
      kind: "farePrices",
      product_ref: "product:adult",
    }).value;
    assert.equal(prices.length, 1);
    assert.equal(prices[0].referenced_prices[0].amount, "2.00");
    assert.equal(prices[0].referenced_prices[0].currency, "GBP");
    assert.equal(prices[0].matrices[0].start_ref, "zone:A");
    const document = result(repo, {
      kind: "fareDocument",
      id: String(products[0].document_id),
    }).value;
    assert.ok(
      document.links.some(
        (l: any) => l.kind === "operator" && l.code === "ONE",
      ),
    );
    assert.ok(
      document.links.some(
        (l: any) =>
          l.kind === "zone_stop" && l.code === "S1" && l.ref === "zone:A",
      ),
    );
    assert.equal(
      result(repo, { kind: "fareProducts", noc: "missing" }).value.length,
      0,
    );
    assert.equal(
      result(repo, { kind: "fareProducts", noc: "NR" }).value.length,
      0,
    );
    assert.equal(
      result(repo, { kind: "fareProducts", line: "wrong", noc: "ONE" }).value
        .length,
      0,
    );
    assert.throws(() => db.exec("DELETE FROM documents"), /readonly/i);
    assert.throws(
      () => result(repo, { kind: "fareDocument", id: "999999" }),
      (e: any) => e.status === 404,
    );
    assert.throws(
      () =>
        result(repo, {
          kind: "fareEntity",
          id: String(prices[0].id),
          category: "product",
        }),
      (e: any) => e.status === 404,
    );
  } finally {
    db.close();
    f.close();
  }
});

test("fares cursors bind filters and generation, and unchanged imports preserve pagination", () => {
  const f = setup();
  const db = openFaresReader(f.fareDatabase);
  try {
    const repo = new FaresRepository(db);
    const query: FaresQuery = { kind: "fareProducts", limit: 1 };
    const first = result(repo, query);
    assert.equal(first.value.length, 1);
    assert.ok(first.cursor);
    assert.equal(f.applyFares().report.sqlite_changes, 0);
    const second = result(repo, { ...query, cursor: first.cursor });
    assert.equal(second.value.length, 1);
    assert.notEqual(second.value[0].id, first.value[0].id);
    assert.throws(
      () => result(repo, { ...query, noc: "ONE", cursor: first.cursor }),
      (e: any) => e.status === 400,
    );
    assert.throws(
      () => result(repo, { ...query, cursor: first.cursor }, 2),
      (e: any) => e.status === 409,
    );
    for (const cursor of ["garbage", Buffer.from("null").toString("base64url")])
      assert.throws(() => result(repo, { ...query, cursor }), QueryError);
  } finally {
    db.close();
    f.close();
  }
});

test("fares workers cache every filter independently and refresh after a committed price update", async () => {
  const f = setup();
  const pool = new QueryPool({
    database: f.fareDatabase,
    resource: "fares",
    workerPath: faresWorker,
    workers: 1,
  });
  try {
    assert.equal(
      (await body(await pool.query({ kind: "fareProducts", noc: "ONE" })))
        .length,
      2,
    );
    assert.equal(
      (await body(await pool.query({ kind: "fareProducts", noc: "OTHER" })))
        .length,
      0,
    );
    const prices = await pool.query({ kind: "farePrices", ref: "price:adult" });
    assert.equal((await body(prices))[0].amount, "2.00");
    writeFileSync(f.xml, readFileSync(f.xml, "utf8").replace("2.00", "2.75"));
    f.applyFares();
    const changed = await pool.query({
      kind: "farePrices",
      ref: "price:adult",
    });
    assert.equal(changed.generation, 2);
    assert.equal((await body(changed))[0].amount, "2.75");
    assert.ok((await pool.invalidate()) > 0);
  } finally {
    await pool.close();
    f.close();
  }
});

test("large fare definitions stream Unicode and keep one snapshot while later payloads are fetched", async () => {
  const f = setup();
  const writer = new Database(f.fareDatabase);
  const text = "🚌".repeat(600000);
  const data = JSON.stringify({ name: "FareZone", children: [text] });
  writer.prepare("UPDATE entities SET data=? WHERE category='zone'").run(data);
  const pool = new QueryPool({
    database: f.fareDatabase,
    resource: "fares",
    workerPath: faresWorker,
    workers: 1,
  });
  try {
    const first = await pool.query({
      kind: "fareEntities",
      category: "zone",
      limit: 2,
    });
    assert.ok(first.body instanceof ReadableStream);
    writer.transaction(() => {
      writer
        .prepare("UPDATE entities SET data=? WHERE category='zone'")
        .run(JSON.stringify({ name: "FareZone", children: ["changed"] }));
      writer.prepare("UPDATE dataset SET generation=2").run();
    })();
    const rows = await body(first);
    assert.equal(rows.length, 2);
    assert.ok(rows.every((r: any) => r.data.children[0] === text));
    const second = await pool.query({
      kind: "fareEntities",
      category: "zone",
      limit: 2,
    });
    assert.equal(second.generation, 2);
    assert.ok(
      (await body(second)).every((r: any) => r.data.children[0] === "changed"),
    );
  } finally {
    await pool.close();
    writer.close();
    f.close();
  }
});

test("built HTTP fares API enforces auth, validates input, returns XML and refreshes committed data", async () => {
  const f = setup();
  const listener = createServer();
  await new Promise<void>((r) => listener.listen(0, "127.0.0.1", r));
  const port = (listener.address() as { port: number }).port;
  await new Promise<void>((r) => listener.close(() => r()));
  const server = spawn(
    process.execPath,
    [resolve(root, "apps/api/.output/server/index.mjs")],
    {
      cwd: root,
      env: {
        ...process.env,
        TIMETABLES_DB_PATH: f.database,
        TIMETABLES_WORKER_PATH: workerPath,
        FARES_DB_PATH: f.fareDatabase,
        FARES_WORKER_PATH: faresWorker,
        AUTH_KEY: "fares-test-token",
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
  const get = (path: string) =>
    fetch(base + path, {
      headers: { Authorization: "Bearer fares-test-token" },
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
    assert.equal((await fetch(base + "/api/fares/products")).status, 400);
    const first = await get("/api/fares/products?limit=1&noc=ONE&line=1");
    assert.equal(first.status, 200);
    assert.equal(first.headers.get("x-fares-generation"), "1");
    const product = (await first.json())[0];
    const cursor = first.headers.get("x-next-cursor");
    assert.ok(cursor);
    assert.equal((await get(`/api/fares/products/${product.id}`)).status, 200);
    const doc = await get(`/api/fares/documents/${product.document_id}`);
    assert.equal(doc.status, 200);
    const xml = await get(`/api/fares/documents/${product.document_id}/xml`);
    assert.match(xml.headers.get("content-type")!, /application\/xml/);
    assert.equal(await xml.text(), readFileSync(f.xml, "utf8"));
    for (const query of [
      "limit=0",
      "limit=1001",
      "limit=1.5",
      "limit=1&limit=2",
      "document_id=NaN",
      "cursor=bad",
    ])
      assert.equal(
        (await get("/api/fares/products?" + query)).status,
        400,
        query,
      );
    assert.equal((await get("/api/fares/entities/999999")).status, 404);
    assert.equal((await get("/api/fares/documents/invalid")).status, 400);
    writeFileSync(f.xml, readFileSync(f.xml, "utf8").replace("2.00", "2.75"));
    f.applyFares();
    const price = await get("/api/fares/prices?ref=price%3Aadult");
    assert.equal(price.headers.get("x-fares-generation"), "2");
    assert.equal((await price.json())[0].amount, "2.75");
    assert.equal(
      (
        await get(
          "/api/fares/products?limit=1&noc=ONE&line=1&cursor=" +
            encodeURIComponent(cursor!),
        )
      ).status,
      409,
    );
    assert.equal((await get("/api/timetables/agencies/noc:ONE")).status, 200);
    for (const path of [
      "/api/timetables/agencies/A1/fares",
      "/api/timetables/routes/101/fares",
      "/api/timetables/stops/S1/fares",
    ]) {
      const linked = await get(path);
      assert.equal(
        linked.status,
        200,
        path + " " + (await linked.clone().text()),
      );
      assert.equal(linked.headers.get("x-fares-generation"), "2");
      assert.equal(linked.headers.get("x-timetable-generation"), "1");
      assert.equal((await linked.json()).length, 2);
    }
    assert.equal(
      (await get("/api/timetables/routes/999999/fares")).status,
      404,
    );
  } finally {
    server.kill("SIGTERM");
    await new Promise<void>((r) => server.once("exit", () => r()));
    f.close();
  }
});
