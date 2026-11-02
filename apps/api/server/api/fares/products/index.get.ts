import { defineHandler } from "nitro";
import { fares } from "../../../../utils/fares.ts";
export default defineHandler((event) => fares(event, { kind: "fareProducts" }));
