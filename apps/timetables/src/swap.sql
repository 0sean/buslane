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
ALTER INDEX index_staging_agencies_on_noc RENAME TO index_agencies_on_noc;

ALTER TABLE services RENAME TO old_services;
ALTER TABLE staging_services RENAME TO services;

ALTER TABLE calendar_dates RENAME TO old_calendar_dates;
ALTER TABLE staging_calendar_dates RENAME TO calendar_dates;
ALTER INDEX index_staging_calendar_dates_on_service_id RENAME TO index_calendar_dates_on_service_id;

ALTER TABLE routes RENAME TO old_routes;
ALTER TABLE staging_routes RENAME TO routes;
ALTER INDEX index_staging_routes_on_agency_id RENAME TO index_routes_on_agency_id;
ALTER INDEX index_staging_routes_on_agency_id_and_id RENAME TO index_routes_on_agency_id_and_id;

ALTER TABLE stops RENAME TO old_stops;
ALTER TABLE staging_stops RENAME TO stops;
ALTER INDEX index_staging_stops_on_parent_station_id RENAME TO index_stops_on_parent_station_id;

ALTER TABLE shapes RENAME TO old_shapes;
ALTER TABLE staging_shapes RENAME TO shapes;

ALTER TABLE trips RENAME TO old_trips;
ALTER TABLE staging_trips RENAME TO trips;
ALTER INDEX index_staging_trips_on_route_id RENAME TO index_trips_on_route_id;
ALTER INDEX index_staging_trips_on_route_id_and_id RENAME TO index_trips_on_route_id_and_id;
ALTER INDEX index_staging_trips_on_service_id RENAME TO index_trips_on_service_id;
ALTER INDEX index_staging_trips_on_shape_id RENAME TO index_trips_on_shape_id;

ALTER TABLE frequencies RENAME TO old_frequencies;
ALTER TABLE staging_frequencies RENAME TO frequencies;
ALTER INDEX index_staging_frequencies_on_trip_id RENAME TO index_frequencies_on_trip_id;

ALTER TABLE stop_times RENAME TO old_stop_times;
ALTER TABLE staging_stop_times RENAME TO stop_times;
ALTER INDEX index_staging_stop_times_on_stop_id RENAME TO index_stop_times_on_stop_id;
ALTER INDEX index_staging_stop_times_on_trip_id RENAME TO index_stop_times_on_trip_id;

COMMIT;
