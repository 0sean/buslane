COPY raw_trips (
  route_id,
  service_id,
  id,
  headsign,
  direction_id,
  block_id,
  shape_id,
  wheelchair_accessible,
  vehicle_journey_code
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
