import type Database from "better-sqlite3";
import { serialize } from "../utils/serialize.ts";

export type Kind =
  | "agencies"
  | "agency"
  | "agencyRoutes"
  | "agencyStops"
  | "route"
  | "routeTrips"
  | "trip"
  | "service"
  | "stops"
  | "stopTimes"
  | "agencyFareContext"
  | "routeFareContext"
  | "stopFareContext";
export interface Query {
  kind: Kind;
  id?: string;
  limit?: number;
  cursor?: string;
}
export class QueryError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
type Row = Record<string, any>;
class StreamedObject {
  constructor(
    public base: Row,
    public field: string,
    public items: () => Iterable<Row>,
  ) {}
}
function* encode(item: Row | StreamedObject): Generator<string> {
  if (item instanceof StreamedObject) {
    const base = JSON.stringify(serialize(item.base));
    yield base.slice(0, -1) + ',"' + item.field + '":[';
    let first = true;
    for (const row of item.items()) {
      if (!first) yield ",";
      first = false;
      yield* encode(row);
    }
    yield "]}";
  } else yield JSON.stringify(serialize(item));
}
const TRIPS = `SELECT t.id _pk,t.route_id _route,t.service_id _service,t.public_id id,r.public_id route_id,s.public_id service_id,t.headsign,t.direction_id,t.block_id,t.wheelchair_accessible,t.vehicle_journey_code,h.public_id shape_id,t.created_at FROM trips t JOIN routes r ON r.id=t.route_id JOIN services s ON s.id=t.service_id LEFT JOIN shapes h ON h.id=t.shape_id`;
const STOP_TIMES = `SELECT st.id,st.trip_id _trip,st.stop_id _stop,t.public_id trip_id,s.public_id stop_id,st.arrival_time,st.departure_time,st.stop_sequence,st.stop_headsign,st.pickup_type,st.drop_off_type,st.shape_dist_traveled,st.timepoint,st.created_at FROM stop_times st JOIN trips t ON t.id=st.trip_id JOIN stops s ON s.id=st.stop_id`;
const ROUTES = `SELECT r.id _pk,r.agency_id _agency,r.public_id id,a.public_id agency_id,r.short_name,r.long_name,r.route_type,r.created_at FROM routes r JOIN agencies a ON a.id=r.agency_id`;

export class Repository {
  private statements = new Map<string, Database.Statement>();
  constructor(public db: Database.Database) {}
  private statement(sql: string) {
    let statement = this.statements.get(sql);
    if (!statement) {
      if (this.statements.size >= 128)
        this.statements.delete(this.statements.keys().next().value!);
      statement = this.db.prepare(sql);
      this.statements.set(sql, statement);
    }
    return statement;
  }
  private get(sql: string, ...params: any[]): Row | undefined {
    return this.statement(sql).get(...params) as Row | undefined;
  }
  private all(sql: string, ...params: any[]): Row[] {
    return this.statement(sql).all(...params) as Row[];
  }
  private required(row: Row | undefined, label: string): Row {
    if (!row) throw new QueryError(404, label + " not found");
    return row;
  }
  private publicRow(row: Row, booleans: string[] = []): Row {
    const result = { ...row };
    if ("public_id" in result) {
      result.id = result.public_id;
      delete result.public_id;
    }
    for (const key of Object.keys(result))
      if (key.startsWith("_")) delete result[key];
    for (const key of booleans)
      if (result[key] !== null) result[key] = Boolean(result[key]);
    for (const key of ["arrival_time", "departure_time"])
      if (result[key] === null) result[key] = "";
    if (typeof result.created_at === "number")
      result.created_at = new Date(result.created_at)
        .toISOString()
        .replace("T", " ")
        .replace("Z", "");
    for (const key of [
      "arrival_time",
      "departure_time",
      "start_time",
      "end_time",
    ])
      if (typeof result[key] === "number") {
        const seconds = result[key];
        result[key] =
          String(Math.floor(seconds / 3600)).padStart(2, "0") +
          ":" +
          String(Math.floor(seconds / 60) % 60).padStart(2, "0") +
          ":" +
          String(seconds % 60).padStart(2, "0");
      }
    for (const key of ["date", "start_date", "end_date"])
      if (typeof result[key] === "number") {
        const date = String(result[key]).padStart(8, "0");
        result[key] =
          date.slice(0, 4) + "-" + date.slice(4, 6) + "-" + date.slice(6, 8);
      }
    return result;
  }
  private agency(id: string) {
    return this.required(
      id.startsWith("noc:")
        ? this.get(
            "SELECT *,id _pk FROM agencies WHERE noc=? ORDER BY public_id LIMIT 1",
            id.slice(4),
          )
        : this.get("SELECT *,id _pk FROM agencies WHERE public_id=?", id),
      "Agency",
    );
  }
  private numeric(id: string) {
    if (!/^-?\d+$/.test(id) || !Number.isSafeInteger(Number(id)))
      throw new QueryError(400, "Invalid numeric ID");
    return Number(id);
  }
  private route(id: string) {
    return this.required(
      this.get(ROUTES + " WHERE r.public_id=?", this.numeric(id)),
      "Route",
    );
  }
  private service(id: string) {
    return this.required(
      this.get(
        "SELECT *,id _pk FROM services WHERE public_id=?",
        this.numeric(id),
      ),
      "Service",
    );
  }
  private stop(id: string) {
    return this.required(
      this.get("SELECT *,id _pk FROM stops WHERE public_id=?", id),
      "Stop",
    );
  }
  private serviceDTO(row: Row): Row {
    return this.publicRow(row, [
      "monday",
      "tuesday",
      "wednesday",
      "thursday",
      "friday",
      "saturday",
      "sunday",
    ]);
  }
  private stopDTO(row: Row): Row {
    return this.publicRow(row, ["wheelchair_boarding"]);
  }
  private tripDTO(row: Row): Row {
    return this.publicRow(row, ["wheelchair_accessible"]);
  }
  private mapRows(sql: string, ids: number[]): Map<number, Row> {
    const result = new Map<number, Row>();
    for (let offset = 0; offset < ids.length; offset += 400) {
      const batch = ids.slice(offset, offset + 400);
      for (const row of this.all(
        sql.replace("$IDS", batch.map(() => "?").join(",")),
        ...batch,
      ))
        result.set(row._pk, row);
    }
    return result;
  }
  private grouped(sql: string, ids: number[], key: string): Map<number, Row[]> {
    const result = new Map<number, Row[]>();
    for (let offset = 0; offset < ids.length; offset += 400) {
      const batch = ids.slice(offset, offset + 400);
      for (const row of this.all(
        sql.replace("$IDS", batch.map(() => "?").join(",")),
        ...batch,
      )) {
        const values = result.get(row[key]) ?? [];
        values.push(row);
        result.set(row[key], values);
      }
    }
    return result;
  }
  private enrichTrips(rows: Row[], withRoute = false): Row[] {
    if (!rows.length) return [];
    const ids = rows.map((r) => r._pk);
    const services = this.mapRows(
      "SELECT *,id _pk FROM services WHERE id IN ($IDS)",
      [...new Set(rows.map((r) => r._service))],
    );
    const frequencies = this.grouped(
      "SELECT f.id,f.trip_id _trip,t.public_id trip_id,f.start_time,f.end_time,f.headway_secs,f.exact_times,f.created_at FROM frequencies f JOIN trips t ON t.id=f.trip_id WHERE f.trip_id IN ($IDS) ORDER BY f.trip_id,f.start_time",
      ids,
      "_trip",
    );
    const times = this.grouped(
      STOP_TIMES +
        " WHERE st.trip_id IN ($IDS) ORDER BY st.trip_id,st.stop_sequence",
      ids,
      "_trip",
    );
    const stops = this.mapRows(
      "SELECT *,id _pk FROM stops WHERE id IN ($IDS)",
      [...new Set([...times.values()].flat().map((r) => r._stop))],
    );
    const routes = withRoute
      ? this.mapRows(ROUTES + " WHERE r.id IN ($IDS)", [
          ...new Set(rows.map((r) => r._route)),
        ])
      : new Map();
    return rows.map((row) => ({
      ...this.tripDTO(row),
      service: this.serviceDTO(services.get(row._service)!),
      frequencies: (frequencies.get(row._pk) ?? []).map((r) =>
        this.publicRow(r),
      ),
      stop_times: (times.get(row._pk) ?? []).map((r) => ({
        ...this.publicRow(r),
        stop: this.stopDTO(stops.get(r._stop)!),
      })),
      ...(withRoute ? { route: this.publicRow(routes.get(row._route)!) } : {}),
    }));
  }
  private enrichTimes(rows: Row[]): Row[] {
    if (!rows.length) return [];
    const trips = this.mapRows(TRIPS + " WHERE t.id IN ($IDS)", [
      ...new Set(rows.map((r) => r._trip)),
    ]);
    const tripRows = [...trips.values()];
    const services = this.mapRows(
      "SELECT *,id _pk FROM services WHERE id IN ($IDS)",
      [...new Set(tripRows.map((r) => r._service))],
    );
    const routes = this.mapRows(ROUTES + " WHERE r.id IN ($IDS)", [
      ...new Set(tripRows.map((r) => r._route)),
    ]);
    const agencies = this.mapRows(
      "SELECT *,id _pk FROM agencies WHERE id IN ($IDS)",
      [...new Set([...routes.values()].map((r) => r._agency))],
    );
    return rows.map((row) => {
      const trip = trips.get(row._trip)!;
      const route = routes.get(trip._route)!;
      return {
        ...this.publicRow(row),
        trip: {
          ...this.tripDTO(trip),
          service: this.serviceDTO(services.get(trip._service)!),
          route: {
            ...this.publicRow(route),
            agency: this.publicRow(agencies.get(route._agency)!),
          },
        },
      };
    });
  }
  private *batches(
    sql: string,
    params: any[],
    key: string,
    after: any,
    limit?: number,
  ): Generator<Row[]> {
    let remaining = limit ?? Infinity;
    while (remaining > 0) {
      const size = Math.min(100, remaining);
      const rows = this.all(
        sql + " AND " + key + ">? ORDER BY " + key + " LIMIT ?",
        ...params,
        after,
        size,
      );
      if (!rows.length) return;
      yield rows;
      after = rows.at(-1)!.id;
      remaining -= rows.length;
      if (rows.length < size) return;
    }
  }
  prepare(
    query: Query,
    generation: number,
  ): { chunks: Generator<string>; nextCursor: () => string | undefined } {
    let after: any = [
      "agencies",
      "stops",
      "agencyStops",
      "routeTrips",
      "agencyRoutes",
    ].includes(query.kind)
      ? ""
      : 0;
    if (query.kind === "agencyRoutes") after = Number.MIN_SAFE_INTEGER;
    if (query.cursor) {
      let cursor: any;
      try {
        cursor = JSON.parse(Buffer.from(query.cursor, "base64url").toString());
      } catch {
        throw new QueryError(400, "Invalid cursor");
      }
      if (
        cursor.v !== 1 ||
        cursor.generation !== generation ||
        cursor.kind !== query.kind ||
        cursor.id !== query.id ||
        typeof cursor.key !== typeof after
      )
        throw new QueryError(
          400,
          "Cursor does not match this dataset and query",
        );
      after = cursor.key;
    }
    let collection = true;
    let items: Iterable<Row | StreamedObject>;
    let pagination: { sql: string; params: any[]; key: string } | undefined;
    const batches = (sql: string, params: any[], key: string) => {
      pagination = { sql, params, key };
      return this.batches(sql, params, key, after, query.limit);
    };
    const self = this;
    function* flattened(
      batches: Iterable<Row[]>,
      transform: (r: Row) => Row | StreamedObject,
    ) {
      for (const batch of batches)
        for (const row of batch) {
          yield transform(row);
        }
    }
    switch (query.kind) {
      case "agencyFareContext": {
        collection = false;
        items = [{ noc: this.agency(query.id!).noc }];
        break;
      }
      case "routeFareContext": {
        collection = false;
        const route = this.route(query.id!);
        const agency = this.required(
          this.get("SELECT noc FROM agencies WHERE id=?", route._agency),
          "Agency",
        );
        if (!route.short_name)
          throw new QueryError(
            422,
            "Route has no public line code for fare matching",
          );
        items = [{ noc: agency.noc, line: route.short_name }];
        break;
      }
      case "stopFareContext": {
        collection = false;
        const stop = this.stop(query.id!);
        items = [{ stop: stop.public_id.replace(/^(?:atco|naptan):/, "") }];
        break;
      }
      case "agencies":
        items = flattened(
          batches(
            "SELECT *,id _pk,public_id id FROM agencies WHERE true",
            [],
            "public_id",
          ),
          (r) => this.publicRow(r),
        );
        break;
      case "stops":
        items = flattened(
          batches(
            "SELECT *,id _pk,public_id id FROM stops WHERE true",
            [],
            "public_id",
          ),
          (r) => this.stopDTO(r),
        );
        break;
      case "agency":
        collection = false;
        items = [this.publicRow(this.agency(query.id!))];
        break;
      case "agencyStops": {
        const agency = this.agency(query.id!);
        items = flattened(
          batches(
            "SELECT s.*,s.id _pk,s.public_id id FROM agency_stops a JOIN stops s ON s.id=a.stop_id WHERE a.agency_id=?",
            [agency._pk],
            "s.public_id",
          ),
          (r) => this.stopDTO(r),
        );
        break;
      }
      case "agencyRoutes": {
        const agency = this.agency(query.id!);
        items = flattened(
          batches(ROUTES + " WHERE r.agency_id=?", [agency._pk], "r.public_id"),
          (r) =>
            new StreamedObject(
              this.publicRow(r),
              "trips",
              () =>
                flattened(
                  this.batches(
                    TRIPS + " WHERE t.route_id=?",
                    [r._pk],
                    "t.public_id",
                    "",
                  ),
                  (t) => this.tripDTO(t),
                ) as Iterable<Row>,
            ),
        );
        break;
      }
      case "route": {
        collection = false;
        const route = this.route(query.id!);
        items = [
          new StreamedObject(
            this.publicRow(route),
            "trips",
            () =>
              flattened(
                this.batches(
                  TRIPS + " WHERE t.route_id=?",
                  [route._pk],
                  "t.public_id",
                  "",
                ),
                (t) => this.tripDTO(t),
              ) as Iterable<Row>,
          ),
        ];
        break;
      }
      case "routeTrips": {
        const route = this.route(query.id!);
        const rows = batches(
          TRIPS + " WHERE t.route_id=?",
          [route._pk],
          "t.public_id",
        );
        items = (function* () {
          for (const batch of rows) {
            for (const trip of self.enrichTrips(batch)) {
              yield trip;
            }
          }
        })();
        break;
      }
      case "trip": {
        collection = false;
        const trip = this.required(
          this.get(TRIPS + " WHERE t.public_id=?", query.id),
          "Trip",
        );
        items = this.enrichTrips([trip], true);
        break;
      }
      case "service": {
        collection = false;
        const service = this.service(query.id!);
        const dates = this.all(
          "SELECT c.id,s.public_id service_id,c.date,c.exception_type,c.created_at FROM calendar_dates c JOIN services s ON s.id=c.service_id WHERE c.service_id=? ORDER BY c.date",
          service._pk,
        );
        items = [
          {
            ...this.serviceDTO(service),
            calendar_dates: dates.map((r) => this.publicRow(r)),
          },
        ];
        break;
      }
      case "stopTimes": {
        const stop = this.stop(query.id!);
        const rows = batches(
          STOP_TIMES + " WHERE st.stop_id=?",
          [stop._pk],
          "st.id",
        );
        items = (function* () {
          for (const batch of rows) {
            for (const time of self.enrichTimes(batch)) {
              yield time;
            }
          }
        })();
        break;
      }
    }
    let nextCursor: string | undefined;
    if (pagination && query.limit !== undefined) {
      const { sql, params, key } = pagination;
      const rows = this.all(
        sql + " AND " + key + ">? ORDER BY " + key + " LIMIT ?",
        ...params,
        after,
        query.limit + 1,
      );
      if (rows.length > query.limit)
        nextCursor = Buffer.from(
          JSON.stringify({
            v: 1,
            generation,
            kind: query.kind,
            id: query.id,
            key: rows[query.limit - 1]!.id,
          }),
        ).toString("base64url");
    }
    if (!collection && (query.limit !== undefined || query.cursor))
      throw new QueryError(400, "Pagination requires a collection endpoint");
    function* chunks(): Generator<string> {
      if (collection) yield "[";
      let first = true;
      for (const item of items) {
        if (!first && collection) yield ",";
        first = false;
        yield* encode(item);
      }
      if (collection) yield "]";
    }
    return { chunks: chunks(), nextCursor: () => nextCursor };
  }
}
