// Authenticated HTTP benchmark plus plans for the actual catalogue SQL.
// Build apps/api and apps/timetables first. Input database is read-only.
import { createRequire } from "node:module";
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { createServer } from "node:net";
import { parseArgs } from "node:util";
const root = resolve(import.meta.dirname, "..");
const { values } = parseArgs({
  options: {
    database: { type: "string" },
    output: { type: "string" },
    noc: { type: "string", default: "ANEA" },
    samples: { type: "string", default: "100" },
  },
});
if (!values.database || !values.output)
  throw Error(
    "--database PATH --output NEW_DIRECTORY [--noc ANEA --samples 100]",
  );
const samples = Number(values.samples);
if (!Number.isInteger(samples) || samples < 5)
  throw Error("samples >=5 required");
const directory = resolve(values.output);
mkdirSync(directory, { recursive: false });
const require = createRequire(resolve(root, "apps/api/package.json"));
const Database = require("better-sqlite3");
const db = new Database(resolve(values.database), {
  readonly: true,
  fileMustExist: true,
});
db.pragma("query_only=ON");
const noc = values.noc;
const object = (k) =>
  db
    .prepare(
      "SELECT * FROM catalogue_objects WHERE noc=? AND kind=? AND active=1 ORDER BY id LIMIT 1",
    )
    .get(noc, k);
const area = object("area"),
  ticket = object("ticket");
const pair = db
  .prepare(
    "SELECT j.* FROM catalogue_journeys j JOIN catalogue_objects o ON o.id=j.route_id WHERE o.noc=? LIMIT 1",
  )
  .get(noc);
const prefix = `/api/fares/operators/${encodeURIComponent(noc)}`;
const scenarios = {
  areas: { path: prefix + "/areas?limit=50", action: "areas" },
  tickets: { path: prefix + "/tickets?limit=50", action: "tickets" },
  search: {
    path: prefix + "/tickets?q=ad&limit=50",
    action: "tickets",
    q: "ad",
  },
  routes: { path: prefix + "/routes?limit=50", action: "routes" },
};
if (area) {
  scenarios.area_families = {
    path: prefix + `/areas/${area.id}/families?limit=50`,
    action: "areaFamilies",
    id: area.id,
  };
  scenarios.area_tickets = {
    path: prefix + `/areas/${area.id}/tickets?limit=50`,
    action: "areaTickets",
    id: area.id,
  };
  scenarios.stops = {
    path: prefix + `/areas/${area.id}/stops?limit=50`,
    action: "stops",
    id: area.id,
  };
}
if (ticket)
  scenarios.detail = {
    path: prefix + `/tickets/${ticket.id}`,
    action: "detail",
    id: ticket.id,
  };
if (pair) {
  scenarios.origins = {
    path: prefix + `/routes/${pair.route_id}/origins?limit=50`,
    action: "origins",
    route_id: pair.route_id,
  };
  scenarios.destinations = {
    path:
      prefix +
      `/routes/${pair.route_id}/destinations?origin=${pair.origin_id}&limit=50`,
    action: "destinations",
    route_id: pair.route_id,
    origin: pair.origin_id,
  };
  scenarios.journey = {
    path:
      prefix +
      `/journey-fares?route_id=${pair.route_id}&origin=${pair.origin_id}&destination=${pair.destination_id}&date=2026-11-04&limit=50`,
    action: "journey",
    route_id: pair.route_id,
    origin: pair.origin_id,
    destination: pair.destination_id,
    date: "2026-11-04",
  };
}
const counts = {};
for (const table of [
  "documents",
  "entities",
  "catalogue_objects",
  "catalogue_sources",
  "catalogue_journeys",
]) {
  counts[table] = db.prepare(`SELECT count(*) n FROM ${table}`).get().n;
}
const storage = db
  .prepare(
    "SELECT CASE WHEN (name LIKE 'catalogue_%' OR name LIKE 'sqlite_autoindex_catalogue_%') THEN 'catalogue' ELSE 'source' END category,sum(pgsize) bytes FROM dbstat GROUP BY category",
  )
  .all();
const generation = db
  .prepare("SELECT generation FROM dataset WHERE id=1")
  .get().generation;
const byKind = db
  .prepare(
    "SELECT kind,count(*) count FROM catalogue_objects WHERE active=1 GROUP BY kind",
  )
  .all();
db.close();
// Import the actual TypeScript repository under tsx; trace every prepared query
// and reject access to raw source tables for all catalogue list/detail requests.
const { FaresRepository } = await import("../apps/api/db/fares/repository.ts");
const plans = {};
for (const [name, s] of Object.entries(scenarios)) {
  const reader = new Database(resolve(values.database), { readonly: true });
  const original = reader.prepare.bind(reader);
  const queries = [];
  reader.prepare = (sql) => {
    if (/\b(?:FROM|JOIN)\s+(?:entities|documents|links)\b/i.test(sql))
      throw Error("Catalogue read raw records: " + sql);
    const stmt = original(sql);
    for (const method of ["get", "all"]) {
      const actual = stmt[method].bind(stmt);
      stmt[method] = (...p) => {
        queries.push({
          sql,
          params: p,
          plan: original("EXPLAIN QUERY PLAN " + sql).all(...p),
        });
        return actual(...p);
      };
    }
    return stmt;
  };
  const repo = new FaresRepository(reader);
  const { path, ...args } = s;
  const plan = repo.prepare(
    { kind: "catalogue", noc, limit: 50, ...args },
    generation,
  );
  [...plan.chunks];
  plans[name] = queries;
  reader.close();
}
const timetable = join(directory, "timetables.sqlite");
execFileSync(resolve(root, "apps/timetables/target/debug/timetables"), [
  "--db",
  timetable,
  "--source",
  resolve(root, "tests/fixtures/gtfs"),
  "--scratch",
  join(directory, "scratch"),
  "--min-free-bytes",
  "0",
]);
const listener = createServer();
await new Promise((r) => listener.listen(0, "127.0.0.1", r));
const port = listener.address().port;
await new Promise((r) => listener.close(r));
const server = spawn(
  process.execPath,
  [resolve(root, "apps/api/.output/server/index.mjs")],
  {
    cwd: root,
    env: {
      ...process.env,
      TIMETABLES_DB_PATH: timetable,
      TIMETABLES_WORKER_PATH: resolve(
        root,
        "apps/api/.output/server/timetable-worker.mjs",
      ),
      FARES_DB_PATH: resolve(values.database),
      FARES_WORKER_PATH: resolve(
        root,
        "apps/api/.output/server/fares-worker.mjs",
      ),
      AUTH_KEY: "catalogue-benchmark",
      SENTRY_DSN: "",
      HOST: "127.0.0.1",
      PORT: String(port),
    },
  },
);
let logs = "";
server.stdout.on("data", (d) => (logs += d));
server.stderr.on("data", (d) => (logs += d));
const base = `http://127.0.0.1:${port}`;
async function request(path, method = "GET") {
  const start = performance.now();
  const r = await fetch(base + path, {
    method,
    headers: { Authorization: "Bearer catalogue-benchmark" },
  });
  const body = await r.text();
  if (!r.ok) throw Error(`${r.status} ${body}`);
  JSON.parse(body);
  return { ms: performance.now() - start, bytes: Buffer.byteLength(body) };
}
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
  if (!ready) throw Error(logs);
  const results = {
    samples,
    clients: 5,
    noc,
    counts,
    byKind,
    storage,
    plans,
    latency: {},
    requests: {
      browse_to_detail: 3,
      grouped_browse_to_detail: 4,
      optional_first_stop_page: 1,
    },
    cold_definition:
      "First request to each scenario in one newly started API process; later scenarios reuse initialized workers; OS caches not cleared",
  };
  for (const [name, s] of Object.entries(scenarios)) {
    const cold = await request(s.path);
    const warm = [];
    let remaining = samples;
    await Promise.all(
      Array.from({ length: 5 }, async () => {
        while (remaining-- > 0) warm.push((await request(s.path)).ms);
      }),
    );
    warm.sort((a, b) => a - b);
    const uncached = [];
    for (let i = 0; i < 10; i++) {
      await request("/api/invalidate", "POST");
      uncached.push((await request(s.path)).ms);
    }
    uncached.sort((a, b) => a - b);
    results.latency[name] = {
      cold_ms: cold.ms,
      warm_p50_ms: warm[Math.floor(warm.length / 2)],
      warm_p95_ms: warm[Math.ceil(warm.length * 0.95) - 1],
      uncached_p50_ms: uncached[5],
      uncached_p95_ms: uncached[9],
      response_bytes: cold.bytes,
      repository_sql_statements: plans[name].length,
    };
  }
  const status = readFileSync(`/proc/${server.pid}/status`, "utf8");
  results.api_peak_rss_kib = Number(status.match(/^VmHWM:\s+(\d+)/m)?.[1]);
  writeFileSync(
    join(directory, "results.json"),
    JSON.stringify(results, null, 2) + "\n",
  );
  console.log(
    JSON.stringify(
      {
        latency: results.latency,
        counts,
        storage,
        api_peak_rss_kib: results.api_peak_rss_kib,
      },
      null,
      2,
    ),
  );
} finally {
  server.kill("SIGTERM");
  if (server.exitCode === null && server.signalCode === null)
    await new Promise((r) => server.once("exit", r));
  writeFileSync(join(directory, "api.log"), logs);
}
