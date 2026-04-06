COPY raw_frequencies (
  trip_id,
  start_time,
  end_time,
  headway_secs,
  exact_times
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
