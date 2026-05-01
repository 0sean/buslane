import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { schema } from "~/db/timetables";
import { useDb } from "#utils/db.ts";
import { useKv, withKvCache } from "#utils/kv.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const kv = useKv(event);
  const routeId = getRouterParam(event, "routeId")!;
  return withKvCache(kv, event.path, 86400, async () => {
    const route = await db.query.routes.findFirst({
      where: eq(schema.routes.id, Number(routeId)),
    });
    if (!route) {
      throw new HTTPError({ status: 404, body: { success: false, error: "Route not found" } });
    }

    const trips = await db.query.trips.findMany({
      where: eq(schema.trips.routeId, route.id),
      with: {
        service: true,
        frequencies: true,
        stopTimes: {
          with: { stop: true },
        },
      },
    });

    return serialize(trips);
  });
});
