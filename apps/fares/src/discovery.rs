use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::config::Config;
use crate::copy::{CopyTarget, Emitters, opt};

#[derive(Clone, Debug)]
pub struct SourcePayload {
    pub batch_id: i64,
    pub payload_uid: u64,
    pub operator_directory: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_type: PayloadType,
    pub size_bytes: u64,
    pub sha256_hex: Option<String>,
    pub read_status: String,
    pub read_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocumentSource {
    pub batch_id: i64,
    pub document_uid: u64,
    pub payload_uid: u64,
    pub payload_path: PathBuf,
    pub zip_member_name: Option<String>,
    pub operator_directory: String,
    pub document_path: String,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadType {
    Xml,
    Zip,
    Other,
}

impl PayloadType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xml => "xml",
            Self::Zip => "zip",
            Self::Other => "other",
        }
    }
}

pub struct Discovery {
    pub payloads: Vec<SourcePayload>,
    pub documents: Vec<DocumentSource>,
}

pub fn discover(config: &Config, batch_id: i64) -> Result<Discovery> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(&config.input_root).follow_links(false) {
        let entry = entry.context("failed walking fares input tree")?;
        if entry.file_type().is_file() {
            paths.push(entry.into_path());
        }
    }
    paths.sort();

    let mut payloads = Vec::new();
    let mut documents = Vec::new();
    let mut next_payload_uid = 1_u64;
    let mut next_document_uid = 1_u64;

    for path in paths {
        let payload_uid = next_payload_uid;
        next_payload_uid += 1;

        let relative_path = relative_path(&config.input_root, &path);
        let operator_directory = operator_directory(&relative_path);
        let file_name = path
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_else(|| relative_path.clone());
        let file_type = classify(&path);
        let metadata =
            fs::metadata(&path).with_context(|| format!("failed to stat {relative_path}"))?;
        let size_bytes = metadata.len();
        let sha256_hex =
            hash_file(&path).with_context(|| format!("failed to hash {relative_path}"))?;

        let mut payload = SourcePayload {
            batch_id,
            payload_uid,
            operator_directory: operator_directory.clone(),
            relative_path: relative_path.clone(),
            file_name,
            file_type,
            size_bytes,
            sha256_hex: Some(sha256_hex),
            read_status: "ok".to_string(),
            read_error: None,
        };

        match file_type {
            PayloadType::Xml => {
                documents.push(DocumentSource {
                    batch_id,
                    document_uid: next_document_uid,
                    payload_uid,
                    payload_path: path.clone(),
                    zip_member_name: None,
                    operator_directory,
                    document_path: relative_path,
                    size_bytes: Some(size_bytes),
                });
                next_document_uid += 1;
            }
            PayloadType::Zip => match enumerate_zip(
                &path,
                payload_uid,
                batch_id,
                &operator_directory,
                &relative_path,
                &mut next_document_uid,
            ) {
                Ok(mut zip_docs) => documents.append(&mut zip_docs),
                Err(error) if config.skip_bad_payloads => {
                    payload.read_status = "failed".to_string();
                    payload.read_error = Some(error.to_string());
                }
                Err(error) => return Err(error),
            },
            PayloadType::Other => {}
        }

        payloads.push(payload);
    }

    Ok(Discovery {
        payloads,
        documents,
    })
}

pub fn emit_payloads(emitters: &Emitters, payloads: &[SourcePayload]) -> Result<()> {
    for payload in payloads {
        emitters.send(
            CopyTarget::SourcePayload,
            &[
                opt(payload.batch_id),
                opt(payload.payload_uid),
                opt(payload.operator_directory.clone()),
                opt(payload.relative_path.clone()),
                opt(payload.file_name.clone()),
                opt(payload.file_type.as_str()),
                opt(payload.size_bytes),
                payload.sha256_hex.clone(),
                None,
                opt(payload.read_status.clone()),
                payload.read_error.clone(),
            ],
        )?;
    }
    Ok(())
}

fn classify(path: &Path) -> PayloadType {
    match path
        .extension()
        .map(|v| v.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("xml") => PayloadType::Xml,
        Some("zip") => PayloadType::Zip,
        _ => PayloadType::Other,
    }
}

fn enumerate_zip(
    path: &Path,
    payload_uid: u64,
    batch_id: i64,
    operator_directory: &str,
    relative_path: &str,
    next_document_uid: &mut u64,
) -> Result<Vec<DocumentSource>> {
    let file = File::open(path).with_context(|| format!("failed to open ZIP {relative_path}"))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read ZIP {relative_path}"))?;
    let mut docs = Vec::new();

    for index in 0..archive.len() {
        let member = archive
            .by_index(index)
            .with_context(|| format!("failed to inspect ZIP member {index} in {relative_path}"))?;
        if !member.is_file() || !member.name().to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        let member_name = member.name().to_string();
        let document_path = format!("{relative_path}!{member_name}");
        docs.push(DocumentSource {
            batch_id,
            document_uid: *next_document_uid,
            payload_uid,
            payload_path: path.to_path_buf(),
            zip_member_name: Some(member_name),
            operator_directory: operator_directory.to_string(),
            document_path,
            size_bytes: Some(member.size()),
        });
        *next_document_uid += 1;
    }

    Ok(docs)
}

fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn operator_directory(relative_path: &str) -> String {
    relative_path
        .split('/')
        .next()
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown")
        .to_string()
}
