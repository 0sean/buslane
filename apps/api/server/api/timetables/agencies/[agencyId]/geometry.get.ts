import { defineHandler } from "nitro";
import { getRouterParam } from "h3";
import { timetable } from "#utils/timetable.ts";
export default defineHandler((event) =>
  timetable(event, {
    kind: "agencyGeometry",
    id: getRouterParam(event, "agencyId")!,
  }),
);
