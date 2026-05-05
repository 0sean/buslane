WITH active_batch AS (
  SELECT DISTINCT batch_id
  FROM fare_staging.load_xml_document
  LIMIT 1
),
fare_table_context AS (
  SELECT
    ft.batch_id,
    ft.object_id AS source_fare_table_object_id,
    obj.document_id AS source_document_id,
    ft.line_object_id,
    COALESCE(ft.line_ref, t.line_ref) AS line_ref,
    l.public_code AS line_public_code,
    t.object_id AS tariff_object_id,
    t.group_of_lines_ref,
    t.group_of_operators_ref,
    t.tariff_basis,
    t.type_of_tariff_ref,
    COALESCE(ft.fare_product_object_id, e.fare_product_object_id, fp.object_id) AS fare_product_object_id,
    fp.product_type,
    fp.product_kind,
    ft.sales_offer_package_object_id,
    sop.type_of_sales_offer_package_ref,
    ft.user_profile_ref,
    up.object_id AS user_profile_object_id,
    up.user_type,
    ft.type_of_travel_document_ref,
    COALESCE(sop.valid_from, t.valid_from) AS valid_from,
    COALESCE(sop.valid_to, t.valid_to) AS valid_to,
    da.distribution_channel_type AS sales_channel,
    da.fulfilment_method_ref AS fulfilment_method,
    da.payment_methods,
    sop.condition_summary
  FROM netex.fare_table ft
  JOIN active_batch ab ON ab.batch_id = ft.batch_id
  JOIN netex.object obj
    ON obj.batch_id = ft.batch_id
   AND obj.object_uid = ft.object_id
  LEFT JOIN netex.tariff t
    ON t.batch_id = ft.batch_id
   AND t.object_id = ft.tariff_object_id
  LEFT JOIN netex.line l
    ON l.batch_id = ft.batch_id
   AND l.object_id = ft.line_object_id
  LEFT JOIN netex.sales_offer_package sop
    ON sop.batch_id = ft.batch_id
   AND sop.object_id = ft.sales_offer_package_object_id
  LEFT JOIN netex.sales_offer_package_element e
    ON e.batch_id = ft.batch_id
   AND e.sales_offer_package_object_id = sop.object_id
  LEFT JOIN netex.fare_product fp
    ON fp.batch_id = ft.batch_id
   AND fp.object_id = COALESCE(ft.fare_product_object_id, e.fare_product_object_id)
  LEFT JOIN netex.distribution_assignment da
    ON da.batch_id = ft.batch_id
   AND da.sales_offer_package_object_id = sop.object_id
  LEFT JOIN netex.object user_obj
    ON user_obj.batch_id = ft.batch_id
   AND user_obj.netex_id = ft.user_profile_ref
   AND user_obj.element_name IN ('UserProfile', 'CompanionProfile', 'CommercialProfile')
  LEFT JOIN netex.usage_parameter up
    ON up.batch_id = user_obj.batch_id
   AND up.object_id = user_obj.object_uid
)
INSERT INTO fare_staging.fare_offer (
  batch_id,
  source_document_id,
  source_fare_table_object_id,
  operator_object_id,
  noc,
  line_object_id,
  line_public_code,
  group_of_lines_ref,
  group_of_operators_ref,
  tariff_object_id,
  fare_product_object_id,
  sales_offer_package_object_id,
  user_profile_object_id,
  product_type,
  product_kind,
  user_type,
  tariff_basis,
  type_of_tariff_ref,
  valid_from,
  valid_to,
  sales_channel,
  fulfilment_method,
  travel_document_type_ref,
  payment_methods,
  condition_summary,
  source_quality_flags
)
SELECT DISTINCT ON (batch_id, source_fare_table_object_id, fare_product_object_id, sales_offer_package_object_id, type_of_travel_document_ref)
  c.batch_id,
  c.source_document_id,
  c.source_fare_table_object_id,
  op.object_id AS operator_object_id,
  op.noc,
  c.line_object_id,
  c.line_public_code,
  c.group_of_lines_ref,
  c.group_of_operators_ref,
  c.tariff_object_id,
  c.fare_product_object_id,
  c.sales_offer_package_object_id,
  c.user_profile_object_id,
  c.product_type,
  c.product_kind,
  c.user_type,
  COALESCE(c.tariff_basis, 'unknown') AS tariff_basis,
  c.type_of_tariff_ref,
  c.valid_from,
  c.valid_to,
  c.sales_channel,
  c.fulfilment_method,
  c.type_of_travel_document_ref,
  c.payment_methods,
  c.condition_summary,
  ARRAY_REMOVE(ARRAY[
    CASE WHEN c.tariff_object_id IS NULL THEN 'missing_tariff' END,
    CASE WHEN c.fare_product_object_id IS NULL THEN 'missing_fare_product' END,
    CASE WHEN c.sales_offer_package_object_id IS NULL THEN 'missing_sales_offer_package' END
  ], NULL)
FROM fare_table_context c
LEFT JOIN netex.operator op
  ON op.batch_id = c.batch_id
 AND (
   op.noc = c.group_of_operators_ref
   OR op.public_code = c.line_ref
   OR op.object_id::text = c.group_of_operators_ref
 )
ORDER BY
  batch_id,
  source_fare_table_object_id,
  fare_product_object_id,
  sales_offer_package_object_id,
  type_of_travel_document_ref,
  c.valid_from DESC NULLS LAST;
