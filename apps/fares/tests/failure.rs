use fs2::FileExt;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

const XML: &str = include_str!("../../../tests/fixtures/netex/fares.xml");
fn command(temp: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fares"));
    cmd.arg("--db")
        .arg(temp.join("fares.sqlite"))
        .arg("--source")
        .arg(temp.join("feed"))
        .arg("--scratch")
        .arg(temp.join("scratch"))
        .args(["--min-free-bytes", "0"]);
    cmd
}
fn seed() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("feed")).unwrap();
    fs::write(temp.path().join("feed/0000.xml"), XML).unwrap();
    let output = command(temp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    temp
}
#[test]
fn lock_prevents_a_competing_import_before_download() {
    let temp = seed();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.path().join("fares.sqlite.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let output = command(temp.path())
        .args(["--source", "https://invalid.example.test/"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("another fares importer"));
}
#[test]
fn killed_writer_retains_committed_data_and_releases_lock() {
    let temp = seed();
    fs::write(
        temp.path().join("feed/0000.xml"),
        XML.replace("2.00", "9.99"),
    )
    .unwrap();
    for i in 1..2000 {
        fs::write(temp.path().join(format!("feed/{i:04}.xml")), XML).unwrap();
    }
    let mut child = command(temp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut log = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            log.read_line(&mut line).unwrap() > 0,
            "writer ended before kill checkpoint"
        );
        if line.contains("1000 documents") {
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let db = rusqlite::Connection::open(temp.path().join("fares.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT generation FROM dataset", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT amount FROM entities WHERE ref='price:adult'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "2.00"
    );
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.path().join("fares.sqlite.lock"))
        .unwrap();
    assert!(lock.try_lock_exclusive().is_ok());
}
#[test]
fn identical_cli_import_does_not_grow_wal_or_change_database_bytes() {
    let temp = seed();
    let database = temp.path().join("fares.sqlite");
    let before = fs::read(&database).unwrap();
    let wal = fs::metadata(temp.path().join("fares.sqlite-wal"))
        .unwrap()
        .len();
    let output = command(temp.path()).output().unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["report"]["sqlite_changes"], 0);
    assert_eq!(fs::read(&database).unwrap(), before);
    assert_eq!(
        fs::metadata(temp.path().join("fares.sqlite-wal"))
            .unwrap()
            .len(),
        wal
    );
}
