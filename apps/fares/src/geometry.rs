//! Approximate published stop coverage, never an official fare boundary.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};

pub const VERSION: i64 = 1;
type Point = [f64; 2];
fn code(value: &str) -> &str {
    for prefix in ["atco:", "naptan:"] {
        if value
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        {
            return &value[prefix.len()..];
        }
    }
    value
}
fn coordinate(lon: &str, lat: &str) -> Option<Point> {
    let lon = lon.trim().parse::<f64>().ok()?;
    let lat = lat.trim().parse::<f64>().ok()?;
    (lon.is_finite() && lat.is_finite() && lon.abs() <= 180.0 && lat.abs() <= 90.0).then_some([
        if lon == 0.0 { 0.0 } else { lon },
        if lat == 0.0 { 0.0 } else { lat },
    ])
}
pub fn open_timetable(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("open timetable coordinates read-only")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-16384; BEGIN")?;
    ensure!(
        conn.query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))? == 0x4255534C,
        "not a Buslane timetable database"
    );
    ensure!(
        (1..=2).contains(&conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?),
        "incompatible timetable schema"
    );
    ensure!(
        timetable_generation(&conn)? > 0,
        "timetable coordinates are not ready"
    );
    Ok(conn)
}
pub fn timetable_generation(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT generation FROM dataset WHERE id=1", [], |r| {
            r.get(0)
        })?,
    )
}
pub fn ready(conn: &Connection, fares: i64, timetable: i64) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM geometry_state WHERE id=1 AND algorithm_version=? AND fares_generation=? AND timetable_generation=?) AND NOT EXISTS(SELECT 1 FROM catalogue_objects o LEFT JOIN operator_geometry g ON g.noc=o.noc WHERE o.active=1 AND g.noc IS NULL)", [VERSION,fares,timetable], |r| r.get(0))?)
}
fn outline(points: &[Point]) -> Option<Vec<Point>> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    sorted.dedup();
    if sorted.len() < 3 {
        return None;
    }
    fn half(points: impl Iterator<Item = Point>) -> Vec<Point> {
        let mut hull: Vec<Point> = Vec::new();
        for p in points {
            while hull.len() >= 2 {
                let a = hull[hull.len() - 2];
                let b = hull[hull.len() - 1];
                if (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) > 0.0 {
                    break;
                }
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
        hull
    }
    let mut hull = half(sorted.iter().copied());
    hull.extend(half(sorted.iter().rev().copied()));
    if hull.len() < 3 {
        return None;
    }
    hull.push(hull[0]);
    Some(hull)
}
fn color(id: &str) -> &'static str {
    let hash = id
        .encode_utf16()
        .fold(0_u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
    [
        "#2563eb", "#dc2626", "#16a34a", "#9333ea", "#ea580c", "#0891b2", "#be185d", "#4f46e5",
        "#a16207", "#0f766e",
    ][hash as usize % 10]
}
#[derive(Default)]
struct Area {
    name: String,
    complete: bool,
    stops: BTreeSet<String>,
}
fn collection(features: Vec<Value>) -> Value {
    json!({"type":"FeatureCollection","features":features})
}
fn feature(properties: &Value, kind: &str, coordinates: Value) -> Value {
    json!({"type":"Feature","properties":properties,"geometry":{"type":kind,"coordinates":coordinates}})
}
fn publish(
    conn: &Connection,
    noc: &str,
    areas: &mut BTreeMap<String, Area>,
    locations: &BTreeMap<String, Point>,
    timetable: i64,
) -> Result<()> {
    let (mut polygons, mut points, mut labels, mut zones, mut missing) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut unmatched = BTreeSet::new();
    for (id, area) in std::mem::take(areas) {
        let mut coords = Vec::new();
        for stop in &area.stops {
            if let Some(p) = locations.get(stop) {
                coords.push(*p);
            } else {
                unmatched.insert(stop.clone());
            }
        }
        let hull = if area.complete {
            outline(&coords)
        } else {
            None
        };
        let properties =
            json!({"zoneId":id,"name":area.name,"color":color(&id),"approximate":hull.is_some()});
        if let Some(hull) = hull {
            polygons.push(feature(&properties, "Polygon", json!([hull])));
        }
        for p in &coords {
            points.push(feature(&properties, "Point", json!(p)));
        }
        if coords.is_empty() {
            missing.push(id.clone());
        } else {
            let count = coords.len() as f64;
            labels.push(feature(
                &properties,
                "Point",
                json!([
                    coords.iter().map(|p| p[0]).sum::<f64>() / count,
                    coords.iter().map(|p| p[1]).sum::<f64>() / count
                ]),
            ));
        }
        zones.push(
            json!({"id":id,"name":area.name,"membershipComplete":area.complete,
            "stopCount":area.stops.len(),"mappedStopCount":coords.len()}),
        );
    }
    // Draw broad envelopes behind local areas, as the current consumer does.
    fn size(feature: &Value) -> f64 {
        let ring = feature["geometry"]["coordinates"][0].as_array().unwrap();
        ring.windows(2)
            .map(|p| {
                p[0][0].as_f64().unwrap() * p[1][1].as_f64().unwrap()
                    - p[1][0].as_f64().unwrap() * p[0][1].as_f64().unwrap()
            })
            .sum::<f64>()
            .abs()
    }
    polygons.sort_by(|a, b| size(b).total_cmp(&size(a)));
    let body = serde_json::to_string(
        &json!({"areas":collection(polygons),"points":collection(points),
        "labels":collection(labels),"zones":zones,"missingAreaIds":missing,
        "unmatchedStopCodes":unmatched,"timetableGeneration":timetable,"approximate":true}),
    )?;
    conn.execute(
        "INSERT INTO operator_geometry(noc,body) VALUES(?,?)",
        params![noc, body],
    )?;
    Ok(())
}
pub fn generate(conn: &Connection, timetable: &Connection, generation: i64) -> Result<()> {
    ensure!(
        !conn.is_autocommit(),
        "geometry generation requires a transaction"
    );
    ensure!(
        !timetable.is_autocommit(),
        "timetable coordinates require a read snapshot"
    );
    let timetable_generation = timetable_generation(timetable)?;
    let mut locations = BTreeMap::<String, Point>::new();
    let mut stmt = timetable.prepare("SELECT public_id,lon,lat FROM stops ORDER BY public_id")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        if let Some(point) = coordinate(&row.get::<_, String>(1)?, &row.get::<_, String>(2)?) {
            let normalized = code(&id).to_string();
            // Refuse ambiguous aliases rather than select arbitrary coordinates.
            if let Some(previous) = locations.insert(normalized, point) {
                ensure!(
                    previous == point,
                    "conflicting timetable stop aliases for {id}"
                );
            }
        }
    }
    conn.execute("DELETE FROM operator_geometry", [])?;
    let mut stmt = conn.prepare(
        "SELECT n.noc,o.id,o.name,o.summary,s.code FROM
        (SELECT DISTINCT noc FROM catalogue_objects WHERE active=1) n
        LEFT JOIN catalogue_objects o ON o.noc=n.noc AND o.kind='area' AND o.active=1
        LEFT JOIN catalogue_stops s ON s.object_id=o.id ORDER BY n.noc,o.id,s.code",
    )?;
    let mut rows = stmt.query([])?;
    let mut noc: Option<String> = None;
    let mut areas = BTreeMap::<String, Area>::new();
    while let Some(row) = rows.next()? {
        let n: String = row.get(0)?;
        if noc.as_ref().is_some_and(|old| old != &n) {
            publish(
                conn,
                noc.as_ref().unwrap(),
                &mut areas,
                &locations,
                timetable_generation,
            )?;
        }
        noc = Some(n);
        if let Some(id) = row.get::<_, Option<String>>(1)? {
            if !areas.contains_key(&id) {
                let summary: Value = serde_json::from_str(&row.get::<_, String>(3)?)?;
                areas.insert(
                    id.clone(),
                    Area {
                        name: row.get(2)?,
                        complete: summary["membership"] == "published_stops",
                        ..Default::default()
                    },
                );
            }
            if let Some(stop) = row.get::<_, Option<String>>(4)? {
                areas
                    .get_mut(&id)
                    .unwrap()
                    .stops
                    .insert(code(&stop).to_string());
            }
        }
    }
    if let Some(noc) = noc {
        publish(conn, &noc, &mut areas, &locations, timetable_generation)?;
    }
    conn.execute("INSERT INTO geometry_state VALUES(1,?,?,?) ON CONFLICT(id) DO UPDATE SET algorithm_version=excluded.algorithm_version,fares_generation=excluded.fares_generation,timetable_generation=excluded.timetable_generation",[VERSION,generation,timetable_generation])?;
    Ok(())
}
pub fn rebuild(
    conn: &mut Connection,
    timetable: &Connection,
    disk: &Path,
    minimum: u64,
) -> Result<i64> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (generation, version): (i64, i64) = tx.query_row(
        "SELECT generation,catalogue_version FROM dataset WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        generation > 0 && version == crate::catalogue::VERSION,
        "fares catalogue is not ready"
    );
    generate(&tx, timetable, generation + 1)?;
    crate::source::check_disk(disk, minimum)?;
    tx.execute("UPDATE dataset SET generation=generation+1 WHERE id=1", [])?;
    tx.commit()?;
    Ok(generation + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    const XML: &str = include_str!("../../../tests/fixtures/netex/catalogue.xml");
    fn timetable(temp: &TempDir) -> Connection {
        let path = temp.path().join("timetables.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA application_id=0x4255534C; PRAGMA user_version=2;
            CREATE TABLE dataset(id INTEGER PRIMARY KEY,generation INTEGER); INSERT INTO dataset VALUES(1,7);
            CREATE TABLE stops(public_id TEXT PRIMARY KEY,lon TEXT,lat TEXT);
            INSERT INTO stops VALUES('S1','0','0'),('atco:S2','1','0'),('S3','0','1'),('BAD','NaN','0');").unwrap();
        drop(conn);
        open_timetable(&path).unwrap()
    }
    fn body(conn: &Connection) -> Value {
        let text: String = conn
            .query_row(
                "SELECT body FROM operator_geometry WHERE noc='ONE'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }
    fn apply(
        conn: &mut Connection,
        temp: &TempDir,
        tt: Option<&Connection>,
    ) -> Result<crate::import::Report> {
        crate::import::import_with_geometry(
            conn,
            &temp.path().join("feed.xml"),
            temp.path(),
            temp.path(),
            0,
            tt,
        )
    }
    #[test]
    fn validates_coordinates_and_degenerate_envelopes() {
        for lon in ["", "NaN", "Infinity", "181", "-181", "abc"] {
            assert_eq!(coordinate(lon, "0"), None);
        }
        for lat in ["", "91", "-91", "NaN"] {
            assert_eq!(coordinate("0", lat), None);
        }
        assert_eq!(coordinate(" -180 ", "90"), Some([-180.0, 90.0]));
        assert_eq!(code("NAPTAN:S1"), "S1");
        assert_eq!(outline(&[[0., 0.], [1., 1.], [2., 2.], [1., 1.]]), None);
        let hull = outline(&[[0., 0.], [1., 0.], [0., 1.], [0., 0.], [0.1, 0.1]]).unwrap();
        assert_eq!(hull.len(), 4);
        assert_eq!(hull.first(), hull.last());
    }
    #[test]
    fn map_keeps_zones_separate_deduplicates_aliases_and_reports_missing() {
        let temp = TempDir::new().unwrap();
        let tt = timetable(&temp);
        let mut conn = crate::db::connect(&temp.path().join("fares.sqlite")).unwrap();
        std::fs::write(temp.path().join("feed.xml"), XML).unwrap();
        apply(&mut conn, &temp, Some(&tt)).unwrap();
        let id: String = conn
            .query_row(
                "SELECT id FROM catalogue_objects WHERE kind='area' AND active=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO catalogue_stops VALUES(?,'S3',NULL),(?,'atco:S1',NULL)",
            params![id, id],
        )
        .unwrap();
        for (id, complete) in [("incomplete", false), ("missing", true), ("unmapped", true)] {
            conn.execute("INSERT INTO catalogue_objects(id,kind,noc,semantic_key,name,search,summary,detail) VALUES(?,'area','ONE',?,?,?,?,'{}')",
                params![id,id,id,id,serde_json::to_string(&json!({"membership":if complete {"published_stops"} else {"incomplete"}})).unwrap()]).unwrap();
        }
        conn.execute_batch("INSERT INTO catalogue_stops VALUES('incomplete','S1',NULL),('incomplete','S2',NULL),('incomplete','S3',NULL),('missing','BAD',NULL),('unmapped','S1',NULL),('unmapped','S2',NULL),('unmapped','S3',NULL),('unmapped','UNKNOWN',NULL);").unwrap();
        rebuild(&mut conn, &tt, temp.path(), 0).unwrap();
        let map = body(&conn);
        assert_eq!(map["zones"].as_array().unwrap().len(), 4);
        assert_eq!(map["areas"]["features"].as_array().unwrap().len(), 2);
        assert_eq!(map["points"]["features"].as_array().unwrap().len(), 9);
        assert_eq!(map["labels"]["features"].as_array().unwrap().len(), 3);
        assert_eq!(map["missingAreaIds"], json!(["missing"]));
        assert_eq!(map["unmatchedStopCodes"], json!(["BAD", "UNKNOWN"]));
        assert_eq!(map["timetableGeneration"], 7);
        let polygon = &map["areas"]["features"][0];
        assert_eq!(polygon["properties"]["zoneId"], id);
        assert_eq!(polygon["properties"]["approximate"], true);
        let zone = map["zones"]
            .as_array()
            .unwrap()
            .iter()
            .find(|z| z["id"] == id)
            .unwrap();
        assert_eq!(zone["stopCount"], 3);
        assert_eq!(zone["mappedStopCount"], 3);
    }
    #[test]
    fn publication_backfill_noop_and_read_snapshots() {
        let temp = TempDir::new().unwrap();
        let tt = timetable(&temp);
        let path = temp.path().join("fares.sqlite");
        let mut conn = crate::db::connect(&path).unwrap();
        std::fs::write(temp.path().join("feed.xml"), XML).unwrap();
        apply(&mut conn, &temp, None).unwrap();
        assert!(!ready(&conn, 1, 7).unwrap());
        apply(&mut conn, &temp, Some(&tt)).unwrap();
        assert!(ready(&conn, 2, 7).unwrap());
        assert_eq!(
            apply(&mut conn, &temp, Some(&tt)).unwrap().sqlite_changes,
            0
        );
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let old = body(&reader);
        // A committed timetable update cannot alter our pinned coordinate snapshot.
        let writer = Connection::open(temp.path().join("timetables.sqlite")).unwrap();
        writer
            .execute_batch(
                "UPDATE stops SET lon='10' WHERE public_id='S1'; UPDATE dataset SET generation=8",
            )
            .unwrap();
        rebuild(&mut conn, &tt, temp.path(), 0).unwrap();
        assert_eq!(body(&conn), old);
        assert!(ready(&conn, 3, 7).unwrap());
        drop(tt);
        let tt = open_timetable(&temp.path().join("timetables.sqlite")).unwrap();
        let report = apply(&mut conn, &temp, Some(&tt)).unwrap();
        assert_eq!(report.generation, 4);
        assert!(ready(&conn, 4, 8).unwrap());
        assert_ne!(body(&conn), old);
        assert_eq!(body(&reader), old);
        reader.execute_batch("COMMIT").unwrap();
        assert_eq!(body(&reader), body(&conn));
        // Geometry failure rolls back both the fare edit and the new generation.
        conn.execute_batch("CREATE TRIGGER fail_map BEFORE INSERT ON operator_geometry BEGIN SELECT RAISE(ABORT,'map generation failed'); END;").unwrap();
        std::fs::write(temp.path().join("feed.xml"), XML.replace("6.00", "7.00")).unwrap();
        assert!(apply(&mut conn, &temp, Some(&tt)).is_err());
        assert!(ready(&conn, 4, 8).unwrap());
        assert!(rebuild(&mut conn, &tt, temp.path(), 0).is_err());
        assert!(ready(&conn, 4, 8).unwrap());
    }
    #[test]
    fn schema_two_upgrade_and_ambiguous_coordinates() {
        let temp = TempDir::new().unwrap();
        let tt = timetable(&temp);
        let path = temp.path().join("fares.sqlite");
        let mut conn = crate::db::connect(&path).unwrap();
        std::fs::write(temp.path().join("feed.xml"), XML).unwrap();
        apply(&mut conn, &temp, None).unwrap();
        conn.execute_batch(
            "DROP TABLE operator_geometry; DROP TABLE geometry_state; PRAGMA user_version=2",
        )
        .unwrap();
        drop(conn);
        conn = crate::db::connect(&path).unwrap();
        rebuild(&mut conn, &tt, temp.path(), 0).unwrap();
        assert!(ready(&conn, 2, 7).unwrap());
        drop(tt);
        let writer = Connection::open(temp.path().join("timetables.sqlite")).unwrap();
        writer
            .execute("INSERT INTO stops VALUES('atco:S1','99','0')", [])
            .unwrap();
        let tt = open_timetable(&temp.path().join("timetables.sqlite")).unwrap();
        assert!(rebuild(&mut conn, &tt, temp.path(), 0).is_err());
        assert!(ready(&conn, 2, 7).unwrap());
    }
}
