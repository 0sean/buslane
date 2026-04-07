use self::db::{acquire_advisory_lock, release_advisory_lock, connect};
use self::validation::run_validation_checks;
use postgres::Client;
use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};

mod db;
mod validation;

static COPIES: &[(&str, &str, &str)] = &[
    ("agencies", "agency.txt", include_str!("copies/agencies.sql")),
    ("stops", "stops.txt", include_str!("copies/stops.sql")),
    ("routes", "routes.txt", include_str!("copies/routes.sql")),
    ("services", "calendar.txt", include_str!("copies/services.sql")),
    ("calendar_dates", "calendar_dates.txt", include_str!("copies/calendar_dates.sql")),
    ("trips", "trips.txt", include_str!("copies/trips.sql")),
    ("shapes", "shapes.txt", include_str!("copies/shapes.sql")),
    ("frequencies", "frequencies.txt", include_str!("copies/frequencies.sql")),
    ("stop_times", "stop_times.txt", include_str!("copies/stop_times.sql")),
];

static TRANSFORMS: &[(&str, &str)] = &[
    ("agencies", include_str!("transforms/agencies.sql")),
    ("stops", include_str!("transforms/stops.sql")),
    ("routes", include_str!("transforms/routes.sql")),
    ("services", include_str!("transforms/services.sql")),
    ("calendar_dates", include_str!("transforms/calendar_dates.sql")),
    ("trips", include_str!("transforms/trips.sql")),
    ("shapes", include_str!("transforms/shapes.sql")),
    ("frequencies", include_str!("transforms/frequencies.sql")),
    ("stop_times", include_str!("transforms/stop_times.sql")),
];

static STAGING_SCHEMA: &str = include_str!("schemas/staging.sql");
static INDEXES: &str = include_str!("indexes.sql");
static SWAP: &str = include_str!("swap.sql");
static DROP: &str = include_str!("drop.sql");

fn main() {
    let client = &mut connect();
    acquire_advisory_lock(client);

    let args: Vec<String> = env::args().collect();

    let path = Path::new(&args[1]);
    if !path.exists() { panic!("Folder does not exist") }

    for (table_name, file_name, sql) in COPIES {
        let file_path = path.join(file_name);
        if !file_path.exists() { panic!("{} does not exist at {}", file_name, file_path.display()) }
        copy_table(client, file_path, table_name, sql);
    };

    for (table_name, sql) in TRANSFORMS {
        println!("Transforming table '{}'", table_name);
        client.simple_query(sql).expect(&format!("Failed to transform table '{table_name}'"));
    }

    println!("Creating indexes");
    client.simple_query(INDEXES).expect("Failed to create indexes");

    println!("Running validation checks");
    run_validation_checks(client);

    println!("Swapping tables");
    client.simple_query(SWAP).expect("Failed to swap tables");

    println!("Dropping and truncating tables");
    client.simple_query(DROP).expect("Failed to drop tables");

    println!("Reimporting staging schema");
    client.simple_query(STAGING_SCHEMA).expect("Failed to reimport staging schema");

    release_advisory_lock(client);
}

fn copy_table(client: &mut Client, path: PathBuf, table_name: &str, sql: &str) {
    println!("Copying table '{}'", table_name);

    let mut writer = client.copy_in(sql).expect("Failed to initiate COPY");
    let mut file = File::open(&path).expect("Cannot open file");
    std::io::copy(&mut file, &mut writer).expect("Failed to copy data");
    writer.finish().expect("Failed to finish COPY");
}