import type Database from "better-sqlite3";
import { createHash } from "node:crypto";
import { inflateRawSync } from "node:zlib";
import { QueryError } from "../repository.ts";

export interface FaresQuery {
  kind:
    | "fareDocuments"
    | "fareDocument"
    | "fareProducts"
    | "farePrices"
    | "fareEntities"
    | "fareEntity"
    | "fareXml";
  id?: string;
  document_id?: number;
  category?: string;
  ref?: string;
  product_ref?: string;
  noc?: string;
  line?: string;
  stop?: string;
  limit?: number;
  cursor?: string;
}
type Row = Record<string, any>;
const documentColumns =
  "d.id,d.path,d.sha256,d.publication_timestamp,d.currency,d.created_at,d.updated_at";
const entityColumns =
  "e.id,e.document_id,e.category,e.kind,e.ref,e.version,e.occurrence,e.name,e.amount,e.currency,e.product_ref,e.profile_ref,e.package_ref,e.matrix_ref,e.price_ref,e.group_ref,e.start_ref,e.end_ref,e.data";
export class FaresRepository {
  private statements = new Map<string, Database.Statement>();
  constructor(public db: Database.Database) {}
  private statement(sql: string) {
    let stmt = this.statements.get(sql);
    if (!stmt) {
      if (this.statements.size >= 64)
        this.statements.delete(this.statements.keys().next().value!);
      stmt = this.db.prepare(sql);
      this.statements.set(sql, stmt);
    }
    return stmt;
  }
  private all(sql: string, ...params: any[]): Row[] {
    return this.statement(sql).all(...params) as Row[];
  }
  private get(sql: string, ...params: any[]): Row | undefined {
    return this.statement(sql).get(...params) as Row | undefined;
  }
  private id(value?: string) {
    if (
      !value ||
      !/^[1-9]\d*$/.test(value) ||
      !Number.isSafeInteger(Number(value))
    )
      throw new QueryError(400, "Invalid fares ID");
    return Number(value);
  }
  private required(row: Row | undefined, name: string): Row {
    if (!row) throw new QueryError(404, name + " not found");
    return row;
  }
  private document(row: Row) {
    return {
      ...row,
      links: this.all(
        "SELECT kind,code,ref,qualifier,name FROM links WHERE document_id=? ORDER BY kind,code,ref,qualifier",
        row.id,
      ),
    };
  }
  private entity(row: Row) {
    const result: Row = { ...row, data: JSON.parse(row.data) };
    if (row.category === "price" && row.amount === null && row.price_ref) {
      // Keep ambiguity visible: a shared ref can have several NeTEx versions.
      result.referenced_prices = this.all(
        "SELECT ref,version,occurrence,amount,currency FROM entities WHERE document_id=? AND ref=? AND category='price' AND amount IS NOT NULL ORDER BY id",
        row.document_id,
        row.price_ref,
      );
    }
    if (row.category === "price" && row.matrix_ref) {
      result.matrices = this.all(
        "SELECT ref,version,occurrence,start_ref,end_ref,group_ref FROM entities WHERE document_id=? AND ref=? AND category='matrix' ORDER BY id",
        row.document_id,
        row.matrix_ref,
      );
    }
    return result;
  }
  prepare(
    query: FaresQuery,
    generation: number,
  ): { chunks: Generator<string>; nextCursor(): string | undefined } {
    const single = ["fareDocument", "fareEntity", "fareXml"].includes(
      query.kind,
    );
    if (single) {
      const id = this.id(query.id);
      let item: Row;
      if (query.kind === "fareEntity") {
        const row = this.required(
          this.get(`SELECT ${entityColumns} FROM entities e WHERE e.id=?`, id),
          "Fare entity",
        );
        if (query.category && row.category !== query.category)
          throw new QueryError(404, "Fare entity not found");
        item = this.entity(row);
      } else if (query.kind === "fareXml") {
        const row = this.required(
          this.get("SELECT xml_deflate FROM documents WHERE id=?", id),
          "Fare document",
        );
        const xml = inflateRawSync(row.xml_deflate, {
          maxOutputLength: 64 * 1024 * 1024,
        }).toString("utf8");
        return {
          chunks: (function* () {
            yield xml;
          })(),
          nextCursor: () => undefined,
        };
      } else
        item = this.document(
          this.required(
            this.get(
              `SELECT ${documentColumns} FROM documents d WHERE d.id=?`,
              id,
            ),
            "Fare document",
          ),
        );
      return {
        chunks: (function* () {
          yield JSON.stringify(item);
        })(),
        nextCursor: () => undefined,
      };
    }
    const limit = query.limit ?? 100;
    if (!Number.isInteger(limit) || limit < 1 || limit > 1000)
      throw new QueryError(400, "limit must be an integer between 1 and 1000");
    const { cursor: _, ...binding } = query;
    const fingerprint = createHash("sha256")
      .update(JSON.stringify(binding))
      .digest("hex");
    let after = 0;
    if (query.cursor) {
      try {
        if (query.cursor.length > 2048) throw new Error();
        const token = JSON.parse(
          Buffer.from(query.cursor, "base64url").toString("utf8"),
        );
        if (token.generation !== generation)
          throw new QueryError(
            409,
            "Fares dataset changed; restart pagination",
          );
        if (
          token.binding !== fingerprint ||
          !Number.isSafeInteger(token.after) ||
          token.after < 1
        )
          throw new Error();
        after = token.after;
      } catch (error) {
        if (error instanceof QueryError) throw error;
        throw new QueryError(400, "Invalid fares cursor");
      }
    }
    const documents = query.kind === "fareDocuments";
    const alias = documents ? "d" : "e";
    const documentId = documents ? "d.id" : "e.document_id";
    const where = [`${alias}.id>?`];
    const params: any[] = [after];
    const category =
      query.kind === "fareProducts"
        ? "product"
        : query.kind === "farePrices"
          ? "price"
          : query.category;
    if (!documents && category) {
      where.push("e.category=?");
      params.push(category);
    }
    if (query.document_id !== undefined) {
      if (!Number.isSafeInteger(query.document_id) || query.document_id < 1)
        throw new QueryError(400, "Invalid document_id");
      where.push(`${documentId}=?`);
      params.push(query.document_id);
    }
    for (const field of ["ref", "product_ref"] as const)
      if (query[field] !== undefined) {
        if (documents)
          throw new QueryError(400, field + " only applies to entities");
        where.push(`e.${field}=?`);
        params.push(query[field]);
      }
    for (const [field, kind] of [
      ["noc", "operator"],
      ["line", "line"],
      ["stop", "stop"],
    ] as const) {
      if (query[field] !== undefined) {
        where.push(
          `${documentId} IN (SELECT document_id FROM links WHERE kind=? AND code=?${field === "noc" ? " AND ref NOT LIKE 'uic:%'" : ""})`,
        );
        params.push(kind, query[field]);
      }
    }
    // A line+NOC filter must refer to the same operator's line. Independent
    // document membership alone is insufficient for multi-operator products.
    if (query.line && query.noc) {
      where.push(
        `EXISTS (SELECT 1 FROM links l JOIN links o ON o.document_id=l.document_id AND o.kind='operator' AND o.ref=l.qualifier WHERE l.document_id=${documentId} AND l.kind='line' AND l.code=? AND o.code=? AND o.ref NOT LIKE 'uic:%')`,
      );
      params.push(query.line, query.noc);
    }
    const rows = this.all(
      `SELECT ${documents ? documentColumns : "e.id"} FROM ${documents ? "documents d" : "entities e"} WHERE ${where.join(" AND ")} ORDER BY ${alias}.id LIMIT ?`,
      ...params,
      limit + 1,
    );
    const more = rows.length > limit;
    if (more) rows.pop();
    const next = more
      ? Buffer.from(
          JSON.stringify({
            generation,
            binding: fingerprint,
            after: rows.at(-1)!.id,
          }),
        ).toString("base64url")
      : undefined;
    const self = this;
    return {
      chunks: (function* () {
        yield "[";
        for (let i = 0; i < rows.length; i++) {
          if (i) yield ",";
          // Page only IDs first: zone/rule JSON can be several megabytes per
          // record. Fetch one payload at a time inside the worker's snapshot.
          yield JSON.stringify(
            documents
              ? rows[i]
              : self.entity(
                  self.required(
                    self.get(
                      `SELECT ${entityColumns} FROM entities e WHERE e.id=?`,
                      rows[i]!.id,
                    ),
                    "Fare entity",
                  ),
                ),
          );
        }
        yield "]";
      })(),
      nextCursor: () => next,
    };
  }
}
