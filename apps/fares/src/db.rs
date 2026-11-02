use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

pub const SCHEMA_VERSION: i64 = 1;
pub const APPLICATION_ID: i64 = 0x42555352;
pub const NORMALISATION_VERSION: i64 = 1;
const MIGRATION: &str = include_str!("../../../packages/fares-db/migrations/001.sql");

pub fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).context("open fares database")?;
    ensure!(
        rusqlite::version_number() >= 3_051_003,
        "SQLite >= 3.51.3 is required"
    );
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.set_prepared_statement_cache_capacity(64);
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        version <= SCHEMA_VERSION,
        "database schema is newer than this importer"
    );
    let application: i64 = conn.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if version == 0 {
        let tables: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            tables == 0 && application == 0,
            "refusing to migrate an unrecognized database"
        );
    } else {
        ensure!(
            application == APPLICATION_ID,
            "not a Buslane fares database"
        );
    }
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA page_size=4096; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA cache_size=-131072; PRAGMA temp_store=FILE;")?;
    // Keep initialized sidecars available to API containers with read-only mounts.
    let mut persist: i32 = 1;
    let code = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            conn.handle(),
            c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_PERSIST_WAL,
            (&mut persist as *mut i32).cast(),
        )
    };
    ensure!(
        code == rusqlite::ffi::SQLITE_OK,
        "cannot preserve WAL sidecars: {code}"
    );
    if version == 0 {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        if let Err(error) = conn.execute_batch(MIGRATION) {
            conn.execute_batch("ROLLBACK")?;
            return Err(error.into());
        }
        conn.execute_batch("COMMIT")?;
    }
    Ok(conn)
}

pub fn checkpoint(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA optimize=0x10002")?;
    conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()))?;
    for _ in 0..3 {
        let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
        if busy == 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("checkpoint deferred: readers still hold a snapshot");
    Ok(())
}

/// SQLite tracks unresolved deferred constraints as rows change. Checking the
/// counter avoids scanning every unchanged fares row on each daily import.
pub fn validate_deferred_foreign_keys(conn: &Connection) -> Result<()> {
    let mut current = 0;
    let mut highwater = 0;
    let status = unsafe {
        rusqlite::ffi::sqlite3_db_status(
            conn.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_DEFERRED_FKS,
            &mut current,
            &mut highwater,
            0,
        )
    };
    ensure!(
        status == rusqlite::ffi::SQLITE_OK,
        "cannot inspect deferred foreign keys"
    );
    ensure!(
        current == 0,
        "validation failed: unresolved foreign key references"
    );
    Ok(())
}
