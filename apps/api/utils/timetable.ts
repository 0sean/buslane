import { createHash } from "node:crypto";
import { HTTPError, getQuery, type HTTPEvent } from "h3";
import { useDb } from "./db.ts";
import { PoolError } from "./pool.ts";
import type { Query } from "../db/repository.ts";
export async function timetable(
  event: HTTPEvent,
  query: Query,
): Promise<Response> {
  const params = getQuery(event);
  if (params.limit !== undefined || params.cursor !== undefined) {
    const limit = params.limit === undefined ? 100 : Number(params.limit);
    if (
      !Number.isInteger(limit) ||
      limit < 1 ||
      limit > 1000 ||
      Array.isArray(params.limit) ||
      Array.isArray(params.cursor)
    )
      throw new HTTPError({
        status: 400,
        body: {
          success: false,
          error: "limit must be an integer between 1 and 1000",
        },
      });
    query.limit = limit;
    if (params.cursor !== undefined) {
      if (typeof params.cursor !== "string" || params.cursor.length > 2048)
        throw new HTTPError({
          status: 400,
          body: { success: false, error: "Invalid cursor" },
        });
      query.cursor = params.cursor;
    }
  }
  try {
    const result = await useDb(event).query(query, event.req.signal);
    const headers = new Headers({
      "content-type": "application/json; charset=utf-8",
      "x-timetable-generation": String(result.generation),
      "cache-control": "private, no-store",
    });
    if (query.kind === "agencyGeometry") {
      // The persisted complete document is validated before the worker sends it.
      // Hashing the generation and request identity also isolates agencies.
      const etag =
        'W/"geometry-v1-' +
        createHash("sha256")
          .update(JSON.stringify([result.generation, query.id]))
          .digest("hex") +
        '"';
      headers.set("etag", etag);
      headers.set("cache-control", "private, no-cache");
      const matches = event.req.headers
        .get("if-none-match")
        ?.split(",")
        .some((tag) =>
          tag.trim() === "*" ||
          tag.trim().replace(/^W\//, "") === etag.replace(/^W\//, ""),
        );
      if (matches) {
        // Cancel any streamed body so its worker releases the read snapshot.
        if (result.body instanceof ReadableStream) await result.body.cancel();
        return new Response(null, { status: 304, headers });
      }
    }
    if (result.nextCursor) headers.set("x-next-cursor", result.nextCursor);
    console.info(
      JSON.stringify({
        event: "timetable_dataset",
        generation: result.generation,
        imported_at: result.importedAt,
        import_age_seconds: result.importAgeSeconds,
      }),
    );
    return new Response(result.body, { headers });
  } catch (error) {
    if (error instanceof PoolError)
      throw new HTTPError({
        status: error.status,
        headers:
          query.kind === "agencyGeometry"
            ? { "cache-control": "no-store" }
            : undefined,
        body: { success: false, error: error.message },
      });
    if (query.kind === "agencyGeometry")
      throw new HTTPError({
        status: 500,
        headers: { "cache-control": "no-store" },
        body: { success: false, error: "Geometry query failed" },
      });
    throw error;
  }
}
