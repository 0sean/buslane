-- Cached operator maps are published with their fares source generation.
CREATE TABLE operator_geometry (
 noc TEXT PRIMARY KEY, body TEXT NOT NULL CHECK(json_valid(body))
) WITHOUT ROWID, STRICT;
CREATE TABLE geometry_state (
 id INTEGER PRIMARY KEY CHECK(id=1), algorithm_version INTEGER NOT NULL,
 fares_generation INTEGER NOT NULL, timetable_generation INTEGER NOT NULL
) STRICT;
PRAGMA user_version=3;
