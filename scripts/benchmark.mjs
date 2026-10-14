import { parseArgs } from "node:util";
import { mkdir, readFile, writeFile, stat } from "node:fs/promises";
import { spawn } from "node:child_process";
import { resolve, join } from "node:path";
import { createServer } from "node:net";
import { createRequire } from "node:module";
import { randomBytes } from "node:crypto";

const { values } = parseArgs({
  options: {
    feed: { type: "string", multiple: true },
    output: { type: "string", default: "benchmark-results" },
    binary: {
      type: "string",
      default: "apps/timetables/target/release/timetables",
    },
    agency: { type: "string" },
    trip: { type: "string" },
    samples: { type: "string", default: "100" },
  },
});
if (!values.feed?.length)
  throw new Error("Provide --feed FIRST.zip and optionally --feed NEXT.zip");
const root = resolve(import.meta.dirname, ".."),
  output = resolve(values.output);
await mkdir(output, { recursive: true });
const database = join(output, "benchmark.sqlite");
try {
  await stat(database);
  throw new Error(
    "Choose a new output directory: the benchmark database already exists",
  );
} catch (error) {
  if (error.code !== "ENOENT") throw error;
}
const binary = resolve(values.binary),
  samples = Number(values.samples);
if (!Number.isInteger(samples) || samples < 5)
  throw new Error("--samples must be an integer >=5");
const require = createRequire(join(root, "apps/api/package.json")),
  Database = require("better-sqlite3");
const imports = [],
  latency = [];
let stopped = false,
  peakWal = 0,
  api;
const run = (command, args, options = {}) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, ...options });
    let stdout = "",
      stderr = "";
    child.stdout.on("data", (b) => (stdout += b));
    child.stderr.on("data", (b) => (stderr += b));
    child.on("error", reject);
    child.on("close", (code) =>
      code === 0
        ? resolve({ stdout, stderr })
        : reject(new Error(command + " failed: " + stderr)),
    );
  });
async function importFeed(feed, label) {
  const metrics = join(output, label + "-resources.json");
  let polling = true;
  const monitor = (async () => {
    while (polling) {
      try {
        peakWal = Math.max(peakWal, (await stat(database + "-wal")).size);
      } catch {}
      await new Promise((r) => setTimeout(r, 50));
    }
  })();
  try {
    const result = await run("/usr/bin/time", [
      "-f",
      '{"wall_seconds":%e,"cpu_user_seconds":%U,"cpu_system_seconds":%S,"max_rss_kib":%M,"filesystem_output_blocks":%O}',
      "-o",
      metrics,
      binary,
      "--db",
      database,
      "--source",
      resolve(feed),
      "--scratch",
      join(output, "scratch"),
    ]);
    const report = JSON.parse(result.stdout.trim());
    imports.push({
      label,
      feed: resolve(feed),
      ...report,
      resources: JSON.parse(await readFile(metrics, "utf8")),
    });
    await writeFile(join(output, label + "-import.log"), result.stderr);
  } finally {
    polling = false;
    await monitor;
  }
}
async function freePort() {
  const server = createServer();
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const port = server.address().port;
  await new Promise((r) => server.close(r));
  return port;
}
const token = randomBytes(24).toString("hex");
let base, paths;
async function probe(label, until) {
  const timings = [];
  let failures = 0,
    index = 0;
  await Promise.all(
    Array.from({ length: 5 }, async () => {
      while (until ? !until() : index < samples) {
        const i = index++;
        const start = performance.now();
        try {
          const r = await fetch(base + paths[i % paths.length], {
            headers: { Authorization: "Bearer " + token },
            signal: AbortSignal.timeout(30000),
          });
          await r.arrayBuffer();
          if (!r.ok) failures++;
          else timings.push(performance.now() - start);
        } catch {
          failures++;
        }
        if (until) await new Promise((r) => setTimeout(r, 50));
      }
    }),
  );
  timings.sort((a, b) => a - b);
  const percentile = (p) =>
    timings[Math.min(timings.length - 1, Math.floor(timings.length * p))] ??
    null;
  latency.push({
    label,
    concurrency: 5,
    successful_requests: timings.length,
    failures,
    p50_ms: percentile(0.5),
    p95_ms: percentile(0.95),
    p99_ms: percentile(0.99),
  });
}
try {
  await importFeed(values.feed[0], "initial");
  const db = new Database(database, { readonly: true });
  const sample = db
    .prepare(
      "SELECT a.public_id agency,t.public_id trip FROM agencies a JOIN routes r ON r.agency_id=a.id JOIN trips t ON t.route_id=r.id LIMIT 1",
    )
    .get();
  db.close();
  const agency = values.agency ?? sample.agency,
    trip = values.trip ?? sample.trip;
  paths = [
    "/api/timetables/agencies/" + encodeURIComponent(agency) + "/stops",
    "/api/timetables/trips/" + encodeURIComponent(trip),
  ];
  const port = await freePort();
  base = "http://127.0.0.1:" + port;
  api = spawn(
    process.execPath,
    [join(root, "apps/api/.output/server/index.mjs")],
    {
      cwd: root,
      env: {
        ...process.env,
        HOST: "127.0.0.1",
        PORT: String(port),
        AUTH_KEY: token,
        SENTRY_DSN: "",
        TIMETABLES_DB_PATH: database,
        TIMETABLES_WORKER_PATH: join(
          root,
          "apps/api/.output/server/timetable-worker.mjs",
        ),
      },
    },
  );
  const apiLog = [];
  api.stdout.on("data", (b) => apiLog.push(b));
  api.stderr.on("data", (b) => apiLog.push(b));
  let ready = false;
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base + "/up")).ok) {
        ready = true;
        break;
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  if (!ready) throw new Error("Benchmark API failed to start");
  // Warm both readers before recording the baseline.
  await probe("warmup");
  await probe("warm");
  stopped = false;
  const repeated = importFeed(values.feed[0], "identical").finally(
    () => (stopped = true),
  );
  await Promise.all([repeated, probe("during-identical", () => stopped)]);
  for (let i = 1; i < values.feed.length; i++) {
    stopped = false;
    const update = importFeed(values.feed[i], "daily-" + i).finally(
      () => (stopped = true),
    );
    await Promise.all([update, probe("during-daily-" + i, () => stopped)]);
  }
  const inspect = new Database(database, { readonly: true });
  let sizes;
  try {
    sizes = inspect
      .prepare("SELECT name,sum(pgsize) bytes FROM dbstat GROUP BY name")
      .all();
  } catch {
    sizes = [];
  }
  const plans = [
    "SELECT * FROM stop_times WHERE trip_id=1 ORDER BY stop_sequence",
    "SELECT * FROM stop_times WHERE stop_id=1 ORDER BY id",
    "SELECT * FROM trips WHERE route_id=1 ORDER BY public_id",
    "SELECT * FROM agency_stops WHERE agency_id=1",
  ].map((sql) => ({
    sql,
    plan: inspect.prepare("EXPLAIN QUERY PLAN " + sql).all(),
  }));
  inspect.close();
  const repeatedReport = imports.find((i) => i.label === "identical").report;
  const result = {
    generated_at: new Date().toISOString(),
    imports,
    latency,
    peak_wal_bytes: peakWal,
    database_bytes: (await stat(database)).size,
    table_and_index_sizes: sizes,
    query_plans: plans,
    acceptance: {
      identical_timetable_changes: Object.values(repeatedReport.tables).every(
        (t) => t.inserted + t.updated + t.deleted === 0,
      ),
      importer_below_512_mib: imports.every(
        (i) => i.resources.max_rss_kib < 512 * 1024,
      ),
      imports_within_daily_window: imports.every(
        (i) => i.resources.wall_seconds < 86400,
      ),
      warm_p95_below_250_ms: latency
        .filter((l) => l.label !== "warmup")
        .every((l) => l.p95_ms !== null && l.p95_ms < 250 && l.failures === 0),
    },
  };
  await writeFile(
    join(output, "results.json"),
    JSON.stringify(result, null, 2) + "\n",
  );
  await writeFile(join(output, "api.log"), Buffer.concat(apiLog));
  console.log(JSON.stringify(result, null, 2));
} finally {
  if (api && api.exitCode === null) {
    api.kill("SIGTERM");
    await new Promise((r) => api.once("exit", r));
  }
}
