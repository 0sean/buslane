CREATE INDEX IF NOT EXISTS index_netex_object_resolution
  ON netex.object (batch_id, element_name, netex_id, version);

CREATE INDEX IF NOT EXISTS index_netex_object_document_identity
  ON netex.object (batch_id, document_id, element_name, netex_id, version);

CREATE INDEX IF NOT EXISTS index_netex_object_document
  ON netex.object (batch_id, document_id, element_name);

CREATE INDEX IF NOT EXISTS index_netex_reference_target
  ON netex.reference (batch_id, target_ref, target_version);

CREATE INDEX IF NOT EXISTS index_netex_reference_source
  ON netex.reference (batch_id, source_object_id, ref_element_name);

CREATE INDEX IF NOT EXISTS index_netex_reference_resolution_status
  ON netex.reference (batch_id, resolution_status);

CREATE INDEX IF NOT EXISTS index_netex_fare_table_tariff
  ON netex.fare_table (batch_id, tariff_object_id);

CREATE INDEX IF NOT EXISTS index_netex_fare_table_line
  ON netex.fare_table (batch_id, line_object_id);

CREATE INDEX IF NOT EXISTS index_netex_fare_table_product
  ON netex.fare_table (batch_id, fare_product_object_id);

CREATE INDEX IF NOT EXISTS index_netex_distance_matrix_element_start_end_zone
  ON netex.distance_matrix_element (batch_id, start_fare_zone_object_id, end_fare_zone_object_id);

CREATE INDEX IF NOT EXISTS index_netex_distance_matrix_element_start_end_stop
  ON netex.distance_matrix_element (batch_id, start_stop_point_object_id, end_stop_point_object_id);

CREATE INDEX IF NOT EXISTS index_netex_distance_matrix_element_price_dme
  ON netex.distance_matrix_element_price (batch_id, distance_matrix_element_object_id);

CREATE INDEX IF NOT EXISTS index_netex_cell_fare_table
  ON netex.cell (batch_id, fare_table_object_id);

CREATE INDEX IF NOT EXISTS index_netex_fare_zone_member_member_ref
  ON netex.fare_zone_member (batch_id, member_ref);

CREATE INDEX IF NOT EXISTS index_fare_staging_offer_fare_table
  ON fare_staging.fare_offer (batch_id, source_fare_table_object_id);

CREATE INDEX IF NOT EXISTS index_fare_staging_price_offer_zone
  ON fare_staging.fare_price (fare_offer_id, start_fare_zone_object_id, end_fare_zone_object_id);

CREATE INDEX IF NOT EXISTS index_fare_staging_price_offer_stop
  ON fare_staging.fare_price (fare_offer_id, start_scheduled_stop_point_object_id, end_scheduled_stop_point_object_id);

CREATE INDEX IF NOT EXISTS index_fare_current_offer_noc_line
  ON fare_current.fare_offer (noc, line_public_code, valid_from, valid_to);

CREATE INDEX IF NOT EXISTS index_fare_current_offer_product
  ON fare_current.fare_offer (fare_product_object_id, sales_offer_package_object_id, user_profile_object_id);

CREATE INDEX IF NOT EXISTS index_fare_current_price_offer_zone
  ON fare_current.fare_price (fare_offer_id, start_fare_zone_object_id, end_fare_zone_object_id);

CREATE INDEX IF NOT EXISTS index_fare_current_price_offer_stop
  ON fare_current.fare_price (fare_offer_id, start_scheduled_stop_point_object_id, end_scheduled_stop_point_object_id);

CREATE INDEX IF NOT EXISTS index_fare_current_zone_member_ref
  ON fare_current.fare_zone_member (member_ref, member_kind);

CREATE INDEX IF NOT EXISTS index_fare_current_operator_line_noc
  ON fare_current.operator_line (noc, line_public_code);
