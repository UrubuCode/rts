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
  // ── ÍNDICE HASH ────────────────────────────────────────────────────────────
  // `#keys`/`#vals` seguem sendo a fonte da verdade e mantêm a ORDEM DE
  // INSERÇÃO, que a spec exige na iteração. O índice só elimina a VARREDURA:
  // `#h[b]` é o primeiro slot do bucket `b` e `#nx[i]` o próximo slot com o
  // mesmo hash (-1 termina) — o mesmo desenho de lista encadeada usado por um
  // hash espacial.
  //
  // Sem ele `get`/`set`/`has` eram O(n) no número de entradas: buscar a última
  // chave de um mapa de 1000 custava 86x buscar a primeira, e qualquer laço que
  // consultasse o Map por item ficava QUADRÁTICO.
  #h: number[] = [];
  #nx: number[] = [];
  #mask: number = 0;
  // `new Map([[k, v], …])` — iterable of [key, value] pairs, like JS. The pair `p`
  // is a for-of binding (unproven array); `p[0]`/`p[1]` read its elements via the
  // generic `__rtsadp_idx_get` runtime path (array element / string char / object).
  constructor(init: [K, V][] = []) {
    for (const p of init) { this.set(p[0], p[1]); }
  }
  // Índice do slot de `k`, ou -1. Percorre só o bucket do hash dela.
  #slot(k: K): number {
    if (this.#mask === 0) return -1;
    let i = this.#h[__hkey(k) & this.#mask];
    while (i >= 0) {
      if (__svz(this.#keys[i], k)) return i;
      i = this.#nx[i];
    }
    return -1;
  }
  // Reconstrói a tabela para caber `want` entradas (potência de 2, carga ~0.5).
  // Chamada ao crescer e após um `delete`, que renumera os slots.
  #rehash(want: number): void {
    let cap = 8;
    while (cap < (want + 1) * 2) cap = cap * 2;
    this.#mask = cap - 1;
    this.#h = [];
    for (let b = 0; b < cap; b++) this.#h.push(-1);
    this.#nx = [];
    for (let i = 0; i < this.#keys.length; i++) this.#nx.push(-1);
    for (let i = 0; i < this.#keys.length; i++) {
      const b = __hkey(this.#keys[i]) & this.#mask;
      this.#nx[i] = this.#h[b];
      this.#h[b] = i;
    }
  }
  set(k: K, v: V): Map<K, V> {
    const at = this.#slot(k);
    if (at >= 0) { this.#vals[at] = v; return this; }
    const i = this.#keys.length;
    this.#keys.push(k); this.#vals.push(v);
    // a tabela precisa crescer? senão, só encadeia o slot novo
    if (this.#mask === 0 || (i + 1) * 2 > this.#mask + 1) {
      this.#rehash(i + 1);
    } else {
      const b = __hkey(k) & this.#mask;
      this.#nx.push(this.#h[b]);
      this.#h[b] = i;
    }
    return this;
  }
  get(k: K): V | undefined {
    const at = this.#slot(k);
    if (at >= 0) return this.#vals[at];
    return undefined;
  }
  has(k: K): boolean {
    return this.#slot(k) >= 0;
  }
  delete(k: K): boolean {
    const at = this.#slot(k);
    if (at < 0) return false;
    // Desloca para preservar a ordem de inserção (a spec exige). Isso renumera
    // todo slot > at, então o índice é reconstruído — `delete` é o caminho raro.
    for (let j = at; j < this.#keys.length - 1; j++) {
      this.#keys[j] = this.#keys[j + 1];
      this.#vals[j] = this.#vals[j + 1];
    }
    this.#keys.pop(); this.#vals.pop();
    this.#rehash(this.#keys.length);
    return true;
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
  clear(): void {
    this.#keys = []; this.#vals = [];
    this.#h = []; this.#nx = []; this.#mask = 0;
  }
  // `Map.groupBy(items, cb)` — ES2024. Groups `items` by `cb(item, index)` under
  // SameValueZero key identity, buckets in first-seen key order (JS spec).
  static groupBy(items: any[], cb: (item: any, index: number) => any): Map<any, any[]> {
    const m = new Map<any, any[]>();
    for (let i = 0; i < items.length; i++) {
      const k = cb(items[i], i);
      const bucket = m.get(k);
      if (bucket === undefined) {
        m.set(k, [items[i]]);
      } else {
        bucket.push(items[i]);
      }
    }
    return m;
  }
  // Default iterator (`for (const [k,v] of map)`): a real generator yielding
  // `[key, value]` pairs in insertion order. Plain JS — the parser desugars the
  // `*`/`yield`, the engine drives it through the generator state machine.
  *[Symbol.iterator](): [K, V][] {
    for (let i = 0; i < this.size; i++) { yield [this.#keys[i], this.#vals[i]]; }
  }
}
class Set<T> {
  #items: T[] = [];
  // Mesmo índice hash do Map (ver lá): `#items` mantém a ordem de inserção,
  // a tabela só evita a varredura em `add`/`has`/`delete`.
  #h: number[] = [];
  #nx: number[] = [];
  #mask: number = 0;
  constructor(init: T[] = []) {
    for (const v of init) { this.add(v); }
  }
  #slot(v: T): number {
    if (this.#mask === 0) return -1;
    let i = this.#h[__hkey(v) & this.#mask];
    while (i >= 0) {
      if (__svz(this.#items[i], v)) return i;
      i = this.#nx[i];
    }
    return -1;
  }
  #rehash(want: number): void {
    let cap = 8;
    while (cap < (want + 1) * 2) cap = cap * 2;
    this.#mask = cap - 1;
    this.#h = [];
    for (let b = 0; b < cap; b++) this.#h.push(-1);
    this.#nx = [];
    for (let i = 0; i < this.#items.length; i++) this.#nx.push(-1);
    for (let i = 0; i < this.#items.length; i++) {
      const b = __hkey(this.#items[i]) & this.#mask;
      this.#nx[i] = this.#h[b];
      this.#h[b] = i;
    }
  }
  add(v: T): Set<T> {
    if (this.#slot(v) >= 0) return this;
    const i = this.#items.length;
    this.#items.push(v);
    if (this.#mask === 0 || (i + 1) * 2 > this.#mask + 1) {
      this.#rehash(i + 1);
    } else {
      const b = __hkey(v) & this.#mask;
      this.#nx.push(this.#h[b]);
      this.#h[b] = i;
    }
    return this;
  }
  has(v: T): boolean {
    return this.#slot(v) >= 0;
  }
  delete(v: T): boolean {
    const at = this.#slot(v);
    if (at < 0) return false;
    for (let j = at; j < this.#items.length - 1; j++) this.#items[j] = this.#items[j + 1];
    this.#items.pop();
    this.#rehash(this.#items.length);
    return true;
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
  clear(): void {
    this.#items = [];
    this.#h = []; this.#nx = []; this.#mask = 0;
  }
  // --- ES2025 Set operations. Each takes another Set and reads it through its
  // public surface (`values()`/`has()`), returning a fresh Set / boolean.
  union(other: Set<T>): Set<T> {
    const out = new Set<T>();
    for (let i = 0; i < this.#items.length; i++) { out.add(this.#items[i]); }
    const ov = other.values();
    for (let i = 0; i < ov.length; i++) { out.add(ov[i]); }
    return out;
  }
  intersection(other: Set<T>): Set<T> {
    const out = new Set<T>();
    for (let i = 0; i < this.#items.length; i++) {
      const v = this.#items[i];
      if (other.has(v)) { out.add(v); }
    }
    return out;
  }
  difference(other: Set<T>): Set<T> {
    const out = new Set<T>();
    for (let i = 0; i < this.#items.length; i++) {
      const v = this.#items[i];
      if (!other.has(v)) { out.add(v); }
    }
    return out;
  }
  symmetricDifference(other: Set<T>): Set<T> {
    const out = new Set<T>();
    for (let i = 0; i < this.#items.length; i++) {
      const v = this.#items[i];
      if (!other.has(v)) { out.add(v); }
    }
    const ov = other.values();
    for (let i = 0; i < ov.length; i++) {
      const v = ov[i];
      if (!this.has(v)) { out.add(v); }
    }
    return out;
  }
  isSubsetOf(other: Set<T>): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (!other.has(this.#items[i])) return false;
    }
    return true;
  }
  isSupersetOf(other: Set<T>): boolean {
    const ov = other.values();
    for (let i = 0; i < ov.length; i++) {
      if (!this.has(ov[i])) return false;
    }
    return true;
  }
  isDisjointFrom(other: Set<T>): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (other.has(this.#items[i])) return false;
    }
    return true;
  }
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

// Hash de uma chave, para o índice de Map/Set. Só precisa de UMA garantia:
// se `__svz(a, b)` então `__hkey(a) === __hkey(b)`. O contrário é livre —
// uma colisão só faz o bucket ter mais de um slot, e o `__svz` decide.
//
// Chaves que não são número nem string (objeto, função, símbolo, null,
// undefined, boolean) caem todas no bucket 0. Isso as mantém CORRETAS (o
// bucket vira a varredura linear de antes) sem exigir identidade de objeto,
// que este subset não expõe. Mapas com chave de objeto são raros e pequenos;
// os que aparecem em laço quente têm chave número ou string.
function __hkey(k: any): number {
  const t = typeof k;
  if (t === "number") {
    // NaN !== NaN, mas SameValueZero os une: um bucket fixo p/ todos os NaN.
    if (k !== k) return 0x7fffffff;
    // Inteiros (o caso comum: id, índice, chave de célula) passam direto pelo
    // mixer. Não-inteiros entram pela parte inteira — colidem entre si, o que
    // é correto, só menos espalhado.
    let n = k | 0;
    if (n !== k) n = (k * 2654435761) | 0;
    // mistura (xorshift-multiply): sem isso, chaves múltiplas de uma potência
    // de 2 cairiam todas no mesmo bucket depois do AND.
    n = (n ^ (n >>> 16)) | 0;
    n = (n * 2246822519) | 0;
    n = (n ^ (n >>> 13)) | 0;
    return n & 0x7fffffff;
  }
  if (t === "string") {
    // FNV-1a de 32 bits sobre os code units.
    let h = 2166136261;
    for (let i = 0; i < k.length; i++) {
      h = (h ^ k.charCodeAt(i)) | 0;
      h = (h * 16777619) | 0;
    }
    return h & 0x7fffffff;
  }
  return 0;
}
