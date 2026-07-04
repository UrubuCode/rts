// Faithful TypeScript `WeakMap`/`WeakSet` — the stdlib for the new engine.
//
// STRONG-reference semantics for now (no real weak collection / GC interaction —
// #217 tracks the weak path): backed by private arrays with `===` (reference)
// key/value compare, exactly like the `Map`/`Set` stdlib (`map_set.ts`). The Weak
// variants expose ONLY the non-iterable surface JS allows (no `size`, no
// `keys`/`values`/`entries`/`forEach`, no iteration) — keys/values are object
// references compared by identity. Embedded as an engine `include`
// (declarations-only): the ambient `class WeakMap`/`class WeakSet` make
// `new WeakMap()` an ordinary user-class construction.

class WeakMap<K, V> {
  #keys: K[] = [];
  #vals: V[] = [];
  // `new WeakMap([[k, v], …])` — iterable of [key, value] pairs, like JS.
  constructor(init: [K, V][] = []) {
    for (const p of init) { this.set(p[0], p[1]); }
  }
  set(k: K, v: V): WeakMap<K, V> {
    // JS: a WeakMap key must be an object (or a non-registered symbol) — a
    // primitive key is a TypeError.
    const t = typeof k;
    if ((t !== "object" && t !== "function" && t !== "symbol") || k === null) {
      throw new TypeError("Invalid value used as weak map key");
    }
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) { this.#vals[i] = v; return this; }
    }
    this.#keys.push(k);
    this.#vals.push(v);
    return this;
  }
  get(k: K): V | undefined {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) { return this.#vals[i]; }
    }
    return undefined;
  }
  has(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) { return true; }
    }
    return false;
  }
  delete(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) {
        const last = this.#keys.length - 1;
        this.#keys[i] = this.#keys[last];
        this.#vals[i] = this.#vals[last];
        this.#keys.pop();
        this.#vals.pop();
        return true;
      }
    }
    return false;
  }
}

class WeakSet<T> {
  #items: T[] = [];
  // `new WeakSet([v, …])` — iterable of values, like JS.
  constructor(init: T[] = []) {
    for (const v of init) { this.add(v); }
  }
  add(v: T): WeakSet<T> {
    // JS: a WeakSet value must be an object (or a non-registered symbol).
    const t = typeof v;
    if ((t !== "object" && t !== "function" && t !== "symbol") || v === null) {
      throw new TypeError("Invalid value used in weak set");
    }
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) { return this; }
    }
    this.#items.push(v);
    return this;
  }
  has(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) { return true; }
    }
    return false;
  }
  delete(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) {
        const last = this.#items.length - 1;
        this.#items[i] = this.#items[last];
        this.#items.pop();
        return true;
      }
    }
    return false;
  }
}
