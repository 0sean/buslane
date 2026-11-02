import { defineHandler } from "nitro";
import { getRouterParam } from "h3";
import { fares } from "../../../../../utils/fares.ts";
export default defineHandler((event) =>
  fares(event, { kind: "fareXml", id: getRouterParam(event, "id") }),
);
