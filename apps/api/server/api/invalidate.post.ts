import { defineHandler } from "nitro";
import { useDb } from "../../utils/db.ts";
import type { QueryPool } from "../../utils/pool.ts";
export default defineHandler(async (event) => ({
  deleted: (
    await Promise.all([
      useDb(event).invalidate(),
      (event.context as { fares?: QueryPool }).fares?.invalidate() ?? 0,
    ])
  ).reduce((sum, count) => sum + count, 0),
}));
