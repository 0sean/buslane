import { eq } from "drizzle-orm";
import type { DB } from "~/db/timetables";
import { schema } from "~/db/timetables";

export async function findAgencyByIdOrNoc(db: DB, idOrNoc: string) {
  if (idOrNoc.startsWith("noc:")) {
    const noc = idOrNoc.slice(4);
    return db.query.agencies.findFirst({ where: eq(schema.agencies.noc, noc) });
  }
  return db.query.agencies.findFirst({ where: eq(schema.agencies.id, idOrNoc) });
}
