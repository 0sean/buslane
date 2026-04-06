CREATE INDEX index_staging_agencies_on_noc
  ON staging_agencies (noc);

CREATE INDEX index_staging_calendar_dates_on_service_id
  ON staging_calendar_dates (service_id);

CREATE INDEX index_staging_frequencies_on_trip_id
  ON staging_frequencies (trip_id);

CREATE INDEX index_staging_routes_on_agency_id
  ON staging_routes (agency_id);

CREATE INDEX index_staging_routes_on_agency_id_and_id
  ON staging_routes (agency_id, id);

CREATE INDEX index_staging_stops_on_parent_station_id
  ON staging_stops (parent_station_id);

CREATE INDEX index_staging_stop_times_on_stop_id
  ON staging_stop_times (stop_id);

CREATE INDEX index_staging_stop_times_on_trip_id
  ON staging_stop_times (trip_id);

CREATE INDEX index_staging_trips_on_route_id
  ON staging_trips (route_id);

CREATE INDEX index_staging_trips_on_route_id_and_id
  ON staging_trips (route_id, id);

CREATE INDEX index_staging_trips_on_service_id
  ON staging_trips (service_id);

CREATE INDEX index_staging_trips_on_shape_id
  ON staging_trips (shape_id);
