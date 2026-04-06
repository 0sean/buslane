COPY raw_shapes (
  id,
  pt_lat,
  pt_lon,
  pt_sequence,
  dist_traveled
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
