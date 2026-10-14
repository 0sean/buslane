use super::*;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gtfs");
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), temp.path().join(entry.file_name())).unwrap();
    }
    temp
}
fn apply(conn: &mut Connection, path: &Path) -> anyhow::Result<import::Report> {
    import::import(conn, &mut source::Source::open(path)?, path, 0)
}
fn setup() -> (TempDir, Connection, PathBuf) {
    let temp = fixture();
    let path = temp.path().join("timetables.sqlite");
    let mut conn = db::connect(&path).unwrap();
    apply(&mut conn, temp.path()).unwrap();
    (temp, conn, path)
}
fn replace(path: &Path, name: &str, from: &str, to: &str) {
    let p = path.join(name);
    fs::write(&p, fs::read_to_string(&p).unwrap().replace(from, to)).unwrap();
}
fn generation(conn: &Connection) -> i64 {
    conn.query_row("SELECT generation FROM dataset", [], |r| r.get(0))
        .unwrap()
}
fn rows(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("SELECT id,created_at FROM {table} ORDER BY id"))
        .unwrap();
    stmt.query_map([], |r| {
        Ok(format!("{}:{}", r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
#[test]
fn identical_and_reordered_are_noop() {
    let (temp, mut conn, _) = setup();
    let baseline = rows(&conn, "stop_times");
    let writes = conn.total_changes();
    let report = apply(&mut conn, temp.path()).unwrap();
    assert_eq!(report.changed_files, 0);
    assert_eq!(conn.total_changes(), writes);
    assert_eq!(generation(&conn), 1);
    let p = temp.path().join("stop_times.txt");
    let data = fs::read_to_string(&p).unwrap();
    let mut lines = data.lines();
    let header = lines.next().unwrap();
    let mut records = lines.collect::<Vec<_>>();
    records.reverse();
    fs::write(p, format!("{header}\n{}\n", records.join("\n"))).unwrap();
    let report = apply(&mut conn, temp.path()).unwrap();
    assert_eq!(generation(&conn), 1);
    assert_eq!(report.tables["stop_times"].unchanged, 4);
    assert_eq!(rows(&conn, "stop_times"), baseline);
}
#[test]
fn changes_and_deletions_preserve_surviving_ids() {
    let (temp, mut conn, _) = setup();
    let baseline = rows(&conn, "stop_times");
    replace(temp.path(), "stops.txt", "First Stop", "Updated Stop");
    replace(
        temp.path(),
        "stop_times.txt",
        "25:10:00,25:10:00",
        "25:10:00,25:11:00",
    );
    let report = apply(&mut conn, temp.path()).unwrap();
    assert_eq!(report.tables["stops"].updated, 1);
    assert_eq!(report.tables["stop_times"].updated, 1);
    assert_eq!(rows(&conn, "stop_times"), baseline);
    assert_eq!(generation(&conn), 2);
    fs::remove_file(temp.path().join("frequencies.txt")).unwrap();
    let report = apply(&mut conn, temp.path()).unwrap();
    assert_eq!(report.tables["frequencies"].deleted, 1);
    for (name, line) in [
        ("agency.txt", "A2,Operator Two,,Europe/London,,,TWO\n"),
        ("routes.txt", "102,A2,2,Second Route,700\n"),
        ("trips.txt", "102,201,T3,Town,0,,SH1,0,VJ3\n"),
        ("stop_times.txt", "T3,09:00:00,09:00:00,S2,1,,0,0,,1\n"),
    ] {
        replace(temp.path(), name, line, "");
    }
    let report = apply(&mut conn, temp.path()).unwrap();
    assert_eq!(report.tables["trips"].deleted, 1);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM agency_stops", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn duplicate_and_orphan_updates_roll_back() {
    let (temp, mut conn, _) = setup();
    let baseline = rows(&conn, "stop_times");
    replace(temp.path(), "stops.txt", "First Stop", "Should Roll Back");
    let p = temp.path().join("stop_times.txt");
    let original = fs::read_to_string(&p).unwrap();
    fs::write(
        &p,
        format!("{original}{}\n", original.lines().nth(1).unwrap()),
    )
    .unwrap();
    assert!(
        apply(&mut conn, temp.path())
            .unwrap_err()
            .to_string()
            .contains("record")
    );
    assert_eq!(generation(&conn), 1);
    assert_eq!(rows(&conn, "stop_times"), baseline);
    assert_eq!(
        conn.query_row("SELECT name FROM stops WHERE public_id='S1'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "First Stop"
    );
    fs::write(p, original.replace("S1,1", "MISSING,1")).unwrap();
    assert!(apply(&mut conn, temp.path()).is_err());
    assert_eq!(generation(&conn), 1);
}
#[test]
fn calendars_union_and_optional_shapes() {
    let (temp, mut conn, _) = setup();
    assert!(
        conn.query_row(
            "SELECT monday IS NULL FROM services WHERE public_id=202",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    fs::remove_file(temp.path().join("calendar.txt")).unwrap();
    apply(&mut conn, temp.path()).unwrap();
    assert!(
        conn.query_row(
            "SELECT monday IS NULL FROM services WHERE public_id=201",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    replace(temp.path(), "trips.txt", ",SH1,", ",,");
    fs::remove_file(temp.path().join("shapes.txt")).unwrap();
    let report = apply(&mut conn, temp.path()).unwrap();
    assert_eq!(report.tables["shape_points"].deleted, 2);
}
#[test]
fn old_reader_snapshot_survives_atomic_commit() {
    let (temp, mut conn, path) = setup();
    let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(generation(&reader), 1);
    replace(temp.path(), "stops.txt", "First Stop", "New Stop");
    apply(&mut conn, temp.path()).unwrap();
    assert_eq!(generation(&conn), 2);
    assert_eq!(generation(&reader), 1);
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(generation(&reader), 2);
}
#[test]
fn disk_full_and_preflight_failure_roll_back() {
    let (temp, mut conn, _) = setup();
    replace(temp.path(), "stops.txt", "First Stop", "Should Roll Back");
    assert!(
        import::import(
            &mut conn,
            &mut source::Source::open(temp.path()).unwrap(),
            temp.path(),
            u64::MAX
        )
        .is_err()
    );
    assert_eq!(generation(&conn), 1);
    let pages: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    conn.execute_batch(&format!("PRAGMA max_page_count={}", pages + 1))
        .unwrap();
    replace(
        temp.path(),
        "trips.txt",
        ",Town,",
        &format!(",{},", "x".repeat(1024 * 1024)),
    );
    assert!(apply(&mut conn, temp.path()).is_err());
    assert_eq!(generation(&conn), 1);
    assert_eq!(
        conn.query_row("SELECT name FROM stops WHERE public_id='S1'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "First Stop"
    );
}
#[test]
fn relationship_change_updates_both_agency_memberships() {
    let (temp, mut conn, _) = setup();
    replace(temp.path(), "routes.txt", "101,A1,", "101,A2,");
    apply(&mut conn, temp.path()).unwrap();
    assert_eq!(conn.query_row("SELECT count(*) FROM agency_stops a JOIN agencies x ON x.id=a.agency_id WHERE x.public_id='A1'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM agency_stops", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn backup_restore_and_read_only_sidecars() {
    let (temp, conn, path) = setup();
    db::checkpoint(&conn).unwrap();
    let backup = temp.path().join("backup.sqlite");
    conn.backup(rusqlite::MAIN_DB, &backup, None).unwrap();
    drop(conn);
    assert!(PathBuf::from(format!("{}-wal", path.display())).exists());
    assert!(PathBuf::from(format!("{}-shm", path.display())).exists());
    let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(generation(&reader), 1);
    assert!(reader.execute("DELETE FROM stops", []).is_err());
    let restored = Connection::open(backup).unwrap();
    assert_eq!(generation(&restored), 1);
    assert_eq!(
        restored
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn interpolated_stop_times_are_nullable_but_exact_times_are_required() {
    let (temp, mut conn, _) = setup();
    replace(
        temp.path(),
        "stop_times.txt",
        "S2,2,,0,0,100.0,1",
        "S2,3,,0,0,100.0,1",
    );
    let p = temp.path().join("stop_times.txt");
    let mut data = fs::read_to_string(&p).unwrap();
    data.push_str("T1,,,S1,2,,0,0,,0\n");
    fs::write(&p, data).unwrap();
    apply(&mut conn, temp.path()).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM stop_times WHERE arrival_time IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    replace(
        temp.path(),
        "stop_times.txt",
        "T1,,,S1,2,,0,0,,0",
        "T1,,,S1,2,,0,0,,1",
    );
    assert!(apply(&mut conn, temp.path()).is_err());
    assert_eq!(generation(&conn), 2);
}

#[test]
fn deleted_referenced_parent_aborts_without_global_fk_scan() {
    let (temp, mut conn, _) = setup();
    replace(
        temp.path(),
        "stops.txt",
        "STATION,,Station,51.5050,-0.1250,0,1,,\n",
        "",
    );
    assert!(apply(&mut conn, temp.path()).is_err());
    assert_eq!(generation(&conn), 1);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM stops WHERE public_id='STATION'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
