import { pgTable, varchar, numeric, boolean, integer, timestamp, foreignKey, bigint, date, text, primaryKey } from "drizzle-orm/pg-core"

export const stops = pgTable("stops", {
	id: varchar().primaryKey().notNull(),
	code: varchar(),
	name: varchar().notNull(),
	lat: numeric().notNull(),
	lon: numeric().notNull(),
	wheelchairBoarding: boolean("wheelchair_boarding"),
	locationType: integer("location_type").notNull(),
	platformCode: varchar("platform_code"),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
	parentStationId: varchar("parent_station_id"),
});

export const stopTimes = pgTable("stop_times", {
	// You can use { mode: "bigint" } if numbers are exceeding js number limitations
	id: bigint({ mode: "number" }).primaryKey().notNull(),
	stopId: varchar("stop_id").notNull(),
	tripId: varchar("trip_id").notNull(),
	arrivalTime: varchar("arrival_time").notNull(),
	departureTime: varchar("departure_time").notNull(),
	stopSequence: integer("stop_sequence"),
	stopHeadsign: varchar("stop_headsign"),
	pickupType: integer("pickup_type"),
	dropOffType: integer("drop_off_type"),
	shapeDistTraveled: numeric("shape_dist_traveled"),
	timepoint: integer(),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
}, (table) => [
	foreignKey({
			columns: [table.stopId],
			foreignColumns: [stops.id],
			name: "stop_times_stop_id_fkey"
		}),
	foreignKey({
			columns: [table.tripId],
			foreignColumns: [trips.id],
			name: "stop_times_trip_id_fkey"
		}),
]);

export const agencies = pgTable("agencies", {
	id: varchar().primaryKey().notNull(),
	name: varchar().notNull(),
	url: varchar(),
	timezone: varchar(),
	lang: varchar(),
	phone: varchar(),
	noc: varchar().notNull(),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
});

export const routes = pgTable("routes", {
	id: integer().primaryKey().notNull(),
	agencyId: varchar("agency_id").notNull(),
	shortName: varchar("short_name"),
	longName: varchar("long_name"),
	routeType: integer("route_type").notNull(),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
}, (table) => [
	foreignKey({
			columns: [table.agencyId],
			foreignColumns: [agencies.id],
			name: "routes_agency_id_fkey"
		}),
]);

export const trips = pgTable("trips", {
	id: varchar().primaryKey().notNull(),
	routeId: integer("route_id").notNull(),
	// You can use { mode: "bigint" } if numbers are exceeding js number limitations
	serviceId: bigint("service_id", { mode: "number" }).notNull(),
	headsign: varchar(),
	directionId: integer("direction_id").notNull(),
	blockId: varchar("block_id").notNull(),
	wheelchairAccessible: boolean("wheelchair_accessible"),
	vehicleJourneyCode: varchar("vehicle_journey_code").notNull(),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
	shapeId: varchar("shape_id"),
}, (table) => [
	foreignKey({
			columns: [table.routeId],
			foreignColumns: [routes.id],
			name: "trips_route_id_fkey"
		}),
	foreignKey({
			columns: [table.serviceId],
			foreignColumns: [services.id],
			name: "trips_service_id_fkey"
		}),
]);

export const services = pgTable("services", {
	// You can use { mode: "bigint" } if numbers are exceeding js number limitations
	id: bigint({ mode: "number" }).primaryKey().notNull(),
	monday: boolean(),
	tuesday: boolean(),
	wednesday: boolean(),
	thursday: boolean(),
	friday: boolean(),
	saturday: boolean(),
	sunday: boolean(),
	startDate: date("start_date"),
	endDate: date("end_date"),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
});

export const calendarDates = pgTable("calendar_dates", {
	// You can use { mode: "bigint" } if numbers are exceeding js number limitations
	id: bigint({ mode: "number" }).primaryKey().notNull(),
	// You can use { mode: "bigint" } if numbers are exceeding js number limitations
	serviceId: bigint("service_id", { mode: "number" }).notNull(),
	date: date().notNull(),
	exceptionType: integer("exception_type").notNull(),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
}, (table) => [
	foreignKey({
			columns: [table.serviceId],
			foreignColumns: [services.id],
			name: "calendar_dates_service_id_fkey"
		}),
]);

export const frequencies = pgTable("frequencies", {
	// You can use { mode: "bigint" } if numbers are exceeding js number limitations
	id: bigint({ mode: "number" }).primaryKey().notNull(),
	tripId: varchar("trip_id").notNull(),
	startTime: varchar("start_time").notNull(),
	endTime: varchar("end_time").notNull(),
	headwaySecs: integer("headway_secs").notNull(),
	exactTimes: integer("exact_times"),
	createdAt: timestamp("created_at", { mode: 'string' }).defaultNow().notNull(),
}, (table) => [
	foreignKey({
			columns: [table.tripId],
			foreignColumns: [trips.id],
			name: "frequencies_trip_id_fkey"
		}),
]);