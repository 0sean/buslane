import { defineHandler } from "nitro";
import { useDb } from "#utils/db.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const stops = await db.query.stops.findMany();
  return serialize(stops);
});
