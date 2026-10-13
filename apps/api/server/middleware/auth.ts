import { defineHandler, HTTPError } from "nitro";
import { timingSafeEqual } from "node:crypto";
export default defineHandler((event) => {
  if (!event.url.pathname.startsWith("/api")) return;
  const expected = process.env.AUTH_KEY;
  const value = event.req.headers.get("Authorization");
  const key = value?.startsWith("Bearer ") ? value.slice(7) : undefined;
  if (
    !expected ||
    !key ||
    Buffer.byteLength(key) !== Buffer.byteLength(expected) ||
    !timingSafeEqual(Buffer.from(key), Buffer.from(expected))
  )
    throw new HTTPError({
      status: 400,
      body: { success: false, error: "Unauthorized" },
    });
});
