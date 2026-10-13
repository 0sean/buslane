import { defineHandler } from "nitro";
import { timetable } from "#utils/timetable.ts";
export default defineHandler((event) => timetable(event, { kind: "agencies" }));
