INSERT INTO fare_staging.fare_zone_member (
  fare_zone_object_id,
  member_ref,
  member_kind,
  scheduled_stop_point_object_id,
  atco_code,
  fare_section_object_id,
  fare_point_object_id,
  operator_object_id,
  line_object_id
)
SELECT
  m.fare_zone_object_id,
  m.member_ref,
  m.member_kind,
  m.scheduled_stop_point_object_id,
  s.atco_code,
  m.fare_section_object_id,
  m.fare_point_object_id,
  NULL::BIGINT,
  NULL::BIGINT
FROM netex.fare_zone_member m
LEFT JOIN netex.scheduled_stop_point s
  ON s.batch_id = m.batch_id
 AND s.object_id = m.scheduled_stop_point_object_id
WHERE m.batch_id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document);

INSERT INTO fare_staging.operator_line (
  operator_object_id,
  noc,
  line_object_id,
  line_public_code,
  group_of_lines_ref,
  group_of_operators_ref,
  source_tariff_object_id
)
SELECT DISTINCT
  op.object_id,
  op.noc,
  l.object_id,
  l.public_code,
  t.group_of_lines_ref,
  t.group_of_operators_ref,
  t.object_id
FROM netex.tariff t
LEFT JOIN netex.object line_obj
  ON line_obj.batch_id = t.batch_id
 AND line_obj.netex_id = t.line_ref
 AND line_obj.element_name = 'Line'
LEFT JOIN netex.line l
  ON l.batch_id = t.batch_id
 AND (l.object_id = line_obj.object_uid OR l.public_code = t.line_ref)
LEFT JOIN netex.operator op
  ON op.batch_id = t.batch_id
 AND (op.noc = t.operator_ref OR op.public_code = t.operator_ref OR op.noc = t.group_of_operators_ref)
WHERE t.batch_id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document);

INSERT INTO fare_staging.import_quality_issue (
  batch_id,
  severity,
  issue_code,
  issue_count,
  details
)
SELECT
  batch_id,
  'fatal',
  'derived_fare_price_without_amount_or_block',
  COUNT(*),
  '{}'::jsonb
FROM fare_staging.fare_price
WHERE amount IS NULL
  AND COALESCE(is_allowed, TRUE) IS TRUE
GROUP BY batch_id;

INSERT INTO fare_staging.import_quality_issue (
  batch_id,
  severity,
  issue_code,
  issue_count,
  details
)
SELECT
  batch_id,
  'warn',
  'fare_zone_member_unresolved',
  COUNT(*),
  jsonb_build_object('member_kind', member_kind)
FROM netex.fare_zone_member
WHERE batch_id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document)
  AND member_kind IN ('scheduled_stop_point', 'fare_section', 'fare_point_in_pattern')
  AND scheduled_stop_point_object_id IS NULL
  AND fare_section_object_id IS NULL
  AND fare_point_object_id IS NULL
GROUP BY batch_id, member_kind;
