import { defineHandler } from "nitro";
import { db } from "~/db/timetables";
import { serialize } from "~/server/utils/serialize";

export default defineHandler(async () => {
  const agencies = await db.query.agencies.findMany();
  return serialize(agencies);
});
