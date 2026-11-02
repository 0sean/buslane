import { QueryPool } from "../../utils/pool.ts";
export default (nitroApp: any) => {
  const database = process.env.FARES_DB_PATH;
  if (!database) return;
  const pool = new QueryPool({
    database,
    resource: "fares",
    workers: Number(process.env.FARES_WORKERS ?? 1),
    cacheBytes: Number(process.env.FARES_CACHE_BYTES ?? 8388608),
  });
  nitroApp.hooks.hook("request", (event: any) => {
    event.context.fares = pool;
  });
  nitroApp.hooks.hook("close", () => pool.close());
};
