import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { db, schema } from "~/db/timetables";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const id = getRouterParam(event, "id")!;
  const service = await db.query.services.findFirst({
    where: eq(schema.services.id, Number(id)),
    with: { calendarDates: true },
  });
  if (!service) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Service not found" } });
  }
  return serialize(service);
});
