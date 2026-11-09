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

const CATALOGUE: &str = include_str!("../../../tests/fixtures/netex/catalogue.xml");
fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}
fn public_ids(conn: &rusqlite::Connection, kind: &str) -> Vec<String> {
    conn.prepare("SELECT id FROM catalogue_objects WHERE kind=? AND active=1 ORDER BY id")
        .unwrap()
        .query_map([kind], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
#[test]
fn catalogue_direct_access_deduplicates_membership_and_keeps_ticket_semantics() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.zip");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    fs::write(
        &path,
        archive(&[
            ("a.xml", CATALOGUE.as_bytes()),
            (
                "b.xml",
                CATALOGUE.replace("00:00:01Z", "00:00:09Z").as_bytes(),
            ),
        ]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 1);
    assert_eq!(public_ids(&c, "ticket").len(), 3);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_edges WHERE relation='area'"
        ),
        2
    );
    assert_eq!(
        count(
            &c,
            "SELECT ticket_count FROM catalogue_objects WHERE kind='area'"
        ),
        2
    );
    assert_eq!(
        count(
            &c,
            "SELECT source_count FROM catalogue_objects WHERE kind='ticket' AND name='County Day'"
        ),
        2
    );
    assert_eq!(count(&c, "SELECT count(*) FROM catalogue_stops"), 2);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND json_extract(summary,'$.travel')=0"
        ),
        1
    );
    let before = public_ids(&c, "ticket");
    let changed = CATALOGUE.replace("6.00", "6.50");
    fs::write(
        &path,
        archive(&[("a.xml", changed.as_bytes()), ("b.xml", changed.as_bytes())]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "ticket"), before);
    assert_eq!(apply(&mut c, &temp, &path).unwrap().sqlite_changes, 0);
    fs::write(
        &path,
        archive(&[
            ("a.xml", changed.as_bytes()),
            ("b.xml", CATALOGUE.as_bytes()),
        ]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "ticket").len(), 4);
    assert_eq!(
        count(
            &c,
            "SELECT ticket_count FROM catalogue_objects WHERE kind='area' AND active=1"
        ),
        3
    );
    assert!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_events WHERE reason='split'"
        ) > 0
    );
    fs::write(
        &path,
        archive(&[("a.xml", changed.as_bytes()), ("b.xml", changed.as_bytes())]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "ticket").len(), 3);
    assert!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_events WHERE reason='merge'"
        ) > 0
    );
    fs::write(&path, archive(&[("b.xml", changed.as_bytes())])).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "ticket").len(), 3);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_sources WHERE document_id NOT IN (SELECT id FROM documents)"
        ),
        0
    );
    assert_eq!(apply(&mut c, &temp, &path).unwrap().sqlite_changes, 0);
}
#[test]
fn catalogue_eligibility_channels_validity_and_coverage_do_not_prove_equivalence() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.zip");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let child = CATALOGUE.replace("<UserType>adult</UserType>", "<UserType>child</UserType>");
    let channel = CATALOGUE.replace("onBoard", "online");
    let date = CATALOGUE.replace("2027-10-31", "2028-10-31");
    let duration = CATALOGUE.replace("P1D", "P2D");
    let price_date=CATALOGUE.replace("<FareFrame id=\"prices\" version=\"1\">","<FareFrame id=\"prices\" version=\"1\"><ValidBetween><FromDate>2027-01-01T00:00:00Z</FromDate></ValidBetween>");
    fs::write(
        &path,
        archive(&[
            ("base.xml", CATALOGUE.as_bytes()),
            ("child.xml", child.as_bytes()),
            ("channel.xml", channel.as_bytes()),
            ("date.xml", date.as_bytes()),
            ("duration.xml", duration.as_bytes()),
            ("price-date.xml", price_date.as_bytes()),
        ]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='family' AND active=1 AND name='County Day'"
        ),
        1
    );
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND active=1 AND name='County Day'"
        ),
        6
    );
    assert_eq!(public_ids(&c, "area").len(), 1);
}
#[test]
fn catalogue_ambiguous_versions_types_scopes_and_incomplete_membership_are_explicit() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let ambiguous=CATALOGUE.replace("<FareZoneRef ref=\"day-zone\" version=\"1\"/>","<FareZoneRef ref=\"day-zone\"/>").replace("</fareZones>","<FareZone id=\"day-zone\" version=\"2\"><Name>Other</Name><members><ScheduledStopPointRef ref=\"atco:S3\"/></members></FareZone></fareZones>");
    fs::write(&path, &ambiguous).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_edges e JOIN catalogue_objects t ON t.id=e.source_id WHERE e.relation='area' AND t.name='County Day'"
        ),
        0
    );
    let detail:String=c.query_row("SELECT detail FROM catalogue_objects WHERE kind='ticket' AND name='County Day' AND active=1",[],|r|r.get(0)).unwrap();
    assert!(detail.contains("ambiguous:FareZoneRef:day-zone"));
    fs::write(
        &path,
        ambiguous.replace(
            "<FareZoneRef ref=\"day-zone\"/>",
            "<FareZoneRef ref=\"day-zone\" version=\"1\"/>",
        ),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 1);
    let version_ref = ambiguous.replace(
        "<FareZoneRef ref=\"day-zone\"/>",
        "<FareZoneRef ref=\"day-zone\" versionRef=\"2\"/>",
    );
    fs::write(&path, version_ref).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 2);
    let incomplete = CATALOGUE.replace("explicitStops", "implicit");
    fs::write(&path, incomplete).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 2);
    let empty = CATALOGUE
        .replace("<ScheduledStopPointRef ref=\"atco:S1\"/>", "")
        .replace("<ScheduledStopPointRef ref=\"atco:S2\"/>", "");
    fs::write(&path, empty).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 2);
}
#[test]
fn catalogue_direction_public_ids_and_atomic_rollback() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    fs::write(&path, XML).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 1);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_journeys a JOIN catalogue_journeys b ON a.origin_id=b.destination_id AND a.destination_id=b.origin_id"
        ),
        0
    );
    let before = public_ids(&c, "ticket");
    let area = public_ids(&c, "point");
    c.execute_batch("CREATE TEMP TRIGGER reject_catalogue BEFORE INSERT ON catalogue_journeys BEGIN SELECT RAISE(ABORT,'projection failure'); END;").unwrap();
    fs::write(&path, XML.replace("2.00", "3.00")).unwrap();
    assert!(apply(&mut c, &temp, &path).is_err());
    assert_eq!(count(&c, "SELECT generation FROM dataset"), 1);
    assert_eq!(
        c.query_row("SELECT amount FROM catalogue_journeys", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "2.00"
    );
    c.execute_batch("DROP TRIGGER reject_catalogue").unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "ticket"), before);
    assert_eq!(public_ids(&c, "point"), area);
    assert_eq!(apply(&mut c, &temp, &path).unwrap().sqlite_changes, 0);
    let mut fresh = db::connect(&temp.path().join("fresh.sqlite")).unwrap();
    apply(&mut fresh, &temp, &path).unwrap();
    // First-publication semantic IDs are deterministic, independently of row IDs.
    assert_eq!(public_ids(&fresh, "point"), area);
}
#[test]
fn catalogue_real_operator_structures() {
    let temp = TempDir::new().unwrap();
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let path = temp.path().join("feed.zip");
    fs::write(
        &path,
        include_bytes!("../../../tests/fixtures/netex/catalogue-anea.zip"),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='area' AND active=1"
        ),
        1
    );
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND active=1 AND name IN ('AD Durham Day Pass','AD Durham WK1 Pass')"
        ),
        2
    );
    assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 6070);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_stops s JOIN catalogue_objects o ON o.id=s.object_id WHERE o.kind='area'"
        ),
        40
    );
    for bytes in [
        include_bytes!("../../../tests/fixtures/netex/catalogue-nuventure.zip").as_slice(),
        include_bytes!("../../../tests/fixtures/netex/catalogue-stagecoach.zip").as_slice(),
    ] {
        fs::write(&path, bytes).unwrap();
        apply(&mut c, &temp, &path).unwrap();
        assert!(!public_ids(&c, "ticket").is_empty());
        let support: String = c
            .query_row(
                "SELECT summary FROM catalogue_objects WHERE kind='ticket' AND active=1 LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let support: serde_json::Value = serde_json::from_str(&support).unwrap();
        if support["product_type"] == "singleTrip" {
            assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 496);
            assert_eq!(support["product_type_source"], "TypeOfFareProductRef");
        } else {
            assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 0);
            assert_eq!(
                support["journey_support"]["status"],
                "not_supported_or_not_journey_dependent"
            );
        }

        assert_eq!(
            count(&c, "SELECT count(*) FROM pragma_foreign_key_check"),
            0
        );
    }
}

#[test]
fn catalogue_backfill_does_not_rewrite_unchanged_source_records() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    fs::write(&path, CATALOGUE).unwrap();
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    let ids = public_ids(&c, "ticket");
    c.execute("UPDATE dataset SET catalogue_version=0", [])
        .unwrap();
    c.execute("UPDATE documents SET updated_at='sentinel'", [])
        .unwrap();
    let report = apply(&mut c, &temp, &path).unwrap();
    assert_eq!(report.documents_updated, 0);
    assert_eq!(report.documents_skipped, 1);
    assert_eq!(report.generation, 2);
    assert_eq!(public_ids(&c, "ticket"), ids);
    let timestamp: String = c
        .query_row("SELECT updated_at FROM documents", [], |r| r.get(0))
        .unwrap();
    assert_eq!(timestamp, "sentinel");
    assert_eq!(apply(&mut c, &temp, &path).unwrap().sqlite_changes, 0);
    let legacy = temp.path().join("legacy.sqlite");
    let l = rusqlite::Connection::open(&legacy).unwrap();
    l.execute_batch(include_str!(
        "../../../packages/fares-db/migrations/001.sql"
    ))
    .unwrap();
    drop(l);
    let migrated = db::connect(&legacy).unwrap();
    assert_eq!(count(&migrated, "PRAGMA user_version"), 3);
    assert_eq!(count(&migrated, "SELECT catalogue_version FROM dataset"), 0);
}

#[test]
fn catalogue_sales_contexts_are_independent_and_keep_urls_when_reordered() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let online = "<FareTable id=\"day-online\" version=\"1\"><pricesFor><PreassignedFareProductRef ref=\"day\" version=\"1\"/></pricesFor><specifics><DistributionChannelType>online</DistributionChannelType></specifics><cells><Cell id=\"online-cell\"><TimeIntervalPrice id=\"online-price\"><Amount>6.00</Amount></TimeIntervalPrice></Cell></cells></FareTable>";
    let xml = CATALOGUE.replace("</fareTables>", &format!("{online}</fareTables>"));
    fs::write(&path, &xml).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE name='County Day' AND kind='ticket' AND active=1"
        ),
        2
    );
    let before = public_ids(&c, "ticket");
    let moved = CATALOGUE.replace("<fareTables>", &format!("<fareTables>{online}"));
    fs::write(&path, moved).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "ticket"), before);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND active=1 AND family_id IS NULL"
        ),
        0
    );
}

#[test]
fn catalogue_shared_tariff_does_not_grant_sibling_products_areas() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let xml=CATALOGUE.replace("<Name>County</Name><members><ScheduledStopPointRef ref=\"atco:S2\"/><ScheduledStopPointRef ref=\"atco:S1\"/></members>","<Name>Other County</Name><members><ScheduledStopPointRef ref=\"atco:S3\"/><ScheduledStopPointRef ref=\"atco:S4\"/></members>").replace("</pricesFor>","</pricesFor><usedIn><TariffRef ref=\"tariff\" version=\"1\"/></usedIn>");
    fs::write(&path, xml).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 2);
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_edges e JOIN catalogue_objects o ON o.id=e.source_id WHERE e.relation='area' AND o.name='County Day'"
        ),
        1
    );
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_edges e JOIN catalogue_objects o ON o.id=e.source_id WHERE e.relation='area' AND o.name='County Week'"
        ),
        1
    );
}

#[test]
fn catalogue_removed_updated_identity_reactivates_without_changing_url() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.zip");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    fs::write(
        &path,
        archive(&[("a.xml", CATALOGUE.as_bytes()), ("b.xml", XML.as_bytes())]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    let id:String=c.query_row("SELECT id FROM catalogue_objects WHERE name='County Day' AND kind='ticket' AND active=1",[],|r|r.get(0)).unwrap();
    let changed = CATALOGUE.replace("6.00", "6.50");
    fs::write(
        &path,
        archive(&[("a.xml", changed.as_bytes()), ("b.xml", XML.as_bytes())]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    fs::write(&path, archive(&[("b.xml", XML.as_bytes())])).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT active FROM catalogue_objects WHERE id=?",
            [&id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    fs::write(
        &path,
        archive(&[("a.xml", changed.as_bytes()), ("b.xml", XML.as_bytes())]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    let restored:String=c.query_row("SELECT id FROM catalogue_objects WHERE name='County Day' AND kind='ticket' AND active=1",[],|r|r.get(0)).unwrap();
    assert_eq!(id, restored);
    assert!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_events WHERE reason='restored'"
        ) > 0
    );
    assert_eq!(apply(&mut c, &temp, &path).unwrap().sqlite_changes, 0);
}

#[test]
fn catalogue_direct_matrix_amount_inherits_cached_frame_currency() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let xml = XML.replace(
        "<GeographicalIntervalPriceRef ref=\"price:adult\"/>",
        "<Amount>3.25</Amount>",
    );
    fs::write(&path, xml).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    let value: (String, String) = c
        .query_row("SELECT amount,currency FROM catalogue_journeys", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(value, ("3.25".into(), "GBP".into()));
}

#[test]
fn catalogue_multiple_versions_do_not_take_identity_from_source_order() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let product = |version: &str, duration: &str| {
        format!(
            "<PreassignedFareProduct id=\"same\" version=\"{version}\"><Name>Same pass</Name><ProductType>periodPass</ProductType><validableElements><ValidableElement id=\"travel-{version}\"><UsageValidityPeriod id=\"duration-{version}\"><StandardDuration>{duration}</StandardDuration></UsageValidityPeriod></ValidableElement></validableElements></PreassignedFareProduct>"
        )
    };
    let xml = |products: String| {
        format!(
            "<PublicationDelivery xmlns=\"http://www.netex.org.uk/netex\"><dataObjects><CompositeFrame id=\"scope\"><Operator id=\"noc:ONE\"><PublicCode>ONE</PublicCode></Operator><fareProducts>{products}</fareProducts></CompositeFrame></dataObjects></PublicationDelivery>"
        )
    };
    fs::write(&path, xml(product("1", "P1D") + &product("2", "P2D"))).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    let mappings = |c: &rusqlite::Connection| {
        c.prepare("SELECT source_version,object_id FROM catalogue_sources WHERE kind='ticket' ORDER BY source_version").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>()
    };
    let before = mappings(&c);
    fs::write(&path, xml(product("2", "P4D") + &product("1", "P3D"))).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(mappings(&c), before);
    assert_eq!(
        count(
            &c,
            "SELECT source_count FROM catalogue_objects WHERE kind='family' AND active=1"
        ),
        2
    );
    let mut duplicates = db::connect(&temp.path().join("duplicates.sqlite")).unwrap();
    fs::write(&path, xml(product("1", "P1D") + &product("1", "P2D"))).unwrap();
    apply(&mut duplicates, &temp, &path).unwrap();
    let ambiguous_before = public_ids(&duplicates, "ticket");
    assert_eq!(
        count(
            &duplicates,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND active=1 AND json_extract(summary,'$.resolution')='unresolved'"
        ),
        2
    );
    fs::write(&path, xml(product("1", "P4D") + &product("1", "P3D"))).unwrap();
    apply(&mut duplicates, &temp, &path).unwrap();
    let ambiguous_after = public_ids(&duplicates, "ticket");
    assert!(
        ambiguous_after
            .iter()
            .all(|id| !ambiguous_before.contains(id))
    );
}

#[test]
fn catalogue_bundle_context_does_not_grant_other_components_areas() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let xml=CATALOGUE.replace("<Name>County</Name><members><ScheduledStopPointRef ref=\"atco:S2\"/><ScheduledStopPointRef ref=\"atco:S1\"/></members>","<Name>Other County</Name><members><ScheduledStopPointRef ref=\"atco:S3\"/><ScheduledStopPointRef ref=\"atco:S4\"/></members>").replace("</distributionAssignments></SalesOfferPackage>","</distributionAssignments><salesOfferPackageElements><SalesOfferPackageElement id=\"bundle\"><PreassignedFareProductRef ref=\"week\" version=\"1\"/></SalesOfferPackageElement></salesOfferPackageElements></SalesOfferPackage>");
    fs::write(&path, xml).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_edges e JOIN catalogue_objects o ON o.id=e.source_id WHERE e.relation='area' AND o.name='County Day'"
        ),
        1
    );
    let detail:String=c.query_row("SELECT detail FROM catalogue_objects WHERE kind='ticket' AND name='County Day' AND active=1",[],|r|r.get(0)).unwrap();
    assert!(detail.contains("unsupported_multi_product_sales_context"));
}

#[test]
fn catalogue_unknown_namespaced_conditions_are_preserved_and_never_equated() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feed.zip");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let extension = |value: &str| {
        CATALOGUE.replace("<ProductType>periodPass</ProductType>",&format!("<ProductType>periodPass</ProductType><x:Condition xmlns:x=\"urn:test:fares\">Only <x:passenger>{value}</x:passenger> permitted</x:Condition>"))
    };
    fs::write(
        &path,
        archive(&[
            ("a.xml", extension("adult").as_bytes()),
            ("b.xml", extension("child").as_bytes()),
        ]),
    )
    .unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND name='County Day' AND active=1"
        ),
        2
    );
    assert_eq!(
        count(
            &c,
            "SELECT count(*) FROM catalogue_objects WHERE kind='ticket' AND name='County Day' AND active=1 AND json_extract(summary,'$.resolution')='unresolved'"
        ),
        2
    );
    let detail:String=c.query_row("SELECT detail FROM catalogue_objects WHERE kind='ticket' AND name='County Day' LIMIT 1",[],|r|r.get(0)).unwrap();
    assert!(detail.contains("unsupported_extension:urn:test:fares:Condition"));
}

#[test]
fn catalogue_matrix_bindings_are_unique_and_context_directions_stay_separate() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let multiple=XML.replace("<GeographicalIntervalPriceRef ref=\"price:adult\"/>","<GeographicalIntervalPriceRef ref=\"price:adult\"/><GeographicalIntervalPriceRef ref=\"price:adult\"/>");
    fs::write(&path, multiple).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 0);
    let detail:String=c.query_row("SELECT detail FROM catalogue_objects WHERE kind='ticket' AND name='Adult single' AND active=1",[],|r|r.get(0)).unwrap();
    assert!(detail.contains("ambiguous_price_reference"));
    let multi_line=XML.replace("</lines>","<Line id=\"line:2\"><Name>Other line</Name><PublicCode>2</PublicCode><OperatorRef ref=\"noc:ONE\"/></Line></lines>").replace("</specifics>","<LineRef ref=\"line:2\"/></specifics>");
    fs::write(&path, multi_line).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 0);
    let start = XML.find("<FareTable id=\"table:adult\">").unwrap();
    let end = XML[start..].find("</FareTable>").unwrap() + start + "</FareTable>".len();
    let table = &XML[start..end];
    let inbound = table.replace(
        "</specifics>",
        "<DirectionType>inbound</DirectionType></specifics>",
    );
    let outbound = table
        .replace("table:adult", "table:out")
        .replace("cell:AB", "cell:out")
        .replace("fare:AB", "fare:out")
        .replace(
            "</specifics>",
            "<DirectionType>outbound</DirectionType></specifics>",
        );
    fs::write(&path, XML.replace(table, &(inbound + &outbound))).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(count(&c, "SELECT count(*) FROM catalogue_journeys"), 2);
    assert_eq!(
        count(
            &c,
            "SELECT count(DISTINCT route_id) FROM catalogue_journeys"
        ),
        2
    );
}

#[test]
fn catalogue_shared_zone_access_contexts_do_not_depend_on_first_product() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("source.xml");
    let mut c = db::connect(&temp.path().join("db.sqlite")).unwrap();
    let xml=CATALOGUE.replace("<FareZoneRef ref=\"week-zone\" version=\"1\"/>","<FareZoneRef ref=\"day-zone\" version=\"1\"/>").replace("<FareFrame id=\"network\"","<ServiceFrame id=\"lines\"><lines><Line id=\"line1\" version=\"1\"><PublicCode>1</PublicCode><OperatorRef ref=\"noc:ONE\"/></Line><Line id=\"line2\" version=\"1\"><PublicCode>2</PublicCode><OperatorRef ref=\"noc:ONE\"/></Line></lines></ServiceFrame><FareFrame id=\"network\"").replace("<FareTable id=\"day-price\" version=\"1\">","<FareTable id=\"day-price\" version=\"1\"><specifics><LineRef ref=\"line1\" version=\"1\"/></specifics>").replace("<FareTable id=\"week-price\" version=\"1\">","<FareTable id=\"week-price\" version=\"1\"><specifics><LineRef ref=\"line2\" version=\"1\"/></specifics>");
    fs::write(&path, &xml).unwrap();
    apply(&mut c, &temp, &path).unwrap();
    assert_eq!(public_ids(&c, "area").len(), 2);
    let before = public_ids(&c, "area");
    let product_block = |name: &str| {
        let start = xml
            .find(&format!("<PreassignedFareProduct id=\"{name}\""))
            .unwrap();
        let end = start
            + xml[start..].find("</PreassignedFareProduct>").unwrap()
            + "</PreassignedFareProduct>".len();
        xml[start..end].to_string()
    };
    let day = product_block("day");
    let week = product_block("week");
    let reordered = xml
        .replace(&day, "__DAY__")
        .replace(&week, &day)
        .replace("__DAY__", &week);
    let mut fresh = db::connect(&temp.path().join("fresh.sqlite")).unwrap();
    fs::write(&path, reordered).unwrap();
    apply(&mut fresh, &temp, &path).unwrap();
    assert_eq!(public_ids(&fresh, "area"), before);
}
