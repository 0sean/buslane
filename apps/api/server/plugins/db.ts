import { QueryPool } from "../../utils/pool.ts";
export default (nitroApp: any) => {
  const database = process.env.TIMETABLES_DB_PATH;
  if (!database) throw new Error("TIMETABLES_DB_PATH is required");
  const pool = new QueryPool({
    database,
    workers: Number(process.env.TIMETABLES_WORKERS ?? 2),
    cacheBytes: Number(process.env.TIMETABLES_CACHE_BYTES ?? 33554432),
  });
  nitroApp.hooks.hook("request", (event: any) => {
    event.context.db = pool;
  });
  nitroApp.hooks.hook("close", () => pool.close());
};
