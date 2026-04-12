import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { eq } from "drizzle-orm";
import { db, schema } from "~/db/timetables";
import { findAgencyByIdOrNoc } from "~/server/utils/agencies";
import { serialize } from "~/server/utils/serialize";

export default defineHandler(async (event) => {
  const agencyId = getRouterParam(event, "agencyId")!;
  const agency = await findAgencyByIdOrNoc(agencyId);
  if (!agency) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Agency not found" } });
  }

  const stops = await db
    .selectDistinct({
      id: schema.stops.id,
      code: schema.stops.code,
      name: schema.stops.name,
      lat: schema.stops.lat,
      lon: schema.stops.lon,
      wheelchairBoarding: schema.stops.wheelchairBoarding,
      locationType: schema.stops.locationType,
      platformCode: schema.stops.platformCode,
      createdAt: schema.stops.createdAt,
      parentStationId: schema.stops.parentStationId,
    })
    .from(schema.stops)
    .innerJoin(schema.stopTimes, eq(schema.stops.id, schema.stopTimes.stopId))
    .innerJoin(schema.trips, eq(schema.stopTimes.tripId, schema.trips.id))
    .innerJoin(schema.routes, eq(schema.trips.routeId, schema.routes.id))
    .where(eq(schema.routes.agencyId, agency.id));

  return serialize(stops);
});
