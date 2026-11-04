import type Database from "better-sqlite3";
import { createHash } from "node:crypto";
import { QueryError } from "../repository.ts";
import type { FaresQuery } from "./repository.ts";

type Row = Record<string, any>;
const columns =
  "o.id,o.kind,o.name,o.revision,o.ticket_count,o.family_id,o.summary";
const compact = (r: Row) => ({
  ...r,
  ...JSON.parse(r.summary),
  summary: undefined,
});
export function catalogue(
  db: Database.Database,
  q: FaresQuery,
  generation: number,
) {
  const get = (sql: string, ...p: any[]) =>
    db.prepare(sql).get(...p) as Row | undefined;
  const all = (sql: string, ...p: any[]) => db.prepare(sql).all(...p) as Row[];
  if (
    get("SELECT catalogue_version FROM dataset WHERE id=1")
      ?.catalogue_version !== 1
  )
    throw new QueryError(
      503,
      "Catalogue projection is not ready; run the matching fares importer",
    );
  if (q.generation !== undefined && !/^[1-9]\d*$/.test(q.generation))
    throw new QueryError(400, "Invalid catalogue generation");
  if (
    q.include_unclassified !== undefined &&
    !["true", "false"].includes(q.include_unclassified)
  )
    throw new QueryError(400, "include_unclassified must be true or false");
  if (q.generation !== undefined && q.generation !== String(generation))
    throw new QueryError(409, "Fares catalogue changed; restart navigation");
  const limit = q.limit ?? 50;
  if (!Number.isInteger(limit) || limit < 1 || limit > 100)
    throw new QueryError(400, "Catalogue limit must be between 1 and 100");
  if (!q.noc || q.noc.length > 100)
    throw new QueryError(400, "Invalid operator");
  const required = (id: string | undefined, kinds: string[]) => {
    if (!id || !/^[a-z]+_[a-f0-9]{64}$/.test(id))
      throw new QueryError(400, "Invalid catalogue ID");
    const row = get(
      "SELECT * FROM catalogue_objects WHERE id=? AND noc=?",
      id,
      q.noc,
    );
    if (!row || !kinds.includes(row.kind))
      throw new QueryError(404, "Catalogue item not found");
    if (!row.active)
      throw new QueryError(
        410,
        "Catalogue item removed or repartitioned; see catalogue changes",
      );
    return row;
  };
  const binding = createHash("sha256")
    .update(
      JSON.stringify(
        Object.entries(q)
          .filter(([k]) => k !== "cursor")
          .sort(([a], [b]) => a.localeCompare(b)),
      ),
    )
    .digest("hex");
  let after: string[] = [];
  if (q.cursor) {
    try {
      const t = JSON.parse(Buffer.from(q.cursor, "base64url").toString("utf8"));
      if (t.generation !== generation)
        throw new QueryError(
          409,
          "Fares catalogue changed; restart pagination",
        );
      if (
        t.binding !== binding ||
        !Array.isArray(t.after) ||
        t.after.length > 4 ||
        !t.after.every((x: unknown) => typeof x === "string" && x.length <= 512)
      )
        throw Error();
      after = t.after;
    } catch (e) {
      if (e instanceof QueryError) throw e;
      throw new QueryError(400, "Invalid catalogue cursor");
    }
  }
  let rows: Row[] = [];
  let keys: string[] = ["id"];
  let single: Row | undefined;
  const params: any[] = [q.noc];
  const where = ["o.noc=?", "o.active=1"];
  const action = q.action;
  if (
    [
      "areas",
      "tickets",
      "families",
      "routes",
      "areaTickets",
      "areaFamilies",
      "familyTickets",
    ].includes(action ?? "")
  ) {
    const kind =
      action === "areas"
        ? "area"
        : action === "routes"
          ? "route"
          : action === "families" || action === "areaFamilies"
            ? "family"
            : "ticket";
    where.push("o.kind=?");
    params.push(kind);
    let from = "catalogue_objects o";
    let order = "o.id";
    if (
      (kind === "ticket" || kind === "family") &&
      q.include_unclassified !== "true"
    )
      where.push("o.travel=1");
    if (
      action === "areaTickets" ||
      action === "areaFamilies" ||
      action === "familyTickets"
    ) {
      required(q.id, [action !== "familyTickets" ? "area" : "family"]);
      from =
        "catalogue_edges e CROSS JOIN catalogue_objects o ON o.id=e.source_id";
      where.push("e.target_id=? AND e.relation=?");
      params.push(q.id, action !== "familyTickets" ? "area" : "family");
      if (q.include_unclassified !== "true") where.push("e.travel=1");
      order = "e.source_id";
      if (action === "areaFamilies") {
        from =
          "catalogue_area_families e CROSS JOIN catalogue_objects o ON o.id=e.family_id";
        where.splice(
          where.findIndex((w) => w.startsWith("e.target_id")),
          1,
          "e.area_id=?",
        );
        params.pop();
        order = "e.family_id";
      } else if (action === "areaTickets" && q.family_id) {
        required(q.family_id, ["family"]);
        from =
          "catalogue_area_ticket_variants e CROSS JOIN catalogue_objects o ON o.id=e.ticket_id";
        where.splice(
          where.findIndex((w) => w.startsWith("e.target_id")),
          1,
          "e.area_id=? AND e.family_id=?",
        );
        params[params.length - 1] = q.family_id;
        order = "e.ticket_id";
      }
    }
    if (q.q) {
      if (q.q.length > 120)
        throw new QueryError(400, "Search is limited to 120 characters");
      const search = q.q.toLowerCase();
      if (after.length && (after.length !== 2 || !after[0]!.startsWith(search)))
        throw new QueryError(400, "Invalid search cursor");
      const searchColumn =
        from !== "catalogue_objects o" ? "e.search" : "o.search";
      keys = ["search", "id"];
      const select = `SELECT ${columns},o.search${action === "areaFamilies" ? ",e.variant_count" : ""} FROM ${from} WHERE ${where.join(" AND ")}`;
      // Two bounded index seeks avoid a skip-scan/sort chosen by SQLite 3.53
      // for a redundant (search,id) range alongside the prefix bounds.
      if (after.length) {
        rows = all(
          `${select} AND ${searchColumn}=? AND ${order}>? ORDER BY ${searchColumn},${order} LIMIT ?`,
          ...params,
          after[0],
          after[1],
          limit + 1,
        );
      }
      if (rows.length < limit + 1) {
        rows.push(
          ...all(
            `${select} AND ${searchColumn}${after.length ? ">" : ">="}? AND ${searchColumn}<? ORDER BY ${searchColumn},${order} LIMIT ?`,
            ...params,
            after[0] ?? search,
            search + "\u{10ffff}",
            limit + 1 - rows.length,
          ),
        );
      }
    } else {
      where.push(`${order}>?`);
      params.push(after[0] ?? "");
      rows = all(
        `SELECT ${columns}${action === "areaFamilies" ? ",e.variant_count" : ""} FROM ${from} WHERE ${where.join(" AND ")} ORDER BY ${order} LIMIT ?`,
        ...params,
        limit + 1,
      );
    }
  } else if (action === "detail" || action === "geometry") {
    const row = required(q.id, ["ticket", "area", "family", "route", "point"]);
    single =
      action === "geometry"
        ? {
            id: row.id,
            geometry: null,
            status: "unavailable",
            reason:
              "Published stop membership is available separately; source geometry is retained in original XML",
          }
        : {
            ...compact(row),
            detail: JSON.parse(row.detail),
            source_count: row.source_count,
            semantic_key: undefined,
            noc: q.noc,
            search: undefined,
            active: undefined,
          };
  } else if (action === "related") {
    required(q.id, ["ticket"]);
    if (!["area", "route", "family"].includes(q.relation ?? ""))
      throw new QueryError(400, "Invalid catalogue relationship");
    rows = all(
      `SELECT ${columns} FROM catalogue_edges e CROSS JOIN catalogue_objects o ON o.id=e.target_id WHERE e.source_id=? AND e.relation=? AND e.target_id>? AND o.active=1 ORDER BY e.target_id LIMIT ?`,
      q.id,
      q.relation,
      after[0] ?? "",
      limit + 1,
    );
  } else if (action === "stops") {
    required(q.id, ["area", "point"]);
    keys = ["code"];
    rows = all(
      "SELECT code,name FROM catalogue_stops WHERE object_id=? AND code>? ORDER BY code LIMIT ?",
      q.id,
      after[0] ?? "",
      limit + 1,
    );
  } else if (action === "sources") {
    required(q.id, ["ticket", "area", "family", "route", "point"]);
    keys = ["document_id", "anchor"];
    rows = all(
      "SELECT document_id,anchor,source_ref,source_version,name,json_extract(detail,'$.source_context') context FROM catalogue_sources WHERE object_id=? AND (document_id,anchor)>(?,?) ORDER BY document_id,anchor LIMIT ?",
      q.id,
      Number(after[0] ?? 0),
      after[1] ?? "",
      limit + 1,
    );
    rows = rows.map((r) => ({
      ...r,
      context: r.context ? JSON.parse(r.context) : null,
    }));
  } else if (action === "changes") {
    keys = ["generation", "previous_id", "current_id", "reason"];
    rows = all(
      "SELECT e.generation,e.previous_id,e.current_id,e.reason FROM catalogue_events e WHERE e.noc=? AND (e.generation,e.previous_id,e.current_id,e.reason)>(?,?,?,?) ORDER BY e.generation,e.previous_id,e.current_id,e.reason LIMIT ?",
      q.noc,
      Number(after[0] ?? 0),
      after[1] ?? "",
      after[2] ?? "",
      after[3] ?? "",
      limit + 1,
    );
  } else if (action === "origins" || action === "destinations") {
    required(q.route_id, ["route"]);
    if (action === "destinations") required(q.origin, ["point"]);
    rows =
      action === "origins"
        ? all(
            `SELECT ${columns} FROM catalogue_route_origins p JOIN catalogue_objects o ON o.id=p.origin_id WHERE p.route_id=? AND p.origin_id>? ORDER BY p.origin_id LIMIT ?`,
            q.route_id,
            after[0] ?? "",
            limit + 1,
          )
        : all(
            `SELECT ${columns} FROM catalogue_route_pairs p JOIN catalogue_objects o ON o.id=p.destination_id WHERE p.route_id=? AND p.origin_id=? AND p.destination_id>? ORDER BY p.destination_id LIMIT ?`,
            q.route_id,
            q.origin,
            after[0] ?? "",
            limit + 1,
          );
  } else if (action === "journey") {
    required(q.route_id, ["route"]);
    required(q.origin, ["point"]);
    required(q.destination, ["point"]);
    if (
      !q.date ||
      !/^\d{4}-\d{2}-\d{2}$/.test(q.date) ||
      !Number.isFinite(Date.parse(q.date)) ||
      new Date(q.date).toISOString().slice(0, 10) !== q.date
    )
      throw new QueryError(400, "date must be a valid YYYY-MM-DD");
    // Exact route + directional pair is an index prefix. Prices stay decimal
    // strings and conditions remain explicit; this is not a best-fare engine.
    keys = ["id", "amount", "currency"];
    rows = all(
      `SELECT ${columns},j.amount,j.currency,o.detail FROM catalogue_journeys j JOIN catalogue_objects o ON o.id=j.ticket_id WHERE j.route_id=? AND j.origin_id=? AND j.destination_id=? AND (j.ticket_id,j.amount,j.currency)>(?,?,?) ORDER BY j.ticket_id,j.amount,j.currency LIMIT ?`,
      q.route_id,
      q.origin,
      q.destination,
      after[0] ?? "",
      after[1] ?? "",
      after[2] ?? "",
      limit + 1,
    );
    rows = rows.map((r) => {
      const detail = JSON.parse(r.detail);
      const terms = detail.conditions ?? {};
      const starts: string[] = terms.FromDate ?? [];
      const ends: string[] = terms.ToDate ?? [];
      const outside =
        starts.some((d) => q.date! < d.slice(0, 10)) ||
        ends.some((d) => q.date! > d.slice(0, 10));
      return {
        ...r,
        detail: undefined,
        date: q.date,
        status: outside ? "outside_published_validity" : "published_candidate",
        conditions_evaluated: false,
        unresolved_count: detail.unresolved_count ?? 0,
      };
    });
  } else throw new QueryError(404, "Unknown catalogue endpoint");
  const more = rows.length > limit;
  if (more) rows.pop();
  const cursor = more
    ? Buffer.from(
        JSON.stringify({
          generation,
          binding,
          after: keys.map((k) => String(rows.at(-1)![k])),
        }),
      ).toString("base64url")
    : undefined;
  const items = rows.map((r) => (r.summary ? compact(r) : r));
  const result =
    single ??
    (action === "journey"
      ? {
          status: items.length
            ? "published_candidates"
            : "no_supported_published_fare",
          candidates: items,
          conditions_evaluated: false,
          unsupported_patterns_possible: true,
        }
      : items);
  return {
    chunks: (function* () {
      yield JSON.stringify(result);
    })(),
    nextCursor: () => cursor,
  };
}
