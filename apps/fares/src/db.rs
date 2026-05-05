use anyhow::{Context, Result};
use dotenvy::dotenv;
use native_tls::TlsConnector;
use postgres::{Client, Config as PgConfig};
use postgres_native_tls::MakeTlsConnector;
use std::collections::HashSet;
use std::env;
use std::str::FromStr;
use std::time::Duration;

use crate::config::Config;

const LOCK_NAME: &str = "buslane_fares_import";
const JOB_VERSION: &str = env!("CARGO_PKG_VERSION");
const SCHEMA_VERSION: &str = "2026-05-04";

pub fn database_url() -> Result<String> {
    dotenv().ok();
    env::var("DATABASE_URL").context("DATABASE_URL must be set")
}

pub fn connect() -> Result<Client> {
    let url = database_url()?;
    connect_with_url(&url)
}

pub fn connect_with_url(url: &str) -> Result<Client> {
    let mut config =
        PgConfig::from_str(url).with_context(|| format!("invalid DATABASE_URL ({url})"))?;
    config
        .keepalives(true)
        .keepalives_idle(Duration::from_secs(30))
        .keepalives_interval(Duration::from_secs(10))
        .keepalives_retries(3)
        .tcp_user_timeout(Duration::from_secs(60));

    let connector = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("failed to build TLS connector")?;
    let tls = MakeTlsConnector::new(connector);
    config
        .connect(tls)
        .with_context(|| format!("error connecting to database ({url})"))
}

pub fn acquire_advisory_lock(client: &mut Client) -> Result<()> {
    let row = client
        .query_one(
            "SELECT pg_try_advisory_lock(hashtextextended($1, 0))",
            &[&LOCK_NAME],
        )
        .context("cannot acquire advisory lock")?;
    let locked: bool = row.get(0);
    if !locked {
        anyhow::bail!("another fares import is already running");
    }
    Ok(())
}

pub fn release_advisory_lock(client: &mut Client) -> Result<()> {
    client
        .query_one(
            "SELECT pg_advisory_unlock(hashtextextended($1, 0))",
            &[&LOCK_NAME],
        )
        .context("cannot release advisory lock")?;
    Ok(())
}

pub fn create_or_resume_batch(client: &mut Client, config: &Config) -> Result<i64> {
    if let Some(batch_id) = config.resume_batch_id {
        let updated = client
            .execute(
                r#"
                UPDATE ingest.import_batch
                SET status = 'running',
                    notes = COALESCE(notes || E'\n', '') || 'resumed by fares importer'
                WHERE id = $1
                "#,
                &[&batch_id],
            )
            .context("failed to mark resumed batch as running")?;
        if updated == 0 {
            anyhow::bail!("cannot resume missing fares import batch {batch_id}");
        }
        return Ok(batch_id);
    }

    let source_root = config.input_root.to_string_lossy().to_string();
    let row = client
        .query_one(
            r#"
            INSERT INTO ingest.import_batch (
              source_root,
              job_version,
              schema_version,
              status,
              xml_failure_threshold,
              store_raw_xml,
              strict_xsd
            )
            VALUES ($1, $2, $3, 'running', $4, $5, $6)
            RETURNING id
            "#,
            &[
                &source_root,
                &JOB_VERSION,
                &SCHEMA_VERSION,
                &config.xml_failure_threshold,
                &config.store_raw_xml,
                &config.strict_xsd,
            ],
        )
        .context("failed to create import batch")?;
    Ok(row.get(0))
}

pub fn typed_loaded_document_uids(client: &mut Client, batch_id: i64) -> Result<HashSet<u64>> {
    let mut completed = HashSet::new();
    for row in client
        .query(
            r#"
            SELECT document_uid
            FROM ingest.xml_document
            WHERE batch_id = $1
              AND parse_status = 'parsed'
              AND typed_loaded_at IS NOT NULL
            "#,
            &[&batch_id],
        )
        .context("failed to load resumable fares documents")?
    {
        let document_uid: i64 = row.get(0);
        if document_uid >= 0 {
            completed.insert(document_uid as u64);
        }
    }
    Ok(completed)
}

pub fn mark_batch_failed(client: &mut Client, batch_id: i64, error: &str) -> Result<()> {
    client
        .execute(
            r#"
            UPDATE ingest.import_batch
            SET status = 'failed',
                completed_at = NOW(),
                notes = COALESCE(notes || E'\n', '') || $2
            WHERE id = $1
            "#,
            &[&batch_id, &error],
        )
        .context("failed to mark batch failed")?;
    Ok(())
}

pub fn ensure_batch_partitions(client: &mut Client, batch_id: i64) -> Result<()> {
    let partition_targets = [
        ("netex", "object"),
        ("netex", "reference"),
        ("netex", "distance_matrix_element"),
        ("netex", "fare_price"),
        ("netex", "distance_matrix_element_price"),
        ("netex", "cell"),
        ("fare_current", "fare_price"),
    ];

    for (schema, table) in partition_targets {
        let partition = format!("{}_{}_b{}", schema, table, batch_id).replace('.', "_");
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {schema}.{partition} PARTITION OF {schema}.{table} FOR VALUES IN ({batch_id})"
        );
        client
            .batch_execute(&sql)
            .with_context(|| format!("failed to ensure partition {schema}.{partition}"))?;
    }

    Ok(())
}
