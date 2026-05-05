DROP SCHEMA IF EXISTS fare_staging CASCADE;
CREATE SCHEMA fare_staging;

CREATE UNLOGGED TABLE fare_staging.load_source_payload (
  batch_id BIGINT NOT NULL,
  payload_uid BIGINT NOT NULL,
  operator_directory TEXT NOT NULL,
  path TEXT NOT NULL,
  file_name TEXT NOT NULL,
  file_type TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  sha256_hex TEXT,
  modified_at TEXT,
  read_status TEXT NOT NULL,
  read_error TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_xml_document (
  batch_id BIGINT NOT NULL,
  document_uid BIGINT NOT NULL,
  payload_uid BIGINT NOT NULL,
  zip_member_name TEXT,
  operator_directory TEXT NOT NULL,
  document_path TEXT NOT NULL,
  size_bytes BIGINT,
  sha256_hex TEXT,
  root_name TEXT NOT NULL,
  netex_namespace TEXT,
  schema_version TEXT,
  parse_status TEXT NOT NULL,
  parse_error TEXT,
  store_raw_xml BOOL NOT NULL DEFAULT FALSE
);

CREATE UNLOGGED TABLE fare_staging.load_xml_document_raw (
  batch_id BIGINT NOT NULL,
  document_uid BIGINT NOT NULL,
  raw_xml_text TEXT,
  compressed_raw_xml_hex TEXT NOT NULL
);

CREATE UNLOGGED TABLE fare_staging.load_netex_object (
  batch_id BIGINT NOT NULL,
  document_uid BIGINT NOT NULL,
  object_uid BIGINT NOT NULL,
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
  attributes_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_netex_reference (
  batch_id BIGINT NOT NULL,
  document_uid BIGINT NOT NULL,
  source_object_id BIGINT,
  source_element_name TEXT,
  ref_element_name TEXT NOT NULL,
  target_element_name TEXT,
  target_ref TEXT NOT NULL,
  target_version TEXT,
  text_value TEXT,
  resolved_object_id BIGINT,
  resolution_status TEXT NOT NULL
);

CREATE UNLOGGED TABLE fare_staging.load_frame (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  frame_type TEXT NOT NULL,
  type_of_frame_ref TEXT,
  default_currency TEXT,
  default_language TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_priceable_object (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  priceable_kind TEXT NOT NULL,
  pricing_rule_ref TEXT,
  rounding_ref TEXT,
  default_price_unit_ref TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_operator (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  operator_kind TEXT NOT NULL,
  noc TEXT,
  public_code TEXT,
  short_name TEXT,
  trading_name TEXT,
  primary_mode TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_line (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  operator_ref TEXT,
  public_code TEXT,
  private_code TEXT,
  line_type TEXT,
  transport_mode TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_scheduled_stop_point (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  atco_code TEXT,
  name TEXT,
  topographic_place_ref TEXT,
  topographic_projection_ref TEXT,
  fare_scheduled_stop_point_object_id BIGINT
);

CREATE UNLOGGED TABLE fare_staging.load_fare_zone (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  zone_ref TEXT,
  parent_fare_zone_ref TEXT,
  scoping_method TEXT,
  zone_topology TEXT,
  type_of_zone_ref TEXT,
  transport_organisation_ref TEXT,
  group_of_operators_refs TEXT,
  contains_fare_zone_refs TEXT,
  neighbour_refs TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_fare_zone_member (
  batch_id BIGINT NOT NULL,
  fare_zone_object_id BIGINT NOT NULL,
  member_ref TEXT NOT NULL,
  member_kind TEXT NOT NULL,
  scheduled_stop_point_object_id BIGINT,
  fare_section_object_id BIGINT,
  fare_point_object_id BIGINT,
  sequence INT
);

CREATE UNLOGGED TABLE fare_staging.load_tariff (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  operator_ref TEXT,
  line_ref TEXT,
  group_of_lines_ref TEXT,
  group_of_operators_ref TEXT,
  type_of_tariff_ref TEXT,
  tariff_basis TEXT,
  return_fare_twice_single BOOL,
  price_unit_ref TEXT,
  valid_from TEXT,
  valid_to TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_fare_product (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  product_kind TEXT NOT NULL,
  operator_ref TEXT,
  type_of_fare_product_ref TEXT,
  product_type TEXT,
  charging_moment_ref TEXT,
  charging_moment_type TEXT,
  price_unit_ref TEXT,
  amount_text TEXT,
  discount_right_kind TEXT,
  discount_basis TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_sales_offer_package (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  type_of_sales_offer_package_ref TEXT,
  group_of_sales_offer_package_refs TEXT,
  rounding_ref TEXT,
  valid_from TEXT,
  valid_to TEXT,
  condition_summary_json TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_sales_offer_package_element (
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
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_distribution_assignment (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  sales_offer_package_object_id BIGINT,
  distribution_channel_ref TEXT,
  distribution_channel_type TEXT,
  fulfilment_method_ref TEXT,
  payment_methods TEXT,
  ticketing_service_facility_list TEXT,
  order_index INT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_usage_parameter (
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
  start_time TEXT,
  end_time TEXT,
  duration TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_fare_table (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  type_of_fare_table_ref TEXT,
  start_date TEXT,
  end_date TEXT,
  embargo_until TEXT,
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
  prices_for_refs TEXT,
  prices_for_object_ids TEXT,
  limitation_refs TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_fare_table_axis (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  fare_table_object_id BIGINT NOT NULL,
  axis TEXT NOT NULL,
  sequence INT,
  parent_axis_object_id BIGINT,
  fare_zone_ref TEXT,
  fare_zone_object_id BIGINT,
  user_profile_ref TEXT,
  time_interval_ref TEXT,
  type_of_travel_document_ref TEXT,
  priceable_object_refs TEXT,
  priceable_object_object_ids TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_distance_matrix_element (
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
  series_constraint_refs TEXT,
  group_of_distance_matrix_elements_refs TEXT,
  name TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_fare_price (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  parent_object_id BIGINT,
  fare_table_object_id BIGINT,
  price_kind TEXT NOT NULL,
  amount_text TEXT,
  currency TEXT,
  currency_source TEXT,
  units TEXT,
  price_unit_ref TEXT,
  pricing_rule_ref TEXT,
  rounding_ref TEXT,
  derived_from_price_ref TEXT,
  is_allowed BOOL,
  priced_object_ref TEXT,
  priced_object_id BIGINT
);

CREATE UNLOGGED TABLE fare_staging.load_distance_matrix_element_price (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  fare_table_object_id BIGINT,
  distance_matrix_element_ref TEXT,
  distance_matrix_element_object_id BIGINT,
  geographical_interval_price_ref TEXT,
  geographical_interval_price_object_id BIGINT,
  fare_price_object_id BIGINT,
  amount_text TEXT,
  is_allowed BOOL,
  currency TEXT,
  currency_source TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_cell (
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
  priceable_object_refs TEXT,
  priceable_object_object_ids TEXT,
  group_of_distance_matrix_elements_ref TEXT,
  group_of_lines_ref TEXT,
  line_ref TEXT,
  fare_section_ref TEXT,
  amount_text TEXT,
  currency TEXT,
  currency_source TEXT,
  is_allowed BOOL
);

CREATE UNLOGGED TABLE fare_staging.load_access_right_parameter_assignment (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  parent_object_id BIGINT,
  assignment_type TEXT,
  validity_parameter_assignment_type TEXT,
  limitation_grouping_type TEXT,
  validity_parameter_grouping_type TEXT,
  extra_parameters_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_access_right_parameter_assignment_item (
  batch_id BIGINT NOT NULL,
  assignment_object_id BIGINT NOT NULL,
  parameter_group TEXT NOT NULL,
  parameter_kind TEXT NOT NULL,
  parameter_ref TEXT,
  parameter_object_id BIGINT,
  operator TEXT,
  value_text TEXT,
  order_index INT
);

CREATE UNLOGGED TABLE fare_staging.load_availability_condition (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  from_date TEXT,
  to_date TEXT,
  day_type_ref TEXT,
  start_time TEXT,
  end_time TEXT,
  days_of_week TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_notice (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  notice_code TEXT,
  text TEXT,
  private_code TEXT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.load_notice_assignment (
  batch_id BIGINT NOT NULL,
  object_id BIGINT NOT NULL,
  assigned_to_object_id BIGINT,
  notice_ref TEXT,
  notice_object_id BIGINT,
  raw_values_json TEXT
);

CREATE UNLOGGED TABLE fare_staging.import_quality_issue (
  id BIGSERIAL PRIMARY KEY,
  batch_id BIGINT NOT NULL,
  severity TEXT NOT NULL CHECK (severity IN ('fatal', 'warn', 'info')),
  issue_code TEXT NOT NULL,
  issue_count BIGINT NOT NULL DEFAULT 1,
  source_object_id BIGINT,
  source_document_uid BIGINT,
  details JSONB,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNLOGGED TABLE fare_staging.fare_offer (LIKE fare_current.fare_offer INCLUDING DEFAULTS);
CREATE UNLOGGED TABLE fare_staging.fare_price (LIKE fare_current.fare_price INCLUDING DEFAULTS);
CREATE UNLOGGED TABLE fare_staging.fare_zone_member (LIKE fare_current.fare_zone_member);
CREATE UNLOGGED TABLE fare_staging.operator_line (LIKE fare_current.operator_line);
