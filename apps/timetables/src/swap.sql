BEGIN;

LOCK TABLE
  agencies,
  calendar_dates,
  frequencies,
  routes,
  services,
  shapes,
  stop_times,
  stops,
  trips,
  staging_agencies,
  staging_calendar_dates,
  staging_frequencies,
  staging_routes,
  staging_services,
  staging_shapes,
  staging_stop_times,
  staging_stops,
  staging_trips
IN ACCESS EXCLUSIVE MODE;

ALTER TABLE agencies RENAME TO old_agencies;
ALTER TABLE staging_agencies RENAME TO agencies;

ALTER TABLE services RENAME TO old_services;
ALTER TABLE staging_services RENAME TO services;

ALTER TABLE calendar_dates RENAME TO old_calendar_dates;
ALTER TABLE staging_calendar_dates RENAME TO calendar_dates;

ALTER TABLE routes RENAME TO old_routes;
ALTER TABLE staging_routes RENAME TO routes;

ALTER TABLE stops RENAME TO old_stops;
ALTER TABLE staging_stops RENAME TO stops;

ALTER TABLE shapes RENAME TO old_shapes;
ALTER TABLE staging_shapes RENAME TO shapes;

ALTER TABLE trips RENAME TO old_trips;
ALTER TABLE staging_trips RENAME TO trips;

ALTER TABLE frequencies RENAME TO old_frequencies;
ALTER TABLE staging_frequencies RENAME TO frequencies;

ALTER TABLE stop_times RENAME TO old_stop_times;
ALTER TABLE staging_stop_times RENAME TO stop_times;

COMMIT;
