import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { schema } from "~/db/timetables";
import { findAgencyByIdOrNoc } from "#utils/agencies.ts";
import { useDb } from "#utils/db.ts";
import { useKv, withKvCache } from "#utils/kv.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const kv = useKv(event);
  const agencyId = getRouterParam(event, "agencyId")!;
  return withKvCache(kv, event.path, 86400, async () => {
    const agency = await findAgencyByIdOrNoc(db, agencyId);
    if (!agency) {
      throw new HTTPError({ status: 404, body: { success: false, error: "Agency not found" } });
    }
    const routes = await db.query.routes.findMany({
      where: eq(schema.routes.agencyId, agency.id),
      with: { trips: true },
    });
    return serialize(routes);
  });
});
