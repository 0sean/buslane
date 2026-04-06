COPY raw_calendar_dates (
  service_id,
  date,
  exception_type
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
