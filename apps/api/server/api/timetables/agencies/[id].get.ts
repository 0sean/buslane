import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { findAgencyByIdOrNoc } from "#utils/agencies.ts";
import { useDb } from "#utils/db.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const db = useDb(event);
  const id = getRouterParam(event, "id")!;
  const agency = await findAgencyByIdOrNoc(db, id);
  if (!agency) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Agency not found" } });
  }
  return serialize(agency);
});
