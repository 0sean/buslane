COPY raw_routes (
  id,
  agency_id,
  short_name,
  long_name,
  route_type
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
