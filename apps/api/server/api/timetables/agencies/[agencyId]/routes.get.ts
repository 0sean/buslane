import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { db, schema } from "~/db/timetables";
import { findAgencyByIdOrNoc } from "#utils/agencies.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const agencyId = getRouterParam(event, "agencyId")!;
  const agency = await findAgencyByIdOrNoc(agencyId);
  if (!agency) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Agency not found" } });
  }
  const routes = await db.query.routes.findMany({
    where: eq(schema.routes.agencyId, agency.id),
    with: { trips: true },
  });
  return serialize(routes);
});
