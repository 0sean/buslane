import { createHash } from "node:crypto";
import { HTTPError, getQuery, type HTTPEvent } from "h3";
import { PoolError, type QueryPool } from "./pool.ts";
import type { FaresQuery } from "../db/fares/repository.ts";
import { useDb } from "./db.ts";

export async function fares(
  event: HTTPEvent,
  query: FaresQuery,
): Promise<Response> {
  const pool = (event as HTTPEvent & { context?: { fares?: QueryPool } })
    .context?.fares;
  if (!pool)
    throw new HTTPError({
      status: 503,
      body: { success: false, error: "Fares database is not configured" },
    });
  const params = getQuery(event);
  const fixedFilters = { ...query };
  for (const key of [
    "limit",
    "document_id",
    "category",
    "ref",
    "product_ref",
    "noc",
    "line",
    "stop",
    "cursor",
    "q",
    "generation",
    "route_id",
    "origin",
    "destination",
    "date",
    "include_unclassified",
    "family_id",
  ] as const) {
    const value = params[key];
    if (value === undefined) continue;
    if (typeof value !== "string" || !value.length || value.length > 2048)
      throw new HTTPError({
        status: 400,
        body: { success: false, error: `Invalid ${key}` },
      });
    if (key === "limit" || key === "document_id") {
      if (
        !/^[1-9]\d*$/.test(value) ||
        !Number.isSafeInteger(Number(value)) ||
        (key === "limit" && Number(value) > 1000)
      )
        throw new HTTPError({
          status: 400,
          body: { success: false, error: `Invalid ${key}` },
        });
      query[key] = Number(value);
    } else query[key] = value;
  }
  query.limit ??= query.kind === "catalogue" ? 50 : 100;
  for (const key of [
    "category",
    "noc",
    "line",
    "stop",
    "ref",
    "product_ref",
    "document_id",
    "route_id",
  ] as const)
    if (fixedFilters[key] !== undefined)
      Object.assign(query, { [key]: fixedFilters[key] });
  try {
    const result = await pool.query(query, event.req.signal);
    const headers = new Headers({
      "content-type": "application/json; charset=utf-8",
      "x-fares-generation": String(result.generation),
      "cache-control": "private, no-store",
    });
    if (query.kind === "catalogue") {
      headers.set("x-catalogue-version", "1");
      headers.set("x-catalogue-generation", String(result.generation));
      const etag =
        '"' +
        createHash("sha256")
          .update(JSON.stringify([result.generation, query]))
          .digest("hex") +
        '"';
      headers.set("etag", etag);
      headers.set("cache-control", "private, no-cache");
      if (
        event.req.headers.get("if-none-match") === etag &&
        typeof result.body === "string"
      )
        return new Response(null, { status: 304, headers });
    }
    if (result.nextCursor) headers.set("x-next-cursor", result.nextCursor);
    if (query.kind === "fareXml") {
      headers.set("content-type", "application/xml; charset=utf-8");
    }
    return new Response(result.body, { headers });
  } catch (error) {
    if (error instanceof PoolError)
      throw new HTTPError({
        status: error.status,
        body: { success: false, error: error.message },
      });
    throw error;
  }
}

export async function timetableFares(
  event: HTTPEvent,
  kind: "agencyFareContext" | "routeFareContext" | "stopFareContext",
  id: string | undefined,
) {
  try {
    const context = await useDb(event).query({ kind, id }, event.req.signal);
    const keys = (await new Response(context.body).json()) as {
      noc?: string;
      line?: string;
      stop?: string;
    };
    if (
      (kind !== "stopFareContext" && !keys.noc) ||
      (kind === "stopFareContext" && !keys.stop)
    )
      throw new HTTPError({
        status: 422,
        body: {
          success: false,
          error: "Timetable record has no fare matching code",
        },
      });
    const response = await fares(event, { kind: "fareProducts", ...keys });
    response.headers.set("x-timetable-generation", String(context.generation));
    return response;
  } catch (error) {
    if (error instanceof PoolError)
      throw new HTTPError({
        status: error.status,
        body: { success: false, error: error.message },
      });
    throw error;
  }
}
