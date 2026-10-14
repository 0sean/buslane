use fs2::FileExt;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn fixture() -> TempDir {
    let temp = TempDir::new().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gtfs");
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), temp.path().join(entry.file_name())).unwrap();
    }
    temp
}
fn command(temp: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_timetables"));
    command.args([
        "--db",
        temp.join("timetables.sqlite").to_str().unwrap(),
        "--source",
        temp.to_str().unwrap(),
        "--scratch",
        temp.join("scratch").to_str().unwrap(),
        "--min-free-bytes",
        "0",
    ]);
    command
}
fn seed(temp: &Path) {
    assert!(command(temp).output().unwrap().status.success());
}
#[test]
fn killed_writer_rolls_back_and_releases_stable_lock() {
    let temp = fixture();
    seed(temp.path());
    let stops = temp.path().join("stops.txt");
    fs::write(
        &stops,
        fs::read_to_string(&stops)
            .unwrap()
            .replace("First Stop", "Uncommitted Stop"),
    )
    .unwrap();
    let mut times = fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join("stop_times.txt"))
        .unwrap();
    for sequence in 10..100_000 {
        writeln!(times, "T1,25:00:00,25:00:00,S1,{sequence},,0,0,,1").unwrap();
    }
    drop(times);
    let mut child = command(temp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            stderr.read_line(&mut line).unwrap() > 0,
            "import exited before test could kill it"
        );
        if line.starts_with("parsed stops.txt") {
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let db = rusqlite::Connection::open(temp.path().join("timetables.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT generation FROM dataset", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT name FROM stops WHERE public_id='S1'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "First Stop"
    );
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.path().join("timetables.sqlite.lock"))
        .unwrap();
    assert!(lock.try_lock_exclusive().is_ok());
}
#[test]
fn competing_import_exits_before_download() {
    let temp = fixture();
    seed(temp.path());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.path().join("timetables.sqlite.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let output = command(temp.path())
        .args(["--source", "https://invalid.example.test/"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("another importer")
    );
}
#[test]
fn zip_input_checksums_contents_and_parses_columns_by_name() {
    let temp = fixture();
    let stops = temp.path().join("stops.txt");
    let mut reader = csv::Reader::from_path(&stops).unwrap();
    let headers = reader
        .headers()
        .unwrap()
        .iter()
        .rev()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let records = reader
        .records()
        .map(|r| {
            r.unwrap()
                .iter()
                .rev()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    drop(reader);
    let mut writer = csv::Writer::from_path(&stops).unwrap();
    writer.write_record(headers).unwrap();
    for r in records {
        writer.write_record(r).unwrap();
    }
    writer.flush().unwrap();
    drop(writer);
    let archive = temp.path().join("feed.zip");
    let file = fs::File::create(&archive).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    for entry in fs::read_dir(temp.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().and_then(|v| v.to_str()) != Some("txt") {
            continue;
        }
        zip.start_file(
            entry.file_name().to_str().unwrap(),
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(&fs::read(entry.path()).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    let output = command(temp.path())
        .args(["--source", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let repeat = command(temp.path())
        .args(["--source", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(repeat.status.success());
    let report: serde_json::Value = serde_json::from_slice(&repeat.stdout).unwrap();
    assert_eq!(report["report"]["changed_files"], 0);
    let db = rusqlite::Connection::open(temp.path().join("timetables.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT name FROM stops WHERE public_id='S1'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "First Stop"
    );
}
