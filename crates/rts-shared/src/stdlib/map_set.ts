// Faithful TypeScript `Map`/`Set` — the REAL stdlib for the new engine.
//
// Generic `Map<K,V>` + `Set<T>` with private array fields, SameValueZero key
// compare (=== plus NaN matches NaN, JS spec),
// `return this` chaining, `delete` via shift+pop, `undefined` on miss, and a
// `get size()` getter. Embedded as an engine `include` (declarations-only, no
// top-level code): its top-level classes become ambient and shadow the native
// Map/Set entirely. Parity with the former native dispatch is proven by
// `stdlib_parity.rs`.

class Map<K, V> {
  #keys: K[] = [];
  #vals: V[] = [];
  // `new Map([[k, v], …])` — iterable of [key, value] pairs, like JS. The pair `p`
  // is a for-of binding (unproven array); `p[0]`/`p[1]` read its elements via the
  // generic `__rtsadp_idx_get` runtime path (array element / string char / object).
  constructor(init: [K, V][] = []) {
    for (const p of init) { this.set(p[0], p[1]); }
  }
  set(k: K, v: V): Map<K, V> {
    for (let i = 0; i < this.#keys.length; i++) {
      if (__svz(this.#keys[i], k)) { this.#vals[i] = v; return this; }
    }
    this.#keys.push(k); this.#vals.push(v);
    return this;
  }
  get(k: K): V | undefined {
    for (let i = 0; i < this.#keys.length; i++) {
      if (__svz(this.#keys[i], k)) return this.#vals[i];
    }
    return undefined;
  }
  has(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (__svz(this.#keys[i], k)) return true;
    }
    return false;
  }
  delete(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (__svz(this.#keys[i], k)) {
        for (let j = i; j < this.#keys.length - 1; j++) {
          this.#keys[j] = this.#keys[j + 1];
          this.#vals[j] = this.#vals[j + 1];
        }
        this.#keys.pop(); this.#vals.pop();
        return true;
      }
    }
    return false;
  }
  get size(): number { return this.#keys.length; }
  // Iteration helpers — return eager arrays so `for (const k of m.keys())` works
  // (the engine iterates a proven array). `entries()` yields `[key, value]` pairs.
  keys(): K[] {
    const out: K[] = [];
    for (let i = 0; i < this.#keys.length; i++) { out.push(this.#keys[i]); }
    return out;
  }
  values(): V[] {
    const out: V[] = [];
    for (let i = 0; i < this.#vals.length; i++) { out.push(this.#vals[i]); }
    return out;
  }
  entries(): [K, V][] {
    const out: [K, V][] = [];
    for (let i = 0; i < this.#keys.length; i++) { out.push([this.#keys[i], this.#vals[i]]); }
    return out;
  }
  forEach(cb: (v: V, k: K, m: Map<K, V>) => void): void {
    for (let i = 0; i < this.#keys.length; i++) { cb(this.#vals[i], this.#keys[i], this); }
  }
  clear(): void { this.#keys = []; this.#vals = []; }
  // Default iterator (`for (const [k,v] of map)`): a real generator yielding
  // `[key, value]` pairs in insertion order. Plain JS — the parser desugars the
  // `*`/`yield`, the engine drives it through the generator state machine.
  *[Symbol.iterator](): [K, V][] {
    for (let i = 0; i < this.size; i++) { yield [this.#keys[i], this.#vals[i]]; }
  }
}
class Set<T> {
  #items: T[] = [];
  constructor(init: T[] = []) {
    for (const v of init) { this.add(v); }
  }
  add(v: T): Set<T> {
    for (let i = 0; i < this.#items.length; i++) {
      if (__svz(this.#items[i], v)) return this;
    }
    this.#items.push(v);
    return this;
  }
  has(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (__svz(this.#items[i], v)) return true;
    }
    return false;
  }
  delete(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (__svz(this.#items[i], v)) {
        for (let j = i; j < this.#items.length - 1; j++) this.#items[j] = this.#items[j + 1];
        this.#items.pop();
        return true;
      }
    }
    return false;
  }
  get size(): number { return this.#items.length; }
  // `values()`/`keys()` both yield the elements (JS Set), `entries()` yields
  // `[v, v]` pairs; eager arrays so `for (const v of s.values())` iterates.
  values(): T[] {
    const out: T[] = [];
    for (let i = 0; i < this.#items.length; i++) { out.push(this.#items[i]); }
    return out;
  }
  keys(): T[] { return this.values(); }
  entries(): [T, T][] {
    const out: [T, T][] = [];
    for (let i = 0; i < this.#items.length; i++) { out.push([this.#items[i], this.#items[i]]); }
    return out;
  }
  forEach(cb: (v: T, v2: T, s: Set<T>) => void): void {
    for (let i = 0; i < this.#items.length; i++) { cb(this.#items[i], this.#items[i], this); }
  }
  clear(): void { this.#items = []; }
  // Default iterator (`for (const x of set)`): a real generator yielding values.
  *[Symbol.iterator](): T[] {
    for (let i = 0; i < this.size; i++) { yield this.#items[i]; }
  }
}

// SameValueZero (JS spec key equality for Map/Set/includes): `===` except
// NaN equals NaN (x !== x detects NaN without naming it).
function __svz(a: any, b: any): boolean {
  if (a === b) return true;
  return a !== a && b !== b;
}
