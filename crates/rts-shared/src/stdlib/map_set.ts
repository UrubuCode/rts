// Faithful TypeScript `Map`/`Set` — the REAL stdlib for the new engine.
//
// Generic `Map<K,V>` + `Set<T>` with private array fields, `===` key compare,
// `return this` chaining, `delete` via shift+pop, `undefined` on miss, and a
// `get size()` getter. Embedded as an engine `include` (declarations-only, no
// top-level code): its top-level classes become ambient and shadow the native
// Map/Set entirely. Parity with the former native dispatch is proven by
// `stdlib_parity.rs`.

class Map<K, V> {
  #keys: K[] = [];
  #vals: V[] = [];
  set(k: K, v: V): Map<K, V> {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) { this.#vals[i] = v; return this; }
    }
    this.#keys.push(k); this.#vals.push(v);
    return this;
  }
  get(k: K): V | undefined {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) return this.#vals[i];
    }
    return undefined;
  }
  has(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) return true;
    }
    return false;
  }
  delete(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) {
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
}
class Set<T> {
  #items: T[] = [];
  add(v: T): Set<T> {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) return this;
    }
    this.#items.push(v);
    return this;
  }
  has(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) return true;
    }
    return false;
  }
  delete(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) {
        for (let j = i; j < this.#items.length - 1; j++) this.#items[j] = this.#items[j + 1];
        this.#items.pop();
        return true;
      }
    }
    return false;
  }
  get size(): number { return this.#items.length; }
}
