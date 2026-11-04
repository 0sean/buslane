import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createServer } from "node:net";
import Database from "better-sqlite3";
import { fixture, root, workerPath } from "./helpers.ts";
import { openFaresReader } from "../db/fares/index.ts";
import { FaresRepository, type FaresQuery } from "../db/fares/repository.ts";

function setup(file = "catalogue.xml") {
  const f = fixture();
  const database = join(f.directory, "fares.sqlite");
  const source = join(f.directory, file);
  copyFileSync(resolve(root, "tests/fixtures/netex", file), source);
  const apply = () =>
    JSON.parse(
      execFileSync(
        resolve(root, "apps/fares/target/debug/fares"),
        [
          "--db",
          database,
          "--source",
          source,
          "--scratch",
          join(f.directory, "fares-scratch"),
          "--min-free-bytes",
          "0",
        ],
        { encoding: "utf8" },
      ),
    );
  apply();
  return { ...f, fareDatabase: database, source, applyFares: apply };
}
function query(
  repo: FaresRepository,
  action: string,
  args: Partial<FaresQuery> = {},
  generation = 1,
) {
  const plan = repo.prepare(
    { kind: "catalogue", action, noc: "ONE", ...args },
    generation,
  );
  return {
    data: JSON.parse([...plan.chunks].join("")),
    cursor: plan.nextCursor(),
  };
}
test("canonical pagination, compact summaries, prefix search, direct areas and independent provenance", () => {
  const f = setup();
  const db = openFaresReader(f.fareDatabase);
  const repo = new FaresRepository(db);
  try {
    const areas = query(repo, "areas").data;
    assert.equal(areas.length, 1);
    assert.equal(areas[0].ticket_count, 2);
    const one = query(repo, "areaTickets", { id: areas[0].id, limit: 1 });
    assert.equal(one.data.length, 1);
    assert.ok(one.cursor);
    const two = query(repo, "areaTickets", {
      id: areas[0].id,
      limit: 1,
      cursor: one.cursor,
    });
    assert.equal(two.data.length, 1);
    assert.notEqual(one.data[0].id, two.data[0].id);
    assert.equal(query(repo, "tickets").data.length, 2);
    assert.equal(
      query(repo, "tickets", { include_unclassified: "true" }).data.length,
      3,
    );
    assert.equal(query(repo, "tickets", { q: "county d" }).data.length, 1);
    const searched = query(repo, "tickets", { q: "county", limit: 1 });
    const searchedNext = query(repo, "tickets", {
      q: "county",
      limit: 1,
      cursor: searched.cursor,
    });
    assert.notEqual(searched.data[0].id, searchedNext.data[0].id);

    const families = query(repo, "areaFamilies", { id: areas[0].id }).data;
    assert.equal(families.length, 2);
    assert.equal(families[0].variant_count, 1);
    const filtered = query(repo, "areaTickets", {
      id: areas[0].id,
      family_id: families[0].id,
    }).data;
    assert.equal(filtered.length, 1);
    assert.equal(filtered[0].family_id, families[0].id);
    const ticket = query(repo, "tickets", { q: "county d" }).data[0];
    assert.deepEqual(ticket.prices, [{ amount: "6.00", currency: "GBP" }]);
    assert.deepEqual(ticket.sales_channels, ["onBoard"]);
    assert.deepEqual(ticket.eligibility, ["adult"]);
    assert.deepEqual(ticket.duration, ["P1D"]);
    assert.ok(!("data" in ticket));
    assert.ok(!("sources" in ticket));
    assert.ok(!("matrices" in ticket));
    assert.equal(
      query(repo, "detail", { id: ticket.id }).data.detail.conditions_evaluated,
      false,
    );
    const sources = query(repo, "sources", { id: ticket.id }).data;
    assert.equal(sources.length, 1);
    assert.equal(sources[0].source_ref, "day");
    assert.deepEqual(sources[0].context, {
      kind: "FareTable",
      ref: "day-price",
      version: "1",
      frame_ref: "prices",
    });
    const stops = query(repo, "stops", { id: areas[0].id, limit: 1 });
    assert.equal(stops.data.length, 1);
    assert.ok(stops.cursor);
    assert.equal(
      query(repo, "geometry", { id: areas[0].id }).data.status,
      "unavailable",
    );
    assert.throws(
      () => query(repo, "areas", { limit: 101 }),
      (e) => (e as any).status === 400,
    );
    assert.throws(
      () =>
        query(
          repo,
          "areaTickets",
          { id: areas[0].id, limit: 1, cursor: one.cursor },
          2,
        ),
      (e) => (e as any).status === 409,
    );
    assert.throws(
      () =>
        query(repo, "areaTickets", {
          id: areas[0].id,
          limit: 1,
          cursor: one.cursor,
          q: "county",
        }),
      (e) => (e as any).status === 400,
    );
    assert.throws(
      () => query(repo, "detail", { id: ticket.id, noc: "OTHER" }),
      (e) => (e as any).status === 404,
    );
  } finally {
    db.close();
    f.close();
  }
});
test("directional journey lookup returns bounded exact decimals and explicit date status", () => {
  const f = setup("fares.xml");
  const db = openFaresReader(f.fareDatabase);
  const repo = new FaresRepository(db);
  try {
    const route = query(repo, "routes").data[0];
    assert.ok(route);
    const origins = query(repo, "origins", { route_id: route.id }).data;
    assert.equal(origins.length, 1);
    const destinations = query(repo, "destinations", {
      route_id: route.id,
      origin: origins[0].id,
    }).data;
    assert.equal(destinations.length, 1);
    const args = {
      route_id: route.id,
      origin: origins[0].id,
      destination: destinations[0].id,
      date: "2026-11-04",
    };
    const result = query(repo, "journey", args).data;
    assert.equal(result.candidates.length, 1);
    assert.equal(result.candidates[0].amount, "2.00");
    assert.equal(result.conditions_evaluated, false);
    assert.equal(
      query(repo, "journey", {
        ...args,
        origin: args.destination,
        destination: args.origin,
      }).data.status,
      "no_supported_published_fare",
    );
    assert.throws(
      () => query(repo, "journey", { ...args, date: "2026-02-30" }),
      (e) => (e as any).status === 400,
    );
    assert.equal(query(repo, "stops", { id: args.origin }).data[0].code, "S1");
  } finally {
    db.close();
    f.close();
  }
});
test("built authenticated HTTP catalogue, conditional caching, generation updates and rollback", async () => {
  const f = setup();
  const listener = createServer();
  await new Promise<void>((r) => listener.listen(0, "127.0.0.1", r));
  const port = (listener.address() as any).port;
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
        FARES_WORKER_PATH: resolve(
          root,
          "apps/api/.output/server/fares-worker.mjs",
        ),
        AUTH_KEY: "catalogue-test",
        SENTRY_DSN: "",
        HOST: "127.0.0.1",
        PORT: String(port),
      },
    },
  );
  let logs = "";
  server.stdout.on("data", (x) => (logs += x));
  server.stderr.on("data", (x) => (logs += x));
  const base = `http://127.0.0.1:${port}`;
  const get = (path: string, headers: Record<string, string> = {}) =>
    fetch(base + path, {
      headers: { Authorization: "Bearer catalogue-test", ...headers },
    });
  const prefix = "/api/fares/operators/ONE";
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
    assert.ok(ready, logs);
    assert.equal((await fetch(base + prefix + "/areas")).status, 400);
    const a = await get(prefix + "/areas");
    assert.equal(a.status, 200, await a.clone().text());
    assert.equal(a.headers.get("x-catalogue-version"), "1");
    const area = (await a.json())[0];
    assert.ok(area.id);
    const ticketsResponse = await get(
      prefix + `/areas/${area.id}/tickets?limit=1`,
    );
    assert.equal(
      ticketsResponse.status,
      200,
      await ticketsResponse.clone().text(),
    );
    const cursor = ticketsResponse.headers.get("x-next-cursor");
    assert.ok(cursor);
    const ticket = (await ticketsResponse.json())[0];
    const response = await get(prefix + `/tickets/${ticket.id}`);
    assert.equal(response.status, 200);
    const etag = response.headers.get("etag")!;
    assert.ok(etag);
    assert.equal(
      (await get(prefix + `/tickets/${ticket.id}`, { "If-None-Match": etag }))
        .status,
      304,
    );
    assert.equal(
      (await get(prefix + `/tickets/${ticket.id}/sources`)).status,
      200,
    );
    assert.equal(
      (await get(prefix + `/tickets/${ticket.id}/families`)).status,
      200,
    );
    assert.equal(
      (await get(prefix + `/areas/${area.id}/families`)).status,
      200,
    );
    assert.equal((await get(prefix + `/areas/${area.id}/stops`)).status, 200);
    assert.equal((await get(prefix + "/tickets?noc=OTHER")).status, 200); // path NOC is fixed
    for (const q of ["limit=101", "q=a&q=b", "generation=2"]) {
      const r = await get(prefix + "/tickets?" + q);
      assert.equal(r.status, q === "generation=2" ? 409 : 400, q);
    }
    assert.equal(f.applyFares().report.sqlite_changes, 0);
    writeFileSync(
      f.source,
      readFileSync(f.source, "utf8").replace("6.00", "6.75"),
    );
    f.applyFares();
    assert.equal(
      (
        await get(
          prefix +
            `/areas/${area.id}/tickets?limit=1&cursor=${encodeURIComponent(cursor!)}`,
        )
      ).status,
      409,
    );
    const updated = await get(prefix + "/tickets?q=county%20d", {
      "If-None-Match": etag,
    });
    assert.equal(updated.status, 200);
    assert.equal(updated.headers.get("x-fares-generation"), "2");
    const day = (await updated.json())[0];
    assert.equal(day.prices[0].amount, "6.75");
    const previous = await get(prefix + `/tickets/${day.id}`);
    assert.equal(previous.status, 200);
    writeFileSync(f.source, "<broken>");
    assert.throws(f.applyFares);
    const after = await get(prefix + `/tickets/${day.id}`);
    assert.equal(after.headers.get("x-fares-generation"), "2");
    assert.deepEqual(await after.json(), await previous.json());
    const writer = new Database(f.fareDatabase);
    writer
      .prepare("UPDATE catalogue_objects SET active=0 WHERE id=?")
      .run(day.id);
    writer.prepare("UPDATE dataset SET generation=generation+1").run();
    writer.close();
    assert.equal((await get(prefix + `/tickets/${day.id}`)).status, 410);
  } finally {
    server.kill("SIGTERM");
    if (server.exitCode === null && server.signalCode === null)
      await new Promise((r) => server.once("exit", r));
    f.close();
  }
});

test("catalogue pages use indexed canonical rows and never read raw source tables", () => {
  const f = setup();
  const db = openFaresReader(f.fareDatabase);
  const original = db.prepare.bind(db);
  const plans: any[] = [];
  (db as any).prepare = (sql: string) => {
    assert.doesNotMatch(
      sql,
      /\b(?:FROM|JOIN)\s+(?:entities|documents|links)\b/i,
    );
    const statement = original(sql);
    const all = statement.all.bind(statement);
    statement.all = (...params: any[]) => {
      plans.push(...original("EXPLAIN QUERY PLAN " + sql).all(...params));
      return all(...params);
    };
    return statement;
  };
  try {
    const repo = new FaresRepository(db);
    const area = query(repo, "areas").data[0];
    for (const action of ["areas", "tickets", "families", "routes"]) {
      query(repo, action, { limit: 1 });
      query(repo, action, { limit: 1, q: "c" });
    }
    query(repo, "areaTickets", { id: area.id, limit: 1 });
    query(repo, "areaFamilies", { id: area.id, limit: 1 });
    assert.ok(plans.length > 0);
    assert.ok(
      plans.every((p) => !/^SCAN /.test(p.detail)),
      JSON.stringify(plans),
    );
    assert.ok(
      plans.every((p) => !p.detail.includes("TEMP B-TREE")),
      JSON.stringify(plans),
    );
  } finally {
    db.close();
    f.close();
  }
});
