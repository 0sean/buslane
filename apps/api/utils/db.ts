import type { HTTPEvent } from "h3";
import type { QueryPool } from "./pool.ts";
export function useDb(event: HTTPEvent): QueryPool {
  const pool = (event as HTTPEvent & { context: { db?: QueryPool } }).context
    ?.db as QueryPool | undefined;
  if (!pool) throw new Error("Timetable query pool is not initialized");
  return pool;
}
