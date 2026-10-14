import { MessageChannel, Worker } from "node:worker_threads";
import { resolve } from "node:path";
import type { Query } from "../db/repository.ts";

export interface PoolResult {
  body: string | ReadableStream<Uint8Array>;
  generation: number;
  importedAt: string;
  importAgeSeconds?: number;
  nextCursor?: string;
}
export class PoolError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
interface Slot {
  worker: Worker;
  busy: boolean;
  ready: boolean;
}
interface Task {
  type: "query" | "health";
  query?: Query;
  resolve: (value: PoolResult) => void;
  reject: (error: unknown) => void;
  signal?: AbortSignal;
  deadline: number;
  queueCleanup?: () => void;
}
export class QueryPool {
  private slots: Slot[] = [];
  private queue: Task[] = [];
  private closed = false;
  private invalidations = new Map<
    number,
    {
      remaining: number;
      deleted: number;
      resolve: (n: number) => void;
      reject: (error: unknown) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  private sequence = 0;
  constructor(
    private options: {
      database: string;
      workerPath?: string;
      workers?: number;
      maximumQueue?: number;
      timeoutMs?: number;
      cacheBytes?: number;
    },
  ) {
    const count = options.workers ?? 2;
    if (!Number.isInteger(count) || count < 1 || count > 4)
      throw new Error("TIMETABLES_WORKERS must be between 1 and 4");
    for (let i = 0; i < count; i++) this.spawn();
  }
  private spawn() {
    if (this.closed) return;
    const workerPath =
      this.options.workerPath ??
      process.env.TIMETABLES_WORKER_PATH ??
      resolve(
        process.cwd(),
        process.cwd().endsWith("/apps/api")
          ? ".output/server/timetable-worker.mjs"
          : "apps/api/.output/server/timetable-worker.mjs",
      );
    const slot: Slot = {
      worker: new Worker(workerPath, {
        workerData: {
          database: this.options.database,
          cacheBytes: this.options.cacheBytes,
        },
      }),
      busy: false,
      ready: false,
    };
    this.slots.push(slot);
    slot.worker.on("message", (message) => {
      if (message.type === "ready") {
        slot.ready = true;
        this.drain();
      }
      if (message.type === "invalidated") {
        const request = this.invalidations.get(message.id);
        if (!request) return;
        request.deleted += message.deleted;
        if (--request.remaining === 0) {
          clearTimeout(request.timer);
          request.resolve(request.deleted);
          this.invalidations.delete(message.id);
        }
      }
    });
    slot.worker.on("error", (error) => {
      console.error(
        "SQLite worker failed:",
        error instanceof Error ? error.message : String(error),
      );
      if (!slot.ready)
        for (const task of this.queue.splice(0)) {
          task.queueCleanup?.();
          task.reject(new PoolError(503, "Timetable database is unavailable"));
        }
    });
    slot.worker.on("exit", () => {
      this.slots = this.slots.filter((s) => s !== slot);
      // Do not spin on startup failures (missing/incompatible database).
      if (slot.ready && !this.closed) this.spawn();
    });
  }
  query(query: Query, signal?: AbortSignal) {
    return this.submit({ type: "query", query }, signal);
  }
  health(signal?: AbortSignal) {
    return this.submit({ type: "health" }, signal);
  }
  private submit(
    input: { type: "query" | "health"; query?: Query },
    signal?: AbortSignal,
  ): Promise<PoolResult> {
    return new Promise((resolve, reject) => {
      if (this.closed || !this.slots.length) {
        reject(new PoolError(503, "Timetable database is unavailable"));
        return;
      }
      if (signal?.aborted) {
        reject(new PoolError(499, "Request cancelled"));
        return;
      }
      if (this.queue.length >= (this.options.maximumQueue ?? 32)) {
        reject(new PoolError(503, "Timetable query queue is full"));
        return;
      }
      const task: Task = {
        ...input,
        resolve,
        reject,
        signal,
        deadline: Date.now() + (this.options.timeoutMs ?? 30000),
      };
      this.queue.push(task);
      const queuedTimer = setTimeout(() => {
        const index = this.queue.indexOf(task);
        if (index >= 0) {
          this.queue.splice(index, 1);
          task.queueCleanup?.();
          reject(new PoolError(504, "Timetable query timed out in queue"));
        }
      }, this.options.timeoutMs ?? 30000);
      const abort = () => {
        const index = this.queue.indexOf(task);
        if (index >= 0) {
          this.queue.splice(index, 1);
          task.queueCleanup?.();
          reject(new PoolError(499, "Request cancelled"));
        }
      };
      signal?.addEventListener("abort", abort, { once: true });
      task.queueCleanup = () => {
        clearTimeout(queuedTimer);
        signal?.removeEventListener("abort", abort);
      };
      this.drain();
    });
  }
  private drain() {
    for (const slot of this.slots) {
      if (slot.ready && !slot.busy && this.queue.length)
        this.execute(slot, this.queue.shift()!);
    }
  }
  private execute(slot: Slot, task: Task) {
    task.queueCleanup?.();
    slot.busy = true;
    const { port1, port2 } = new MessageChannel();
    let controller: ReadableStreamDefaultController<Uint8Array> | undefined;
    let settled = false;
    let finished = false;
    let waiting = false;
    const started = performance.now();
    const release = (replace = false) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      task.signal?.removeEventListener("abort", abort);
      slot.worker.removeListener("exit", exit);
      port1.close();
      slot.busy = replace;
      if (replace) void slot.worker.terminate();
      this.drain();
      console.info(
        JSON.stringify({
          event: "timetable_query",
          kind: task.query?.kind ?? "health",
          duration_ms: Math.round(performance.now() - started),
        }),
      );
    };
    const fail = (error: unknown, replace = false) => {
      if (finished) return;
      if (settled) controller?.error(error);
      else task.reject(error);
      release(replace);
    };
    const abort = () => {
      fail(new PoolError(499, "Request cancelled"), true);
    };
    const exit = () => fail(new PoolError(503, "Query worker stopped"));
    const timer = setTimeout(
      () => {
        fail(new PoolError(504, "Timetable query timed out"), true);
      },
      Math.max(1, task.deadline - Date.now()),
    );
    task.signal?.addEventListener("abort", abort, { once: true });
    slot.worker.once("exit", exit);
    port1.on("message", (message) => {
      if (finished) return;
      if (message.type === "error") {
        fail(new PoolError(message.status, message.error));
        return;
      }
      if (message.type === "complete") {
        settled = true;
        task.resolve(message);
        release();
        return;
      }
      if (message.type === "start") {
        settled = true;
        const body = new ReadableStream<Uint8Array>(
          {
            start(c) {
              controller = c;
            },
            pull() {
              if (!waiting && !finished) {
                waiting = true;
                port1.postMessage({ type: "pull" });
              }
            },
            cancel() {
              release(true);
            },
          },
          { highWaterMark: 1 },
        );
        task.resolve({ ...message, body });
        return;
      }
      if (message.type === "chunk") {
        waiting = false;
        controller!.enqueue(new TextEncoder().encode(message.chunk));
        return;
      }
      if (message.type === "end") {
        controller!.close();
        release();
      }
    });
    slot.worker.postMessage(
      { type: task.type, query: task.query, port: port2 },
      [port2],
    );
  }
  invalidate(): Promise<number> {
    return new Promise((resolve, reject) => {
      if (this.closed || !this.slots.length) {
        reject(new PoolError(503, "Timetable database is unavailable"));
        return;
      }
      const id = ++this.sequence;
      const timer = setTimeout(() => {
        this.invalidations.delete(id);
        reject(new PoolError(504, "Cache clear timed out"));
      }, 5000);
      this.invalidations.set(id, {
        remaining: this.slots.length,
        deleted: 0,
        resolve,
        reject,
        timer,
      });
      for (const slot of this.slots)
        slot.worker.postMessage({ type: "invalidate", id });
    });
  }
  async close() {
    this.closed = true;
    for (const task of this.queue.splice(0)) {
      task.queueCleanup?.();
      task.reject(new PoolError(503, "Server shutting down"));
    }
    for (const request of this.invalidations.values()) {
      clearTimeout(request.timer);
      request.reject(new PoolError(503, "Server shutting down"));
    }
    this.invalidations.clear();
    await Promise.all(this.slots.map((s) => s.worker.terminate()));
    this.slots = [];
  }
}
