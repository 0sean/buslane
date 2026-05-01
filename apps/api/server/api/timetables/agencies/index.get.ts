import { defineHandler } from "nitro";
import { useDb } from "#utils/db.ts";
import { useKv, withKvCache } from "#utils/kv.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const kv = useKv(event);
  return withKvCache(kv, event.path, 86400, async () => {
    const agencies = await db.query.agencies.findMany();
    return serialize(agencies);
  });
});
