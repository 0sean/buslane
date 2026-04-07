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
