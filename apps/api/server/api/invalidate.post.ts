import { defineHandler } from "nitro";
import { useKv } from "#utils/kv.ts";

export default defineHandler(async (event) => {
  const kv = useKv(event);
  if (!kv) {
    return { deleted: 0 };
  }

  let deleted = 0;
  let cursor: string | undefined;

  do {
    const result = await kv.list(cursor ? { cursor } : undefined);
    await Promise.all(result.keys.map((k) => kv.delete(k.name)));
    deleted += result.keys.length;
    cursor = result.list_complete ? undefined : result.cursor;
  } while (cursor);

  return { deleted };
});
