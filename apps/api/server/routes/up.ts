import { defineHandler } from "nitro";
import { HTTPError } from "h3";
import { useDb } from "../../utils/db.ts";
export default defineHandler(async (event) => {
  try {
    const result = await useDb(event).health(event.req.signal);
    return new Response(result.body, {
      headers: {
        "content-type": "application/json",
        "x-timetable-generation": String(result.generation),
      },
    });
  } catch {
    throw new HTTPError({
      status: 503,
      body: { success: false, error: "Timetable database is unavailable" },
    });
  }
});
