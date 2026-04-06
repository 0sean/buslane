COPY raw_agencies (
  id,
  name,
  url,
  timezone,
  lang,
  phone,
  noc
)
FROM STDIN
WITH (FORMAT csv, HEADER true);
