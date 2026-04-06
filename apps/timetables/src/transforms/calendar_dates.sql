INSERT INTO staging_services (id)
SELECT DISTINCT NULLIF(service_id, '')::bigint
FROM raw_calendar_dates
WHERE NULLIF(service_id, '') IS NOT NULL
ON CONFLICT (id) DO NOTHING;

INSERT INTO staging_calendar_dates (
  id,
  service_id,
  date,
  exception_type
)
SELECT
  ROW_NUMBER() OVER ()::bigint,
  NULLIF(service_id, '')::bigint,
  to_date(date, 'YYYYMMDD'),
  NULLIF(exception_type, '')::integer
FROM raw_calendar_dates;
