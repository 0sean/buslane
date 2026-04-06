COPY raw_services (
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
FROM STDIN
WITH (FORMAT csv, HEADER true);
