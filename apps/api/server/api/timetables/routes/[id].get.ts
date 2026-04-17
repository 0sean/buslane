import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { schema } from "~/db/timetables";
import { useDb } from "#utils/db.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
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
