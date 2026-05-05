UPDATE netex.reference r
SET
  resolved_object_id = o.object_uid,
  resolution_status = 'resolved_same_document'
FROM netex.object o
WHERE r.batch_id = o.batch_id
  AND r.document_id = o.document_id
  AND r.resolved_object_id IS NULL
  AND r.target_ref = o.netex_id
  AND (r.target_version IS NULL OR r.target_version = o.version)
  AND (
    r.target_element_name IS NULL
    OR r.target_element_name = o.element_name
    OR r.target_element_name = replace(o.element_name, 'FareScheduledStopPoint', 'ScheduledStopPoint')
  );

UPDATE netex.reference r
SET
  resolved_object_id = o.object_uid,
  resolution_status = 'resolved_same_document_any_element'
FROM netex.object o
WHERE r.batch_id = o.batch_id
  AND r.document_id = o.document_id
  AND r.resolved_object_id IS NULL
  AND r.target_ref = o.netex_id
  AND (r.target_version IS NULL OR r.target_version = o.version);

UPDATE netex.reference r
SET
  resolved_object_id = o.object_uid,
  resolution_status = 'resolved_same_batch'
FROM netex.object o
WHERE r.batch_id = o.batch_id
  AND r.resolved_object_id IS NULL
  AND r.target_ref = o.netex_id
  AND (r.target_version IS NULL OR r.target_version = o.version)
  AND (r.target_element_name IS NULL OR r.target_element_name = o.element_name);

UPDATE netex.fare_zone_member m
SET scheduled_stop_point_object_id = o.object_uid
FROM netex.object o
WHERE m.batch_id = o.batch_id
  AND m.member_kind = 'scheduled_stop_point'
  AND m.scheduled_stop_point_object_id IS NULL
  AND m.member_ref = o.netex_id
  AND o.element_name IN ('ScheduledStopPoint', 'FareScheduledStopPoint');

UPDATE netex.fare_zone_member m
SET fare_section_object_id = o.object_uid
FROM netex.object o
WHERE m.batch_id = o.batch_id
  AND m.member_kind = 'fare_section'
  AND m.fare_section_object_id IS NULL
  AND m.member_ref = o.netex_id
  AND o.element_name = 'FareSection';

UPDATE netex.fare_zone_member m
SET fare_point_object_id = o.object_uid
FROM netex.object o
WHERE m.batch_id = o.batch_id
  AND m.member_kind = 'fare_point_in_pattern'
  AND m.fare_point_object_id IS NULL
  AND m.member_ref = o.netex_id
  AND o.element_name = 'FarePointInPattern';

UPDATE netex.fare_table ft
SET tariff_object_id = o.object_uid
FROM netex.object o
WHERE ft.batch_id = o.batch_id
  AND ft.tariff_object_id IS NULL
  AND ft.tariff_ref = o.netex_id
  AND o.element_name = 'Tariff';

UPDATE netex.fare_table ft
SET line_object_id = o.object_uid
FROM netex.object o
WHERE ft.batch_id = o.batch_id
  AND ft.line_object_id IS NULL
  AND ft.line_ref = o.netex_id
  AND o.element_name = 'Line';

UPDATE netex.fare_table ft
SET fare_product_object_id = o.object_uid
FROM netex.object o
WHERE ft.batch_id = o.batch_id
  AND ft.fare_product_object_id IS NULL
  AND ft.fare_product_ref = o.netex_id
  AND o.element_name IN ('PreassignedFareProduct', 'AmountOfPriceUnitProduct', 'SupplementProduct', 'ThirdPartyProduct', 'UsageDiscountRight', 'SaleDiscountRight', 'CappedDiscountRight');

UPDATE netex.fare_table ft
SET sales_offer_package_object_id = o.object_uid
FROM netex.object o
WHERE ft.batch_id = o.batch_id
  AND ft.sales_offer_package_object_id IS NULL
  AND ft.sales_offer_package_ref = o.netex_id
  AND o.element_name = 'SalesOfferPackage';

UPDATE netex.sales_offer_package_element e
SET fare_product_object_id = o.object_uid
FROM netex.object o
WHERE e.batch_id = o.batch_id
  AND e.fare_product_object_id IS NULL
  AND COALESCE(e.fare_product_ref, e.preassigned_fare_product_ref, e.amount_of_price_unit_product_ref) = o.netex_id
  AND o.element_name IN ('PreassignedFareProduct', 'AmountOfPriceUnitProduct', 'SupplementProduct', 'ThirdPartyProduct', 'UsageDiscountRight', 'SaleDiscountRight', 'CappedDiscountRight');

UPDATE netex.distance_matrix_element d
SET start_stop_point_object_id = o.object_uid
FROM netex.object o
WHERE d.batch_id = o.batch_id
  AND d.start_stop_point_object_id IS NULL
  AND d.start_stop_point_ref = o.netex_id
  AND o.element_name IN ('ScheduledStopPoint', 'FareScheduledStopPoint');

UPDATE netex.distance_matrix_element d
SET end_stop_point_object_id = o.object_uid
FROM netex.object o
WHERE d.batch_id = o.batch_id
  AND d.end_stop_point_object_id IS NULL
  AND d.end_stop_point_ref = o.netex_id
  AND o.element_name IN ('ScheduledStopPoint', 'FareScheduledStopPoint');

UPDATE netex.distance_matrix_element d
SET start_fare_zone_object_id = o.object_uid
FROM netex.object o
WHERE d.batch_id = o.batch_id
  AND d.start_fare_zone_object_id IS NULL
  AND d.start_fare_zone_ref = o.netex_id
  AND o.element_name = 'FareZone';

UPDATE netex.distance_matrix_element d
SET end_fare_zone_object_id = o.object_uid
FROM netex.object o
WHERE d.batch_id = o.batch_id
  AND d.end_fare_zone_object_id IS NULL
  AND d.end_fare_zone_ref = o.netex_id
  AND o.element_name = 'FareZone';

UPDATE netex.distance_matrix_element_price p
SET distance_matrix_element_object_id = o.object_uid
FROM netex.object o
WHERE p.batch_id = o.batch_id
  AND p.distance_matrix_element_object_id IS NULL
  AND p.distance_matrix_element_ref = o.netex_id
  AND o.element_name = 'DistanceMatrixElement';

UPDATE netex.cell c
SET distance_matrix_element_price_object_id = o.object_uid
FROM netex.object o
WHERE c.batch_id = o.batch_id
  AND c.distance_matrix_element_price_object_id IS NULL
  AND c.distance_matrix_element_price_ref = o.netex_id
  AND o.element_name = 'DistanceMatrixElementPrice';

UPDATE netex.fare_price p
SET priced_object_id = po.object_id
FROM netex.priceable_object po
JOIN netex.object o
  ON o.batch_id = po.batch_id
 AND o.object_uid = po.object_id
WHERE p.batch_id = po.batch_id
  AND p.priced_object_id IS NULL
  AND p.priced_object_ref = o.netex_id;

INSERT INTO fare_staging.import_quality_issue (
  batch_id,
  severity,
  issue_code,
  issue_count,
  details
)
SELECT
  batch_id,
  'info',
  'unresolved_reference',
  COUNT(*),
  jsonb_build_object('target_element_name', target_element_name)
FROM netex.reference
WHERE resolution_status = 'unresolved'
GROUP BY batch_id, target_element_name;
