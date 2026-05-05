use anyhow::{Context, Result};
use std::path::Path;
use std::process;
use std::time::Instant;
use tracing::{info, warn};

mod config;
mod copy;
mod db;
mod discovery;
mod parser;
mod strict_xsd;
mod validation;

use crate::config::Config;
use crate::copy::CopyManager;
use crate::db::{
    acquire_advisory_lock, connect, create_or_resume_batch, database_url, ensure_batch_partitions,
    mark_batch_failed, release_advisory_lock, typed_loaded_document_uids,
};
use crate::discovery::{discover, emit_payloads};
use crate::parser::{emit_skipped_documents, parse_documents};
use crate::strict_xsd::validate_documents;
use crate::validation::run_validation_checks;

static MAIN_SCHEMA: &str = include_str!("schemas/main.sql");
static STAGING_SCHEMA: &str = include_str!("schemas/staging.sql");
static INGEST_TRANSFORM: &str = include_str!("transforms/ingest.sql");
static TYPED_OBJECTS_TRANSFORM: &str = include_str!("transforms/typed_objects.sql");
static INDEXES: &str = include_str!("indexes.sql");
static REFERENCE_RESOLUTION: &str = include_str!("transforms/reference_resolution.sql");
static DERIVED_FARE_OFFER: &str = include_str!("transforms/derived_fare_offer.sql");
static DERIVED_FARE_PRICE: &str = include_str!("transforms/derived_fare_price.sql");
static DERIVED_HELPERS: &str = include_str!("transforms/derived_helpers.sql");
static PUBLISH: &str = include_str!("publish.sql");
static DROP: &str = include_str!("drop.sql");

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if let Err(error) = run() {
        eprintln!("fares import failed: {error:?}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let started = Instant::now();
    let config = Config::from_env_args()?;
    ensure_input_root(&config.input_root)?;

    let mut client = connect()?;
    acquire_advisory_lock(&mut client)?;

    let mut batch_id = None;
    let result = (|| -> Result<()> {
        info!("creating schemas");
        client
            .batch_execute(MAIN_SCHEMA)
            .context("failed to apply fares main schema")?;
        client
            .batch_execute(STAGING_SCHEMA)
            .context("failed to apply fares staging schema")?;

        let active_batch_id = create_or_resume_batch(&mut client, &config)?;
        batch_id = Some(active_batch_id);
        ensure_batch_partitions(&mut client, active_batch_id)?;

        info!("discovering source payloads");
        let mut discovery = discover(&config, active_batch_id)?;
        info!(
            payloads = discovery.payloads.len(),
            documents = discovery.documents.len(),
            "source discovery complete"
        );
        let skipped_documents = if config.resume_batch_id.is_some() {
            let completed = typed_loaded_document_uids(&mut client, active_batch_id)?;
            let documents = std::mem::take(&mut discovery.documents);
            let (skipped, pending): (Vec<_>, Vec<_>) = documents
                .into_iter()
                .partition(|document| completed.contains(&document.document_uid));
            discovery.documents = pending;
            info!(
                skipped = skipped.len(),
                pending = discovery.documents.len(),
                "resume document filtering complete"
            );
            skipped
        } else {
            Vec::new()
        };
        validate_documents(&config, &discovery.documents)?;

        let copy_manager = CopyManager::start(database_url()?, config.channel_bound);
        let emitters = copy_manager.emitters();
        emit_payloads(&emitters, &discovery.payloads)?;
        emit_skipped_documents(&emitters, &skipped_documents)?;

        info!("parsing XML documents");
        parse_documents(&config, discovery.documents, emitters.clone())?;
        drop(emitters);
        copy_manager.finish()?;

        run_sql(&mut client, "ingest transform", INGEST_TRANSFORM)?;
        run_sql(
            &mut client,
            "typed object transform",
            TYPED_OBJECTS_TRANSFORM,
        )?;
        run_sql(&mut client, "indexes", INDEXES)?;
        run_sql(&mut client, "reference resolution", REFERENCE_RESOLUTION)?;
        run_sql(&mut client, "derived fare offer", DERIVED_FARE_OFFER)?;
        run_sql(&mut client, "derived fare price", DERIVED_FARE_PRICE)?;
        run_sql(&mut client, "derived helpers", DERIVED_HELPERS)?;

        info!("running validation checks");
        run_validation_checks(&mut client, config.fail_on_warnings)?;

        info!("publishing fare_current tables");
        run_sql(&mut client, "publish", PUBLISH)?;

        info!("dropping staging schema");
        run_sql(&mut client, "drop staging", DROP)?;
        client
            .batch_execute(STAGING_SCHEMA)
            .context("failed to recreate empty fares staging schema")?;

        Ok(())
    })();

    if let Err(error) = &result {
        if let Some(id) = batch_id {
            if let Err(mark_error) = mark_batch_failed(&mut client, id, &error.to_string()) {
                warn!("failed to mark fares batch failed: {mark_error:?}");
            }
        }
    }

    release_advisory_lock(&mut client)?;
    result?;

    info!(
        elapsed_seconds = started.elapsed().as_secs(),
        "fares import complete"
    );
    Ok(())
}

fn run_sql(client: &mut postgres::Client, name: &str, sql: &str) -> Result<()> {
    info!("running {name}");
    client
        .batch_execute(sql)
        .with_context(|| format!("failed to run {name}"))
}

fn ensure_input_root(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("input folder does not exist: {}", path.display());
    }
    if !path.is_dir() {
        anyhow::bail!("input path is not a folder: {}", path.display());
    }
    Ok(())
}
