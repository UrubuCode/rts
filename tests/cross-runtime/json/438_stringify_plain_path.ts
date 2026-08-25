// `JSON.stringify` walks a plain object straight off its layout rather than
// building a key list on the heap (`rts-core`'s `json/write.rs`, `plain`).
// Everything below is a shape that path must REFUSE, beside one it must take —
// so a refusal quietly stopping working shows up here as wrong output rather
// than as a faster wrong answer.

// The shape it takes: data properties, string keys, nothing inherited.
console.log(JSON.stringify({ a: 1, b: "two", c: [1, 2, 3], d: { e: true } }));
console.log(JSON.stringify({}));

// A getter is observable: it must RUN, and exactly once.
let reads = 0;
const accessor = {
  a: 1,
  get b() {
    reads++;
    return reads;
  },
};
console.log(`${JSON.stringify(accessor)} lido ${reads}`);

// Enumeration order: an array-index key comes first and in numeric order,
// whatever order the object was written in.
console.log(JSON.stringify({ b: 1, 2: 2, a: 3, 1: 4 }));

// A symbol key is not enumerated. Its key lives in a reserved name space, so a
// check that merely asked "does this key have text" would emit the engine's own
// spelling here.
const marker = Symbol("s");
console.log(JSON.stringify({ a: 1, [marker]: 2 }));

// A non-enumerable property is skipped.
console.log(JSON.stringify(Object.defineProperty({ a: 1 }, "hidden", { value: 2 })));

// `toJSON` replaces the value, and it is handed the KEY — which the fast path
// never materialises unless a hook is reached.
console.log(JSON.stringify({ d: { toJSON: (k: string) => `k=${k}` } }));
console.log(JSON.stringify({ when: new Date(0) }));

// A replacer, in both spellings.
console.log(JSON.stringify({ a: 1, b: 2, c: 3 }, ["c", "a"]));
console.log(JSON.stringify({ a: 1, b: 2 }, (k, v) => (typeof v === "number" ? v * 10 : v)));

// Indentation, and a member that disappears must not leave its comma behind.
console.log(JSON.stringify({ a: 1, gone: undefined, f: () => 1, b: 2 }));
console.log(JSON.stringify({ a: 1, b: { c: 2 } }, null, 2));

// A cycle is a TypeError, not an output.
const cycle: Record<string, unknown> = { a: 1 };
cycle.self = cycle;
try {
  JSON.stringify(cycle);
  console.log("sem erro");
} catch (e) {
  console.log(`erro ${(e as Error).name}`);
}

// Inherited properties are not own ones, so none of them is written.
const base = { inherited: 1 };
const derived = Object.create(base) as Record<string, unknown>;
derived.own = 2;
console.log(JSON.stringify(derived));

// Keys that need escaping, and values at the edges of the number rules.
console.log(JSON.stringify({ 'a"b': 1, "a\b": 2, "a\nb": 3, "": 4 }));
console.log(JSON.stringify({ zero: -0, big: 1e21, inf: Infinity, nan: NaN }));
