import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fixture } from "./helpers.ts";
import { Repository } from "../db/repository.ts";
test("valid interpolated GTFS times retain empty strings in JSON", () => {
  const f = fixture();
  const path = join(f.feed, "stop_times.txt");
  const data =
    readFileSync(path, "utf8").replace(
      "S2,2,,0,0,100.0,1",
      "S2,3,,0,0,100.0,1",
    ) + "T1,,,S1,2,,0,0,,0\n";
  writeFileSync(path, data);
  f.apply();
  const db = f.open();
  try {
    const response = JSON.parse(
      [
        ...new Repository(db).prepare({ kind: "trip", id: "T1" }, 2).chunks,
      ].join(""),
    );
    assert.equal(response.stop_times[1].arrival_time, "");
    assert.equal(response.stop_times[1].departure_time, "");
    assert.equal(response.stop_times[1].timepoint, "approximate");
    assert.equal(typeof response.created_at, "string");
  } finally {
    db.close();
    f.close();
  }
});
