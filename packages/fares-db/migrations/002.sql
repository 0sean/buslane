-- Derived passenger catalogue. Raw entities and original XML remain authoritative.
ALTER TABLE dataset ADD COLUMN catalogue_version INTEGER NOT NULL DEFAULT 0;
CREATE TABLE catalogue_objects (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL, noc TEXT NOT NULL,
 semantic_key TEXT NOT NULL, name TEXT NOT NULL, search TEXT NOT NULL,
 summary TEXT NOT NULL CHECK(json_valid(summary)), detail TEXT NOT NULL CHECK(json_valid(detail)),
 family_id TEXT,
 travel INTEGER NOT NULL DEFAULT 0,
 revision INTEGER NOT NULL DEFAULT 1, active INTEGER NOT NULL DEFAULT 1,
 ticket_count INTEGER NOT NULL DEFAULT 0,
 source_count INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX catalogue_browse ON catalogue_objects(noc,kind,active,id);
CREATE INDEX catalogue_travel_browse ON catalogue_objects(noc,kind,active,travel,id);
CREATE INDEX catalogue_travel_search ON catalogue_objects(noc,kind,active,travel,search,id);
CREATE INDEX catalogue_semantics ON catalogue_objects(noc,kind,semantic_key,active);
-- Prefix search uses an indexed range, never an operator-wide LIKE scan.
CREATE INDEX catalogue_search ON catalogue_objects(noc,kind,active,search,id);
CREATE TABLE catalogue_sources (
 document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 anchor TEXT NOT NULL, noc TEXT NOT NULL, kind TEXT NOT NULL, semantic_key TEXT NOT NULL,
 object_id TEXT REFERENCES catalogue_objects(id), name TEXT NOT NULL,
 summary TEXT NOT NULL, detail TEXT NOT NULL, source_ref TEXT NOT NULL, source_version TEXT NOT NULL,
 PRIMARY KEY(document_id,anchor)
) WITHOUT ROWID, STRICT;
CREATE INDEX catalogue_sources_pending ON catalogue_sources(object_id) WHERE object_id IS NULL;
CREATE INDEX catalogue_sources_key ON catalogue_sources(noc,kind,semantic_key);
CREATE INDEX catalogue_sources_object ON catalogue_sources(object_id,document_id,anchor);
CREATE TABLE catalogue_source_edges (
 document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 source_anchor TEXT NOT NULL, target_anchor TEXT NOT NULL, relation TEXT NOT NULL,
 PRIMARY KEY(document_id,source_anchor,target_anchor,relation)
) WITHOUT ROWID, STRICT;
CREATE TABLE catalogue_edges (
 source_id TEXT NOT NULL REFERENCES catalogue_objects(id), target_id TEXT NOT NULL REFERENCES catalogue_objects(id),
 relation TEXT NOT NULL, search TEXT NOT NULL, travel INTEGER NOT NULL, PRIMARY KEY(source_id,relation,target_id)
) WITHOUT ROWID, STRICT;
CREATE INDEX catalogue_edges_target ON catalogue_edges(target_id,relation,source_id);
CREATE INDEX catalogue_edges_browse ON catalogue_edges(target_id,relation,travel,source_id);
CREATE INDEX catalogue_edges_search ON catalogue_edges(target_id,relation,travel,search,source_id);
CREATE TABLE catalogue_source_stops (
 document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 anchor TEXT NOT NULL, code TEXT NOT NULL, name TEXT,
 PRIMARY KEY(document_id,anchor,code)
) WITHOUT ROWID, STRICT;
CREATE TABLE catalogue_stops (
 object_id TEXT NOT NULL REFERENCES catalogue_objects(id), code TEXT NOT NULL, name TEXT,
 PRIMARY KEY(object_id,code)
) WITHOUT ROWID, STRICT;
CREATE TABLE catalogue_source_journeys (
 document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 anchor TEXT NOT NULL, route_anchor TEXT NOT NULL, origin_anchor TEXT NOT NULL, destination_anchor TEXT NOT NULL,
 amount TEXT NOT NULL, currency TEXT NOT NULL, matrix_ref TEXT NOT NULL,
 PRIMARY KEY(document_id,anchor,route_anchor,origin_anchor,destination_anchor,matrix_ref,amount,currency)
) WITHOUT ROWID, STRICT;
CREATE TABLE catalogue_journeys (
 route_id TEXT NOT NULL REFERENCES catalogue_objects(id), origin_id TEXT NOT NULL REFERENCES catalogue_objects(id),
 destination_id TEXT NOT NULL REFERENCES catalogue_objects(id), ticket_id TEXT NOT NULL REFERENCES catalogue_objects(id),
 amount TEXT NOT NULL, currency TEXT NOT NULL,
 PRIMARY KEY(route_id,origin_id,destination_id,ticket_id,amount,currency)
) WITHOUT ROWID, STRICT;
CREATE INDEX catalogue_journeys_ticket ON catalogue_journeys(ticket_id);
CREATE INDEX catalogue_journeys_destination ON catalogue_journeys(route_id,destination_id,origin_id);
CREATE TABLE catalogue_events (
 generation INTEGER NOT NULL, previous_id TEXT NOT NULL, current_id TEXT NOT NULL,
 reason TEXT NOT NULL, noc TEXT NOT NULL, PRIMARY KEY(noc,generation,previous_id,current_id,reason)
) WITHOUT ROWID, STRICT;
CREATE INDEX catalogue_events_generation ON catalogue_events(generation);
PRAGMA user_version=2;
CREATE TABLE catalogue_route_origins (
 route_id TEXT NOT NULL, origin_id TEXT NOT NULL,
 PRIMARY KEY(route_id,origin_id)
) WITHOUT ROWID, STRICT;
CREATE TABLE catalogue_route_pairs (
 route_id TEXT NOT NULL, origin_id TEXT NOT NULL, destination_id TEXT NOT NULL,
 PRIMARY KEY(route_id,origin_id,destination_id)
) WITHOUT ROWID, STRICT;
-- Navigation families within an area are distinct from equivalent variants.
CREATE TABLE catalogue_area_families (
 area_id TEXT NOT NULL, family_id TEXT NOT NULL, variant_count INTEGER NOT NULL,
 search TEXT NOT NULL, travel INTEGER NOT NULL,
 PRIMARY KEY(area_id,family_id)
) WITHOUT ROWID, STRICT;
CREATE INDEX catalogue_area_families_browse ON catalogue_area_families(area_id,travel,family_id);
CREATE INDEX catalogue_area_families_search ON catalogue_area_families(area_id,travel,search,family_id);
CREATE TABLE catalogue_area_ticket_variants (
 area_id TEXT NOT NULL, family_id TEXT NOT NULL, ticket_id TEXT NOT NULL, search TEXT NOT NULL, travel INTEGER NOT NULL,
 PRIMARY KEY(area_id,family_id,ticket_id)
) WITHOUT ROWID, STRICT;
CREATE INDEX catalogue_area_variants_browse ON catalogue_area_ticket_variants(area_id,family_id,travel,ticket_id);
CREATE INDEX catalogue_area_variants_search ON catalogue_area_ticket_variants(area_id,family_id,travel,search,ticket_id);
