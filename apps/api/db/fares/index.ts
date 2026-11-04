import Database from "better-sqlite3";
export const SCHEMA_VERSION = 2;

export function openFaresReader(path: string) {
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
    if (
      sqlite.pragma("application_id", { simple: true }) !== 0x42555352 ||
      sqlite.pragma("user_version", { simple: true }) !== SCHEMA_VERSION
    )
      throw new Error(
        "Incompatible fares database; run the matching fares importer",
      );
    if (sqlite.pragma("journal_mode", { simple: true }) !== "wal")
      throw new Error("Fares database must use WAL");
    sqlite.pragma("query_only=ON");
    sqlite.pragma("foreign_keys=ON");
    sqlite.pragma("cache_size=-16384");
    return sqlite;
  } catch (error) {
    sqlite.close();
    throw error;
  }
}
