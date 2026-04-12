import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { db, schema } from "~/db/timetables";
import { serialize } from "~/server/utils/serialize";

export default defineHandler(async (event) => {
  const id = getRouterParam(event, "id")!;
  const route = await db.query.routes.findFirst({
    where: eq(schema.routes.id, Number(id)),
    with: { trips: true },
  });
  if (!route) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Route not found" } });
  }
  return serialize(route);
});
