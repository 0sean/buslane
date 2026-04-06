COPY raw_stops (
  id,
  code,
  name,
  lat,
  lon,
  wheelchair_boarding,
  location_type,
  parent_station_id,
  platform_code
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
