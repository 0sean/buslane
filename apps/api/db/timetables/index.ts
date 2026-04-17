import { drizzle } from 'drizzle-orm/postgres-js';
import * as schemaModels from './schema';
import * as schemaRelations from './relations';

export * as schema from "./schema";
export * as relations from "./relations";

export function createDb(connectionString: string) {
  return drizzle(connectionString, { schema: { ...schemaModels, ...schemaRelations } });
}
