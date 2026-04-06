CREATE TABLE "agencies"(
	"id" VARCHAR NOT NULL PRIMARY KEY,
	"name" VARCHAR NOT NULL,
	"url" VARCHAR,
	"timezone" VARCHAR,
	"lang" VARCHAR,
	"phone" VARCHAR,
	"noc" VARCHAR NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE "shapes"(
	"id" VARCHAR NOT NULL,
	"pt_lat" VARCHAR NOT NULL,
	"pt_lon" VARCHAR NOT NULL,
	"pt_sequence" INT4 NOT NULL,
	"dist_traveled" NUMERIC,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	PRIMARY KEY ("id", "pt_sequence")
);

CREATE TABLE "services"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"monday" BOOL,
	"tuesday" BOOL,
	"wednesday" BOOL,
	"thursday" BOOL,
	"friday" BOOL,
	"saturday" BOOL,
	"sunday" BOOL,
	"start_date" DATE,
	"end_date" DATE,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE "stops"(
	"id" VARCHAR NOT NULL PRIMARY KEY,
	"code" VARCHAR,
	"name" VARCHAR NOT NULL,
	"lat" NUMERIC NOT NULL,
	"lon" NUMERIC NOT NULL,
	"wheelchair_boarding" BOOL,
	"location_type" INT4 NOT NULL,
	"platform_code" VARCHAR,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	"parent_station_id" VARCHAR
);

CREATE TABLE "routes"(
	"id" INT4 NOT NULL PRIMARY KEY,
	"agency_id" VARCHAR NOT NULL,
	"short_name" VARCHAR,
	"long_name" VARCHAR,
	"route_type" INT4 NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("agency_id") REFERENCES "agencies"("id")
);

CREATE TABLE "trips"(
	"id" VARCHAR NOT NULL PRIMARY KEY,
	"route_id" INT4 NOT NULL,
	"service_id" INT8 NOT NULL,
	"headsign" VARCHAR,
	"direction_id" INT4 NOT NULL,
	"block_id" VARCHAR NOT NULL,
	"wheelchair_accessible" BOOL,
	"vehicle_journey_code" VARCHAR NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	"shape_id" VARCHAR,
	FOREIGN KEY ("route_id") REFERENCES "routes"("id"),
	FOREIGN KEY ("service_id") REFERENCES "services"("id")
);

CREATE TABLE "calendar_dates"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"service_id" INT8 NOT NULL,
	"date" DATE NOT NULL,
	"exception_type" INT4 NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("service_id") REFERENCES "services"("id")
);

CREATE TABLE "stop_times"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"stop_id" VARCHAR NOT NULL,
	"trip_id" VARCHAR NOT NULL,
	"arrival_time" VARCHAR NOT NULL,
	"departure_time" VARCHAR NOT NULL,
	"stop_sequence" INT4,
	"stop_headsign" VARCHAR,
	"pickup_type" INT4,
	"drop_off_type" INT4,
	"shape_dist_traveled" NUMERIC,
	"timepoint" INT4,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("stop_id") REFERENCES "stops"("id"),
	FOREIGN KEY ("trip_id") REFERENCES "trips"("id")
);

CREATE TABLE "frequencies"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"trip_id" VARCHAR NOT NULL,
	"start_time" VARCHAR NOT NULL,
	"end_time" VARCHAR NOT NULL,
	"headway_secs" INT4 NOT NULL,
	"exact_times" INT4,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("trip_id") REFERENCES "trips"("id")
);

-- Raw tables: all columns as text, no constraints, no timestamps
CREATE TABLE "raw_agencies"(
	"id" TEXT,
	"name" TEXT,
	"url" TEXT,
	"timezone" TEXT,
	"lang" TEXT,
	"phone" TEXT,
	"noc" TEXT
);

CREATE TABLE "raw_shapes"(
	"id" TEXT,
	"pt_lat" TEXT,
	"pt_lon" TEXT,
	"pt_sequence" TEXT,
	"dist_traveled" TEXT
);

CREATE TABLE "raw_services"(
	"id" TEXT,
	"monday" TEXT,
	"tuesday" TEXT,
	"wednesday" TEXT,
	"thursday" TEXT,
	"friday" TEXT,
	"saturday" TEXT,
	"sunday" TEXT,
	"start_date" TEXT,
	"end_date" TEXT
);

CREATE TABLE "raw_stops"(
	"id" TEXT,
	"code" TEXT,
	"name" TEXT,
	"lat" TEXT,
	"lon" TEXT,
	"wheelchair_boarding" TEXT,
	"location_type" TEXT,
	"platform_code" TEXT,
	"parent_station_id" TEXT
);

CREATE TABLE "raw_routes"(
	"id" TEXT,
	"agency_id" TEXT,
	"short_name" TEXT,
	"long_name" TEXT,
	"route_type" TEXT
);

CREATE TABLE "raw_trips"(
	"id" TEXT,
	"route_id" TEXT,
	"service_id" TEXT,
	"headsign" TEXT,
	"direction_id" TEXT,
	"block_id" TEXT,
	"wheelchair_accessible" TEXT,
	"vehicle_journey_code" TEXT,
	"shape_id" TEXT
);

CREATE TABLE "raw_calendar_dates"(
	"service_id" TEXT,
	"date" TEXT,
	"exception_type" TEXT
);

CREATE TABLE "raw_stop_times"(
	"stop_id" TEXT,
	"trip_id" TEXT,
	"arrival_time" TEXT,
	"departure_time" TEXT,
	"stop_sequence" TEXT,
	"stop_headsign" TEXT,
	"pickup_type" TEXT,
	"drop_off_type" TEXT,
	"shape_dist_traveled" TEXT,
	"timepoint" TEXT
);

CREATE TABLE "raw_frequencies"(
	"trip_id" TEXT,
	"start_time" TEXT,
	"end_time" TEXT,
	"headway_secs" TEXT,
	"exact_times" TEXT
);

-- Staging tables: same types and constraints as main tables, created_at only
CREATE TABLE "staging_agencies"(
	"id" VARCHAR NOT NULL PRIMARY KEY,
	"name" VARCHAR NOT NULL,
	"url" VARCHAR,
	"timezone" VARCHAR,
	"lang" VARCHAR,
	"phone" VARCHAR,
	"noc" VARCHAR NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE "staging_shapes"(
	"id" VARCHAR NOT NULL,
	"pt_lat" VARCHAR NOT NULL,
	"pt_lon" VARCHAR NOT NULL,
	"pt_sequence" INT4 NOT NULL,
	"dist_traveled" NUMERIC,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	PRIMARY KEY ("id", "pt_sequence")
);

CREATE TABLE "staging_services"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"monday" BOOL,
	"tuesday" BOOL,
	"wednesday" BOOL,
	"thursday" BOOL,
	"friday" BOOL,
	"saturday" BOOL,
	"sunday" BOOL,
	"start_date" DATE,
	"end_date" DATE,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE "staging_stops"(
	"id" VARCHAR NOT NULL PRIMARY KEY,
	"code" VARCHAR,
	"name" VARCHAR NOT NULL,
	"lat" NUMERIC NOT NULL,
	"lon" NUMERIC NOT NULL,
	"wheelchair_boarding" BOOL,
	"location_type" INT4 NOT NULL,
	"platform_code" VARCHAR,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	"parent_station_id" VARCHAR
);

CREATE TABLE "staging_routes"(
	"id" INT4 NOT NULL PRIMARY KEY,
	"agency_id" VARCHAR NOT NULL,
	"short_name" VARCHAR,
	"long_name" VARCHAR,
	"route_type" INT4 NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("agency_id") REFERENCES "staging_agencies"("id")
);

CREATE TABLE "staging_trips"(
	"id" VARCHAR NOT NULL PRIMARY KEY,
	"route_id" INT4 NOT NULL,
	"service_id" INT8 NOT NULL,
	"headsign" VARCHAR,
	"direction_id" INT4 NOT NULL,
	"block_id" VARCHAR NOT NULL,
	"wheelchair_accessible" BOOL,
	"vehicle_journey_code" VARCHAR NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	"shape_id" VARCHAR,
	FOREIGN KEY ("route_id") REFERENCES "staging_routes"("id"),
	FOREIGN KEY ("service_id") REFERENCES "staging_services"("id")
);

CREATE TABLE "staging_calendar_dates"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"service_id" INT8 NOT NULL,
	"date" DATE NOT NULL,
	"exception_type" INT4 NOT NULL,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("service_id") REFERENCES "staging_services"("id")
);

CREATE TABLE "staging_stop_times"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"stop_id" VARCHAR NOT NULL,
	"trip_id" VARCHAR NOT NULL,
	"arrival_time" VARCHAR NOT NULL,
	"departure_time" VARCHAR NOT NULL,
	"stop_sequence" INT4,
	"stop_headsign" VARCHAR,
	"pickup_type" INT4,
	"drop_off_type" INT4,
	"shape_dist_traveled" NUMERIC,
	"timepoint" INT4,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("stop_id") REFERENCES "staging_stops"("id"),
	FOREIGN KEY ("trip_id") REFERENCES "staging_trips"("id")
);

CREATE TABLE "staging_frequencies"(
	"id" INT8 NOT NULL PRIMARY KEY,
	"trip_id" VARCHAR NOT NULL,
	"start_time" VARCHAR NOT NULL,
	"end_time" VARCHAR NOT NULL,
	"headway_secs" INT4 NOT NULL,
	"exact_times" INT4,
	"created_at" TIMESTAMP NOT NULL DEFAULT NOW(),
	FOREIGN KEY ("trip_id") REFERENCES "staging_trips"("id")
);
