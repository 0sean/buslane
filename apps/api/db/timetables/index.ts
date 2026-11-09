import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import * as schema from "../../../../packages/timetable-db/schema.ts";
export { schema };
export const SCHEMA_VERSION = 2;
export function openReader(path: string) {
  const sqlite = new Database(path, {
    readonly: true,
    fileMustExist: true,
    timeout: 5000,
  });
  try {
    const version = String(
      sqlite.prepare("SELECT sqlite_version()").pluck().get(),
    )
      .split(".")
      .map(Number);
    if (
      version[0]! < 3 ||
      (version[0] === 3 &&
        (version[1]! < 51 || (version[1] === 51 && version[2]! < 3)))
    )
      throw new Error("SQLite >=3.51.3 is required");
    if (sqlite.pragma("user_version", { simple: true }) !== SCHEMA_VERSION)
      throw new Error(
        "Incompatible timetable schema; run the matching importer",
      );
    if (sqlite.pragma("application_id", { simple: true }) !== 0x4255534c)
      throw new Error("Not a Buslane timetable database");
    if (sqlite.pragma("journal_mode", { simple: true }) !== "wal")
      throw new Error("Timetable database must use WAL");
    sqlite.pragma("query_only=ON");
    sqlite.pragma("foreign_keys=ON");
    sqlite.pragma("cache_size=-32768");
    return { sqlite, orm: drizzle(sqlite, { schema }) };
  } catch (error) {
    sqlite.close();
    throw error;
  }
}
