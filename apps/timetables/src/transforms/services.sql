INSERT INTO staging_services (
  id,
  monday,
  tuesday,
  wednesday,
  thursday,
  friday,
  saturday,
  sunday,
  start_date,
  end_date
)
SELECT
  NULLIF(id, '')::bigint,
  COALESCE(NULLIF(monday, ''), '0')::integer = 1,
  COALESCE(NULLIF(tuesday, ''), '0')::integer = 1,
  COALESCE(NULLIF(wednesday, ''), '0')::integer = 1,
  COALESCE(NULLIF(thursday, ''), '0')::integer = 1,
  COALESCE(NULLIF(friday, ''), '0')::integer = 1,
  COALESCE(NULLIF(saturday, ''), '0')::integer = 1,
  COALESCE(NULLIF(sunday, ''), '0')::integer = 1,
  to_date(start_date, 'YYYYMMDD'),
  to_date(end_date, 'YYYYMMDD')
FROM raw_services;
