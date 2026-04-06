INSERT INTO staging_routes (
  id,
  agency_id,
  short_name,
  long_name,
  route_type
)
SELECT
  NULLIF(id, '')::integer,
  agency_id,
  NULLIF(short_name, ''),
  NULLIF(long_name, ''),
  NULLIF(route_type, '')::integer
FROM raw_routes;
