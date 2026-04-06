INSERT INTO staging_stops (
  id,
  code,
  name,
  lat,
  lon,
  wheelchair_boarding,
  location_type,
  platform_code,
  parent_station_id
)
SELECT
  id,
  NULLIF(code, ''),
  name,
  NULLIF(lat, '')::numeric,
  NULLIF(lon, '')::numeric,
  COALESCE(NULLIF(wheelchair_boarding, ''), '0')::integer = 1,
  COALESCE(NULLIF(location_type, ''), '0')::integer,
  NULLIF(platform_code, ''),
  NULLIF(parent_station_id, '')
FROM raw_stops;
