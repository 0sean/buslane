-- The writer owns migrations. NeTEx identifiers are scoped to a document,
-- element type, version and occurrence; they are not globally unique.
CREATE TABLE dataset (
 id INTEGER PRIMARY KEY CHECK(id=1), generation INTEGER NOT NULL DEFAULT 0,
 imported_at TEXT, normalisation_version INTEGER NOT NULL DEFAULT 1
) STRICT;
INSERT INTO dataset(id) VALUES(1);
CREATE TABLE archives (
 path TEXT PRIMARY KEY, sha256 TEXT NOT NULL, parent_path TEXT
) WITHOUT ROWID, STRICT;
CREATE INDEX archives_parent ON archives(parent_path);
CREATE TABLE documents (
 id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT NOT NULL UNIQUE,
 archive_path TEXT, sha256 TEXT NOT NULL, xml_deflate BLOB NOT NULL,
 publication_timestamp TEXT, currency TEXT,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX documents_archive ON documents(archive_path);
CREATE TABLE entities (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 identity TEXT NOT NULL, category TEXT NOT NULL, kind TEXT NOT NULL,
 ref TEXT NOT NULL, version TEXT NOT NULL, occurrence INTEGER NOT NULL,
 name TEXT, amount TEXT, currency TEXT, product_ref TEXT, profile_ref TEXT,
 package_ref TEXT, matrix_ref TEXT, price_ref TEXT, group_ref TEXT,
 start_ref TEXT, end_ref TEXT,
 data TEXT NOT NULL CHECK(json_valid(data)), sha256 TEXT NOT NULL,
 UNIQUE(document_id, identity)
) STRICT;
CREATE INDEX entities_category ON entities(category,id);
CREATE INDEX entities_document_category ON entities(document_id,category,id);
CREATE INDEX entities_ref ON entities(document_id,ref,kind);
CREATE INDEX entities_product ON entities(document_id,product_ref,category,id);
CREATE TABLE links (
 document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 kind TEXT NOT NULL, code TEXT NOT NULL, ref TEXT NOT NULL,
 qualifier TEXT NOT NULL DEFAULT '', name TEXT,
 PRIMARY KEY(document_id,kind,code,ref,qualifier)
) WITHOUT ROWID, STRICT;
CREATE INDEX links_lookup ON links(kind,code,document_id);
PRAGMA application_id=1112888146; -- 0x42555352, separate from timetables
PRAGMA user_version=1;
