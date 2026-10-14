-- Authoritative timetable schema. Only the importer applies migrations.
CREATE TABLE dataset (
 id INTEGER PRIMARY KEY CHECK(id = 1), generation INTEGER NOT NULL DEFAULT 0,
 imported_at TEXT, normalisation_version INTEGER NOT NULL DEFAULT 1
) STRICT;
INSERT INTO dataset(id) VALUES(1);
CREATE TABLE source_files(name TEXT PRIMARY KEY, sha256 TEXT NOT NULL, normalisation_version INTEGER NOT NULL) WITHOUT ROWID, STRICT;
CREATE TABLE agencies (
 id INTEGER PRIMARY KEY AUTOINCREMENT, public_id TEXT NOT NULL UNIQUE,
 name TEXT NOT NULL, url TEXT, timezone TEXT, lang TEXT, phone TEXT, noc TEXT NOT NULL,
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER))
) STRICT;
CREATE INDEX agencies_noc ON agencies(noc, public_id);
CREATE TABLE stops (
 id INTEGER PRIMARY KEY AUTOINCREMENT, public_id TEXT NOT NULL UNIQUE,
 code TEXT, name TEXT NOT NULL, lat TEXT NOT NULL, lon TEXT NOT NULL,
 wheelchair_boarding INTEGER NOT NULL CHECK(wheelchair_boarding IN (0,1)),
 location_type INTEGER NOT NULL CHECK(location_type BETWEEN 0 AND 4), platform_code TEXT,
 parent_station_id TEXT REFERENCES stops(public_id) DEFERRABLE INITIALLY DEFERRED,
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER))
) STRICT;
CREATE INDEX stops_parent ON stops(parent_station_id);
CREATE TABLE routes (
 id INTEGER PRIMARY KEY AUTOINCREMENT, public_id INTEGER NOT NULL UNIQUE,
 agency_id INTEGER NOT NULL REFERENCES agencies(id) DEFERRABLE INITIALLY DEFERRED,
 short_name TEXT, long_name TEXT, route_type INTEGER NOT NULL CHECK(route_type >= 0),
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER))
) STRICT;
CREATE INDEX routes_agency ON routes(agency_id, public_id);
CREATE TABLE services (
 id INTEGER PRIMARY KEY AUTOINCREMENT, public_id INTEGER NOT NULL UNIQUE,
 monday INTEGER CHECK(monday IN (0,1)), tuesday INTEGER CHECK(tuesday IN (0,1)),
 wednesday INTEGER CHECK(wednesday IN (0,1)), thursday INTEGER CHECK(thursday IN (0,1)),
 friday INTEGER CHECK(friday IN (0,1)), saturday INTEGER CHECK(saturday IN (0,1)),
 sunday INTEGER CHECK(sunday IN (0,1)), start_date INTEGER, end_date INTEGER,
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER))
) STRICT;
CREATE TABLE shapes(id INTEGER PRIMARY KEY AUTOINCREMENT, public_id TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE shape_points (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 shape_id INTEGER NOT NULL REFERENCES shapes(id) DEFERRABLE INITIALLY DEFERRED,
 pt_sequence INTEGER NOT NULL CHECK(pt_sequence >= 0), pt_lat REAL NOT NULL, pt_lon REAL NOT NULL,
 dist_traveled REAL, UNIQUE(shape_id, pt_sequence)
) STRICT;
CREATE TABLE trips (
 id INTEGER PRIMARY KEY AUTOINCREMENT, public_id TEXT NOT NULL UNIQUE,
 route_id INTEGER NOT NULL REFERENCES routes(id) DEFERRABLE INITIALLY DEFERRED,
 service_id INTEGER NOT NULL REFERENCES services(id) DEFERRABLE INITIALLY DEFERRED,
 headsign TEXT, direction_id INTEGER NOT NULL CHECK(direction_id IN (0,1)), block_id TEXT NOT NULL,
 wheelchair_accessible INTEGER NOT NULL CHECK(wheelchair_accessible IN (0,1)),
 vehicle_journey_code TEXT NOT NULL,
 shape_id INTEGER REFERENCES shapes(id) DEFERRABLE INITIALLY DEFERRED,
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER))
) STRICT;
CREATE INDEX trips_route ON trips(route_id, public_id);
CREATE INDEX trips_service ON trips(service_id);
CREATE INDEX trips_shape ON trips(shape_id);
CREATE TABLE stop_times (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 trip_id INTEGER NOT NULL REFERENCES trips(id) DEFERRABLE INITIALLY DEFERRED,
 stop_id INTEGER NOT NULL REFERENCES stops(id) DEFERRABLE INITIALLY DEFERRED,
 arrival_time INTEGER CHECK(arrival_time >= 0), departure_time INTEGER CHECK(departure_time >= 0), stop_sequence INTEGER NOT NULL CHECK(stop_sequence >= 0),
 stop_headsign TEXT, pickup_type INTEGER NOT NULL CHECK(pickup_type BETWEEN 0 AND 3),
 drop_off_type INTEGER NOT NULL CHECK(drop_off_type BETWEEN 0 AND 3), shape_dist_traveled TEXT,
 timepoint INTEGER NOT NULL CHECK(timepoint IN (0,1)),
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER)),
 UNIQUE(trip_id, stop_sequence),
 CHECK(timepoint=0 OR (arrival_time IS NOT NULL AND departure_time IS NOT NULL))
) STRICT;
CREATE INDEX stop_times_stop ON stop_times(stop_id, id);
CREATE TABLE calendar_dates (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 service_id INTEGER NOT NULL REFERENCES services(id) DEFERRABLE INITIALLY DEFERRED,
 date INTEGER NOT NULL, exception_type INTEGER NOT NULL CHECK(exception_type IN (1,2)),
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER)), UNIQUE(service_id, date)
) STRICT;
CREATE TABLE frequencies (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 trip_id INTEGER NOT NULL REFERENCES trips(id) DEFERRABLE INITIALLY DEFERRED,
 start_time INTEGER NOT NULL CHECK(start_time >= 0), end_time INTEGER NOT NULL CHECK(end_time > start_time), headway_secs INTEGER NOT NULL CHECK(headway_secs > 0),
 exact_times INTEGER NOT NULL CHECK(exact_times IN (0,1)),
 created_at INTEGER NOT NULL DEFAULT(CAST(unixepoch('subsec')*1000 AS INTEGER)), UNIQUE(trip_id, start_time)
) STRICT;
CREATE TABLE agency_stops (
 agency_id INTEGER NOT NULL REFERENCES agencies(id) DEFERRABLE INITIALLY DEFERRED,
 stop_id INTEGER NOT NULL REFERENCES stops(id) DEFERRABLE INITIALLY DEFERRED,
 PRIMARY KEY(agency_id, stop_id)
) WITHOUT ROWID, STRICT;
CREATE INDEX agency_stops_stop ON agency_stops(stop_id);
PRAGMA user_version = 1;
PRAGMA application_id = 0x4255534C;
