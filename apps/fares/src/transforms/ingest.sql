WITH payload_rows AS (
  INSERT INTO ingest.source_payload (
    batch_id,
    payload_uid,
    operator_directory,
    path,
    file_name,
    file_type,
    size_bytes,
    sha256,
    modified_at,
    read_status,
    read_error
  )
  SELECT
    batch_id,
    payload_uid,
    operator_directory,
    path,
    file_name,
    file_type,
    size_bytes,
    CASE WHEN sha256_hex IS NULL OR sha256_hex = '' THEN NULL ELSE decode(sha256_hex, 'hex') END,
    NULLIF(modified_at, '')::timestamptz,
    read_status,
    read_error
  FROM fare_staging.load_source_payload
  ON CONFLICT (batch_id, payload_uid) DO UPDATE SET
    read_status = EXCLUDED.read_status,
    read_error = EXCLUDED.read_error
  RETURNING id, batch_id, payload_uid
),
document_rows AS (
  INSERT INTO ingest.xml_document (
    batch_id,
    document_uid,
    payload_uid,
    payload_id,
    zip_member_name,
    operator_directory,
    document_path,
    size_bytes,
    sha256,
    root_name,
    netex_namespace,
    schema_version,
    store_raw_xml,
    parse_status,
    parse_error
  )
  SELECT
    d.batch_id,
    d.document_uid,
    d.payload_uid,
    p.id,
    d.zip_member_name,
    d.operator_directory,
    d.document_path,
    d.size_bytes,
    CASE WHEN d.sha256_hex IS NULL OR d.sha256_hex = '' THEN NULL ELSE decode(d.sha256_hex, 'hex') END,
    d.root_name,
    d.netex_namespace,
    d.schema_version,
    d.store_raw_xml,
    d.parse_status,
    d.parse_error
  FROM fare_staging.load_xml_document d
  JOIN payload_rows p
    ON p.batch_id = d.batch_id
   AND p.payload_uid = d.payload_uid
  ON CONFLICT (batch_id, document_uid) DO UPDATE SET
    payload_id = EXCLUDED.payload_id,
    sha256 = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.sha256 ELSE EXCLUDED.sha256 END,
    root_name = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.root_name ELSE EXCLUDED.root_name END,
    netex_namespace = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.netex_namespace ELSE EXCLUDED.netex_namespace END,
    schema_version = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.schema_version ELSE EXCLUDED.schema_version END,
    store_raw_xml = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.store_raw_xml ELSE EXCLUDED.store_raw_xml END,
    parse_status = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.parse_status ELSE EXCLUDED.parse_status END,
    parse_error = CASE WHEN EXCLUDED.parse_status = 'skipped' THEN ingest.xml_document.parse_error ELSE EXCLUDED.parse_error END
  RETURNING id, batch_id, document_uid, parse_status
),
raw_rows AS (
  INSERT INTO ingest.xml_document_raw (
    document_id,
    raw_xml,
    raw_xml_text,
    compressed_raw_xml
  )
  SELECT
    dr.id,
    NULL::xml,
    r.raw_xml_text,
    decode(r.compressed_raw_xml_hex, 'hex')
  FROM fare_staging.load_xml_document_raw r
  JOIN document_rows dr
    ON dr.batch_id = r.batch_id
   AND dr.document_uid = r.document_uid
  JOIN fare_staging.load_xml_document d
    ON d.batch_id = r.batch_id
   AND d.document_uid = r.document_uid
  WHERE d.store_raw_xml
    AND dr.parse_status <> 'skipped'
  ON CONFLICT (document_id) DO UPDATE SET
    raw_xml = EXCLUDED.raw_xml,
    raw_xml_text = EXCLUDED.raw_xml_text,
    compressed_raw_xml = EXCLUDED.compressed_raw_xml
  RETURNING 1
)
UPDATE ingest.import_batch b
SET
  status = 'loaded',
  discovered_payload_count = (
    SELECT COUNT(*) FROM fare_staging.load_source_payload WHERE batch_id = b.id
  ) + (SELECT COUNT(*) * 0 FROM raw_rows),
  discovered_xml_count = (SELECT COUNT(*) FROM fare_staging.load_xml_document WHERE batch_id = b.id),
  parsed_xml_count = (SELECT COUNT(*) FROM document_rows WHERE batch_id = b.id AND parse_status = 'parsed'),
  failed_xml_count = (SELECT COUNT(*) FROM document_rows WHERE batch_id = b.id AND parse_status = 'failed')
WHERE b.id IN (SELECT DISTINCT batch_id FROM fare_staging.load_source_payload);
