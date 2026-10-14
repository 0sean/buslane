export class ByteCache<T> {
  private entries = new Map<
    string,
    { value: T; bytes: number; expires: number }
  >();
  private bytes = 0;
  constructor(
    private maximum = 32 * 1024 * 1024,
    private ttl = 86400_000,
  ) {
    if (!Number.isSafeInteger(maximum) || maximum < 0)
      throw new Error(
        "TIMETABLES_CACHE_BYTES must be a non-negative safe integer",
      );
  }
  get(key: string): T | undefined {
    const entry = this.entries.get(key);
    if (!entry) return;
    if (entry.expires <= Date.now()) {
      this.remove(key);
      return;
    }
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }
  put(key: string, value: T, bytes: number) {
    if (bytes > this.maximum) return;
    this.remove(key);
    while (this.bytes + bytes > this.maximum && this.entries.size)
      this.remove(this.entries.keys().next().value!);
    this.entries.set(key, { value, bytes, expires: Date.now() + this.ttl });
    this.bytes += bytes;
  }
  private remove(key: string) {
    const entry = this.entries.get(key);
    if (entry) {
      this.bytes -= entry.bytes;
      this.entries.delete(key);
    }
  }
  clear() {
    const count = this.entries.size;
    this.entries.clear();
    this.bytes = 0;
    return count;
  }
}
