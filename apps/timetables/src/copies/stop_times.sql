COPY raw_stop_times (
  trip_id,
  arrival_time,
  departure_time,
  stop_id,
  stop_sequence,
  stop_headsign,
  pickup_type,
  drop_off_type,
  shape_dist_traveled,
  timepoint
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
