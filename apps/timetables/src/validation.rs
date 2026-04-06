use postgres::Client;

struct ValidationCheck {
    name: &'static str,
    sql: &'static str,
}

struct CountComparison {
    name: &'static str,
    raw_sql: &'static str,
    staging_sql: &'static str,
}

const VALIDATION_CHECKS: &[ValidationCheck] = &[
    ValidationCheck {
        name: "staging_agencies_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_agencies
            WHERE id IS NULL
               OR name IS NULL
               OR noc IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_services_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_services
            WHERE id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_routes_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_routes
            WHERE id IS NULL
               OR agency_id IS NULL
               OR route_type IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_stops_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stops
            WHERE id IS NULL
               OR name IS NULL
               OR lat IS NULL
               OR lon IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_shapes_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_shapes
            WHERE id IS NULL
               OR pt_lat IS NULL
               OR pt_lon IS NULL
               OR pt_sequence IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_trips_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_trips
            WHERE id IS NULL
               OR route_id IS NULL
               OR service_id IS NULL
               OR direction_id IS NULL
               OR block_id IS NULL
               OR vehicle_journey_code IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_frequencies_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_frequencies
            WHERE trip_id IS NULL
               OR start_time IS NULL
               OR end_time IS NULL
               OR headway_secs IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_stop_times_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stop_times
            WHERE stop_id IS NULL
               OR trip_id IS NULL
               OR arrival_time IS NULL
               OR departure_time IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_calendar_dates_missing_required_fields",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_calendar_dates
            WHERE service_id IS NULL
               OR date IS NULL
               OR exception_type IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_routes_orphan_agencies",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_routes r
            LEFT JOIN staging_agencies a ON a.id = r.agency_id
            WHERE a.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_trips_orphan_routes",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_trips t
            LEFT JOIN staging_routes r ON r.id = t.route_id
            WHERE r.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_trips_orphan_services",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_trips t
            LEFT JOIN staging_services s ON s.id = t.service_id
            WHERE s.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_trips_orphan_shapes",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_trips t
            LEFT JOIN staging_shapes s ON s.id = t.shape_id
            WHERE t.shape_id IS NOT NULL
              AND s.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_calendar_dates_orphan_services",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_calendar_dates cd
            LEFT JOIN staging_services s ON s.id = cd.service_id
            WHERE s.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_frequencies_orphan_trips",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_frequencies f
            LEFT JOIN staging_trips t ON t.id = f.trip_id
            WHERE t.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_stop_times_orphan_stops",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stop_times st
            LEFT JOIN staging_stops s ON s.id = st.stop_id
            WHERE s.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_stop_times_orphan_trips",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stop_times st
            LEFT JOIN staging_trips t ON t.id = st.trip_id
            WHERE t.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_stops_orphan_parent_stations",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stops s
            LEFT JOIN staging_stops parent ON parent.id = s.parent_station_id
            WHERE s.parent_station_id IS NOT NULL
              AND parent.id IS NULL
        "#,
    },
    ValidationCheck {
        name: "staging_trips_invalid_direction_id",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_trips
            WHERE direction_id NOT IN (0, 1)
        "#,
    },
    ValidationCheck {
        name: "staging_stop_times_invalid_pickup_dropoff",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stop_times
            WHERE pickup_type NOT IN (0, 1, 2, 3)
               OR drop_off_type NOT IN (0, 1, 2, 3)
        "#,
    },
    ValidationCheck {
        name: "staging_stop_times_invalid_timepoint",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_stop_times
            WHERE timepoint NOT IN (0, 1)
        "#,
    },
    ValidationCheck {
        name: "staging_frequencies_invalid_exact_times",
        sql: r#"
            SELECT COUNT(*)
            FROM staging_frequencies
            WHERE exact_times NOT IN (0, 1)
        "#,
    },
];

const COUNT_COMPARISONS: &[CountComparison] = &[
    CountComparison {
        name: "agency_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_agencies",
        staging_sql: "SELECT COUNT(*) FROM staging_agencies",
    },
    CountComparison {
        name: "service_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_services",
        staging_sql: "SELECT COUNT(*) FROM staging_services WHERE start_date IS NOT NULL AND end_date IS NOT NULL",
    },
    CountComparison {
        name: "calendar_dates_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_calendar_dates",
        staging_sql: "SELECT COUNT(*) FROM staging_calendar_dates",
    },
    CountComparison {
        name: "routes_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_routes",
        staging_sql: "SELECT COUNT(*) FROM staging_routes",
    },
    CountComparison {
        name: "stops_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_stops",
        staging_sql: "SELECT COUNT(*) FROM staging_stops",
    },
    CountComparison {
        name: "shapes_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_shapes",
        staging_sql: "SELECT COUNT(*) FROM staging_shapes",
    },
    CountComparison {
        name: "trips_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_trips",
        staging_sql: "SELECT COUNT(*) FROM staging_trips",
    },
    CountComparison {
        name: "frequencies_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_frequencies",
        staging_sql: "SELECT COUNT(*) FROM staging_frequencies",
    },
    CountComparison {
        name: "stop_times_count_match",
        raw_sql: "SELECT COUNT(*) FROM raw_stop_times",
        staging_sql: "SELECT COUNT(*) FROM staging_stop_times",
    },
];


pub fn run_validation_checks(client: &mut Client) {
    for check in VALIDATION_CHECKS {
        let row = client.query_one(check.sql, &[]).expect("Validation check query failed");
        let count: i64 = row.get(0);
    
        if count != 0 {
            println!("validation failed: {} = {}", check.name, count);
        }
    }

    for comparison in COUNT_COMPARISONS {
        let raw_count: i64 = client.query_one(comparison.raw_sql, &[]).expect("Validation check query failed").get(0);
        let staging_count: i64 = client.query_one(comparison.staging_sql, &[]).expect("Validation check query failed").get(0);
    
        if raw_count != staging_count {
            println!(
                "validation failed: {} raw={} staging={}",
                comparison.name,
                raw_count,
                staging_count
            );
        }
    }    
}