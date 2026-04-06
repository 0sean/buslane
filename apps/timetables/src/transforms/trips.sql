INSERT INTO staging_trips (
  id,
  route_id,
  service_id,
  headsign,
  direction_id,
  block_id,
  wheelchair_accessible,
  vehicle_journey_code,
  shape_id
)
SELECT
  id,
  NULLIF(route_id, '')::integer,
  NULLIF(service_id, '')::bigint,
  NULLIF(headsign, ''),
  COALESCE(NULLIF(direction_id, ''), '0')::integer,
  COALESCE(NULLIF(block_id, ''), ''),
  COALESCE(NULLIF(wheelchair_accessible, ''), '0')::integer = 1,
  vehicle_journey_code,
  NULLIF(shape_id, '')
FROM raw_trips;
