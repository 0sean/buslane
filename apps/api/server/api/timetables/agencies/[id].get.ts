import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { findAgencyByIdOrNoc } from "#utils/agencies.ts";
import { useDb } from "#utils/db.ts";
import { useKv, withKvCache } from "#utils/kv.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const kv = useKv(event);
  const id = getRouterParam(event, "id")!;
  return withKvCache(kv, event.path, 86400, async () => {
    const agency = await findAgencyByIdOrNoc(db, id);
    if (!agency) {
      throw new HTTPError({ status: 404, body: { success: false, error: "Agency not found" } });
    }
    return serialize(agency);
  });
});
