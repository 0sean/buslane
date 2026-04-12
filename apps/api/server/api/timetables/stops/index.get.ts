import { defineHandler } from "nitro";
import { db } from "~/db/timetables";
import { serialize } from "~/server/utils/serialize";

export default defineHandler(async () => {
  const stops = await db.query.stops.findMany();
  return serialize(stops);
});
