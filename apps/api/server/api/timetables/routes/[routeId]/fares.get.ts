import { defineHandler } from "nitro";
import { getRouterParam } from "h3";
import { timetableFares } from "../../../../../utils/fares.ts";
export default defineHandler((event) =>
  timetableFares(event, "routeFareContext", getRouterParam(event, "routeId")),
);
