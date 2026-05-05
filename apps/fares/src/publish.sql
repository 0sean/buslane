BEGIN;

LOCK TABLE
  fare_current.fare_offer,
  fare_current.fare_price,
  fare_current.fare_zone_member,
  fare_current.operator_line,
  fare_current.import_quality_issue
IN ACCESS EXCLUSIVE MODE;

TRUNCATE fare_current.fare_offer RESTART IDENTITY;
TRUNCATE fare_current.fare_price RESTART IDENTITY;
TRUNCATE fare_current.fare_zone_member;
TRUNCATE fare_current.operator_line;
TRUNCATE fare_current.import_quality_issue RESTART IDENTITY;

INSERT INTO fare_current.fare_offer
SELECT * FROM fare_staging.fare_offer;

INSERT INTO fare_current.fare_price
SELECT * FROM fare_staging.fare_price;

INSERT INTO fare_current.fare_zone_member
SELECT * FROM fare_staging.fare_zone_member;

INSERT INTO fare_current.operator_line
SELECT * FROM fare_staging.operator_line;

INSERT INTO fare_current.import_quality_issue (
  batch_id,
  severity,
  issue_code,
  issue_count,
  source_object_id,
  source_document_id,
  details,
  created_at
)
SELECT
  q.batch_id,
  q.severity,
  q.issue_code,
  q.issue_count,
  q.source_object_id,
  d.id,
  q.details,
  q.created_at
FROM fare_staging.import_quality_issue q
LEFT JOIN ingest.xml_document d
  ON d.batch_id = q.batch_id
 AND d.document_uid = q.source_document_uid;

UPDATE ingest.import_batch
SET
  status = 'published',
  completed_at = NOW()
WHERE id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document);

COMMIT;
