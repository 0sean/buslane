import { mkdtempSync, cpSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { execFileSync } from "node:child_process";
import Database from "better-sqlite3";
export const root = resolve(import.meta.dirname, "../../..");
export const workerPath = resolve(
  root,
  "apps/api/.output/server/timetable-worker.mjs",
);
export function fixture() {
  const directory = mkdtempSync(join(tmpdir(), "buslane-test-"));
  const feed = join(directory, "feed");
  cpSync(resolve(root, "tests/fixtures/gtfs"), feed, { recursive: true });
  const database = join(directory, "timetables.sqlite");
  const binary = resolve(root, "apps/timetables/target/debug/timetables");
  const apply = () =>
    JSON.parse(
      execFileSync(
        binary,
        [
          "--db",
          database,
          "--source",
          feed,
          "--scratch",
          join(directory, "scratch"),
          "--min-free-bytes",
          "0",
        ],
        { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
      ),
    );
  apply();
  return {
    directory,
    feed,
    database,
    apply,
    open: () => new Database(database),
    close: () => rmSync(directory, { recursive: true, force: true }),
  };
}
export const migration = () =>
  ["001.sql", "002.sql"]
    .map((name) => readFileSync(
      resolve(root, "packages/timetable-db/migrations/" + name),
      "utf8",
    ))
    .join("\n");
