import 'dotenv/config';
import { drizzle } from 'drizzle-orm/node-postgres';
import * as schemaModels from './schema';
import * as schemaRelations from './relations';

export const db = drizzle(process.env.TIMETABLES_DATABASE_URL!, {
  schema: { ...schemaModels, ...schemaRelations },
});

export * as schema from "./schema";
export * as relations from "./relations";