INSERT INTO staging_agencies (
  id,
  name,
  url,
  timezone,
  lang,
  phone,
  noc
)
SELECT
  id,
  name,
  NULLIF(url, ''),
  NULLIF(timezone, ''),
  NULLIF(lang, ''),
  NULLIF(phone, ''),
  noc
FROM raw_agencies;
