use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;

use crate::config::Config;
use crate::discovery::DocumentSource;

const XSD_BATCH_SIZE: usize = 200;

pub fn validate_documents(config: &Config, documents: &[DocumentSource]) -> Result<()> {
    if !config.strict_xsd {
        return Ok(());
    }

    let schema = config
        .netex_schema_path
        .as_ref()
        .context("FARES_STRICT_XSD=true requires FARES_NETEX_SCHEMA")?;

    let temp_dir = std::env::temp_dir().join(format!("fares-xsd-{}", std::process::id()));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).context("failed to clear strict XSD temp directory")?;
    }
    fs::create_dir_all(&temp_dir).context("failed to create strict XSD temp directory")?;

    let result = (|| -> Result<()> {
        let mut loose_files = Vec::new();
        let mut zip_members: BTreeMap<PathBuf, Vec<&DocumentSource>> = BTreeMap::new();

        for document in documents {
            if document.zip_member_name.is_some() {
                zip_members
                    .entry(document.payload_path.clone())
                    .or_default()
                    .push(document);
            } else {
                loose_files.push(document.payload_path.clone());
            }
        }

        validate_paths(schema, &loose_files)?;

        for (zip_path, docs) in zip_members {
            let extracted = extract_zip_members(&temp_dir, &zip_path, &docs)?;
            validate_paths(schema, &extracted)?;
        }

        Ok(())
    })();

    let cleanup_result = fs::remove_dir_all(&temp_dir);
    if result.is_ok() {
        cleanup_result.context("failed to remove strict XSD temp directory")?;
    }

    result
}

fn extract_zip_members(
    temp_dir: &Path,
    zip_path: &Path,
    documents: &[&DocumentSource],
) -> Result<Vec<PathBuf>> {
    let file =
        File::open(zip_path).with_context(|| format!("failed to open {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to read ZIP {}", zip_path.display()))?;
    let mut extracted = Vec::with_capacity(documents.len());

    for document in documents {
        let member_name = document
            .zip_member_name
            .as_ref()
            .expect("zip member documents are grouped here");
        let mut member = archive
            .by_name(member_name)
            .with_context(|| format!("failed to open ZIP member {}", document.document_path))?;
        let output_path = temp_dir.join(format!("{}.xml", document.document_uid));
        let mut output = File::create(&output_path)
            .with_context(|| format!("failed to create {}", output_path.display()))?;
        copy(&mut member, &mut output)
            .with_context(|| format!("failed to extract {}", document.document_path))?;
        extracted.push(output_path);
    }

    Ok(extracted)
}

fn validate_paths(schema: &Path, paths: &[PathBuf]) -> Result<()> {
    for chunk in paths.chunks(XSD_BATCH_SIZE) {
        if chunk.is_empty() {
            continue;
        }
        let status = Command::new("xmllint")
            .arg("--noout")
            .arg("--schema")
            .arg(schema)
            .args(chunk)
            .status()
            .context("failed to run xmllint for strict XSD validation")?;
        if !status.success() {
            anyhow::bail!(
                "strict XSD validation failed for a batch of {} XML documents",
                chunk.len()
            );
        }
    }
    Ok(())
}
