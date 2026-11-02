// Build the API and timetable importer first. Uses an existing fares DB read-only.
import { createRequire } from "node:module";
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { createServer } from "node:net";
import { parseArgs } from "node:util";
const root = resolve(import.meta.dirname, "..");
const { values } = parseArgs({
  options: {
    database: { type: "string" },
    output: { type: "string" },
    samples: { type: "string", default: "100" },
  },
});
if (!values.database || !values.output)
  throw new Error("--database PATH --output NEW_SSD_DIRECTORY [--samples 100]");
const samples = Number(values.samples);
if (!Number.isInteger(samples) || samples < 5)
  throw new Error("samples must be at least 5");
const directory = resolve(values.output);
mkdirSync(directory, { recursive: false });
const database = resolve(values.database);
const require = createRequire(resolve(root, "apps/api/package.json"));
const Database = require("better-sqlite3");
const db = new Database(database, { readonly: true, fileMustExist: true });
db.pragma("query_only=ON");
const product = db
  .prepare(
    "SELECT id,document_id FROM entities WHERE category='product' ORDER BY id LIMIT 1",
  )
  .get();
if (!product) throw new Error("Fares database has no products");
const noc = db
  .prepare(
    "SELECT code FROM links WHERE document_id=? AND kind='operator' AND ref LIKE 'noc:%' ORDER BY code LIMIT 1",
  )
  .get(product.document_id)?.code;
const lateNoc = db
  .prepare(
    "SELECT code FROM links WHERE kind='operator' AND ref LIKE 'noc:%' ORDER BY document_id DESC LIMIT 1",
  )
  .get()?.code;
const plans = {
  products: db
    .prepare(
      "EXPLAIN QUERY PLAN SELECT id FROM entities WHERE category='product' AND id>0 ORDER BY id LIMIT 101",
    )
    .all(),
  document_prices: db
    .prepare(
      "EXPLAIN QUERY PLAN SELECT id FROM entities WHERE document_id=? AND category='price' AND id>0 ORDER BY id LIMIT 101",
    )
    .all(product.document_id),
  operator_products: db
    .prepare(
      "EXPLAIN QUERY PLAN SELECT id FROM entities WHERE category='product' AND id>0 AND document_id IN (SELECT document_id FROM links WHERE kind='operator' AND code=? AND ref NOT LIKE 'uic:%') ORDER BY id LIMIT 101",
    )
    .all(noc ?? ""),
  late_operator_products: db
    .prepare(
      "EXPLAIN QUERY PLAN SELECT * FROM entities WHERE category='product' AND id>0 AND document_id IN (SELECT document_id FROM links WHERE kind='operator' AND code=? AND ref NOT LIKE 'uic:%') ORDER BY id LIMIT 101",
    )
    .all(lateNoc ?? ""),
};
db.close();
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
      FARES_DB_PATH: database,
      FARES_WORKER_PATH: resolve(
        root,
        "apps/api/.output/server/fares-worker.mjs",
      ),
      AUTH_KEY: "benchmark-only-token",
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
async function request(path) {
  const start = performance.now();
  const response = await fetch(base + path, {
    headers: { Authorization: "Bearer benchmark-only-token" },
  });
  const text = await response.text();
  if (!response.ok) throw new Error(`${response.status}: ${text}`);
  JSON.parse(text);
  return performance.now() - start;
}
try {
  let ready = false;
  for (let i = 0; i < 200; i++) {
    try {
      if ((await fetch(base + "/up")).ok) {
        ready = true;
        break;
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 50));
  }
  if (!ready) throw new Error(logs);
  const paths = {
    products: "/api/fares/products?limit=100",
    prices: `/api/fares/prices?document_id=${product.document_id}&limit=100`,
    product: `/api/fares/products/${product.id}`,
    ...(noc
      ? {
          operator: `/api/fares/products?noc=${encodeURIComponent(noc)}&limit=100`,
        }
      : {}),
    ...(lateNoc && lateNoc !== noc
      ? {
          late_operator: `/api/fares/products?noc=${encodeURIComponent(lateNoc)}&limit=100`,
        }
      : {}),
  };
  const results = { samples, clients: 5, plans, latency: {} };
  for (const [name, path] of Object.entries(paths)) {
    const cold = await request(path);
    await request(path);
    const timings = [];
    let remaining = samples;
    await Promise.all(
      Array.from({ length: 5 }, async () => {
        while (remaining-- > 0) timings.push(await request(path));
      }),
    );
    timings.sort((a, b) => a - b);
    results.latency[name] = {
      cold_ms: cold,
      p50_ms: timings[Math.floor(timings.length * 0.5)],
      p95_ms:
        timings[
          Math.min(timings.length - 1, Math.ceil(timings.length * 0.95) - 1)
        ],
      maximum_ms: timings.at(-1),
    };
  }
  writeFileSync(
    join(directory, "results.json"),
    JSON.stringify(results, null, 2) + "\n",
  );
  console.log(JSON.stringify(results, null, 2));
} finally {
  server.kill("SIGTERM");
  if (server.exitCode === null && server.signalCode === null)
    await new Promise((r) => server.once("exit", r));
  writeFileSync(join(directory, "api.log"), logs);
}
