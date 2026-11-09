-- Geometry is published in the same transaction and generation as its source.
CREATE TABLE agency_geometry (
 agency_id INTEGER PRIMARY KEY REFERENCES agencies(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
 generation INTEGER NOT NULL,
 algorithm_version INTEGER NOT NULL,
 body TEXT NOT NULL CHECK(json_valid(body))
) STRICT;
PRAGMA user_version = 2;
