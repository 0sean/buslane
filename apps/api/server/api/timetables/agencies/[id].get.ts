import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { findAgencyByIdOrNoc } from "#utils/agencies.ts";
import { serialize } from "#utils/serialize.ts";

export default defineHandler(async (event) => {
  const id = getRouterParam(event, "id")!;
  const agency = await findAgencyByIdOrNoc(id);
  if (!agency) {
    throw new HTTPError({ status: 404, body: { success: false, error: "Agency not found" } });
  }
  return serialize(agency);
});
