// Cross-runtime: the ES2025 iterator helpers reached from a Map/Set iterator.
// They are lazy — built on the live cursor, so a mutation between building the
// chain and draining it is visible — and each one hands back an Iterator Helper
// rather than an array.

const s = new Set([1, 2, 3, 4, 5]);
const m = new Map<string, number>([["a", 1], ["b", 2], ["c", 3]]);

// --- the helpers exist on the shared %IteratorPrototype% ---
const helperNames = ["map", "filter", "take", "drop", "flatMap", "reduce", "toArray", "forEach", "some", "every", "find"];
const iterProto = Object.getPrototypeOf(Object.getPrototypeOf(s.values()));
let present = "";
for (const n of helperNames) present += n + ":" + typeof (iterProto as any)[n] + " ";
console.log("on_iterator_prototype=" + present.trim());
console.log("shared_with_array=" + (iterProto === Object.getPrototypeOf(Object.getPrototypeOf([].values()))));
console.log("iterator_global=" + typeof Iterator + ":" + (Iterator.prototype === iterProto));

// --- each helper's arity ---
let arities = "";
for (const n of helperNames) arities += n + ":" + (iterProto as any)[n].length + " ";
console.log("arities=" + arities.trim());

// --- the results ---
console.log("map=" + (s.values() as any).map((v: number) => v * 2).toArray().join(","));
console.log("filter=" + (s.values() as any).filter((v: number) => v % 2 === 1).toArray().join(","));
console.log("take=" + (s.values() as any).take(2).toArray().join(","));
console.log("drop=" + (s.values() as any).drop(3).toArray().join(","));
console.log("flatMap=" + (s.values() as any).flatMap((v: number) => [v, -v]).take(6).toArray().join(","));
console.log("reduce=" + (s.values() as any).reduce((a: number, b: number) => a + b, 0));
console.log("reduce_no_seed=" + (s.values() as any).reduce((a: number, b: number) => a + b));
console.log("some=" + (s.values() as any).some((v: number) => v === 3));
console.log("every=" + (s.values() as any).every((v: number) => v < 10));
console.log("find=" + (s.values() as any).find((v: number) => v > 3));
console.log("find_miss=" + String((s.values() as any).find((v: number) => v > 99)));

// --- the callbacks receive (value, index) ---
const pairs: string[] = [];
(s.values() as any).map((v: number, i: number) => { pairs.push(i + ":" + v); return v; }).toArray();
console.log("map_index=" + pairs.join(","));

const mapPairs: string[] = [];
(m.entries() as any).forEach((e: any, i: number) => mapPairs.push(i + ":" + e[0] + "=" + e[1]));
console.log("entries_forEach=" + mapPairs.join(","));
console.log("map_keys_chain=" + (m.keys() as any).map((k: string) => k.toUpperCase()).toArray().join(","));
console.log("map_values_chain=" + (m.values() as any).filter((v: number) => v > 1).toArray().join(","));

// --- chaining, and the helper is itself an iterator ---
const chain = (s.values() as any).map((v: number) => v * 10).filter((v: number) => v > 20).drop(1).take(2);
console.log("chain_tag=" + Object.prototype.toString.call(chain));
console.log("chain_self_iterable=" + (chain[Symbol.iterator]() === chain));
console.log("chain_result=" + [...chain].join(","));
console.log("chain_exhausted=" + JSON.stringify(chain.next()));

// --- laziness: the source is only walked on demand ---
let pulled = 0;
const counting = new Set([1, 2, 3, 4, 5, 6]);
const lazy = (counting.values() as any).map((v: number) => { pulled++; return v; });
console.log("pulled_before=" + pulled);
console.log("lazy_first=" + lazy.next().value);
console.log("pulled_after_one=" + pulled);
console.log("lazy_two_more=" + lazy.take(2).toArray().join(","));
console.log("pulled_total=" + pulled);

// --- built before a mutation, drained after: the mutation is seen ---
const live = new Set([1, 2]);
const pending = (live.values() as any).map((v: number) => v * 100);
live.add(3);
console.log("live_seen=" + pending.toArray().join(","));

// --- take(0) yields nothing, and a generator that was never STARTED runs no
//     finally block when it is closed ---
let closed = "no";
function* closable(): any {
  try { yield 1; yield 2; } finally { closed = "yes"; }
}
console.log("take0=" + (closable() as any).take(0).toArray().join(",") + ":closed=" + closed);

// --- an exhausted helper stays done ---
const once = (new Set([1]).values() as any).map((v: number) => v);
console.log("once_1=" + JSON.stringify(once.next()));
console.log("once_2=" + JSON.stringify(once.next()));
console.log("once_3=" + JSON.stringify(once.next()));

// --- helpers refuse a plain object and a non-callable argument ---
function probe(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
probe("map_on_plain", () => (iterProto as any).map.call({}, (v: any) => v));
probe("map_no_callback", () => (s.values() as any).map());
probe("map_non_callable", () => (s.values() as any).map(42));
probe("take_negative", () => (s.values() as any).take(-1).toArray());
probe("take_nan", () => (s.values() as any).take(NaN).toArray());
probe("drop_negative", () => (s.values() as any).drop(-1).toArray());
probe("take_infinity", () => (s.values() as any).take(Infinity).toArray().join(","));
probe("take_string", () => (s.values() as any).take("2").toArray().join(","));
probe("flatMap_non_iterable", () => (s.values() as any).flatMap((v: number) => v).toArray());
probe("reduce_empty_no_seed", () => (new Set() as any).values().reduce((a: any, b: any) => a + b));

// --- Iterator.from wraps a bare next()-bearing object ---
let i = 0;
const bare: any = { next() { return i < 3 ? { value: i++, done: false } : { value: undefined, done: true }; } };
const wrapped = (Iterator as any).from(bare);
console.log("iterator_from=" + wrapped.map((v: number) => v + 1).toArray().join(","));
console.log("iterator_from_set=" + (Iterator as any).from(new Set([7, 8])).toArray().join(","));
