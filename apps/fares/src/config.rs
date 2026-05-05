use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub input_root: PathBuf,
    pub workers: usize,
    pub channel_bound: usize,
    pub store_raw_xml: bool,
    pub strict_xsd: bool,
    pub netex_schema_path: Option<PathBuf>,
    pub skip_bad_payloads: bool,
    pub fail_on_warnings: bool,
    pub xml_failure_threshold: f64,
    pub resume_batch_id: Option<i64>,
}

impl Config {
    pub fn from_env_args() -> Result<Self> {
        let input_root = env::args()
            .nth(1)
            .or_else(|| env::var("FARES_INPUT_ROOT").ok())
            .map(PathBuf::from)
            .context("usage: fares <input-root> or set FARES_INPUT_ROOT")?;

        let workers = env_usize("FARES_WORKERS")?
            .unwrap_or_else(num_cpus::get)
            .max(1);
        let channel_bound = env_usize("FARES_CHANNEL_BOUND")?
            .unwrap_or(100_000)
            .max(1_000);

        Ok(Self {
            input_root,
            workers,
            channel_bound,
            store_raw_xml: env_bool("FARES_STORE_RAW_XML")?.unwrap_or(false),
            strict_xsd: env_bool("FARES_STRICT_XSD")?.unwrap_or(false),
            netex_schema_path: env::var("FARES_NETEX_SCHEMA").ok().map(PathBuf::from),
            skip_bad_payloads: env_bool("FARES_SKIP_BAD_PAYLOADS")?.unwrap_or(false),
            fail_on_warnings: env_bool("FARES_FAIL_ON_WARNINGS")?.unwrap_or(false),
            xml_failure_threshold: env_f64("FARES_XML_FAILURE_THRESHOLD")?.unwrap_or(0.01),
            resume_batch_id: env_i64("FARES_RESUME_BATCH_ID")?,
        })
    }
}

fn env_bool(name: &str) -> Result<Option<bool>> {
    match env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" => Ok(Some(true)),
            "0" | "false" | "no" | "n" => Ok(Some(false)),
            _ => anyhow::bail!("{name} must be a boolean"),
        },
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {name}")),
    }
}

fn env_usize(name: &str) -> Result<Option<usize>> {
    match env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .map(Some)
            .with_context(|| format!("{name} must be an unsigned integer")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {name}")),
    }
}

fn env_i64(name: &str) -> Result<Option<i64>> {
    match env::var(name) {
        Ok(value) => value
            .parse::<i64>()
            .map(Some)
            .with_context(|| format!("{name} must be an integer")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {name}")),
    }
}

fn env_f64(name: &str) -> Result<Option<f64>> {
    match env::var(name) {
        Ok(value) => value
            .parse::<f64>()
            .map(Some)
            .with_context(|| format!("{name} must be a number")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {name}")),
    }
}
