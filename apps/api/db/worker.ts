import { parentPort, workerData, type MessagePort } from "node:worker_threads";
import { openReader, SCHEMA_VERSION } from "./timetables/index.ts";
import { Repository, QueryError, type Query } from "./repository.ts";
import { ByteCache } from "./cache.ts";

const { sqlite } = openReader(workerData.database);
const repository = new Repository(sqlite);
const cache = new ByteCache<{ body: string; nextCursor?: string }>(
  workerData.cacheBytes ?? 32 * 1024 * 1024,
);
const maximum = 1024 * 1024;
let active = false;
parentPort!.on("message", (message) => {
  if (message.type === "invalidate") {
    parentPort!.postMessage({
      type: "invalidated",
      id: message.id,
      deleted: cache.clear(),
    });
    return;
  }
  if (message.type === "close") {
    if (sqlite.inTransaction) sqlite.exec("ROLLBACK");
    sqlite.close();
    parentPort!.close();
    return;
  }
  const port: MessagePort = message.port;
  if (active) {
    port.postMessage({
      type: "error",
      status: 503,
      error: "Query worker is busy",
    });
    port.close();
    return;
  }
  active = true;
  const started = performance.now();
  let iterator: Generator<string> | undefined;
  let buffered = "";
  let ended = false;
  const finish = () => {
    if (ended) return;
    ended = true;
    if (sqlite.inTransaction) sqlite.exec("COMMIT");
    active = false;
    port.close();
  };
  const fail = (error: unknown) => {
    if (ended) return;
    ended = true;
    iterator?.return(undefined as never);
    if (sqlite.inTransaction) sqlite.exec("ROLLBACK");
    active = false;
    port.postMessage({
      type: "error",
      status: error instanceof QueryError ? error.status : 500,
      error:
        error instanceof QueryError ? error.message : "Database query failed",
    });
    port.close();
    if (!(error instanceof QueryError)) console.error(error);
  };
  try {
    sqlite.exec("BEGIN");
    const dataset = sqlite
      .prepare("SELECT generation,imported_at FROM dataset WHERE id=1")
      .get() as { generation: number; imported_at: string | null };
    if (
      sqlite.pragma("user_version", { simple: true }) !== SCHEMA_VERSION ||
      !dataset?.generation
    )
      throw new QueryError(503, "Timetable database is not ready");
    const metadata = {
      generation: dataset.generation,
      importedAt: dataset.imported_at,
      importAgeSeconds: dataset.imported_at
        ? Math.max(
            0,
            Math.round(
              (Date.now() -
                Date.parse(dataset.imported_at.replace(" ", "T") + "Z")) /
                1000,
            ),
          )
        : null,
    };
    if (message.type === "health") {
      port.postMessage({
        type: "complete",
        body: JSON.stringify({ success: true }),
        ...metadata,
      });
      finish();
      return;
    }
    const query: Query = message.query;
    const key = JSON.stringify([
      dataset.generation,
      query.kind,
      query.id ?? null,
      query.limit ?? null,
      query.cursor ?? null,
    ]);
    const cached = cache.get(key);
    if (cached) {
      port.postMessage({ type: "complete", ...cached, ...metadata });
      finish();
      return;
    }
    const plan = repository.prepare(query, dataset.generation);
    iterator = plan.chunks;
    let bytes = 0;
    let next: IteratorResult<string>;
    do {
      next = iterator.next();
      if (next.done) break;
      buffered += next.value;
      bytes += Buffer.byteLength(next.value);
    } while (bytes <= maximum);
    if (next.done) {
      const result = { body: buffered, nextCursor: plan.nextCursor() };
      cache.put(key, result, bytes + Buffer.byteLength(key));
      port.postMessage({ type: "complete", ...result, ...metadata });
      finish();
      return;
    }
    port.postMessage({
      type: "start",
      nextCursor: plan.nextCursor(),
      ...metadata,
    });
    port.on("message", (request) => {
      if (ended) return;
      if (request.type === "cancel") {
        iterator?.return(undefined as never);
        finish();
        return;
      }
      if (request.type !== "pull") return;
      try {
        // One acknowledged chunk at a time bounds cross-thread buffering.
        while (buffered.length < 65536) {
          const item = iterator!.next();
          if (item.done) break;
          buffered += item.value;
        }
        if (!buffered) {
          port.postMessage({
            type: "end",
            elapsedMs: performance.now() - started,
          });
          finish();
          return;
        }
        let end = Math.min(65536, buffered.length);
        const last = buffered.charCodeAt(end - 1);
        if (last >= 0xd800 && last <= 0xdbff) end--;
        const chunk = buffered.slice(0, end);
        buffered = buffered.slice(end);
        port.postMessage({ type: "chunk", chunk });
      } catch (error) {
        fail(error);
      }
    });
    port.on("close", () => {
      if (!ended) {
        iterator?.return(undefined as never);
        finish();
      }
    });
  } catch (error) {
    fail(error);
  }
});
parentPort!.postMessage({ type: "ready" });
