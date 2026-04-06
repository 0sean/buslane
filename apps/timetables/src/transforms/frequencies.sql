INSERT INTO staging_frequencies (
  id,
  trip_id,
  start_time,
  end_time,
  headway_secs,
  exact_times
)
SELECT
  ROW_NUMBER() OVER ()::bigint,
  trip_id,
  start_time,
  end_time,
  NULLIF(headway_secs, '')::integer,
  COALESCE(NULLIF(exact_times, ''), '0')::integer
FROM raw_frequencies;
