// Cross-runtime: the CALLBACK side of Map.groupBy — the (element, index) pair,
// the index type, which sources it accepts, and what it refuses. (The key
// handling itself is pinned elsewhere; this pins the call contract.)

// --- the callback receives element and a numeric index ---
const seen: string[] = [];
Map.groupBy(["a", "b", "c"], (el: any, i: any) => {
  seen.push(String(el) + "@" + String(i) + "/" + typeof i);
  return "g";
});
console.log("args=" + seen.join(","));

// --- callback arity reported to it is 2 ---
console.log("index_starts_at=" + seen[0].split("@")[1]);

// --- the index counts POSITIONS, not group members ---
const idx: number[] = [];
Map.groupBy([5, 6, 7, 8], (el: any, i: any) => { idx.push(i); return el % 2; });
console.log("indices=" + idx.join(","));

// --- the result is a real Map with Map.prototype ---
const r = Map.groupBy([1, 2, 3, 4, 5], (n: any) => (n % 2 === 0 ? "even" : "odd"));
console.log("is_map=" + (r instanceof Map));
console.log("proto_is_map=" + (Object.getPrototypeOf(r) === Map.prototype));
console.log("tag=" + Object.prototype.toString.call(r));
console.log("size=" + r.size);
console.log("keys=" + [...r.keys()].join(","));
console.log("odd=" + (r.get("odd") as any).join(","));
console.log("even=" + (r.get("even") as any).join(","));

// --- group order is FIRST APPEARANCE of the key, not sort order ---
const order = Map.groupBy(["zz", "a", "yyy", "b"], (s: any) => s.length);
console.log("first_appearance=" + [...order.keys()].join(","));

// --- the group values are plain Arrays ---
const grp: any = r.get("odd");
console.log("group_is_array=" + Array.isArray(grp));
console.log("group_proto=" + (Object.getPrototypeOf(grp) === Array.prototype));

// --- any iterable is accepted, not just arrays ---
console.log("from_string=" + [...Map.groupBy("banana", (c: any) => c).keys()].join(","));
console.log("from_set=" + [...Map.groupBy(new Set([1, 2, 3]), (n: any) => n > 1).keys()].join(","));
function* gen() { yield 10; yield 21; yield 32; }
console.log("from_generator=" + [...Map.groupBy(gen(), (n: any) => n % 2).keys()].join(","));
const mapSrc = new Map([["a", 1], ["b", 2]]);
console.log("from_map=" + [...Map.groupBy(mapSrc, (e: any) => e[0]).keys()].join(","));

// --- an empty source gives an empty Map, and never calls back ---
let calls = 0;
const empty = Map.groupBy([], (_e: any) => { calls++; return 1; });
console.log("empty=" + empty.size + ":calls=" + calls);

// --- holes in a sparse array are visited as undefined ---
const sparse = [1, , 3];
console.log("sparse=" + [...Map.groupBy(sparse, (v: any) => String(v)).keys()].join(","));

// --- refusals ---
function bad(label: string, fn: () => any): void {
  try { fn(); console.log(label + "=no_throw"); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("non_callable", () => (Map as any).groupBy([1], 42));
bad("undefined_cb", () => (Map as any).groupBy([1], undefined));
bad("non_iterable", () => (Map as any).groupBy(42, (x: any) => x));
bad("null_source", () => (Map as any).groupBy(null, (x: any) => x));
bad("undefined_source", () => (Map as any).groupBy(undefined, (x: any) => x));
bad("plain_object_source", () => (Map as any).groupBy({ a: 1 }, (x: any) => x));

// --- a throwing callback propagates and no Map comes back ---
bad("throwing_cb", () => Map.groupBy([1, 2], (n: any) => { if (n === 2) throw new RangeError("x"); return n; }));

// --- groupBy is a static of Map, not of Map.prototype ---
console.log("static=" + (typeof Map.groupBy) + ":" + ("groupBy" in Map.prototype));
console.log("groupby_length=" + Map.groupBy.length + ":" + Map.groupBy.name);
const gd: any = Object.getOwnPropertyDescriptor(Map, "groupBy");
console.log("groupby_flags=" + gd.writable + ":" + gd.enumerable + ":" + gd.configurable);
