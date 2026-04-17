import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import * as schemaModels from './schema';
import * as schemaRelations from './relations';

export * as schema from "./schema";
export * as relations from "./relations";

type DB = ReturnType<typeof drizzle<typeof schemaModels & typeof schemaRelations>>;
let _db: DB | null = null;

export function initDb(connectionString: string) {
  if (!_db) {
    const client = postgres(connectionString, { prepare: false });
    _db = drizzle(client, {
      schema: { ...schemaModels, ...schemaRelations },
    }) as DB;
  }
}

// Lazy proxy — existing db.query.xxx imports work unchanged
export const db = new Proxy({} as DB, {
  get(_, prop) {
    if (!_db) throw new Error('db not initialized');
    return (_db as any)[prop];
  },
});
