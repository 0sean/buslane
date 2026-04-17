import type { HTTPEvent } from 'h3';
import type { DB } from '~/db/timetables';

export function useDb(event: HTTPEvent): DB {
  const db = (event.context as any).db as DB | undefined;
  if (!db) throw new Error('db not initialized — is the db plugin loaded?');
  return db;
}
