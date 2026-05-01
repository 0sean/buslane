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
  const id = getRouterParam(event, "id")!;
  return withKvCache(kv, event.path, 86400, async () => {
    const trip = await db.query.trips.findFirst({
      where: eq(schema.trips.id, id),
      with: {
        service: true,
        frequencies: true,
        route: true,
        stopTimes: {
          with: { stop: true },
        },
      },
    });
    if (!trip) {
      throw new HTTPError({ status: 404, body: { success: false, error: "Trip not found" } });
    }
    return serialize(trip);
  });
});
