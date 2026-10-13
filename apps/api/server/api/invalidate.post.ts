import { defineHandler } from "nitro";
import { useDb } from "../../utils/db.ts";
export default defineHandler(async (event) => ({
  deleted: await useDb(event).invalidate(),
}));
