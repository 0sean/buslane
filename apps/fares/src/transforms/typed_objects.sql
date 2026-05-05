INSERT INTO netex.object (
  batch_id,
  object_uid,
  document_id,
  document_uid,
  object_ordinal,
  parent_object_id,
  element_name,
  netex_id,
  version,
  codespace,
  substitution_group,
  xsd_type,
  frame_object_id,
  frame_type_ref,
  name,
  description,
  private_code,
  public_code,
  path_in_document,
  attributes
)
SELECT
  o.batch_id,
  o.object_uid,
  d.id,
  o.document_uid,
  o.object_ordinal,
  o.parent_object_id,
  o.element_name,
  o.netex_id,
  o.version,
  o.codespace,
  o.substitution_group,
  o.xsd_type,
  o.frame_object_id,
  o.frame_type_ref,
  o.name,
  o.description,
  o.private_code,
  o.public_code,
  o.path_in_document,
  COALESCE(NULLIF(o.attributes_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_netex_object o
JOIN ingest.xml_document d
  ON d.batch_id = o.batch_id
 AND d.document_uid = o.document_uid
WHERE d.parse_status = 'parsed'
ON CONFLICT (batch_id, object_uid) DO NOTHING;

DELETE FROM netex.reference r
USING ingest.xml_document d
JOIN fare_staging.load_xml_document l
  ON l.batch_id = d.batch_id
 AND l.document_uid = d.document_uid
WHERE r.batch_id = d.batch_id
  AND r.document_id = d.id
  AND d.parse_status = 'parsed';

INSERT INTO netex.reference (
  batch_id,
  document_id,
  document_uid,
  source_object_id,
  source_element_name,
  ref_element_name,
  target_element_name,
  target_ref,
  target_version,
  text_value,
  resolved_object_id,
  resolution_status
)
SELECT
  r.batch_id,
  d.id,
  r.document_uid,
  r.source_object_id,
  r.source_element_name,
  r.ref_element_name,
  r.target_element_name,
  r.target_ref,
  r.target_version,
  r.text_value,
  r.resolved_object_id,
  r.resolution_status
FROM fare_staging.load_netex_reference r
JOIN ingest.xml_document d
  ON d.batch_id = r.batch_id
 AND d.document_uid = r.document_uid
WHERE d.parse_status = 'parsed';

INSERT INTO netex.frame
SELECT * FROM fare_staging.load_frame
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.priceable_object
SELECT
  batch_id,
  object_id,
  priceable_kind,
  pricing_rule_ref,
  rounding_ref,
  default_price_unit_ref,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_priceable_object
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.operator
SELECT * FROM fare_staging.load_operator
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.line
SELECT * FROM fare_staging.load_line
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.scheduled_stop_point
SELECT * FROM fare_staging.load_scheduled_stop_point
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.fare_zone
SELECT
  batch_id,
  object_id,
  zone_ref,
  parent_fare_zone_ref,
  scoping_method,
  zone_topology,
  type_of_zone_ref,
  transport_organisation_ref,
  CASE WHEN NULLIF(group_of_operators_refs, '') IS NULL THEN NULL ELSE string_to_array(group_of_operators_refs, chr(31)) END,
  CASE WHEN NULLIF(contains_fare_zone_refs, '') IS NULL THEN NULL ELSE string_to_array(contains_fare_zone_refs, chr(31)) END,
  CASE WHEN NULLIF(neighbour_refs, '') IS NULL THEN NULL ELSE string_to_array(neighbour_refs, chr(31)) END
FROM fare_staging.load_fare_zone
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.fare_zone_member
SELECT * FROM fare_staging.load_fare_zone_member
ON CONFLICT (batch_id, fare_zone_object_id, member_ref, member_kind) DO NOTHING;

INSERT INTO netex.tariff
SELECT
  batch_id,
  object_id,
  operator_ref,
  line_ref,
  group_of_lines_ref,
  group_of_operators_ref,
  type_of_tariff_ref,
  tariff_basis,
  return_fare_twice_single,
  price_unit_ref,
  NULLIF(valid_from, '')::timestamptz,
  NULLIF(valid_to, '')::timestamptz
FROM fare_staging.load_tariff
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.fare_product
SELECT
  batch_id,
  object_id,
  product_kind,
  operator_ref,
  type_of_fare_product_ref,
  product_type,
  charging_moment_ref,
  charging_moment_type,
  price_unit_ref,
  CASE WHEN amount_text ~ '^-?[0-9]+(\.[0-9]+)?$' THEN amount_text::numeric END,
  amount_text,
  discount_right_kind,
  discount_basis,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_fare_product
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.sales_offer_package
SELECT
  batch_id,
  object_id,
  type_of_sales_offer_package_ref,
  CASE WHEN NULLIF(group_of_sales_offer_package_refs, '') IS NULL THEN NULL ELSE string_to_array(group_of_sales_offer_package_refs, chr(31)) END,
  rounding_ref,
  NULLIF(valid_from, '')::timestamptz,
  NULLIF(valid_to, '')::timestamptz,
  COALESCE(NULLIF(condition_summary_json, '')::jsonb, '{}'::jsonb),
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_sales_offer_package
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.sales_offer_package_element
SELECT
  batch_id,
  object_id,
  sales_offer_package_object_id,
  fare_product_ref,
  fare_product_object_id,
  preassigned_fare_product_ref,
  amount_of_price_unit_product_ref,
  type_of_travel_document_ref,
  requires_validation,
  order_index,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_sales_offer_package_element
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.distribution_assignment
SELECT
  batch_id,
  object_id,
  sales_offer_package_object_id,
  distribution_channel_ref,
  distribution_channel_type,
  fulfilment_method_ref,
  CASE WHEN NULLIF(payment_methods, '') IS NULL THEN NULL ELSE string_to_array(payment_methods, chr(31)) END,
  CASE WHEN NULLIF(ticketing_service_facility_list, '') IS NULL THEN NULL ELSE string_to_array(ticketing_service_facility_list, chr(31)) END,
  order_index,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_distribution_assignment
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.usage_parameter
SELECT
  batch_id,
  object_id,
  parameter_type,
  user_type,
  minimum_age,
  maximum_age,
  discount_basis,
  proof_required,
  frequency_type,
  trip_type,
  can_transfer,
  NULLIF(start_time, '')::time,
  NULLIF(end_time, '')::time,
  duration,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_usage_parameter
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.fare_table
SELECT
  batch_id,
  object_id,
  type_of_fare_table_ref,
  NULLIF(start_date, '')::date,
  NULLIF(end_date, '')::date,
  NULLIF(embargo_until, '')::timestamptz,
  tariff_ref,
  tariff_object_id,
  line_ref,
  line_object_id,
  fare_product_ref,
  fare_product_object_id,
  sales_offer_package_ref,
  sales_offer_package_object_id,
  user_profile_ref,
  type_of_travel_document_ref,
  CASE WHEN NULLIF(prices_for_refs, '') IS NULL THEN NULL ELSE string_to_array(prices_for_refs, chr(31)) END,
  CASE WHEN NULLIF(prices_for_object_ids, '') IS NULL THEN NULL ELSE string_to_array(prices_for_object_ids, chr(31))::bigint[] END,
  CASE WHEN NULLIF(limitation_refs, '') IS NULL THEN NULL ELSE string_to_array(limitation_refs, chr(31)) END,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_fare_table
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.fare_table_axis
SELECT
  batch_id,
  object_id,
  fare_table_object_id,
  axis,
  sequence,
  parent_axis_object_id,
  fare_zone_ref,
  fare_zone_object_id,
  user_profile_ref,
  time_interval_ref,
  type_of_travel_document_ref,
  CASE WHEN NULLIF(priceable_object_refs, '') IS NULL THEN NULL ELSE string_to_array(priceable_object_refs, chr(31)) END,
  CASE WHEN NULLIF(priceable_object_object_ids, '') IS NULL THEN NULL ELSE string_to_array(priceable_object_object_ids, chr(31))::bigint[] END
FROM fare_staging.load_fare_table_axis
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.distance_matrix_element
SELECT
  batch_id,
  object_id,
  start_stop_point_ref,
  start_stop_point_object_id,
  start_fare_zone_ref,
  start_fare_zone_object_id,
  start_fare_section_ref,
  start_fare_point_ref,
  end_stop_point_ref,
  end_stop_point_object_id,
  end_fare_zone_ref,
  end_fare_zone_object_id,
  end_fare_section_ref,
  end_fare_point_ref,
  CASE WHEN NULLIF(series_constraint_refs, '') IS NULL THEN NULL ELSE string_to_array(series_constraint_refs, chr(31)) END,
  CASE WHEN NULLIF(group_of_distance_matrix_elements_refs, '') IS NULL THEN NULL ELSE string_to_array(group_of_distance_matrix_elements_refs, chr(31)) END,
  name
FROM fare_staging.load_distance_matrix_element
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.fare_price
SELECT
  batch_id,
  object_id,
  parent_object_id,
  fare_table_object_id,
  price_kind,
  CASE WHEN amount_text ~ '^-?[0-9]+(\.[0-9]+)?$' THEN amount_text::numeric(12, 2) END,
  amount_text,
  currency,
  COALESCE(currency_source, 'unknown'),
  CASE WHEN units ~ '^-?[0-9]+(\.[0-9]+)?$' THEN units::numeric END,
  price_unit_ref,
  pricing_rule_ref,
  rounding_ref,
  derived_from_price_ref,
  is_allowed,
  priced_object_ref,
  priced_object_id
FROM fare_staging.load_fare_price
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.distance_matrix_element_price
SELECT
  batch_id,
  object_id,
  fare_table_object_id,
  distance_matrix_element_ref,
  distance_matrix_element_object_id,
  geographical_interval_price_ref,
  geographical_interval_price_object_id,
  fare_price_object_id,
  CASE WHEN amount_text ~ '^-?[0-9]+(\.[0-9]+)?$' THEN amount_text::numeric(12, 2) END,
  amount_text,
  is_allowed,
  currency,
  COALESCE(currency_source, 'unknown')
FROM fare_staging.load_distance_matrix_element_price
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.cell
SELECT
  batch_id,
  object_id,
  fare_table_object_id,
  row_ref,
  row_axis_object_id,
  column_ref,
  column_axis_object_id,
  fare_price_ref,
  fare_price_object_id,
  price_group_ref,
  price_group_object_id,
  distance_matrix_element_price_ref,
  distance_matrix_element_price_object_id,
  CASE WHEN NULLIF(priceable_object_refs, '') IS NULL THEN NULL ELSE string_to_array(priceable_object_refs, chr(31)) END,
  CASE WHEN NULLIF(priceable_object_object_ids, '') IS NULL THEN NULL ELSE string_to_array(priceable_object_object_ids, chr(31))::bigint[] END,
  group_of_distance_matrix_elements_ref,
  group_of_lines_ref,
  line_ref,
  fare_section_ref,
  CASE WHEN amount_text ~ '^-?[0-9]+(\.[0-9]+)?$' THEN amount_text::numeric(12, 2) END,
  amount_text,
  currency,
  COALESCE(currency_source, 'unknown'),
  is_allowed
FROM fare_staging.load_cell
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.access_right_parameter_assignment
SELECT
  batch_id,
  object_id,
  parent_object_id,
  assignment_type,
  validity_parameter_assignment_type,
  limitation_grouping_type,
  validity_parameter_grouping_type,
  COALESCE(NULLIF(extra_parameters_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_access_right_parameter_assignment
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.access_right_parameter_assignment_item
SELECT * FROM fare_staging.load_access_right_parameter_assignment_item
ON CONFLICT (batch_id, assignment_object_id, parameter_group, parameter_kind, order_index) DO NOTHING;

INSERT INTO netex.availability_condition
SELECT
  batch_id,
  object_id,
  NULLIF(from_date, '')::timestamptz,
  NULLIF(to_date, '')::timestamptz,
  day_type_ref,
  NULLIF(start_time, '')::time,
  NULLIF(end_time, '')::time,
  CASE WHEN NULLIF(days_of_week, '') IS NULL THEN NULL ELSE string_to_array(days_of_week, chr(31)) END,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_availability_condition
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.notice
SELECT
  batch_id,
  object_id,
  notice_code,
  text,
  private_code,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_notice
ON CONFLICT (batch_id, object_id) DO NOTHING;

INSERT INTO netex.notice_assignment
SELECT
  batch_id,
  object_id,
  assigned_to_object_id,
  notice_ref,
  notice_object_id,
  COALESCE(NULLIF(raw_values_json, '')::jsonb, '{}'::jsonb)
FROM fare_staging.load_notice_assignment
ON CONFLICT (batch_id, object_id) DO NOTHING;

UPDATE ingest.xml_document d
SET typed_loaded_at = COALESCE(d.typed_loaded_at, NOW())
FROM fare_staging.load_xml_document l
WHERE d.batch_id = l.batch_id
  AND d.document_uid = l.document_uid
  AND d.parse_status = 'parsed';
