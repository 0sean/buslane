INSERT INTO staging_shapes (
  id,
  pt_lat,
  pt_lon,
  pt_sequence,
  dist_traveled
)
SELECT
  id,
  pt_lat,
  pt_lon,
  NULLIF(pt_sequence, '')::integer,
  NULLIF(dist_traveled, '')::numeric
FROM raw_shapes;
