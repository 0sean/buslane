CREATE SCHEMA IF NOT EXISTS ingest;
CREATE SCHEMA IF NOT EXISTS netex;
CREATE SCHEMA IF NOT EXISTS fare_current;
CREATE SCHEMA IF NOT EXISTS ref;

CREATE TABLE IF NOT EXISTS ingest.import_batch (
  id BIGSERIAL PRIMARY KEY,
  started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMPTZ,
  source_root TEXT NOT NULL,
  job_version TEXT NOT NULL,
  schema_version TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('running', 'loading', 'loaded', 'validating', 'published', 'failed')),
  xml_failure_threshold NUMERIC NOT NULL DEFAULT 0.01,
  store_raw_xml BOOL NOT NULL DEFAULT FALSE,
  strict_xsd BOOL NOT NULL DEFAULT FALSE,
  discovered_payload_count BIGINT NOT NULL DEFAULT 0,
  discovered_xml_count BIGINT NOT NULL DEFAULT 0,
  parsed_xml_count BIGINT NOT NULL DEFAULT 0,
  failed_xml_count BIGINT NOT NULL DEFAULT 0,
  notes TEXT
);

CREATE TABLE IF NOT EXISTS ingest.source_payload (
  id BIGSERIAL PRIMARY KEY,
  batch_id BIGINT NOT NULL REFERENCES ingest.import_batch(id),
  payload_uid BIGINT NOT NULL,
  operator_directory TEXT NOT NULL,
  path TEXT NOT NULL,
  file_name TEXT NOT NULL,
  file_type TEXT NOT NULL CHECK (file_type IN ('xml', 'zip', 'other')),
  size_bytes BIGINT NOT NULL,
  sha256 BYTEA,
  modified_at TIMESTAMPTZ,
  read_status TEXT NOT NULL,
  read_error TEXT,
  UNIQUE (batch_id, payload_uid),
  UNIQUE (batch_id, path)
);

CREATE TABLE IF NOT EXISTS ingest.xml_document (
  id BIGSERIAL PRIMARY KEY,
  batch_id BIGINT NOT NULL REFERENCES ingest.import_batch(id),
  document_uid BIGINT NOT NULL,
  payload_uid BIGINT NOT NULL,
  payload_id BIGINT,
  zip_member_name TEXT,
  operator_directory TEXT NOT NULL,
  document_path TEXT NOT NULL,
  size_bytes BIGINT,
  sha256 BYTEA,
  publication_timestamp TIMESTAMPTZ,
  participant_ref TEXT,
  root_name TEXT NOT NULL,
  netex_namespace TEXT,
  schema_version TEXT,
  store_raw_xml BOOL NOT NULL DEFAULT FALSE,
  parse_status TEXT NOT NULL CHECK (parse_status IN ('parsed', 'failed', 'skipped')),
  parse_error TEXT,
  typed_loaded_at TIMESTAMPTZ,
  UNIQUE (batch_id, document_uid)
);

ALTER TABLE IF EXISTS ingest.xml_document
  ADD COLUMN IF NOT EXISTS typed_loaded_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS index_xml_document_payload_member
  ON ingest.xml_document (batch_id, payload_uid, COALESCE(zip_member_name, ''));

CREATE TABLE IF NOT EXISTS ingest.xml_document_raw (
  document_id BIGINT PRIMARY KEY REFERENCES ingest.xml_document(id) ON DELETE CASCADE,
  raw_xml XML,
  raw_xml_text TEXT,
  compressed_raw_xml BYTEA
);

CREATE TABLE IF NOT EXISTS netex.object (
  batch_id BIGINT NOT NULL,
  object_uid BIGINT NOT NULL,
  document_id BIGINT NOT NULL,
  document_uid BIGINT NOT NULL,
  object_ordinal INT NOT NULL,
  parent_object_id BIGINT,
  element_name TEXT NOT NULL,
  netex_id TEXT NOT NULL,
  version TEXT,
  codespace TEXT,
  substitution_group TEXT,
  xsd_type TEXT,
  frame_object_id BIGINT,
  frame_type_ref TEXT,
  name TEXT,
  description TEXT,
  private_code TEXT,
  public_code TEXT,
  path_in_document TEXT,
  attributes JSONB,
  PRIMARY KEY (batch_id, object_uid)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS netex.reference (
  batch_id BIGINT NOT NULL,
  id BIGSERIAL NOT NULL,
  document_id BIGINT NOT NULL,
  document_uid BIGINT NOT NULL,
  source_object_id BIGINT,
  source_element_name TEXT,
  ref_element_name TEXT NOT NULL,
  target_element_name TEXT,
  target_ref TEXT NOT NULL,
  target_version TEXT,
  text_value TEXT,
  resolved_object_id BIGINT,
  resolution_status TEXT NOT NULL DEFAULT 'unresolved',
  PRIMARY KEY (batch_id, id)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS netex.frame (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  frame_type TEXT NOT NULL,
  type_of_frame_ref TEXT,
  default_currency TEXT,
  default_language TEXT,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.priceable_object (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  priceable_kind TEXT NOT NULL,
  pricing_rule_ref TEXT,
  rounding_ref TEXT,
  default_price_unit_ref TEXT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.operator (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  operator_kind TEXT NOT NULL,
  noc TEXT,
  public_code TEXT,
  short_name TEXT,
  trading_name TEXT,
  primary_mode TEXT,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.line (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  operator_ref TEXT,
  public_code TEXT,
  private_code TEXT,
  line_type TEXT,
  transport_mode TEXT,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.scheduled_stop_point (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  atco_code TEXT,
  name TEXT,
  topographic_place_ref TEXT,
  topographic_projection_ref TEXT,
  fare_scheduled_stop_point_object_id BIGINT,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.fare_zone (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  zone_ref TEXT,
  parent_fare_zone_ref TEXT,
  scoping_method TEXT,
  zone_topology TEXT,
  type_of_zone_ref TEXT,
  transport_organisation_ref TEXT,
  group_of_operators_refs TEXT[],
  contains_fare_zone_refs TEXT[],
  neighbour_refs TEXT[],
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.fare_zone_member (
  batch_id BIGINT NOT NULL,
  fare_zone_object_id BIGINT NOT NULL,
  member_ref TEXT NOT NULL,
  member_kind TEXT NOT NULL CHECK (member_kind IN ('scheduled_stop_point', 'topographic_projection', 'fare_section', 'fare_point_in_pattern', 'other')),
  scheduled_stop_point_object_id BIGINT,
  fare_section_object_id BIGINT,
  fare_point_object_id BIGINT,
  sequence INT,
  PRIMARY KEY (batch_id, fare_zone_object_id, member_ref, member_kind)
);

CREATE TABLE IF NOT EXISTS netex.tariff (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  operator_ref TEXT,
  line_ref TEXT,
  group_of_lines_ref TEXT,
  group_of_operators_ref TEXT,
  type_of_tariff_ref TEXT,
  tariff_basis TEXT CHECK (tariff_basis IN ('flat','distance','unitSection','zone','zoneToZone','pointToPoint','route','tour','group','discount','period','free','other')),
  return_fare_twice_single BOOL,
  price_unit_ref TEXT,
  valid_from TIMESTAMPTZ,
  valid_to TIMESTAMPTZ,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.fare_product (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  product_kind TEXT NOT NULL,
  operator_ref TEXT,
  type_of_fare_product_ref TEXT,
  product_type TEXT,
  charging_moment_ref TEXT,
  charging_moment_type TEXT,
  price_unit_ref TEXT,
  amount NUMERIC,
  amount_text TEXT,
  discount_right_kind TEXT,
  discount_basis TEXT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.sales_offer_package (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  type_of_sales_offer_package_ref TEXT,
  group_of_sales_offer_package_refs TEXT[],
  rounding_ref TEXT,
  valid_from TIMESTAMPTZ,
  valid_to TIMESTAMPTZ,
  condition_summary JSONB,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.sales_offer_package_element (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  sales_offer_package_object_id BIGINT,
  fare_product_ref TEXT,
  fare_product_object_id BIGINT,
  preassigned_fare_product_ref TEXT,
  amount_of_price_unit_product_ref TEXT,
  type_of_travel_document_ref TEXT,
  requires_validation BOOL,
  order_index INT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.distribution_assignment (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  sales_offer_package_object_id BIGINT,
  distribution_channel_ref TEXT,
  distribution_channel_type TEXT,
  fulfilment_method_ref TEXT,
  payment_methods TEXT[],
  ticketing_service_facility_list TEXT[],
  order_index INT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.usage_parameter (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  parameter_type TEXT NOT NULL,
  user_type TEXT,
  minimum_age INT,
  maximum_age INT,
  discount_basis TEXT,
  proof_required TEXT,
  frequency_type TEXT,
  trip_type TEXT,
  can_transfer BOOL,
  start_time TIME,
  end_time TIME,
  duration TEXT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.fare_table (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  type_of_fare_table_ref TEXT,
  start_date DATE,
  end_date DATE,
  embargo_until TIMESTAMPTZ,
  tariff_ref TEXT,
  tariff_object_id BIGINT,
  line_ref TEXT,
  line_object_id BIGINT,
  fare_product_ref TEXT,
  fare_product_object_id BIGINT,
  sales_offer_package_ref TEXT,
  sales_offer_package_object_id BIGINT,
  user_profile_ref TEXT,
  type_of_travel_document_ref TEXT,
  prices_for_refs TEXT[],
  prices_for_object_ids BIGINT[],
  limitation_refs TEXT[],
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.fare_table_axis (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  fare_table_object_id BIGINT NOT NULL,
  axis TEXT NOT NULL CHECK (axis IN ('row', 'column')),
  sequence INT,
  parent_axis_object_id BIGINT,
  fare_zone_ref TEXT,
  fare_zone_object_id BIGINT,
  user_profile_ref TEXT,
  time_interval_ref TEXT,
  type_of_travel_document_ref TEXT,
  priceable_object_refs TEXT[],
  priceable_object_object_ids BIGINT[],
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.distance_matrix_element (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  start_stop_point_ref TEXT,
  start_stop_point_object_id BIGINT,
  start_fare_zone_ref TEXT,
  start_fare_zone_object_id BIGINT,
  start_fare_section_ref TEXT,
  start_fare_point_ref TEXT,
  end_stop_point_ref TEXT,
  end_stop_point_object_id BIGINT,
  end_fare_zone_ref TEXT,
  end_fare_zone_object_id BIGINT,
  end_fare_section_ref TEXT,
  end_fare_point_ref TEXT,
  series_constraint_refs TEXT[],
  group_of_distance_matrix_elements_refs TEXT[],
  name TEXT,
  PRIMARY KEY (batch_id, object_id)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS netex.fare_price (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  parent_object_id BIGINT,
  fare_table_object_id BIGINT,
  price_kind TEXT NOT NULL,
  amount NUMERIC(12, 2),
  amount_text TEXT,
  currency TEXT,
  currency_source TEXT CHECK (currency_source IN ('xml', 'frame_default', 'assumed', 'unknown')),
  units NUMERIC,
  price_unit_ref TEXT,
  pricing_rule_ref TEXT,
  rounding_ref TEXT,
  derived_from_price_ref TEXT,
  is_allowed BOOL,
  priced_object_ref TEXT,
  priced_object_id BIGINT,
  PRIMARY KEY (batch_id, object_id)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS netex.distance_matrix_element_price (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  fare_table_object_id BIGINT,
  distance_matrix_element_ref TEXT,
  distance_matrix_element_object_id BIGINT,
  geographical_interval_price_ref TEXT,
  geographical_interval_price_object_id BIGINT,
  fare_price_object_id BIGINT,
  amount NUMERIC(12, 2),
  amount_text TEXT,
  is_allowed BOOL,
  currency TEXT,
  currency_source TEXT CHECK (currency_source IN ('xml', 'frame_default', 'assumed', 'unknown')),
  PRIMARY KEY (batch_id, object_id)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS netex.cell (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  fare_table_object_id BIGINT NOT NULL,
  row_ref TEXT,
  row_axis_object_id BIGINT,
  column_ref TEXT,
  column_axis_object_id BIGINT,
  fare_price_ref TEXT,
  fare_price_object_id BIGINT,
  price_group_ref TEXT,
  price_group_object_id BIGINT,
  distance_matrix_element_price_ref TEXT,
  distance_matrix_element_price_object_id BIGINT,
  priceable_object_refs TEXT[],
  priceable_object_object_ids BIGINT[],
  group_of_distance_matrix_elements_ref TEXT,
  group_of_lines_ref TEXT,
  line_ref TEXT,
  fare_section_ref TEXT,
  amount NUMERIC(12, 2),
  amount_text TEXT,
  currency TEXT,
  currency_source TEXT CHECK (currency_source IN ('xml', 'frame_default', 'assumed', 'unknown')),
  is_allowed BOOL,
  PRIMARY KEY (batch_id, object_id)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS netex.access_right_parameter_assignment (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  parent_object_id BIGINT,
  assignment_type TEXT,
  validity_parameter_assignment_type TEXT,
  limitation_grouping_type TEXT,
  validity_parameter_grouping_type TEXT,
  extra_parameters JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.access_right_parameter_assignment_item (
  batch_id BIGINT NOT NULL,
  assignment_object_id BIGINT NOT NULL,
  parameter_group TEXT NOT NULL CHECK (parameter_group IN ('include', 'limitation', 'validity_parameter')),
  parameter_kind TEXT NOT NULL,
  parameter_ref TEXT,
  parameter_object_id BIGINT,
  operator TEXT,
  value_text TEXT,
  order_index INT,
  PRIMARY KEY (batch_id, assignment_object_id, parameter_group, parameter_kind, order_index)
);

CREATE TABLE IF NOT EXISTS netex.availability_condition (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  from_date TIMESTAMPTZ,
  to_date TIMESTAMPTZ,
  day_type_ref TEXT,
  start_time TIME,
  end_time TIME,
  days_of_week TEXT[],
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.notice (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  notice_code TEXT,
  text TEXT,
  private_code TEXT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS netex.notice_assignment (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  assigned_to_object_id BIGINT,
  notice_ref TEXT,
  notice_object_id BIGINT,
  raw_values JSONB,
  PRIMARY KEY (batch_id, object_id)
);

CREATE TABLE IF NOT EXISTS fare_current.fare_offer (
  id BIGSERIAL PRIMARY KEY,
  batch_id BIGINT NOT NULL,
  source_document_id BIGINT NOT NULL,
  source_fare_table_object_id BIGINT,
  operator_object_id BIGINT,
  noc TEXT,
  line_object_id BIGINT,
  line_public_code TEXT,
  group_of_lines_ref TEXT,
  group_of_operators_ref TEXT,
  tariff_object_id BIGINT,
  fare_product_object_id BIGINT,
  sales_offer_package_object_id BIGINT,
  user_profile_object_id BIGINT,
  product_type TEXT,
  product_kind TEXT,
  user_type TEXT,
  tariff_basis TEXT,
  type_of_tariff_ref TEXT,
  type_of_fare_structure_element_ref TEXT,
  valid_from TIMESTAMPTZ,
  valid_to TIMESTAMPTZ,
  sales_channel TEXT,
  fulfilment_method TEXT,
  travel_document_type_ref TEXT,
  payment_methods TEXT[],
  condition_summary JSONB,
  source_quality_flags TEXT[]
);

CREATE TABLE IF NOT EXISTS fare_current.fare_price (
  id BIGSERIAL NOT NULL,
  batch_id BIGINT NOT NULL,
  fare_offer_id BIGINT NOT NULL,
  pricing_basis TEXT NOT NULL CHECK (pricing_basis IN ('flat','zone','zoneToZone','pointToPoint','distance','unitSection','route','tour','group','discount','period','free','other','unknown')),
  source_price_kind TEXT,
  start_fare_zone_object_id BIGINT,
  end_fare_zone_object_id BIGINT,
  start_scheduled_stop_point_object_id BIGINT,
  end_scheduled_stop_point_object_id BIGINT,
  start_fare_section_object_id BIGINT,
  end_fare_section_object_id BIGINT,
  start_fare_point_object_id BIGINT,
  end_fare_point_object_id BIGINT,
  start_stop_refs TEXT[],
  end_stop_refs TEXT[],
  distance_units NUMERIC,
  amount NUMERIC(12, 2),
  amount_minor_units INT,
  amount_text TEXT,
  currency TEXT,
  currency_source TEXT CHECK (currency_source IN ('xml', 'frame_default', 'assumed', 'unknown')),
  is_allowed BOOL,
  is_derived BOOL NOT NULL DEFAULT FALSE,
  pricing_rule_refs TEXT[],
  rounding_ref TEXT,
  source_cell_object_id BIGINT,
  source_price_object_id BIGINT,
  source_priceable_object_id BIGINT,
  PRIMARY KEY (batch_id, id)
) PARTITION BY LIST (batch_id);

CREATE TABLE IF NOT EXISTS fare_current.fare_zone_member (
  fare_zone_object_id BIGINT NOT NULL,
  member_ref TEXT NOT NULL,
  member_kind TEXT NOT NULL,
  scheduled_stop_point_object_id BIGINT,
  atco_code TEXT,
  fare_section_object_id BIGINT,
  fare_point_object_id BIGINT,
  operator_object_id BIGINT,
  line_object_id BIGINT
);

CREATE TABLE IF NOT EXISTS fare_current.operator_line (
  operator_object_id BIGINT,
  noc TEXT,
  line_object_id BIGINT,
  line_public_code TEXT,
  group_of_lines_ref TEXT,
  group_of_operators_ref TEXT,
  source_tariff_object_id BIGINT
);

CREATE TABLE IF NOT EXISTS fare_current.import_quality_issue (
  id BIGSERIAL PRIMARY KEY,
  batch_id BIGINT NOT NULL,
  severity TEXT NOT NULL CHECK (severity IN ('fatal', 'warn', 'info')),
  issue_code TEXT NOT NULL,
  issue_count BIGINT NOT NULL DEFAULT 1,
  source_object_id BIGINT,
  source_document_id BIGINT,
  details JSONB,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
