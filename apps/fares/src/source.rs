use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_SOURCE: &str = "https://data.bus-data.dft.gov.uk/fares/download/bulk_archive";
pub fn check_disk(path: &Path, minimum: u64) -> Result<()> {
    let available = fs2::available_space(path)?;
    ensure!(
        available >= minimum,
        "insufficient disk space: {available} bytes available, {minimum} required"
    );
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
pub struct DownloadMetadata {
    pub source: String,
    pub etag: Option<String>,
    pub modified: Option<String>,
}

pub fn download(
    source: &str,
    scratch: &Path,
    minimum: u64,
) -> Result<(PathBuf, Option<DownloadMetadata>)> {
    if !source.starts_with("https://") && !source.starts_with("http://") {
        return Ok((PathBuf::from(source), None));
    }
    fs::create_dir_all(scratch)?;
    check_disk(scratch, minimum)?;
    let metadata_path = scratch.join("download.json");
    let metadata: DownloadMetadata = fs::read(&metadata_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let archive = scratch.join("feed.zip");
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(3600))
        .build()?;
    let mut request = client.get(source);
    if metadata.source == source && archive.exists() {
        if let Some(etag) = metadata.etag {
            request = request.header("If-None-Match", etag);
        }
        if let Some(modified) = metadata.modified {
            request = request.header("If-Modified-Since", modified);
        }
    }
    let mut response = request.send().context("download fares archive")?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok((archive, None));
    }
    if !response.status().is_success() {
        bail!("fares download returned {}", response.status());
    }
    if let Some(length) = response.content_length() {
        check_disk(scratch, minimum.saturating_add(length))?;
    }
    let metadata = DownloadMetadata {
        source: source.into(),
        etag: response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        modified: response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    };
    let partial = scratch.join("feed.zip.part");
    let _cleanup = PartialDownload(partial.clone());
    let mut file = File::create(&partial)?;
    let mut buffer = [0; 65536];
    let mut bytes = 0;
    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        bytes += n;
        if bytes >= 8 * 1024 * 1024 {
            check_disk(scratch, minimum)?;
            bytes = 0;
        }
    }
    file.sync_all()?;
    fs::rename(partial, &archive)?;
    Ok((archive, Some(metadata)))
}

struct PartialDownload(PathBuf);
impl Drop for PartialDownload {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("could not remove incomplete download: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;
    use tempfile::TempDir;

    #[test]
    #[ignore = "downloads the national archive; explicitly run with TMPDIR on SSD"]
    fn live_bods_download_and_conditional_redirect() {
        let temp = TempDir::new().unwrap();
        let (archive, metadata) = download(DEFAULT_SOURCE, temp.path(), 0).unwrap();
        let metadata = metadata.expect("first download must supply validators");
        let zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        assert!(!zip.is_empty());
        assert!(
            zip.file_names()
                .any(|n| n.to_ascii_lowercase().ends_with(".xml"))
        );
        assert!(metadata.etag.is_some() || metadata.modified.is_some());
        drop(zip);
        fs::write(
            temp.path().join("download.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let (cached, validators) = download(DEFAULT_SOURCE, temp.path(), 0).unwrap();
        assert_eq!(cached, archive);
        assert!(
            validators.is_none(),
            "BODS redirect should preserve conditional headers and return 304"
        );
    }

    fn server(responses: Vec<&'static str>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/feed.zip", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "download did not reach the test server"
                            );
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 1024];
                while !bytes.ends_with(b"\r\n\r\n") {
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    assert!(bytes.len() < 32768);
                }
                requests.push(String::from_utf8(bytes).unwrap());
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        (url, handle)
    }
    #[test]
    fn interrupted_download_removes_partial_and_preserves_complete_archive() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("feed.zip"), b"previous").unwrap();
        let (url, server) = server(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 10000\r\nConnection: close\r\n\r\nshort",
        ]);
        assert!(download(&url, temp.path(), 0).is_err());
        assert!(!temp.path().join("feed.zip.part").exists());
        assert_eq!(fs::read(temp.path().join("feed.zip")).unwrap(), b"previous");
        server.join().unwrap();
    }
    #[test]
    fn successful_download_supports_conditional_requests_after_commit() {
        let temp = TempDir::new().unwrap();
        let (url, server) = server(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nETag: \"v1\"\r\nLast-Modified: Tue, 13 Oct 2026 02:00:00 GMT\r\nConnection: close\r\n\r\narchive",
            "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n",
        ]);
        let (archive, metadata) = download(&url, temp.path(), 0).unwrap();
        assert_eq!(fs::read(&archive).unwrap(), b"archive");
        assert!(
            !temp.path().join("download.json").exists(),
            "uncommitted metadata must not be persisted"
        );
        fs::write(
            temp.path().join("download.json"),
            serde_json::to_vec(&metadata.unwrap()).unwrap(),
        )
        .unwrap();
        let (cached, metadata) = download(&url, temp.path(), 0).unwrap();
        assert_eq!(cached, archive);
        assert!(metadata.is_none());
        let requests = server.join().unwrap();
        assert!(requests[1].to_lowercase().contains("if-none-match: \"v1\""));
        assert!(
            requests[1]
                .to_lowercase()
                .contains("if-modified-since: tue, 13 oct 2026 02:00:00 gmt")
        );
    }
}
