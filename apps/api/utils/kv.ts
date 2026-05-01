import type { HTTPEvent } from 'h3';

interface KVNamespace {
  get(key: string): Promise<string | null>;
  put(key: string, value: string, options?: { expirationTtl?: number }): Promise<void>;
  delete(key: string): Promise<void>;
  list(options?: { cursor?: string }): Promise<{ keys: { name: string }[]; list_complete: boolean; cursor?: string }>;
}

export function useKv(event: HTTPEvent): KVNamespace | null {
  const env = (event.req.runtime as any)?.cloudflare?.env;
  return (env?.TIMETABLES_CACHE as KVNamespace) ?? null;
}

export async function withKvCache<T>(
  kv: KVNamespace | null,
  key: string,
  ttl: number,
  fn: () => Promise<T>
): Promise<T> {
  if (kv) {
    const cached = await kv.get(key);
    if (cached !== null) return JSON.parse(cached) as T;
  }
  const result = await fn();
  if (kv) {
    await kv.put(key, JSON.stringify(result), { expirationTtl: ttl });
  }
  return result;
}
