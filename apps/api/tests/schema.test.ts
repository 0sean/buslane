import { test } from "node:test";
import assert from "node:assert/strict";
import Database from "better-sqlite3";
import { getTableConfig } from "drizzle-orm/sqlite-core";
import * as schema from "../../../packages/timetable-db/schema.ts";
import { migration, fixture } from "./helpers.ts";
import { openReader } from "../db/timetables/index.ts";

test("Drizzle columns, keys, indexes and references match authoritative SQL", () => {
  const db = new Database(":memory:");
  db.exec(migration());
  for (const table of Object.values(schema)) {
    const config = getTableConfig(table);
    const columns = db
      .prepare("SELECT * FROM pragma_table_info(?)")
      .all(config.name) as any[];
    assert.equal(columns.length, config.columns.length, config.name);
    for (const column of config.columns) {
      const actual = columns.find((c) => c.name === column.name);
      assert.ok(actual, config.name + "." + column.name);
      assert.equal(
        actual.type.toLowerCase(),
        column.getSQLType(),
        config.name + "." + column.name,
      );
      assert.equal(
        Boolean(actual.notnull) || Boolean(actual.pk),
        column.notNull,
        config.name + "." + column.name,
      );
      assert.equal(
        Boolean(actual.pk),
        column.primary ||
          config.primaryKeys.some((k) => k.columns.includes(column)),
        config.name + "." + column.name,
      );
      if (column.default !== undefined)
        assert.notEqual(
          actual.dflt_value,
          null,
          config.name + "." + column.name,
        );
    }
    const indexes = db
      .prepare("SELECT * FROM pragma_index_list(?)")
      .all(config.name) as any[];
    const signatures = indexes.map((i) => ({
      unique: Boolean(i.unique),
      columns: (
        db
          .prepare("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
          .all(i.name) as any[]
      ).map((r) => r.name),
    }));
    for (const idx of config.indexes)
      assert.ok(
        signatures.some(
          (s) =>
            s.unique === Boolean(idx.config.unique) &&
            JSON.stringify(s.columns) ===
              JSON.stringify(idx.config.columns.map((c: any) => c.name)),
        ),
        config.name + " index " + idx.config.name,
      );
    for (const column of config.columns.filter((c) => c.isUnique))
      assert.ok(
        signatures.some(
          (s) =>
            s.unique && s.columns.length === 1 && s.columns[0] === column.name,
        ),
        config.name + " unique " + column.name,
      );
    const actualFKs = db
      .prepare("SELECT * FROM pragma_foreign_key_list(?)")
      .all(config.name) as any[];
    for (const fk of config.foreignKeys) {
      const reference = fk.reference();
      const target = getTableConfig(reference.foreignTable);
      assert.ok(
        actualFKs.some(
          (f) =>
            f.table === target.name &&
            f.from === reference.columns[0]!.name &&
            f.to === reference.foreignColumns[0]!.name,
        ),
        config.name + " foreign key",
      );
    }
    assert.equal(
      actualFKs.length,
      config.foreignKeys.length,
      config.name + " reference count",
    );
  }
  db.close();
});
test("API connections reject writes and incompatible schemas", () => {
  const f = fixture();
  try {
    const reader = openReader(f.database);
    assert.throws(() => reader.sqlite.exec("DELETE FROM stops"), /readonly/i);
    reader.sqlite.close();
    const writer = f.open();
    writer.pragma("user_version=99");
    writer.close();
    assert.throws(() => openReader(f.database), /Incompatible/);
  } finally {
    f.close();
  }
});
test("hot lookup plans use existing indexes", () => {
  const f = fixture();
  const db = f.open();
  try {
    db.exec(`WITH RECURSIVE seq(x) AS (VALUES(10) UNION ALL SELECT x+1 FROM seq WHERE x<5010)
      INSERT INTO stop_times(trip_id,stop_id,arrival_time,departure_time,stop_sequence,pickup_type,drop_off_type,timepoint)
      SELECT 1,2,90000,90000,x,0,0,1 FROM seq;
      WITH RECURSIVE seq(x) AS (VALUES(10) UNION ALL SELECT x+1 FROM seq WHERE x<5010)
      INSERT INTO trips(public_id,route_id,service_id,direction_id,block_id,wheelchair_accessible,vehicle_journey_code)
      SELECT 'BULK'||x,2,1,0,'',0,'VJ'||x FROM seq;
      WITH RECURSIVE seq(x) AS (VALUES(10) UNION ALL SELECT x+1 FROM seq WHERE x<5010)
      INSERT INTO stops(public_id,name,lat,lon,wheelchair_boarding,location_type)
      SELECT 'BULK'||x,'Bulk','51.5','-0.1',0,0 FROM seq;
      INSERT INTO agency_stops SELECT 2,id FROM stops WHERE public_id LIKE 'BULK%'; ANALYZE;`);
    for (const sql of [
      "SELECT * FROM stop_times WHERE trip_id=1 ORDER BY stop_sequence",
      "SELECT * FROM stop_times WHERE stop_id=1",
      "SELECT * FROM trips WHERE route_id=1 ORDER BY public_id",
      "SELECT * FROM agency_stops WHERE agency_id=1",
    ]) {
      const plans = db.prepare("EXPLAIN QUERY PLAN " + sql).all() as any[];
      assert.ok(
        plans.some((p) => /SEARCH.*(INDEX|PRIMARY KEY)/.test(p.detail)),
        sql,
      );
    }
  } finally {
    db.close();
    f.close();
  }
});
