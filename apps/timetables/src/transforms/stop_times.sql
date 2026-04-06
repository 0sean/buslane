INSERT INTO staging_stop_times (
  id,
  stop_id,
  trip_id,
  arrival_time,
  departure_time,
  stop_sequence,
  stop_headsign,
  pickup_type,
  drop_off_type,
  shape_dist_traveled,
  timepoint
)
SELECT
  ROW_NUMBER() OVER ()::bigint,
  stop_id,
  trip_id,
  arrival_time,
  departure_time,
  NULLIF(stop_sequence, '')::integer,
  NULLIF(stop_headsign, ''),
  COALESCE(NULLIF(pickup_type, ''), '0')::integer,
  COALESCE(NULLIF(drop_off_type, ''), '0')::integer,
  NULLIF(shape_dist_traveled, '')::numeric,
  COALESCE(NULLIF(timepoint, ''), '1')::integer
FROM raw_stop_times;
