import {
  sqliteTable,
  integer,
  text,
  real,
  index,
  uniqueIndex,
  primaryKey,
  check,
} from "drizzle-orm/sqlite-core";
import { sql } from "drizzle-orm";

const id = () => integer("id").primaryKey({ autoIncrement: true });
const createdAt = () =>
  integer("created_at", { mode: "timestamp_ms" })
    .notNull()
    .default(sql`(CAST(unixepoch('subsec')*1000 AS INTEGER))`);
const boolean = (name: string) => integer(name, { mode: "boolean" });
export const dataset = sqliteTable(
  "dataset",
  {
    id: integer("id").primaryKey(),
    generation: integer("generation").notNull().default(0),
    importedAt: text("imported_at"),
    normalisationVersion: integer("normalisation_version").notNull().default(1),
  },
  (t) => [check("dataset_singleton", sql`${t.id}=1`)],
);
export const sourceFiles = sqliteTable("source_files", {
  name: text("name").primaryKey(),
  sha256: text("sha256").notNull(),
  normalisationVersion: integer("normalisation_version").notNull(),
});
export const agencies = sqliteTable(
  "agencies",
  {
    id: id(),
    publicId: text("public_id").notNull().unique(),
    name: text("name").notNull(),
    url: text("url"),
    timezone: text("timezone"),
    lang: text("lang"),
    phone: text("phone"),
    noc: text("noc").notNull(),
    createdAt: createdAt(),
  },
  (t) => [index("agencies_noc").on(t.noc, t.publicId)],
);
export const stops = sqliteTable(
  "stops",
  {
    id: id(),
    publicId: text("public_id").notNull().unique(),
    code: text("code"),
    name: text("name").notNull(),
    lat: text("lat").notNull(),
    lon: text("lon").notNull(),
    wheelchairBoarding: boolean("wheelchair_boarding").notNull(),
    locationType: integer("location_type").notNull(),
    platformCode: text("platform_code"),
    parentStationId: text("parent_station_id").references(
      (): any => stops.publicId,
    ),
    createdAt: createdAt(),
  },
  (t) => [index("stops_parent").on(t.parentStationId)],
);
export const routes = sqliteTable(
  "routes",
  {
    id: id(),
    publicId: integer("public_id").notNull().unique(),
    agencyId: integer("agency_id")
      .notNull()
      .references(() => agencies.id),
    shortName: text("short_name"),
    longName: text("long_name"),
    routeType: integer("route_type").notNull(),
    createdAt: createdAt(),
  },
  (t) => [index("routes_agency").on(t.agencyId, t.publicId)],
);
export const services = sqliteTable("services", {
  id: id(),
  publicId: integer("public_id").notNull().unique(),
  monday: boolean("monday"),
  tuesday: boolean("tuesday"),
  wednesday: boolean("wednesday"),
  thursday: boolean("thursday"),
  friday: boolean("friday"),
  saturday: boolean("saturday"),
  sunday: boolean("sunday"),
  startDate: integer("start_date"),
  endDate: integer("end_date"),
  createdAt: createdAt(),
});
export const shapes = sqliteTable("shapes", {
  id: id(),
  publicId: text("public_id").notNull().unique(),
});
export const shapePoints = sqliteTable(
  "shape_points",
  {
    id: id(),
    shapeId: integer("shape_id")
      .notNull()
      .references(() => shapes.id),
    ptSequence: integer("pt_sequence").notNull(),
    ptLat: real("pt_lat").notNull(),
    ptLon: real("pt_lon").notNull(),
    distTraveled: real("dist_traveled"),
  },
  (t) => [uniqueIndex("shape_points_identity").on(t.shapeId, t.ptSequence)],
);
export const trips = sqliteTable(
  "trips",
  {
    id: id(),
    publicId: text("public_id").notNull().unique(),
    routeId: integer("route_id")
      .notNull()
      .references(() => routes.id),
    serviceId: integer("service_id")
      .notNull()
      .references(() => services.id),
    headsign: text("headsign"),
    directionId: integer("direction_id").notNull(),
    blockId: text("block_id").notNull(),
    wheelchairAccessible: boolean("wheelchair_accessible").notNull(),
    vehicleJourneyCode: text("vehicle_journey_code").notNull(),
    shapeId: integer("shape_id").references(() => shapes.id),
    createdAt: createdAt(),
  },
  (t) => [
    index("trips_route").on(t.routeId, t.publicId),
    index("trips_service").on(t.serviceId),
    index("trips_shape").on(t.shapeId),
  ],
);
export const stopTimes = sqliteTable(
  "stop_times",
  {
    id: id(),
    tripId: integer("trip_id")
      .notNull()
      .references(() => trips.id),
    stopId: integer("stop_id")
      .notNull()
      .references(() => stops.id),
    arrivalTime: integer("arrival_time"),
    departureTime: integer("departure_time"),
    stopSequence: integer("stop_sequence").notNull(),
    stopHeadsign: text("stop_headsign"),
    pickupType: integer("pickup_type").notNull(),
    dropOffType: integer("drop_off_type").notNull(),
    shapeDistTraveled: text("shape_dist_traveled"),
    timepoint: integer("timepoint").notNull(),
    createdAt: createdAt(),
  },
  (t) => [
    uniqueIndex("stop_times_identity").on(t.tripId, t.stopSequence),
    index("stop_times_stop").on(t.stopId, t.id),
  ],
);
export const calendarDates = sqliteTable(
  "calendar_dates",
  {
    id: id(),
    serviceId: integer("service_id")
      .notNull()
      .references(() => services.id),
    date: integer("date").notNull(),
    exceptionType: integer("exception_type").notNull(),
    createdAt: createdAt(),
  },
  (t) => [uniqueIndex("calendar_dates_identity").on(t.serviceId, t.date)],
);
export const frequencies = sqliteTable(
  "frequencies",
  {
    id: id(),
    tripId: integer("trip_id")
      .notNull()
      .references(() => trips.id),
    startTime: integer("start_time").notNull(),
    endTime: integer("end_time").notNull(),
    headwaySecs: integer("headway_secs").notNull(),
    exactTimes: integer("exact_times").notNull(),
    createdAt: createdAt(),
  },
  (t) => [uniqueIndex("frequencies_identity").on(t.tripId, t.startTime)],
);
export const agencyStops = sqliteTable(
  "agency_stops",
  {
    agencyId: integer("agency_id")
      .notNull()
      .references(() => agencies.id),
    stopId: integer("stop_id")
      .notNull()
      .references(() => stops.id),
  },
  (t) => [
    primaryKey({ columns: [t.agencyId, t.stopId] }),
    index("agency_stops_stop").on(t.stopId),
  ],
);
