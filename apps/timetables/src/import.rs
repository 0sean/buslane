use crate::db::NORMALISATION_VERSION;
use crate::source::{Source, check_disk};
use anyhow::{Context, Result, bail, ensure};
use chrono::NaiveDate;
use lru::LruCache;
use roaring::RoaringTreemap;
use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Value};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Instant;

#[derive(Default, Debug, Serialize)]
pub struct Counts {
    pub inserted: u64,
    pub updated: u64,
    pub deleted: u64,
    pub unchanged: u64,
}
impl Counts {
    fn changes(&self) -> u64 {
        self.inserted + self.updated + self.deleted
    }
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub generation: i64,
    pub tables: BTreeMap<String, Counts>,
    pub elapsed_ms: u128,
    pub changed_files: usize,
}

pub fn import(
    conn: &mut Connection,
    source: &mut Source,
    disk: &Path,
    minimum: u64,
) -> Result<Report> {
    let start = Instant::now();
    let checksums = source.checksums()?;
    let mut changed = HashSet::new();
    for (name, hash) in &checksums {
        let previous: Option<(String, i64)> = conn
            .query_row(
                "SELECT sha256,normalisation_version FROM source_files WHERE name=?",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if previous != Some((hash.clone(), NORMALISATION_VERSION)) {
            changed.insert(name.clone());
        }
    }
    let generation: i64 = conn.query_row("SELECT generation FROM dataset WHERE id=1", [], |r| {
        r.get(0)
    })?;
    if changed.is_empty() {
        return Ok(Report {
            generation,
            tables: BTreeMap::new(),
            elapsed_ms: start.elapsed().as_millis(),
            changed_files: 0,
        });
    }
    check_disk(disk, minimum)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("PRAGMA defer_foreign_keys=ON")?;
    let mut importer = Importer {
        conn: &tx,
        seen: HashMap::new(),
        plans: HashMap::new(),
        counts: BTreeMap::new(),
        affected: BTreeSet::new(),
        cache: LruCache::new(NonZeroUsize::new(65536).unwrap()),
        relation_cache: LruCache::new(NonZeroUsize::new(65536).unwrap()),
        rows: 0,
        disk,
        minimum,
    };
    for name in ["agency.txt", "stops.txt", "routes.txt"] {
        if changed.contains(name) {
            importer.file(source, name)?;
        }
    }
    if changed.contains("calendar.txt") || changed.contains("calendar_dates.txt") {
        importer
            .seen
            .insert("services".into(), RoaringTreemap::new());
        importer.file(source, "calendar.txt")?;
        importer.file(source, "calendar_dates.txt")?;
    }
    if changed.contains("shapes.txt") {
        importer.file(source, "shapes.txt")?;
    }
    for name in ["trips.txt", "frequencies.txt", "stop_times.txt"] {
        if changed.contains(name) {
            importer.file(source, name)?;
        }
    }
    importer.delete_missing()?;
    importer.memberships()?;
    crate::db::validate_deferred_foreign_keys(&tx)?;
    let populated: i64 = tx.query_row("SELECT EXISTS(SELECT 1 FROM agencies) AND EXISTS(SELECT 1 FROM routes) AND EXISTS(SELECT 1 FROM trips) AND EXISTS(SELECT 1 FROM stops) AND EXISTS(SELECT 1 FROM stop_times)", [], |r| r.get(0))?;
    ensure!(
        populated == 1,
        "validation failed: required timetable tables are empty"
    );
    let mut generation = generation;
    // Cache generation changes only when publicly served records change.
    if importer.counts.iter().any(|(table, count)| {
        !matches!(table.as_str(), "shapes" | "shape_points") && count.changes() > 0
    }) || generation == 0
    {
        generation += 1;
        tx.execute("UPDATE dataset SET generation=?,imported_at=strftime('%Y-%m-%d %H:%M:%f','now'),normalisation_version=? WHERE id=1", [generation,NORMALISATION_VERSION])?;
    }
    for (name, hash) in &checksums {
        if changed.contains(name) {
            tx.execute("INSERT INTO source_files VALUES(?,?,?) ON CONFLICT(name) DO UPDATE SET sha256=excluded.sha256,normalisation_version=excluded.normalisation_version", rusqlite::params![name,hash,NORMALISATION_VERSION])?;
        }
    }
    let tables = std::mem::take(&mut importer.counts);
    drop(importer);
    check_disk(disk, minimum)?;
    tx.commit().context("commit validated timetable update")?;
    Ok(Report {
        generation,
        tables,
        elapsed_ms: start.elapsed().as_millis(),
        changed_files: changed.len(),
    })
}

struct SyncSql {
    select: String,
    updates: HashMap<u64, String>,
    insert: String,
}
struct Importer<'a> {
    conn: &'a Connection,
    seen: HashMap<String, RoaringTreemap>,
    plans: HashMap<String, SyncSql>,
    counts: BTreeMap<String, Counts>,
    affected: BTreeSet<i64>,
    cache: LruCache<(String, String), i64>,
    relation_cache: LruCache<(&'static str, i64), i64>,
    rows: usize,
    disk: &'a Path,
    minimum: u64,
}
impl Importer<'_> {
    fn file(&mut self, source: &mut Source, name: &str) -> Result<()> {
        let table = match name {
            "agency.txt" => "agencies",
            "stops.txt" => "stops",
            "routes.txt" => "routes",
            "calendar.txt" => "services",
            "calendar_dates.txt" => "calendar_dates",
            "shapes.txt" => "shape_points",
            "trips.txt" => "trips",
            "frequencies.txt" => "frequencies",
            "stop_times.txt" => "stop_times",
            _ => bail!("unknown file"),
        };
        self.seen.entry(table.into()).or_default();
        if name == "shapes.txt" {
            self.seen.entry("shapes".into()).or_default();
        }
        let start = Instant::now();
        let count = source
            .read(name, |reader| self.csv(reader, name))?
            .unwrap_or(0);
        eprintln!(
            "parsed {name}: {count} rows in {} ms",
            start.elapsed().as_millis()
        );
        Ok(())
    }
    fn csv(&mut self, reader: &mut dyn Read, name: &str) -> Result<usize> {
        let mut csv = csv::ReaderBuilder::new()
            .trim(csv::Trim::Headers)
            .from_reader(reader);
        let headers = csv.headers()?.clone();
        let mut unique = HashSet::new();
        for h in &headers {
            ensure!(unique.insert(h), "duplicate column {h} in {name}");
        }
        let columns: HashMap<&str, usize> = headers
            .iter()
            .enumerate()
            .map(|(i, h)| (h.trim_start_matches('\u{feff}'), i))
            .collect();
        let mut count = 0;
        for record in csv.records() {
            let record =
                record.with_context(|| format!("invalid CSV in {name}, record {}", count + 1))?;
            let row = Row {
                columns: &columns,
                record: &record,
            };
            self.record(name, &row)
                .with_context(|| format!("{name}, record {}", count + 1))?;
            count += 1;
            self.rows += 1;
            if self.rows.is_multiple_of(8192) {
                check_disk(self.disk, self.minimum)?;
            }
        }
        // Header-only optional files are valid; mandatory headers are checked even without records.
        let required: &[&str] = match name {
            "agency.txt" => &["agency_id", "agency_name", "agency_noc"],
            "stops.txt" => &["stop_id", "stop_name", "stop_lat", "stop_lon"],
            "routes.txt" => &["route_id", "agency_id", "route_type"],
            "calendar.txt" => &[
                "service_id",
                "start_date",
                "end_date",
                "monday",
                "tuesday",
                "wednesday",
                "thursday",
                "friday",
                "saturday",
                "sunday",
            ],
            "calendar_dates.txt" => &["service_id", "date", "exception_type"],
            "shapes.txt" => &[
                "shape_id",
                "shape_pt_lat",
                "shape_pt_lon",
                "shape_pt_sequence",
            ],
            "trips.txt" => &["trip_id", "route_id", "service_id", "vehicle_journey_code"],
            "frequencies.txt" => &["trip_id", "start_time", "end_time", "headway_secs"],
            "stop_times.txt" => &[
                "trip_id",
                "stop_id",
                "arrival_time",
                "departure_time",
                "stop_sequence",
            ],
            _ => &[],
        };
        for column in required {
            ensure!(
                columns.contains_key(column),
                "missing column {column} in {name}"
            );
        }
        Ok(count)
    }
    fn record(&mut self, name: &str, r: &Row<'_>) -> Result<()> {
        match name {
            "agency.txt" => {
                self.sync(
                    "agencies",
                    &[
                        "public_id",
                        "name",
                        "url",
                        "timezone",
                        "lang",
                        "phone",
                        "noc",
                    ],
                    vec![
                        r.text("agency_id")?,
                        r.text("agency_name")?,
                        r.optional("agency_url"),
                        r.optional("agency_timezone"),
                        r.optional("agency_lang"),
                        r.optional("agency_phone"),
                        r.text_allow_empty("agency_noc")?,
                    ],
                    &[0],
                )?;
            }
            "stops.txt" => {
                self.sync(
                    "stops",
                    &[
                        "public_id",
                        "code",
                        "name",
                        "lat",
                        "lon",
                        "wheelchair_boarding",
                        "location_type",
                        "platform_code",
                        "parent_station_id",
                    ],
                    vec![
                        r.text("stop_id")?,
                        r.optional("stop_code"),
                        r.text("stop_name")?,
                        r.decimal("stop_lat", false, Some((-90., 90.)))?,
                        r.decimal("stop_lon", false, Some((-180., 180.)))?,
                        r.access("wheelchair_boarding")?,
                        r.int_default("location_type", 0)?,
                        r.optional("platform_code"),
                        r.optional("parent_station"),
                    ],
                    &[0],
                )?;
            }
            "routes.txt" => {
                let agency = self.reference("agencies", r.required("agency_id")?)?;
                self.sync(
                    "routes",
                    &[
                        "public_id",
                        "agency_id",
                        "short_name",
                        "long_name",
                        "route_type",
                    ],
                    vec![
                        r.integer("route_id")?,
                        Value::Integer(agency),
                        r.optional("route_short_name"),
                        r.optional("route_long_name"),
                        r.integer("route_type")?,
                    ],
                    &[0],
                )?;
            }
            "calendar.txt" => {
                let start = r.date("start_date")?;
                let end = r.date("end_date")?;
                ensure!(
                    r.required("start_date")? <= r.required("end_date")?,
                    "calendar end date precedes start date"
                );
                let mut values = vec![r.integer("service_id")?];
                for day in [
                    "monday",
                    "tuesday",
                    "wednesday",
                    "thursday",
                    "friday",
                    "saturday",
                    "sunday",
                ] {
                    let v = r.int_default(day, 0)?;
                    ensure!(
                        matches!(v, Value::Integer(0 | 1)),
                        "invalid calendar boolean"
                    );
                    values.push(v);
                }
                values.extend([start, end]);
                self.sync(
                    "services",
                    &[
                        "public_id",
                        "monday",
                        "tuesday",
                        "wednesday",
                        "thursday",
                        "friday",
                        "saturday",
                        "sunday",
                        "start_date",
                        "end_date",
                    ],
                    values,
                    &[0],
                )?;
            }
            "calendar_dates.txt" => {
                let public = r.required("service_id")?;
                let existing = self.reference_optional("services", public)?;
                let service =
                    if existing.is_some_and(|id| self.seen["services"].contains(id as u64)) {
                        existing.unwrap()
                    } else {
                        let mut values = vec![r.integer("service_id")?];
                        values.extend(std::iter::repeat_n(Value::Null, 9));
                        self.sync(
                            "services",
                            &[
                                "public_id",
                                "monday",
                                "tuesday",
                                "wednesday",
                                "thursday",
                                "friday",
                                "saturday",
                                "sunday",
                                "start_date",
                                "end_date",
                            ],
                            values,
                            &[0],
                        )?
                    };
                self.sync(
                    "calendar_dates",
                    &["service_id", "date", "exception_type"],
                    vec![
                        Value::Integer(service),
                        r.date("date")?,
                        r.integer("exception_type")?,
                    ],
                    &[0, 1],
                )?;
            }
            "shapes.txt" => {
                let public = r.required("shape_id")?;
                let shape = if let Some(id) = self.reference_optional("shapes", public)? {
                    if self.seen.get_mut("shapes").unwrap().insert(id as u64) {
                        self.counts.entry("shapes".into()).or_default().unchanged += 1;
                    }
                    id
                } else {
                    self.sync(
                        "shapes",
                        &["public_id"],
                        vec![Value::Text(public.into())],
                        &[0],
                    )?
                };
                self.sync(
                    "shape_points",
                    &[
                        "shape_id",
                        "pt_sequence",
                        "pt_lat",
                        "pt_lon",
                        "dist_traveled",
                    ],
                    vec![
                        Value::Integer(shape),
                        r.integer("shape_pt_sequence")?,
                        r.real("shape_pt_lat", false, Some((-90., 90.)))?,
                        r.real("shape_pt_lon", false, Some((-180., 180.)))?,
                        r.real("shape_dist_traveled", true, None)?,
                    ],
                    &[0, 1],
                )?;
            }
            "trips.txt" => {
                let route = self.reference("routes", r.required("route_id")?)?;
                let service = self.reference("services", r.required("service_id")?)?;
                let shape = match r.get("shape_id") {
                    Some(v) if !v.is_empty() => Value::Integer(self.reference("shapes", v)?),
                    _ => Value::Null,
                };
                self.sync(
                    "trips",
                    &[
                        "public_id",
                        "route_id",
                        "service_id",
                        "headsign",
                        "direction_id",
                        "block_id",
                        "wheelchair_accessible",
                        "vehicle_journey_code",
                        "shape_id",
                    ],
                    vec![
                        r.text("trip_id")?,
                        Value::Integer(route),
                        Value::Integer(service),
                        r.optional("trip_headsign"),
                        r.int_default("direction_id", 0)?,
                        Value::Text(r.get("block_id").unwrap_or("").into()),
                        r.access("wheelchair_accessible")?,
                        r.text_allow_empty("vehicle_journey_code")?,
                        shape,
                    ],
                    &[0],
                )?;
            }
            "frequencies.txt" => {
                let trip = self.reference("trips", r.required("trip_id")?)?;
                let start = r.time("start_time")?;
                let end = r.time("end_time")?;
                ensure!(
                    time_seconds(r.required("start_time")?)?
                        < time_seconds(r.required("end_time")?)?,
                    "frequency end must follow start"
                );
                self.sync(
                    "frequencies",
                    &[
                        "trip_id",
                        "start_time",
                        "end_time",
                        "headway_secs",
                        "exact_times",
                    ],
                    vec![
                        Value::Integer(trip),
                        start,
                        end,
                        r.integer("headway_secs")?,
                        r.int_default("exact_times", 0)?,
                    ],
                    &[0, 1],
                )?;
            }
            "stop_times.txt" => {
                let trip = self.reference("trips", r.required("trip_id")?)?;
                let stop = self.reference("stops", r.required("stop_id")?)?;
                self.sync(
                    "stop_times",
                    &[
                        "trip_id",
                        "stop_id",
                        "arrival_time",
                        "departure_time",
                        "stop_sequence",
                        "stop_headsign",
                        "pickup_type",
                        "drop_off_type",
                        "shape_dist_traveled",
                        "timepoint",
                    ],
                    vec![
                        Value::Integer(trip),
                        Value::Integer(stop),
                        r.stop_time("arrival_time")?,
                        r.stop_time("departure_time")?,
                        r.integer("stop_sequence")?,
                        r.optional("stop_headsign"),
                        r.int_default("pickup_type", 0)?,
                        r.int_default("drop_off_type", 0)?,
                        r.decimal("shape_dist_traveled", true, None)?,
                        r.int_default("timepoint", 1)?,
                    ],
                    &[0, 4],
                )?;
            }
            _ => bail!("unknown GTFS file {name}"),
        }
        Ok(())
    }
    fn reference_optional(&mut self, table: &str, public: &str) -> Result<Option<i64>> {
        let key = (table.to_owned(), public.to_owned());
        if let Some(id) = self.cache.get(&key) {
            return Ok(Some(*id));
        }
        let sql = format!("SELECT id FROM {table} WHERE public_id=?");
        let value = self
            .conn
            .prepare_cached(&sql)?
            .query_row([public], |r| r.get(0))
            .optional()?;
        if let Some(id) = value {
            self.cache.put(key, id);
        }
        Ok(value)
    }
    fn reference(&mut self, table: &str, public: &str) -> Result<i64> {
        self.reference_optional(table, public)?
            .with_context(|| format!("missing {table} reference {public}"))
    }
    fn mark_relation(&mut self, kind: &'static str, id: i64) -> Result<()> {
        let key = (kind, id);
        let agency = if let Some(agency) = self.relation_cache.get(&key) {
            *agency
        } else {
            let sql = match kind {
                "route" => "SELECT agency_id FROM routes WHERE id=?",
                "trip" => {
                    "SELECT r.agency_id FROM trips t JOIN routes r ON r.id=t.route_id WHERE t.id=?"
                }
                _ => unreachable!(),
            };
            let agency = self
                .conn
                .prepare_cached(sql)?
                .query_row([id], |r| r.get(0))?;
            self.relation_cache.put(key, agency);
            agency
        };
        self.affected.insert(agency);
        Ok(())
    }
    fn mark_values(
        &mut self,
        table: &str,
        id: i64,
        columns: &[&str],
        values: &[Value],
    ) -> Result<()> {
        let (column, kind) = match table {
            "agencies" => {
                self.affected.insert(id);
                return Ok(());
            }
            "routes" => ("agency_id", "agency"),
            "trips" => ("route_id", "route"),
            "stop_times" => ("trip_id", "trip"),
            _ => return Ok(()),
        };
        let Value::Integer(parent) = values[columns.iter().position(|&c| c == column).unwrap()]
        else {
            bail!("invalid internal reference");
        };
        if kind == "agency" {
            self.affected.insert(parent);
            Ok(())
        } else {
            self.mark_relation(kind, parent)
        }
    }
    fn affected_for(&mut self, table: &str, id: i64) -> Result<()> {
        // Missing stop-time IDs need one lookup; agency resolution is cached
        // per trip instead of joining three tables for every deleted stop.
        if table == "stop_times" {
            let trip = self
                .conn
                .prepare_cached("SELECT trip_id FROM stop_times WHERE id=?")?
                .query_row([id], |r| r.get(0))?;
            return self.mark_relation("trip", trip);
        }
        if table == "trips" {
            return self.mark_relation("trip", id);
        }
        let sql = match table {
            "routes" => "SELECT agency_id FROM routes WHERE id=?",
            "trips" => {
                "SELECT r.agency_id FROM trips t JOIN routes r ON r.id=t.route_id WHERE t.id=?"
            }
            "stop_times" => {
                "SELECT r.agency_id FROM stop_times s JOIN trips t ON t.id=s.trip_id JOIN routes r ON r.id=t.route_id WHERE s.id=?"
            }
            "agencies" => "SELECT id FROM agencies WHERE id=?",
            "stops" => "SELECT agency_id FROM agency_stops WHERE stop_id=?",
            _ => return Ok(()),
        };
        let mut statement = self.conn.prepare_cached(sql)?;
        for id in statement.query_map([id], |r| r.get::<_, i64>(0))? {
            self.affected.insert(id?);
        }
        Ok(())
    }
    fn sync(
        &mut self,
        table: &str,
        columns: &[&str],
        values: Vec<Value>,
        keys: &[usize],
    ) -> Result<i64> {
        self.plans.entry(table.into()).or_insert_with(|| {
            let predicate = keys
                .iter()
                .map(|&k| format!("{}=?", columns[k]))
                .collect::<Vec<_>>()
                .join(" AND ");
            SyncSql {
                select: format!(
                    "SELECT id,{} FROM {table} WHERE {predicate}",
                    columns.join(",")
                ),
                updates: HashMap::new(),
                insert: format!(
                    "INSERT INTO {table}({}) VALUES({})",
                    columns.join(","),
                    vec!["?"; columns.len()].join(",")
                ),
            }
        });
        let sql = &self.plans[table].select;
        let existing: Option<(i64, Vec<Value>)> = self
            .conn
            .prepare_cached(sql)?
            .query_row(params_from_iter(keys.iter().map(|&k| &values[k])), |r| {
                Ok((
                    r.get(0)?,
                    (0..columns.len())
                        .map(|i| r.get(i + 1))
                        .collect::<rusqlite::Result<_>>()?,
                ))
            })
            .optional()?;
        let id;
        if let Some((old_id, old)) = existing {
            id = old_id;
            ensure!(
                !self
                    .seen
                    .get(table)
                    .is_some_and(|seen| seen.contains(id as u64)),
                "duplicate identity in {table}"
            );
            if old == values {
                self.counts.entry(table.into()).or_default().unchanged += 1;
            } else {
                let relationship = columns.iter().enumerate().any(|(i, col)| {
                    old[i] != values[i]
                        && match table {
                            "routes" => *col == "agency_id",
                            "trips" => *col == "route_id",
                            "stop_times" => *col == "trip_id" || *col == "stop_id",
                            _ => false,
                        }
                });
                if relationship {
                    self.mark_values(table, id, columns, &old)?;
                }
                // Only assign changed columns, so time/name edits do not
                // rewrite unchanged identity and foreign-key indexes.
                let changed: Vec<usize> = (0..columns.len())
                    .filter(|&i| old[i] != values[i])
                    .collect();
                let mask = changed.iter().fold(0u64, |mask, &i| mask | (1u64 << i));
                let update = self
                    .plans
                    .get_mut(table)
                    .unwrap()
                    .updates
                    .entry(mask)
                    .or_insert_with(|| {
                        format!(
                            "UPDATE {table} SET {} WHERE id=?",
                            changed
                                .iter()
                                .map(|&i| format!("{}=?", columns[i]))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    });
                let mut params = changed
                    .iter()
                    .map(|&i| values[i].clone())
                    .collect::<Vec<_>>();
                params.push(Value::Integer(id));
                self.conn
                    .prepare_cached(update)?
                    .execute(params_from_iter(params))?;
                if relationship {
                    self.mark_values(table, id, columns, &values)?;
                }
                self.counts.entry(table.into()).or_default().updated += 1;
            }
        } else {
            let insert = &self.plans[table].insert;
            self.conn
                .prepare_cached(insert)?
                .execute(params_from_iter(&values))?;
            id = self.conn.last_insert_rowid();
            self.mark_values(table, id, columns, &values)?;
            self.counts.entry(table.into()).or_default().inserted += 1;
        }
        self.seen.entry(table.into()).or_default().insert(id as u64);
        if let Some(index) = columns.iter().position(|c| *c == "public_id") {
            let public = match &values[index] {
                Value::Text(v) => v.clone(),
                Value::Integer(v) => v.to_string(),
                _ => bail!("invalid public ID"),
            };
            self.cache.put((table.into(), public), id);
        }
        Ok(id)
    }
    fn delete_missing(&mut self) -> Result<()> {
        for table in [
            "stop_times",
            "frequencies",
            "calendar_dates",
            "trips",
            "shape_points",
            "routes",
            "services",
            "stops",
            "shapes",
            "agencies",
        ] {
            let Some(seen) = self.seen.get(table) else {
                continue;
            };
            let mut missing = RoaringTreemap::new();
            {
                let mut stmt = self
                    .conn
                    .prepare(&format!("SELECT id FROM {table} ORDER BY id"))?;
                for id in stmt.query_map([], |r| r.get::<_, i64>(0))? {
                    let id = id?;
                    if !seen.contains(id as u64) {
                        missing.insert(id as u64);
                    }
                }
            }
            let delete = format!("DELETE FROM {table} WHERE id=?");
            for id in missing.iter() {
                self.affected_for(table, id as i64)?;
                self.conn.prepare_cached(&delete)?.execute([id as i64])?;
                self.counts.entry(table.into()).or_default().deleted += 1;
                self.rows += 1;
                if self.rows.is_multiple_of(8192) {
                    check_disk(self.disk, self.minimum)?;
                }
            }
        }
        Ok(())
    }
    fn memberships(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TEMP TABLE desired_stops(stop_id INTEGER PRIMARY KEY) WITHOUT ROWID",
        )?;
        for &agency in &self.affected {
            self.conn.execute("DELETE FROM desired_stops", [])?;
            self.conn.execute("INSERT INTO desired_stops SELECT DISTINCT st.stop_id FROM routes r JOIN trips t ON t.route_id=r.id JOIN stop_times st ON st.trip_id=t.id WHERE r.agency_id=?",[agency])?;
            self.conn.execute("DELETE FROM agency_stops WHERE agency_id=? AND stop_id NOT IN (SELECT stop_id FROM desired_stops)",[agency])?;
            self.conn.execute("INSERT INTO agency_stops SELECT ?,stop_id FROM desired_stops WHERE true ON CONFLICT DO NOTHING",[agency])?;
        }
        self.conn.execute_batch("DROP TABLE desired_stops")?;
        Ok(())
    }
}

struct Row<'a> {
    columns: &'a HashMap<&'a str, usize>,
    record: &'a csv::StringRecord,
}
impl Row<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.columns.get(name).and_then(|&i| self.record.get(i))
    }
    fn required(&self, name: &str) -> Result<&str> {
        self.get(name)
            .filter(|v| !v.is_empty())
            .with_context(|| format!("missing {name}"))
    }
    fn text(&self, name: &str) -> Result<Value> {
        Ok(Value::Text(self.required(name)?.into()))
    }
    fn text_allow_empty(&self, name: &str) -> Result<Value> {
        Ok(Value::Text(
            self.get(name)
                .with_context(|| format!("missing column {name}"))?
                .into(),
        ))
    }
    fn optional(&self, name: &str) -> Value {
        self.get(name)
            .filter(|v| !v.is_empty())
            .map(|v| Value::Text(v.into()))
            .unwrap_or(Value::Null)
    }
    fn integer(&self, name: &str) -> Result<Value> {
        let v = self
            .required(name)?
            .parse::<i64>()
            .with_context(|| format!("invalid integer {name}"))?;
        ensure!(
            v.unsigned_abs() <= 9_007_199_254_740_991,
            "{name} exceeds JavaScript safe integer range"
        );
        Ok(Value::Integer(v))
    }
    fn int_default(&self, name: &str, default: i64) -> Result<Value> {
        match self.get(name) {
            Some(v) if !v.is_empty() => Ok(Value::Integer(
                v.parse().with_context(|| format!("invalid {name}"))?,
            )),
            _ => Ok(Value::Integer(default)),
        }
    }
    fn access(&self, name: &str) -> Result<Value> {
        let Value::Integer(v) = self.int_default(name, 0)? else {
            unreachable!()
        };
        ensure!((0..=2).contains(&v), "invalid {name}");
        Ok(Value::Integer(i64::from(v == 1)))
    }
    fn date(&self, name: &str) -> Result<Value> {
        let date = NaiveDate::parse_from_str(self.required(name)?, "%Y%m%d")
            .with_context(|| format!("invalid {name}"))?;
        Ok(Value::Integer(date.format("%Y%m%d").to_string().parse()?))
    }
    fn time(&self, name: &str) -> Result<Value> {
        let value = time_seconds(self.required(name)?)?;
        ensure!(
            value <= 9_007_199_254_740_991,
            "GTFS time exceeds safe integer range"
        );
        Ok(Value::Integer(value as i64))
    }
    fn stop_time(&self, name: &str) -> Result<Value> {
        if self.get(name).context("missing time column")?.is_empty() {
            Ok(Value::Null)
        } else {
            self.time(name)
        }
    }
    fn decimal(&self, name: &str, optional: bool, range: Option<(f64, f64)>) -> Result<Value> {
        let value = match self.get(name).filter(|v| !v.is_empty()) {
            Some(v) => v,
            None if optional => return Ok(Value::Null),
            _ => bail!("missing {name}"),
        };
        let number = value
            .parse::<f64>()
            .with_context(|| format!("invalid decimal {name}"))?;
        ensure!(number.is_finite(), "invalid decimal {name}");
        if let Some((min, max)) = range {
            ensure!((min..=max).contains(&number), "{name} out of range");
        }
        Ok(Value::Text(value.into()))
    }
    fn real(&self, name: &str, optional: bool, range: Option<(f64, f64)>) -> Result<Value> {
        match self.decimal(name, optional, range)? {
            Value::Text(v) => Ok(Value::Real(v.parse()?)),
            v => Ok(v),
        }
    }
}
fn time_seconds(value: &str) -> Result<u64> {
    let parts: Vec<_> = value.split(':').collect();
    ensure!(parts.len() == 3, "invalid GTFS time {value}");
    let hours = parts[0].parse::<u64>()?;
    let minutes = parts[1].parse::<u64>()?;
    let seconds = parts[2].parse::<u64>()?;
    ensure!(
        parts[0].len() >= 2
            && parts[1].len() == 2
            && parts[2].len() == 2
            && minutes < 60
            && seconds < 60,
        "invalid GTFS time {value}"
    );
    hours
        .checked_mul(3600)
        .and_then(|h| h.checked_add(minutes * 60 + seconds))
        .context("GTFS time overflow")
}
