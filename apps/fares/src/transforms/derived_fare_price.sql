WITH direct_distance_prices AS (
  SELECT
    o.batch_id,
    o.id AS fare_offer_id,
    COALESCE(NULLIF(o.tariff_basis, ''), 'unknown') AS pricing_basis,
    'DistanceMatrixElementPrice' AS source_price_kind,
    d.start_fare_zone_object_id,
    d.end_fare_zone_object_id,
    d.start_stop_point_object_id AS start_scheduled_stop_point_object_id,
    d.end_stop_point_object_id AS end_scheduled_stop_point_object_id,
    NULL::BIGINT AS start_fare_section_object_id,
    NULL::BIGINT AS end_fare_section_object_id,
    NULL::BIGINT AS start_fare_point_object_id,
    NULL::BIGINT AS end_fare_point_object_id,
    ARRAY_REMOVE(ARRAY[d.start_stop_point_ref], NULL) AS start_stop_refs,
    ARRAY_REMOVE(ARRAY[d.end_stop_point_ref], NULL) AS end_stop_refs,
    NULL::NUMERIC AS distance_units,
    p.amount,
    p.amount_text,
    p.currency,
    p.currency_source,
    p.is_allowed,
    FALSE AS is_derived,
    NULL::TEXT[] AS pricing_rule_refs,
    NULL::TEXT AS rounding_ref,
    NULL::BIGINT AS source_cell_object_id,
    p.object_id AS source_price_object_id,
    p.distance_matrix_element_object_id AS source_priceable_object_id
  FROM netex.distance_matrix_element_price p
  JOIN fare_staging.fare_offer o
    ON o.batch_id = p.batch_id
   AND o.source_fare_table_object_id = p.fare_table_object_id
  LEFT JOIN netex.distance_matrix_element d
    ON d.batch_id = p.batch_id
   AND d.object_id = p.distance_matrix_element_object_id
),
cell_prices AS (
  SELECT
    o.batch_id,
    o.id AS fare_offer_id,
    COALESCE(NULLIF(o.tariff_basis, ''), 'unknown') AS pricing_basis,
    CASE
      WHEN c.distance_matrix_element_price_object_id IS NOT NULL THEN 'CellDistanceMatrixElementPrice'
      WHEN c.fare_price_object_id IS NOT NULL THEN 'CellFarePrice'
      WHEN c.price_group_object_id IS NOT NULL THEN 'CellPriceGroup'
      ELSE 'Cell'
    END AS source_price_kind,
    d.start_fare_zone_object_id,
    d.end_fare_zone_object_id,
    d.start_stop_point_object_id AS start_scheduled_stop_point_object_id,
    d.end_stop_point_object_id AS end_scheduled_stop_point_object_id,
    NULL::BIGINT AS start_fare_section_object_id,
    NULL::BIGINT AS end_fare_section_object_id,
    NULL::BIGINT AS start_fare_point_object_id,
    NULL::BIGINT AS end_fare_point_object_id,
    ARRAY_REMOVE(ARRAY[d.start_stop_point_ref], NULL) AS start_stop_refs,
    ARRAY_REMOVE(ARRAY[d.end_stop_point_ref], NULL) AS end_stop_refs,
    NULL::NUMERIC AS distance_units,
    COALESCE(c.amount, p.amount, fp.amount) AS amount,
    COALESCE(c.amount_text, p.amount_text, fp.amount_text) AS amount_text,
    COALESCE(c.currency, p.currency, fp.currency) AS currency,
    COALESCE(c.currency_source, p.currency_source, fp.currency_source, 'unknown') AS currency_source,
    COALESCE(c.is_allowed, p.is_allowed, fp.is_allowed) AS is_allowed,
    FALSE AS is_derived,
    ARRAY_REMOVE(ARRAY[fp.pricing_rule_ref], NULL) AS pricing_rule_refs,
    fp.rounding_ref,
    c.object_id AS source_cell_object_id,
    COALESCE(p.object_id, fp.object_id) AS source_price_object_id,
    COALESCE(p.distance_matrix_element_object_id, fp.priced_object_id) AS source_priceable_object_id
  FROM netex.cell c
  JOIN fare_staging.fare_offer o
    ON o.batch_id = c.batch_id
   AND o.source_fare_table_object_id = c.fare_table_object_id
  LEFT JOIN netex.distance_matrix_element_price p
    ON p.batch_id = c.batch_id
   AND p.object_id = c.distance_matrix_element_price_object_id
  LEFT JOIN netex.fare_price fp
    ON fp.batch_id = c.batch_id
   AND fp.object_id = c.fare_price_object_id
  LEFT JOIN netex.distance_matrix_element d
    ON d.batch_id = c.batch_id
   AND d.object_id = p.distance_matrix_element_object_id
),
flat_prices AS (
  SELECT
    o.batch_id,
    o.id AS fare_offer_id,
    COALESCE(NULLIF(o.tariff_basis, ''), 'unknown') AS pricing_basis,
    p.price_kind AS source_price_kind,
    NULL::BIGINT AS start_fare_zone_object_id,
    NULL::BIGINT AS end_fare_zone_object_id,
    NULL::BIGINT AS start_scheduled_stop_point_object_id,
    NULL::BIGINT AS end_scheduled_stop_point_object_id,
    NULL::BIGINT AS start_fare_section_object_id,
    NULL::BIGINT AS end_fare_section_object_id,
    NULL::BIGINT AS start_fare_point_object_id,
    NULL::BIGINT AS end_fare_point_object_id,
    NULL::TEXT[] AS start_stop_refs,
    NULL::TEXT[] AS end_stop_refs,
    p.units AS distance_units,
    p.amount,
    p.amount_text,
    p.currency,
    p.currency_source,
    p.is_allowed,
    p.derived_from_price_ref IS NOT NULL AS is_derived,
    ARRAY_REMOVE(ARRAY[p.pricing_rule_ref], NULL) AS pricing_rule_refs,
    p.rounding_ref,
    NULL::BIGINT AS source_cell_object_id,
    p.object_id AS source_price_object_id,
    p.priced_object_id AS source_priceable_object_id
  FROM netex.fare_price p
  JOIN fare_staging.fare_offer o
    ON o.batch_id = p.batch_id
   AND o.source_fare_table_object_id = p.fare_table_object_id
  WHERE p.price_kind NOT IN ('DistanceMatrixElementPrice', 'CellPrice')
),
all_prices AS (
  SELECT * FROM direct_distance_prices
  UNION ALL
  SELECT * FROM cell_prices
  UNION ALL
  SELECT * FROM flat_prices
)
INSERT INTO fare_staging.fare_price (
  batch_id,
  fare_offer_id,
  pricing_basis,
  source_price_kind,
  start_fare_zone_object_id,
  end_fare_zone_object_id,
  start_scheduled_stop_point_object_id,
  end_scheduled_stop_point_object_id,
  start_fare_section_object_id,
  end_fare_section_object_id,
  start_fare_point_object_id,
  end_fare_point_object_id,
  start_stop_refs,
  end_stop_refs,
  distance_units,
  amount,
  amount_minor_units,
  amount_text,
  currency,
  currency_source,
  is_allowed,
  is_derived,
  pricing_rule_refs,
  rounding_ref,
  source_cell_object_id,
  source_price_object_id,
  source_priceable_object_id
)
SELECT
  batch_id,
  fare_offer_id,
  CASE
    WHEN pricing_basis IN ('flat','zone','zoneToZone','pointToPoint','distance','unitSection','route','tour','group','discount','period','free','other') THEN pricing_basis
    ELSE 'unknown'
  END,
  source_price_kind,
  start_fare_zone_object_id,
  end_fare_zone_object_id,
  start_scheduled_stop_point_object_id,
  end_scheduled_stop_point_object_id,
  start_fare_section_object_id,
  end_fare_section_object_id,
  start_fare_point_object_id,
  end_fare_point_object_id,
  start_stop_refs,
  end_stop_refs,
  distance_units,
  amount,
  CASE WHEN amount IS NULL THEN NULL ELSE (amount * 100)::INT END,
  amount_text,
  currency,
  COALESCE(currency_source, 'unknown'),
  is_allowed,
  COALESCE(is_derived, FALSE),
  pricing_rule_refs,
  rounding_ref,
  source_cell_object_id,
  source_price_object_id,
  source_priceable_object_id
FROM all_prices
WHERE amount IS NOT NULL OR is_allowed = FALSE;
