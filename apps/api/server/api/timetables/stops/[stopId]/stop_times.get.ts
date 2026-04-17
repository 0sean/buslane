import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { schema } from "~/db/timetables";
import { useDb } from "#utils/db.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const stopId = getRouterParam(event, "stopId")!;
  const stop = await db.query.stops.findFirst({
    where: eq(schema.stops.id, stopId),
  });
  if (!stop) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Stop not found" } });
  }

  const stopTimes = await db.query.stopTimes.findMany({
    where: eq(schema.stopTimes.stopId, stopId),
    with: {
      trip: {
        with: {
          route: {
            with: { agency: true },
          },
          service: true,
        },
      },
    },
  });

  return serialize(stopTimes);
});
