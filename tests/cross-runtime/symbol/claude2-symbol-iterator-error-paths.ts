// Cross-runtime: how each consumer of the iteration protocol FAILS. The hook
// may be missing, present but not callable, or callable but wrong at every
// later step — and the same broken object gives the same answer to every
// consumer, except Array.from, which quietly falls back to the array-like path.

function broken(kind: string): any {
  const o: any = { length: 2, 0: "a", 1: "b" };
  if (kind === "missing") return o;
  if (kind === "undefined") { o[Symbol.iterator] = undefined; return o; }
  if (kind === "null") { o[Symbol.iterator] = null; return o; }
  if (kind === "number") { o[Symbol.iterator] = 42; return o; }
  if (kind === "object") { o[Symbol.iterator] = {}; return o; }
  if (kind === "returns_primitive") { o[Symbol.iterator] = function () { return 5; }; return o; }
  if (kind === "returns_null") { o[Symbol.iterator] = function () { return null; }; return o; }
  if (kind === "no_next") { o[Symbol.iterator] = function () { return {}; }; return o; }
  if (kind === "next_not_callable") { o[Symbol.iterator] = function () { return { next: 1 }; }; return o; }
  if (kind === "next_returns_primitive") { o[Symbol.iterator] = function () { return { next() { return 7; } }; }; return o; }
  if (kind === "next_throws") { o[Symbol.iterator] = function () { return { next() { throw new EvalError("next"); } }; }; return o; }
  if (kind === "hook_throws") { o[Symbol.iterator] = function () { throw new URIError("hook"); }; return o; }
  if (kind === "getter_throws") {
    Object.defineProperty(o, Symbol.iterator, { get() { throw new RangeError("getter"); } });
    return o;
  }
  if (kind === "ok") { o[Symbol.iterator] = function () { let i = 0; return { next: () => i < 2 ? { value: "v" + i++, done: false } : { value: undefined, done: true } }; }; return o; }
  return o;
}

const kinds = ["ok", "missing", "undefined", "null", "number", "object", "returns_primitive", "returns_null", "no_next", "next_not_callable", "next_returns_primitive", "next_throws", "hook_throws", "getter_throws"];

const consumers: any[] = [
  ["for_of", (v: any) => { const out: string[] = []; for (const x of v) out.push(String(x)); return out.join(","); }],
  ["spread_array", (v: any) => [...v].join(",")],
  ["spread_call", (v: any) => (function (...args: any[]) { return args.join(","); })(...v)],
  ["destructure", (v: any) => { const [a, b] = v; return String(a) + "," + String(b); }],
  ["array_from", (v: any) => Array.from(v).join(",")],
  ["new_set", (v: any) => [...new Set(v)].join(",")],
  ["yield_star", (v: any) => { function* g(): any { yield* v; } return [...g()].join(","); }],
  ["object_fromEntries", (v: any) => { try { return Object.keys(Object.fromEntries(v)).join(","); } catch (e: any) { throw e; } }],
];

for (const kind of kinds) {
  let row = kind + ":";
  for (const c of consumers) {
    let tag: string;
    try { tag = "ok(" + c[1](broken(kind)) + ")"; }
    catch (e: any) { tag = e.constructor.name; }
    row += " " + c[0] + "=" + tag;
  }
  console.log(row);
}

// --- Array.from on an object with NO hook uses length and indices ---
console.log("array_from_arraylike=" + Array.from({ length: 3, 0: "x", 2: "z" } as any).map(String).join(","));
console.log("array_from_no_length=" + Array.from({ 0: "x" } as any).length);
console.log("array_from_string=" + Array.from("ab").join(","));

// --- a `done` that is merely truthy/falsy is enough ---
function withDone(v: any): any {
  let i = 0;
  return { [Symbol.iterator]() { return { next() { i++; return { value: i, done: i > 2 ? v : false }; } }; } };
}
console.log("done_truthy_string=" + [...withDone("yes")].join(","));
console.log("done_one=" + [...withDone(1)].join(","));
console.log("done_object=" + [...withDone({})].join(","));

// --- a missing `value` reads as undefined ---
console.log("missing_value=" + [...{ [Symbol.iterator]() { let n = 0; return { next: () => ({ done: n++ > 1 }) }; } } as any].map(String).join(","));

// --- the hook is read ONCE per consumption ---
let reads = 0;
const counted: any = {};
Object.defineProperty(counted, Symbol.iterator, {
  get() { reads++; return function () { let i = 0; return { next: () => i < 1 ? { value: i++, done: false } : { done: true } }; }; },
});
console.log("one_read=" + [...counted].join(",") + ":reads=" + reads);
console.log("two_reads=" + [...counted].join(",") + ":reads=" + reads);

// --- `in` and typeof do not consult it; only consumption does ---
const bad = broken("number");
console.log("brand_visible=" + (Symbol.iterator in bad) + ":" + typeof bad[Symbol.iterator]);
console.log("array_isArray=" + Array.isArray(bad));

// --- a primitive is never iterable except a string ---
function consume(v: any): string {
  try { return "ok:" + [...v].length; }
  catch (e: any) { return e.constructor.name; }
}
console.log("prim_number=" + consume(1));
console.log("prim_boolean=" + consume(true));
console.log("prim_null=" + consume(null));
console.log("prim_undefined=" + consume(undefined));
console.log("prim_symbol=" + consume(Symbol("s")));
console.log("prim_bigint=" + consume(10n));
console.log("prim_string=" + consume("abc"));
console.log("plain_object=" + consume({ a: 1 }));

// --- a Map with its hook removed stops being iterable but keeps working ---
const m: any = new Map([["a", 1]]);
m[Symbol.iterator] = undefined;
console.log("map_hook_removed=" + consume(m) + ":get=" + m.get("a") + ":size=" + m.size);
console.log("map_entries_still_ok=" + [...m.entries()].map((e: any) => e.join(":")).join(","));
delete m[Symbol.iterator];
console.log("map_hook_restored=" + consume(m));
