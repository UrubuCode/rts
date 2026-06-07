// Cross-runtime: small LRU cache relying on Map insertion order.
class Lru {
  limit: number;
  map = new Map<string, number>();
  constructor(limit: number) { this.limit = limit; }
  get(k: string) {
    if (!this.map.has(k)) return -1;
    const v = this.map.get(k)!;
    this.map.delete(k);
    this.map.set(k, v);
    return v;
  }
  set(k: string, v: number) {
    if (this.map.has(k)) this.map.delete(k);
    this.map.set(k, v);
    if (this.map.size > this.limit) this.map.delete(this.map.keys().next().value);
  }
}

const l = new Lru(3);
l.set("a", 1); l.set("b", 2); l.set("c", 3);
console.log(l.get("a"));
l.set("d", 4);
console.log([...l.map.entries()].map(([k, v]) => k + v).join(","));
console.log(l.get("b"));
