use chrono::Local;
use dotenvy::dotenv;
use native_tls::TlsConnector;
use postgres::Client;
use postgres_native_tls::MakeTlsConnector;
use std::env;

pub fn connect() -> Client {
    dotenv().ok();
    let url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connector = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("Failed to build TLS connector");
    let tls = MakeTlsConnector::new(connector);
    Client::connect(&url, tls)
        .unwrap_or_else(|_| panic!("Error connecting to database ({})", url))
}

pub fn acquire_advisory_lock(client: &mut Client) {
    let lock_key = Local::now().format("%Y%m%d").to_string().parse::<i64>().unwrap();
    let row = client
        .query_one("SELECT pg_try_advisory_lock($1)", &[&lock_key])
        .expect("Cannot acquire advisory lock");
    let locked: bool = row.get(0);
    if !locked {
        panic!("Cannot acquire advisory lock");
    }
}

pub fn release_advisory_lock(client: &mut Client) {
    let lock_key = Local::now().format("%Y%m%d").to_string().parse::<i64>().unwrap();
    client
        .query_one("SELECT pg_advisory_unlock($1)", &[&lock_key])
        .expect("Cannot release advisory lock");
}
