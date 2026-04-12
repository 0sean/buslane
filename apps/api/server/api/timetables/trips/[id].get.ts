import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { db, schema } from "~/db/timetables";
import { serialize } from "~/server/utils/serialize";

export default defineHandler(async (event) => {
  const id = getRouterParam(event, "id")!;
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
