//! Stop-pattern geometry, generated from a single ordered bulk scan.
use anyhow::{Result, ensure};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const ALGORITHM_VERSION: i64 = 1;
type Point = [f64; 2];

fn point(lon: Option<String>, lat: Option<String>) -> Option<Point> {
    let lon = lon?.trim().parse::<f64>().ok()?;
    let lat = lat?.trim().parse::<f64>().ok()?;
    (lon.is_finite()
        && lat.is_finite()
        && (-180.0..=180.0).contains(&lon)
        && (-90.0..=90.0).contains(&lat))
    .then_some([
        if lon == 0.0 { 0.0 } else { lon },
        if lat == 0.0 { 0.0 } else { lat },
    ])
}

#[derive(Default)]
struct Route {
    // JSON keys give deterministic ordering and deduplicate whole directed lines.
    lines: BTreeMap<String, Vec<Point>>,
    trips: usize,
    missing: bool,
    representatives: BTreeSet<(Option<i64>, i64, Option<String>)>,
}
impl Route {
    fn segment(&mut self, points: &mut Vec<Point>) -> Result<bool> {
        let line = std::mem::take(points);
        if line.len() < 2 {
            return Ok(false);
        }
        self.lines.insert(serde_json::to_string(&line)?, line);
        Ok(true)
    }
}
fn publish(
    conn: &Connection,
    agency: i64,
    generation: i64,
    routes: &mut BTreeMap<i64, Route>,
) -> Result<()> {
    let mut features = Vec::<Value>::new();
    let mut missing = Vec::new();
    for (id, route) in std::mem::take(routes) {
        if route.trips == 0 || route.missing {
            missing.push(id);
        }
        if !route.lines.is_empty() {
            features.push(json!({"type":"Feature","id":id,"properties":{"routeId":id},
                "geometry":{"type":"MultiLineString","coordinates":route.lines.into_values().collect::<Vec<_>>()}}));
        }
    }
    let body = serde_json::to_string(
        &json!({"network":{"type":"FeatureCollection","features":features},"missingRouteIds":missing}),
    )?;
    conn.execute(
        "INSERT INTO agency_geometry VALUES(?,?,?,?)",
        params![agency, generation, ALGORITHM_VERSION, body],
    )?;
    Ok(())
}

pub fn ready(conn: &Connection, generation: i64) -> Result<bool> {
    Ok(conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM agencies a LEFT JOIN agency_geometry g ON g.agency_id=a.id WHERE g.agency_id IS NULL OR g.generation!=? OR g.algorithm_version!=?)", [generation, ALGORITHM_VERSION], |r| r.get(0))?)
}

/// Caller owns the write transaction. Any query/serialization/write error aborts publication.
pub fn generate(conn: &Connection, generation: i64) -> Result<()> {
    ensure!(
        !conn.is_autocommit(),
        "geometry generation requires a transaction"
    );
    conn.execute("DELETE FROM agency_geometry", [])?;
    // Include agencies/routes without trips and trips without stop times. All stop
    // patterns are considered, including branches sharing a shape or no shape.
    let mut stmt = conn.prepare(
        "SELECT a.id,r.public_id,t.id,s.lon,s.lat,t.shape_id,t.direction_id,t.headsign
        FROM agencies a LEFT JOIN routes r ON r.agency_id=a.id
        LEFT JOIN trips t ON t.route_id=r.id
        LEFT JOIN stop_times st ON st.trip_id=t.id
        LEFT JOIN stops s ON s.id=st.stop_id
        ORDER BY a.id,r.public_id,t.public_id,st.stop_sequence",
    )?;
    let mut rows = stmt.query([])?;
    let mut agency = None;
    let mut routes = BTreeMap::<i64, Route>::new();
    let mut trip = None;
    let mut route_id = None;
    let mut points = Vec::new();
    let mut usable = false;
    let mut representative = false;
    while let Some(row) = rows.next()? {
        let a: i64 = row.get(0)?;
        let r: Option<i64> = row.get(1)?;
        let t: Option<i64> = row.get(2)?;
        if trip != t || agency != Some(a) || route_id != r {
            if trip.is_some() {
                let route = routes.get_mut(&route_id.unwrap()).unwrap();
                usable |= route.segment(&mut points)?;
                route.missing |= representative && !usable;
            }
            if agency.is_some() && agency != Some(a) {
                publish(conn, agency.unwrap(), generation, &mut routes)?;
            }
            points.clear();
            usable = false;
            trip = t;
            route_id = r;
            if let Some(id) = r {
                let route = routes.entry(id).or_default();
                if t.is_some() {
                    route.trips += 1;
                    // Public trip ordering matches the existing routes endpoint.
                    representative =
                        route
                            .representatives
                            .insert((row.get(5)?, row.get(6)?, row.get(7)?));
                }
            }
        }
        agency = Some(a);
        if let (Some(id), Some(_)) = (r, t) {
            match point(row.get(3)?, row.get(4)?) {
                Some(p) => {
                    if points.last() != Some(&p) {
                        points.push(p);
                    }
                }
                None => {
                    usable |= routes.get_mut(&id).unwrap().segment(&mut points)?;
                }
            }
        }
    }
    if trip.is_some() {
        let route = routes.get_mut(&route_id.unwrap()).unwrap();
        usable |= route.segment(&mut points)?;
        route.missing |= representative && !usable;
    }
    if let Some(a) = agency {
        publish(conn, a, generation, &mut routes)?;
    }
    Ok(())
}

/// Rebuild existing imports without downloading or reimporting source files.
pub fn rebuild(conn: &mut Connection) -> Result<i64> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let generation: i64 = tx.query_row("SELECT generation FROM dataset WHERE id=1", [], |r| {
        r.get(0)
    })?;
    ensure!(
        generation > 0,
        "import a timetable before rebuilding geometry"
    );
    generate(&tx, generation + 1)?;
    tx.execute("UPDATE dataset SET generation=generation+1 WHERE id=1", [])?;
    tx.commit()?;
    Ok(generation + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coordinates_require_finite_in_range_numbers() {
        for value in [
            "",
            " ",
            "NaN",
            "Infinity",
            "-Infinity",
            "181",
            "-181",
            "bad",
            "1junk",
        ] {
            assert_eq!(point(Some(value.into()), Some("0".into())), None);
        }
        for value in ["91", "-91", "NaN", ""] {
            assert_eq!(point(Some("0".into()), Some(value.into())), None);
        }
        assert_eq!(point(None, Some("0".into())), None);
        assert_eq!(point(Some("0".into()), None), None);
        assert_eq!(
            point(Some(" -180 ".into()), Some("90".into())),
            Some([-180.0, 90.0])
        );
        assert_eq!(
            point(Some("-0".into()), Some("0.0".into())),
            Some([0.0, 0.0])
        );
    }
}
