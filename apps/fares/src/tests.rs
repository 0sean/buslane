use super::*;
use std::{
    fs,
    io::{Cursor, Write},
};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

const XML: &str = include_str!("../../../tests/fixtures/netex/fares.xml");
fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in members {
        zip.start_file(
            *name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn apply(conn: &mut rusqlite::Connection, temp: &TempDir, path: &Path) -> Result<import::Report> {
    import::import(conn, path, temp.path(), temp.path(), 0)
}

#[test]
fn nested_zip_incremental_reconciliation_preserves_ids_and_noop_writes() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("fares.sqlite");
    let path = temp.path().join("feed.zip");
    let inner = archive(&[("folder/adult.xml", XML.as_bytes())]);
    fs::write(
        &path,
        archive(&[
            ("operator/nested.zip", &inner),
            ("direct.xml", XML.as_bytes()),
        ]),
    )
    .unwrap();
    let mut conn = db::connect(&database).unwrap();
    let first = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(first.documents_inserted, 2);
    assert!(first.entities_inserted > 10);
    let ids: Vec<(i64, String)> = conn
        .prepare("SELECT id,identity FROM entities ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let repeat = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(repeat.sqlite_changes, 0);
    assert_eq!(repeat.generation, 1);
    assert_eq!(repeat.documents_skipped, 2);
    assert_eq!(repeat.archives_skipped, 2);
    // Outer archive is repacked but nested archive is byte-identical.
    fs::write(
        &path,
        archive(&[
            ("operator/nested.zip", &inner),
            ("direct.xml", XML.replace("2.00", "2.50").as_bytes()),
        ]),
    )
    .unwrap();
    let changed = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(changed.documents_updated, 1);
    assert_eq!(changed.documents_skipped, 1);
    assert_eq!(changed.entities_updated, 2); // price and its price group
    assert_eq!(changed.entities_inserted, 0);
    assert_eq!(changed.entities_deleted, 0);
    assert!(changed.entities_unchanged > 10);
    let after: Vec<(i64, String)> = conn
        .prepare("SELECT id,identity FROM entities ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(ids, after);
    fs::write(&path, archive(&[("operator/nested.zip", &inner)])).unwrap();
    let removed = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(removed.documents_deleted, 1);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[test]
fn changed_document_removes_entities_and_links_without_replacing_survivors() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    fs::write(&path, XML).unwrap();
    let mut conn = db::connect(&temp.path().join("db.sqlite")).unwrap();
    apply(&mut conn, &temp, &path).unwrap();
    let id: i64 = conn
        .query_row(
            "SELECT id FROM entities WHERE ref='product:adult'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let removed=XML.replace("<PreassignedFareProduct id=\"product:child\" version=\"1\"><Name>Child single</Name><ProductType>singleTrip</ProductType></PreassignedFareProduct>","").replace("<ScheduledStopPoint id=\"atco:S2\"><Name>Second stop</Name></ScheduledStopPoint>","").replace("<ScheduledStopPointRef ref=\"atco:S2\"/>","");
    fs::write(&path, removed).unwrap();
    let report = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(report.entities_deleted, 1);
    assert_eq!(
        conn.query_row(
            "SELECT id FROM entities WHERE ref='product:adult'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        id
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM links WHERE code='S2'", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn malformed_input_rolls_back_all_changes_and_empty_feed_cannot_erase_dataset() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.zip");
    let mut conn = db::connect(&temp.path().join("db.sqlite")).unwrap();
    fs::write(&path, archive(&[("good.xml", XML.as_bytes())])).unwrap();
    apply(&mut conn, &temp, &path).unwrap();
    let before: String = conn
        .query_row("SELECT group_concat(id || sha256) FROM entities", [], |r| {
            r.get(0)
        })
        .unwrap();
    fs::write(
        &path,
        archive(&[
            ("good.xml", XML.replace("2.00", "3.00").as_bytes()),
            ("bad.xml", b"<bad>"),
        ]),
    )
    .unwrap();
    assert!(apply(&mut conn, &temp, &path).is_err());
    assert_eq!(
        conn.query_row("SELECT group_concat(id || sha256) FROM entities", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        before
    );
    fs::write(&path, archive(&[("readme.txt", b"no data")])).unwrap();
    assert!(apply(&mut conn, &temp, &path).is_err());
    assert_eq!(
        conn.query_row("SELECT group_concat(id || sha256) FROM entities", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        before
    );
}

#[test]
fn parser_handles_prefixes_cdata_duplicates_and_exact_decimal_context() {
    let prefixed = XML
        .replace("xmlns=", "xmlns:n=")
        .replace('<', "<n:")
        .replace("<n:/", "</n:")
        .replace("<n:?xml", "<?xml");
    let parsed = netex::parse(prefixed.as_bytes()).unwrap();
    assert_eq!(parsed.currency.as_deref(), Some("GBP"));
    assert!(
        parsed
            .links
            .iter()
            .any(|l| l.code == "ONE" && l.name.as_deref() == Some("One & Sons"))
    );
    let fare = parsed
        .entities
        .iter()
        .find(|e| e.reference == "fare:AB")
        .unwrap();
    assert_eq!(fare.product_ref.as_deref(), Some("product:adult"));
    assert_eq!(fare.profile_ref.as_deref(), Some("adult"));
    assert_eq!(fare.price_ref.as_deref(), Some("price:adult"));
    assert_eq!(fare.matrix_ref.as_deref(), Some("matrix:AB"));
    let duplicate=XML.replace("</fareProducts>","<PreassignedFareProduct id=\"product:adult\" version=\"1\"><Name><![CDATA[Other <adult>]]></Name></PreassignedFareProduct></fareProducts>");
    let parsed = netex::parse(duplicate.as_bytes()).unwrap();
    let products: Vec<_> = parsed
        .entities
        .iter()
        .filter(|e| e.reference == "product:adult")
        .collect();
    assert_eq!(products.len(), 2);
    assert_ne!(products[0].identity, products[1].identity);
    assert_eq!(products[1].name.as_deref(), Some("Other <adult>"));
    assert!(netex::parse(XML.replace("2.00", "NaN").as_bytes()).is_err());
    assert!(
        netex::parse(b"<!DOCTYPE x [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]><x>&secret;</x>")
            .is_err()
    );
    assert!(netex::parse(b"<PublicationDelivery/>").is_err());
}

#[test]
fn unsafe_members_and_unrecognized_databases_are_rejected() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bad.zip");
    fs::write(&path, archive(&[("../escape.xml", XML.as_bytes())])).unwrap();
    let mut conn = db::connect(&temp.path().join("db.sqlite")).unwrap();
    assert!(apply(&mut conn, &temp, &path).is_err());
    let other = temp.path().join("other.sqlite");
    rusqlite::Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE private(x)")
        .unwrap();
    assert!(db::connect(&other).is_err());
}

#[test]
fn readers_keep_a_snapshot_during_publication_and_backups_restore() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("db.sqlite");
    let path = temp.path().join("source.xml");
    fs::write(&path, XML).unwrap();
    let mut writer = db::connect(&database).unwrap();
    apply(&mut writer, &temp, &path).unwrap();
    let reader = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(
        reader
            .query_row("SELECT generation FROM dataset", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    fs::write(&path, XML.replace("2.00", "2.50")).unwrap();
    apply(&mut writer, &temp, &path).unwrap();
    assert_eq!(
        reader
            .query_row(
                "SELECT amount FROM entities WHERE ref='price:adult'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "2.00"
    );
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(
        reader
            .query_row(
                "SELECT amount FROM entities WHERE ref='price:adult'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "2.50"
    );
    let backup = temp.path().join("backup.sqlite");
    writer.backup(rusqlite::MAIN_DB, &backup, None).unwrap();
    assert_eq!(
        db::connect(&backup)
            .unwrap()
            .query_row("SELECT generation FROM dataset", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn sqlite_full_and_low_space_leave_the_previous_generation_intact() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.xml");
    fs::write(&path, XML).unwrap();
    let mut conn = db::connect(&temp.path().join("db.sqlite")).unwrap();
    apply(&mut conn, &temp, &path).unwrap();
    assert!(import::import(&mut conn, &path, temp.path(), temp.path(), u64::MAX).is_err());
    let pages: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    conn.pragma_update(None, "max_page_count", pages).unwrap();
    let many=(0..5000).map(|i|format!("<PreassignedFareProduct id=\"extra:{i}\"><Name>More products</Name></PreassignedFareProduct>")).collect::<String>();
    fs::write(
        &path,
        XML.replace("</fareProducts>", &(many + "</fareProducts>")),
    )
    .unwrap();
    assert!(apply(&mut conn, &temp, &path).is_err());
    assert_eq!(
        conn.query_row("SELECT generation FROM dataset", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM entities WHERE ref LIKE 'extra:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn bods_revision_archive_renames_reuse_the_dataset_namespace() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bulk.zip");
    let nested = archive(&[("adult.xml", XML.as_bytes())]);
    let mut conn = db::connect(&temp.path().join("db.sqlite")).unwrap();
    fs::write(
        &path,
        archive(&[(
            "Operator_1/123_100_2026-11-01_00-00-00_current.zip",
            &nested,
        )]),
    )
    .unwrap();
    apply(&mut conn, &temp, &path).unwrap();
    fs::write(
        &path,
        archive(&[(
            "Operator_1/123_101_2026-11-02_00-00-00_current.zip",
            &nested,
        )]),
    )
    .unwrap();
    let renamed = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(renamed.documents_skipped, 1);
    assert_eq!(renamed.documents_inserted, 0);
    assert_eq!(renamed.documents_deleted, 0);
    assert_eq!(renamed.sqlite_changes, 1);
    assert_eq!(renamed.generation, 1);
    let changed = archive(&[("adult.xml", XML.replace("2.00", "2.50").as_bytes())]);
    fs::write(
        &path,
        archive(&[(
            "Operator_1/123_102_2026-11-03_00-00-00_current.zip",
            &changed,
        )]),
    )
    .unwrap();
    let update = apply(&mut conn, &temp, &path).unwrap();
    assert_eq!(update.documents_updated, 1);
    assert_eq!(update.entities_updated, 2);
    assert_eq!(update.entities_inserted, 0);
}

#[cfg(unix)]
#[test]
fn unchanged_nested_archives_need_no_scratch_writes() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bulk.zip");
    let nested = archive(&[("adult.xml", XML.as_bytes())]);
    let mut conn = db::connect(&temp.path().join("db.sqlite")).unwrap();
    fs::write(
        &path,
        archive(&[("nested.zip", &nested), ("direct.xml", XML.as_bytes())]),
    )
    .unwrap();
    apply(&mut conn, &temp, &path).unwrap();
    fs::write(
        &path,
        archive(&[
            ("nested.zip", &nested),
            ("direct.xml", XML.replace("2.00", "2.50").as_bytes()),
        ]),
    )
    .unwrap();
    let scratch = temp.path().join("readonly");
    fs::create_dir(&scratch).unwrap();
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o555)).unwrap();
    let report = import::import(&mut conn, &path, &scratch, temp.path(), 0).unwrap();
    assert_eq!(report.archives_skipped, 1);
    assert_eq!(report.entities_updated, 2);
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
}
